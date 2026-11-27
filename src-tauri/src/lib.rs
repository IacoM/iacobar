mod apps;
mod audio_out;
mod autostart;
mod backup;
mod bars;
mod config;
mod icons;
mod jumplist;
mod media;
mod mixer;
mod preview;
mod radios;
mod system;
mod taskbar;
mod tray;
mod tray_orphans;
mod vdesk;
mod wifi;
mod wallpaper;
mod wins;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, RunEvent, WebviewWindow, WindowEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_A, VK_CONTROL, VK_D, VK_ESCAPE, VK_LWIN, VK_N, VK_S, VK_SHIFT, VK_TAB, VK_X,
};

/// Gap between bar islands and flyouts, logical px.
const FLYOUT_GAP: f64 = 6.0;

// ---------------------------------------------------------------- cleanup --

fn cleanup() {
    tray::stop(); // tray icons go back to Explorer
    bars::release_all();
    taskbar::restore();
}

fn install_safety_nets() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // %TEMP%\iacobar-crash.txt: what went wrong, for support
        let _ = std::fs::write(
            std::env::temp_dir().join("iacobar-crash.txt"),
            format!("{info}\n\n{}", std::backtrace::Backtrace::force_capture()),
        );
        cleanup();
        prev(info);
    }));

    // Ctrl+C / closing the console in `tauri dev`.
    use windows::Win32::System::Console::SetConsoleCtrlHandler;
    use windows::core::BOOL;
    unsafe extern "system" fn handler(_ctrl: u32) -> BOOL {
        cleanup();
        BOOL(0) // let the default handler terminate the process
    }
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(handler), true);
    }
}

// ----------------------------------------------------------------- layout --

/// WebView2 in a transparent window may not repaint after `show()` when it
/// was created hidden: a 1px resize round-trip forces a new frame.
fn nudge(win: &WebviewWindow) {
    let w = win.clone();
    std::thread::spawn(move || {
        for delay in [30u64, 400] {
            std::thread::sleep(Duration::from_millis(delay));
            if let Ok(size) = w.inner_size() {
                let _ = w.set_size(tauri::PhysicalSize::new(size.width, size.height + 1));
                let _ = w.set_size(size);
            }
        }
    });
}

/// Physical rect of a popup of `width`x`height` (logical) centred on `x`
/// (logical, relative to the calling bar), sitting just above the islands.
fn popup_rect(bar: &WebviewWindow, x: f64, width: f64, height: f64, gap: f64) -> Result<(i32, i32, i32, i32), String> {
    let scale = bar.scale_factor().map_err(|e| e.to_string())?;
    let pos = bar.outer_position().map_err(|e| e.to_string())?;
    let size = bar.outer_size().map_err(|e| e.to_string())?;
    let w = (width * scale).round() as i32;
    let h = (height * scale).round() as i32;
    let g = (gap * scale).round() as i32;
    let min_x = pos.x + g;
    let max_x = pos.x + size.width as i32 - w - g;
    let px = (pos.x + (x * scale).round() as i32 - w / 2).clamp(min_x, max_x.max(min_x));
    // popups sit just above the islands
    let py = pos.y + (bars::island_top() * scale).round() as i32 - h - g;
    Ok((px, py, w, h))
}

// ---------------------------------------------------------------- flyouts --

struct FlyoutState {
    panel: String,
    hidden_at: Option<Instant>,
}

static FLYOUT: Mutex<FlyoutState> = Mutex::new(FlyoutState { panel: String::new(), hidden_at: None });

#[derive(Serialize, Clone)]
struct FlyoutOpen {
    panel: String,
    data: serde_json::Value,
}

/// Opens a flyout above the calling bar, horizontally centred on `x`.
/// Clicking the same button again closes it.
#[tauri::command]
fn show_flyout(
    app: AppHandle,
    bar: WebviewWindow,
    panel: String,
    x: f64,
    width: f64,
    height: f64,
    data: Option<serde_json::Value>,
) -> Result<(), String> {
    let Some(fly) = app.get_webview_window("flyout") else {
        return Err("window missing".into());
    };
    preview::hide_now(&app);
    {
        let mut st = FLYOUT.lock().unwrap();
        let visible = fly.is_visible().unwrap_or(false);
        let just_hidden = st.hidden_at.map(|t| t.elapsed() < Duration::from_millis(300)).unwrap_or(false);
        if st.panel == panel && (visible || just_hidden) {
            drop(st);
            close_flyout(&app);
            FLYOUT.lock().unwrap().hidden_at = None; // a new click may reopen it right away
            return Ok(());
        }
        st.panel = panel.clone();
        st.hidden_at = None;
    }
    OPEN_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let (px, py, _, _) = popup_rect(&bar, x, width, height, FLYOUT_GAP)?;
    // move first (DPI of the target monitor), then size in logical px
    fly.set_position(PhysicalPosition::new(px, py)).map_err(|e| e.to_string())?;
    fly.set_size(LogicalSize::new(width, height)).map_err(|e| e.to_string())?;
    fly.set_position(PhysicalPosition::new(px, py)).map_err(|e| e.to_string())?;
    fly.emit("flyout-open", FlyoutOpen { panel, data: data.unwrap_or(serde_json::Value::Null) })
        .map_err(|e| e.to_string())?;
    fly.show().map_err(|e| e.to_string())?;
    let _ = app.emit("flyout-shown", ());
    nudge(&fly);
    fly.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Resizes the open flyout to its content's height (logical px), keeping
/// its bottom edge in place, so panels never show an empty band.
#[tauri::command]
fn fit_flyout(app: AppHandle, height: f64) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};
    let Some(fly) = app.get_webview_window("flyout") else { return Ok(()) };
    if !fly.is_visible().unwrap_or(false) || !(40.0..=900.0).contains(&height) {
        return Ok(());
    }
    let scale = fly.scale_factor().map_err(|e| e.to_string())?;
    let pos = fly.outer_position().map_err(|e| e.to_string())?;
    let size = fly.outer_size().map_err(|e| e.to_string())?;
    let h = (height * scale).round() as i32;
    if (h - size.height as i32).abs() <= 1 {
        return Ok(());
    }
    let bottom = pos.y + size.height as i32;
    let hwnd = fly.hwnd().map_err(|e| e.to_string())?;
    // move + resize in one step: no frame with the old top and new height
    unsafe {
        let _ = SetWindowPos(
            HWND(hwnd.0 as *mut _),
            None,
            pos.x,
            bottom - h,
            size.width as i32,
            h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    Ok(())
}

/// Windows accent colour (for the "system" accent of the themes).
#[tauri::command]
fn system_accent() -> String {
    system::accent()
}

/// Hides the flyout window now. Called by the flyout page once its panel has
/// faded out (so the last frame left in the hidden window is empty and never
/// flashes when it is shown again).
#[tauri::command]
fn hide_flyout(app: AppHandle) {
    if let Some(fly) = app.get_webview_window("flyout") {
        if fly.is_visible().unwrap_or(false) {
            let _ = fly.hide();
            let _ = app.emit("flyout-hidden", ());
        }
    }
}

/// Bumped on every open: a pending fallback hide only applies to its own panel.
static OPEN_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Closes the flyout the nice way: the page fades its panel out, then calls
/// `hide_flyout`. If the page doesn't answer, hide it anyway shortly after.
fn close_flyout(app: &AppHandle) {
    let Some(fly) = app.get_webview_window("flyout") else { return };
    if !fly.is_visible().unwrap_or(false) {
        return;
    }
    FLYOUT.lock().unwrap().hidden_at = Some(Instant::now());
    let _ = fly.emit("flyout-close", ());
    let opened = OPEN_GEN.load(std::sync::atomic::Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        if OPEN_GEN.load(std::sync::atomic::Ordering::SeqCst) == opened {
            hide_flyout(app);
        }
    });
}

/// The flyout page asks to close (focus lost, Esc, an action done).
#[tauri::command]
fn request_close_flyout(app: AppHandle) {
    close_flyout(&app);
}

// --------------------------------------------------------------- previews --

#[derive(Serialize, Clone)]
struct PreviewOpen {
    data: serde_json::Value,
}

/// Shows live previews above the calling bar, centred on `x`.
#[tauri::command]
fn show_preview(
    app: AppHandle,
    bar: WebviewWindow,
    x: f64,
    width: f64,
    height: f64,
    data: serde_json::Value,
) -> Result<(), String> {
    if app.get_webview_window("flyout").and_then(|f| f.is_visible().ok()).unwrap_or(false) {
        return Ok(()); // a panel is open: no hover previews
    }
    let Some(pw) = app.get_webview_window("preview") else {
        return Err("no preview window".into());
    };
    let (px, py, w, h) = popup_rect(&bar, x, width, height, FLYOUT_GAP)?;
    pw.emit("preview-open", PreviewOpen { data }).map_err(|e| e.to_string())?;
    preview::show_at(&app, px, py, w, h);
    Ok(())
}

#[tauri::command]
fn set_thumbs(app: AppHandle, rects: Vec<preview::ThumbRect>) {
    preview::set_thumbs(&app, &rects);
}

#[tauri::command]
fn hide_preview(app: AppHandle, delay: Option<u64>) {
    preview::hide_later(app, delay.unwrap_or(0));
}

#[tauri::command]
fn preview_hold(on: bool) {
    preview::hold(on);
}

// ------------------------------------------------------------------- bars --

#[tauri::command]
fn bar_info(window: WebviewWindow) -> Option<bars::BarInfo> {
    bars::info(window.label())
}

#[tauri::command]
fn set_region(window: WebviewWindow, shapes: Vec<bars::Shape>) {
    bars::set_region(&window, &shapes);
}

// --------------------------------------------------------------- commands --
//
// Anything that talks to OTHER processes (show/minimise their windows, launch
// apps, tray clicks, AppBar messages) runs on a worker thread: if that app is
// slow or hung, only the worker waits, never the bar's UI thread.

fn off_ui<F: FnOnce() + Send + 'static>(f: F) {
    tauri::async_runtime::spawn_blocking(f);
}

#[tauri::command]
async fn get_windows() -> Vec<wins::WinInfo> {
    tauri::async_runtime::spawn_blocking(wins::enumerate).await.unwrap_or_default()
}

#[tauri::command]
fn get_active() -> isize {
    wins::active()
}

#[tauri::command]
fn window_action(app: AppHandle, hwnd: isize, action: String) {
    preview::hide_now(&app);
    off_ui(move || match action.as_str() {
        "toggle" => wins::toggle(hwnd),
        "activate" => wins::activate(hwnd),
        "minimize" => wins::minimize(hwnd),
        "close" => wins::close(hwnd),
        _ => {}
    });
}

#[tauri::command]
async fn get_icon(path: String, size: Option<i32>) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || icons::icon_for_path(&path, size.unwrap_or(64)))
        .await
        .ok()
        .flatten()
}

#[tauri::command]
async fn get_window_icon(hwnd: isize, size: Option<i32>) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        icons::icon_from_window(windows::Win32::Foundation::HWND(hwnd as *mut _), size.unwrap_or(64))
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
async fn list_apps(refresh: Option<bool>) -> Vec<apps::AppEntry> {
    tauri::async_runtime::spawn_blocking(move || {
        if refresh.unwrap_or(false) {
            apps::refresh();
        }
        apps::list()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn resolve_shortcut(path: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || apps::resolve_lnk(&path)).await.ok().flatten()
}

#[tauri::command]
fn launch(app: AppHandle, target: String, args: Option<String>) {
    close_flyout(&app);
    preview::hide_now(&app);
    off_ui(move || apps::launch(&target, args.as_deref()));
}

#[tauri::command]
fn get_settings(app: AppHandle) -> config::Settings {
    config::load(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: config::Settings) {
    config::save(&app, settings.clone());
    let _ = app.emit("settings-changed", settings.clone());
    if settings.floating == bars::is_fixed() {
        // reserve / release the screen strip from the UI thread, exactly like
        // at startup (from a worker thread Windows ignored the reservation and
        // maximized windows kept going under the bar)
        let h = app.clone();
        let fixed = !settings.floating;
        let _ = app.run_on_main_thread(move || bars::apply_mode(&h, fixed));
    }
    off_ui(move || autostart::apply(settings.autostart));
}

#[tauri::command]
async fn system_status() -> Option<system::SystemStatus> {
    tauri::async_runtime::spawn_blocking(system::status).await.ok()
}

// ---------------------------------------------------------------- backups --

#[tauri::command]
fn list_backups(app: AppHandle) -> Vec<backup::BackupInfo> {
    config::path(&app).map(|p| backup::list(&p)).unwrap_or_default()
}

/// Settings stored in an automatic copy (the frontend saves them back).
#[tauri::command]
fn read_backup(app: AppHandle, id: String) -> Result<config::Settings, String> {
    let p = config::path(&app).ok_or("cartella impostazioni non trovata")?;
    backup::read_copy(&p, &id)
}

#[tauri::command]
async fn export_settings(app: AppHandle) -> Result<bool, String> {
    close_flyout(&app); // the dialog takes the focus anyway
    let s = config::load(&app);
    tauri::async_runtime::spawn_blocking(move || backup::export(&s)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn import_settings(app: AppHandle) -> Result<Option<config::Settings>, String> {
    close_flyout(&app);
    tauri::async_runtime::spawn_blocking(backup::import).await.map_err(|e| e.to_string())?
}

/// Recent items of an app, for its right-click menu.
#[tauri::command]
async fn jump_list(hwnd: Option<isize>, target: String, exe: String) -> Vec<jumplist::JumpItem> {
    tauri::async_runtime::spawn_blocking(move || jumplist::items(hwnd, &target, &exe, 8))
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn app_volumes() -> Vec<mixer::AppVolume> {
    tauri::async_runtime::spawn_blocking(mixer::list).await.unwrap_or_default()
}

#[tauri::command]
async fn set_app_volume(pid: u32, volume: Option<f32>, muted: Option<bool>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || mixer::set(pid, volume, muted))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn wifi_networks() -> Result<wifi::WifiList, String> {
    tauri::async_runtime::spawn_blocking(wifi::scan).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn wifi_connect(ssid: String, password: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || wifi::connect(&ssid, password.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn wifi_disconnect() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(wifi::disconnect).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn audio_outputs() -> Vec<audio_out::AudioOut> {
    tauri::async_runtime::spawn_blocking(audio_out::outputs).await.unwrap_or_default()
}

#[tauri::command]
async fn set_audio_output(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || audio_out::set_output(&id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_volume(volume: Option<f32>, muted: Option<bool>) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        if let Some(v) = volume {
            system::set_volume(v);
        }
        if let Some(m) = muted {
            system::set_mute(m);
        }
    })
    .await;
}

#[tauri::command]
async fn media_info() -> Option<media::MediaInfo> {
    tauri::async_runtime::spawn_blocking(media::info).await.ok().flatten()
}

#[tauri::command]
async fn desktops() -> vdesk::Desktops {
    tauri::async_runtime::spawn_blocking(vdesk::read).await.unwrap_or_default()
}

/// Switch to desktop `index` (or a new one when `index` is null).
#[tauri::command]
fn switch_desktop(app: AppHandle, index: Option<usize>) {
    close_flyout(&app);
    std::thread::spawn(move || vdesk::switch(index.unwrap_or(usize::MAX)));
}

/// Cover art of the current track (data URL), asked once per song.
#[tauri::command]
async fn media_art() -> Option<String> {
    tauri::async_runtime::spawn_blocking(media::art).await.ok().flatten()
}

/// Colours of the desktop wallpaper, for the "Dallo sfondo" theme.
#[tauri::command]
async fn wallpaper_palette() -> Option<wallpaper::Palette> {
    tauri::async_runtime::spawn_blocking(wallpaper::palette).await.ok().flatten()
}

#[tauri::command]
async fn media_action(action: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || media::action(&action)).await;
}

/// Native Windows features reached through their keyboard shortcuts.
#[tauri::command]
fn shell_action(app: AppHandle, action: String) {
    close_flyout(&app);
    preview::hide_now(&app);
    let keys: &[VIRTUAL_KEY] = match action.as_str() {
        "start" => &[VK_LWIN],
        "search" => &[VK_LWIN, VK_S],
        "taskview" => &[VK_LWIN, VK_TAB],
        "quick" => &[VK_LWIN, VK_A],
        "notifications" => &[VK_LWIN, VK_N],
        "desktop" => &[VK_LWIN, VK_D],
        "winx" => &[VK_LWIN, VK_X],
        "taskmgr" => &[VK_CONTROL, VK_SHIFT, VK_ESCAPE],
        "settings" => {
            off_ui(|| apps::launch("ms-settings:", None));
            return;
        }
        // Alt+F4 on the bar, like on Windows' own: the "Shut down Windows" dialog
        "shutdown" => {
            std::thread::spawn(|| unsafe {
                use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx};
                use windows::Win32::UI::Shell::{IShellDispatch, Shell};
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                if let Ok(sh) = CoCreateInstance::<_, IShellDispatch>(&Shell, None, CLSCTX_INPROC_SERVER) {
                    let _ = sh.ShutdownWindows();
                }
            });
            return;
        }
        _ => return,
    };
    // Let the flyout disappear / focus settle first.
    let keys = keys.to_vec();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(60));
        wins::send_keys(&keys);
    });
}

#[tauri::command]
async fn radio_status() -> Option<radios::Radios> {
    tauri::async_runtime::spawn_blocking(radios::status).await.ok()
}

#[tauri::command]
async fn set_radio(kind: String, on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || radios::set(&kind, on))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tray_icons() -> Vec<tray::TrayItem> {
    tauri::async_runtime::spawn_blocking(tray::list).await.unwrap_or_default()
}

#[tauri::command]
async fn tray_click(app: AppHandle, id: String, kind: String, x: i32, y: i32) -> Result<(), String> {
    // Click FIRST, while our panel is still the foreground window: Windows
    // only lets the foreground process hand the foreground over to the app
    // (AllowSetForegroundWindow). Hiding the panel first made that fail now
    // and then, and the app's window stayed behind / never showed up.
    let hide = kind == "left" || kind == "double";
    let res = tauri::async_runtime::spawn_blocking(move || tray::click(&id, &kind, x, y))
        .await
        .map_err(|e| e.to_string())?;
    if hide {
        tauri::async_runtime::spawn(async move {
            tokio_sleep(250).await;
            close_flyout(&app);
        });
    }
    res
}

async fn tokio_sleep(ms: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(Duration::from_millis(ms))).await;
}

/// Frontend diagnostics (errors, lifecycle) printed in the dev console.
#[tauri::command]
fn log_js(window: tauri::Window, msg: String) {
    eprintln!("[js:{}] {}", window.label(), msg);
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    cleanup();
    app.exit(0);
}

/// Standalone recovery: native taskbar visible, no autostart.
pub fn restore_windows() {
    tray::stop(); // ask apps to re-add their tray icons to Explorer
    taskbar::force_show();
    autostart::remove();
}

// -------------------------------------------------------------------- run --

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_safety_nets();

    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            show_flyout,
            fit_flyout,
            system_accent,
            audio_outputs,
            set_audio_output,
            app_volumes,
            set_app_volume,
            wifi_networks,
            wifi_connect,
            wifi_disconnect,
            jump_list,
            media_art,
            wallpaper_palette,
            desktops,
            switch_desktop,
            list_backups,
            read_backup,
            export_settings,
            import_settings,
            hide_flyout,
            request_close_flyout,
            show_preview,
            set_thumbs,
            hide_preview,
            preview_hold,
            bar_info,
            set_region,
            get_windows,
            get_active,
            window_action,
            get_icon,
            get_window_icon,
            list_apps,
            resolve_shortcut,
            launch,
            get_settings,
            save_settings,
            system_status,
            set_volume,
            media_info,
            media_action,
            shell_action,
            quit_app,
            log_js,
            radio_status,
            set_radio,
            tray_icons,
            tray_click,
        ])
        .setup(|app| {
            // Hide the native taskbar(s), then one floating bar per monitor.
            taskbar::hide();
            let cfg = config::load(app.handle());
            autostart::apply(cfg.autostart);
            bars::apply_mode(app.handle(), !cfg.floating); // no bars yet: sets the mode
            bars::sync(app.handle());
            preview::make_noactivate(app.handle());
            tray::start(app.handle().clone());
            wins::start(app.handle().clone());
            vdesk::start(app.handle().clone());
            // Pre-load the launcher's app list in the background.
            std::thread::spawn(|| {
                apps::list();
            });

            // Watchdog: native taskbar hidden, monitors in sync, fullscreen apps.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut fs_monitor: Option<isize> = None;
                let mut sig = bars::signature();
                loop {
                    std::thread::sleep(Duration::from_millis(700));
                    taskbar::enforce();
                    tray::keep_on_top();

                    let now_sig = bars::signature();
                    if now_sig != sig {
                        sig = now_sig;
                        eprintln!("[bar] monitor layout changed");
                        let h = handle.clone();
                        let _ = handle.run_on_main_thread(move || bars::sync(&h));
                        continue;
                    }

                    let fs = wins::fullscreen_monitor();
                    bars::keep_in_place(fs);
                    if bars::needs_reserve() {
                        let _ = handle.run_on_main_thread(bars::keep_reserved);
                    }
                    if fs != fs_monitor {
                        eprintln!("[bar] fullscreen monitor: {fs:?}");
                        for label in bars::labels() {
                            let Some(win) = handle.get_webview_window(&label) else { continue };
                            let mon = bars::monitor_of(&label);
                            if fs.is_some() && mon == fs {
                                let _ = win.hide();
                            } else if fs_monitor.is_some() && mon == fs_monitor {
                                let _ = win.show();
                            }
                        }
                        fs_monitor = fs;
                        bars::raise_all();
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            let label = window.label();
            if label == "flyout" || label == "preview" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            if label.starts_with("bar-") {
                // Alt+F4 on a bar: quit and give Windows its taskbar back.
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    cleanup();
                    std::process::exit(0);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app, event| {
        if let RunEvent::Exit = event {
            cleanup();
        }
    });
}

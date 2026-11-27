//! One bar per monitor, in one of two modes:
//! - fixed (default): the bar reserves its strip like the native taskbar
//!   (AppBar), windows stop above it, and the whole strip is Windows acrylic;
//! - floating: no reserved space, windows go underneath, and a window region
//!   makes the gaps between the islands click-through.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW,
    HDC, HGDIOBJ, HMONITOR, MONITORINFO, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::{ABE_BOTTOM, ABM_NEW, ABM_REMOVE, ABM_SETPOS, APPBARDATA, SHAppBarMessage};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOOWNERZORDER, SWP_NOSIZE, SetWindowPos, WM_USER, WS_EX_TOPMOST,
};
use windows::core::BOOL;

/// Bar height in logical pixels (CSS px): floating keeps room above the
/// islands for the magnified icons, fixed is a compact 4px frame.
pub const FLOATING_HEIGHT: f64 = 64.0;
pub const FIXED_HEIGHT: f64 = 56.0;

static FIXED: AtomicBool = AtomicBool::new(true);

pub fn is_fixed() -> bool {
    FIXED.load(Ordering::SeqCst)
}

/// Distance from the bar window top to the islands, logical px.
pub fn island_top() -> f64 {
    if is_fixed() { 4.0 } else { 8.0 }
}

/// WebView2 flags shared by every window: one renderer process for all of
/// them (they are all our own trusted pages) instead of one each.
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --renderer-process-limit=1";

#[derive(Clone, Debug, PartialEq)]
pub struct Monitor {
    pub handle: isize,
    pub rect: RECT,
    pub scale: f64,
    pub primary: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BarInfo {
    pub monitor: isize,
    pub primary: bool,
}

struct Bar {
    hwnd: isize,
    rect: RECT,
    monitor: Monitor,
    info: BarInfo,
    appbar: bool,
}

static BARS: LazyLock<Mutex<HashMap<String, Bar>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn monitors() -> Vec<Monitor> {
    let mut out: Vec<Monitor> = Vec::new();
    unsafe extern "system" fn cb(hmon: HMONITOR, _hdc: HDC, _rc: *mut RECT, lparam: LPARAM) -> BOOL {
        let v = unsafe { &mut *(lparam.0 as *mut Vec<Monitor>) };
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        unsafe {
            if GetMonitorInfoW(hmon, &mut mi).as_bool() {
                let (mut dx, mut dy) = (96u32, 96u32);
                let _ = GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
                v.push(Monitor {
                    handle: hmon.0 as isize,
                    rect: mi.rcMonitor,
                    scale: dx as f64 / 96.0,
                    primary: mi.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
                });
            }
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

fn label_for(m: &Monitor) -> String {
    format!("bar-{}", m.handle)
}

fn bar_rect(m: &Monitor) -> RECT {
    let logical = if is_fixed() { FIXED_HEIGHT } else { FLOATING_HEIGHT };
    let h = (logical * m.scale).round() as i32;
    RECT { left: m.rect.left, top: m.rect.bottom - h, right: m.rect.right, bottom: m.rect.bottom }
}

fn place(hwnd: isize, rc: RECT) {
    unsafe {
        let _ = SetWindowPos(
            HWND(hwnd as *mut _),
            Some(HWND_TOPMOST),
            rc.left,
            rc.top,
            rc.right - rc.left,
            rc.bottom - rc.top,
            SWP_NOACTIVATE,
        );
    }
}

// --------------------------------------------------------------- appbar --

fn appbar_data(hwnd: isize) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: HWND(hwnd as *mut _),
        uCallbackMessage: WM_USER + 77,
        uEdge: ABE_BOTTOM,
        ..Default::default()
    }
}

/// Reserves `rc` (bottom strip of its monitor) like the native taskbar.
fn appbar_set(hwnd: isize, rc: RECT, registered: bool) {
    let mut abd = appbar_data(hwnd);
    // our tray host must not intercept these (it shares Explorer's class name)
    crate::tray::step_aside(|| unsafe {
        if !registered {
            SHAppBarMessage(ABM_NEW, &mut abd);
        }
        abd.rc = rc;
        SHAppBarMessage(ABM_SETPOS, &mut abd);
    });
}

fn appbar_remove(hwnd: isize) {
    let mut abd = appbar_data(hwnd);
    crate::tray::step_aside(|| unsafe {
        SHAppBarMessage(ABM_REMOVE, &mut abd);
    });
}

/// Gives back every reserved strip (on exit).
pub fn release_all() {
    for b in BARS.lock().unwrap().values_mut() {
        if b.appbar {
            appbar_remove(b.hwnd);
            b.appbar = false;
        }
    }
}

/// Applies the current mode to one bar: size, reserved space, blur, region.
fn apply_to(app: &AppHandle, label: &str) {
    let fixed = is_fixed();
    let Some((hwnd, mon, registered)) =
        BARS.lock().unwrap().get(label).map(|b| (b.hwnd, b.monitor.clone(), b.appbar))
    else {
        return;
    };
    let rect = bar_rect(&mon);
    if fixed {
        appbar_set(hwnd, rect, registered);
    } else if registered {
        appbar_remove(hwnd);
    }
    if let Some(b) = BARS.lock().unwrap().get_mut(label) {
        b.rect = rect;
        b.appbar = fixed;
    }
    place(hwnd, rect);
    if app.get_webview_window(label).is_some() {
        if fixed {
            // whole window clickable again, with the Windows blur behind it
            unsafe {
                SetWindowRgn(HWND(hwnd as *mut _), None, true);
            }
        }
        // the strip is solid now: no blur behind it (kept for future themes)
        set_blur(hwnd, false);
    }
}

/// Switches every bar between fixed and floating.
pub fn apply_mode(app: &AppHandle, fixed: bool) {
    FIXED.store(fixed, Ordering::SeqCst);
    for label in labels() {
        apply_to(app, &label);
    }
    // Windows doesn't re-fit windows that are already open: do it ourselves
    // (fixed: pull them above the bar; floating: maximized ones grow back)
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(400));
        fit_windows_above();
    });
}

/// Fits open windows to the current work area: shortens the ones running
/// into a reserved strip, and keeps maximized ones exactly on its bottom.
fn fit_windows_above() {
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, SWP_NOZORDER, WS_EX_TOOLWINDOW,
    };

    unsafe extern "system" fn cb(hwnd: HWND, _: LPARAM) -> BOOL {
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == std::process::id()
                || !IsWindowVisible(hwnd).as_bool()
                || IsIconic(hwnd).as_bool()
                || GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0
            {
                return BOOL(1);
            }
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4);
            if cloaked != 0 {
                return BOOL(1);
            }
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if !GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
                return BOOL(1);
            }
            let work = mi.rcWork;
            let mut r = RECT::default();
            let _ = GetWindowRect(hwnd, &mut r);
            // visible frame (without the invisible resize borders)
            let mut vis = r;
            let _ = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut vis as *mut _ as *mut _,
                std::mem::size_of::<RECT>() as u32,
            );
            let over = vis.bottom - work.bottom;
            // Windows does NOT re-fit already-maximized windows when the
            // reserved strip changes (fixed <-> floating): keep them glued to
            // the work area's bottom, both ways. Other windows: only shorten
            // the ones that END inside the strip (not ones off the screen).
            let fix = if IsZoomed(hwnd).as_bool() {
                over.abs() > 1
            } else {
                over > 1 && vis.bottom <= mi.rcMonitor.bottom + 2 && vis.top < work.bottom - 120
            };
            if fix {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top - over,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
                );
            }
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(0));
    }
}

// ----------------------------------------------------------------- blur --

/// Frosted, see-through blur behind the whole window.
///
/// Windows 11 "materials" (Mica / Acrylic backdrops) only render while the
/// window is active and fall back to a flat grey otherwise, and a taskbar is
/// never active. The (undocumented, but stable since Windows 10) accent
/// policy used by the shell itself and by tools like TranslucentTB keeps the
/// blur on for inactive windows too.
fn set_blur(hwnd: isize, on: bool) {
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows::core::{s, w};

    #[repr(C)]
    struct AccentPolicy {
        state: u32,
        flags: u32,
        gradient: u32, // AABBGGRR tint
        animation: u32,
    }
    #[repr(C)]
    struct CompositionData {
        attribute: u32,
        data: *mut std::ffi::c_void,
        size: usize,
    }
    type Swca = unsafe extern "system" fn(HWND, *mut CompositionData) -> BOOL;

    const WCA_ACCENT_POLICY: u32 = 19;
    const ACCENT_DISABLED: u32 = 0;
    const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;

    unsafe {
        let Ok(user32) = GetModuleHandleW(w!("user32.dll")) else { return };
        let Some(f) = GetProcAddress(user32, s!("SetWindowCompositionAttribute")) else { return };
        let swca: Swca = std::mem::transmute(f);
        let mut policy = AccentPolicy {
            state: if on { ACCENT_ENABLE_ACRYLICBLURBEHIND } else { ACCENT_DISABLED },
            flags: 0,
            // very light, slightly cool tint: mostly see-through
            gradient: 0x1A_20_14_14,
            animation: 0,
        };
        let mut data = CompositionData {
            attribute: WCA_ACCENT_POLICY,
            data: &mut policy as *mut _ as *mut _,
            size: std::mem::size_of::<AccentPolicy>(),
        };
        let _ = swca(HWND(hwnd as *mut _), &mut data);
    }
}

// ------------------------------------------------------------------ bars --

fn create(app: &AppHandle, m: &Monitor) -> tauri::Result<WebviewWindow> {
    let label = label_for(m);
    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("/".into()))
        .title("Iacobar")
        .decorations(false)
        .transparent(true)
        // explicit transparent background: WebView2's default is WHITE, and
        // it shows for a frame while resizing / changing monitor DPI
        .background_color(tauri::window::Color(0, 0, 0, 0))
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .visible(false)
        // must match the windows in tauri.conf.json (one shared WebView2 env)
        .additional_browser_args(BROWSER_ARGS)
        .build()?;
    let hwnd = win.hwnd()?.0 as isize;
    let rect = bar_rect(m);
    place(hwnd, rect);
    win.show()?;
    place(hwnd, rect);
    eprintln!("[bar] {label} primary={} scale={} rect={},{},{},{}", m.primary, m.scale, rect.left, rect.top, rect.right, rect.bottom);
    BARS.lock().unwrap().insert(
        label.clone(),
        Bar { hwnd, rect, monitor: m.clone(), info: BarInfo { monitor: m.handle, primary: m.primary }, appbar: false },
    );
    apply_to(app, &label);
    Ok(win)
}

/// Creates / removes / re-places bars so there is exactly one per monitor.
/// Must run on the main thread.
pub fn sync(app: &AppHandle) {
    let mons = monitors();
    let wanted: HashMap<String, &Monitor> = mons.iter().map(|m| (label_for(m), m)).collect();

    // remove bars of unplugged monitors
    let stale: Vec<String> = BARS.lock().unwrap().keys().filter(|l| !wanted.contains_key(*l)).cloned().collect();
    for label in stale {
        if let Some(b) = BARS.lock().unwrap().remove(&label) {
            if b.appbar {
                appbar_remove(b.hwnd);
            }
        }
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.destroy();
        }
    }
    for (label, m) in wanted {
        let existing = BARS.lock().unwrap().get(&label).map(|b| b.hwnd);
        match existing {
            Some(_) => {
                if let Some(b) = BARS.lock().unwrap().get_mut(&label) {
                    b.monitor = m.clone();
                    b.info.primary = m.primary;
                }
                apply_to(app, &label);
            }
            None => {
                if let Err(e) = create(app, m) {
                    eprintln!("[bar] cannot create {label}: {e}");
                }
            }
        }
    }
}

/// Cheap signature of the monitor layout, to detect plug/unplug/resolution.
pub fn signature() -> Vec<(isize, i32, i32, i32, i32, u32)> {
    monitors()
        .iter()
        .map(|m| (m.handle, m.rect.left, m.rect.top, m.rect.right, m.rect.bottom, (m.scale * 100.0) as u32))
        .collect()
}

pub fn info(label: &str) -> Option<BarInfo> {
    BARS.lock().unwrap().get(label).map(|b| b.info.clone())
}

pub fn labels() -> Vec<String> {
    BARS.lock().unwrap().keys().cloned().collect()
}

pub fn monitor_of(label: &str) -> Option<isize> {
    BARS.lock().unwrap().get(label).map(|b| b.info.monitor)
}

/// Puts every bar back at the top of the "always on top" band, without
/// moving, resizing or activating it. Asynchronous: never blocks the caller.
pub fn raise_all() {
    let hwnds: Vec<isize> = BARS.lock().unwrap().values().map(|b| b.hwnd).collect();
    for hwnd in hwnds {
        raise(hwnd);
    }
}

fn raise(hwnd: isize) {
    unsafe {
        let _ = SetWindowPos(
            HWND(hwnd as *mut _),
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
        );
    }
}

fn is_topmost(hwnd: isize) -> bool {
    unsafe { GetWindowLongPtrW(HWND(hwnd as *mut _), GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0 }
}

/// Re-applies positions if something moved a bar, and the "always on top"
/// flag if Windows took it away (fullscreen apps, Show desktop, ...).
pub fn keep_in_place(skip_monitor: Option<isize>) {
    let bars: Vec<(isize, RECT, isize)> =
        BARS.lock().unwrap().values().map(|b| (b.hwnd, b.rect, b.info.monitor)).collect();
    for (hwnd, rc, mon) in bars {
        if Some(mon) == skip_monitor {
            continue;
        }
        let mut cur = RECT::default();
        unsafe {
            let _ = GetWindowRect(HWND(hwnd as *mut _), &mut cur);
        }
        if cur != rc {
            place(hwnd, rc);
        } else if !is_topmost(hwnd) {
            eprintln!("[bar] lost topmost, raising again");
            raise(hwnd);
        }
    }
}

/// Bars whose strip is missing from their monitor's work area.
fn strip_lost() -> Vec<(isize, RECT, bool)> {
    use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
    let bars: Vec<(isize, RECT, bool)> = BARS.lock().unwrap().values().map(|b| (b.hwnd, b.rect, b.appbar)).collect();
    bars.into_iter()
        .filter(|(hwnd, rc, _)| {
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let ok = unsafe { GetMonitorInfoW(MonitorFromWindow(HWND(*hwnd as *mut _), MONITOR_DEFAULTTONEAREST), &mut mi) };
            ok.as_bool() && mi.rcWork.bottom > rc.top + 1
        })
        .collect()
}

/// Windows sometimes recomputes the work area without our strip (e.g. when
/// the native taskbar's auto-hide gets switched on after we registered):
/// windows would then go under the bar. Cheap check, called by the watchdog;
/// the re-registration itself runs on the UI thread, at most every 3 s.
pub fn needs_reserve() -> bool {
    use std::sync::atomic::AtomicU64;
    static LAST: AtomicU64 = AtomicU64::new(0);
    if !is_fixed() || strip_lost().is_empty() {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    if now.saturating_sub(LAST.load(Ordering::SeqCst)) < 3000 {
        return false;
    }
    LAST.store(now, Ordering::SeqCst);
    true
}

/// Re-reserves the strips Windows dropped. Must run on the main thread.
pub fn keep_reserved() {
    let mut fixed_any = false;
    for (hwnd, rc, registered) in strip_lost() {
        eprintln!("[bar] work area lost the strip above y={}, reserving again", rc.top);
        if registered {
            appbar_remove(hwnd);
        }
        appbar_set(hwnd, rc, false);
        place(hwnd, rc);
        if let Some(b) = BARS.lock().unwrap().values_mut().find(|b| b.hwnd == hwnd) {
            b.appbar = true;
        }
        fixed_any = true;
    }
    if fixed_any {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(400));
            fit_windows_above();
        });
    }
}

// ----------------------------------------------------------- hit regions --

#[derive(Deserialize, Debug)]
pub struct Shape {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// corner radius (0 = square)
    pub r: f64,
}

/// Restricts the window (drawing + mouse) to the given shapes, so clicks
/// between the islands reach the applications underneath.
pub fn set_region(win: &WebviewWindow, shapes: &[Shape]) {
    let Ok(hwnd) = win.hwnd() else { return };
    if shapes.is_empty() || is_fixed() {
        // fixed bar: the whole strip is ours (and must stay blurred)
        unsafe {
            SetWindowRgn(HWND(hwnd.0 as *mut _), None, true);
        }
        return;
    }
    let s = win.scale_factor().unwrap_or(1.0);
    let px = |v: f64| (v * s).round() as i32;
    unsafe {
        let total = CreateRectRgn(0, 0, 0, 0);
        for sh in shapes {
            let part = if sh.r > 0.0 {
                CreateRoundRectRgn(px(sh.x), px(sh.y), px(sh.x + sh.w) + 1, px(sh.y + sh.h) + 1, px(sh.r * 2.0), px(sh.r * 2.0))
            } else {
                CreateRectRgn(px(sh.x), px(sh.y), px(sh.x + sh.w), px(sh.y + sh.h))
            };
            CombineRgn(Some(total), Some(total), Some(part), RGN_OR);
            let _ = DeleteObject(HGDIOBJ(part.0));
        }
        // The system owns the region after this call.
        SetWindowRgn(HWND(hwnd.0 as *mut _), Some(total), true);
    }
}

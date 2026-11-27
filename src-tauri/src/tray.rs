//! Notification-area (system tray) host.
//!
//! Apps add their tray icons with `Shell_NotifyIcon`, which sends a
//! WM_COPYDATA to the first top-level window of class "Shell_TrayWnd". We
//! create such a window (kept above Explorer's in z-order), so the icons come
//! to us; every message is also forwarded to Explorer, so Windows keeps a
//! consistent state and gets everything back when we quit. Clicks on our
//! icons are sent to the owning app exactly like Explorer would.
//! Same technique as other shell replacements (e.g. ManagedShell/RetroBar).

use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, CopyIcon, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow,
    DispatchMessageW, GetMessageW, GetWindowThreadProcessId, HICON, HWND_BOTTOM, HWND_BROADCAST, HWND_TOPMOST, IsWindow,
    MSG, MSGFLT_ALLOW, PostMessageW, RegisterClassW, RegisterWindowMessageW, SMTO_ABORTIFHUNG,
    SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageTimeoutW, SendNotifyMessageW, SetWindowPos,
    TranslateMessage, WINDOW_EX_STYLE, WM_CLOSE, WM_CONTEXTMENU, WM_COPYDATA, WM_DESTROY,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_USER, WNDCLASSW,
    WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, ChangeWindowMessageFilterEx,
};
use windows::core::{PCWSTR, w};

const TRAY_MAGIC: u32 = 0x3475_3423;
const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIM_SETVERSION: u32 = 4;
const NIF_MESSAGE: u32 = 0x01;
const NIF_ICON: u32 = 0x02;
const NIF_TIP: u32 = 0x04;
const NIF_STATE: u32 = 0x08;
const NIF_GUID: u32 = 0x20;
const NIS_HIDDEN: u32 = 0x01;
const NIN_SELECT: u32 = WM_USER;

/// NOTIFYICONDATAW as it travels in WM_COPYDATA: handles are always 32-bit.
#[repr(C)]
#[derive(Clone, Copy)]
struct Nid32 {
    cb_size: u32,
    hwnd: u32,
    uid: u32,
    flags: u32,
    callback: u32,
    hicon: u32,
    tip: [u16; 128],
    state: u32,
    state_mask: u32,
    info: [u16; 256],
    version: u32, // union with uTimeout
    info_title: [u16; 64],
    info_flags: u32,
    guid: [u8; 16],
    balloon_icon: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TrayData {
    magic: u32,
    message: u32,
    nid: Nid32,
}

struct Icon {
    id: String,
    hwnd: isize,
    uid: u32,
    callback: u32,
    version: u32,
    hicon: isize, // our own copy
    png: Option<String>,
    tip: String,
    hidden: bool,
}

#[derive(Serialize, Clone)]
pub struct TrayItem {
    pub id: String,
    pub tip: String,
    pub icon: Option<String>,
}

static ICONS: LazyLock<Mutex<Vec<Icon>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static APP: OnceLock<AppHandle> = OnceLock::new();
static HOST: AtomicIsize = AtomicIsize::new(0);

fn wstr(buf: &[u16]) -> String {
    let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

fn explorer() -> Option<HWND> {
    crate::taskbar::explorer_tray()
}

fn forward(msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(ex) = explorer() else { return LRESULT(0) };
    // never bounce Explorer's own messages back to it (ping-pong = stalls)
    let mut sender_pid = 0u32;
    let mut ex_pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(HWND(wparam.0 as *mut _), Some(&mut sender_pid));
        GetWindowThreadProcessId(ex, Some(&mut ex_pid));
    }
    if sender_pid != 0 && sender_pid == ex_pid {
        return LRESULT(0);
    }
    let mut res = 0usize;
    unsafe {
        SendMessageTimeoutW(ex, msg, wparam, lparam, SMTO_ABORTIFHUNG, 500, Some(&mut res));
    }
    LRESULT(res as isize)
}

fn owner_exe(hwnd: isize) -> Option<String> {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
    }
    let exe = crate::wins::exe_of_pid(pid)?;
    Some(exe.rsplit('\\').next().unwrap_or(&exe).to_lowercase())
}

/// Orphans are looked up in the registry + process list: refresh them at
/// most every few seconds (icons can update many times a second).
static ORPHANS: LazyLock<Mutex<(Option<std::time::Instant>, Vec<crate::tray_orphans::Orphan>)>> =
    LazyLock::new(|| Mutex::new((None, Vec::new())));

pub fn list() -> Vec<TrayItem> {
    let mut icons = ICONS.lock().unwrap();
    // apps that died without removing their icon
    icons.retain(|i| unsafe { IsWindow(Some(HWND(i.hwnd as *mut _))).as_bool() });
    let mut items: Vec<TrayItem> = icons
        .iter()
        .filter(|i| !i.hidden)
        .map(|i| TrayItem { id: i.id.clone(), tip: i.tip.clone(), icon: i.png.clone() })
        .collect();
    let have: std::collections::HashSet<String> = icons.iter().filter_map(|i| owner_exe(i.hwnd)).collect();
    drop(icons);

    let mut cache = ORPHANS.lock().unwrap();
    if cache.0.map(|t| t.elapsed() > Duration::from_secs(5)).unwrap_or(true) {
        *cache = (Some(std::time::Instant::now()), crate::tray_orphans::list(&have));
    }
    items.extend(
        cache
            .1
            .iter()
            .filter(|o| !have.contains(&o.exe))
            .map(|o| TrayItem { id: format!("orphan:{}", o.exe), tip: o.tip.clone(), icon: o.icon.clone() }),
    );
    items
}

fn notify() {
    if let Some(app) = APP.get() {
        let _ = app.emit("tray-changed", list());
    }
    write_diagnostics();
}

/// %TEMP%\iacobar-tray.txt: every icon received (hidden ones too),
/// to understand why an app might be missing from the panel.
fn write_diagnostics() {
    let icons = ICONS.lock().unwrap();
    let mut out = String::from("app\tpid\tnascosta\timmagine\tversione\ttooltip\n");
    for i in icons.iter() {
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(HWND(i.hwnd as *mut _), Some(&mut pid));
        }
        let exe = crate::wins::exe_of_pid(pid).unwrap_or_else(|| "?".into());
        let name = exe.rsplit('\\').next().unwrap_or(&exe).to_string();
        out.push_str(&format!(
            "{name}\t{pid}\t{}\t{}\t{}\t{}\n",
            if i.hidden { "si" } else { "no" },
            if i.png.is_some() { "si" } else { "no" },
            i.version,
            i.tip.replace(['\n', '\r', '\t'], " ")
        ));
    }
    let _ = std::fs::write(std::env::temp_dir().join("iacobar-tray.txt"), out);
}

/// Applies one Shell_NotifyIcon call to our icon list.
fn handle(data: &TrayData) {
    if data.magic != TRAY_MAGIC {
        return;
    }
    let nid = &data.nid;
    // fields beyond cbSize are garbage for old callers
    let guid_end = std::mem::offset_of!(Nid32, guid) + 16;
    let has_guid = nid.flags & NIF_GUID != 0 && nid.cb_size as usize >= guid_end;
    let id = if has_guid {
        nid.guid.iter().map(|b| format!("{b:02x}")).collect::<String>()
    } else {
        format!("{}:{}", nid.hwnd, nid.uid)
    };
    let mut icons = ICONS.lock().unwrap();
    match data.message {
        NIM_ADD | NIM_MODIFY => {
            let idx = match icons.iter().position(|i| i.id == id) {
                Some(i) => i,
                None => {
                    icons.push(Icon {
                        id: id.clone(),
                        hwnd: nid.hwnd as isize,
                        uid: nid.uid,
                        callback: 0,
                        version: 0,
                        hicon: 0,
                        png: None,
                        tip: String::new(),
                        hidden: false,
                    });
                    icons.len() - 1
                }
            };
            let ic = &mut icons[idx];
            ic.hwnd = nid.hwnd as isize;
            ic.uid = nid.uid;
            if nid.flags & NIF_MESSAGE != 0 {
                ic.callback = nid.callback;
            }
            if nid.flags & NIF_ICON != 0 && nid.hicon != 0 {
                unsafe {
                    if let Ok(copy) = CopyIcon(HICON(nid.hicon as isize as *mut _)) {
                        if ic.hicon != 0 {
                            let _ = DestroyIcon(HICON(ic.hicon as *mut _));
                        }
                        ic.hicon = copy.0 as isize;
                        ic.png = crate::icons::hicon_to_data_url(copy);
                    }
                }
            }
            if nid.flags & NIF_TIP != 0 {
                ic.tip = wstr(&nid.tip);
            }
            if nid.flags & NIF_STATE != 0 && nid.state_mask & NIS_HIDDEN != 0 {
                ic.hidden = nid.state & NIS_HIDDEN != 0;
            }
        }
        NIM_DELETE => {
            if let Some(i) = icons.iter().position(|i| i.id == id) {
                let ic = icons.remove(i);
                if ic.hicon != 0 {
                    unsafe {
                        let _ = DestroyIcon(HICON(ic.hicon as *mut _));
                    }
                }
            }
        }
        NIM_SETVERSION => {
            if let Some(ic) = icons.iter_mut().find(|i| i.id == id) {
                ic.version = nid.version;
            }
        }
        _ => {}
    }
    drop(icons);
    notify();
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_COPYDATA && lparam.0 != 0 {
        let cds = unsafe { &*(lparam.0 as *const COPYDATASTRUCT) };
        if cds.dwData == 1 && !cds.lpData.is_null() {
            // Shell_NotifyIcon: read what fits, the rest stays zeroed
            let mut data: TrayData = unsafe { std::mem::zeroed() };
            let n = (cds.cbData as usize).min(std::mem::size_of::<TrayData>());
            unsafe {
                std::ptr::copy_nonoverlapping(cds.lpData as *const u8, &mut data as *mut _ as *mut u8, n);
            }
            handle(&data);
            let _ = forward(msg, wparam, lparam);
            return LRESULT(1);
        }
        // AppBar messages (our own bars included), icon rects, ...: Explorer's job
        return forward(msg, wparam, lparam);
    }
    if msg == WM_DESTROY {
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn broadcast_taskbar_created() {
    unsafe {
        let m = RegisterWindowMessageW(w!("TaskbarCreated"));
        let _ = SendNotifyMessageW(HWND_BROADCAST, m, WPARAM(0), LPARAM(0));
    }
}

/// Creates the host window on its own thread and asks every app to
/// (re-)add its icons.
pub fn start(app: AppHandle) {
    let _ = APP.set(app);
    std::thread::spawn(|| unsafe {
        let Ok(hinst) = GetModuleHandleW(None) else { return };
        let class = w!("Shell_TrayWnd");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let child_class = w!("TrayNotifyWnd");
        let wc2 = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: child_class,
            ..Default::default()
        };
        RegisterClassW(&wc2);

        let Ok(host) = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class,
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst.into()),
            None,
        ) else {
            return;
        };
        // some apps look for this child to place their popups
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            child_class,
            PCWSTR::null(),
            WS_CHILD,
            0,
            0,
            0,
            0,
            Some(host),
            None,
            Some(hinst.into()),
            None,
        );
        // elevated apps (VPNs, drivers' tools) may also send us their icons
        let _ = ChangeWindowMessageFilterEx(host, WM_COPYDATA, MSGFLT_ALLOW, None);
        HOST.store(host.0 as isize, Ordering::SeqCst);
        keep_on_top();
        broadcast_taskbar_created();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

/// True while our host steps aside (see `step_aside`).
static ASIDE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static ASIDE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs `f` while Explorer's Shell_TrayWnd is the one apps find.
///
/// SHAppBarMessage (reserving the bar's strip) talks to the first
/// "Shell_TrayWnd" window, which is normally our tray host: Explorer then
/// never got our AppBar requests and windows went under the fixed bar.
pub fn step_aside<R>(f: impl FnOnce() -> R) -> R {
    let h = HOST.load(Ordering::SeqCst);
    if h == 0 {
        return f();
    }
    let _guard = ASIDE_LOCK.lock().unwrap();
    ASIDE.store(true, Ordering::SeqCst);
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
    unsafe {
        // HWND_BOTTOM also drops the topmost flag: Explorer's window is first
        let _ = SetWindowPos(HWND(h as *mut _), Some(HWND_BOTTOM), 0, 0, 0, 0, flags);
    }
    let out = f();
    unsafe {
        let _ = SetWindowPos(HWND(h as *mut _), Some(HWND_TOPMOST), 0, 0, 0, 0, flags);
    }
    ASIDE.store(false, Ordering::SeqCst);
    out
}

/// Explorer may move its own Shell_TrayWnd above ours: stay first so
/// Shell_NotifyIcon keeps finding us.
pub fn keep_on_top() {
    let h = HOST.load(Ordering::SeqCst);
    if h != 0 && !ASIDE.load(Ordering::SeqCst) {
        unsafe {
            let _ = SetWindowPos(
                HWND(h as *mut _),
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
            );
        }
    }
}

/// Gives the icons back to Explorer (on exit).
pub fn stop() {
    let h = HOST.swap(0, Ordering::SeqCst);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        std::thread::sleep(Duration::from_millis(150));
        if unsafe { IsWindow(Some(HWND(h as *mut _))).as_bool() } {
            unsafe {
                let _ = DestroyWindow(HWND(h as *mut _));
            }
        }
    }
    broadcast_taskbar_created();
}

/// Sends a click to the app that owns the icon, like Explorer does.
/// `x`, `y`: screen position (physical px) used by apps to place menus.
pub fn click(id: &str, kind: &str, x: i32, y: i32) -> Result<(), String> {
    if let Some(exe) = id.strip_prefix("orphan:") {
        if kind == "right" {
            return Err("Per questa app il menu non è disponibile: clic sinistro per aprirla.".into());
        }
        return crate::tray_orphans::open(exe);
    }
    let (hwnd, uid, callback, version) = {
        let icons = ICONS.lock().unwrap();
        let ic = icons.iter().find(|i| i.id == id).ok_or("icona non trovata")?;
        (ic.hwnd, ic.uid, ic.callback, ic.version)
    };
    if callback == 0 {
        return Err("l'app non accetta clic".into());
    }
    let target = HWND(hwnd as *mut _);
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(target, Some(&mut pid));
        let _ = AllowSetForegroundWindow(pid); // so it can show its menu/window in front
    }
    let send = |m: u32| {
        let (wp, lp) = if version >= 4 {
            (((x as u32 & 0xffff) | ((y as u32 & 0xffff) << 16)) as usize, ((m & 0xffff) | (uid << 16)) as isize)
        } else {
            (uid as usize, m as isize)
        };
        unsafe {
            let _ = SendNotifyMessageW(target, callback, WPARAM(wp), LPARAM(lp));
        }
    };
    match kind {
        "right" => {
            send(WM_RBUTTONDOWN);
            send(WM_RBUTTONUP);
            if version >= 4 {
                send(WM_CONTEXTMENU);
            }
        }
        "double" => {
            send(WM_LBUTTONDBLCLK);
            send(WM_LBUTTONUP);
        }
        _ => {
            send(WM_LBUTTONDOWN);
            send(WM_LBUTTONUP);
            if version >= 4 {
                send(NIN_SELECT);
            }
        }
    }
    Ok(())
}

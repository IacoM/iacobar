//! Tracks top-level application windows (what the taskbar shows) using
//! WinEvent hooks, and exposes actions: activate, minimize, close.

use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY, VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, ShowWindowAsync,
    DispatchMessageW, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE,
    EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, EnumChildWindows, EnumWindows,
    GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow, GetMessageW, GetWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsZoomed, IsWindowVisible, MSG, PostMessageW, SW_MINIMIZE,
    SW_RESTORE, SetForegroundWindow, TranslateMessage, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_CLOSE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};
use windows::core::BOOL;

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WinInfo {
    pub hwnd: isize,
    pub title: String,
    /// Full path of the executable that owns the window.
    pub exe: String,
    /// Grouping key (lower-case exe path).
    pub app_id: String,
    pub minimized: bool,
    /// HMONITOR the window is (mostly) on.
    pub monitor: isize,
    /// The app asked for attention (FlashWindow: new message, download done...)
    /// and hasn't been looked at yet.
    pub attention: bool,
}

enum Ev {
    Changed,
    Foreground(isize),
}

/// Windows that flashed and haven't been activated since.
static ATTENTION: std::sync::LazyLock<Mutex<std::collections::HashSet<isize>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));
static SHELLHOOK_MSG: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Hidden window registered as a shell hook: Windows tells it when an app
/// flashes its taskbar button (the same signal the native taskbar gets).
use windows::Win32::Foundation::{LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::DefWindowProcW;

unsafe extern "system" fn shell_hook_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    const HSHELL_WINDOWACTIVATED: usize = 4;
    const HSHELL_RUDEAPPACTIVATED: usize = 0x8004;
    const HSHELL_FLASH: usize = 0x8006;
    let id = SHELLHOOK_MSG.load(Ordering::SeqCst);
    if id != 0 && msg == id {
        let target = lparam.0 as isize;
        let changed = match wparam.0 {
            HSHELL_FLASH => ATTENTION.lock().unwrap().insert(target),
            HSHELL_WINDOWACTIVATED | HSHELL_RUDEAPPACTIVATED => ATTENTION.lock().unwrap().remove(&target),
            _ => false,
        };
        if changed {
            if let Some(tx) = TX.get() {
                let _ = tx.lock().unwrap().send(Ev::Changed);
            }
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn create_shell_hook() {
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, RegisterClassW, RegisterShellHookWindow, RegisterWindowMessageW, WNDCLASSW, WS_EX_TOOLWINDOW,
        WS_POPUP,
    };
    use windows::core::w;
    unsafe {
        let Ok(inst) = GetModuleHandleW(None) else { return };
        let class = w!("IacobarShellHook");
        let wc = WNDCLASSW { lpfnWndProc: Some(shell_hook_proc), hInstance: inst.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        let Ok(h) = CreateWindowExW(WS_EX_TOOLWINDOW, class, w!(""), WS_POPUP, 0, 0, 0, 0, None, None, Some(inst.into()), None)
        else {
            return;
        };
        SHELLHOOK_MSG.store(RegisterWindowMessageW(w!("SHELLHOOK")), Ordering::SeqCst);
        if !RegisterShellHookWindow(h).as_bool() {
            eprintln!("[wins] RegisterShellHookWindow failed");
        }
    }
}

static TX: OnceLock<Mutex<Sender<Ev>>> = OnceLock::new();
static LAST_FG: AtomicIsize = AtomicIsize::new(0);

fn hwnd_of(v: isize) -> HWND {
    HWND(v as *mut _)
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n])
}

fn title_of(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf) as usize;
        String::from_utf16_lossy(&buf[..n])
    }
}

static EXE_CACHE: std::sync::LazyLock<Mutex<std::collections::HashMap<u32, String>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Executable path of a process (cached: windows are re-enumerated often).
pub fn exe_of_pid(pid: u32) -> Option<String> {
    if let Some(e) = EXE_CACHE.lock().unwrap().get(&pid) {
        return Some(e.clone());
    }
    let exe = exe_of_pid_uncached(pid)?;
    let mut c = EXE_CACHE.lock().unwrap();
    if c.len() > 512 {
        c.clear(); // pids get reused: start over now and then
    }
    c.insert(pid, exe.clone());
    Some(exe)
}

fn exe_of_pid_uncached(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let r = QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(h);
        r.ok()?;
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    }
}

/// UWP apps live inside ApplicationFrameHost.exe: find the real process
/// from the hosted CoreWindow child.
fn uwp_real_exe(frame: HWND) -> Option<String> {
    let frame_pid = pid_of(frame);
    let mut found: (u32, u32) = (frame_pid, 0);
    unsafe extern "system" fn cb(child: HWND, lparam: LPARAM) -> BOOL {
        let data = unsafe { &mut *(lparam.0 as *mut (u32, u32)) };
        let pid = pid_of(child);
        if pid != data.0 {
            data.1 = pid;
            return BOOL(0);
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumChildWindows(Some(frame), Some(cb), LPARAM(&mut found as *mut _ as isize));
    }
    if found.1 != 0 { exe_of_pid(found.1) } else { None }
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
    }
}

fn is_task_window(hwnd: HWND, own_pid: u32) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let app_window = ex & WS_EX_APPWINDOW.0 != 0;
        let owner = GetWindow(hwnd, GW_OWNER).unwrap_or_default();
        if !owner.is_invalid() && !app_window {
            return false;
        }
        if ex & WS_EX_TOOLWINDOW.0 != 0 && !app_window {
            return false;
        }
    }
    if pid_of(hwnd) == own_pid || is_cloaked(hwnd) {
        return false;
    }
    let class = class_of(hwnd);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return false;
    }
    unsafe { GetWindowTextLengthW(hwnd) > 0 }
}

pub fn enumerate() -> Vec<WinInfo> {
    let mut hwnds: Vec<HWND> = Vec::new();
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let v = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        v.push(hwnd);
        BOOL(1)
    }
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut hwnds as *mut _ as isize));
    }
    let own = std::process::id();
    let attention = ATTENTION.lock().unwrap().clone();
    let mut out = Vec::new();
    for hwnd in hwnds {
        if !is_task_window(hwnd, own) {
            continue;
        }
        let mut exe = exe_of_pid(pid_of(hwnd)).unwrap_or_default();
        if exe.to_lowercase().ends_with("applicationframehost.exe") {
            if let Some(real) = uwp_real_exe(hwnd) {
                exe = real;
            }
        }
        out.push(WinInfo {
            hwnd: hwnd.0 as isize,
            title: title_of(hwnd),
            // elevated processes hide their path: keep each window on its own
            app_id: if exe.is_empty() { format!("hwnd:{}", hwnd.0 as isize) } else { exe.to_lowercase() },
            exe,
            minimized: unsafe { IsIconic(hwnd).as_bool() },
            monitor: unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST).0 as isize },
            attention: attention.contains(&(hwnd.0 as isize)),
        });
    }
    // EnumWindows is z-ordered; keep a stable order by creation instead.
    out.sort_by_key(|w| w.hwnd);
    out
}

pub fn active() -> isize {
    LAST_FG.load(Ordering::SeqCst)
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if id_object != 0 || hwnd.is_invalid() {
        return; // only OBJID_WINDOW
    }
    // Ignore child windows / controls: apps like Task Manager fire a storm
    // of events for their internals, we only care about top-level windows.
    if event != EVENT_SYSTEM_FOREGROUND && unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
        return;
    }
    if event == EVENT_SYSTEM_FOREGROUND {
        // another window came forward: make sure no app slid above the bars
        crate::bars::raise_all();
    }
    if let Some(tx) = TX.get() {
        let tx = tx.lock().unwrap();
        if event == EVENT_SYSTEM_FOREGROUND {
            let _ = tx.send(Ev::Foreground(hwnd.0 as isize));
        } else {
            let _ = tx.send(Ev::Changed);
        }
    }
}

/// Starts the hook thread and the worker that emits `windows-changed`
/// and `active-changed` events to the frontend.
pub fn start(app: AppHandle) {
    let (tx, rx) = channel::<Ev>();
    let _ = TX.set(Mutex::new(tx));

    let fg = unsafe { GetForegroundWindow() };
    if pid_of(fg) != std::process::id() {
        LAST_FG.store(fg.0 as isize, Ordering::SeqCst);
    }

    // Hook thread (needs its own message loop).
    std::thread::spawn(|| unsafe {
        create_shell_hook(); // its messages are pumped by the loop below
        let ranges = [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
            (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
            (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
        ];
        for (min, max) in ranges {
            SetWinEventHook(
                min,
                max,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });

    // Worker: debounce events, diff, emit.
    std::thread::spawn(move || {
        let mut last: Vec<WinInfo> = Vec::new();
        let mut last_active = 0isize;
        let mut dirty = true;
        loop {
            match rx.recv_timeout(Duration::from_millis(if dirty { 120 } else { 2000 })) {
                Ok(Ev::Foreground(h)) => {
                    if pid_of(hwnd_of(h)) != std::process::id() {
                        LAST_FG.store(h, Ordering::SeqCst);
                    }
                    // looked at: no longer asking for attention
                    ATTENTION.lock().unwrap().remove(&h);
                    dirty = true;
                    continue;
                }
                Ok(Ev::Changed) => {
                    dirty = true;
                    continue;
                }
                Err(_) => {} // timeout: flush
            }
            let now = enumerate();
            if now != last {
                let _ = app.emit("windows-changed", &now);
                last = now;
            }
            let act = active();
            if act != last_active {
                let _ = app.emit("active-changed", act);
                last_active = act;
            }
            dirty = false;
        }
    });
}

/// Trick to be allowed to call SetForegroundWindow: a synthetic Alt tap.
fn tap_alt() {
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VK_MENU, dwFlags: flags, ..Default::default() },
        },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn activate(hwnd: isize) {
    let h = hwnd_of(hwnd);
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindowAsync(h, SW_RESTORE);
        }
        tap_alt();
        let _ = SetForegroundWindow(h);
    }
    LAST_FG.store(hwnd, Ordering::SeqCst);
}

/// Taskbar click behaviour: focused -> minimize, otherwise bring to front.
pub fn toggle(hwnd: isize) {
    let h = hwnd_of(hwnd);
    let minimized = unsafe { IsIconic(h).as_bool() };
    if active() == hwnd && !minimized {
        unsafe {
            let _ = ShowWindowAsync(h, SW_MINIMIZE);
        }
    } else {
        activate(hwnd);
    }
}

pub fn minimize(hwnd: isize) {
    unsafe {
        let _ = ShowWindowAsync(hwnd_of(hwnd), SW_MINIMIZE);
    }
}

pub fn close(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(Some(hwnd_of(hwnd)), WM_CLOSE, Default::default(), Default::default());
    }
}

/// Monitor whose whole area is covered by the foreground window (games, video).
pub fn fullscreen_monitor() -> Option<isize> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() || pid_of(fg) == std::process::id() {
            return None;
        }
        let class = class_of(fg);
        // Desktop, shell surfaces and maximized windows are not "fullscreen".
        if matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Windows.UI.Core.CoreWindow" | "XamlExplorerHostIslandWindow"
        ) || IsZoomed(fg).as_bool()
        {
            return None;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return None;
        }
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return None;
        }
        let m = mi.rcMonitor;
        (r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom).then_some(mon.0 as isize)
    }
}

pub fn send_keys(keys: &[VIRTUAL_KEY]) {
    let mk = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                ..Default::default()
            },
        },
    };
    let mut inputs: Vec<INPUT> = keys.iter().map(|k| mk(*k, false)).collect();
    inputs.extend(keys.iter().rev().map(|k| mk(*k, true)));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

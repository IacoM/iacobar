//! Hides the native Windows taskbar while we run and restores it on exit.
//! Nothing is deleted: we only set "auto-hide" + hide the window, and put
//! back the original state when the app closes (or panics).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::Shell::{ABM_GETSTATE, ABM_SETSTATE, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible, SW_HIDE, SW_SHOW, ShowWindow,
};
use windows::core::BOOL;

static HIDDEN: AtomicBool = AtomicBool::new(false);
static ORIGINAL_STATE: Mutex<Option<u32>> = Mutex::new(None);

/// Returns true if the user asked to keep the Windows taskbar visible
/// (useful during development): set IACOBAR_KEEP_WINDOWS_TASKBAR=1.
pub fn keep_native() -> bool {
    std::env::var("IACOBAR_KEEP_WINDOWS_TASKBAR").map(|v| v == "1").unwrap_or(false)
}

/// Explorer's taskbar windows (never our own tray-host window, which uses
/// the same class name so that apps send their tray icons to us).
fn taskbar_windows() -> Vec<HWND> {
    let mut out: Vec<HWND> = Vec::new();
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let mut buf = [0u16; 64];
        let n = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
        let class = String::from_utf16_lossy(&buf[..n]);
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid != std::process::id() && (class == "Shell_TrayWnd" || class == "Shell_SecondaryTrayWnd") {
            let v = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
            v.push(hwnd);
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

/// Explorer's main taskbar window.
pub fn explorer_tray() -> Option<HWND> {
    taskbar_windows().into_iter().find(|h| {
        let mut buf = [0u16; 64];
        let n = unsafe { GetClassNameW(*h, &mut buf) } as usize;
        String::from_utf16_lossy(&buf[..n]) == "Shell_TrayWnd"
    })
}

fn set_autohide(state: u32) {
    unsafe {
        if let Some(tray) = explorer_tray() {
            let mut abd = APPBARDATA {
                cbSize: std::mem::size_of::<APPBARDATA>() as u32,
                hWnd: tray,
                lParam: LPARAM(state as isize),
                rc: RECT::default(),
                ..Default::default()
            };
            SHAppBarMessage(ABM_SETSTATE, &mut abd);
        }
    }
}

fn get_state() -> u32 {
    unsafe {
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            ..Default::default()
        };
        SHAppBarMessage(ABM_GETSTATE, &mut abd) as u32
    }
}

/// Hide the native taskbar (auto-hide + SW_HIDE so it never pops up).
pub fn hide() {
    if keep_native() {
        return;
    }
    {
        let mut orig = ORIGINAL_STATE.lock().unwrap();
        if orig.is_none() {
            *orig = Some(get_state());
        }
    }
    set_autohide(ABS_AUTOHIDE);
    for hwnd in taskbar_windows() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
    HIDDEN.store(true, Ordering::SeqCst);
}

/// Called periodically: explorer.exe may restart or re-show its taskbar.
pub fn enforce() {
    if !HIDDEN.load(Ordering::SeqCst) {
        return;
    }
    for hwnd in taskbar_windows() {
        unsafe {
            if IsWindowVisible(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

/// Unconditional restore (no saved state): auto-hide off, taskbars shown.
pub fn force_show() {
    set_autohide(0);
    for hwnd in taskbar_windows() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
    }
}

/// Put everything back the way it was.
pub fn restore() {
    if !HIDDEN.swap(false, Ordering::SeqCst) {
        return;
    }
    let orig = ORIGINAL_STATE.lock().map(|g| *g).ok().flatten().unwrap_or(0);
    set_autohide(orig);
    for hwnd in taskbar_windows() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
    }
}

//! Virtual desktops indicator: how many desktops, which one is current,
//! switching between them.
//!
//! Windows keeps the list in the registry (only once there is more than one
//! desktop); the current one comes from the public IVirtualDesktopManager
//! (desktop of the foreground window). Switching uses Windows' own
//! shortcuts (Win+Ctrl+Left/Right), the only supported way.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME,
    RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_D, VK_LEFT, VK_LWIN, VK_RIGHT};
use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::{GUID, PCWSTR, w};

const KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops");

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Desktops {
    /// names ("Desktop 2" when the user didn't name it)
    pub names: Vec<String>,
    /// index of the current desktop
    pub current: usize,
}

fn reg_binary(sub: PCWSTR, name: PCWSTR) -> Option<Vec<u8>> {
    let mut size = 0u32;
    unsafe {
        if RegGetValueW(HKEY_CURRENT_USER, sub, name, RRF_RT_REG_BINARY, None, None, Some(&mut size)) != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        if RegGetValueW(HKEY_CURRENT_USER, sub, name, RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size))
            != ERROR_SUCCESS
        {
            return None;
        }
        buf.truncate(size as usize);
        Some(buf)
    }
}

fn guid_of(b: &[u8]) -> GUID {
    GUID::from_values(
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u16::from_le_bytes([b[4], b[5]]),
        u16::from_le_bytes([b[6], b[7]]),
        [b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]],
    )
}

fn desktop_name(id: &GUID) -> Option<String> {
    let sub: Vec<u16> = format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops\Desktops\{{{id:?}}}")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut buf = vec![0u16; 256];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), w!("Name"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size))
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buf[..n])).filter(|s| !s.is_empty())
}

/// Desktop of the foreground window (public API).
fn current_id() -> Option<GUID> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let m: IVirtualDesktopManager = CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).ok()?;
        let fg: HWND = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }
        m.GetWindowDesktopId(fg).ok().filter(|g| *g != GUID::zeroed())
    }
}

pub fn read() -> Desktops {
    let ids: Vec<GUID> = reg_binary(KEY, w!("VirtualDesktopIDs"))
        .map(|b| b.chunks_exact(16).map(guid_of).collect())
        .unwrap_or_default();
    if ids.len() < 2 {
        return Desktops { names: vec!["Desktop 1".into()], current: 0 };
    }
    let cur = reg_binary(KEY, w!("CurrentVirtualDesktop"))
        .filter(|b| b.len() == 16)
        .map(|b| guid_of(&b))
        .or_else(current_id);
    let current = cur.and_then(|c| ids.iter().position(|g| *g == c)).unwrap_or(0);
    let names = ids
        .iter()
        .enumerate()
        .map(|(i, g)| desktop_name(g).unwrap_or_else(|| format!("Desktop {}", i + 1)))
        .collect();
    Desktops { names, current }
}

/// Watches the registry key: emits `desktops-changed` when desktops are
/// added / removed / renamed / switched (no polling).
pub fn start(app: AppHandle) {
    std::thread::spawn(move || unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, KEY, Some(0), KEY_READ | KEY_NOTIFY, &mut key) != ERROR_SUCCESS {
            return;
        }
        let mut last = read();
        let _ = app.emit("desktops-changed", &last);
        loop {
            let r = RegNotifyChangeKeyValue(key, true, REG_NOTIFY_CHANGE_LAST_SET | REG_NOTIFY_CHANGE_NAME, None, false);
            if r != ERROR_SUCCESS {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(80)); // let Windows finish writing
            let now = read();
            if now != last {
                let _ = app.emit("desktops-changed", &now);
                last = now;
            }
        }
    });
}

/// Moves to desktop `target` (or creates a new one with `target == usize::MAX`).
pub fn switch(target: usize) {
    if target == usize::MAX {
        crate::wins::send_keys(&[VK_LWIN, VK_CONTROL, VK_D]);
        return;
    }
    let d = read();
    if target >= d.names.len() || target == d.current {
        return;
    }
    let (key, steps) = if target > d.current { (VK_RIGHT, target - d.current) } else { (VK_LEFT, d.current - target) };
    for _ in 0..steps {
        crate::wins::send_keys(&[VK_LWIN, VK_CONTROL, key]);
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
}

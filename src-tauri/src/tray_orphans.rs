//! "Orphan" tray apps: running apps that have a notification-area icon in
//! Windows but never sent it to us (old apps that ignore "TaskbarCreated",
//! e.g. some VPN clients and other MFC programs, started before our bar).
//!
//! Windows remembers every tray app under
//! HKCU\Control Panel\NotifyIconSettings, including a PNG snapshot of its
//! icon. We list the running ones we are missing; clicking them brings up the
//! app's own window (we can't open their right-click menu: only the app
//! knows how, through the icon we never received).

use std::collections::HashSet;

use base64::Engine;
use serde::Serialize;
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW,
    RegOpenKeyExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SW_RESTORE, ShowWindowAsync,
};
use windows::core::{BOOL, PCWSTR, PWSTR, w};


#[derive(Serialize, Clone)]
pub struct Orphan {
    pub exe: String, // file name, lower case
    pub tip: String,
    pub icon: Option<String>,
}

fn reg_string(key: HKEY, sub: &[u16], name: PCWSTR) -> Option<String> {
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(key, PCWSTR(sub.as_ptr()), name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size))
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buf[..n]))
}

fn reg_binary(key: HKEY, sub: &[u16], name: PCWSTR) -> Option<Vec<u8>> {
    let mut size = 0u32;
    unsafe {
        if RegGetValueW(key, PCWSTR(sub.as_ptr()), name, RRF_RT_REG_BINARY, None, None, Some(&mut size)) != ERROR_SUCCESS
            || size == 0
        {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        if RegGetValueW(key, PCWSTR(sub.as_ptr()), name, RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size))
            != ERROR_SUCCESS
        {
            return None;
        }
        buf.truncate(size as usize);
        Some(buf)
    }
}

/// Every tray app Windows knows about: (exe file name, tooltip, PNG).
fn known() -> Vec<(String, String, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    unsafe {
        let mut root = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, w!("Control Panel\\NotifyIconSettings"), Some(0), KEY_READ, &mut root)
            != ERROR_SUCCESS
        {
            return out;
        }
        let mut i = 0u32;
        loop {
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(root, i, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None) != ERROR_SUCCESS {
                break;
            }
            i += 1;
            let sub: Vec<u16> = name[..len as usize].iter().copied().chain(std::iter::once(0)).collect();
            let Some(path) = reg_string(root, &sub, w!("ExecutablePath")) else { continue };
            let exe = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_lowercase();
            let tip = reg_string(root, &sub, w!("InitialTooltip")).unwrap_or_default();
            let png = reg_binary(root, &sub, w!("IconSnapshot")).filter(|b| b.starts_with(&[0x89, b'P', b'N', b'G']));
            out.push((exe, tip, png));
        }
        let _ = RegCloseKey(root);
    }
    out
}

/// Running processes: (pid, exe file name lower case).
fn processes() -> Vec<(u32, String)> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                let n = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(0);
                out.push((e.th32ProcessID, String::from_utf16_lossy(&e.szExeFile[..n]).to_lowercase()));
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// Running tray apps whose icon we never received.
/// `have`: exe names (lower case) of the icons we do have.
pub fn list(have: &HashSet<String>) -> Vec<Orphan> {
    let running: HashSet<String> = processes().into_iter().map(|(_, n)| n).collect();
    let own = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (exe, tip, png) in known() {
        // Explorer's own icons (volume, network...) are on our bar already
        if exe == "explorer.exe" || exe == own || have.contains(&exe) || !running.contains(&exe) || !seen.insert(exe.clone()) {
            continue;
        }
        let icon = png.map(|b| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b)));
        let tip = if tip.is_empty() { pretty(&exe) } else { tip };
        out.push(Orphan { exe, tip, icon });
    }
    out
}

fn pretty(exe: &str) -> String {
    match exe {
        "scgui.exe" => "Sophos Connect".into(),
        _ => {
            let base = exe.trim_end_matches(".exe");
            let mut c = base.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    }
}

/// Opens the app the way its Start-menu shortcut would: a visible window is
/// simply brought to the front; otherwise the exe is started again, and
/// single-instance apps (many VPN clients) then show their own window
/// with their own logic (much more reliable than forcing hidden windows).
/// Must not run on the UI thread.
pub fn open(exe: &str) -> Result<(), String> {
    let pids: Vec<u32> = processes().into_iter().filter(|(_, n)| n == exe).map(|(p, _)| p).collect();
    if pids.is_empty() {
        return Err("l'app non è più in esecuzione".into());
    }
    // titled top-level windows of those processes
    let mut found: Vec<(HWND, u32)> = Vec::new();
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let v = unsafe { &mut *(lparam.0 as *mut Vec<(HWND, u32)>) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if unsafe { GetWindowTextLengthW(hwnd) } > 0 {
            v.push((hwnd, pid));
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut found as *mut _ as isize));
    }
    let skip = ["IME", "MSCTFIME UI", "tooltips_class32"];
    let target = found.into_iter().find(|(h, pid)| {
        if !pids.contains(pid) || !unsafe { IsWindowVisible(*h).as_bool() } {
            return false;
        }
        let mut buf = [0u16; 64];
        let n = unsafe { GetClassNameW(*h, &mut buf) } as usize;
        !skip.contains(&String::from_utf16_lossy(&buf[..n]).as_str())
    });
    if let Some((h, _)) = target {
        if unsafe { IsIconic(h).as_bool() } {
            unsafe {
                let _ = ShowWindowAsync(h, SW_RESTORE);
            }
        }
        crate::wins::activate(h.0 as isize);
        return Ok(());
    }
    let path = pids.iter().find_map(|p| crate::wins::exe_of_pid(*p)).ok_or("percorso dell'app non trovato")?;
    crate::apps::launch(&path, None);
    Ok(())
}

//! Start with Windows: a value under HKCU\...\Run pointing to our exe.
//! Only release builds register themselves (never the dev binary).

use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegGetValueW, RRF_RT_REG_SZ, RegSetKeyValueW};
use windows::core::{PCWSTR, w};

use crate::icons::wide;

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("Iacobar");

fn current() -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    };
    if r.is_err() {
        return None;
    }
    let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buf[..n]))
}

/// Removes the autostart entry (used by the uninstaller).
pub fn remove() {
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE);
    }
}

/// Makes the registry match the user's setting.
pub fn apply(enabled: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let cmd = format!("\"{}\"", exe.display());
    unsafe {
        if enabled {
            if current().as_deref() != Some(cmd.as_str()) {
                let data = wide(&cmd);
                let _ = RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    RUN_KEY,
                    VALUE,
                    REG_SZ.0,
                    Some(data.as_ptr() as *const _),
                    (data.len() * 2) as u32,
                );
            }
        } else if current().is_some() {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE);
        }
    }
}

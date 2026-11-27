//! Per-app volume (like Windows' volume mixer): the audio sessions of the
//! default output device, each with its own volume and mute.

use serde::Serialize;
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree};
use windows::core::{GUID, Interface};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppVolume {
    /// process id (0 = Windows system sounds)
    pub pid: u32,
    pub name: String,
    /// exe path, for the icon
    pub exe: String,
    pub volume: f32,
    pub muted: bool,
    /// playing sound right now
    pub active: bool,
}

fn sessions() -> windows::core::Result<Vec<(IAudioSessionControl2, ISimpleAudioVolume)>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None)?;
        let list = mgr.GetSessionEnumerator()?;
        let mut out = Vec::new();
        for i in 0..list.GetCount()? {
            let Ok(ctl) = list.GetSession(i) else { continue };
            let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
            let Ok(vol) = ctl.cast::<ISimpleAudioVolume>() else { continue };
            out.push((ctl2, vol));
        }
        Ok(out)
    }
}

fn pretty(exe: &str) -> String {
    let file = exe.rsplit('\\').next().unwrap_or(exe);
    let base = file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file);
    let mut c = base.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Apps with an audio session, one row per app (sessions of the same app merged).
pub fn list() -> Vec<AppVolume> {
    let Ok(all) = sessions() else { return vec![] };
    let mut out: Vec<AppVolume> = Vec::new();
    for (ctl, vol) in all {
        unsafe {
            let state = ctl.GetState().unwrap_or(AudioSessionStateExpired);
            if state == AudioSessionStateExpired {
                continue;
            }
            let system = ctl.IsSystemSoundsSession().is_ok() && ctl.IsSystemSoundsSession() == windows::core::HRESULT(0);
            let pid = if system { 0 } else { ctl.GetProcessId().unwrap_or(0) };
            let exe = if pid == 0 { String::new() } else { crate::wins::exe_of_pid(pid).unwrap_or_default() };
            if pid != 0 && exe.is_empty() {
                continue; // process gone
            }
            let display = ctl
                .GetDisplayName()
                .map(|p| {
                    let s = p.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(p.0 as *const _));
                    s
                })
                .unwrap_or_default();
            let name = if system {
                "Suoni di sistema".to_string()
            } else if !display.is_empty() && !display.starts_with('@') {
                display
            } else {
                pretty(&exe)
            };
            let v = vol.GetMasterVolume().unwrap_or(1.0);
            let m = vol.GetMute().map(|b| b.as_bool()).unwrap_or(false);
            let active = state == AudioSessionStateActive;
            if let Some(row) = out.iter_mut().find(|r| r.pid == pid) {
                row.active |= active;
                continue;
            }
            out.push(AppVolume { pid, name, exe, volume: v, muted: m, active });
        }
    }
    // playing apps first, system sounds last
    out.sort_by_key(|a| (a.pid == 0, !a.active, a.name.to_lowercase()));
    out
}

/// Sets volume (0..1) and/or mute of every session of `pid`.
pub fn set(pid: u32, volume: Option<f32>, muted: Option<bool>) -> Result<(), String> {
    let all = sessions().map_err(|e| e.to_string())?;
    let ctx = GUID::zeroed();
    for (ctl, vol) in all {
        unsafe {
            let system = ctl.IsSystemSoundsSession() == windows::core::HRESULT(0);
            let p = if system { 0 } else { ctl.GetProcessId().unwrap_or(0) };
            if p != pid {
                continue;
            }
            if let Some(v) = volume {
                let _ = vol.SetMasterVolume(v.clamp(0.0, 1.0), &ctx);
            }
            if let Some(m) = muted {
                let _ = vol.SetMute(m, &ctx);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn mixer_prints() {
        for (c, _) in super::sessions().unwrap_or_default() {
            unsafe {
                println!("raw pid={:?} state={:?}", c.GetProcessId(), c.GetState().map(|s| s.0));
            }
        }
        for a in super::list() {
            println!("{:?}", a);
        }
    }
}

//! System status: battery, network, volume, CPU/RAM, accent color.

use std::sync::Mutex;

use serde::Serialize;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Graphics::Dwm::DwmGetColorizationColor;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::Networking::NetworkListManager::{
    INetworkListManager, NLM_CONNECTIVITY_IPV4_INTERNET, NLM_CONNECTIVITY_IPV6_INTERNET,
    NetworkListManager,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::core::BOOL;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub battery: Option<Battery>,
    pub online: bool,
    pub volume: f32,
    pub muted: bool,
    pub cpu: f32,
    pub ram: u32,
    pub accent: String,
}

fn com_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn endpoint() -> windows::core::Result<IAudioEndpointVolume> {
    com_init();
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
        dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
    }
}

pub fn volume() -> (f32, bool) {
    endpoint()
        .and_then(|ep| unsafe { Ok((ep.GetMasterVolumeLevelScalar()?, ep.GetMute()?.as_bool())) })
        .unwrap_or((0.0, false))
}

pub fn set_volume(v: f32) {
    if let Ok(ep) = endpoint() {
        unsafe {
            let _ = ep.SetMasterVolumeLevelScalar(v.clamp(0.0, 1.0), std::ptr::null());
            if v > 0.0 {
                let _ = ep.SetMute(false, std::ptr::null());
            }
        }
    }
}

pub fn set_mute(m: bool) {
    if let Ok(ep) = endpoint() {
        unsafe {
            let _ = ep.SetMute(m, std::ptr::null());
        }
    }
}

fn online() -> bool {
    com_init();
    unsafe {
        let nlm: windows::core::Result<INetworkListManager> =
            CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL);
        match nlm.and_then(|n| n.GetConnectivity()) {
            Ok(c) => {
                c.0 & (NLM_CONNECTIVITY_IPV4_INTERNET.0 | NLM_CONNECTIVITY_IPV6_INTERNET.0) != 0
            }
            Err(_) => true,
        }
    }
}

fn battery() -> Option<Battery> {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s).ok()? };
    // 128 = no system battery, 255 = unknown
    if s.BatteryFlag & 128 != 0 || s.BatteryFlag == 255 || s.BatteryLifePercent > 100 {
        return None;
    }
    Some(Battery { percent: s.BatteryLifePercent, charging: s.ACLineStatus == 1 })
}

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

static CPU_PREV: Mutex<(u64, u64)> = Mutex::new((0, 0));

fn cpu() -> f32 {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe {
        if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_err() {
            return 0.0;
        }
    }
    let idle = ft(idle);
    let total = ft(kernel) + ft(user); // kernel time includes idle
    let mut prev = CPU_PREV.lock().unwrap();
    let (pi, pt) = *prev;
    *prev = (idle, total);
    let dt = total.saturating_sub(pt);
    if pt == 0 || dt == 0 {
        return 0.0;
    }
    let di = idle.saturating_sub(pi);
    (1.0 - di as f32 / dt as f32).clamp(0.0, 1.0) * 100.0
}

fn ram() -> u32 {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe {
        if GlobalMemoryStatusEx(&mut m).is_err() {
            return 0;
        }
    }
    m.dwMemoryLoad
}

pub fn accent() -> String {
    let mut color = 0u32;
    let mut opaque = BOOL(0);
    unsafe {
        if DwmGetColorizationColor(&mut color, &mut opaque).is_err() {
            return "#4cc2ff".into();
        }
    }
    format!("#{:06x}", color & 0x00ff_ffff)
}

pub fn status() -> SystemStatus {
    let (volume, muted) = volume();
    SystemStatus {
        battery: battery(),
        online: online(),
        volume,
        muted,
        cpu: cpu(),
        ram: ram(),
        accent: accent(),
    }
}

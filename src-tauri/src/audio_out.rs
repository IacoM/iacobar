//! Audio output devices: list them and switch the default one directly from
//! the panel (no trip to Settings).
//!
//! Listing uses the public Core Audio API. Windows has no public API to
//! change the default device: like the Sound control panel and tools such as
//! SoundSwitch / EarTrumpet we use the (stable since Windows 7) internal
//! `IPolicyConfig` interface.

#![allow(non_snake_case)]

use serde::Serialize;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, ERole, IMMDeviceEnumerator, MMDeviceEnumerator, eCommunications, eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, interface};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AudioOut {
    pub id: String,
    pub name: String,
    pub default: bool,
}

#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self, id: PCWSTR, fmt: *mut *mut core::ffi::c_void) -> HRESULT;
    fn GetDeviceFormat(&self, id: PCWSTR, default: i32, fmt: *mut *mut core::ffi::c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, id: PCWSTR, a: *mut core::ffi::c_void, b: *mut core::ffi::c_void) -> HRESULT;
    fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, a: *mut i64, b: *mut i64) -> HRESULT;
    fn SetProcessingPeriod(&self, id: PCWSTR, a: *mut i64) -> HRESULT;
    fn GetShareMode(&self, id: PCWSTR, mode: *mut core::ffi::c_void) -> HRESULT;
    fn SetShareMode(&self, id: PCWSTR, mode: *mut core::ffi::c_void) -> HRESULT;
    fn GetPropertyValue(&self, id: PCWSTR, key: *const core::ffi::c_void, v: *mut core::ffi::c_void) -> HRESULT;
    fn SetPropertyValue(&self, id: PCWSTR, key: *const core::ffi::c_void, v: *const core::ffi::c_void) -> HRESULT;
    fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
}

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// PKEY_Device_FriendlyName ("Cuffie (WH-1000XM5)")
const FRIENDLY_NAME: PROPERTYKEY =
    PROPERTYKEY { fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0), pid: 14 };

fn com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

/// Active output devices, the default one flagged.
pub fn outputs() -> Vec<AudioOut> {
    com();
    let mut out = Vec::new();
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
            return out;
        };
        let default_id = en
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .and_then(|d| d.GetId())
            .map(|p| {
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const _));
                s
            })
            .unwrap_or_default();
        let Ok(list) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return out };
        let n = list.GetCount().unwrap_or(0);
        for i in 0..n {
            let Ok(dev) = list.Item(i) else { continue };
            let Ok(pid) = dev.GetId() else { continue };
            let id = pid.to_string().unwrap_or_default();
            CoTaskMemFree(Some(pid.0 as *const _));
            let name = dev
                .OpenPropertyStore(STGM_READ)
                .and_then(|ps| ps.GetValue(&FRIENDLY_NAME))
                .map(|v| v.to_string())
                .unwrap_or_default();
            out.push(AudioOut { default: id == default_id, name: if name.is_empty() { "Dispositivo audio".into() } else { name }, id });
        }
    }
    out
}

/// Makes `id` the default output for every role (apps, multimedia, calls).
pub fn set_output(id: &str) -> Result<(), String> {
    com();
    let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let pc: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        for role in [eConsole, eMultimedia, eCommunications] {
            pc.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role).ok().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

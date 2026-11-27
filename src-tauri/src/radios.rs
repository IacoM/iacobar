//! Direct Wi-Fi / Bluetooth on-off (Windows.Devices.Radios) and the name of
//! the network we are connected to.

use serde::Serialize;
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Devices::Radios::{Radio, RadioKind, RadioState};
use windows::Networking::Connectivity::NetworkInformation;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Radios {
    /// None = this PC has no such radio
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
    /// SSID (Wi-Fi) or connection name (Ethernet)
    pub network: Option<String>,
    /// names of the Bluetooth devices connected right now
    pub bt_devices: Vec<String>,
}

fn com_mta() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn all() -> windows::core::Result<Vec<Radio>> {
    com_mta();
    let _ = Radio::RequestAccessAsync()?.get();
    let list = Radio::GetRadiosAsync()?.get()?;
    Ok(list.into_iter().collect())
}

fn network_name() -> Option<String> {
    let profile = NetworkInformation::GetInternetConnectionProfile().ok()?;
    if profile.IsWlanConnectionProfile().unwrap_or(false) {
        if let Ok(ssid) = profile.WlanConnectionProfileDetails().and_then(|d| d.GetConnectedSsid()) {
            let s = ssid.to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    profile.ProfileName().ok().map(|n| n.to_string()).filter(|s| !s.is_empty())
}

/// Bluetooth devices currently connected (classic + Low Energy).
fn bt_connected() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let selectors = [
        BluetoothDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected),
        BluetoothLEDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected),
    ];
    for sel in selectors.into_iter().flatten() {
        let Ok(list) = DeviceInformation::FindAllAsyncAqsFilter(&sel).and_then(|op| op.get()) else { continue };
        for d in list {
            if let Ok(name) = d.Name() {
                let name = name.to_string();
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

pub fn status() -> Radios {
    let mut out = Radios { network: network_name(), ..Default::default() };
    if let Ok(radios) = all() {
        for r in radios {
            let on = r.State().map(|s| s == RadioState::On).unwrap_or(false);
            match r.Kind() {
                Ok(k) if k == RadioKind::WiFi => out.wifi = Some(out.wifi.unwrap_or(false) || on),
                Ok(k) if k == RadioKind::Bluetooth => out.bluetooth = Some(out.bluetooth.unwrap_or(false) || on),
                _ => {}
            }
        }
    }
    if out.bluetooth == Some(true) {
        out.bt_devices = bt_connected();
    }
    out
}

/// Turns the radios of `kind` ("wifi" | "bluetooth") on or off.
pub fn set(kind: &str, on: bool) -> Result<(), String> {
    let want = match kind {
        "wifi" => RadioKind::WiFi,
        "bluetooth" => RadioKind::Bluetooth,
        _ => return Err("tipo sconosciuto".into()),
    };
    let radios = all().map_err(|e| e.to_string())?;
    let state = if on { RadioState::On } else { RadioState::Off };
    let mut found = false;
    for r in radios {
        if r.Kind().map(|k| k == want).unwrap_or(false) {
            found = true;
            r.SetStateAsync(state).and_then(|op| op.get()).map_err(|e| e.to_string())?;
        }
    }
    if found { Ok(()) } else { Err("nessuna radio di questo tipo".into()) }
}

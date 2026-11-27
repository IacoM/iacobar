//! Wi-Fi networks from the panel: scan, connect (saved networks directly,
//! new secured ones with a password), disconnect. Windows.Devices.WiFi.

use serde::Serialize;
use windows::Devices::WiFi::{WiFiAccessStatus, WiFiAdapter, WiFiAvailableNetwork, WiFiConnectionStatus, WiFiReconnectionKind};
use windows::Networking::Connectivity::NetworkAuthenticationType;
use windows::Security::Credentials::PasswordCredential;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::HSTRING;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub ssid: String,
    /// 0..4 (0 also when unknown: saved networks without a scan)
    pub bars: u8,
    pub secure: bool,
    pub connected: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WifiList {
    pub networks: Vec<Network>,
    /// true = location is off: only the saved networks can be shown
    pub saved_only: bool,
}

/// Without location permission Windows hides the networks in range (their
/// names reveal where you are): fall back to the saved profiles, which the
/// classic WLAN API still lists and can connect to.
mod saved {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::WiFi::{
        WLAN_CONNECTION_ATTRIBUTES, WLAN_CONNECTION_PARAMETERS, WLAN_INTERFACE_INFO_LIST, WLAN_PROFILE_INFO_LIST,
        WlanCloseHandle, WlanConnect, WlanDisconnect, WlanEnumInterfaces, WlanFreeMemory, WlanGetProfileList,
        WlanOpenHandle, WlanQueryInterface, dot11_BSS_type_any, wlan_connection_mode_profile,
        wlan_intf_opcode_current_connection,
    };
    use windows::core::{GUID, PCWSTR};

    struct Wlan(HANDLE, GUID);
    impl Drop for Wlan {
        fn drop(&mut self) {
            unsafe {
                let _ = WlanCloseHandle(self.0, None);
            }
        }
    }

    fn open() -> Result<Wlan, String> {
        unsafe {
            let mut ver = 0u32;
            let mut h = HANDLE::default();
            if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
                return Err("servizio Wi-Fi non disponibile".into());
            }
            let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
            if WlanEnumInterfaces(h, None, &mut list) != 0 || list.is_null() || (*list).dwNumberOfItems == 0 {
                let _ = WlanCloseHandle(h, None);
                return Err("nessuna scheda Wi-Fi".into());
            }
            let guid = (*list).InterfaceInfo[0].InterfaceGuid;
            WlanFreeMemory(list as *const _);
            Ok(Wlan(h, guid))
        }
    }

    fn wstr(buf: &[u16]) -> String {
        String::from_utf16_lossy(&buf[..buf.iter().position(|c| *c == 0).unwrap_or(buf.len())])
    }

    /// (saved profile names, profile currently connected)
    pub fn list() -> Result<(Vec<String>, Option<String>), String> {
        let w = open()?;
        unsafe {
            let mut pl: *mut WLAN_PROFILE_INFO_LIST = std::ptr::null_mut();
            if WlanGetProfileList(w.0, &w.1, None, &mut pl) != 0 || pl.is_null() {
                return Err("elenco reti salvate non disponibile".into());
            }
            let n = (*pl).dwNumberOfItems as usize;
            let items = std::slice::from_raw_parts((*pl).ProfileInfo.as_ptr(), n);
            let names: Vec<String> = items.iter().map(|p| wstr(&p.strProfileName)).collect();
            WlanFreeMemory(pl as *const _);

            let mut size = 0u32;
            let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
            let current = if WlanQueryInterface(w.0, &w.1, wlan_intf_opcode_current_connection, None, &mut size, &mut data, None) == 0
                && !data.is_null()
            {
                let attr = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
                let name = wstr(&attr.strProfileName);
                WlanFreeMemory(data);
                Some(name).filter(|s| !s.is_empty())
            } else {
                None
            };
            Ok((names, current))
        }
    }

    pub fn connect(profile: &str) -> Result<(), String> {
        let w = open()?;
        let name: Vec<u16> = profile.encode_utf16().chain(Some(0)).collect();
        let params = WLAN_CONNECTION_PARAMETERS {
            wlanConnectionMode: wlan_connection_mode_profile,
            strProfile: PCWSTR(name.as_ptr()),
            dot11BssType: dot11_BSS_type_any,
            ..Default::default()
        };
        let r = unsafe { WlanConnect(w.0, &w.1, &params, None) };
        if r == 0 { Ok(()) } else { Err("rete non raggiungibile da qui".into()) }
    }

    pub fn disconnect() -> Result<(), String> {
        let w = open()?;
        let r = unsafe { WlanDisconnect(w.0, &w.1, None) };
        if r == 0 { Ok(()) } else { Err("disconnessione non riuscita".into()) }
    }
}

/// Error text the panel recognises to ask for a password.
pub const NEED_PASSWORD: &str = "password";

fn adapter() -> Result<WiFiAdapter, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let access = WiFiAdapter::RequestAccessAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
    if access != WiFiAccessStatus::Allowed {
        return Err("Windows non concede l'accesso al Wi-Fi: attiva \"Consenti alle app desktop di accedere alla posizione\" in Impostazioni › Privacy › Posizione".into());
    }
    let list = WiFiAdapter::FindAllAdaptersAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
    list.into_iter().next().ok_or_else(|| "nessuna scheda Wi-Fi".into())
}

fn connected_ssid(a: &WiFiAdapter) -> Option<String> {
    let profile = a.NetworkAdapter().ok()?.GetConnectedProfileAsync().ok()?.get().ok()?;
    if !profile.IsWlanConnectionProfile().unwrap_or(false) {
        return None;
    }
    profile.WlanConnectionProfileDetails().ok()?.GetConnectedSsid().ok().map(|s| s.to_string())
}

fn is_secure(n: &WiFiAvailableNetwork) -> bool {
    n.SecuritySettings()
        .and_then(|s| s.NetworkAuthenticationType())
        .map(|t| t != NetworkAuthenticationType::Open80211 && t != NetworkAuthenticationType::None)
        .unwrap_or(true)
}

/// Networks in range (or the saved ones when location is off).
pub fn scan() -> Result<WifiList, String> {
    match scan_in_range() {
        Ok(networks) => Ok(WifiList { networks, saved_only: false }),
        Err(_) => {
            let (names, current) = saved::list()?;
            let mut networks: Vec<Network> = names
                .into_iter()
                .map(|n| Network { connected: current.as_deref() == Some(n.as_str()), ssid: n, bars: 0, secure: true })
                .collect();
            networks.sort_by_key(|n| (!n.connected, n.ssid.to_lowercase()));
            Ok(WifiList { networks, saved_only: true })
        }
    }
}

/// Scans and returns the networks in range, strongest first (connected on top).
fn scan_in_range() -> Result<Vec<Network>, String> {
    let a = adapter()?;
    a.ScanAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
    let current = connected_ssid(&a);
    let report = a.NetworkReport().map_err(|e| e.to_string())?;
    let mut out: Vec<Network> = Vec::new();
    for n in report.AvailableNetworks().map_err(|e| e.to_string())? {
        let ssid = n.Ssid().map(|s| s.to_string()).unwrap_or_default();
        if ssid.is_empty() {
            continue; // hidden networks
        }
        let bars = n.SignalBars().unwrap_or(0).min(4);
        if let Some(existing) = out.iter_mut().find(|x| x.ssid == ssid) {
            existing.bars = existing.bars.max(bars); // same network, several access points
            continue;
        }
        out.push(Network { connected: current.as_deref() == Some(ssid.as_str()), secure: is_secure(&n), ssid, bars });
    }
    out.sort_by_key(|n| (!n.connected, std::cmp::Reverse(n.bars), n.ssid.to_lowercase()));
    Ok(out)
}

/// Connects to `ssid`. Saved networks need no password; a secured network
/// Windows doesn't know yet returns NEED_PASSWORD so the panel can ask.
pub fn connect(ssid: &str, password: Option<&str>) -> Result<(), String> {
    // location off: connect to the saved profile by name
    let Ok(a) = adapter() else { return saved::connect(ssid) };
    let report = a.NetworkReport().map_err(|e| e.to_string())?;
    let net = report
        .AvailableNetworks()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|n| n.Ssid().map(|s| s.to_string()).as_deref() == Ok(ssid))
        .max_by_key(|n| n.SignalBars().unwrap_or(0))
        .ok_or("rete non più disponibile")?;
    let kind = WiFiReconnectionKind::Automatic;
    let result = match password {
        Some(pw) => {
            let cred = PasswordCredential::new().map_err(|e| e.to_string())?;
            cred.SetPassword(&HSTRING::from(pw)).map_err(|e| e.to_string())?;
            a.ConnectWithPasswordCredentialAsync(&net, kind, &cred)
        }
        None => a.ConnectAsync(&net, kind),
    }
    .and_then(|op| op.get())
    .and_then(|r| r.ConnectionStatus())
    .map_err(|e| e.to_string())?;
    match result {
        WiFiConnectionStatus::Success => Ok(()),
        WiFiConnectionStatus::InvalidCredential => Err(NEED_PASSWORD.into()),
        // unknown secured network without password: Windows reports a failure
        _ if password.is_none() && is_secure(&net) => Err(NEED_PASSWORD.into()),
        WiFiConnectionStatus::NetworkNotAvailable => Err("rete non disponibile".into()),
        WiFiConnectionStatus::Timeout => Err("tempo scaduto".into()),
        WiFiConnectionStatus::AccessRevoked => Err("accesso revocato".into()),
        _ => Err("connessione non riuscita".into()),
    }
}

pub fn disconnect() -> Result<(), String> {
    match adapter() {
        Ok(a) => a.Disconnect().map_err(|e| e.to_string()),
        Err(_) => saved::disconnect(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn wifi_scan_prints() {
        match super::scan() {
            Ok(v) => {
                println!("saved_only={}", v.saved_only);
                v.networks.iter().for_each(|n| println!("{n:?}"));
            }
            Err(e) => println!("error: {e}"),
        }
    }
}

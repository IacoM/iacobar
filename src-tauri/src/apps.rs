//! Installed apps for the launcher (Start menu shortcuts + Store apps)
//! and launching of paths / shell targets.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use crate::icons::wide;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub name: String,
    /// What to launch: a .lnk/.exe path or `shell:AppsFolder\<AUMID>`.
    pub target: String,
    /// Path used to extract an icon (empty when unknown).
    pub icon_path: String,
}

static APPS: LazyLock<Mutex<Option<Vec<AppEntry>>>> = LazyLock::new(|| Mutex::new(None));

fn scan_dir(dir: &Path, out: &mut Vec<AppEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            scan_dir(&p, out);
        } else if p.extension().map(|x| x.eq_ignore_ascii_case("lnk")).unwrap_or(false) {
            // the name Windows shows (localized via desktop.ini: "Command
            // Prompt.lnk" is "Prompt dei comandi"), falling back to the file name
            let name = display_name(&p)
                .unwrap_or_else(|| p.file_stem().unwrap_or_default().to_string_lossy().to_string());
            let low = name.to_lowercase();
            // Skip uninstallers / docs clutter.
            if low.contains("uninstall") || low.contains("disinstalla") || low.contains("readme") {
                continue;
            }
            let s = p.to_string_lossy().to_string();
            out.push(AppEntry { name, target: s.clone(), icon_path: s });
        }
    }
}

/// Shell display name of a file (without the ".lnk" extension).
fn display_name(p: &Path) -> Option<String> {
    use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
    use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_DISPLAYNAME, SHGetFileInfoW};
    let path = wide(&p.to_string_lossy());
    let mut info = SHFILEINFOW::default();
    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(path.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_DISPLAYNAME,
        )
    };
    if ok == 0 {
        return None;
    }
    let n = info.szDisplayName.iter().position(|c| *c == 0).unwrap_or(info.szDisplayName.len());
    let s = String::from_utf16_lossy(&info.szDisplayName[..n]);
    let s = s.strip_suffix(".lnk").unwrap_or(&s).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Every other app Windows lists in Start (`Get-StartApps`): Store apps, but
/// also desktop apps registered only by an app id (e.g. Docker Desktop, whose
/// Start menu folder holds no shortcut) and Windows tools.
fn store_apps(known: &HashSet<String>) -> Vec<AppEntry> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | ConvertTo-Json -Compress",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(out) = out else { return vec![] };
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Sa {
        name: String,
        app_i_d: String,
    }
    let list: Vec<Sa> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    list.into_iter()
        // skip what the .lnk scan already has, web links and uninstallers
        .filter(|a| {
            let low = a.name.to_lowercase();
            !known.contains(&low)
                && !a.app_i_d.contains("://")
                && !low.contains("uninstall")
                && !low.contains("disinstalla")
        })
        .map(|a| AppEntry {
            name: a.name,
            target: format!("shell:AppsFolder\\{}", a.app_i_d),
            icon_path: format!("shell:AppsFolder\\{}", a.app_i_d),
        })
        .collect()
}

pub fn list() -> Vec<AppEntry> {
    if let Some(v) = APPS.lock().unwrap().as_ref() {
        return v.clone();
    }
    let mut out = Vec::new();
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(pd) = std::env::var("ProgramData") {
        roots.push(Path::new(&pd).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Ok(ad) = std::env::var("APPDATA") {
        roots.push(Path::new(&ad).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    for r in &roots {
        scan_dir(r, &mut out);
    }
    let mut seen = HashSet::new();
    out.retain(|a| seen.insert(a.name.to_lowercase()));
    out.extend(store_apps(&seen));
    out.sort_by_key(|a| a.name.to_lowercase());
    *APPS.lock().unwrap() = Some(out.clone());
    out
}

pub fn refresh() {
    *APPS.lock().unwrap() = None;
}

/// Expands `%VAR%` references (shortcut icon paths often use them).
fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(a) = rest.find('%') {
        let Some(b) = rest[a + 1..].find('%') else { break };
        let name = &rest[a + 1..a + 1 + b];
        out.push_str(&rest[..a]);
        match std::env::var(name) {
            Ok(v) => out.push_str(&v),
            Err(_) => out.push_str(&rest[a..a + b + 2]),
        }
        rest = &rest[a + b + 2..];
    }
    out.push_str(rest);
    out
}

/// The icon a shortcut explicitly asks for: (file, index), when set.
pub fn lnk_icon_location(path: &str) -> Option<(String, i32)> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::Interface;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let pf: IPersistFile = link.cast().ok()?;
        let w = wide(path);
        pf.Load(PCWSTR(w.as_ptr()), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        let mut index = 0i32;
        link.GetIconLocation(&mut buf, &mut index).ok()?;
        let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
        if n == 0 {
            return None;
        }
        let file = expand_env(&String::from_utf16_lossy(&buf[..n]));
        std::path::Path::new(&file).exists().then_some((file, index))
    }
}

/// Resolves a .lnk shortcut to the executable it points to.
pub fn resolve_lnk(path: &str) -> Option<String> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile,
        STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::Interface;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let pf: IPersistFile = link.cast().ok()?;
        let w = wide(path);
        pf.Load(PCWSTR(w.as_ptr()), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
        if n == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..n]))
    }
}

/// Opens a file, shortcut, exe, URL or `shell:` target.
pub fn launch(target: &str, args: Option<&str>) {
    let (file, params) = if target.starts_with("shell:") {
        ("explorer.exe".to_string(), Some(target.to_string()))
    } else {
        (target.to_string(), args.map(|s| s.to_string()))
    };
    let f = wide(&file);
    let p = params.as_deref().map(wide);
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(f.as_ptr()),
            p.as_ref().map(|v| PCWSTR(v.as_ptr())).unwrap_or(PCWSTR::null()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

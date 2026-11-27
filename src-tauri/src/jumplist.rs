//! Recent items of an app (the "jump list" Windows shows on right-click):
//! recent files / folders of VS Code, Office, Notepad, Explorer...
//!
//! Uses the documented `IApplicationDocumentLists`, which needs the app's
//! AppUserModelID. Explicit ids come from the open window or the pinned
//! shortcut; apps without one get the id Windows derives from the exe path
//! (known folder GUID + relative path, as `Get-StartApps` shows it).

use serde::Serialize;
use windows::Win32::Foundation::HWND;
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::IObjectArray;
use windows::Win32::UI::Shell::PropertiesSystem::{GPS_DEFAULT, IPropertyStore, SHGetPropertyStoreFromParsingName};
use windows::Win32::UI::Shell::{
    ADLT_FREQUENT, ADLT_RECENT, APPDOCLISTTYPE, ApplicationDocumentLists, FOLDERID_LocalAppData, FOLDERID_ProgramFilesX64,
    FOLDERID_ProgramFilesX86, FOLDERID_System, FOLDERID_UserProgramFiles, FOLDERID_Windows, IApplicationDocumentLists,
    IShellItem, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY,
};
use windows::core::{GUID, PCWSTR};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JumpItem {
    /// what to show ("report.docx", "my-project")
    pub name: String,
    /// file / folder path (or a shell parsing name)
    pub path: String,
    /// arguments, for items that are shortcuts (tasks)
    pub args: String,
    /// true for shortcuts: launch `path args` as is
    pub link: bool,
    /// an action of the app ("New Window") rather than a recent file/folder
    pub task: bool,
}

/// Shortcuts whose arguments point at a file/folder/uri are "recent"
/// entries; the others are the app's tasks.
fn classify(mut v: Vec<JumpItem>) -> Vec<JumpItem> {
    for j in &mut v {
        j.task = j.link && !(j.args.contains("://") || j.args.contains(":\\") || j.args.contains(":/"));
    }
    v.sort_by_key(|j| j.task); // recent first, stable
    v
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn pwstr_take(p: windows::core::PWSTR) -> String {
    unsafe {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

/// System.AppUserModel.ID of a shortcut (.lnk), if it declares one.
fn lnk_aumid(path: &str) -> Option<String> {
    unsafe {
        let w = wide(path);
        let store: IPropertyStore = SHGetPropertyStoreFromParsingName(PCWSTR(w.as_ptr()), None, GPS_DEFAULT).ok()?;
        let pv = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&pv).ok()?;
        Some(pwstr_take(s)).filter(|v| !v.is_empty())
    }
}

/// The id Windows derives for apps that don't set one:
/// "C:\Program Files\App\app.exe" -> "{6D809377-...}\App\app.exe".
fn implicit_aumid(exe: &str) -> Option<String> {
    let folders: [&GUID; 6] = [
        &FOLDERID_UserProgramFiles, // %LOCALAPPDATA%\Programs (before LocalAppData)
        &FOLDERID_LocalAppData,
        &FOLDERID_ProgramFilesX64,
        &FOLDERID_ProgramFilesX86,
        &FOLDERID_System,
        &FOLDERID_Windows,
    ];
    let low = exe.to_lowercase();
    for id in folders {
        let Ok(p) = (unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }) else { continue };
        let base = pwstr_take(p);
        let bl = base.to_lowercase();
        if !bl.is_empty() && low.starts_with(&bl) && low[bl.len()..].starts_with('\\') {
            return Some(format!("{{{:?}}}{}", id, &exe[base.len()..]));
        }
    }
    None
}

fn read_list(adl: &IApplicationDocumentLists, kind: APPDOCLISTTYPE, max: usize, out: &mut Vec<JumpItem>) {
    unsafe {
        let Ok(arr) = adl.GetList::<IObjectArray>(kind, max as u32) else { return };
        let n = arr.GetCount().unwrap_or(0);
        for i in 0..n {
            if out.len() >= max {
                return;
            }
            if let Ok(item) = arr.GetAt::<IShellItem>(i) {
                let name = item.GetDisplayName(SIGDN_NORMALDISPLAY).map(pwstr_take).unwrap_or_default();
                let path = item.GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING).map(pwstr_take).unwrap_or_default();
                if !path.is_empty() && !out.iter().any(|j| j.path.eq_ignore_ascii_case(&path)) {
                    out.push(JumpItem { name: if name.is_empty() { path.clone() } else { name }, path, args: String::new(), link: false, task: false });
                }
            } else if let Ok(link) = arr.GetAt::<IShellLinkW>(i) {
                let mut buf = [0u16; 1024];
                let _ = link.GetPath(&mut buf, std::ptr::null_mut(), 0);
                let path = String::from_utf16_lossy(&buf[..buf.iter().position(|c| *c == 0).unwrap_or(0)]);
                let mut abuf = [0u16; 1024];
                let _ = link.GetArguments(&mut abuf);
                let args = String::from_utf16_lossy(&abuf[..abuf.iter().position(|c| *c == 0).unwrap_or(0)]);
                let mut dbuf = [0u16; 512];
                let _ = link.GetDescription(&mut dbuf);
                let mut name = String::from_utf16_lossy(&dbuf[..dbuf.iter().position(|c| *c == 0).unwrap_or(0)]);
                if name.is_empty() {
                    name = args.trim_matches('"').rsplit('\\').next().unwrap_or(&args).to_string();
                }
                if !path.is_empty() && !name.is_empty() {
                    out.push(JumpItem { name, path, args, link: true, task: false });
                }
            }
        }
    }
}

/// "Custom" jump lists (set by Electron apps: VS Code, Postman, Teams...) have
/// no read API. Their files are just shell links stored back to back: load
/// each one and keep those launching `exe` (recent folders, tasks).
fn custom_items(exe: &str, max: usize) -> Vec<JumpItem> {
    use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
    use windows::Win32::System::Com::IPersistStream;
    use windows::Win32::UI::Shell::{SHCreateMemStream, ShellLink};
    use windows::core::Interface;

    const HEADER: [u8; 20] = [
        0x4C, 0, 0, 0, 0x01, 0x14, 0x02, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46,
    ];
    let Ok(appdata) = std::env::var("APPDATA") else { return vec![] };
    let dir = std::path::Path::new(&appdata).join(r"Microsoft\Windows\Recent\CustomDestinations");
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let want = exe.to_lowercase();
    let mut out: Vec<JumpItem> = Vec::new();
    for e in rd.flatten() {
        let Ok(bytes) = std::fs::read(e.path()) else { continue };
        let mut found = Vec::new();
        let mut i = 0;
        while i + HEADER.len() <= bytes.len() {
            if bytes[i..i + HEADER.len()] == HEADER {
                unsafe {
                    let Some(stream) = SHCreateMemStream(Some(&bytes[i..])) else { break };
                    let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { break };
                    let ok = link.cast::<IPersistStream>().and_then(|ps| ps.Load(&stream)).is_ok();
                    if ok {
                        let mut buf = [0u16; 1024];
                        let _ = link.GetPath(&mut buf, std::ptr::null_mut(), 0);
                        let path = String::from_utf16_lossy(&buf[..buf.iter().position(|c| *c == 0).unwrap_or(0)]);
                        if path.to_lowercase() == want {
                            let mut abuf = [0u16; 2048];
                            let _ = link.GetArguments(&mut abuf);
                            let args = String::from_utf16_lossy(&abuf[..abuf.iter().position(|c| *c == 0).unwrap_or(0)]);
                            // the label is the link's Title property (else its description)
                            let mut name = link
                                .cast::<IPropertyStore>()
                                .and_then(|ps| ps.GetValue(&PKEY_Title))
                                .ok()
                                .and_then(|pv| PropVariantToStringAlloc(&pv).ok())
                                .map(pwstr_take)
                                .unwrap_or_default();
                            if name.is_empty() {
                                let mut dbuf = [0u16; 512];
                                let _ = link.GetDescription(&mut dbuf);
                                name = String::from_utf16_lossy(&dbuf[..dbuf.iter().position(|c| *c == 0).unwrap_or(0)]);
                            }
                            if !name.is_empty() {
                                found.push(JumpItem { name, path, args, link: true, task: false });
                            }
                        }
                    }
                }
                i += HEADER.len();
            } else {
                i += 1;
            }
        }
        if !found.is_empty() {
            for f in found {
                if out.len() < max && !out.iter().any(|o| o.name == f.name && o.args == f.args) {
                    out.push(f);
                }
            }
        }
    }
    out
}

/// Recent (or, if none, frequent) items of the app owning `hwnd` / launched
/// by `target` (exe, .lnk or shell:AppsFolder\<id>) / running `exe`.
pub fn items(hwnd: Option<isize>, target: &str, exe: &str, max: usize) -> Vec<JumpItem> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let mut ids: Vec<String> = Vec::new();
    let mut push = |s: Option<String>| {
        if let Some(s) = s {
            if !ids.iter().any(|x| x.eq_ignore_ascii_case(&s)) {
                ids.push(s);
            }
        }
    };
    if let Some(h) = hwnd {
        push(crate::icons::window_aumid(HWND(h as *mut _)));
    }
    let tl = target.to_lowercase();
    if let Some(id) = tl.starts_with("shell:appsfolder\\").then(|| target["shell:AppsFolder\\".len()..].to_string()) {
        push(Some(id));
    }
    if tl.ends_with(".lnk") {
        push(lnk_aumid(target));
        if let Some(t) = crate::apps::resolve_lnk(target) {
            push(implicit_aumid(&t));
        }
    }
    if tl.ends_with(".exe") {
        push(implicit_aumid(target));
    }
    if !exe.is_empty() {
        push(implicit_aumid(exe));
    }
    // Explorer's own list lives under its explicit id
    if tl.ends_with("\\explorer.exe") || exe.to_lowercase().ends_with("\\explorer.exe") {
        push(Some("Microsoft.Windows.Explorer".into()));
    }

    let Ok(adl) = (unsafe { CoCreateInstance::<_, IApplicationDocumentLists>(&ApplicationDocumentLists, None, CLSCTX_INPROC_SERVER) })
    else {
        return vec![];
    };
    for id in ids {
        let w = wide(&id);
        if unsafe { adl.SetAppID(PCWSTR(w.as_ptr())) }.is_err() {
            continue;
        }
        let mut out = Vec::new();
        read_list(&adl, ADLT_RECENT, max, &mut out);
        if out.is_empty() {
            read_list(&adl, ADLT_FREQUENT, max, &mut out);
        }
        if !out.is_empty() {
            return classify(out);
        }
    }
    // Electron & co.: their own ("custom") list
    let real_exe = if !exe.is_empty() {
        exe.to_string()
    } else if tl.ends_with(".lnk") {
        crate::apps::resolve_lnk(target).unwrap_or_default()
    } else if tl.ends_with(".exe") {
        target.to_string()
    } else {
        String::new()
    };
    if !real_exe.is_empty() {
        return classify(custom_items(&real_exe, max));
    }
    vec![]
}

#[cfg(test)]
mod tests {
    /// Prints this machine's lists (manual check: `cargo test jumplist -- --nocapture`).
    #[test]
    fn jumplist_prints_recent_items() {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let code = format!(r"{local}\Programs\Microsoft VS Code\Code.exe");
        for (label, target) in [("code", code.as_str()), ("explorer", r"C:\Windows\explorer.exe")] {
            println!("--- {label}: implicit id {:?}", super::implicit_aumid(target));
            for j in super::items(None, target, "", 8) {
                println!("  {} | {} {}", j.name, j.path, j.args);
            }
        }
    }
}

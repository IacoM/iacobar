//! Settings backups: automatic copies before each change (at most one every
//! few minutes, last 10 kept), export / import through the native Windows
//! file dialogs, restore of an automatic copy.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH};
use windows::core::{PCWSTR, w};

use crate::config::Settings;

const KEEP: usize = 10;
const MIN_GAP: Duration = Duration::from_secs(5 * 60);

fn dir(config: &Path) -> PathBuf {
    config.parent().unwrap_or(Path::new(".")).join("backups")
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Automatic copies, newest first: (path, unix seconds).
fn copies(config: &Path) -> Vec<(PathBuf, u64)> {
    let mut v: Vec<(PathBuf, u64)> = std::fs::read_dir(dir(config))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let secs = name.strip_prefix("config-")?.strip_suffix(".json")?.parse().ok()?;
                    Some((e.path(), secs))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

/// Called before the settings file is overwritten with `new_json`.
pub fn before_save(config: &Path, new_json: &[u8]) {
    let Ok(old) = std::fs::read(config) else { return }; // first save
    if old == new_json {
        return;
    }
    let list = copies(config);
    if let Some((_, last)) = list.first() {
        if now_secs().saturating_sub(*last) < MIN_GAP.as_secs() {
            return; // a recent copy already covers this burst of changes
        }
    }
    let d = dir(config);
    let _ = std::fs::create_dir_all(&d);
    let _ = std::fs::write(d.join(format!("config-{}.json", now_secs())), &old);
    for (p, _) in copies(config).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(p);
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub id: String,
    /// unix seconds
    pub time: u64,
    pub pinned: usize,
}

pub fn list(config: &Path) -> Vec<BackupInfo> {
    copies(config)
        .into_iter()
        .filter_map(|(p, time)| {
            let s: Settings = parse(&std::fs::read(&p).ok()?)?;
            Some(BackupInfo { id: p.file_name()?.to_string_lossy().to_string(), time, pinned: s.pinned.len() })
        })
        .collect()
}

pub fn read_copy(config: &Path, id: &str) -> Result<Settings, String> {
    // only plain names of our own copies
    if !id.starts_with("config-") || id.contains(['\\', '/']) {
        return Err("copia non valida".into());
    }
    let bytes = std::fs::read(dir(config).join(id)).map_err(|e| e.to_string())?;
    parse(&bytes).ok_or_else(|| "copia illeggibile".into())
}

/// Newest readable automatic copy (used when config.json is damaged).
pub fn newest(config: &Path) -> Option<Settings> {
    copies(config).into_iter().find_map(|(p, _)| parse(&std::fs::read(p).ok()?))
}

pub fn parse(bytes: &[u8]) -> Option<Settings> {
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    serde_json::from_slice(body).ok()
}

fn sta() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

const FILTER_NAME: PCWSTR = w!("Impostazioni Iacobar (*.json)");
const FILTER_SPEC: PCWSTR = w!("*.json");

/// "Save as" dialog; writes `settings` there. Ok(false) = cancelled.
pub fn export(settings: &Settings) -> Result<bool, String> {
    sta();
    unsafe {
        let dlg: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let filters = [COMDLG_FILTERSPEC { pszName: FILTER_NAME, pszSpec: FILTER_SPEC }];
        let _ = dlg.SetFileTypes(&filters);
        let _ = dlg.SetDefaultExtension(w!("json"));
        let _ = dlg.SetFileName(w!("iacobar-impostazioni.json"));
        let _ = dlg.SetTitle(w!("Esporta impostazioni della barra"));
        if dlg.Show(None).is_err() {
            return Ok(false); // cancelled
        }
        let item = dlg.GetResult().map_err(|e| e.to_string())?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).map_err(|e| e.to_string())?;
        let path = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(true)
    }
}

/// "Open" dialog; returns the settings read from the chosen file.
pub fn import() -> Result<Option<Settings>, String> {
    sta();
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let filters = [COMDLG_FILTERSPEC { pszName: FILTER_NAME, pszSpec: FILTER_SPEC }];
        let _ = dlg.SetFileTypes(&filters);
        let _ = dlg.SetTitle(w!("Importa impostazioni della barra"));
        if dlg.Show(None).is_err() {
            return Ok(None);
        }
        let item = dlg.GetResult().map_err(|e| e.to_string())?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).map_err(|e| e.to_string())?;
        let path = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let s = parse(&bytes).ok_or("Il file non contiene impostazioni valide della barra")?;
        Ok(Some(s))
    }
}

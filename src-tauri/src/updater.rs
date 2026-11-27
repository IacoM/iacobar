//! Automatic updates from GitHub Releases (`latest.json`, see the README).
//!
//! A background check runs shortly after startup and then every few hours;
//! when a newer version is out, every window gets `update-available` and the
//! bar shows a dot. Installing downloads the signed NSIS installer, gives
//! Windows its taskbar back, then hands over to the installer, which restarts
//! Iacobar when it's done.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

/// First check after startup (the bar settles first), then every 6 hours.
const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

static PENDING: Mutex<Option<Update>> = Mutex::new(None);

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
}

#[derive(Serialize, Clone)]
pub struct UpdateStatus {
    pub current: String,
    pub available: Option<UpdateInfo>,
}

#[derive(Serialize, Clone)]
struct Progress {
    done: usize,
    total: Option<u64>,
}

fn info(u: &Update) -> UpdateInfo {
    UpdateInfo { version: u.version.clone(), notes: u.body.clone().unwrap_or_default() }
}

pub fn status(app: &AppHandle) -> UpdateStatus {
    UpdateStatus {
        current: app.package_info().version.to_string(),
        available: PENDING.lock().unwrap().as_ref().map(info),
    }
}

/// Asks GitHub for a newer version; remembers it and tells the windows.
pub async fn check(app: &AppHandle) -> Result<UpdateStatus, String> {
    let updater = app
        .updater_builder()
        .on_before_exit(crate::cleanup) // the installer closes us: taskbar back first
        .build()
        .map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(|e| e.to_string())?;
    if let Some(u) = &found {
        let _ = app.emit("update-available", info(u));
    }
    *PENDING.lock().unwrap() = found;
    Ok(status(app))
}

/// Downloads and runs the installer. On success the process exits inside
/// `download_and_install`, so returning at all means it failed.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let Some(update) = PENDING.lock().unwrap().take() else {
        return Err("nessun aggiornamento disponibile".into());
    };
    let mut done = 0usize;
    let h = app.clone();
    let res = update
        .download_and_install(
            |chunk, total| {
                done += chunk;
                let _ = h.emit("update-progress", Progress { done, total });
            },
            || {},
        )
        .await;
    if let Err(e) = res {
        *PENDING.lock().unwrap() = Some(update); // let the user retry
        return Err(e.to_string());
    }
    Ok(())
}

pub fn start(app: AppHandle) {
    if cfg!(debug_assertions) {
        return; // `tauri dev`: no update checks against the released app
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            if let Err(e) = tauri::async_runtime::block_on(check(&app)) {
                eprintln!("[update] check failed: {e}"); // offline, GitHub down...
            }
            std::thread::sleep(EVERY);
        }
    });
}

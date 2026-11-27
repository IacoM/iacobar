//! User settings + pinned apps, stored as JSON in the app config dir
//! (%APPDATA%\app.iacobar\config.json).

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pinned {
    pub name: String,
    /// What to launch (exe, .lnk or shell: target).
    pub target: String,
    /// Lower-case exe path, used to merge with running windows.
    pub app_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub pinned: Vec<Pinned>,
    pub show_stats: bool,
    pub show_media: bool,
    pub show_labels: bool,
    pub centered: bool,
    /// Show every window on every monitor's bar (default: only its own).
    pub all_monitors: bool,
    /// Start together with Windows (release builds only).
    pub autostart: bool,
    /// Hide the bar when the pointer leaves it; show at the bottom edge.
    pub auto_hide: bool,
    /// Floating islands over the windows instead of a fixed, blurred bar.
    pub floating: bool,
    /// Colour theme id (see src/lib/theme.ts).
    pub theme: String,
    /// Accent colour: "system" (Windows), "" (the theme's own) or "#rrggbb".
    pub accent: String,
    /// Dock icon size: "small" | "medium" | "large".
    pub icon_size: String,
    /// Island corners: "square" | "medium" | "round".
    pub roundness: String,
    /// Clock with seconds.
    pub clock_seconds: bool,
    /// Turn off animations.
    pub reduce_motion: bool,
    /// Automatic light theme: "off" | "windows" (follow Windows' app mode) |
    /// "time" (light 7:00-19:00).
    pub auto_theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        let explorer = r"C:\Windows\explorer.exe".to_string();
        Settings {
            pinned: vec![Pinned {
                name: "Esplora file".into(),
                app_id: explorer.to_lowercase(),
                target: explorer,
            }],
            show_stats: true,
            show_media: true,
            show_labels: false,
            centered: true,
            all_monitors: false,
            autostart: true,
            auto_hide: false,
            floating: false,
            theme: "aurora".into(),
            accent: "system".into(),
            icon_size: "medium".into(),
            roundness: "medium".into(),
            clock_seconds: false,
            reduce_motion: false,
            auto_theme: "off".into(),
        }
    }
}

static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);

pub fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("config.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    let mut g = SETTINGS.lock().unwrap();
    if let Some(s) = g.as_ref() {
        return s.clone();
    }
    let file = path(app);
    let bytes = file.as_ref().and_then(|p| std::fs::read(p).ok());
    let s = match bytes {
        None => Settings::default(), // first run
        Some(b) => {
            // tolerate a UTF-8 BOM (files saved by Notepad / PowerShell)
            let body = b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&b);
            serde_json::from_slice(body).unwrap_or_else(|e| {
                // unreadable: keep a copy instead of silently losing the
                // user's pinned apps on the next save
                eprintln!("[config] unreadable ({e}), using the last backup or defaults");
                if let Some(p) = &file {
                    let _ = std::fs::copy(p, p.with_extension("bad.json"));
                }
                file.as_deref().and_then(crate::backup::newest).unwrap_or_default()
            })
        }
    };
    *g = Some(s);
    g.clone().unwrap()
}

pub fn save(app: &AppHandle, s: Settings) {
    if let Some(p) = path(app) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(&s) {
            crate::backup::before_save(&p, &json);
            let _ = std::fs::write(p, json);
        }
    }
    *SETTINGS.lock().unwrap() = Some(s);
}

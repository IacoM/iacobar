//! "Now playing" from any app that integrates with Windows media controls
//! (Spotify, browsers, YouTube, Media Player, ...).

use serde::Serialize;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub source: String,
}

fn session() -> windows::core::Result<Session> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    Manager::RequestAsync()?.get()?.GetCurrentSession()
}

pub fn info() -> Option<MediaInfo> {
    let s = session().ok()?;
    let props = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = props.Title().ok()?.to_string();
    if title.is_empty() {
        return None;
    }
    let playing = s
        .GetPlaybackInfo()
        .and_then(|p| p.PlaybackStatus())
        .map(|st| st == Status::Playing)
        .unwrap_or(false);
    Some(MediaInfo {
        title,
        artist: props.Artist().map(|a| a.to_string()).unwrap_or_default(),
        playing,
        source: s.SourceAppUserModelId().map(|a| a.to_string()).unwrap_or_default(),
    })
}

/// Cover art of the current track as a data URL. Read only when the track
/// changes (cached by title + artist): the bar asks for it on every new song.
pub fn art() -> Option<String> {
    use base64::Engine;
    use std::sync::Mutex;
    use windows::Storage::Streams::DataReader;
    static CACHE: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);

    let s = session().ok()?;
    let props = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let key = format!(
        "{}|{}|{}",
        props.Title().map(|t| t.to_string()).unwrap_or_default(),
        props.Artist().map(|t| t.to_string()).unwrap_or_default(),
        props.AlbumTitle().map(|t| t.to_string()).unwrap_or_default()
    );
    if let Some((k, v)) = CACHE.lock().unwrap().as_ref() {
        if *k == key {
            return v.clone();
        }
    }
    let url = (|| -> windows::core::Result<Option<String>> {
        let Ok(thumb) = props.Thumbnail() else { return Ok(None) };
        let stream = thumb.OpenReadAsync()?.get()?;
        let size = stream.Size()? as u32;
        if size == 0 || size > 4 * 1024 * 1024 {
            return Ok(None);
        }
        let mime = stream.ContentType().map(|c| c.to_string()).unwrap_or_default();
        let reader = DataReader::CreateDataReader(&stream)?;
        reader.LoadAsync(size)?.get()?;
        let mut buf = vec![0u8; size as usize];
        reader.ReadBytes(&mut buf)?;
        let mime = if mime.starts_with("image/") { mime } else { "image/jpeg".into() };
        Ok(Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&buf))))
    })()
    .ok()
    .flatten();
    *CACHE.lock().unwrap() = Some((key, url.clone()));
    url
}

pub fn action(what: &str) {
    if let Ok(s) = session() {
        let r = match what {
            "next" => s.TrySkipNextAsync(),
            "prev" => s.TrySkipPreviousAsync(),
            _ => s.TryTogglePlayPauseAsync(),
        };
        if let Ok(op) = r {
            let _ = op.get();
        }
    }
}

//! Live window previews: DWM thumbnails drawn into the "preview" window,
//! exactly where the frontend placed the cards.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_SOURCECLIENTAREAONLY,
    DWM_TNP_VISIBLE, DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmUnregisterThumbnail,
    DwmUpdateThumbnailProperties,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, IsIconic, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_NOACTIVATE,
};
use windows::core::BOOL;

static THUMBS: Mutex<Vec<isize>> = Mutex::new(Vec::new());
/// Incremented on every show; a pending hide only applies to its own token.
static TOKEN: AtomicU64 = AtomicU64::new(0);
/// True while the mouse is over the preview window.
static HOLD: AtomicBool = AtomicBool::new(false);
static VISIBLE: AtomicBool = AtomicBool::new(false);

#[derive(Deserialize, Debug)]
pub struct ThumbRect {
    pub hwnd: isize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn preview_hwnd(app: &AppHandle) -> Option<HWND> {
    let w = app.get_webview_window("preview")?;
    Some(HWND(w.hwnd().ok()?.0 as *mut _))
}

/// The preview must never take focus from the app the user is looking at.
pub fn make_noactivate(app: &AppHandle) {
    if let Some(h) = preview_hwnd(app) {
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
        }
    }
}

fn clear() {
    for t in THUMBS.lock().unwrap().drain(..) {
        unsafe {
            let _ = DwmUnregisterThumbnail(t);
        }
    }
}

/// Positions (physical px) and shows the preview window without activating it.
pub fn show_at(app: &AppHandle, x: i32, y: i32, w: i32, h: i32) {
    TOKEN.fetch_add(1, Ordering::SeqCst);
    let Some(hwnd) = preview_hwnd(app) else { return };
    clear();
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    VISIBLE.store(true, Ordering::SeqCst);
}

/// Registers one live thumbnail per card (rects are logical px).
pub fn set_thumbs(app: &AppHandle, rects: &[ThumbRect]) {
    let Some(dest) = preview_hwnd(app) else { return };
    let scale = app.get_webview_window("preview").and_then(|w| w.scale_factor().ok()).unwrap_or(1.0);
    clear();
    let mut list = THUMBS.lock().unwrap();
    for r in rects {
        let src = HWND(r.hwnd as *mut _);
        unsafe {
            if IsIconic(src).as_bool() {
                continue; // minimized windows have nothing to show
            }
            let Ok(th) = DwmRegisterThumbnail(dest, src) else { continue };
            // aspect-fit the source inside the card area
            let (bx, by, bw, bh) = (r.x * scale, r.y * scale, r.w * scale, r.h * scale);
            let (dw, dh) = match DwmQueryThumbnailSourceSize(th) {
                Ok(sz) if sz.cx > 0 && sz.cy > 0 => {
                    let k = (bw / sz.cx as f64).min(bh / sz.cy as f64);
                    (sz.cx as f64 * k, sz.cy as f64 * k)
                }
                _ => (bw, bh),
            };
            let left = bx + (bw - dw) / 2.0;
            let top = by + (bh - dh) / 2.0;
            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: RECT {
                    left: left.round() as i32,
                    top: top.round() as i32,
                    right: (left + dw).round() as i32,
                    bottom: (top + dh).round() as i32,
                },
                opacity: 255,
                fVisible: BOOL(1),
                fSourceClientAreaOnly: BOOL(0),
                ..Default::default()
            };
            let _ = DwmUpdateThumbnailProperties(th, &props);
            list.push(th);
        }
    }
}

pub fn hide_now(app: &AppHandle) {
    clear();
    HOLD.store(false, Ordering::SeqCst);
    VISIBLE.store(false, Ordering::SeqCst);
    if let Some(h) = preview_hwnd(app) {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}

/// Hides after `delay` ms unless the mouse enters the preview or another
/// preview is shown meanwhile.
pub fn hide_later(app: AppHandle, delay: u64) {
    if delay == 0 {
        hide_now(&app);
        return;
    }
    let token = TOKEN.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(delay));
        if TOKEN.load(Ordering::SeqCst) == token && !HOLD.load(Ordering::SeqCst) {
            hide_now(&app);
        }
    });
}

pub fn hold(on: bool) {
    HOLD.store(on, Ordering::SeqCst);
}


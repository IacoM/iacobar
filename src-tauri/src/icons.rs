//! Icon extraction -> PNG data URLs, cached per (source, pixel size).
//!
//! Sources, best first:
//! - executables: `PrivateExtractIconsW` at the exact pixel size requested;
//! - anything the shell knows (shortcuts, Store apps via
//!   `shell:AppsFolder\<AUMID>`, documents): `IShellItemImageFactory`;
//! - windows: their AppUserModelID (UWP / packaged apps), then WM_GETICON.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use base64::Engine;
use windows::Win32::Foundation::{HWND, LPARAM, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW,
    HBITMAP, HGDIOBJ, ReleaseDC,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GCLP_HICON, GetClassLongPtrW, GetIconInfo, HICON, ICON_BIG, ICONINFO, PrivateExtractIconsW,
    SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_GETICON,
};
use windows::core::PCWSTR;

static CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn com_sta() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

fn png_data_url(rgba: &[u8], w: u32, h: u32) -> Option<String> {
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png_bytes)))
}

/// Reads a 32bpp bitmap as straight-alpha RGBA.
/// `premultiplied`: shell bitmaps are premultiplied, icon bitmaps are not.
fn bitmap_to_data_url(hbm: HBITMAP, premultiplied: bool) -> Option<String> {
    unsafe {
        let mut bmp = BITMAP::default();
        if GetObjectW(HGDIOBJ(hbm.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bmp as *mut _ as *mut _)) == 0 {
            return None;
        }
        let (w, h) = (bmp.bmWidth, bmp.bmHeight.abs());
        if w <= 0 || h <= 0 {
            return None;
        }
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let hdc = GetDC(None);
        let lines = GetDIBits(hdc, hbm, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        ReleaseDC(None, hdc);
        if lines == 0 {
            return None;
        }
        let has_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2); // BGRA -> RGBA
            if !has_alpha {
                px[3] = 255; // legacy icons without an alpha channel
            } else if premultiplied && px[3] > 0 && px[3] < 255 {
                let a = px[3] as u32;
                for c in &mut px[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        if !buf.chunks_exact(4).any(|p| p[3] != 0) {
            return None;
        }
        png_data_url(&buf, w as u32, h as u32)
    }
}

pub fn hicon_to_data_url(hicon: HICON) -> Option<String> {
    unsafe {
        let mut info = ICONINFO::default();
        GetIconInfo(hicon, &mut info).ok()?;
        // native size: colour bitmap, or half the height of a mask-only icon
        let mut size = 32;
        let probe = if info.hbmColor.is_invalid() { info.hbmMask } else { info.hbmColor };
        let mut bmp = BITMAP::default();
        if !probe.is_invalid()
            && GetObjectW(HGDIOBJ(probe.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bmp as *mut _ as *mut _)) != 0
        {
            size = if info.hbmColor.is_invalid() { bmp.bmHeight.abs() / 2 } else { bmp.bmWidth };
        }
        let url = if info.hbmColor.is_invalid() { None } else { bitmap_to_data_url(info.hbmColor, false) };
        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        // monochrome / mask-only / odd formats: draw it like Windows does
        url.or_else(|| render_icon(hicon, size.clamp(16, 64)))
    }
}

/// Draws any icon (monochrome, mask-only, no alpha...) exactly as Windows
/// would, once on black and once on white, and derives the transparency
/// from the difference.
fn render_icon(hicon: HICON, size: i32) -> Option<String> {
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, CreateDIBSection, DeleteDC, GdiFlush, SelectObject};
    use windows::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DrawIconEx};
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let result = (|| {
            let dib = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            let old = SelectObject(dc, HGDIOBJ(dib.0));
            let n = (size * size) as usize;
            let px = std::slice::from_raw_parts_mut(bits as *mut u32, n);
            let mut draw = |bg: u32| {
                px.fill(bg);
                let _ = GdiFlush();
                let _ = DrawIconEx(dc, 0, 0, hicon, size, size, 0, None, DI_NORMAL);
                let _ = GdiFlush();
                px.to_vec()
            };
            let on_black = draw(0x0000_0000);
            let on_white = draw(0x00FF_FFFF);
            SelectObject(dc, old);
            let _ = DeleteObject(HGDIOBJ(dib.0));
            let mut rgba = vec![0u8; n * 4];
            let mut any = false;
            for i in 0..n {
                let (b, w) = (on_black[i], on_white[i]);
                let ch = |v: u32, s: u32| ((v >> s) & 0xff) as i32;
                let alpha = (255 - (ch(w, 8) - ch(b, 8))).clamp(0, 255);
                if alpha > 0 {
                    any = true;
                    let un = |c: i32| ((c * 255 + alpha / 2) / alpha).clamp(0, 255) as u8;
                    rgba[i * 4] = un(ch(b, 16));
                    rgba[i * 4 + 1] = un(ch(b, 8));
                    rgba[i * 4 + 2] = un(ch(b, 0));
                    rgba[i * 4 + 3] = alpha as u8;
                }
            }
            if any { png_data_url(&rgba, size as u32, size as u32) } else { None }
        })();
        let _ = DeleteDC(dc);
        ReleaseDC(None, screen);
        result
    }
}

/// Icon embedded in an .exe / .dll / .ico at exactly `size` pixels.
fn icon_from_exe(path: &str, size: i32) -> Option<String> {
    icon_from_file_index(path, 0, size)
}

/// Icon number `index` (negative = resource id) of a binary / .ico file.
fn icon_from_file_index(path: &str, index: i32, size: i32) -> Option<String> {
    let mut name = [0u16; 260];
    for (i, c) in path.encode_utf16().take(259).enumerate() {
        name[i] = c;
    }
    let mut icons = [HICON::default(); 1];
    let n = unsafe { PrivateExtractIconsW(&name, index, size, size, Some(&mut icons), None, 0) };
    if n == 0 || n == u32::MAX || icons[0].is_invalid() {
        return None;
    }
    let url = hicon_to_data_url(icons[0]);
    unsafe {
        let _ = DestroyIcon(icons[0]);
    }
    url
}

/// What Explorer shows for a path / parsing name (shortcuts, Store apps, files).
fn icon_from_shell(parsing_name: &str, size: i32) -> Option<String> {
    com_sta();
    let w = wide(parsing_name);
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None).ok()?;
        let hbm = factory.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY).ok()?;
        let url = bitmap_to_data_url(hbm, true);
        let _ = DeleteObject(HGDIOBJ(hbm.0));
        url
    }
}

/// AppUserModelID of a window (packaged / UWP apps and many modern apps).
pub fn window_aumid(hwnd: HWND) -> Option<String> {
    com_sta();
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        let pv = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&pv).ok()?;
        let out = s.to_string().ok();
        CoTaskMemFree(Some(s.0 as *const _));
        out.filter(|v| !v.is_empty())
    }
}

/// Icon for a window: its app identity first, then what the window advertises.
pub fn icon_from_window(hwnd: HWND, size: i32) -> Option<String> {
    let key = format!("hwnd:{}@{size}", hwnd.0 as isize);
    if let Some(v) = CACHE.lock().unwrap().get(&key) {
        return v.clone();
    }
    let url = window_aumid(hwnd)
        .and_then(|id| icon_from_shell(&format!("shell:AppsFolder\\{id}"), size))
        .or_else(|| unsafe {
            let mut res: usize = 0;
            SendMessageTimeoutW(hwnd, WM_GETICON, WPARAM(ICON_BIG as usize), LPARAM(0), SMTO_ABORTIFHUNG, 100, Some(&mut res));
            let mut h = res as isize;
            if h == 0 {
                h = GetClassLongPtrW(hwnd, GCLP_HICON) as isize;
            }
            // not ours: never destroy it
            if h == 0 { None } else { hicon_to_data_url(HICON(h as *mut _)) }
        });
    CACHE.lock().unwrap().insert(key, url.clone());
    url
}

/// Cached icon for an exe, shortcut, `shell:` target or any file.
pub fn icon_for_path(path: &str, size: i32) -> Option<String> {
    let size = size.clamp(16, 256);
    let low = path.to_lowercase();
    let key = format!("{low}@{size}");
    if let Some(v) = CACHE.lock().unwrap().get(&key) {
        return v.clone();
    }
    let is_bin = low.ends_with(".exe") || low.ends_with(".ico") || low.ends_with(".dll");
    let url = if low.starts_with("shell:") {
        icon_from_shell(path, size)
    } else if is_bin {
        icon_from_exe(path, size).or_else(|| icon_from_shell(path, size))
    } else if low.ends_with(".lnk") {
        // 1. the icon the shortcut explicitly points to (file + index)
        // 2. the target exe's main icon   3. whatever the shell draws
        crate::apps::lnk_icon_location(path)
            .and_then(|(file, idx)| {
                let f = file.to_lowercase();
                if f.ends_with(".exe") || f.ends_with(".dll") || f.ends_with(".ico") {
                    icon_from_file_index(&file, idx, size)
                } else {
                    icon_from_shell(&file, size)
                }
            })
            .or_else(|| {
                crate::apps::resolve_lnk(path)
                    .filter(|t| t.to_lowercase().ends_with(".exe"))
                    .and_then(|t| icon_from_exe(&t, size))
            })
            .or_else(|| icon_from_shell(path, size))
    } else {
        icon_from_shell(path, size)
    };
    CACHE.lock().unwrap().insert(key, url.clone());
    url
}

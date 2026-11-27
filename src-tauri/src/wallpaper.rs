//! Colours of the desktop wallpaper for the "Dallo sfondo" theme.
//!
//! The wallpaper (whatever its format, Spotlight and slideshows included:
//! Windows keeps the current one in TranscodedWallpaper) is decoded and
//! shrunk with WIC, then its vivid colours are grouped by hue: the strongest
//! group becomes the accent, the next two the glow colours, and the overall
//! average tints the dark surfaces.

use std::sync::Mutex;
use std::time::SystemTime;

use serde::Serialize;
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory, WICBitmapDitherTypeNone,
    WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::UI::WindowsAndMessaging::{SPI_GETDESKWALLPAPER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW};
use windows::core::PCWSTR;

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Palette {
    pub accent: String,
    pub a2: String,
    pub a3: String,
    /// average colour (tints the surfaces)
    pub base: String,
}

const W: u32 = 64;
const H: u32 = 36;

fn wallpaper_path() -> Option<String> {
    // the image actually on screen (also for Spotlight / slideshows)
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = format!(r"{appdata}\Microsoft\Windows\Themes\TranscodedWallpaper");
        if std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }
    let mut buf = [0u16; 520];
    unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buf.len() as u32,
            Some(buf.as_mut_ptr() as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .ok()?;
    }
    let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
    let s = String::from_utf16_lossy(&buf[..n]);
    (!s.is_empty()).then_some(s)
}

/// BGRA pixels of the wallpaper shrunk to W x H.
fn pixels(path: &str) -> windows::core::Result<Vec<u8>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let f: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let dec = f.CreateDecoderFromFilename(PCWSTR(wide.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)?;
        let frame = dec.GetFrame(0)?;
        let scaler = f.CreateBitmapScaler()?;
        scaler.Initialize(&frame, W, H, WICBitmapInterpolationModeFant)?;
        let conv = f.CreateFormatConverter()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
        let mut buf = vec![0u8; (W * H * 4) as usize];
        conv.CopyPixels(std::ptr::null(), W * 4, &mut buf)?;
        Ok(buf)
    }
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

fn hsv_to_hex(h: f32, s: f32, v: f32) -> String {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", to(r), to(g), to(b))
}

/// Vivid hue groups, strongest first: (mean hue, mean saturation, weight).
fn hue_groups(px: &[u8]) -> (Vec<(f32, f32, f32)>, (f32, f32, f32)) {
    const BINS: usize = 18; // 20 degrees each
    let mut w = [0f32; BINS];
    let mut hx = [0f32; BINS]; // hue as a vector (circular mean)
    let mut hy = [0f32; BINS];
    let mut sat = [0f32; BINS];
    let (mut ar, mut ag, mut ab) = (0f32, 0f32, 0f32);
    let n = (px.len() / 4) as f32;
    for p in px.chunks_exact(4) {
        let (b, g, r) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
        ar += r;
        ag += g;
        ab += b;
        let (h, s, v) = rgb_to_hsv(r, g, b);
        if s < 0.22 || v < 0.18 {
            continue; // greys and near-blacks say nothing about the mood
        }
        let k = s * v; // vivid and bright pixels count more
        let i = ((h / 360.0 * BINS as f32) as usize).min(BINS - 1);
        w[i] += k;
        hx[i] += k * h.to_radians().cos();
        hy[i] += k * h.to_radians().sin();
        sat[i] += k * s;
    }
    let mut groups: Vec<(f32, f32, f32)> = (0..BINS)
        .filter(|&i| w[i] > 0.0)
        .map(|i| {
            let mut h = hy[i].atan2(hx[i]).to_degrees();
            if h < 0.0 {
                h += 360.0;
            }
            (h, sat[i] / w[i], w[i])
        })
        .collect();
    groups.sort_by(|a, b| b.2.total_cmp(&a.2));
    (groups, (ar / n, ag / n, ab / n))
}

fn hue_dist(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 360.0;
    d.min(360.0 - d)
}

fn compute(px: &[u8]) -> Palette {
    let (groups, (ar, ag, ab)) = hue_groups(px);
    let (bh, bs, _) = rgb_to_hsv(ar, ag, ab);
    // dark, slightly tinted base for the surfaces
    let base = hsv_to_hex(bh, (bs * 0.9).min(0.55), 0.16);
    // colours tuned for a dark UI: bright and fairly saturated
    let tune = |h: f32, s: f32| hsv_to_hex(h, s.clamp(0.5, 0.85), 0.95);
    let mut picked: Vec<(f32, f32)> = Vec::new();
    for (h, s, _) in &groups {
        if picked.iter().all(|(ph, _)| hue_dist(*ph, *h) >= 35.0) {
            picked.push((*h, *s));
        }
        if picked.len() == 3 {
            break;
        }
    }
    // greyish wallpaper: derive the missing colours from the main hue
    let main = picked.first().copied().unwrap_or((if bs > 0.05 { bh } else { 210.0 }, 0.6));
    while picked.len() < 3 {
        let k = picked.len() as f32;
        picked.push(((main.0 + 40.0 * k + 200.0 * (k - 1.0).max(0.0)) % 360.0, main.1));
    }
    Palette {
        accent: tune(picked[0].0, picked[0].1),
        a2: tune(picked[1].0, picked[1].1),
        a3: tune(picked[2].0, picked[2].1),
        base,
    }
}

/// Palette of the current wallpaper (cached until the image changes).
pub fn palette() -> Option<Palette> {
    static CACHE: Mutex<Option<(String, SystemTime, Palette)>> = Mutex::new(None);
    let path = wallpaper_path()?;
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    if let Some((p, t, pal)) = CACHE.lock().unwrap().as_ref() {
        if *p == path && *t == mtime {
            return Some(pal.clone());
        }
    }
    let px = pixels(&path).ok()?;
    let pal = compute(&px);
    *CACHE.lock().unwrap() = Some((path, mtime, pal.clone()));
    Some(pal)
}

#[cfg(test)]
mod tests {
    #[test]
    fn wallpaper_palette_prints() {
        println!("path: {:?}", super::wallpaper_path());
        println!("palette: {:?}", super::palette());
    }
}

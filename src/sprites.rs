use crate::config::log;
use crate::render::Canvas;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

const FALLBACK_JSON: &str = include_str!("../themes/office/theme.json");
const FALLBACK_PNG: &[u8] = include_bytes!("../themes/office/sheet.png");

#[derive(Deserialize, Clone)]
pub struct AnimDef {
    pub row: u32,
    pub frames: u32,
    pub fps: f32,
}

#[derive(Deserialize)]
struct SwapDef {
    key: String,
    choices: Vec<String>,
}

#[derive(Deserialize)]
struct ThemeFile {
    name: String,
    frame_w: u32,
    frame_h: u32,
    #[serde(default = "default_sheet")]
    sheet: String,
    anims: HashMap<String, AnimDef>,
    #[serde(default)]
    swaps: Vec<SwapDef>,
}

fn default_sheet() -> String {
    "sheet.png".into()
}

pub struct Theme {
    pub name: String,
    pub frame_w: u32,
    pub frame_h: u32,
    anims: HashMap<String, AnimDef>,
    swaps: Vec<([u8; 3], Vec<[u8; 3]>)>,
    sheet: Vec<u8>, // RGBA, straight alpha
    sheet_w: u32,
}

fn hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

pub fn seed_of(s: &str) -> u64 {
    s.bytes()
        .fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn decode_png(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let buf = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf.to_vec(),
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        other => return Err(format!("unsupported png colour type {other:?}")),
    };
    Ok((rgba, info.width, info.height))
}

impl Theme {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(dir.join("theme.json")).map_err(|e| e.to_string())?;
        let tf: ThemeFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        let png = std::fs::read(dir.join(&tf.sheet)).map_err(|e| e.to_string())?;
        Self::build(tf, &png)
    }

    pub fn fallback() -> Self {
        let tf: ThemeFile = serde_json::from_str(FALLBACK_JSON).expect("embedded theme json");
        Self::build(tf, FALLBACK_PNG).expect("embedded theme png")
    }

    fn build(tf: ThemeFile, png: &[u8]) -> Result<Self, String> {
        if !tf.anims.contains_key("idle") {
            return Err("theme needs at least an 'idle' anim".into());
        }
        let (sheet, sheet_w, _) = decode_png(png)?;
        let swaps = tf
            .swaps
            .iter()
            .filter_map(|s| Some((hex(&s.key)?, s.choices.iter().filter_map(|c| hex(c)).collect())))
            .collect();
        Ok(Theme {
            name: tf.name,
            frame_w: tf.frame_w,
            frame_h: tf.frame_h,
            anims: tf.anims,
            swaps,
            sheet,
            sheet_w,
        })
    }

    fn anim(&self, name: &str) -> &AnimDef {
        self.anims.get(name).unwrap_or_else(|| &self.anims["idle"])
    }

    fn swap(&self, rgb: [u8; 3], seed: u64) -> [u8; 3] {
        for (i, (key, choices)) in self.swaps.iter().enumerate() {
            if *key == rgb && !choices.is_empty() {
                let h = mix(seed ^ (i as u64 + 1).wrapping_mul(0x9E3779B97F4A7C15));
                return choices[(h % choices.len() as u64) as usize];
            }
        }
        rgb
    }

    /// Sheet frames face right; `flip` mirrors them.
    pub fn draw(&self, c: &mut Canvas, anim: &str, t: f32, seed: u64, x: i32, y: i32, flip: bool) {
        let a = self.anim(anim);
        let frame = ((t * a.fps) as u32) % a.frames.max(1);
        let (fw, fh) = (self.frame_w, self.frame_h);
        let (ox, oy) = (frame * fw, a.row * fh);
        for sy in 0..fh {
            for sx in 0..fw {
                let i = (((oy + sy) * self.sheet_w + ox + sx) * 4) as usize;
                let Some(p) = self.sheet.get(i..i + 4) else { continue };
                if p[3] < 128 {
                    continue;
                }
                let rgb = self.swap([p[0], p[1], p[2]], seed);
                let dx = if flip { fw - 1 - sx } else { sx };
                c.dot(x + dx as i32, y + sy as i32, rgb, 255);
            }
        }
    }

    /// RGBA icon for the tray, cropped to the sprite's bounding box.
    pub fn icon_rgba(&self, size: u32) -> Vec<u8> {
        let (fw, fh) = (self.frame_w, self.frame_h);
        let mut c = Canvas::new(fw, fh, 1);
        self.draw(&mut c, "idle", 0.0, 0, 0, 0, false);
        let (mut x0, mut y0, mut x1, mut y1) = (fw, fh, 0, 0);
        for y in 0..fh {
            for x in 0..fw {
                if c.px[((y * fw + x) * 4 + 3) as usize] > 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        let mut out = vec![0u8; (size * size * 4) as usize];
        if x1 < x0 {
            return out;
        }
        let side = (x1 - x0 + 1).max(y1 - y0 + 1);
        let (cx, cy) = (x0 + (x1 - x0 + 1) / 2, y0 + (y1 - y0 + 1) / 2);
        for y in 0..size {
            for x in 0..size {
                let sx = cx as i64 - side as i64 / 2 + (x * side / size) as i64;
                let sy = cy as i64 - side as i64 / 2 + (y * side / size) as i64;
                if sx < 0 || sy < 0 || sx >= fw as i64 || sy >= fh as i64 {
                    continue;
                }
                let si = ((sy as u32 * fw + sx as u32) * 4) as usize;
                let di = ((y * size + x) * 4) as usize;
                out[di] = c.px[si + 2];
                out[di + 1] = c.px[si + 1];
                out[di + 2] = c.px[si];
                out[di + 3] = c.px[si + 3];
            }
        }
        out
    }
}

pub fn load_theme(base: &Path, name: &str) -> Theme {
    Theme::load(&base.join("themes").join(name)).unwrap_or_else(|e| {
        log(format!("theme '{name}' failed ({e}), using built-in office"));
        Theme::fallback()
    })
}

/// Theme folders = any subfolder of themes/ containing theme.json.
pub fn list_themes(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().join("theme.json").is_file())
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if v.is_empty() {
        v.push("office".into());
    }
    v.sort();
    v
}

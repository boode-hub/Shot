//! Text: load a system font once, measure and draw lines into a pixmap.
//!
//! Nothing is bundled: every desktop OS ships a usable sans-serif font, so the
//! binary stays small. If none is found, text-based UI degrades silently.

use ab_glyph::{Font as _, FontVec, PxScale, ScaleFont, point};
use std::path::Path;
use tiny_skia::Pixmap;

use crate::style::Color;

pub struct Font {
    inner: FontVec,
}

const CANDIDATES: &[&str] = &[
    // Windows
    r"C:\Windows\Fonts\segoeui.ttf",
    r"C:\Windows\Fonts\arial.ttf",
    r"C:\Windows\Fonts\tahoma.ttf",
    // macOS
    "/System/Library/Fonts/SFNS.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    // Linux
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
];

impl Font {
    pub fn from_bytes(bytes: Vec<u8>) -> Option<Font> {
        FontVec::try_from_vec_and_index(bytes, 0).ok().map(|inner| Font { inner })
    }

    pub fn from_file(path: &Path) -> Option<Font> {
        Self::from_bytes(std::fs::read(path).ok()?)
    }

    /// First usable font from the well-known locations, then any `.ttf`
    /// under the usual font directories.
    pub fn load_system() -> Option<Font> {
        if let Some(f) = CANDIDATES.iter().find_map(|p| Self::from_file(Path::new(p))) {
            return Some(f);
        }
        let mut dirs = vec!["/usr/share/fonts".to_string(), "/usr/local/share/fonts".to_string()];
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(format!("{}/.local/share/fonts", home.to_string_lossy()));
        }
        dirs.iter().find_map(|d| find_ttf(Path::new(d), 4))
    }

    fn scaled(&self, px: f32) -> ab_glyph::PxScaleFont<&FontVec> {
        self.inner.as_scaled(PxScale::from(px))
    }

    pub fn line_height(&self, px: f32) -> f32 {
        let s = self.scaled(px);
        s.ascent() - s.descent() + s.line_gap()
    }

    pub fn line_width(&self, line: &str, px: f32) -> f32 {
        let s = self.scaled(px);
        let mut w = 0.0;
        let mut prev = None;
        for ch in line.chars() {
            let id = s.glyph_id(ch);
            if let Some(p) = prev {
                w += s.kern(p, id);
            }
            w += s.h_advance(id);
            prev = Some(id);
        }
        w
    }

    /// Width and height of (possibly multi-line) text.
    pub fn measure(&self, text: &str, px: f32) -> (f32, f32) {
        let lines: Vec<&str> = text.split('\n').collect();
        let w = lines.iter().map(|l| self.line_width(l, px)).fold(0.0, f32::max);
        (w, lines.len() as f32 * self.line_height(px))
    }

    /// Draw text with its top-left corner at (x, y). Returns the pen position
    /// after the last glyph (useful for a caret).
    pub fn draw(&self, pm: &mut Pixmap, text: &str, x: f32, y: f32, px: f32, color: Color) -> (f32, f32) {
        let s = self.scaled(px);
        let lh = self.line_height(px);
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let data = pm.data_mut();
        let mut end = (x, y);
        for (i, line) in text.split('\n').enumerate() {
            let top = y + i as f32 * lh;
            let baseline = top + s.ascent();
            let mut pen = x;
            let mut prev = None;
            for ch in line.chars() {
                let id = s.glyph_id(ch);
                if let Some(p) = prev {
                    pen += s.kern(p, id);
                }
                let glyph = id.with_scale_and_position(s.scale(), point(pen, baseline));
                pen += s.h_advance(id);
                prev = Some(id);
                let Some(og) = self.inner.outline_glyph(glyph) else { continue };
                let b = og.px_bounds();
                let (ox, oy) = (b.min.x as i32, b.min.y as i32);
                og.draw(|gx, gy, cov| {
                    let (px_, py_) = (ox + gx as i32, oy + gy as i32);
                    if px_ < 0 || py_ < 0 || px_ >= pw || py_ >= ph {
                        return;
                    }
                    let i = ((py_ * pw + px_) * 4) as usize;
                    blend(&mut data[i..i + 4], color, cov.min(1.0));
                });
            }
            end = (pen, top);
        }
        end
    }
}

/// Source-over blend of a straight-alpha colour, scaled by `cov`, onto a
/// premultiplied RGBA pixel.
#[inline]
pub fn blend(dst: &mut [u8], c: Color, cov: f32) {
    let a = c[3] as f32 / 255.0 * cov;
    if a <= 0.0 {
        return;
    }
    let inv = 1.0 - a;
    for k in 0..3 {
        dst[k] = (c[k] as f32 * a + dst[k] as f32 * inv).round() as u8;
    }
    dst[3] = (255.0 * a + dst[3] as f32 * inv).round() as u8;
}

fn find_ttf(dir: &Path, depth: u32) -> Option<Font> {
    let mut entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in &entries {
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ttf"))
            && let Some(f) = Font::from_file(p)
        {
            return Some(f);
        }
    }
    if depth == 0 {
        return None;
    }
    entries.iter().filter(|p| p.is_dir()).find_map(|p| find_ttf(p, depth - 1))
}

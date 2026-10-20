//! Annotations: what the user draws on top of a capture, and how each one is
//! rendered. Coordinates are physical pixels in the captured image.

use tiny_skia::{BlendMode, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};

use crate::geom::{IRect, Pt};
use crate::style::{self, Color};
use crate::text::Font;

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Pen(Vec<Pt>),
    Marker(Vec<Pt>),
    Line(Pt, Pt),
    Arrow(Pt, Pt),
    Rect(Pt, Pt),
    Ellipse(Pt, Pt),
    Blur(Pt, Pt),
    Text { at: Pt, text: String },
    Step { at: Pt, n: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub shape: Shape,
    pub color: Color,
    /// Stroke width for lines and shapes, font size for text, radius for
    /// steps, blur radius for blur. Already scaled for the monitor's DPI.
    pub width: f32,
}

impl Annotation {
    pub fn draw(&self, pm: &mut Pixmap, font: Option<&Font>) {
        let w = self.width.max(1.0);
        match &self.shape {
            Shape::Pen(pts) => {
                if let Some(path) = smooth_path(pts) {
                    stroke(pm, &path, self.color, w, BlendMode::SourceOver);
                }
            }
            Shape::Marker(pts) => {
                if let Some(path) = smooth_path(pts) {
                    // Multiply keeps dark text readable under the ink, like a real highlighter.
                    stroke(pm, &path, style::with_alpha(self.color, 200), w, BlendMode::Multiply);
                }
            }
            Shape::Line(a, b) => {
                if let Some(path) = line_path(*a, *b) {
                    stroke(pm, &path, self.color, w, BlendMode::SourceOver);
                }
            }
            Shape::Arrow(a, b) => draw_arrow(pm, *a, *b, self.color, w),
            Shape::Rect(a, b) => {
                if let Some(r) = rect_of(*a, *b) {
                    stroke(pm, &PathBuilder::from_rect(r), self.color, w, BlendMode::SourceOver);
                }
            }
            Shape::Ellipse(a, b) => {
                if let Some(path) = rect_of(*a, *b).and_then(PathBuilder::from_oval) {
                    stroke(pm, &path, self.color, w, BlendMode::SourceOver);
                }
            }
            Shape::Blur(a, b) => blur_region(pm, IRect::from_points(*a, *b), w.round() as usize),
            Shape::Text { at, text } => {
                if let Some(f) = font {
                    f.draw(pm, text, at.x, at.y, w, self.color);
                }
            }
            Shape::Step { at, n } => draw_step(pm, *at, *n, self.color, w, font),
        }
    }

    /// Pixel bounds touched by this annotation (generous; used for hit tests).
    pub fn bounds(&self, font: Option<&Font>) -> IRect {
        let pad = self.width;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        let mut add = |p: Pt| {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        };
        match &self.shape {
            Shape::Pen(pts) | Shape::Marker(pts) => pts.iter().for_each(|p| add(*p)),
            Shape::Line(a, b) | Shape::Arrow(a, b) | Shape::Rect(a, b) | Shape::Ellipse(a, b) | Shape::Blur(a, b) => {
                add(*a);
                add(*b);
            }
            Shape::Text { at, text } => {
                let (w, h) = font.map(|f| f.measure(text, self.width)).unwrap_or((0.0, 0.0));
                add(*at);
                add(Pt::new(at.x + w, at.y + h));
            }
            Shape::Step { at, .. } => add(*at),
        }
        if x0 > x1 {
            return IRect::default();
        }
        IRect::from_points(Pt::new(x0 - pad, y0 - pad), Pt::new(x1 + pad, y1 + pad))
    }
}

pub fn rect_of(a: Pt, b: Pt) -> Option<Rect> {
    Rect::from_ltrb(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
}

fn paint(color: Color, mode: BlendMode) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(style::skia(color));
    p.anti_alias = true;
    p.blend_mode = mode;
    p
}

pub fn stroke(pm: &mut Pixmap, path: &Path, color: Color, width: f32, mode: BlendMode) {
    let s = Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    pm.stroke_path(path, &paint(color, mode), &s, Transform::identity(), None);
}

pub fn fill(pm: &mut Pixmap, path: &Path, color: Color) {
    pm.fill_path(path, &paint(color, BlendMode::SourceOver), FillRule::Winding, Transform::identity(), None);
}

fn line_path(a: Pt, b: Pt) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(a.x, a.y);
    // A zero-length line still needs a visible dot under a round cap.
    pb.line_to(b.x + if a == b { 0.01 } else { 0.0 }, b.y);
    pb.finish()
}

/// Freehand stroke smoothed with quadratic curves through segment midpoints.
fn smooth_path(pts: &[Pt]) -> Option<Path> {
    match pts.len() {
        0 => None,
        1 => line_path(pts[0], pts[0]),
        _ => {
            let mut pb = PathBuilder::new();
            pb.move_to(pts[0].x, pts[0].y);
            for w in pts.windows(2).skip(1) {
                let mid = Pt::new((w[0].x + w[1].x) / 2.0, (w[0].y + w[1].y) / 2.0);
                pb.quad_to(w[0].x, w[0].y, mid.x, mid.y);
            }
            let last = pts[pts.len() - 1];
            pb.line_to(last.x, last.y);
            pb.finish()
        }
    }
}

fn draw_arrow(pm: &mut Pixmap, a: Pt, b: Pt, color: Color, w: f32) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let head = (w * 3.0 + 10.0).min(len);
    let half = head * 0.5;
    let base = Pt::new(b.x - ux * head, b.y - uy * head);
    if let Some(path) = line_path(a, Pt::new(base.x + ux * 1.0, base.y + uy * 1.0)) {
        stroke(pm, &path, color, w, BlendMode::SourceOver);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(b.x, b.y);
    pb.line_to(base.x - uy * half, base.y + ux * half);
    pb.line_to(base.x + uy * half, base.y - ux * half);
    pb.close();
    if let Some(path) = pb.finish() {
        fill(pm, &path, color);
        // Soften the tip so it matches the round shaft.
        stroke(pm, &path, color, (w * 0.5).max(1.0), BlendMode::SourceOver);
    }
}

fn draw_step(pm: &mut Pixmap, at: Pt, n: u32, color: Color, r: f32, font: Option<&Font>) {
    if let Some(c) = PathBuilder::from_circle(at.x, at.y, r) {
        fill(pm, &c, color);
        stroke(pm, &c, [255, 255, 255, 230], (r * 0.12).max(1.5), BlendMode::SourceOver);
    }
    if let Some(f) = font {
        let label = n.to_string();
        let px = r * 1.15;
        let (tw, th) = f.measure(&label, px);
        f.draw(pm, &label, at.x - tw / 2.0, at.y - th / 2.0, px, style::contrast(color));
    }
}

/// Blur a region in place. The region is shrunk, box-blurred three times
/// (≈ Gaussian) and scaled back up smoothly: cheap enough to preview live at
/// 4K, and text underneath becomes unreadable.
pub fn blur_region(pm: &mut Pixmap, r: IRect, radius: usize) {
    let bounds = IRect::new(0, 0, pm.width() as i32, pm.height() as i32);
    let r = r.intersect(&bounds);
    if r.is_empty() || radius == 0 {
        return;
    }
    let (w, h) = (r.w as usize, r.h as usize);
    let f = (radius / 3).clamp(1, 8);
    let (sw, sh) = (w.div_ceil(f), h.div_ceil(f));
    let stride = pm.width() as usize * 4;
    let data = pm.data_mut();
    let at = |x: usize, y: usize| (r.y as usize + y) * stride + (r.x as usize + x) * 4;

    // Shrink: average each f×f block.
    let mut small = vec![[0u32; 4]; sw * sh];
    for (i, cell) in small.iter_mut().enumerate() {
        let (bx, by) = ((i % sw) * f, (i / sw) * f);
        let (mut sum, mut n) = ([0u32; 4], 0u32);
        for y in by..(by + f).min(h) {
            for x in bx..(bx + f).min(w) {
                let p = &data[at(x, y)..at(x, y) + 4];
                for k in 0..4 {
                    sum[k] += p[k] as u32;
                }
                n += 1;
            }
        }
        *cell = sum.map(|v| (v + n / 2) / n);
    }

    let rs = (radius / f).max(1);
    let mut tmp = vec![[0u32; 4]; sw.max(sh)];
    for _ in 0..3 {
        for y in 0..sh {
            box_line(&mut small[y * sw..(y + 1) * sw], 1, sw, rs, &mut tmp);
        }
        for x in 0..sw {
            box_line(&mut small[x..], sw, sh, rs, &mut tmp);
        }
    }

    // Grow back with bilinear filtering (8-bit fixed-point weights).
    let sample = |pos: usize, max: usize| {
        let c = ((pos as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, (max - 1) as f32);
        let i = (c.floor() as usize).min(max - 1);
        (i, (i + 1).min(max - 1), ((c - i as f32) * 256.0) as u32)
    };
    let xs: Vec<_> = (0..w).map(|x| sample(x, sw)).collect();
    for y in 0..h {
        let (y0, y1, ty) = sample(y, sh);
        let (r0, r1) = (&small[y0 * sw..(y0 + 1) * sw], &small[y1 * sw..(y1 + 1) * sw]);
        let row = at(0, y);
        for (x, &(x0, x1, tx)) in xs.iter().enumerate() {
            let (a, b, c, d) = (r0[x0], r0[x1], r1[x0], r1[x1]);
            let mut px = [0u8; 4];
            for k in 0..4 {
                let top = a[k] * (256 - tx) + b[k] * tx;
                let bot = c[k] * (256 - tx) + d[k] * tx;
                px[k] = ((top * (256 - ty) + bot * ty + (1 << 15)) >> 16).min(255) as u8;
            }
            // Keep the premultiplied invariant: colour never exceeds alpha.
            for k in 0..3 {
                px[k] = px[k].min(px[3]);
            }
            data[row + x * 4..row + x * 4 + 4].copy_from_slice(&px);
        }
    }
}

/// One running-sum box blur over `n` samples spaced `step` apart, edges clamped.
fn box_line(v: &mut [[u32; 4]], step: usize, n: usize, r: usize, tmp: &mut [[u32; 4]]) {
    let at = |v: &[[u32; 4]], i: isize| v[(i.clamp(0, n as isize - 1) as usize) * step];
    let win = (2 * r + 1) as u32;
    let mut sum = [0u32; 4];
    for i in -(r as isize)..=(r as isize) {
        let p = at(v, i);
        for k in 0..4 {
            sum[k] += p[k];
        }
    }
    for (i, t) in tmp.iter_mut().enumerate().take(n) {
        *t = sum.map(|s| (s + win / 2) / win);
        let add = at(v, i as isize + r as isize + 1);
        let sub = at(v, i as isize - r as isize);
        for k in 0..4 {
            sum[k] = sum[k] + add[k] - sub[k];
        }
    }
    for (i, t) in tmp.iter().enumerate().take(n) {
        v[i * step] = *t;
    }
}

/// Snap `b` so the segment a→b lies on a multiple of 45°.
pub fn snap45(a: Pt, b: Pt) -> Pt {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    let ang = (dy.atan2(dx) / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
    Pt::new(a.x + len * ang.cos(), a.y + len * ang.sin())
}

/// Snap `b` so a→b spans a square.
pub fn snap_square(a: Pt, b: Pt) -> Pt {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let s = dx.abs().max(dy.abs());
    Pt::new(a.x + s * dx.signum(), a.y + s * dy.signum())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Pixmap {
        let mut pm = Pixmap::new(w, h).unwrap();
        pm.fill(style::skia(c));
        pm
    }

    fn px(pm: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let p = pm.pixel(x, y).unwrap();
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    #[test]
    fn blur_mixes_an_edge_and_stays_inside_its_region() {
        let mut pm = solid(40, 20, [0, 0, 0, 255]);
        // Right half white.
        for y in 0..20 {
            for x in 20..40 {
                pm.pixels_mut()[(y * 40 + x) as usize] = tiny_skia::PremultipliedColorU8::from_rgba(255, 255, 255, 255).unwrap();
            }
        }
        blur_region(&mut pm, IRect::new(10, 0, 20, 10), 4);
        let edge = px(&pm, 20, 5);
        assert!(edge[0] > 40 && edge[0] < 215, "edge should be grey, got {edge:?}");
        assert_eq!(px(&pm, 20, 15), [255, 255, 255, 255], "outside region untouched");
        assert_eq!(px(&pm, 5, 5), [0, 0, 0, 255]);
    }

    #[test]
    fn blur_of_flat_colour_is_identity() {
        let mut pm = solid(16, 16, [10, 120, 200, 255]);
        blur_region(&mut pm, IRect::new(-5, -5, 100, 100), 6);
        assert_eq!(px(&pm, 8, 8), [10, 120, 200, 255]);
        assert_eq!(px(&pm, 0, 15), [10, 120, 200, 255]);
    }

    #[test]
    fn arrow_paints_tip_and_shaft() {
        let mut pm = solid(100, 40, [255, 255, 255, 255]);
        let a = Annotation { shape: Shape::Arrow(Pt::new(5.0, 20.0), Pt::new(95.0, 20.0)), color: [255, 0, 0, 255], width: 4.0 };
        a.draw(&mut pm, None);
        assert_eq!(px(&pm, 30, 20), [255, 0, 0, 255]);
        assert_eq!(px(&pm, 88, 20), [255, 0, 0, 255]);
        // Head is wider than the shaft.
        assert_eq!(px(&pm, 85, 24)[1], 0);
        assert_eq!(px(&pm, 30, 26), [255, 255, 255, 255]);
    }

    #[test]
    fn marker_keeps_black_black() {
        let mut pm = solid(20, 20, [0, 0, 0, 255]);
        let m = Annotation { shape: Shape::Marker(vec![Pt::new(0.0, 10.0), Pt::new(20.0, 10.0)]), color: [255, 214, 10, 255], width: 8.0 };
        m.draw(&mut pm, None);
        assert_eq!(px(&pm, 10, 10), [0, 0, 0, 255]);
        let mut white = solid(20, 20, [255, 255, 255, 255]);
        m.draw(&mut white, None);
        let p = px(&white, 10, 10);
        assert!(p[2] < 100 && p[0] > 240, "white turns yellow, got {p:?}");
    }

    #[test]
    fn snapping() {
        let s = snap45(Pt::new(0.0, 0.0), Pt::new(10.0, 1.0));
        assert!((s.y).abs() < 1e-3);
        let q = snap_square(Pt::new(0.0, 0.0), Pt::new(-10.0, 4.0));
        assert_eq!(q, Pt::new(-10.0, 10.0));
    }
}

//! Shot's logo: the three-tomoe Sharingan, drawn exactly like the Sharingan
//! Tray Monitor's stage-3 eye (and the 4N style `brand/logo.svg`): a red iris
//! with a black rim, the inner ring, three tomoe and the pupil, on a 128-unit
//! canvas (viewBox 2 2 124 124). The tomoe tail is the traced outline from
//! that artwork, point for point.

use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

/// Iris colour of the tray eye (`$global:eyeColor`, 195,0,0).
pub const IRIS: [u8; 4] = [195, 0, 0, 255];
const INK: [u8; 4] = [0, 0, 0, 255];

/// The traced tail of the top tomoe (head at 64,29), from `brand/logo.svg`.
const TOMOE_TAIL: &str = "M54.7 27.3 54.7 26.8 54.8 26.3 54.9 25.8 55 25.3 55.1 24.8 55.2 24.2 55.4 23.7 55.6 23.2 55.8 22.7 56 22.2 56.3 21.7 56.6 21.2 56.9 20.7 57.3 20.3 57.7 19.8 58.1 19.3 58.5 18.8 58.9 18.3 59.4 17.9 59.9 17.4 60.4 17 61 16.5 61.6 16.2 62.3 15.8 62.9 15.6 63.6 15.3 64.4 15 65.1 14.8 65.9 14.6 66.7 14.6 67.5 14.6 68.3 14.6 69.1 14.5 70 14.6 70.8 14.7 71.7 14.9 72.5 15.1 73.3 15.4 74.2 15.7 75 16.1 75.8 16.6 76.5 17.1 77.4 17.6 78.2 18.1 79 18.7 79.7 19.4 80.5 20.1 81 20.9 81 20.9 79.5 20.6 78.3 20.2 77.2 20 76 19.8 75 19.6 73.9 19.6 72.9 19.6 71.9 19.7 70.9 20 69.9 20.4 69.1 20.7 68.4 21 67.9 20.8 67.5 20.6 67 20.4 66.6 20.2 66.1 20.1 65.7 20 65.2 19.9 64.7 19.9 64.2 19.9 63.8 19.9 63.3 19.9 62.8 19.9 62.3 20 61.9 20.1 61.4 20.2 61 20.4 60.5 20.6 60.1 20.8 59.6 21 59.2 21.2 58.8 21.5 58.4 21.8 58.1 22.1 57.7 22.4 57.4 22.7 57 23.1 56.8 23.4 56.5 23.8 56.2 24.2 56 24.6 55.8 25.1 55.6 25.5 55.4 25.9 55.2 26.4 55.1 26.9 55 27.3Z";

fn paint(c: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

/// The tail outline: an `M x y x y … Z` polygon.
fn tail_path() -> Option<Path> {
    let nums: Vec<f32> = TOMOE_TAIL.split(|c: char| c.is_whitespace() || c == 'M' || c == 'Z').filter_map(|t| t.parse().ok()).collect();
    let mut pb = PathBuilder::new();
    let (pts, _) = nums.as_chunks::<2>();
    let ([x0, y0], rest) = pts.split_first()?;
    pb.move_to(*x0, *y0);
    for [x, y] in rest {
        pb.line_to(*x, *y);
    }
    pb.close();
    pb.finish()
}

/// The logo on a transparent square of `size` pixels.
pub fn app_icon(size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size, size).expect("icon size");
    // Map the 124-unit viewBox (2..126) onto the pixmap.
    let k = size as f32 / 124.0;
    let view = Transform::from_row(k, 0.0, 0.0, k, -2.0 * k, -2.0 * k);
    let fill = |pm: &mut Pixmap, path: &Path, c: [u8; 4], t: Transform| {
        pm.fill_path(path, &paint(c), FillRule::Winding, t, None);
    };
    let ring = |pm: &mut Pixmap, r: f32, w: f32| {
        if let Some(p) = PathBuilder::from_circle(64.0, 64.0, r) {
            pm.stroke_path(&p, &paint(INK), &Stroke { width: w, ..Stroke::default() }, view, None);
        }
    };

    // Iris with its black rim.
    if let Some(p) = PathBuilder::from_circle(64.0, 64.0, 58.0) {
        fill(&mut pm, &p, IRIS, view);
    }
    ring(&mut pm, 58.0, 4.5);
    // Inner ring the tomoe sit on.
    ring(&mut pm, 35.0, 3.2);
    // Three tomoe, 120° apart.
    let head = PathBuilder::from_circle(64.0, 29.0, 9.14);
    let tail = tail_path();
    for i in 0..3 {
        let t = view.pre_concat(Transform::from_rotate_at(120.0 * i as f32, 64.0, 64.0));
        if let Some(h) = &head {
            fill(&mut pm, h, INK, t);
        }
        if let Some(tl) = &tail {
            fill(&mut pm, tl, INK, t);
        }
    }
    // Pupil.
    if let Some(p) = PathBuilder::from_circle(64.0, 64.0, 14.0) {
        fill(&mut pm, &p, INK, view);
    }
    pm
}

/// Straight (non-premultiplied) RGBA bytes, as OS icon APIs expect.
pub fn app_icon_rgba(size: u32) -> Vec<u8> {
    app_icon(size)
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

/// A Windows `.ico` holding the logo at the usual sizes (PNG-compressed
/// entries, supported since Windows Vista).
pub fn app_icon_ico() -> Vec<u8> {
    let sizes = [16u32, 20, 24, 32, 40, 48, 64, 128, 256];
    let pngs: Vec<Vec<u8>> = sizes.iter().map(|&s| app_icon(s).encode_png().expect("png")).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, png) in sizes.iter().zip(&pngs) {
        let dim = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in &pngs {
        out.extend_from_slice(png);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_outline_is_the_traced_one() {
        let b = tail_path().unwrap().bounds();
        // The traced tail spans x 54.7..81, y 14.5..27.3 on the 128 canvas.
        assert!((b.left() - 54.7).abs() < 0.01 && (b.right() - 81.0).abs() < 0.01);
        assert!((b.top() - 14.5).abs() < 0.01 && (b.bottom() - 27.3).abs() < 0.01);
    }

    #[test]
    fn ico_header_lists_every_size() {
        let ico = app_icon_ico();
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 9);
        // First entry: 16×16, its PNG starts where the directory says.
        assert_eq!(ico[6], 16);
        let off = u32::from_le_bytes([ico[18], ico[19], ico[20], ico[21]]) as usize;
        assert_eq!(&ico[off..off + 4], b"\x89PNG");
    }

    #[test]
    fn logo_is_red_with_a_dark_pupil() {
        let pm = app_icon(64);
        let c = pm.pixel(32, 32).unwrap();
        assert_eq!([c.red(), c.green(), c.blue(), c.alpha()], [0, 0, 0, 255], "pupil is black");
        // Between the rim and the ring, off the tomoe: plain iris red.
        let iris = pm.pixel(32, 56).unwrap().demultiply();
        assert_eq!([iris.red(), iris.green(), iris.blue()], [IRIS[0], IRIS[1], IRIS[2]]);
        assert_eq!(pm.pixel(0, 0).unwrap().alpha(), 0, "corners transparent");
    }
}

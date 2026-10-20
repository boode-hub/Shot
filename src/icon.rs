//! Shot's icon, drawn in code: a blue rounded square with a white viewfinder.

use tiny_skia::{LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::geom::IRect;
use crate::style;
use crate::toolbar::round_rect;

pub fn app_icon(size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size, size).expect("icon size");
    let s = size as f32 / 32.0;
    round_rect(&mut pm, IRect::new(0, 0, size as i32, size as i32), 7.0 * s, style::ACCENT);
    let mut pb = PathBuilder::new();
    for (x, y, dx, dy) in [(7.0, 7.0, 1.0, 1.0), (25.0, 7.0, -1.0, 1.0), (7.0, 25.0, 1.0, -1.0), (25.0, 25.0, -1.0, -1.0)] {
        pb.move_to(x * s, (y + 6.0 * dy) * s);
        pb.line_to(x * s, y * s);
        pb.line_to((x + 6.0 * dx) * s, y * s);
    }
    let mut paint = Paint::default();
    paint.set_color(style::skia([255, 255, 255, 255]));
    paint.anti_alias = true;
    if let Some(p) = pb.finish() {
        let stroke = Stroke { width: 2.6 * s, line_cap: LineCap::Round, ..Stroke::default() };
        pm.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
    }
    if let Some(c) = PathBuilder::from_circle(16.0 * s, 16.0 * s, 3.2 * s) {
        pm.fill_path(&c, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
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

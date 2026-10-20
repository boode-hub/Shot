//! Renders the editor in a few states to PNG files, for eyeballing the UI
//! without a display: `cargo run --example preview -- <out-dir>`.

use std::rc::Rc;

use shot::editor::{Button, Editor, Key, Mods};
use shot::geom::{IRect, Pt};
use shot::text::Font;
use tiny_skia::{Color, Paint, Pixmap, Rect, Transform};

fn fake_desktop(w: u32, h: u32, font: Option<&Font>) -> Pixmap {
    let mut pm = Pixmap::new(w, h).unwrap();
    for (i, px) in pm.pixels_mut().iter_mut().enumerate() {
        let (x, y) = ((i as u32 % w) as f32 / w as f32, (i as u32 / w) as f32 / h as f32);
        let c = Color::from_rgba(0.15 + 0.3 * x, 0.25 + 0.2 * y, 0.45 + 0.2 * (1.0 - x), 1.0).unwrap();
        *px = c.premultiply().to_color_u8();
    }
    let mut paint = Paint::default();
    paint.set_color_rgba8(250, 250, 250, 255);
    pm.fill_rect(Rect::from_xywh(160.0, 120.0, 900.0, 560.0).unwrap(), &paint, Transform::identity(), None);
    paint.set_color_rgba8(230, 232, 236, 255);
    pm.fill_rect(Rect::from_xywh(160.0, 120.0, 900.0, 36.0).unwrap(), &paint, Transform::identity(), None);
    if let Some(f) = font {
        let body = "Invoice #4471 — Account: 0042-7781-9921\nCustomer: Jane Example\nTotal due: $1,284.00\n\nThe quick brown fox jumps over the lazy dog.";
        f.draw(&mut pm, "Notes — Untitled", 180.0, 128.0, 16.0, [60, 60, 67, 255]);
        f.draw(&mut pm, body, 190.0, 190.0, 22.0, [28, 28, 30, 255]);
    }
    pm
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let font = Font::load_system().map(Rc::new);
    let (w, h) = (1600u32, 900u32);
    let desk = fake_desktop(w, h, font.as_deref());
    desk.save_png(format!("{out}/0-desktop.png")).unwrap();
    shot::icon::app_icon(64).save_png(format!("{out}/icon.png")).unwrap();
    let windows = vec![IRect::new(160, 120, 900, 560)];
    let mut frame = Pixmap::new(w, h).unwrap();
    let none = Mods::default();

    // 1. Before selecting: hovering a window.
    let mut e = Editor::new(desk.clone(), 1.0, font.clone(), windows.clone());
    e.set_ocr_available(true);
    e.mouse_move(Pt::new(600.0, 400.0));
    e.render(&mut frame);
    frame.save_png(format!("{out}/1-hover.png")).unwrap();

    // 2. Selected and annotated, palette open, hovering a button.
    let drag = |e: &mut Editor, a: (f32, f32), b: (f32, f32)| {
        e.mouse_down(Button::Left, Pt::new(a.0, a.1));
        e.mouse_move(Pt::new(b.0, b.1));
        e.mouse_up(Button::Left, Pt::new(b.0, b.1));
    };
    drag(&mut e, (170.0, 165.0), (1040.0, 600.0));
    e.key(Key::Char('r'), none);
    drag(&mut e, (182.0, 186.0), (690.0, 222.0));
    e.key(Key::Char('a'), none);
    drag(&mut e, (900.0, 420.0), (700.0, 260.0));
    e.key(Key::Char('h'), none);
    drag(&mut e, (186.0, 268.0), (420.0, 268.0));
    e.key(Key::Char('b'), none);
    drag(&mut e, (318.0, 196.0), (560.0, 222.0));
    e.key(Key::Char('n'), none);
    for (x, y) in [(165.0f32, 200.0f32), (165.0, 270.0)] {
        e.mouse_down(Button::Left, Pt::new(x + 10.0, y));
        e.mouse_up(Button::Left, Pt::new(x + 10.0, y));
    }
    e.key(Key::Char('4'), none);
    e.key(Key::Char('e'), none);
    drag(&mut e, (600.0, 330.0), (780.0, 380.0));
    e.key(Key::Char('2'), none);
    e.key(Key::Char('t'), none);
    e.mouse_down(Button::Left, Pt::new(760.0, 460.0));
    e.mouse_up(Button::Left, Pt::new(760.0, 460.0));
    e.text_input("Check this total!");
    e.render(&mut frame);
    frame.save_png(format!("{out}/2-annotated.png")).unwrap();
    e.key(Key::Escape, none);
    e.key(Key::Char('p'), none);
    e.mouse_move(Pt::new(842.0, 627.0));
    e.render(&mut frame);
    frame.save_png(format!("{out}/3-toolbar.png")).unwrap();

    // 4. Exported image (what lands on the clipboard / in the file / in a pin).
    e.export().unwrap().save_png(format!("{out}/4-export.png")).unwrap();

    // 5. HiDPI (150%) with the colour palette open.
    let mut e = Editor::new(desk, 1.5, font, windows);
    drag(&mut e, (300.0, 200.0), (1100.0, 560.0));
    e.key(Key::Char('p'), none);
    e.render(&mut frame);
    frame.save_png(format!("{out}/5-hidpi.png")).unwrap();
    println!("wrote previews to {out}");
}

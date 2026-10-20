//! Frame-time check at 4K: `cargo run --release --example bench`.

use std::rc::Rc;
use std::time::Instant;

use shot::editor::{Button, Editor, Key, Mods};
use shot::geom::Pt;
use shot::text::Font;
use tiny_skia::Pixmap;

fn main() {
    let (w, h) = (3840u32, 2160u32);
    let mut base = Pixmap::new(w, h).unwrap();
    base.fill(tiny_skia::Color::from_rgba8(120, 140, 160, 255));
    let font = Font::load_system().map(Rc::new);
    let t = Instant::now();
    let mut e = Editor::new(base, 2.0, font, vec![]);
    println!("editor setup: {:?}", t.elapsed());
    let mut frame = Pixmap::new(w, h).unwrap();
    let mut buf = vec![0u32; (w * h) as usize];
    let n = 60;

    let time = |label: &str, e: &mut Editor, frame: &mut Pixmap, buf: &mut Vec<u32>, step: &mut dyn FnMut(&mut Editor, usize)| {
        let t = Instant::now();
        for i in 0..n {
            step(e, i);
            e.render(frame);
            for (d, p) in buf.iter_mut().zip(frame.data().as_chunks::<4>().0) {
                *d = (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
            }
        }
        println!("{label}: {:.1} ms/frame", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
    };
    time("idle crosshair", &mut e, &mut frame, &mut buf, &mut |e, i| e.mouse_move(Pt::new(100.0 + i as f32, 100.0)));
    e.mouse_down(Button::Left, Pt::new(200.0, 200.0));
    time("selecting", &mut e, &mut frame, &mut buf, &mut |e, i| e.mouse_move(Pt::new(800.0 + i as f32 * 20.0, 600.0 + i as f32 * 10.0)));
    e.mouse_up(Button::Left, Pt::new(3000.0, 1800.0));
    e.key(Key::Char('p'), Mods::default());
    e.mouse_down(Button::Left, Pt::new(400.0, 400.0));
    time("drawing pen", &mut e, &mut frame, &mut buf, &mut |e, i| {
        e.mouse_move(Pt::new(400.0 + i as f32 * 30.0, 400.0 + (i as f32).sin() * 200.0))
    });
    e.mouse_up(Button::Left, Pt::new(2000.0, 400.0));
    e.key(Key::Char('b'), Mods::default());
    e.mouse_down(Button::Left, Pt::new(400.0, 400.0));
    time("dragging blur (live)", &mut e, &mut frame, &mut buf, &mut |e, i| {
        e.mouse_move(Pt::new(800.0 + i as f32 * 20.0, 700.0 + i as f32 * 10.0))
    });
}

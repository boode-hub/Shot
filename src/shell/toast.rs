//! Short status messages ("Copied to clipboard") near the bottom of the screen.

use std::rc::Rc;
use std::time::Instant;

use shot::geom::IRect;
use shot::style;
use shot::text::Font;
use shot::toolbar::panel;
use tiny_skia::Pixmap;
use winit::window::Window;

use super::present::{self, Surface};

pub struct Toast {
    pub window: Rc<Window>,
    surface: Surface,
    image: Pixmap,
    pub until: Instant,
}

/// The toast's picture: a dark pill with the message in it.
pub fn picture(font: Option<&Font>, text: &str, scale: f32) -> Pixmap {
    let px = 15.0 * scale;
    let (tw, th) = font.map(|f| f.measure(text, px)).unwrap_or((text.len() as f32 * px * 0.5, px));
    let (padx, pady) = (16.0 * scale, 10.0 * scale);
    let (w, h) = ((tw + padx * 2.0).ceil() as u32, (th + pady * 2.0).ceil() as u32);
    let mut pm = Pixmap::new(w.max(1), h.max(1)).expect("toast size");
    // Opaque windows: fill with the panel colour so the rounded corners blend in.
    pm.fill(style::skia([0x1F, 0x1F, 0x23, 0xFF]));
    panel(&mut pm, IRect::new(0, 0, w as i32, h as i32), 0.0);
    if let Some(f) = font {
        f.draw(&mut pm, text, padx, pady, px, style::ICON);
    }
    pm
}

impl Toast {
    pub fn new(window: Rc<Window>, surface: Surface, image: Pixmap, until: Instant) -> Toast {
        Toast { window, surface, image, until }
    }

    pub fn render(&mut self) {
        if let Err(e) = present::present(&mut self.surface, &self.window, &self.image) {
            eprintln!("shot: toast present failed: {e}");
        }
    }
}

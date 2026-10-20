//! A pinned screenshot: a small always-on-top window you can move, zoom,
//! copy from and read while working in another app.

use std::rc::Rc;

use shot::style;
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::present::{self, Surface};

pub struct Pin {
    pub window: Rc<Window>,
    surface: Surface,
    pub image: Pixmap,
    zoom: f32,
    frame: Option<Pixmap>,
    pub focused: bool,
}

impl Pin {
    pub fn new(window: Rc<Window>, surface: Surface, image: Pixmap) -> Pin {
        Pin { window, surface, image, zoom: 1.0, frame: None, focused: true }
    }

    pub fn zoom_by(&mut self, steps: f32) {
        self.set_zoom(self.zoom * 1.1f32.powf(steps));
    }

    pub fn set_zoom(&mut self, z: f32) {
        // Keep the window between ~40 px and 4x the image.
        let min = (40.0 / self.image.width().min(self.image.height()) as f32).min(1.0);
        self.zoom = z.clamp(min, 4.0);
        let (w, h) = ((self.image.width() as f32 * self.zoom).round().max(1.0), (self.image.height() as f32 * self.zoom).round().max(1.0));
        let _ = self.window.request_inner_size(PhysicalSize::new(w as u32, h as u32));
        self.window.request_redraw();
    }

    pub fn render(&mut self) {
        let size = self.window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        if self.frame.as_ref().is_none_or(|f| f.width() != w || f.height() != h) {
            self.frame = Pixmap::new(w, h);
        }
        let Some(frame) = self.frame.as_mut() else { return };
        frame.fill(tiny_skia::Color::BLACK);
        let (kx, ky) = (w as f32 / self.image.width() as f32, h as f32 / self.image.height() as f32);
        let quality = if (kx - 1.0).abs() < 1e-3 && (ky - 1.0).abs() < 1e-3 { FilterQuality::Nearest } else { FilterQuality::Bicubic };
        let paint = PixmapPaint { quality, ..PixmapPaint::default() };
        frame.draw_pixmap(0, 0, self.image.as_ref(), &paint, Transform::from_scale(kx, ky), None);
        // A thin border tells the pin apart from what is underneath.
        let border = if self.focused { style::ACCENT } else { [128, 128, 128, 255] };
        if let Some(r) = tiny_skia::Rect::from_xywh(0.5, 0.5, w as f32 - 1.0, h as f32 - 1.0) {
            let mut p = tiny_skia::Paint::default();
            p.set_color(style::skia(border));
            frame.stroke_path(
                &tiny_skia::PathBuilder::from_rect(r),
                &p,
                &tiny_skia::Stroke { width: 1.0, ..Default::default() },
                Transform::identity(),
                None,
            );
        }
        if let Err(e) = present::present(&mut self.surface, &self.window, frame) {
            eprintln!("shot: pin present failed: {e}");
        }
    }
}

//! Copy a pixmap to a window through softbuffer (CPU only, no GPU needed).

use std::num::NonZeroU32;
use std::rc::Rc;

use tiny_skia::Pixmap;
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

pub type Surface = softbuffer::Surface<OwnedDisplayHandle, Rc<Window>>;
pub type Context = softbuffer::Context<OwnedDisplayHandle>;

pub fn surface(ctx: &Context, window: &Rc<Window>) -> Result<Surface, String> {
    softbuffer::Surface::new(ctx, window.clone()).map_err(|e| e.to_string())
}

/// Show `pm` at the window's top-left; anything outside it is black.
pub fn present(surface: &mut Surface, window: &Window, pm: &Pixmap) -> Result<(), String> {
    let size = window.inner_size();
    let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return Ok(()) };
    surface.resize(w, h).map_err(|e| e.to_string())?;
    let mut buf = surface.buffer_mut().map_err(|e| e.to_string())?;
    let (bw, bh) = (size.width as usize, size.height as usize);
    let (pw, ph) = (pm.width() as usize, pm.height() as usize);
    let src = pm.data();
    let cw = bw.min(pw);
    for y in 0..bh {
        let row = &mut buf[y * bw..(y + 1) * bw];
        if y >= ph {
            row.fill(0);
            continue;
        }
        let s = &src[y * pw * 4..y * pw * 4 + cw * 4];
        for (d, p) in row[..cw].iter_mut().zip(s.as_chunks::<4>().0) {
            *d = (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
        }
        row[cw..].fill(0);
    }
    buf.present().map_err(|e| e.to_string())
}

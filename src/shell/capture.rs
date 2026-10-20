//! Grab every monitor (and the window layout, for click-to-pick) in one go.

use shot::editor::pixmap_from_rgba;
use shot::geom::IRect;
use tiny_skia::Pixmap;

pub struct Grab {
    /// Monitor origin on the virtual desktop, physical pixels.
    pub x: i32,
    pub y: i32,
    pub scale: f32,
    pub image: Pixmap,
    /// Top-level windows on this monitor, monitor-local, topmost first.
    pub windows: Vec<IRect>,
}

pub fn grab_all() -> Result<Vec<Grab>, String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("cannot list monitors: {e}"))?;
    // Pixels first, so the screenshot is as close as possible to the key press.
    let mut grabs = Vec::new();
    for m in &monitors {
        let (Ok(x), Ok(y)) = (m.x(), m.y()) else { continue };
        let img = match m.capture_image() {
            Ok(i) => i,
            Err(e) => {
                eprintln!("shot: capture failed on monitor at {x},{y}: {e}");
                continue;
            }
        };
        let (w, h) = (img.width(), img.height());
        let Some(image) = pixmap_from_rgba(img.into_raw(), w, h) else { continue };
        let scale = m.scale_factor().unwrap_or(1.0);
        grabs.push(Grab { x, y, scale, image, windows: Vec::new() });
    }
    if grabs.is_empty() {
        return Err("no monitor could be captured".into());
    }
    let wins = window_rects();
    for g in &mut grabs {
        let mon = IRect::new(g.x, g.y, g.image.width() as i32, g.image.height() as i32);
        g.windows = wins.iter().filter(|w| !w.intersect(&mon).is_empty()).map(|w| IRect::new(w.x - g.x, w.y - g.y, w.w, w.h)).collect();
    }
    Ok(grabs)
}

/// Visible top-level windows in global coordinates, topmost first. Runs on
/// the calling (UI) thread: xcap may query our own windows' titles, which
/// would deadlock if the UI thread were blocked waiting on a helper thread.
fn window_rects() -> Vec<IRect> {
    let Ok(all) = xcap::Window::all() else { return Vec::new() };
    let mut v: Vec<(i32, IRect)> = all
        .iter()
        .enumerate()
        .filter(|(_, w)| !w.is_minimized().unwrap_or(false))
        .filter_map(|(i, w)| {
            let r = IRect::new(w.x().ok()?, w.y().ok()?, w.width().ok()? as i32, w.height().ok()? as i32);
            // Windows and macOS list windows front to back already; X11 needs z().
            let z = if cfg!(any(windows, target_os = "macos")) { -(i as i32) } else { w.z().unwrap_or(0) };
            (!r.is_empty()).then_some((z, r))
        })
        .collect();
    v.sort_by_key(|(z, _)| std::cmp::Reverse(*z));
    v.into_iter().map(|(_, r)| r).collect()
}

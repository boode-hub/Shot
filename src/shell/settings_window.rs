//! The Settings window: an ordinary, decorated window showing [`shot::settings::View`].

use std::rc::Rc;

use shot::geom::Pt;
use shot::settings::View;
use tiny_skia::Pixmap;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Icon, Window};

use super::present::{self, Context, Surface};

pub struct SettingsWindow {
    pub window: Rc<Window>,
    surface: Surface,
    pub view: View,
    frame: Option<Pixmap>,
    pub mods: ModifiersState,
    /// Last cursor position, physical pixels.
    pub pos: Pt,
}

pub fn window_icon() -> Option<Icon> {
    Icon::from_rgba(shot::icon::app_icon_rgba(64), 64, 64).ok()
}

impl SettingsWindow {
    pub fn open(el: &ActiveEventLoop, ctx: &Context, mut view: View) -> Result<SettingsWindow, String> {
        let mut attrs = Window::default_attributes()
            .with_title("Shot Settings")
            .with_inner_size(LogicalSize::new(shot::settings::WIDTH as f64, shot::settings::HEIGHT as f64))
            .with_resizable(false)
            .with_window_icon(window_icon())
            .with_visible(false);
        // Centre on the primary screen.
        if let Some(m) = el.primary_monitor() {
            let sf = m.scale_factor();
            let (w, h) = ((shot::settings::WIDTH as f64 * sf) as i32, (shot::settings::HEIGHT as f64 * sf) as i32);
            let (p, s) = (m.position(), m.size());
            attrs = attrs.with_position(PhysicalPosition::new(p.x + (s.width as i32 - w) / 2, p.y + (s.height as i32 - h) / 2));
        }
        let window = Rc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
        view.set_scale(window.scale_factor() as f32);
        let surface = present::surface(ctx, &window)?;
        let sw = SettingsWindow { window, surface, view, frame: None, mods: ModifiersState::empty(), pos: Pt::default() };
        sw.window.set_visible(true);
        // Paint once the window is mapped; drawing into a hidden window is lost on some systems.
        sw.window.request_redraw();
        sw.window.focus_window();
        Ok(sw)
    }

    pub fn render(&mut self) {
        let size = self.window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        if self.frame.as_ref().is_none_or(|f| f.width() != w || f.height() != h) {
            self.frame = Pixmap::new(w, h);
        }
        let Some(frame) = self.frame.as_mut() else { return };
        self.view.render(frame);
        if let Err(e) = present::present(&mut self.surface, &self.window, frame) {
            eprintln!("shot: settings present failed: {e}");
        }
    }
}

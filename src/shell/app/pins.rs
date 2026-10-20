//! Pinned screenshots: creating them, their input, and their right-click menu.

use std::rc::Rc;

use shot::editor::{Key, Mods};
use shot::geom::Pt;
use tiny_skia::Pixmap;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;

use super::super::pin::Pin;
use super::super::present;
use super::{App, Clicks, map_key, mods, wheel_lines};
use shot::config::SaveMode;

pub(super) struct PinSlot {
    pub(super) pin: Pin,
    clicks: Clicks,
    mods: Mods,
}

impl App {
    pub(super) fn make_pin(&mut self, el: &ActiveEventLoop, img: Pixmap, at: PhysicalPosition<i32>) {
        let attrs = Self::popup("Shot pin").with_position(at).with_inner_size(PhysicalSize::new(img.width(), img.height()));
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => return self.show_toast(el, &format!("Cannot pin: {e}"), 4.0),
        };
        let surface = match present::surface(&self.ctx, &window) {
            Ok(s) => s,
            Err(e) => return self.show_toast(el, &format!("Cannot pin: {e}"), 4.0),
        };
        let mut pin = Pin::new(window, surface, img);
        pin.render();
        pin.window.request_redraw();
        pin.window.focus_window();
        self.pins.push(PinSlot { pin, clicks: Clicks::default(), mods: Mods::default() });
    }

    pub(super) fn pin_event(&mut self, el: &ActiveEventLoop, i: usize, event: WindowEvent) {
        let slot = &mut self.pins[i];
        match event {
            WindowEvent::CloseRequested => {
                self.pins.remove(i);
            }
            WindowEvent::RedrawRequested => slot.pin.render(),
            WindowEvent::Resized(_) => slot.pin.window.request_redraw(),
            WindowEvent::Focused(f) => {
                slot.pin.focused = f;
                slot.pin.window.request_redraw();
            }
            WindowEvent::ModifiersChanged(m) => slot.mods = mods(m.state()),
            WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => match button {
                MouseButton::Left => {
                    // Double-click closes; otherwise drag the pin around.
                    slot.pin.window.focus_window();
                    if slot.clicks.press(Pt::default()) {
                        self.pins.remove(i);
                    } else {
                        let _ = slot.pin.window.drag_window();
                    }
                }
                MouseButton::Middle => {
                    self.pins.remove(i);
                }
                MouseButton::Right => {
                    self.menu_target = Some(slot.pin.window.id());
                    #[cfg(any(windows, target_os = "macos"))]
                    if let Some(menu) = &self.pin_menu {
                        menu.show(&slot.pin.window);
                    }
                }
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => slot.pin.zoom_by(wheel_lines(delta)),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let ctrl = slot.mods.ctrl;
                match map_key(&event) {
                    Some(Key::Escape) => {
                        self.pins.remove(i);
                    }
                    Some(Key::Char('c')) if ctrl => self.pin_action(el, i, PinAction::Copy),
                    Some(Key::Char('s')) if ctrl => self.pin_action(el, i, PinAction::Save),
                    Some(Key::Char('t')) if ctrl => self.pin_action(el, i, PinAction::Ocr),
                    Some(Key::Char('0')) => slot.pin.set_zoom(1.0),
                    Some(Key::Char('=' | '+')) => slot.pin.zoom_by(1.0),
                    Some(Key::Char('-')) => slot.pin.zoom_by(-1.0),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub(super) fn pin_action(&mut self, el: &ActiveEventLoop, i: usize, a: PinAction) {
        let Some(slot) = self.pins.get_mut(i) else { return };
        match a {
            PinAction::Copy => {
                let img = slot.pin.image.clone();
                self.copy(el, &img);
            }
            PinAction::Save => {
                let img = slot.pin.image.clone();
                let ask = self.cfg.save_mode == SaveMode::Ask;
                self.save(el, &img, ask);
            }
            PinAction::Ocr => {
                let img = slot.pin.image.clone();
                self.read_text(el, img);
            }
            PinAction::ActualSize => slot.pin.set_zoom(1.0),
            PinAction::Close => {
                self.pins.remove(i);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub enum PinAction {
    Copy,
    Save,
    Ocr,
    ActualSize,
    Close,
}

/// Right-click menu on pins (Windows and macOS; Linux uses the shortcuts).
#[cfg(any(windows, target_os = "macos"))]
pub(super) mod pin_menu {
    use tray_icon::menu::{ContextMenu, Menu, MenuId, MenuItem, PredefinedMenuItem};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    use super::PinAction;

    pub struct PinMenu {
        menu: Menu,
        items: Vec<(MenuId, PinAction)>,
        pub close_all: MenuId,
    }

    impl PinMenu {
        pub fn new() -> Result<PinMenu, String> {
            let entries = [
                ("Copy\tCtrl+C", PinAction::Copy),
                ("Save…\tCtrl+S", PinAction::Save),
                ("Copy text (OCR)\tCtrl+T", PinAction::Ocr),
                ("Actual size\t0", PinAction::ActualSize),
                ("Close\tEsc", PinAction::Close),
            ];
            let menu = Menu::new();
            let mut items = Vec::new();
            for (i, (label, action)) in entries.iter().enumerate() {
                if i == 4 {
                    menu.append(&PredefinedMenuItem::separator()).map_err(|e| e.to_string())?;
                }
                let item = MenuItem::new(*label, true, None);
                menu.append(&item).map_err(|e| e.to_string())?;
                items.push((item.id().clone(), *action));
            }
            let all = MenuItem::new("Close all pins", true, None);
            menu.append(&all).map_err(|e| e.to_string())?;
            let close_all = all.id().clone();
            items.push((close_all.clone(), PinAction::Close));
            Ok(PinMenu { menu, items, close_all })
        }

        pub fn action(&self, id: &MenuId) -> Option<PinAction> {
            self.items.iter().find(|(i, _)| i == id).map(|(_, a)| *a)
        }

        pub fn show(&self, window: &Window) {
            let Ok(handle) = window.window_handle() else { return };
            match handle.as_raw() {
                #[cfg(windows)]
                RawWindowHandle::Win32(h) => unsafe {
                    self.menu.show_context_menu_for_hwnd(h.hwnd.get(), None);
                },
                #[cfg(target_os = "macos")]
                RawWindowHandle::AppKit(h) => unsafe {
                    self.menu.show_context_menu_for_nsview(h.ns_view.as_ptr(), None);
                },
                _ => {}
            }
        }
    }
}

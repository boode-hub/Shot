//! The capture shortcut and the Settings window.

use global_hotkey::hotkey::HotKey;
use shot::geom::Pt;
use shot::hotkey;
use shot::settings::{Action as SetAction, Note, State as SetState, View as SetView};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::PhysicalKey;
use winit::window::CursorIcon;

use super::super::settings_window::SettingsWindow;
use super::super::{autostart, output};
use super::App;

impl App {
    // ----- capture shortcut -------------------------------------------------

    pub(super) fn active_hotkey_name(&self) -> Option<&str> {
        self.active_hotkey.as_ref().map(|(n, _)| n.as_str())
    }

    /// Switch the global shortcut to `name`. On failure the previous one stays.
    pub(super) fn register_hotkey(&mut self, name: &str) -> Result<(), String> {
        if self.active_hotkey_name() == Some(name) {
            return Ok(());
        }
        let hk: HotKey = name.parse().map_err(|_| format!("{} can't be used as a shortcut.", hotkey::pretty(name)))?;
        let m = self.hotkeys.as_ref().ok_or("Global shortcuts are not available on this system.")?;
        m.register(hk).map_err(|_| format!("{} is used by another app. Try another shortcut.", hotkey::pretty(name)))?;
        if let Some((_, old)) = self.active_hotkey.replace((name.to_string(), hk)) {
            let _ = m.unregister(old);
        }
        Ok(())
    }

    pub(super) fn start_recording(&mut self) {
        // Free the current shortcut so pressing it reaches the Settings window.
        if let Some((name, hk)) = self.active_hotkey.take() {
            if let Some(m) = &self.hotkeys {
                let _ = m.unregister(hk);
            }
            self.paused_hotkey = Some(name);
        }
        self.recording = true;
        self.hotkey_error = None;
    }

    /// End recording with the new shortcut, or `None` to keep the old one.
    pub(super) fn finish_recording(&mut self, new: Option<Result<String, String>>) {
        if !self.recording {
            return;
        }
        self.recording = false;
        let old = self.paused_hotkey.take();
        let restore = |app: &mut App| {
            if let Some(o) = &old
                && let Err(e) = app.register_hotkey(o)
            {
                app.hotkey_error = Some(e);
            }
        };
        match new {
            Some(Ok(name)) => match self.register_hotkey(&name) {
                Ok(()) => {
                    self.hotkey_error = None;
                    self.cfg.hotkey = name;
                    self.save_config();
                }
                Err(e) => {
                    restore(self);
                    self.hotkey_error = Some(e);
                }
            },
            Some(Err(why)) => {
                restore(self);
                self.hotkey_error = Some(why);
            }
            None => restore(self),
        }
        if let Some(t) = &self.tray {
            t.set_hotkey(self.active_hotkey_name());
        }
    }

    // ----- settings window -------------------------------------------------

    pub(super) fn settings_state(&self) -> SetState {
        let note = if self.recording {
            Note::Recording
        } else {
            match &self.hotkey_error {
                Some(e) => Note::Error(e.clone()),
                None if self.active_hotkey.is_some() => Note::Active,
                None => Note::Error("No capture shortcut is set.".into()),
            }
        };
        SetState {
            hotkey: self.active_hotkey_name().unwrap_or(&self.cfg.hotkey).to_string(),
            note,
            save_dir: self.cfg.save_dir.display().to_string(),
            save_mode: self.cfg.save_mode,
            copy_on_save: self.cfg.copy_on_save,
            autostart: autostart::is_enabled(),
            install_path: autostart::install_path().display().to_string(),
            portable: shot::config::portable_dir().is_some(),
        }
    }

    pub(super) fn open_settings(&mut self, el: &ActiveEventLoop) {
        if let Some(s) = &self.settings {
            s.window.focus_window();
            return;
        }
        let view = SetView::new(self.settings_state(), 1.0, self.font.clone(), self.bold.clone());
        match SettingsWindow::open(el, &self.ctx, view) {
            Ok(w) => self.settings = Some(w),
            Err(e) => self.show_toast(el, &format!("Cannot open Settings: {e}"), 4.0),
        }
    }

    pub(super) fn refresh_settings(&mut self) {
        let st = self.settings_state();
        if let Some(s) = &mut self.settings {
            s.view.state = st;
            s.window.request_redraw();
        }
    }

    pub(super) fn settings_event(&mut self, el: &ActiveEventLoop, event: WindowEvent) {
        let Some(s) = self.settings.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => {
                self.finish_recording(None);
                self.settings = None;
            }
            WindowEvent::RedrawRequested => s.render(),
            WindowEvent::Resized(_) => s.window.request_redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                s.view.set_scale(scale_factor as f32);
                s.window.request_redraw();
            }
            WindowEvent::ModifiersChanged(m) => s.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                s.pos = Pt::new(position.x as f32, position.y as f32);
                if s.view.mouse_move(s.pos) {
                    s.window.set_cursor(if s.view.over_control() { CursorIcon::Pointer } else { CursorIcon::Default });
                    s.window.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                s.view.mouse_leave();
                s.window.request_redraw();
            }
            WindowEvent::Focused(false) if self.recording => {
                self.finish_recording(None);
                self.refresh_settings();
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                if let Some(a) = s.view.click(s.pos) {
                    self.settings_action(el, a);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                let name = format!("{code:?}");
                // Windows reports Print Screen only on release.
                let pressed = event.state == ElementState::Pressed || name == "PrintScreen";
                if !pressed || event.repeat {
                    return;
                }
                let m = s.mods;
                let none = !(m.shift_key() || m.control_key() || m.alt_key() || m.super_key());
                if self.recording {
                    if name == "Escape" && none {
                        self.finish_recording(None);
                    } else if !hotkey::is_modifier(&name) {
                        let mods = hotkey::Modifiers { ctrl: m.control_key(), alt: m.alt_key(), shift: m.shift_key(), meta: m.super_key() };
                        self.finish_recording(Some(hotkey::from_press(mods, &name)));
                    }
                    self.refresh_settings();
                } else if name == "Escape" && none {
                    self.settings = None;
                }
            }
            _ => {}
        }
    }

    pub(super) fn settings_action(&mut self, el: &ActiveEventLoop, a: SetAction) {
        match a {
            SetAction::StartRecording => self.start_recording(),
            SetAction::CancelRecording => self.finish_recording(None),
            SetAction::PickFolder => {
                if let Some(d) = output::pick_folder(&self.cfg.save_dir) {
                    self.cfg.save_dir = d;
                    self.save_config();
                }
            }
            SetAction::SetSaveMode(m) => {
                self.cfg.save_mode = m;
                self.save_config();
            }
            SetAction::SetCopyOnSave(on) => {
                self.cfg.copy_on_save = on;
                self.save_config();
            }
            SetAction::SetAutostart(on) => {
                if let Err(e) = autostart::set(on) {
                    self.show_toast(el, &format!("Start-up setting failed: {e}"), 5.0);
                }
            }
            SetAction::OpenFolder => {
                let _ = std::fs::create_dir_all(&self.cfg.save_dir);
                output::open(&self.cfg.save_dir);
            }
            SetAction::Quit => el.exit(),
        }
        self.refresh_settings();
    }
}

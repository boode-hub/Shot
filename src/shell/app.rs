//! The event loop: hotkey and tray in, overlays / pins / toasts out.

use std::rc::Rc;
use std::time::{Duration, Instant};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use shot::config::{self, Config, SaveMode};
use shot::editor::{self, Command, CursorKind, Editor, Key, Mods};
use shot::geom::Pt;
use shot::text::Font;
use tiny_skia::Pixmap;
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton as TrayButton, MouseButtonState, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Fullscreen, Window, WindowAttributes, WindowId, WindowLevel};

use super::pin::Pin;
use super::present::{self, Context, Surface};
use super::toast::{self, Toast};
use super::{capture, ocr, output, tray};

pub enum UserEvent {
    Hotkey,
    Tray(TrayIconEvent),
    Menu(MenuEvent),
    OcrDone(Result<String, String>),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Capture as soon as Shot starts.
    pub capture_now: bool,
    /// No tray or hotkey; quit once the capture (and any pins) are closed.
    pub once: bool,
}

const FALLBACK_HOTKEY: &str = "Ctrl+PrintScreen";

pub fn run(opts: Options) -> Result<(), String> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().map_err(|e| e.to_string())?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();
    let p = proxy.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.state() == HotKeyState::Pressed {
            let _ = p.send_event(UserEvent::Hotkey);
        }
    }));
    let p = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |e| {
        let _ = p.send_event(UserEvent::Tray(e));
    }));
    let p = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = p.send_event(UserEvent::Menu(e));
    }));
    let ctx = Context::new(event_loop.owned_display_handle()).map_err(|e| e.to_string())?;
    let mut app = App::new(proxy, opts, ctx);
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

/// Detects a second press close in time and space to the first.
#[derive(Default)]
struct Clicks {
    last: Option<(Instant, Pt)>,
}

impl Clicks {
    fn press(&mut self, p: Pt) -> bool {
        let now = Instant::now();
        let double = self.last.is_some_and(|(t, q)| now - t < Duration::from_millis(450) && q.dist(p) < 6.0);
        self.last = if double { None } else { Some((now, p)) };
        double
    }
}

struct Overlay {
    window: Rc<Window>,
    surface: Surface,
    editor: Editor,
    frame: Pixmap,
    origin: PhysicalPosition<i32>,
    pos: Pt,
    clicks: Clicks,
    cursor: CursorKind,
}

struct PinSlot {
    pin: Pin,
    clicks: Clicks,
    mods: Mods,
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    opts: Options,
    ctx: Context,
    cfg: Config,
    cfg_path: std::path::PathBuf,
    font: Option<Rc<Font>>,
    clipboard: Option<arboard::Clipboard>,
    hotkeys: Option<GlobalHotKeyManager>,
    tray: Option<tray::Tray>,
    #[cfg(any(windows, target_os = "macos"))]
    pin_menu: Option<pin_menu::PinMenu>,
    menu_target: Option<WindowId>,
    overlays: Vec<Overlay>,
    pins: Vec<PinSlot>,
    toast: Option<Toast>,
    ocr_ok: bool,
    ocr_pending: usize,
    captured: bool,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>, opts: Options, ctx: Context) -> App {
        let cfg_path = config::app_dir().join("settings.txt");
        App {
            proxy,
            opts,
            ctx,
            cfg: Config::load(&cfg_path),
            cfg_path,
            font: Font::load_system().map(Rc::new),
            clipboard: arboard::Clipboard::new().ok(),
            hotkeys: None,
            tray: None,
            #[cfg(any(windows, target_os = "macos"))]
            pin_menu: None,
            menu_target: None,
            overlays: Vec::new(),
            pins: Vec::new(),
            toast: None,
            ocr_ok: ocr::available(),
            ocr_pending: 0,
            captured: false,
        }
    }

    fn setup(&mut self, el: &ActiveEventLoop) {
        #[cfg(any(windows, target_os = "macos"))]
        {
            self.pin_menu = pin_menu::PinMenu::new().ok();
        }
        if !self.opts.once {
            let mut active = None;
            match GlobalHotKeyManager::new() {
                Ok(m) => {
                    for name in [self.cfg.hotkey.as_str(), FALLBACK_HOTKEY] {
                        match name.parse::<HotKey>() {
                            Ok(hk) if m.register(hk).is_ok() => {
                                active = Some(name.to_string());
                                break;
                            }
                            Ok(_) => eprintln!("shot: hotkey {name} is taken by another app"),
                            Err(e) => eprintln!("shot: bad hotkey {name:?}: {e}"),
                        }
                    }
                    self.hotkeys = Some(m);
                }
                Err(e) => eprintln!("shot: global hotkeys unavailable: {e}"),
            }
            match tray::build(active.as_deref()) {
                Ok(t) => self.tray = Some(t),
                Err(e) => eprintln!("shot: no tray icon: {e}"),
            }
            match &active {
                Some(k) if *k != self.cfg.hotkey => {
                    self.show_toast(el, &format!("{} is in use by another app, so Shot uses {k}", self.cfg.hotkey), 5.0)
                }
                Some(k) if !self.opts.capture_now => self.show_toast(el, &format!("Shot is running. Press {k} to capture."), 2.5),
                None => self.show_toast(el, "No hotkey available. Use the tray icon to capture.", 5.0),
                _ => {}
            }
        }
        if self.opts.capture_now {
            self.start_capture(el);
        }
    }

    // ----- windows ---------------------------------------------------------

    fn popup(title: &str) -> WindowAttributes {
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop);
        #[cfg(windows)]
        let attrs = {
            use winit::platform::windows::WindowAttributesExtWindows;
            attrs.with_skip_taskbar(true)
        };
        attrs
    }

    fn start_capture(&mut self, el: &ActiveEventLoop) {
        if !self.overlays.is_empty() {
            return;
        }
        self.captured = true;
        // Pins and toasts must not end up in the screenshot; give the
        // compositor a moment to take them off screen.
        let hid = self.toast.take().is_some() || !self.pins.is_empty();
        for p in &self.pins {
            p.pin.window.set_visible(false);
        }
        if hid {
            std::thread::sleep(Duration::from_millis(80));
        }
        let grabs = capture::grab_all();
        for p in &self.pins {
            p.pin.window.set_visible(true);
        }
        let grabs = match grabs {
            Ok(g) => g,
            Err(e) => return self.show_toast(el, &format!("Capture failed: {e}"), 4.0),
        };
        let monitors: Vec<_> = el.available_monitors().collect();
        for g in grabs {
            let pos = PhysicalPosition::new(g.x, g.y);
            let monitor = monitors.iter().find(|m| m.position() == pos).cloned();
            let scale = monitor.as_ref().map(|m| m.scale_factor() as f32).unwrap_or(g.scale);
            let (w, h) = (g.image.width(), g.image.height());
            let mut attrs = Self::popup("Shot").with_position(pos).with_inner_size(PhysicalSize::new(w, h)).with_visible(false);
            // Borderless fullscreen covers taskbars and panels; on macOS it would
            // open a new Space with an animation, so a plain top-level window is used.
            if !cfg!(target_os = "macos") {
                attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(monitor)));
            }
            let window = match el.create_window(attrs) {
                Ok(w) => Rc::new(w),
                Err(e) => {
                    eprintln!("shot: cannot open overlay: {e}");
                    continue;
                }
            };
            let surface = match present::surface(&self.ctx, &window) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("shot: cannot draw to overlay: {e}");
                    continue;
                }
            };
            let Some(frame) = Pixmap::new(w, h) else { continue };
            let mut editor = Editor::new(g.image, scale, self.font.clone(), g.windows);
            editor.set_style(self.cfg.style);
            editor.set_ocr_available(self.ocr_ok);
            window.set_cursor(CursorIcon::Crosshair);
            self.overlays.push(Overlay {
                window,
                surface,
                editor,
                frame,
                origin: pos,
                pos: Pt::default(),
                clicks: Clicks::default(),
                cursor: CursorKind::Crosshair,
            });
        }
        if self.overlays.is_empty() {
            return self.show_toast(el, "Capture failed: could not open a window", 4.0);
        }
        for o in &mut self.overlays {
            o.editor.render(&mut o.frame);
            let _ = present::present(&mut o.surface, &o.window, &o.frame);
            o.window.set_visible(true);
            o.window.request_redraw();
        }
        // Keyboard focus to the primary monitor's overlay (Esc must work at once).
        let primary = el.primary_monitor().map(|m| m.position());
        let i = self.overlays.iter().position(|o| Some(o.origin) == primary).unwrap_or(0);
        self.overlays[i].window.focus_window();
    }

    fn close_overlays(&mut self) {
        self.overlays.clear();
    }

    fn show_toast(&mut self, el: &ActiveEventLoop, text: &str, secs: f32) {
        self.toast = None;
        let Some(m) = el.primary_monitor().or_else(|| el.available_monitors().next()) else { return };
        let scale = m.scale_factor() as f32;
        let image = toast::picture(self.font.as_deref(), text, scale);
        let (mp, ms) = (m.position(), m.size());
        let pos = PhysicalPosition::new(
            mp.x + (ms.width as i32 - image.width() as i32) / 2,
            mp.y + ms.height as i32 - image.height() as i32 - (90.0 * scale) as i32,
        );
        let attrs =
            Self::popup("Shot").with_position(pos).with_inner_size(PhysicalSize::new(image.width(), image.height())).with_active(false);
        let Ok(window) = el.create_window(attrs) else { return };
        let window = Rc::new(window);
        let Ok(surface) = present::surface(&self.ctx, &window) else { return };
        let mut t = Toast::new(window, surface, image, Instant::now() + Duration::from_secs_f32(secs));
        t.render();
        t.window.request_redraw();
        self.toast = Some(t);
    }

    fn make_pin(&mut self, el: &ActiveEventLoop, img: Pixmap, at: PhysicalPosition<i32>) {
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
        pin.window.focus_window();
        self.pins.push(PinSlot { pin, clicks: Clicks::default(), mods: Mods::default() });
    }

    // ----- what to do with a finished capture --------------------------------

    fn finish(&mut self, el: &ActiveEventLoop, i: usize, cmd: Command) {
        let o = &mut self.overlays[i];
        let img = if cmd == Command::Cancel { None } else { o.editor.export() };
        let at = o.editor.selection().map(|s| PhysicalPosition::new(o.origin.x + s.x, o.origin.y + s.y));
        let style = o.editor.style();
        self.close_overlays();
        if style != self.cfg.style {
            self.cfg.style = style;
            let _ = self.cfg.save(&self.cfg_path);
        }
        let Some(img) = img else { return };
        match cmd {
            Command::Copy => self.copy(el, &img),
            Command::Save if self.cfg.save_mode == SaveMode::Ask => self.save(el, &img, true),
            Command::Save | Command::QuickSave => self.save(el, &img, false),
            Command::Pin => self.make_pin(el, img, at.unwrap_or_default()),
            Command::Ocr => self.read_text(el, img),
            Command::Cancel => {}
        }
    }

    fn clipboard(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().ok();
        }
        self.clipboard.as_mut()
    }

    fn copy(&mut self, el: &ActiveEventLoop, img: &Pixmap) {
        let r = match self.clipboard() {
            Some(cb) => output::copy_image(cb, img),
            None => Err("clipboard unavailable".into()),
        };
        match r {
            Ok(()) => self.show_toast(el, "Copied to clipboard", 1.4),
            Err(e) => self.show_toast(el, &format!("Copy failed: {e}"), 4.0),
        }
    }

    fn save(&mut self, el: &ActiveEventLoop, img: &Pixmap, ask: bool) {
        let name = config::file_name_now();
        let path = if ask {
            match output::ask_path(&self.cfg.save_dir, &name) {
                Some(p) => p,
                None => return,
            }
        } else {
            config::unique_path(&self.cfg.save_dir, &name)
        };
        match output::write_png(img, &path) {
            Ok(()) => {
                if self.cfg.copy_on_save
                    && let Some(cb) = self.clipboard()
                {
                    let _ = output::copy_image(cb, img);
                }
                let shown = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.show_toast(el, &format!("Saved {shown}"), 2.0)
            }
            Err(e) => self.show_toast(el, &format!("Save failed: {e}"), 5.0),
        }
    }

    fn read_text(&mut self, el: &ActiveEventLoop, img: Pixmap) {
        if !self.ocr_ok {
            return self.show_toast(el, "Text recognition is not available on this system", 4.0);
        }
        self.ocr_pending += 1;
        self.show_toast(el, "Reading text…", 30.0);
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let _ = proxy.send_event(UserEvent::OcrDone(ocr::recognize(&img)));
        });
    }

    // ----- input -----------------------------------------------------------

    fn overlay_event(&mut self, el: &ActiveEventLoop, i: usize, event: WindowEvent) {
        let o = &mut self.overlays[i];
        let mut cmd = None;
        let mut redraw = true;
        match event {
            WindowEvent::CloseRequested => cmd = Some(Command::Cancel),
            WindowEvent::RedrawRequested => {
                o.editor.render(&mut o.frame);
                if let Err(e) = present::present(&mut o.surface, &o.window, &o.frame) {
                    eprintln!("shot: present failed: {e}");
                }
                redraw = false;
            }
            WindowEvent::ModifiersChanged(m) => o.editor.set_mods(mods(m.state())),
            WindowEvent::CursorMoved { position, .. } => {
                o.pos = Pt::new(position.x as f32, position.y as f32);
                o.editor.mouse_move(o.pos);
            }
            WindowEvent::CursorLeft { .. } => o.editor.mouse_leave(),
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => editor::Button::Left,
                    MouseButton::Right => editor::Button::Right,
                    MouseButton::Middle => editor::Button::Middle,
                    _ => return,
                };
                if state == ElementState::Pressed {
                    o.window.focus_window();
                    if b == editor::Button::Left && o.clicks.press(o.pos) {
                        cmd = o.editor.double_click(o.pos);
                    }
                    if cmd.is_none() {
                        cmd = o.editor.mouse_down(b, o.pos);
                    }
                    // Only one monitor holds a selection at a time.
                    if o.editor.is_engaged() {
                        for (j, other) in self.overlays.iter_mut().enumerate() {
                            if j != i && other.editor.is_engaged() {
                                other.editor.reset();
                                other.window.request_redraw();
                            }
                        }
                    }
                } else {
                    cmd = o.editor.mouse_up(b, o.pos);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => o.editor.wheel(wheel_lines(delta)),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let m = o.editor.mods();
                if let Some(k) = map_key(&event) {
                    cmd = o.editor.key(k, m);
                }
                if !m.ctrl
                    && let Some(t) = &event.text
                {
                    o.editor.text_input(t);
                }
            }
            _ => redraw = false,
        }
        let o = &mut self.overlays[i];
        let c = o.editor.cursor_kind();
        if c != o.cursor {
            o.cursor = c;
            o.window.set_cursor(cursor_icon(c));
        }
        if redraw {
            o.window.request_redraw();
        }
        if let Some(c) = cmd {
            self.finish(el, i, c);
        }
    }

    fn pin_event(&mut self, el: &ActiveEventLoop, i: usize, event: WindowEvent) {
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

    fn pin_action(&mut self, el: &ActiveEventLoop, i: usize, a: PinAction) {
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

    fn exit_if_done(&self, el: &ActiveEventLoop) {
        let idle = self.overlays.is_empty() && self.pins.is_empty() && self.toast.is_none() && self.ocr_pending == 0;
        if self.opts.once && self.captured && idle {
            el.exit();
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

impl ApplicationHandler<UserEvent> for App {
    fn new_events(&mut self, el: &ActiveEventLoop, cause: StartCause) {
        if cause == StartCause::Init {
            self.setup(el);
        }
    }

    fn resumed(&mut self, _el: &ActiveEventLoop) {}

    fn user_event(&mut self, el: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Hotkey => self.start_capture(el),
            UserEvent::Tray(TrayIconEvent::Click { button: TrayButton::Left, button_state: MouseButtonState::Up, .. }) => {
                self.start_capture(el)
            }
            UserEvent::Tray(_) => {}
            UserEvent::Menu(e) => {
                #[cfg(any(windows, target_os = "macos"))]
                if let Some(a) = self.pin_menu.as_ref().and_then(|m| m.action(&e.id)) {
                    if a == PinAction::Close && e.id == self.pin_menu.as_ref().unwrap().close_all {
                        self.pins.clear();
                    } else if let Some(i) = self.menu_target.and_then(|id| self.pins.iter().position(|p| p.pin.window.id() == id)) {
                        self.pin_action(el, i, a);
                    }
                    return;
                }
                let Some(t) = &self.tray else { return };
                if e.id == t.capture {
                    self.start_capture(el);
                } else if e.id == t.folder {
                    let _ = std::fs::create_dir_all(&self.cfg.save_dir);
                    output::open(&self.cfg.save_dir);
                } else if e.id == t.settings {
                    let _ = self.cfg.save(&self.cfg_path);
                    output::edit(&self.cfg_path);
                } else if e.id == t.close_pins {
                    self.pins.clear();
                } else if e.id == *t.autostart.id() {
                    let want = !super::autostart::is_enabled();
                    let r = super::autostart::set(want);
                    t.autostart.set_checked(super::autostart::is_enabled());
                    if let Err(e) = r {
                        self.show_toast(el, &format!("Could not change start-up setting: {e}"), 4.0);
                    }
                } else if e.id == t.quit {
                    el.exit();
                }
            }
            UserEvent::OcrDone(r) => {
                self.ocr_pending = self.ocr_pending.saturating_sub(1);
                match r {
                    Ok(text) if text.trim().is_empty() => self.show_toast(el, "No text found", 2.0),
                    Ok(text) => {
                        let lines = text.lines().count();
                        let ok = self.clipboard().map(|cb| cb.set_text(text).is_ok()).unwrap_or(false);
                        let msg = if ok {
                            format!("Copied text ({lines} line{})", if lines == 1 { "" } else { "s" })
                        } else {
                            "Copy failed: clipboard unavailable".into()
                        };
                        self.show_toast(el, &msg, 2.0);
                    }
                    Err(e) => self.show_toast(el, &format!("Text recognition failed: {e}"), 5.0),
                }
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(i) = self.overlays.iter().position(|o| o.window.id() == id) {
            self.overlay_event(el, i, event);
        } else if let Some(i) = self.pins.iter().position(|p| p.pin.window.id() == id) {
            self.pin_event(el, i, event);
        } else if let Some(t) = self.toast.as_mut().filter(|t| t.window.id() == id) {
            match event {
                WindowEvent::RedrawRequested => t.render(),
                WindowEvent::MouseInput { state: ElementState::Pressed, .. } => self.toast = None,
                _ => {}
            }
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.toast.as_ref().is_some_and(|t| t.until <= Instant::now()) {
            self.toast = None;
        }
        match &self.toast {
            Some(t) => el.set_control_flow(ControlFlow::WaitUntil(t.until)),
            None => el.set_control_flow(ControlFlow::Wait),
        }
        self.exit_if_done(el);
    }
}

fn mods(m: ModifiersState) -> Mods {
    Mods {
        shift: m.shift_key(),
        // Cmd is the shortcut key on macOS. Elsewhere AltGr arrives as Ctrl+Alt;
        // it must type characters (@, €, {…), not trigger Ctrl shortcuts.
        ctrl: if cfg!(target_os = "macos") { m.super_key() } else { m.control_key() && !m.alt_key() },
        alt: m.alt_key(),
    }
}

fn wheel_lines(d: MouseScrollDelta) -> f32 {
    match d {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => (p.y / 40.0) as f32,
    }
}

fn map_key(ev: &KeyEvent) -> Option<Key> {
    use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
    match &ev.logical_key {
        WKey::Named(n) => Some(match n {
            NamedKey::Escape => Key::Escape,
            NamedKey::Enter => Key::Enter,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::ArrowLeft => Key::Left,
            NamedKey::ArrowRight => Key::Right,
            NamedKey::ArrowUp => Key::Up,
            NamedKey::ArrowDown => Key::Down,
            NamedKey::F3 => Key::F3,
            _ => return None,
        }),
        _ => match ev.key_without_modifiers() {
            WKey::Character(s) => s.chars().next().and_then(|c| c.to_lowercase().next()).map(Key::Char),
            _ => None,
        },
    }
}

fn cursor_icon(c: CursorKind) -> CursorIcon {
    match c {
        CursorKind::Crosshair => CursorIcon::Crosshair,
        CursorKind::Arrow => CursorIcon::Default,
        CursorKind::Move => CursorIcon::Move,
        CursorKind::Text => CursorIcon::Text,
        CursorKind::ResizeNwse => CursorIcon::NwseResize,
        CursorKind::ResizeNesw => CursorIcon::NeswResize,
        CursorKind::ResizeEw => CursorIcon::EwResize,
        CursorKind::ResizeNs => CursorIcon::NsResize,
    }
}

/// Right-click menu on pins (Windows and macOS; Linux uses the shortcuts).
#[cfg(any(windows, target_os = "macos"))]
mod pin_menu {
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

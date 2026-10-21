//! The Settings window's content: layout, drawing and clicks. Like the
//! editor it is pure logic plus a pixmap, so it is tested without a display;
//! the shell turns its [`Action`]s into real changes.

use std::rc::Rc;

use tiny_skia::{Pixmap, PixmapPaint, Transform};

use crate::config::SaveMode;
use crate::geom::{IRect, Pt};
use crate::hotkey;
use crate::icon;
use crate::style::Color;
use crate::text::Font;
use crate::toolbar::{round_rect, round_rect_path};

const BG: Color = [0x11, 0x11, 0x13, 0xFF];
const CARD: Color = [0x1C, 0x1C, 0x1F, 0xFF];
const DIVIDER: Color = [0x2A, 0x2A, 0x2E, 0xFF];
const TEXT: Color = [0xF2, 0xF2, 0xF7, 0xFF];
const MUTED: Color = [0x8E, 0x8E, 0x96, 0xFF];
const CONTROL: Color = [0x2C, 0x2C, 0x30, 0xFF];
const CONTROL_HOVER: Color = [0x3A, 0x3A, 0x40, 0xFF];
const KNOB_OFF: Color = [0x3A, 0x3A, 0x3F, 0xFF];
/// Sharingan red, the app's accent.
pub const ACCENT: Color = [0xE0, 0x18, 0x2D, 0xFF];
const GOOD: Color = [0x34, 0xC7, 0x59, 0xFF];

/// Logical (unscaled) window size.
pub const WIDTH: f32 = 460.0;
pub const HEIGHT: f32 = 606.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// Shortcut registered and working.
    Active,
    Recording,
    Error(String),
}

/// Everything the window shows; the shell keeps it in sync with reality.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub hotkey: String,
    pub note: Note,
    pub save_dir: String,
    pub save_mode: SaveMode,
    pub copy_on_save: bool,
    pub autostart: bool,
    /// Where Shot installs itself for start at login.
    pub install_path: String,
    /// Running as a portable copy (settings beside the program).
    pub portable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Hotkey,
    ChangeFolder,
    ModeAsk,
    ModeFolder,
    CopyOnSave,
    Autostart,
    OpenFolder,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    StartRecording,
    CancelRecording,
    PickFolder,
    SetSaveMode(SaveMode),
    SetCopyOnSave(bool),
    SetAutostart(bool),
    OpenFolder,
    Quit,
}

pub struct View {
    pub state: State,
    scale: f32,
    font: Option<Rc<Font>>,
    bold: Option<Rc<Font>>,
    logo: Pixmap,
    hover: Option<Hit>,
}

/// Rectangles of everything on the page, in physical pixels.
struct Layout {
    header_logo: IRect,
    cards: [IRect; 3],
    labels: [(f32, &'static str); 3],
    hotkey_row: IRect,
    hotkey_button: IRect,
    note_y: f32,
    folder_row: IRect,
    folder_button: IRect,
    mode_row: IRect,
    mode_ask: IRect,
    mode_folder: IRect,
    copy_row: IRect,
    copy_toggle: IRect,
    auto_row: IRect,
    auto_toggle: IRect,
    install_y: f32,
    open_folder: IRect,
    quit: IRect,
}

impl View {
    pub fn new(state: State, scale: f32, font: Option<Rc<Font>>, bold: Option<Rc<Font>>) -> View {
        let scale = scale.max(0.5);
        View { state, scale, font, bold, logo: icon::app_icon((52.0 * scale).round() as u32), hover: None }
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.max(0.5);
        self.logo = icon::app_icon((52.0 * self.scale).round() as u32);
    }

    /// Window size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        ((WIDTH * self.scale).round() as u32, (HEIGHT * self.scale).round() as u32)
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn layout(&self) -> Layout {
        let s = |v: f32| self.s(v);
        let pad = 24.0;
        let inner = WIDTH - pad * 2.0;
        let card = |y: f32, h: f32| IRect::new(s(pad), s(y), s(inner), s(h));
        let row = |y: f32, h: f32| IRect::new(s(pad), s(y), s(inner), s(h));
        let right = |y: f32, h: f32, w: f32, rh: f32| IRect::new(s(pad + inner - 16.0 - w), s(y + (h - rh) / 2.0), s(w), s(rh));

        let c1 = 120.0;
        let c2 = 250.0;
        let c3 = 450.0;
        Layout {
            header_logo: IRect::new(s(pad), s(24.0), s(52.0), s(52.0)),
            cards: [card(c1, 90.0), card(c2, 168.0), card(c3, 80.0)],
            labels: [(c1 - 22.0, "CAPTURE SHORTCUT"), (c2 - 22.0, "SAVING"), (c3 - 22.0, "STARTUP")],
            hotkey_row: row(c1, 62.0),
            hotkey_button: right(c1, 62.0, 84.0, 32.0),
            note_y: c1 + 60.0,
            folder_row: row(c2, 56.0),
            folder_button: right(c2, 56.0, 84.0, 30.0),
            mode_row: row(c2 + 56.0, 56.0),
            mode_ask: IRect::new(s(pad + inner - 16.0 - 230.0), s(c2 + 56.0 + 13.0), s(115.0), s(30.0)),
            mode_folder: IRect::new(s(pad + inner - 16.0 - 115.0), s(c2 + 56.0 + 13.0), s(115.0), s(30.0)),
            copy_row: row(c2 + 112.0, 56.0),
            copy_toggle: right(c2 + 112.0, 56.0, 44.0, 26.0),
            auto_row: row(c3, 54.0),
            auto_toggle: right(c3, 54.0, 44.0, 26.0),
            install_y: c3 + 50.0,
            open_folder: IRect::new(s(pad), s(556.0), s(200.0), s(28.0)),
            quit: IRect::new(s(pad + inner - 92.0), s(556.0), s(92.0), s(28.0)),
        }
    }

    pub fn hit(&self, p: Pt) -> Option<Hit> {
        let l = self.layout();
        let zones = [
            (Hit::Hotkey, l.hotkey_row),
            (Hit::ChangeFolder, l.folder_button),
            (Hit::ModeAsk, l.mode_ask),
            (Hit::ModeFolder, l.mode_folder),
            (Hit::CopyOnSave, l.copy_row),
            (Hit::Autostart, l.auto_row),
            (Hit::OpenFolder, l.open_folder),
            (Hit::Quit, l.quit),
        ];
        zones.into_iter().find(|(_, r)| r.contains(p)).map(|(h, _)| h)
    }

    /// Returns true if the hover changed (so the window needs a redraw).
    pub fn mouse_move(&mut self, p: Pt) -> bool {
        let h = self.hit(p);
        let changed = h != self.hover;
        self.hover = h;
        changed
    }

    pub fn mouse_leave(&mut self) {
        self.hover = None;
    }

    /// Whether the pointer is over something clickable.
    pub fn over_control(&self) -> bool {
        self.hover.is_some()
    }

    pub fn click(&mut self, p: Pt) -> Option<Action> {
        let recording = self.state.note == Note::Recording;
        match self.hit(p) {
            Some(Hit::Hotkey) => Some(if recording { Action::CancelRecording } else { Action::StartRecording }),
            // Any other click ends a recording first.
            _ if recording => Some(Action::CancelRecording),
            Some(Hit::ChangeFolder) => Some(Action::PickFolder),
            Some(Hit::ModeAsk) => Some(Action::SetSaveMode(SaveMode::Ask)),
            Some(Hit::ModeFolder) => Some(Action::SetSaveMode(SaveMode::Folder)),
            Some(Hit::CopyOnSave) => Some(Action::SetCopyOnSave(!self.state.copy_on_save)),
            Some(Hit::Autostart) => Some(Action::SetAutostart(!self.state.autostart)),
            Some(Hit::OpenFolder) => Some(Action::OpenFolder),
            Some(Hit::Quit) => Some(Action::Quit),
            None => None,
        }
    }

    // ----- drawing --------------------------------------------------------

    pub fn render(&self, pm: &mut Pixmap) {
        pm.fill(crate::style::skia(BG));
        let l = self.layout();
        let sc = self.scale;
        let px = |v: f32| v * sc;

        // Header: logo, name, tagline, version.
        pm.draw_pixmap(l.header_logo.x, l.header_logo.y, self.logo.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        let tx = (l.header_logo.right() + self.s(14.0)) as f32;
        self.text_bold(pm, "Shot", tx, px(27.0), px(24.0), TEXT);
        self.text(pm, "Capture, annotate, pin.", tx, px(58.0), px(13.5), MUTED);
        let ver = concat!("v", env!("CARGO_PKG_VERSION"));
        let vw = self.measure(ver, px(12.0));
        self.text(pm, ver, px(WIDTH - 24.0) - vw, px(30.0), px(12.0), MUTED);

        for (y, label) in l.labels {
            self.text(pm, label, px(30.0), px(y), px(11.0), MUTED);
        }
        for c in l.cards {
            round_rect(pm, c, px(12.0), CARD);
        }

        // Shortcut card.
        let recording = self.state.note == Note::Recording;
        if recording {
            outline(pm, l.cards[0], px(12.0), ACCENT, px(1.5));
        }
        let cy = l.hotkey_row.y as f32 + l.hotkey_row.h as f32 / 2.0;
        if recording {
            self.text_mid(pm, "Press your new shortcut…", px(40.0), cy, px(15.0), ACCENT);
        } else {
            self.keycaps(pm, &self.state.hotkey, px(40.0), cy);
        }
        let btn = if recording { "Cancel" } else { "Change" };
        self.button(pm, l.hotkey_button, btn, self.hover == Some(Hit::Hotkey));
        let (note, color) = match &self.state.note {
            Note::Active => ("Press it anywhere to take a screenshot.".to_string(), MUTED),
            Note::Recording => ("Hold the modifier keys, then press a key. Esc cancels.".to_string(), MUTED),
            Note::Error(e) => (e.clone(), ACCENT),
        };
        if self.state.note == Note::Active {
            if let Some(dot) = tiny_skia::PathBuilder::from_circle(px(44.0), px(l.note_y + 7.5), px(3.5)) {
                crate::annot::fill(pm, &dot, GOOD);
            }
            self.text(pm, &note, px(54.0), px(l.note_y), px(12.0), color);
        } else {
            self.text(pm, &note, px(40.0), px(l.note_y), px(12.0), color);
        }

        // Saving card.
        self.divider(pm, l.folder_row.bottom());
        self.divider(pm, l.mode_row.bottom());
        let fy = l.folder_row.y as f32 + l.folder_row.h as f32 / 2.0;
        self.text_mid(pm, "Folder", px(40.0), fy, px(14.0), TEXT);
        let max_w = (l.folder_button.x as f32) - px(12.0) - px(110.0);
        let shown = self.ellipsize(&self.state.save_dir, px(12.5), max_w);
        self.text_mid(pm, &shown, px(110.0), fy, px(12.5), MUTED);
        self.button(pm, l.folder_button, "Change", self.hover == Some(Hit::ChangeFolder));

        let my = l.mode_row.y as f32 + l.mode_row.h as f32 / 2.0;
        self.text_mid(pm, "Save button", px(40.0), my, px(14.0), TEXT);
        let seg = IRect::new(l.mode_ask.x, l.mode_ask.y, l.mode_folder.right() - l.mode_ask.x, l.mode_ask.h);
        round_rect(pm, seg, px(8.0), CONTROL);
        for (r, label, mode, hit) in
            [(l.mode_ask, "Ask where", SaveMode::Ask, Hit::ModeAsk), (l.mode_folder, "To folder", SaveMode::Folder, Hit::ModeFolder)]
        {
            let on = self.state.save_mode == mode;
            if on {
                round_rect(pm, inset(r, self.s(2.0)), px(6.5), [0x4A, 0x4A, 0x52, 0xFF]);
            } else if self.hover == Some(hit) {
                round_rect(pm, inset(r, self.s(2.0)), px(6.5), CONTROL_HOVER);
            }
            self.text_center(pm, label, r, px(13.0), if on { TEXT } else { MUTED });
        }

        let ky = l.copy_row.y as f32 + l.copy_row.h as f32 / 2.0;
        self.text_mid(pm, "Also copy when saving", px(40.0), ky, px(14.0), TEXT);
        toggle(pm, l.copy_toggle, self.state.copy_on_save, sc);

        // Startup card.
        let ay = l.auto_row.y as f32 + l.auto_row.h as f32 / 2.0;
        self.text_mid(pm, "Start Shot when I log in", px(40.0), ay, px(14.0), TEXT);
        toggle(pm, l.auto_toggle, self.state.autostart, sc);
        let install = match (self.state.portable, self.state.autostart) {
            (true, true) => format!("Portable: runs {} at every login.", self.state.install_path),
            (true, false) => "Portable: runs this copy at every login; nothing is installed.".to_string(),
            (false, true) => format!("Installed at {}", self.state.install_path),
            (false, false) => "Installs Shot on this PC and runs it at every login.".to_string(),
        };
        let install = self.ellipsize(&install, px(11.5), px(WIDTH - 80.0));
        self.text(pm, &install, px(40.0), px(l.install_y), px(11.5), MUTED);

        // Footer.
        let open_c = if self.hover == Some(Hit::OpenFolder) { TEXT } else { MUTED };
        self.text_mid(pm, "Open screenshots folder  ›", px(28.0), l.open_folder.y as f32 + l.open_folder.h as f32 / 2.0, px(13.0), open_c);
        if self.hover == Some(Hit::Quit) {
            round_rect(pm, l.quit, px(8.0), [0x3A, 0x14, 0x18, 0xFF]);
        }
        self.text_center(pm, "Quit Shot", l.quit, px(13.0), [0xFF, 0x6B, 0x75, 0xFF]);
    }

    fn keycaps(&self, pm: &mut Pixmap, hk: &str, x: f32, cy: f32) {
        let sc = self.scale;
        let fs = 14.5 * sc;
        let h = 32.0 * sc;
        let mut x = x;
        for (i, cap) in hotkey::keycaps(hk).iter().enumerate() {
            if i > 0 {
                let w = self.measure("+", fs);
                self.text_mid(pm, "+", x + 7.0 * sc, cy, fs, MUTED);
                x += w + 14.0 * sc;
            }
            let w = (self.measure(cap, fs) + 24.0 * sc).max(h);
            let r = IRect::new(x.round() as i32, (cy - h / 2.0).round() as i32, w.round() as i32, h.round() as i32);
            // A keyboard key: slightly darker bottom edge, lighter face.
            round_rect(pm, r, 7.0 * sc, [0x0E, 0x0E, 0x10, 0xFF]);
            round_rect(pm, IRect::new(r.x, r.y, r.w, r.h - (2.0 * sc).round() as i32), 7.0 * sc, [0x3A, 0x3A, 0x40, 0xFF]);
            outline(pm, IRect::new(r.x, r.y, r.w, r.h - (2.0 * sc).round() as i32), 7.0 * sc, [0x55, 0x55, 0x5C, 0xFF], sc);
            self.text_center(pm, cap, IRect::new(r.x, r.y, r.w, r.h - (2.0 * sc).round() as i32), fs, TEXT);
            x += w;
        }
    }

    fn button(&self, pm: &mut Pixmap, r: IRect, label: &str, hover: bool) {
        round_rect(pm, r, 8.0 * self.scale, if hover { CONTROL_HOVER } else { CONTROL });
        self.text_center(pm, label, r, 13.0 * self.scale, TEXT);
    }

    fn divider(&self, pm: &mut Pixmap, y: i32) {
        let r = IRect::new(self.s(40.0), y, self.s(WIDTH - 80.0), self.s(1.0).max(1));
        round_rect(pm, r, 0.0, DIVIDER);
    }

    fn measure(&self, t: &str, px: f32) -> f32 {
        self.font.as_ref().map(|f| f.measure(t, px).0).unwrap_or(0.0)
    }

    fn text(&self, pm: &mut Pixmap, t: &str, x: f32, y: f32, px: f32, c: Color) {
        if let Some(f) = &self.font {
            f.draw(pm, t, x, y, px, c);
        }
    }

    fn text_bold(&self, pm: &mut Pixmap, t: &str, x: f32, y: f32, px: f32, c: Color) {
        if let Some(f) = self.bold.as_ref().or(self.font.as_ref()) {
            f.draw(pm, t, x, y, px, c);
        }
    }

    /// Text vertically centred on `cy`.
    fn text_mid(&self, pm: &mut Pixmap, t: &str, x: f32, cy: f32, px: f32, c: Color) {
        if let Some(f) = &self.font {
            f.draw(pm, t, x, cy - f.line_height(px) / 2.0, px, c);
        }
    }

    fn text_center(&self, pm: &mut Pixmap, t: &str, r: IRect, px: f32, c: Color) {
        let w = self.measure(t, px);
        self.text_mid(pm, t, r.x as f32 + (r.w as f32 - w) / 2.0, r.y as f32 + r.h as f32 / 2.0, px, c);
    }

    /// Shorten with an ellipsis in the middle, keeping the end of a path visible.
    fn ellipsize(&self, t: &str, px: f32, max_w: f32) -> String {
        if self.font.is_none() || self.measure(t, px) <= max_w {
            return t.to_string();
        }
        let chars: Vec<char> = t.chars().collect();
        let mut keep = chars.len();
        while keep > 4 {
            keep -= 1;
            let head = keep / 3;
            let tail = keep - head;
            let s: String = chars[..head].iter().chain(['…'].iter()).chain(chars[chars.len() - tail..].iter()).collect();
            if self.measure(&s, px) <= max_w {
                return s;
            }
        }
        "…".into()
    }
}

fn inset(r: IRect, d: i32) -> IRect {
    IRect::new(r.x + d, r.y + d, r.w - 2 * d, r.h - 2 * d)
}

fn outline(pm: &mut Pixmap, r: IRect, radius: f32, c: Color, w: f32) {
    if let Some(p) = round_rect_path(r, radius) {
        crate::annot::stroke(pm, &p, c, w, tiny_skia::BlendMode::SourceOver);
    }
}

fn toggle(pm: &mut Pixmap, r: IRect, on: bool, sc: f32) {
    round_rect(pm, r, r.h as f32 / 2.0, if on { ACCENT } else { KNOB_OFF });
    let kr = r.h as f32 / 2.0 - 3.0 * sc;
    let kx = if on { r.right() as f32 - r.h as f32 / 2.0 } else { r.x as f32 + r.h as f32 / 2.0 };
    if let Some(k) = tiny_skia::PathBuilder::from_circle(kx, r.y as f32 + r.h as f32 / 2.0, kr) {
        crate::annot::fill(pm, &k, [0xFF, 0xFF, 0xFF, 0xFF]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        let st = State {
            hotkey: "Shift+Space".into(),
            note: Note::Active,
            save_dir: r"C:\Users\someone\Pictures\Shot".into(),
            save_mode: SaveMode::Ask,
            copy_on_save: false,
            autostart: false,
            install_path: r"C:\Users\someone\AppData\Local\Programs\Shot\Shot.exe".into(),
            portable: false,
        };
        View::new(st, 1.0, None, None)
    }

    fn centre(r: IRect) -> Pt {
        Pt::new(r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0)
    }

    #[test]
    fn every_control_answers_a_click() {
        let mut v = view();
        let l = v.layout();
        assert_eq!(v.click(centre(l.hotkey_button)), Some(Action::StartRecording));
        assert_eq!(v.click(centre(l.folder_button)), Some(Action::PickFolder));
        assert_eq!(v.click(centre(l.mode_folder)), Some(Action::SetSaveMode(SaveMode::Folder)));
        assert_eq!(v.click(centre(l.mode_ask)), Some(Action::SetSaveMode(SaveMode::Ask)));
        assert_eq!(v.click(centre(l.copy_toggle)), Some(Action::SetCopyOnSave(true)));
        assert_eq!(v.click(centre(l.auto_row)), Some(Action::SetAutostart(true)));
        assert_eq!(v.click(centre(l.open_folder)), Some(Action::OpenFolder));
        assert_eq!(v.click(centre(l.quit)), Some(Action::Quit));
        assert_eq!(v.click(Pt::new(3.0, 3.0)), None);
    }

    #[test]
    fn clicks_while_recording_cancel_it() {
        let mut v = view();
        v.state.note = Note::Recording;
        let l = v.layout();
        assert_eq!(v.click(centre(l.hotkey_row)), Some(Action::CancelRecording));
        assert_eq!(v.click(centre(l.quit)), Some(Action::CancelRecording));
    }

    #[test]
    fn layout_fits_the_window_at_any_scale() {
        for sc in [1.0, 1.25, 1.5, 2.0] {
            let mut v = view();
            v.set_scale(sc);
            let (w, h) = v.size();
            let l = v.layout();
            for r in [l.quit, l.open_folder, l.cards[2], l.auto_toggle, l.mode_ask] {
                assert!(r.x >= 0 && r.right() <= w as i32 && r.bottom() <= h as i32, "{r:?} outside {w}x{h} at {sc}");
            }
            let mut pm = Pixmap::new(w, h).unwrap();
            v.render(&mut pm);
        }
    }

    #[test]
    fn hover_reports_changes_once() {
        let mut v = view();
        let p = centre(v.layout().quit);
        assert!(v.mouse_move(p));
        assert!(!v.mouse_move(p));
        assert!(v.over_control());
    }
}

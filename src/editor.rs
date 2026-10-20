//! The capture editor for one monitor: pick a region, annotate it, then hand a
//! command to the shell. Pure logic plus rendering into a pixmap, so it runs
//! (and is tested) without any window system.

use std::rc::Rc;

use tiny_skia::{IntSize, Pixmap, PixmapPaint, Transform};

use crate::annot::{self, Annotation, Shape};
use crate::geom::{IRect, Pt};
use crate::style::{self, Color, PALETTE};
use crate::text::Font;
use crate::toolbar::{self, Action, Bar, BarState, Item, Tool};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// `ctrl` means Cmd on macOS; the shell maps it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
    Enter,
    Backspace,
    Left,
    Right,
    Up,
    Down,
    F3,
    /// A character key, lower-cased, for shortcuts. Typed text arrives
    /// separately through [`Editor::text_input`].
    Char(char),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Copy,
    Save,
    QuickSave,
    Pin,
    Ocr,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Crosshair,
    Arrow,
    Move,
    Text,
    ResizeNwse,
    ResizeNesw,
    ResizeEw,
    ResizeNs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

#[derive(Clone, Debug)]
enum Drag {
    None,
    Selecting { start: Pt, moved: bool },
    Moving { grab: Pt, orig: IRect },
    Resizing { handle: Handle, orig: IRect, start: Pt },
    Drawing { start: Pt, ann: Annotation },
}

#[derive(Clone, Debug)]
struct TextEdit {
    at: Pt,
    text: String,
}

/// Drawing style that carries over from one capture to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub color: Color,
    /// The highlighter keeps its own colour so it stays yellow by default.
    pub marker: Color,
    pub size: u32,
}

impl Default for Style {
    fn default() -> Self {
        Style { color: PALETTE[0], marker: PALETTE[2], size: 2 }
    }
}

pub struct Editor {
    base: Pixmap,
    dimmed: Pixmap,
    /// `base` with the annotations burned in; created on the first one, so a
    /// plain capture holds two full-screen copies instead of three.
    composite: Option<Pixmap>,
    screen: IRect,
    scale: f32,
    font: Option<Rc<Font>>,
    windows: Vec<IRect>,

    sel: Option<IRect>,
    annots: Vec<Annotation>,
    redo: Vec<Annotation>,
    drag: Drag,
    text: Option<TextEdit>,

    tool: Tool,
    style: Style,
    palette_open: bool,
    cursor: Option<Pt>,
    mods: Mods,
    hover: Option<Item>,
    ocr_available: bool,
}

pub const MAX_SIZE: u32 = 10;

impl Editor {
    /// `windows` are top-level window rectangles in this image's coordinates,
    /// topmost first; clicking without dragging picks one.
    pub fn new(base: Pixmap, scale: f32, font: Option<Rc<Font>>, windows: Vec<IRect>) -> Editor {
        let screen = IRect::new(0, 0, base.width() as i32, base.height() as i32);
        let windows = windows.into_iter().map(|w| w.intersect(&screen)).filter(|w| w.w > 8 && w.h > 8).collect();
        let mut dimmed = base.clone();
        let keep = ((1.0 - style::DIM) * 256.0) as u32;
        let lut: [u8; 256] = std::array::from_fn(|v| ((v as u32 * keep) >> 8) as u8);
        for px in dimmed.data_mut().as_chunks_mut::<4>().0 {
            px[0] = lut[px[0] as usize];
            px[1] = lut[px[1] as usize];
            px[2] = lut[px[2] as usize];
        }
        Editor {
            composite: None,
            dimmed,
            base,
            screen,
            scale: scale.max(0.5),
            font,
            windows,
            sel: None,
            annots: Vec::new(),
            redo: Vec::new(),
            drag: Drag::None,
            text: None,
            tool: Tool::Select,
            style: Style::default(),
            palette_open: false,
            cursor: None,
            mods: Mods::default(),
            hover: None,
            ocr_available: false,
        }
    }

    pub fn set_style(&mut self, s: Style) {
        self.style = Style { size: s.size.clamp(1, MAX_SIZE), ..s };
    }

    pub fn style(&self) -> Style {
        self.style
    }

    /// Colour of the active tool.
    fn color(&self) -> Color {
        if self.tool == Tool::Marker { self.style.marker } else { self.style.color }
    }

    fn set_color(&mut self, c: Color) {
        if self.tool == Tool::Marker {
            self.style.marker = c;
        } else {
            self.style.color = c;
        }
    }

    pub fn set_ocr_available(&mut self, on: bool) {
        self.ocr_available = on;
    }

    pub fn width(&self) -> u32 {
        self.base.width()
    }

    pub fn height(&self) -> u32 {
        self.base.height()
    }

    pub fn selection(&self) -> Option<IRect> {
        self.sel
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn annotations(&self) -> &[Annotation] {
        &self.annots
    }

    /// True once the user has started picking a region on this monitor.
    pub fn is_engaged(&self) -> bool {
        self.sel.is_some() || !matches!(self.drag, Drag::None)
    }

    /// Forget the selection (another monitor took over).
    pub fn reset(&mut self) {
        self.commit_text();
        self.sel = None;
        self.annots.clear();
        self.redo.clear();
        self.composite = None;
        self.drag = Drag::None;
        self.tool = Tool::Select;
        self.palette_open = false;
    }

    pub fn set_selection(&mut self, r: IRect) {
        let r = r.intersect(&self.screen);
        self.sel = (!r.is_empty()).then_some(r);
    }

    // ----- sizes, all scaled for the monitor ------------------------------

    fn stroke_width(&self) -> f32 {
        (1.0 + self.style.size as f32 * 1.5) * self.scale
    }

    fn width_for(&self, tool: Tool) -> f32 {
        let s = self.style.size as f32;
        match tool {
            Tool::Marker => (8.0 + s * 4.0) * self.scale,
            Tool::Text => (12.0 + s * 4.0) * self.scale,
            Tool::Blur => (4.0 + s * 2.0) * self.scale,
            Tool::Step => (9.0 + s * 2.0) * self.scale,
            _ => self.stroke_width(),
        }
    }

    fn ui(&self, v: f32) -> f32 {
        v * self.scale
    }

    fn bar(&self) -> Option<Bar> {
        self.sel
            .filter(|_| !matches!(self.drag, Drag::Selecting { .. }))
            .map(|s| Bar::layout(s, self.screen, self.scale, self.palette_open))
    }

    // ----- input ----------------------------------------------------------

    pub fn set_mods(&mut self, m: Mods) {
        self.mods = m;
    }

    pub fn mods(&self) -> Mods {
        self.mods
    }

    pub fn mouse_leave(&mut self) {
        self.cursor = None;
        self.hover = None;
    }

    pub fn mouse_move(&mut self, p: Pt) {
        self.cursor = Some(p);
        self.hover = self.bar().and_then(|b| b.hit(p));
        let shift = self.mods.shift;
        let screen = self.screen;
        match &mut self.drag {
            Drag::None => {}
            Drag::Selecting { start, moved } => {
                if !*moved && start.dist(p) >= 3.0 {
                    *moved = true;
                }
                if *moved {
                    let s = *start;
                    self.sel = Some(IRect::from_points(screen.clamp(s), screen.clamp(p)));
                }
            }
            Drag::Moving { grab, orig } => {
                let (dx, dy) = ((p.x - grab.x).round() as i32, (p.y - grab.y).round() as i32);
                let nx = (orig.x + dx).clamp(0, screen.w - orig.w);
                let ny = (orig.y + dy).clamp(0, screen.h - orig.h);
                self.sel = Some(IRect::new(nx, ny, orig.w, orig.h));
            }
            Drag::Resizing { handle, orig, start } => {
                let (dx, dy) = (p.x - start.x, p.y - start.y);
                let (mut x0, mut y0, mut x1, mut y1) = (orig.x as f32, orig.y as f32, orig.right() as f32, orig.bottom() as f32);
                match handle {
                    Handle::N | Handle::NE | Handle::NW => y0 += dy,
                    Handle::S | Handle::SE | Handle::SW => y1 += dy,
                    _ => {}
                }
                match handle {
                    Handle::W | Handle::NW | Handle::SW => x0 += dx,
                    Handle::E | Handle::NE | Handle::SE => x1 += dx,
                    _ => {}
                }
                let r = IRect::from_points(screen.clamp(Pt::new(x0, y0)), screen.clamp(Pt::new(x1, y1)));
                self.sel = Some(IRect::new(r.x, r.y, r.w.max(1), r.h.max(1)));
            }
            Drag::Drawing { start, ann } => {
                let sel = self.sel.unwrap_or(screen);
                let p = sel.clamp(p);
                let start = *start;
                match &mut ann.shape {
                    Shape::Pen(pts) | Shape::Marker(pts) => {
                        if shift {
                            pts.truncate(1);
                            pts.push(p);
                        } else if pts.last().is_none_or(|l| l.dist(p) >= 1.5) {
                            pts.push(p);
                        }
                    }
                    Shape::Line(_, b) | Shape::Arrow(_, b) => *b = if shift { annot::snap45(start, p) } else { p },
                    Shape::Rect(_, b) | Shape::Ellipse(_, b) => *b = if shift { annot::snap_square(start, p) } else { p },
                    Shape::Blur(_, b) => *b = p,
                    Shape::Text { .. } | Shape::Step { .. } => {}
                }
            }
        }
    }

    pub fn mouse_down(&mut self, b: Button, p: Pt) -> Option<Command> {
        self.cursor = Some(p);
        match b {
            Button::Left => self.left_down(p),
            Button::Right => self.right_down(),
            Button::Middle => None,
        }
    }

    fn left_down(&mut self, p: Pt) -> Option<Command> {
        let Some(sel) = self.sel else {
            self.drag = Drag::Selecting { start: p, moved: false };
            return None;
        };
        if let Some(bar) = self.bar() {
            if let Some(item) = bar.hit(p) {
                return self.click_item(item);
            }
            if bar.contains(p) {
                return None;
            }
        }
        self.palette_open = false;
        if let Some(h) = self.handle_at(p) {
            self.commit_text();
            self.drag = Drag::Resizing { handle: h, orig: sel, start: p };
            return None;
        }
        if !sel.contains(p) {
            if self.tool == Tool::Select {
                // Start over with a fresh region; annotations stay where they are.
                self.commit_text();
                self.sel = None;
                self.drag = Drag::Selecting { start: p, moved: false };
            } else {
                self.commit_text();
            }
            return None;
        }
        match self.tool {
            Tool::Select => self.drag = Drag::Moving { grab: p, orig: sel },
            Tool::Text => {
                self.commit_text();
                let lh = self.font.as_ref().map(|f| f.line_height(self.width_for(Tool::Text))).unwrap_or(0.0);
                self.text = Some(TextEdit { at: Pt::new(p.x, p.y - lh / 2.0), text: String::new() });
            }
            Tool::Step => {
                self.commit_text();
                let n =
                    self.annots.iter().filter_map(|a| if let Shape::Step { n, .. } = a.shape { Some(n) } else { None }).max().unwrap_or(0)
                        + 1;
                let ann = Annotation { shape: Shape::Step { at: p, n }, color: self.color(), width: self.width_for(Tool::Step) };
                self.push(ann);
            }
            tool => {
                self.commit_text();
                let shape = match tool {
                    Tool::Pen => Shape::Pen(vec![p]),
                    Tool::Marker => Shape::Marker(vec![p]),
                    Tool::Line => Shape::Line(p, p),
                    Tool::Arrow => Shape::Arrow(p, p),
                    Tool::Rect => Shape::Rect(p, p),
                    Tool::Ellipse => Shape::Ellipse(p, p),
                    _ => Shape::Blur(p, p),
                };
                let ann = Annotation { shape, color: self.color(), width: self.width_for(tool) };
                self.drag = Drag::Drawing { start: p, ann };
            }
        }
        None
    }

    fn right_down(&mut self) -> Option<Command> {
        if matches!(self.drag, Drag::Drawing { .. }) {
            self.drag = Drag::None;
        } else if self.text.is_some() {
            self.commit_text();
        } else if self.palette_open {
            self.palette_open = false;
        } else if self.sel.is_some() {
            self.reset();
        } else {
            return Some(Command::Cancel);
        }
        None
    }

    pub fn mouse_up(&mut self, b: Button, p: Pt) -> Option<Command> {
        if b != Button::Left {
            return None;
        }
        self.mouse_move(p);
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Selecting { moved: false, .. } => {
                // A click: take the window under the cursor, else the whole screen.
                self.sel = Some(self.window_at(p).unwrap_or(self.screen));
            }
            Drag::Selecting { moved: true, .. } => {
                if self.sel.is_some_and(|s| s.w < 2 || s.h < 2) {
                    self.sel = None;
                }
            }
            Drag::Drawing { ann, .. } => {
                let keep = match &ann.shape {
                    Shape::Pen(_) | Shape::Marker(_) => true,
                    Shape::Blur(a, b) => !IRect::from_points(*a, *b).is_empty(),
                    Shape::Line(a, b) | Shape::Arrow(a, b) | Shape::Rect(a, b) | Shape::Ellipse(a, b) => a.dist(*b) >= 2.0,
                    _ => true,
                };
                if keep {
                    self.push(ann);
                }
            }
            _ => {}
        }
        None
    }

    pub fn double_click(&mut self, p: Pt) -> Option<Command> {
        let inside = self.sel.is_some_and(|s| s.contains(p));
        let on_bar = self.bar().is_some_and(|b| b.contains(p));
        (inside && !on_bar && self.tool == Tool::Select).then_some(Command::Copy)
    }

    /// Mouse wheel, in lines (positive = away from the user).
    pub fn wheel(&mut self, lines: f32) {
        if self.sel.is_none() || lines == 0.0 {
            return;
        }
        let step = if lines > 0.0 { 1 } else { -1 };
        self.style.size = (self.style.size as i32 + step).clamp(1, MAX_SIZE as i32) as u32;
    }

    pub fn key(&mut self, k: Key, m: Mods) -> Option<Command> {
        self.mods = m;
        if self.text.is_some() && !m.ctrl {
            match k {
                Key::Escape => self.commit_text(),
                Key::Enter => self.text.as_mut().unwrap().text.push('\n'),
                Key::Backspace => {
                    self.text.as_mut().unwrap().text.pop();
                }
                _ => {}
            }
            return None;
        }
        if m.ctrl {
            return match k {
                Key::Char('c') => Some(Command::Copy),
                Key::Char('s') if m.shift => Some(Command::QuickSave),
                Key::Char('s') => Some(Command::Save),
                Key::Char('p') => Some(Command::Pin),
                Key::Char('t') => self.ocr_available.then_some(Command::Ocr),
                Key::Char('z') if m.shift => {
                    self.redo();
                    None
                }
                Key::Char('z') => {
                    self.undo();
                    None
                }
                Key::Char('y') => {
                    self.redo();
                    None
                }
                Key::Char('a') => {
                    self.commit_text();
                    self.sel = Some(self.screen);
                    None
                }
                Key::Enter => Some(Command::Copy),
                _ => None,
            };
        }
        match k {
            Key::Escape => {
                if matches!(self.drag, Drag::Drawing { .. } | Drag::Selecting { .. }) {
                    self.drag = Drag::None;
                    return None;
                }
                if self.palette_open {
                    self.palette_open = false;
                    return None;
                }
                Some(Command::Cancel)
            }
            Key::Enter => self.sel.map(|_| Command::Copy),
            Key::F3 => self.sel.map(|_| Command::Pin),
            Key::Left | Key::Right | Key::Up | Key::Down => {
                let d = if m.shift { 10 } else { 1 };
                if let Some(s) = self.sel {
                    let (dx, dy) = match k {
                        Key::Left => (-d, 0),
                        Key::Right => (d, 0),
                        Key::Up => (0, -d),
                        _ => (0, d),
                    };
                    let nx = (s.x + dx).clamp(0, self.screen.w - s.w);
                    let ny = (s.y + dy).clamp(0, self.screen.h - s.h);
                    self.sel = Some(IRect::new(nx, ny, s.w, s.h));
                }
                None
            }
            Key::Char(c) if self.sel.is_some() => {
                if let Some(t) = Tool::from_shortcut(c) {
                    self.set_tool(t);
                } else if let Some(d) = c.to_digit(10).filter(|d| (1..=8).contains(d)) {
                    self.set_color(PALETTE[d as usize - 1]);
                } else if c == '[' {
                    self.wheel(-1.0);
                } else if c == ']' {
                    self.wheel(1.0);
                }
                None
            }
            _ => None,
        }
    }

    /// Text typed by the user (already composed by the OS keyboard layout).
    pub fn text_input(&mut self, s: &str) {
        if let Some(t) = &mut self.text {
            t.text.extend(s.chars().filter(|c| !c.is_control()));
        }
    }

    fn set_tool(&mut self, t: Tool) {
        self.commit_text();
        self.tool = t;
        self.palette_open = false;
    }

    fn click_item(&mut self, item: Item) -> Option<Command> {
        match item {
            Item::Tool(t) => self.set_tool(t),
            Item::Color => self.palette_open = !self.palette_open,
            Item::Swatch(i) => {
                self.set_color(PALETTE[i]);
                self.palette_open = false;
            }
            Item::Action(a) => {
                self.palette_open = false;
                return match a {
                    Action::Undo => {
                        self.undo();
                        None
                    }
                    Action::Redo => {
                        self.redo();
                        None
                    }
                    Action::Ocr => self.ocr_available.then_some(Command::Ocr),
                    Action::Pin => Some(Command::Pin),
                    Action::Save => Some(Command::Save),
                    Action::Copy => Some(Command::Copy),
                    Action::Close => Some(Command::Cancel),
                };
            }
        }
        None
    }

    // ----- annotation stack -----------------------------------------------

    /// What the selection currently shows: the capture plus annotations.
    fn picture(&self) -> &Pixmap {
        self.composite.as_ref().unwrap_or(&self.base)
    }

    fn burn_in(&mut self, a: &Annotation) {
        let pm = self.composite.get_or_insert_with(|| self.base.clone());
        a.draw(pm, self.font.as_deref());
    }

    fn push(&mut self, a: Annotation) {
        self.burn_in(&a);
        self.annots.push(a);
        self.redo.clear();
    }

    fn commit_text(&mut self) {
        if let Some(t) = self.text.take()
            && !t.text.trim().is_empty()
        {
            let ann = Annotation { shape: Shape::Text { at: t.at, text: t.text }, color: self.color(), width: self.width_for(Tool::Text) };
            self.push(ann);
        }
    }

    pub fn undo(&mut self) {
        if self.text.take().is_some() {
            return;
        }
        if let Some(a) = self.annots.pop() {
            self.redo.push(a);
            self.rebuild();
        }
    }

    pub fn redo(&mut self) {
        if let Some(a) = self.redo.pop() {
            self.burn_in(&a);
            self.annots.push(a);
        }
    }

    fn rebuild(&mut self) {
        self.composite = None;
        for a in std::mem::take(&mut self.annots) {
            self.burn_in(&a);
            self.annots.push(a);
        }
    }

    // ----- queries ----------------------------------------------------------

    fn window_at(&self, p: Pt) -> Option<IRect> {
        self.windows.iter().find(|w| w.contains(p)).copied()
    }

    fn handles(&self, s: IRect) -> [(Handle, Pt); 8] {
        let (x0, y0, x1, y1) = (s.x as f32, s.y as f32, s.right() as f32, s.bottom() as f32);
        let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        [
            (Handle::NW, Pt::new(x0, y0)),
            (Handle::N, Pt::new(mx, y0)),
            (Handle::NE, Pt::new(x1, y0)),
            (Handle::E, Pt::new(x1, my)),
            (Handle::SE, Pt::new(x1, y1)),
            (Handle::S, Pt::new(mx, y1)),
            (Handle::SW, Pt::new(x0, y1)),
            (Handle::W, Pt::new(x0, my)),
        ]
    }

    fn handle_at(&self, p: Pt) -> Option<Handle> {
        let s = self.sel?;
        let tol = self.ui(7.0);
        if let Some((h, _)) = self.handles(s).into_iter().find(|(_, c)| (c.x - p.x).abs() <= tol && (c.y - p.y).abs() <= tol) {
            return Some(h);
        }
        // Edges are grabbable too, but only with the move tool so drawing near an edge still draws.
        if self.tool != Tool::Select {
            return None;
        }
        let tol = self.ui(4.0);
        let (x0, y0, x1, y1) = (s.x as f32, s.y as f32, s.right() as f32, s.bottom() as f32);
        let in_x = p.x > x0 && p.x < x1;
        let in_y = p.y > y0 && p.y < y1;
        if in_x && (p.y - y0).abs() <= tol {
            Some(Handle::N)
        } else if in_x && (p.y - y1).abs() <= tol {
            Some(Handle::S)
        } else if in_y && (p.x - x0).abs() <= tol {
            Some(Handle::W)
        } else if in_y && (p.x - x1).abs() <= tol {
            Some(Handle::E)
        } else {
            None
        }
    }

    pub fn cursor_kind(&self) -> CursorKind {
        let Some(p) = self.cursor else { return CursorKind::Arrow };
        match &self.drag {
            Drag::Moving { .. } => return CursorKind::Move,
            Drag::Resizing { handle, .. } => return handle_cursor(*handle),
            Drag::Selecting { .. } | Drag::Drawing { .. } => return CursorKind::Crosshair,
            Drag::None => {}
        }
        let Some(sel) = self.sel else { return CursorKind::Crosshair };
        if self.bar().is_some_and(|b| b.contains(p)) {
            return CursorKind::Arrow;
        }
        if let Some(h) = self.handle_at(p) {
            return handle_cursor(h);
        }
        match (sel.contains(p), self.tool) {
            (true, Tool::Select) => CursorKind::Move,
            (true, Tool::Text) => CursorKind::Text,
            (true, _) => CursorKind::Crosshair,
            (false, Tool::Select) => CursorKind::Crosshair,
            (false, _) => CursorKind::Arrow,
        }
    }

    // ----- output -----------------------------------------------------------

    /// The selected region with all annotations burned in.
    pub fn export(&mut self) -> Option<Pixmap> {
        self.commit_text();
        let s = self.sel?.intersect(&self.screen);
        crop(self.picture(), s)
    }

    /// Render the whole overlay (dimmed screen, selection, chrome) into `out`,
    /// which must be the size of the capture.
    pub fn render(&self, out: &mut Pixmap) {
        out.data_mut().copy_from_slice(self.dimmed.data());

        let bright = match (&self.drag, self.sel) {
            (_, Some(s)) => Some(s),
            (Drag::None, None) => self.cursor.and_then(|c| self.window_at(c)),
            _ => None,
        };
        if let Some(r) = bright {
            blit(out, self.picture(), r);
        }

        if let Drag::Drawing { ann, .. } = &self.drag {
            ann.draw(out, self.font.as_deref());
        }
        if let Some(t) = &self.text {
            self.render_text_edit(out, t);
        }

        match self.sel {
            Some(s) => self.render_selection(out, s),
            None => {
                if let Some(w) = bright {
                    self.frame(out, w, self.ui(2.0));
                }
                self.render_crosshair(out);
                self.render_hint(out);
            }
        }
    }

    fn render_selection(&self, out: &mut Pixmap, s: IRect) {
        self.frame(out, s, self.ui(1.0).max(1.0));
        let hs = self.ui(7.0);
        for (_, c) in self.handles(s) {
            let r = IRect::new((c.x - hs / 2.0).round() as i32, (c.y - hs / 2.0).round() as i32, hs as i32, hs as i32);
            toolbar::round_rect(out, r, self.ui(1.5), style::ACCENT);
            toolbar::round_rect(out, toolbar::inset(r, self.ui(1.0).max(1.0) as i32), self.ui(1.0), [255, 255, 255, 255]);
        }
        self.render_size_label(out, s);

        // Brush preview for freehand tools.
        if let (Some(p), Tool::Pen | Tool::Marker, Drag::None) = (self.cursor, self.tool, &self.drag)
            && s.contains(p)
            && !self.bar().is_some_and(|b| b.contains(p))
        {
            let r = self.width_for(self.tool) / 2.0;
            if let Some(c) = tiny_skia::PathBuilder::from_circle(p.x, p.y, r.max(2.0)) {
                annot::stroke(out, &c, [255, 255, 255, 160], 1.0, tiny_skia::BlendMode::SourceOver);
            }
        }

        if let Some(bar) = self.bar() {
            let st = BarState {
                tool: self.tool,
                color: self.color(),
                marker: self.style.marker,
                size: self.style.size,
                hover: self.hover,
                can_undo: !self.annots.is_empty() || self.text.is_some(),
                can_redo: !self.redo.is_empty(),
                ocr: self.ocr_available,
            };
            bar.draw(out, &st);
            if let Some(item) = self.hover
                && let Some(r) = bar.button_rect(item)
            {
                let mut label = item.label();
                if item == Item::Action(Action::Ocr) && !self.ocr_available {
                    label.push_str("  ·  unavailable");
                }
                self.render_tooltip(out, &label, r, &bar);
            }
        }
    }

    fn frame(&self, out: &mut Pixmap, r: IRect, w: f32) {
        if let Some(rect) =
            tiny_skia::Rect::from_xywh(r.x as f32 + w / 2.0, r.y as f32 + w / 2.0, (r.w as f32 - w).max(1.0), (r.h as f32 - w).max(1.0))
        {
            let path = tiny_skia::PathBuilder::from_rect(rect);
            let stroke = tiny_skia::Stroke { width: w, ..Default::default() };
            let mut paint = tiny_skia::Paint::default();
            paint.set_color(style::skia(style::ACCENT));
            out.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }

    fn pill(&self, out: &mut Pixmap, text: &str, x: f32, y: f32, px: f32) -> Option<IRect> {
        let f = self.font.as_ref()?;
        let (tw, th) = f.measure(text, px);
        let (padx, pady) = (self.ui(8.0), self.ui(4.0));
        let r = IRect::new(x.round() as i32, y.round() as i32, (tw + padx * 2.0).ceil() as i32, (th + pady * 2.0).ceil() as i32);
        toolbar::panel(out, r, self.ui(6.0));
        f.draw(out, text, x + padx, y + pady, px, style::ICON);
        Some(r)
    }

    fn pill_size(&self, text: &str, px: f32) -> (f32, f32) {
        let Some(f) = self.font.as_ref() else { return (0.0, 0.0) };
        let (tw, th) = f.measure(text, px);
        (tw + self.ui(16.0), th + self.ui(8.0))
    }

    fn render_size_label(&self, out: &mut Pixmap, s: IRect) {
        let label = format!("{} × {}", s.w, s.h);
        let px = self.ui(13.0);
        let (_, h) = self.pill_size(&label, px);
        let gap = self.ui(6.0);
        let y = if s.y as f32 - gap - h >= 0.0 { s.y as f32 - gap - h } else { s.y as f32 + gap };
        let x = if s.y as f32 - gap - h >= 0.0 { s.x as f32 } else { s.x as f32 + gap };
        self.pill(out, &label, x, y, px);
    }

    fn render_tooltip(&self, out: &mut Pixmap, label: &str, btn: IRect, bar: &Bar) {
        let px = self.ui(13.0);
        let (w, h) = self.pill_size(label, px);
        let gap = self.ui(6.0);
        let mut x = btn.x as f32 + btn.w as f32 / 2.0 - w / 2.0;
        x = x.clamp(0.0, (self.screen.w as f32 - w).max(0.0));
        let above = bar.rect.y as f32 - gap - h;
        let y = if above >= 0.0 { above } else { bar.rect.bottom() as f32 + gap };
        self.pill(out, label, x, y, px);
    }

    fn render_hint(&self, out: &mut Pixmap) {
        let text = "Drag to select  ·  Click a window  ·  Ctrl+A full screen  ·  Esc to cancel";
        let px = self.ui(14.0);
        let (w, _) = self.pill_size(text, px);
        let x = (self.screen.w as f32 - w) / 2.0;
        self.pill(out, text, x, self.ui(24.0), px);
    }

    fn render_crosshair(&self, out: &mut Pixmap) {
        let Some(p) = self.cursor else { return };
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(style::skia([255, 255, 255, 70]));
        paint.anti_alias = false;
        let (x, y) = (p.x.floor(), p.y.floor());
        if let Some(r) = tiny_skia::Rect::from_xywh(0.0, y, self.screen.w as f32, 1.0) {
            out.fill_rect(r, &paint, Transform::identity(), None);
        }
        if let Some(r) = tiny_skia::Rect::from_xywh(x, 0.0, 1.0, self.screen.h as f32) {
            out.fill_rect(r, &paint, Transform::identity(), None);
        }
    }

    fn render_text_edit(&self, out: &mut Pixmap, t: &TextEdit) {
        let Some(f) = self.font.as_ref() else { return };
        let px = self.width_for(Tool::Text);
        let (end_x, end_y) = f.draw(out, &t.text, t.at.x, t.at.y, px, self.color());
        let lh = f.line_height(px);
        let (w, h) = f.measure(&t.text, px);
        // Dashed-looking box: a faint outline around what is being typed.
        let pad = self.ui(4.0);
        if let Some(rect) = tiny_skia::Rect::from_xywh(t.at.x - pad, t.at.y - pad, w.max(px * 0.6) + pad * 2.0, h.max(lh) + pad * 2.0) {
            let path = tiny_skia::PathBuilder::from_rect(rect);
            let stroke = tiny_skia::Stroke { width: 1.0, dash: tiny_skia::StrokeDash::new(vec![4.0, 3.0], 0.0), ..Default::default() };
            let mut paint = tiny_skia::Paint::default();
            paint.set_color(style::skia(style::ACCENT));
            out.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
        let cx = if t.text.is_empty() { t.at.x } else { end_x };
        let cy = if t.text.is_empty() { t.at.y } else { end_y };
        if let Some(r) = tiny_skia::Rect::from_xywh(cx + 1.0, cy + lh * 0.1, self.ui(1.5).max(1.0), lh * 0.85) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color(style::skia(self.color()));
            paint.anti_alias = false;
            out.fill_rect(r, &paint, Transform::identity(), None);
        }
    }
}

fn handle_cursor(h: Handle) -> CursorKind {
    match h {
        Handle::NW | Handle::SE => CursorKind::ResizeNwse,
        Handle::NE | Handle::SW => CursorKind::ResizeNesw,
        Handle::E | Handle::W => CursorKind::ResizeEw,
        Handle::N | Handle::S => CursorKind::ResizeNs,
    }
}

/// Copy `r` of `src` onto the same place in `dst` (both the same size).
fn blit(dst: &mut Pixmap, src: &Pixmap, r: IRect) {
    let bounds = IRect::new(0, 0, src.width() as i32, src.height() as i32);
    let r = r.intersect(&bounds);
    if r.is_empty() {
        return;
    }
    let stride = src.width() as usize * 4;
    let (x0, x1) = (r.x as usize * 4, r.right() as usize * 4);
    let s = src.data();
    let d = dst.data_mut();
    for y in r.y as usize..r.bottom() as usize {
        let o = y * stride;
        d[o + x0..o + x1].copy_from_slice(&s[o + x0..o + x1]);
    }
}

pub fn crop(src: &Pixmap, r: IRect) -> Option<Pixmap> {
    let r = r.intersect(&IRect::new(0, 0, src.width() as i32, src.height() as i32));
    if r.is_empty() {
        return None;
    }
    let mut out = Pixmap::new(r.w as u32, r.h as u32)?;
    out.draw_pixmap(-r.x, -r.y, src.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    Some(out)
}

/// Build a pixmap from straight RGBA bytes (alpha forced opaque: screen
/// captures sometimes carry garbage alpha).
pub fn pixmap_from_rgba(mut rgba: Vec<u8>, w: u32, h: u32) -> Option<Pixmap> {
    for px in rgba.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
    Pixmap::from_vec(rgba, IntSize::from_wh(w, h)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(w: u32, h: u32) -> Editor {
        let mut pm = Pixmap::new(w, h).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(200, 200, 200, 255));
        Editor::new(pm, 1.0, None, vec![IRect::new(100, 100, 300, 200), IRect::new(0, 0, w as i32, h as i32)])
    }

    fn drag(e: &mut Editor, a: (f32, f32), b: (f32, f32)) {
        e.mouse_down(Button::Left, Pt::new(a.0, a.1));
        e.mouse_move(Pt::new((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0));
        e.mouse_move(Pt::new(b.0, b.1));
        e.mouse_up(Button::Left, Pt::new(b.0, b.1));
    }

    #[test]
    fn drag_selects_region() {
        let mut e = editor(800, 600);
        drag(&mut e, (50.0, 60.0), (250.0, 160.0));
        assert_eq!(e.selection(), Some(IRect::new(50, 60, 200, 100)));
        let img = e.export().unwrap();
        assert_eq!((img.width(), img.height()), (200, 100));
    }

    #[test]
    fn click_picks_window_then_screen() {
        let mut e = editor(800, 600);
        e.mouse_down(Button::Left, Pt::new(150.0, 150.0));
        e.mouse_up(Button::Left, Pt::new(150.0, 150.0));
        assert_eq!(e.selection(), Some(IRect::new(100, 100, 300, 200)));
        e.reset();
        e.mouse_down(Button::Left, Pt::new(700.0, 500.0));
        e.mouse_up(Button::Left, Pt::new(700.0, 500.0));
        assert_eq!(e.selection(), Some(IRect::new(0, 0, 800, 600)));
    }

    #[test]
    fn selection_is_clamped_to_screen() {
        let mut e = editor(800, 600);
        drag(&mut e, (700.0, 500.0), (900.0, 700.0));
        assert_eq!(e.selection(), Some(IRect::new(700, 500, 100, 100)));
    }

    #[test]
    fn move_and_resize_selection() {
        let mut e = editor(800, 600);
        drag(&mut e, (100.0, 100.0), (300.0, 200.0));
        drag(&mut e, (200.0, 150.0), (250.0, 170.0));
        assert_eq!(e.selection(), Some(IRect::new(150, 120, 200, 100)));
        // Bottom-right handle.
        drag(&mut e, (350.0, 220.0), (400.0, 260.0));
        assert_eq!(e.selection(), Some(IRect::new(150, 120, 250, 140)));
        // Can't be dragged off screen.
        drag(&mut e, (200.0, 150.0), (-500.0, -500.0));
        assert_eq!(e.selection().unwrap().x, 0);
        assert_eq!(e.selection().unwrap().y, 0);
    }

    #[test]
    fn draw_undo_redo() {
        let mut e = editor(800, 600);
        drag(&mut e, (100.0, 100.0), (500.0, 400.0));
        e.key(Key::Char('r'), Mods::default());
        assert_eq!(e.tool(), Tool::Rect);
        drag(&mut e, (150.0, 150.0), (300.0, 250.0));
        assert_eq!(e.annotations().len(), 1);
        let before = e.export().unwrap();
        let ctrl = Mods { ctrl: true, ..Mods::default() };
        e.key(Key::Char('z'), ctrl);
        assert!(e.annotations().is_empty());
        let undone = e.export().unwrap();
        assert_ne!(before.data(), undone.data());
        e.key(Key::Char('y'), ctrl);
        assert_eq!(e.export().unwrap().data(), before.data());
    }

    #[test]
    fn drawing_is_clamped_inside_selection() {
        let mut e = editor(800, 600);
        drag(&mut e, (100.0, 100.0), (300.0, 300.0));
        e.key(Key::Char('l'), Mods::default());
        drag(&mut e, (150.0, 150.0), (700.0, 150.0));
        match &e.annotations()[0].shape {
            Shape::Line(_, b) => assert_eq!(b.x, 300.0),
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn tiny_drags_are_discarded() {
        let mut e = editor(800, 600);
        drag(&mut e, (100.0, 100.0), (300.0, 300.0));
        e.key(Key::Char('a'), Mods::default());
        e.mouse_down(Button::Left, Pt::new(150.0, 150.0));
        e.mouse_up(Button::Left, Pt::new(150.5, 150.0));
        assert!(e.annotations().is_empty());
    }

    #[test]
    fn highlighter_keeps_its_own_colour() {
        let mut e = editor(800, 600);
        drag(&mut e, (100.0, 100.0), (500.0, 400.0));
        e.key(Key::Char('h'), Mods::default());
        drag(&mut e, (150.0, 150.0), (300.0, 150.0));
        e.key(Key::Char('5'), Mods::default());
        e.key(Key::Char('p'), Mods::default());
        drag(&mut e, (150.0, 200.0), (300.0, 200.0));
        assert_eq!(e.annotations()[0].color, PALETTE[2]);
        assert_eq!(e.annotations()[1].color, PALETTE[0]);
        // '5' while highlighting changed only the highlighter's colour.
        assert_eq!(e.style(), Style { color: PALETTE[0], marker: PALETTE[4], size: 2 });
    }

    #[test]
    fn steps_number_themselves() {
        let mut e = editor(800, 600);
        drag(&mut e, (100.0, 100.0), (500.0, 400.0));
        e.key(Key::Char('n'), Mods::default());
        for x in [150.0, 200.0, 250.0] {
            e.mouse_down(Button::Left, Pt::new(x, 150.0));
            e.mouse_up(Button::Left, Pt::new(x, 150.0));
        }
        let ns: Vec<u32> = e.annotations().iter().map(|a| if let Shape::Step { n, .. } = a.shape { n } else { 0 }).collect();
        assert_eq!(ns, vec![1, 2, 3]);
    }

    #[test]
    fn typing_goes_to_text_not_shortcuts() {
        let mut e = editor(800, 600);
        drag(&mut e, (100.0, 100.0), (500.0, 400.0));
        e.key(Key::Char('t'), Mods::default());
        e.mouse_down(Button::Left, Pt::new(150.0, 150.0));
        e.mouse_up(Button::Left, Pt::new(150.0, 150.0));
        e.key(Key::Char('p'), Mods::default());
        e.text_input("pa");
        e.key(Key::Backspace, Mods::default());
        e.text_input("int\u{8}");
        assert_eq!(e.tool(), Tool::Text);
        assert_eq!(e.key(Key::Escape, Mods::default()), None, "first Esc commits text");
        match &e.annotations()[0].shape {
            Shape::Text { text, .. } => assert_eq!(text, "pint"),
            s => panic!("{s:?}"),
        }
        assert_eq!(e.key(Key::Escape, Mods::default()), Some(Command::Cancel));
    }

    #[test]
    fn commands_from_keys_and_toolbar() {
        let mut e = editor(800, 600);
        assert_eq!(e.key(Key::Enter, Mods::default()), None, "nothing to copy yet");
        drag(&mut e, (100.0, 100.0), (500.0, 300.0));
        assert_eq!(e.key(Key::Enter, Mods::default()), Some(Command::Copy));
        let ctrl = Mods { ctrl: true, ..Mods::default() };
        assert_eq!(e.key(Key::Char('s'), ctrl), Some(Command::Save));
        assert_eq!(e.key(Key::Char('s'), Mods { shift: true, ..ctrl }), Some(Command::QuickSave));
        assert_eq!(e.key(Key::Char('p'), ctrl), Some(Command::Pin));
        assert_eq!(e.key(Key::Char('t'), ctrl), None, "OCR off unless the platform has it");
        let bar = e.bar().unwrap();
        let pin = bar.button_rect(Item::Action(Action::Pin)).unwrap();
        let c = Pt::new(pin.x as f32 + 5.0, pin.y as f32 + 5.0);
        assert_eq!(e.mouse_down(Button::Left, c), Some(Command::Pin));
        assert_eq!(e.double_click(Pt::new(200.0, 200.0)), Some(Command::Copy));
    }

    #[test]
    fn right_click_steps_back() {
        let mut e = editor(800, 600);
        drag(&mut e, (100.0, 100.0), (500.0, 300.0));
        assert_eq!(e.mouse_down(Button::Right, Pt::new(10.0, 10.0)), None);
        assert_eq!(e.selection(), None);
        assert_eq!(e.mouse_down(Button::Right, Pt::new(10.0, 10.0)), Some(Command::Cancel));
    }

    #[test]
    fn render_does_not_panic_in_any_state() {
        let mut e = editor(640, 480);
        let mut out = Pixmap::new(640, 480).unwrap();
        e.mouse_move(Pt::new(150.0, 150.0));
        e.render(&mut out);
        drag(&mut e, (0.0, 0.0), (640.0, 480.0));
        e.render(&mut out);
        for t in ['p', 'h', 'l', 'a', 'r', 'e', 'b'] {
            e.key(Key::Char(t), Mods::default());
            e.mouse_down(Button::Left, Pt::new(10.0, 10.0));
            e.mouse_move(Pt::new(630.0, 470.0));
            e.render(&mut out);
            e.mouse_up(Button::Left, Pt::new(630.0, 470.0));
        }
        e.render(&mut out);
        assert_eq!(e.annotations().len(), 7);
    }
}

//! The editing toolbar: layout around the selection, hit testing, and its
//! vector icons (drawn in code, so there are no image assets to ship).

use tiny_skia::{BlendMode, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};

use crate::geom::{IRect, Pt};
use crate::style::{self, Color, PALETTE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Pen,
    Marker,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Text,
    Blur,
    Step,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Undo,
    Redo,
    Ocr,
    Pin,
    Save,
    Copy,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Tool(Tool),
    Color,
    Swatch(usize),
    Action(Action),
}

const TOOLS: [Tool; 10] =
    [Tool::Select, Tool::Pen, Tool::Marker, Tool::Line, Tool::Arrow, Tool::Rect, Tool::Ellipse, Tool::Text, Tool::Blur, Tool::Step];

impl Tool {
    pub fn shortcut(self) -> char {
        match self {
            Tool::Select => 'v',
            Tool::Pen => 'p',
            Tool::Marker => 'h',
            Tool::Line => 'l',
            Tool::Arrow => 'a',
            Tool::Rect => 'r',
            Tool::Ellipse => 'e',
            Tool::Text => 't',
            Tool::Blur => 'b',
            Tool::Step => 'n',
        }
    }

    pub fn from_shortcut(c: char) -> Option<Tool> {
        TOOLS.into_iter().find(|t| t.shortcut() == c.to_ascii_lowercase())
    }
}

impl Item {
    pub fn label(self) -> String {
        match self {
            Item::Tool(t) => {
                let name = match t {
                    Tool::Select => "Move / resize",
                    Tool::Pen => "Pen",
                    Tool::Marker => "Highlighter",
                    Tool::Line => "Line",
                    Tool::Arrow => "Arrow",
                    Tool::Rect => "Rectangle",
                    Tool::Ellipse => "Ellipse",
                    Tool::Text => "Text",
                    Tool::Blur => "Blur",
                    Tool::Step => "Numbered step",
                };
                format!("{name}  ({})", t.shortcut().to_ascii_uppercase())
            }
            Item::Color => "Colour  (1-8)  ·  size: wheel or [ ]".into(),
            Item::Swatch(i) => format!("Colour {}", i + 1),
            Item::Action(a) => match a {
                Action::Undo => "Undo  (Ctrl+Z)",
                Action::Redo => "Redo  (Ctrl+Y)",
                Action::Ocr => "Copy text (OCR)  (Ctrl+T)",
                Action::Pin => "Pin to screen  (Ctrl+P)",
                Action::Save => "Save  (Ctrl+S)",
                Action::Copy => "Copy  (Ctrl+C / Enter)",
                Action::Close => "Close  (Esc)",
            }
            .into(),
        }
    }
}

/// What the bar needs to know to draw itself.
pub struct BarState {
    pub tool: Tool,
    pub color: Color,
    /// Highlighter colour, shown in its icon.
    pub marker: Color,
    pub size: u32,
    pub hover: Option<Item>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub ocr: bool,
}

pub struct Bar {
    pub rect: IRect,
    pub buttons: Vec<(Item, IRect)>,
    pub palette: Option<(IRect, Vec<(Item, IRect)>)>,
    btn: i32,
}

impl Bar {
    pub fn layout(sel: IRect, screen: IRect, scale: f32, palette_open: bool) -> Bar {
        let s = |v: f32| (v * scale).round() as i32;
        let (btn, pad, sep, gap) = (s(34.0), s(4.0), s(9.0), s(8.0));
        let mut groups: Vec<Vec<Item>> = vec![TOOLS.iter().map(|t| Item::Tool(*t)).collect(), vec![Item::Color]];
        groups.push(vec![Item::Action(Action::Undo), Item::Action(Action::Redo)]);
        groups.push([Action::Ocr, Action::Pin, Action::Save, Action::Copy, Action::Close].map(Item::Action).to_vec());
        let n: i32 = groups.iter().map(|g| g.len() as i32).sum();
        let w = n * btn + (groups.len() as i32 - 1) * sep + pad * 2;
        let h = btn + pad * 2;

        let x = (sel.right() - w).clamp(screen.x, (screen.right() - w).max(screen.x));
        let below = sel.bottom() + gap;
        let above = sel.y - gap - h;
        let y = if below + h <= screen.bottom() {
            below
        } else if above >= screen.y {
            above
        } else {
            (sel.bottom() - gap - h).max(screen.y)
        };
        let rect = IRect::new(x, y, w, h);

        let mut buttons = Vec::new();
        let mut bx = x + pad;
        for (gi, g) in groups.iter().enumerate() {
            if gi > 0 {
                bx += sep;
            }
            for it in g {
                buttons.push((*it, IRect::new(bx, y + pad, btn, btn)));
                bx += btn;
            }
        }

        let palette = palette_open.then(|| {
            let sw = btn;
            let pw = PALETTE.len() as i32 * sw + pad * 2;
            let color_btn = buttons.iter().find(|(i, _)| *i == Item::Color).map(|(_, r)| *r).unwrap_or(rect);
            let px = (color_btn.x - pad).clamp(screen.x, (screen.right() - pw).max(screen.x));
            let under = rect.bottom() + s(4.0);
            let py = if rect.y > sel.y && under + h <= screen.bottom() { under } else { (rect.y - s(4.0) - h).max(screen.y) };
            let prect = IRect::new(px, py, pw, h);
            let sw_rects = (0..PALETTE.len()).map(|i| (Item::Swatch(i), IRect::new(px + pad + i as i32 * sw, py + pad, sw, sw))).collect();
            (prect, sw_rects)
        });

        Bar { rect, buttons, palette, btn }
    }

    pub fn contains(&self, p: Pt) -> bool {
        self.rect.contains(p) || self.palette.as_ref().is_some_and(|(r, _)| r.contains(p))
    }

    pub fn hit(&self, p: Pt) -> Option<Item> {
        let pal = self.palette.iter().flat_map(|(_, v)| v.iter());
        pal.chain(self.buttons.iter()).find(|(_, r)| r.contains(p)).map(|(i, _)| *i)
    }

    pub fn button_rect(&self, item: Item) -> Option<IRect> {
        let pal = self.palette.iter().flat_map(|(_, v)| v.iter());
        pal.chain(self.buttons.iter()).find(|(i, _)| *i == item).map(|(_, r)| *r)
    }

    pub fn draw(&self, pm: &mut Pixmap, st: &BarState) {
        let radius = self.btn as f32 * 0.28;
        panel(pm, self.rect, radius);
        for (item, r) in &self.buttons {
            let enabled = match item {
                Item::Action(Action::Undo) => st.can_undo,
                Item::Action(Action::Redo) => st.can_redo,
                Item::Action(Action::Ocr) => st.ocr,
                _ => true,
            };
            let active = matches!(item, Item::Tool(t) if *t == st.tool);
            if active {
                round_rect(pm, inset(*r, 2), radius * 0.7, style::ACCENT);
            } else if enabled && st.hover == Some(*item) {
                round_rect(pm, inset(*r, 2), radius * 0.7, style::BAR_HOVER);
            }
            let color = if enabled { style::ICON } else { style::ICON_DISABLED };
            draw_icon(pm, *item, *r, color, st);
        }
        if let Some((prect, swatches)) = &self.palette {
            panel(pm, *prect, radius);
            for (item, r) in swatches {
                let Item::Swatch(i) = item else { continue };
                let c = PALETTE[*i];
                let (cx, cy) = (r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0);
                let rad = r.w as f32 * 0.28;
                if let Some(circle) = PathBuilder::from_circle(cx, cy, rad) {
                    fill_path(pm, &circle, c);
                    let selected = c == st.color;
                    let ring = if selected || st.hover == Some(*item) { style::ICON } else { [255, 255, 255, 70] };
                    stroke_path(pm, &circle, ring, if selected { 2.5 } else { 1.0 } * r.w as f32 / 34.0, Transform::identity());
                }
            }
        }
    }
}

pub fn inset(r: IRect, d: i32) -> IRect {
    IRect::new(r.x + d, r.y + d, r.w - 2 * d, r.h - 2 * d)
}

/// Dark rounded panel with a hairline border, the backdrop of all chrome.
pub fn panel(pm: &mut Pixmap, r: IRect, radius: f32) {
    round_rect(pm, r, radius, style::BAR_BG);
    if let Some(p) = round_rect_path(r, radius) {
        stroke_path(pm, &p, [255, 255, 255, 28], 1.0, Transform::identity());
    }
}

pub fn round_rect(pm: &mut Pixmap, r: IRect, radius: f32, c: Color) {
    if let Some(p) = round_rect_path(r, radius) {
        fill_path(pm, &p, c);
    }
}

pub fn round_rect_path(r: IRect, radius: f32) -> Option<Path> {
    let (x, y, w, h) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
    let rad = radius.min(w / 2.0).min(h / 2.0);
    let k = rad * 0.4477; // cubic approximation of a quarter circle
    let mut pb = PathBuilder::new();
    pb.move_to(x + rad, y);
    pb.line_to(x + w - rad, y);
    pb.cubic_to(x + w - k, y, x + w, y + k, x + w, y + rad);
    pb.line_to(x + w, y + h - rad);
    pb.cubic_to(x + w, y + h - k, x + w - k, y + h, x + w - rad, y + h);
    pb.line_to(x + rad, y + h);
    pb.cubic_to(x + k, y + h, x, y + h - k, x, y + h - rad);
    pb.line_to(x, y + rad);
    pb.cubic_to(x, y + k, x + k, y, x + rad, y);
    pb.close();
    pb.finish()
}

fn color_paint(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(style::skia(c));
    p.anti_alias = true;
    p.blend_mode = BlendMode::SourceOver;
    p
}

fn fill_path(pm: &mut Pixmap, path: &Path, c: Color) {
    pm.fill_path(path, &color_paint(c), FillRule::Winding, Transform::identity(), None);
}

fn stroke_path(pm: &mut Pixmap, path: &Path, c: Color, w: f32, t: Transform) {
    let s = Stroke { width: w, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    pm.stroke_path(path, &color_paint(c), &s, t, None);
}

/// Tiny SVG-like path language for icons: `M x y`, `L x y`, `C x1 y1 x2 y2 x y`, `Z`.
fn icon_path(d: &str) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let mut it = d.split_whitespace();
    let num = |it: &mut std::str::SplitWhitespace| it.next().and_then(|v| v.parse::<f32>().ok());
    while let Some(cmd) = it.next() {
        match cmd {
            "M" => pb.move_to(num(&mut it)?, num(&mut it)?),
            "L" => pb.line_to(num(&mut it)?, num(&mut it)?),
            "C" => {
                let v: Vec<f32> = (0..6).map(|_| num(&mut it)).collect::<Option<_>>()?;
                pb.cubic_to(v[0], v[1], v[2], v[3], v[4], v[5]);
            }
            "Z" => pb.close(),
            _ => return None,
        }
    }
    pb.finish()
}

/// Icons are designed on a 24×24 grid and drawn centred in the button.
fn draw_icon(pm: &mut Pixmap, item: Item, r: IRect, c: Color, st: &BarState) {
    let k = r.w as f32 / 34.0;
    let t = Transform::from_row(k, 0.0, 0.0, k, r.x as f32 + 5.0 * k, r.y as f32 + 5.0 * k);
    let line = |pm: &mut Pixmap, d: &str, w: f32| {
        if let Some(p) = icon_path(d) {
            stroke_path(pm, &p, c, w, t);
        }
    };
    let solid = |pm: &mut Pixmap, d: &str, col: Color| {
        if let Some(p) = icon_path(d) {
            pm.fill_path(&p, &color_paint(col), FillRule::Winding, t, None);
        }
    };
    const W: f32 = 1.8;
    match item {
        Item::Tool(Tool::Select) => line(pm, "M 7 3 L 7 19 L 11 15 L 14 21 L 16.5 20 L 13.5 14 L 19 14 Z", W),
        Item::Tool(Tool::Pen) => {
            line(pm, "M 4 20 L 5 15 L 16 4 L 20 8 L 9 19 Z", W);
            line(pm, "M 13.5 6.5 L 17.5 10.5", W);
        }
        Item::Tool(Tool::Marker) => {
            solid(pm, "M 5 15 L 15 5 L 19 9 L 9 19 Z", style::with_alpha(st.marker, 210));
            line(pm, "M 5 15 L 15 5 L 19 9 L 9 19 Z", 1.2);
            line(pm, "M 3 21 L 9 21", W);
        }
        Item::Tool(Tool::Line) => line(pm, "M 5 19 L 19 5", W),
        Item::Tool(Tool::Arrow) => line(pm, "M 5 19 L 18 6 M 10 6 L 18 6 L 18 14", W),
        Item::Tool(Tool::Rect) => line(pm, "M 4 6 L 20 6 L 20 18 L 4 18 Z", W),
        Item::Tool(Tool::Ellipse) => {
            if let Some(p) = Rect::from_xywh(3.0, 6.0, 18.0, 12.0).and_then(PathBuilder::from_oval) {
                stroke_path(pm, &p, c, W, t);
            }
        }
        Item::Tool(Tool::Text) => line(pm, "M 6 6 L 18 6 M 12 6 L 12 19 M 9.5 19 L 14.5 19", W),
        Item::Tool(Tool::Blur) => {
            for (i, (x, y)) in [(4, 4), (10, 4), (16, 4), (4, 10), (10, 10), (16, 10), (4, 16), (10, 16), (16, 16)].iter().enumerate() {
                let a = [230, 120, 60, 120, 60, 120, 60, 120, 230][i];
                let (x, y) = (*x as f32, *y as f32);
                solid(
                    pm,
                    &format!("M {x} {y} L {} {y} L {} {} L {x} {} Z", x + 4.5, x + 4.5, y + 4.5, y + 4.5),
                    style::with_alpha(c, a.min(c[3])),
                );
            }
        }
        Item::Tool(Tool::Step) => {
            if let Some(p) = PathBuilder::from_circle(12.0, 12.0, 8.5) {
                stroke_path(pm, &p, c, W, t);
            }
            line(pm, "M 10.5 9.5 L 12.8 7.8 L 12.8 16.5", W);
        }
        Item::Color => {
            let rad = 4.0 + st.size.min(10) as f32 * 0.55;
            if let Some(p) = PathBuilder::from_circle(12.0, 12.0, rad) {
                pm.fill_path(&p, &color_paint(st.color), FillRule::Winding, t, None);
                stroke_path(pm, &p, [255, 255, 255, 200], 1.2, t);
            }
        }
        Item::Swatch(_) => {}
        Item::Action(a) => match a {
            Action::Undo => line(pm, "M 8 5 L 4 9 L 8 13 M 4 9 L 14 9 C 17.5 9 20 11.5 20 14.5 C 20 17.5 17.5 20 14.5 20 L 10 20", W),
            Action::Redo => line(pm, "M 16 5 L 20 9 L 16 13 M 20 9 L 10 9 C 6.5 9 4 11.5 4 14.5 C 4 17.5 6.5 20 9.5 20 L 14 20", W),
            Action::Ocr => {
                line(pm, "M 4 8 L 4 4 L 8 4 M 16 4 L 20 4 L 20 8 M 20 16 L 20 20 L 16 20 M 8 20 L 4 20 L 4 16", W);
                line(pm, "M 8.5 9 L 15.5 9 M 12 9 L 12 16", W);
            }
            Action::Pin => line(pm, "M 8.5 4 L 15.5 4 M 10 4 L 10 10 L 6.5 14 L 17.5 14 L 14 10 L 14 4 M 12 14 L 12 21", W),
            Action::Save => line(pm, "M 12 3.5 L 12 14 M 7.5 9.5 L 12 14 L 16.5 9.5 M 4.5 15 L 4.5 20 L 19.5 20 L 19.5 15", W),
            Action::Copy => {
                line(pm, "M 9 8 L 20 8 L 20 20 L 9 20 Z", W);
                line(pm, "M 15 8 L 15 4 L 4 4 L 4 16 L 9 16", W);
            }
            Action::Close => {
                if let Some(p) = icon_path("M 6.5 6.5 L 17.5 17.5 M 17.5 6.5 L 6.5 17.5") {
                    stroke_path(pm, &p, if c == style::ICON { style::DANGER } else { c }, 2.2, t);
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: IRect = IRect::new(0, 0, 1920, 1080);

    #[test]
    fn bar_sits_below_selection_right_aligned() {
        let sel = IRect::new(400, 200, 900, 400);
        let bar = Bar::layout(sel, SCREEN, 1.0, false);
        assert_eq!(bar.rect.right(), sel.right());
        assert!(bar.rect.y >= sel.bottom());
    }

    #[test]
    fn bar_flips_above_then_inside_when_no_room() {
        let bar = Bar::layout(IRect::new(400, 300, 900, 770), SCREEN, 1.0, false);
        assert!(bar.rect.bottom() <= 300, "above: {:?}", bar.rect);
        let full = Bar::layout(SCREEN, SCREEN, 1.0, true);
        assert!(full.rect.y >= 0 && full.rect.bottom() <= 1080);
        let (prect, _) = full.palette.as_ref().unwrap();
        assert!(prect.y >= 0 && prect.bottom() <= 1080);
    }

    #[test]
    fn bar_is_clamped_on_screen_horizontally() {
        let bar = Bar::layout(IRect::new(0, 100, 50, 50), SCREEN, 1.0, false);
        assert_eq!(bar.rect.x, 0);
    }

    #[test]
    fn hit_finds_each_button() {
        let bar = Bar::layout(IRect::new(400, 200, 900, 400), SCREEN, 1.5, true);
        for (item, r) in bar.buttons.iter().chain(bar.palette.as_ref().unwrap().1.iter()) {
            let c = Pt::new(r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0);
            assert_eq!(bar.hit(c), Some(*item));
        }
    }

    #[test]
    fn shortcuts_round_trip() {
        for t in TOOLS {
            assert_eq!(Tool::from_shortcut(t.shortcut()), Some(t));
        }
    }
}

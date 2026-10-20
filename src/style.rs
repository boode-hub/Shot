//! Colours and the shared look of the UI.

/// Straight-alpha RGBA.
pub type Color = [u8; 4];

/// Annotation palette, in toolbar order.
pub const PALETTE: [Color; 8] = [
    [0xFF, 0x3B, 0x30, 0xFF], // red
    [0xFF, 0x95, 0x00, 0xFF], // orange
    [0xFF, 0xD6, 0x0A, 0xFF], // yellow
    [0x34, 0xC7, 0x59, 0xFF], // green
    [0x0A, 0x84, 0xFF, 0xFF], // blue
    [0xBF, 0x5A, 0xF2, 0xFF], // purple
    [0xFF, 0xFF, 0xFF, 0xFF], // white
    [0x1C, 0x1C, 0x1E, 0xFF], // black
];

pub const ACCENT: Color = [0x0A, 0x84, 0xFF, 0xFF];
pub const BAR_BG: Color = [0x1F, 0x1F, 0x23, 0xF2];
pub const BAR_HOVER: Color = [0xFF, 0xFF, 0xFF, 0x1F];
pub const ICON: Color = [0xF2, 0xF2, 0xF7, 0xFF];
pub const ICON_DISABLED: Color = [0xF2, 0xF2, 0xF7, 0x55];
pub const DANGER: Color = [0xFF, 0x45, 0x3A, 0xFF];

/// How much the unselected screen is darkened (0 = none, 1 = black).
pub const DIM: f32 = 0.45;

pub fn skia(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c[0], c[1], c[2], c[3])
}

pub fn with_alpha(c: Color, a: u8) -> Color {
    [c[0], c[1], c[2], a]
}

/// Readable text colour (black or white) on top of `bg`.
pub fn contrast(bg: Color) -> Color {
    let l = 0.299 * bg[0] as f32 + 0.587 * bg[1] as f32 + 0.114 * bg[2] as f32;
    if l > 160.0 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }
}

/// `#rrggbb` or `#rrggbbaa`.
pub fn parse_hex(s: &str) -> Option<Color> {
    let h = s.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

pub fn to_hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(parse_hex("#ff3b30"), Some([0xFF, 0x3B, 0x30, 0xFF]));
        assert_eq!(parse_hex("0a84ff80"), Some([0x0A, 0x84, 0xFF, 0x80]));
        assert_eq!(parse_hex("#xyz"), None);
        assert_eq!(to_hex(PALETTE[0]), "#ff3b30");
    }
}

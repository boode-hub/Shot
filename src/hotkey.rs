//! Capture shortcuts as text, e.g. `Shift+Space` or `Ctrl+Alt+KeyS`.
//! Key names are the W3C `KeyboardEvent.code` names used by both winit and
//! the global-hotkey crate, so a recorded key round-trips exactly.

pub const DEFAULT: &str = "Shift+Space";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Windows key (Cmd on macOS).
    pub meta: bool,
}

impl Modifiers {
    fn any(&self) -> bool {
        self.ctrl || self.alt || self.shift || self.meta
    }
}

/// Keys that are only modifiers: pressing them alone keeps recording.
pub fn is_modifier(code: &str) -> bool {
    matches!(
        code,
        "ShiftLeft"
            | "ShiftRight"
            | "ControlLeft"
            | "ControlRight"
            | "AltLeft"
            | "AltRight"
            | "SuperLeft"
            | "SuperRight"
            | "MetaLeft"
            | "MetaRight"
    )
}

/// Keys fine to use without any modifier: they type nothing.
fn standalone_ok(code: &str) -> bool {
    matches!(code, "PrintScreen" | "Pause" | "ScrollLock" | "Insert" | "Help")
        || code.strip_prefix('F').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Turn a key press into a shortcut, or explain why it can't be one.
pub fn from_press(m: Modifiers, code: &str) -> Result<String, String> {
    if code.is_empty() || code == "Unidentified" || code.starts_with("Unidentified") {
        return Err("That key can't be used. Try another.".into());
    }
    if !m.any() && !standalone_ok(code) {
        return Err(format!("{} alone would block typing. Add Ctrl, Alt, Shift or Win.", pretty_key(code)));
    }
    if m.shift && !m.ctrl && !m.alt && !m.meta && code.starts_with("Key") {
        return Err("Shift + a letter would block capital letters. Add Ctrl or Alt.".into());
    }
    let mut parts: Vec<&str> = Vec::new();
    if m.ctrl {
        parts.push("Ctrl");
    }
    if m.alt {
        parts.push("Alt");
    }
    if m.shift {
        parts.push("Shift");
    }
    if m.meta {
        parts.push("Super");
    }
    parts.push(code);
    Ok(parts.join("+"))
}

/// The shortcut as separate key labels for display: `["Shift", "Space"]`.
pub fn keycaps(hotkey: &str) -> Vec<String> {
    hotkey
        .split('+')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| match t.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "Ctrl".to_string(),
            "alt" | "option" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "super" | "meta" | "cmd" | "command" | "win" => (if cfg!(target_os = "macos") { "Cmd" } else { "Win" }).to_string(),
            _ => pretty_key(t),
        })
        .collect()
}

/// `Shift + Space`, `Ctrl + Alt + S`.
pub fn pretty(hotkey: &str) -> String {
    keycaps(hotkey).join(" + ")
}

pub fn pretty_key(code: &str) -> String {
    if let Some(c) = code.strip_prefix("Key").filter(|c| c.len() == 1) {
        return c.to_string();
    }
    if let Some(d) = code.strip_prefix("Digit").filter(|d| d.len() == 1) {
        return d.to_string();
    }
    if let Some(n) = code.strip_prefix("Numpad") {
        return format!("Num {n}");
    }
    if let Some(a) = code.strip_prefix("Arrow") {
        return a.to_string();
    }
    match code {
        "PrintScreen" => "Print Screen".into(),
        "PageUp" => "Page Up".into(),
        "PageDown" => "Page Down".into(),
        "ScrollLock" => "Scroll Lock".into(),
        "Backquote" => "`".into(),
        "Minus" => "-".into(),
        "Equal" => "=".into(),
        "BracketLeft" => "[".into(),
        "BracketRight" => "]".into(),
        "Backslash" => "\\".into(),
        "Semicolon" => ";".into(),
        "Quote" => "'".into(),
        "Comma" => ",".into(),
        "Period" => ".".into(),
        "Slash" => "/".into(),
        _ => code.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIFT: Modifiers = Modifiers { ctrl: false, alt: false, shift: true, meta: false };
    const CTRL: Modifiers = Modifiers { ctrl: true, alt: false, shift: false, meta: false };

    #[test]
    fn records_combos_in_a_fixed_order() {
        assert_eq!(from_press(SHIFT, "Space").unwrap(), "Shift+Space");
        let all = Modifiers { ctrl: true, alt: true, shift: true, meta: true };
        assert_eq!(from_press(all, "KeyS").unwrap(), "Ctrl+Alt+Shift+Super+KeyS");
        assert_eq!(from_press(Modifiers::default(), "PrintScreen").unwrap(), "PrintScreen");
        assert_eq!(from_press(Modifiers::default(), "F9").unwrap(), "F9");
    }

    #[test]
    fn refuses_shortcuts_that_break_typing() {
        assert!(from_press(Modifiers::default(), "KeyA").is_err());
        assert!(from_press(Modifiers::default(), "Space").is_err());
        assert!(from_press(SHIFT, "KeyA").is_err());
        assert!(from_press(Modifiers::default(), "Fn").is_err());
        assert!(from_press(CTRL, "KeyA").is_ok());
    }

    #[test]
    fn pretty_names() {
        assert_eq!(pretty("Shift+Space"), "Shift + Space");
        assert_eq!(pretty("Ctrl+Alt+KeyS"), "Ctrl + Alt + S");
        assert_eq!(pretty("PrintScreen"), "Print Screen");
        assert_eq!(keycaps("ctrl+Digit1"), vec!["Ctrl", "1"]);
    }

    #[test]
    fn modifier_keys_are_recognised() {
        assert!(is_modifier("ShiftLeft") && is_modifier("SuperRight"));
        assert!(!is_modifier("Space"));
    }
}

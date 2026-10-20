//! Settings: a tiny `key = value` file next to the other per-user app data.
//! Unknown keys are ignored and bad values fall back to defaults, so a typo
//! never stops Shot from starting.

use std::path::{Path, PathBuf};

use crate::editor::{MAX_SIZE, Style};
use crate::style::{parse_hex, to_hex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveMode {
    /// Show a Save dialog, starting in `save_dir`.
    Ask,
    /// Save straight into `save_dir` with a timestamped name.
    Folder,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub hotkey: String,
    pub save_dir: PathBuf,
    pub save_mode: SaveMode,
    /// Also copy the image to the clipboard when saving.
    pub copy_on_save: bool,
    pub style: Style,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            hotkey: "PrintScreen".into(),
            save_dir: pictures_dir().join("Shot"),
            save_mode: SaveMode::Ask,
            copy_on_save: false,
            style: Style::default(),
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Config {
        let mut c = Config::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim().trim_matches('"');
            match k.trim() {
                "hotkey" if !v.is_empty() => c.hotkey = v.to_string(),
                "save_dir" if !v.is_empty() => c.save_dir = expand_home(v),
                "save_mode" => {
                    c.save_mode = match v.to_ascii_lowercase().as_str() {
                        "folder" => SaveMode::Folder,
                        _ => SaveMode::Ask,
                    }
                }
                "copy_on_save" => c.copy_on_save = matches!(v.to_ascii_lowercase().as_str(), "true" | "yes" | "1"),
                "color" => c.style.color = parse_hex(v).unwrap_or(c.style.color),
                "highlighter" => c.style.marker = parse_hex(v).unwrap_or(c.style.marker),
                "size" => c.style.size = v.parse::<u32>().map(|s| s.clamp(1, MAX_SIZE)).unwrap_or(c.style.size),
                _ => {}
            }
        }
        c
    }

    pub fn render(&self) -> String {
        format!(
            "# Shot settings. Restart Shot after editing (tray icon -> Quit, then start it again).\n\
             \n\
             # Capture hotkey, e.g. PrintScreen, Ctrl+PrintScreen, Ctrl+Shift+KeyA, Alt+F1\n\
             hotkey = {}\n\
             \n\
             # Where screenshots are saved.\n\
             save_dir = {}\n\
             # ask = show a Save dialog; folder = save straight into save_dir\n\
             save_mode = {}\n\
             # Also copy the image to the clipboard when saving (true/false)\n\
             copy_on_save = {}\n\
             \n\
             # Last used drawing style (updated automatically).\n\
             color = {}\n\
             highlighter = {}\n\
             size = {}\n",
            self.hotkey,
            self.save_dir.display(),
            if self.save_mode == SaveMode::Folder { "folder" } else { "ask" },
            self.copy_on_save,
            to_hex(self.style.color),
            to_hex(self.style.marker),
            self.style.size,
        )
    }

    /// Read the settings file, creating it with defaults on first run.
    pub fn load(path: &Path) -> Config {
        match std::fs::read_to_string(path) {
            Ok(t) => Config::parse(&t),
            Err(_) => {
                let c = Config::default();
                let _ = c.save(path);
                c
            }
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.render())
    }
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

fn expand_home(v: &str) -> PathBuf {
    match v.strip_prefix("~/").or_else(|| v.strip_prefix("~\\")) {
        Some(rest) => home().join(rest),
        None => PathBuf::from(v),
    }
}

/// Per-user directory for Shot's own files (settings, lock file).
pub fn app_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(home).join("Shot")
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support/Shot")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("shot")
    }
}

pub fn pictures_dir() -> PathBuf {
    if !cfg!(windows) && !cfg!(target_os = "macos") {
        // Honour a localised XDG pictures folder when user-dirs.dirs says so.
        let dirs = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("user-dirs.dirs");
        if let Ok(t) = std::fs::read_to_string(dirs) {
            for line in t.lines() {
                if let Some(v) = line.strip_prefix("XDG_PICTURES_DIR=") {
                    let v = v.trim_matches('"').replace("$HOME", &home().to_string_lossy());
                    return PathBuf::from(v);
                }
            }
        }
    }
    home().join("Pictures")
}

/// `Shot_2026-10-20_14-03-07.png`, in local time.
pub fn file_name_now() -> String {
    chrono::Local::now().format("Shot_%Y-%m-%d_%H-%M-%S.png").to_string()
}

/// A path in `dir` that does not exist yet, based on `name`.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, "png"));
    (2..).map(|i| dir.join(format!("{stem}_{i}.{ext}"))).find(|p| !p.exists()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::PALETTE;

    #[test]
    fn round_trip() {
        let c = Config {
            hotkey: "Ctrl+Shift+KeyA".into(),
            save_mode: SaveMode::Folder,
            copy_on_save: true,
            style: Style { color: PALETTE[4], marker: PALETTE[3], size: 7 },
            ..Config::default()
        };
        assert_eq!(Config::parse(&c.render()), c);
    }

    #[test]
    fn junk_falls_back_to_defaults() {
        let c = Config::parse("hotkey =\nsize = 99\ncolor = nope\nwhat = ever\nno equals sign\n");
        let d = Config::default();
        assert_eq!(c.hotkey, d.hotkey);
        assert_eq!(c.style.size, MAX_SIZE);
        assert_eq!(c.style.color, d.style.color);
    }

    #[test]
    fn unique_path_skips_existing() {
        let dir = std::env::temp_dir().join(format!("shot-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.png"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "a.png"), dir.join("a_2.png"));
        assert_eq!(unique_path(&dir, "b.png"), dir.join("b.png"));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(file_name_now().starts_with("Shot_") && file_name_now().ends_with(".png"));
    }
}

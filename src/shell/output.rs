//! Where finished screenshots go: clipboard, files.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use tiny_skia::Pixmap;

pub fn straight_rgba(pm: &Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity(pm.data().len());
    for p in pm.pixels() {
        let c = p.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}

pub fn copy_image(cb: &mut arboard::Clipboard, pm: &Pixmap) -> Result<(), String> {
    let img = arboard::ImageData { width: pm.width() as usize, height: pm.height() as usize, bytes: Cow::Owned(straight_rgba(pm)) };
    cb.set_image(img).map_err(|e| e.to_string())
}

pub fn write_png(pm: &Pixmap, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let bytes = pm.encode_png().map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Ask where to save; `None` if the user cancelled.
pub fn ask_path(dir: &Path, name: &str) -> Option<PathBuf> {
    let _ = std::fs::create_dir_all(dir);
    let mut p = rfd::FileDialog::new()
        .set_title("Save screenshot")
        .set_directory(dir)
        .set_file_name(name)
        .add_filter("PNG image", &["png"])
        .save_file()?;
    if p.extension().is_none() {
        p.set_extension("png");
    }
    Some(p)
}

/// Ask for a folder; `None` if the user cancelled.
pub fn pick_folder(start: &Path) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title("Folder for screenshots").set_directory(start).pick_folder()
}

/// Open a file or folder with the system's default handler.
pub fn open(path: &Path) {
    #[cfg(windows)]
    let cmd = ("explorer", vec![path.as_os_str()]);
    #[cfg(target_os = "macos")]
    let cmd = ("open", vec![path.as_os_str()]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = ("xdg-open", vec![path.as_os_str()]);
    if let Err(e) = std::process::Command::new(cmd.0).args(cmd.1).spawn() {
        eprintln!("shot: cannot open {}: {e}", path.display());
    }
}

//! "Start Shot when I log in": installs a copy of Shot in the user's own
//! programs folder and registers it to run at login. Nothing needs admin
//! rights, and turning it off removes the registration.
//!
//! Windows: `%LOCALAPPDATA%\Programs\Shot\Shot.exe`, the per-user Run key, and
//! a Start Menu shortcut. macOS: a LaunchAgent. Linux: `~/.local/bin/shot`
//! and an XDG autostart entry.

use std::path::{Path, PathBuf};

/// Where the copy that runs at login lives. A portable Shot is never copied:
/// it is registered where it is.
pub fn install_path() -> PathBuf {
    if shot::config::portable_dir().is_some()
        && let Ok(me) = std::env::current_exe()
    {
        return me;
    }
    imp::install_path()
}

pub fn is_enabled() -> bool {
    imp::is_registered()
}

/// Turn start-at-login on (installing Shot first) or off.
pub fn set(on: bool) -> Result<(), String> {
    if on {
        let target = install_path();
        copy_self(&target, true)?;
        imp::register(&target)
    } else {
        imp::unregister()
    }
}

/// When start-at-login is on and this is a newer build than the installed
/// copy, update the installed copy so the next login runs the new version.
pub fn refresh_install() {
    if is_enabled() {
        let _ = copy_self(&install_path(), false);
    }
}

fn copy_self(target: &Path, always: bool) -> Result<(), String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    if same_file(&me, target) {
        return Ok(());
    }
    if !always {
        let newer = match (modified(&me), modified(target)) {
            (Some(a), Some(b)) => a > b,
            _ => true,
        };
        if !newer {
            return Ok(());
        }
    }
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::copy(&me, target).map_err(|e| format!("cannot install to {}: {e}", target.display()))?;
    Ok(())
}

fn modified(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};

    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW};
    use windows::core::w;

    const RUN: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
    const NAME: windows::core::PCWSTR = w!("Shot");
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn env_dir(var: &str) -> PathBuf {
        std::env::var_os(var).map(PathBuf::from).unwrap_or_default()
    }

    pub fn install_path() -> PathBuf {
        env_dir("LOCALAPPDATA").join(r"Programs\Shot\Shot.exe")
    }

    fn shortcut() -> PathBuf {
        env_dir("APPDATA").join(r"Microsoft\Windows\Start Menu\Programs\Shot.lnk")
    }

    pub fn is_registered() -> bool {
        unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, NAME, RRF_RT_REG_SZ, None, None, None) == ERROR_SUCCESS }
    }

    pub fn register(exe: &Path) -> Result<(), String> {
        let cmd = format!("\"{}\"", exe.display());
        let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
        let r = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, RUN, NAME, REG_SZ.0, Some(wide.as_ptr().cast()), (wide.len() * 2) as u32) };
        if r != ERROR_SUCCESS {
            return Err(format!("could not add Shot to start-up (error {})", r.0));
        }
        // Start Menu entry, so Shot can be found by searching. Best effort, off the UI thread.
        let lnk = shortcut();
        let exe = exe.to_path_buf();
        std::thread::spawn(move || {
            let q = |p: &Path| p.display().to_string().replace('\'', "''");
            let dir = exe.parent().map(q).unwrap_or_default();
            let script = format!(
                "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.Description='Shot screenshot tool';$s.Save()",
                q(&lnk),
                q(&exe),
                dir
            );
            let _ = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        });
        Ok(())
    }

    pub fn unregister() -> Result<(), String> {
        unsafe {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, NAME);
        }
        let _ = std::fs::remove_file(shortcut());
        if is_registered() { Err("could not remove Shot from start-up".into()) } else { Ok(()) }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::{Path, PathBuf};

    fn home() -> PathBuf {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
    }

    fn plist() -> PathBuf {
        home().join("Library/LaunchAgents/app.shot.plist")
    }

    pub fn install_path() -> PathBuf {
        shot::config::app_dir().join("shot")
    }

    pub fn is_registered() -> bool {
        plist().exists()
    }

    pub fn register(exe: &Path) -> Result<(), String> {
        let p = plist();
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>app.shot</string>\n<key>ProgramArguments</key><array><string>{}</string></array>\n<key>RunAtLoad</key><true/>\n</dict></plist>\n",
            exe.display()
        );
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, body).map_err(|e| e.to_string())
    }

    pub fn unregister() -> Result<(), String> {
        let _ = std::fs::remove_file(plist());
        Ok(())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use std::path::{Path, PathBuf};

    fn home() -> PathBuf {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
    }

    fn entry() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("autostart/shot.desktop")
    }

    pub fn install_path() -> PathBuf {
        home().join(".local/bin/shot")
    }

    pub fn is_registered() -> bool {
        entry().exists()
    }

    pub fn register(exe: &Path) -> Result<(), String> {
        let p = entry();
        let body = format!(
            "[Desktop Entry]\nType=Application\nName=Shot\nComment=Screenshot tool\nExec=\"{}\"\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, body).map_err(|e| e.to_string())
    }

    pub fn unregister() -> Result<(), String> {
        let _ = std::fs::remove_file(entry());
        Ok(())
    }
}

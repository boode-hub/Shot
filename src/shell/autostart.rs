//! "Start at login": the per-user Run key on Windows, a LaunchAgent on macOS,
//! an XDG autostart entry on Linux. Nothing needs admin rights.

use std::path::PathBuf;

fn exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW};
    use windows::core::{HSTRING, w};

    const RUN: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
    const NAME: windows::core::PCWSTR = w!("Shot");

    pub fn is_enabled() -> bool {
        unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, NAME, RRF_RT_REG_SZ, None, None, None) == ERROR_SUCCESS }
    }

    pub fn set(on: bool) -> Result<(), String> {
        let r = unsafe {
            if on {
                let cmd = format!("\"{}\"", super::exe()?.display());
                let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
                RegSetKeyValueW(HKEY_CURRENT_USER, RUN, NAME, REG_SZ.0, Some(wide.as_ptr().cast()), (wide.len() * 2) as u32)
            } else {
                RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, NAME)
            }
        };
        if r == ERROR_SUCCESS || !on { Ok(()) } else { Err(HSTRING::from(format!("registry error {}", r.0)).to_string()) }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;

    fn plist() -> PathBuf {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Library/LaunchAgents/app.shot.plist")
    }

    pub fn is_enabled() -> bool {
        plist().exists()
    }

    pub fn set(on: bool) -> Result<(), String> {
        let p = plist();
        if !on {
            let _ = std::fs::remove_file(&p);
            return Ok(());
        }
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>app.shot</string>\n<key>ProgramArguments</key><array><string>{}</string></array>\n<key>RunAtLoad</key><true/>\n</dict></plist>\n",
            super::exe()?.display()
        );
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, body).map_err(|e| e.to_string())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use std::path::PathBuf;

    fn entry() -> PathBuf {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")).join("autostart/shot.desktop")
    }

    pub fn is_enabled() -> bool {
        entry().exists()
    }

    pub fn set(on: bool) -> Result<(), String> {
        let p = entry();
        if !on {
            let _ = std::fs::remove_file(&p);
            return Ok(());
        }
        let body = format!(
            "[Desktop Entry]\nType=Application\nName=Shot\nComment=Screenshot tool\nExec=\"{}\"\nX-GNOME-Autostart-enabled=true\n",
            super::exe()?.display()
        );
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, body).map_err(|e| e.to_string())
    }
}

pub use imp::{is_enabled, set};

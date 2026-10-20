// No console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod shell;

use std::fs::File;

const USAGE: &str = "Shot — capture, annotate, copy, save or pin a part of the screen.

Usage: shot [--capture] [--once] [--settings]

  (no option)  run in the tray; press the shortcut (Shift+Space) to capture.
               If Shot is already running, this opens its Settings window.
  --capture    also start a capture right away
  --settings   open the Settings window
  --once       capture now and quit when done (no tray, no hotkey); handy to
               bind to a key in your desktop's own shortcut settings
  --version    print the version";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("--help") || has("-h") {
        println!("{USAGE}");
        return;
    }
    if has("--version") || has("-V") {
        println!("shot {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let opts = shell::Options { capture_now: has("--capture") || has("--once"), once: has("--once"), settings: has("--settings") };

    // One tray instance per user. `--once` runs alongside it on purpose.
    let _lock = if opts.once {
        None
    } else {
        let dir = shot::config::app_dir();
        let _ = std::fs::create_dir_all(&dir);
        match File::create(dir.join("shot.lock")) {
            Ok(f) => {
                if f.try_lock().is_err() {
                    // Already running: bring up its Settings window instead.
                    if !shell::ask_running_to_open_settings() {
                        eprintln!("Shot is already running (look for its tray icon).");
                    }
                    return;
                }
                Some(f)
            }
            Err(_) => None,
        }
    };

    if let Err(e) = shell::run(opts) {
        eprintln!("shot: {e}");
        std::process::exit(1);
    }
}

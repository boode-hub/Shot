//! The desktop shell around the core: windows, capture, tray, hotkey, output.

mod app;
mod autostart;
mod capture;
mod ipc;
mod ocr;
mod output;
mod pin;
mod present;
mod settings_window;
mod toast;
mod tray;

pub use app::{Options, run};
pub use ipc::ask_running_to_open_settings;

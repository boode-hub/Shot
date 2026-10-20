//! The desktop shell around the core: windows, capture, tray, hotkey, output.

mod app;
mod autostart;
mod capture;
mod ocr;
mod output;
mod pin;
mod present;
mod toast;
mod tray;

pub use app::{Options, run};

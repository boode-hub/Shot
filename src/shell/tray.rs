//! System tray icon and its menu.

use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub struct Tray {
    _icon: TrayIcon,
    pub capture: MenuId,
    pub folder: MenuId,
    pub settings: MenuId,
    pub close_pins: MenuId,
    pub autostart: CheckMenuItem,
    pub quit: MenuId,
}

pub fn build(hotkey: Option<&str>) -> Result<Tray, String> {
    let capture = MenuItem::new(
        match hotkey {
            Some(k) => format!("Capture\t{k}"),
            None => "Capture".into(),
        },
        true,
        None,
    );
    let folder = MenuItem::new("Open screenshots folder", true, None);
    let settings = MenuItem::new("Settings…", true, None);
    let close_pins = MenuItem::new("Close all pins", true, None);
    let label = if cfg!(windows) { "Start with Windows" } else { "Start at login" };
    let autostart = CheckMenuItem::new(label, true, super::autostart::is_enabled(), None);
    let quit = MenuItem::new("Quit Shot", true, None);
    let menu = Menu::new();
    menu.append_items(&[
        &capture,
        &PredefinedMenuItem::separator(),
        &folder,
        &settings,
        &close_pins,
        &autostart,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .map_err(|e| e.to_string())?;
    let icon = Icon::from_rgba(shot::icon::app_icon_rgba(32), 32, 32).map_err(|e| e.to_string())?;
    let tip = match hotkey {
        Some(k) => format!("Shot — press {k} to capture"),
        None => "Shot — click to capture".into(),
    };
    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip(tip)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(Tray {
        _icon: tray,
        capture: capture.id().clone(),
        folder: folder.id().clone(),
        settings: settings.id().clone(),
        close_pins: close_pins.id().clone(),
        autostart,
        quit: quit.id().clone(),
    })
}

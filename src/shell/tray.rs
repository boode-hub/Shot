//! System tray icon and its menu.

use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub struct Tray {
    icon: TrayIcon,
    capture_item: MenuItem,
    pub capture: MenuId,
    pub settings: MenuId,
    pub folder: MenuId,
    pub close_pins: MenuId,
    pub quit: MenuId,
}

impl Tray {
    /// Show the current shortcut in the menu and the tooltip.
    pub fn set_hotkey(&self, hotkey: Option<&str>) {
        let pretty = hotkey.map(shot::hotkey::pretty);
        self.capture_item.set_text(match &pretty {
            Some(k) => format!("Capture\t{k}"),
            None => "Capture".into(),
        });
        let tip = match &pretty {
            Some(k) => format!("Shot: press {k} to capture"),
            None => "Shot: click to capture".into(),
        };
        let _ = self.icon.set_tooltip(Some(tip));
    }
}

pub fn build(hotkey: Option<&str>) -> Result<Tray, String> {
    let capture_item = MenuItem::new("Capture", true, None);
    let settings = MenuItem::new("Settings…", true, None);
    let folder = MenuItem::new("Open screenshots folder", true, None);
    let close_pins = MenuItem::new("Close all pins", true, None);
    let quit = MenuItem::new("Quit Shot", true, None);
    let menu = Menu::new();
    menu.append_items(&[
        &capture_item,
        &settings,
        &PredefinedMenuItem::separator(),
        &folder,
        &close_pins,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .map_err(|e| e.to_string())?;
    let icon = Icon::from_rgba(shot::icon::app_icon_rgba(32), 32, 32).map_err(|e| e.to_string())?;
    let icon = TrayIconBuilder::new()
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|e| e.to_string())?;
    let tray = Tray {
        icon,
        capture: capture_item.id().clone(),
        capture_item,
        settings: settings.id().clone(),
        folder: folder.id().clone(),
        close_pins: close_pins.id().clone(),
        quit: quit.id().clone(),
    };
    tray.set_hotkey(hotkey);
    Ok(tray)
}

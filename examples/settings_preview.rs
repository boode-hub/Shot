//! Renders the Settings window to PNGs: `cargo run --example settings_preview -- <out-dir>`.

use std::rc::Rc;

use shot::config::SaveMode;
use shot::geom::Pt;
use shot::settings::{Note, State, View};
use shot::text::Font;
use tiny_skia::Pixmap;

type Setup = Box<dyn Fn(&mut View)>;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let font = Font::load_system().map(Rc::new);
    let bold = Font::load_system_bold().map(Rc::new);
    let state = State {
        hotkey: "Shift+Space".into(),
        note: Note::Active,
        save_dir: r"C:\Users\boode\Pictures\Shot".into(),
        save_mode: SaveMode::Ask,
        copy_on_save: false,
        autostart: true,
        install_path: r"C:\Users\boode\AppData\Local\Programs\Shot\Shot.exe".into(),
        portable: false,
    };
    let shots: [(&str, f32, Setup); 3] = [
        (
            "settings",
            1.0,
            Box::new(|v: &mut View| {
                v.mouse_move(Pt::new(400.0, 150.0));
            }),
        ),
        (
            "settings-recording",
            1.0,
            Box::new(|v: &mut View| {
                v.state.note = Note::Recording;
                v.state.autostart = false;
                v.state.save_mode = SaveMode::Folder;
                v.state.copy_on_save = true;
            }),
        ),
        (
            "settings-error-150",
            1.5,
            Box::new(|v: &mut View| {
                v.state.hotkey = "Ctrl+Alt+KeyS".into();
                v.state.note = Note::Error("Shift + Space is used by another app. Try another shortcut.".into());
                v.state.save_dir = r"D:\Very\Long\Folder\Structure\That\Keeps\Going\For\A\While\Screenshots\Shot".into();
            }),
        ),
    ];
    for (name, scale, setup) in shots {
        let mut v = View::new(state.clone(), scale, font.clone(), bold.clone());
        setup(&mut v);
        let (w, h) = v.size();
        let mut pm = Pixmap::new(w, h).unwrap();
        v.render(&mut pm);
        pm.save_png(format!("{out}/{name}.png")).unwrap();
    }
    println!("wrote settings previews to {out}");
}

//! Writes the logo: `cargo run --example logo -- <out-dir>` → logo-<size>.png
//! and shot.ico (the file embedded in shot.exe lives at assets/shot.ico).

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    for s in [16, 32, 64, 256] {
        shot::icon::app_icon(s).save_png(format!("{out}/logo-{s}.png")).unwrap();
    }
    std::fs::write(format!("{out}/shot.ico"), shot::icon::app_icon_ico()).unwrap();
    println!("wrote logo files to {out}");
}

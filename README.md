# Shot

A tiny, fast screenshot tool written in Rust. Press **Shift + Space** (or any
shortcut you choose), drag over what you want, mark it up, then **copy**,
**save**, or **pin** it to the screen so you can keep reading it while you work
in another window.

No GPU, no web view, no runtime: one small executable that sits in the tray.

## Features

- **Capture:** all monitors at once, each at its native resolution and DPI.
  Drag to select, **click a window** to grab just that window, or **Ctrl+A**
  for the whole screen. Move the selection by dragging it or with the arrow
  keys (Shift = 10 px), and resize it with the eight handles.
- **Annotate:**
  | Tool | Key | Notes |
  |---|---|---|
  | Move / resize | V | move and resize the selection |
  | Pen | P | freehand; hold Shift for a straight line |
  | Highlighter | H | real highlighter look: dark text stays readable; has its own colour (yellow) |
  | Line | L | Shift snaps to 45° |
  | Arrow | A | Shift snaps to 45° |
  | Rectangle | R | Shift makes a square |
  | Ellipse | E | Shift makes a circle |
  | Text | T | click, type; Enter = new line, Esc or click elsewhere = done |
  | Blur | B | drag over passwords, emails, account numbers… |
  | Numbered steps | N | click to drop 1, 2, 3… |

  Colours: click the colour dot, or press **1–8**. Size: mouse **wheel** or
  **[ ]**. **Ctrl+Z / Ctrl+Y** undo/redo. The last colour and size are
  remembered.
- **Finish:**
  | Action | Keys |
  |---|---|
  | Copy to clipboard | Ctrl+C, Enter, or double-click the selection |
  | Save | Ctrl+S (dialog or straight to folder, see settings) |
  | Quick save to folder | Ctrl+Shift+S |
  | **Pin to screen** | Ctrl+P or F3 |
  | **Copy text (OCR)** | Ctrl+T |
  | Cancel | Esc or right-click |
- **Pins** stay on top of everything. Drag to move, wheel to zoom, **0** for
  actual size, **Ctrl+C** copy, **Ctrl+S** save, **Ctrl+T** copy its text,
  **Esc**/double-click/middle-click to close. Right-click for a menu (Windows
  and macOS).
- **OCR (copy text from a screenshot):** on Windows this uses the OCR engine
  built into Windows 10/11 (the one the Snipping Tool uses), so nothing extra
  to install. On Linux and macOS, Shot uses `tesseract` if it is installed.
- **Tray icon** (the Sharingan): left-click to capture; right-click for
  Settings, the screenshots folder, close all pins, quit.
- **Settings window:** pick the capture shortcut (click *Change* and press the
  keys), the screenshots folder, how the Save button behaves, and **Start Shot
  when I log in**. It opens on first run, from the tray, or by starting Shot
  again while it is already running.

## Get it (Windows)

The repository includes a ready-made, portable **`Shot.exe`** (64-bit, no
install, no dependencies). Get it with Git and run it:

```
git clone https://github.com/BOoDe007/Shot D:\SS\Shot
D:\SS\Shot\Shot.exe
```

Or put `build.ps1` in a folder and run
`powershell -ExecutionPolicy Bypass -File D:\SS\Shot\build.ps1` (or
double-click `build.cmd`): it gets or updates the repository (installing Git
with winget if needed) and starts `Shot.exe`. Run it again any time to update.

A file that arrives through Git is not marked as downloaded from the internet,
so Windows does not block it as an unknown program the way it does a browser
download.

**Build it yourself instead:** `build.ps1 -FromSource` installs Rust (GNU
toolchain, no Visual Studio) and LLVM's dlltool if needed, compiles on your PC
and starts the result. The ready-made `Shot.exe` is built the same way:
`cargo build --release --target x86_64-pc-windows-gnu` with the Rust version in
`rust-toolchain.toml`.

The Settings window opens the first time. Turn on **Start Shot when I log in**
to install it: Shot copies itself to
`%LOCALAPPDATA%\Programs\Shot\Shot.exe`, adds itself to your start-up apps
and to the Start Menu (no admin rights needed). Running a newer build later
updates that installed copy automatically.

### About Shift + Space

A global shortcut belongs to Shot everywhere, so while Shot runs, Shift + Space
no longer types a space in other apps when Shift is held. If that gets in the
way, pick something else in Settings (for example Print Screen or
Ctrl + Shift + S). Windows can't tell left Shift from right Shift for global
shortcuts, so both work. Only one app can own a shortcut; if another app has
it, Settings says so.

## Command line

```
shot              run in the tray (if already running: open its Settings)
shot --capture    run in the tray and capture right away
shot --settings   run in the tray and open Settings
shot --once       capture now, quit when done (no tray, no shortcut)
```

`--once` is handy for binding a key in your desktop's own shortcut settings
(for example on Linux Wayland, where apps cannot grab global hotkeys).

## Linux and macOS

The code is cross-platform and compiles for all three. Linux (X11) is tested
end to end. macOS compiles but is untested. On Linux, building needs:
`libxkbcommon-dev libpipewire-0.3-dev libgbm-dev libegl-dev libwayland-dev libclang-dev`
(the names vary by distribution). On Wayland, global hotkeys don't work; bind
`shot --once` to a key instead.

## Layout

```
src/
  lib.rs        core library (no window system, fully unit-tested)
  geom.rs       points and rectangles
  style.rs      palette and UI colours
  text.rs       system font loading and text drawing
  annot.rs      annotation shapes, rendering, blur
  toolbar.rs    toolbar layout, hit-testing, vector icons
  editor.rs     the capture editor: selection, tools, undo, rendering
  settings.rs   the Settings window's layout, drawing and clicks
  hotkey.rs     shortcut recording, validation and display names
  config.rs     settings storage
  icon.rs       the Sharingan logo, drawn in code (+ .ico export)
  main.rs       entry point, command line, single instance
  shell/        desktop integration
    app/mod.rs    event loop: shortcut, tray, overlays, toasts
    app/pins.rs   pinned screenshots: input and right-click menu
    app/settings.rs  capture shortcut and the Settings window
    capture.rs    grab monitors and window rectangles (xcap)
    present.rs    pixels to window (softbuffer, CPU only)
    pin.rs        pin window drawing and zoom
    settings_window.rs  the Settings window itself
    toast.rs      "Copied to clipboard" messages
    tray.rs       tray icon and menu
    output.rs     clipboard, PNG files, Save / folder dialogs
    ocr.rs        Windows OCR / tesseract
    autostart.rs  install + start at login
    ipc.rs        a second launch opens the running copy's Settings
examples/
  preview.rs           editor UI to PNGs (no display needed)
  settings_preview.rs  Settings window to PNGs
  logo.rs              logo PNGs and shot.ico
  bench.rs             frame-time check at 4K
.github/workflows/ci.yml   build, lint, test; downloadable shot.exe
Shot.exe                   ready-made portable Windows build
build.ps1 / build.cmd      update from GitHub and start Shot (-FromSource: build it)
rust-toolchain.toml        the one Rust version every build uses
```

All drawing is done on the CPU with `tiny-skia`, and pixels go to the screen
with `softbuffer`. At 4K a frame takes about 3–6 ms (about 10 ms while
dragging a large blur), and opening a capture takes about 25 ms per monitor.

```
cargo test                                  # core tests
cargo run --example preview -- <out-dir>    # UI screenshots without a display
cargo run --release --example bench         # 4K frame timings
```

## Ideas for later

- Click an annotation to move, edit or delete it
- Magnifier loupe with the pixel colour while selecting
- Scrolling capture and screen recording (GIF/MP4)
- Upload or share link
- Embed the logo in `shot.exe` itself (`cargo run --example logo -- assets`
  writes `shot.ico`; a small build script can attach it)

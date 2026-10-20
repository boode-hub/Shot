# Shot

A tiny, fast screenshot tool written in Rust. Press **PrintScreen**, drag over
what you want, mark it up, then **copy**, **save**, or **pin** it to the screen
so you can keep reading it while you work in another window.

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
- **Tray icon:** left-click to capture; right-click for screenshots folder,
  settings, close all pins, **Start with Windows**, quit.

## Build and run (Windows)

1. Install Rust from <https://rustup.rs> (take the default MSVC option; it
   installs the Visual Studio build tools if they are missing).
2. In this folder:
   ```
   cargo build --release
   ```
3. Run `target\release\shot.exe`. A blue icon appears in the tray. Press
   **PrintScreen**.

To start it with Windows, tick **Start with Windows** in the tray menu.

### PrintScreen already taken?

Only one app can own a hotkey. If **LightShot** is running, quit it (and remove
it from start-up). On Windows 11, also turn off *Settings → Accessibility →
Keyboard → "Use the Print screen key to open screen capture"*. If PrintScreen
is still taken, Shot falls back to **Ctrl+PrintScreen** and tells you so; you
can pick any hotkey in the settings.

## Settings

Tray → **Settings…** opens `settings.txt` (on Windows:
`%APPDATA%\Shot\settings.txt`). Restart Shot after editing.

```
hotkey = PrintScreen          # e.g. Ctrl+PrintScreen, Ctrl+Shift+KeyA, Alt+F1
save_dir = C:\Users\you\Pictures\Shot
save_mode = ask               # ask = Save dialog, folder = save straight away
copy_on_save = false          # also copy when saving
color = #ff3b30               # remembered automatically
highlighter = #ffd60a
size = 2
```

## Command line

```
shot              run in the tray (default)
shot --capture    run in the tray and capture right away
shot --once       capture now, quit when done (no tray, no hotkey)
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
  config.rs     settings file
  icon.rs       the app icon, drawn in code
  main.rs       entry point, command line, single instance
  shell/        desktop integration
    app.rs        event loop: hotkey, tray, overlays, pins, toasts
    capture.rs    grab monitors and window rectangles (xcap)
    present.rs    pixels to window (softbuffer, CPU only)
    pin.rs        pinned screenshots
    toast.rs      "Copied to clipboard" messages
    tray.rs       tray icon and menu
    output.rs     clipboard, PNG files, Save dialog
    ocr.rs        Windows OCR / tesseract
    autostart.rs  start at login
examples/
  preview.rs    renders the editor UI to PNGs (no display needed)
  bench.rs      frame-time check at 4K
```

All drawing is done on the CPU with `tiny-skia`, and pixels go to the screen
with `softbuffer`. At 4K a frame takes about 3–6 ms (about 11 ms while
dragging a large blur).

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
- Window icon and version info embedded in `shot.exe`

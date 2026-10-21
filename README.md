<p align="center">
  <img src="docs/logo.png" width="120" alt="Shot logo: a three-tomoe Sharingan">
</p>

<h1 align="center">Shot</h1>

<p align="center">
  <b>Snip it. Mark it. Blur it. Pin it.</b><br>
  A tiny, fast screenshot tool for Windows, written in Rust.
</p>

<p align="center">
  <a href="#get-started"><img src="https://img.shields.io/badge/Windows-10%20%7C%2011-C30000?style=for-the-badge&logo=windows&logoColor=white" alt="Windows 10 | 11"></a>
  <a href="#build-from-source"><img src="https://img.shields.io/badge/Rust-1.99-8B0000?style=for-the-badge&logo=rust&logoColor=white" alt="Rust 1.99"></a>
  <a href="#get-started"><img src="https://img.shields.io/badge/portable-1.6%20MB-5C0000?style=for-the-badge" alt="Portable, 1.6 MB"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-5C0000?style=for-the-badge" alt="MIT license"></a>
  <a href="https://buymeacoffee.com/boode"><img src="https://img.shields.io/badge/Buy%20me%20a%20coffee-E01B1B?style=for-the-badge&logo=buy-me-a-coffee&logoColor=white" alt="Buy me a coffee"></a>
</p>

<p align="center">
  <img src="docs/demo.gif" width="900" alt="Shot in action: select a dashboard, add an arrow, highlight a number, blur e-mail addresses, add numbered steps and text, then pin the result on top of another window">
</p>
<p align="center"><sub>A real recording: Shift + Space, select, mark up, blur, then pin it above the next window.</sub></p>

## Why Shot

- **One small file.** `Shot.exe` is 1.6 MB and portable: no installer, no runtime, no GPU needed.
- **Instant.** Press **Shift + Space** and the screen freezes under your cursor; drawing a 4K frame takes 3–6 ms.
- **Pin anything.** Keep a screenshot floating above every window while you read from it, type from it or compare.
- **Copy the text.** Turn a screenshot into text with the OCR engine built into Windows.
- **Private.** Blur what others should not see. Shot never connects to the internet.

## Get started

1. **Get it:** `git clone https://github.com/boode-hub/Shot`, or **Code → Download ZIP** and extract it.
2. **Run** `Shot.exe`. The Settings window opens the first time, and the red eye appears by the clock.
3. **Press Shift + Space**, drag over what you want, mark it up, then copy, save or pin it.

Turn on **Start Shot when I log in** in Settings to keep it running. To update, run `git pull` in the
folder, or `build.ps1` (it pulls and restarts Shot).

> [!NOTE]
> Shot is not code-signed yet. If Windows shows *"Windows protected your PC"* for a downloaded copy,
> choose **More info → Run anyway**. A copy you get with `git clone` is not marked as downloaded.

**Portable:** the `portable.txt` next to `Shot.exe` makes Shot keep its settings in a `data` folder beside
it and install nothing. Copy both files to any folder or USB stick; delete `portable.txt` to keep settings
in your user profile instead.

## Shortcuts

**Capture** — drag to select, **click** a window to take just that window, **Ctrl + A** for the whole
screen, or **Ctrl + C** straight away to copy what is highlighted. Drag the selection or use the arrow keys
to move it, and the handles to resize it.

| Tool | Key | | Tool | Key |
|---|:-:|---|---|:-:|
| Move / resize | **V** | | Rectangle | **R** |
| Pen | **P** | | Ellipse | **E** |
| Highlighter | **H** | | Text | **T** |
| Line | **L** | | Blur | **B** |
| Arrow | **A** | | Numbered steps | **N** |

Colours **1–8** · size **mouse wheel** or **[ ]** · hold **Shift** for straight lines, squares and circles ·
**Ctrl + Z / Ctrl + Y** undo and redo.

| Finish | Keys |
|---|---|
| Copy | **Ctrl + C**, **Enter**, or double-click |
| Save | **Ctrl + S** (quick save to the folder: **Ctrl + Shift + S**) |
| Pin to the screen | **Ctrl + P** or **F3** |
| Copy the text (OCR) | **Ctrl + T** |
| Cancel | **Esc** or right-click |

**Pins** stay on top: drag to move, **wheel** to zoom, **0** for actual size, **Ctrl + C / S / T** to copy,
save or copy the text, **Esc** or double-click to close, right-click for a menu.

## Settings

Open them from the tray icon, or start `Shot.exe` again while it runs. Choose the capture shortcut (click
**Change** and press the keys), the screenshots folder, what the Save button does, and whether Shot starts
when you log in.

## Build from source

`build.ps1 -FromSource` does everything on Windows: it installs Rust (GNU toolchain, no Visual Studio) and
LLVM's dlltool if they are missing, compiles, and starts the result. By hand, with the Rust version pinned
in `rust-toolchain.toml`:

```
cargo build --release --target x86_64-pc-windows-gnu
cargo test
```

Every push is linted and tested on Windows and Linux, and when the code changes the ready-made `Shot.exe`
is rebuilt and committed automatically. All drawing happens on the CPU (`tiny-skia` + `softbuffer`); the
core editor has no window-system code and is unit-tested.

## Support

If Shot saves you time, a coffee keeps it going.

<p align="center">
  <a href="https://buymeacoffee.com/boode"><img src="docs/coffee.png" height="48" alt="Buy me a coffee"></a>
</p>

## License

[MIT](LICENSE) © 2026 boode-hub

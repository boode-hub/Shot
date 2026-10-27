<p align="center">
  <img src="docs/logo.png" width="120" alt="Shot logo: a three-tomoe Sharingan">
</p>

<h1 align="center">Shot</h1>

<p align="center">
  <b>Snip it. Mark it. Blur it. Pin it.</b><br>
  A tiny, fast screenshot tool for Windows. Pure PowerShell, nothing to install.
</p>

<p align="center">
  <a href="#get-started"><img src="https://img.shields.io/badge/Windows-10%20%7C%2011-C30000?style=for-the-badge&logo=windows&logoColor=white" alt="Windows 10 | 11"></a>
  <a href="#how-it-works"><img src="https://img.shields.io/badge/PowerShell-5.1-8B0000?style=for-the-badge&logo=powershell&logoColor=white" alt="PowerShell 5.1"></a>
  <a href="#get-started"><img src="https://img.shields.io/badge/portable-no%20exe-5C0000?style=for-the-badge" alt="Portable, no exe"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-5C0000?style=for-the-badge" alt="MIT license"></a>
  <a href="https://buymeacoffee.com/boode"><img src="https://img.shields.io/badge/Buy%20me%20a%20coffee-E01B1B?style=for-the-badge&logo=buy-me-a-coffee&logoColor=white" alt="Buy me a coffee"></a>
</p>

## Why Shot

- **Portable.** Two files you can read: `Shot.ps1` and `Shot.vbs`. No installer, no `.exe`, no admin
  rights. Settings stay in a `data` folder beside them, so the folder works from anywhere, even a USB stick.
- **Instant.** Press **Shift + Space** and the screen freezes under your cursor.
- **Pin anything.** Keep a screenshot floating above every window while you read from it, type from it or compare.
- **Copy the text.** Turn a screenshot into text with the OCR engine built into Windows.
- **Private.** Blur what others should not see. Shot never connects to the internet.

## Get started

1. **Get it:** `git clone https://github.com/boode-hub/Shot`, or **Code → Download ZIP** and extract it
   anywhere.
2. **Double-click `Shot.vbs`.** The Settings window opens the first time, and the red eye appears by the clock.
3. **Press Shift + Space**, drag over what you want, mark it up, then copy, save or pin it.

Turn on **Start Shot when I log in** in Settings to keep it running: it adds this folder's `Shot.vbs` to
your start-up apps (the per-user Run key, no admin needed). Turn it off to remove it. If you move the
folder, turn it off and on again.

To update, run `git pull` in the folder, or download the ZIP again and replace the files (keep `data`).

> [!TIP]
> If Windows blocks a downloaded copy, right-click the ZIP **before** extracting, choose **Properties**,
> tick **Unblock**, then extract. A copy you get with `git clone` is not marked as downloaded.

## Shortcuts

**Capture**: drag to select, **click** a window to take just that window, **Ctrl + A** for the whole
screen, or **Ctrl + C** straight away to copy what is highlighted. Drag the selection or use the arrow keys
to move it, and the handles to resize it.

| Tool | Key | | Tool | Key |
|---|:-:|---|---|:-:|
| Move / resize | **V** | | Rectangle | **R** |
| Pen | **P** | | Ellipse | **E** |
| Highlighter | **H** | | Text | **T** |
| Line | **L** | | Blur | **B** |
| Arrow | **A** | | Numbered steps | **N** |

Colours **1–8**, or **0** (the colour wheel in the palette) for any colour: the full Windows colour panel
with a spectrum, RGB / HSL values and 16 custom slots Shot remembers · size **mouse wheel** or **[ ]** · hold **Shift** for straight lines, squares and circles ·
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

Open them from the tray icon, or double-click `Shot.vbs` again while Shot runs. Choose the capture shortcut
(click **Change** and press the keys), the screenshots folder, what the Save button does, and whether Shot
starts when you log in.

A global shortcut belongs to Shot everywhere, so while Shot runs, Shift + Space no longer types a space in
other apps. If that gets in the way, pick something else, for example Print Screen or Ctrl + Shift + S.

## How it works

`Shot.ps1` holds the whole app. Each time it starts, Windows PowerShell 5.1 compiles the C# inside it in
memory (with the .NET Framework that ships with Windows) and runs it: WinForms windows, GDI+ drawing, the
Windows OCR engine. `Shot.vbs` only starts it without a console window. Start-up takes about two seconds.

```
Shot.vbs                    double-click to run (no console window)
powershell -File Shot.ps1 -Capture    run and capture right away
powershell -File Shot.ps1 -Settings   run and open Settings
powershell -File Shot.ps1 -Test       self-check: prints ok or what failed
```

## Support

If Shot saves you time, a coffee keeps it going.

<p align="center">
  <a href="https://buymeacoffee.com/boode"><img src="docs/coffee.png" height="48" alt="Buy me a coffee"></a>
</p>

## License

[MIT](LICENSE) © 2026 boode-hub

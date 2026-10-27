<#
  Shot: a tiny, fast screenshot tool. Select, annotate, then copy, save or pin
  it to the screen.

  Pure PowerShell and the .NET that ships with Windows: the C# below is
  compiled in memory every time Shot starts, so there is no .exe and nothing
  to install. Double-click Shot.vbs to start it without a console window.

    Shot.ps1             run in the tray (if already running: open its Settings)
    Shot.ps1 -Capture    run in the tray and capture right away
    Shot.ps1 -Settings   run in the tray and open Settings
    Shot.ps1 -Test       self-check, prints "ok" or what failed
    Shot.ps1 -Remove     stop Shot, take it out of start-up, and (if you say so) move
                         this folder to the Recycle Bin; Remove Shot.vbs runs this
#>
param([switch]$Capture, [switch]$Settings, [switch]$Test, [switch]$Remove)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing

if ($Remove) {
    $dir = $PSScriptRoot
    $answer = [System.Windows.Forms.MessageBox]::Show(
        "Remove Shot?`n`nYes: stop Shot, take it out of start-up, and move this folder (with your settings) to the Recycle Bin:`n$dir`n`nNo: only stop Shot and take it out of start-up. The files stay, and Shot.vbs starts it again.`n`nScreenshots you saved elsewhere are not touched.",
        'Remove Shot', 'YesNoCancel', 'Question', 'Button3')
    if ($answer -eq 'Cancel') { exit }

    # Ask a running Shot to quit, and wait for it to let go of its one-Shot lock.
    try { [void][System.Threading.EventWaitHandle]::OpenExisting('Local\Shot.Quit').Set() } catch { }
    for ($i = 0; $i -lt 50; $i++) {
        $m = $null
        if (-not [System.Threading.Mutex]::TryOpenExisting('Local\Shot.Screenshot', [ref]$m)) { break }
        $m.Dispose()
        Start-Sleep -Milliseconds 100
    }
    # An older Shot without the quit signal: stop the one running from this folder.
    Get-CimInstance Win32_Process -Filter "Name = 'powershell.exe'" | Where-Object {
        $_.ProcessId -ne $PID -and $_.CommandLine -and $_.CommandLine.IndexOf("$dir\Shot.ps1", [StringComparison]::OrdinalIgnoreCase) -ge 0
    } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }

    # Start-up: only the entry that starts this copy.
    $run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
    $entry = (Get-ItemProperty $run -Name Shot -ErrorAction SilentlyContinue).Shot
    if ($entry -and $entry.IndexOf("$dir\Shot.vbs", [StringComparison]::OrdinalIgnoreCase) -ge 0) { Remove-ItemProperty $run -Name Shot }

    $done = 'Shot is stopped and no longer starts when you log in.'
    if ($answer -eq 'Yes') {
        # Out of the folder first: a folder a program is working in cannot be moved.
        Set-Location $env:TEMP
        [Environment]::CurrentDirectory = $env:TEMP
        try {
            Add-Type -AssemblyName Microsoft.VisualBasic
            [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($dir, 'OnlyErrorDialogs', 'SendToRecycleBin')
            $done = "Shot is removed. Its folder is in the Recycle Bin, if you want it back."
        } catch {
            $done += "`n`nThe folder could not be moved to the Recycle Bin ($($_.Exception.Message)). Close anything using it and delete it by hand:`n$dir"
        }
    }
    [void][System.Windows.Forms.MessageBox]::Show($done, 'Remove Shot', 'OK', 'Information')
    exit
}

# One Shot per user. A second launch asks the running one to show Settings.
if (-not $Test) {
    $isFirst = $false
    $mutex = New-Object System.Threading.Mutex($true, 'Local\Shot.Screenshot', [ref]$isFirst)
    if (-not $isFirst) {
        try { [void][System.Threading.EventWaitHandle]::OpenExisting('Local\Shot.OpenSettings').Set() } catch { }
        exit
    }
}

$source = @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Drawing.Text;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

namespace Shot
{
    static class Native
    {
        [DllImport("user32.dll")] public static extern bool RegisterHotKey(IntPtr h, int id, uint mods, uint vk);
        [DllImport("user32.dll")] public static extern bool UnregisterHotKey(IntPtr h, int id);
        [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr ctx);
        [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
        public delegate bool EnumProc(IntPtr h, IntPtr l);
        [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
        [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
        [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
        [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
        [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
        [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
        [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out int v, int size);
        [DllImport("dwmapi.dll")] public static extern int DwmSetWindowAttribute(IntPtr h, int attr, ref int v, int size);
        [DllImport("user32.dll")] public static extern IntPtr MonitorFromPoint(POINT p, uint flags);
        [DllImport("shcore.dll")] public static extern int GetDpiForMonitor(IntPtr mon, int type, out uint x, out uint y);
        [DllImport("user32.dll")] public static extern bool ReleaseCapture();
        [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, int msg, IntPtr w, IntPtr l);
        [DllImport("user32.dll")] public static extern short GetKeyState(int vk);
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
        [DllImport("user32.dll")] public static extern bool RegisterRawInputDevices(RAWINPUTDEVICE[] d, uint n, uint size);
        [DllImport("user32.dll")] public static extern uint GetRawInputData(IntPtr h, uint cmd, IntPtr data, ref uint size, uint header);
        [StructLayout(LayoutKind.Sequential)] public struct RAWINPUTDEVICE { public ushort Page, Usage; public uint Flags; public IntPtr Target; }
        [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
        [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }

        public static float ScaleAt(Point p)
        {
            try
            {
                var pt = new POINT { X = p.X, Y = p.Y };
                uint x, y;
                if (GetDpiForMonitor(MonitorFromPoint(pt, 2), 0, out x, out y) == 0) return Math.Max(0.5f, x / 96f);
            }
            catch { }
            return 1f;
        }

        /// Windows 11 dark title bar and rounded corners; ignored elsewhere.
        public static void Dark(IntPtr h, bool round)
        {
            try
            {
                int on = 1, corner = 2;
                DwmSetWindowAttribute(h, 20, ref on, 4);
                if (round) DwmSetWindowAttribute(h, 33, ref corner, 4);
            }
            catch { }
        }
    }

    // ----- look ----------------------------------------------------------------

    static class Ui
    {
        public static Color Hex(int rgb, int a = 255) { return Color.FromArgb(a, (rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255); }

        public static readonly Color[] Palette = {
            Hex(0xFF3B30), Hex(0xFF9500), Hex(0xFFD60A), Hex(0x34C759),
            Hex(0x0A84FF), Hex(0xBF5AF2), Hex(0xFFFFFF), Hex(0x1C1C1E) };
        public static readonly Color Accent = Hex(0x0A84FF), BarBg = Hex(0x1F1F23, 0xF2), BarHover = Hex(0xFFFFFF, 0x1F),
            Icon = Hex(0xF2F2F7), IconOff = Hex(0xF2F2F7, 0x55), Danger = Hex(0xFF453A);
        /// How much the unselected screen is darkened (0 = none, 1 = black).
        public const float Dim = 0.45f;

        public static Color Contrast(Color c) { return 0.299 * c.R + 0.587 * c.G + 0.114 * c.B > 160 ? Color.Black : Color.White; }
        /// Fully saturated colour at `deg` degrees around the colour wheel.
        public static Color Hue(int deg)
        {
            int f = (int)(255 * (deg % 60) / 60f), q = 255 - f;
            switch (deg / 60 % 6)
            {
                case 0: return Color.FromArgb(255, f, 0);
                case 1: return Color.FromArgb(q, 255, 0);
                case 2: return Color.FromArgb(0, 255, f);
                case 3: return Color.FromArgb(0, q, 255);
                case 4: return Color.FromArgb(f, 0, 255);
                default: return Color.FromArgb(255, 0, q);
            }
        }
        public static string ToHex(Color c) { return string.Format("#{0:x2}{1:x2}{2:x2}", c.R, c.G, c.B); }
        public static bool TryHex(string s, out Color c)
        {
            c = Color.Empty;
            s = s.Trim().TrimStart('#');
            int v;
            if (s.Length != 6 || !int.TryParse(s, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out v)) return false;
            c = Hex(v);
            return true;
        }

        static readonly Dictionary<string, Font> fonts = new Dictionary<string, Font>();
        public static Font Font(float px, bool bold = false)
        {
            px = Math.Max(1f, (float)Math.Round(px * 2) / 2);
            string key = px.ToString(CultureInfo.InvariantCulture) + (bold ? "b" : "");
            Font f;
            if (!fonts.TryGetValue(key, out f))
            {
                f = new Font("Segoe UI", px, bold ? FontStyle.Bold : FontStyle.Regular, GraphicsUnit.Pixel);
                fonts[key] = f;
            }
            return f;
        }

        public static readonly StringFormat Fmt = MakeFmt();
        static StringFormat MakeFmt()
        {
            var f = (StringFormat)StringFormat.GenericTypographic.Clone();
            f.FormatFlags |= StringFormatFlags.MeasureTrailingSpaces;
            return f;
        }
        static readonly Graphics measurer = Graphics.FromImage(new Bitmap(1, 1));

        public static float LineHeight(Font f) { return f.GetHeight(96f); }
        public static SizeF Measure(string t, Font f)
        {
            if (string.IsNullOrEmpty(t)) return new SizeF(0, LineHeight(f));
            lock (measurer) return measurer.MeasureString(t, f, PointF.Empty, Fmt);
        }
        public static void Text(Graphics g, string t, Font f, Color c, float x, float y)
        {
            using (var b = new SolidBrush(c)) g.DrawString(t, f, b, x, y, Fmt);
        }

        public static GraphicsPath Round(RectangleF r, float rad)
        {
            var p = new GraphicsPath();
            rad = Math.Min(rad, Math.Min(r.Width, r.Height) / 2);
            if (rad < 0.5f) { p.AddRectangle(r); return p; }
            float d = rad * 2;
            p.AddArc(r.X, r.Y, d, d, 180, 90);
            p.AddArc(r.Right - d, r.Y, d, d, 270, 90);
            p.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
            p.AddArc(r.X, r.Bottom - d, d, d, 90, 90);
            p.CloseFigure();
            return p;
        }
        public static void Fill(Graphics g, RectangleF r, float rad, Color c)
        {
            using (var p = Round(r, rad)) using (var b = new SolidBrush(c)) g.FillPath(b, p);
        }
        public static void Outline(Graphics g, RectangleF r, float rad, Color c, float w)
        {
            using (var p = Round(r, rad)) using (var pen = new Pen(c, w)) g.DrawPath(pen, p);
        }
        /// Dark rounded panel with a hairline border, the backdrop of all chrome.
        public static void Panel(Graphics g, Rectangle r, float rad)
        {
            Fill(g, r, rad, BarBg);
            Outline(g, new RectangleF(r.X + 0.5f, r.Y + 0.5f, r.Width - 1, r.Height - 1), rad, Hex(0xFFFFFF, 28), 1);
        }
        public static Pen RoundPen(Color c, float w)
        {
            var p = new Pen(c, w);
            p.StartCap = p.EndCap = LineCap.Round;
            p.LineJoin = LineJoin.Round;
            return p;
        }
        public static Rectangle Inset(Rectangle r, int d) { return new Rectangle(r.X + d, r.Y + d, r.Width - 2 * d, r.Height - 2 * d); }
        public static int Clamp(int v, int lo, int hi) { return v < lo ? lo : v > hi ? hi : v; }
        public static Rectangle Union(Rectangle a, Rectangle b) { return a.IsEmpty ? b : b.IsEmpty ? a : Rectangle.Union(a, b); }

        // The Sharingan logo, the same artwork as Sharingan Monitor's stage-3 eye,
        // on a 128-unit canvas (viewBox 2 2 124 124).
        const string TomoeTail = "54.7 27.3 54.7 26.8 54.8 26.3 54.9 25.8 55 25.3 55.1 24.8 55.2 24.2 55.4 23.7 55.6 23.2 55.8 22.7 56 22.2 56.3 21.7 56.6 21.2 56.9 20.7 57.3 20.3 57.7 19.8 58.1 19.3 58.5 18.8 58.9 18.3 59.4 17.9 59.9 17.4 60.4 17 61 16.5 61.6 16.2 62.3 15.8 62.9 15.6 63.6 15.3 64.4 15 65.1 14.8 65.9 14.6 66.7 14.6 67.5 14.6 68.3 14.6 69.1 14.5 70 14.6 70.8 14.7 71.7 14.9 72.5 15.1 73.3 15.4 74.2 15.7 75 16.1 75.8 16.6 76.5 17.1 77.4 17.6 78.2 18.1 79 18.7 79.7 19.4 80.5 20.1 81 20.9 81 20.9 79.5 20.6 78.3 20.2 77.2 20 76 19.8 75 19.6 73.9 19.6 72.9 19.6 71.9 19.7 70.9 20 69.9 20.4 69.1 20.7 68.4 21 67.9 20.8 67.5 20.6 67 20.4 66.6 20.2 66.1 20.1 65.7 20 65.2 19.9 64.7 19.9 64.2 19.9 63.8 19.9 63.3 19.9 62.8 19.9 62.3 20 61.9 20.1 61.4 20.2 61 20.4 60.5 20.6 60.1 20.8 59.6 21 59.2 21.2 58.8 21.5 58.4 21.8 58.1 22.1 57.7 22.4 57.4 22.7 57 23.1 56.8 23.4 56.5 23.8 56.2 24.2 56 24.6 55.8 25.1 55.6 25.5 55.4 25.9 55.2 26.4 55.1 26.9 55 27.3";

        public static Bitmap Logo(int size)
        {
            var bmp = new Bitmap(size, size, PixelFormat.Format32bppArgb);
            string[] n = TomoeTail.Split(' ');
            var tail = new PointF[n.Length / 2];
            for (int i = 0; i < tail.Length; i++)
                tail[i] = new PointF(float.Parse(n[2 * i], CultureInfo.InvariantCulture), float.Parse(n[2 * i + 1], CultureInfo.InvariantCulture));
            using (var g = Graphics.FromImage(bmp))
            {
                g.SmoothingMode = SmoothingMode.AntiAlias;
                g.ScaleTransform(size / 124f, size / 124f);
                g.TranslateTransform(-2, -2);
                using (var iris = new SolidBrush(Color.FromArgb(195, 0, 0))) g.FillEllipse(iris, 6, 6, 116, 116);
                using (var rim = new Pen(Color.Black, 4.5f)) g.DrawEllipse(rim, 6, 6, 116, 116);
                using (var ring = new Pen(Color.Black, 3.2f)) g.DrawEllipse(ring, 29, 29, 70, 70);
                for (int i = 0; i < 3; i++)
                {
                    var st = g.Save();
                    g.TranslateTransform(64, 64);
                    g.RotateTransform(120 * i);
                    g.TranslateTransform(-64, -64);
                    g.FillEllipse(Brushes.Black, 64 - 9.14f, 29 - 9.14f, 18.28f, 18.28f);
                    g.FillPolygon(Brushes.Black, tail);
                    g.Restore(st);
                }
                g.FillEllipse(Brushes.Black, 50, 50, 28, 28);
            }
            return bmp;
        }
    }

    // ----- shortcuts -------------------------------------------------------------

    /// Capture shortcuts as text, e.g. "Shift+Space" or "Ctrl+Alt+KeyS". Key names
    /// are the W3C KeyboardEvent.code names, as Shot has always stored them.
    static class Shortcut
    {
        public const string Default = "Shift+Space";
        static readonly Dictionary<string, Keys> codes = new Dictionary<string, Keys>(StringComparer.OrdinalIgnoreCase);

        static Shortcut()
        {
            for (char c = 'A'; c <= 'Z'; c++) codes["Key" + c] = (Keys)c;
            for (int i = 0; i <= 9; i++) { codes["Digit" + i] = Keys.D0 + i; codes["Numpad" + i] = Keys.NumPad0 + i; }
            for (int i = 1; i <= 24; i++) codes["F" + i] = Keys.F1 + i - 1;
            string[] names = { "Space", "Enter", "Tab", "Backspace", "Insert", "Delete", "Home", "End", "PageUp", "PageDown",
                "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "PrintScreen", "Pause", "ScrollLock", "Backquote", "Minus",
                "Equal", "BracketLeft", "BracketRight", "Backslash", "Semicolon", "Quote", "Comma", "Period", "Slash",
                "NumpadAdd", "NumpadSubtract", "NumpadMultiply", "NumpadDivide", "NumpadDecimal" };
            Keys[] keys = { Keys.Space, Keys.Enter, Keys.Tab, Keys.Back, Keys.Insert, Keys.Delete, Keys.Home, Keys.End, Keys.PageUp, Keys.PageDown,
                Keys.Left, Keys.Right, Keys.Up, Keys.Down, Keys.PrintScreen, Keys.Pause, Keys.Scroll, Keys.Oem3, Keys.OemMinus,
                Keys.Oemplus, Keys.OemOpenBrackets, Keys.Oem6, Keys.Oem5, Keys.Oem1, Keys.Oem7, Keys.Oemcomma, Keys.OemPeriod, Keys.Oem2,
                Keys.Add, Keys.Subtract, Keys.Multiply, Keys.Divide, Keys.Decimal };
            for (int i = 0; i < names.Length; i++) codes[names[i]] = keys[i];
        }

        public static string CodeOf(Keys k)
        {
            foreach (var kv in codes) if (kv.Value == k) return kv.Key;
            return null;
        }

        public static bool IsModifier(Keys k)
        {
            return k == Keys.ShiftKey || k == Keys.ControlKey || k == Keys.Menu || k == Keys.LWin || k == Keys.RWin
                || k == Keys.LShiftKey || k == Keys.RShiftKey || k == Keys.LControlKey || k == Keys.RControlKey || k == Keys.LMenu || k == Keys.RMenu;
        }

        /// RegisterHotKey's modifier flags and virtual key for a shortcut.
        public static bool Parse(string hk, out uint mods, out uint vk)
        {
            mods = 0; vk = 0;
            foreach (string raw in hk.Split('+'))
            {
                string t = raw.Trim().ToLowerInvariant();
                if (t == "ctrl" || t == "control") mods |= 2;
                else if (t == "alt") mods |= 1;
                else if (t == "shift") mods |= 4;
                else if (t == "super" || t == "win" || t == "meta") mods |= 8;
                else
                {
                    Keys k;
                    if (vk != 0 || !codes.TryGetValue(raw.Trim(), out k)) return false;
                    vk = (uint)k;
                }
            }
            return vk != 0;
        }

        static bool StandaloneOk(string code)
        {
            if (code == "PrintScreen" || code == "Pause" || code == "ScrollLock" || code == "Insert") return true;
            int n;
            return code.Length > 1 && code[0] == 'F' && int.TryParse(code.Substring(1), out n);
        }

        /// Turn a key press into a shortcut, or explain why it can't be one.
        public static string FromPress(bool ctrl, bool alt, bool shift, bool win, string code, out string error)
        {
            error = null;
            if (!(ctrl || alt || shift || win) && !StandaloneOk(code))
                error = PrettyKey(code) + " alone would block typing. Add Ctrl, Alt, Shift or Win.";
            else if (shift && !ctrl && !alt && !win && code.StartsWith("Key"))
                error = "Shift + a letter would block capital letters. Add Ctrl or Alt.";
            if (error != null) return null;
            var parts = new List<string>();
            if (ctrl) parts.Add("Ctrl");
            if (alt) parts.Add("Alt");
            if (shift) parts.Add("Shift");
            if (win) parts.Add("Super");
            parts.Add(code);
            return string.Join("+", parts.ToArray());
        }

        /// The shortcut as separate key labels for display: ["Shift", "Space"].
        public static List<string> Keycaps(string hk)
        {
            var caps = new List<string>();
            foreach (string raw in hk.Split('+'))
            {
                string t = raw.Trim();
                if (t.Length == 0) continue;
                string l = t.ToLowerInvariant();
                caps.Add(l == "ctrl" || l == "control" ? "Ctrl" : l == "alt" ? "Alt" : l == "shift" ? "Shift"
                    : l == "super" || l == "win" || l == "meta" ? "Win" : PrettyKey(t));
            }
            return caps;
        }

        public static string Pretty(string hk) { return string.Join(" + ", Keycaps(hk).ToArray()); }

        public static string PrettyKey(string code)
        {
            if (code.StartsWith("Key") && code.Length == 4) return code.Substring(3);
            if (code.StartsWith("Digit") && code.Length == 6) return code.Substring(5);
            if (code.StartsWith("Numpad")) return "Num " + code.Substring(6);
            if (code.StartsWith("Arrow")) return code.Substring(5);
            switch (code)
            {
                case "PrintScreen": return "Print Screen";
                case "PageUp": return "Page Up";
                case "PageDown": return "Page Down";
                case "ScrollLock": return "Scroll Lock";
                case "Backquote": return "`";
                case "Minus": return "-";
                case "Equal": return "=";
                case "BracketLeft": return "[";
                case "BracketRight": return "]";
                case "Backslash": return "\\";
                case "Semicolon": return ";";
                case "Quote": return "'";
                case "Comma": return ",";
                case "Period": return ".";
                case "Slash": return "/";
                default: return code;
            }
        }
    }

    // ----- settings file -----------------------------------------------------------

    /// A tiny key = value file in the data folder beside Shot.ps1. People change it
    /// in the Settings window; unknown keys are ignored, bad values fall back.
    class Config
    {
        public string Hotkey = Shortcut.Default;
        public string SaveDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures), "Shot");
        /// Save button shows a dialog (true) or saves straight into SaveDir.
        public bool AskWhere = true;
        public bool CopyOnSave;
        public Color Color = Ui.Palette[0];
        /// The highlighter keeps its own colour so it stays yellow by default.
        public Color Marker = Ui.Palette[2];
        public int Size = 2;
        /// The colour panel's 16 custom slots, as ColorDialog keeps them (0x00BBGGRR).
        public int[] CustomColors = new int[0];
        public const int MaxSize = 10;

        public static Config Parse(string text)
        {
            var c = new Config();
            foreach (string raw in text.Split('\n'))
            {
                string line = raw.Trim();
                int eq = line.IndexOf('=');
                if (line.StartsWith("#") || eq < 0) continue;
                string k = line.Substring(0, eq).Trim(), v = line.Substring(eq + 1).Trim().Trim('"');
                Color col;
                int n;
                switch (k)
                {
                    case "hotkey": if (v.Length > 0) c.Hotkey = v; break;
                    case "save_dir": if (v.Length > 0) c.SaveDir = v; break;
                    case "save_mode": c.AskWhere = !v.Equals("folder", StringComparison.OrdinalIgnoreCase); break;
                    case "copy_on_save": c.CopyOnSave = v == "true" || v == "yes" || v == "1"; break;
                    case "color": if (Ui.TryHex(v, out col)) c.Color = col; break;
                    case "highlighter": if (Ui.TryHex(v, out col)) c.Marker = col; break;
                    case "size": if (int.TryParse(v, out n)) c.Size = Ui.Clamp(n, 1, MaxSize); break;
                    case "custom_colors":
                        var list = new List<int>();
                        foreach (string h in v.Split(',')) if (list.Count < 16 && Ui.TryHex(h, out col)) list.Add(col.R | col.G << 8 | col.B << 16);
                        c.CustomColors = list.ToArray();
                        break;
                }
            }
            return c;
        }

        public string Render()
        {
            return "# Shot settings, managed by Shot's Settings window (tray icon -> Settings).\r\nversion = 2\r\n\r\n" +
                "# Capture shortcut, e.g. Shift+Space, PrintScreen, Ctrl+Alt+KeyS\r\nhotkey = " + Hotkey + "\r\n\r\n" +
                "# Where screenshots are saved.\r\nsave_dir = " + SaveDir + "\r\n" +
                "# ask = show a Save dialog; folder = save straight into save_dir\r\nsave_mode = " + (AskWhere ? "ask" : "folder") + "\r\n" +
                "# Also copy the image to the clipboard when saving (true/false)\r\ncopy_on_save = " + (CopyOnSave ? "true" : "false") + "\r\n\r\n" +
                "# Last used drawing style (updated automatically).\r\ncolor = " + Ui.ToHex(Color) + "\r\nhighlighter = " + Ui.ToHex(Marker) +
                "\r\nsize = " + Size + "\r\ncustom_colors = " + string.Join(",", Array.ConvertAll(CustomColors,
                    v => Ui.ToHex(Color.FromArgb(v & 255, (v >> 8) & 255, (v >> 16) & 255)))) + "\r\n";
        }

        public static Config Load(string path, out bool first)
        {
            first = !File.Exists(path);
            if (!first) return Parse(File.ReadAllText(path));
            var c = new Config();
            try { c.Save(path); } catch { }
            return c;
        }

        public void Save(string path)
        {
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            File.WriteAllText(path, Render());
        }
    }

    // ----- annotations -------------------------------------------------------------

    enum Tool { Select, Pen, Marker, Line, Arrow, Rect, Ellipse, Text, Blur, Step }

    enum Cmd { None, Copy, Save, QuickSave, Pin, Ocr, Cancel }

    /// One thing drawn on a capture. Width is the stroke width for lines and
    /// shapes, font size for text, radius for steps, blur radius for blur.
    class Annot
    {
        public Tool Kind;
        public List<PointF> Pts = new List<PointF>();
        public PointF A, B;
        public Color Color;
        public float Width;
        public string Text = "";
        public int N;

        public bool IsEffect { get { return Kind == Tool.Marker || Kind == Tool.Blur; } }
        public RectangleF R { get { return RectangleF.FromLTRB(Math.Min(A.X, B.X), Math.Min(A.Y, B.Y), Math.Max(A.X, B.X), Math.Max(A.Y, B.Y)); } }

        /// Draw onto g. Blur and highlighter change what is underneath, so they
        /// read the picture's current pixels from `pic`.
        public void Draw(Graphics g, Bitmap pic)
        {
            float w = Math.Max(1f, Width);
            if (IsEffect)
            {
                Rectangle at;
                using (var patch = Effect(pic, out at))
                    if (patch != null) g.DrawImage(patch, at, 0, 0, at.Width, at.Height, GraphicsUnit.Pixel);
                return;
            }
            using (var pen = Ui.RoundPen(Color, w))
            {
                switch (Kind)
                {
                    case Tool.Pen: Stroke(g, pen); break;
                    case Tool.Line: g.DrawLine(pen, A, A == B ? new PointF(B.X + 0.01f, B.Y) : B); break;
                    case Tool.Arrow: DrawArrow(g, w); break;
                    case Tool.Rect: var r = R; g.DrawRectangle(pen, r.X, r.Y, r.Width, r.Height); break;
                    case Tool.Ellipse: g.DrawEllipse(pen, R); break;
                    case Tool.Text: using (var b = new SolidBrush(Color)) g.DrawString(Text, Ui.Font(w), b, A, Ui.Fmt); break;
                    case Tool.Step: DrawStep(g, w); break;
                }
            }
        }

        void Stroke(Graphics g, Pen pen)
        {
            if (Pts.Count == 1)
                using (var b = new SolidBrush(pen.Color)) g.FillEllipse(b, Pts[0].X - pen.Width / 2, Pts[0].Y - pen.Width / 2, pen.Width, pen.Width);
            else if (Pts.Count > 1) g.DrawLines(pen, Pts.ToArray());
        }

        void DrawArrow(Graphics g, float w)
        {
            float dx = B.X - A.X, dy = B.Y - A.Y, len = (float)Math.Sqrt(dx * dx + dy * dy);
            if (len < 1) return;
            float ux = dx / len, uy = dy / len, head = Math.Min(w * 3 + 10, len), half = head * 0.5f;
            var bas = new PointF(B.X - ux * head, B.Y - uy * head);
            using (var pen = Ui.RoundPen(Color, w)) g.DrawLine(pen, A, new PointF(bas.X + ux, bas.Y + uy));
            var tri = new[] { B, new PointF(bas.X - uy * half, bas.Y + ux * half), new PointF(bas.X + uy * half, bas.Y - ux * half) };
            using (var b = new SolidBrush(Color)) g.FillPolygon(b, tri);
            // Soften the tip so it matches the round shaft.
            using (var pen = Ui.RoundPen(Color, Math.Max(1, w * 0.5f))) g.DrawPolygon(pen, tri);
        }

        void DrawStep(Graphics g, float r)
        {
            using (var b = new SolidBrush(Color)) g.FillEllipse(b, A.X - r, A.Y - r, 2 * r, 2 * r);
            using (var pen = new Pen(Color.FromArgb(230, 255, 255, 255), Math.Max(1.5f, r * 0.12f))) g.DrawEllipse(pen, A.X - r, A.Y - r, 2 * r, 2 * r);
            using (var b = new SolidBrush(Ui.Contrast(Color)))
            using (var f = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center })
                g.DrawString(N.ToString(), Ui.Font(r * 1.15f, true), b, new RectangleF(A.X - r, A.Y - r, 2 * r, 2 * r), f);
        }

        /// Pixel bounds touched by this annotation, generously.
        public Rectangle Bounds()
        {
            float pad = Width + 2, x0 = float.MaxValue, y0 = float.MaxValue, x1 = float.MinValue, y1 = float.MinValue;
            var pts = new List<PointF>(Pts);
            if (Kind != Tool.Pen && Kind != Tool.Marker) pts.Add(A);
            if (Kind != Tool.Pen && Kind != Tool.Marker && Kind != Tool.Step && Kind != Tool.Text) pts.Add(B);
            if (Kind == Tool.Text) { var s = Ui.Measure(Text, Ui.Font(Width)); pts.Add(new PointF(A.X + s.Width, A.Y + s.Height)); }
            foreach (var p in pts) { x0 = Math.Min(x0, p.X); y0 = Math.Min(y0, p.Y); x1 = Math.Max(x1, p.X); y1 = Math.Max(y1, p.Y); }
            if (x0 > x1) return Rectangle.Empty;
            return Rectangle.FromLTRB((int)Math.Floor(x0 - pad), (int)Math.Floor(y0 - pad), (int)Math.Ceiling(x1 + pad), (int)Math.Ceiling(y1 + pad));
        }

        /// Blur or highlighter: the changed pixels as a patch to draw at `at`.
        public Bitmap Effect(Bitmap pic, out Rectangle at)
        {
            var area = new Rectangle(0, 0, pic.Width, pic.Height);
            if (Kind == Tool.Blur)
            {
                var r = R;
                at = Rectangle.Intersect(Rectangle.FromLTRB((int)Math.Round(r.Left), (int)Math.Round(r.Top), (int)Math.Round(r.Right), (int)Math.Round(r.Bottom)), area);
                return at.Width < 1 || at.Height < 1 ? null : Blur(pic, at, Math.Max(2, (int)Math.Round(Width)));
            }
            at = Rectangle.Intersect(Bounds(), area);
            return at.Width < 1 || at.Height < 1 ? null : Highlight(pic, at);
        }

        // Shrink and grow back smoothly: text underneath becomes unreadable, and
        // it is cheap enough to preview live.
        static Bitmap Blur(Bitmap pic, Rectangle at, int f)
        {
            var patch = new Bitmap(at.Width, at.Height, PixelFormat.Format32bppPArgb);
            using (var small = new Bitmap(Math.Max(1, at.Width / f), Math.Max(1, at.Height / f), PixelFormat.Format32bppPArgb))
            using (var ia = new ImageAttributes())
            {
                ia.SetWrapMode(WrapMode.TileFlipXY);
                using (var g = Graphics.FromImage(small))
                {
                    g.InterpolationMode = InterpolationMode.HighQualityBilinear;
                    g.DrawImage(pic, new Rectangle(0, 0, small.Width, small.Height), at.X, at.Y, at.Width, at.Height, GraphicsUnit.Pixel, ia);
                }
                using (var g = Graphics.FromImage(patch))
                {
                    g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                    g.DrawImage(small, new Rectangle(0, 0, at.Width, at.Height), 0, 0, small.Width, small.Height, GraphicsUnit.Pixel, ia);
                }
            }
            return patch;
        }

        // A real highlighter multiplies: dark text stays readable under the ink.
        Bitmap Highlight(Bitmap pic, Rectangle at)
        {
            var patch = pic.Clone(at, PixelFormat.Format32bppArgb);
            using (var mask = new Bitmap(at.Width, at.Height, PixelFormat.Format32bppArgb))
            {
                using (var g = Graphics.FromImage(mask))
                using (var pen = Ui.RoundPen(Color.White, Math.Max(1f, Width)))
                {
                    g.SmoothingMode = SmoothingMode.AntiAlias;
                    g.TranslateTransform(-at.X, -at.Y);
                    Stroke(g, pen);
                }
                var full = new Rectangle(0, 0, at.Width, at.Height);
                var pd = patch.LockBits(full, ImageLockMode.ReadWrite, PixelFormat.Format32bppArgb);
                var md = mask.LockBits(full, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
                var px = new byte[pd.Stride * at.Height];
                var mk = new byte[md.Stride * at.Height];
                Marshal.Copy(pd.Scan0, px, 0, px.Length);
                Marshal.Copy(md.Scan0, mk, 0, mk.Length);
                float cb = Color.B / 255f, cg = Color.G / 255f, cr = Color.R / 255f;
                for (int i = 0; i < px.Length; i += 4)
                {
                    float k = mk[i + 3] / 255f * (200f / 255f);
                    if (k <= 0) continue;
                    px[i] = (byte)(px[i] * (1 - k + k * cb));
                    px[i + 1] = (byte)(px[i + 1] * (1 - k + k * cg));
                    px[i + 2] = (byte)(px[i + 2] * (1 - k + k * cr));
                }
                Marshal.Copy(px, 0, pd.Scan0, px.Length);
                patch.UnlockBits(pd);
                mask.UnlockBits(md);
            }
            return patch;
        }
    }

    // ----- the toolbar ---------------------------------------------------------------

    /// Items: 0-9 tools, 10 colour, 20-27 swatches, 30 undo, 31 redo, 32 OCR,
    /// 33 pin, 34 save, 35 copy, 36 close.
    class Bar
    {
        public Rectangle Rect, Pal;
        public bool HasPal;
        public int Btn;
        public List<int> Items = new List<int>();
        public List<Rectangle> Rects = new List<Rectangle>();
        static readonly int[][] Groups = { new[] { 0, 1, 2, 3, 4, 5, 6, 7, 8, 9 }, new[] { 10 }, new[] { 30, 31 }, new[] { 32, 33, 34, 35, 36 } };
        public const string ToolKeys = "vphlaretbn";

        public static Bar Layout(Rectangle sel, Rectangle scr, float sc, bool palette)
        {
            Func<float, int> s = v => (int)Math.Round(v * sc);
            int btn = s(34), pad = s(4), sep = s(9), gap = s(8), n = 0;
            foreach (var grp in Groups) n += grp.Length;
            int w = n * btn + (Groups.Length - 1) * sep + pad * 2, h = btn + pad * 2;
            int x = Ui.Clamp(sel.Right - w, scr.X, Math.Max(scr.X, scr.Right - w));
            int below = sel.Bottom + gap, above = sel.Y - gap - h;
            int y = below + h <= scr.Bottom ? below : above >= scr.Y ? above : Math.Max(sel.Bottom - gap - h, scr.Y);
            var b = new Bar { Rect = new Rectangle(x, y, w, h), Btn = btn };
            int bx = x + pad;
            for (int gi = 0; gi < Groups.Length; gi++)
            {
                if (gi > 0) bx += sep;
                foreach (int it in Groups[gi]) { b.Items.Add(it); b.Rects.Add(new Rectangle(bx, y + pad, btn, btn)); bx += btn; }
            }
            if (palette)
            {
                int pw = 9 * btn + pad * 2, under = b.Rect.Bottom + s(4);
                int px = Ui.Clamp(b.RectOf(10).X - pad, scr.X, Math.Max(scr.X, scr.Right - pw));
                int py = b.Rect.Y > sel.Y && under + h <= scr.Bottom ? under : Math.Max(b.Rect.Y - s(4) - h, scr.Y);
                b.HasPal = true;
                b.Pal = new Rectangle(px, py, pw, h);
                // Swatches 20-27, then 28: any colour, from the full colour panel.
                for (int i = 0; i < 9; i++) { b.Items.Add(20 + i); b.Rects.Add(new Rectangle(px + pad + i * btn, py + pad, btn, btn)); }
            }
            return b;
        }

        public bool Contains(Point p) { return Rect.Contains(p) || (HasPal && Pal.Contains(p)); }
        public int Hit(Point p)
        {
            for (int i = Rects.Count - 1; i >= 0; i--) if (Rects[i].Contains(p)) return Items[i];
            return -1;
        }
        public Rectangle RectOf(int item) { int i = Items.IndexOf(item); return i < 0 ? Rectangle.Empty : Rects[i]; }

        public static string Label(int it)
        {
            string[] names = { "Move / resize", "Pen", "Highlighter", "Line", "Arrow", "Rectangle", "Ellipse", "Text", "Blur", "Numbered step" };
            if (it < 10) return names[it] + "  (" + char.ToUpper(ToolKeys[it]) + ")";
            if (it == 10) return "Colour  (1-8, 0 = any)  \u00b7  size: wheel or [ ]";
            if (it == 28) return "Any colour\u2026  (0)";
            if (it < 30) return "Colour " + (it - 19);
            string[] actions = { "Undo  (Ctrl+Z)", "Redo  (Ctrl+Y)", "Copy text (OCR)  (Ctrl+T)", "Pin to screen  (Ctrl+P)", "Save  (Ctrl+S)", "Copy  (Ctrl+C / Enter)", "Close  (Esc)" };
            return actions[it - 30];
        }

        /// Tiny SVG-like path language for icons: M x y, L x y, C x1 y1 x2 y2 x y, Z.
        public static GraphicsPath IconPath(string d)
        {
            var p = new GraphicsPath();
            string[] t = d.Split(' ');
            Func<int, float> f = i => float.Parse(t[i], CultureInfo.InvariantCulture);
            PointF cur = PointF.Empty;
            for (int i = 0; i < t.Length; )
            {
                string c = t[i++];
                if (c == "M") { cur = new PointF(f(i), f(i + 1)); i += 2; p.StartFigure(); }
                else if (c == "L") { var n = new PointF(f(i), f(i + 1)); i += 2; p.AddLine(cur, n); cur = n; }
                else if (c == "C") { var n = new PointF(f(i + 4), f(i + 5)); p.AddBezier(cur, new PointF(f(i), f(i + 1)), new PointF(f(i + 2), f(i + 3)), n); i += 6; cur = n; }
                else if (c == "Z") p.CloseFigure();
            }
            return p;
        }
    }

    // ----- the capture editor ------------------------------------------------------------

    enum Drag { None, Selecting, Moving, Resizing, Drawing }

    /// One borderless window over every monitor: pick a region, annotate it, then
    /// hand a command to App. Coordinates are physical pixels of the capture.
    class Overlay : Form
    {
        Bitmap baseImg, dimmed, composite;
        readonly Rectangle screen;
        readonly List<Rectangle> windows = new List<Rectangle>(), monitors;
        readonly List<float> scales;
        readonly bool ocr;

        internal bool HasSel;
        internal Rectangle Sel;
        internal readonly List<Annot> Annots = new List<Annot>();
        readonly List<Annot> redo = new List<Annot>();
        Drag drag;
        Point dragStart;
        bool moved;
        Rectangle orig;
        int handle;
        Annot cur;
        bool editing;
        PointF textAt;
        string text = "";

        internal Tool Tool;
        internal Color StyleColor, StyleMarker;
        internal int StyleSize;
        bool paletteOpen;
        bool hasCursor;
        Point cursor;
        int hover = -1;
        Rectangle lastChrome;
        Point lastCursor;

        public Overlay(Bitmap shot, Rectangle virt, List<Rectangle> wins, List<Rectangle> mons, List<float> monScales, Config style, bool ocrAvailable)
        {
            baseImg = shot;
            screen = new Rectangle(0, 0, shot.Width, shot.Height);
            foreach (var w in wins)
            {
                var r = Rectangle.Intersect(w, screen);
                if (r.Width > 8 && r.Height > 8) windows.Add(r);
            }
            monitors = mons;
            scales = monScales;
            ocr = ocrAvailable;
            StyleColor = style.Color;
            StyleMarker = style.Marker;
            StyleSize = Ui.Clamp(style.Size, 1, Config.MaxSize);
            dimmed = new Bitmap(shot.Width, shot.Height, PixelFormat.Format32bppPArgb);
            using (var g = Graphics.FromImage(dimmed))
            using (var ia = new ImageAttributes())
            {
                float k = 1 - Ui.Dim;
                ia.SetColorMatrix(new ColorMatrix(new[] {
                    new float[] { k, 0, 0, 0, 0 }, new float[] { 0, k, 0, 0, 0 }, new float[] { 0, 0, k, 0, 0 },
                    new float[] { 0, 0, 0, 1, 0 }, new float[] { 0, 0, 0, 0, 1 } }));
                g.DrawImage(shot, screen, 0, 0, shot.Width, shot.Height, GraphicsUnit.Pixel, ia);
            }

            Text = "Shot";
            FormBorderStyle = FormBorderStyle.None;
            StartPosition = FormStartPosition.Manual;
            AutoScaleMode = AutoScaleMode.None;
            ShowInTaskbar = false;
            TopMost = true;
            Bounds = virt;
            Cursor = Cursors.Cross;
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.Opaque, true);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                baseImg.Dispose();
                dimmed.Dispose();
                if (composite != null) composite.Dispose();
            }
            base.Dispose(disposing);
        }

        // ----- sizes, scaled for the monitor in use ---------------------------------

        Rectangle Mon(Point p)
        {
            for (int i = 0; i < monitors.Count; i++) if (monitors[i].Contains(p)) return monitors[i];
            return monitors.Count > 0 ? monitors[0] : screen;
        }

        Point FocusPt() { return HasSel ? new Point(Sel.X + Sel.Width / 2, Sel.Y + Sel.Height / 2) : hasCursor ? cursor : Point.Empty; }

        float Sc()
        {
            var p = FocusPt();
            for (int i = 0; i < monitors.Count; i++) if (monitors[i].Contains(p)) return scales[i];
            return scales.Count > 0 ? scales[0] : 1f;
        }

        float WidthFor(Tool t)
        {
            float s = StyleSize, k = Sc();
            switch (t)
            {
                case Tool.Marker: return (8 + s * 4) * k;
                case Tool.Text: return (12 + s * 4) * k;
                case Tool.Blur: return (4 + s * 2) * k;
                case Tool.Step: return (9 + s * 2) * k;
                default: return (1 + s * 1.5f) * k;
            }
        }

        Color CurColor() { return Tool == Tool.Marker ? StyleMarker : StyleColor; }
        void SetColor(Color c) { if (Tool == Tool.Marker) StyleMarker = c; else StyleColor = c; }

        Bar GetBar()
        {
            if (!HasSel || drag == Drag.Selecting) return null;
            return Bar.Layout(Sel, Mon(FocusPt()), Sc(), paletteOpen);
        }

        /// The full Windows colour panel: a spectrum, any RGB or HSL value, and 16
        /// custom slots that Shot remembers.
        void PickColor()
        {
            paletteOpen = false;
            using (var d = new ColorDialog { FullOpen = true, AnyColor = true, Color = CurColor(), CustomColors = App.Cfg.CustomColors })
            {
                if (d.ShowDialog(this) == DialogResult.OK) SetColor(Color.FromArgb(255, d.Color));
                App.Cfg.CustomColors = d.CustomColors;
                App.SaveConfig();
            }
            Activate();
            Invalidate();
        }

        static Point ClampTo(Rectangle r, Point p) { return new Point(Ui.Clamp(p.X, r.X, r.Right), Ui.Clamp(p.Y, r.Y, r.Bottom)); }
        static Rectangle FromPoints(Point a, Point b) { return Rectangle.FromLTRB(Math.Min(a.X, b.X), Math.Min(a.Y, b.Y), Math.Max(a.X, b.X), Math.Max(a.Y, b.Y)); }
        static double Dist(Point a, Point b) { return Math.Sqrt((a.X - b.X) * (a.X - b.X) + (a.Y - b.Y) * (a.Y - b.Y)); }
        static PointF Snap45(Point a, Point b)
        {
            double dx = b.X - a.X, dy = b.Y - a.Y, len = Math.Sqrt(dx * dx + dy * dy);
            double ang = Math.Round(Math.Atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
            return new PointF((float)(a.X + len * Math.Cos(ang)), (float)(a.Y + len * Math.Sin(ang)));
        }
        static PointF SnapSquare(Point a, Point b)
        {
            int dx = b.X - a.X, dy = b.Y - a.Y, s = Math.Max(Math.Abs(dx), Math.Abs(dy));
            return new PointF(a.X + s * Math.Sign(dx), a.Y + s * Math.Sign(dy));
        }

        // ----- input ----------------------------------------------------------------

        internal void MouseMoveAt(Point p)
        {
            int oldHover = hover;
            cursor = p;
            hasCursor = true;
            var bar = GetBar();
            hover = bar != null ? bar.Hit(p) : -1;
            bool shift = (ModifierKeys & Keys.Shift) != 0;
            switch (drag)
            {
                case Drag.Selecting:
                    if (!moved && Dist(dragStart, p) >= 3) moved = true;
                    if (moved) { Sel = FromPoints(ClampTo(screen, dragStart), ClampTo(screen, p)); HasSel = true; }
                    break;
                case Drag.Moving:
                    Sel = new Rectangle(Ui.Clamp(orig.X + p.X - dragStart.X, 0, screen.Width - orig.Width),
                        Ui.Clamp(orig.Y + p.Y - dragStart.Y, 0, screen.Height - orig.Height), orig.Width, orig.Height);
                    break;
                case Drag.Resizing:
                    {
                        int dx = p.X - dragStart.X, dy = p.Y - dragStart.Y, x0 = orig.X, y0 = orig.Y, x1 = orig.Right, y1 = orig.Bottom;
                        if (handle <= 2) y0 += dy;
                        if (handle >= 4 && handle <= 6) y1 += dy;
                        if (handle == 0 || handle >= 6) x0 += dx;
                        if (handle >= 2 && handle <= 4) x1 += dx;
                        var r = FromPoints(ClampTo(screen, new Point(x0, y0)), ClampTo(screen, new Point(x1, y1)));
                        Sel = new Rectangle(r.X, r.Y, Math.Max(1, r.Width), Math.Max(1, r.Height));
                        break;
                    }
                case Drag.Drawing:
                    {
                        var q = ClampTo(HasSel ? Sel : screen, p);
                        switch (cur.Kind)
                        {
                            case Tool.Pen:
                            case Tool.Marker:
                                if (shift) { if (cur.Pts.Count > 1) cur.Pts.RemoveRange(1, cur.Pts.Count - 1); cur.Pts.Add(q); }
                                else
                                {
                                    var l = cur.Pts[cur.Pts.Count - 1];
                                    if (Math.Abs(l.X - q.X) + Math.Abs(l.Y - q.Y) >= 1.5f) cur.Pts.Add(q);
                                }
                                break;
                            case Tool.Line:
                            case Tool.Arrow: cur.B = shift ? Snap45(dragStart, q) : q; break;
                            case Tool.Rect:
                            case Tool.Ellipse: cur.B = shift ? SnapSquare(dragStart, q) : q; break;
                            case Tool.Blur: cur.B = q; break;
                        }
                        break;
                    }
            }
            // Only redraw when something on screen follows the mouse.
            if (drag != Drag.None || hover != oldHover || !HasSel || Tool == Tool.Pen || Tool == Tool.Marker) Changed();
            UpdateCursor();
        }

        internal Cmd MouseDownAt(MouseButtons b, Point p)
        {
            cursor = p;
            hasCursor = true;
            Cmd c = b == MouseButtons.Left ? LeftDown(p) : b == MouseButtons.Right ? RightDown() : Cmd.None;
            Changed();
            return c;
        }

        Cmd LeftDown(Point p)
        {
            if (!HasSel) { StartSelecting(p); return Cmd.None; }
            var bar = GetBar();
            if (bar != null)
            {
                int item = bar.Hit(p);
                if (item >= 0) return ClickItem(item);
                if (bar.Contains(p)) return Cmd.None;
            }
            paletteOpen = false;
            int h = HandleAt(p);
            if (h >= 0)
            {
                CommitText();
                drag = Drag.Resizing; handle = h; orig = Sel; dragStart = p;
                return Cmd.None;
            }
            if (!Sel.Contains(p))
            {
                CommitText();
                // Start over with a fresh region; annotations stay where they are.
                if (Tool == Tool.Select) { HasSel = false; StartSelecting(p); }
                return Cmd.None;
            }
            switch (Tool)
            {
                case Tool.Select: drag = Drag.Moving; dragStart = p; orig = Sel; break;
                case Tool.Text:
                    CommitText();
                    editing = true;
                    text = "";
                    textAt = new PointF(p.X, p.Y - Ui.LineHeight(Ui.Font(WidthFor(Tool.Text))) / 2);
                    break;
                case Tool.Step:
                    {
                        CommitText();
                        int n = 0;
                        foreach (var a in Annots) if (a.Kind == Tool.Step) n = Math.Max(n, a.N);
                        Push(new Annot { Kind = Tool.Step, A = p, N = n + 1, Color = CurColor(), Width = WidthFor(Tool.Step) });
                        break;
                    }
                default:
                    CommitText();
                    cur = new Annot { Kind = Tool, A = p, B = p, Color = CurColor(), Width = WidthFor(Tool) };
                    if (Tool == Tool.Pen || Tool == Tool.Marker) cur.Pts.Add(p);
                    drag = Drag.Drawing;
                    dragStart = p;
                    break;
            }
            return Cmd.None;
        }

        void StartSelecting(Point p) { drag = Drag.Selecting; dragStart = p; moved = false; }

        Cmd RightDown()
        {
            if (drag == Drag.Drawing) drag = Drag.None;
            else if (editing) CommitText();
            else if (paletteOpen) paletteOpen = false;
            else if (HasSel) Reset();
            else return Cmd.Cancel;
            return Cmd.None;
        }

        void Reset()
        {
            CommitText();
            HasSel = false;
            Annots.Clear();
            redo.Clear();
            if (composite != null) { composite.Dispose(); composite = null; }
            drag = Drag.None;
            Tool = Tool.Select;
            paletteOpen = false;
        }

        internal Cmd MouseUpAt(MouseButtons b, Point p)
        {
            if (b != MouseButtons.Left) return Cmd.None;
            MouseMoveAt(p);
            var d = drag;
            drag = Drag.None;
            if (d == Drag.Selecting && !moved)
            {
                // A click: take the window under the cursor, else the whole monitor.
                Sel = WindowAt(p);
                if (Sel.IsEmpty) Sel = Mon(p);
                HasSel = true;
            }
            else if (d == Drag.Selecting && HasSel && (Sel.Width < 2 || Sel.Height < 2)) HasSel = false;
            else if (d == Drag.Drawing)
            {
                var r = cur.R;
                bool keep = cur.Kind == Tool.Pen || cur.Kind == Tool.Marker
                    || (cur.Kind == Tool.Blur ? r.Width >= 1 && r.Height >= 1 : Math.Sqrt(r.Width * r.Width + r.Height * r.Height) >= 2);
                if (keep) Push(cur);
                cur = null;
            }
            Changed();
            UpdateCursor();
            return Cmd.None;
        }

        internal Cmd DoubleClickAt(Point p)
        {
            var bar = GetBar();
            return HasSel && Sel.Contains(p) && (bar == null || !bar.Contains(p)) && Tool == Tool.Select ? Cmd.Copy : Cmd.None;
        }

        internal void Wheel(int lines)
        {
            if (!HasSel || lines == 0) return;
            StyleSize = Ui.Clamp(StyleSize + Math.Sign(lines), 1, Config.MaxSize);
            Changed();
        }

        static char CharOf(Keys k)
        {
            if (k >= Keys.A && k <= Keys.Z) return (char)('a' + (k - Keys.A));
            if (k >= Keys.D0 && k <= Keys.D9) return (char)('0' + (k - Keys.D0));
            if (k >= Keys.NumPad0 && k <= Keys.NumPad9) return (char)('0' + (k - Keys.NumPad0));
            if (k == Keys.OemOpenBrackets) return '[';
            if (k == Keys.Oem6) return ']';
            return '\0';
        }

        internal Cmd KeyAt(Keys k, bool ctrl, bool shift)
        {
            char c = CharOf(k);
            Cmd result = Cmd.None;
            if (editing && !ctrl)
            {
                if (k == Keys.Escape) CommitText();
                else if (k == Keys.Enter) text += "\n";
                else if (k == Keys.Back && text.Length > 0) text = text.Substring(0, text.Length - 1);
            }
            else if (ctrl)
            {
                if (c == 'c' || k == Keys.Enter) result = Cmd.Copy;
                else if (c == 's') result = shift ? Cmd.QuickSave : Cmd.Save;
                else if (c == 'p') result = Cmd.Pin;
                else if (c == 't') result = ocr ? Cmd.Ocr : Cmd.None;
                else if (c == 'z') { if (shift) Redo(); else Undo(); }
                else if (c == 'y') Redo();
                else if (c == 'a') { CommitText(); Sel = Mon(hasCursor ? cursor : Point.Empty); HasSel = true; }
            }
            else if (k == Keys.Escape)
            {
                if (drag == Drag.Drawing || drag == Drag.Selecting) drag = Drag.None;
                else if (paletteOpen) paletteOpen = false;
                else result = Cmd.Cancel;
            }
            else if (k == Keys.Enter) result = Cmd.Copy;
            else if (k == Keys.F3) result = Cmd.Pin;
            else if (k == Keys.Left || k == Keys.Right || k == Keys.Up || k == Keys.Down)
            {
                int d = shift ? 10 : 1;
                if (HasSel)
                {
                    int dx = k == Keys.Left ? -d : k == Keys.Right ? d : 0, dy = k == Keys.Up ? -d : k == Keys.Down ? d : 0;
                    Sel = new Rectangle(Ui.Clamp(Sel.X + dx, 0, screen.Width - Sel.Width), Ui.Clamp(Sel.Y + dy, 0, screen.Height - Sel.Height), Sel.Width, Sel.Height);
                }
            }
            else if (c != '\0' && HasSel)
            {
                int t = Bar.ToolKeys.IndexOf(c);
                if (t >= 0) SetTool((Tool)t);
                else if (c >= '1' && c <= '8') SetColor(Ui.Palette[c - '1']);
                else if (c == '0') PickColor();
                else if (c == '[') Wheel(-1);
                else if (c == ']') Wheel(1);
            }
            Changed();
            UpdateCursor();
            return result;
        }

        /// Text typed by the user, already composed by the keyboard layout.
        internal void TextInput(char ch)
        {
            if (!editing || char.IsControl(ch)) return;
            text += ch;
            Changed();
        }

        void SetTool(Tool t) { CommitText(); Tool = t; paletteOpen = false; }

        Cmd ClickItem(int item)
        {
            if (item < 10) SetTool((Tool)item);
            else if (item == 10) paletteOpen = !paletteOpen;
            else if (item == 28) PickColor();
            else if (item < 30) { SetColor(Ui.Palette[item - 20]); paletteOpen = false; }
            else
            {
                paletteOpen = false;
                switch (item)
                {
                    case 30: Undo(); break;
                    case 31: Redo(); break;
                    case 32: return ocr ? Cmd.Ocr : Cmd.None;
                    case 33: return Cmd.Pin;
                    case 34: return Cmd.Save;
                    case 35: return Cmd.Copy;
                    case 36: return Cmd.Cancel;
                }
            }
            return Cmd.None;
        }

        // ----- annotation stack ------------------------------------------------------

        /// What the selection shows: the capture plus annotations.
        Bitmap Picture() { return composite ?? baseImg; }

        void BurnIn(Annot a)
        {
            if (composite == null) composite = baseImg.Clone(screen, PixelFormat.Format32bppPArgb);
            Rectangle at = Rectangle.Empty;
            Bitmap patch = a.IsEffect ? a.Effect(composite, out at) : null;
            using (var g = Graphics.FromImage(composite))
            {
                g.SmoothingMode = SmoothingMode.AntiAlias;
                g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
                if (patch != null) { g.DrawImage(patch, at, 0, 0, at.Width, at.Height, GraphicsUnit.Pixel); patch.Dispose(); }
                else if (!a.IsEffect) a.Draw(g, composite);
            }
        }

        void Push(Annot a) { BurnIn(a); Annots.Add(a); redo.Clear(); }

        void CommitText()
        {
            if (!editing) return;
            editing = false;
            if (text.Trim().Length > 0) Push(new Annot { Kind = Tool.Text, A = textAt, Text = text, Color = CurColor(), Width = WidthFor(Tool.Text) });
            text = "";
        }

        internal void Undo()
        {
            if (editing) { editing = false; text = ""; return; }
            if (Annots.Count == 0) return;
            redo.Add(Annots[Annots.Count - 1]);
            Annots.RemoveAt(Annots.Count - 1);
            if (composite != null) { composite.Dispose(); composite = null; }
            foreach (var a in Annots) BurnIn(a);
        }

        internal void Redo()
        {
            if (redo.Count == 0) return;
            var a = redo[redo.Count - 1];
            redo.RemoveAt(redo.Count - 1);
            BurnIn(a);
            Annots.Add(a);
        }

        // ----- queries ---------------------------------------------------------------

        Rectangle WindowAt(Point p)
        {
            foreach (var w in windows) if (w.Contains(p)) return w;
            return Rectangle.Empty;
        }

        Point[] Handles(Rectangle s)
        {
            int mx = s.X + s.Width / 2, my = s.Y + s.Height / 2;
            return new[] { new Point(s.X, s.Y), new Point(mx, s.Y), new Point(s.Right, s.Y), new Point(s.Right, my),
                new Point(s.Right, s.Bottom), new Point(mx, s.Bottom), new Point(s.X, s.Bottom), new Point(s.X, my) };
        }

        /// 0 NW, 1 N, 2 NE, 3 E, 4 SE, 5 S, 6 SW, 7 W, or -1.
        int HandleAt(Point p)
        {
            if (!HasSel) return -1;
            float tol = 7 * Sc();
            var hs = Handles(Sel);
            for (int i = 0; i < 8; i++) if (Math.Abs(hs[i].X - p.X) <= tol && Math.Abs(hs[i].Y - p.Y) <= tol) return i;
            // Edges are grabbable too, but only with the move tool so drawing near an edge still draws.
            if (Tool != Tool.Select) return -1;
            tol = 4 * Sc();
            bool inX = p.X > Sel.X && p.X < Sel.Right, inY = p.Y > Sel.Y && p.Y < Sel.Bottom;
            if (inX && Math.Abs(p.Y - Sel.Y) <= tol) return 1;
            if (inX && Math.Abs(p.Y - Sel.Bottom) <= tol) return 5;
            if (inY && Math.Abs(p.X - Sel.X) <= tol) return 7;
            if (inY && Math.Abs(p.X - Sel.Right) <= tol) return 3;
            return -1;
        }

        static Cursor HandleCursor(int h) { return h == 0 || h == 4 ? Cursors.SizeNWSE : h == 2 || h == 6 ? Cursors.SizeNESW : h == 3 || h == 7 ? Cursors.SizeWE : Cursors.SizeNS; }

        void UpdateCursor()
        {
            Cursor c;
            if (!hasCursor) c = Cursors.Default;
            else if (drag == Drag.Moving) c = Cursors.SizeAll;
            else if (drag == Drag.Resizing) c = HandleCursor(handle);
            else if (drag != Drag.None || !HasSel) c = Cursors.Cross;
            else
            {
                var bar = GetBar();
                int h = HandleAt(cursor);
                bool inside = Sel.Contains(cursor);
                if (bar != null && bar.Contains(cursor)) c = Cursors.Default;
                else if (h >= 0) c = HandleCursor(h);
                else if (Tool == Tool.Select) c = inside ? Cursors.SizeAll : Cursors.Cross;
                else if (!inside) c = Cursors.Default;
                else c = Tool == Tool.Text ? Cursors.IBeam : Cursors.Cross;
            }
            if (Cursor != c) Cursor = c;
        }

        // ----- output ------------------------------------------------------------------

        /// What Copy / Save / Pin act on: the selection; before one is made, the
        /// window under the cursor, else the monitor.
        internal Rectangle Target()
        {
            if (HasSel) return Sel;
            var w = hasCursor ? WindowAt(cursor) : Rectangle.Empty;
            return w.IsEmpty ? Mon(hasCursor ? cursor : Point.Empty) : w;
        }

        /// The target with all annotations burned in.
        internal Bitmap Export()
        {
            CommitText();
            var t = Rectangle.Intersect(Target(), screen);
            return t.Width < 1 || t.Height < 1 ? null : Picture().Clone(t, PixelFormat.Format32bppArgb);
        }

        // ----- drawing -------------------------------------------------------------------

        /// Everything that can change between frames, so only that is repainted.
        Rectangle Chrome()
        {
            float k = Sc();
            int pad = (int)(48 * k);
            var r = Rectangle.Empty;
            if (HasSel)
            {
                r = Rectangle.Inflate(Sel, pad, pad);
                var bar = GetBar();
                if (bar != null)
                {
                    r = Ui.Union(r, Rectangle.Inflate(bar.Rect, (int)(160 * k), pad));
                    if (bar.HasPal) r = Ui.Union(r, Rectangle.Inflate(bar.Pal, pad, pad));
                }
            }
            else
            {
                var w = hasCursor && drag == Drag.None ? WindowAt(cursor) : Rectangle.Empty;
                if (!w.IsEmpty) r = Rectangle.Inflate(w, 4, 4);
                r = Ui.Union(r, Rectangle.Inflate(HintRect(), 2, 2));
            }
            if (editing) r = Ui.Union(r, Rectangle.Inflate(TextBox(), pad, pad));
            return r;
        }

        void Changed()
        {
            if (!IsHandleCreated) return;
            var now = Chrome();
            if (!lastChrome.IsEmpty) Invalidate(lastChrome);
            if (!now.IsEmpty) Invalidate(now);
            // Crosshair lines span the whole screen: repaint just those strips.
            Invalidate(new Rectangle(0, lastCursor.Y - 1, screen.Width, 3));
            Invalidate(new Rectangle(lastCursor.X - 1, 0, 3, screen.Height));
            if (hasCursor && !HasSel)
            {
                Invalidate(new Rectangle(0, cursor.Y - 1, screen.Width, 3));
                Invalidate(new Rectangle(cursor.X - 1, 0, 3, screen.Height));
            }
            lastChrome = now;
            lastCursor = hasCursor && !HasSel ? cursor : new Point(-10, -10);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            var clip = e.ClipRectangle;
            g.CompositingMode = CompositingMode.SourceCopy;
            g.InterpolationMode = InterpolationMode.NearestNeighbor;
            g.PixelOffsetMode = PixelOffsetMode.Half;
            g.DrawImage(dimmed, clip, clip, GraphicsUnit.Pixel);
            var bright = HasSel ? Sel : drag == Drag.None && hasCursor ? WindowAt(cursor) : Rectangle.Empty;
            var lit = Rectangle.Intersect(bright, clip);
            if (lit.Width > 0 && lit.Height > 0) g.DrawImage(Picture(), lit, lit, GraphicsUnit.Pixel);
            g.CompositingMode = CompositingMode.SourceOver;
            g.PixelOffsetMode = PixelOffsetMode.Default;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;

            if (drag == Drag.Drawing)
            {
                g.SetClip(Sel);
                cur.Draw(g, Picture());
                g.ResetClip();
            }
            if (editing) RenderTextEdit(g);
            if (HasSel) RenderSelection(g);
            else
            {
                if (!bright.IsEmpty) Frame(g, bright, 2 * Sc());
                if (hasCursor)
                    using (var b = new SolidBrush(Color.FromArgb(70, 255, 255, 255)))
                    {
                        g.FillRectangle(b, 0, cursor.Y, screen.Width, 1);
                        g.FillRectangle(b, cursor.X, 0, 1, screen.Height);
                    }
                Pill(g, Hint, HintRect().X, HintRect().Y, 14 * Sc());
            }
        }

        const string Hint = "Drag to select  \u00b7  Click a window  \u00b7  Ctrl+C copy now  \u00b7  Esc to cancel";

        Rectangle HintRect()
        {
            float k = Sc();
            var sz = PillSize(Hint, 14 * k);
            var m = Mon(hasCursor ? cursor : Point.Empty);
            return new Rectangle(m.X + (int)((m.Width - sz.Width) / 2), m.Y + (int)(24 * k), (int)Math.Ceiling(sz.Width), (int)Math.Ceiling(sz.Height));
        }

        void Frame(Graphics g, Rectangle r, float w)
        {
            using (var pen = new Pen(Ui.Accent, w)) g.DrawRectangle(pen, r.X + w / 2, r.Y + w / 2, Math.Max(1, r.Width - w), Math.Max(1, r.Height - w));
        }

        SizeF PillSize(string t, float px)
        {
            var s = Ui.Measure(t, Ui.Font(px));
            float k = Sc();
            return new SizeF(s.Width + 16 * k, s.Height + 8 * k);
        }

        void Pill(Graphics g, string t, float x, float y, float px)
        {
            float k = Sc();
            var sz = PillSize(t, px);
            Ui.Panel(g, new Rectangle((int)Math.Round(x), (int)Math.Round(y), (int)Math.Ceiling(sz.Width), (int)Math.Ceiling(sz.Height)), 6 * k);
            Ui.Text(g, t, Ui.Font(px), Ui.Icon, x + 8 * k, y + 4 * k);
        }

        void RenderSelection(Graphics g)
        {
            float k = Sc();
            Frame(g, Sel, Math.Max(1, k));
            float hs = 7 * k;
            foreach (var c in Handles(Sel))
            {
                var r = new Rectangle((int)Math.Round(c.X - hs / 2), (int)Math.Round(c.Y - hs / 2), (int)hs, (int)hs);
                Ui.Fill(g, r, 1.5f * k, Ui.Accent);
                Ui.Fill(g, Ui.Inset(r, Math.Max(1, (int)k)), k, Color.White);
            }

            // Size label above the selection, or inside it at the top of the screen.
            string label = Sel.Width + " \u00d7 " + Sel.Height;
            var ls = PillSize(label, 13 * k);
            float gap = 6 * k;
            bool above = Sel.Y - gap - ls.Height >= Mon(FocusPt()).Y;
            Pill(g, label, above ? Sel.X : Sel.X + gap, above ? Sel.Y - gap - ls.Height : Sel.Y + gap, 13 * k);

            var bar = GetBar();
            // Brush preview for freehand tools.
            if (hasCursor && drag == Drag.None && (Tool == Tool.Pen || Tool == Tool.Marker) && Sel.Contains(cursor) && (bar == null || !bar.Contains(cursor)))
            {
                float r = Math.Max(2, WidthFor(Tool) / 2);
                using (var pen = new Pen(Color.FromArgb(160, 255, 255, 255), 1)) g.DrawEllipse(pen, cursor.X - r, cursor.Y - r, 2 * r, 2 * r);
            }
            if (bar == null) return;
            DrawBar(g, bar);
            if (hover >= 0)
            {
                var br = bar.RectOf(hover);
                string tip = Bar.Label(hover) + (hover == 32 && !ocr ? "  \u00b7  unavailable" : "");
                var ts = PillSize(tip, 13 * k);
                float x = Math.Max(0, Math.Min(br.X + br.Width / 2f - ts.Width / 2, screen.Width - ts.Width));
                float y = bar.Rect.Y - gap - ts.Height;
                if (y < Mon(FocusPt()).Y) y = bar.Rect.Bottom + gap;
                Pill(g, tip, x, y, 13 * k);
            }
        }

        void DrawBar(Graphics g, Bar bar)
        {
            float radius = bar.Btn * 0.28f;
            Ui.Panel(g, bar.Rect, radius);
            if (bar.HasPal) Ui.Panel(g, bar.Pal, radius);
            for (int i = 0; i < bar.Items.Count; i++)
            {
                int item = bar.Items[i];
                var r = bar.Rects[i];
                if (item >= 20 && item < 30)
                {
                    float cx = r.X + r.Width / 2f, cy = r.Y + r.Height / 2f, rad = r.Width * 0.28f;
                    bool custom = Array.FindIndex(Ui.Palette, p => p.ToArgb() == CurColor().ToArgb()) < 0;
                    bool selected = item == 28 ? custom : Ui.Palette[item - 20].ToArgb() == CurColor().ToArgb();
                    if (item < 28) using (var b = new SolidBrush(Ui.Palette[item - 20])) g.FillEllipse(b, cx - rad, cy - rad, 2 * rad, 2 * rad);
                    else
                    {
                        // A colour wheel, with the current colour in the middle when it is a custom one.
                        for (int h = 0; h < 360; h += 30)
                            using (var b = new SolidBrush(Ui.Hue(h))) g.FillPie(b, cx - rad, cy - rad, 2 * rad, 2 * rad, h - 90, 31);
                        float inner = rad * 0.5f;
                        using (var b = new SolidBrush(custom ? CurColor() : Ui.BarBg)) g.FillEllipse(b, cx - inner, cy - inner, 2 * inner, 2 * inner);
                    }
                    using (var pen = new Pen(selected || hover == item ? Ui.Icon : Color.FromArgb(70, 255, 255, 255), (selected ? 2.5f : 1f) * r.Width / 34f))
                        g.DrawEllipse(pen, cx - rad, cy - rad, 2 * rad, 2 * rad);
                    continue;
                }
                bool enabled = item == 30 ? Annots.Count > 0 || editing : item == 31 ? redo.Count > 0 : item == 32 ? ocr : true;
                if (item < 10 && item == (int)Tool) Ui.Fill(g, Ui.Inset(r, 2), radius * 0.7f, Ui.Accent);
                else if (enabled && hover == item) Ui.Fill(g, Ui.Inset(r, 2), radius * 0.7f, Ui.BarHover);
                DrawIcon(g, item, r, enabled ? Ui.Icon : Ui.IconOff);
            }
        }

        /// Icons are designed on a 24x24 grid and drawn centred in the button.
        void DrawIcon(Graphics g, int item, Rectangle r, Color c)
        {
            float k = r.Width / 34f;
            var st = g.Save();
            g.TranslateTransform(r.X + 5 * k, r.Y + 5 * k);
            g.ScaleTransform(k, k);
            const float W = 1.8f;
            Action<string, float> line = (d, w) => { using (var pen = Ui.RoundPen(c, w)) using (var p = Bar.IconPath(d)) g.DrawPath(pen, p); };
            Action<string, Color> solid = (d, col) => { using (var b = new SolidBrush(col)) using (var p = Bar.IconPath(d)) g.FillPath(b, p); };
            switch (item)
            {
                case 0: line("M 7 3 L 7 19 L 11 15 L 14 21 L 16.5 20 L 13.5 14 L 19 14 Z", W); break;
                case 1: line("M 4 20 L 5 15 L 16 4 L 20 8 L 9 19 Z", W); line("M 13.5 6.5 L 17.5 10.5", W); break;
                case 2:
                    solid("M 5 15 L 15 5 L 19 9 L 9 19 Z", Color.FromArgb(210, StyleMarker));
                    line("M 5 15 L 15 5 L 19 9 L 9 19 Z", 1.2f);
                    line("M 3 21 L 9 21", W);
                    break;
                case 3: line("M 5 19 L 19 5", W); break;
                case 4: line("M 5 19 L 18 6 M 10 6 L 18 6 L 18 14", W); break;
                case 5: line("M 4 6 L 20 6 L 20 18 L 4 18 Z", W); break;
                case 6: using (var pen = Ui.RoundPen(c, W)) g.DrawEllipse(pen, 3, 6, 18, 12); break;
                case 7: line("M 6 6 L 18 6 M 12 6 L 12 19 M 9.5 19 L 14.5 19", W); break;
                case 8:
                    int[] alpha = { 230, 120, 60, 120, 60, 120, 60, 120, 230 };
                    for (int i = 0; i < 9; i++)
                        using (var b = new SolidBrush(Color.FromArgb(Math.Min(alpha[i], (int)c.A), c))) g.FillRectangle(b, 4 + (i % 3) * 6, 4 + (i / 3) * 6, 4.5f, 4.5f);
                    break;
                case 9:
                    using (var pen = Ui.RoundPen(c, W)) g.DrawEllipse(pen, 3.5f, 3.5f, 17, 17);
                    line("M 10.5 9.5 L 12.8 7.8 L 12.8 16.5", W);
                    break;
                case 10:
                    {
                        float rad = 4 + Math.Min(StyleSize, 10) * 0.55f;
                        using (var b = new SolidBrush(CurColor())) g.FillEllipse(b, 12 - rad, 12 - rad, 2 * rad, 2 * rad);
                        using (var pen = new Pen(Color.FromArgb(200, 255, 255, 255), 1.2f)) g.DrawEllipse(pen, 12 - rad, 12 - rad, 2 * rad, 2 * rad);
                        break;
                    }
                case 30: line("M 8 5 L 4 9 L 8 13 M 4 9 L 14 9 C 17.5 9 20 11.5 20 14.5 C 20 17.5 17.5 20 14.5 20 L 10 20", W); break;
                case 31: line("M 16 5 L 20 9 L 16 13 M 20 9 L 10 9 C 6.5 9 4 11.5 4 14.5 C 4 17.5 6.5 20 9.5 20 L 14 20", W); break;
                case 32:
                    line("M 4 8 L 4 4 L 8 4 M 16 4 L 20 4 L 20 8 M 20 16 L 20 20 L 16 20 M 8 20 L 4 20 L 4 16", W);
                    line("M 8.5 9 L 15.5 9 M 12 9 L 12 16", W);
                    break;
                case 33: line("M 8.5 4 L 15.5 4 M 10 4 L 10 10 L 6.5 14 L 17.5 14 L 14 10 L 14 4 M 12 14 L 12 21", W); break;
                case 34: line("M 12 3.5 L 12 14 M 7.5 9.5 L 12 14 L 16.5 9.5 M 4.5 15 L 4.5 20 L 19.5 20 L 19.5 15", W); break;
                case 35: line("M 9 8 L 20 8 L 20 20 L 9 20 Z", W); line("M 15 8 L 15 4 L 4 4 L 4 16 L 9 16", W); break;
                case 36:
                    using (var pen = Ui.RoundPen(c.ToArgb() == Ui.Icon.ToArgb() ? Ui.Danger : c, 2.2f))
                    using (var p = Bar.IconPath("M 6.5 6.5 L 17.5 17.5 M 17.5 6.5 L 6.5 17.5")) g.DrawPath(pen, p);
                    break;
            }
            g.Restore(st);
        }

        Rectangle TextBox()
        {
            var f = Ui.Font(WidthFor(Tool.Text));
            var s = Ui.Measure(text, f);
            float pad = 4 * Sc(), lh = Ui.LineHeight(f);
            return Rectangle.Round(new RectangleF(textAt.X - pad, textAt.Y - pad, Math.Max(s.Width, f.Size * 0.6f) + pad * 2, Math.Max(s.Height, lh) + pad * 2));
        }

        void RenderTextEdit(Graphics g)
        {
            var f = Ui.Font(WidthFor(Tool.Text));
            Ui.Text(g, text, f, CurColor(), textAt.X, textAt.Y);
            using (var pen = new Pen(Ui.Accent, 1) { DashPattern = new float[] { 4, 3 } })
            {
                var r = TextBox();
                g.DrawRectangle(pen, r);
            }
            string[] lines = text.Split('\n');
            float lh = Ui.LineHeight(f);
            float cx = textAt.X + Ui.Measure(lines[lines.Length - 1], f).Width, cy = textAt.Y + (lines.Length - 1) * lh;
            using (var b = new SolidBrush(CurColor())) g.FillRectangle(b, cx + 1, cy + lh * 0.1f, Math.Max(1, 1.5f * Sc()), lh * 0.85f);
        }

        // ----- window events -------------------------------------------------------------

        protected override bool IsInputKey(Keys k) { return true; }
        protected override void OnMouseMove(MouseEventArgs e) { MouseMoveAt(e.Location); }
        protected override void OnMouseLeave(EventArgs e) { hasCursor = false; hover = -1; Changed(); }
        protected override void OnMouseDown(MouseEventArgs e)
        {
            Activate();
            Cmd c = e.Button == MouseButtons.Left && e.Clicks >= 2 ? DoubleClickAt(e.Location) : Cmd.None;
            if (c == Cmd.None) c = MouseDownAt(e.Button, e.Location);
            App.Run(this, c);
        }
        protected override void OnMouseUp(MouseEventArgs e) { App.Run(this, MouseUpAt(e.Button, e.Location)); }
        protected override void OnMouseWheel(MouseEventArgs e) { Wheel(Math.Sign(e.Delta)); }
        protected override void OnKeyDown(KeyEventArgs e)
        {
            e.Handled = true;
            App.Run(this, KeyAt(e.KeyCode, e.Control && !e.Alt, e.Shift));
        }
        protected override void OnKeyPress(KeyPressEventArgs e)
        {
            // AltGr arrives as Ctrl+Alt and must type characters (@, {, ...).
            var m = ModifierKeys;
            if ((m & Keys.Control) == 0 || (m & Keys.Alt) != 0) TextInput(e.KeyChar);
        }
        protected override void OnFormClosed(FormClosedEventArgs e) { App.OverlayClosed(this); base.OnFormClosed(e); }
    }

    // ----- pins, toasts, the tray's hidden host ---------------------------------------------------

    /// A pinned screenshot: always on top, drag to move, wheel to zoom.
    class Pin : Form
    {
        internal readonly Bitmap Img;
        float zoom = 1;
        int lastClick;
        Point lastAt;

        public Pin(Bitmap img, Point at)
        {
            Img = img;
            Text = "Shot pin";
            Icon = App.AppIcon;
            FormBorderStyle = FormBorderStyle.None;
            StartPosition = FormStartPosition.Manual;
            AutoScaleMode = AutoScaleMode.None;
            ShowInTaskbar = false;
            TopMost = true;
            Location = at;
            ClientSize = img.Size;
            DoubleBuffered = true;
            var m = new ContextMenuStrip();
            Item(m, "Copy", "Ctrl+C", () => App.CopyImage(Img));
            Item(m, "Save\u2026", "Ctrl+S", () => App.SaveImage(Img, App.Cfg.AskWhere));
            Item(m, "Copy text (OCR)", "Ctrl+T", () => App.ReadText(new Bitmap(Img)));
            Item(m, "Actual size", "0", () => SetZoom(1));
            m.Items.Add(new ToolStripSeparator());
            Item(m, "Close", "Esc", Close);
            Item(m, "Close all pins", "", App.CloseAllPins);
            ContextMenuStrip = m;
        }

        static void Item(ContextMenuStrip m, string text, string keys, Action a)
        {
            var it = new ToolStripMenuItem(text, null, (s, e) => a());
            it.ShortcutKeyDisplayString = keys;
            m.Items.Add(it);
        }

        void ZoomBy(float steps) { SetZoom(zoom * (float)Math.Pow(1.1, steps)); }

        void SetZoom(float z)
        {
            // Keep the window between about 40 px and 4x the image.
            float min = Math.Min(1f, 40f / Math.Min(Img.Width, Img.Height));
            zoom = Math.Max(min, Math.Min(4f, z));
            ClientSize = new Size(Math.Max(1, (int)Math.Round(Img.Width * zoom)), Math.Max(1, (int)Math.Round(Img.Height * zoom)));
            Invalidate();
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            g.InterpolationMode = Math.Abs(zoom - 1) < 0.001f ? InterpolationMode.NearestNeighbor : InterpolationMode.HighQualityBicubic;
            g.PixelOffsetMode = PixelOffsetMode.Half;
            g.DrawImage(Img, ClientRectangle, 0, 0, Img.Width, Img.Height, GraphicsUnit.Pixel);
            g.PixelOffsetMode = PixelOffsetMode.Default;
            // A thin border tells the pin apart from what is underneath.
            using (var pen = new Pen(ContainsFocus ? Ui.Accent : Color.Gray)) g.DrawRectangle(pen, 0, 0, ClientSize.Width - 1, ClientSize.Height - 1);
        }

        protected override void OnActivated(EventArgs e) { Invalidate(); base.OnActivated(e); }
        protected override void OnDeactivate(EventArgs e) { Invalidate(); base.OnDeactivate(e); }

        protected override void OnMouseDown(MouseEventArgs e)
        {
            if (e.Button == MouseButtons.Middle) { Close(); return; }
            if (e.Button != MouseButtons.Left) return;
            // Double-click closes; otherwise drag the pin around.
            int now = Environment.TickCount;
            if (now - lastClick < 450 && Math.Abs(Cursor.Position.X - lastAt.X) < 6 && Math.Abs(Cursor.Position.Y - lastAt.Y) < 6) { Close(); return; }
            lastClick = now;
            lastAt = Cursor.Position;
            Native.ReleaseCapture();
            Native.SendMessage(Handle, 0xA1, (IntPtr)2, IntPtr.Zero);
        }

        protected override void OnMouseWheel(MouseEventArgs e) { ZoomBy(e.Delta / 120f); }

        protected override void OnKeyDown(KeyEventArgs e)
        {
            bool ctrl = e.Control && !e.Alt;
            var k = e.KeyCode;
            if (k == Keys.Escape) Close();
            else if (ctrl && k == Keys.C) App.CopyImage(Img);
            else if (ctrl && k == Keys.S) App.SaveImage(Img, App.Cfg.AskWhere);
            else if (ctrl && k == Keys.T) App.ReadText(new Bitmap(Img));
            else if (k == Keys.D0 || k == Keys.NumPad0) SetZoom(1);
            else if (k == Keys.Oemplus || k == Keys.Add) ZoomBy(1);
            else if (k == Keys.OemMinus || k == Keys.Subtract) ZoomBy(-1);
        }

        protected override void OnFormClosed(FormClosedEventArgs e)
        {
            App.Pins.Remove(this);
            Img.Dispose();
            base.OnFormClosed(e);
        }
    }

    /// Short status messages ("Copied to clipboard") near the bottom of the screen.
    class Toast : Form
    {
        readonly string msg;
        readonly float sc;
        readonly System.Windows.Forms.Timer timer = new System.Windows.Forms.Timer();

        public Toast(string text, float secs)
        {
            msg = text;
            var mon = Screen.PrimaryScreen.Bounds;
            sc = Native.ScaleAt(new Point(mon.X + mon.Width / 2, mon.Y + mon.Height / 2));
            var s = Ui.Measure(msg, Ui.Font(15 * sc));
            var size = new Size((int)Math.Ceiling(s.Width + 32 * sc), (int)Math.Ceiling(s.Height + 20 * sc));
            FormBorderStyle = FormBorderStyle.None;
            StartPosition = FormStartPosition.Manual;
            AutoScaleMode = AutoScaleMode.None;
            ShowInTaskbar = false;
            BackColor = Ui.Hex(0x1F1F23);
            Bounds = new Rectangle(mon.X + (mon.Width - size.Width) / 2, mon.Bottom - size.Height - (int)(90 * sc), size.Width, size.Height);
            timer.Interval = Math.Max(100, (int)(secs * 1000));
            timer.Tick += (o, e) => Close();
            timer.Start();
        }

        protected override bool ShowWithoutActivation { get { return true; } }
        protected override CreateParams CreateParams
        {
            // Topmost, no taskbar button, never takes the focus.
            get { var cp = base.CreateParams; cp.ExStyle |= 0x08000088; return cp; }
        }
        protected override void OnHandleCreated(EventArgs e) { Native.Dark(Handle, true); base.OnHandleCreated(e); }
        protected override void OnPaint(PaintEventArgs e)
        {
            e.Graphics.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
            using (var pen = new Pen(Ui.Hex(0x3A3A40))) e.Graphics.DrawRectangle(pen, 0, 0, Width - 1, Height - 1);
            Ui.Text(e.Graphics, msg, Ui.Font(15 * sc), Ui.Icon, 16 * sc, 10 * sc);
        }
        protected override void OnMouseDown(MouseEventArgs e) { Close(); }
        protected override void OnFormClosed(FormClosedEventArgs e) { timer.Dispose(); base.OnFormClosed(e); }
    }

    /// Double-tap Ctrl: every screen goes half dark except a circle of light around the cursor.
    /// Clicks go straight through it.
    class Spotlight : Form
    {
        readonly System.Windows.Forms.Timer timer = new System.Windows.Forms.Timer { Interval = 15 };
        Point last = new Point(int.MinValue, 0);

        public Spotlight()
        {
            FormBorderStyle = FormBorderStyle.None;
            StartPosition = FormStartPosition.Manual;
            AutoScaleMode = AutoScaleMode.None;
            ShowInTaskbar = false;
            BackColor = Color.Black;
            Opacity = 0.5;
            Bounds = SystemInformation.VirtualScreen;
            timer.Tick += (s, e) => Follow();
        }

        protected override bool ShowWithoutActivation { get { return true; } }
        protected override CreateParams CreateParams
        {
            // Topmost, click-through, no taskbar button, never takes the focus.
            get { var cp = base.CreateParams; cp.ExStyle |= 0x080000A8; return cp; }
        }
        protected override void OnShown(EventArgs e) { Follow(); timer.Start(); base.OnShown(e); }

        // The window is a dark sheet with a round hole that follows the cursor.
        void Follow()
        {
            var p = Cursor.Position;
            if (p == last) return;
            last = p;
            float r = 150 * Native.ScaleAt(p);
            using (var path = new GraphicsPath())
            {
                path.AddRectangle(new Rectangle(0, 0, Width, Height));
                path.AddEllipse(p.X - Left - r, p.Y - Top - r, 2 * r, 2 * r);
                var old = Region;
                Region = new Region(path);
                if (old != null) old.Dispose();
            }
        }

        protected override void OnFormClosed(FormClosedEventArgs e) { timer.Dispose(); base.OnFormClosed(e); }
    }

    /// Spots a double tap of Ctrl: two presses of Ctrl alone within the double-click time.
    /// Any other key or a click in between (Ctrl+C, Ctrl+click) spoils it.
    class CtrlTaps
    {
        bool down, clean, tapped;
        uint lastTap;

        /// Feed every key event; true on the second tap.
        internal bool Key(int vk, bool isDown, uint time, uint within)
        {
            if (vk != 0x11 && vk != 0xA2 && vk != 0xA3) { Spoil(); return false; }
            if (isDown) { if (!down) { down = true; clean = true; } return false; }
            down = false;
            if (!clean) return false;
            if (tapped && time - lastTap <= within) { tapped = false; return true; }
            tapped = true;
            lastTap = time;
            return false;
        }

        internal void Spoil() { clean = false; tapped = false; }
    }

    /// Never shown: receives the global shortcut and runs work on the UI thread.
    class Host : Form
    {
        protected override void WndProc(ref Message m)
        {
            if (m.Msg == 0x0312) App.StartCapture();
            if (m.Msg == 0x00FF) RawInput(m.LParam);
            base.WndProc(ref m);
        }

        // WM_INPUT: a copy of every key and mouse event, for the double tap of Ctrl.
        static void RawInput(IntPtr h)
        {
            uint size = 0, head = (uint)(8 + 2 * IntPtr.Size);
            Native.GetRawInputData(h, 0x10000003, IntPtr.Zero, ref size, head);
            if (size < head + 8) return;
            var buf = Marshal.AllocHGlobal((int)size);
            try
            {
                if (Native.GetRawInputData(h, 0x10000003, buf, ref size, head) != size) return;
                int type = Marshal.ReadInt32(buf);
                // RAWKEYBOARD: MakeCode, Flags (1 = released), Reserved, VKey. RAWMOUSE: usFlags, pad, usButtonFlags.
                if (type == 1) App.OnRawKey(Marshal.ReadInt16(buf, (int)head + 6) & 0xFFFF, (Marshal.ReadInt16(buf, (int)head + 2) & 1) == 0);
                else if (type == 0 && (Marshal.ReadInt16(buf, (int)head + 4) & 0xFFFF) != 0) App.OnRawClick();
            }
            finally { Marshal.FreeHGlobal(buf); }
        }
    }

    // ----- the Settings window ---------------------------------------------------------------

    class SettingsForm : Form
    {
        static readonly Color BG = Ui.Hex(0x111113), CARD = Ui.Hex(0x1C1C1F), DIVIDER = Ui.Hex(0x2A2A2E), TEXT = Ui.Hex(0xF2F2F7),
            MUTED = Ui.Hex(0x8E8E96), CONTROL = Ui.Hex(0x2C2C30), CONTROL_HOVER = Ui.Hex(0x3A3A40), KNOB_OFF = Ui.Hex(0x3A3A3F),
            ACCENT = Ui.Hex(0xE0182D), GOOD = Ui.Hex(0x34C759);
        const float W = 460, H = 606, Pad = 24, Inner = W - Pad * 2;
        enum Hit { None = -1, Hotkey, ChangeFolder, ModeAsk, ModeFolder, CopyOnSave, Autostart, OpenFolder, Quit }

        readonly float sc;
        readonly Bitmap logo;
        Hit hover = Hit.None;

        public SettingsForm()
        {
            var mon = Screen.PrimaryScreen.Bounds;
            sc = Native.ScaleAt(new Point(mon.X + mon.Width / 2, mon.Y + mon.Height / 2));
            logo = Ui.Logo(S(52));
            Text = "Shot settings";
            Icon = App.AppIcon;
            FormBorderStyle = FormBorderStyle.FixedSingle;
            MaximizeBox = false;
            AutoScaleMode = AutoScaleMode.None;
            StartPosition = FormStartPosition.CenterScreen;
            BackColor = BG;
            ClientSize = new Size(S(W), S(H));
            DoubleBuffered = true;
        }

        int S(float v) { return (int)Math.Round(v * sc); }
        float P(float v) { return v * sc; }
        Rectangle R(float x, float y, float w, float h) { return new Rectangle(S(x), S(y), S(w), S(h)); }
        Rectangle RightOf(float y, float h, float w, float rh) { return R(Pad + Inner - 16 - w, y + (h - rh) / 2, w, rh); }

        const float C1 = 120, C2 = 250, C3 = 450;
        Rectangle Zone(Hit h)
        {
            switch (h)
            {
                case Hit.Hotkey: return R(Pad, C1, Inner, 62);
                case Hit.ChangeFolder: return RightOf(C2, 56, 84, 30);
                case Hit.ModeAsk: return R(Pad + Inner - 16 - 230, C2 + 69, 115, 30);
                case Hit.ModeFolder: return R(Pad + Inner - 16 - 115, C2 + 69, 115, 30);
                case Hit.CopyOnSave: return R(Pad, C2 + 112, Inner, 56);
                case Hit.Autostart: return R(Pad, C3, Inner, 54);
                case Hit.OpenFolder: return R(Pad, 556, 200, 28);
                case Hit.Quit: return R(Pad + Inner - 92, 556, 92, 28);
            }
            return Rectangle.Empty;
        }

        Hit HitAt(Point p)
        {
            for (var h = Hit.Hotkey; h <= Hit.Quit; h++) if (Zone(h).Contains(p)) return h;
            return Hit.None;
        }

        protected override void OnHandleCreated(EventArgs e) { Native.Dark(Handle, false); base.OnHandleCreated(e); }
        protected override bool IsInputKey(Keys k) { return true; }

        protected override void OnMouseMove(MouseEventArgs e)
        {
            var h = HitAt(e.Location);
            if (h == hover) return;
            hover = h;
            Cursor = h == Hit.None ? Cursors.Default : Cursors.Hand;
            Invalidate();
        }
        protected override void OnMouseLeave(EventArgs e) { hover = Hit.None; Invalidate(); }

        protected override void OnMouseDown(MouseEventArgs e)
        {
            if (e.Button != MouseButtons.Left) return;
            var h = HitAt(e.Location);
            var cfg = App.Cfg;
            if (h == Hit.Hotkey) { if (App.Recording) App.FinishRecording(null, null); else App.StartRecording(); }
            // Any other click ends a recording first.
            else if (App.Recording) App.FinishRecording(null, null);
            else if (h == Hit.ChangeFolder) App.PickFolder(this);
            else if (h == Hit.ModeAsk) { cfg.AskWhere = true; App.SaveConfig(); }
            else if (h == Hit.ModeFolder) { cfg.AskWhere = false; App.SaveConfig(); }
            else if (h == Hit.CopyOnSave) { cfg.CopyOnSave = !cfg.CopyOnSave; App.SaveConfig(); }
            else if (h == Hit.Autostart) App.SetAutostart(!App.AutostartOn());
            else if (h == Hit.OpenFolder) App.OpenFolder();
            else if (h == Hit.Quit) App.Quit();
            Invalidate();
        }

        protected override void OnKeyDown(KeyEventArgs e)
        {
            e.Handled = e.SuppressKeyPress = true;
            // Windows reports Print Screen only when it is released.
            if (e.KeyCode != Keys.PrintScreen) Key(e);
        }
        protected override void OnKeyUp(KeyEventArgs e) { if (e.KeyCode == Keys.PrintScreen) Key(e); }

        void Key(KeyEventArgs e)
        {
            bool win = (Native.GetKeyState(0x5B) & 0x8000) != 0 || (Native.GetKeyState(0x5C) & 0x8000) != 0;
            bool none = !e.Control && !e.Alt && !e.Shift && !win;
            if (!App.Recording) { if (e.KeyCode == Keys.Escape && none) Close(); return; }
            if (e.KeyCode == Keys.Escape && none) App.FinishRecording(null, null);
            else if (!Shortcut.IsModifier(e.KeyCode))
            {
                string code = Shortcut.CodeOf(e.KeyCode), err = null;
                string hk = code == null ? null : Shortcut.FromPress(e.Control, e.Alt, e.Shift, win, code, out err);
                App.FinishRecording(hk, code == null ? "That key can't be used. Try another." : err);
            }
            Invalidate();
        }

        protected override void OnDeactivate(EventArgs e)
        {
            if (App.Recording) { App.FinishRecording(null, null); Invalidate(); }
            base.OnDeactivate(e);
        }

        protected override void OnFormClosed(FormClosedEventArgs e)
        {
            App.FinishRecording(null, null);
            App.SettingsClosed();
            logo.Dispose();
            base.OnFormClosed(e);
        }

        // ----- drawing ---------------------------------------------------------------

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
            var cfg = App.Cfg;

            // Header: logo, name, tagline, version.
            g.DrawImage(logo, S(Pad), S(24), logo.Width, logo.Height);
            float tx = P(Pad + 52 + 14);
            Ui.Text(g, "Shot", Ui.Font(P(24), true), TEXT, tx, P(24));
            Ui.Text(g, "Capture, annotate, pin.", Ui.Font(P(13.5f)), MUTED, tx, P(56));
            string ver = "v" + App.Version;
            Ui.Text(g, ver, Ui.Font(P(12)), MUTED, P(W - 24) - Ui.Measure(ver, Ui.Font(P(12))).Width, P(30));

            string[] labels = { "CAPTURE SHORTCUT", "SAVING", "STARTUP" };
            float[] ys = { C1, C2, C3 }, hs = { 90, 168, 80 };
            for (int i = 0; i < 3; i++)
            {
                Ui.Text(g, labels[i], Ui.Font(P(11)), MUTED, P(30), P(ys[i] - 22));
                Ui.Fill(g, R(Pad, ys[i], Inner, hs[i]), P(12), CARD);
            }

            // Shortcut card.
            var row = Zone(Hit.Hotkey);
            float cy = row.Y + row.Height / 2f;
            if (App.Recording)
            {
                Ui.Outline(g, R(Pad, C1, Inner, 90), P(12), ACCENT, P(1.5f));
                TextMid(g, "Press your new shortcut\u2026", P(40), cy, P(15), ACCENT);
            }
            else Keycaps(g, App.ShownHotkey, P(40), cy);
            Button(g, RightOf(C1, 62, 84, 32), App.Recording ? "Cancel" : "Change", hover == Hit.Hotkey);
            string note = App.Recording ? "Hold the modifier keys, then press a key. Esc cancels." : App.HotkeyNote;
            float ny = P(C1 + 60);
            if (!App.Recording && App.HotkeyError == null && App.ActiveHotkey != null)
            {
                using (var b = new SolidBrush(GOOD)) g.FillEllipse(b, P(44 - 3.5f), ny + P(7.5f - 3.5f), P(7), P(7));
                Ui.Text(g, note, Ui.Font(P(12)), MUTED, P(54), ny);
            }
            else Ui.Text(g, note, Ui.Font(P(12)), App.Recording ? MUTED : ACCENT, P(40), ny);

            // Saving card.
            Divider(g, S(C2 + 56));
            Divider(g, S(C2 + 112));
            float fy = P(C2 + 28);
            TextMid(g, "Folder", P(40), fy, P(14), TEXT);
            var fb = Zone(Hit.ChangeFolder);
            TextMid(g, Ellipsize(cfg.SaveDir, Ui.Font(P(12.5f)), fb.X - P(12) - P(110)), P(110), fy, P(12.5f), MUTED);
            Button(g, fb, "Change", hover == Hit.ChangeFolder);

            TextMid(g, "Save button", P(40), P(C2 + 84), P(14), TEXT);
            var ask = Zone(Hit.ModeAsk);
            var fold = Zone(Hit.ModeFolder);
            Ui.Fill(g, Rectangle.FromLTRB(ask.X, ask.Y, fold.Right, fold.Bottom), P(8), CONTROL);
            Segment(g, ask, "Ask where", cfg.AskWhere, hover == Hit.ModeAsk);
            Segment(g, fold, "To folder", !cfg.AskWhere, hover == Hit.ModeFolder);

            TextMid(g, "Also copy when saving", P(40), P(C2 + 140), P(14), TEXT);
            Toggle(g, RightOf(C2 + 112, 56, 44, 26), cfg.CopyOnSave);

            // Startup card.
            bool on = App.AutostartOn();
            TextMid(g, "Start Shot when I log in", P(40), P(C3 + 27), P(14), TEXT);
            Toggle(g, RightOf(C3, 54, 44, 26), on);
            string install = on ? "Runs " + App.LauncherPath + " at every login." : "Runs this copy at every login; nothing is installed.";
            Ui.Text(g, Ellipsize(install, Ui.Font(P(11.5f)), P(W - 80)), Ui.Font(P(11.5f)), MUTED, P(40), P(C3 + 50));

            // Footer.
            var of = Zone(Hit.OpenFolder);
            TextMid(g, "Open screenshots folder  \u203a", P(28), of.Y + of.Height / 2f, P(13), hover == Hit.OpenFolder ? TEXT : MUTED);
            var q = Zone(Hit.Quit);
            if (hover == Hit.Quit) Ui.Fill(g, q, P(8), Ui.Hex(0x3A1418));
            TextCenter(g, "Quit Shot", q, P(13), Ui.Hex(0xFF6B75));
        }

        void Keycaps(Graphics g, string hk, float x, float cy)
        {
            float fs = P(14.5f), h = P(32);
            var caps = Shortcut.Keycaps(hk);
            for (int i = 0; i < caps.Count; i++)
            {
                if (i > 0)
                {
                    TextMid(g, "+", x + P(7), cy, fs, MUTED);
                    x += Ui.Measure("+", Ui.Font(fs)).Width + P(14);
                }
                float w = Math.Max(Ui.Measure(caps[i], Ui.Font(fs)).Width + P(24), h);
                var r = Rectangle.Round(new RectangleF(x, cy - h / 2, w, h));
                // A keyboard key: a darker bottom edge under a lighter face.
                var face = new Rectangle(r.X, r.Y, r.Width, r.Height - S(2));
                Ui.Fill(g, r, P(7), Ui.Hex(0x0E0E10));
                Ui.Fill(g, face, P(7), Ui.Hex(0x3A3A40));
                Ui.Outline(g, face, P(7), Ui.Hex(0x55555C), sc);
                TextCenter(g, caps[i], face, fs, TEXT);
                x += w;
            }
        }

        void Button(Graphics g, Rectangle r, string label, bool hot) { Ui.Fill(g, r, P(8), hot ? CONTROL_HOVER : CONTROL); TextCenter(g, label, r, P(13), TEXT); }

        void Segment(Graphics g, Rectangle r, string label, bool on, bool hot)
        {
            if (on) Ui.Fill(g, Ui.Inset(r, S(2)), P(6.5f), Ui.Hex(0x4A4A52));
            else if (hot) Ui.Fill(g, Ui.Inset(r, S(2)), P(6.5f), CONTROL_HOVER);
            TextCenter(g, label, r, P(13), on ? TEXT : MUTED);
        }

        void Toggle(Graphics g, Rectangle r, bool on)
        {
            Ui.Fill(g, r, r.Height / 2f, on ? ACCENT : KNOB_OFF);
            float kr = r.Height / 2f - P(3), kx = on ? r.Right - r.Height / 2f : r.X + r.Height / 2f, ky = r.Y + r.Height / 2f;
            g.FillEllipse(Brushes.White, kx - kr, ky - kr, 2 * kr, 2 * kr);
        }

        void Divider(Graphics g, int y) { using (var b = new SolidBrush(DIVIDER)) g.FillRectangle(b, S(40), y, S(W - 80), Math.Max(1, S(1))); }

        /// Text vertically centred on cy.
        void TextMid(Graphics g, string t, float x, float cy, float px, Color c)
        {
            var f = Ui.Font(px);
            Ui.Text(g, t, f, c, x, cy - Ui.LineHeight(f) / 2);
        }

        void TextCenter(Graphics g, string t, Rectangle r, float px, Color c)
        {
            TextMid(g, t, r.X + (r.Width - Ui.Measure(t, Ui.Font(px)).Width) / 2, r.Y + r.Height / 2f, px, c);
        }

        /// Shorten with an ellipsis in the middle, keeping the end of a path visible.
        static string Ellipsize(string t, Font f, float maxW)
        {
            if (Ui.Measure(t, f).Width <= maxW) return t;
            for (int keep = t.Length - 1; keep > 4; keep--)
            {
                int head = keep / 3;
                string s = t.Substring(0, head) + "\u2026" + t.Substring(t.Length - (keep - head));
                if (Ui.Measure(s, f).Width <= maxW) return s;
            }
            return "\u2026";
        }
    }

    // ----- reading text (OCR) ----------------------------------------------------------------------

    /// A line of text and where it was found, in the screenshot's own pixels.
    class OcrLine
    {
        public RectangleF Box;
        public string Text;
        public OcrLine(RectangleF box, string text) { Box = box; Text = text; }
    }

    /// One reading: how the picture was prepared for Windows OCR, and what it saw.
    class OcrPass
    {
        public string Name;
        public List<OcrLine> Lines;
    }

    /// Reads text the careful way. The screenshot goes to Windows OCR several times, prepared
    /// differently each time (enlarged to the letter size it reads best, grey, black and white,
    /// thicker), and each line is the version the readings agree on most. In IP and MAC
    /// addresses and numbers, look-alike letters are put right (O to 0, l to 1).
    static class Reader
    {
        // Measured on screenshots of addresses: Windows OCR reads best when a line's box is about
        // this many pixels tall (12 px text enlarged 2 to 2.5 times); much larger reads worse.
        const float Target = 24;
        const int Pad = 24;        // a white margin: text touching the edge is often missed
        enum Prep { None, Bw }

        internal static List<OcrPass> Read(Bitmap img, bool harder, Func<Bitmap, string> ocr, int maxDim)
        {
            var passes = new List<OcrPass>();
            var done = new List<KeyValuePair<string, float>>();
            float side = Math.Max(img.Width, img.Height), limit = (maxDim > 0 ? maxDim : 4096) - 2 * Pad;
            using (var grey = Grey(img))
            {
                var ink = new Ink(grey);
                // Each way once: the same preparation at nearly the same size reads the same.
                Action<string, Bitmap, float, Prep> pass = (name, src, k, prep) =>
                {
                    k = Math.Max(0.25f, Math.Min(k, limit / side));
                    if (done.Exists(d => d.Key == name && Math.Abs(d.Value - k) < 0.2f)) return;
                    done.Add(new KeyValuePair<string, float>(name, k));
                    passes.Add(Pass(ocr, name, src, k, prep, ink));
                };
                pass("grey", grey, 2, Prep.None);
                float h = LineHeight(passes[0]);
                float s = h > 0 ? Math.Max(1.5f, Math.Min(4, Target / h)) : 2;
                pass("grey", grey, s, Prep.None);
                pass("original colours", img, s, Prep.None);
                pass("black and white", grey, s, Prep.Bw);
                pass("grey", grey, s * 1.25f, Prep.None);
                pass("grey", grey, s * 0.8f, Prep.None);
                if (harder)
                {
                    pass("original colours", img, s * 0.8f, Prep.None);
                    pass("original colours", img, s * 1.25f, Prep.None);
                    pass("black and white", grey, s * 0.8f, Prep.Bw);
                    pass("grey", grey, 1.5f, Prep.None);
                    pass("grey", grey, 3, Prep.None);
                    pass("original colours", img, 2, Prep.None);
                }
            }
            return passes;
        }

        static OcrPass Pass(Func<Bitmap, string> ocr, string name, Bitmap src, float s, Prep prep, Ink ink)
        {
            int w = Math.Max(1, (int)Math.Round(src.Width * s)), h = Math.Max(1, (int)Math.Round(src.Height * s));
            using (var b = new Bitmap(w + 2 * Pad, h + 2 * Pad, PixelFormat.Format32bppArgb))
            {
                using (var g = Graphics.FromImage(b))
                using (var ia = new ImageAttributes())
                {
                    g.Clear(Color.White);
                    g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                    g.PixelOffsetMode = PixelOffsetMode.HighQuality;
                    ia.SetWrapMode(WrapMode.TileFlipXY);
                    g.DrawImage(src, new Rectangle(Pad, Pad, w, h), 0, 0, src.Width, src.Height, GraphicsUnit.Pixel, ia);
                }
                if (prep == Prep.Bw) BlackAndWhite(b);
                return new OcrPass { Name = name + ", " + s.ToString("0.#", CultureInfo.InvariantCulture) + "\u00d7", Lines = Parse(ocr(b) ?? "", s, Pad, ink) };
            }
        }

        static byte[] Pixels(Bitmap b)
        {
            var d = b.LockBits(new Rectangle(0, 0, b.Width, b.Height), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
            var px = new byte[d.Stride * b.Height];
            Marshal.Copy(d.Scan0, px, 0, px.Length);
            b.UnlockBits(d);
            return px;
        }

        static void SetPixels(Bitmap b, byte[] px)
        {
            var d = b.LockBits(new Rectangle(0, 0, b.Width, b.Height), ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
            Marshal.Copy(px, 0, d.Scan0, px.Length);
            b.UnlockBits(d);
        }

        static int Percentile(int[] hist, int n, float q)
        {
            int at = (int)(n * q), sum = 0;
            for (int i = 0; i < 256; i++) { sum += hist[i]; if (sum > at) return i; }
            return 255;
        }

        /// Grey, dark text on a light background, at full contrast.
        internal static Bitmap Grey(Bitmap img)
        {
            int w = img.Width, h = img.Height, n = w * h;
            var b = new Bitmap(w, h, PixelFormat.Format32bppArgb);
            using (var g = Graphics.FromImage(b)) g.DrawImage(img, new Rectangle(0, 0, w, h), 0, 0, w, h, GraphicsUnit.Pixel);
            var px = Pixels(b);
            var hist = new int[256];
            for (int i = 0; i < px.Length; i += 4)
            {
                int l = (px[i + 2] * 299 + px[i + 1] * 587 + px[i] * 114) / 1000;
                px[i] = (byte)l;
                hist[l]++;
            }
            // Mostly dark means light text on a dark background: turn it around.
            bool invert = Percentile(hist, n, 0.5f) < 128;
            int lo = Percentile(hist, n, 0.01f), hi = Percentile(hist, n, 0.99f);
            if (hi - lo < 40) { lo = 0; hi = 255; }
            for (int i = 0; i < px.Length; i += 4)
            {
                int v = Ui.Clamp((px[i] - lo) * 255 / (hi - lo), 0, 255);
                if (invert) v = 255 - v;
                px[i] = px[i + 1] = px[i + 2] = (byte)v;
                px[i + 3] = 255;
            }
            SetPixels(b, px);
            return b;
        }

        /// Otsu's threshold: the grey level that best splits text from background.
        internal static int Otsu(int[] hist, int n)
        {
            double sum = 0, sumB = 0, best = -1;
            for (int i = 0; i < 256; i++) sum += i * (double)hist[i];
            int wB = 0, t = 128;
            for (int i = 0; i < 256; i++)
            {
                wB += hist[i];
                if (wB == 0) continue;
                int wF = n - wB;
                if (wF == 0) break;
                sumB += i * (double)hist[i];
                double mB = sumB / wB, mF = (sum - sumB) / wF, between = (double)wB * wF * (mB - mF) * (mB - mF);
                if (between > best) { best = between; t = i; }
            }
            return t;
        }

        static void BlackAndWhite(Bitmap b)
        {
            var px = Pixels(b);
            var hist = new int[256];
            for (int i = 0; i < px.Length; i += 4) hist[px[i]]++;
            int t = Otsu(hist, px.Length / 4);
            for (int i = 0; i < px.Length; i += 4) px[i] = px[i + 1] = px[i + 2] = (byte)(px[i] <= t ? 0 : 255);
            SetPixels(b, px);
        }

        static float LineHeight(OcrPass p)
        {
            if (p.Lines.Count == 0) return 0;
            var hs = new List<float>();
            foreach (var l in p.Lines) hs.Add(l.Box.Height);
            hs.Sort();
            return hs[hs.Count / 2];
        }

        /// Shot.ps1 hands over one word per row: line number, x, y, width, height, text,
        /// in the enlarged picture's pixels.
        internal static List<OcrLine> Parse(string raw, float s, int pad, Ink ink)
        {
            var rows = new List<List<KeyValuePair<RectangleF, string>>>();
            string at = null;
            foreach (var row in raw.Split('\n'))
            {
                var f = row.TrimEnd('\r').Split(new[] { '\t' }, 6);
                if (f.Length < 6 || f[5].Length == 0) continue;
                if (f[0] != at) rows.Add(new List<KeyValuePair<RectangleF, string>>());
                at = f[0];
                Func<int, float> num = i => float.Parse(f[i], CultureInfo.InvariantCulture);
                rows[rows.Count - 1].Add(new KeyValuePair<RectangleF, string>(new RectangleF((num(1) - pad) / s, (num(2) - pad) / s, num(3) / s, num(4) / s), f[5]));
            }
            // OCR breaks a line at a wide gap ("Listening on 0." | "0.0.0:8080"); a gap
            // narrower than the letters are tall is still the same line.
            for (bool merged = true; merged; )
            {
                merged = false;
                for (int i = 0; i < rows.Count && !merged; i++)
                    for (int j = 0; j < rows.Count && !merged; j++)
                    {
                        if (i == j) continue;
                        RectangleF a = Box(rows[i]), b = Box(rows[j]);
                        float v = Math.Min(a.Bottom, b.Bottom) - Math.Max(a.Top, b.Top), h = Math.Max(a.Height, b.Height), gap = b.Left - a.Right;
                        if (v < 0.6f * Math.Min(a.Height, b.Height) || gap < -0.2f * h || gap > h) continue;
                        rows[i].AddRange(rows[j]);
                        rows.RemoveAt(j);
                        merged = true;
                    }
            }
            if (ink != null) foreach (var r in rows) Pipes(ink, r);
            return rows.ConvertAll<OcrLine>(Line);
        }

        /// How dark each pixel of the grey picture is, for finding the | that OCR gets wrong.
        internal class Ink
        {
            public readonly int W, H;
            readonly byte[] d;
            public Ink(Bitmap grey)
            {
                W = grey.Width;
                H = grey.Height;
                var px = Pixels(grey);
                d = new byte[W * H];
                for (int i = 0; i < d.Length; i++) d[i] = (byte)(255 - px[i * 4]);
            }
            // Fairly low: a thin stroke between two pixel columns is only half dark in each.
            public bool At(int x, int y) { return x >= 0 && y >= 0 && x < W && y < H && d[y * W + x] > 100; }
        }

        /// Windows OCR reads | as l or I, or leaves it out. A | is a thin straight stroke that
        /// stands alone, reaches up to the top of the letters and goes below the line they sit
        /// on, which l, I and 1 never do. Each one found is put right in `words`, or added.
        static void Pipes(Ink ink, List<KeyValuePair<RectangleF, string>> words)
        {
            var box = Box(words);
            float lh = box.Height;
            int x0 = Math.Max(0, (int)(box.Left - lh)), x1 = Math.Min(ink.W - 1, (int)(box.Right + lh));
            int y0 = Math.Max(0, (int)(box.Top - 0.5f * lh)), y1 = Math.Min(ink.H - 1, (int)(box.Bottom + 0.5f * lh));
            int n = x1 - x0 + 1;
            if (n < 3 || y1 - y0 < 6) return;

            // Each column's longest run of ink.
            var top = new int[n];
            var bot = new int[n];
            for (int i = 0; i < n; i++)
            {
                top[i] = -1;
                for (int y = y0, run = 0, best = 0; y <= y1; y++)
                {
                    if (!ink.At(x0 + i, y)) { run = 0; continue; }
                    if (++run > best) { best = run; top[i] = y - run + 1; bot[i] = y; }
                }
            }
            // The band of small letters (or digits) holds the most ink per row: from its top (mid)
            // down to the line the letters sit on (baseLine).
            int r0 = Math.Max(0, (int)box.Top), r1 = Math.Min(ink.H - 1, (int)Math.Ceiling(box.Bottom));
            if (r1 - r0 < 4) return;
            var rows = new int[r1 - r0 + 1];
            int peak = 0;
            for (int y = r0; y <= r1; y++)
            {
                for (int x = x0; x <= x1; x++) if (ink.At(x, y)) rows[y - r0]++;
                if (rows[y - r0] > rows[peak]) peak = y - r0;
            }
            int lim = Math.Max(1, rows[peak] * 2 / 5), lo = peak, hi = peak;
            while (lo > 0 && rows[lo - 1] >= lim) lo--;
            while (hi < rows.Length - 1 && rows[hi + 1] >= lim) hi++;
            int mid = r0 + lo, baseLine = r0 + hi, body = baseLine - mid;
            if (body < 4) return;
            int side = Math.Max(1, (int)Math.Round(0.25 * body)), maxW = Math.Max(2, (int)Math.Round(0.25 * body));

            Func<int, bool> tall = i => top[i] >= 0 && top[i] > y0 && bot[i] < y1
                && top[i] <= mid + 0.15 * body && bot[i] >= baseLine + Math.Max(1, 0.12 * body);
            for (int i = 0; i < n; i++)
            {
                if (!tall(i)) continue;
                int j = i;
                while (j + 1 < n && tall(j + 1)) j++;
                int gL = i, gR = j;
                i = j;
                if (gR - gL + 1 > maxW) continue;
                int ts = int.MaxValue, bs = 0;
                for (int k = gL; k <= gR; k++) { ts = Math.Min(ts, top[k]); bs = Math.Max(bs, bot[k]); }
                // Alone: nothing beside it from the top of the small letters down (a p or q has its
                // bowl there), and no arms at its ends (a [ or ]).
                // (Its own soft edge, one pixel either side, is not a neighbour.)
                bool alone = true;
                for (int y = ts; y <= bs && alone; y++)
                    for (int k = 2; k <= side + 1 && alone; k++)
                        if (ink.At(x0 + gL - k, y) || ink.At(x0 + gR + k, y)) alone = false;
                if (!alone) continue;

                float cx = x0 + (gL + gR) / 2f + 0.5f;
                int w = words.FindIndex(p => cx >= p.Key.Left - 0.1f * lh && cx <= p.Key.Right + 0.1f * lh);
                if (w < 0)
                {
                    // OCR left it out.
                    int at = words.FindIndex(p => p.Key.Left > cx);
                    words.Insert(at < 0 ? words.Count : at, new KeyValuePair<RectangleF, string>(new RectangleF(cx - 1, ts, 2, bs - ts + 1), "|"));
                    continue;
                }
                // The l, I or 1 nearest to where the stroke is in the word.
                var word = words[w];
                string t = word.Value;
                float pos = (cx - word.Key.Left) / Math.Max(1, word.Key.Width) * t.Length - 0.5f;
                int pick = -1;
                for (int c = 0; c < t.Length; c++)
                    if ("lI1|!i".IndexOf(t[c]) >= 0 && Math.Abs(c - pos) <= 1.5f && (pick < 0 || Math.Abs(c - pos) < Math.Abs(pick - pos))) pick = c;
                if (pick < 0) continue;
                // It stands alone, so OCR's "MACI" was "MAC |" (but "II" stays together as "||").
                string before = t.Substring(0, pick), after = t.Substring(pick + 1);
                if (before.Length > 0 && "lI1|!i".IndexOf(before[before.Length - 1]) < 0) before += " ";
                if (after.Length > 0 && "lI1|!i".IndexOf(after[0]) < 0) after = " " + after;
                words[w] = new KeyValuePair<RectangleF, string>(word.Key, before + "|" + after);
            }
        }

        static RectangleF Box(List<KeyValuePair<RectangleF, string>> words)
        {
            var box = words[0].Key;
            foreach (var w in words) box = RectangleF.Union(box, w.Key);
            return box;
        }

        static OcrLine Line(List<KeyValuePair<RectangleF, string>> words)
        {
            float h = 0;
            foreach (var w in words) h = Math.Max(h, w.Key.Height);
            var box = words[0].Key;
            var sb = new StringBuilder(words[0].Value);
            string word = words[0].Value;
            for (int i = 1; i < words.Count; i++)
            {
                var a = words[i - 1];
                var b = words[i];
                float gap = b.Key.Left - a.Key.Right;
                // (|| in a code font: two strokes a letter apart; "| |" is three letters apart.)
                bool pipes = word.EndsWith("|") && b.Value.StartsWith("|") && gap < 0.8f * h;
                if (gap < 0.1f * h || pipes || gap < 0.6f * h && Glue(word, b.Value)) word += b.Value;
                else { sb.Append(' '); word = b.Value; }
                sb.Append(b.Value);
                box = RectangleF.Union(box, b.Key);
            }
            return new OcrLine(box, sb.ToString());
        }

        const string Seps = ".:-";

        /// "192.168." + "1.10", "00:1" + "a:2b:3c:4d:5e": an address that OCR split in two.
        internal static bool Glue(string a, string b)
        {
            if (a.EndsWith("|") || b.StartsWith("|")) return false;
            if (Address(Trim(a + b)) != null) return true;
            if (Seps.IndexOf(a[a.Length - 1]) < 0 && Seps.IndexOf(b[0]) < 0) return false;
            return Part(a) && Part(b);
        }

        static string Trim(string t) { return t.Trim("([{<|\"'".ToCharArray()).TrimEnd(",;)]}>|\"'.".ToCharArray()); }

        // Hex digits and separators only, with a digit in it or one group of two (a MAC's "AA").
        static bool Part(string s)
        {
            bool digit = false;
            foreach (char c in s)
            {
                if (c >= '0' && c <= '9') digit = true;
                else if (Seps.IndexOf(c) < 0 && !Uri.IsHexDigit(c) && HexFrom.IndexOf(c) < 0) return false;
            }
            return digit || s.Trim(Seps.ToCharArray()).Length == 2;
        }

        static readonly Regex IPv4 = new Regex(@"^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(/\d{1,2})?(:\d{1,5})?$"),
            Mac = new Regex(@"^[0-9A-Fa-f]{2}([:-])[0-9A-Fa-f]{2}(\1[0-9A-Fa-f]{2}){4}$"),
            MacDots = new Regex(@"^[0-9A-Fa-f]{4}\.[0-9A-Fa-f]{4}\.[0-9A-Fa-f]{4}$"),
            IPv6 = new Regex(@"^((?=.*::)[0-9A-Fa-f]{0,4}(:[0-9A-Fa-f]{0,4}){2,7}|[0-9A-Fa-f]{1,4}(:[0-9A-Fa-f]{1,4}){7})(/\d{1,3})?$");

        // What OCR mistakes for a digit (\u00f8 and e: the slashed zero of Consolas and other code fonts).
        // Plain numbers only take the safest ones.
        const string DigitFrom = "OoQDe\u00f8\u00d8\u03b8lIi!SsBZzgqGbT", DigitTo = "0000000011115582299667";
        const string HexFrom = "OoQ\u00f8\u00d8\u03b8lIi!SsZzGT", HexTo = "0000001111552267";
        const string PlainFrom = "Oo\u00f8\u00d8lI", PlainTo = "000011";

        static string Map(string s, string from, string to, string keep)
        {
            var sb = new StringBuilder(s.Length);
            foreach (char c in s)
            {
                int k;
                if (c >= '0' && c <= '9' || keep.IndexOf(c) >= 0) sb.Append(c);
                else if ((k = from.IndexOf(c)) >= 0) sb.Append(to[k]);
                else return null;
            }
            return sb.ToString();
        }

        static int Count(string s, Func<char, bool> f) { int n = 0; foreach (char c in s) if (f(c)) n++; return n; }

        internal static string Fix(string line)
        {
            var parts = line.Split(' ');
            for (int i = 0; i < parts.Length; i++) parts[i] = FixWord(parts[i]);
            return string.Join(" ", parts);
        }

        /// One word, with any brackets, quotes and trailing punctuation left as they are.
        internal static string FixWord(string t)
        {
            int a = 0, b = t.Length;
            while (a < b && "([{<|\"'".IndexOf(t[a]) >= 0) a++;
            while (b > a && ",;)]}>|\"'.".IndexOf(t[b - 1]) >= 0) b--;
            string core = t.Substring(a, b - a), done = FixCore(core);
            return done == null ? t : t.Substring(0, a) + done + t.Substring(b);
        }

        static string FixCore(string c)
        {
            string a = Address(c);
            if (a != null) return a;
            // A plain number, date or time (1O24, 2O26-1O-28), or the numbers in SN-2O26-1O28, bytes=3Z.
            var n = Plain(c);
            if (n != null) return n;
            var parts = Regex.Split(c, "([-=:/_])");
            bool any = false;
            for (int i = 0; i < parts.Length; i += 2)
            {
                var p = Plain(parts[i]);
                if (p != null) { parts[i] = p; any = true; }
            }
            return any ? string.Concat(parts) : null;
        }

        static string Plain(string c)
        {
            int digits = Count(c, ch => ch >= '0' && ch <= '9'), swaps = Count(c, ch => PlainFrom.IndexOf(ch) >= 0);
            return c.Length >= 3 && digits >= 2 && digits > swaps ? Map(c, PlainFrom, PlainTo, ".,:-/") : null;
        }

        /// An IP or MAC address with its look-alike letters put right, or null.
        static string Address(string c)
        {
            int digits = Count(c, ch => ch >= '0' && ch <= '9');
            if (c.Length < 3) return null;

            // IPv4, also with a comma read for a dot, a /prefix or a :port. "O.O.O.O" is 0.0.0.0,
            // but "S.S.S.S" could be 5s or 8s, so it needs some digits or zeros to go on.
            if (Count(c, ch => ch == '.' || ch == ',') == 3 && Count(c, ch => ch >= '0' && ch <= '9' || "Oo\u00f8\u00d8\u03b8".IndexOf(ch) >= 0) >= 2)
            {
                var ip = Map(c.Replace(',', '.'), DigitFrom, DigitTo, ".:/");
                if (ip != null && IsIPv4(ip)) return ip;
            }
            // MAC: six pairs; a ; . , or a dot in the middle of the line read for a separator.
            const string MacSeps = ":-.;,\u2022\u00b7";
            if (c.Length == 17 && Count(c, ch => MacSeps.IndexOf(ch) >= 0) == 5)
            {
                char sep = c.IndexOf('-') >= 0 && c.IndexOf(':') < 0 ? '-' : ':';
                var sb = new StringBuilder(c);
                for (int i = 0; i < sb.Length; i++) if (MacSeps.IndexOf(sb[i]) >= 0) sb[i] = sep;
                var m = Map(sb.ToString(), HexFrom, HexTo, "abcdefABCDEF:-");
                if (m != null && Mac.IsMatch(m)) return SameCase(m);
            }
            // MAC with a separator lost ("00:1a:2b:3c4d:5e"): twelve hex digits, four or five separators of one kind.
            foreach (char sep in ":-")
            {
                if (Count(c, ch => ch == sep) < 4 || Count(c, ch => ch == sep) > 5) continue;
                var m = Map(c.Replace(sep.ToString(), ""), HexFrom, HexTo, "abcdefABCDEF");
                if (m == null || m.Length != 12) continue;
                var sb = new StringBuilder();
                for (int i = 0; i < 12; i += 2) sb.Append(i > 0 ? sep.ToString() : "").Append(m, i, 2);
                return SameCase(sb.ToString());
            }
            if (c.Length == 14 && Count(c, ch => ch == '.') == 2)
            {
                var m = Map(c, HexFrom, HexTo, "abcdefABCDEF.");
                if (m != null && MacDots.IsMatch(m)) return SameCase(m);
            }
            if (digits == 0) return null;
            // IPv6.
            if (Count(c, ch => ch == ':') >= 2)
            {
                var m = Map(c.Replace('\u2022', ':').Replace('\u00b7', ':'), HexFrom, HexTo, "abcdefABCDEF:/");
                if (m != null && IPv6.IsMatch(m) && m.IndexOf("::") == m.LastIndexOf("::")) return m;
            }
            return null;
        }

        static bool IsIPv4(string s)
        {
            var m = IPv4.Match(s);
            if (!m.Success) return false;
            for (int i = 1; i <= 4; i++) if (int.Parse(m.Groups[i].Value, CultureInfo.InvariantCulture) > 255) return false;
            return true;
        }

        // A MAC address is all capitals or all small letters.
        static string SameCase(string m)
        {
            return Count(m, char.IsUpper) >= Count(m, char.IsLower) ? m.ToUpperInvariant() : m.ToLowerInvariant();
        }

        /// Lines in reading order; lines side by side (columns, tables) share a row, tab apart.
        internal static string Layout(List<OcrLine> lines)
        {
            var ls = new List<OcrLine>(lines);
            ls.Sort((a, b) => a.Box.Top.CompareTo(b.Box.Top));
            var used = new bool[ls.Count];
            var sb = new StringBuilder();
            for (int i = 0; i < ls.Count; i++)
            {
                if (used[i]) continue;
                float mid = ls[i].Box.Top + ls[i].Box.Height / 2;
                var row = new List<OcrLine>();
                for (int j = i; j < ls.Count; j++)
                    if (!used[j] && ls[j].Box.Top <= mid && ls[j].Box.Bottom >= mid) { used[j] = true; row.Add(ls[j]); }
                row.Sort((a, b) => a.Box.Left.CompareTo(b.Box.Left));
                if (sb.Length > 0) sb.Append("\r\n");
                for (int k = 0; k < row.Count; k++) sb.Append(k > 0 ? "\t" : "").Append(row[k].Text);
            }
            return sb.ToString();
        }

        internal static string PassText(OcrPass p, bool fix)
        {
            var ls = new List<OcrLine>();
            foreach (var l in p.Lines) ls.Add(new OcrLine(l.Box, fix ? Fix(l.Text) : l.Text));
            return Layout(ls);
        }

        static bool Same(RectangleF a, RectangleF b)
        {
            float v = Math.Min(a.Bottom, b.Bottom) - Math.Max(a.Top, b.Top);
            return v >= 0.5f * Math.Min(a.Height, b.Height) && Math.Min(a.Right, b.Right) > Math.Max(a.Left, b.Left);
        }

        /// Line by line, the text the readings agree on most. `agree`: how many readings
        /// had exactly that line, in percent.
        internal static string Vote(List<OcrPass> passes, bool fix, out int agree)
        {
            // Group the lines that sit in the same place; a line that spans two groups joins them.
            var groups = new List<List<KeyValuePair<int, OcrLine>>>();
            for (int p = 0; p < passes.Count; p++)
                foreach (var l in passes[p].Lines)
                {
                    var home = new List<KeyValuePair<int, OcrLine>> { new KeyValuePair<int, OcrLine>(p, l) };
                    foreach (var g in groups.FindAll(g => g.Exists(m => Same(m.Value.Box, l.Box)))) { home.AddRange(g); groups.Remove(g); }
                    groups.Add(home);
                }
            var chosen = new List<OcrLine>();
            int matched = 0, total = 0;
            foreach (var g in groups)
            {
                // One version per reading (a reading may have split the line in two), best readings first.
                var byPass = new SortedDictionary<int, List<OcrLine>>();
                var box = g[0].Value.Box;
                foreach (var m in g)
                {
                    if (!byPass.ContainsKey(m.Key)) byPass[m.Key] = new List<OcrLine>();
                    byPass[m.Key].Add(m.Value);
                    box = RectangleF.Union(box, m.Value.Box);
                }
                // Seen by just one of several readings: a speck, not text.
                if (byPass.Count == 1 && passes.Count >= 3) continue;
                var cands = new List<string>();
                foreach (var ls in byPass.Values)
                {
                    ls.Sort((a, b) => a.Box.Left.CompareTo(b.Box.Left));
                    var t = string.Join(" ", ls.ConvertAll(l => l.Text).ToArray());
                    cands.Add(fix ? Fix(t) : t);
                }
                // The version closest to all the others, then word by word what most of them say.
                // Windows OCR leaves out words it is unsure of rather than misreading them, so a
                // version with a valid address in it counts as if some more readings agreed.
                int best = 0;
                double top = -1;
                for (int i = 0; i < cands.Count; i++)
                {
                    double score = 0.25 * (cands.Count - 1) * Array.FindAll(cands[i].Split(' '), x => Address(Trim(x)) != null).Length;
                    for (int j = 0; j < cands.Count; j++) if (j != i) score += Similar(cands[i], cands[j]);
                    if (score > top + 1e-9) { top = score; best = i; }
                }
                string line = Words(cands, best);
                matched += cands.FindAll(c => c == line).Count;
                total += passes.Count;
                chosen.Add(new OcrLine(box, line));
            }
            agree = total == 0 ? 0 : matched * 100 / total;
            return Layout(chosen);
        }

        /// Each word of cands[main], replaced by the spelling most versions have in its place.
        static string Words(List<string> cands, int main)
        {
            var w = cands[main].Split(' ');
            var votes = new List<Dictionary<string, int>>();
            foreach (var x in w) votes.Add(new Dictionary<string, int> { { x, 1 } });
            for (int c = 0; c < cands.Count; c++)
            {
                if (c == main) continue;
                var o = cands[c].Split(' ');
                foreach (var pair in Align(w, o))
                {
                    var v = votes[pair.Key];
                    int n;
                    v.TryGetValue(o[pair.Value], out n);
                    v[o[pair.Value]] = n + 1;
                }
            }
            // A valid address beats a broken one when the votes are close.
            Func<string, double> weight = x => Address(Trim(x)) != null ? 0.5 : 0;
            for (int i = 0; i < w.Length; i++)
                foreach (var kv in votes[i])
                    if (kv.Value + weight(kv.Key) > votes[i][w[i]] + weight(w[i])) w[i] = kv.Key;
            return string.Join(" ", w);
        }

        /// Pairs of words (index in a, index in b) that line up, by edit distance over words.
        static List<KeyValuePair<int, int>> Align(string[] a, string[] b)
        {
            int n = a.Length, m = b.Length;
            var d = new double[n + 1, m + 1];
            for (int i = 0; i <= n; i++) d[i, 0] = i;
            for (int j = 0; j <= m; j++) d[0, j] = j;
            for (int i = 1; i <= n; i++)
                for (int j = 1; j <= m; j++)
                    d[i, j] = Math.Min(Math.Min(d[i - 1, j] + 1, d[i, j - 1] + 1), d[i - 1, j - 1] + 1 - Similar(a[i - 1], b[j - 1]));
            var pairs = new List<KeyValuePair<int, int>>();
            for (int i = n, j = m; i > 0 && j > 0; )
            {
                if (Math.Abs(d[i, j] - (d[i - 1, j - 1] + 1 - Similar(a[i - 1], b[j - 1]))) < 1e-9)
                {
                    // Only words that look alike count as the same word read differently.
                    if (Similar(a[i - 1], b[j - 1]) >= 0.5) pairs.Add(new KeyValuePair<int, int>(i - 1, j - 1));
                    i--; j--;
                }
                else if (Math.Abs(d[i, j] - (d[i - 1, j] + 1)) < 1e-9) i--;
                else j--;
            }
            return pairs;
        }

        /// 1 for the same text, down to 0: one minus the edit distance over the length.
        internal static double Similar(string a, string b)
        {
            if (a == b) return 1;
            int n = a.Length, m = b.Length;
            var d = new int[m + 1];
            for (int j = 0; j <= m; j++) d[j] = j;
            for (int i = 1; i <= n; i++)
            {
                int diag = d[0];
                d[0] = i;
                for (int j = 1; j <= m; j++)
                {
                    int up = d[j];
                    d[j] = Math.Min(Math.Min(d[j] + 1, d[j - 1] + 1), diag + (a[i - 1] == b[j - 1] ? 0 : 1));
                    diag = up;
                }
            }
            return 1 - (double)d[m] / Math.Max(n, m);
        }
    }

    /// What OCR read, before anything is copied: fix it by hand, flip through the other
    /// readings, read harder, or take the screenshot again.
    class OcrPreview : Form
    {
        static readonly Color BG = Ui.Hex(0x111113), CARD = Ui.Hex(0x1C1C1F), TEXT = Ui.Hex(0xF2F2F7),
            MUTED = Ui.Hex(0x8E8E96), CONTROL = Ui.Hex(0x2C2C30), CONTROL_HOVER = Ui.Hex(0x3A3A40), ACCENT = Ui.Hex(0xE0182D);
        readonly Bitmap img;
        readonly float sc;
        readonly PictureBox pic;
        readonly TextBox box;
        readonly Label info;
        readonly CheckBox fixBox;
        readonly Button prev, next, harder, copy;
        List<OcrPass> passes = new List<OcrPass>();
        string best = "", bestRaw = "";
        int agree, agreeRaw, shown;
        bool readHarder;

        int S(float v) { return (int)Math.Round(v * sc); }

        public OcrPreview(Bitmap image)
        {
            img = image;
            sc = Native.ScaleAt(Cursor.Position);
            var mon = Screen.FromPoint(Cursor.Position).WorkingArea;
            Text = "Shot: copy text";
            Icon = App.AppIcon;
            AutoScaleMode = AutoScaleMode.None;
            StartPosition = FormStartPosition.Manual;
            BackColor = BG;
            ForeColor = TEXT;
            Font = Ui.Font(14 * sc);
            KeyPreview = true;
            var size = new Size(Math.Min(S(820), mon.Width), Math.Min(S(600), mon.Height));
            Bounds = new Rectangle(mon.X + (mon.Width - size.Width) / 2, mon.Y + (mon.Height - size.Height) / 2, size.Width, size.Height);
            MinimumSize = new Size(S(560), S(380));

            pic = new PictureBox { Dock = DockStyle.Top, Image = img, SizeMode = PictureBoxSizeMode.Zoom, BackColor = CARD };
            pic.Height = Ui.Clamp(img.Height + S(20), S(48), S(200));
            info = new Label { Dock = DockStyle.Top, Height = S(34), ForeColor = MUTED, TextAlign = ContentAlignment.MiddleLeft, Padding = new Padding(S(14), 0, 0, 0), Text = "Reading\u2026" };
            box = new TextBox
            {
                Dock = DockStyle.Fill, Multiline = true, AcceptsReturn = false, WordWrap = false, ScrollBars = ScrollBars.Both,
                BorderStyle = BorderStyle.None, BackColor = CARD, ForeColor = TEXT, Font = new Font("Consolas", 16 * sc, GraphicsUnit.Pixel), ReadOnly = true
            };
            var textArea = new Panel { Dock = DockStyle.Fill, Padding = new Padding(S(14), S(10), S(14), S(10)), BackColor = CARD };
            textArea.Controls.Add(box);

            var bar = new FlowLayoutPanel { Dock = DockStyle.Bottom, Height = S(58), FlowDirection = FlowDirection.RightToLeft, WrapContents = false, Padding = new Padding(S(10), S(10), S(10), 0) };
            copy = Btn("Copy", true, Copy);
            var retake = Btn("Retake", false, Retake);
            harder = Btn("Read harder", false, () => Run(true));
            next = Btn("\u25b6", false, () => ShowReading(shown + 1));
            prev = Btn("\u25c0", false, () => ShowReading(shown - 1));
            fixBox = new CheckBox { Text = "Fix IPs, MACs, numbers", Checked = true, AutoSize = true, ForeColor = TEXT, Margin = new Padding(S(4), S(8), S(16), 0) };
            fixBox.CheckedChanged += (s, e) => ShowReading(shown);
            bar.Controls.AddRange(new Control[] { copy, retake, harder, next, prev, fixBox });
            var tips = new ToolTip();
            tips.SetToolTip(prev, "Previous reading");
            tips.SetToolTip(next, "Next reading: what each way of reading saw");
            tips.SetToolTip(harder, "Read again more ways (slower)");
            tips.SetToolTip(retake, "Take the screenshot again");
            tips.SetToolTip(copy, "Copy the text (Enter)");
            AcceptButton = copy;

            // Docked last is laid out first.
            Controls.Add(textArea);
            Controls.Add(bar);
            Controls.Add(info);
            Controls.Add(pic);
            foreach (Control c in bar.Controls) c.Enabled = false;
        }

        Button Btn(string text, bool accent, Action click)
        {
            var b = new Button { Text = text, AutoSize = true, FlatStyle = FlatStyle.Flat, BackColor = accent ? ACCENT : CONTROL, ForeColor = TEXT, Height = S(36), Padding = new Padding(S(8), 0, S(8), 0), Margin = new Padding(S(4), 0, S(4), 0), Cursor = Cursors.Hand };
            b.FlatAppearance.BorderSize = 0;
            b.FlatAppearance.MouseOverBackColor = accent ? Ui.Hex(0xF0283D) : CONTROL_HOVER;
            b.Click += (s, e) => click();
            return b;
        }

        protected override void OnHandleCreated(EventArgs e) { Native.Dark(Handle, false); base.OnHandleCreated(e); }
        protected override void OnShown(EventArgs e) { base.OnShown(e); BeginInvoke((Action)(() => Run(false))); }

        protected override void OnKeyDown(KeyEventArgs e)
        {
            if (e.KeyCode == Keys.Escape) { e.Handled = true; Close(); }
            base.OnKeyDown(e);
        }

        void Run(bool more)
        {
            foreach (Control c in copy.Parent.Controls) c.Enabled = false;
            box.ReadOnly = true;
            info.Text = more ? "Reading harder\u2026" : "Reading\u2026";
            Cursor = Cursors.WaitCursor;
            Update();
            try
            {
                passes = Reader.Read(img, more, App.Ocr, App.OcrMax);
                best = Reader.Vote(passes, true, out agree);
                bestRaw = Reader.Vote(passes, false, out agreeRaw);
                readHarder |= more;
            }
            catch (Exception ex) { passes = new List<OcrPass>(); best = bestRaw = ""; info.Text = "Text recognition failed: " + ex.GetBaseException().Message; }
            Cursor = Cursors.Default;
            foreach (Control c in copy.Parent.Controls) c.Enabled = true;
            box.ReadOnly = false;
            if (passes.Count > 0) ShowReading(0);
            box.Focus();
        }

        /// 0: the best reading; 1..n: what each way of reading saw.
        void ShowReading(int k)
        {
            if (passes.Count == 0) return;
            shown = Ui.Clamp(k, 0, passes.Count);
            bool fix = fixBox.Checked;
            string top = fix ? best : bestRaw, t = shown == 0 ? top : Reader.PassText(passes[shown - 1], fix);
            box.Text = t;
            box.Select(0, 0);
            if (t.Trim().Length == 0) info.Text = "No text found. Try Read harder, or Retake with a larger area.";
            else if (shown == 0) info.Text = "Best of " + passes.Count + " readings  \u00b7  " + (fix ? agree : agreeRaw) + "% agree  \u00b7  edit anything, then Copy";
            else info.Text = "Reading " + shown + " of " + passes.Count + ": " + passes[shown - 1].Name + (t == top ? "  \u00b7  same as the best" : "");
            prev.Enabled = shown > 0;
            next.Enabled = shown < passes.Count;
            harder.Enabled = !readHarder;
            copy.Enabled = t.Trim().Length > 0;
        }

        void Copy()
        {
            string t = box.Text.TrimEnd();
            if (t.Length == 0) return;
            try
            {
                Clipboard.SetText(t);
                int lines = t.Split('\n').Length;
                App.ShowToast("Copied text (" + lines + " line" + (lines == 1 ? ")" : "s)"), 2);
                Close();
            }
            catch (Exception e) { App.ShowToast("Copy failed: " + e.Message, 4); }
        }

        void Retake()
        {
            Close();
            App.CaptureSoon();
        }

        protected override void OnFormClosed(FormClosedEventArgs e)
        {
            pic.Image = null;
            img.Dispose();
            base.OnFormClosed(e);
        }
    }

    // ----- the app: tray, shortcut, captures, outputs ---------------------------------------------

    public static class App
    {
        public const string Version = "2.3.0";
        const string Fallback = "Ctrl+PrintScreen";
        const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";

        /// Reads a picture with Windows OCR, one word per row (see Reader.Parse). Set by
        /// Shot.ps1 when Windows OCR is there, with the largest picture it takes.
        public static Func<Bitmap, string> Ocr;
        public static int OcrMax;

        internal static Config Cfg = new Config();
        internal static Icon AppIcon;
        internal static readonly List<Pin> Pins = new List<Pin>();
        internal static string ActiveHotkey, HotkeyError;
        internal static bool Recording;
        static string root, cfgPath, pausedHotkey;
        static Host host;
        static NotifyIcon tray;
        static ToolStripMenuItem captureItem;
        static Overlay overlay;
        static Toast toast;
        static SettingsForm settings;
        static Spotlight spotlight;
        static readonly CtrlTaps taps = new CtrlTaps();

        public static void Run(string rootDir, bool captureNow, bool openSettings)
        {
            // Physical pixels everywhere, on every monitor.
            if (Native.SetThreadDpiAwarenessContext(new IntPtr(-4)) == IntPtr.Zero) Native.SetProcessDPIAware();
            Application.EnableVisualStyles();
            try { Application.SetUnhandledExceptionMode(UnhandledExceptionMode.CatchException); } catch { }
            Application.ThreadException += (s, e) => { Log(e.Exception); ShowToast("Something went wrong: " + e.Exception.Message, 5); };

            root = rootDir;
            cfgPath = Path.Combine(root, "data", "settings.txt");
            bool first;
            try { Cfg = Config.Load(cfgPath, out first); } catch { first = true; }
            using (var b = Ui.Logo(32)) AppIcon = Icon.FromHandle(b.GetHicon());
            host = new Host();
            if (host.Handle == IntPtr.Zero) throw new InvalidOperationException("no window handle");

            // A second launch of Shot asks this one to show Settings.
            var ev = new EventWaitHandle(false, EventResetMode.AutoReset, @"Local\Shot.OpenSettings");
            var listener = new Thread(() => { while (ev.WaitOne()) host.BeginInvoke((Action)OpenSettings); });
            listener.IsBackground = true;
            listener.Start();
            // Remove Shot.vbs asks it to quit.
            var quit = new EventWaitHandle(false, EventResetMode.AutoReset, @"Local\Shot.Quit");
            var quitter = new Thread(() => { if (quit.WaitOne()) host.BeginInvoke((Action)Quit); });
            quitter.IsBackground = true;
            quitter.Start();

            string wanted = Cfg.Hotkey;
            HotkeyError = Register(wanted);
            bool fallback = HotkeyError != null && Register(Fallback) == null;
            BuildTray();
            if (first || openSettings) OpenSettings();
            else if (ActiveHotkey == null) ShowToast("No capture shortcut works. Open Settings from the tray icon.", 5);
            else if (fallback) ShowToast(Shortcut.Pretty(wanted) + " is used by another app, so Shot uses " + Shortcut.Pretty(ActiveHotkey) + ". Change it in Settings.", 5);
            else if (!captureNow) ShowToast("Shot is running. Press " + Shortcut.Pretty(ActiveHotkey) + " to capture.", 2.5f);
            if (captureNow) host.BeginInvoke((Action)StartCapture);
            WatchCtrl();

            Application.Run();
            tray.Visible = false;
            tray.Dispose();
        }

        internal static void Quit() { Application.Exit(); }

        static void Log(Exception e)
        {
            try { File.AppendAllText(Path.Combine(root, "data", "error.log"), DateTime.Now.ToString("s") + " " + e + "\r\n\r\n"); } catch { }
        }

        internal static void SaveConfig()
        {
            try { Cfg.Save(cfgPath); }
            catch (Exception e) { ShowToast("Settings could not be saved: " + e.Message, 5); }
        }

        // ----- tray ----------------------------------------------------------------------

        static void BuildTray()
        {
            var m = new ContextMenuStrip();
            captureItem = new ToolStripMenuItem("Capture", null, (s, e) => StartCapture());
            m.Items.Add(captureItem);
            m.Items.Add("Settings\u2026", null, (s, e) => OpenSettings());
            m.Items.Add(new ToolStripSeparator());
            m.Items.Add("Open screenshots folder", null, (s, e) => OpenFolder());
            m.Items.Add("Close all pins", null, (s, e) => CloseAllPins());
            m.Items.Add(new ToolStripSeparator());
            m.Items.Add("Quit Shot", null, (s, e) => Quit());
            tray = new NotifyIcon { Icon = AppIcon, ContextMenuStrip = m };
            // Windows remembers tray icons by program and icon number. Every PowerShell tray app
            // (Sharingan Monitor too) is powershell.exe with icon 1, so give Shot a number of its own:
            // then "show or hide in the taskbar" is Shot's own setting.
            var id = typeof(NotifyIcon).GetField("id", System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance);
            if (id != null) id.SetValue(tray, 0x5307);
            tray.Visible = true;
            tray.MouseClick += (s, e) => { if (e.Button == MouseButtons.Left) StartCapture(); };
            UpdateTray();
        }

        static void UpdateTray()
        {
            if (tray == null) return;
            string k = ActiveHotkey == null ? null : Shortcut.Pretty(ActiveHotkey);
            captureItem.ShortcutKeyDisplayString = k ?? "";
            string tip = k == null ? "Shot: click to capture" : "Shot: press " + k + " to capture";
            tray.Text = tip.Length > 63 ? tip.Substring(0, 63) : tip;
        }

        // ----- capture shortcut ------------------------------------------------------------

        /// Switch the global shortcut to `name`; null if that worked, else why not.
        /// On failure the previous one stays.
        static string Register(string name)
        {
            uint mods, vk;
            if (!Shortcut.Parse(name, out mods, out vk)) return Shortcut.Pretty(name) + " can't be used as a shortcut.";
            if (name == ActiveHotkey) return null;
            Native.UnregisterHotKey(host.Handle, 1);
            if (Native.RegisterHotKey(host.Handle, 1, mods | 0x4000, vk)) { ActiveHotkey = name; UpdateTray(); return null; }
            if (ActiveHotkey != null && Shortcut.Parse(ActiveHotkey, out mods, out vk)) Native.RegisterHotKey(host.Handle, 1, mods | 0x4000, vk);
            return Shortcut.Pretty(name) + " is used by another app. Try another shortcut.";
        }

        internal static string ShownHotkey { get { return ActiveHotkey ?? Cfg.Hotkey; } }
        internal static string HotkeyNote
        {
            get { return HotkeyError ?? (ActiveHotkey == null ? "No capture shortcut is set." : "Press it anywhere to take a screenshot."); }
        }

        internal static void StartRecording()
        {
            // Free the current shortcut so pressing it reaches the Settings window.
            pausedHotkey = ActiveHotkey;
            Native.UnregisterHotKey(host.Handle, 1);
            ActiveHotkey = null;
            Recording = true;
            HotkeyError = null;
        }

        /// End recording with the new shortcut, or keep the old one when `name` is null.
        internal static void FinishRecording(string name, string why)
        {
            if (!Recording) return;
            Recording = false;
            string old = pausedHotkey;
            pausedHotkey = null;
            string err = name == null ? why : Register(name);
            if (name != null && err == null) { HotkeyError = null; Cfg.Hotkey = name; SaveConfig(); }
            else
            {
                string back = old == null ? null : Register(old);
                HotkeyError = err ?? back;
            }
            UpdateTray();
        }

        // ----- start at login --------------------------------------------------------------

        internal static string LauncherPath { get { return Path.Combine(root, "Shot.vbs"); } }
        static string StartCommand { get { return "wscript.exe //nologo \"" + LauncherPath + "\""; } }

        internal static bool AutostartOn()
        {
            using (var k = Registry.CurrentUser.OpenSubKey(RunKey))
            {
                var v = k == null ? null : k.GetValue("Shot") as string;
                return v != null && v.Equals(StartCommand, StringComparison.OrdinalIgnoreCase);
            }
        }

        /// The per-user Run key: no admin rights, and this copy runs where it is.
        internal static void SetAutostart(bool on)
        {
            try
            {
                using (var k = Registry.CurrentUser.CreateSubKey(RunKey))
                {
                    if (on) k.SetValue("Shot", StartCommand);
                    else if (AutostartOn()) k.DeleteValue("Shot", false);
                }
            }
            catch (Exception e) { ShowToast("Start-up setting failed: " + e.Message, 5); }
        }

        // ----- windows -----------------------------------------------------------------------

        internal static void OpenSettings()
        {
            if (settings == null)
            {
                settings = new SettingsForm();
                settings.Show();
            }
            if (settings.WindowState == FormWindowState.Minimized) settings.WindowState = FormWindowState.Normal;
            // A background app may not take the focus; topmost for a moment still brings it to the front.
            settings.TopMost = true;
            settings.TopMost = false;
            settings.Activate();
        }

        internal static void SettingsClosed() { settings = null; }

        internal static void ShowToast(string text, float secs)
        {
            if (toast != null) toast.Close();
            toast = new Toast(text, secs);
            var t = toast;
            t.FormClosed += (s, e) => { if (toast == t) toast = null; };
            t.Show();
            t.Update();
        }

        internal static void CloseAllPins() { foreach (var p in Pins.ToArray()) p.Close(); }

        // ----- spotlight -------------------------------------------------------------------

        /// Raw input: Windows copies every key and click to the Host window, even when another
        /// app has the focus. Shot only looks; nothing is held back from other apps.
        static void WatchCtrl()
        {
            var d = new[] {
                new Native.RAWINPUTDEVICE { Page = 1, Usage = 6, Flags = 0x100, Target = host.Handle },
                new Native.RAWINPUTDEVICE { Page = 1, Usage = 2, Flags = 0x100, Target = host.Handle } };
            Native.RegisterRawInputDevices(d, 2, (uint)Marshal.SizeOf(typeof(Native.RAWINPUTDEVICE)));
        }

        internal static void OnRawKey(int vk, bool down)
        {
            if (taps.Key(vk, down, (uint)Environment.TickCount, (uint)SystemInformation.DoubleClickTime)) ToggleSpotlight();
            else if (down && vk == 0x1B && spotlight != null) ToggleSpotlight();
        }

        // Any button or wheel: Ctrl+click and Ctrl+wheel are not taps.
        internal static void OnRawClick() { taps.Spoil(); }

        internal static void ToggleSpotlight()
        {
            if (spotlight != null) { spotlight.Close(); spotlight = null; }
            else if (overlay == null) { spotlight = new Spotlight(); spotlight.Show(); }
        }

        internal static void StartCapture()
        {
            if (overlay != null || host == null) return;
            if (toast != null) toast.Close();
            // Pins and the spotlight must not end up in the screenshot; give the compositor a moment.
            bool hide = Pins.Count > 0 || spotlight != null;
            if (spotlight != null) ToggleSpotlight();
            foreach (var p in Pins) p.Visible = false;
            if (hide) Thread.Sleep(80);
            Bitmap shot;
            var v = SystemInformation.VirtualScreen;
            try
            {
                shot = new Bitmap(v.Width, v.Height, PixelFormat.Format32bppPArgb);
                using (var g = Graphics.FromImage(shot)) g.CopyFromScreen(v.Location, Point.Empty, v.Size);
            }
            catch (Exception e)
            {
                foreach (var p in Pins) p.Visible = true;
                ShowToast("Capture failed: " + e.Message, 4);
                return;
            }
            var wins = WindowRects(v.Location);
            foreach (var p in Pins) p.Visible = true;
            var mons = new List<Rectangle>();
            var scales = new List<float>();
            foreach (var s in Screen.AllScreens)
            {
                var b = s.Bounds;
                mons.Add(new Rectangle(b.X - v.X, b.Y - v.Y, b.Width, b.Height));
                scales.Add(Native.ScaleAt(new Point(b.X + b.Width / 2, b.Y + b.Height / 2)));
            }
            overlay = new Overlay(shot, v, wins, mons, scales, Cfg, Ocr != null);
            overlay.Show();
            overlay.Activate();
            Native.SetForegroundWindow(overlay.Handle);
        }

        /// Visible top-level windows of other apps, topmost first, in capture coordinates.
        static List<Rectangle> WindowRects(Point origin)
        {
            var list = new List<Rectangle>();
            uint me = (uint)Process.GetCurrentProcess().Id;
            Native.EnumProc cb = (h, l) =>
            {
                int cloaked;
                uint pid;
                Native.RECT r;
                if (!Native.IsWindowVisible(h) || Native.IsIconic(h)) return true;
                if (Native.DwmGetWindowAttribute(h, 14, out cloaked, 4) == 0 && cloaked != 0) return true;
                Native.GetWindowThreadProcessId(h, out pid);
                if (pid == me) return true;
                if (Native.DwmGetWindowAttribute(h, 9, out r, 16) != 0 && !Native.GetWindowRect(h, out r)) return true;
                var rc = Rectangle.FromLTRB(r.L - origin.X, r.T - origin.Y, r.R - origin.X, r.B - origin.Y);
                if (rc.Width > 8 && rc.Height > 8) list.Add(rc);
                return true;
            };
            Native.EnumWindows(cb, IntPtr.Zero);
            GC.KeepAlive(cb);
            return list;
        }

        internal static void OverlayClosed(Overlay o) { if (overlay == o) overlay = null; }

        /// What to do with a finished capture.
        internal static void Run(Overlay o, Cmd cmd)
        {
            if (cmd == Cmd.None || overlay != o) return;
            Bitmap img = cmd == Cmd.Cancel ? null : o.Export();
            var t = o.Target();
            var at = new Point(o.Left + t.X, o.Top + t.Y);
            if (o.StyleColor != Cfg.Color || o.StyleMarker != Cfg.Marker || o.StyleSize != Cfg.Size)
            {
                Cfg.Color = o.StyleColor;
                Cfg.Marker = o.StyleMarker;
                Cfg.Size = o.StyleSize;
                SaveConfig();
            }
            overlay = null;
            o.Close();
            if (img == null) return;
            switch (cmd)
            {
                case Cmd.Copy: CopyImage(img); img.Dispose(); break;
                case Cmd.Save: SaveImage(img, Cfg.AskWhere); img.Dispose(); break;
                case Cmd.QuickSave: SaveImage(img, false); img.Dispose(); break;
                case Cmd.Pin:
                    var pin = new Pin(img, at);
                    Pins.Add(pin);
                    pin.Show();
                    pin.Activate();
                    break;
                case Cmd.Ocr: ReadText(img); break;
                default: img.Dispose(); break;
            }
        }

        internal static void CopyImage(Bitmap img)
        {
            try { Clipboard.SetImage(img); ShowToast("Copied to clipboard", 1.4f); }
            catch (Exception e) { ShowToast("Copy failed: " + e.Message, 4); }
        }

        internal static void SaveImage(Bitmap img, bool ask)
        {
            string name = "Shot_" + DateTime.Now.ToString("yyyy-MM-dd_HH-mm-ss", CultureInfo.InvariantCulture) + ".png";
            try
            {
                Directory.CreateDirectory(Cfg.SaveDir);
                string path;
                if (ask)
                {
                    using (var d = new SaveFileDialog { Title = "Save screenshot", InitialDirectory = Cfg.SaveDir, FileName = name, Filter = "PNG image (*.png)|*.png", DefaultExt = "png", AddExtension = true })
                    {
                        if (d.ShowDialog() != DialogResult.OK) return;
                        path = d.FileName;
                    }
                }
                else
                {
                    path = Path.Combine(Cfg.SaveDir, name);
                    for (int i = 2; File.Exists(path); i++) path = Path.Combine(Cfg.SaveDir, Path.GetFileNameWithoutExtension(name) + "_" + i + ".png");
                }
                img.Save(path, ImageFormat.Png);
                if (Cfg.CopyOnSave) try { Clipboard.SetImage(img); } catch { }
                ShowToast("Saved " + Path.GetFileName(path), 2);
            }
            catch (Exception e) { ShowToast("Save failed: " + e.Message, 5); }
        }

        /// Takes ownership of `img`.
        internal static void ReadText(Bitmap img)
        {
            if (Ocr == null) { img.Dispose(); ShowToast("Text recognition is not available on this system", 4); return; }
            var p = new OcrPreview(img);
            p.Show();
            p.TopMost = true;
            p.TopMost = false;
            p.Activate();
        }

        /// A new capture once the window that asked for it has gone from the screen.
        internal static void CaptureSoon()
        {
            var t = new System.Windows.Forms.Timer { Interval = 150 };
            t.Tick += (s, e) => { t.Dispose(); StartCapture(); };
            t.Start();
        }

        internal static void OpenFolder()
        {
            try
            {
                Directory.CreateDirectory(Cfg.SaveDir);
                Process.Start("explorer.exe", "\"" + Cfg.SaveDir + "\"");
            }
            catch (Exception e) { ShowToast("Cannot open the folder: " + e.Message, 4); }
        }

        internal static void PickFolder(IWin32Window owner)
        {
            using (var d = new FolderBrowserDialog { Description = "Folder for screenshots", SelectedPath = Cfg.SaveDir, ShowNewFolderButton = true })
            {
                if (d.ShowDialog(owner) != DialogResult.OK) return;
                Cfg.SaveDir = d.SelectedPath;
                SaveConfig();
            }
        }
    }

    // ----- self-check ------------------------------------------------------------------------------

    public static class Tests
    {
        static Overlay Editor()
        {
            var bmp = new Bitmap(800, 600, PixelFormat.Format32bppPArgb);
            using (var g = Graphics.FromImage(bmp)) g.Clear(Color.FromArgb(200, 200, 200));
            var screen = new Rectangle(0, 0, 800, 600);
            return new Overlay(bmp, screen, new List<Rectangle> { new Rectangle(100, 100, 300, 200), screen },
                new List<Rectangle> { screen }, new List<float> { 1f }, new Config(), false);
        }

        static void Drag(Overlay e, int x0, int y0, int x1, int y1)
        {
            e.MouseDownAt(MouseButtons.Left, new Point(x0, y0));
            e.MouseMoveAt(new Point((x0 + x1) / 2, (y0 + y1) / 2));
            e.MouseMoveAt(new Point(x1, y1));
            e.MouseUpAt(MouseButtons.Left, new Point(x1, y1));
        }

        static void Click(Overlay e, int x, int y) { e.MouseDownAt(MouseButtons.Left, new Point(x, y)); e.MouseUpAt(MouseButtons.Left, new Point(x, y)); }

        public static string Run()
        {
            var fails = new List<string>();
            Action<bool, string> check = (ok, what) => { if (!ok) fails.Add(what); };
            string err;
            uint mods, vk;

            check(Shortcut.FromPress(false, false, true, false, "Space", out err) == "Shift+Space", "shift+space records");
            check(Shortcut.FromPress(true, true, true, true, "KeyS", out err) == "Ctrl+Alt+Shift+Super+KeyS", "modifier order");
            check(Shortcut.FromPress(false, false, false, false, "PrintScreen", out err) == "PrintScreen", "print screen alone");
            check(Shortcut.FromPress(false, false, false, false, "KeyA", out err) == null && err != null, "letter alone refused");
            check(Shortcut.FromPress(false, false, true, false, "KeyA", out err) == null, "shift+letter refused");
            check(Shortcut.Pretty("Ctrl+Alt+KeyS") == "Ctrl + Alt + S" && Shortcut.Pretty("PrintScreen") == "Print Screen", "pretty names");
            check(Shortcut.Parse("Shift+Space", out mods, out vk) && mods == 4 && vk == (uint)Keys.Space, "parse shift+space");
            check(!Shortcut.Parse("Ctrl+Nope", out mods, out vk), "unknown key refused");
            check(Shortcut.CodeOf(Keys.S) == "KeyS" && Shortcut.CodeOf(Keys.D1) == "Digit1", "key codes");

            var c = new Config { Hotkey = "Ctrl+Shift+KeyA", AskWhere = false, CopyOnSave = true, Size = 7, Color = Ui.Hex(0x123456), Marker = Ui.Palette[3], CustomColors = new[] { 0x563412, 0xFFFFFF } };
            var d = Config.Parse(c.Render());
            check(d.Hotkey == c.Hotkey && !d.AskWhere && d.CopyOnSave && d.Size == 7 && d.SaveDir == c.SaveDir
                && d.Color.ToArgb() == c.Color.ToArgb() && d.Marker.ToArgb() == c.Marker.ToArgb()
                && d.CustomColors.Length == 2 && d.CustomColors[0] == 0x563412 && d.CustomColors[1] == 0xFFFFFF, "config round trip");
            check(Ui.Hue(0) == Color.FromArgb(255, 0, 0) && Ui.Hue(120).ToArgb() == Color.FromArgb(0, 255, 0).ToArgb(), "colour wheel hues");
            var j = Config.Parse("hotkey =\nsize = 99\ncolor = nope\nno equals sign\n");
            check(j.Hotkey == Shortcut.Default && j.Size == Config.MaxSize && j.Color == Ui.Palette[0], "junk config falls back");

            using (var e = Editor())
            {
                Drag(e, 50, 60, 250, 160);
                check(e.HasSel && e.Sel == new Rectangle(50, 60, 200, 100), "drag selects region");
                using (var img = e.Export()) check(img.Size == new Size(200, 100), "export is the selection");
            }
            using (var e = Editor())
            {
                Click(e, 150, 150);
                check(e.Sel == new Rectangle(100, 100, 300, 200), "click picks the window");
                e.MouseDownAt(MouseButtons.Right, new Point(10, 10));
                Click(e, 700, 500);
                check(e.Sel == new Rectangle(0, 0, 800, 600), "click elsewhere picks the screen");
            }
            using (var e = Editor())
            {
                e.MouseMoveAt(new Point(150, 150));
                check(e.KeyAt(Keys.C, true, false) == Cmd.Copy && e.Target() == new Rectangle(100, 100, 300, 200), "ctrl+c copies the window under the cursor");
                check(e.KeyAt(Keys.Enter, false, false) == Cmd.Copy, "enter copies");
            }
            using (var e = Editor())
            {
                Drag(e, 100, 100, 300, 200);
                Drag(e, 200, 150, 250, 170);
                check(e.Sel == new Rectangle(150, 120, 200, 100), "drag moves the selection");
                Drag(e, 350, 220, 400, 260);
                check(e.Sel == new Rectangle(150, 120, 250, 140), "corner handle resizes");
                Drag(e, 200, 150, -500, -500);
                var s = e.Sel;
                check(s.X == 0 && s.Y == 0, "selection stays on screen");
            }
            using (var e = Editor())
            {
                Drag(e, 100, 100, 500, 400);
                e.KeyAt(Keys.R, false, false);
                check(e.Tool == Tool.Rect, "R picks the rectangle");
                Drag(e, 150, 150, 300, 250);
                check(e.Annots.Count == 1, "rectangle drawn");
                Func<Color> px = () => { using (var img = e.Export()) return img.GetPixel(50, 75); };
                Color drawn = px();
                e.KeyAt(Keys.Z, true, false);
                check(e.Annots.Count == 0 && px().ToArgb() == Color.FromArgb(200, 200, 200).ToArgb(), "undo removes it");
                e.KeyAt(Keys.Y, true, false);
                check(px().ToArgb() == drawn.ToArgb() && drawn.ToArgb() != Color.FromArgb(200, 200, 200).ToArgb(), "redo puts it back");
                e.KeyAt(Keys.L, false, false);
                Drag(e, 150, 300, 900, 300);
                check(e.Annots[1].B.X == 500, "drawing is clamped inside the selection");
            }
            using (var e = Editor())
            {
                Drag(e, 100, 100, 500, 400);
                e.KeyAt(Keys.H, false, false);
                Drag(e, 150, 150, 300, 150);
                e.KeyAt(Keys.D5, false, false);
                e.KeyAt(Keys.P, false, false);
                Drag(e, 150, 200, 300, 200);
                check(e.Annots[0].Color == Ui.Palette[2] && e.Annots[1].Color == Ui.Palette[0] && e.StyleMarker == Ui.Palette[4], "highlighter keeps its own colour");
                e.KeyAt(Keys.N, false, false);
                Click(e, 150, 300); Click(e, 200, 300); Click(e, 250, 300);
                check(e.Annots[2].N == 1 && e.Annots[4].N == 3, "steps number themselves");
                foreach (var k in new[] { Keys.L, Keys.A, Keys.E, Keys.B })
                {
                    e.KeyAt(k, false, false);
                    Drag(e, 120, 120, 480, 380);
                }
                check(e.Annots.Count == 9, "every tool draws");
                using (var img = e.Export()) check(img != null, "export with every tool");
            }
            using (var e = Editor())
            {
                Drag(e, 100, 100, 500, 400);
                e.KeyAt(Keys.T, false, false);
                Click(e, 150, 150);
                e.KeyAt(Keys.P, false, false);
                foreach (char ch in "pa") e.TextInput(ch);
                e.KeyAt(Keys.Back, false, false);
                foreach (char ch in "int\b") e.TextInput(ch);
                check(e.Tool == Tool.Text, "typing does not switch tools");
                check(e.KeyAt(Keys.Escape, false, false) == Cmd.None && e.Annots.Count == 1 && e.Annots[0].Text == "pint", "first Esc commits the text");
                check(e.KeyAt(Keys.Escape, false, false) == Cmd.Cancel, "second Esc cancels");
            }
            using (var e = Editor())
            {
                Drag(e, 100, 100, 500, 300);
                check(e.KeyAt(Keys.S, true, true) == Cmd.QuickSave && e.KeyAt(Keys.P, true, false) == Cmd.Pin, "save and pin keys");
                check(e.KeyAt(Keys.T, true, false) == Cmd.None, "OCR off when unavailable");
                check(e.DoubleClickAt(new Point(200, 200)) == Cmd.Copy, "double-click copies");
                check(e.MouseDownAt(MouseButtons.Right, new Point(10, 10)) == Cmd.None && !e.HasSel, "right-click drops the selection");
                check(e.MouseDownAt(MouseButtons.Right, new Point(10, 10)) == Cmd.Cancel, "then cancels");
            }
            var t = new CtrlTaps();
            Func<int, bool, uint, bool> key = (code, down, ms) => t.Key(code, down, ms, 500);
            key(0xA2, true, 0); key(0xA2, true, 30); key(0xA2, false, 80); key(0xA2, true, 200);
            check(key(0xA2, false, 260), "double Ctrl turns the spotlight on");
            key(0xA2, true, 1000); key(0xA2, false, 1050); key(0xA2, true, 1700);
            check(!key(0xA2, false, 1760), "slow taps do nothing");
            key(0xA3, true, 2000); key(0x43, true, 2010); key(0xA3, false, 2050); key(0xA3, true, 2100);
            check(!key(0xA3, false, 2150), "Ctrl+C is not a tap");
            key(0xA2, true, 3000); key(0xA2, false, 3050); t.Spoil(); key(0xA2, true, 3100);
            check(!key(0xA2, false, 3150), "a click in between spoils it");

            check(Reader.FixWord("192.168.l.1O") == "192.168.1.10" && Reader.FixWord("(l92,168,0,1),") == "(192.168.0.1),", "IPv4 look-alikes");
            check(Reader.FixWord("10.0.0.1/24") == "10.0.0.1/24" && Reader.FixWord("10.O.0.1:8O8O") == "10.0.0.1:8080", "IPv4 prefix and port");
            check(Reader.FixWord("300.1.1.1") == "300.1.1.1" && Reader.FixWord("v1.2.3") == "v1.2.3", "not an IP stays");
            check(Reader.FixWord("0O-1A-2B-3C-4D-5E") == "00-1A-2B-3C-4D-5E" && Reader.FixWord("aa:bb;cc:dd:ee:fO") == "aa:bb:cc:dd:ee:f0", "MAC look-alikes");
            check(Reader.FixWord("00:1a:2b:3c4d:5e") == "00:1a:2b:3c:4d:5e" && Reader.FixWord("3C-52-82-1F-AO-B7") == "3C-52-82-1F-A0-B7" && Reader.FixWord("2026-10-28") == "2026-10-28", "MAC with a separator lost");
            check(Reader.FixWord("fe8O::1") == "fe80::1" && Reader.FixWord("1O24") == "1024" && Reader.FixWord("2O26-1O-28") == "2026-10-28", "IPv6 and numbers");
            check(Reader.Fix("Hello SOS B12 IPv4 x64 Win11 S.S.S.S") == "Hello SOS B12 IPv4 x64 Win11 S.S.S.S", "words stay as they are");
            check(Reader.Fix("on O.O.O.O:8O8O SN-2O26-1O28 l\u00f8.\u00f8.\u00f8.l") == "on 0.0.0.0:8080 SN-2026-1028 10.0.0.1", "zeros, serials, slashed zeros");
            check(Reader.Layout(Reader.Parse("0\t10\t10\t20\t20\te.\n0\t36\t10\t90\t20\t\u00f8.\u00f8.\u00f8:8080\n", 1, 0, null)) == "e.\u00f8.\u00f8.\u00f8:8080" && Reader.Fix("e.\u00f8.\u00f8.\u00f8:8080") == "0.0.0.0:8080", "an address split at a dot");
            check(Reader.Layout(Reader.Parse("0\t10\t10\t70\t20\t192.168.\n0\t84\t10\t30\t20\t1.10\n1\t10\t40\t30\t20\tIP:\n1\t46\t40\t60\t20\t10.0.0.1\n", 1, 0, null)) == "192.168.1.10\r\nIP: 10.0.0.1", "split addresses glued, words spaced");
            // A | that OCR read as l, next to an l that is an l.
            using (var bmp = new Bitmap(260, 40))
            using (var font = new Font("Segoe UI", 20, GraphicsUnit.Pixel))
            {
                using (var g = Graphics.FromImage(bmp)) { g.Clear(Color.White); TextRenderer.DrawText(g, "grep | wc -l", font, new Point(4, 6), Color.Black, Color.White, TextFormatFlags.NoPadding); }
                // The word boxes Windows OCR gives for this picture.
                const string raw = "0\t4\t18\t40\t16\tgrep\n0\t51\t13\t3\t21\tl\n0\t59\t18\t24\t11\twc\n0\t89\t13\t11\t16\t-l\n";
                using (var grey = Reader.Grey(bmp))
                {
                    string got = Reader.Layout(Reader.Parse(raw, 1, 0, new Reader.Ink(grey)));
                    check(got == "grep | wc -l", "a | read as l is found: " + got);
                }
            }
            Func<string, OcrPass> one = s => new OcrPass { Name = s, Lines = new List<OcrLine> { new OcrLine(new RectangleF(0, 0, 100, 20), s) } };
            int ag;
            check(Reader.Vote(new List<OcrPass> { one("10.0.0.7"), one("10.0.0.l"), one("10.0.0.7") }, false, out ag) == "10.0.0.7" && ag == 66, "readings vote");
            check(Reader.Vote(new List<OcrPass> { one("10.0.0.l"), one("10.0.0.1"), one("1O.0.0.1") }, true, out ag) == "10.0.0.1" && ag == 100, "fixes before the vote");

            using (var logo = Ui.Logo(64))
                check(logo.GetPixel(32, 32).ToArgb() == Color.Black.ToArgb() && logo.GetPixel(0, 0).A == 0, "logo draws");
            return fails.Count == 0 ? "ok" : "FAILED: " + string.Join("; ", fails.ToArray());
        }
    }
}
'@

try {
    Add-Type -ReferencedAssemblies System.Windows.Forms, System.Drawing -TypeDefinition $source

    if ($Test) {
        $result = [Shot.Tests]::Run()
        Write-Output $result
        exit [int]($result -ne 'ok')
    }

    # Copy text out of screenshots with the OCR engine built into Windows 10/11.
    function Wait-WinRT($op, [type]$type) {
        $task = $script:asTask.MakeGenericMethod($type).Invoke($null, @($op))
        $task.Wait()
        $task.Result
    }
    function Read-Text([System.Drawing.Bitmap]$bmp) {
        $ms = New-Object System.IO.MemoryStream
        try {
            $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
            $ms.Position = 0
            $stream = [System.IO.WindowsRuntimeStreamExtensions]::AsRandomAccessStream($ms)
            $decoder = Wait-WinRT ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
            $bitmap = Wait-WinRT ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
            $result = Wait-WinRT ($script:ocrEngine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
            # One word per row: line number, box, text. Shot puts the words together itself.
            $sb = New-Object System.Text.StringBuilder
            $i = 0
            foreach ($line in $result.Lines) {
                foreach ($w in $line.Words) {
                    $r = $w.BoundingRect
                    [void]$sb.Append("$i`t$([int]$r.X)`t$([int]$r.Y)`t$([int]$r.Width)`t$([int]$r.Height)`t$($w.Text)`n")
                }
                $i++
            }
            return $sb.ToString()
        } finally { $ms.Dispose() }
    }
    try {
        Add-Type -AssemblyName System.Runtime.WindowsRuntime
        $null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
        $null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics, ContentType = WindowsRuntime]
        $script:asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
            $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
        } | Select-Object -First 1
        $script:ocrEngine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
        if ($script:ocrEngine -and $script:asTask) {
            [Shot.App]::Ocr = [Func[System.Drawing.Bitmap, string]] { param($bmp) Read-Text $bmp }
            [Shot.App]::OcrMax = [Windows.Media.Ocr.OcrEngine]::MaxImageDimension
        }
    } catch { }

    [Shot.App]::Run($PSScriptRoot, [bool]$Capture, [bool]$Settings)
} catch {
    if ($Test) { throw }
    [void][System.Windows.Forms.MessageBox]::Show("Shot could not start:`n`n$_", 'Shot', 'OK', 'Error')
    exit 1
}

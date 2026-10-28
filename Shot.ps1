<#
  Shot: a tiny, fast screenshot tool. Select, annotate, then copy, save or pin
  it to the screen.

  Pure PowerShell and the .NET that ships with Windows: the app is the C# in
  Shot.cs, compiled on this PC the first time it starts, so there is no .exe
  and nothing to install. Double-click Shot.vbs to start it without a console
  window.

    Shot.ps1             run in the tray (if already running: open its Settings)
    Shot.ps1 -Capture    run in the tray and capture right away
    Shot.ps1 -Settings   run in the tray and open Settings
    Shot.ps1 -Test       self-check, prints "ok" or what failed

  Remove Shot.vbs (Remove.ps1) takes it out of start-up and can recycle the folder.
#>
param([switch]$Capture, [switch]$Settings, [switch]$Test)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing

# One Shot per user. A second launch asks the running one to show Settings.
if (-not $Test) {
    $isFirst = $false
    $mutex = New-Object System.Threading.Mutex($true, 'Local\Shot.Screenshot', [ref]$isFirst)
    if (-not $isFirst) {
        try { [void][System.Threading.EventWaitHandle]::OpenExisting('Local\Shot.OpenSettings').Set() } catch { }
        exit
    }
}

# The app is Shot.cs beside this file. It is compiled once per version and kept in data\
# (named by a fingerprint of Shot.cs), so starting takes about a second; a folder that
# cannot be written to compiles it in memory each time instead.
$cs = Join-Path $PSScriptRoot 'Shot.cs'
$refs = 'System.Windows.Forms', 'System.Drawing'
function Use-Shot {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $id = [BitConverter]::ToString($sha.ComputeHash([IO.File]::ReadAllBytes($cs)), 0, 8).Replace('-', '')
    $sha.Dispose()
    $dll = Join-Path $PSScriptRoot "data\Shot-$id.dll"
    try {
        if (-not (Test-Path $dll)) {
            [void](New-Item -ItemType Directory -Force (Split-Path $dll))
            Add-Type -ReferencedAssemblies $refs -TypeDefinition ([IO.File]::ReadAllText($cs)) -OutputAssembly $dll -OutputType Library
        }
        Add-Type -Path $dll
        # Earlier versions' builds are no longer needed (one still in use goes next time).
        Get-ChildItem (Split-Path $dll) -Filter 'Shot-*.dll' | Where-Object { $_.FullName -ne $dll } | Remove-Item -ErrorAction SilentlyContinue
    } catch {
        Remove-Item $dll -ErrorAction SilentlyContinue
        Add-Type -ReferencedAssemblies $refs -TypeDefinition ([IO.File]::ReadAllText($cs))
    }
}

try {
    Use-Shot

    if ($Test) {
        $result = [Shot.Tests]::Run()
        Write-Output $result
        exit [int]($result -ne 'ok')
    }

    # Copy text out of screenshots with the OCR engine built into Windows 10/11.
    # Several pictures at once, one OCR engine each: an engine reads one picture at a time,
    # and engines side by side use the other processor cores. Engines are made for each
    # reading (a few ms) and let go after, so their memory goes when the reading is done.
    function Read-Text([System.Drawing.Bitmap[]]$bmps) {
        $engines = @(foreach ($b in $bmps) { [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages() })
        $pictures = New-Object System.Collections.Generic.List[object]
        try {
            foreach ($bmp in $bmps) {
                # The pixels as they are: no image file in between.
                $rect = New-Object System.Drawing.Rectangle 0, 0, $bmp.Width, $bmp.Height
                $data = $bmp.LockBits($rect, 'ReadOnly', [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
                try {
                    $bytes = New-Object byte[] ($data.Stride * $bmp.Height)
                    [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
                } finally { $bmp.UnlockBits($data) }
                $buffer = [System.Runtime.InteropServices.WindowsRuntime.WindowsRuntimeBufferExtensions]::AsBuffer($bytes)
                $pictures.Add([Windows.Graphics.Imaging.SoftwareBitmap]::CreateCopyFromBuffer($buffer, [Windows.Graphics.Imaging.BitmapPixelFormat]::Bgra8, $bmp.Width, $bmp.Height))
                $bytes = $null
            }
            $reading = $script:asTask.MakeGenericMethod([Windows.Media.Ocr.OcrResult])
            $tasks = for ($n = 0; $n -lt $pictures.Count; $n++) { $reading.Invoke($null, @($engines[$n].RecognizeAsync($pictures[$n]))) }
            foreach ($task in @($tasks)) {
                $task.Wait()
                # One word per row: line number, box, text. Shot puts the words together itself.
                $sb = New-Object System.Text.StringBuilder
                $i = 0
                foreach ($line in $task.Result.Lines) {
                    foreach ($w in $line.Words) {
                        $r = $w.BoundingRect
                        [void]$sb.Append("$i`t$([int]$r.X)`t$([int]$r.Y)`t$([int]$r.Width)`t$([int]$r.Height)`t$($w.Text)`n")
                    }
                    $i++
                }
                $sb.ToString()
            }
        } finally { foreach ($pic in $pictures) { $pic.Dispose() } }
    }
    try {
        Add-Type -AssemblyName System.Runtime.WindowsRuntime
        $null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
        $null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics, ContentType = WindowsRuntime]
        $script:asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
            $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
        } | Select-Object -First 1
        if ([Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages() -and $script:asTask) {
            [Shot.App]::Ocr = [Func[[System.Drawing.Bitmap[]], [string[]]]] { param($bmps) Read-Text $bmps }
            [Shot.App]::OcrMax = [Windows.Media.Ocr.OcrEngine]::MaxImageDimension
        }
    } catch { }

    [Shot.App]::Run($PSScriptRoot, [bool]$Capture, [bool]$Settings)
} catch {
    if ($Test) { throw }
    [void][System.Windows.Forms.MessageBox]::Show("Shot could not start:`n`n$_", 'Shot', 'OK', 'Error')
    exit 1
}

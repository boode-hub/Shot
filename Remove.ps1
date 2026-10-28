<#
  Remove Shot: Remove Shot.vbs runs this. Asks first, then stops Shot, takes this copy
  out of start-up and, on Yes, moves this folder (with its settings) to the Recycle Bin.
  Kept apart from Shot.ps1, which then starts faster.
#>
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
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

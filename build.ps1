<#
.SYNOPSIS
    Builds Shot on this PC and starts it. Run it again any time to update.

.DESCRIPTION
    1. Gets the latest Shot code from GitHub (installs Git with winget if it is
       missing; the first time, Git asks you to sign in to GitHub).
    2. Installs Rust if it is missing, plus LLVM's dlltool (one time). Uses
       Rust's GNU toolchain, so Visual Studio is not needed.
    3. Builds Shot.exe in this folder and starts it.

    Shot.exe is compiled here rather than downloaded, so Windows does not treat
    it as an unknown program from the internet.

.PARAMETER NoUpdate
    Build the code already in this folder; don't pull from GitHub.

.PARAMETER NoRun
    Build only; don't start Shot.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File D:\SS\Shot\build.ps1
#>
[CmdletBinding()]
param(
    [switch]$NoUpdate,
    [switch]$NoRun
)

$ErrorActionPreference = 'Stop'
# Invoke-WebRequest is many times slower with its progress bar on.
$ProgressPreference = 'SilentlyContinue'

$Root = $PSScriptRoot
$Repo = 'https://github.com/BOoDe007/Shot.git'
$Toolchain = '1.99.0-x86_64-pc-windows-gnu'
$CargoBin = Join-Path $env:USERPROFILE '.cargo\bin'

function Step([string]$Message) {
    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Fail([string]$Message) {
    Write-Host "`n$Message" -ForegroundColor Red
    exit 1
}

function Test-Command([string]$Name) {
    [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

# Run a program and stop the script if it fails.
function Invoke-Checked([string]$Exe, [string[]]$Arguments, [string]$Hint = '') {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) {
        Fail "'$Exe $($Arguments -join ' ')' failed (exit code $LASTEXITCODE).$Hint"
    }
}

# Pick up programs that installers just added to PATH.
function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = "$CargoBin;$machine;$user"
}

# --- 1. Code ------------------------------------------------------------------

if (-not $NoUpdate) {
    Step 'Getting the latest Shot code'
    if (-not (Test-Command git)) {
        if (-not (Test-Command winget)) {
            Fail 'Git is needed. Install it from https://git-scm.com/download/win, then run this script again.'
        }
        Write-Host 'Installing Git (one time)...'
        & winget install --id Git.Git -e --source winget --silent --accept-package-agreements --accept-source-agreements
        Update-SessionPath
        if (-not (Test-Command git)) { $env:Path += ';C:\Program Files\Git\cmd' }
        if (-not (Test-Command git)) {
            Fail 'Git was installed but is not on PATH yet. Close this window, open a new PowerShell and run the script again.'
        }
    }

    if (-not (Test-Path (Join-Path $Root '.git'))) {
        Invoke-Checked git @('-C', $Root, 'init', '-q')
        Invoke-Checked git @('-C', $Root, 'remote', 'add', 'origin', $Repo)
    }
    # Windows PowerShell 5.1 turns a native program's stderr into errors under
    # 'Stop', so relax it for this probe.
    $ErrorActionPreference = 'Continue'
    & git -C $Root rev-parse --verify -q HEAD *> $null
    $hasCode = ($LASTEXITCODE -eq 0)
    $ErrorActionPreference = 'Stop'
    $scriptBefore = (Get-FileHash -LiteralPath $PSCommandPath).Hash
    if ($hasCode) {
        Invoke-Checked git @('-C', $Root, 'pull', '--ff-only', 'origin', 'main')
    }
    else {
        # First run: make this folder a copy of the repository.
        Invoke-Checked git @('-C', $Root, 'fetch', 'origin', 'main')
        Invoke-Checked git @('-C', $Root, 'checkout', '-q', '-f', '-B', 'main', 'origin/main')
        Invoke-Checked git @('-C', $Root, 'branch', '-q', '--set-upstream-to=origin/main', 'main')
    }

    # The update may have brought a newer build.ps1: hand over to it, so a fix
    # to this script takes effect on the same run.
    if ((Get-FileHash -LiteralPath $PSCommandPath).Hash -ne $scriptBefore) {
        Write-Host 'build.ps1 was updated; continuing with the new version.'
        $relay = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $PSCommandPath, '-NoUpdate')
        if ($NoRun) { $relay += '-NoRun' }
        & (Get-Process -Id $PID).Path @relay
        exit $LASTEXITCODE
    }
}

if (-not (Test-Path (Join-Path $Root 'Cargo.toml'))) {
    Fail "There is no Shot source code in $Root. Run the script without -NoUpdate."
}

# --- 2. Rust --------------------------------------------------------------------

Step 'Checking Rust'
if (Test-Path $CargoBin) { $env:Path = "$CargoBin;$env:Path" }
if (-not (Test-Command rustup)) {
    Write-Host 'Installing Rust (one time, a few hundred MB)...'
    $init = Join-Path $env:TEMP 'rustup-init.exe'
    Invoke-WebRequest -UseBasicParsing -OutFile $init `
        -Uri 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe'
    Invoke-Checked $init @('-y', '--default-host', 'x86_64-pc-windows-gnu', '--default-toolchain', 'none', '--profile', 'minimal')
    Remove-Item $init -ErrorAction SilentlyContinue
    $env:Path = "$CargoBin;$env:Path"
}
# Install the pinned Rust once. --no-self-update: rustup must not try to
# replace its own .exe, which locked-down PCs refuse (os error 5).
$installed = (& rustup toolchain list) -match [regex]::Escape($Toolchain)
if (-not $installed) {
    Invoke-Checked rustup @('toolchain', 'install', $Toolchain, '--profile', 'minimal', '--no-self-update')
}

# The GNU toolchain carries its own gcc and ld (the rust-mingw component) in a
# folder the compiler does not search on its own; put it on PATH.
$sysroot = (& rustc "+$Toolchain" --print sysroot).Trim()
$mingw = Join-Path $sysroot 'lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained'
if (-not (Test-Path (Join-Path $mingw 'ld.exe'))) {
    Invoke-Checked rustup @('component', 'add', 'rust-mingw', '--toolchain', $Toolchain)
}
if (-not (Test-Path (Join-Path $mingw 'ld.exe'))) {
    Fail "Rust's build tools are missing from $mingw. Try: rustup toolchain uninstall $Toolchain, then run this script again."
}

# Rust's own dlltool needs an assembler it does not ship, so use LLVM's
# dlltool (no assembler needed) from llvm-mingw. Only its four files are kept,
# and each must match the hash of the copy Shot was tested with.
$Tools = Join-Path $env:LOCALAPPDATA 'Shot\buildtools'
$LlvmMingw = 'llvm-mingw-20260311-ucrt-x86_64'
$ToolHashes = @{
    'dlltool.exe'    = 'A82E0E18AE664B6215DC92CAAC41CD4925034DAED712BA73B9BDBBFE4B00647E'
    'libLLVM-22.dll' = '60FE76D5CCC133772D96A798A349CFF37D3D7143998E428EF341F015FBAE8887'
    'libc++.dll'     = 'A94862388DED8B71846140550469D2ED408F385FAFCF0310963ABD2352F15868'
    'libunwind.dll'  = '0F45138D6DBDB9233E0EC91B84E5AE224502975DDEA2E1784568AF51296DB07D'
}
function Test-Tools {
    foreach ($name in $ToolHashes.Keys) {
        $file = Join-Path $Tools $name
        if (-not (Test-Path $file)) { return $false }
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash -ne $ToolHashes[$name]) { return $false }
    }
    return $true
}
if (-not (Test-Tools)) {
    Write-Host 'Downloading LLVM dlltool (one time, about 190 MB download, 80 MB kept)...'
    New-Item -ItemType Directory -Force -Path $Tools | Out-Null
    $zip = Join-Path $env:TEMP "$LlvmMingw.zip"
    Invoke-WebRequest -UseBasicParsing -OutFile $zip `
        -Uri "https://github.com/mstorsjo/llvm-mingw/releases/download/20260311/$LlvmMingw.zip"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zip)
    try {
        foreach ($name in $ToolHashes.Keys) {
            $entry = $archive.GetEntry("$LlvmMingw/bin/$name")
            if (-not $entry) { Fail "$name is missing from $LlvmMingw.zip." }
            [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $Tools $name), $true)
        }
    }
    finally {
        $archive.Dispose()
        Remove-Item $zip -ErrorAction SilentlyContinue
    }
    if (-not (Test-Tools)) {
        Remove-Item (Join-Path $Tools '*') -ErrorAction SilentlyContinue
        Fail 'The downloaded build tools did not match the expected checksums, so they were deleted. Run the script again.'
    }
}
$env:Path = "$Tools;$mingw;$env:Path"

# --- 3. Build -------------------------------------------------------------------

Step 'Building Shot (the first build takes a few minutes, later ones seconds)'
$blocked = "`nIf it says 'An Application Control policy has blocked this file' (os error 4551), " +
    'Smart App Control or a company policy blocks every program built on this PC, not only Shot.'
Push-Location $Root
try {
    Invoke-Checked cargo @("+$Toolchain", 'build', '--release', '--locked') $blocked
}
finally {
    Pop-Location
}
$built = Join-Path $Root 'target\release\shot.exe'
if (-not (Test-Path $built)) { Fail "The build finished but $built is missing." }

# A running Shot (including the copy that starts at login) locks the file.
Get-Process -Name shot -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 400
$exe = Join-Path $Root 'Shot.exe'
Copy-Item $built $exe -Force
Write-Host "Built $exe" -ForegroundColor Green

# --- 4. Run ---------------------------------------------------------------------

if (-not $NoRun) {
    Step 'Starting Shot'
    try {
        Start-Process -FilePath $exe -WorkingDirectory $Root
    }
    catch {
        Fail "Windows would not start Shot.exe: $($_.Exception.Message)$blocked"
    }
    Write-Host 'Shot is running: the red eye icon is by the clock (click ^ if hidden).' -ForegroundColor Green
    Write-Host 'Press Shift+Space to capture. Turn on "Start Shot when I log in" in Settings to keep it.'
}

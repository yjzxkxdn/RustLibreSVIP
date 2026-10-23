# Install the GUI into a normal per-user folder.
#
# Why this exists: the repository may live in a directory that carries a
# low-integrity (sandbox) label - for example a workspace created by an agent
# harness. Every executable built there inherits that label, and Windows then
# treats it as an untrusted program: the shell shows the "Open File - Security
# Warning", and the process is started with a restricted (Low integrity) token
# that cannot write anywhere.
#
# Copying the binary to %LOCALAPPDATA%\Programs resets the inherited label, and
# `icacls /setintegritylevel Medium` makes it explicit.

param(
    [switch]$Build
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$source = Join-Path $root 'target\release\libresvip-gui.exe'

if ($Build -or -not (Test-Path $source)) {
    Push-Location $root
    try {
        & cargo build --release -p libresvip-gui
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    } finally {
        Pop-Location
    }
}

$dest = Join-Path $env:LOCALAPPDATA 'Programs\RustLibreSVIP'
New-Item -ItemType Directory -Force $dest | Out-Null
$target = Join-Path $dest 'libresvip-gui.exe'
Copy-Item -LiteralPath $source -Destination $target -Force
icacls $target /setintegritylevel Medium | Out-Null

$startMenu = [Environment]::GetFolderPath('Programs')
$shortcutPath = Join-Path $startMenu 'LibreSVIP (Rust port).lnk'
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $target
$shortcut.WorkingDirectory = $dest
$shortcut.IconLocation = "$target,0"
$shortcut.Description = 'LibreSVIP (Rust port)'
$shortcut.Save()

Write-Host "installed: $target"
Write-Host "shortcut : $shortcutPath"
Write-Host "integrity:"
icacls $target | Select-String -Pattern 'Mandatory Label'

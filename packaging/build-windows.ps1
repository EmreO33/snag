<#
.SYNOPSIS
    Builds the Windows release artifacts: the bare exe, the portable zip, and
    the installer.

.DESCRIPTION
    This is what the release workflow runs, kept as a script so the same steps
    can be run locally. Everything lands in dist\.

.PARAMETER Version
    Version to stamp the artifacts with. Defaults to the version in Cargo.toml.

.PARAMETER SkipBuild
    Reuse an existing snag.exe instead of running cargo build.

.PARAMETER ExePath
    Where to find snag.exe. Defaults to the crate's release build; CI
    passes the per-target path.

.EXAMPLE
    .\packaging\build-windows.ps1
    .\packaging\build-windows.ps1 -Version 0.2.0
#>
[CmdletBinding()]
param(
    [string]$Version,
    [switch]$SkipBuild,
    [string]$ExePath
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$crate = Join-Path $root "snag"
$dist = Join-Path $root "dist"

if (-not $Version) {
    $cargo = Get-Content (Join-Path $crate "Cargo.toml") -Raw
    if ($cargo -match '(?m)^version\s*=\s*"([^"]+)"') {
        $Version = $Matches[1]
    } else {
        throw "could not read the version out of Cargo.toml; pass -Version"
    }
}
Write-Host "building Snag $Version" -ForegroundColor Cyan

New-Item -ItemType Directory -Force -Path $dist | Out-Null

# --- 1. the executable ------------------------------------------------------
$exe = if ($ExePath) { $ExePath } else { Join-Path $crate "target\release\snag.exe" }
if (-not $SkipBuild) {
    Push-Location $crate
    try { cargo build --release; if ($LASTEXITCODE -ne 0) { throw "cargo build failed" } }
    finally { Pop-Location }
}
if (-not (Test-Path $exe)) { throw "snag.exe not found at $exe" }

Copy-Item $exe (Join-Path $dist "snag-windows-x86_64.exe") -Force
Write-Host "  bare exe" -ForegroundColor Green

# --- 2. the portable zip ----------------------------------------------------
$stage = Join-Path $dist "portable-stage"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Copy-Item $exe (Join-Path $stage "snag.exe")
Copy-Item (Join-Path $PSScriptRoot "portable\portable.txt") $stage
Copy-Item (Join-Path $PSScriptRoot "portable\README.txt") $stage

$zip = Join-Path $dist "Snag-$Version-windows-portable.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
Remove-Item $stage -Recurse -Force
Write-Host "  portable zip" -ForegroundColor Green

# --- 3. the installer -------------------------------------------------------
# winget installs Inno Setup per-user by default, so check there too.
$iscc = @(
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles}\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $iscc) {
    $cmd = Get-Command iscc -ErrorAction SilentlyContinue
    if ($cmd) { $iscc = $cmd.Source }
}

if (-not $iscc) {
    Write-Warning "Inno Setup (ISCC.exe) not found, skipping the installer."
    Write-Warning "Install it with: winget install JRSoftware.InnoSetup"
} else {
    # The .iss picks the exe up from beside itself.
    $staged = Join-Path $PSScriptRoot "windows\snag.exe"
    Copy-Item $exe $staged -Force
    try {
        & $iscc "/DAppVersion=$Version" (Join-Path $PSScriptRoot "windows\snag.iss") | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "iscc failed with exit code $LASTEXITCODE" }
        Write-Host "  installer" -ForegroundColor Green
    } finally {
        Remove-Item $staged -Force -ErrorAction SilentlyContinue
    }
}

Write-Host ""
Get-ChildItem $dist -File | Select-Object Name, @{n="MB";e={[math]::Round($_.Length/1MB,2)}}

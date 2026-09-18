param(
    [string]$BuildRoot = (Join-Path $PSScriptRoot '..\..\.local\build-tools')
)

$ErrorActionPreference = 'Stop'
$BuildRoot = [System.IO.Path]::GetFullPath($BuildRoot)
$env:RUSTUP_HOME = Join-Path $BuildRoot 'rustup'
$env:CARGO_HOME = Join-Path $BuildRoot 'cargo'
$env:CARGO_TARGET_DIR = Join-Path $BuildRoot 'target'
$env:RUSTUP_AUTO_INSTALL = '0'
$env:TEMP = Join-Path $BuildRoot 'temp'
$env:TMP = $env:TEMP
$env:PATH = "$(Join-Path $env:CARGO_HOME 'bin');$env:PATH"
New-Item -ItemType Directory -Path $env:TEMP -Force | Out-Null

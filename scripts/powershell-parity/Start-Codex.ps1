[CmdletBinding()]
param(
    [string]$BackendPath,
    [string]$DesktopPath,
    [string]$UserDataPath,
    [switch]$ValidateOnly
)

$ErrorActionPreference = 'Stop'
$compatibility = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'compatibility.json') -Raw | ConvertFrom-Json
if ($BackendPath) {
    $backend = [System.IO.Path]::GetFullPath($BackendPath)
} elseif (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'codex.exe')) {
    $backend = Join-Path $PSScriptRoot 'codex.exe'
} else {
    $repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
    $backend = Join-Path $repo 'artifacts\powershell-parity\codex.exe'
}
if (-not (Test-Path -LiteralPath $backend -PathType Leaf)) {
    throw "Build is missing: $backend"
}
$version = & $backend --version
if ($LASTEXITCODE -ne 0 -or $version -ne "codex-cli $($compatibility.backendVersion)") {
    throw "Unexpected backend version: $version"
}
foreach ($component in @('codex-command-runner.exe', 'codex-windows-sandbox-setup.exe', 'codex-code-mode-host.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path (Split-Path $backend) $component) -PathType Leaf)) {
        throw "Missing backend component: $component"
    }
}
$package = Get-AppxPackage -Name 'OpenAI.Codex' | Select-Object -First 1
if ($null -eq $package -or $package.Version -notin $compatibility.desktopVersions) {
    throw "This launcher supports desktop versions: $($compatibility.desktopVersions -join ', '). Recheck compatibility after an app update."
}
$app = Join-Path $package.InstallLocation 'app\ChatGPT.exe'
if ($DesktopPath) {
    $desktop = (Resolve-Path -LiteralPath $DesktopPath).Path
    if ((Get-FileHash -LiteralPath $desktop).Hash -ne (Get-FileHash -LiteralPath $app).Hash) {
        throw 'The custom desktop executable must match the installed, supported desktop version.'
    }
    $app = $desktop
}
$isolatedProfile = -not [string]::IsNullOrWhiteSpace($UserDataPath)
if ($isolatedProfile) {
    $UserDataPath = [System.IO.Path]::GetFullPath($UserDataPath)
} else {
    $UserDataPath = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'Codex\web\Codex'
}
if ($ValidateOnly) {
    [pscustomobject]@{ DesktopVersion = $package.Version; BackendVersion = $version; BackendPath = $backend; DesktopPath = $app; UserDataPath = $UserDataPath }
    return
}
$running = Get-CimInstance Win32_Process -Filter "Name='ChatGPT.exe'" |
    Where-Object {
        ($_.ExecutablePath -eq $app -or $_.ExecutablePath -eq (Join-Path $package.InstallLocation 'app\ChatGPT.exe')) -and
        (-not $isolatedProfile -or $_.CommandLine -match ('--user-data-dir=(?:"' + [regex]::Escape($UserDataPath) + '"|' + [regex]::Escape($UserDataPath) + '(?=\s|$))'))
    }
if ($running) {
    throw 'Save your work and close Codex yourself, then run this launcher again. It never terminates an active session.'
}
# Environment belongs only to the new process; user/machine variables are untouched.
$existingBackendIds = @(Get-CimInstance Win32_Process -Filter "Name='codex.exe'" |
    Where-Object { $_.ExecutablePath -eq $backend } | Select-Object -ExpandProperty ProcessId)
$launchEnvironment = @{ CODEX_CLI_PATH = $backend }
if ($isolatedProfile) {
    $launchEnvironment.CODEX_ELECTRON_USER_DATA_PATH = $UserDataPath
}
# Owl's native browser starts before Electron reads its environment overrides.
$started = Start-Process -FilePath $app -ArgumentList ('--user-data-dir="' + $UserDataPath + '"') -Environment $launchEnvironment -PassThru
$deadline = (Get-Date).AddSeconds(30)
do {
    Start-Sleep -Milliseconds 500
    $process = Get-CimInstance Win32_Process -Filter "Name='codex.exe'" |
        Where-Object { $_.ExecutablePath -eq $backend -and $_.ProcessId -notin $existingBackendIds }
} while (-not $process -and (Get-Date) -lt $deadline)
if (-not $process) {
    $observed = @(Get-CimInstance Win32_Process -Filter "Name='codex.exe'" |
        Select-Object -ExpandProperty ExecutablePath -Unique) -join ', '
    $logs = Join-Path $env:LOCALAPPDATA 'Codex\Logs'
    throw "Custom backend was not observed: $backend. Observed backends: $observed. Launcher PID: $($started.Id). Inspect application logs: $logs"
}
$process | Select-Object ProcessId, ParentProcessId, ExecutablePath
Write-Host 'Custom backend verified. To roll back, close Codex and open its usual Start-menu shortcut.'

[CmdletBinding()]
param(
    [string]$BackendPath,
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
if ($ValidateOnly) {
    [pscustomobject]@{ DesktopVersion = $package.Version; BackendVersion = $version; BackendPath = $backend }
    return
}
$running = Get-CimInstance Win32_Process -Filter "Name='ChatGPT.exe'" |
    Where-Object { $_.ExecutablePath -eq $app }
if ($running) {
    throw 'Save your work and close Codex yourself, then run this launcher again. It never terminates an active session.'
}
# Environment belongs only to the new process; user/machine variables are untouched.
$existingBackendIds = @(Get-CimInstance Win32_Process -Filter "Name='codex.exe'" |
    Where-Object { $_.ExecutablePath -eq $backend } | Select-Object -ExpandProperty ProcessId)
$started = Start-Process -FilePath $app -Environment @{ CODEX_CLI_PATH = $backend } -PassThru
$deadline = (Get-Date).AddSeconds(30)
do {
    Start-Sleep -Milliseconds 500
    $process = Get-CimInstance Win32_Process -Filter "Name='codex.exe'" |
        Where-Object { $_.ExecutablePath -eq $backend -and $_.ProcessId -notin $existingBackendIds }
    $started.Refresh()
} while (-not $process -and (Get-Date) -lt $deadline -and -not $started.HasExited)
if (-not $process) {
    throw 'The app was launched, but the custom backend path was not observed. Do not assume the override is active; inspect the application logs.'
}
$process | Select-Object ProcessId, ParentProcessId, ExecutablePath
Write-Host 'Custom backend verified. To roll back, close Codex and open its usual Start-menu shortcut.'

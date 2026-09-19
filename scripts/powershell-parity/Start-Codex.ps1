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
$backendDirectory = Split-Path $backend
$package = Get-AppxPackage -Name 'OpenAI.Codex' | Sort-Object { [version]$_.Version } -Descending | Select-Object -First 1
if ($null -eq $package) { throw 'Install the Codex desktop app before using this launcher.' }
$installedDirectory = Join-Path $package.InstallLocation 'app'
$installedArchiveHash = (Get-FileHash -LiteralPath (Join-Path $installedDirectory 'resources\app.asar')).Hash
if (-not $DesktopPath) {
    $registrationPath = Join-Path $backendDirectory 'desktop-bundle.json'
    if (Test-Path -LiteralPath $registrationPath -PathType Leaf) {
        $registration = Get-Content -LiteralPath $registrationPath -Raw | ConvertFrom-Json
        $DesktopPath = $registration.desktopPath
        if ([string]::IsNullOrWhiteSpace($DesktopPath)) {
            throw "Missing desktopPath in registration: $registrationPath"
        }
    }
    $preparedManifest = $null
    if ($DesktopPath -and (Test-Path -LiteralPath $DesktopPath -PathType Leaf)) {
        $preparedManifest = Get-Content -LiteralPath (Join-Path (Split-Path $DesktopPath) 'desktop-patch-manifest.json') -Raw | ConvertFrom-Json
    }
    if ($null -eq $preparedManifest -or $preparedManifest.sourceAsarSha256 -ne $installedArchiveHash -or
        $preparedManifest.executableSha256 -ne (Get-FileHash -LiteralPath (Join-Path $installedDirectory 'ChatGPT.exe')).Hash) {
        $preparer = Join-Path $PSScriptRoot 'prepare_desktop.py'
        if (-not (Test-Path -LiteralPath $preparer -PathType Leaf)) {
            throw 'Desktop update tools are missing. Use the launcher from the repository or a complete backend package.'
        }
        $python = (Get-Command python -ErrorAction Stop).Source
        $null = Get-Command node -ErrorAction Stop
        $output = Join-Path $backendDirectory ('desktop-bundles\app-' + $package.Version + '-' + [guid]::NewGuid().ToString('N'))
        Write-Host "Preparing Codex desktop $($package.Version)..."
        $preparationLog = & $python $preparer --source-app $installedDirectory --backend-dir $backendDirectory --output $output
        if ($LASTEXITCODE -ne 0) {
            $preparationLog | Out-Host
            throw 'Desktop preparation failed. The previous bundle is preserved; see the error above.'
        }
        $registration = Get-Content -LiteralPath $registrationPath -Raw | ConvertFrom-Json
        $DesktopPath = $registration.desktopPath
    }
}
$desktop = (Resolve-Path -LiteralPath $DesktopPath).Path
$bundledBackend = Join-Path (Split-Path $desktop) 'resources\codex.exe'
if (-not $BackendPath) { $backend = $bundledBackend }
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
$app = Join-Path $package.InstallLocation 'app\ChatGPT.exe'
if ($DesktopPath) {
    if ((Get-FileHash -LiteralPath $desktop).Hash -ne (Get-FileHash -LiteralPath $app).Hash) {
        throw 'The custom desktop executable must match the installed desktop version. Omit -DesktopPath to prepare it automatically.'
    }
    $manifestPath = Join-Path (Split-Path $desktop) 'desktop-patch-manifest.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.sourceAsarSha256 -ne $installedArchiveHash) {
        throw 'This bundle was prepared from a different desktop archive. Omit -DesktopPath to update it automatically.'
    }
    if ($manifest.backend.version -ne $compatibility.backendVersion) {
        throw 'This desktop does not contain a verified custom backend. Rebuild it with prepare_desktop.py --backend-dir.'
    }
    foreach ($name in @('codex.exe', 'codex-command-runner.exe', 'codex-windows-sandbox-setup.exe', 'codex-code-mode-host.exe')) {
        $entry = @($manifest.backend.files | Where-Object { $_.file -eq $name })
        $bundledComponent = Join-Path (Split-Path $bundledBackend) $name
        $selectedComponent = if ($name -eq 'codex.exe') { $backend } else { Join-Path (Split-Path $backend) $name }
        if ($entry.Count -ne 1 -or (Get-FileHash -LiteralPath $bundledComponent).Hash -ne $entry[0].sha256 -or
            (Get-FileHash -LiteralPath $selectedComponent).Hash -ne $entry[0].sha256) {
            throw "Custom backend component does not match the desktop manifest: $name"
        }
    }
    $archive = Join-Path (Split-Path $desktop) 'resources\app.asar'
    if ((Get-FileHash -LiteralPath $archive).Hash -ne $manifest.patchedAsarSha256) {
        throw 'Desktop UI archive does not match the prepared manifest.'
    }
    $app = $desktop
    if ($manifest.gitLabels -eq 'stock') {
        Write-Warning 'This desktop uses standard Git labels. PowerShell action classification still uses the custom backend.'
    }
}
$backendPaths = @($backend)
if ($bundledBackend -and $bundledBackend -ne $backend) { $backendPaths += $bundledBackend }
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
        $_.CommandLine -match ('--user-data-dir=(?:"' + [regex]::Escape($UserDataPath) + '"|' + [regex]::Escape($UserDataPath) + '(?=\s|$))') -or
        (-not $isolatedProfile -and ($_.ExecutablePath -eq $app -or $_.ExecutablePath -eq (Join-Path $package.InstallLocation 'app\ChatGPT.exe')))
    }
if ($running) {
    throw 'Save your work and close Codex yourself, then run this launcher again. It never terminates an active session.'
}
# Environment belongs only to the new process; user/machine variables are untouched.
$existingBackendIds = @(Get-CimInstance Win32_Process -Filter "Name='codex.exe'" |
    Where-Object { $_.ExecutablePath -in $backendPaths } | Select-Object -ExpandProperty ProcessId)
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
        Where-Object { $_.ExecutablePath -in $backendPaths -and $_.ProcessId -notin $existingBackendIds }
} while (-not $process -and (Get-Date) -lt $deadline)
if (-not $process) {
    $observed = @(Get-CimInstance Win32_Process -Filter "Name='codex.exe'" |
        Select-Object -ExpandProperty ExecutablePath -Unique) -join ', '
    $logs = Join-Path $env:LOCALAPPDATA 'Codex\Logs'
    throw "Custom backend was not observed: $backend. Observed backends: $observed. Launcher PID: $($started.Id). Inspect application logs: $logs"
}
$process | Select-Object ProcessId, ParentProcessId, ExecutablePath
Write-Host 'Custom backend verified. To roll back, close Codex and open its usual Start-menu shortcut.'

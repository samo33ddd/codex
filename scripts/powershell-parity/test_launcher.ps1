$ErrorActionPreference = 'Stop'
$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('codex-launcher-test-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $fixtureRoot -Force | Out-Null
$version = (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'compatibility.json') -Raw | ConvertFrom-Json)
$backend = Join-Path $fixtureRoot 'backend.ps1'
Set-Content -LiteralPath $backend -Value ("'codex-cli " + $version.backendVersion + "'`n" + '$global:LASTEXITCODE = 0')
foreach ($name in @('codex-command-runner.exe', 'codex-windows-sandbox-setup.exe', 'codex-code-mode-host.exe')) {
    New-Item -ItemType File -Path (Join-Path $fixtureRoot $name) | Out-Null
}
$global:LauncherFixture = @{
    Package = [pscustomobject]@{ Version = $version.desktopVersions[0]; InstallLocation = $fixtureRoot }
    Backend = $backend
    Queries = 0
    Started = 0
    Running = $null
}
function Get-AppxPackage { param($Name) $global:LauncherFixture.Package }
function Get-CimInstance {
    param($ClassName, $Filter)
    if ($Filter -eq "Name='ChatGPT.exe'") { return $global:LauncherFixture.Running }
    $global:LauncherFixture.Queries++
    if ($global:LauncherFixture.Queries -ge 3) {
        [pscustomobject]@{ ProcessId = 43; ParentProcessId = 42; ExecutablePath = $global:LauncherFixture.Backend }
    }
}
function Start-Process {
    param($FilePath, $ArgumentList, $Environment, [switch]$PassThru)
    $global:LauncherFixture.Started++
    $global:LauncherFixture.Arguments = $ArgumentList
    $global:LauncherFixture.Environment = $Environment
    [pscustomobject]@{ Id = 42; HasExited = $true }
}
function Start-Sleep { param($Milliseconds) }
try {
    $before = $env:CODEX_CLI_PATH
    $result = & (Join-Path $PSScriptRoot 'Start-Codex.ps1') -BackendPath $backend
    $normalProfile = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'Codex\web\Codex'
    if ($global:LauncherFixture.Arguments -ne ('--user-data-dir="' + $normalProfile + '"')) { throw 'Native profile was not passed explicitly.' }
    if ($result.ProcessId -ne 43 -or $global:LauncherFixture.Queries -ne 3) { throw 'Backend was not observed after launcher exit.' }
    if ($global:LauncherFixture.Environment.CODEX_CLI_PATH -ne $backend -or $env:CODEX_CLI_PATH -ne $before) { throw 'Backend override escaped the child environment.' }
    if ($global:LauncherFixture.Environment.ContainsKey('CODEX_ELECTRON_USER_DATA_PATH')) { throw 'Normal Electron profile must be preserved.' }
    Write-Host 'PASS: native profile, delayed child, process-local override'

    $global:LauncherFixture.Queries = 0
    $isolated = Join-Path $fixtureRoot 'profile with [brackets]'
    $null = & (Join-Path $PSScriptRoot 'Start-Codex.ps1') -BackendPath $backend -UserDataPath $isolated
    if ($global:LauncherFixture.Environment.CODEX_ELECTRON_USER_DATA_PATH -ne $isolated) { throw 'Isolated Electron profile was not set.' }
    Write-Host 'PASS: isolated native and Electron profiles'

    $global:LauncherFixture.Running = [pscustomobject]@{ ExecutablePath = (Join-Path $fixtureRoot 'app\ChatGPT.exe'); CommandLine = ('ChatGPT.exe --user-data-dir="' + $isolated + '"') }
    $starts = $global:LauncherFixture.Started
    try {
        $null = & (Join-Path $PSScriptRoot 'Start-Codex.ps1') -BackendPath $backend -UserDataPath $isolated
        throw 'Expected running-profile rejection.'
    } catch {
        if ($_.Exception.Message -notlike 'Save your work*') { throw }
    }
    if ($global:LauncherFixture.Started -ne $starts) { throw 'Already running profile was launched again.' }
    Write-Host 'PASS: running profile rejected without launching or terminating it'

    $installedApp = Join-Path $fixtureRoot 'app\ChatGPT.exe'
    $preparedApp = Join-Path $fixtureRoot 'prepared\ChatGPT.exe'
    $preparedResources = Join-Path $fixtureRoot 'prepared\resources'
    New-Item -ItemType Directory -Path (Split-Path $installedApp), $preparedResources -Force | Out-Null
    Set-Content -LiteralPath $installedApp -Value 'desktop executable fixture'
    Copy-Item -LiteralPath $installedApp -Destination $preparedApp
    $entries = foreach ($name in @('codex.exe', 'codex-command-runner.exe', 'codex-windows-sandbox-setup.exe', 'codex-code-mode-host.exe')) {
        $source = if ($name -eq 'codex.exe') { $backend } else { Join-Path $fixtureRoot $name }
        Copy-Item -LiteralPath $source -Destination (Join-Path $preparedResources $name)
        @{ file = $name; sha256 = (Get-FileHash -LiteralPath $source).Hash }
    }
    $archive = Join-Path $preparedResources 'app.asar'
    Set-Content -LiteralPath $archive -Value 'desktop UI fixture'
    @{
        backend = @{ version = $version.backendVersion; files = @($entries) }
        patchedAsarSha256 = (Get-FileHash -LiteralPath $archive).Hash
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path (Split-Path $preparedApp) 'desktop-patch-manifest.json')
    $global:LauncherFixture.Running = $null
    $global:LauncherFixture.Queries = 0
    $global:LauncherFixture.Backend = Join-Path $preparedResources 'codex.exe'
    $result = & (Join-Path $PSScriptRoot 'Start-Codex.ps1') -BackendPath $backend -DesktopPath $preparedApp -UserDataPath $isolated
    if ($result.ExecutablePath -ne $global:LauncherFixture.Backend) { throw 'Verified bundled backend fallback was not recognized.' }
    Write-Host 'PASS: desktop uses verified bundled backend when the environment override is lost'

    $global:LauncherFixture.Running = [pscustomobject]@{ ExecutablePath = (Join-Path $fixtureRoot 'older-copy\ChatGPT.exe'); CommandLine = ('ChatGPT.exe --user-data-dir="' + $isolated + '"') }
    $starts = $global:LauncherFixture.Started
    try {
        $null = & (Join-Path $PSScriptRoot 'Start-Codex.ps1') -BackendPath $backend -DesktopPath $preparedApp -UserDataPath $isolated
        throw 'Expected other-copy profile rejection.'
    } catch {
        if ($_.Exception.Message -notlike 'Save your work*') { throw }
    }
    if ($global:LauncherFixture.Started -ne $starts) { throw 'Another copy sharing the profile was ignored.' }
    Write-Host 'PASS: another desktop copy with the same profile blocks launch'

    $global:LauncherFixture.Running = $null
    Set-Content -LiteralPath (Join-Path $preparedResources 'codex.exe') -Value 'stock backend substituted'
    try {
        $null = & (Join-Path $PSScriptRoot 'Start-Codex.ps1') -BackendPath $backend -DesktopPath $preparedApp -UserDataPath $isolated
        throw 'Expected substituted backend rejection.'
    } catch {
        if ($_.Exception.Message -notlike 'Custom backend component does not match*') { throw }
    }
    if ($global:LauncherFixture.Started -ne $starts) { throw 'A substituted backend was launched.' }
    Write-Host 'PASS: stock backend substitution is rejected before launching'
} finally {
    $resolvedFixture = [System.IO.Path]::GetFullPath($fixtureRoot)
    $temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    if ($resolvedFixture.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path $resolvedFixture -Leaf).StartsWith('codex-launcher-test-')) {
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
    Remove-Variable -Name LauncherFixture -Scope Global
}

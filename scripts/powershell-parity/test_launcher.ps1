$ErrorActionPreference = 'Stop'
$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('codex-launcher-test-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $fixtureRoot -Force | Out-Null
$version = (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'compatibility.json') -Raw | ConvertFrom-Json)
$backend = Join-Path $fixtureRoot 'backend.ps1'
$launcher = Join-Path $fixtureRoot 'Start-Codex.ps1'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Start-Codex.ps1') -Destination $launcher
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'compatibility.json') -Destination $fixtureRoot
New-Item -ItemType File -Path (Join-Path $fixtureRoot 'prepare_desktop.py') | Out-Null
Set-Content -LiteralPath $backend -Value ("'codex-cli " + $version.backendVersion + "'`n" + '$global:LASTEXITCODE = 0')
Copy-Item -LiteralPath $backend -Destination (Join-Path $fixtureRoot 'codex.exe')
foreach ($name in @('codex-command-runner.exe', 'codex-windows-sandbox-setup.exe', 'codex-code-mode-host.exe')) {
    New-Item -ItemType File -Path (Join-Path $fixtureRoot $name) | Out-Null
}
$global:LauncherFixture = @{
    Package = [pscustomobject]@{ Version = $version.desktopVersions[0]; InstallLocation = $fixtureRoot }
    Backend = $backend
    Queries = 0
    Started = 0
    Running = $null
    Prepared = 0
    FailPreparation = $false
}
function Get-Command {
    param($Name, $ErrorAction)
    if ($Name -eq 'python') { return [pscustomobject]@{ Source = 'Invoke-DesktopPreparation' } }
    if ($Name -eq 'node') { return [pscustomobject]@{ Source = 'node' } }
    throw "Unexpected command lookup: $Name"
}
function Invoke-DesktopPreparation {
    $global:LauncherFixture.Prepared++
    if ($global:LauncherFixture.FailPreparation) { $global:LASTEXITCODE = 1; return }
    $output = $args[-1]
    New-Item -ItemType Directory -Path (Split-Path $output) -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $fixtureRoot 'prepared') -Destination $output -Recurse
    Copy-Item -LiteralPath $installedApp -Destination (Join-Path $output 'ChatGPT.exe')
    $manifestPath = Join-Path $output 'desktop-patch-manifest.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $manifest.sourceAsarSha256 = (Get-FileHash -LiteralPath $installedArchive).Hash
    $manifest.executableSha256 = (Get-FileHash -LiteralPath $installedApp).Hash
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath
    @{ desktopPath = (Join-Path $output 'ChatGPT.exe') } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $fixtureRoot 'desktop-bundle.json')
    Set-Item -LiteralPath ('Function:\global:' + (Join-Path $output 'resources\codex.exe')) -Value $versionCommand
    $global:LASTEXITCODE = 0
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
    $global:LauncherFixture.Desktop = $FilePath
    $global:LauncherFixture.Arguments = $ArgumentList
    $global:LauncherFixture.Environment = $Environment
    [pscustomobject]@{ Id = 42; HasExited = $true }
}
function Start-Sleep { param($Milliseconds) }
try {
    $installedApp = Join-Path $fixtureRoot 'app\ChatGPT.exe'
    $preparedApp = Join-Path $fixtureRoot 'prepared\ChatGPT.exe'
    $preparedResources = Join-Path $fixtureRoot 'prepared\resources'
    New-Item -ItemType Directory -Path (Split-Path $installedApp), $preparedResources -Force | Out-Null
    Set-Content -LiteralPath $installedApp -Value 'desktop executable fixture'
    $installedArchive = Join-Path $fixtureRoot 'app\resources\app.asar'
    New-Item -ItemType Directory -Path (Split-Path $installedArchive) | Out-Null
    Set-Content -LiteralPath $installedArchive -Value 'installed UI fixture'
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
        sourceAsarSha256 = (Get-FileHash -LiteralPath $installedArchive).Hash
        executableSha256 = (Get-FileHash -LiteralPath $installedApp).Hash
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path (Split-Path $preparedApp) 'desktop-patch-manifest.json')
    $fixtureVersion = $version.backendVersion
    $versionCommand = { "codex-cli $fixtureVersion"; $global:LASTEXITCODE = 0 }.GetNewClosure()
    Set-Item -LiteralPath ('Function:\global:' + (Join-Path $fixtureRoot 'codex.exe')) -Value $versionCommand
    Set-Item -LiteralPath ('Function:\global:' + (Join-Path $preparedResources 'codex.exe')) -Value $versionCommand
    $validated = & $launcher -ValidateOnly
    if ($global:LauncherFixture.Prepared -ne 1 -or $global:LauncherFixture.Started -ne 0 -or $validated.DesktopPath -eq $installedApp) { throw 'First launch did not prepare a separate bundle.' }
    $null = & $launcher -ValidateOnly
    if ($global:LauncherFixture.Prepared -ne 1) { throw 'Unchanged desktop was rebuilt.' }
    Write-Host 'PASS: first use prepares a bundle and unchanged launches reuse it'
    $registration = Join-Path $fixtureRoot 'desktop-bundle.json'
    @{ desktopPath = $preparedApp } | ConvertTo-Json | Set-Content -LiteralPath $registration
    $global:LauncherFixture.Backend = Join-Path $preparedResources 'codex.exe'
    $validated = & $launcher -ValidateOnly
    if ($validated.DesktopPath -ne $preparedApp -or $validated.BackendPath -ne $global:LauncherFixture.Backend) { throw 'Default validation did not select the registered bundle.' }
    $result = & $launcher
    if ($result.ExecutablePath -ne $global:LauncherFixture.Backend -or $global:LauncherFixture.Desktop -ne $preparedApp) { throw 'Default launch did not start the registered bundle.' }
    if ($global:LauncherFixture.Environment.CODEX_CLI_PATH -ne $global:LauncherFixture.Backend) { throw 'Default launch did not select the bundled backend.' }
    Write-Host 'PASS: no-argument validation and launch select the registered desktop and backend'
    $global:LauncherFixture.Queries = 0
    $before = $env:CODEX_CLI_PATH
    $result = & $launcher -BackendPath $backend
    $normalProfile = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'Codex\web\Codex'
    if ($global:LauncherFixture.Arguments -ne ('--user-data-dir="' + $normalProfile + '"')) { throw 'Native profile was not passed explicitly.' }
    if ($result.ProcessId -ne 43 -or $global:LauncherFixture.Queries -ne 3) { throw 'Backend was not observed after launcher exit.' }
    if ($global:LauncherFixture.Environment.CODEX_CLI_PATH -ne $backend -or $env:CODEX_CLI_PATH -ne $before) { throw 'Backend override escaped the child environment.' }
    if ($global:LauncherFixture.Environment.ContainsKey('CODEX_ELECTRON_USER_DATA_PATH')) { throw 'Normal Electron profile must be preserved.' }
    Write-Host 'PASS: native profile, delayed child, process-local override'

    $global:LauncherFixture.Queries = 0
    $isolated = Join-Path $fixtureRoot 'profile with [brackets]'
    $null = & $launcher -BackendPath $backend -UserDataPath $isolated
    if ($global:LauncherFixture.Environment.CODEX_ELECTRON_USER_DATA_PATH -ne $isolated) { throw 'Isolated Electron profile was not set.' }
    Write-Host 'PASS: isolated native and Electron profiles'

    $global:LauncherFixture.Running = [pscustomobject]@{ ExecutablePath = (Join-Path $fixtureRoot 'app\ChatGPT.exe'); CommandLine = ('ChatGPT.exe --user-data-dir="' + $isolated + '"') }
    $starts = $global:LauncherFixture.Started
    try {
        $null = & $launcher -BackendPath $backend -UserDataPath $isolated
        throw 'Expected running-profile rejection.'
    } catch {
        if ($_.Exception.Message -notlike 'Save your work*') { throw }
    }
    if ($global:LauncherFixture.Started -ne $starts) { throw 'Already running profile was launched again.' }
    Write-Host 'PASS: running profile rejected without launching or terminating it'

    @{ desktopPath = '' } | ConvertTo-Json | Set-Content -LiteralPath $registration
    try {
        $null = & $launcher
        throw 'Expected invalid desktop registration rejection.'
    } catch {
        if ($_.Exception.Message -notlike 'Missing desktopPath in registration*') { throw }
    }
    if ($global:LauncherFixture.Started -ne $starts) { throw 'Invalid registration started the stock desktop.' }
    Write-Host 'PASS: invalid registration is rejected without fallback to the stock app'

    $global:LauncherFixture.Running = $null
    $global:LauncherFixture.Queries = 0
    $global:LauncherFixture.Backend = Join-Path $preparedResources 'codex.exe'
    $result = & $launcher -BackendPath $backend -DesktopPath $preparedApp -UserDataPath $isolated
    if ($result.ExecutablePath -ne $global:LauncherFixture.Backend) { throw 'Verified bundled backend fallback was not recognized.' }
    Write-Host 'PASS: explicit desktop overrides registration and recognizes its verified backend'

    $global:LauncherFixture.Running = [pscustomobject]@{ ExecutablePath = (Join-Path $fixtureRoot 'older-copy\ChatGPT.exe'); CommandLine = ('ChatGPT.exe --user-data-dir="' + $isolated + '"') }
    $starts = $global:LauncherFixture.Started
    try {
        $null = & $launcher -BackendPath $backend -DesktopPath $preparedApp -UserDataPath $isolated
        throw 'Expected other-copy profile rejection.'
    } catch {
        if ($_.Exception.Message -notlike 'Save your work*') { throw }
    }
    if ($global:LauncherFixture.Started -ne $starts) { throw 'Another copy sharing the profile was ignored.' }
    Write-Host 'PASS: another desktop copy with the same profile blocks launch'

    $global:LauncherFixture.Running = $null
    @{ desktopPath = $preparedApp } | ConvertTo-Json | Set-Content -LiteralPath $registration
    $oldRegistration = Get-Content -LiteralPath $registration -Raw
    $global:LauncherFixture.Package.Version = '99.1.2.3'
    Set-Content -LiteralPath $installedArchive -Value 'updated UI with unchanged executable'
    $global:LauncherFixture.FailPreparation = $true
    try {
        $null = & $launcher -ValidateOnly
        throw 'Expected preparation failure.'
    } catch {
        if ($_.Exception.Message -notlike 'Desktop preparation failed*') { throw }
    }
    if ((Get-Content -LiteralPath $registration -Raw) -ne $oldRegistration -or $global:LauncherFixture.Started -ne $starts) { throw 'Failed update changed the registration or launched a process.' }
    $global:LauncherFixture.FailPreparation = $false
    $validated = & $launcher -ValidateOnly
    if ($validated.DesktopVersion -ne '99.1.2.3' -or $validated.DesktopPath -eq $preparedApp) { throw 'Unknown desktop update was not prepared.' }
    $preparedCount = $global:LauncherFixture.Prepared
    $global:LauncherFixture.Queries = 0
    $global:LauncherFixture.Backend = $validated.BackendPath
    $result = & $launcher
    if ($global:LauncherFixture.Prepared -ne $preparedCount -or $result.ExecutablePath -ne $validated.BackendPath) { throw 'Updated launch did not reuse and start the verified backend.' }
    if (-not (Test-Path -LiteralPath $preparedApp)) { throw 'Previous bundle was removed.' }
    Write-Host 'PASS: unknown desktop update, unchanged EXE, rollback on failure, reuse and launch'
    Set-Content -LiteralPath $installedArchive -Value 'installed UI fixture'
    $starts = $global:LauncherFixture.Started
    Set-Content -LiteralPath (Join-Path $preparedResources 'codex.exe') -Value 'stock backend substituted'
    try {
        $null = & $launcher -BackendPath $backend -DesktopPath $preparedApp -UserDataPath $isolated
        throw 'Expected substituted backend rejection.'
    } catch {
        if ($_.Exception.Message -notlike 'Custom backend component does not match*') { throw }
    }
    if ($global:LauncherFixture.Started -ne $starts) { throw 'A substituted backend was launched.' }
    Write-Host 'PASS: stock backend substitution is rejected before launching'
} finally {
    Remove-Item -LiteralPath ('Function:\' + (Join-Path $fixtureRoot 'codex.exe')), ('Function:\' + (Join-Path $preparedResources 'codex.exe')) -ErrorAction SilentlyContinue
    $resolvedFixture = [System.IO.Path]::GetFullPath($fixtureRoot)
    $temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    if ($resolvedFixture.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path $resolvedFixture -Leaf).StartsWith('codex-launcher-test-')) {
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
    Remove-Variable -Name LauncherFixture -Scope Global
}

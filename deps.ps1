[CmdletBinding()]
param([switch]$CheckOnly)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'LanternLeaf deps.ps1 currently supports Windows only.' }

$repoRoot = (Resolve-Path $PSScriptRoot).Path
$dependencyManifestPath = Join-Path $repoRoot 'deps.windows.json'
if (-not (Test-Path $dependencyManifestPath -PathType Leaf)) {
    throw "Missing dependency manifest: $dependencyManifestPath"
}
$deps = Get-Content -LiteralPath $dependencyManifestPath -Raw | ConvertFrom-Json
if ($deps.schema_version -ne 1) {
    throw "Unsupported deps.windows.json schema version: $($deps.schema_version)"
}

function Refresh-ProcessPath {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $extras = @(
        (Join-Path $env:USERPROFILE '.cargo\bin'),
        (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Links'),
        (Join-Path $env:ProgramFiles 'CMake\bin'),
        (Join-Path $env:ProgramFiles 'Pandoc'),
        (Join-Path $env:LOCALAPPDATA 'Pandoc')
    )
    $parts = @($machine, $user) + $extras
    $env:Path = ($parts | Where-Object { $_ -and (Test-Path $_) } | Select-Object -Unique) -join ';'
}

function Has-Command([string]$Name) {
    return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Require-Winget {
    if (-not (Has-Command 'winget.exe')) {
        throw 'Windows Package Manager (winget) is required for automatic bootstrap. Install/update Microsoft App Installer, then rerun .\deps.ps1.'
    }
}

function Install-WingetPackage([string]$Id, [string]$Label, [string]$Override = '') {
    Require-Winget
    Write-Host "Installing $Label ($Id)..."
    $args = @('install','--source','winget','--exact','--id',$Id,'--accept-source-agreements','--accept-package-agreements')
    if ($Override) { $args += @('--override', $Override) }
    & winget.exe @args
    if ($LASTEXITCODE -ne 0) { throw "winget failed while installing $Label ($Id), exit code $LASTEXITCODE" }
    Refresh-ProcessPath
}

function Find-VsWhere {
    $pf86 = [Environment]::GetFolderPath('ProgramFilesX86')
    $candidates = @(
        (Join-Path $pf86 'Microsoft Visual Studio\Installer\vswhere.exe'),
        (Join-Path $env:ProgramFiles 'Microsoft Visual Studio\Installer\vswhere.exe')
    )
    return $candidates | Where-Object { $_ -and (Test-Path $_ -PathType Leaf) } | Select-Object -First 1
}

function Find-VcInstance {
    $vswhere = Find-VsWhere
    if (-not $vswhere) { return $null }
    $path = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0) { return $null }
    if ($path) { return ($path | Select-Object -First 1).Trim() }
    return $null
}

function Find-AnyVsInstance {
    $vswhere = Find-VsWhere
    if (-not $vswhere) { return $null }
    $path = & $vswhere -latest -products '*' -property installationPath
    if ($LASTEXITCODE -ne 0) { return $null }
    if ($path) { return ($path | Select-Object -First 1).Trim() }
    return $null
}

function Ensure-Rust {
    Refresh-ProcessPath
    if (-not (Has-Command 'rustup.exe')) {
        if ($CheckOnly) { throw 'Missing dependency: rustup (Rustlang.Rustup)' }
        Install-WingetPackage $deps.rust.winget_id 'Rustup'
    }
    Refresh-ProcessPath
    if (-not (Has-Command 'cargo.exe')) {
        if ($CheckOnly) { throw 'Missing dependency: cargo' }
        & rustup.exe default stable
        if ($LASTEXITCODE -ne 0) { throw 'rustup failed to install/select stable Rust.' }
    }
    $targets = @(& rustup.exe target list --installed)
    if ($targets -notcontains $deps.rust.target) {
        if ($CheckOnly) { throw 'Missing Rust target: x86_64-pc-windows-msvc' }
        & rustup.exe target add x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { throw 'rustup failed to add x86_64-pc-windows-msvc.' }
    }
}

function Ensure-CommandPackage([string]$Command, [string]$Id, [string]$Label) {
    Refresh-ProcessPath
    if (Has-Command $Command) { return }
    if ($CheckOnly) { throw "Missing dependency: $Label ($Command)" }
    Install-WingetPackage $Id $Label
    Refresh-ProcessPath
    if (-not (Has-Command $Command)) {
        throw "$Label installed but $Command is still unavailable. Open a fresh terminal and rerun .\deps.ps1."
    }
}

function Ensure-Msvc {
    if (Find-VcInstance) { return }
    if ($CheckOnly) { throw "Missing dependency: $($deps.visual_studio.name) / $($deps.visual_studio.workload)" }

    $existing = Find-AnyVsInstance
    if ($existing) {
        $pf86 = [Environment]::GetFolderPath('ProgramFilesX86')
        $setup = Join-Path $pf86 'Microsoft Visual Studio\Installer\setup.exe'
        if (-not (Test-Path $setup -PathType Leaf)) {
            throw "Visual Studio is installed at '$existing' but the Visual Studio Installer could not be found to add the C++ workload."
        }
        Write-Host "Adding the Visual C++ workload to: $existing"
        & $setup modify --installPath $existing --add $($deps.visual_studio.workload) --includeRecommended --passive --norestart
        if ($LASTEXITCODE -notin @(0, 3010)) { throw "Visual Studio Installer failed with exit code $LASTEXITCODE" }
    } else {
        $includeRecommended = if ($deps.visual_studio.include_recommended) { ' --includeRecommended' } else { '' }
        $override = "--wait --passive --norestart --add $($deps.visual_studio.workload)$includeRecommended"
        Install-WingetPackage $deps.visual_studio.winget_id "$($deps.visual_studio.name) + C++ workload" $override
    }

    if (-not (Find-VcInstance)) {
        throw 'Visual C++ build tools are still unavailable. A reboot or a fresh terminal may be required; then rerun .\deps.ps1.'
    }
}

Write-Host 'LanternLeaf Windows dependency bootstrap'
Write-Host "Repository: $repoRoot"
if ($CheckOnly) { Write-Host 'Mode: check only (no installs)' }

Ensure-Rust
foreach ($package in $deps.packages) {
    Ensure-CommandPackage $package.command $package.winget_id $package.name
}
Ensure-Msvc
& (Join-Path $repoRoot 'scripts\windows-dev-env.ps1') -Quiet

Write-Host 'LanternLeaf Windows dependencies are ready.'
Write-Host 'Normal manual QA entrypoint: .\qa.ps1'

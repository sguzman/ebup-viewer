[CmdletBinding()]
param([switch]$CheckOnly)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($env:OS -ne 'Windows_NT') {
    throw 'LanternLeaf deps.ps1 currently supports Windows only.'
}

$repoRoot = (Resolve-Path $PSScriptRoot).Path
$scoopfile = Join-Path $repoRoot 'Scoopfile.json'
if (-not (Test-Path $scoopfile -PathType Leaf)) {
    throw "Missing Scoop dependency file: $scoopfile"
}
$scoopDeps = Get-Content -LiteralPath $scoopfile -Raw | ConvertFrom-Json

function Refresh-ProcessPath {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $scoopRoot = if ($env:SCOOP) { $env:SCOOP } else { Join-Path $env:USERPROFILE 'scoop' }
    $extras = @(
        (Join-Path $scoopRoot 'shims'),
        (Join-Path $env:USERPROFILE '.cargo\bin')
    )
    $parts = @($machine, $user) + $extras
    $env:Path = ($parts | Where-Object { $_ -and (Test-Path $_) } | Select-Object -Unique) -join ';'
}

function Has-Command([string]$Name) {
    return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Ensure-Scoop {
    Refresh-ProcessPath
    if (Has-Command 'scoop') { return }
    if ($CheckOnly) {
        throw 'Missing dependency manager: Scoop. Run .\deps.ps1 without -CheckOnly to bootstrap it.'
    }

    Write-Host 'Bootstrapping Scoop from https://get.scoop.sh ...'
    $previousPolicy = Get-ExecutionPolicy -Scope Process
    $installer = Join-Path $env:TEMP 'lanternleaf-scoop-install.ps1'
    try {
        Set-ExecutionPolicy -ExecutionPolicy Bypass -Scope Process -Force
        Invoke-RestMethod -Uri 'https://get.scoop.sh' -OutFile $installer
        $principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
        $isAdmin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
        if ($isAdmin) {
            & $installer -RunAsAdmin
        } else {
            & $installer
        }
        if ($LASTEXITCODE -ne 0) {
            throw "Scoop installer failed with exit code $LASTEXITCODE"
        }
    } finally {
        if ($previousPolicy) {
            Set-ExecutionPolicy -ExecutionPolicy $previousPolicy -Scope Process -Force
        }
        Remove-Item $installer -Force -ErrorAction SilentlyContinue
    }

    Refresh-ProcessPath
    if (-not (Has-Command 'scoop')) {
        throw 'Scoop installed but is not available in the current process PATH.'
    }
}

function Ensure-ScoopDependencies {
    Ensure-Scoop
    if ($CheckOnly) {
        foreach ($app in $scoopDeps.apps) {
            & scoop prefix $app.Name *> $null
            if ($LASTEXITCODE -ne 0) {
                throw "Missing Scoop dependency: $($app.Source)/$($app.Name)"
            }
        }
        return
    }

    Write-Host "Importing Scoop dependencies from $scoopfile ..."
    & scoop import $scoopfile
    if ($LASTEXITCODE -ne 0) {
        throw "scoop import failed with exit code $LASTEXITCODE"
    }
    Refresh-ProcessPath
}

function Ensure-Rust {
    Refresh-ProcessPath
    if (-not (Has-Command 'rustup.exe')) {
        if ($CheckOnly) { throw 'Missing Rust toolchain manager: rustup.' }
        Ensure-Scoop
        Write-Host 'Installing rustup through Scoop...'
        & scoop install main/rustup
        if ($LASTEXITCODE -ne 0) {
            throw "scoop install main/rustup failed with exit code $LASTEXITCODE"
        }
        Refresh-ProcessPath
    }

    if (-not (Has-Command 'cargo.exe')) {
        if ($CheckOnly) { throw 'Missing dependency: cargo' }
        & rustup.exe default stable
        if ($LASTEXITCODE -ne 0) { throw 'rustup failed to install/select stable Rust.' }
        Refresh-ProcessPath
    }

    $requiredTarget = 'x86_64-pc-windows-msvc'
    $targets = @(& rustup.exe target list --installed)
    if ($targets -notcontains $requiredTarget) {
        if ($CheckOnly) { throw "Missing Rust target: $requiredTarget" }
        & rustup.exe target add $requiredTarget
        if ($LASTEXITCODE -ne 0) { throw "rustup failed to add $requiredTarget." }
    }
}

function Find-VsWhere {
    $command = Get-Command 'vswhere.exe' -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
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

function Require-WingetForMsvc {
    if (-not (Has-Command 'winget.exe')) {
        throw 'Visual Studio C++ Build Tools are missing, and WinGet is unavailable for that Windows-native bootstrap. Install Microsoft App Installer or the Visual Studio C++ workload, then rerun .\deps.ps1.'
    }
}

function Ensure-Msvc {
    if (Find-VcInstance) { return }
    if ($CheckOnly) {
        throw 'Missing dependency: Visual Studio 2022 C++ build tools / Microsoft.VisualStudio.Workload.VCTools'
    }

    $existing = Find-AnyVsInstance
    if ($existing) {
        $pf86 = [Environment]::GetFolderPath('ProgramFilesX86')
        $setup = Join-Path $pf86 'Microsoft Visual Studio\Installer\setup.exe'
        if (-not (Test-Path $setup -PathType Leaf)) {
            throw "Visual Studio is installed at '$existing' but the Visual Studio Installer could not be found to add the C++ workload."
        }
        Write-Host "Adding the Visual C++ workload to: $existing"
        & $setup modify --installPath $existing --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive --norestart
        if ($LASTEXITCODE -notin @(0, 3010)) {
            throw "Visual Studio Installer failed with exit code $LASTEXITCODE"
        }
    } else {
        Require-WingetForMsvc
        Write-Host 'Installing Visual Studio 2022 Build Tools + C++ workload (the one non-Scoop dependency)...'
        $wingetArgs = @(
            'install', '--source', 'winget', '--exact', '--id', 'Microsoft.VisualStudio.2022.BuildTools',
            '--accept-source-agreements', '--accept-package-agreements',
            '--override', '--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
        )
        & winget.exe @wingetArgs
        if ($LASTEXITCODE -ne 0) {
            throw "WinGet failed while installing Visual Studio Build Tools, exit code $LASTEXITCODE"
        }
    }

    if (-not (Find-VcInstance)) {
        throw 'Visual C++ build tools are still unavailable. A reboot or fresh terminal may be required; then rerun .\deps.ps1.'
    }
}

Write-Host 'LanternLeaf Windows dependency bootstrap'
Write-Host "Repository: $repoRoot"
Write-Host "Scoopfile: $scoopfile"
if ($CheckOnly) { Write-Host 'Mode: check only (no installs)' }

Ensure-ScoopDependencies
Ensure-Rust
Ensure-Msvc
& (Join-Path $repoRoot 'scripts\windows-dev-env.ps1') -Quiet

Write-Host 'LanternLeaf Windows dependencies are ready.'
Write-Host 'Scoop dependencies can also be restored directly with: scoop import .\Scoopfile.json'
Write-Host 'Normal manual QA entrypoint: .\qa.ps1'

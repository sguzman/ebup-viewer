[CmdletBinding()]
param([switch]$Quiet)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'windows-dev-env.ps1 supports Windows only.' }

function Add-PathIfPresent([string]$Path) {
    if ($Path -and (Test-Path $Path) -and (($env:Path -split ';') -notcontains $Path)) {
        $env:Path = "$Path;$env:Path"
    }
}

$scoopRoot = if ($env:SCOOP) { $env:SCOOP } else { Join-Path $env:USERPROFILE 'scoop' }
Add-PathIfPresent (Join-Path $scoopRoot 'shims')
Add-PathIfPresent (Join-Path $env:USERPROFILE '.cargo\bin')

$vswhereCommand = Get-Command 'vswhere.exe' -ErrorAction SilentlyContinue
$vswhere = if ($vswhereCommand) { $vswhereCommand.Source } else { $null }
if (-not $vswhere) {
    $pf86 = [Environment]::GetFolderPath('ProgramFilesX86')
    $vswhereCandidates = @(
        (Join-Path $pf86 'Microsoft Visual Studio\Installer\vswhere.exe'),
        (Join-Path $env:ProgramFiles 'Microsoft Visual Studio\Installer\vswhere.exe')
    )
    $vswhere = $vswhereCandidates | Where-Object { $_ -and (Test-Path $_ -PathType Leaf) } | Select-Object -First 1
}
if (-not $vswhere) { throw 'Visual Studio locator (vswhere.exe) is missing. Run .\deps.ps1.' }

$installPath = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $installPath) {
    throw 'No Visual Studio instance with the C++ toolchain was found. Run .\deps.ps1.'
}
$installPath = ($installPath | Select-Object -First 1).Trim()

$vsDevCmd = Join-Path $installPath 'Common7\Tools\VsDevCmd.bat'
if (-not (Test-Path $vsDevCmd -PathType Leaf)) {
    throw "VsDevCmd.bat not found under '$installPath'. Run .\deps.ps1."
}

$cmdLine = '"' + $vsDevCmd + '" -no_logo -arch=x64 -host_arch=x64 >nul && set'
$lines = & $env:ComSpec /d /s /c $cmdLine
if ($LASTEXITCODE -ne 0) { throw "VsDevCmd failed with exit code $LASTEXITCODE" }

foreach ($line in $lines) {
    if ($line -match '^([^=]+)=(.*)$') {
        $name = $Matches[1]
        if ($name.StartsWith('=')) { continue }
        [Environment]::SetEnvironmentVariable($name, $Matches[2], 'Process')
    }
}

$required = @('cargo.exe', 'cl.exe', 'MSBuild.exe', 'cmake.exe', 'ninja.exe', 'pandoc.exe')
$missing = @($required | Where-Object { $null -eq (Get-Command $_ -ErrorAction SilentlyContinue) })
if ($missing.Count -gt 0) {
    throw "Windows development environment is incomplete: $($missing -join ', '). Run .\deps.ps1."
}

function Test-MsvcCompiler {
    $probeRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("lanternleaf-msvc-probe-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $probeRoot | Out-Null
    try {
        $cSource = Join-Path $probeRoot 'probe.c'
        $cppSource = Join-Path $probeRoot 'probe.cpp'
        Set-Content -LiteralPath $cSource -Value 'int lanternleaf_c_probe(void) { return 0; }' -Encoding Ascii
        Set-Content -LiteralPath $cppSource -Value 'int lanternleaf_cpp_probe() { return 0; }' -Encoding Ascii

        & cl.exe /nologo /c $cSource /Fo"$probeRoot\probe-c.obj" *> $null
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path (Join-Path $probeRoot 'probe-c.obj'))) {
            throw 'MSVC C compiler probe failed even though cl.exe is on PATH.'
        }

        & cl.exe /nologo /c $cppSource /Fo"$probeRoot\probe-cpp.obj" *> $null
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path (Join-Path $probeRoot 'probe-cpp.obj'))) {
            throw 'MSVC C++ compiler probe failed even though cl.exe is on PATH.'
        }
    } finally {
        Remove-Item $probeRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Test-MsvcCompiler

if (-not $Quiet) {
    Write-Host "MSVC environment: $installPath"
    Write-Host "cl: $((Get-Command cl.exe).Source)"
    Write-Host "MSBuild: $((Get-Command MSBuild.exe).Source)"
    Write-Host "CMake: $((Get-Command cmake.exe).Source)"
    Write-Host "Ninja: $((Get-Command ninja.exe).Source)"
    Write-Host "Pandoc: $((Get-Command pandoc.exe).Source)"
    Write-Host "Cargo: $((Get-Command cargo.exe).Source)"
}

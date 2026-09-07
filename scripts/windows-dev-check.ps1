$ErrorActionPreference = "Continue"

Write-Host "LanternLeaf Windows development prerequisite check"
Write-Host "Repository: $((Get-Location).Path)"

$rustcInfo = & rustc -Vv 2>&1
if ($LASTEXITCODE -eq 0) {
    $rustcInfo | ForEach-Object { Write-Host $_ }
} else {
    Write-Host "MISSING: rustc"
}

$missing = [System.Collections.Generic.List[string]]::new()

function Report-Command([string] $Name, [string] $Purpose) {
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        Write-Host "OK: $Purpose ($($command.Source))"
        return $true
    }

    Write-Host "MISSING: $Purpose ($Name)"
    $missing.Add($Name)
    return $false
}

$null = Report-Command "cargo" "Cargo"
$null = Report-Command "cmake" "CMake"
$null = Report-Command "pandoc" "Pandoc for HTML/DOC conversion tests"

$cl = Get-Command cl.exe -ErrorAction SilentlyContinue
if ($null -ne $cl) {
    Write-Host "OK: MSVC C compiler ($($cl.Source))"
} else {
    Write-Host "MISSING: MSVC C/C++ compiler (cl.exe is not on PATH)"
    Write-Host "GUIDANCE: run this from a Visual Studio Developer PowerShell or install the Desktop development with C++ workload."
    $missing.Add("cl.exe")
}

$msbuild = Get-Command MSBuild.exe -ErrorAction SilentlyContinue
if ($null -ne $msbuild) {
    Write-Host "OK: MSBuild ($($msbuild.Source))"
} else {
    Write-Host "MISSING: MSBuild.exe (required by the CMake Visual Studio generator)"
    $missing.Add("MSBuild.exe")
}

if ($missing.Count -gt 0) {
    Write-Host "Missing prerequisites: $($missing -join ', ')"
    exit 1
}

Write-Host "All detected Windows development prerequisites are available."

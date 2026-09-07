[CmdletBinding()]
param(
    [switch]$SkipDependencyCheck,
    [switch]$Release,
    [switch]$ResetQaState,
    [switch]$PrepareOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($env:OS -ne 'Windows_NT') {
    throw 'LanternLeaf qa.ps1 currently supports Windows only.'
}

$repoRoot = (Resolve-Path $PSScriptRoot).Path

if (-not $SkipDependencyCheck) {
    & (Join-Path $repoRoot 'deps.ps1')
}

& (Join-Path $repoRoot 'scripts\windows-dev-env.ps1') -Quiet

$qaRoot = Join-Path $repoRoot '.qa\windows'
$fixtureRoot = Join-Path $qaRoot 'fixtures'
$configRoot = Join-Path $qaRoot 'conf'
$cacheRoot = Join-Path $qaRoot 'cache'
$logRoot = Join-Path $qaRoot 'logs'
$handoffRoot = Join-Path $qaRoot 'handoff'

if ($ResetQaState -and (Test-Path $qaRoot)) {
    Remove-Item $qaRoot -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $fixtureRoot, $configRoot, $cacheRoot, $logRoot, $handoffRoot | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $configRoot 'pandoc') | Out-Null

foreach ($file in @('config.toml', 'normalizer.toml', 'abbreviations.toml', 'quack-check.toml')) {
    $destination = Join-Path $configRoot $file
    if (-not (Test-Path $destination)) {
        Copy-Item (Join-Path $repoRoot "conf\$file") $destination
    }
}
$filterDestination = Join-Path $configRoot 'pandoc\strip-nontext.lua'
if (-not (Test-Path $filterDestination)) {
    Copy-Item (Join-Path $repoRoot 'conf\pandoc\strip-nontext.lua') $filterDestination
}

foreach ($fixture in @('representative.txt', 'representative.md', 'representative.html')) {
    Copy-Item (Join-Path $repoRoot "tests\fixtures\source-ingestion\$fixture") (Join-Path $fixtureRoot $fixture) -Force
}
& (Join-Path $repoRoot 'scripts\new-representative-epub.ps1') -OutputPath (Join-Path $fixtureRoot 'representative.epub')

$runId = Get-Date -Format 'yyyyMMdd-HHmmss'
$runLogs = Join-Path $logRoot $runId
$handoff = Join-Path $handoffRoot $runId
New-Item -ItemType Directory -Force -Path $runLogs, $handoff | Out-Null

$env:LANTERNLEAF_CONFIG_PATH = Join-Path $configRoot 'config.toml'
$env:LANTERNLEAF_NORMALIZER_CONFIG_PATH = Join-Path $configRoot 'normalizer.toml'
$env:LANTERNLEAF_ABBREVIATIONS_CONFIG_PATH = Join-Path $configRoot 'abbreviations.toml'
$env:QUACK_CHECK_CONFIG = Join-Path $configRoot 'quack-check.toml'
$env:LANTERNLEAF_LOG_DIR = $runLogs
$env:LANTERNLEAF_CACHE_DIR = $cacheRoot

Write-Host ''
Write-Host 'LanternLeaf real-desktop QA'
Write-Host "Fixtures: $fixtureRoot"
Write-Host "Checklist: $(Join-Path $repoRoot 'docs\qa\non-pdf-reader-windows-checklist.md')"
Write-Host "Logs: $runLogs"
Write-Host ''
Write-Host 'Open the four files under .qa\windows\fixtures from LanternLeaf.'
Write-Host 'Windows TTS is the critical audio path; Piper may still require a configured model.'
Write-Host ''

if ($PrepareOnly) {
    Write-Host 'Repo-native QA preparation passed.'
    exit 0
}

Push-Location $repoRoot
try {
    $cargoArgs = @('run', '--bin', 'lanternleaf')
    if ($Release) {
        $cargoArgs = @('run', '--release', '--bin', 'lanternleaf')
    }
    & cargo.exe @cargoArgs
    $exitCode = $LASTEXITCODE
} finally {
    Pop-Location
}

Get-ChildItem -LiteralPath $runLogs -File -ErrorAction SilentlyContinue | Copy-Item -Destination $handoff -Force
@(
    "Repository: $repoRoot",
    "Fixtures: $fixtureRoot",
    "Logs: $runLogs",
    "Application exit code: $exitCode"
) | Set-Content -LiteralPath (Join-Path $handoff 'README.txt')

Write-Host ''
Write-Host "LanternLeaf exited with code $exitCode."
Write-Host "QA handoff: $handoff"
exit $exitCode

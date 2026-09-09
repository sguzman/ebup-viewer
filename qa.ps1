[CmdletBinding()]
param(
    [switch]$SkipDependencyCheck,
    [switch]$Release,
    [switch]$ResetQaState,
    [switch]$PrepareOnly,
    [ValidateSet('Windows', 'Piper')]
    [string]$TtsBackend = 'Windows'
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

# The library-service config is part of the QA contract, not ambient cwd state.
$calibreConfig = Join-Path $configRoot 'calibre.toml'
Copy-Item (Join-Path $repoRoot 'conf\calibre.toml') $calibreConfig -Force

foreach ($fixture in @('representative.txt', 'representative.md', 'representative.html')) {
    Copy-Item (Join-Path $repoRoot "tests\fixtures\source-ingestion\$fixture") (Join-Path $fixtureRoot $fixture) -Force
}
& (Join-Path $repoRoot 'scripts\new-representative-epub.ps1') -OutputPath (Join-Path $fixtureRoot 'representative.epub')

$runId = Get-Date -Format 'yyyyMMdd-HHmmss'
$runLogs = Join-Path $logRoot $runId
$handoff = Join-Path $handoffRoot $runId
New-Item -ItemType Directory -Force -Path $runLogs, $handoff | Out-Null

$env:LANTERNLEAF_CONFIG_PATH = Join-Path $configRoot 'config.toml'
$env:LANTERNLEAF_QA_TTS_BACKEND = $TtsBackend.ToLowerInvariant()
$env:LANTERNLEAF_NORMALIZER_CONFIG_PATH = Join-Path $configRoot 'normalizer.toml'
$env:LANTERNLEAF_ABBREVIATIONS_CONFIG_PATH = Join-Path $configRoot 'abbreviations.toml'
$env:QUACK_CHECK_CONFIG = Join-Path $configRoot 'quack-check.toml'
$env:CALIBRE_CONFIG_PATH = $calibreConfig
$env:LANTERNLEAF_LOG_DIR = $runLogs
$env:LANTERNLEAF_CACHE_DIR = $cacheRoot
if (-not $env:RUST_LOG) {
    $env:RUST_LOG = 'info,lanternleaf=debug,lanternleaf_app=debug,lanternleaf_core=debug,lanternleaf_egui=debug,html5ever=info'
}

Write-Host ''
Write-Host 'LanternLeaf real-desktop QA'
Write-Host "Fixtures: $fixtureRoot"
Write-Host "Checklist: $(Join-Path $repoRoot 'docs\qa\non-pdf-reader-windows-checklist.md')"
Write-Host "Caliberate config: $calibreConfig"
Write-Host "Logs: $runLogs"
Write-Host "Effective QA TTS backend: $TtsBackend $(if ($TtsBackend -eq 'Windows') { '(repo-native default)' } else { '(explicit QA override)' })"
Write-Host ''
Write-Host 'Open the four files under .qa\windows\fixtures from LanternLeaf.'
Write-Host 'Windows TTS is the critical audio path; Piper remains available only by explicit -TtsBackend Piper override.'
Write-Host ''

if ($PrepareOnly) {
    Write-Host 'Repo-native QA preparation passed.'
    return
}

function Clear-StaleEspeakGeneratorCache {
    $cacheFiles = Get-ChildItem -Path (Join-Path $repoRoot 'target') -Filter 'CMakeCache.txt' -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match "[\\/]build[\\/]espeak-rs-sys-[^\\/]+[\\/]out[\\/]build[\\/]CMakeCache\.txt$" }

    foreach ($cache in $cacheFiles) {
        $generatorLine = Get-Content -LiteralPath $cache.FullName -ErrorAction SilentlyContinue |
            Where-Object { $_ -like 'CMAKE_GENERATOR:INTERNAL=*' } |
            Select-Object -First 1

        if ($generatorLine -and $generatorLine -ne 'CMAKE_GENERATOR:INTERNAL=Ninja') {
            Write-Host "Removing stale eSpeak CMake generator cache: $generatorLine"
            & cargo.exe clean -p espeak-rs-sys
            if ($LASTEXITCODE -ne 0) {
                throw 'Failed to clean stale eSpeak native build state.'
            }
            return
        }
    }
}

Push-Location $repoRoot
try {
    Clear-StaleEspeakGeneratorCache

    $buildArgs = @('build', '--profile', 'qa', '--bin', 'lanternleaf')
    $binary = Join-Path $repoRoot 'target\qa\lanternleaf.exe'
    if ($Release) {
        $buildArgs = @('build', '--release', '--bin', 'lanternleaf')
        $binary = Join-Path $repoRoot 'target\release\lanternleaf.exe'
    }

    & cargo.exe @buildArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Host ''
        Write-Host 'Initial build failed. Retrying once after targeted native-dependency cleanup...'
        & cargo.exe clean -p espeak-rs-sys
        & cargo.exe clean -p sonic-rs-sys
        & cargo.exe @buildArgs
    }
    if ($LASTEXITCODE -ne 0) {
        throw 'LanternLeaf build failed after the targeted native-dependency retry.'
    }
    if (-not (Test-Path $binary -PathType Leaf)) {
        throw "Expected LanternLeaf executable was not produced: $binary"
    }

    & $binary
    $exitCode = $LASTEXITCODE
} finally {
    Pop-Location
}

$logFiles = @(Get-ChildItem -LiteralPath $runLogs -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime)
$logFiles | Copy-Item -Destination $handoff -Force

if ($logFiles.Count -gt 0) {
    $latestLog = $logFiles[-1]
    Get-Content -LiteralPath $latestLog.FullName -Tail 250 -ErrorAction SilentlyContinue |
        Set-Content -LiteralPath (Join-Path $handoff 'latest-log-tail.txt')

    $diagnosticPattern = 'error|warn|failed|failure|timeout|timed out|calibre|caliberate|source[_ -]?open|materializ|tts|piper|voice|audio|synth'
    @(Select-String -LiteralPath $latestLog.FullName -Pattern $diagnosticPattern -CaseSensitive:$false -ErrorAction SilentlyContinue |
        Select-Object -Last 250 |
        ForEach-Object { $_.Line }) |
        Set-Content -LiteralPath (Join-Path $handoff 'latest-errors.txt')
}

@(
    "Repository: $repoRoot",
    "Fixtures: $fixtureRoot",
    "Logs: $runLogs",
    "Application exit code: $exitCode",
    "Small diagnostic files: latest-log-tail.txt, latest-errors.txt"
) | Set-Content -LiteralPath (Join-Path $handoff 'README.txt')

Write-Host ''
Write-Host "LanternLeaf exited with code $exitCode."
Write-Host "QA handoff: $handoff"
if ($exitCode -ne 0) { throw "LanternLeaf exited with code $exitCode" }

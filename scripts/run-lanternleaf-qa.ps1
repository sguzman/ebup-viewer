[CmdletBinding()]
param(
    [switch]$ValidateOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$bundleRoot = (Resolve-Path $PSScriptRoot).Path
$binary = Join-Path $bundleRoot 'lanternleaf.exe'
$conf = Join-Path $bundleRoot 'conf'
$pandocDir = Join-Path $bundleRoot 'tools\pandoc'
$sources = Join-Path $bundleRoot 'qa-sources'
$required = @(
    $binary,
    (Join-Path $conf 'config.toml'),
    (Join-Path $conf 'normalizer.toml'),
    (Join-Path $conf 'abbreviations.toml'),
    (Join-Path $conf 'quack-check.toml'),
    (Join-Path $conf 'pandoc\strip-nontext.lua'),
    (Join-Path $pandocDir 'pandoc.exe'),
    (Join-Path $sources 'representative.txt'),
    (Join-Path $sources 'representative.md'),
    (Join-Path $sources 'representative.html'),
    (Join-Path $sources 'representative.epub')
)
foreach ($path in $required) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "QA bundle is incomplete; missing $path" }
}

$runId = Get-Date -Format 'yyyyMMdd-HHmmss'
$logDir = Join-Path $bundleRoot "qa-logs\$runId"
$handoffDir = Join-Path $bundleRoot "qa-handoff\$runId"
$cacheDir = Join-Path $bundleRoot 'qa-cache'
New-Item -ItemType Directory -Force -Path $logDir, $handoffDir, $cacheDir | Out-Null
$stdout = Join-Path $handoffDir 'lanternleaf.stdout.log'
$stderr = Join-Path $handoffDir 'lanternleaf.stderr.log'
@("Bundle: $bundleRoot", "Fixtures: $sources", "Logs: $logDir", "Cache: $cacheDir") | Set-Content -LiteralPath (Join-Path $handoffDir 'README.txt')

if ($ValidateOnly) {
    Write-Output "QA bundle validation passed: $bundleRoot"
    exit 0
}

$oldPath = $env:Path
$env:LANTERNLEAF_CONFIG_PATH = Join-Path $conf 'config.toml'
$env:LANTERNLEAF_NORMALIZER_CONFIG_PATH = Join-Path $conf 'normalizer.toml'
$env:LANTERNLEAF_ABBREVIATIONS_CONFIG_PATH = Join-Path $conf 'abbreviations.toml'
$env:QUACK_CHECK_CONFIG = Join-Path $conf 'quack-check.toml'
$env:LANTERNLEAF_LOG_DIR = $logDir
$env:LANTERNLEAF_CACHE_DIR = $cacheDir
$env:Path = "$pandocDir;$oldPath"
Write-Output "Launching LanternLeaf from $bundleRoot"
Write-Output "QA fixtures are under $sources"
Write-Output "Logs will be preserved under $handoffDir"
$process = Start-Process -FilePath $binary -WorkingDirectory $bundleRoot -RedirectStandardOutput $stdout -RedirectStandardError $stderr -Wait -PassThru
Get-ChildItem -LiteralPath $logDir -File -ErrorAction SilentlyContinue | Copy-Item -Destination $handoffDir -Force
Write-Output "LanternLeaf exited with code $($process.ExitCode). QA handoff: $handoffDir"
exit $process.ExitCode

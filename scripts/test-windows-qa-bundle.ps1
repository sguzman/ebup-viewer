[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BundleDirectory
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$bundle = (Resolve-Path $BundleDirectory).Path
$required = @(
    'lanternleaf.exe', 'run-lanternleaf-qa.ps1', 'BUNDLE-MANIFEST.txt',
    'conf/config.toml', 'conf/normalizer.toml', 'conf/abbreviations.toml',
    'conf/quack-check.toml', 'conf/pandoc/strip-nontext.lua',
    'tools/pandoc/pandoc.exe', 'qa-sources/representative.txt',
    'qa-sources/representative.md', 'qa-sources/representative.html',
    'qa-sources/representative.epub'
)
foreach ($relative in $required) {
    $path = Join-Path $bundle $relative
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing bundle file: $relative" }
}
$launcher = Get-Content -LiteralPath (Join-Path $bundle 'run-lanternleaf-qa.ps1') -Raw
if ($launcher -notmatch '\$PSScriptRoot') { throw 'Launcher does not resolve paths relative to its own directory' }
if ($launcher -match '[A-Za-z]:\\') { throw 'Launcher contains a source-tree absolute path' }
$archivePath = Join-Path $bundle 'qa-sources/representative.epub'
$archive = [System.IO.Compression.ZipFile]::OpenRead($archivePath)
try {
    $names = @($archive.Entries | ForEach-Object FullName)
    foreach ($entry in @('mimetype', 'META-INF/container.xml', 'OEBPS/content.opf', 'OEBPS/chapter1.xhtml', 'OEBPS/chapter2.xhtml')) {
        if ($names -notcontains $entry) { throw "EPUB fixture is missing $entry" }
    }
} finally { $archive.Dispose() }
$launcherResult = & pwsh -NoProfile -ExecutionPolicy Bypass -File (Join-Path $bundle 'run-lanternleaf-qa.ps1') -ValidateOnly
if ($LASTEXITCODE -ne 0) { throw 'QA launcher validation mode failed' }
Write-Output ($launcherResult -join [Environment]::NewLine)
Write-Output "Windows QA bundle structure passed: $bundle"

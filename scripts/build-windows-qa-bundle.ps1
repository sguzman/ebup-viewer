[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BinaryPath,

    [Parameter(Mandatory = $true)]
    [string]$PandocPath,

    [Parameter(Mandatory = $true)]
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-RequiredFile([string]$Path, [string]$Description) {
    $resolved = Resolve-Path -LiteralPath $Path -ErrorAction SilentlyContinue
    if ($null -eq $resolved -or -not (Test-Path -LiteralPath $resolved.Path -PathType Leaf)) {
        throw "Required $Description was not found: $Path"
    }
    return $resolved.Path
}

function Write-Utf8File([string]$Path, [string]$Content) {
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function Add-ZipTextEntry([System.IO.Compression.ZipArchive]$Archive, [string]$Name, [string]$Content) {
    $entry = $Archive.CreateEntry($Name, [System.IO.Compression.CompressionLevel]::Optimal)
    $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
    $stream = $entry.Open()
    try {
        $writer = [System.IO.StreamWriter]::new($stream, [System.Text.UTF8Encoding]::new($false))
        try { $writer.Write($Content) } finally { $writer.Dispose() }
    } finally { $stream.Dispose() }
}

function Add-ZipBytesEntry([System.IO.Compression.ZipArchive]$Archive, [string]$Name, [byte[]]$Bytes, [System.IO.Compression.CompressionLevel]$Level) {
    $entry = $Archive.CreateEntry($Name, $Level)
    $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
    $stream = $entry.Open()
    try { $stream.Write($Bytes, 0, $Bytes.Length) } finally { $stream.Dispose() }
}

function New-RepresentativeEpub([string]$Path) {
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $container = '<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>'
    $opf = '<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>LanternLeaf QA Fixture</dc:title><dc:language>en</dc:language><dc:identifier id="uid">urn:lanternleaf:qa</dc:identifier></metadata><manifest><item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="chapter2.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/></spine></package>'
    $chapter1 = '<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Chapter One</h1><p>EPUB alpha appears here. The first chapter has an internal link to chapter two.</p><p>The repeated search term appears in chapter one.</p><p>Unicode café remains readable.</p></body></html>'
    $chapter2 = '<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Chapter Two</h1><p>EPUB beta appears here. EPUB alpha appears again for search navigation.</p><p>The repeated search term appears again.</p><ul><li>Native list item.</li></ul></body></html>'
    $stream = [System.IO.File]::Create($Path)
    try {
        $archive = [System.IO.Compression.ZipArchive]::new($stream, [System.IO.Compression.ZipArchiveMode]::Create, $false)
        try {
            Add-ZipBytesEntry $archive 'mimetype' ([System.Text.Encoding]::ASCII.GetBytes('application/epub+zip')) ([System.IO.Compression.CompressionLevel]::NoCompression)
            Add-ZipTextEntry $archive 'META-INF/container.xml' $container
            Add-ZipTextEntry $archive 'OEBPS/content.opf' $opf
            Add-ZipTextEntry $archive 'OEBPS/chapter1.xhtml' $chapter1
            Add-ZipTextEntry $archive 'OEBPS/chapter2.xhtml' $chapter2
        } finally { $archive.Dispose() }
    } finally { $stream.Dispose() }
}

$binary = Resolve-RequiredFile $BinaryPath 'LanternLeaf executable'
$pandoc = Resolve-RequiredFile $PandocPath 'Pandoc executable'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$output = [System.IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) { Remove-Item -LiteralPath $output -Recurse -Force }
New-Item -ItemType Directory -Force -Path $output | Out-Null

Copy-Item -LiteralPath $binary -Destination (Join-Path $output 'lanternleaf.exe')
New-Item -ItemType Directory -Force -Path (Join-Path $output 'conf'), (Join-Path $output 'conf\pandoc'), (Join-Path $output 'tools\pandoc'), (Join-Path $output 'qa-sources') | Out-Null
Get-ChildItem -LiteralPath (Join-Path $repoRoot 'conf') -Filter '*.toml' -File | Copy-Item -Destination (Join-Path $output 'conf')
Copy-Item -LiteralPath (Join-Path $repoRoot 'conf\pandoc\strip-nontext.lua') -Destination (Join-Path $output 'conf\pandoc')
Copy-Item -LiteralPath $pandoc -Destination (Join-Path $output 'tools\pandoc\pandoc.exe')

foreach ($fixture in @('representative.txt', 'representative.md', 'representative.html')) {
    Copy-Item -LiteralPath (Join-Path $repoRoot "tests\fixtures\source-ingestion\$fixture") -Destination (Join-Path $output "qa-sources\$fixture")
}
New-RepresentativeEpub (Join-Path $output 'qa-sources\representative.epub')
Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts\run-lanternleaf-qa.ps1') -Destination (Join-Path $output 'run-lanternleaf-qa.ps1')

$manifest = @"
LanternLeaf Windows QA bundle
Executable: lanternleaf.exe
Configuration: conf/*.toml and conf/pandoc/strip-nontext.lua
Pandoc runtime: tools/pandoc/pandoc.exe
QA corpus: qa-sources/representative.txt, representative.md, representative.html, representative.epub
Launcher: run-lanternleaf-qa.ps1
Logs/cache: created beneath qa-logs/ and qa-cache/ by the launcher
"@.Trim() + "`r`n"
Write-Utf8File (Join-Path $output 'BUNDLE-MANIFEST.txt') $manifest
Write-Output "QA bundle staged at $output"

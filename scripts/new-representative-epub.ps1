[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

function Add-TextEntry(
    [System.IO.Compression.ZipArchive]$Archive,
    [string]$Name,
    [string]$Content,
    [System.IO.Compression.CompressionLevel]$Level = [System.IO.Compression.CompressionLevel]::Optimal
) {
    $entry = $Archive.CreateEntry($Name, $Level)
    $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
    $stream = $entry.Open()
    try {
        $writer = [System.IO.StreamWriter]::new($stream, [System.Text.UTF8Encoding]::new($false))
        try { $writer.Write($Content) } finally { $writer.Dispose() }
    } finally {
        $stream.Dispose()
    }
}

$fullPath = [System.IO.Path]::GetFullPath($OutputPath)
$parent = Split-Path -Parent $fullPath
New-Item -ItemType Directory -Force -Path $parent | Out-Null
if (Test-Path $fullPath) { Remove-Item $fullPath -Force }

$container = '<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>'
$opf = '<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>LanternLeaf QA Fixture</dc:title><dc:language>en</dc:language><dc:identifier id="uid">urn:lanternleaf:qa</dc:identifier></metadata><manifest><item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="chapter2.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/></spine></package>'
$chapter1 = '<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Chapter One</h1><p>EPUB alpha appears here. The first chapter has an internal link to chapter two.</p><p>The repeated search term appears in chapter one.</p><p>Unicode café remains readable.</p></body></html>'
$chapter2 = '<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Chapter Two</h1><p>EPUB beta appears here. EPUB alpha appears again for search navigation.</p><p>The repeated search term appears again.</p><ul><li>Native list item.</li></ul></body></html>'

$stream = [System.IO.File]::Create($fullPath)
try {
    $archive = [System.IO.Compression.ZipArchive]::new($stream, [System.IO.Compression.ZipArchiveMode]::Create, $false)
    try {
        Add-TextEntry $archive 'mimetype' 'application/epub+zip' ([System.IO.Compression.CompressionLevel]::NoCompression)
        Add-TextEntry $archive 'META-INF/container.xml' $container
        Add-TextEntry $archive 'OEBPS/content.opf' $opf
        Add-TextEntry $archive 'OEBPS/chapter1.xhtml' $chapter1
        Add-TextEntry $archive 'OEBPS/chapter2.xhtml' $chapter2
    } finally {
        $archive.Dispose()
    }
} finally {
    $stream.Dispose()
}

Write-Host "Representative EPUB ready: $fullPath"

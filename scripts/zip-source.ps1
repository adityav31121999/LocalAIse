<#
.SYNOPSIS
    Packages the LocalAIse source code into a clean .zip archive, excluding build cache and dependency folders.

.PARAMETER Output
    Destination zip file path. Defaults to "LocalAIse-source.zip" in the repository root.

.EXAMPLE
    .\scripts\zip-source.ps1
    .\scripts\zip-source.ps1 -Output "C:\backups\my-localaise.zip"
#>
param(
    [string]$Output = "$PSScriptRoot\..\LocalAIse-source.zip"
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$repoRoot = (Resolve-Path "$PSScriptRoot\..").Path
$zipPath = [System.IO.Path]::GetFullPath($Output)

if (Test-Path $zipPath) {
    Remove-Item -Force $zipPath
}

$excludePatterns = @(
    '\node_modules',
    '\src-tauri\target',
    '\dist',
    '\.git',
    '\vendor\llama.cpp'
)

$sourceUri = New-Object System.Uri($repoRoot.TrimEnd('\') + '\')
$zip = [System.IO.Compression.ZipFile]::Open($zipPath, [System.IO.Compression.ZipArchiveMode]::Create)

$files = Get-ChildItem -Path $repoRoot -Recurse -File | Where-Object {
    $full = $_.FullName
    if ($full -eq $zipPath) {
        return $false
    }
    foreach ($ex in $excludePatterns) {
        if ($full.Contains($ex)) {
            return $false
        }
    }
    return $true
}

Write-Host "Archiving $($files.Count) source files..." -ForegroundColor Cyan

$count = 0
foreach ($file in $files) {
    $fileUri = New-Object System.Uri($file.FullName)
    $relativePath = $sourceUri.MakeRelativeUri($fileUri).ToString().Replace('\', '/')
    $entryName = "LocalAIse-main/" + [System.Uri]::UnescapeDataString($relativePath)
    
    [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
        $zip,
        $file.FullName,
        $entryName,
        [System.IO.Compression.CompressionLevel]::Optimal
    ) | Out-Null
    
    $count++
    if ($count % 50 -eq 0 -or $count -eq $files.Count) {
        Write-Host "  Processed $count / $($files.Count) files..."
    }
}

$zip.Dispose()

$zipItem = Get-Item $zipPath
Write-Host "✓ Successfully created: $zipPath ($([math]::Round($zipItem.Length / 1MB, 2)) MB)" -ForegroundColor Green

# pack-release.ps1 - package AetherRS binary-only release into dist\ (.exe, scripts, LICENSE, SHA256).
# Source files (.rs) and internal docs (docs/port/) are never included.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File ".\pack-release.ps1" -Version 0.1.0
#   powershell -NoProfile -ExecutionPolicy Bypass -File ".\pack-release.ps1" -Version 0.1.0 -Build
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+([\-+][0-9A-Za-z\.-]+)?$')]
    [string]$Version,
    [switch]$Build
)

$ErrorActionPreference = 'Stop'
$proj = Split-Path -Parent $MyInvocation.MyCommand.Path
$dist = Join-Path $proj 'dist'
$stageDir = Join-Path $dist "AetherRS-v$Version-win64"
$zipPath = "$stageDir.zip"
$sumPath = Join-Path $dist "SHA256SUMS-v$Version.txt"

if ($Build) {
    $env:Path = 'C:\msys64\ucrt64\bin;' + $env:Path
    Push-Location $proj
    try {
        cmd /c 'cargo build --release --bins -j 2'
        if ($LASTEXITCODE -ne 0) { throw 'cargo build --release failed.' }
    } finally {
        Pop-Location
    }
}

$bins = @('win12shell.exe', 'win12files.exe', 'win12boot.exe', 'win12gov.exe', 'win12guard.exe')
$relDir = Join-Path $proj 'target\release'
foreach ($b in $bins) {
    $p = Join-Path $relDir $b
    if (-not (Test-Path -LiteralPath $p)) {
        throw "Binary not found: $p. Run 'cargo build --release --bins -j 2' or pass -Build."
    }
}

if (Test-Path -LiteralPath $stageDir) { Remove-Item -LiteralPath $stageDir -Recurse -Force }
if (Test-Path -LiteralPath $zipPath)  { Remove-Item -LiteralPath $zipPath -Force }
New-Item -ItemType Directory -Force -Path $stageDir | Out-Null

# Copy only release binaries and user scripts (no source code).
foreach ($b in $bins) {
    Copy-Item -LiteralPath (Join-Path $relDir $b) -Destination (Join-Path $stageDir $b) -Force
}
foreach ($f in @('LICENSE', 'CHANGELOG.md', 'win12-files-assoc.ps1', 'uninstall-win12-rust.ps1')) {
    $src = Join-Path $proj $f
    if (Test-Path -LiteralPath $src) {
        Copy-Item -LiteralPath $src -Destination (Join-Path $stageDir $f) -Force
    }
}

Compress-Archive -Path (Join-Path $stageDir '*') -DestinationPath $zipPath -CompressionLevel Optimal -Force

# Generate SHA256 checksums for user verification.
$lines = New-Object System.Collections.Generic.List[string]
$zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$lines.Add("$zipHash  $(Split-Path $zipPath -Leaf)")
foreach ($b in $bins) {
    $h = (Get-FileHash -LiteralPath (Join-Path $stageDir $b) -Algorithm SHA256).Hash.ToLowerInvariant()
    $lines.Add("$h  $b")
}
$lines | Set-Content -LiteralPath $sumPath -Encoding ASCII

# Clean up staging folder so dist\ only contains the archive and checksums.
Remove-Item -LiteralPath $stageDir -Recurse -Force

Write-Output 'Release package ready:'
Write-Output "  Archive  : $zipPath ($([math]::Round((Get-Item $zipPath).Length / 1KB, 1)) KB)"
Write-Output "  Checksums: $sumPath"

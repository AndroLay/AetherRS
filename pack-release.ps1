# pack-release.ps1 - mengemas rilis biner publik Win12 Shell (Rust) ke folder dist\.
# Hanya menyertakan binary .exe hasil build release, skrip pengguna, README, dan LICENSE.
# Kode sumber (.rs) dan dokumen internal (docs/port/) TIDAK pernah dimasukkan ke paket rilis.
#
# Pemakaian:
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
$stageDir = Join-Path $dist "win12-shell-v$Version-win64"
$zipPath = "$stageDir.zip"
$sumPath = Join-Path $dist "SHA256SUMS-v$Version.txt"

if ($Build) {
    $env:Path = 'C:\msys64\ucrt64\bin;' + $env:Path
    Push-Location $proj
    try {
        cmd /c 'cargo build --release --bins -j 2'
        if ($LASTEXITCODE -ne 0) { throw 'cargo build --release gagal.' }
    } finally {
        Pop-Location
    }
}

$bins = @('win12shell.exe', 'win12files.exe', 'win12boot.exe', 'win12gov.exe', 'win12guard.exe')
$relDir = Join-Path $proj 'target\release'
foreach ($b in $bins) {
    $p = Join-Path $relDir $b
    if (-not (Test-Path -LiteralPath $p)) {
        throw "Binary tidak ditemukan: $p. Jalankan cargo build --release --bins -j 2 atau tambahkan -Build."
    }
}

if (Test-Path -LiteralPath $stageDir) { Remove-Item -LiteralPath $stageDir -Recurse -Force }
if (Test-Path -LiteralPath $zipPath)  { Remove-Item -LiteralPath $zipPath -Force }
New-Item -ItemType Directory -Force -Path $stageDir | Out-Null

# Salin hanya artefak biner & skrip pengguna (tanpa kode sumber).
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

# Buat berkas checksum SHA256 untuk verifikasi pengguna.
$lines = New-Object System.Collections.Generic.List[string]
$zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$lines.Add("$zipHash  $(Split-Path $zipPath -Leaf)")
foreach ($b in $bins) {
    $h = (Get-FileHash -LiteralPath (Join-Path $stageDir $b) -Algorithm SHA256).Hash.ToLowerInvariant()
    $lines.Add("$h  $b")
}
$lines | Set-Content -LiteralPath $sumPath -Encoding ASCII

# Hapus folder staging agar dist\ hanya berisi arsip rilis & checksum.
Remove-Item -LiteralPath $stageDir -Recurse -Force

Write-Output "Paket rilis siap:"
Write-Output "  Arsip    : $zipPath ($([math]::Round((Get-Item $zipPath).Length / 1KB, 1)) KB)"
Write-Output "  Checksum : $sumPath"

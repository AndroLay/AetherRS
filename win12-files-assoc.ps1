# Win12 Files: asosiasi folder ke win12files.exe (build Rust, install\win12files.exe). Hanya HKCU; tidak menyentuh cabang mesin.
# Salinan untuk repo Rust (2026-11-18) dari Pictures\win12-shell\win12-files-assoc.ps1; satu-satunya beda: lokasi exe.
# Tujuan: folder yang dibuka aplikasi lain lewat ShellExecute (mis. tautan folder, 'Buka folder') masuk ke win12files,
# bukan explorer.exe, sehingga di mode shell Explorer tidak ikut menyala. Panggilan langsung ke explorer.exe tidak tertangkap.
#   -Action Enable  : cadangkan nilai HKCU yang sudah ada (sekali saja), lalu set command Folder, Directory, dan Drive
#                     ke "<folder skrip>\win12files.exe" "%1". Tanpa nilai DelegateExecute.
#   -Action Disable : pulihkan cadangan; tanpa cadangan hanya hapus command yang sama dengan nilai Win12.
#   -Action Status  : cetak nilai HKCU dan nilai gabungan HKCR (hanya baca).
# Agen tidak menjalankan Enable; hanya pengguna yang memutuskan. uninstall-win12.ps1 memanggil Disable.
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('Enable', 'Disable', 'Status')]
    [string]$Action
)
$ErrorActionPreference = 'Stop'
$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $dir 'install\win12files.exe'
$command = '"' + $exe + '" "%1"'
$classes = 'Folder', 'Directory', 'Drive'
$backupKey = 'HKCU:\Software\Win12Shell\FilesAssocBackup'

function Get-CommandKey([string]$cls) {
    return "HKCU:\Software\Classes\$cls\shell\open\command"
}

function Remove-IfEmpty([string]$path) {
    if (-not (Test-Path -LiteralPath $path)) { return }
    $k = Get-Item -LiteralPath $path
    if ($k.ValueCount -eq 0 -and $k.SubKeyCount -eq 0) { Remove-Item -LiteralPath $path -Force }
}

function Enable-FilesAssoc {
    if (-not (Test-Path -LiteralPath $exe)) {
        throw "win12files.exe tidak ada di $dir. Pasang dulu lewat deploy-win12-rust.ps1."
    }
    if (-not (Test-Path -LiteralPath $backupKey)) {
        New-Item -Path $backupKey -Force | Out-Null
        foreach ($cls in $classes) {
            $key = Get-CommandKey $cls
            $existed = Test-Path -LiteralPath $key
            $old = ''
            if ($existed) { $old = [string](Get-Item -LiteralPath $key).GetValue('') }
            Set-ItemProperty -LiteralPath $backupKey -Name ($cls + '_Existed') -Value ([int]$existed) -Type DWord
            Set-ItemProperty -LiteralPath $backupKey -Name ($cls + '_Value') -Value $old
        }
    }
    foreach ($cls in $classes) {
        $key = Get-CommandKey $cls
        New-Item -Path $key -Force | Out-Null
        Set-Item -LiteralPath $key -Value $command
    }
    Write-Output "Asosiasi folder diaktifkan: $command"
}

function Disable-FilesAssoc {
    $hasBackup = Test-Path -LiteralPath $backupKey
    foreach ($cls in $classes) {
        $key = Get-CommandKey $cls
        if ($hasBackup) {
            $b = Get-Item -LiteralPath $backupKey
            if ([int]$b.GetValue($cls + '_Existed', 0) -eq 1) {
                New-Item -Path $key -Force | Out-Null
                Set-Item -LiteralPath $key -Value ([string]$b.GetValue($cls + '_Value', ''))
                continue
            }
        }
        if (Test-Path -LiteralPath $key) {
            $cur = [string](Get-Item -LiteralPath $key).GetValue('')
            if ($hasBackup -or $cur -eq $command) { Remove-Item -LiteralPath $key -Recurse -Force }
        }
        Remove-IfEmpty "HKCU:\Software\Classes\$cls\shell\open"
        Remove-IfEmpty "HKCU:\Software\Classes\$cls\shell"
        Remove-IfEmpty "HKCU:\Software\Classes\$cls"
    }
    if ($hasBackup) { Remove-Item -LiteralPath $backupKey -Recurse -Force }
    Remove-IfEmpty 'HKCU:\Software\Win12Shell'
    Write-Output 'Asosiasi folder dinonaktifkan (cadangan dipulihkan).'
}

function Show-FilesAssoc {
    $state = if (Test-Path -LiteralPath $exe) { 'ada' } else { 'tidak ada' }
    Write-Output "win12files.exe: $state ($dir)"
    $bk = if (Test-Path -LiteralPath $backupKey) { 'ada' } else { 'tidak ada' }
    Write-Output "Cadangan: $bk"
    foreach ($cls in $classes) {
        $key = Get-CommandKey $cls
        $user = if (Test-Path -LiteralPath $key) { [string](Get-Item -LiteralPath $key).GetValue('') } else { '(tidak ada)' }
        $merged = $null
        $hkcr = "Registry::HKEY_CLASSES_ROOT\$cls\shell\open\command"
        if (Test-Path -LiteralPath $hkcr) { $merged = [string](Get-Item -LiteralPath $hkcr).GetValue('') }
        if ($null -eq $merged) { $merged = '(tidak ada)' }
        Write-Output "$cls HKCU: $user"
        Write-Output "$cls HKCR: $merged"
    }
}

switch ($Action) {
    'Enable' { Enable-FilesAssoc }
    'Disable' { Disable-FilesAssoc }
    'Status' { Show-FilesAssoc }
}

# uninstall-win12-rust.ps1 - pemulihan & pencabutan Win12 Shell (Rust) secara deterministik dan dapat dibalik.
# Terinspirasi pola uninstall satu-klik WinRust: mengembalikan asosiasi folder, autostart, Winlogon\Shell
# (bila mengarah ke Win12), dan taskbar/Explorer bawaan. Pengaturan pengguna di install\ dipertahankan.
#
# Pemakaian (dijalankan sendiri oleh pengguna):
#   powershell -NoProfile -ExecutionPolicy Bypass -File ".\uninstall-win12-rust.ps1" -DryRun
#   powershell -NoProfile -ExecutionPolicy Bypass -File ".\uninstall-win12-rust.ps1"
param(
    [string]$InstallDir = 'C:\Users\andro\Pictures\win12-shell-rust\install',
    [switch]$DryRun,
    [switch]$KeepSettings = $true
)

$ErrorActionPreference = 'Stop'
$proj = Split-Path -Parent $MyInvocation.MyCommand.Path
$pasangPrefix = [System.IO.Path]::GetFullPath($InstallDir).TrimEnd('\') + '\'
$shellPath = Join-Path $InstallDir 'win12shell.exe'
$assocScript = Join-Path $proj 'win12-files-assoc.ps1'

function Log-Step([string]$msg) {
    if ($DryRun) { Write-Output "[DRY-RUN] $msg" } else { Write-Output $msg }
}

function Is-Admin {
    ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator
    )
}

Add-Type -Name NativeUninst -Namespace W12 -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern System.IntPtr FindWindow(string lpClassName, string lpWindowName);
[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern bool ShowWindowAsync(System.IntPtr hWnd, int nCmdShow);
'@

# 1. Hentikan win12shell.exe dari folder pasang lewat --exit (tanpa paksa).
$runningShell = @(Get-CimInstance Win32_Process -Filter "Name='win12shell.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($pasangPrefix, [System.StringComparison]::OrdinalIgnoreCase) })
if ($runningShell.Count -gt 0) {
    Log-Step "Menghentikan win12shell.exe (pid $($runningShell[0].ProcessId)) lewat --exit ..."
    if (-not $DryRun -and (Test-Path -LiteralPath $shellPath)) {
        $p = Start-Process -FilePath $shellPath -ArgumentList '--exit' -PassThru
        [void]$p.WaitForExit(15000)
    }
} else {
    Log-Step 'Tidak ada win12shell.exe aktif dari folder pasang.'
}

# 2. Pulihkan asosiasi folder Win12 Files (HKCU).
if (Test-Path -LiteralPath $assocScript) {
    Log-Step 'Menonaktifkan asosiasi folder win12files (win12-files-assoc.ps1 -Action Disable) ...'
    if (-not $DryRun) {
        & $assocScript -Action Disable
    }
}

# 3. Hapus autostart HKCU Run (hanya nilai milik Win12).
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
foreach ($name in @('Win12Shell', 'Win12Boot')) {
    $val = (Get-ItemProperty -LiteralPath $runKey -Name $name -ErrorAction SilentlyContinue).$name
    if ($val) {
        Log-Step "Menghapus autostart HKCU Run\$name ($val)"
        if (-not $DryRun) {
            Remove-ItemProperty -LiteralPath $runKey -Name $name -ErrorAction SilentlyContinue
        }
    }
}

# 4. Hapus tugas terjadwal milik Win12 (jika ada).
foreach ($taskName in @('Win12Shell', 'Andro-Penjaga-Sumber-Daya')) {
    $t = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    if ($t) {
        Log-Step "Menghapus tugas terjadwal: $taskName"
        if (-not $DryRun) {
            Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
        }
    }
}

# 5. Periksa Winlogon\Shell di HKCU dan HKLM; kembalikan ke explorer.exe bila menunjuk Win12.
$wlPaths = @(
    @{ Path = 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Winlogon'; NeedsAdmin = $false; RemoveIfWin12 = $true },
    @{ Path = 'HKLM:\Software\Microsoft\Windows NT\CurrentVersion\Winlogon'; NeedsAdmin = $true;  RemoveIfWin12 = $false }
)
foreach ($wl in $wlPaths) {
    $cur = (Get-ItemProperty -LiteralPath $wl.Path -Name Shell -ErrorAction SilentlyContinue).Shell
    if ($cur -and ($cur -match 'win12(boot|shell)\.exe')) {
        if ($wl.NeedsAdmin -and -not (Is-Admin)) {
            Log-Step "PERINGATAN: $($wl.Path)\Shell = '$cur' butuh PowerShell Administrator untuk dipulihkan ke explorer.exe."
            continue
        }
        if ($wl.RemoveIfWin12) {
            Log-Step "Menghapus override $($wl.Path)\Shell ($cur)"
            if (-not $DryRun) {
                Remove-ItemProperty -LiteralPath $wl.Path -Name Shell -ErrorAction SilentlyContinue
            }
        } else {
            Log-Step "Mengembalikan $($wl.Path)\Shell dari '$cur' ke 'explorer.exe'"
            if (-not $DryRun) {
                Set-ItemProperty -LiteralPath $wl.Path -Name Shell -Value 'explorer.exe'
            }
        }
    }
}

# 6. Pastikan taskbar & Explorer bawaan berjalan.
$tray = [W12.NativeUninst]::FindWindow('Shell_TrayWnd', $null)
if ($tray -ne [System.IntPtr]::Zero) {
    Log-Step 'Menampilkan kembali taskbar bawaan Windows (Shell_TrayWnd).'
    if (-not $DryRun) {
        [void][W12.NativeUninst]::ShowWindowAsync($tray, 4) # SW_SHOWNOACTIVATE
    }
} else {
    Log-Step 'Shell_TrayWnd tidak ditemukan; menjalankan explorer.exe ...'
    if (-not $DryRun) {
        Start-Process -FilePath 'explorer.exe'
    }
}

if ($KeepSettings) {
    Log-Step "Pengaturan pengguna di $InstallDir (settings.ini, apps.txt, widgets.ini) dipertahankan."
}
Log-Step 'Selesai. Windows kembali menggunakan Explorer standar.'

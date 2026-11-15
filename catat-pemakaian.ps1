# catat-pemakaian.ps1 - mencatat pemakaian memori dan CPU proses Win12 Shell ke docs\port\.
# Pemakaian (dijalankan sendiri oleh pengguna):
#   powershell -NoProfile -ExecutionPolicy Bypass -File ".\catat-pemakaian.ps1" -Label cpp    # baseline, saat C++ berjalan
#   powershell -NoProfile -ExecutionPolicy Bypass -File ".\catat-pemakaian.ps1" -Label rust   # setelah shell Rust berjalan
# Skrip hanya membaca: tidak menghentikan proses, tidak memasang, tidak menulis registri.
param(
    [Parameter(Mandatory)][ValidateSet('cpp', 'rust')][string]$Label,
    [int]$Detik = 30
)

$dirRust = 'C:\Users\andro\Pictures\win12-shell-rust'
$dirCpp  = 'C:\Users\andro\Pictures\win12-shell'
$laporan = Join-Path $dirRust ('docs\port\pemakaian-{0}-{1}.txt' -f $Label, (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $laporan) | Out-Null

$namaProses = 'win12shell', 'win12guard', 'win12boot', 'win12files'
$cores = [int]$env:NUMBER_OF_PROCESSORS

function Ambil-Proses {
    Get-Process -Name $namaProses -ErrorAction SilentlyContinue | ForEach-Object {
        $cim = Get-CimInstance Win32_Process -Filter "ProcessId=$($_.Id)" -ErrorAction SilentlyContinue
        [pscustomobject]@{
            Nama     = $_.ProcessName
            PID      = $_.Id
            CPUdetik = $_.TotalProcessorTime.TotalSeconds
            Handles  = $_.HandleCount
            Threads  = $_.Threads.Count
            WS_MB    = [math]::Round($_.WorkingSet64 / 1MB, 1)
            Priv_MB  = [math]::Round($_.PrivateMemorySize64 / 1MB, 1)
            Path     = $cim.ExecutablePath
            Args     = $cim.CommandLine
        }
    }
}

# Sampel dua kali dengan jeda, agar pemakaian CPU bisa dihitung.
$t0 = @(Ambil-Proses)
Start-Sleep -Seconds $Detik
$t1 = @(Ambil-Proses)

$baris = foreach ($p in $t1) {
    $awal = $t0 | Where-Object { $_.PID -eq $p.PID } | Select-Object -First 1
    $cpuPersen = 'n/a'
    if ($awal) { $cpuPersen = [math]::Round(($p.CPUdetik - $awal.CPUdetik) / $Detik / $cores * 100, 2) }
    [pscustomobject]@{
        Nama       = $p.Nama
        PID        = $p.PID
        WS_MB      = $p.WS_MB
        Priv_MB    = $p.Priv_MB
        Handles    = $p.Handles
        Threads    = $p.Threads
        CPU_persen = $cpuPersen
        Path       = $p.Path
        Args       = $p.Args
    }
}

$os = Get-CimInstance Win32_OperatingSystem
$ram = 'RAM total {0:N0} MB, tersedia {1:N0} MB' -f ($os.TotalVisibleMemorySize / 1KB), ($os.FreePhysicalMemory / 1KB)

$exe = foreach ($f in @((Join-Path $dirCpp 'win12shell.exe'), (Join-Path $dirRust 'install\win12shell.exe'))) {
    if (Test-Path -LiteralPath $f) {
        $i = Get-Item -LiteralPath $f
        '{0}  {1} B  {2:yyyy-MM-dd HH:mm}  SHA256={3}' -f $f, $i.Length, $i.LastWriteTime, (Get-FileHash -LiteralPath $f -Algorithm SHA256).Hash
    } else {
        '{0}  (tidak ada)' -f $f
    }
}

$run = (Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name Win12Shell -ErrorAction SilentlyContinue).Win12Shell
$winlogonShell = (Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Winlogon' -Name Shell -ErrorAction SilentlyContinue).Shell
$startup = @(Get-ChildItem -LiteralPath ([Environment]::GetFolderPath('Startup')) -Filter 'Win12*' -ErrorAction SilentlyContinue).Name
$tugas = @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskName -like '*Win12*' }).TaskName

$teks = New-Object System.Collections.Generic.List[string]
$teks.Add("Label=$Label  waktu=$(Get-Date -Format 's')  sampel=${Detik}s  cores=$cores")
$teks.Add($ram)
$teks.Add('--- Proses Win12 ---')
if ($baris) { $teks.Add(($baris | Format-List | Out-String).TrimEnd()) } else { $teks.Add('(tidak ada proses win12*)') }
$teks.Add('--- Berkas exe ---')
foreach ($e in $exe) { $teks.Add($e) }
$teks.Add('--- Autostart ---')
$teks.Add('Run Win12Shell   : ' + $(if ($run) { $run } else { '(tidak ada)' }))
$teks.Add('Winlogon Shell   : ' + $(if ($winlogonShell) { $winlogonShell } else { '(tidak ada)' }))
$teks.Add('Pintasan Startup : ' + $(if ($startup) { $startup -join ', ' } else { '(tidak ada)' }))
$teks.Add('Tugas terjadwal  : ' + $(if ($tugas) { $tugas -join ', ' } else { '(tidak ada)' }))

$teks | Set-Content -LiteralPath $laporan -Encoding UTF8
Write-Output "Tersimpan: $laporan"
$teks

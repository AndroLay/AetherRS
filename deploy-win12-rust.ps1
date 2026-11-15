# deploy-win12-rust.ps1 - membangun bin Rust Win12 Shell (cargo build --release) ke folder TEMP,
# menghentikan shell lama lewat --exit (tanpa paksa), memasang salinan ke folder pasang,
# lalu menjalankan shell baru dan memeriksa pil dok.
#
# PORT: skrip ini DIBUAT tetapi TIDAK DIJALANKAN oleh sesi port. Pemasangan dilakukan pengguna sendiri.
# Meniru deploy-win12.ps1 (C++) dengan perbedaan:
#  - build lewat cargo (bukan g++/gcc); keluaran ke folder TEMP terpisah (win12build-rust);
#  - tidak ada fallback Stop-Process (header skrip C++ menyatakan "tanpa Stop-Process");
#  - win12guard.exe tidak dipasang; hanya proses guard lama yang ditunggu berhenti.
#
# Cara pakai:
#   powershell -NoProfile -ExecutionPolicy Bypass -File "C:\Users\andro\Pictures\win12-shell-rust\deploy-win12-rust.ps1"
#   -SkipBuild   : pasang build yang sudah ada di folder TEMP tanpa cargo build.
#   -InstallDir  : folder tempat exe Rust dipasang. Default: win12-shell-rust\install (terpisah dari folder C++).
#                  Skrip ini hanya mengelola proses dari folder pasang itu; bila build C++ masih berjalan dari
#                  folder lain, skrip berhenti tanpa menyentuh apa pun dan meminta Anda menutupnya dulu.
# -Shell: jalankan shell baru dengan --shell (mode shell penuh) walau win12boot belum berjalan.
# Syarat: Progman (desktop Explorer) sudah tidak ada; --shell diabaikan bila masih ada.
param(
    [switch]$SkipBuild,
    [switch]$Shell,
    [string]$InstallDir = 'C:\Users\andro\Pictures\win12-shell-rust\install'
)

$ErrorActionPreference = 'Stop'
$proj      = 'C:\Users\andro\Pictures\win12-shell-rust'
$dir       = $InstallDir
$src       = Join-Path $env:TEMP 'win12build-rust'
$ts        = Get-Date -Format 'yyyyMMdd-HHmmss'
$shellPath = Join-Path $dir 'win12shell.exe'
$newShell  = Join-Path $src 'win12shell.exe'
$bootPath  = Join-Path $dir 'win12boot.exe'
$newBoot   = Join-Path $src 'win12boot.exe'
$filesPath = Join-Path $dir 'win12files.exe'
$newFiles  = Join-Path $src 'win12files.exe'
$ourKey    = 'HKCU:\Software\Win12Shell'
$stage     = 'awal'
# Mode shell penuh: win12boot.exe adalah shell login dan menjalankan "win12shell.exe --shell".
$pasangPrefix = [System.IO.Path]::GetFullPath($dir).TrimEnd('\') + '\'
$shellMode = @(Get-CimInstance Win32_Process -Filter "Name='win12boot.exe'" -ErrorAction SilentlyContinue | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($pasangPrefix, [System.StringComparison]::OrdinalIgnoreCase) }).Count -gt 0

function Clear-Updating { Remove-ItemProperty -LiteralPath $ourKey -Name Updating -ErrorAction SilentlyContinue }

Add-Type -Name Native -Namespace W12 -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)] public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);'

function Get-Win12 {
    # Hanya proses dari folder pasang Rust ($pasangPrefix); build C++ di folder lain tidak disentuh.
    @(Get-CimInstance Win32_Process -Filter "Name='win12shell.exe' OR Name='win12guard.exe'" | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($pasangPrefix, [System.StringComparison]::OrdinalIgnoreCase) })
}

function Get-Win12Asing {
    # Proses Win12 (shell, guard, boot) yang BUKAN dari folder pasang ini, termasuk yang path-nya tidak terbaca.
    @(Get-CimInstance Win32_Process -Filter "Name='win12shell.exe' OR Name='win12guard.exe' OR Name='win12boot.exe'" | Where-Object { -not ($_.ExecutablePath -and $_.ExecutablePath.StartsWith($pasangPrefix, [System.StringComparison]::OrdinalIgnoreCase)) })
}

function Wait-Until([scriptblock]$cond, [int]$maxMs) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($true) {
        if (& $cond) { return $true }
        if ($sw.ElapsedMilliseconds -ge $maxMs) { return $false }
        Start-Sleep -Milliseconds 300
    }
}

try {
    # 1. Bangun ke folder TEMP. Shell lama belum disentuh pada tahap ini.
    $stage = 'build'
    if (-not $SkipBuild) {
        'Membangun (cargo build --release -j 2) ...'
        New-Item -ItemType Directory -Force -Path $src | Out-Null
        $env:Path = 'C:\msys64\ucrt64\bin;' + $env:Path
        $cargoLog = Join-Path $env:TEMP 'w12_release_build.log'
        Push-Location $proj
        try {
            cmd /c "cargo build --release -j 2 --bins > $cargoLog 2>&1"
            if ($LASTEXITCODE -ne 0) { throw "cargo build --release gagal. Lihat $cargoLog" }
        }
        finally { Pop-Location }
        foreach ($n in @('win12shell.exe', 'win12boot.exe', 'win12files.exe')) {
            Copy-Item (Join-Path $proj "target\release\$n") (Join-Path $src $n) -Force
        }
        $warn = @(Select-String -Path $cargoLog -Pattern '^warning' -ErrorAction SilentlyContinue).Count
        "Build OK (peringatan kompiler: $warn). Log: $cargoLog"
    }

    # 2. Periksa build di TEMP ada dan tidak lebih lama daripada kode sumber.
    $stage = 'cek build'
    foreach ($p in @($newShell, $newBoot, $newFiles)) {
        if (-not (Test-Path $p)) { throw "Build tidak ditemukan: $p. Jalankan tanpa -SkipBuild." }
    }
    $tShell = (Get-Item $newShell).LastWriteTime
    $srcList = @(Get-ChildItem -Path (Join-Path $proj 'src') -Recurse -Filter '*.rs' | ForEach-Object { $_.FullName })
    $srcList += @(
        (Join-Path $proj 'build.rs'),
        (Join-Path $proj 'Cargo.toml'),
        (Join-Path $proj 'win12shell.rc'),
        (Join-Path $proj 'win12shell.manifest')
    )
    foreach ($f in $srcList) {
        if ((Get-Item $f).LastWriteTime -gt $tShell) { throw "$(Split-Path $f -Leaf) lebih baru daripada build. Jalankan tanpa -SkipBuild." }
    }
    $hashShell = (Get-FileHash $newShell -Algorithm SHA256).Hash
    $hashBoot  = (Get-FileHash $newBoot -Algorithm SHA256).Hash
    $hashFiles = (Get-FileHash $newFiles -Algorithm SHA256).Hash
    "Siap dipasang: shell $($hashShell.Substring(0,16)), boot $($hashBoot.Substring(0,16)), files $($hashFiles.Substring(0,16))"

    # 2b. Tolak bila ada shell/guard/boot dari folder lain (mis. build C++) yang masih berjalan. Dicek sebelum
    #     flag Updating ditulis, jadi berhenti di sini tidak meninggalkan state. Skrip ini tidak menutup proses itu.
    $asing = @(Get-Win12Asing)
    if ($asing.Count -gt 0) {
        $asing | ForEach-Object { "  proses dari folder lain: $($_.Name) pid=$($_.ProcessId)" }
        throw 'Ada proses Win12 dari folder lain yang masih berjalan. Tutup dulu lewat cara biasanya (skrip ini tidak memaksa), lalu jalankan ulang.'
    }

    # 2c. -Shell hanya valid bila Progman (desktop Explorer) tidak ada. Diperiksa sebelum shell lama dihentikan.
    $stage = 'cek mode shell'
    if ($Shell -and [W12.Native]::FindWindow('Progman', $null).ToInt64() -ne 0) {
        throw 'mode shell diminta, tetapi Progman (desktop Explorer) masih ada; --shell akan diabaikan. Shell lama tidak disentuh.'
    }

    # 3. Hentikan shell lama lewat --exit (tanpa paksa).
    $stage = 'hentikan shell lama'
    if ($shellMode) {
        'Mode shell penuh aktif: win12boot menunggu sampai file baru terpasang.'
        if (-not (Test-Path $ourKey)) { New-Item -Path $ourKey -Force | Out-Null }
        Set-ItemProperty -LiteralPath $ourKey -Name Updating -Value 1 -Type DWord
    }
    $old = Get-Win12 | Where-Object { $_.Name -eq 'win12shell.exe' } | Select-Object -First 1
    if ($old) {
        $oldExe = if ($old.ExecutablePath) { $old.ExecutablePath } else { $shellPath }
        "Menghentikan shell lama (pid $($old.ProcessId)) lewat --exit ..."
        $p = Start-Process -FilePath $oldExe -ArgumentList '--exit' -PassThru
        if (-not $p.WaitForExit(15000)) { throw '--exit belum selesai dalam 15 detik.' }
    }
    if (-not (Wait-Until { (Get-Win12).Count -eq 0 } 5000)) {
        Get-Win12 | ForEach-Object { "  masih jalan: $($_.Name) pid=$($_.ProcessId)" }
        throw 'shell atau pengawas lama belum berhenti (skrip ini tidak memaksa).'
    }
    'Shell dan pengawas lama sudah berhenti.'

    # 4. Cadangan versi yang terpasang sekarang (untuk kembali bila perlu).
    $stage = 'cadangan'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    if (Test-Path $shellPath) { Copy-Item $shellPath (Join-Path $dir "win12shell-before-deploy-rust-${ts}.exe") }
    "Cadangan shell dibuat dengan stempel $ts (jika versi lama ada)."

    # 5. Pasang dan verifikasi salinan.
    $stage = 'pasang'
    Copy-Item $newShell $shellPath -Force
    if ((Get-FileHash $shellPath -Algorithm SHA256).Hash -ne $hashShell) { throw 'salinan win12shell.exe tidak cocok dengan build.' }
    'win12shell.exe terpasang dan cocok dengan build.'
    # win12files.exe bisa sedang berjalan: diganti nama dulu; versi baru berlaku saat Win12 Files dibuka lagi.
    if (-not (Test-Path $filesPath) -or (Get-FileHash $filesPath -Algorithm SHA256).Hash -ne $hashFiles) {
        if (Test-Path $filesPath) {
            Move-Item $filesPath (Join-Path $dir "win12files-before-deploy-rust-${ts}.exe") -Force
        }
        Copy-Item $newFiles $filesPath -Force
        if ((Get-FileHash $filesPath -Algorithm SHA256).Hash -ne $hashFiles) { throw 'salinan win12files.exe tidak cocok dengan build.' }
        'win12files.exe terpasang.'
    }
    if (-not (Test-Path $bootPath) -or (Get-FileHash $bootPath -Algorithm SHA256).Hash -ne $hashBoot) {
        if (Test-Path $bootPath) {
            # win12boot.exe yang sedang berjalan tidak bisa ditimpa, tetapi bisa diganti nama; versi baru berlaku login berikutnya.
            Move-Item $bootPath (Join-Path $dir "win12boot-before-deploy-rust-${ts}.exe") -Force
        }
        Copy-Item $newBoot $bootPath -Force
        if ((Get-FileHash $bootPath -Algorithm SHA256).Hash -ne $hashBoot) { throw 'salinan win12boot.exe tidak cocok dengan build.' }
        if ($shellMode) { 'win12boot.exe baru terpasang; berlaku mulai login berikutnya.' } else { 'win12boot.exe terpasang.' }
    }

    # 6. Jalankan shell baru. Mode shell penuh: win12boot yang menjalankannya dengan --shell.
    $stage = 'jalankan'
    if ($shellMode) {
        Clear-Updating
    }
    else {
        $shellCmd = if ($Shell) { "`"$shellPath`" --shell" } else { $shellPath }
        $cimRes = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{CommandLine=$shellCmd} -ErrorAction SilentlyContinue
        if (-not $cimRes -or $cimRes.ReturnValue -ne 0) {
            if ($Shell) { Start-Process -FilePath $shellPath -ArgumentList '--shell' } else { Start-Process -FilePath $shellPath }
        }
    }
    if (-not (Wait-Until { @(Get-Win12 | Where-Object { $_.Name -eq 'win12shell.exe' }).Count -gt 0 } 8000)) { throw 'shell baru tidak berjalan dalam 8 detik.' }
    $pillUp = Wait-Until { [W12.Native]::FindWindow('Win12ShellPill', $null).ToInt64() -ne 0 } 8000

    # 7. Ringkasan.
    $stage = 'ringkasan'
    $sp = Get-Win12 | Where-Object { $_.Name -eq 'win12shell.exe' } | Select-Object -First 1
    "Shell berjalan: pid $($sp.ProcessId) dari $($sp.ExecutablePath)"
    if ($pillUp) { 'Pil dok terdeteksi (UI aktif).' } else { 'PERINGATAN: pil dok belum terdeteksi. Cek layar.' }
    $startup = Join-Path $dir 'win12-startup.ps1'
    if (Test-Path $startup) {
        & $startup -Action Status | Where-Object { $_ -like 'Autostart*' -or $_ -like 'PERINGATAN*' }
    }
    "SELESAI. Build terpasang: shell $($hashShell.Substring(0,16)), boot $($hashBoot.Substring(0,16))"
    'Uji di dok: geser ikon lalu lepas, klik Task View, tekan Win+D, lalu klik sebuah aplikasi.'
    exit 0
}
catch {
    "GAGAL pada tahap '$stage': $($_.Exception.Message)"
    if ($shellMode) { Clear-Updating; 'Mode shell penuh: win12boot menjalankan lagi win12shell.exe yang ada di folder pasang.' }
    if ($Shell -and -not $shellMode -and $stage -eq 'jalankan') { 'Mode shell (-Shell) gagal dijalankan. Shell belum aktif; cek boot.log.' }
    if ($stage -eq 'build' -or $stage -eq 'cek build') {
        'Shell lama tidak disentuh dan file terpasang belum diganti.'
    }
    elseif ($stage -eq 'hentikan shell lama') {
        'File terpasang belum diganti. Shell lama tidak dipaksa berhenti.'
    }
    else {
        if (Test-Path (Join-Path $dir "win12shell-before-deploy-rust-${ts}.exe")) {
            "Untuk kembali ke versi lama: salin win12shell-before-deploy-rust-${ts}.exe ke win12shell.exe, lalu jalankan win12shell.exe dari folder pasang."
        }
        'Bila shell belum berjalan: jalankan win12shell.exe dari folder pasang.'
    }
    exit 1
}

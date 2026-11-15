# Tindak lanjut 2026-11-14

Dokumen ini merangkum status setiap item tertunda. Kolom bukti memakai `file:baris` dari pembacaan kode hari ini. Tidak ada klaim paritas runtime.

Build terakhir: `cargo check --lib --bins -j 2` rc=0 dan `cargo build --release -j 2` rc=0, keduanya 0 error dan 0 warning. Waktu ubah berkas sumber terakhir (`win12boot.rs` 17:02:50, `core_c4b.rs` 17:02:57) lebih awal dari exe release (`win12shell.exe` 17:03:45, `win12boot.exe` 17:03:48). Ini bukti dari waktu ubah, bukan hash.

Keadaan terkini (read-only, 14 Nov sore):
- Explorer PID 7816 hidup dengan `Shell_TrayWnd`, tetapi `FindWindow("Progman")` kosong. Dugaan lama bahwa 7816 memegang Progman tidak didukung.
- `win12boot` PID 12772 dimulai 16:10:42 dan belum dimulai ulang, jadi tidak memuat perbaikan 17:02.
- `install\win12boot.exe` (16:38:47) dan `install\win12shell.exe` (16:38:44) berbeda hash SHA-256 dari build release. Build 17:03 belum terpasang.
- `win12shell` PID 6656 (`--shell`, induk 12772) berjalan dari `install\`. Penyebab shell mode belum terbukti belum diketahui.

## 1. Status per item

| Item | Status | Bukti |
|---|---|---|
| Panic hook | Sudah ada | `src\shell\core_c4b.rs:1248–1265` |
| Jalur app Keamanan Windows | Sudah ada | `src\shell\hotkeys.rs:273–275`, `:323`, `:423–424` |
| Alt+Tab tanpa thumbnail DWM | Keputusan permintaan 2026-11-14 (daftar vertikal). Tidak diubah. | `src\shell\overlay.rs:695`, `src\tasks_logic.rs:71` |
| `_heapmin()` tidak diport | Ditutup. `HeapCompact` sudah dipanggil; `_heapmin` hanya mengurus heap CRT, sedangkan Rust memakai heap proses. Belum diuji runtime. | `src\shell\core_c4b.rs:76–80` |
| Chevron menampilkan ikon tersembunyi | Keputusan terdokumentasi. Ikon tersembunyi tampil setelah ikon biasa. | `src\shell\core_c3.rs:995–996` |
| G_RUN digabung ke G_RIGHT | Keputusan terdokumentasi (menghindari celah antar pil). | `src\shell\core_c2.rs:555` |
| Win+V, Win+., Win+;, Win+H, Win+G | Masih toast "butuh Windows biasa". Belum ada alternatif yang terverifikasi, dan belum diuji di runtime. | `src\shell\hotkeys.rs:1204–1208` |
| Autostart menu abu-abu di mode shell | Keputusan pengguna 2026-11-14: (A) tetap abu-abu. (B) kunci Run HKCU baru dipertimbangkan setelah uji runtime stabil. Catatan risiko B: Explorer mungkin tetap berjalan saat login (Shell Winlogon tidak diganti), sehingga bisa ganda dengan shell Rust. Sumber kebenaran tetap `win12-startup.ps1`. | `src\shell\core_c3b.rs:166–168`, `:283–285`, `:354`; memori `win12-rust-port.md` |
| Ikon desktop: seret-lepas dan pemantauan folder | Tampilan dasar sudah ada: baca folder Desktop pengguna dan publik, grid, klik pilih, ganda klik buka, muat ulang lewat menu "Segarkan". Seret-lepas belum ada. Pemantauan folder sengaja tidak dibuat, karena `ReadDirectoryChangesW` pernah membocorkan IRP dan membuat freeze. Perlu desain dan uji runtime sebelum ditambahkan. | `src\shell\desk_icons.rs:5–6`, `:184`, `:288` |

### Autostart (HKCU, dicek 2026-11-14)
Status efektif memakai `StartupApproved\Run` (byte0 ganjil = nonaktif).

- Sudah nonaktif: MicrosoftEdgeAutoLaunch, GoogleDriveFS, RobloxPlayerBeta, AMDNoiseSuppression.
- Aktif: OneDrive (`/background`), Nectarlink (jangan diubah), Win12Shell (tanpa entri Run, nilai approval saja).
- Ambigu: `Microsoft.Lists` aktif, tanpa entri approval, dan nilainya menunjuk `OneDrive.Sync.Service.exe`. Belum diubah. Perlu keputusan Anda.

Tidak ada perubahan registry yang saya buat.

## 2. Perintah admin (jalankan sendiri di PowerShell elevated)

Sesi ini tidak admin, jadi perintah di bawah tidak saya jalankan.

```powershell
Set-Service -Name PhoneSvc -StartupType Disabled
Remove-AppxProvisionedPackage -Online -PackageName Microsoft.YourPhone_1.26092.119.0_neutral_~_8wekyb3d8bbwe
Remove-AppxProvisionedPackage -Online -PackageName MicrosoftWindows.CrossDevice_1.26101.77.0_neutral_~_cw5n1h2txyewy
```

- Entri `neutral_~` untuk kedua paket masih ada di `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Appx\AppxAllUserStore\Applications` (dibaca 2026-11-14).
- `Remove-AppxProvisionedPackage` tidak mudah dibatalkan. Pemasangan ulang butuh Microsoft Store.
- Layanan kandidat (`Set-Service -StartupType Disabled`): SysMain, DoSvc, CDPSvc, CDPUserSvc_dcef1, WpnService, WpnUserService_dcef1, UnistoreSvc_dcef1, UserDataSvc_dcef1, PimIndexMaintenanceSvc_dcef1, camsvc, NPSMSvc_dcef1, cbdhsvc_dcef1, UdkUserSvc_dcef1. Catat StartType asal sebelum mengubah. Dampak ke NectarLink belum diperiksa.

## 3. Skrip C++ yang masih merujuk exe lama

Folder `C:\Users\andro\Pictures\win12-shell` hanya baca, jadi file-file ini belum diubah. Pilih salah satu: ubah manual, atau salin ke Rust. Rujukan di bawah dicek ulang dengan Grep pada pola `win12shell\.(exe|cpp|rc|manifest)`.

- `build.ps1:8, 12, 18, 23`: membangun `win12shell.exe`.
- `deploy-win12.ps1:15–16, 23, 32, 54, 56–57, 85, 100, 109, 125, 154, 159, 170, 179, 181`.
- `win12.ps1:7, 15, 123`.
- `win12-startup.ps1:36, 162`.
- `uninstall-win12.ps1:22`.
- `tools\freeze-probe.ps1:1, 12, 131`.
- `tools\measure-win12.ps1:83` (komentar).
- `tests\startup_autostart_test.ps1:4, 42` (membaca `win12shell.cpp`).

Catatan: baris pada ringkasan sebelumnya (`win12.ps1:14–18, 114, 164`) tidak cocok dengan hasil pencarian hari ini, jadi yang dipakai hasil terbaru. `tests\w12diag_logic_test.cpp` hanya berisi string uji, bukan rujukan berkas.

## 4. Uji runtime yang perlu dilakukan Anda

1. Deploy build Rust (`deploy-win12-rust.ps1`). Catat RAM Explorer, SearchHost, dan msedgewebview2 sebelum dan sesudah.
2. Hentikan supervisor lama `win12boot` PID 12772 (binary lama). Perlu persetujuan.
3. Jalankan `install\win12boot.exe --switch`. Menutup Explorer PID 7816 perlu persetujuan. `--switch` keluar diam-diam bila supervisor lama masih hidup (mutex `Win12BootSingleInstance`), jadi langkah 2 dulu.
4. Periksa `install\boot.log`. Setelah `--switch` harus ada "desk:" dan "pintasan mode shell". Log "--shell diabaikan" hanya ada di build terbaru (17:03, belum terpasang) dan hanya muncul bila Progman masih ada.
5. Periksa Progman: `FindWindow("Progman")` harus kosong.
6. Jalankan checklist `docs\port\paritas-explorer.md` langkah 2–9.
7. Uji pemulihan ke Explorer (`--switch`) dan catat hasilnya.
8. Setelah uji, perbarui ledger dengan hasilnya. Ledger bersifat append-only.

## 5. Rekomendasi untuk keputusan (belum disetujui)

Ini usulan saya. Tidak ada yang dijalankan atau diubah berdasarkan bagian ini.

- Layanan kandidat: jangan dinonaktifkan dulu. Dampak ke NectarLink belum dicek. Pengecualian yang disarankan: `PhoneSvc` Disabled, karena Phone Link sudah digantikan NectarLink. Ini butuh admin dan dijalankan Anda.
- Win+V, Win+., Win+;, Win+H, Win+G: terima sebagai batasan sampai ada uji URI di runtime. Jika ingin fungsinya kembali, ujinya perlu dilakukan Anda.
- `Microsoft.Lists` (Run HKCU ke `OneDrive.Sync.Service.exe`): biarkan. Nilainya ambigu dan tidak mengganggu shell Rust.
- Folder C++ yang merujuk exe lama: biarkan. Sumber C++ hanya baca, dan jalur Rust tidak bergantung padanya.
- Deviasi di bagian 1 dan entri dua PORT baru: terima, karena tiap deviasi punya alasan dan tercatat di kode.
- Autostart: tetap abu-abu (keputusan A pengguna). Opsi B ditunda sampai uji runtime stabil.

## 6. Hasil deploy dan --switch (2026-11-14, 17:47–17:51)

Dijalankan atas izin pengguna (pesan "anda saja yang selesaikan semuanya itu").

- `deploy-win12-rust.ps1` dengan build release: build OK, 0 peringatan kompiler. Shell 6656 dihentikan lewat `--exit`. `install\win12boot.exe` (hash `B98C24A5FC2F2A21`) dan `install\win12shell.exe` (hash `E23A179DC5B8086B`) diganti. Cadangan: `win12boot-before-deploy-rust-20261114-174728.exe`.
- Shell 13768 (`--shell`, induk 12772) tidak masuk mode shell. Log 17:47:33: "--shell diabaikan: Progman (Explorer) masih berjalan" (`src\shell\core_c4b.rs:1346`). Shell itu berjalan sebagai aplikasi biasa.
- Supervisor lama 12772 (image `win12boot-before-deploy-rust-20261114-165739.exe`) dihentikan atas izin pengguna. 13768 tetap hidup.
- `install\win12boot.exe --switch` (PID 13564, mulai 17:50:00). Log:
  - 17:50:15 "beralih ke Win12 penuh: Explorer keluar"
  - 17:50:15 "mulai mode shell (tanpa aplikasi Startup: pindah di tengah sesi)"
  - 17:50:16 "desk: 13 ikon desktop dimuat"
  - 17:50:16 "pintasan mode shell: 78 terdaftar, 5 dipegang proses lain"
  - 17:50:27 "stopHosts: SearchHost.exe pid 5948 dihentikan". Ini dari `settings.ini` (stopHosts), bukan perintah manual.
- Keadaan 17:51 (read-only): `GetShellWindow` dipegang `win12shell` PID 1176, kelas jendela "Progman". Supervisor 13564 hidup. Shell 13768 dan SearchHost 5948 sudah tidak ada.
- **Explorer PID 7816 TIDAK ditutup.** Ia hanya kehilangan peran shell. Log "Explorer keluar" dibuat di `src\bin\win12boot.rs:593–594`: `gone = true` setelah `explorer_is_shell()` menjadi false, tanpa memastikan proses berakhir. Explorer 7816 masih hidup dan memakai 363 MB. Jendela `Shell_TrayWnd` tidak terlihat (rect 0,0,1,1). Menutup 7816 perlu izin, karena aturannya hanya boleh lewat `--switch`.
- Belum terjelaskan: `FindWindow("Progman")` kosong, tetapi `GetShellWindow` mengembalikan jendela berkelas "Progman". `src\shell\core_c4b.rs:1341` memakai FindWindow untuk cek awal. Ini tidak memengaruhi hasil saat ini, tetapi cek awal itu bisa salah.
- Memori (17:51): explorer 7816 363 MB, win12boot 13564 10 MB, win12shell 1176 24 MB. Sebelumnya: explorer 359,6 MB, win12boot 12772 11,7 MB, win12shell 6656 15,5 MB.

Belum dilakukan:
- Checklist `docs\port\paritas-explorer.md` langkah 2–9 (belum ada hasil dari pengguna).
- Uji pemulihan manual (`Start-Process explorer.exe`) belum diuji.
- Ledger `docs\plans\ledger.md` belum ditambah (append-only).

Keputusan terbuka:
1. Explorer 7816 ditutup? Perlu izin, dan hanya lewat jalur yang disetujui.
2. Deviasi di bagian 5 diterima atau ditolak.

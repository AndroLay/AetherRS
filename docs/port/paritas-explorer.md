# Paritas Explorer: status kode dan daftar uji manual

Tanggal: 2026-11-14. Cakupan: pintasan Win, app dan halaman pengaturan, Start, tray, dan batas dengan Explorer di build Rust (`win12shell.exe`).

**Status verifikasi:** `cargo check --lib -j 2` dan `cargo check --bins -j 2` sama-sama rc=0, 0 error, 0 warning (log `%TEMP%\w12_check_lib.log` dan `w12_check_bins.log`). Build release (`cargo build --release -j 2`, log `%TEMP%\w12_build_release.log`): rc=0, 0 error, 0 warning; `target\release\win12shell.exe` 4374158 B (13:54), SHA256 prefiks C8FC5EB3C714086A…. Salinan di `install\` pukul 13:54:51 (bukan dari saya); proses `win12shell` berjalan dari sana sejak 13:56:55 (PID 14168). Penataan ulang jendela maksimal saat area kerja berubah (`core_c3.rs` `zoom_refit_apply`) ditambahkan setelah build itu, dengan guard `is_our_window` agar jendela shell sendiri dilewati: `cargo check` lib dan bins ulang, rc=0, 0 error, 0 warning. Setelah itu ada edit `core_c3b.rs` (komentar, dua `#[allow(dead_code)]` dihapus): `cargo check` lib dan bins ulang, rc=0, 0 error, 0 warning (log `%TEMP%\w12_check_lib5.log`, `w12_check_bins5.log`). Build release dan salinan `install\` **belum** memuat perubahan ini. **Belum ada uji runtime.** Tidak ada klaim paritas runtime. Semua perilaku di bawah ini berdasarkan pembacaan kode.

## Yang sudah tersambung di kode

| Area | Lokasi kode | Keadaan |
|---|---|---|
| Win+E | `core_c3b.rs` `shell_key` (0x45) → `open_folder_window("")` → `win12files.exe` bila `FILES_OWN` | Tersambung. Jalur gagal: `explorer.exe`. |
| Win+R | `core_c3b.rs` `shell_key` (0x52) → `run_dlg_thread` (shell32 ordinal 61) | Tersambung. Komentar "TIDAK DIPANGGIL" yang basi sudah dihapus. Juga dipakai `WX_RUN` (`winx.rs:106`). |
| Win+I | `core_c3b.rs` `shell_key` (0x49) → `shell_open("ms-settings:")` | Tersambung. Mode shell: `shell_mode_open` (`hotkeys.rs:398–410`) memanggil `hk_launch_settings_uri` (ActivateForProtocol ke Pengaturan). Dialog "beralih" hanya muncul bila aktivasi gagal. Belum diuji runtime. |
| Keamanan Windows | `hotkeys.rs` `shell_mode_open`: `ms-settings:windowsdefender` → `hk_launch_packaged(HK_SECHEALTH_AUMID)` | Baru. AUMID `Microsoft.SecHealthUI_8wekyb3d8bbwe!SecHealthUI` diverifikasi dengan `Get-StartApps` di mesin ini. Bila aktivasi gagal, `hk_launch_job` memanggil `hk_post_needexp` (dialog beralih). |
| Start: Kalkulator dan Notepad | `install\apps.txt`, bagian `[start]` | Baru. Hanya dua entri yang ditambahkan. Cadangan: `install\apps.txt.bak-20261114`. |
| Chevron (tray) | `core_c3.rs` `fly_layout` | Baru: flyout menampilkan semua ikon tray. Ikon NIS_HIDDEN ditaruh setelah ikon biasa. Bar tidak menggambar ikon tray, jadi flyout adalah satu-satunya tempat ikon tray tampil. |
| Refit jendela maksimal saat ganti mode | `core_c3.rs` `shell_work_area` → `zoom_refit_enum` (EnumWindows) dan `zoom_refit_apply` (SetWindowPos) | Baru, mode shell saja. Rect jendela maksimal di monitor utama dicatat sebelum `SPI_SETWORKAREA`. Sesudahnya tepi dihitung ulang (area baru + selisih bingkai) dan dipasang sekali tanpa animasi, dengan `SWP_ASYNCWINDOWPOS`. Belum terbukti di runtime. |
| Repaint chevron dan jendela berlapis | `w12_invalidate` (`core_c1.rs:286–297`) | Dari sesi sebelumnya. Belum terbukti di runtime. |
| Toast (Win+V, Win+., Win+H, Win+G) | `hotkeys.rs` (hk_toast dengan `NT_WND`) | Tampil sebagai toast saja. |

## Ditunda (di luar cakupan Explorer)

Panel emoji (Win+.), riwayat clipboard (Win+V), dikte (Win+H), dan Game Bar (Win+G) **tetap hanya menampilkan toast**. C++ juga hanya menampilkan toast untuk pintasan ini, termasuk Win+; yang juga membuka panel emoji (`w12hotkeys.inc:619–623`, teks sama dengan `hotkeys.rs:1204–1208`), jadi ini paritas, bukan celah. Alasan:
- Masing-masing membutuhkan subsistem UI baru yang belum ada di port Rust: panel emoji, penyimpanan riwayat clipboard, host dikte, dan host Game Bar.
- Tidak bisa diuji tanpa menjalankan shell, jadi risiko regresi tinggi untuk nilai yang belum terbukti.
- Pekerjaan ini dibuat terpisah dan dibahas setelah uji runtime fase A–C.

## Belum diverifikasi

- Win+L: biasanya ditangani winlogon. Tidak didaftar di tabel pintasan.
- Win+S: pencarian Start (`hotkeys.rs:1171`, C++ `w12hotkeys.inc:586`). Paritas di kode.
- Win+Shift+S: `ms-screenclip:` (`hotkeys.rs:1170`, kait `core_c4b.rs:323`, `core_c3b.rs:798`). C++ juga memakai `ms-screenclip:` (`w12hotkeys.inc:585`, `win12shell.cpp:3726`, `win12shell.cpp:4668`). Paritas di kode. Koreksi: catatan sebelumnya yang menyebut C++ tanpa padanan itu keliru. Belum diuji runtime.
- Tempat sampah, ikon desktop, wallpaper dari Settings, multi-monitor.
  - Ikon desktop: tidak ada di C++ (pencarian `SysListView32`/`SHELLDLL_DefView` kosong; di mode shell Explorer tidak berjalan, `win12-startup.ps1:14`). Rust kini punya ikon desktop di mode shell (`src/shell/desk_icons.rs`): membaca folder Desktop pengguna dan publik, ikon dari SHGetFileInfoW, tata letak grid dihitung ulang saat WM_PAINT dan saat WM_SETTINGCHANGE SPI_SETWORKAREA, klik pilih, klik ganda buka, menu "Segarkan" (ID 4). Tanpa seret-lepas, menu per ikon, atau pemantauan folder (dihindari setelah freeze 2026-11-13). Deviasi baru yang disengaja. `cargo check` rc=0; belum diuji runtime.
- `SetForegroundWindow(fly)` pada chevron (`core_c3.rs:1113`). Kini bila gagal, dipakai cadangan `SwitchToThisWindow` seperti `ActivateWindow`. C++ 3130 tidak mengecek hasilnya. Belum diuji runtime.
- Transisi mode: pil dan bar berpindah instan (`apply_pills`, `core_c2.rs`), dan jendela maksimal ikut dipasang sekali tanpa animasi. Tween atau animasi pil belum diputuskan.
- Pemulihan ke Explorer. `win12boot --switch` pernah dijalankan (12:31:05, shell Rust PID 4260), tetapi pemulihannya belum diuji.

## Deviasi dari C++ (tercatat)

- Chevron: semua ikon tray tampil; C++ hanya menampilkan ikon tidak tersembunyi.
- Keamanan Windows: C++ hanya memakai `ms-settings:windowsdefender` (`w12start.inc:98`). Rust menambah jalur app paket di mode shell (`hotkeys.rs:399–401`, AUMID di `hotkeys.rs:348`), karena `ms-settings:windowsdefender` tidak punya padanan Win32 di mode shell (komentar `hotkeys.rs:347`).
- `_heapmin()` tidak diport. `T_TRIM` (`core_c4b.rs:72–82`) hanya memanggil `HeapCompact` (`core_c4b.rs:78`), dengan komentar PORT di `core_c4b.rs:75`. Alasannya: Rust tidak memakai heap CRT, jadi `_heapmin` tidak berpengaruh, dan menambah FFI baru belum diverifikasi lewat build release. Alasan ini berasal dari keputusan sesi sebelumnya, belum diuji.
- `autostart_set` (`core_c3b.rs:179`) kini dipanggil dari `main_menu` (CMD_AUTOSTART, C++ 3524 `AutostartSet(!autoOn)`), hanya saat bukan mode shell. Item abu-abu saat mode shell, dan klik memeriksa ulang `shell_mode_configured()` agar mode Auto tidak memilih Shell (Winlogon). Menu tidak pernah menulis Winlogon. Belum diuji runtime.
  - Padanan C++ (`win12shell.cpp:3494–3496`): di mode shell item **tidak** abu-abu, labelnya "Start with Windows (Win12 is the shell)", dan klik (`win12shell.cpp:3524`) memanggil `AutostartSet(!autoOn)`. Skrip Disable pada mode Shell memanggil `Remove-Win12ShellMode` (`win12-startup.ps1:259–263`), yang menghapus `Winlogon\Shell` (`win12-startup.ps1:154`). Jadi C++ mengizinkan klik yang menghapus nilai Winlogon.
  - Rust (`core_c3b.rs:391–393`, handler CMD_AUTOSTART): sengaja abu-abu dan tidak memanggil `autostart_set` saat mode shell, karena aturan sesi melarang penulisan Winlogon.
  - Konsekuensi meniru C++ (diverifikasi `win12-startup.ps1:259–263`): `-Action Disable` dengan Method Auto, saat `Winlogon\Shell` berisi `win12boot.exe`, memanggil `Remove-Win12ShellMode` (`win12-startup.ps1:152`, penghapusan di `:154`). Akibatnya login berikutnya kembali ke Explorer. Jadi klik "Start with Windows" di mode shell dapat mematikan shell saat login.
  - **Keputusan pengguna (2026-11-14): (A) tetap abu-abu.** Tidak ada kode autostart yang ditulis. (B) kunci Run di HKCU dipertimbangkan setelah uji runtime stabil. Risiko B: Explorer kemungkinan tetap jalan saat login, karena `Winlogon\Shell` tidak diganti, sehingga bisa dobel dengan shell Rust.
- `DeactivateNativeStartProcesses` dan `ResumeNativeStartProcesses` no-op di Rust (`core_c4b.rs:414`, `417`). Ini **paritas**, bukan deviasi: C++ juga keluar di baris pertama karena `kFreezeNativeStart = false` (`win12shell.cpp:4730`), sehingga tidak ada pembekuan native Start.
- `win12guard` belum dipasang.
- Refit jendela maksimal saat area kerja berubah: Rust menambahkannya. C++ `ShellWorkArea` (`win12shell.cpp:3290`) tidak menata ulang jendela zoomed, jadi ini perbaikan dari port, bukan paritas.
- Panic hook (`core_c4b.rs`, sesudah `SetUnhandledExceptionFilter`): panic Rust menulis baris `shell panic` ke `crash.log`, lalu memanggil `crash_recover()` yang juga dipakai `crash_filter` (pemulihan tray, native start, dan jendela Explorer). C++ hanya punya `CrashFilter` untuk exception SEH. Belum diuji runtime.

## Uji manual (dijalankan pengguna sendiri)

1. Pasang build terbaru lewat skrip deploy (build ulang, tanpa `-SkipBuild`):
   `powershell -NoProfile -ExecutionPolicy Bypass -File "C:\Users\andro\Pictures\win12-shell-rust\deploy-win12-rust.ps1"`
2. Pintasan (mode shell):
   - Win+E: `win12files` terbuka di Beranda.
   - Win+R: dialog Jalankan muncul.
   - Win+I: Pengaturan terbuka tanpa dialog. Bila aktivasi gagal, dialog "Beralih ke Windows biasa sekarang?" muncul; Ya: keluar ke Explorer.
   - Win+A: Quick Settings. Win+N: kalender dan notifikasi. Win+D: tampilkan desktop. Win+X: menu cepat.
   - Win+panah: snap jendela.
3. Start, cari "Keamanan Windows": app SecHealthUI terbuka tanpa dialog. Bila dialog beralih muncul, berarti aktivasi app paket gagal. Catat pesannya.
4. Start: Kalkulator dan Notepad ada di daftar yang disematkan.
5. Chevron (^) di bilah: flyout muncul dan berisi ikon tray. Ikon yang tersembunyi ada setelah ikon biasa. Catat ikon yang tidak tampil.
6. Alt+Tab: kartu mengalir per baris (horizontal) dengan thumbnail DWM, dan nama kartu terisi. Kode `ov_layout` (`overlay.rs`) tidak menghasilkan daftar vertikal; catatan "daftar vertikal" di `tindak-lanjut-2026-11-14.md` baris 19 belum dicocokkan dengan keputusan pengguna, perlu konfirmasi. Glow dok tidak menambah bayangan.
7. Win+V, Win+., Win+H, Win+G: hanya toast (sesuai daftar ditunda).
8. Pemakaian: `catat-pemakaian.ps1 -Label rust`. Baseline C++ ada di `docs\port\pemakaian-cpp-20261114-121209.txt` (WS 101.3 MB, private 43.4 MB, handles 880, threads 31).
9. Refit mode: buka satu jendela Explorer dan satu aplikasi, keduanya dimaksimalkan. Ganti mode lewat menu bar, Classic, Dock, lalu Segmented, lalu kembali. Tepi bawah jendela harus menempel di atas bar atau pil, tanpa celah dan tanpa tumpang tindih. Catat mode dan celah yang terlihat. Bila jendela dimaksimalkan ulang setelah pergantian mode, catat juga.

Catat hasil tiap langkah: lulus, gagal (dengan pesan), atau tidak bisa diuji.

## Pemeriksaan kode sesi ini (2026-11-14, hanya baca kode, tanpa run)

- **Re-entry WM_SETTINGCHANGE:** `shell_work_area` (`core_c3.rs:1523`) memakai `SPI_SETWORKAREA` dengan `SPIF_SENDCHANGE` (`core_c3.rs:1553–1556`), sama dengan C++ `win12shell.cpp:3295`. Pemanggil yang dicek tidak memegang borrow RefCell saat memanggil (`core_c3b.rs:86`, `357`, `570`; `core_c4b.rs:892`, `902`, `1006`, `1048`; `core_c4.rs:555`). `core_c4b.rs:825–835` melepas `APPS.borrow_mut()` sebelum `invalidate_pills`. Belum terbukti runtime.
- **usb menu:** `usb.rs:291–301` memakai `MSG.get()` untuk `SetForegroundWindow` dan `TrackPopupMenu`. C++ `w12usb.inc:157–158` juga memakai `g_msg`. Paritas.
- **Wait boot:** `win12boot.rs:687` `WaitForSingleObject(p, INFINITE)` identik dengan `win12boot.c:395`. Paritas. Di kedua sisi, hang tidak di-restart.
- **Tempat sampah:** 0 hasil di kode C++ (`win12shell.cpp`, `w12*.inc`) dan Rust (`src`). Kata itu hanya muncul di dokumen rencana. Paritas.
- **Win+L:** tidak ada di tabel pintasan C++ maupun Rust. Diteruskan ke Windows (komentar `core_c4b.rs:168`). Paritas.
- **Chevron NIS_HIDDEN:** C++ membaca `NIS_HIDDEN` (`win12shell.cpp:3015–3016`). Rust menampilkan semua ikon di flyout (`core_c3.rs` `fly_layout`). Ini deviasi yang sudah tercatat di atas.

## Batas

- Winlogon tidak ditulis. `WINLOGON_WRITE_ENABLED = false` (`src/bin/win12boot.rs:50`). Komentar `// PORT: menulis Winlogon\Shell, JANGAN dijalankan` ada di `win12boot.rs:456`, `489`, `637`. Aturan "tidak boleh dipanggil dari tes" ada di komentar `win12boot.rs:3`; belum dicek dengan pencarian pemanggil.
- Tidak ada commit, push, atau PR dari pekerjaan ini.
- Line ending dibiarkan apa adanya (CRLF di `overlay.rs`, LF di sebagian file lain). `CONVENTIONS.md` tidak mengatur ini, dan rustc menerima keduanya. Menormalisasi berarti menulis ulang file besar tanpa dampak fungsional.

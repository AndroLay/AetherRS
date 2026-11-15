# Jurnal port Rust (append-only)

Aturan: hanya tambah entri di bawah; entri lama tidak diubah. Jurnal SDD milik rencana C++ (`win12-shell\docs\plans\ledger.md`) tidak ditulis karena sumber C++ hanya dibaca.

## 2026-11-14 — Alt+Tab panah, win12files tanpa Explorer, sisa C++, deploy

- **Alt+Tab** (`src\shell\overlay.rs`, `sw_vstep`): atas/bawah tanpa baris di arah itu membungkus ke baris ujung; satu baris berfungsi sebagai kiri/kanan (berikutnya/sebelumnya); input saat panel belum tampil tidak dibuang. Layout horizontal tidak diubah. Belum diuji runtime.
- **win12files** (`src\files\frame.rs`, `fm_fallback_explorer`): bila browser gagal dan Explorer tidak berjalan, tampil MessageBox (sebelumnya gagal diam-diam). Belum diuji runtime.
- **Sisa C++**: `%TEMP%\win12build\win12shell.exe` (704.512 byte, bukan build Rust) dihapus atas permintaan. Run key HKCU `Win12Shell` tidak ada. `MpCmdRun -Restore -ListAll` tidak lagi memuat item win12. `Win12 Off.lnk` dibiarkan.
- **Build**: `cargo build --release -j 2`, rc=0, 0 error, 0 warning.
- **Deploy**: `deploy-win12-rust.ps1` (tanpa `-SkipBuild`), rc=0. Shell baru PID 17952 dari `install\`, pil dok terdeteksi. Cadangan `win12shell-before-deploy-rust-20261114-223552.exe`.
- **Explorer**: `explorer.exe` lama dihentikan. Winlogon (`HKLM\...\Winlogon\Shell` = `explorer.exe`) menjalankannya lagi otomatis (PID 11400, 22:36:01). Mematikannya permanen butuh pengubahan shell Winlogon, belum diputuskan.
- **Belum terverifikasi runtime**: navigasi panah Alt+Tab, pesan win12files, dan semua fitur dengan Explorer benar-benar tidak berjalan.

## 2026-11-14 — Tangkapan area Win+Shift+S native

- **Tangkapan area** (`src\shell\shot_area.rs`, baru): Win+Shift+S di mode shell (`core_c3b.rs`, `shell_key` 0x53) tidak lagi membuka `ms-screenclip:`. Layar virtual ditangkap sebelum overlay muncul; seret untuk memilih, lepas untuk menyimpan. Esc, klik kanan, dan klik tanpa seret membatalkan. Bingkai memakai aksen #0078D4.
- **Pemisahan publish** (`src\shell\hotkeys.rs`): clipboard, PNG, dan toast dipindah dari `hk_screenshot` ke `pub fn hk_publish_shot(px, w, h)`, dipakai PrtScn dan tangkapan area. Header BMP untuk clipboard sekarang dibangun manual.
- **Modul**: `pub mod shot_area;` ditambahkan di `src\shell\mod.rs`.
- **Build**: `cargo check --manifest-path ... -j 2 --lib --bins`, rc=0, 0 error, 0 warning. Tidak ada exe yang dijalankan dan tidak ada deploy.
- **Belum terverifikasi runtime**: overlay tampil, seleksi area, Esc (bergantung fokus jendela; hasil `SetForegroundWindow` tidak dicek), dan isi PNG serta clipboard.
- **Fn+P**: kode yang dikirim keyboard belum diketahui, jadi belum dipetakan. Bila ternyata mengirim PrtScn, jalur `HK_SHOT` yang sudah ada menanganinya.

## 2026-11-14 — Alt+PrtScn dan PrtScn polos

- **Alt+PrtScn** (`src\shell\hotkeys.rs`, `hk_screenshot_window`, id `HK_SHOT_WIN` = 19): tangkapan jendela aktif. Batas dari `DWMWA_EXTENDED_FRAME_BOUNDS` (tanpa bayangan); bila DWM gagal, pakai `GetWindowRect`. Hasilnya lewat `hk_shot_rect` dan `hk_publish_shot`, sama seperti PrtScn.
- **PrtScn polos**: sebelumnya hanya Win+PrtScn yang terdaftar. Sekarang PrtScn polos juga terdaftar ke `HK_SHOT` (layar penuh).
- **Refactor**: bagian penangkapan `hk_screenshot` dipindah ke `hk_shot_rect(x, y, w, h)`; perilaku layar penuh tidak berubah.
- **Indeks**: dua pintasan baru ditaruh di akhir `hk_build` agar id WM_HOTKEY pintasan lama tidak bergeser.
- **Build**: `cargo check -j 2 --lib --bins`, rc=0, 0 error, 0 warning (log `%TEMP%\w12_check_shotwin.log`). Tidak ada exe yang dijalankan dan tidak ada deploy.
- **Risiko**: PrtScn atau Alt+PrtScn bisa gagal didaftarkan bila shell atau fitur sistem sudah memegang tombol itu; `hotkeys_start` mencatat pintasan yang gagal. Alt+PrtScn juga bisa dikalahkan handler sistem.
- **Belum terverifikasi runtime**: pendaftaran kedua tombol, isi PNG jendela, dan batas jendela pada monitor kedua.

## 2026-11-14 — Win+V: riwayat clipboard (tahap 1 dan 2)

- **Tahap 1, pemantauan** (`src\shell\cliphist.rs`, baru): `AddClipboardFormatListener` lewat `ch_start` dan dilepas di `ch_stop`. Setiap `WM_CLIPBOARDUPDATE` (0x031D) dibaca `CF_UNICODETEXT` saja. Teks kosong atau spasi dilewati. Teks yang sama dipindah ke depan. Batas 25 entri (`CH_MAX`) dan 4096 karakter per entri (`CH_MAX_CHARS`). Riwayat hanya di memori; `ch_stop` mengosongkannya dan tidak ada yang ditulis ke disk.
- **Tahap 1, wiring**: `pub mod cliphist;` di `src\shell\mod.rs`; arm `WM_CLIPBOARDUPDATE` di `core_c4b.rs` (sekitar baris 961); `hotkeys_start` dan `hotkeys_stop` di `hotkeys.rs` memanggil `ch_start` dan `ch_stop`.
- **Tahap 2, panel** (`src\shell\cliphist_ui.rs`, baru): jendela `Win12ClipPanel` (`WS_EX_TOPMOST | WS_EX_TOOLWINDOW`, `WS_POPUP`) di pojok kanan bawah work area monitor tempat kursor berada. `clp_toggle()` membuka atau menutup panel. Saat dibuka, jendela depan disimpan sebagai tujuan tempel.
- **Tahap 2, interaksi**: panah atas/bawah dan scroll 8 baris (`CLP_VIS`); Enter atau klik kiri memilih item. Esc menutup. Kehilangan fokus (`WM_ACTIVATE` WA_INACTIVE) juga menutup. Memilih item: teks masuk ke clipboard (`CF_UNICODETEXT`), jendela tujuan diaktifkan lagi, panel ditutup, lalu Ctrl+V dikirim lewat `SendInput` setelah timer 120 ms. Input sintetis diberi `dwExtraInfo = W12_KBSIG` agar kait keyboard mengabaikannya.
- **Tahap 2, tampilan**: GDI dengan latar #202020 dan sorot #0078D4; judul "Riwayat clipboard"; bila kosong, "Belum ada riwayat. Salin teks dulu." Teks dipotong dengan elipsis dan karakter kontrol diganti spasi.
- **Pintasan**: `HK_CLIP` = 20 (`hotkeys.rs`) menggantikan entri Win+V yang sebelumnya hanya toast "butuh Windows biasa" (HK_INFO). Lengan dispatch memanggil `clp_toggle()`.
- **Build**: `cargo check --manifest-path ... -j 2 --lib --bins`, rc=0, 0 error, 0 warning (log `%TEMP%\w12_check_clp.log`). Tidak ada exe yang dijalankan dan tidak ada deploy.
- **Keterbatasan yang diketahui**: bila Win masih tertekan saat Enter, Ctrl+V bisa terbaca Win+Ctrl+V. Bila `SetForegroundWindow` ditolak, panel mungkin tidak mendapat fokus; Win+V kedua menutupnya. Win+Enter atau Win+panah saat panel terbuka bisa memicu pintasan sistem. Item tidak disimpan ke disk dan tidak bisa dihapus dari panel.
- **Belum terverifikasi runtime**: panel tampil, fokus, pilih lalu tempel, Esc, klik di luar, scroll, dan pemantauan clipboard di tahap 1.
- **Win+V (laporan pengguna)**: pengguna melaporkan "sudah berhasil". Rincian per fungsi belum dicatat.

## 2026-11-14 — Panel emoji (Win+. dan Win+;)

- **Panel** (`src\shell\emoji_ui.rs`, baru): kisi 8×6 berisi 48 emoji tetap (`EMO_LIST`). Kelas `Win12EmojiPanel` di pojok kanan bawah work area monitor kursor, seperti panel riwayat. Panah menggeser sorot. Enter atau klik sel menyalin emoji ke clipboard, lalu Ctrl+V dikirim setelah 120 ms. Esc, klik di luar, dan pintasan yang sama menutup panel.
- **Pintasan**: `HK_EMOJI` = 21 (`hotkeys.rs`) untuk Win+. dan Win+; menggantikan toast HK_INFO. Lengan dispatch memanggil `emo_toggle()`. Win+H tetap toast.
- **Pembagian kode**: `clp_set_clipboard` dan `clp_send_ctrl_v` di `cliphist_ui.rs` dibuat `pub(super)`. Perilakunya tidak berubah.
- **Modul**: `pub mod emoji_ui;` di `src\shell\mod.rs`.
- **Keterbatasan**: glyph GDI memakai Segoe UI Emoji sehingga tampil monokrom. Daftar tetap 48 emoji tanpa pencarian dan kategori.
- **Build**: `cargo check --manifest-path ... -j 2 --lib --bins`, rc=0, 0 error, 0 warning (log `%TEMP%\w12_check_emo.log`). Error awal E0308 pada argumen `CreateFontW` sudah diperbaiki (charset, precision, quality harus tipe newtype). Tidak ada exe yang dijalankan dan tidak ada deploy.
- **Belum terverifikasi runtime**: panel tampil, glyph terbaca, sorot bergerak, dan emoji tertempel ke jendela tujuan.
- **Deploy (dijalankan pengguna)**: `deploy-win12-rust.ps1` tanpa `-SkipBuild`, stempel cadangan 23:38:53. `install\win12shell.exe` 4487891 byte pukul 23:39:12; `win12files.exe` dan `win12boot.exe` juga 23:39. Cadangan `win12shell-before-deploy-rust-20261114-233853.exe` (4467585 byte). Source `emoji_ui.rs` terakhir diubah 23:34:53, sebelum build ini.
- **Proses**: `win12shell` PID 20196 mulai 23:39:18, working set 23,1 MB. Path proses tidak terbaca dari sesi ini (`ExecutablePath` kosong); asal folder disimpulkan dari urutan waktu, belum dikonfirmasi langsung.
- **Hasil uji Win+V, Win+. dan Win+; oleh pengguna**: belum dilaporkan.

## 2026-11-14 — Penutupan checklist pemasangan

- **Pemasangan**: pengguna melaporkan "sudah berhasil" pada pesan terakhir, tanpa rincian fitur mana yang diuji. Rincian per fitur (Win+V, Win+., Win+;, Alt+PrtScn, PrtScn, Win+Shift+S, Alt+Tab, flyout Wi-Fi) belum dicatat satu per satu; hanya status umum yang diterima.
- **Checklist** (`docs\port\rencana-sisa.md` bagian 8): butir "pasang (pengguna)" ditandai selesai.
- **Belum terverifikasi per fitur**: `ms-availablenetworks:` tanpa Explorer, Win+H (masih toast), Fn+P (VK belum diketahui).

## 2026-11-14 — Uji mandiri build terpasang: pintasan belum aktif (mode non-shell)

- **Kondisi saat uji**: shell PID 20196 (mulai 23:39:18) berjalan tanpa `--shell`. `deploy-win12-rust.ps1:37` menentukan `$shellMode` dari ada tidaknya `win12boot.exe` yang berjalan dari folder pasang. Saat deploy boot tidak berjalan, sehingga shell dijalankan tanpa argumen (`deploy-win12-rust.ps1:168`).
- **Akibat**: `hotkeys_start()` hanya dipanggil dari `shell_mode_start()` (`core_c4b.rs:1543`, `core_c3b.rs:551`). Mode shell aktif bila `--shell` diberikan dan Progman tidak ada (`core_c4b.rs:1347`). `boot.log` tidak memuat baris "pintasan mode shell" maupun "mulai mode shell" untuk 23:39. Win+V, Win+./;, PrtScn, Alt+PrtScn, Win+Shift+S, dan Win+H tidak didaftarkan oleh Win12; Windows yang menangani.
- **Pemeriksaan**: Progman tidak ada (`FindWindow`) dan Explorer PID 11400 hidup. Syarat mode shell sekarang terpenuhi, jadi pemasangan ulang tidak diperlukan untuk itu.
- **Hasil uji mandiri (tanpa Notepad)**:
  - `prt`: tidak ada berkas baru di `Pictures\Screenshots`; clipboard berisi gambar dari Windows.
  - `altprt`: tidak ada berkas baru.
  - `alttab`: tampil panel Alt+Tab Windows 11 (kartu Group), bukan overlay Win12 (`alttab1.png`). Fokus pindah ke jendela sebelumnya.
  - `area`: tidak ada berkas baru. Overlay Win12 tidak terbukti (`area1.png` belum diperiksa visual).
  - `ms`: `ms-settings:display`, `bluetooth`, dan `network-wifi` membuka Settings. `ms-availablenetworks:` membuka "Quick settings". Tes ini berjalan dengan Explorer aktif, jadi belum mewakili kondisi tanpa Explorer.
  - `toast` (Win+H): tidak ada toast Win12 (`winh.png`).
- **Belum diuji**: Fn+P (VK belum diketahui); Win+G (`wing.png` belum diperiksa).
- **Tindak lanjut (perlu persetujuan pengguna)**: aktifkan mode shell dengan menghentikan PID 20196 lewat `--exit` (seperti `deploy-win12-rust.ps1:123`), lalu jalankan `install\win12shell.exe --shell` atau `win12boot.exe`. Dampaknya: host tray Explorer ikut dihentikan (`stopHosts`). Belum dijalankan.
- **Efek samping uji**: clipboard berubah; Kalkulator dan Settings dibuka lalu ditutup skrip. Notepad tidak dibuka pada tes ini.

## 2026-11-15 — B1/B2 selesai kode; deploy -Shell menunggu admin

- `deploy-win12-rust.ps1`: parameter baru `-Shell`. Tanpa `-Shell` perilaku tidak berubah. Dengan `-Shell`: cek Progman sebelum shell lama dihentikan (bila Progman masih ada, berhenti tanpa menyentuh apa pun); shell baru dijalankan `"win12shell.exe" --shell`. Parse PowerShell: 0 error.
- Riwayat clipboard: tombol Del menghapus item (`clp_delete` di `cliphist_ui.rs`, `ch_remove` di `cliphist.rs`). Pilihan dijaga; panel tutup bila kosong.
- Panel emoji: kisi 8×6 menjadi 8×8 (64 emoji). Tetap GDI monokrom; pencarian dan kategori belum ada.
- Verifikasi: `cargo check -j 2 --lib --bins` rc=0, 0 error, 0 warning. Build release dari deploy (`cargo build --release -j 2`) OK, 0 peringatan.
- Deploy belum terpasang. Sesi asisten tidak elevated: `ExecutablePath` shell PID 20196 kosong, sehingga deploy menolak dengan "proses dari folder lain" sebelum menghentikan shell lama. `install\` tidak berubah. Prompt UAC untuk elevasi dibatalkan pengguna.
- Yang perlu dijalankan pengguna di PowerShell admin: `pwsh -NoProfile -ExecutionPolicy Bypass -File "$env:TEMP\w12_admin_run.ps1"` (deploy `-Shell`, PhoneSvc Disabled, hapus dua paket provisioned Phone Link). Log: `%TEMP%\w12_admin_run.log`.
- Dibersihkan (artefak uji asisten di TEMP): `w12_def_list.ps1`, `w12_def_restore.ps1`, folder `w12uji` (12 tangkapan layar).
- `Win12 Off.lnk`: target `install\win12shell.exe --exit`, valid; dibiarkan.
- Belum diuji runtime: Del dan emoji 8×8; mode shell; Win+V, emoji, PrtScn, Alt+PrtScn, Win+Shift+S, Win+H, Win+G, Fn+P.

- 2026-11-15 cek lanjutan: sesi asisten masih `ELEVATED=False`. `FindWindow('Progman')` = 0, jadi gerbang 2c deploy `-Shell` lolos; `Shell_TrayWnd` ada. Penolakan "proses dari folder lain" sebelumnya tetap berlaku di sesi non-elevated. Aksi A (deploy `-Shell`, PhoneSvc, paket Phone Link) belum dijalankan: menunggu `w12_admin_run.ps1` dijalankan di PowerShell admin pengguna. Tidak ada UAC baru dipicu.

- 2026-11-15 A8 selesai (dijalankan pengguna di PowerShell admin): dua paket provisioned Phone Link dihapus (`Microsoft.YourPhone_1.26092.119.0`, `MicrosoftWindows.CrossDevice_1.26101.77.0`). Cek ulang `Get-AppxProvisionedPackage` kosong. `PhoneSvc` Disabled (Stopped). Log skrip admin berhenti sebelum tahap paket, jadi penghapusan dilakukan manual.
- 2026-11-15 deploy `-Shell` oleh pengguna (08:14:40, `DEPLOY_RC=0`): build B1/B2 terpasang (shell `6DA5D4301520C0A2`, boot `C4BE51A3BA8C34B5`, files `3A71D7AC9C4297F1`). Shell lama pid 1064 berhenti, shell baru pid 20596 dari `install\win12shell.exe`. Argumen `--shell` tidak bisa dibaca dari sesi non-elevated. Belum diuji: mode shell (Win+.), Del riwayat clipboard, emoji 8×8, Win+V, PrtScn, Win+Shift+S, Win+H.

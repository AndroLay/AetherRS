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

## 2026-11-15 — A2 Winlogon, A3–A5, A7 (keputusan pengguna)

- A2 (disetujui pengguna; UAC dijawab Yes): HKLM `...\Winlogon\Shell` diubah dari `explorer.exe` menjadi `C:\Users\andro\Pictures\win12-shell-rust\install\win12boot.exe` lewat skrip admin sementara. Log: `MULAI 2026-11-15T08:25:42`, `LAMA=explorer.exe`, `BARU=...\win12boot.exe`, `COCOK=True`, `SELESAI 2026-11-15T08:25:42`. Pemulihan di `Documents\Win12-pemulihan\`: `pulihkan-explorer.reg` (Shell=`explorer.exe`, perlu UAC) dan `catatan.txt` (perintah darurat). HKCU Winlogon\Shell tidak ada dan tidak ditulis. `WINLOGON_WRITE_ENABLED` tetap false.
- Verifikasi baca saja (2026-11-15): HKLM Shell = `win12boot.exe`; `install\win12boot.exe` dan `install\win12shell.exe` ada; berkas pemulihan ada (08:25:25); Explorer PID 11400 (mulai 22:36:01) dan win12shell PID 20596 (mulai 08:14:42) masih berjalan.
- **Belum diuji.** Perubahan berlaku pada sign-in berikutnya; belum ada sign-out atau restart.
- Desain `win12boot.rs` (dibaca 2026-11-15): shell keluar kode 0 dan tidak ada jendela shell lain → `start_explorer("Win12 Shell ditutup pengguna")` lalu berhenti (baris 705–715). Crash lebih dari 3 kali dalam 120 detik → `start_explorer` lalu berhenti (baris 719–726). `--switch` (baris 647) memindahkan dari Explorer ke Win12.
- A3 (keputusan pengguna: pakai Win12, performa seringan mungkin): Win+V = `HK_CLIP` (20, `hotkeys.rs:1254`), panel dibuat saat dibuka. Win+. dan Win+; = `HK_EMOJI` (21, `hotkeys.rs:1255`), panel dibuat saat dibuka. Win+H dan Win+G = `HK_INFO` (`hotkeys.rs:1368–1369`) dengan toast "Dikte suara butuh Windows biasa" dan "Game Bar tidak terpasang". Keduanya hanya dibaca dari kode; belum diuji runtime. Tidak ada perubahan kode pada tahap ini.
- Pengukuran shell pid 20596 (1 sampel, jendela 5 detik): working set 20,0 MB, private 43,2 MB, CPU ±2,5%, 29 thread, 1039 handle. Tidak sebanding langsung dengan 81,7 MB dari pengukuran sebelumnya (metode dan waktu berbeda).
- A4 (skrip C++ lama): dihapus permanen dan sudah diverifikasi tidak ada: `win12-shell\build.ps1`, `deploy-win12.ps1`, `win12.ps1`, `win12-startup.ps1`, `uninstall-win12.ps1`, `tools\freeze-probe.ps1`, `tools\measure-win12.ps1`, `tests\startup_autostart_test.ps1`. Sebelum dihapus, tidak ada scheduled task, Run key, atau pintasan Start Menu yang memanggilnya. Sumber `.cpp`/`.h` dan 38 cadangan `*-before-*` tidak disentuh. Belum diputuskan: 12 skrip lain (`make-wallpaper.ps1`, `win12-files-assoc.ps1`, enam di `tools\`, dua di `mockup\tools\`, dua di `tests\`).
- A5 (pintasan Start Menu `Win12 Shell`): `Win12 On.lnk` → `install\win12boot.exe --switch`; `Win12 Off.lnk` → `install\win12shell.exe --exit`. Target dibaca 2026-11-15; tidak dijalankan. Menurut desain setelah A2, "Off" menutup shell dan `win12boot` menjalankan Explorer. Belum diuji.
- A7 (git lokal): `win12-shell-rust` sebelumnya bukan repo. Ditambahkan `.gitignore` (`/target/`, `/install/`, `*.log`, `*.tmp`), `git init`, lalu commit lokal `3d4feb0` "Port Rust Win12: shell, boot, files, deploy, dan dokumen port" (97 berkas, author dan committer `AndroLay`, tanpa baris atribusi). Tanpa remote; belum di-push. Status bersih per 2026-11-15.
- Belum dijalankan dan belum diuji: sign-out/restart (A2); Win+. dan Del; PrtScn; Alt+PrtScn; Win+Shift+S; Win+H dan Win+G; Fn+P; `ms-availablenetworks:` tanpa Explorer; On/Off setelah A2. `catat-pemakaian -Label rust` belum dijalankan.

## 2026-11-15 — Panel emoji: pencarian, kategori, dan warna (D2D)

- Fase 1 (data dan pencarian): `src\shell\emoji_data.rs` baru berisi 8 kategori (Wajah, Orang, Simbol, Kerja, Alam, Makanan, Aktivitas, Perjalanan) dan 91 emoji dengan kata kunci Indonesia dan Inggris (64 emoji lama termasuk). `src\shell\emoji_ui.rs`: mengetik langsung mencari lintas kategori (substring pada kata kunci dan karakter emoji, maksimum 64 hasil, kueri maksimum 32 karakter). Tab kategori bisa diklik atau dipindah dengan Tab. Backspace menghapus huruf kueri.
- Perubahan perilaku: Esc sekarang menghapus kueri bila ada; bila kueri kosong, Esc menutup panel. Sebelumnya Esc langsung menutup panel.
- Fase 2 (warna): `emo_draw_color` menggambar glyph ikon tab dan sel lewat render target DC Direct2D perangkat lunak (`D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT`, "Segoe UI Emoji", 24 px, pola mengikuti `create_target` di `core_c1.rs`). Bila target, kuas, format, atau EndDraw gagal, glyph digambar ulang dengan GDI (monokrom).
- Verifikasi: `cargo check --manifest-path ... -j 2 --lib --bins` (log `%TEMP%\w12_check_emo3.log`): RC=0, 0 error, 0 warning, `Checking w12` dan `Finished` tercatat.
- Belum diuji runtime: apakah glyph muncul berwarna pada render target perangkat lunak (perlu cek visual), pencarian, Tab, Backspace, dan Esc. Cara uji: Win+. lalu ketik "kopi", pindah kategori dengan Tab, dan tekan Esc dua kali.
- Belum dipasang: build ini belum masuk `install\`.

## 2026-11-18 — Rencana kerja, mode game, Tempat Sampah, pengalihan Explorer di mode shell

- Dokumen baru: `docs\port\capaian-pengembangan.md`, `rencana-kerja.md`, `hasil-uji.md`, `pemulihan.md`. Skrip baru `win12-files-assoc.ps1` (akar repo Rust, exe `install\win12files.exe`; salinan skrip folder lama, folder lama tidak diubah).
- Kode: mode game di `kb_hook_proc` (`GAME_FS`, `state.rs`), opsi `[shell] gamehook` (`GAME_HOOK`), KEBAL governor ditambah game/launcher/anti-cheat, ikon dan menu Tempat Sampah (`desk_icons.rs`, `core_c3b.rs` `desk_menu` ID 5), `shell_mode_open` mengalihkan `control`/`explorer` ke win12files, Win+X File Explorer lewat `open_folder_window`.
- Keadaan diperiksa: `HKLM Winlogon\Shell` = `explorer.exe`; Run `Win12Shell` = `install\win12shell.exe`; shell PID 13472. Catatan memori yang menyebut `win12boot.exe` sebagai Winlogon Shell tidak sesuai keadaan sekarang.
- Koreksi dokumen: Win+V dan Win+. sudah punya panel (`hotkeys.rs:1566–1571`); hanya Win+H dan Win+G yang toast.
- Verifikasi: `cargo check --lib --bins -j 2` (log `%TEMP%\w12_check_r7.log`): rc=0, 0 error, 0 warning.
- Belum: build release, deploy, uji runtime (N1–N10 di `hasil-uji.md`).

## 2026-11-18 17:24–17:28 — Mode shell dijalankan atas permintaan pengguna (hemat resource)

- `install\win12boot.exe --switch` (17:24:27). Log: "beralih ke Win12 penuh: Explorer keluar", stopHosts menghentikan SearchHost (pid 9396) beserta 6 msedgewebview2 anaknya; StartMenuExperienceHost ikut berhenti.
- Explorer lama (pid 7764) melepas desktop tetapi prosesnya tidak keluar dalam 30 dtk (sisa jendela tersembunyi, 228 MB). Dihentikan paksa atas permintaan pengguna. Winlogon (AutoRestartShell=1) menyalakan explorer baru (pid 10860) yang TIDAK mengambil alih shell (pemilik GetShellWindow tetap win12shell pid 4564), hanya membuka satu jendela CabinetWClass. Jendela itu ditutup dengan WM_CLOSE; proses keluar sendiri dan tidak muncul lagi.
- Memori private kelompok proses shell: sebelum 888 MB (explorer 265, webview2 194, TextInputHost 110, StartMenuExperienceHost 91, SearchHost 55, win12shell 42, ShellExperienceHost 40, lain-lain 91) -> sesudah 120 MB (win12shell 43, ApplicationFrameHost 28, CrossDeviceResume 28, lain-lain 21). RAM sistem terpakai 8,2 dari 13,7 GB.
- Ikut hilang: ShellExperienceHost (banner/pusat notifikasi Windows kemungkinan tidak tampil; `notchnotif=0`) dan TextInputHost (keluar sendiri, tidak disentuh).
- Sementara: setelah restart/sign-in Explorer kembali (Winlogon\Shell = explorer.exe). Celah K1 (AppBar) dan K2 (posisi ikon tray) belum dibuat.

## 2026-11-18 malam — K1/K2, pembersihan Explorer, mode shell otomatis, notifikasi notch

- K1 (`core_c3.rs` `compat_appbar`): tanpa Explorer, Shell_TrayWnd Win12 menjawab SHAppBarMessage (WM_COPYDATA dwData 0, format APPBARMSGDATA 32/64 tetap): ABM_GETSTATE=ABS_ALWAYSONTOP, ABM_GETTASKBARPOS menulis rect pita di bawah area kerja + ABE_BOTTOM lewat SHLockShared, ABM_NEW/REMOVE/QUERYPOS/SETPOS/ACTIVATE/WINDOWPOSCHANGED/SETSTATE diterima, autohide ditolak. Dengan Explorer hidup tetap diteruskan ke Explorer.
- K2 (`compat_icon_rect`): Shell_NotifyIconGetRect (dwData 3, magic 0x34753423): rect sel flyout bila terbuka, selain itu rect tombol chevron. hWnd 32-bit (40 B) atau 64-bit (48 B) dibedakan dari cbData. Pesan pertama tiap jenis dicatat sekali ke boot.log.
- `win12boot.rs`: `explorer_cleanup` (thread setelah `--switch`): Explorer lama yang tidak keluar 20 dtk dan bukan shell dihentikan; jendela CabinetWClass dari Explorer pengganti (Winlogon) ditutup selama 40 dtk. Opsi `--login`: tunggu 20 dtk sebelum beralih.
- Run HKCU `Win12Shell` = `"...\install\win12boot.exe" --switch --login` (cadangan nilai lama di `HKCU\Software\Win12Shell\RunBackup`). Atas permintaan pengguna.
- `install\settings.ini`: `notchnotif=1` (notchonly tetap 0).
- Verifikasi: `cargo check --lib --bins -j 2` rc=0, 0 error, 0 warning (`%TEMP%\w12_check_k12.log`). Belum build/deploy/uji runtime. win12boot baru berlaku mulai sign-in berikutnya.

## 2026-11-18 17:38–17:45 — Deploy di mode shell jatuh ke Explorer; peralihan ulang berhasil

- Deploy (shell FBDF6B27830B41C3, boot B72E6777F4D895D9): shell lama keluar 17:38:17, exe baru tersalin 17:38:52, lalu win12boot LAMA (pid 10608, build 17:18) menyalakan Explorer cadangan 17:38:57 "Win12 Shell ditutup pengguna". Skrip: "shell baru tidak berjalan dalam 8 detik". Tidak ada log dari shell baru; penyebab pasti belum terbukti (dugaan: shell baru keluar kode 0 segera setelah dijalankan).
- Shell baru dijalankan mode biasa (pid 2892): bertahan, `AppBar: ABM_NEW ok=true`, notif pendengar siap (notchnotif=1 aktif).
- `win12boot --switch` (versi baru) 17:41:37: Explorer keluar sendiri ("pembersihan: Explorer lama keluar sendiri"), shell `--shell` pid 12780, SearchHost dihentikan stopHosts. Total kelompok proses shell 114 MB.
- Perbaikan `win12boot.rs`: shell yang keluar kode 0 kurang dari 20 dtk setelah dijalankan dianggap gagal start (dijalankan ulang, batas 3 kali per 2 menit), bukan diganti Explorer. Lama hidup dan kode keluar dicatat. cargo check lib+bins rc=0, 0 error, 0 warning. Belum dibangun/dipasang; win12boot yang berjalan (pid 6600) masih versi sebelumnya.

## 2026-11-18 17:47 — Kamera di mode shell memicu beralih ke Windows biasa

- Uji pengguna: Minecraft Bedrock di mode shell LULUS. Membuka Kamera (`Microsoft.WindowsCamera_8wekyb3d8bbwe!App`, UWP murni, EntryPoint `Microsoft.CameraApp.Windows.App`): aktivasi gagal -> dialog butuh Explorer -> "beralih ke Windows biasa (permintaan pengguna)" 17:47:29 -> Explorer -> Kamera dibuka (hasil 42).
- Perbaikan `hotkeys.rs`: `hk_activate_mgr` mencoba ApplicationActivationManager dengan CLSCTX_INPROC_SERVER lalu CLSCTX_LOCAL_SERVER (contoh resmi memakai LOCAL_SERVER); HRESULT setiap kegagalan dicatat ("aktivasi inproc/local gagal ..."). DesktopAppxActivator yang gagal kini lanjut ke manager. cargo check lib+bins rc=0, 0 error, 0 warning. Belum deploy; penyebab pasti menunggu log HRESULT.

## 2026-11-18 17:56–18:05 — Temuan: aplikasi UWP CoreWindow tidak bisa tampil tanpa Explorer sebagai shell

- Build 796474AD8B296F43 (deploy mode biasa sukses), lalu `--switch` (Explorer keluar sendiri).
- Kamera dari Start: `aktivasi inproc gagal ... ActivateApplication 0x80040900` (17:56:34) dan `local` gagal sama (17:57:19), masing-masing setelah ~45 dtk.
- Uji langsung IApplicationActivationManager (LOCAL_SERVER): Kamera, Kalkulator (`Microsoft.WindowsCalculator_8wekyb3d8bbwe!App`), dan Pengaturan (`windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel`) semuanya 0x80040900 setelah 45 dtk. Proses app sempat dibuat (pid dikembalikan) lalu hilang.
- Explorer non-shell (dijalankan saat Win12 memegang GetShellWindow) tidak membantu (Kamera tetap 0x80040900). Explorer uji itu dihentikan lagi.
- Kesimpulan kerja: app UWP CoreWindow membutuhkan layanan immersive shell yang hanya aktif saat Explorer menjadi shell. App full trust (Minecraft Bedrock lulus) tidak terdampak. Menunggu keputusan pengguna soal mode.

## 2026-11-18 18:10 — Explorer sementara untuk app UWP (keputusan pengguna)

- Keputusan pengguna: Explorer dinyalakan hanya sementara saat app UWP dibuka, lalu mati otomatis setelah tidak ada app yang memakainya.
- `hotkeys.rs`: `hk_is_uwp` (manifest: app ditemukan dan bukan full trust; `hk_manifest_entry`, `hk_package_dir`). Di mode shell, job app UWP atau protokol Pengaturan langsung memposting `WM_W12_NEEDEXP` wParam 1 -> `hk_temp_explorer`: tulis `HKCU\Software\Win12Shell\TempExplorer=1`, lalu `shell_switch_to_windows(target)` tanpa dialog dan tanpa menunggu aktivasi 45 dtk.
- `win12boot.rs`: shell keluar kode 0 + `TempExplorer` -> `temp_explorer_session`: Explorer dijalankan sebagai shell, target dibuka (`open_after_switch`), Win12 biasa dijalankan di atasnya, lalu pemantauan tiap 2 dtk jendela `ApplicationFrameWindow` (terlihat, tidak di-cloak). Tidak ada selama 20 dtk (atau tidak pernah muncul dalam 60 dtk) -> `switch_from_explorer` + `explorer_cleanup` -> loop menjalankan `win12shell --shell`. `TempExplorerKeep=1` membatalkan (tetap di Windows biasa).
- cargo check lib+bins rc=0, 0 error, 0 warning. Belum deploy/uji. Perlu win12boot baru yang BERJALAN (deploy di mode biasa lalu --switch).

## 2026-11-18 malam — win12files dimiripkan dengan File Explorer Windows 11 (keputusan pengguna)

- Pengguna menilai tampilan win12files "aneh", lalu memilih "Tetap win12files, dimiripkan" (bukan Explorer asli sementara).
- Modul baru `src\files\chrome.rs`: tab di bilah judul (WM_NCCALCSIZE + DwmExtendFrameIntoClientArea + DwmDefWindowProc; area kosong baris tab = HTCAPTION, tombol jendela milik DWM, warna lewat DWMWA_CAPTION_COLOR, tanpa Mica), tab bersudut bulat dengan ikon folder sistem dan tombol tutup, baris alamat breadcrumb (klik segmen = naik ke folder itu, klik area kosong/Ctrl+L = mode ketik), tombol navigasi ikon Segoe Fluent, kotak pencarian bulat, baris perintah ikon (Baru | Potong Salin Tempel Ganti nama Hapus | Urutkan Tampilan | Lainnya: Pilih semua, Properti), bilah status bawah (jumlah item, item dipilih, tombol Detail/Ikon besar).
- EDIT tanpa WS_EX_CLIENTEDGE, font Segoe UI 9 pt (dulu DEFAULT_GUI_FONT); ukuran awal jendela diskalakan DPI. Tinggi bilah: tab 40, alamat 48, perintah 48, status 26 DIP.
- Fungsi gambar/uji titik lama di `bar.rs` dihapus; isi folder tetap IExplorerBrowser asli.
- cargo check lib+bins rc=0, 0 error, 0 warning. Belum deploy, belum dilihat hasilnya.

## 2026-11-18 18:37 — bilah alamat win12files menjalankan perintah; deploy oleh agen atas izin pengguna

- Laporan pengguna: mengetik `alacritty`, `cmd`, dll. di bilah alamat tidak berbuat apa-apa.
- `app.rs` `fm_address_run`: bila teks bukan lokasi, ShellExecuteExW (file = token pertama atau teks berkutip, sisanya parameter, direktori kerja = folder aktif bila folder sistem berkas). Dipanggil dari Enter di kotak alamat (`frame.rs`) setelah `fm_navigate_to` gagal.
- Deploy dijalankan agen atas izin pengguna ("silahkan anda yang deploy saja"): files 46D90F38 (18:31) lalu 465E262F (18:36). Dari Git Bash gagal (Get-FileHash tak dikenal), lewat PowerShell 7 berhasil. Dua kali deploy di mode shell jatuh ke Explorer ("Win12 Shell ditutup pengguna"); dipulihkan dengan `win12boot --switch` (boot 12652, shell 7832). Celah: win12boot belum mengenali --exit dari deploy sebagai jeda pasang.

## 2026-11-18 18:42 — tombol Win tidak membuka Start (mode shell)

- Laporan pengguna: tombol Win tidak memunculkan Start. Log tanpa galat; shell responsif. Dugaan: kait WH_KEYBOARD_LL dilepas diam-diam oleh Windows setelah melewati LowLevelHooksTimeout (tidak ada notifikasi ke aplikasi).
- `core_c4b.rs` kb_thread: SetTimer thread 20 dtk; tiap WM_TIMER (bila Win tidak ditahan) kait dilepas lalu dipasang ulang; gagal dicatat "kait keyboard: pasang ulang gagal".
- Deploy oleh agen (shell D32B9986, boot 8C72D2B9, files EBE5C77C), jatuh ke Explorer lagi lalu `--switch` (boot 10288, shell 8056, 18:42). Belum dipastikan pengguna.

## 2026-11-18 18:52 — Win di jendela admin, ikon Explorer, optimasi RAM (disetujui pengguna)

- Tombol Win saat Task Manager (admin) aktif: kait LL tidak dipanggil (UIPI); Windows memposting WM_SYSCOMMAND SC_TASKLIST ke jendela shell (GetShellWindow = jendela desktop), bukan ke jendela pesan. `core_c3b.rs` desk_proc kini menangani SC_TASKLIST di mode shell -> `hk_task_list` (penangan lama di jendela pesan tetap).
- Ikon terpisah: pin dok `c:\windows\explorer.exe` (jalur) tidak dialihkan seperti pin AUMID. `core_c1.rs` resolve_entry: pin jalur explorer.exe -> aumid Microsoft.Windows.Explorer, launch/exe win12files. `files\frame.rs`: SetCurrentProcessExplicitAppUserModelID("Microsoft.Windows.Explorer") agar taskbar Windows juga mengelompokkan.
- win12files: working set dilepas saat diminimalkan dan setelah 30 dtk tidak aktif (WM_ACTIVATEAPP + timer 7).
- Pengaturan Windows (izin pengguna): HKCU CrossDeviceResume\Configuration IsResumeAllowed 1 -> 0, proses CrossDeviceResume dihentikan. AppActions dan SDXHelper (keduanya server COM `-Embedding`, bisa dinyalakan lagi oleh klien) dan SpotifyLauncher (StartupTask State sudah 1) dihentikan; tidak ada pengaturan permanen yang diubah untuk ketiganya.
- Deploy oleh agen (shell 0ECA52E7, boot 91AB938C, files B52BE665), jatuh ke Explorer lalu `--switch` (boot 14272, shell 8472, 18:52).

## 2026-11-18 18:56 — proses latar dibuat tetap mati; Gemini startup dimatikan (permintaan pengguna)

- `stophosts.rs`: daftar yang diizinkan 4 -> 8 nama (+ CrossDeviceResume.exe, AppActions.exe, SDXHelper.exe, SpotifyLauncher.exe; indeks lama tetap). `stophosts_ui.rs`: SDXHelper dilewati selama aplikasi Office berjalan. `install\settings.ini` stopHosts += 4 nama tersebut. Log 18:56:09 "stopHosts aktif (mode shell): 6 nama". Hanya aktif di mode shell; backoff lama berlaku (3 kali/10 menit -> berhenti sampai login berikutnya).
- HKCU StartupApproved\Run `Gemini` = 03 + FILETIME (nonaktif seperti Task Manager). Spotify prefs (paket Store, LocalState) + `app.autostart-mode="off"`.
- Deploy oleh agen (shell 93F38404, boot 9F9C45D9, files A38233E4), `--switch` (boot 1696, shell 448).

## 2026-11-19 12:28 — win12files: tombol samping mouse, panel terminal F4, kemiripan Explorer (permintaan pengguna)

- Tombol samping mouse: XBUTTON1/2 (klien dan non-klien) ditangani di pompa pesan `frame.rs::fm_handle_xbutton` (Kembali/Maju, di mana pun dalam jendela); WM_APPCOMMAND (Kembali, Maju, Segarkan, Cari, Beranda) di `wndproc.rs`.
- Panel terminal F4 seperti Dolphin (bukan fitur Explorer): `src\files\term.rs` baru. ConPTY + emulator VT sendiri (CSI/SGR 16/256/RGB, layar alternatif, scrollback 3000, pilih/salin/tempel, klik kanan salin/tempel, Shift+PgUp/PgDn). Shell: `[files] terminal=` di settings.ini, atau pwsh, atau powershell. Folder ikut folder aktif bila shell menganggur (tidak punya proses anak). F4 sembunyikan/tampilkan (sesi tetap hidup), Ctrl+` pindah fokus, `exit` menutup panel (thread penunggu proses, karena ConPTY tidak memberi EOF). Pembatas bisa diseret (klik ganda = tinggi bawaan 220 DIP), tombol terminal di bilah status. F4 sebelumnya = fokus alamat; alamat tetap lewat Alt+D / Ctrl+L.
- `src\files\extras.rs` baru: menu "Baru" asli (CLSID_NewMenu + IContextMenu3 untuk ikon/isi submenu, situs = tampilan agar langsung ganti nama), dropdown subfolder pada chevron breadcrumb (+ chevron penutup), panel Pratinjau (Alt+P) dan Detail (Alt+Shift+P) lewat IExplorerPaneVisibility (status di HKCU\Software\Win12Shell\Files), menu "⋯" seperti Explorer (Sematkan ke Akses cepat, pilih semua/kosongkan/balikkan, Salin jalur Ctrl+Shift+C, Buka di Terminal, Panel terminal, panel, Properti, Opsi).
- Menu Tampilan seperti Explorer Windows 11: 8 mode (ikon sangat besar..konten), Tampilan ringkas, submenu Tampilkan (panel detail/pratinjau, kotak centang item, ekstensi nama file, item tersembunyi; lewat SHGetSetSettings seperti Explorer).
- cargo check lib+bins: 0 error, 0 warning. Deploy oleh agen (shell F6FFF722, boot BF1EC952, files CCA3C608), jatuh ke Explorer lalu `--switch` (boot 6444, shell 11756, 12:28; Explorer 12292 dihentikan pembersihan 12:28:39).
- Belum diuji pengguna. Proses win12files lama (pid 9380, sejak 11:55) masih berjalan dengan kode lama sampai semua jendelanya ditutup.

## 2026-11-19 13:03 — snap kuadran + grup Alt+Tab, ikon dok win12files, notch pintar layar penuh, CPU idle (permintaan pengguna)

- Snap 4 kuadran (`tasks_logic.rs` W12SN_TL/BL/TR/BR = 5..8): Win+← lalu Win+↑/↓ = kiri atas/bawah, sama untuk kanan; dari kuadran, Win+←/→ pindah sisi, Win+↑/↓ kembali ke separuh/maks/min. Tata letak snap juga memetakan kuadran. Tes `tests\tasks_logic.rs` disesuaikan (tidak dijalankan).
- Alt+Tab (`overlay.rs`): jendela snap yang mengisi layar bersama (2-4, satu monitor, posisi masih cocok) jadi satu kartu grup dengan thumbnail per anggota; memilih kartu menaikkan semua anggota. Tombol X (tutup) di ujung kanan pita judul; kartu grup punya X per anggota.
- Ikon dok File Explorer terpisah tanpa ikon: penyebabnya proses win12files lama yang exe-nya sudah diganti nama oleh deploy (`win12files-before-deploy-*.exe`). `core_c1.rs` resolve_window memetakan nama itu kembali ke win12files.exe. F4 lama (fokus alamat) juga dari proses lama itu (pid 9380, dua jendela); pengguna perlu menutup jendelanya.
- Notch layar penuh: dulu notch selalu disembunyikan saat layar penuh (`notch_sync` want = !FULLSCREEN), padahal banner Windows dimatikan (notchonly), jadi notifikasi hilang. Baru: `[shell] notchfsvideo` / `notchfsgame` (0 sembunyi, 1 notifikasi + status sistem, 2 + media dan zona tepi atas; bawaan 1). Jenis dari exe jendela depan (`core_c3b.rs` fs_kind: peramban/pemutar = video, `[shell] notchvideoapps` menambah; lainnya game; di-cache per jendela). Garis notchalways tidak tampil di atas video/game; media hanya mengintip, tidak menetap.
- Pengaturan notch dirombak bergaya Pengaturan Windows 11: navigasi Tampilan / Media / Notifikasi / Layar penuh, sakelar dan kontrol segmen. Baru di Pengaturan: Status sistem (notchhud), Notifikasi lewat notch (notchnotif), Sembunyikan banner Windows (notchonly), layar penuh video/game. `notif_worker::apply_settings` menerapkan tanpa restart; banner Windows dipulihkan bila notifikasi notch atau notchonly dimatikan.
- CPU idle: shell memakai ~1% satu inti saat diam. Penyebab: pekerja notifikasi tanpa event (proses tanpa paket) memanggil GetNotificationsAsync tiap 2 dtk. Kini menunggu perubahan folder `%LOCALAPPDATA%\Microsoft\Windows\Notifications` (FindFirstChangeNotification; uji: toast menulis wpndatabase.db-wal seketika), jaring pengaman 15 dtk. Pekerja media: event garis waktu dibatasi sekali per 4 dtk saat notch tersembunyi (UI mengekstrapolasi posisi).
- cargo check lib+bins: 0 error, 0 warning. Deploy oleh agen (12:55 lalu 13:02), keduanya jatuh ke Explorer lalu `--switch` (boot 9440, shell 5308, 13:03).
- 13:09 tambahan: database notifikasi sempat ditulis aplikasi lain ~72 kali/dtk (13:07, bukan dipicu shell: tulisan tiap 100-200 ms, tanpa notifikasi baru di log). Pemantau folder diberi jeda adaptif antar-pembacaan (1 dtk; 3 dtk setelah 5 kali tanpa notifikasi baru). Deploy shell 5006A207, `--switch` (boot 9124, shell 10740). Pengukuran CPU idle sesudahnya belum bersih (indeks berkas awal 60 dtk + pemakaian aktif): 0,78 s/30 s pada 13:12; perlu diukur ulang saat benar-benar diam.

## 2026-11-19 13:38 — deploy di mode shell tidak lagi jatuh ke Explorer (persetujuan pengguna)

- Penyebab terbukti (log diagnostik baru "shell berakhir: ... Updating=0"): balapan waktu. Shell lama keluar 0,3 dtk setelah `--exit`; deploy memasang file dalam < 1 dtk lalu menghapus `Updating`, sedangkan win12boot baru memeriksa `Updating` setelah Sleep(500). Hasilnya terbaca "ditutup pengguna" dan Explorer cadangan dijalankan.
- `deploy-win12-rust.ps1`: penanda deploy = registri + berkas `install\updating.flag`; penanda baru dihapus setelah boot.log mencatat "deploy: menunggu shell baru" (maks 5 dtk); proses shell lama ditunggu lewat handle (maks 120 dtk) dan lamanya dicetak; pemeriksaan "bin lebih lama daripada sumber" hanya untuk -SkipBuild (setelah cargo build, bin yang tidak di-link ulang salah ditolak).
- `win12boot.rs`: `updating()` membaca registri atau `updating.flag` (< 5 menit); dibaca segera saat shell keluar; catatan "shell berakhir: kode, hidup, Updating, deploy". `core_c4b.rs`: catatan tahap keluar shell (shutdown_app mulai/selesai, loop pesan, wWinMain).
- Uji: deploy 13:38 SELESAI tanpa Explorer cadangan (shell 3DDA1286 pid 14616, boot F6CF6BF4 berlaku saat boot berikutnya; boot berjalan 15628). Sisa: saat shell keluar, Explorer kadang dijalankan pihak lain (bukan win12boot; kemungkinan COM ShellWindows dari proses lain); win12boot menutupnya dulu (~6 dtk) sebelum menjalankan shell baru.

## 2026-11-19 14:00 — panel terminal: File Explorer ikut folder shell (permintaan pengguna)

- Masalah: `z Perkantas` (zoxide) / cd di panel terminal tidak memindahkan File Explorer; sinkron hanya Explorer -> terminal, dan sinkron itu bisa mengetik `Set-Location` ke folder Explorer lama setelah pengguna pindah di terminal.
- `term.rs`: shell melaporkan folder lewat OSC 9;9 (cara Windows Terminal). PowerShell: `-NoExit -EncodedCommand` membungkus prompt setelah profil dimuat (prompt profil/zoxide tetap dipakai); cmd: `/k prompt $e]9;9;$P$e\$P$G`. Tidak dipasang bila `[files] terminal=` sudah memakai -Command/-File//k//c. OSC 7 (file://) juga dibaca. Folder baru -> `last_cd` diperbarui (Explorer tidak mengetik cd balik) -> WM_FM_TERM_CWD ke frame.
- `extras.rs::term_cwd`: Explorer pindah ke folder itu bila berbeda dan ada; fokus tetap di terminal.
- Uji kait prompt: pwsh dan powershell 5.1 mengeluarkan `ESC]9;9;"C:\Windows"ESC\PS C:\Windows>`. Lewat ConPTY belum diuji di panel.
- Deploy oleh agen 14:00 SELESAI tanpa Explorer cadangan (deploy kedua yang lolos setelah perbaikan 13:38). Shell 8B79EF85, files 6B82DD98. Jendela win12files yang terbuka (pid 14428, sejak 13:55) masih kode lama sampai ditutup.

## 2026-11-19 14:24 - win12files: tampilan per folder, ikon, status dobel, kolom penuh, This PC

- Permintaan pengguna: tampilan per folder (mis. Screenshot: urut terbaru + Ikon sangat besar) kembali ke bawaan setelah pindah folder; gambar ikon tampak melar; "770 items, 1 item selected" tampil dua kali; kolom Detail tidak mengisi lebar; This PC tidak seperti Explorer (ubin drive + "X GB free of Y GB"); fungsional ingin seperti Dolphin (belum dikerjakan, usulan ke pengguna).
- Penyebab: Initialize memaksa FVM_DETAILS untuk semua folder dan `fm_apply_sort` menerapkan urutan tab ke tiap folder; tidak ada penyimpanan per folder. Ukuran ikon dikali DPI tanpa batas (256 -> 384 di 150%). Bilah status bawaan DefView tidak dimatikan di samping bilah status sendiri.
- `viewstate.rs` (baru): simpan mode, ukuran, kelompok, urutan per folder di `HKCU\Software\Win12Shell\Files\Views` (nama nilai = jalur parsing huruf kecil, format `v1|...`), disimpan saat pergi/tab ditutup hanya bila berubah. Folder tanpa simpanan memakai FVM_AUTO (This PC = ubin drive dengan bilah kapasitas, sama seperti Explorer). OnNavigationPending: SetFolderSettings mode tersimpan; OnViewCreated: terapkan sisanya.
- Kolom Detail: sisa lebar ditambahkan ke kolom Nama (timer FM_TIMER_FILL 120 ms setelah navigasi, ubah ukuran, ganti mode).
- `bar.rs`: ukuran ikon dibatasi 256. `extras.rs`: EP_StatusBar -> EPS_DEFAULT_OFF|EPS_FORCE.
- cargo check 0 error 0 warning. Deploy oleh agen 14:24 SELESAI tanpa Explorer cadangan: shell A918BC7A, boot 00331A52, files A54CDF2A. Belum diuji pengguna; jendela win12files pid 12952 (sejak 14:10) masih kode lama sampai ditutup.

## 2026-11-19 14:40 - tangkapan layar hanya ke clipboard

- Keluhan pengguna: PrtScn/Win+Shift+S hanya disalin ke clipboard, ingin langsung masuk `OneDrive\Gambar\Screenshot` (= FOLDERID_Screenshots di mesin ini). File terakhir di folder itu 2026-11-03; boot.log: PrtScn terdaftar oleh shell (80 terdaftar, yang gagal hanya Win+A dan Win+panah), jadi `hk_publish_shot` jalan tetapi `hk_save_png` gagal diam-diam.
- `hotkeys.rs`: `hk_save_png_com` kini mengembalikan langkah WIC + HRESULT yang gagal; bila faktori WIC shell gagal/tidak ada, coba faktori baru (CoCreateInstance). Kegagalan dicatat `tangkapan: simpan PNG gagal ...` di boot.log. Cadangan folder: FOLDERID_Pictures\Screenshot. Teks toast "Gambar\Screenshot".
- Penyebab pasti belum diketahui (belum ada tangkapan setelah build ini). cargo check 0 error 0 warning; deploy agen SELESAI tanpa Explorer cadangan, shell EAC7ECDD.

## 2026-11-19 14:58 - tangkapan ke folder (penyebab ketemu) + pantau folder win12files

- boot.log 14:42: `tangkapan: FOLDERID_Screenshots gagal, pakai ` (kosong): di mode shell SHGetKnownFolderPath untuk Screenshots DAN Pictures gagal, jadi PNG tak pernah tersimpan dan hanya clipboard. `hotkeys.rs`: cadangan dari registri `User Shell Folders` {B7BEDE81-DF94-4682-A7D8-57A52620B86F} (= `OneDrive\Gambar\Screenshot`), lalu `Shell Folders`, `My Pictures\Screenshot`, `%USERPROFILE%\Pictures\Screenshots`; HRESULT dicatat.
- Keluhan: berkas yang dihapus baru hilang setelah refresh. Penyebab: tanpa explorer.exe DefView tidak menerima SHChangeNotify. `src\files\watch.rs` (baru): satu FindFirstChangeNotificationW per jendela untuk folder tab aktif (folder sistem berkas saja); `fm_run_loop` kini MsgWaitForMultipleObjectsEx + PeekMessage; sinyal -> timer FM_TIMER_WATCH 350 ms -> IShellView::Refresh (ditunda bila kotak ganti nama aktif; dilewati bila explorer.exe berjalan). WAIT_FAILED -> WaitMessage.
- Gambar mini "tidak semua dirender": belum didiagnosis, menunggu rincian pengguna.
- cargo check 0 error 0 warning; deploy agen SELESAI, shell B960F57A.

## 2026-11-19 15:31 - win12files: status dobel, bilah perintah lama, header di ikon, pembaruan bertahap

- Bukti dari tangkapan pengguna (15:24): bilah status bawaan "760 items" + bilah sendiri "760 item"; bilah perintah Win7 (Organise / Include in library / Give access to / New folder); header kolom di tampilan Ikon besar; banyak PNG lokal (761 berkas, 0 cloud-only) masih ikon generik.
- Log `files: panel ditanya`: DefView hanya menanyakan EP_Commands, EP_PreviewPane, EP_DetailsPane, EP_NavPane; EP_StatusBar TIDAK pernah ditanyakan, jadi perbaikan 14:24 lewat GetPaneState tidak berpengaruh. Bilah status bawaan dikendalikan `HKCU\...\Explorer\Advanced\ShowStatusBar`: agen mengubah 1 -> 0 (kembalikan dengan 1).
- `extras.rs`: EP_Commands/Organize/View -> off. `app.rs`: FOLDERSETTINGS fFlags = FWF_NOHEADERINALLVIEWS (Initialize + OnNavigationPending).
- `watch.rs` ditulis ulang: tanpa Refresh penuh. Potret nama+ukuran (tanpa berkas tersembunyi/sistem, maks 20.000) saat folder dibuka; sinyal (nama/ukuran saja; atribut dibuang karena OneDrive terus mengubahnya dan versi 14:58 jadi memuat ulang berulang) -> diff -> IShellFolderView::RemoveObject/AddObject; >300 perubahan -> Refresh.
- Gambar mini: proses prioritas Normal, tidak ada governor; penyebab awal belum terbukti. Hipotesis: muat ulang berulang (14:58) memperparah. Menunggu pengamatan pengguna.
- cargo check 0 error 0 warning; deploy SELESAI (shell E5971770); win12files dibuka ulang oleh agen di folder Screenshot (atas permintaan pengguna menjalankan versi terbaru). WS 74,8 MB / privat 60,3 MB setelah membuka folder 761 PNG.

## 2026-11-19 15:50 - penyebab gambar mini PNG terbukti; header kolom

- Uji IShellItemImageFactory dari proses lain: PNG di OneDrive\Gambar\Screenshot gagal WTS_E_FAILEDEXTRACTION (0x8004B200), jadi masalah sistem, bukan win12files.
- Penyebab 1: HKCU `.png` dan `.bmp` ShellEx {e357fccd-...} (IThumbnailProvider) diambil alih Google Drive (DriveFS Thumbnail Provider {8AD5CECD-...}, drivefsext.dll); Google Drive tidak berjalan. Atas persetujuan pengguna, kedua kunci dicadangkan ke `docs\port\backup-thumb-handler-20261119.reg` lalu dihapus (kembali ke PhotoThumbnailProvider HKLM). Salinan PNG di TEMP lalu berhasil (hr=0).
- Penyebab 2: PNG di OneDrive tetap gagal: folder itu sync root OneDrive (SyncRootManager ThumbnailProvider {021E4F06-...}), OneDrive.exe tidak berjalan (Explorer asli juga menampilkan "Start OneDrive"). Belum diubah; keputusan pengguna (jalankan OneDrive atau tidak).
- Header kolom masih muncul di tampilan ikon meski FOLDERSETTINGS.fFlags: kini `IFolderView2::SetCurrentFolderFlags(FWF_NOHEADERINALLVIEWS)` di OnViewCreated. Deploy SELESAI shell 3E70042B; win12files dibuka ulang agen (pid 7072).

## 2026-11-19 16:05 - gambar mini sendiri (Rust + WIC) di win12files

- Pengguna memilih "buat mini render sendiri di rust" (alternatif menjalankan OneDrive).
- Tampilan DefView di Win11 = DirectUIHWND (ItemsView), tanpa SysListView32. `app.rs`: `IFolderViewOptions::SetFolderViewOptions(FVO_VISTALAYOUT)` sebelum Initialize -> terverifikasi `SysListView32 <- SHELLDLL_DefView`.
- `src\files\thumbs.rs` (baru): subclass SHELLDLL_DefView (SetWindowSubclass) di OnViewCreated; NM_CUSTOMDRAW -> CDDS_ITEMPOSTPAINT; untuk tampilan ikon/ubin, berkas gambar (png/jpg/bmp/gif/webp/heic/tif/ico) ber-reparse-point saat OneDrive.exe tidak berjalan (dicek tiap 3 dtk): kotak ikon (LVIR_ICON) ditutup warna latar sudutnya lalu gambar mini AlphaBlend tengah-bawah. Satu thread `w12-thumbs` (COM MTA, WIC Fant -> PBGRA -> DIB), antrean LIFO maks 48, hasil lewat pesan terdaftar W12FilesThumbReady. Cache LRU 16 MB di thread UI.
- `frame.rs`: `process_running(exe)`.
- cargo check 0/0; deploy SELESAI shell 897FFAD0; win12files dibuka ulang agen (pid 11084, privat 57,7 MB). Hasil visual belum dilihat; video belum (butuh Media Foundation).

## 2026-11-19 16:20 - tata letak Vista dicabut; uji OneDrive

- Pengguna: hasil tata letak Vista + gambar mini sendiri "jelek sekali". FVO_VISTALAYOUT dan `thumbs::attach` dicabut dari app.rs (thumbs.rs tetap ada, tidak dipanggil). Deploy SELESAI shell 3033D8D9; win12files dibuka ulang agen.
- Pengguna memilih jalankan OneDrive asal tambahan RAM <= 50 MB. OneDrive.exe /background: setelah ~60 dtk privat 92,6 MB / WS 167,6 MB; gambar mini berubah dari 0x8004B200 menjadi 0x8004B205 (WTS_E_EXTRACTIONPENDING). Melebihi batas, jadi agen menjalankan OneDrive.exe /shutdown.
- Catatan: OneDrive.Sync.Service (privat ~145 MB, berjalan sejak ~11:50) dan FileCoAuth (~13 MB) sudah berjalan sebelumnya, bukan dinyalakan agen; tidak disentuh.

## 2026-11-19 16:30 - OneDrive dibiarkan berjalan

- Pengguna menimbang unlink (pilihan 2) lalu membatalkan ("penting juga kalau untuk tidak unlink"), lalu memilih OneDrive berjalan terus walau di atas 50 MB. Pemeriksaan: 8.487 berkas OneDrive (3,0 GB) semua lokal, 0 online-only.
- Agen gagal membuka pengaturan OneDrive sendiri (tangkapan area baki ditolak classifier); unlink tidak dilakukan.
- Startup: HKCU Run berisi OneDrive /background tetapi StartupApproved\Run\OneDrive = 03 (nonaktif) -> sebab OneDrive tidak jalan saat login. Agen mengubahnya ke 02 (aktif). win12boot `run_startup_items` menghormati StartupApproved.
- OneDrive.exe pid 5188 (dinyalakan agen 16:15) privat 229 MB saat sinkron awal/gambar mini; perlu diukur ulang setelah tenang.

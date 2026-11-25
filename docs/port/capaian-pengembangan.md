# Capaian pengembangan Win12 Shell (Rust)

Ditulis 2026-11-18. Status: **draf, menunggu persetujuan pengguna** untuk tahap yang ditandai [SETUJUI].
Tujuan: Win12 menggantikan hampir seluruh Windows shell (Explorer), sementara game dan aplikasi tetap berjalan normal.

## 0. Keadaan nyata (diperiksa 2026-11-18)

- `HKLM\...\Winlogon\Shell` = `explorer.exe`. Win12 belum menjadi shell sign-in. (Catatan memori `win12-rust-port` dan `win12-explorer-bebas` yang menyebut `win12boot.exe` tidak sesuai; perlu diperbaiki di Fase 6.)
- Run key `Win12Shell` = `install\win12shell.exe` (build Rust).
- Proses berjalan: `win12shell.exe` (PID 13472, mulai 13:46:42) dan `explorer.exe` (PID 7764). `win12files`, `win12guard`, dan `win12boot` tidak berjalan.
- `install\settings.ini`: `notchnotif=0`, `notchonly=0` (integrasi notifikasi notch nonaktif, banner Windows dipulihkan).
- Build Rust: `cargo check` 0 error 0 warning (2026-11-14). Build release OK. Belum ada uji fitur formal.
- Kode C++ sudah dihapus (2026-11-15). Skrip `win12-files-assoc.ps1` dan `tests\files_assoc_test.ps1` masih merujuk exe yang sudah tidak ada.

## 1. Prinsip

1. Win12 menambah lapisan di atas Windows. Aplikasi dan game tidak boleh terganggu.
2. Setiap tahap bisa dikembalikan ke Explorer. Tahap berikutnya hanya dimulai bila tahap sebelumnya lulus.
3. Setiap capaian punya kriteria lulus. Hasil dicatat sebagai lulus, gagal (dengan pesan), atau tidak bisa diuji.
4. Uji layar dilakukan pengguna sendiri. Saya tidak membaca screenshot dan tidak mengotomasi mouse atau keyboard.
5. Tidak ada tes atau eksekusi exe oleh saya kecuali diminta. Verifikasi kode saya batasi pada `cargo check -j 2`.

## 2. Fase 0: Baseline (selesai)

- [x] Port Rust T01–T16 dan integrasi 1–4. `cargo check` 0 error 0 warning; build release OK (2026-11-14).
- [x] Deploy build Rust ke `install` (2026-11-14 malam, dijalankan pengguna).
- [x] Build `-Shell` terpasang (2026-11-15). PhoneSvc dinonaktifkan dan paket Phone Link dihapus.
- [x] Kode C++ dan cadangan `*-before-*` dikirim ke Recycle Bin (2026-11-15).
- [x] Integrasi notifikasi notch dinonaktifkan lewat `settings.ini` dan terkonfirmasi di log.
- [x] Baseline C++ tercatat: WS 101,3 MB, private 43,4 MB, handle 880, thread 31 (`docs\port\pemakaian-cpp-20261114-121209.txt`).

## 3. Fase 1: Paritas Explorer dalam Win12 (Explorer tetap hidup)

Target: pintasan dan fitur Win12 berjalan berdampingan dengan Explorer tanpa merusak apa pun. Daftar uji lengkap ada di `docs\port\paritas-explorer.md`.

| No | Capaian | Kriteria lulus |
|---|---|---|
| 1.1 | Win+E membuka `win12files` di Beranda | Jendela terbuka. Bila gagal, jalur cadangan `explorer.exe` |
| 1.2 | Win+R membuka dialog Jalankan | Dialog muncul |
| 1.3 | Win+I membuka Pengaturan | Terbuka tanpa dialog. Bila gagal, dialog "beralih" muncul; Ya kembali ke Explorer |
| 1.4 | Win+A, Win+N, Win+D, Win+X, Win+panah | Quick Settings, kalender/notifikasi, tampil desktop, menu cepat, dan snap berfungsi |
| 1.5 | Start: "Keamanan Windows" | Terbuka tanpa dialog (AUMID SecHealthUI). Catat pesan bila gagal |
| 1.6 | Start: Kalkulator dan Notepad | Keduanya ada di daftar yang disematkan |
| 1.7 | Chevron (^) di bilah | Flyout berisi semua ikon tray. Ikon tersembunyi di belakang ikon biasa. Catat ikon yang tidak tampil |
| 1.8 | Alt+Tab | Kartu tersusun, thumbnail DWM tampil, nama kartu terisi. [KEPUTUSAN] Layout horizontal dipertahankan atau daftar vertikal (catatan `tindak-lanjut-2026-11-14.md` baris 19 belum dicocokkan) |
| 1.9 | Refit jendela maksimal saat ganti mode | Tepi bawah jendela menempel di atas bar atau pil, tanpa celah dan tanpa tumpang tindih |
| 1.10 | Win+V, Win+. (dan Win+;), Win+H, Win+G | Win+V membuka panel riwayat clipboard (`cliphist_ui`), Win+. membuka panel emoji (`emoji_ui`), lihat `ledger.md` 2026-11-14/15. Win+H dan Win+G hanya toast (bagian 9) |
| 1.11 | Chevron dibuka dengan `SetForegroundWindow` | Flyout benar-benar mendapat fokus. Bila gagal, cadangan `SwitchToThisWindow` dipakai |
| 1.12 | Win+Shift+S dan Win+S | Snipping dan pencarian Start berjalan |
| 1.13 | Notch: lirik, bar progres media, mode Terus/Sementara | Tampil sesuai pengaturan. Lirik dengan elipsis benar. Notifikasi tidak lagi masuk notch |
| 1.14 | Panel Pengaturan notch dan ikon exe di Start | Tampil. Ikon exe ter-load |
| 1.15 | Volume tanpa OSD | Perubahan volume tidak memunculkan OSD ganda |

## 4. Fase 2: Kompatibilitas aplikasi dan game (Explorer tetap hidup)

Kriteria umum untuk setiap aplikasi: berjalan normal, pil dan taskbar tidak muncul di atas saat fullscreen, Alt+Tab tidak terganggu, ikon tray (bila ada) tetap berfungsi, dan input tidak terblokir oleh hook keyboard.

| No | Aplikasi | Risiko | Yang diuji |
|---|---|---|---|
| 2.1 | Minecraft Bedrock (Store) | Sedang | Fullscreen, Alt+Tab, input saat gameplay |
| 2.2 | Minecraft Java dan Prism Launcher | Rendah | Fullscreen, Alt+Tab, jendela launcher |
| 2.3 | Roblox (Store, anti-cheat) | **Tinggi** | Hook keyboard `WH_KEYBOARD_LL` tidak mengganggu input. Governor tidak membatasi proses game. Fullscreen |
| 2.4 | HoYoPlay (launcher dan game) | **Tinggi** | Anti-cheat dan hook keyboard. Governor. Fullscreen |
| 2.5 | Steam (client, overlay, Idle Slayer, Idle Cave Miner, Vampire’s Fall 2) | Sedang | Tray Steam, overlay, fullscreen |
| 2.6 | Terraria | Rendah | Fullscreen, input |
| 2.7 | Valorant | **Tinggi, belum terpasang** | Hanya setelah [SETUJUI]: `vgc` dan `vgk` ditambahkan ke daftar KEBAL governor sebelum uji. Hook keyboard ditinjau ulang |
| 2.8 | Wallpaper Engine | **Tinggi** | Wallpaper tampil di layer desktop dan tidak tertutup pil. Tidak bertabrakan dengan wallpaper Win12 |
| 2.9 | WhatsApp, Telegram, Discord, Teams, Zoom | Sedang | Ikon tray, toast, fokus jendela saat notifikasi diklik |
| 2.10 | Edge, Chrome, Brave, Zen | Rendah | Video fullscreen (F11), Alt+Tab |
| 2.11 | OBS Studio | Sedang | Capture jendela dan layar tetap berjalan |
| 2.12 | Office, OneNote, Notion, Obsidian, Canva, Spotify, MPV, CapCut, dan aplikasi Store lain | Rendah | Jendela standar, Alt+Tab, Win+panah |
| 2.13 | Tray: OneDrive, Google Drive, AMD Software, FxSound, Mem Reduct, DroidCam, NectarLink, Epson Scan | Sedang | Ikon tampil di chevron. NectarLink hanya dilihat, tidak disentuh |
| 2.14 | Alat pengembangan: VS Installer, Git, Windows Terminal, Alacritty, Postman, WSL, Android Platform-Tools | Rendah | Jendela dan terminal berfungsi |
| 2.15 | Pengaturan (`ms-settings:`) | Sedang | Buka halaman umum. Halaman gaming (Game Bar tidak terpasang) menampilkan pesan yang wajar |

## 5. Fase 3: Explorer dimatikan (mode `--shell`) [SETUJUI]

Syarat masuk: Fase 1 dan Fase 2 lulus untuk semua item risiko tinggi. Matriks uji tercatat. Jalur kembali ke Explorer sudah teruji.

| No | Capaian | Kriteria lulus |
|---|---|---|
| 3.1 | Ikon desktop (`desk_icons.rs`) | Membaca folder Desktop. Klik dua kali, klik kanan, dan seleksi berfungsi |
| 3.2 | Wallpaper dan pengaturan wallpaper tanpa Settings | [KEPUTUSAN] Ganti wallpaper cukup lewat Win12 atau tetap lewat Pengaturan |
| 3.3 | Tempat sampah | [KEPUTUSAN] Dibuat, atau dikeluarkan dari cakupan. Saat ini belum ada |
| 3.4 | Multi-monitor | [KEPUTUSAN] Didukung atau dikeluarkan dari cakupan. Belum diverifikasi |
| 3.5 | Jendela file (`win12files`) untuk semua aksi Explorer | Buka folder, salin, pindah, ganti nama, hapus ke Recycle Bin, Properties |
| 3.6 | Dialog "Buka dengan" dan asosiasi file | `win12-files-assoc.ps1` diperbarui dengan exe yang benar. Asosiasi tetap berfungsi |
| 3.7 | Tray tanpa Explorer | Aplikasi yang memakai `TaskbarCreated` tetap punya ikon. Ikon tray tetap bisa dibuka |
| 3.8 | Notifikasi dan Action Center | Banner Windows tampil. Win+N tetap berfungsi |
| 3.9 | Pintasan mode shell | Win+V, Win+., Win+H, Win+G, Win+Shift+S, PrtScn, Alt+PrtScn sesuai daftar bagian 9 |
| 3.10 | Keluar dan masuk mode | `--exit` menutup halus. `win12boot --switch` kembali ke Explorer tanpa kehilangan jendela |
| 3.11 | Refit dan pil saat mode shell | Jendela maksimal menempel di atas bar. Tidak ada pil ganda |
| 3.12 | Pemakaian memori | Tidak lebih tinggi dari baseline C++ (WS 101,3 MB). Pengukuran dengan `catat-pemakaian.ps1 -Label rust` oleh pengguna |
| 3.13 | Pemulihan | Ada langkah tertulis untuk kembali ke Explorer bila shell macet |

## 6. Fase 4: Keamanan dan pemulihan (wajib sebelum Fase 5)

- [ ] 4.1 Dokumen pemulihan tertulis: kembali ke Explorer dari sesi biasa, dari Safe Mode, dan dari keadaan tanpa Explorer. Lokasi: `Documents\Win12-pemulihan`. Diperbarui dan diuji langkahnya oleh pengguna.
- [ ] 4.2 `win12guard` diputuskan: dipasang atau tidak. [SETUJUI] bila dipasang.
- [ ] 4.3 Governor `win12gov`: daftar KEBAL (`src\bin\win12gov.rs` baris 65–69) mencakup proses anti-cheat dan game sebelum uji game. [SETUJUI] untuk setiap perubahan daftar.
- [ ] 4.4 Hook keyboard `WH_KEYBOARD_LL` (`core_c4b.rs`): perilaku saat game fullscreen diputuskan. Hook dilepas atau dilewati saat game aktif bila perlu.
- [ ] 4.5 `WINLOGON_WRITE_ENABLED` (`src\bin\win12boot.rs:50`) tetap `false` sampai Fase 5.

## 7. Fase 5: Ganti `Winlogon\Shell` (paling akhir) [SETUJUI KERAS]

Syarat masuk: Fase 3 dan Fase 4 lulus. Rencana pemulihan tertulis dan sudah diuji langkahnya.

- [ ] 5.1 Pengguna menyetujui perubahan `Winlogon\Shell` secara eksplisit. Saya tidak mengubahnya tanpa persetujuan itu.
- [ ] 5.2 Setelah diganti: sign-in masuk ke `win12boot` lalu `win12shell`. Uji sign-in, lock screen, sleep dan resume, logout, dan shutdown.
- [ ] 5.3 Bila gagal: langkah pemulihan dari bagian 6 (4.1) dijalankan. Nilai `Winlogon\Shell` dikembalikan ke `explorer.exe`.

## 8. Fase 6: Pembersihan dan pemeliharaan

- [ ] 6.1 Skrip yang merujuk exe lama (`win12-files-assoc.ps1`, `tests\files_assoc_test.ps1`) diperbarui atau dihapus. [SETUJUI] untuk penghapusan.
- [ ] 6.2 Memori `win12-rust-port` dan `win12-explorer-bebas` diperbarui. Catatan `Winlogon\Shell = win12boot.exe` diperbaiki menjadi `explorer.exe` (sesuai bagian 0).
- [ ] 6.3 `docs\port\ledger.md` diperbarui dengan entri baru. File ini bersifat append-only. Baris 53 dan 105 tidak boleh diubah.
- [ ] 6.4 Setiap perubahan kode diverifikasi dengan `cargo check --lib -j 2` dan `cargo check --bins -j 2`.

## 9. Pekerjaan tertunda (di luar cakupan sampai Fase 5 selesai)

- Dikte (Win+H) dan host Game Bar (Win+G): saat ini hanya toast. Dibahas setelah uji runtime Fase 1–3. (Koreksi 2026-11-18: panel emoji dan riwayat clipboard sudah ada di kode, belum diuji runtime.)
- Animasi atau tween pil saat transisi mode. Belum diputuskan.
- Optimasi render (cache layout `text()` di `core_c1.rs`, dan `T_FGPOLL` 500 ms). Dilakukan hanya setelah pengukuran waktu render. Mengubah `T_FGPOLL` menyimpang dari paritas C++.

## 10. Batas yang berlaku di semua fase

- Tidak menyentuh `mockup\`, `.impeccable\`, SuhuAman (termasuk `settings.ini`-nya), NectarLink, Defender, SecurityHealth, dwm, csrss, lsass, svchost, driver AMD, TextInputHost, dan pengaturan daya.
- Tidak menghentikan Explorer atau proses lain tanpa izin. Tidak membuat service atau scheduled task.
- Tidak menjalankan boot, stopHosts, diet-apply, `--diag-cycle`, atau `catat-pemakaian.ps1` tanpa izin.
- Tidak menulis `Winlogon\Shell` tanpa persetujuan (lihat 5.1).
- Tidak memasang Valorant, dan tidak menambah anti-cheat ke KEBAL tanpa persetujuan.
- Commit dan push hanya atas nama akun pengguna dan hanya bila diminta.

## 11. Persetujuan yang dibutuhkan

1. Menyimpan daftar aplikasi dan dokumen ini ke `docs\port\` (ini dokumen, bukan berkas kode).
2. Menghentikan Explorer untuk mode `--shell` (Fase 3).
3. Deploy build baru.
4. Mengubah `Winlogon\Shell` (Fase 5).
5. Memasang Valorant dan menambah `vgc`, `vgk` ke KEBAL (2.7).
6. Memasang `win12guard` (4.2).
7. Menghapus atau mengubah skrip exe lama (6.1).

## 12. Keputusan yang masih terbuka

- Alt+Tab: layout horizontal atau daftar vertikal (1.8).
- Wallpaper: lewat Win12 atau tetap lewat Pengaturan (3.2).
- Tempat sampah dan multi-monitor: didukung atau dikeluarkan dari cakupan (3.3, 3.4).
- Hook keyboard saat game fullscreen (4.4).

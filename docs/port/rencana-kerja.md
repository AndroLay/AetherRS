# Rencana kerja Win12 Shell (Rust)

Ditulis 2026-11-18. Turunan dari `capaian-pengembangan.md`: dokumen itu memuat *apa* yang harus dicapai, dokumen ini memuat *pekerjaan* dan urutannya.
Legenda pelaksana: **S** = dikerjakan saya (kode/dokumen, diverifikasi `cargo check -j 2`), **P** = dijalankan pengguna (uji layar, deploy, admin), **[SETUJUI]** = butuh persetujuan sebelum mulai.

## Urutan dan ketergantungan

```
R0 (persiapan) -> R1 (uji paritas) -> R2 (perbaikan hasil R1)
                        |
                        +-> R3 (mode game) -> R4 (uji game & aplikasi)
R4 lulus -> R5 (celah mode shell) -> R6 (uji mode shell) -> R7 (pemulihan) -> R8 (Winlogon)
R9 (pembersihan) dan R10 (optimasi) berjalan kapan saja di sela, tanpa mengganggu urutan.
```

## R0. Persiapan (S, tanpa persetujuan tambahan)

| ID | Pekerjaan | File | Selesai bila |
|---|---|---|---|
| R0.1 | Templat hasil uji: satu baris per capaian (1.1–3.13, 2.1–2.15), kolom hasil/pesan/tanggal | `docs\port\hasil-uji.md` (baru) | Templat ada, nomor sama dengan `capaian-pengembangan.md` |
| R0.2 | Perbaiki memori: `Winlogon\Shell` nyata = `explorer.exe` | memori `win12-rust-port`, `win12-explorer-bebas`, `MEMORY.md` | Catatan sesuai bagian 0 capaian |
| R0.3 | Entri ledger untuk dokumen baru (append-only) | `docs\port\ledger.md` (akhir file) | Baris 53 dan 105 tidak berubah |

## R1. Uji paritas Fase 1 (P)

- Pengguna menjalankan capaian 1.1–1.15 dengan build yang terpasang sekarang (PID 13472), Explorer tetap hidup.
- Hasil ditulis ke `hasil-uji.md` (atau dikirim di chat, saya yang menulis).
- Tidak ada deploy di tahap ini.

## R2. Perbaikan hasil R1 (S, lalu deploy oleh P [SETUJUI])

- Setiap item gagal: cari akar penyebab di kode dulu (sebut file:baris), baru perbaiki. Satu perbaikan per item.
- Verifikasi: `cargo check --lib -j 2` dan `--bins -j 2`, 0 error 0 warning.
- Build release dan deploy dikumpulkan jadi satu putaran, dijalankan pengguna dengan `deploy-win12-rust.ps1`.
- Keputusan terbuka yang harus dijawab di sini: layout Alt+Tab (1.8).

## R3. Mode game (S, kode; perilaku butuh keputusan pengguna)

Tujuan: Win12 tidak menyentuh input dan prioritas game.

| ID | Pekerjaan | Lokasi | Catatan |
|---|---|---|---|
| R3.1 | Saat jendela depan fullscreen (`FULLSCREEN`), `kb_hook_proc` hanya memproses tombol Win; tombol lain langsung `CallNextHookEx` tanpa logika | `src\shell\core_c4b.rs:209` | Tombol volume saat ini selalu ditelan (baris ~229); [KEPUTUSAN] tetap ditelan atau dilewatkan saat game |
| R3.2 | Opsi `gamehook=0/1` di `[shell]` untuk melepas hook sepenuhnya saat fullscreen | `core_c4b.rs` (`kb_hook_start` baris 406), `settings.ini` | Default aman: hook tetap, logika minimum |
| R3.3 | Daftar KEBAL governor: tambah launcher dan game yang terpasang (`hoyoplay`, `steam`, `steamwebhelper`, `javaw`, `terraria`, `wallpaper32`, `wallpaper64`) | `src\bin\win12gov.rs:65–69` | [SETUJUI] per perubahan daftar. `vgc`/`vgk` hanya bila Valorant dipasang |
| R3.4 | Deteksi Wallpaper Engine: bila `wallpaper32/64.exe` berjalan, layer wallpaper Win12 mengalah (tidak menggambar di atas WorkerW) | modul wallpaper/desktop (dicari dulu dengan Grep) | Tujuannya 2.8 |
| R3.5 | Tidak ada pil, notch, atau toast muncul di atas game fullscreen, kecuali dipanggil tombol Win | `core_c3b.rs` `update_foreground` (1212–1316) | Periksa jalur yang sudah ada dulu, baru ubah bila perlu |

## R4. Uji game dan aplikasi Fase 2 (P)

- Urutan: risiko tinggi dulu (2.3 Roblox, 2.4 HoYoPlay, 2.8 Wallpaper Engine), lalu sedang, lalu rendah.
- Valorant (2.7) dilewati sampai pengguna memutuskan memasangnya.
- Hasil masuk `hasil-uji.md`. Item gagal kembali ke R2/R3.

## R5. Celah mode shell (S)

Dikerjakan sebelum Explorer dimatikan, agar uji R6 tidak sia-sia.

| ID | Pekerjaan | Keadaan kode sekarang | Keputusan |
|---|---|---|---|
| R5.1 | Tempat sampah: ikon desktop + buka di `win12files` + kosongkan (`SHEmptyRecycleBinW`) | Tidak ada referensi Recycle Bin di `src` | [KEPUTUSAN] dibuat atau keluar cakupan |
| R5.2 | Multi-monitor: bar/pil hanya di monitor utama, jendela di monitor lain tidak rusak | `MonitorFromWindow` dipakai di `core_c3b`, `core_c4`, `overlay`, `snap`, `tasks_ui` | [KEPUTUSAN] cakupan |
| R5.3 | Wallpaper tanpa Explorer: perubahan dari Pengaturan diterapkan Win12 | `T_WALLPAPER` 15000 ms ada di `core_c4b.rs` | Periksa apakah cukup |
| R5.4 | Tray tanpa Explorer: siarkan `TaskbarCreated` saat host tray siap, supaya aplikasi mendaftar ulang ikon | host tray di `core_c3.rs` (`tray_check` 1474+) | Grep dulu apakah sudah ada |
| R5.5 | `win12files`: salin, pindah, ganti nama, hapus ke Recycle Bin, Properties | `src\bin\win12files` | Daftar celah ditulis dulu, lalu dikerjakan |
| R5.6 | Asosiasi file: `win12-files-assoc.ps1` merujuk exe lama | Berkas ada di `Pictures\win12-shell\` (folder lama), bukan di repo Rust | Diperbarui ke `win12-shell-rust\install\` atau dipindah ke repo |
| R5.7 | Pintasan mode shell (Win+V, Win+., Win+H, Win+G) | Win+V dan Win+. punya panel sendiri (`hotkeys.rs:1566–1571`); Win+H dan Win+G toast | Uji panel; Win+H/Win+G tetap toast sampai R8 selesai |

## R5b. Aplikasi sistem Windows yang bergantung pada Explorer (S kode, P uji)

Prinsip: aplikasi sistem tidak diganti. Win12 hanya memastikan (a) aplikasi itu bisa dibuka tanpa Explorer, dan (b) bila aplikasi itu sendiri memanggil `explorer.exe`, shell Explorer tidak ikut menyala diam-diam.

Keadaan kode: jalur internal Win12 sudah dijaga. `open_folder_window` (`core_c3b.rs:728–756`) dan cadangan `win12files` (`files\frame.rs:417–440`) tidak menyalakan `explorer.exe` di mode shell. Pin Explorer dialihkan ke `win12files` (`core_c1.rs:1296`). Pengaturan dibuka lewat `hk_launch_settings_uri` (`hotkeys.rs:296`), dan app paket lewat `hk_launch_packaged` (`hotkeys.rs:285`). **Yang belum dijaga:** aplikasi lain yang memanggil `explorer.exe` langsung (contoh: "Tampilkan di folder" di browser, Panel Kontrol).

| ID | Aplikasi / fitur | Ketergantungan pada Explorer | Pekerjaan Win12 | Risiko |
|---|---|---|---|---|
| R5b.1 | Pengaturan (`ms-settings:`) | Aplikasi paket, tidak butuh Explorer. Halaman Taskbar, Start, dan Personalisasi mengatur fitur Explorer yang tidak berlaku di Win12 | Uji halaman umum (Sistem, Bluetooth, Jaringan, Aplikasi, Akun, Waktu, Pembaruan). Daftar halaman yang tidak relevan dicatat | Sedang |
| R5b.2 | Panel Kontrol (`control.exe`) | Halaman utama dan sebagian applet ditampilkan dalam jendela Explorer | Uji applet `.cpl` langsung (`ncpa.cpl`, `mmsys.cpl`, `appwiz.cpl`, `powercfg.cpl` hanya dibuka, tidak diubah). [KEPUTUSAN] halaman utama Panel Kontrol: dibiarkan memakai Explorer atau dialihkan | **Tinggi** |
| R5b.3 | "Tampilkan di folder" / `explorer.exe /select` dari aplikasi lain | Memanggil `explorer.exe` langsung; di mode shell bisa menyalakan shell Explorer | Asosiasi `Folder` dan `Directory` diarahkan ke `win12files` (skrip R5.6). Uji dari Chrome, Edge, Steam, dan OBS | **Tinggi** |
| R5b.4 | Dialog Buka/Simpan (comdlg32) | Berjalan di dalam proses aplikasi, tidak butuh Explorer | Uji saja (Office, browser, Notepad) | Rendah |
| R5b.5 | Notifikasi Windows dan pusat notifikasi | Banner digambar ShellExperienceHost, yang dihidupkan Explorer | Uji apakah banner tetap tampil tanpa Explorer. Bila tidak, `notchnotif=1` menjadi pengganti | **Tinggi** |
| R5b.6 | Clipboard (Win+V), emoji (Win+.), dikte (Win+H) | UI bawaan ada di TextInputHost, pintasannya didaftarkan Explorer | Win12 punya panel sendiri untuk Win+V dan Win+.; Win+H toast. TextInputHost tidak disentuh | Sedang |
| R5b.7 | Progres dan badge taskbar (`ITaskbarList3`), jump list | Fitur taskbar Explorer; tanpa Explorer, panggilan aplikasi diabaikan tanpa error | Tidak ada di kode Win12 sekarang. [KEPUTUSAN] dibuat atau keluar cakupan | Rendah |
| R5b.8 | Tray (`TaskbarCreated`) | Aplikasi mendaftar ulang ikon saat pesan ini disiarkan | Win12 mendaftarkan pesannya (`core_c4b.rs:1467`). Periksa apakah disiarkan saat host tray siap (R5.4) | Sedang |
| R5b.9 | Task Manager, Device Manager, Event Viewer, Disk Management, Windows Security, Snipping Tool, Store, App Installer | Proses mandiri, tidak butuh Explorer | Uji buka dari Start dan Win+X | Rendah |
| R5b.10 | AutoPlay USB | Ditangani Explorer | Win12 punya `usb.rs`; uji colok USB | Sedang |
| R5b.11 | Pintasan Windows (`.lnk`), `shell:` URI (`shell:startup`, `shell:appsfolder`) | Diproses lewat shell namespace, sebagian membuka jendela Explorer | Arahkan ke `win12files` bila jalurnya folder. Uji dari Win+R | Sedang |
| R5b.12 | Wallpaper dan tema dari Pengaturan | Pengaturan menulis nilai; Explorer yang menerapkan | R5.3 | Sedang |

Uji R5b.1–R5b.12 dilakukan dua kali: dengan Explorer hidup (R4) dan tanpa Explorer (R6).

## R6. Uji mode shell Fase 3 (P) [SETUJUI]

1. Pastikan jalan kembali sudah teruji: `win12shell.exe --exit`, lalu `win12boot --switch`.
2. Masuk mode `--shell` (Explorer berhenti) hanya atas izin pengguna.
3. Jalankan 3.1–3.13 dan ulangi item risiko tinggi R4.
4. Pengukuran memori (`catat-pemakaian.ps1 -Label rust`) dijalankan pengguna.

## R7. Pemulihan (S menulis, P menguji)

- Perbarui `Documents\Win12-pemulihan`: kembali ke Explorer dari sesi biasa, dari shell macet (Ctrl+Shift+Esc -> jalankan `explorer.exe`), dan dari Safe Mode (mengembalikan `Winlogon\Shell`).
- Putuskan `win12guard` (pemulih otomatis bila shell mati) [SETUJUI bila dipasang].
- Selesai bila pengguna sudah mencoba setiap langkah sekali.

## R8. Ganti Winlogon (P) [SETUJUI KERAS]

- Hanya setelah R6 dan R7 lulus.
- `WINLOGON_WRITE_ENABLED` (`src\bin\win12boot.rs:50`) baru diubah bila pengguna menyetujui secara eksplisit.
- Uji: sign-in, lock, sleep/resume, logout, shutdown. Gagal -> langkah R7.

## R9. Pembersihan (S, kapan saja)

- Skrip yang merujuk exe lama (R5.6, `tests\files_assoc_test.ps1` di folder lama): perbarui; hapus hanya [SETUJUI].
- Ledger dan memori diperbarui di akhir setiap tahap.

## R10. Optimasi (S, setelah ada pengukuran)

- Ukur dulu: waktu render per frame dan CPU idle (log sementara, dimatikan setelah ukur).
- Kandidat: cache `IDWriteTextLayout` di `text()` (`core_c1.rs`), interval `T_FGPOLL` 500 ms (menyimpang dari paritas C++, butuh keputusan).
- Tidak dikerjakan sebelum R4, karena perilaku lebih penting daripada angka.

## Yang bisa saya kerjakan sekarang tanpa persetujuan tambahan

1. R0.1–R0.3 (templat uji, memori, ledger).
2. R3.1 dan R3.2 sebagai kode + `cargo check` (belum deploy).
3. Pemeriksaan kode R5.3 dan R5.4 (baca saja), hasilnya ditulis di dokumen ini.
4. Daftar celah `win12files` untuk R5.5 (baca saja).

## Status pengerjaan 2026-11-18 (kode lolos `cargo check --lib --bins -j 2`, rc=0, 0 error, 0 warning; belum build release, belum deploy, belum uji runtime)

| ID | Status | Bukti / catatan |
|---|---|---|
| R0.1 | Selesai | `docs\port\hasil-uji.md` (termasuk N1–N10 untuk perubahan hari ini) |
| R0.3 | Selesai | Entri ledger 2026-11-18 |
| R3.1 | Selesai kode | `core_c4b.rs` `kb_hook_proc`: bila `GAME_FS` dan bukan mode shell, tombol selain Win langsung `CallNextHookEx`. Keputusan default: tombol volume saat game diserahkan ke Windows |
| R3.2 | Selesai kode | `[shell] gamehook` (default 1, hanya dibaca, `core_c1.rs` `load_settings`). `gamehook=0` melepas kait selama fullscreen di luar mode shell (`core_c3b.rs` setelah `FULLSCREEN.set`), dengan log. Di mode shell kait tetap aktif (dibutuhkan untuk Alt+Tab dan tombol Win) |
| R3.3 | Selesai kode | `win12gov.rs` KEBAL: `gamelaunchhelper`, `robloxcrashhandler`, `hyp`, `hyphelper`, `steam`, `steamwebhelper`, `prismlauncher`, `javaw`, `terraria`, `wallpaper32`, `wallpaper64`, `vgc`. Nama exe diperiksa di mesin ini. Game HoYoPlay belum ada yang terpasang |
| R3.4 | Diperiksa, sebagian | Mode Explorer: pil atas ditaruh di atas blok Progman+WorkerW (`core_c2.rs` `pills_to_desk`), tidak bertabrakan. Mode shell: Progman Win12 belum menangani pesan `0x052C` (pembuatan WorkerW), jadi Wallpaper Engine belum didukung di mode shell. Ditunda sampai bisa diuji |
| R3.5 | Sudah ada | `notch.rs:2535` `notch_sync`: notch disembunyikan saat fullscreen; `nt_show_temp` dan toast menolak tampil |
| R5.1 | Selesai kode | `desk_icons.rs`: ikon pertama Tempat Sampah (ikon stok penuh/kosong), klik ganda -> `open_folder_window` (win12files). Menu desktop: "Kosongkan Tempat Sampah" (`SHEmptyRecycleBinW`, konfirmasi Windows) |
| R5.4 | Sudah ada | `TaskbarCreated` disiarkan di `core_c3.rs:1168` dan `core_c4b.rs:117` |
| R5.6 / R5b.3 | Selesai skrip | `win12-files-assoc.ps1` baru di akar repo Rust (exe `install\win12files.exe`). Belum dijalankan; Enable dijalankan pengguna. Asosiasi HKCU saat ini kosong |
| R5b.2 / R5b.11 | Selesai kode | `hotkeys.rs` `shell_mode_open` (mode shell saja): `control` tanpa argumen -> Panel Kontrol di win12files; `explorer` tanpa argumen atau dengan jalur folder -> win12files. Argumen `/select,` dan `.cpl` tidak diubah |
| Win+X | Diperbaiki | `winx.rs` `WX_EXPLORER`: sebelumnya `explorer.exe` langsung (bisa menyalakan shell Explorer di mode shell), sekarang `open_folder_window("")` |
| R7 | Selesai dokumen | `docs\port\pemulihan.md` (A–F, termasuk WinRE). Langkah belum dicoba pengguna |
| R5b.5 | Belum | Banner tanpa Explorer harus diuji dulu. `notchnotif` tidak diubah (pengguna menonaktifkannya) |
| R5b.7 | Keluar cakupan (default) | Progres taskbar dan jump list tidak dibuat; aplikasi tidak rusak |
| R5.2 multi-monitor | Belum | Butuh keputusan cakupan |
| R1, R2, R4, R6, R8 | Menunggu pengguna | Uji layar, deploy, dan Winlogon dijalankan pengguna |

Langkah berikutnya: pengguna menjalankan deploy (`deploy-win12-rust.ps1`, membangun ulang) lalu mengisi N1–N10 dan Fase 1 di `hasil-uji.md`.

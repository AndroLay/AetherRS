# Hasil uji Win12 Shell (Rust)

Diisi pengguna (atau saya, dari laporan di chat). Nomor sama dengan `capaian-pengembangan.md` dan `rencana-kerja.md`.
Hasil: **L** lulus, **G** gagal (tulis pesan/gejala), **T** tidak bisa diuji. Kolom Build: waktu build atau PID shell saat uji.

## Baru pada 2026-11-18 (butuh build + deploy dulu)

| No | Uji | Cara | Build | Hasil | Catatan |
|---|---|---|---|---|---|
| N1 | Mode game: tombol volume di game fullscreen | Mode biasa, game fullscreen, tekan volume: OSD Windows yang muncul, bukan notch | | | |
| N2 | Mode game: tombol Win tetap jalan | Di game fullscreen tekan Win: Start Win12 terbuka | | | |
| N3 | Mode game: input game normal | Main 10 menit (Roblox, Minecraft): tidak ada tombol tertelan | | | |
| N4 | `gamehook=0` | Tambah `gamehook=0` di `[shell]`, mulai ulang Win12, masuk game fullscreen: kait lepas (log), keluar fullscreen: kait aktif lagi | | | |
| N5 | Tempat Sampah di desktop (mode shell) | Ikon pertama; klik ganda membuka win12files di Tempat Sampah | | | |
| N6 | Kosongkan Tempat Sampah | Klik kanan desktop -> Kosongkan: dialog konfirmasi Windows; ikon berubah kosong | | | |
| N7 | Win+X -> File Explorer | Membuka win12files (bukan Explorer) | | | |
| N8 | Panel Kontrol (mode shell) | Win+R `control`: Panel Kontrol terbuka di win12files, Explorer tidak menyala | | | |
| N9 | `explorer.exe` dari Win+R (mode shell) | Win+R `explorer`: win12files terbuka | | | |
| N11 | Explorer sementara (mode shell) | Buka Pengaturan/Kamera/Kalkulator: Explorer menyala, app terbuka; tutup app, ±20 dtk kemudian Win12 penuh kembali (Explorer mati) | | | |
| N10 | Asosiasi folder | `win12-files-assoc.ps1 -Action Enable`, lalu buka folder dari aplikasi lain (mis. "Buka folder" unduhan): win12files | | | |

## Fase 1: paritas

| No | Build | Hasil | Catatan |
|---|---|---|---|
| 1.1 | | | |
| 1.2 | | | |
| 1.3 | | | |
| 1.4 | | | |
| 1.5 | | | |
| 1.6 | | | |
| 1.7 | | | |
| 1.8 | | | |
| 1.9 | | | |
| 1.10 | | | |
| 1.11 | | | |
| 1.12 | | | |
| 1.13 | | | |
| 1.14 | | | |
| 1.15 | | | |

## Fase 2: aplikasi dan game

| No | Aplikasi | Build | Hasil | Catatan |
|---|---|---|---|---|
| 2.1 | Minecraft Bedrock | | | |
| 2.2 | Minecraft Java / Prism | | | |
| 2.3 | Roblox | | | |
| 2.4 | HoYoPlay | | | |
| 2.5 | Steam + game | | | |
| 2.6 | Terraria | | | |
| 2.7 | Valorant | | T | Belum terpasang |
| 2.8 | Wallpaper Engine | | | Mode shell: belum didukung (lihat rencana-kerja R3.4) |
| 2.9 | WhatsApp, Telegram, Discord, Teams, Zoom | | | |
| 2.10 | Browser | | | |
| 2.11 | OBS | | | |
| 2.12 | Office dan aplikasi lain | | | |
| 2.13 | Ikon tray | | | |
| 2.14 | Alat pengembangan | | | |
| 2.15 | Pengaturan | | | |

## Aplikasi sistem (R5b), dua kali

| No | Aplikasi | Explorer hidup | Mode shell | Catatan |
|---|---|---|---|---|
| R5b.1 | Pengaturan | | | |
| R5b.2 | Panel Kontrol | | | |
| R5b.3 | "Tampilkan di folder" | | | |
| R5b.4 | Dialog Buka/Simpan | | | |
| R5b.5 | Banner notifikasi | | | |
| R5b.6 | Win+V, Win+., Win+H | | | |
| R5b.7 | Progres taskbar, jump list | | | |
| R5b.8 | Tray | | | |
| R5b.9 | Task Manager, Device Manager, dll. | | | |
| R5b.10 | AutoPlay USB | | | |
| R5b.11 | `shell:` URI | | | |
| R5b.12 | Wallpaper dari Pengaturan | | | |

## Fase 3: mode shell

| No | Build | Hasil | Catatan |
|---|---|---|---|
| 3.1 | | | |
| 3.2 | | | |
| 3.3 | | | |
| 3.4 | | | |
| 3.5 | | | |
| 3.6 | | | |
| 3.7 | | | |
| 3.8 | | | |
| 3.9 | | | |
| 3.10 | | | |
| 3.11 | | | |
| 3.12 | | | |
| 3.13 | | | |

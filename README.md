# AetherRS

Desktop shell kustom untuk Windows 10/11 yang ditulis pakai Rust. Fokusnya ringan, tampilan kaca/Mica lewat Direct2D & DirectWrite, pil dok melayang, notch media + lirik, dan file manager ber-tab dengan terminal tersemat.

## Isi Proyek

| Binary | Ukuran Release | Keterangan |
|---|---:|---|
| `win12shell.exe` | ~1,38 MB | Shell utama: pil dok, Start menu, notch media + lirik (LRCLIB), Quick Settings, flyout Wi-Fi/BT/baterai, Alt+Tab thumbnail DWM, ikon desktop, clipboard (`Win+V`), emoji (`Win+.`), gestur touchpad PTP. |
| `win12files.exe` | ~523 KB | File manager berbasis `IExplorerBrowser` (menu konteks & shell extension Windows tetap jalan), tab, breadcrumb, pencarian file, watcher, dan terminal ConPTY (`F4`). |
| `win12boot.exe` | ~338 KB | Peluncur dan pengawas mode `--shell` penuh, otomatis balik ke `explorer.exe` kalau shell gagal start berulang. |
| `win12gov.exe` | ~433 KB | Pengatur EcoQoS, prioritas RAM, dan GPU untuk proses latar (tanpa hak admin, ada daftar pengecualian untuk game & anti-cheat). |
| `win12guard.exe` | ~283 KB | Watchdog ringan untuk restart otomatis shell saat crash (maks 3x per 2 menit). |

## Build

Butuh Windows x64, Rust (`x86_64-pc-windows-gnu`), dan `windres.exe` (MSYS2/MinGW-w64).

```powershell
# Cek kompilasi
cargo check --lib --bins -j 2

# Jalankan unit/integration test
cargo test -j 2

# Build release
cargo build --release --bins -j 2
```

## Deploy, Uninstall & Rilis

```powershell
# Pasang ke folder install\ lokal
powershell -NoProfile -ExecutionPolicy Bypass -File .\deploy-win12-rust.ps1 -SkipBuild

# Cek dulu sebelum uninstall / balik ke Explorer
powershell -NoProfile -ExecutionPolicy Bypass -File .\uninstall-win12-rust.ps1 -DryRun

# Kemas rilis biner (.zip + SHA256) ke folder dist\ tanpa kode sumber
powershell -NoProfile -ExecutionPolicy Bypass -File .\pack-release.ps1 -Version 0.1.0
```

## Lisensi

All rights reserved. Lihat [LICENSE](LICENSE).

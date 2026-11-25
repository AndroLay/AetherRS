# Win12 Shell (Rust)

Shell desktop Windows modern berbahasa Rust dengan rendering Direct2D/DirectWrite, efek Mica/glass, Dynamic Island (Notch), manajer berkas ber-tab dengan terminal tersemat, serta pengawas mode shell penuh yang dapat dikembalikan ke Explorer kapan saja.

> **Status Repositori:** Repositori kode sumber ini bersifat **Private (Proprietary — All Rights Reserved)**. Distribusi ke pengguna publik dilakukan melalui paket biner (`.zip` + `SHA256SUMS.txt`) tanpa menyertakan kode sumber.

---

## Komponen Utama

| Binary | Ukuran Release | Fungsi |
|---|---:|---|
| `win12shell.exe` | ~1,38 MB | Shell utama: pil dok tersegmentasi, Start menu, Notch media + lirik tersinkron (LRCLIB), Quick Settings, flyout Wi-Fi/BT/baterai, Alt+Tab thumbnail DWM, desktop icons, clipboard (`Win+V`), emoji (`Win+.`), gestur touchpad PTP. |
| `win12files.exe` | ~523 KB | Pengganti File Explorer berbasis `IExplorerBrowser` (kompatibel penuh dengan menu konteks & shell extension Windows), tab, breadcrumb, pencarian indeks, file watcher, dan terminal ConPTY tersemat (`F4`) dengan sinkronisasi folder. |
| `win12boot.exe` | ~338 KB | Peluncur dan pengawas mode `--shell` penuh dengan deteksi deploy (`updating.flag`) dan fallback otomatis ke `explorer.exe` jika terjadi crash-loop. |
| `win12gov.exe` | ~433 KB | Resource governor tanpa admin (EcoQoS, prioritas memori & GPU) untuk proses latar, dengan daftar kebal untuk game dan anti-cheat. |
| `win12guard.exe` | ~283 KB | Watchdog ringan untuk memantau dan me-restart `win12shell.exe` secara terkendali (maks. 3 kali per 2 menit). |

---

## Prasyarat Build

- Windows 10 / 11 (x64)
- Rust toolchain `x86_64-pc-windows-gnu` (stable)
- MinGW-w64 / MSYS2 (`windres.exe` di `C:\msys64\ucrt64\bin` atau `PATH`)

## Perintah Pengembangan

```powershell
# Verifikasi kompilasi (lib + 5 binary)
cargo check --lib --bins -j 2

# Jalankan seluruh tes logika & paritas
cargo test -j 2

# Build release ramping (opt-level="s", lto="thin", strip=true)
cargo build --release --bins -j 2
```

## Pemasangan, Pemulihan & Pengemasan Rilis

```powershell
# Pasang ke folder install\ lokal (opsional: -SkipBuild, -RestorePoint, -KeepBackups 5)
powershell -NoProfile -ExecutionPolicy Bypass -File .\deploy-win12-rust.ps1 -SkipBuild

# Simulasi pencabutan / kembali ke Explorer (aman, tidak mengubah apa pun)
powershell -NoProfile -ExecutionPolicy Bypass -File .\uninstall-win12-rust.ps1 -DryRun

# Kemas rilis biner publik ke dist\ (hanya .exe, skrip pengguna, LICENSE, SHA256SUMS.txt — tanpa kode sumber)
powershell -NoProfile -ExecutionPolicy Bypass -File .\pack-release.ps1 -Version 0.1.0
```

## Struktur Repositori

```text
src/
  bin/          Entry point 5 binary (win12shell, win12files, win12boot, win12gov, win12guard)
  shell/        UI shell Win32/Direct2D/DirectWrite/WinRT
  files/        UI & backend win12files (IExplorerBrowser, chrome, ConPTY terminal, watcher)
  *.rs          Modul logika murni & utilitas (sync, reg, log, wide, query, search, fm_logic, dll.)
tests/          14 suite tes paritas & logika (in-memory, tidak menyentuh sistem nyata)
docs/port/      Catatan internal pengembangan & paritas (tidak disertakan dalam rilis biner)
.github/        Workflow CI otomatis (cek sintaks PS1, cargo check, cargo test)
```

## Lisensi

Hak cipta dilindungi — lihat [LICENSE](LICENSE).

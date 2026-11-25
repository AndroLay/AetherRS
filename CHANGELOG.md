# Changelog

Semua perubahan penting pada proyek **Win12 Shell (Rust)** dicatat di file ini.

## [0.1.0] - 2026-11-25

### Ditambahkan
- **Shell pekerja asinkron modern**: `media_worker.rs` (GSMTC MTA thread), `notif_worker.rs`, `net_watch.rs`, `lyrics.rs` (lirik tersinkron LRCLIB di Notch), `emoji_data.rs`, dan `start_ctx.rs`.
- **Win12 Files lanjutan**: tampilan chrome kustom (`chrome.rs`), fitur tambahan (`extras.rs`), terminal ConPTY tersemat `F4` dengan sinkronisasi folder (`term.rs`), penyimpanan tampilan per folder (`viewstate.rs`), pemantau perubahan folder (`watch.rs`), dan thumbnail (`thumbs.rs`).
- **Skrip operasional & rilis**:
  - `win12-files-assoc.ps1` untuk asosiasi folder HKCU ke `win12files.exe`.
  - `uninstall-win12-rust.ps1` untuk pencabutan & pemulihan deterministik ke Explorer (mendukung `-DryRun`).
  - `pack-release.ps1` untuk mengemas rilis biner publik (`.zip` + `SHA256SUMS.txt`) tanpa menyertakan kode sumber.
  - Opsi `-KeepBackups` dan `-RestorePoint` pada `deploy-win12-rust.ps1`.
  - Workflow CI GitHub Actions (`.github/workflows/ci.yml`).
- **Dokumentasi proyek**: `README.md`, `LICENSE` (Proprietary / All Rights Reserved), dan `CHANGELOG.md`.

### Diperbaiki & Diperkuat
- **Keamanan konkurensi lintas-thread**: mengganti global `SyncCell` lintas-thread di `state.rs`, `core_c4b.rs` (`kb_thread`/`kb_hook_proc`), dan `filesearch.rs` dengan pembungkus atomik (`AtomicFlag`, `AtomicI32Cell`, `AtomicU32Cell`, `AtomicHandle`) serta `Mutex<T>` di `src/sync.rs`.
- **Keamanan memori terminal (`files/term.rs`)**: menghilangkan `&'static mut Term` dan use-after-free pada `WM_NCDESTROY` menggunakan `Box<RefCell<Term>>` + `with_term`.
- **Kompatibilitas game fullscreen**: menunda pasang-ulang hook `WH_KEYBOARD_LL` 20 detik saat game fullscreen aktif di luar mode `--shell`.
- **Pembersihan dead code**: menghapus helper vtable mentah `async_results` dan `thumb_begin` di `core_c1.rs` yang sudah digantikan `media_worker.rs`.
- **Tes paritas**: menyelaraskan `tests/stophosts.rs` dengan penambahan daftar `W12_STOP_NAME_TABLE` (8 proses).

### Dioptimalkan
- **Profil release `Cargo.toml`** (`opt-level = "s"`, `lto = "thin"`, `codegen-units = 1`, `strip = true`): memangkas total ukuran 5 binary dari **27,25 MB menjadi 2,96 MB (-89,1%)**.

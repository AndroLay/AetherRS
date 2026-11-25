# Changelog

## 0.1.0 - 2026-11-25

### Fitur
- Pekerja asinkron untuk media GSMTC (`media_worker.rs`), notifikasi (`notif_worker.rs`), jaringan (`net_watch.rs`), dan lirik tersinkron LRCLIB di notch (`lyrics.rs`).
- Data emoji (`emoji_data.rs`) dan menu konteks Start (`start_ctx.rs`).
- Pembaruan `win12files`: chrome kustom (`chrome.rs`), fitur tambahan (`extras.rs`), terminal ConPTY `F4` (`term.rs`), simpan tampilan per folder (`viewstate.rs`), watcher folder (`watch.rs`), dan thumbnail (`thumbs.rs`).
- Skrip operasional: `win12-files-assoc.ps1`, `uninstall-win12-rust.ps1`, `pack-release.ps1`, opsi `-KeepBackups` & `-RestorePoint` di `deploy-win12-rust.ps1`, serta CI GitHub Actions.

### Perbaikan
- Ganti global lintas-thread (`HI`, `MSG`, `WIN_KEY`, `SHELL_MODE`, `QUITTING`, `KB_*`, dan state `filesearch.rs`) ke tipe atomik dan `Mutex` di `src/sync.rs`.
- Bungkus `Term` di `src/files/term.rs` dengan `RefCell` dan tangani `WM_NCDESTROY` di awal `term_proc` agar tidak terjadi aliasing atau drop saat borrow aktif.
- Tunda pasang-ulang `WH_KEYBOARD_LL` saat game fullscreen aktif di luar mode `--shell`.
- Hapus helper vtable lama `async_results` dan `thumb_begin` di `core_c1.rs`.
- Sesuaikan `tests/stophosts.rs` dengan daftar proses terbaru (`W12STOP_COUNT = 8`).

### Optimasi
- Profil release `Cargo.toml` (`opt-level = "s"`, `lto = "thin"`, `codegen-units = 1`, `strip = true`): ukuran total 5 binary turun dari 27,25 MB menjadi 2,96 MB.

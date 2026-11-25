# Changelog

## 0.1.0 - 2026-11-25

### Added
- Asynchronous workers for GSMTC media (`media_worker.rs`), notifications (`notif_worker.rs`), network changes (`net_watch.rs`), and synced LRCLIB lyrics in the notch (`lyrics.rs`).
- Emoji dataset (`emoji_data.rs`) and Start menu context actions (`start_ctx.rs`).
- `win12files` upgrades: custom chrome (`chrome.rs`), extra panels (`extras.rs`), embedded `F4` ConPTY terminal with folder sync (`term.rs`), per-folder view state persistence (`viewstate.rs`), folder watcher (`watch.rs`), and thumbnails (`thumbs.rs`).
- Operational scripts: `win12-files-assoc.ps1`, `uninstall-win12-rust.ps1`, `pack-release.ps1`, `-KeepBackups` & `-RestorePoint` flags in `deploy-win12-rust.ps1`, and GitHub Actions CI.

### Fixed
- Replaced cross-thread `SyncCell` globals (`HI`, `MSG`, `WIN_KEY`, `SHELL_MODE`, `QUITTING`, `KB_*`, and `filesearch.rs` worker state) with atomic wrappers and `Mutex` in `src/sync.rs`.
- Wrapped `Term` in `src/files/term.rs` with `RefCell` and handled `WM_NCDESTROY` before borrowing to eliminate aliasing and drop-while-borrowed UB.
- Deferred periodic `WH_KEYBOARD_LL` re-install while fullscreen games are active outside `--shell` mode.
- Removed unused raw vtable helpers (`async_results`, `thumb_begin`) from `core_c1.rs`.
- Updated `tests/stophosts.rs` to match the expanded process table (`W12STOP_COUNT = 8`).

### Performance
- Tuned `Cargo.toml` release profile (`opt-level = "s"`, `lto = "thin"`, `codegen-units = 1`, `strip = true`), reducing total size across all 5 binaries from 27.25 MB to 2.96 MB.

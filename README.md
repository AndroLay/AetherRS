# AetherRS

A lightweight custom desktop shell for Windows 10/11 written in Rust. Built with Direct2D and DirectWrite for Mica/glass surfaces, a floating segmented dock, a live media + synced lyrics notch, and a tabbed file manager with an embedded ConPTY terminal.

## Components

| Binary | Release Size | Overview |
|---|---:|---|
| `win12shell.exe` | ~1.38 MB | Main shell: floating dock, Start menu, media & synced lyrics notch (LRCLIB), Quick Settings, Wi-Fi/BT/battery flyouts, DWM Alt+Tab thumbnails, desktop icons, clipboard history (`Win+V`), emoji picker (`Win+.`), and PTP touchpad gestures. |
| `win12files.exe` | ~523 KB | File manager built on `IExplorerBrowser` (preserves native Windows context menus and shell extensions), with tabs, breadcrumbs, indexed search, folder watcher, and an embedded ConPTY terminal (`F4`). |
| `win12boot.exe` | ~338 KB | Full `--shell` mode launcher and supervisor with automatic fallback to `explorer.exe` on repeated startup failures. |
| `win12gov.exe` | ~433 KB | Non-admin background resource governor (EcoQoS, memory, and GPU priority) with an allowlist for games and anti-cheat processes. |
| `win12guard.exe` | ~283 KB | Lightweight crash watchdog that restarts the shell (up to 3 times per 2 minutes). |

## Build

Requires Windows x64, Rust (`x86_64-pc-windows-gnu`), and `windres.exe` (MSYS2/MinGW-w64).

```powershell
# Type-check library and all binaries
cargo check --lib --bins -j 2

# Run unit and integration tests
cargo test -j 2

# Build optimized release binaries
cargo build --release --bins -j 2
```

## Deploy, Uninstall & Release Packaging

```powershell
# Install to local install\ directory
powershell -NoProfile -ExecutionPolicy Bypass -File .\deploy-win12-rust.ps1 -SkipBuild

# Preview uninstall / revert to Windows Explorer without making changes
powershell -NoProfile -ExecutionPolicy Bypass -File .\uninstall-win12-rust.ps1 -DryRun

# Package binary-only release (.zip + SHA256SUMS) into dist\ (no source code included)
powershell -NoProfile -ExecutionPolicy Bypass -File .\pack-release.ps1 -Version 0.1.0
```

## License

All rights reserved. See [LICENSE](LICENSE).

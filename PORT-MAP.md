# PORT-MAP — kepemilikan file dan tahap kerja

Sumber: `C:\Users\andro\Pictures\win12-shell` (baca saja). Aturan penulisan: `CONVENTIONS.md`.
Baris sumber dalam tanda kurung adalah jumlah baris.

## Tahap A (paralel)

| Agen | Sumber C++/C | File Rust yang dimiliki | Tes parity (`tests/`) |
|---|---|---|---|
| P1 | w12query.cpp (267), w12query.h (24), w12search.cpp (178), w12search.h (27), w12nl_json.cpp (463), w12nl_json.h (26), w12start_lists.h (66) | `src/query.rs`, `src/search.rs`, `src/nl_json.rs`, `src/start_lists.rs` | `query.rs`, `search.rs`, `nl_json.rs`, `start_lists.rs` |
| P2 | w12ui_logic.h (101), w12cal_logic.h (40), w12gesture_logic.h (73), w12dock_logic.h (14), w12lazy.h (44), w12vdfile.h (44), w12tasks_logic.h (196), w12stophosts.h (126), w12diag.h (203) | `src/ui_logic.rs`, `cal_logic.rs`, `gesture_logic.rs`, `dock_logic.rs`, `lazy.rs`, `vdfile.rs`, `tasks_logic.rs`, `stophosts.rs`, `diag.rs` | `ui_logic.rs`, `cal_logic.rs`, `gesture_logic.rs`, `dock_logic.rs`, `lazy.rs`, `tasks_logic.rs`, `stophosts.rs`, `diag_logic.rs` |
| P3 | w12fm_logic.h (318) | `src/fm_logic.rs` | `fm_logic.rs` |
| F1 | w12files.cpp (535), w12files.h (38); global dari win12files.cpp, w12fm_bar.inc, w12fm_search.inc (hanya deklarasi) | `src/files/model.rs`, `src/files/state.rs` | `files.rs` |
| F2 | win12files.cpp (749) | `src/files/app.rs`, `src/bin/win12files/main.rs` | — |
| F3 | w12fm_bar.inc (840), w12fm_search.inc (191) | `src/files/bar.rs`, `src/files/search_ui.rs` | — |
| B | win12boot.c (415), win12guard.c (64) | `src/bin/win12boot.rs`, `src/bin/win12guard.rs` | — |
| SH0 | seluruh shell: win12shell.cpp dan semua .inc (hanya deklarasi, pembagian, dan kontrak) | `src/shell/state.rs`, `src/shell/mod.rs` (dengan placeholder untuk semua file di Tahap B), `docs/port/shell-split.md`, `docs/port/shell-funcs.md` | — |

Aturan tambahan Tahap A:
- P1–P3 mengikuti nama dan urutan parameter di `.h` (lihat kolom sumber). Modul ini dipakai shell, jadi jangan mengubah nama publik.
- F1 menulis `files/state.rs` pertama. F2 dan F3 memakai nama global sesuai konvensi (`g_foo` → `FOO`) dan menyesuaikan saat integrasi.
- B memakai `crate::vdfile` (P2) dan `crate::log`, `crate::reg`. Bila `vdfile.rs` belum selesai, tulis pemanggilan dengan nama yang sama lalu catat di laporan.

## Tahap B (paralel, setelah SH0 selesai)

Pembagian `win12shell.cpp` ke C1–C4 (rentang baris dan nama file) ada di `docs/port/shell-split.md`. Kontrak fungsi ada di `docs/port/shell-funcs.md`.

| Agen | Sumber C++/C | File Rust yang dimiliki |
|---|---|---|
| C1–C4 | win12shell.cpp, sesuai `shell-split.md` | `src/shell/core_*.rs` sesuai `shell-split.md` |
| U1 | w12notch.inc (1269) | `src/shell/notch.rs` |
| U2 | w12start.inc (1460) | `src/shell/start.rs` |
| U3 | w12flyout.inc (906) | `src/shell/flyout.rs` |
| U4 | w12qs.inc (891) | `src/shell/qs.rs` |
| U5 | w12hotkeys.inc (694), w12snap.inc (148), w12winx.inc (115) | `src/shell/hotkeys.rs`, `snap.rs`, `winx.rs` |
| U6 | w12overlay.inc (611), w12tasks.inc (477) | `src/shell/overlay.rs`, `tasks_ui.rs` |
| U7 | w12panel.inc (401), w12widgets.inc (399), w12calendar.inc (112), w12gesture.inc (286) | `src/shell/panel.rs`, `widgets.rs`, `calendar.rs`, `gesture_ui.rs` |
| U8 | w12filesearch.inc (634), w12usb.inc (176), w12nl.inc (163), w12diag.inc (167), w12stophosts.inc (82) | `src/shell/filesearch.rs`, `usb.rs`, `nl_ui.rs`, `diag_ui.rs`, `stophosts_ui.rs` |

Integrator (saya) memiliki `src/bin/win12shell/main.rs`, `src/files/mod.rs`, `src/shell/mod.rs` (setelah SH0), `build.rs`, dan `Cargo.toml`.

## Tahap C (integrasi, dikerjakan integrator)
1. `cargo check --lib` dan `cargo check --bins` sampai bersih, lalu catat jumlah error dan warning.
2. `cargo test` untuk tes parity; bandingkan dengan hasil tes C++ (`tests\*_test.cpp`, dijalankan dengan g++ MSYS2).
3. Laporan paritas: fitur yang sudah diport, yang `todo!()`, dan yang belum diverifikasi (UI D2D, interaksi pengguna, deploy).

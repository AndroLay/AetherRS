//! Pustaka inti Win12 Shell (port Rust dari sumber C++/C di `..\win12-shell`).
//!
//! Aturan penulisan ada di `CONVENTIONS.md`; pemetaan file C++ -> modul Rust
//! ada di `PORT-MAP.md`. Modul di bawah ini diisi bertahap oleh pemilik masing-masing.

#![allow(dead_code, unused_imports, unused_variables, non_snake_case, non_upper_case_globals)]

pub mod sync;
pub mod wide;
pub mod log;
pub mod reg;

// Modul logika murni (tanpa Win32 UI); punya tes paritas di `tests/`.
pub mod query;
pub mod search;
pub mod nl_json;
pub mod start_lists;
pub mod ui_logic;
pub mod cal_logic;
pub mod gesture_logic;
pub mod dock_logic;
pub mod tasks_logic;
pub mod stophosts;
pub mod diag;
pub mod lazy;
pub mod vdfile;
pub mod fm_logic;

// Shell (UI Win32/D2D).
pub mod shell;

// Aplikasi berkas (win12files).
pub mod files;

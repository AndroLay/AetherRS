# Konvensi port Rust — Win12 Shell

Sumber C++/C (baca saja): `C:\Users\andro\Pictures\win12-shell`
Hasil port (satu-satunya tempat tulis): `C:\Users\andro\Pictures\win12-shell-rust`
Kepemilikan file: lihat `PORT-MAP.md`.

## Aturan kerja
1. Satu penulis per file. Tulis hanya file yang tercantum atas nama kamu di `PORT-MAP.md`.
2. Jangan ubah `Cargo.toml`, `src/lib.rs`, `src/shell/mod.rs`, `src/files/mod.rs`, `PORT-MAP.md`, `CONVENTIONS.md`. Bila butuh fitur crate `windows` yang belum ada atau modul baru, tulis di laporan akhir.
3. Jangan sentuh folder sumber. Jangan jalankan `.exe`, skrip `.ps1`, atau `cargo run`. Jangan tulis `HKCU\...\Winlogon\Shell`. Jangan buat service atau scheduled task. Jangan commit/push. Jangan hapus file.
4. Verifikasi: `cargo check --lib` dari `C:\Users\andro\Pictures\win12-shell-rust` (PATH perlu `$HOME/.cargo/bin` dan `/c/msys64/ucrt64/bin`). Error di file milik orang lain bukan tanggunganmu; saring keluaran dengan `grep` ke nama file kamu.
5. Keterbatasan Haiku: Read maksimal ±200 baris per panggilan (pakai offset/limit); maksimal satu Read atau perintah berkeluaran besar per giliran; jangan pakai `allow_large`; tulis maksimal 1–2 file per respons; satu Write maksimal ±300 baris. File lebih besar: Write kerangka dengan penanda `// @@LANJUT@@` di baris terakhir, lalu Edit yang mengganti penanda itu dengan bagian berikutnya dan penanda baru. Setelah selesai, hapus semua penanda.
6. Tes: hanya port tes C++ yang ditugaskan (`tests\*_test.cpp` → `tests/<nama>.rs`, memakai `w12::...`). Jalankan `cargo test --test <nama>` untuk file tes kamu.
7. Laporan akhir maksimal 25 baris: file yang ditulis; perkiraan baris C++ yang diport; bagian yang dilewati atau `todo!()` beserta alasan; hasil `cargo check` untuk file kamu; permintaan (fitur crate, modul, perubahan kontrak).

## Crate `windows` 0.61.3
Sumber crate untuk cek signature: `C:\Users\andro\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\windows-0.61.3\src\`. Cari dengan `grep -rn "pub unsafe fn NamaFungsi" <path>` dan batasi keluaran dengan `head`.

Jebakan yang sudah terbukti:
- Parameter opsional (NULL) bertipe `Option<T>`. Contoh: `RegOpenKeyExW(..., uloptions: Option<u32>, ...)` → `Some(0)`.
- Parameter `PCWSTR`: pakai `crate::wide::pcw(&buf)`; `buf` berasal dari `to_wide_z`, dan harus hidup selama pemanggilan.
- `WIN32_ERROR` dibandingkan dengan `ERROR_SUCCESS` (`==`), bukan dengan `0`.
- Hampir semua fungsi Win32 `unsafe`. Bungkus pemanggilan dengan `unsafe { }`.
- Newtype (`HWND`, `HKEY`, `WPARAM`, `LPARAM`, `POINT`) memakai field `.0` untuk nilai mentah.

## Pemetaan tipe C/C++ → Rust
| C/C++ | Rust |
|---|---|
| `int`, `LONG` | `i32` |
| `unsigned`, `DWORD`, `UINT` | `u32` |
| `size_t` | `usize` |
| `__int64`, `long long` | `i64` |
| `unsigned long long` | `u64` |
| `BOOL`, `bool` | `bool` |
| `float`, `double` | `f32`, `f64` |
| `wchar_t`, `WCHAR` (elemen buffer) | `u16` |
| `const wchar_t *s` (input) | `&str`; ubah dengan `to_wide_z` saat memanggil Win32 |
| `wchar_t *buf, int cch` (output) | `&mut [u16]` |
| `HWND`, `HDC`, `HFONT`, dll. | tipe `windows::Win32::...` yang sama |
| `RECT`, `POINT`, `SIZE` | `RECT`, `POINT`, `SIZE` dari crate `windows` (jangan buat struct sendiri) |
| `T *p` yang boleh NULL | `Option<&T>` atau `Option<&mut T>` |
| `WNDPROC` | `unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT` |
| struct C | `struct` dengan field snake_case; `#[derive(Clone, Copy, Default)]` bila cocok |
| enum C | `#[derive(Clone, Copy, PartialEq, Eq)] enum`, atau `const` `i32` bila dipakai sebagai angka |

## Penamaan
- Fungsi: C `PascalCase` → Rust `snake_case` dengan urutan parameter sama. Contoh: `DiagLog` → `diag_log`, `OurDword` → `our_dword`. Bila sudah ada di `crate::log` atau `crate::reg`, pakai itu.
- Konstanta: `#define X 12` atau `static const int kX` → `pub const X: i32 = 12;` (UPPER_SNAKE, awalan `k` dihapus).
- Global `g_camelCase` → UPPER_SNAKE. Contoh: `g_hwndMain` → `HWND_MAIN`. Global shell dideklarasikan di `shell::state`; global aplikasi berkas di `files::state`. Global yang hanya dipakai satu file tetap di file itu sebagai `static` privat.
- Nama tipe Windows dipakai langsung, tanpa alias.

## Global (thread UI tunggal)
- Nilai `Copy`: `pub static NAMA: SyncCell<Cell<T>> = SyncCell::new(Cell::new(nilai_awal));`
- Vec, String, atau struct: `pub static NAMA: SyncCell<RefCell<T>> = SyncCell::new(RefCell::new(nilai_awal));`
- Contoh HWND: `pub static HWND_MAIN: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));`
- Akses: `HWND_MAIN.get()` dan `HWND_MAIN.set(h)`. Untuk RefCell: `ITEMS.borrow_mut().push(x)`.
- Jangan memegang `borrow()`/`borrow_mut()` melewati pemanggilan yang bisa mengirim pesan (SendMessage, DispatchMessage, MessageBox, timer). Salin data dulu, lepas borrow, baru panggil.

## Log dan registry
- `crate::log::log(&str)` menggantikan `DiagLog` dan `Log`. Format dan lokasi boot.log sama dengan C++. Untuk string terformat, pakai `crate::log::log(&format!(...))`.
- `crate::reg::{get_dword, set_dword, get_string, set_string, delete_value, our_dword, set_our_dword}`.
- Jangan pernah menulis `Winlogon\Shell`.

## Penanganan error
- Jangan `unwrap()`/`expect()` pada pemanggilan Win32 yang bisa gagal saat runtime. Pakai `if`, `match`, atau `let _ =`.
- Cabang C yang mengembalikan 0, FALSE, atau NULL menjadi `None` atau `false` dengan perilaku yang sama.

## Dokumentasi di file
- Setiap file diawali `//!` satu sampai tiga baris (bahasa Indonesia): sumber C++ (file:baris) dan status (lengkap, atau sebagian beserta alasan).

//! Port `w12lazy.h` (baris 1–44): DLL dimuat saat dipakai, bukan impor statis (Fase 4 Task 3). Status: lengkap.
//! `W12_LAZY` dan `W12_LAZY_VOID` menjadi makro `w12_lazy!` dan `w12_lazy_void!`. Tipe fungsi ditulis eksplisit
//! (Rust tidak punya `decltype`), dan nama pembungkus diberikan pemanggil (`macro_rules!` tidak menggabungkan identifier).

use std::cell::Cell;

use windows::core::PCSTR;
use windows::Win32::Foundation::{FARPROC, HMODULE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

use crate::wide::{pcw, to_wide_z};

/// Memuat DLL sekali dan menyimpan handle di `slot`. Gagal -> `None`; panggilan berikutnya mencoba lagi.
/// Port `W12LazyLoad`.
pub fn w12_lazy_load(slot: &Cell<HMODULE>, dll: &str) -> Option<HMODULE> {
    let m = slot.get();
    if !m.0.is_null() {
        return Some(m);
    }
    let w = to_wide_z(dll);
    let m = unsafe { LoadLibraryExW(pcw(&w), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }.ok()?;
    slot.set(m);
    Some(m)
}

/// Alamat ekspor `name` (ANSI) dari `dll`; DLL dimuat ke `slot` bila belum. `None` bila DLL atau ekspor tidak ada.
pub fn w12_lazy_proc(slot: &Cell<HMODULE>, dll: &str, name: &str) -> FARPROC {
    let m = w12_lazy_load(slot, dll)?;
    let mut n = name.as_bytes().to_vec();
    n.push(0);
    unsafe { GetProcAddress(m, PCSTR(n.as_ptr())) }
}

/// Pengganti `W12_LAZY`: membuat `$vis unsafe fn $wrap(...) -> $ret` yang memuat `$dll` ke `$slot` pada panggilan
/// pertama, lalu memanggil ekspor `$name`. Bila DLL atau ekspor tidak ada, wrapper mengembalikan `$fail`.
/// `$slot`: `static SLOT: SyncCell<Cell<HMODULE>>` milik pemanggil (nilai awal `HMODULE(null)`).
/// Penunjuk fungsi di-cache di `static` lokal wrapper; yang gagal dicoba lagi pada panggilan berikutnya.
#[macro_export]
macro_rules! w12_lazy {
    ($slot:path, $dll:expr, $name:literal, $vis:vis fn $wrap:ident($($a:ident : $t:ty),*) -> $ret:ty, $fail:expr) => {
        $vis unsafe fn $wrap($($a: $t),*) -> $ret {
            type P = unsafe extern "system" fn($($t),*) -> $ret;
            static PROC: $crate::sync::SyncCell<::std::cell::Cell<Option<P>>> =
                $crate::sync::SyncCell::new(::std::cell::Cell::new(None));
            if PROC.get().is_none() {
                if let Some(f) = $crate::lazy::w12_lazy_proc(&$slot, $dll, $name) {
                    PROC.set(Some(::std::mem::transmute::<unsafe extern "system" fn() -> isize, P>(f)));
                }
            }
            match PROC.get() {
                Some(f) => f($($a),*),
                None => $fail,
            }
        }
    };
}

/// Pengganti `W12_LAZY_VOID`: sama seperti `w12_lazy!` untuk fungsi tanpa nilai kembali.
/// Bila gagal, tidak ada yang dikembalikan dan argumen tidak diteruskan.
#[macro_export]
macro_rules! w12_lazy_void {
    ($slot:path, $dll:expr, $name:literal, $vis:vis fn $wrap:ident($($a:ident : $t:ty),*)) => {
        $crate::w12_lazy!($slot, $dll, $name, $vis fn $wrap($($a: $t),*) -> (), ());
    };
}

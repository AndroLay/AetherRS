//! Port `tests\w12lazy_test.cpp` (baris 1–43): uji `w12_lazy!` dan `w12_lazy_void!` dari `src\lazy.rs`. Status: lengkap.
//! Sembilan `CHECK` sumber (enam perilaku) semuanya diport ke `check!`. Hanya membaca PID sendiri dan last-error thread.

use std::cell::Cell;

use windows::Win32::Foundation::{GetLastError, HMODULE};
use windows::Win32::System::Threading::GetCurrentProcessId;

use w12::sync::SyncCell;
use w12::{w12_lazy, w12_lazy_void};

// Slot DLL per pembungkus (sumber: `static` di w12lazy_test.cpp).
static G_HK32: SyncCell<Cell<HMODULE>> = SyncCell::new(Cell::new(HMODULE(std::ptr::null_mut())));
static G_HMISSING: SyncCell<Cell<HMODULE>> = SyncCell::new(Cell::new(HMODULE(std::ptr::null_mut())));
static G_HK32MISS: SyncCell<Cell<HMODULE>> = SyncCell::new(Cell::new(HMODULE(std::ptr::null_mut())));
static G_HK32VOID: SyncCell<Cell<HMODULE>> = SyncCell::new(Cell::new(HMODULE(std::ptr::null_mut())));

w12_lazy!(G_HK32, "kernel32.dll", "GetCurrentProcessId", fn get_current_process_id_l() -> u32, 0);
w12_lazy!(G_HMISSING, "w12_tidak_ada_xyz.dll", "W12MissingDllFn", fn w12_missing_dll_fn_l() -> u32, 77);
w12_lazy!(G_HK32MISS, "kernel32.dll", "W12NoSuchExport", fn w12_no_such_export_l() -> u32, 99);
w12_lazy_void!(G_HK32VOID, "kernel32.dll", "SetLastError", fn set_last_error_l(kode: u32));

#[test]
fn lazy_semua_uji() {
    let mut gagal = 0u32;
    macro_rules! check {
        ($c:expr) => {
            if !($c) {
                eprintln!("FAIL baris {}: {}", line!(), stringify!($c));
                gagal += 1;
            }
        };
    }

    // belum dimuat sebelum panggilan pertama
    check!(G_HK32.get().0.is_null());
    // DLL dimuat pada panggilan pertama
    check!(unsafe { get_current_process_id_l() } == unsafe { GetCurrentProcessId() });
    check!(!G_HK32.get().0.is_null());
    // panggilan kedua memakai penunjuk yang sudah ada
    check!(unsafe { get_current_process_id_l() } == unsafe { GetCurrentProcessId() });

    // DLL tidak ada: nilai gagal
    check!(unsafe { w12_missing_dll_fn_l() } == 77);
    check!(G_HMISSING.get().0.is_null());
    // fungsi tidak ada: nilai gagal
    check!(unsafe { w12_no_such_export_l() } == 99);
    check!(!G_HK32MISS.get().0.is_null()); // DLL-nya tetap dimuat

    // varian void meneruskan argumen
    unsafe { set_last_error_l(1234) };
    check!(unsafe { GetLastError() }.0 == 1234);

    assert_eq!(gagal, 0, "w12lazy_test: {gagal} gagal");
}

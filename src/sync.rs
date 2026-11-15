//! Pembungkus global statis untuk thread UI tunggal.
//!
//! Sumber C++ memakai global biasa yang hanya disentuh dari thread UI (message loop).
//! `SyncCell<T>` hanya memberi tanda `Sync` agar global seperti itu bisa dideklarasikan
//! sebagai `static`. Isi di dalamnya WAJIB berupa `Cell`/`RefCell` (atau tipe aman
//! interior-mutability lain), sehingga akses berulang (mis. callback yang masuk lagi
//! saat SendMessage) tidak menjadi UB. Pelanggaran reentrancy pada `RefCell` akan panik,
//! bukan korupsi memori.
//!
//! Contoh:
//! ```ignore
//! static MAIN_WND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
//! static ITEMS: SyncCell<RefCell<Vec<String>>> = SyncCell::new(RefCell::new(Vec::new()));
//! ```

use std::ops::Deref;

pub struct SyncCell<T>(T);

// SAFETY: global ini hanya diakses dari thread UI yang sama, seperti global C++ di sumber.
// Tidak ada data yang dibagi lintas thread lewat tipe ini.
unsafe impl<T> Sync for SyncCell<T> {}

impl<T> SyncCell<T> {
    pub const fn new(value: T) -> Self {
        SyncCell(value)
    }
}

impl<T> Deref for SyncCell<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

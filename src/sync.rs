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

use std::ffi::c_void;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, Ordering};

use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND};
use windows::Win32::UI::WindowsAndMessaging::HHOOK;

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

/// Flag boolean lintas-thread berbasis `AtomicBool` dengan API `.get()`/`.set()` yang sama
/// seperti `Cell<bool>`, untuk global yang juga dibaca/ditulis thread pekerja atau hook.
pub struct AtomicFlag(AtomicBool);

impl AtomicFlag {
    pub const fn new(value: bool) -> Self {
        Self(AtomicBool::new(value))
    }

    #[inline]
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    #[inline]
    pub fn set(&self, value: bool) {
        self.0.store(value, Ordering::Release);
    }
}

/// Nilai `i32` lintas-thread berbasis `AtomicI32` dengan API `.get()`/`.set()`.
pub struct AtomicI32Cell(AtomicI32);

impl AtomicI32Cell {
    pub const fn new(value: i32) -> Self {
        Self(AtomicI32::new(value))
    }

    #[inline]
    pub fn get(&self) -> i32 {
        self.0.load(Ordering::Acquire)
    }

    #[inline]
    pub fn set(&self, value: i32) {
        self.0.store(value, Ordering::Release);
    }
}

/// Nilai `u32` lintas-thread berbasis `AtomicU32` dengan API `.get()`/`.set()`.
pub struct AtomicU32Cell(AtomicU32);

impl AtomicU32Cell {
    pub const fn new(value: u32) -> Self {
        Self(AtomicU32::new(value))
    }

    #[inline]
    pub fn get(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    #[inline]
    pub fn set(&self, value: u32) {
        self.0.store(value, Ordering::Release);
    }
}

/// Handle Win32 berbasis pointer yang bisa disimpan secara atomik sebagai `isize`.
pub trait PtrHandle: Copy {
    fn to_isize(self) -> isize;
    fn from_isize(v: isize) -> Self;
}

impl PtrHandle for HWND {
    #[inline]
    fn to_isize(self) -> isize {
        self.0 as isize
    }
    #[inline]
    fn from_isize(v: isize) -> Self {
        HWND(v as *mut c_void)
    }
}

impl PtrHandle for HINSTANCE {
    #[inline]
    fn to_isize(self) -> isize {
        self.0 as isize
    }
    #[inline]
    fn from_isize(v: isize) -> Self {
        HINSTANCE(v as *mut c_void)
    }
}

impl PtrHandle for HHOOK {
    #[inline]
    fn to_isize(self) -> isize {
        self.0 as isize
    }
    #[inline]
    fn from_isize(v: isize) -> Self {
        HHOOK(v as *mut c_void)
    }
}

impl PtrHandle for HANDLE {
    #[inline]
    fn to_isize(self) -> isize {
        self.0 as isize
    }
    #[inline]
    fn from_isize(v: isize) -> Self {
        HANDLE(v as *mut c_void)
    }
}

/// Handle Win32 lintas-thread berbasis `AtomicIsize` dengan API `.get()`/`.set()`.
pub struct AtomicHandle<T: PtrHandle>(AtomicIsize, PhantomData<fn() -> T>);

impl<T: PtrHandle> AtomicHandle<T> {
    pub const fn null() -> Self {
        Self(AtomicIsize::new(0), PhantomData)
    }

    #[inline]
    pub fn get(&self) -> T {
        T::from_isize(self.0.load(Ordering::Acquire))
    }

    #[inline]
    pub fn set(&self, value: T) {
        self.0.store(value.to_isize(), Ordering::Release);
    }
}

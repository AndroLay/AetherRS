//! Global aplikasi berkas (hanya deklarasi). Sumber: win12files.cpp:23-74, w12fm_bar.inc:7-23, w12fm_search.inc:6-28.
//! Status: lengkap untuk deklarasi. Fungsi ada di app.rs, bar.rs, search_ui.rs. Tipe fm_logic dari P3, W12FxView dari model.rs.
//! Nama di file ini dipakai F2 dan F3. Jangan diganti tanpa koordinasi.

use std::cell::{Cell, RefCell};
use std::sync::Mutex;

use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, MAX_PATH};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::IExplorerBrowser;
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use crate::files::model::W12FxView;
use crate::fm_logic::{W12FmPalette, W12FmSort, W12Hist, W12Tabs, W12FC_COUNT, W12FM_MAXTABS};
use crate::sync::SyncCell;

// ---- win12files.cpp:23-28 ----

/// kClassFrame. Pakai dengan to_wide_z saat memanggil Win32.
pub const CLASS_FRAME: &str = "Win12Files";
/// kClassMsg.
pub const CLASS_MSG: &str = "Win12FilesMsg";
/// kMutexName.
pub const MUTEX_NAME: &str = "Win12FilesSingleInstance";
/// kHomeAddress.
pub const HOME_ADDRESS: &str = "shell:::{f874310e-b6b7-47dc-bc84-b9e6b38f5903}";
/// kCopyTag (ULONG_PTR, dipetakan ke usize).
pub const COPY_TAG: usize = 0x5731_4631;

pub const FM_MAX_WINDOWS: usize = 16;
pub const FM_TABH_DIP: i32 = 36;
pub const FM_NAVH_DIP: i32 = 40;
pub const FM_CMDH_DIP: i32 = 40;
pub const FM_HISTMAX: usize = 50;

// Alias PIDL. Tidak ada di crate windows 0.61.3 (shtypes.h). Tipe dasarnya ITEMIDLIST.
#[allow(non_camel_case_types)]
pub type PIDLIST_ABSOLUTE = *mut ITEMIDLIST;
#[allow(non_camel_case_types)]
pub type PCIDLIST_ABSOLUTE = *const ITEMIDLIST;
#[allow(non_camel_case_types)]
pub type LPITEMIDLIST = *mut ITEMIDLIST;

// ---- win12files.cpp:37-49 ----

/// Satu tab = satu IExplorerBrowser. `eb` memegang referensi COM. Drop melepasnya (pengganti Release).
pub struct FmTab {
    pub eb: Option<IExplorerBrowser>,
    pub cookie: u32,
    pub title: [u16; 128],
    /// Folder aktif (CoTaskMemAlloc).
    pub cur: PIDLIST_ABSOLUTE,
    /// Item yang dipilih setelah navigasi ke induk (/select,).
    pub pending: LPITEMIDLIST,
    pub hist: W12Hist,
    pub hpidl: [PIDLIST_ABSOLUTE; FM_HISTMAX],
    pub hist_pending: bool,
    pub hist_from: i32,
    pub sort: W12FmSort,
    /// Isi tab sekarang hasil pencarian (cur tetap folder asal).
    pub searching: bool,
}

// ---- win12files.cpp:52-54 ----

pub const FM_HIT_NONE: i32 = 0;
pub const FM_HIT_TAB: i32 = 1;
pub const FM_HIT_TABCLOSE: i32 = 2;
pub const FM_HIT_PLUS: i32 = 3;
pub const FM_HIT_NAV: i32 = 4;
pub const FM_HIT_CMD: i32 = 5;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FmHit {
    pub kind: i32,
    pub index: i32,
}

// ---- win12files.cpp:55-72 ----

pub struct FmWin {
    pub h: HWND,
    pub addr: HWND,
    pub search: HWND,
    pub tabs: W12Tabs,
    pub tab: [FmTab; W12FM_MAXTABS],
    pub status: [u16; 256],
    pub status_error: bool,
    /// Tooltip bilah.
    pub tip: HWND,
    /// Warna tema aktif.
    pub pal: W12FmPalette,
    pub dark: bool,
    pub accent: COLORREF,
    /// Latar kotak edit.
    pub edit_brush: HBRUSH,
    /// Sorotan kursor dan tombol yang ditekan.
    pub hot: FmHit,
    pub press: FmHit,
    /// Tab sedang diseret.
    pub dragging: bool,
    pub drag_tab: i32,
    /// TrackMouseEvent(TME_LEAVE) aktif.
    pub tracking: bool,
    /// Ada item terpilih di tampilan aktif.
    pub has_sel: bool,
    /// Nomor pencarian terakhir. Hasil dengan nomor lain dibuang.
    pub search_gen: u32,
}

// ---- win12files.cpp:74 (g_wins) ----

/// Pointer FmWin hasil Box::into_raw. Null = slot kosong. Array memakai RefCell, bukan Cell.
pub static WINS: SyncCell<RefCell<[*mut FmWin; FM_MAX_WINDOWS]>> =
    SyncCell::new(RefCell::new([std::ptr::null_mut(); FM_MAX_WINDOWS]));

// ---- w12fm_bar.inc:7-23 ----

/// FM_TIMER_SEL: id timer sinkronisasi status pilihan (300 ms). Tipe usize sesuai nIDEvent.
pub const FM_TIMER_SEL: usize = 1;
/// FM_TOOL_ID: id alat tooltip (satu alat untuk seluruh klien). Tipe usize sesuai TTTOOLINFOW.uId.
pub const FM_TOOL_ID: usize = 1;

pub const CMD_DIP: [i32; W12FC_COUNT] = [96, 64, 64, 64, 88, 64, 88, 96];
pub const CMD_LABEL: [&str; W12FC_COUNT] = [
    "Baru folder",
    "Potong",
    "Salin",
    "Tempel",
    "Ganti nama",
    "Hapus",
    "Urutkan",
    "Tampilan",
];
pub const CMD_TIP: [&str; W12FC_COUNT] = [
    "Buat folder baru",
    "Potong (Ctrl+X)",
    "Salin (Ctrl+C)",
    "Tempel (Ctrl+V)",
    "Ganti nama (F2)",
    "Hapus (Del)",
    "Urutkan menurut kolom",
    "Ubah tampilan",
];
pub const NAV_TIP: [&str; 4] = ["Kembali", "Maju", "Naik ke folder induk", "Segarkan"];
pub const SORT_LABEL: [&str; 4] = ["Nama", "Tanggal diubah", "Ukuran", "Jenis"];
pub const VIEW_LABEL: [&str; 5] = ["Detail", "Daftar", "Ikon sedang", "Ikon besar", "Ubin"];

/// Geometri bilah dalam piksel untuk DPI jendela saat ini (selaras dengan FmLayout).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FmGeo {
    pub tab_h: i32,
    pub nav_top: i32,
    pub nav_h: i32,
    pub cmd_top: i32,
    pub cmd_h: i32,
    pub plus_w: i32,
    pub nav_btn_w: i32,
    pub close_size: i32,
    pub close_pad: i32,
    pub pad: i32,
}

// ---- w12fm_search.inc:6-12 ----

/// kMsgSearchDone: WM_APP + 0x31. Hasil pencarian selesai dikirim ke thread UI.
pub const MSG_SEARCH_DONE: u32 = WM_APP + 0x31;
pub const FM_SEARCH_SCAN: usize = 4000;
pub const FM_SEARCH_MAX: usize = 500;
pub const FM_SEARCH_QUERY: usize = 256;
pub const FM_SEARCH_PATH: usize = 1024;

/// g_fxCs. Pengganti CRITICAL_SECTION (fitur Win32_System_Kernel kini aktif di Cargo.toml; tetap memakai Mutex std).
/// Semua akses ke FX_VIEW, FX_OPEN, dan FX_PATH wajib di bawah `FX_CS.lock()`.
pub static FX_CS: Mutex<()> = Mutex::new(());
/// g_fxView. Nol-inisialisasi sama dengan global C++.
pub static FX_VIEW: SyncCell<RefCell<W12FxView>> = SyncCell::new(RefCell::new(W12FxView {
    file: HANDLE(std::ptr::null_mut()),
    map: HANDLE(std::ptr::null_mut()),
    h: std::ptr::null(),
    e: std::ptr::null(),
    pool: std::ptr::null(),
}));
/// g_fxOpen.
pub static FX_OPEN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
/// g_fxPath (MAX_PATH wchar).
pub static FX_PATH: SyncCell<RefCell<[u16; MAX_PATH as usize]>> =
    SyncCell::new(RefCell::new([0u16; MAX_PATH as usize]));

// ---- w12fm_search.inc:14-28 ----

/// FmSearchJob: diserahkan ke thread pekerja.
#[derive(Clone, Copy)]
pub struct FmSearchJob {
    pub hwnd: HWND,
    pub gen: u32,
    pub tab_id: i32,
    /// Teks kueri.
    pub query: [u16; FM_SEARCH_QUERY],
    /// Jalur folder tab saat Enter ditekan. Kosong = tanpa filter.
    pub folder: [u16; MAX_PATH as usize],
}

/// FmSearchResult: hasil yang dikirim sebagai PIDL. Setiap entri CoTaskMemAlloc.
/// Penyimpangan: field `n` (C) dihapus. Jumlahnya sama dengan `pidl.len()`.
pub struct FmSearchResult {
    pub gen: u32,
    pub tab_id: i32,
    pub indexed: bool,
    /// Daftar PIDL hasil (C: array calloc FM_SEARCH_MAX, n = jumlah terisi).
    pub pidl: Vec<PCIDLIST_ABSOLUTE>,
}

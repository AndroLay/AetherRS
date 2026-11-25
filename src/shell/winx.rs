//! Sumber: w12winx.inc (seluruh file, 115 baris). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: T10.
//! Deviasi: `WinXMenu(owner)` menjadi `winx_menu(Option<HWND>)` (None = Win+X, pakai `winx_start_owner`). `WinXOpen` mengembalikan `isize` (INT_PTR).
use super::prelude::*;
use windows::Win32::UI::Shell::ShellExecuteW;

// Sumber: enum di w12winx.inc baris 6–10 (nilai sama dengan C).
pub const WX_APPS: i32 = 1;
pub const WX_POWER_OPTS: i32 = 2;
pub const WX_EVENTS: i32 = 3;
pub const WX_SYSTEM: i32 = 4;
pub const WX_DEVMGR: i32 = 5;
pub const WX_NETWORK: i32 = 6;
pub const WX_DISK: i32 = 7;
pub const WX_COMPMGMT: i32 = 8;
pub const WX_TERMINAL: i32 = 9;
pub const WX_TERMINAL_ADMIN: i32 = 10;
pub const WX_TASKMGR: i32 = 11;
pub const WX_SETTINGS: i32 = 12;
pub const WX_EXPLORER: i32 = 13;
pub const WX_SEARCH: i32 = 14;
pub const WX_RUN: i32 = 15;
pub const WX_DESKTOP: i32 = 16;
pub const WX_SWITCH: i32 = 17;
/// WX_PWR_BASE + kode StartPowerDo: 1 Tidur, 2 Mulai ulang, 3 Matikan, 4 Keluar.
pub const WX_PWR_BASE: i32 = 100;

/// `WinXOpen` (w12winx.inc 14–18). ShellExecuteW dengan hasil isize (di atas 32 berarti berhasil).
/// `ms-settings:` tanpa verb lewat `shell_open` (tanpa Explorer), hasilnya 33.
fn winx_open(file: &str, verb: Option<&str>) -> isize {
    if verb.is_none() && file.get(..12).map_or(false, |p| p.eq_ignore_ascii_case("ms-settings:")) {
        shell_open(file);
        return 33;
    }
    let f = to_wide_z(file);
    let v = verb.map(to_wide_z);
    let vp = match &v {
        Some(w) => pcw(w),
        None => PCWSTR::null(),
    };
    // SAFETY: f dan v hidup sampai pemanggilan selesai; string berakhir NUL.
    unsafe { ShellExecuteW(None, vp, pcw(&f), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL).0 as isize }
}

/// `WinXTerminal` (w12winx.inc 22–26). Hanya wt.exe. PORT: fallback powershell.exe (C++) dihapus, shell tidak
/// menjalankan PowerShell. Bila wt.exe tidak ada, item ini tidak membuka apa pun.
fn winx_terminal(verb: Option<&str>) {
    let _ = winx_open("wt.exe", verb);
}

/// `WinXRun` (w12winx.inc 28–50).
fn winx_run(cmd: i32) {
    if cmd > WX_PWR_BASE && cmd <= WX_PWR_BASE + 4 {
        start_power_do(cmd - WX_PWR_BASE);
        return;
    }
    match cmd {
        WX_APPS => {
            winx_open("ms-settings:appsfeatures", None);
        }
        WX_POWER_OPTS => {
            winx_open("ms-settings:powersleep", None);
        }
        WX_EVENTS => {
            winx_open("eventvwr.msc", None);
        }
        WX_SYSTEM => {
            winx_open("ms-settings:about", None);
        }
        WX_DEVMGR => {
            winx_open("devmgmt.msc", None);
        }
        WX_NETWORK => {
            winx_open("ms-settings:network", None);
        }
        WX_DISK => {
            winx_open("diskmgmt.msc", None);
        }
        WX_COMPMGMT => {
            winx_open("compmgmt.msc", None);
        }
        WX_TERMINAL => winx_terminal(None),
        WX_TERMINAL_ADMIN => winx_terminal(Some("runas")),
        WX_TASKMGR => {
            winx_open("taskmgr.exe", None);
        }
        WX_SETTINGS => {
            winx_open("ms-settings:", None);
        }
        WX_EXPLORER => {
            // Sama dengan Win+E: win12files bila ada. explorer.exe langsung dapat menyalakan shell Explorer di mode shell.
            super::core_c3b::open_folder_window("");
        }
        WX_SEARCH => {
            // Sama dengan Win+S. SAFETY: PostMessageW ke jendela shell dengan WPARAM bilangan bulat biasa.
            let _ = unsafe {
                PostMessageW(
                    Some(MSG.get()),
                    WM_WINKEY,
                    WPARAM(W12_STARTKEY_SEARCH as usize),
                    LPARAM(0),
                )
            };
        }
        WX_RUN => shell_key(b'R' as i32),
        WX_DESKTOP => show_desktop_toggle(),
        WX_SWITCH => shell_switch_to_windows(None),
        _ => {}
    }
}

/// `WinXAnchor` (w12winx.inc 53–66). Tepi kiri dan tepi atas pil tombol Start yang tampil;
/// tanpa tombol Start: sudut kiri bawah area kerja.
fn winx_anchor() -> POINT {
    {
        let n = NITEMS.get().max(0) as usize;
        let items = ITEMS.borrow();
        let pills = PILLS.borrow();
        for it in items.iter().take(n) {
            if it.ty == IT_START && it.pill >= 0 && it.pill < NBOTTOM {
                if let Some(pl) = pills.get(it.pill as usize) {
                    if pl.shown {
                        return POINT {
                            x: pl.x + it.x as i32,
                            y: pl.y,
                        };
                    }
                }
            }
        }
    }
    let mut work = RECT::default();
    // SAFETY: work adalah RECT yang hidup selama pemanggilan (SPI_GETWORKAREA mengisi RECT).
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work as *mut RECT as *mut std::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    POINT {
        x: work.left,
        y: work.bottom,
    }
}

/// `WinXStartOwner` (w12winx.inc 70–78). Pemilik menu Win+X: HWND pil tombol Start bila tampil;
/// jika tidak, `MSG`. `MSG` tidak pernah ditampilkan, jadi SetForegroundWindow padanya gagal.
/// Dipakai juga oleh snap.rs (WAJIB persis nama dan signature ini).
pub fn winx_start_owner() -> HWND {
    {
        let n = NITEMS.get().max(0) as usize;
        let items = ITEMS.borrow();
        let pills = PILLS.borrow();
        for it in items.iter().take(n) {
            if it.ty == IT_START && it.pill >= 0 && it.pill < NBOTTOM {
                if let Some(pl) = pills.get(it.pill as usize) {
                    if pl.shown && !pl.h.0.is_null() {
                        return pl.h;
                    }
                }
            }
        }
    }
    MSG.get()
}

/// Tambah satu item teks ke menu (AppendMenuW, teks UTF-16 berakhir NUL).
fn winx_item(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, text: &str) {
    let w = to_wide_z(text);
    // SAFETY: menu adalah handle menu yang valid; w hidup sampai pemanggilan selesai.
    let _ = unsafe { AppendMenuW(menu, flags, id, pcw(&w)) };
}

/// Tambah pemisah ke menu.
fn winx_sep(menu: HMENU) {
    // SAFETY: menu adalah handle menu yang valid dari CreatePopupMenu.
    let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) };
}

/// `WinXMenu` (w12winx.inc 82–115). `owner`: jendela yang diklik (klik kanan tombol Start), atau None
/// untuk Win+X (dipakai `winx_start_owner`). Urutan dan perintah sama dengan C.
pub fn winx_menu(owner: Option<HWND>) {
    let owner = owner.unwrap_or_else(winx_start_owner);
    // SAFETY: m dan pw dibuat di sini; m dihancurkan di bawah (ikut menghancurkan submenu pw).
    let m = match unsafe { CreatePopupMenu() } {
        Ok(h) => h,
        Err(_) => return,
    };
    let pw = match unsafe { CreatePopupMenu() } {
        Ok(h) => h,
        Err(_) => {
            // SAFETY: m adalah handle menu yang baru dibuat dan belum dipakai.
            let _ = unsafe { DestroyMenu(m) };
            return;
        }
    };
    winx_item(pw, MF_STRING, (WX_PWR_BASE + 4) as usize, "Keluar");
    winx_item(pw, MF_STRING, (WX_PWR_BASE + 1) as usize, "Tidur");
    winx_item(pw, MF_STRING, (WX_PWR_BASE + 2) as usize, "Mulai ulang");
    winx_item(pw, MF_STRING, (WX_PWR_BASE + 3) as usize, "Matikan");
    winx_item(m, MF_STRING, WX_APPS as usize, "Aplikasi terinstal");
    winx_item(m, MF_STRING, WX_POWER_OPTS as usize, "Opsi daya");
    winx_item(m, MF_STRING, WX_EVENTS as usize, "Penampil Peristiwa");
    winx_item(m, MF_STRING, WX_SYSTEM as usize, "Sistem");
    winx_item(m, MF_STRING, WX_DEVMGR as usize, "Pengelola Perangkat");
    winx_item(m, MF_STRING, WX_NETWORK as usize, "Koneksi jaringan");
    winx_item(m, MF_STRING, WX_DISK as usize, "Manajemen Disk");
    winx_item(m, MF_STRING, WX_COMPMGMT as usize, "Manajemen Komputer");
    winx_sep(m);
    winx_item(m, MF_STRING, WX_TERMINAL as usize, "Terminal");
    winx_item(m, MF_STRING, WX_TERMINAL_ADMIN as usize, "Terminal (Admin)");
    winx_sep(m);
    winx_item(m, MF_STRING, WX_TASKMGR as usize, "Pengelola Tugas");
    winx_item(m, MF_STRING, WX_SETTINGS as usize, "Pengaturan");
    winx_item(m, MF_STRING, WX_EXPLORER as usize, "Win12 Explorer");
    winx_item(m, MF_STRING, WX_SEARCH as usize, "Cari");
    winx_item(m, MF_STRING, WX_RUN as usize, "Jalankan");
    winx_sep(m);
    // MF_POPUP: id berisi handle submenu daya.
    winx_item(m, MF_POPUP, pw.0 as usize, "Matikan atau keluar");
    winx_item(m, MF_STRING, WX_DESKTOP as usize, "Desktop");
    winx_sep(m);
    winx_item(m, MF_STRING, WX_SWITCH as usize, "Beralih ke Windows biasa");
    let cmd = track_menu(m, owner, winx_anchor(), false);
    // SAFETY: m masih handle menu yang valid; dihancurkan sekali di sini.
    let _ = unsafe { DestroyMenu(m) };
    winx_run(cmd);
}

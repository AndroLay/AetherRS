//! Port `win12shell.cpp:3338-3960` (SetMode s.d. UpdateForeground).
//! Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H02.
//! ShellModeStart/Stop dipanggil dari core_c4b (startup dan keluar). Autostart dipanggil dari menu bila bukan mode shell.
//! Tidak ada penulisan `Winlogon\Shell` di rentang ini. ShellModeConfigured hanya membaca.

use super::prelude::*;

use std::ffi::c_void;
use std::mem::{size_of, zeroed};

use windows::core::{BSTR, PCSTR};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Storage::FileSystem::{GetFileAttributesW, INVALID_FILE_ATTRIBUTES};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY};
use windows::Win32::System::TaskScheduler::ITaskService;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;
use windows::Win32::UI::Shell::Common::IObjectArray;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::Shell::{
    ApplicationDocumentLists, IApplicationDocumentLists, IShellItem, SHGetIDListFromObject,
    SHGetKnownFolderPath, ShellExecuteExW, ShellExecuteW, ADLT_RECENT, FOLDERID_Startup,
    KF_FLAG_DEFAULT, SEE_MASK_IDLIST, SHELLEXECUTEINFOW, SIGDN_NORMALDISPLAY,
};

// ---------------------------------------------------------------- konstanta menu (enum CMD_* di C++)
pub const CMD_SMALL: i32 = 100;
pub const CMD_SEARCH: i32 = 101;
pub const CMD_TRAYCLOCK: i32 = 102;
pub const CMD_SEGMENTED: i32 = 103;
pub const CMD_DOCK: i32 = 104;
pub const CMD_CLASSIC: i32 = 105;
pub const CMD_TASKMGR: i32 = 106;
pub const CMD_TBSETTINGS: i32 = 107;
pub const CMD_EXIT: i32 = 108;
pub const CMD_AUTOSTART: i32 = 109;
pub const CMD_SWITCH: i32 = 110;
pub const CMD_APP_OPEN: i32 = 200;
pub const CMD_APP_PIN: i32 = 201;
pub const CMD_APP_CLOSE: i32 = 202;
pub const CMD_APP_SPIN: i32 = 203;
pub const CMD_APP_GROUP: i32 = 204;
pub const CMD_APP_ADMIN: i32 = 205;
pub const CMD_APP_LOC: i32 = 206;
pub const CMD_WIN0: i32 = 300;
pub const CMD_RECENT0: i32 = 400;
pub const JL_MAX: u32 = 8;

/// CLSID Task Scheduler (`kClsidTaskSched`).
pub const CLSID_TASK_SCHED: GUID = GUID::from_u128(0x0f87369f_a4e5_4cfc_bd3e_73e6154572dd);

// ---------------------------------------------------------------- global (C++ g_mbmp, g_sdWins, dst.)
/// Bitmap ikon menu (`g_mbmp[6]`). Dimiliki modul ini; dibebaskan di `menu_icons`.
pub static MENU_BMP: SyncCell<RefCell<[HBITMAP; 6]>> =
    SyncCell::new(RefCell::new([HBITMAP(std::ptr::null_mut()); 6]));
/// Jendela yang diminimalkan oleh `desk_minimize_all` (`g_sdWins`, batas 64).
pub static SD_WINS: SyncCell<RefCell<Vec<HWND>>> = SyncCell::new(RefCell::new(Vec::new()));
/// Item Jump List (`JL_ITEMS`), paralel dengan `JL_NAMES`.
pub static JL_ITEMS: SyncCell<RefCell<Vec<IShellItem>>> = SyncCell::new(RefCell::new(Vec::new()));
pub static JL_NAMES: SyncCell<RefCell<Vec<String>>> = SyncCell::new(RefCell::new(Vec::new()));
/// Jendela aplikasi terakhir yang aktif (`g_lastApp`).
pub static LAST_APP: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
/// Thumbnail DWM untuk animasi (`g_thumb`). Crate memakai isize (HTHUMBNAIL tidak ada di windows 0.61.3).
pub static ANIM_THUMB: SyncCell<Cell<isize>> = SyncCell::new(Cell::new(0));
/// Hook WinEvent (`g_hkFg`, `g_hkMin`, `g_hkCloak`, `g_hkLoc`, `g_hkLife`).
pub static HK_WEV_FG: SyncCell<Cell<HWINEVENTHOOK>> =
    SyncCell::new(Cell::new(HWINEVENTHOOK(std::ptr::null_mut())));
pub static HK_WEV_MIN: SyncCell<Cell<HWINEVENTHOOK>> =
    SyncCell::new(Cell::new(HWINEVENTHOOK(std::ptr::null_mut())));
pub static HK_WEV_CLOAK: SyncCell<Cell<HWINEVENTHOOK>> =
    SyncCell::new(Cell::new(HWINEVENTHOOK(std::ptr::null_mut())));
pub static HK_WEV_LOC: SyncCell<Cell<HWINEVENTHOOK>> =
    SyncCell::new(Cell::new(HWINEVENTHOOK(std::ptr::null_mut())));
pub static HK_WEV_LIFE: SyncCell<Cell<HWINEVENTHOOK>> =
    SyncCell::new(Cell::new(HWINEVENTHOOK(std::ptr::null_mut())));

// ---------------------------------------------------------------- mode taskbar
/// `SetMode(m)`: simpan mode, sembunyikan taskbar asli, daftarkan AppBar, lalu atur ulang tata letak.
pub fn set_mode(m: i32) {
    MODE.set(m);
    save_settings();
    hide_native_taskbar();
    app_bar_register();
    app_bar_set_pos(); // pub(crate) di core_c3.rs
    relayout();
    base_update(true); // core_c4.rs:236
}

// ---------------------------------------------------------------- autostart (menu "Start with Windows")
// Sumber kebenaran: win12-startup.ps1 (shortcut Startup atau tugas "\Win12 Shell" saat masuk). Shell hanya membaca
// keadaannya saat menu dibuka. PORT: shell tidak lagi menulis autostart (tanpa PowerShell dari Rust).

/// `AutostartTask()`: -1 tidak ada, 0 nonaktif, 1 aktif. Tugas terjadwal "\Win12 Shell".
pub fn autostart_task() -> i32 {
    let ts: ITaskService = match unsafe {
        CoCreateInstance(&CLSID_TASK_SCHED, None::<&IUnknown>, CLSCTX_INPROC_SERVER)
    } {
        Ok(t) => t,
        Err(_) => return -1,
    };
    let mut r = -1;
    let v = VARIANT::default();
    let root = BSTR::from("\\");
    let name = BSTR::from("Win12 Shell");
    if unsafe { ts.Connect(&v, &v, &v, &v) }.is_ok() {
        if let Ok(f) = unsafe { ts.GetFolder(&root) } {
            if let Ok(t) = unsafe { f.GetTask(&name) } {
                r = match unsafe { t.Enabled() } {
                    Ok(en) if en.0 != 0 => 1,
                    _ => 0,
                };
            }
        }
    }
    r
}

/// Folder Startup pengguna (setara `SHGetFolderPathW(CSIDL_STARTUP)`).
fn startup_dir() -> Option<String> {
    let p = unsafe { SHGetKnownFolderPath(&FOLDERID_Startup, KF_FLAG_DEFAULT, None) }.ok()?;
    let s = unsafe { from_pcwstr(p.0) };
    unsafe { CoTaskMemFree(Some(p.0 as *const c_void)) };
    Some(s)
}

/// `AutostartOn()`: tugas aktif, atau shortcut ada dan tidak dinonaktifkan di StartupApproved.
pub fn autostart_on() -> bool {
    if autostart_task() == 1 {
        return true;
    }
    let Some(dir) = startup_dir() else {
        return false;
    };
    let lnk = format!("{}\\Win12 Shell.lnk", dir);
    if !std::path::Path::new(&lnk).exists() {
        return false;
    }
    // StartupApproved: byte pertama ganjil (03) = dinonaktifkan; tidak ada nilai = aktif
    let mut b = [0u8; 12];
    let mut cb: u32 = b.len() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder"),
            w!("Win12 Shell.lnk"),
            RRF_RT_REG_BINARY,
            None,
            Some(b.as_mut_ptr() as *mut c_void),
            Some(&mut cb),
        )
    };
    if rc != ERROR_SUCCESS || cb == 0 {
        return true;
    }
    (b[0] & 1) == 0
}

/// `ShellModeConfigured()`: HKCU Winlogon\Shell memuat win12boot.exe. Hanya membaca.
pub fn shell_mode_configured() -> bool {
    crate::reg::get_string(r"Software\Microsoft\Windows NT\CurrentVersion\Winlogon", "Shell")
        .map(|v| v.to_lowercase().contains("win12boot.exe"))
        .unwrap_or(false)
}

// PORT: `AutostartScript` dan `AutostartSet` (C++ win12shell.cpp:3524) dihapus. Keduanya menjalankan
// powershell.exe dengan win12-startup.ps1, dan shell Rust tidak lagi menjalankan PowerShell. Ubah autostart
// secara manual lewat win12-startup.ps1 di luar shell. Item menu abu-abu dan hanya menampilkan keadaan.

// ---------------------------------------------------------------- ikon menu (glyph Segoe MDL2)
/// `GlyphBitmap(g)`: bitmap 32 bit 16x16 berisi satu glyph ikon menu.
pub fn glyph_bitmap(g: u16) -> HBITMAP {
    let sz = s(16.0);
    let mut bi: BITMAPINFO = unsafe { zeroed() };
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = sz;
    bi.bmiHeader.biHeight = -sz; // top-down
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut bits: *mut c_void = std::ptr::null_mut();
    let hb = match unsafe { CreateDIBSection(None, &bi, DIB_RGB_COLORS, &mut bits, None, 0) } {
        Ok(h) if !h.0.is_null() => h,
        _ => return HBITMAP(std::ptr::null_mut()),
    };
    let dc = unsafe { CreateCompatibleDC(None) };
    let old = unsafe { SelectObject(dc, HGDIOBJ(hb.0)) };
    let rect = RECT { left: 0, top: 0, right: sz, bottom: sz };
    let rt = RT.borrow().as_ref().cloned();
    if let Some(rt) = rt {
        if unsafe { rt.BindDC(dc, &rect) }.is_ok() {
            unsafe {
                rt.BeginDraw();
                rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            }
            let f = sz as f32;
            glyph(g, TF_GLYPH_MD.borrow().as_ref(), f / 2.0, f / 2.0, f, rgba(32, 32, 32, 1.0), 1.0);
            let _ = unsafe { rt.EndDraw(None, None) };
        }
    }
    unsafe {
        SelectObject(dc, old);
        let _ = DeleteDC(dc);
    }
    hb
}

/// `MenuIcons()`: ganti bitmap `MENU_BMP` dengan glyph baru (bitmap lama dihapus).
pub fn menu_icons() {
    const GL: [u16; 6] = [0xE8A9, 0xE721, 0xE7F4, 0xE9D9, 0xE713, 0xE7E8];
    let mut bmp = MENU_BMP.borrow_mut();
    for (i, g) in GL.iter().enumerate() {
        if !bmp[i].0.is_null() {
            let _ = unsafe { DeleteObject(HGDIOBJ(bmp[i].0)) };
        }
        bmp[i] = glyph_bitmap(*g);
    }
}

/// `SetItemBitmap(m, id, byPos, hb)`: pasang bitmap pada item menu (hasil diabaikan, seperti C++).
pub fn set_item_bitmap(m: HMENU, id: u32, by_pos: bool, hb: HBITMAP) {
    let mut mi: MENUITEMINFOW = unsafe { zeroed() };
    mi.cbSize = size_of::<MENUITEMINFOW>() as u32;
    mi.fMask = MIIM_BITMAP;
    mi.hbmpItem = hb;
    let _ = unsafe { SetMenuItemInfoW(m, id, by_pos, &mi) };
}

// ---------------------------------------------------------------- menu utama (klik kanan)
/// Gabungkan flag menu: `base` ditambah `extra` bila `on`.
fn menu_flags(base: MENU_ITEM_FLAGS, extra: MENU_ITEM_FLAGS, on: bool) -> MENU_ITEM_FLAGS {
    if on {
        MENU_ITEM_FLAGS(base.0 | extra.0)
    } else {
        base
    }
}

/// `MainMenu(owner, pt, fromTop)`: menu utama Win12 (klik kanan taskbar atau desktop).
pub fn main_menu(owner: HWND, pt: POINT, from_top: bool) {
    let (Ok(m), Ok(sub)) = (unsafe { CreatePopupMenu() }, unsafe { CreatePopupMenu() }) else {
        return;
    };
    let mode = MODE.get();
    let _ = unsafe {
        AppendMenuW(
            sub,
            menu_flags(MF_STRING, MF_CHECKED, mode == MODE_SEGMENTED),
            CMD_SEGMENTED as usize,
            w!("Segmented"),
        )
    };
    let _ = unsafe {
        AppendMenuW(sub, menu_flags(MF_STRING, MF_CHECKED, mode == MODE_DOCK), CMD_DOCK as usize, w!("Dock"))
    };
    let _ = unsafe {
        AppendMenuW(
            sub,
            menu_flags(MF_STRING, MF_CHECKED, mode == MODE_CLASSIC),
            CMD_CLASSIC as usize,
            w!("Classic"),
        )
    };
    let _ = unsafe {
        AppendMenuW(
            m,
            menu_flags(MF_STRING, MF_CHECKED, SMALL.get()),
            CMD_SMALL as usize,
            w!("Small taskbar buttons"),
        )
    };
    let _ = unsafe {
        AppendMenuW(m, menu_flags(MF_STRING, MF_CHECKED, SEARCH.get()), CMD_SEARCH as usize, w!("Search"))
    };
    let _ = unsafe { AppendMenuW(m, MF_POPUP, sub.0 as usize, w!("Taskbar modes")) };
    let _ = unsafe {
        AppendMenuW(
            m,
            menu_flags(MF_STRING, MF_CHECKED, TRAY_CLOCK.get()),
            CMD_TRAYCLOCK as usize,
            w!("Clock on taskbar"),
        )
    };
    // Mode shell: autostart diatur lewat skrip saja. Aktif hanya bila bukan mode shell dan skrip ada.
    let as_shell = shell_mode_configured();
    let auto_on = as_shell || autostart_on();
    // PORT: item hanya menampilkan keadaan; shell tidak menjalankan PowerShell. Ubah lewat win12-startup.ps1 manual.
    let auto_flags = menu_flags(
        menu_flags(MF_STRING, MF_CHECKED, auto_on),
        MF_GRAYED,
        true,
    );
    let auto_text = if as_shell {
        w!("Start with Windows (Win12 is the shell)")
    } else {
        w!("Start with Windows")
    };
    let _ = unsafe { AppendMenuW(m, auto_flags, CMD_AUTOSTART as usize, auto_text) };
    let _ = unsafe { AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null()) };
    let _ = unsafe { AppendMenuW(m, MF_STRING, CMD_TASKMGR as usize, w!("Task Manager")) };
    let _ = unsafe { AppendMenuW(m, MF_STRING, CMD_TBSETTINGS as usize, w!("Taskbar settings")) };
    let _ = unsafe { AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null()) };
    if SHELL_MODE.get() {
        let _ = unsafe { AppendMenuW(m, MF_STRING, CMD_SWITCH as usize, w!("Beralih ke Windows biasa")) };
    } else if boot_exe_path().is_some() {
        let _ = unsafe { AppendMenuW(m, MF_STRING, CMD_SWITCH as usize, w!("Beralih ke Win12 penuh")) };
    }
    let _ = unsafe { AppendMenuW(m, MF_STRING, CMD_EXIT as usize, w!("Exit Win12 Shell")) };
    let mb = *MENU_BMP.borrow();
    set_item_bitmap(m, CMD_SMALL as u32, false, mb[0]);
    set_item_bitmap(m, CMD_SEARCH as u32, false, mb[1]);
    set_item_bitmap(m, 2, true, mb[2]);
    set_item_bitmap(m, CMD_TASKMGR as u32, false, mb[3]);
    set_item_bitmap(m, CMD_TBSETTINGS as u32, false, mb[4]);
    set_item_bitmap(m, CMD_EXIT as u32, false, mb[5]);
    let cmd = track_menu(m, owner, pt, from_top);
    let _ = unsafe { DestroyMenu(m) };
    match cmd {
        CMD_SMALL => {
            SMALL.set(!SMALL.get());
            save_settings();
            reload_icons();
            relayout();
            app_bar_set_pos(); // pub(crate) di core_c3.rs:1566
        }
        CMD_SEARCH => {
            SEARCH.set(!SEARCH.get());
            save_settings();
            relayout();
        }
        CMD_TRAYCLOCK => {
            TRAY_CLOCK.set(!TRAY_CLOCK.get());
            save_settings();
            relayout();
        }
        CMD_SEGMENTED => set_mode(MODE_SEGMENTED),
        CMD_DOCK => set_mode(MODE_DOCK),
        CMD_CLASSIC => set_mode(MODE_CLASSIC),
        CMD_TASKMGR => {
            let _ = unsafe {
                ShellExecuteW(None, PCWSTR::null(), w!("taskmgr.exe"), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL)
            };
        }
        CMD_TBSETTINGS => shell_open("ms-settings:taskbar"),
        CMD_SWITCH => {
            if SHELL_MODE.get() {
                shell_switch_to_windows(None);
            } else {
                normal_switch_to_shell();
            }
        }
        CMD_EXIT => {
            let _ = unsafe { PostMessageW(Some(MSG.get()), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
        // PORT: C++ win12shell.cpp:3524 `AutostartSet(!autoOn)` dihapus. Item CMD_AUTOSTART abu-abu, jadi tidak
        // bisa diklik, dan shell tidak menjalankan PowerShell sehingga tidak ada penulisan HKCU Winlogon\Shell.
        _ => {}
    }
}

// ---------------------------------------------------------------- mode shell: desktop, Win+D, Win+R, Win+A
// Jendela desktop berkelas "Progman" agar kode lapisan desktop (PillsToDesk, deteksi desktop aktif) bekerja sama
// seperti dengan Explorer. Wallpaper digambar lewat PaintDesktop; jendela selalu paling bawah.

/// `DeskFit()`: rentangkan jendela desktop ke seluruh layar virtual, di paling bawah.
pub fn desk_fit() {
    let d = DESK.get();
    if d.0.is_null() {
        return;
    }
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let _ = SetWindowPos(d, Some(HWND_BOTTOM), x, y, w, h, SWP_NOACTIVATE);
    }
}

/// `DeskMenu(pt)`: menu klik kanan desktop.
/// Menu latar (Tampilan, Tempel, Baru, wallpaper, ...) kini ada di desk_icons.rs.
pub fn desk_menu(pt: POINT) {
    super::desk_icons::desk_background_menu(pt);
}

/// `DeskProc`: WNDPROC jendela desktop "Progman" Win12.
pub unsafe extern "system" fn desk_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Ikon desktop: mouse, keyboard, menu shell, ganti nama, pemberitahuan perubahan.
    if let Some(r) = super::desk_icons::desk_icons_on_message(h, m, wp, lp) {
        return r;
    }
    match m {
        // Tombol Win saat jendela admin (mis. Task Manager) aktif: kait LL tidak dipanggil (UIPI), Windows
        // memposting WM_SYSCOMMAND SC_TASKLIST ke jendela shell (GetShellWindow = jendela desktop ini).
        windows::Win32::UI::WindowsAndMessaging::WM_SYSCOMMAND
            if super::state::SHELL_MODE.get()
                && ((wp.0 as u32) & 0xFFF0) == windows::Win32::UI::WindowsAndMessaging::SC_TASKLIST =>
        {
            crate::shell::hotkeys::hk_task_list();
            return LRESULT(0);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = unsafe { zeroed() };
            let dc = unsafe { BeginPaint(h, &mut ps) };
            // PaintDesktop adalah export user32 (windows 0.61: Graphics::Gdi), tidak perlu port ulang.
            let _ = unsafe { windows::Win32::Graphics::Gdi::PaintDesktop(dc) };
            // Ikon desktop (mode shell) digambar di atas wallpaper.
            super::desk_icons::desk_icons_paint(dc);
            let _ = unsafe { EndPaint(h, &ps) };
            return LRESULT(0);
        }
        WM_WINDOWPOSCHANGING => {
            // tidak pernah naik di atas jendela lain
            let p = lp.0 as *mut WINDOWPOS;
            unsafe {
                (*p).hwndInsertAfter = HWND_BOTTOM;
                (*p).flags = SET_WINDOW_POS_FLAGS((*p).flags.0 & !SWP_NOZORDER.0);
            }
            return LRESULT(0);
        }
        WM_SETTINGCHANGE => {
            if wp.0 == SPI_SETDESKWALLPAPER.0 as usize {
                desk_wall_load();
                let _ = unsafe { InvalidateRect(Some(h), None, true) };
            } else if wp.0 == SPI_SETWORKAREA.0 as usize {
                // Area kerja berubah (bilah Win12): ikon desktop dihitung ulang saat WM_PAINT.
                let _ = unsafe { InvalidateRect(Some(h), None, true) };
            }
        }
        WM_DISPLAYCHANGE => {
            desk_fit();
            super::desk_icons::desk_icons_layout();
            desk_wall_load();
            let _ = unsafe { InvalidateRect(Some(h), None, true) };
        }
        WM_CLOSE => return LRESULT(0), // Alt+F4 di desktop: desktop tidak boleh tertutup
        _ => {}
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

// ---------------------------------------------------------------- mode shell: mulai/hentikan (TIDAK DIPANGGIL, aturan T09)

/// `ShellModeStart()`: jendela "Progman" sebagai shell, lalu tugas dan pintasan ikut jalan.
/// Dipanggil dari core_c4b (`shell_mode` saat startup, sekali). C++ 5248.
pub fn shell_mode_start() {
    // SAFETY: WNDCLASSEXW adalah struct POD; nol berarti None/null untuk setiap pointer dan handle.
    let mut wc: WNDCLASSEXW = unsafe { zeroed() };
    wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(desk_proc);
    wc.hInstance = HI.get();
    // SAFETY: IDC_ARROW adalah kursor sistem.
    if let Ok(cur) = unsafe { LoadCursorW(None, IDC_ARROW) } {
        wc.hCursor = cur;
    }
    // CS_DBLCLKS: klik ganda terkirim sebagai WM_LBUTTONDBLCLK (ikon desktop, desk_icons.rs).
    wc.style = CS_DBLCLKS;
    wc.lpszClassName = w!("Progman");
    // Gagal bila kelas sudah terdaftar: diabaikan (sama seperti C).
    let _ = unsafe { RegisterClassExW(&wc) };
    let desk = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            w!("Progman"),
            w!("Program Manager"),
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(HI.get()),
            None,
        )
    }
    .unwrap_or_default();
    DESK.set(desk);
    if !desk.0.is_null() {
        desk_fit();
        // SAFETY: desk adalah jendela yang baru dibuat di atas.
        let _ = unsafe { ShowWindow(desk, SW_SHOWNA) };
    }
    // SetShellWindow dan SetTaskmanWindow tidak ada di crate windows: diambil dari user32 seperti di C.
    if let Ok(u) = unsafe { GetModuleHandleW(w!("user32.dll")) } {
        if let Some(f) = unsafe { GetProcAddress(u, PCSTR::from_raw(b"SetShellWindow\0".as_ptr())) } {
            if !desk.0.is_null() {
                // SAFETY: SetShellWindow dari user32 menerima satu HWND dan mengembalikan BOOL.
                let set_shell: unsafe extern "system" fn(HWND) -> BOOL = unsafe { std::mem::transmute(f) };
                let _ = unsafe { set_shell(desk) };
            }
        }
        if let Some(f) = unsafe { GetProcAddress(u, PCSTR::from_raw(b"SetTaskmanWindow\0".as_ptr())) } {
            // SAFETY: SetTaskmanWindow dari user32 menerima satu HWND dan mengembalikan BOOL.
            let set_taskman: unsafe extern "system" fn(HWND) -> BOOL = unsafe { std::mem::transmute(f) };
            let _ = unsafe { set_taskman(MSG.get()) };
        }
    }
    // Tanpa Explorer, jendela yang diminimalkan menumpuk sebagai bilah kecil: sembunyikan (ARW_HIDE).
    let mut mm: MINIMIZEDMETRICS = unsafe { zeroed() };
    mm.cbSize = size_of::<MINIMIZEDMETRICS>() as u32;
    // SAFETY: mm hidup selama pemanggilan; ukuran diteruskan sesuai cbSize.
    let got = unsafe {
        SystemParametersInfoW(
            SPI_GETMINIMIZEDMETRICS,
            mm.cbSize,
            Some(&mut mm as *mut MINIMIZEDMETRICS as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if got.is_ok() && (mm.iArrange.0 & ARW_HIDE) == 0 {
        mm.iArrange = MINIMIZEDMETRICS_ARRANGE(mm.iArrange.0 | ARW_HIDE);
        // SAFETY: sama seperti pemanggilan GET di atas.
        let _ = unsafe {
            SystemParametersInfoW(
                SPI_SETMINIMIZEDMETRICS,
                mm.cbSize,
                Some(&mut mm as *mut MINIMIZEDMETRICS as *mut c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };
    }
    shell_work_area(true); // pub(crate) di core_c3.rs:1523
    // Ikon desktop setelah area kerja diatur: tata letak memakai SPI_GETWORKAREA (desk_icons.rs).
    super::desk_icons::desk_icons_reload();
    tasks_start();
    hotkeys_start(); // pintasan Win yang biasanya ditangani Explorer, juga saat jendela admin fokus
    // SAFETY: GetShellWindow tanpa parameter.
    if unsafe { GetShellWindow() } != desk {
        // Tanpa catatan ini, kegagalan SetShellWindow tidak terlihat dari luar.
        log("shell: GetShellWindow bukan jendela desktop Win12 (SetShellWindow gagal)");
    }
}

/// `ShellModeStop()`: kebalikan `shell_mode_start`. Dipanggil dari core_c4b saat keluar (C++ 4873).
pub fn shell_mode_stop() {
    hotkeys_stop();
    tasks_stop();
    super::desk_icons::desk_icons_clear();
    let desk = DESK.get();
    if !desk.0.is_null() {
        // SAFETY: desk adalah jendela milik kita.
        let _ = unsafe { DestroyWindow(desk) };
        DESK.set(HWND::default());
    }
}

/// `DeskRestoreAll()`: kembalikan jendela yang diminimalkan Win+D/Win+M/Win+Home, dari yang paling bawah
/// agar urutan seperti semula. `false` bila tidak ada yang perlu dikembalikan.
pub fn desk_restore_all() -> bool {
    let wins: Vec<HWND> = SD_WINS.borrow().to_vec();
    let iconic = |w: HWND| unsafe { IsWindow(Some(w)).as_bool() && IsIconic(w).as_bool() };
    let any = wins.iter().any(|&w| iconic(w));
    if any {
        for &w in wins.iter().rev() {
            if iconic(w) {
                // SAFETY: w sudah diperiksa IsWindow dan IsIconic di atas.
                let _ = unsafe { ShowWindowAsync(w, SW_RESTORE) };
            }
        }
    }
    SD_WINS.borrow_mut().clear();
    any
}

/// `DeskMinimizeAll(keep)`: minimalkan semua jendela app kecuali `keep` (Win+Home). Jendela admin (High IL)
/// tidak bisa diminimalkan dari proses biasa (UIPI) dan tetap tampil.
pub fn desk_minimize_all(keep: Option<HWND>) {
    let mut found: Vec<HWND> = Vec::new();
    // SAFETY: GetTopWindow dan GetWindow hanya membaca urutan Z jendela.
    let mut w = unsafe { GetTopWindow(None) }.unwrap_or_default();
    while !w.0.is_null() && found.len() < 64 {
        let visible_app = unsafe { IsWindowVisible(w).as_bool() && !IsIconic(w).as_bool() };
        if Some(w) != keep && is_task_window(w) && visible_app {
            found.push(w);
        }
        w = unsafe { GetWindow(w, GW_HWNDNEXT) }.unwrap_or_default();
    }
    for &h in &found {
        // SAFETY: h adalah jendela yang baru dienumerasi.
        let _ = unsafe { ShowWindowAsync(h, SW_SHOWMINNOACTIVE) };
    }
    *SD_WINS.borrow_mut() = found;
    match keep {
        Some(k) => activate_window(k),
        None => {
            let desk = DESK.get();
            if !desk.0.is_null() {
                // SAFETY: desk adalah jendela milik kita.
                let _ = unsafe { SetForegroundWindow(desk) };
            }
        }
    }
}

/// `RunDlgThread`: dialog Run bawaan (shell32 ordinal 61) bersifat modal, jadi jalan di thread sendiri.
/// Dipanggil dari `shell_key` (Win+R, VK 0x52) dan dari `WX_RUN` lewat `shell_key(b'R')` (winx.rs).
pub unsafe extern "system" fn run_dlg_thread(_arg: *mut c_void) -> u32 {
    type RunFileDlg = unsafe extern "system" fn(HWND, HICON, PCWSTR, PCWSTR, PCWSTR, u32);
    // SAFETY: thread ini sendiri; CoInitializeEx dan CoUninitialize berpasangan.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if let Ok(sh) = unsafe { GetModuleHandleW(w!("shell32.dll")) } {
        // Ordinal 61 sama dengan MAKEINTRESOURCEA(61).
        if let Some(f) = unsafe { GetProcAddress(sh, PCSTR(61usize as *const u8)) } {
            let run: RunFileDlg = unsafe { std::mem::transmute(f) };
            // SAFETY: argumen null sama seperti pemanggilan C.
            unsafe { run(HWND::default(), HICON::default(), PCWSTR::null(), PCWSTR::null(), PCWSTR::null(), 0) };
        }
    }
    unsafe { CoUninitialize() };
    0
}

/// `ShellOpenQs()`: buka panel Quick Settings dari ikon volume di bilah bawah; bila tidak ada, di pojok kanan bawah.
pub fn shell_open_qs() {
    let nitems = NITEMS.get().max(0) as usize;
    let anchor = {
        let items = ITEMS.borrow();
        let pills = PILLS.borrow();
        let found = items.iter().take(nitems).find_map(|it| {
            if it.ty != IT_VOL || it.pill < 0 || it.pill >= NBOTTOM {
                return None;
            }
            let p = pills.get(it.pill as usize)?;
            if !p.shown {
                return None;
            }
            Some((p.x + (it.x + it.w / 2.0) as i32, p.y))
        });
        found
    };
    if let Some((ax, ay)) = anchor {
        panel_toggle(PN_QS, ax, ay);
        return;
    }
    let mut work = RECT::default();
    // SAFETY: work adalah RECT yang hidup selama pemanggilan.
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work as *mut RECT as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    panel_toggle(PN_QS, work.right - s(200.0), work.bottom);
}

/// `DockNumber(n)`: Win+1..9 membuka aplikasi dok ke-n (urutan IT_APP pada bilah bawah).
/// Pilihan jendela (fokus, minimalkan, atau luncurkan) mengikuti `w12_dock_pick`.
pub fn dock_number(n: i32) {
    let nitems = NITEMS.get().max(0) as usize;
    let ai = {
        let items = ITEMS.borrow();
        let mut seen = 0;
        let mut found = None;
        for it in items.iter().take(nitems) {
            if it.ty != IT_APP {
                continue;
            }
            seen += 1;
            if seen == n {
                found = Some(it.app);
                break;
            }
        }
        found
    };
    let Some(ai) = ai else {
        return;
    };
    let Ok(idx) = usize::try_from(ai) else {
        return;
    };
    // Salin jendela app agar borrow APPS tidak dipegang saat memanggil fungsi lain.
    let wins: Vec<HWND> = match APPS.borrow().get(idx) {
        Some(a) => a.wins.clone(),
        None => return,
    };
    // SAFETY: GetForegroundWindow tanpa parameter.
    let fg = unsafe { GetForegroundWindow() };
    // C menyimpan indeks terakhir yang cocok; rposition memberi hasil yang sama.
    let cur = wins.iter().rposition(|&w| w == fg).map_or(-1, |p| p as i32);
    let pick = w12_dock_pick(wins.len() as i32, cur);
    if pick == W12DK_LAUNCH {
        // Tanpa jendela: sama dengan klik dok.
        click_app(ai, false);
        return;
    }
    quiet_start(1000);
    if pick == W12DK_MINIMIZE {
        if let Some(&first) = wins.first() {
            minimize_dock_window(first);
        }
    } else if let Some(&w) = usize::try_from(pick).ok().and_then(|p| wins.get(p)) {
        activate_window(w);
    }
}

/// `OpenFolderWindow(path)`: buka `win12files.exe` bila `FILES_OWN` dan berkasnya ada; selain itu `explorer.exe`.
/// `path` kosong berarti tanpa argumen lokasi.
pub fn open_folder_window(path: &str) {
    let has_path = !path.is_empty();
    let dir = DIR.borrow().clone();
    let wdir = to_wide_z(&dir);
    let mut exe: Option<Vec<u16>> = None;
    if FILES_OWN.get() {
        let wexe = to_wide_z(&format!("{dir}\\win12files.exe"));
        // SAFETY: wexe NUL-terminated dan hidup selama pemanggilan.
        if unsafe { GetFileAttributesW(pcw(&wexe)) } != INVALID_FILE_ATTRIBUTES {
            exe = Some(wexe);
        }
    }
    let wargs = has_path.then(|| to_wide_z(&format!("\"{path}\"")));
    let arg_ptr = wargs.as_ref().map_or(PCWSTR::null(), |w| pcw(w));
    // SAFETY: semua buffer wide hidup sampai ShellExecuteW kembali.
    unsafe {
        match exe {
            Some(wexe) => {
                ShellExecuteW(None, w!("open"), pcw(&wexe), arg_ptr, pcw(&wdir), SW_SHOWNORMAL);
            }
            None => {
                // PORT: di mode shell explorer.exe tidak dipakai, karena akan menyalakan shell Explorer. Tanpa win12files,
                // Win+E/folder tidak membuka apa pun (perilaku baru; dilaporkan).
                if SHELL_MODE.get() {
                    return;
                }
                let wexplorer = to_wide_z("explorer.exe");
                ShellExecuteW(None, w!("open"), pcw(&wexplorer), arg_ptr, PCWSTR::null(), SW_SHOWNORMAL);
            }
        }
    }
}

/// `ShellKey(vk)`: pintasan tombol Win (kode VK huruf/angka/panah). Dipanggil dari hook mode shell (core_c4b.rs) dan tabel pintasan (hotkeys.rs).
pub fn shell_key(vk: i32) {
    match vk {
        0x45 => open_folder_window(""), // 'E'
        0x52 => {
            // Sama seperti C: CreateThread(RunDlgThread), thread sendiri, handle langsung ditutup.
            let res = unsafe {
                windows::Win32::System::Threading::CreateThread(
                    None,
                    0,
                    Some(run_dlg_thread),
                    None,
                    windows::Win32::System::Threading::THREAD_CREATION_FLAGS(0),
                    None,
                )
            };
            if let Ok(t) = res {
                let _ = unsafe { windows::Win32::Foundation::CloseHandle(t) };
            }
        }
        0x49 => shell_open("ms-settings:"), // 'I': mode shell lewat hk_launch_settings_uri (hotkeys.rs); dialog beralih hanya bila gagal
        0x44 => show_desktop_toggle(),      // 'D'
        0x41 => shell_open_qs(),            // 'A'
        0x58 => winx_menu(None),            // 'X': owner pill tombol Start (WinXStartOwner) belum dipakai
        0x53 => super::shot_area::shot_area_start(), // 'S': tangkapan area native (ganti ms-screenclip:)
        0x25 | 0x26 | 0x27 | 0x28 => snap_key(vk), // VK_LEFT/UP/RIGHT/DOWN
        0x31..=0x39 => dock_number(vk - 0x30),     // '1'..'9'
        _ => {}
    }
}

/// `FindAppByIdentity(s)`: indeks app di `APPS` yang identitasnya sama dengan `s`.
/// Bila `s` punya AUMID, cocok AUMID tanpa membedakan huruf besar/kecil; bila tidak, cocok exe dan launch
/// pada app yang juga tanpa AUMID. Deviasi: membaca `APPS` langsung (tanpa `g_napps`).
fn app_index_by_identity(s: &App) -> Option<usize> {
    let apps = APPS.borrow();
    apps.iter().position(|a| {
        if !s.aumid.is_empty() {
            a.aumid.eq_ignore_ascii_case(&s.aumid)
        } else {
            a.aumid.is_empty() && a.exe == s.exe && a.launch == s.launch
        }
    })
}

/// `TogglePin(s)`: sematkan atau lepas app dari bilah. Melepas app tanpa jendela dan tanpa urutan Start
/// menghapusnya dari daftar. Menyematkan memindahkan app ke akhir daftar dengan `pin = 1`.
pub fn toggle_pin(s: &App) {
    let Some(i) = app_index_by_identity(s) else {
        return;
    };
    {
        let mut apps = APPS.borrow_mut();
        if apps[i].pin >= 0 {
            apps[i].pin = -1;
            if apps[i].wins.is_empty() && apps[i].spin == 0 {
                let mut gone = apps.remove(i);
                icon_free(&mut gone.icon);
            }
        } else {
            if apps[i].launch.is_empty() {
                return;
            }
            let a = apps.remove(i);
            apps.push(a);
            let last = apps.len() - 1;
            apps[last].pin = 1;
        }
    }
    save_apps();
    start_lists_invalidate(); // C: TogglePin (win12shell.cpp:3761)
    relayout();
}

/// Identitas apps.txt dari nama parsing Start (`shell:AppsFolder\<AUMID>` -> AUMID; jalur tetap jalur).
pub fn ident_from_parsing(parsing: &str) -> String {
    let p = parsing.trim();
    let low = p.to_ascii_lowercase();
    if low.starts_with("shell:appsfolder\\") {
        p["shell:AppsFolder\\".len()..].to_string()
    } else {
        p.to_string()
    }
}

/// Indeks `APPS` untuk identitas apps.txt (AUMID atau jalur launch/exe), tanpa membedakan huruf besar-kecil.
pub fn app_index_by_ident(ident: &str) -> Option<usize> {
    if ident.is_empty() {
        return None;
    }
    let is_path = ident.contains('\\') || ident.contains('/');
    APPS.borrow().iter().position(|a| {
        if is_path {
            a.aumid.is_empty() && (a.launch.eq_ignore_ascii_case(ident) || a.exe.eq_ignore_ascii_case(ident))
        } else {
            a.aumid.eq_ignore_ascii_case(ident)
        }
    })
}

/// (disematkan di Start, disematkan di taskbar) untuk identitas apps.txt.
pub fn ident_pin_state(ident: &str) -> (bool, bool) {
    match app_index_by_ident(ident) {
        Some(i) => APPS.borrow().get(i).map_or((false, false), |a| (a.spin > 0, a.pin >= 0)),
        None => (false, false),
    }
}

/// Sematkan/lepas dari Start (2026-11-15). Aplikasi yang belum ada di `APPS` dibuat dari `resolve_entry`.
/// Melepas aplikasi tanpa jendela dan tanpa pin taskbar menghapusnya dari daftar, seperti `toggle_pin`.
pub fn start_pin_toggle_ident(ident: &str) {
    let idx = app_index_by_ident(ident);
    {
        let mut apps = APPS.borrow_mut();
        let top = apps.iter().map(|a| a.spin).fold(0, i32::max);
        match idx {
            Some(i) if apps[i].spin > 0 => {
                let k = apps[i].spin;
                apps[i].spin = 0;
                for a in apps.iter_mut() {
                    if a.spin > k {
                        a.spin -= 1;
                    }
                }
                if apps[i].pin < 0 && apps[i].wins.is_empty() {
                    let mut gone = apps.remove(i);
                    icon_free(&mut gone.icon);
                }
            }
            Some(i) => apps[i].spin = top + 1,
            None => {
                if apps.len() >= MAXAPPS as usize {
                    return;
                }
                drop(apps);
                let Some(mut a) = resolve_entry(ident) else {
                    log(&format!("start: sematkan gagal, identitas tidak dikenal: {ident}"));
                    return;
                };
                a.spin = top + 1;
                a.pin = -1;
                APPS.borrow_mut().push(a);
            }
        }
    }
    save_apps();
    start_lists_invalidate();
    super::start::st_all_apps_invalidate();
    relayout();
}

/// Sematkan/lepas dari taskbar berdasarkan identitas (menu Start, 2026-11-15).
pub fn taskbar_pin_toggle_ident(ident: &str) {
    if let Some(i) = app_index_by_ident(ident) {
        let snap = {
            let apps = APPS.borrow();
            let a = &apps[i];
            App { aumid: a.aumid.clone(), exe: a.exe.clone(), launch: a.launch.clone(), ..Default::default() }
        };
        toggle_pin(&snap);
        return;
    }
    if APPS.borrow().len() >= MAXAPPS as usize {
        return;
    }
    let Some(mut a) = resolve_entry(ident) else {
        log(&format!("taskbar: sematkan gagal, identitas tidak dikenal: {ident}"));
        return;
    };
    if a.launch.is_empty() {
        return;
    }
    a.pin = 1;
    APPS.borrow_mut().push(a);
    save_apps();
    start_lists_invalidate();
    super::start::st_all_apps_invalidate();
    relayout();
}

// ---------------------------------------------------------------- jump list "Terbaru" (Fase 7 Task 5)
// Dokumen terbaru dari ApplicationDocumentLists untuk AUMID app dok. Dibaca saat menu klik kanan dibuka dan
// dilepas setelah menu tertutup (jump_list_free dipanggil di akhir app_menu).

/// `JumpListFree()`: lepaskan item jump list (Drop `IShellItem` melepas referensi COM).
pub fn jump_list_free() {
    JL_ITEMS.borrow_mut().clear();
    JL_NAMES.borrow_mut().clear();
}

/// `JumpListLoad(aumid)`: muat hingga `JL_MAX` dokumen terbaru untuk AUMID. Kosong bila AUMID kosong atau COM gagal.
pub fn jump_list_load(aumid: &str) {
    jump_list_free();
    if aumid.is_empty() {
        return;
    }
    let adl: IApplicationDocumentLists =
        match unsafe { CoCreateInstance(&ApplicationDocumentLists, None::<&IUnknown>, CLSCTX_INPROC_SERVER) } {
            Ok(v) => v,
            Err(_) => return,
        };
    let wid = to_wide_z(aumid);
    // SAFETY: wid NUL-terminated dan hidup selama pemanggilan.
    if unsafe { adl.SetAppID(pcw(&wid)) }.is_err() {
        return;
    }
    let arr: IObjectArray = match unsafe { adl.GetList::<IObjectArray>(ADLT_RECENT, JL_MAX) } {
        Ok(v) => v,
        Err(_) => return,
    };
    let n = unsafe { arr.GetCount() }.unwrap_or(0);
    let mut items = JL_ITEMS.borrow_mut();
    let mut names = JL_NAMES.borrow_mut();
    for i in 0..n {
        if items.len() >= JL_MAX as usize {
            break;
        }
        let it: IShellItem = match unsafe { arr.GetAt::<IShellItem>(i) } {
            Ok(v) => v,
            Err(_) => continue,
        };
        // SAFETY: string COM dari GetDisplayName dibebaskan dengan CoTaskMemFree di bawah.
        let Ok(p) = (unsafe { it.GetDisplayName(SIGDN_NORMALDISPLAY) }) else {
            continue;
        };
        if p.0.is_null() {
            continue;
        }
        // SAFETY: p.0 menunjuk string NUL-terminated dari COM.
        let nama = unsafe { from_pcwstr(p.0 as *const u16) };
        names.push(copy_wide(&nama, 260));
        // SAFETY: p.0 berasal dari GetDisplayName dan belum dibebaskan.
        unsafe { CoTaskMemFree(Some(p.0 as *const c_void)) };
        items.push(it);
    }
}

/// `JumpListOpen(owner, item)`: buka dokumen terbaru lewat PIDL item (`SEE_MASK_IDLIST`).
/// Item tetap dimiliki `JL_ITEMS`.
pub fn jump_list_open(owner: HWND, item: &IShellItem) {
    // SAFETY: item hidup selama pemanggilan; pidl dibebaskan di bawah dengan CoTaskMemFree.
    let pidl = match unsafe { SHGetIDListFromObject(item) } {
        Ok(p) if !p.is_null() => p,
        _ => return,
    };
    // SAFETY: struct SHELLEXECUTEINFOW berupa POD; nol adalah nilai awal yang sama seperti `= {}` di C.
    let mut sei: SHELLEXECUTEINFOW = unsafe { zeroed() };
    sei.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    sei.fMask = SEE_MASK_IDLIST;
    sei.hwnd = owner;
    sei.nShow = SW_SHOWNORMAL.0;
    sei.lpIDList = pidl as *mut c_void;
    // SAFETY: sei dan pidl hidup selama pemanggilan ShellExecuteExW.
    let _ = unsafe { ShellExecuteExW(&mut sei) };
    // SAFETY: pidl dari SHGetIDListFromObject, dibebaskan sekali di sini.
    unsafe { CoTaskMemFree(Some(pidl as *const c_void)) };
}

/// Tambah item menu dengan teks dinamis. String wide NUL-terminated dibuat di sini; AppendMenuW menyalinnya.
fn append_text(m: HMENU, flags: MENU_ITEM_FLAGS, id: i32, text: &str) {
    let wt = to_wide_z(text);
    // SAFETY: wt NUL-terminated dan hidup selama pemanggilan.
    let _ = unsafe { AppendMenuW(m, flags, id as usize, pcw(&wt)) };
}

/// `AppMenu(owner, pt, ai)`: menu klik kanan app di bilah (jendela, jump list "Terbaru", sematkan, tutup).
/// PORT: `property_store_aumid` di core_c1.rs harus dibuat `pub(crate)` oleh integrator (sekarang `fn` privat).
pub fn app_menu(owner: HWND, pt: POINT, ai: i32) {
    // Salinan app: daftar APPS bisa berubah selama menu terbuka. App tidak punya Clone, field disalin manual.
    let snap = {
        let apps = APPS.borrow();
        let Some(a) = usize::try_from(ai).ok().and_then(|i| apps.get(i)) else {
            return;
        };
        App {
            aumid: a.aumid.clone(),
            exe: a.exe.clone(),
            launch: a.launch.clone(),
            icon_name: a.icon_name.clone(),
            name: a.name.clone(),
            pin: a.pin,
            spin: a.spin,
            wins: a.wins.clone(),
            fg: a.fg,
            flash: a.flash,
            ..Default::default()
        }
    };
    let Ok(m) = (unsafe { CreatePopupMenu() }) else {
        return;
    };
    let nwin = snap.wins.len();
    let nw = nwin.min(12);
    for (i, &hw) in snap.wins.iter().take(nw).enumerate() {
        // C: GetWindowTextW ke buffer 90; judul > 60 karakter dipotong 57 + "..."; judul kosong memakai nama app.
        let mut buf = [0u16; 90];
        // SAFETY: buf hidup selama pemanggilan; hw adalah jendela app yang masih dicatat.
        let n = unsafe { GetWindowTextW(hw, &mut buf) }.clamp(0, 89) as usize;
        let mut t: Vec<u16> = buf[..n].to_vec();
        if t.len() > 60 {
            t.truncate(57);
            t.extend("...".encode_utf16());
        }
        let judul = String::from_utf16_lossy(&t);
        let label = if judul.is_empty() { snap.name.as_str() } else { judul.as_str() };
        append_text(m, MF_STRING, CMD_WIN0 + i as i32, label);
    }
    if nw > 0 {
        // SAFETY: separator tidak memakai teks (lpnewitem null), sama seperti nullptr di C.
        let _ = unsafe { AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null()) };
    }
    // Jump list "Terbaru": AUMID dari app; bila kosong, dari jendela pertama. Tanpa AUMID, bagian ini dilewati.
    let mut aumid = snap.aumid.clone();
    if aumid.is_empty() && nw > 0 {
        // SAFETY: hwnd jendela pertama app yang masih dicatat.
        if let Ok(ps) = unsafe { SHGetPropertyStoreForWindow::<IPropertyStore>(snap.wins[0]) } {
            aumid = copy_wide(&property_store_aumid(&ps), 256);
        }
    }
    jump_list_load(&aumid);
    let jl: Vec<String> = JL_NAMES.borrow().iter().cloned().collect();
    let jl_n = jl.len();
    if jl_n > 0 {
        // SAFETY: "Terbaru" adalah literal wide; item abu-abu (MF_GRAYED) tanpa perintah.
        let _ = unsafe { AppendMenuW(m, MF_STRING | MF_GRAYED, 0, w!("Terbaru")) };
        for (i, name) in jl.iter().enumerate() {
            // C: '&' digandakan agar tidak menjadi tanda mnemonik menu; batas 598 karakter.
            let mut t: Vec<u16> = Vec::new();
            for u in name.encode_utf16() {
                if t.len() >= 598 {
                    break;
                }
                if u == u16::from(b'&') {
                    t.push(u);
                }
                t.push(u);
            }
            t.push(0);
            // SAFETY: t NUL-terminated dan hidup selama pemanggilan.
            let _ = unsafe { AppendMenuW(m, MF_STRING, (CMD_RECENT0 + i as i32) as usize, pcw(&t)) };
        }
        // SAFETY: separator tanpa teks.
        let _ = unsafe { AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null()) };
    }
    // 2026-11-15: menu dilengkapi (admin, sematkan Start, grup segmented, lokasi file), label Bahasa Indonesia.
    let ident = if snap.aumid.is_empty() { snap.launch.clone() } else { snap.aumid.clone() };
    let packaged = snap.aumid.contains('!');
    if !snap.launch.is_empty() {
        let nama = if snap.name.is_empty() { "Buka" } else { snap.name.as_str() };
        append_text(m, MF_STRING, CMD_APP_OPEN, nama);
        if !packaged && !snap.exe.is_empty() {
            append_text(m, MF_STRING, CMD_APP_ADMIN, "Jalankan sebagai administrator");
        }
        // SAFETY: separator tanpa teks.
        let _ = unsafe { AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null()) };
    }
    if !snap.launch.is_empty() || snap.pin >= 0 {
        let teks = if snap.pin >= 0 { "Lepas dari taskbar" } else { "Sematkan ke taskbar" };
        append_text(m, MF_STRING, CMD_APP_PIN, teks);
    }
    if !ident.is_empty() {
        let teks = if snap.spin > 0 { "Lepas dari Start" } else { "Sematkan ke Start" };
        append_text(m, MF_STRING, CMD_APP_SPIN, teks);
    }
    if MODE.get() == MODE_SEGMENTED && snap.pin >= 0 {
        let teks = if snap.pin == 0 { "Pindah ke grup kanan" } else { "Pindah ke grup kiri" };
        append_text(m, MF_STRING, CMD_APP_GROUP, teks);
    }
    if !snap.exe.is_empty() {
        append_text(m, MF_STRING, CMD_APP_LOC, "Buka lokasi file");
    }
    if nwin > 0 {
        // SAFETY: separator tanpa teks.
        let _ = unsafe { AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null()) };
        let teks = if nwin > 1 { "Tutup semua jendela" } else { "Tutup jendela" };
        append_text(m, MF_STRING, CMD_APP_CLOSE, teks);
    }
    let cmd = track_menu(m, owner, pt, false);
    // SAFETY: m dibuat CreatePopupMenu di atas dan belum dihancurkan.
    let _ = unsafe { DestroyMenu(m) };
    if cmd >= CMD_RECENT0 && cmd < CMD_RECENT0 + jl_n as i32 {
        let item = JL_ITEMS.borrow().get((cmd - CMD_RECENT0) as usize).cloned();
        if let Some(item) = item {
            jump_list_open(owner, &item);
        }
    } else if cmd >= CMD_WIN0 && cmd < CMD_WIN0 + nw as i32 {
        let hw = snap.wins[(cmd - CMD_WIN0) as usize];
        // SAFETY: IsWindow aman untuk handle apa pun.
        if unsafe { IsWindow(Some(hw)) }.as_bool() {
            activate_window(hw);
        }
    } else if cmd == CMD_APP_OPEN {
        launch_app(&snap.launch);
    } else if cmd == CMD_APP_CLOSE {
        for &hw in &snap.wins {
            // SAFETY: IsWindow dicek dulu; PostMessageW dengan SC_CLOSE sama seperti C.
            if unsafe { IsWindow(Some(hw)) }.as_bool() {
                let _ = unsafe { PostMessageW(Some(hw), WM_SYSCOMMAND, WPARAM(SC_CLOSE as usize), LPARAM(0)) };
            }
        }
    } else if cmd == CMD_APP_PIN {
        toggle_pin(&snap);
    } else if cmd == CMD_APP_SPIN {
        start_pin_toggle_ident(&ident);
    } else if cmd == CMD_APP_GROUP {
        if let Some(i) = app_index_by_identity(&snap) {
            {
                let mut apps = APPS.borrow_mut();
                if apps[i].pin >= 0 {
                    apps[i].pin = 1 - apps[i].pin.min(1);
                }
            }
            save_apps();
            relayout();
        }
    } else if cmd == CMD_APP_ADMIN {
        let wexe = to_wide_z(&snap.exe);
        // SAFETY: wexe hidup selama pemanggilan; verba "runas" memunculkan UAC.
        unsafe {
            ShellExecuteW(None, w!("runas"), pcw(&wexe), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        }
    } else if cmd == CMD_APP_LOC {
        if let Some((dir, _)) = snap.exe.rsplit_once('\\') {
            open_folder_window(dir);
        }
    }
    jump_list_free();
}

// ---------------------------------------------------------------- foreground, fullscreen, auto-hide Widgets Bar

/// `IsSystemFlyout(cls)`: kelas popup sistem (Start, Search, Quick Settings, Task View). Pil atas disembunyikan
/// selama popup tersebut terbuka.
pub(crate) fn is_system_flyout(cls: &str) -> bool {
    matches!(
        cls,
        "Windows.UI.Core.CoreWindow"
            | "XamlExplorerHostIslandWindow"
            | "TopLevelWindowForOverflowXamlIsland"
            | "ForegroundStaging"
            | "MultitaskingViewFrame"
            | "#32768"
            | "tooltips_class32"
            | "ControlCenterWindow" // Quick Settings (Win+A) dari dok: keadaan pill jam tidak boleh ikut berubah
    )
}

/// `UpdateForeground(force)`: perbarui keadaan fokus, fullscreen, dan flyout, lalu atur ulang pil.
/// Dipanggil dari hook WinEvent dan timer `T_REFRESH`.
pub fn update_foreground(force: bool) {
    // SAFETY: GetForegroundWindow tanpa parameter.
    let fg = unsafe { GetForegroundWindow() };
    let ada_fg = !fg.0.is_null();
    if ada_fg && is_our_window(fg) {
        // flyout dok diatur POP_OPEN; popup sistem sudah tertutup
        if POP_OPEN.get() {
            POP_OPEN.set(false);
            apply_pills(false);
        }
        return;
    }
    let cls: String = if ada_fg { window_class(fg) } else { String::new() };
    // Desktop aktif (klik desktop, Win+D): pastikan widget tetap tepat di atas desktop, termasuk bila Windows
    // menaikkan jendela desktop ke atas jendela lain saat "tampilkan desktop".
    let desk_cls = cls == "Progman" || cls == "WorkerW";
    if ada_fg && DESK_LAYER.get() && desk_cls {
        pills_to_desk(Some(fg));
    }
    if ada_fg && is_system_flyout(&cls) {
        // Start, Search, Quick Settings, Task View: pil atas disembunyikan selama terbuka.
        if !POP_OPEN.get() {
            POP_OPEN.set(true);
            apply_pills(false);
        }
        return;
    }
    let mut valid = false;
    let mut fs = false;
    let mut fr = RECT::default();
    if ada_fg
        && unsafe { IsWindowVisible(fg) }.as_bool()
        && !unsafe { IsIconic(fg) }.as_bool()
        && !desk_cls
        && cls != "Shell_TrayWnd"
        && cls != "Shell_SecondaryTrayWnd"
    {
        // SAFETY: fr adalah RECT yang hidup selama pemanggilan; ukuran dikirim sesuai struct.
        let dwm_ok = unsafe {
            DwmGetWindowAttribute(
                fg,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut fr as *mut RECT as *mut c_void,
                size_of::<RECT>() as u32,
            )
        }
        .is_ok();
        if !dwm_ok {
            let _ = unsafe { GetWindowRect(fg, &mut fr) };
        }
        valid = true;
        let mut wr = RECT::default();
        let _ = unsafe { GetWindowRect(fg, &mut wr) };
        // SAFETY: mi.cbSize diisi sesuai ukuran struct sebelum GetMonitorInfoW.
        let mut mi: MONITORINFO = unsafe { zeroed() };
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        let mon = unsafe { MonitorFromWindow(fg, MONITOR_DEFAULTTOPRIMARY) };
        if unsafe { GetMonitorInfoW(mon, &mut mi) }.as_bool() {
            fs = wr.left <= mi.rcMonitor.left
                && wr.top <= mi.rcMonitor.top
                && wr.right >= mi.rcMonitor.right
                && wr.bottom >= mi.rcMonitor.bottom;
        }
    }
    if valid {
        LAST_APP.set(fg);
    } else {
        // Win+D, Win+M, atau minimize: app terakhir diminimalkan lalu desktop aktif. Desktop sengaja dibuka, jadi masa
        // tenang dihentikan dan widget langsung tampil (permintaan pengguna 2026-11-06). Saat app ditutup, jendelanya
        // sudah hilang (IsWindow gagal), jadi masa tenang tetap menahan pil seperti sebelumnya.
        let la = LAST_APP.get();
        let la_minimized = !la.0.is_null()
            && unsafe { IsWindow(Some(la)) }.as_bool()
            && unsafe { IsIconic(la) }.as_bool();
        if QUIET.get() && FG_VALID.get() && ada_fg && desk_cls && la_minimized {
            QUIET.set(false);
            kill_timer(T_QUIET);
        }
    }
    // Masa tenang: transisi sementara (desktop, bilah tugas, tanpa fokus) tidak dicatat.
    if QUIET.get() && !valid {
        return;
    }
    let pop_was = POP_OPEN.get();
    POP_OPEN.set(false);
    let old_rect = FG_RECT.get();
    // SAFETY: kedua RECT hidup selama pemanggilan.
    let same_rect = unsafe { EqualRect(&fr, &old_rect) }.as_bool();
    // Jenis layar penuh untuk notch pintar: video (peramban/pemutar) atau game (selain itu).
    let kind = if fs { fs_kind(fg) } else { 0 };
    let kind_same = kind == super::state::FS_KIND.get();
    super::state::FS_KIND.set(kind);
    if !force && !pop_was && kind_same && valid == FG_VALID.get() && fs == FULLSCREEN.get() && (!valid || same_rect) {
        apply_pills(false); // keadaan fokus sama, tetapi flyout dok mungkin baru ditutup
        base_update(false); // jendela lain bisa saja baru dipulihkan/diminimalkan
        return;
    }
    FG_VALID.set(valid);
    FG_RECT.set(fr);
    FULLSCREEN.set(fs);
    // Mode game: kait keyboard membaca GAME_FS. Dengan gamehook=0 (di luar mode shell) kait dilepas selama fullscreen.
    let game_was = GAME_FS.swap(fs, std::sync::atomic::Ordering::Relaxed);
    if game_was != fs && !GAME_HOOK.get() && !SHELL_MODE.get() {
        if fs {
            crate::shell::core_c4b::kb_hook_stop();
            log("mode game: fullscreen, kait keyboard dilepas (gamehook=0)");
        } else {
            crate::shell::core_c4b::kb_hook_start();
            log("mode game: keluar fullscreen, kait keyboard aktif lagi");
        }
    }
    base_update(false); // sebelum release_mem: alas yang memudar keluar masih memakai buffer bersama
    if fs {
        // Dock tidak tampil saat fullscreen: lepas buffer dan kecilkan working set.
        release_mem();
        // C++ win12shell.cpp:3949 memanggil TrimWorkingSet(true) langsung (bukan TrimSoon).
        crate::shell::core_c4::trim_working_set(true);
    }
    apply_pills(false);
}

/// Proses yang layar penuhnya dianggap video. `[shell] notchvideoapps` menambah nama exe lain (dipisah koma,
/// tanpa ".exe"). Semua layar penuh lain dianggap game.
const FS_VIDEO_EXES: &[&str] = &[
    "zen", "firefox", "chrome", "msedge", "brave", "opera", "opera_gx", "vivaldi", "librewolf", "floorp",
    "waterfox", "thorium", "chromium", "arc", "vlc", "mpv", "mpc-hc", "mpc-hc64", "mpc-be", "mpc-be64",
    "potplayer", "potplayermini", "potplayermini64", "wmplayer", "applicationframehost", "microsoft.media.player",
    "kodi", "plex", "stremio", "spotify", "netflix", "smplayer", "mpvnet", "celluloid", "obs64",
];

/// Jendela terakhir yang jenisnya sudah dihitung: T_FGPOLL (500 ms) tidak membuka proses berulang selama layar penuh.
static FS_KIND_CACHE: SyncCell<Cell<(isize, u8)>> = SyncCell::new(Cell::new((0, 0)));

/// 1 = video, 2 = game.
fn fs_kind(h: HWND) -> u8 {
    let (ch, ck) = FS_KIND_CACHE.get();
    if ch == h.0 as isize && ck != 0 {
        return ck;
    }
    let k = fs_kind_uncached(h);
    FS_KIND_CACHE.set((h.0 as isize, k));
    k
}

fn fs_kind_uncached(h: HWND) -> u8 {
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let mut pid = 0u32;
    // SAFETY: pid lokal; h dari GetForegroundWindow.
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid as *mut u32)) };
    if pid == 0 {
        return 2;
    }
    let mut name = String::new();
    // SAFETY: handle proses ditutup sebelum keluar; buffer lokal.
    unsafe {
        if let Ok(hp) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = [0u16; 260];
            let mut n = buf.len() as u32;
            if QueryFullProcessImageNameW(hp, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n)
                .is_ok()
            {
                name = String::from_utf16_lossy(&buf[..n as usize]);
            }
            let _ = windows::Win32::Foundation::CloseHandle(hp);
        }
    }
    let base = name.rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
    let base = base.strip_suffix(".exe").unwrap_or(&base).to_string();
    if base.is_empty() {
        return 2;
    }
    if FS_VIDEO_EXES.contains(&base.as_str()) {
        return 1;
    }
    let extra = super::core_c1::ini_string("shell", "notchvideoapps", "", 512).to_ascii_lowercase();
    if extra.split(',').map(|s| s.trim().trim_end_matches(".exe")).any(|s| !s.is_empty() && s == base) {
        return 1;
    }
    2
}

/// `ScheduleRefresh()`: jadwalkan penyegaran keadaan fokus 60 ms lagi (timer `T_REFRESH`).
pub fn schedule_refresh() {
    set_timer(T_REFRESH, 60);
}

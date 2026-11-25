//! Port win12files.cpp:518-618 (FmWndProc) dan 642-653 (FmMsgProc). Status: belum diuji runtime; hanya cargo check.
//! Pola: tiap cabang menghasilkan Option<LRESULT>; None berarti DefWindowProcW. Body fn sudah unsafe (edisi 2021).

use std::ffi::c_void;
use std::mem::zeroed;
use std::slice::from_raw_parts;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DeleteObject, EndPaint, GetStockObject, InvalidateRect, SetBkColor, SetTextColor, DEFAULT_GUI_FONT, HDC,
    HGDIOBJ, PAINTSTRUCT,
};
use windows::Win32::Graphics::Dwm::DwmDefWindowProc;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, ShowWindow, EN_KILLFOCUS, EN_SETFOCUS, SWP_FRAMECHANGED, SWP_NOMOVE,
    SWP_NOSIZE, SW_HIDE, WINDOW_EX_STYLE, WM_ACTIVATE, WM_ACTIVATEAPP, WM_COMMAND, SetTimer, WM_NCCALCSIZE, WM_NCHITTEST, DestroyIcon, KillTimer, PostQuitMessage, SendMessageW, SetWindowPos, CREATESTRUCTW,
    ES_AUTOHSCROLL, HICON, HMENU, ICON_SMALL, SWP_NOACTIVATE, SWP_NOZORDER, WINDOW_STYLE, WM_COPYDATA, WM_CREATE,
    WM_CTLCOLOREDIT, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETFONT, WM_SETICON, WM_SIZE, WM_TIMER,
    WS_CHILD, WS_EX_CLIENTEDGE, WS_VISIBLE,
};

use crate::fm_logic::W12FM_MAXTABS;
use crate::files::app::{dpi_of, fm_layout, fm_tab_destroy, fm_win_of, px};
use crate::files::bar::{fm_init_bar, fm_mouse, fm_paint, fm_refresh_selection, fm_rgb};
use crate::files::frame::{fm_count, fm_hinstance, fm_open_from_command_line, fm_take_by_hwnd};
use crate::files::search_ui::{fm_search_discard, fm_search_done};
use crate::files::state::{FmWin, COPY_TAG, FM_CMDH_DIP, FM_NAVH_DIP, FM_TABH_DIP, FM_TIMER_SEL, MSG_SEARCH_DONE};
use crate::wide::{from_wide, pcw, to_wide_z};

/// Membuat child EDIT (id 101 = alamat, 102 = pencarian). None bila gagal.
fn make_edit(parent: HWND, id: usize, inst: HINSTANCE) -> Option<HWND> {
    let cls = to_wide_z("EDIT");
    let empty = to_wide_z("");
    let style = WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32);
    // SAFETY: buffer UTF-16 lokal hidup selama panggilan; parent berasal dari sistem.
    let created = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            pcw(&cls),
            pcw(&empty),
            style,
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(id as *mut c_void)),
            Some(inst),
            None,
        )
    };
    match created {
        Ok(x) if !x.0.is_null() => Some(x),
        _ => None,
    }
}

/// WM_CREATE (win12files.cpp:541-566): dua EDIT, font default, lalu isi bilah.
fn on_create(h: HWND) -> LRESULT {
    if fm_win_of(h).is_none() {
        return LRESULT(-1);
    }
    let inst = fm_hinstance();
    let addr = make_edit(h, 101, inst);
    let search = make_edit(h, 102, inst);
    let (addr, search) = match (addr, search) {
        (Some(a), Some(s)) => (a, s),
        _ => return LRESULT(-1),
    };
    if let Some(w) = fm_win_of(h) {
        w.addr = addr;
        w.search = search;
    }
    // Segoe UI 9 pt seperti Explorer; dihapus di WM_NCDESTROY.
    let font = crate::files::chrome::ui_font(dpi_of(h));
    if let Some(w) = fm_win_of(h) {
        w.font = font;
    }
    // SAFETY: addr dan search baru dibuat di atas dan masih valid.
    unsafe {
        let _ = SendMessageW(addr, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(0)));
        let _ = SendMessageW(search, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(0)));
    }
    // Kotak alamat mulai sebagai breadcrumb; EDIT tampil saat diklik atau Ctrl+L.
    unsafe {
        let _ = ShowWindow(addr, SW_HIDE);
    }
    if let Some(w) = fm_win_of(h) {
        fm_init_bar(w);
    }
    // Tab di bilah judul: hitung ulang area non-klien lalu perluas bingkai DWM.
    unsafe {
        let _ = SetWindowPos(h, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
    crate::files::chrome::extend_frame(h);
    LRESULT(0)
}

/// WM_SIZE: atur ulang tata letak, lalu invalidasi area bilah atas.
fn on_size(h: HWND, lp: LPARAM) -> LRESULT {
    match fm_win_of(h) {
        Some(w) => fm_layout(w),
        None => return LRESULT(0),
    }
    let _ = lp;
    crate::files::chrome::extend_frame(h);
    // Bilah atas dan bilah status ikut berubah; anak jendela dikecualikan (WS_CLIPCHILDREN).
    unsafe {
        let _ = InvalidateRect(Some(h), None, false);
    }
    LRESULT(0)
}

/// WM_DPICHANGED: pakai persegi yang disarankan sistem (lParam).
fn on_dpi_changed(h: HWND, lp: LPARAM) -> LRESULT {
    if lp.0 != 0 {
        // SAFETY: sistem memberi RECT yang valid untuk lParam WM_DPICHANGED.
        let sr = unsafe { *(lp.0 as *const RECT) };
        // SAFETY: h adalah jendela frame yang sedang diproses.
        unsafe {
            let _ = SetWindowPos(
                h,
                None,
                sr.left,
                sr.top,
                sr.right - sr.left,
                sr.bottom - sr.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    if let Some(w) = fm_win_of(h) {
        let font = crate::files::chrome::ui_font(dpi_of(h));
        unsafe {
            let _ = SendMessageW(w.addr, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
            let _ = SendMessageW(w.search, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
            if !w.font.0.is_null() {
                let _ = DeleteObject(HGDIOBJ(w.font.0));
            }
        }
        w.font = font;
        fm_layout(w);
    }
    LRESULT(0)
}

/// WM_PAINT: BeginPaint, FmPaint bila jendela ada, lalu EndPaint.
fn on_paint(h: HWND) -> LRESULT {
    let mut ps: PAINTSTRUCT = unsafe { zeroed() };
    // SAFETY: ps hidup selama panggilan; h adalah jendela frame.
    let dc = unsafe { BeginPaint(h, &mut ps) };
    if let Some(w) = fm_win_of(h) {
        fm_paint(w, dc);
    }
    // SAFETY: pasangan BeginPaint/EndPaint untuk h.
    unsafe {
        let _ = EndPaint(h, &ps);
    }
    LRESULT(0)
}

/// WM_CTLCOLOREDIT: warna teks dan latar kotak edit. None bila kuas belum ada (C++ break ke DefWindowProc).
fn on_ctl_color_edit(h: HWND, wp: WPARAM) -> Option<LRESULT> {
    let w = fm_win_of(h)?;
    if w.edit_brush.0.is_null() {
        return None;
    }
    let hdc = HDC(wp.0 as *mut c_void);
    let text = fm_rgb(w.pal.text);
    let panel = fm_rgb(crate::files::chrome::box_color(w.dark));
    // SAFETY: wp berisi HDC yang diberikan sistem untuk WM_CTLCOLOREDIT.
    unsafe {
        let _ = SetTextColor(hdc, text);
        let _ = SetBkColor(hdc, panel);
    }
    Some(LRESULT(w.edit_brush.0 as isize))
}

/// WM_DESTROY: hentikan timer, lepas semua tab, lalu lepas ikon kecil yang dipasang.
fn on_destroy(h: HWND) -> LRESULT {
    // SAFETY: h adalah jendela frame yang sedang dihancurkan.
    unsafe {
        let _ = KillTimer(Some(h), FM_TIMER_SEL);
    }
    if let Some(w) = fm_win_of(h) {
        let n = w.tabs.n.clamp(0, W12FM_MAXTABS as i32) as usize;
        for t in w.tab.iter_mut().take(n) {
            fm_tab_destroy(t);
        }
    }
    // SAFETY: WM_SETICON dengan ICON_SMALL hanya mengembalikan ikon lama.
    let icon = unsafe { SendMessageW(h, WM_SETICON, Some(WPARAM(ICON_SMALL as usize)), Some(LPARAM(0))) };
    if icon.0 != 0 {
        // SAFETY: ikon ini dibuat oleh pemanggil dan belum dilepas.
        unsafe {
            let _ = DestroyIcon(HICON(icon.0 as *mut c_void));
        }
    }
    LRESULT(0)
}

/// WM_NCDESTROY: lepas FmWin dan kuas edit. Bila jendela terakhir, keluar dari loop pesan.
fn on_nc_destroy(h: HWND) -> Option<LRESULT> {
    if let Some(boxed) = fm_take_by_hwnd(h) {
        if !boxed.font.0.is_null() {
            // SAFETY: font dibuat on_create/on_dpi_changed dan hanya dilepas di sini.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(boxed.font.0));
            }
        }
        if !boxed.edit_brush.0.is_null() {
            // SAFETY: kuas dibuat oleh FmInitBar dan hanya dilepas di sini.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(boxed.edit_brush.0));
            }
        }
        drop(boxed);
    }
    if fm_count() == 0 {
        // SAFETY: PostQuitMessage aman dipanggil dari window proc.
        unsafe {
            PostQuitMessage(0);
        }
    }
    None
}

/// WM_TIMER: segarkan pilihan bila timer pilihan (FM_TIMER_SEL) berbunyi.
fn on_timer(h: HWND, wp: WPARAM) -> LRESULT {
    if wp.0 == FM_TIMER_TRIM {
        let _ = unsafe { KillTimer(Some(h), FM_TIMER_TRIM) };
        fm_trim();
        return LRESULT(0);
    }
    if wp.0 == crate::files::watch::FM_TIMER_WATCH {
        if let Some(w) = fm_win_of(h) {
            crate::files::watch::on_timer(w);
        } else {
            let _ = unsafe { KillTimer(Some(h), crate::files::watch::FM_TIMER_WATCH) };
        }
        return LRESULT(0);
    }
    if wp.0 == crate::files::viewstate::FM_TIMER_FILL {
        let _ = unsafe { KillTimer(Some(h), crate::files::viewstate::FM_TIMER_FILL) };
        if let Some(w) = fm_win_of(h) {
            if let Some(t) = crate::files::app::fm_active(w) {
                crate::files::viewstate::fill_columns(t);
            }
        }
        return LRESULT(0);
    }
    if wp.0 == FM_TIMER_SEL {
        if let Some(w) = fm_win_of(h) {
            fm_refresh_selection(w);
        }
    }
    LRESULT(0)
}

/// MSG_SEARCH_DONE: hasil pencarian dari thread pencari. Tanpa jendela, hasilnya dibuang.
fn on_search_done(h: HWND, lp: LPARAM) -> LRESULT {
    match fm_win_of(h) {
        // SAFETY: lp adalah hasil pencarian yang dikirim thread pencari untuk jendela ini.
        Some(w) => unsafe { fm_search_done(w, lp) },
        // SAFETY: lp tetap milik kita dan harus dibebaskan bila jendela sudah tidak ada.
        None => unsafe { fm_search_discard(lp) },
    }
    LRESULT(0)
}

const FM_TIMER_TRIM: usize = 7;

/// Lepas working set proses win12files (SetProcessWorkingSetSize -1,-1 = EmptyWorkingSet).
fn fm_trim() {
    use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
    let _ = unsafe { SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX) };
}

/// WM_COMMAND dari kotak EDIT: alamat kehilangan fokus = kembali ke breadcrumb; fokus pencarian = garis aksen.
fn on_command(h: HWND, wp: WPARAM) {
    let id = (wp.0 & 0xFFFF) as u32;
    let code = ((wp.0 >> 16) & 0xFFFF) as u32;
    let Some(w) = fm_win_of(h) else {
        return;
    };
    if id == 101 && code == EN_KILLFOCUS {
        crate::files::chrome::addr_edit(w, false);
    } else if id == 102 && (code == EN_SETFOCUS || code == EN_KILLFOCUS) {
        unsafe {
            let _ = InvalidateRect(Some(h), None, false);
        }
    }
}

/// Window proc frame file manager (FmWndProc, win12files.cpp:518-618).
pub unsafe extern "system" fn fm_wnd_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Tombol jendela (min/maks/tutup) digambar dan diuji DWM di area bingkai yang diperluas.
    // Klik tombol jendela ditangani sendiri (DwmDefWindowProc tidak selalu mengenalinya, lihat caption_button_at).
    if matches!(msg, 0x00A1..=0x00A3) {
        // WM_NCLBUTTONDOWN / WM_NCLBUTTONUP / WM_NCLBUTTONDBLCLK
        if let Some(r) = crate::files::chrome::nc_caption_button(h, msg, wp) {
            return r;
        }
    }
    match msg {
        // WM_NCMOUSEMOVE: sorot tombol di bawah kursor; WM_NCMOUSELEAVE dilacak (TME_NONCLIENT).
        0x00A0 => {
            crate::files::chrome::cap_set_hot(h, wp.0 as u32);
            let mut tme = windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT {
                cbSize: size_of::<windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT>() as u32,
                dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::TME_LEAVE
                    | windows::Win32::UI::Input::KeyboardAndMouse::TME_NONCLIENT,
                hwndTrack: h,
                dwHoverTime: 0,
            };
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::TrackMouseEvent(&mut tme);
        }
        // WM_NCMOUSELEAVE, atau kursor masuk ke area klien.
        0x02A2 | 0x0200 => crate::files::chrome::cap_set_hot(h, 0),
        WM_TIMER if wp.0 == crate::files::chrome::FM_TIMER_CAP => {
            crate::files::chrome::cap_tick(h);
            return LRESULT(0);
        }
        // WM_ACTIVATE / WM_NCACTIVATE: glyph meredup saat jendela tidak aktif.
        0x0006 | 0x0086 => crate::files::chrome::cap_refresh(h),
        WM_NCDESTROY => crate::files::chrome::cap_forget(h),
        _ => {}
    }
    let mut dwm = LRESULT(0);
    if DwmDefWindowProc(h, msg, wp, lp, &mut dwm).as_bool() {
        return dwm;
    }
    let handled: Option<LRESULT> = match msg {
        WM_NCCALCSIZE => crate::files::chrome::nc_calcsize(h, wp, lp),
        WM_NCHITTEST => Some(crate::files::chrome::nc_hittest(h, wp, lp)),
        WM_ACTIVATE => {
            crate::files::chrome::extend_frame(h);
            None
        }
        WM_COMMAND => {
            on_command(h, wp);
            None
        }
        WM_NCCREATE => {
            // lpCreateParams = FmWin* yang dipasang FmOpen sebelum CreateWindowExW.
            let cs = lp.0 as *const CREATESTRUCTW;
            if !cs.is_null() {
                let p = (*cs).lpCreateParams as *mut FmWin;
                if !p.is_null() {
                    (*p).h = h;
                }
            }
            None
        }
        WM_CREATE => Some(on_create(h)),
        WM_NCDESTROY => on_nc_destroy(h),
        WM_DESTROY => Some(on_destroy(h)),
        WM_SIZE => {
            if wp.0 == 1 {
                // SIZE_MINIMIZED: lepas working set (halaman kembali dimuat saat dipakai lagi).
                fm_trim();
            }
            Some(on_size(h, lp))
        }
        WM_ACTIVATEAPP => {
            // Tidak aktif 30 dtk -> working set dilepas; aktif lagi -> batalkan.
            if wp.0 == 0 {
                let _ = SetTimer(Some(h), FM_TIMER_TRIM, 30_000, None);
            } else {
                let _ = KillTimer(Some(h), FM_TIMER_TRIM);
            }
            None
        }
        WM_DPICHANGED => Some(on_dpi_changed(h, lp)),
        WM_ERASEBKGND => Some(LRESULT(1)),
        WM_PAINT => Some(on_paint(h)),
        WM_CTLCOLOREDIT => on_ctl_color_edit(h, wp),
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_LBUTTONUP | WM_MBUTTONDOWN | WM_MOUSEMOVE | WM_MOUSELEAVE | 0x0205 => {
            if let Some(w) = fm_win_of(h) {
                fm_mouse(w, msg, lp);
            }
            Some(LRESULT(0))
        }
        MSG_SEARCH_DONE => Some(on_search_done(h, lp)),
        // Menu "Baru" asli: ikon dan isi submenu dari IContextMenu3 selama menu terbuka.
        0x0117 | 0x002B | 0x002C | 0x0120 => crate::files::extras::menu_msg(msg, wp, lp),
        crate::files::term::WM_FM_TERM_CWD => {
            if lp.0 != 0 {
                // SAFETY: pengirim (term.rs osc_done) menyerahkan Box<String> lewat LPARAM.
                let path: String = *unsafe { Box::from_raw(lp.0 as *mut String) };
                if let Some(w) = fm_win_of(h) {
                    crate::files::extras::term_cwd(w, &path);
                }
            }
            Some(LRESULT(0))
        }
        crate::files::term::WM_FM_TERM_CLOSED => {
            if let Some(w) = fm_win_of(h) {
                crate::files::extras::term_closed(w, HWND(lp.0 as *mut c_void));
            }
            Some(LRESULT(0))
        }
        // Tombol browser keyboard/mouse (WM_APPCOMMAND): Kembali, Maju, Segarkan, Cari, Beranda.
        0x0319 => {
            let cmd = ((lp.0 as usize >> 16) & 0x0FFF) as u32;
            match fm_win_of(h) {
                Some(w) => match cmd {
                    1 => {
                        crate::files::bar::fm_hist_go(w, -1);
                        Some(LRESULT(1))
                    }
                    2 => {
                        crate::files::bar::fm_hist_go(w, 1);
                        Some(LRESULT(1))
                    }
                    3 => {
                        crate::files::frame::fm_command(w, crate::fm_logic::W12FA_REFRESH, -1);
                        Some(LRESULT(1))
                    }
                    5 => {
                        crate::files::frame::fm_command(w, crate::fm_logic::W12FA_FOCUS_SEARCH, -1);
                        Some(LRESULT(1))
                    }
                    7 => {
                        crate::files::bar::fm_navigate_home(w);
                        Some(LRESULT(1))
                    }
                    _ => None,
                },
                None => None,
            }
        }
        // Kursor ubah ukuran di atas pembatas panel terminal.
        0x0020 => {
            let hit_client = (lp.0 & 0xFFFF) as u32 == 1;
            match fm_win_of(h) {
                Some(w) if hit_client && wp.0 as isize == h.0 as isize => {
                    let mut pt = windows::Win32::Foundation::POINT::default();
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
                    let _ = windows::Win32::Graphics::Gdi::ScreenToClient(h, &mut pt);
                    let hit = crate::files::chrome::hit_test(w, pt.x, pt.y);
                    if hit.kind == crate::files::state::FM_HIT_SPLIT
                        || w.press.kind == crate::files::state::FM_HIT_SPLIT
                    {
                        if let Ok(c) = windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                            None,
                            windows::Win32::UI::WindowsAndMessaging::IDC_SIZENS,
                        ) {
                            windows::Win32::UI::WindowsAndMessaging::SetCursor(Some(c));
                        }
                        Some(LRESULT(1))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        WM_TIMER => Some(on_timer(h, wp)),
        _ => None,
    };
    match handled {
        Some(r) => r,
        None => DefWindowProcW(h, msg, wp, lp),
    }
}

/// Isi WM_COPYDATA dari instance kedua (win12files.cpp:642-653 dan 621-627).
fn on_copy_data(lp: LPARAM) -> LRESULT {
    let cds = lp.0 as *const COPYDATASTRUCT;
    if cds.is_null() {
        return LRESULT(0);
    }
    // SAFETY: sistem memberi COPYDATASTRUCT yang valid selama panggilan WM_COPYDATA ini.
    let c = unsafe { &*cds };
    if c.dwData != COPY_TAG || c.lpData.is_null() || (c.cbData as usize) < 2 {
        return LRESULT(0);
    }
    let chars = (c.cbData as usize) / 2;
    // SAFETY: lpData berisi cbData byte; chars = cbData/2 unit UTF-16 di dalamnya.
    let units = unsafe { from_raw_parts(c.lpData as *const u16, chars) };
    if units[chars - 1] == 0 {
        let text = from_wide(units);
        fm_open_from_command_line(&text);
    }
    LRESULT(1)
}

/// Window proc jendela pesan tersembunyi (FmMsgProc, win12files.cpp:642-653).
pub unsafe extern "system" fn fm_msg_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_COPYDATA {
        return on_copy_data(lp);
    }
    DefWindowProcW(h, msg, wp, lp)
}

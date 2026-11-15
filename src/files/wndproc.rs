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
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyIcon, KillTimer, PostQuitMessage, SendMessageW, SetWindowPos, CREATESTRUCTW,
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
            WS_EX_CLIENTEDGE,
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
    // SAFETY: GetStockObject mengembalikan handle stok sistem.
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    // SAFETY: addr dan search baru dibuat di atas dan masih valid.
    unsafe {
        let _ = SendMessageW(addr, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(0)));
        let _ = SendMessageW(search, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(0)));
    }
    if let Some(w) = fm_win_of(h) {
        fm_init_bar(w);
    }
    LRESULT(0)
}

/// WM_SIZE: atur ulang tata letak, lalu invalidasi area bilah atas.
fn on_size(h: HWND, lp: LPARAM) -> LRESULT {
    match fm_win_of(h) {
        Some(w) => fm_layout(w),
        None => return LRESULT(0),
    }
    let dpi = dpi_of(h);
    let bar = RECT {
        left: 0,
        top: 0,
        right: (lp.0 & 0xFFFF) as i32,
        bottom: px(FM_TABH_DIP + FM_NAVH_DIP + FM_CMDH_DIP, dpi),
    };
    // SAFETY: bar hidup selama panggilan; h adalah jendela frame.
    unsafe {
        let _ = InvalidateRect(Some(h), Some(&bar as *const RECT), false);
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
    let panel = fm_rgb(w.pal.panel);
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

/// Window proc frame file manager (FmWndProc, win12files.cpp:518-618).
pub unsafe extern "system" fn fm_wnd_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let handled: Option<LRESULT> = match msg {
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
        WM_SIZE => Some(on_size(h, lp)),
        WM_DPICHANGED => Some(on_dpi_changed(h, lp)),
        WM_ERASEBKGND => Some(LRESULT(1)),
        WM_PAINT => Some(on_paint(h)),
        WM_CTLCOLOREDIT => on_ctl_color_edit(h, wp),
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_LBUTTONUP | WM_MBUTTONDOWN | WM_MOUSEMOVE | WM_MOUSELEAVE => {
            if let Some(w) = fm_win_of(h) {
                fm_mouse(w, msg, lp);
            }
            Some(LRESULT(0))
        }
        MSG_SEARCH_DONE => Some(on_search_done(h, lp)),
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

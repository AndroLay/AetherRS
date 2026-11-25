//! Bingkai win12files bergaya File Explorer Windows 11 (2026-11-18, keputusan pengguna "tetap win12files, dimiripkan").
//! Isi: tab di bilah judul (DwmExtendFrameIntoClientArea + WM_NCCALCSIZE), baris alamat breadcrumb dengan tombol
//! navigasi ikon, baris perintah ikon + label, bilah status (jumlah item, pilihan, tombol tampilan).
//! Menggantikan geometri, uji titik, dan fungsi gambar lama di bar.rs. Isi folder tetap IExplorerBrowser asli.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmGetWindowAttribute, DWMWA_CAPTION_BUTTON_BOUNDS,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, FillRect, GetStockObject, GetTextExtentPoint32W,
    InvalidateRect, RoundRect, ScreenToClient, SelectObject, SetBkMode, SetTextColor, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DRAW_TEXT_FORMAT,
    DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL, HDC, HFONT,
    HGDIOBJ, NULL_PEN, OUT_DEFAULT_PRECIS, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Controls::{
    ImageList_Draw, EM_SETSEL, HIMAGELIST, ILD_TRANSPARENT, MARGINS, TTM_UPDATETIPTEXTW, TTTOOLINFOW,
};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows::Win32::UI::Shell::{ILClone, ILRemoveLastID, SHGetNameFromIDList, SIGDN_NORMALDISPLAY};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetClientRect, GetWindowRect, IsZoomed, PostMessageW, SendMessageW, ShowWindow, HTCAPTION,
    HTCLIENT, HTCLOSE, HTMAXBUTTON, HTMINBUTTON, HTTOP, HTTOPLEFT, HTTOPRIGHT, NCCALCSIZE_PARAMS, SC_CLOSE,
    SC_MAXIMIZE, SC_MINIMIZE, SC_RESTORE, SM_CXPADDEDBORDER, SM_CYFRAME, SW_HIDE, SW_SHOW, WM_NCCALCSIZE,
    WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_SYSCOMMAND,
};

use crate::files::app::{fm_active, fm_browse, fm_win_of, free_pidl};
use crate::files::state::{
    FmHit, FmWin, FM_CMDH_DIP, FM_HIT_ADDR, FM_HIT_CMD, FM_HIT_CRUMB, FM_HIT_NAV, FM_HIT_NONE, FM_HIT_PLUS,
    FM_HIT_TAB, FM_HIT_TABCLOSE, FM_HIT_VIEWBTN, FM_HIT_SPLIT, FM_HIT_TERMBTN, FM_HIT_CHEV, FM_NAVH_DIP, FM_STATH_DIP, FM_TABH_DIP, FM_TOOL_ID, NAV_TIP,
};
use crate::fm_logic::w12_fm_cmd_enabled;
use crate::wide::{pcw, to_wide, to_wide_z};

// ---- perintah bilah (indeks 0..7 = W12FC_*, 8 = Lainnya) ----

/// Jumlah tombol baris perintah: 8 kode W12FC_* ditambah "Lainnya".
pub(crate) const CMD_N: usize = 9;
pub(crate) const CMD_MORE: i32 = 8;
const CMD_W_DIP: [i32; CMD_N] = [84, 40, 40, 40, 40, 40, 104, 110, 40];
const CMD_GLYPH: [&str; CMD_N] = [
    "\u{E710}", "\u{E8C6}", "\u{E8C8}", "\u{E77F}", "\u{E8AC}", "\u{E74D}", "\u{E8CB}", "\u{E80A}", "\u{E712}",
];
const CMD_TEXT: [&str; CMD_N] = ["Baru", "", "", "", "", "", "Urutkan", "Tampilan", ""];
const CMD_CARET: [bool; CMD_N] = [false, false, false, false, false, false, true, true, false];
pub(crate) const CMD_TIP2: [&str; CMD_N] = [
    "Folder baru (Ctrl+Shift+N)",
    "Potong (Ctrl+X)",
    "Salin (Ctrl+C)",
    "Tempel (Ctrl+V)",
    "Ganti nama (F2)",
    "Hapus (Del)",
    "Urutkan",
    "Tampilan",
    "Lainnya",
];
/// Garis pemisah sesudah tombol ini (Baru | edit | urut-tampilan | Lainnya).
const CMD_SEP_AFTER: [bool; CMD_N] = [true, false, false, false, false, true, false, true, false];
const NAV_GLYPH: [&str; 4] = ["\u{E72B}", "\u{E72A}", "\u{E74A}", "\u{E72C}"];
const VIEW_GLYPH: [&str; 2] = ["\u{E8FD}", "\u{E80A}"];
pub(crate) const VIEW_TIP: [&str; 2] = ["Detail", "Ikon besar"];
const ICON_FACE: &str = "Segoe Fluent Icons";

// ---- palet ----

#[derive(Clone, Copy)]
pub(crate) struct Pal {
    pub tabrow: u32,
    pub layer: u32,
    pub box_fill: u32,
    pub box_line: u32,
    pub line: u32,
    pub sep: u32,
    pub text: u32,
    pub dim: u32,
    pub hover: u32,
    pub press: u32,
    pub tab_hover: u32,
    pub status: u32,
}

/// Warna mendekati File Explorer Windows 11 (terang/gelap). 0xRRGGBB.
pub(crate) fn pal(dark: bool) -> Pal {
    if dark {
        Pal {
            tabrow: 0x202020,
            layer: 0x2C2C2C,
            box_fill: 0x373737,
            box_line: 0x404040,
            line: 0x1D1D1D,
            sep: 0x484848,
            text: 0xFFFFFF,
            dim: 0x6E6E6E,
            hover: 0x383838,
            press: 0x323232,
            tab_hover: 0x2D2D2D,
            status: 0x191919,
        }
    } else {
        Pal {
            tabrow: 0xF3F3F3,
            layer: 0xF9F9F9,
            box_fill: 0xFFFFFF,
            box_line: 0xE3E3E3,
            line: 0xE5E5E5,
            sep: 0xD6D6D6,
            text: 0x1A1A1A,
            dim: 0xA0A0A0,
            hover: 0xEDEDED,
            press: 0xE4E4E4,
            tab_hover: 0xEAEAEA,
            status: 0xFFFFFF,
        }
    }
}

/// Latar kotak alamat/pencarian (dipakai WM_CTLCOLOREDIT).
pub(crate) fn box_color(dark: bool) -> u32 {
    pal(dark).box_fill
}

/// Warna lebar bilah judul (DWMWA_CAPTION_COLOR) agar area tombol jendela menyatu dengan baris tab.
pub(crate) fn caption_color(dark: bool) -> u32 {
    pal(dark).tabrow
}

fn rgb(c: u32) -> COLORREF {
    COLORREF(((c >> 16) & 0xFF) | (c & 0xFF00) | ((c & 0xFF) << 16))
}

// ---- util ----

fn px(dip: i32, dpi: i32) -> i32 {
    ((dip as i64 * dpi as i64 + 48) / 96) as i32
}

fn dpi_of(h: HWND) -> i32 {
    let d = unsafe { GetDpiForWindow(h) } as i32;
    if d > 0 {
        d
    } else {
        96
    }
}

fn inside(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

/// Font GDI ClearType dari ukuran poin.
pub(crate) fn font(dpi: i32, pt: i32, face: &str) -> HFONT {
    let name = to_wide_z(face);
    unsafe {
        CreateFontW(
            -((pt as i64 * dpi as i64 + 36) / 72) as i32,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            pcw(&name),
        )
    }
}

/// Font UI kotak edit (Segoe UI 9 pt, seperti Explorer).
pub(crate) fn ui_font(dpi: i32) -> HFONT {
    font(dpi, 9, "Segoe UI")
}

fn fill(dc: HDC, r: &RECT, c: u32) {
    unsafe {
        let b = CreateSolidBrush(rgb(c));
        if b.0.is_null() {
            return;
        }
        let _ = FillRect(dc, r, b);
        let _ = DeleteObject(HGDIOBJ(b.0));
    }
}

/// Persegi bersudut bulat tanpa garis tepi.
fn round_fill(dc: HDC, r: &RECT, radius: i32, c: u32) {
    if r.right <= r.left || r.bottom <= r.top {
        return;
    }
    unsafe {
        let b = CreateSolidBrush(rgb(c));
        if b.0.is_null() {
            return;
        }
        let ob = SelectObject(dc, HGDIOBJ(b.0));
        let op = SelectObject(dc, GetStockObject(NULL_PEN));
        let _ = RoundRect(dc, r.left, r.top, r.right + 1, r.bottom + 1, radius * 2, radius * 2);
        let _ = SelectObject(dc, op);
        let _ = SelectObject(dc, ob);
        let _ = DeleteObject(HGDIOBJ(b.0));
    }
}

fn text(dc: HDC, r: &RECT, s: &str, c: u32, f: HFONT, fmt: DRAW_TEXT_FORMAT) {
    let mut w = to_wide(s);
    text_w(dc, r, &mut w, c, f, fmt);
}

fn text_w(dc: HDC, r: &RECT, s: &mut [u16], c: u32, f: HFONT, fmt: DRAW_TEXT_FORMAT) {
    let mut rr = *r;
    unsafe {
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let _ = SetTextColor(dc, rgb(c));
        let _ = SetBkMode(dc, TRANSPARENT);
        let _ = DrawTextW(dc, s, &mut rr, fmt | DT_SINGLELINE | DT_NOPREFIX);
        let _ = SelectObject(dc, old);
    }
}

fn text_width(dc: HDC, s: &[u16], f: HFONT) -> i32 {
    let mut sz = SIZE::default();
    unsafe {
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let _ = GetTextExtentPoint32W(dc, s, &mut sz);
        let _ = SelectObject(dc, old);
    }
    sz.cx
}

fn vline(dc: HDC, x: i32, y1: i32, y2: i32, c: u32) {
    fill(dc, &RECT { left: x, top: y1, right: x + 1, bottom: y2 }, c);
}

// ---- geometri ----

#[derive(Clone, Copy, Default)]
pub(crate) struct Geo {
    pub dpi: i32,
    pub cw: i32,
    pub ch: i32,
    pub tab_h: i32,
    pub nav_top: i32,
    pub nav_h: i32,
    pub cmd_top: i32,
    pub cmd_h: i32,
    pub bar_h: i32,
    pub stat_top: i32,
    pub stat_h: i32,
    /// Pembatas dan panel terminal F4 (sama dengan stat_top bila panel tidak tampil).
    pub split_top: i32,
    pub term_top: i32,
    pub x0: i32,
    pub tab_w: i32,
    pub plus_w: i32,
    /// Tepi kiri tombol jendela (min/maks/tutup) dalam koordinat klien.
    pub cap_x: i32,
}

/// Tepi kiri tombol jendela dari DWM; cadangan 138 DIP.
fn caption_left(h: HWND, cw: i32, dpi: i32) -> i32 {
    let mut b = RECT::default();
    let ok = unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_CAPTION_BUTTON_BOUNDS,
            &mut b as *mut RECT as *mut c_void,
            size_of::<RECT>() as u32,
        )
    }
    .is_ok();
    if ok && b.right > b.left {
        let mut wr = RECT::default();
        let mut pt = POINT { x: 0, y: 0 };
        unsafe {
            let _ = GetWindowRect(h, &mut wr);
            let _ = ClientToScreen(h, &mut pt);
        }
        let off = pt.x - wr.left;
        return (b.left - off).clamp(0, cw);
    }
    (cw - px(138, dpi)).max(0)
}

pub(crate) fn geo(w: &FmWin) -> Geo {
    let dpi = dpi_of(w.h);
    let mut rc = RECT::default();
    let _ = unsafe { GetClientRect(w.h, &mut rc) };
    let tab_h = px(FM_TABH_DIP, dpi);
    let nav_h = px(FM_NAVH_DIP, dpi);
    let cmd_h = px(FM_CMDH_DIP, dpi);
    let stat_h = px(FM_STATH_DIP, dpi);
    let cap_x = caption_left(w.h, rc.right, dpi);
    let x0 = px(8, dpi);
    let plus_w = px(40, dpi);
    let avail = cap_x - px(40, dpi) - x0 - plus_w;
    let n = w.tabs.n.max(1);
    let tab_w = (avail / n).clamp(px(72, dpi), px(240, dpi));
    let bar_h = tab_h + nav_h + cmd_h;
    let stat_top = (rc.bottom - stat_h).max(bar_h);
    let (split_top, term_top) = if w.term_on && !w.term.0.is_null() {
        let split = px(5, dpi);
        let avail_h = (stat_top - bar_h - split).max(0);
        let th = px(w.term_h, dpi).min(avail_h - px(120, dpi)).max(px(80, dpi).min(avail_h));
        let tt = stat_top - th;
        ((tt - split).max(bar_h), tt)
    } else {
        (stat_top, stat_top)
    };
    Geo {
        dpi,
        cw: rc.right,
        ch: rc.bottom,
        tab_h,
        nav_top: tab_h,
        nav_h,
        cmd_top: tab_h + nav_h,
        cmd_h,
        bar_h: tab_h + nav_h + cmd_h,
        stat_top,
        stat_h,
        split_top,
        term_top,
        x0,
        tab_w,
        plus_w,
        cap_x,
    }
}

/// Area isi (IExplorerBrowser): di antara bilah atas dan bilah status.
pub(crate) fn body_rect(w: &FmWin) -> RECT {
    let g = geo(w);
    RECT { left: 0, top: g.bar_h.min(g.ch), right: g.cw, bottom: g.split_top.max(g.bar_h.min(g.ch)) }
}

fn tab_rect(g: &Geo, i: i32) -> RECT {
    let left = g.x0 + i * g.tab_w;
    RECT { left, top: px(6, g.dpi), right: left + g.tab_w, bottom: g.tab_h }
}

fn tab_close_rect(g: &Geo, r: &RECT) -> RECT {
    let s = px(24, g.dpi);
    let right = r.right - px(6, g.dpi);
    let top = (r.top + r.bottom - s) / 2;
    RECT { left: right - s, top, right, bottom: top + s }
}

fn plus_rect(g: &Geo, n: i32) -> RECT {
    let left = g.x0 + n * g.tab_w + px(4, g.dpi);
    let s = px(32, g.dpi);
    let top = (px(6, g.dpi) + g.tab_h - s) / 2;
    RECT { left, top, right: left + s, bottom: top + s }
}

fn row_box(g: &Geo, top: i32, h: i32) -> (i32, i32) {
    let bh = px(32, g.dpi);
    let t = top + (h - bh) / 2;
    (t, t + bh)
}

fn nav_rect(g: &Geo, i: i32) -> RECT {
    let (t, b) = row_box(g, g.nav_top, g.nav_h);
    let w = px(36, g.dpi);
    let left = px(8, g.dpi) + i * (w + px(4, g.dpi));
    RECT { left, top: t, right: left + w, bottom: b }
}

pub(crate) fn search_box(g: &Geo) -> RECT {
    let (t, b) = row_box(g, g.nav_top, g.nav_h);
    let width = (g.cw / 4).clamp(px(200, g.dpi), px(320, g.dpi));
    let right = g.cw - px(12, g.dpi);
    RECT { left: (right - width).max(0), top: t, right, bottom: b }
}

pub(crate) fn addr_box(g: &Geo) -> RECT {
    let (t, b) = row_box(g, g.nav_top, g.nav_h);
    let left = nav_rect(g, 3).right + px(8, g.dpi);
    let right = (search_box(g).left - px(8, g.dpi)).max(left);
    RECT { left, top: t, right, bottom: b }
}

/// Kotak kontrol EDIT di dalam kotak alamat atau pencarian (teks rata tengah vertikal).
pub(crate) fn edit_rect(g: &Geo, boxr: &RECT, right_pad_dip: i32) -> RECT {
    let h = px(20, g.dpi);
    let top = (boxr.top + boxr.bottom - h) / 2;
    RECT {
        left: boxr.left + px(10, g.dpi),
        top,
        right: (boxr.right - px(right_pad_dip, g.dpi)).max(boxr.left + px(10, g.dpi)),
        bottom: top + h,
    }
}

fn cmd_rect_g(g: &Geo, c: usize) -> RECT {
    let (t, b) = row_box(g, g.cmd_top, g.cmd_h);
    let mut left = px(12, g.dpi);
    for k in 0..c {
        left += px(CMD_W_DIP[k], g.dpi) + px(2, g.dpi);
        if CMD_SEP_AFTER[k] {
            left += px(15, g.dpi);
        }
    }
    RECT { left, top: t, right: left + px(CMD_W_DIP[c], g.dpi), bottom: b }
}

/// Kotak tombol perintah ke-c (untuk posisi menu popup).
pub(crate) fn cmd_rect(w: &FmWin, c: usize) -> RECT {
    cmd_rect_g(&geo(w), c.min(CMD_N - 1))
}

fn view_btn_rect(g: &Geo, i: i32) -> RECT {
    let bw = px(28, g.dpi);
    let bh = px(22, g.dpi);
    let top = g.stat_top + (g.stat_h - bh) / 2;
    let right = g.cw - px(8, g.dpi) - (1 - i) * (bw + px(2, g.dpi));
    RECT { left: right - bw, top, right, bottom: top + bh }
}

/// Tombol panel terminal (F4) di bilah status, kiri tombol tampilan.
fn term_btn_rect(g: &Geo) -> RECT {
    let v = view_btn_rect(g, 0);
    let bw = v.right - v.left;
    let right = v.left - px(10, g.dpi);
    RECT { left: right - bw, top: v.top, right, bottom: v.bottom }
}

// ---- uji titik ----

pub(crate) fn hit_test(w: &FmWin, x: i32, y: i32) -> FmHit {
    let mut hit = FmHit { kind: FM_HIT_NONE, index: -1 };
    if y < 0 {
        return hit;
    }
    let g = geo(w);
    if y < g.tab_h {
        let n = w.tabs.n.clamp(0, w.tab.len() as i32);
        for i in 0..n {
            let r = tab_rect(&g, i);
            if inside(&r, x, y) {
                hit.index = i;
                hit.kind = if inside(&tab_close_rect(&g, &r), x, y) { FM_HIT_TABCLOSE } else { FM_HIT_TAB };
                return hit;
            }
        }
        if inside(&plus_rect(&g, n), x, y) {
            hit.kind = FM_HIT_PLUS;
        }
        return hit;
    }
    if y < g.cmd_top {
        for i in 0..4 {
            if inside(&nav_rect(&g, i), x, y) {
                hit.kind = FM_HIT_NAV;
                hit.index = i;
                return hit;
            }
        }
        if !w.editing && inside(&addr_box(&g), x, y) {
            for (idx, r) in &w.chev_rects {
                if inside(r, x, y) {
                    hit.kind = FM_HIT_CHEV;
                    hit.index = *idx;
                    return hit;
                }
            }
            for (idx, r) in &w.crumb_rects {
                if inside(r, x, y) {
                    hit.kind = FM_HIT_CRUMB;
                    hit.index = *idx;
                    return hit;
                }
            }
            hit.kind = FM_HIT_ADDR;
        }
        return hit;
    }
    if y < g.bar_h {
        for c in 0..CMD_N {
            if inside(&cmd_rect_g(&g, c), x, y) {
                hit.kind = FM_HIT_CMD;
                hit.index = c as i32;
                return hit;
            }
        }
        return hit;
    }
    if g.term_top > g.split_top && y >= g.split_top && y < g.term_top {
        hit.kind = FM_HIT_SPLIT;
        return hit;
    }
    if y >= g.stat_top {
        if inside(&term_btn_rect(&g), x, y) {
            hit.kind = FM_HIT_TERMBTN;
            return hit;
        }
        for i in 0..2 {
            if inside(&view_btn_rect(&g, i), x, y) {
                hit.kind = FM_HIT_VIEWBTN;
                hit.index = i;
                return hit;
            }
        }
    }
    hit
}

/// Slot tab di bawah x pada baris tab (untuk seret); -1 di luar.
pub(crate) fn slot_at_x(w: &FmWin, x: i32, y: i32) -> i32 {
    let g = geo(w);
    if y < 0 || y >= g.tab_h || g.tab_w <= 0 || x < g.x0 {
        return -1;
    }
    let s = (x - g.x0) / g.tab_w;
    if s < w.tabs.n {
        s
    } else {
        -1
    }
}

pub(crate) fn update_tip(w: &FmWin, hit: FmHit) {
    if w.tip.0.is_null() {
        return;
    }
    let s: &str = match hit.kind {
        FM_HIT_NAV => NAV_TIP[hit.index.clamp(0, 3) as usize],
        FM_HIT_CMD => CMD_TIP2[hit.index.clamp(0, CMD_N as i32 - 1) as usize],
        FM_HIT_PLUS => "Tab baru (Ctrl+T)",
        FM_HIT_TABCLOSE => "Tutup tab (Ctrl+W)",
        FM_HIT_VIEWBTN => VIEW_TIP[hit.index.clamp(0, 1) as usize],
        FM_HIT_TERMBTN => "Terminal (F4)",
        _ => "",
    };
    let mut buf = to_wide_z(s);
    let ti = TTTOOLINFOW {
        cbSize: size_of::<TTTOOLINFOW>() as u32,
        hwnd: w.h,
        uId: FM_TOOL_ID,
        lpszText: windows::core::PWSTR(buf.as_mut_ptr()),
        ..Default::default()
    };
    unsafe {
        SendMessageW(w.tip, TTM_UPDATETIPTEXTW, None, Some(LPARAM(&ti as *const TTTOOLINFOW as isize)));
    }
}

pub(crate) fn set_hot(w: &mut FmWin, hit: FmHit) {
    if hit == w.hot {
        return;
    }
    w.hot = hit;
    update_tip(w, hit);
    unsafe {
        let _ = InvalidateRect(Some(w.h), None, false);
    }
}

// ---- bingkai jendela (tab di bilah judul) ----

fn frame_y(h: HWND) -> i32 {
    let dpi = dpi_of(h) as u32;
    unsafe { GetSystemMetricsForDpi(SM_CYFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi) }
}

/// WM_NCCALCSIZE: buang bilah judul bawaan; tepi kiri/kanan/bawah tetap dari sistem.
pub(crate) unsafe fn nc_calcsize(h: HWND, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    if wp.0 == 0 || lp.0 == 0 {
        return None;
    }
    let p = lp.0 as *mut NCCALCSIZE_PARAMS;
    let top = (*p).rgrc[0].top;
    let r = DefWindowProcW(h, WM_NCCALCSIZE, wp, lp);
    (*p).rgrc[0].top = top;
    if IsZoomed(h).as_bool() {
        (*p).rgrc[0].top += frame_y(h);
    }
    Some(r)
}

/// Tombol jendela pada titik layar (sx, sy): HTMINBUTTON / HTMAXBUTTON / HTCLOSE, dari batas tombol DWM.
/// 2026-11-24: DwmDefWindowProc mengembalikan HTCAPTION di atas tombol (terukur saat jendela dimaksimalkan),
/// jadi klik tombol tidak berefek. Batas dibagi tiga sama lebar (min, maks, tutup), seperti Windows Terminal.
pub(crate) fn caption_button_at(h: HWND, sx: i32, sy: i32) -> Option<u32> {
    let rects = cap_rects(h)?;
    let mut pt = POINT { x: sx, y: sy };
    let _ = unsafe { ScreenToClient(h, &mut pt) };
    // y negatif (tepi atas layar saat dimaksimalkan, atau bingkai atas) tetap mengenai tombol.
    let i = rects.iter().position(|r| pt.x >= r.left && pt.x < r.right && pt.y < r.bottom)?;
    Some([HTMINBUTTON, HTMAXBUTTON, HTCLOSE][i])
}

// ---- tombol jendela yang digambar sendiri (2026-11-24) ----
// Tombol DWM tidak menerima hover/tekan (DwmDefWindowProc tidak mengenalinya), jadi tombol digambar sendiri
// bergaya Windows 11: lebar dari batas tombol DWM, setinggi baris tab, sorotan memudar ~100 ms.

/// Timer animasi pudar tombol jendela.
pub(crate) const FM_TIMER_CAP: usize = 0xCA9;

struct CapAnim {
    h: isize,
    hot: u32,
    press: u32,
    a: [f32; 3],
    last: std::time::Instant,
}

thread_local! {
    static CAP_ANIM: std::cell::RefCell<Vec<CapAnim>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn ht_index(ht: u32) -> Option<usize> {
    match ht {
        HTMINBUTTON => Some(0),
        HTMAXBUTTON => Some(1),
        HTCLOSE => Some(2),
        _ => None,
    }
}

/// Kotak tombol min/maks/tutup dalam koordinat klien.
fn cap_rects(h: HWND) -> Option<[RECT; 3]> {
    let mut rc = RECT::default();
    let _ = unsafe { GetClientRect(h, &mut rc) };
    let dpi = dpi_of(h);
    let left = caption_left(h, rc.right, dpi);
    let bottom = px(FM_TABH_DIP, dpi);
    let w3 = (rc.right - left) / 3;
    if w3 <= 0 {
        return None;
    }
    let r = |i: i32| RECT {
        left: left + w3 * i,
        top: 0,
        right: if i == 2 { rc.right } else { left + w3 * (i + 1) },
        bottom,
    };
    Some([r(0), r(1), r(2)])
}

fn cap_invalidate(h: HWND) {
    if let Some(r) = cap_rects(h) {
        let u = RECT { left: r[0].left, top: 0, right: r[2].right, bottom: r[0].bottom };
        let _ = unsafe { InvalidateRect(Some(h), Some(&u), false) };
    }
}

fn cap_with<R>(h: HWND, f: impl FnOnce(&mut CapAnim) -> R) -> R {
    CAP_ANIM.with(|v| {
        let mut v = v.borrow_mut();
        let i = match v.iter().position(|c| c.h == h.0 as isize) {
            Some(i) => i,
            None => {
                v.push(CapAnim { h: h.0 as isize, hot: 0, press: 0, a: [0.0; 3], last: std::time::Instant::now() });
                v.len() - 1
            }
        };
        f(&mut v[i])
    })
}

/// Tombol di bawah kursor berubah (0 = tidak ada): mulai animasi pudar.
pub(crate) fn cap_set_hot(h: HWND, ht: u32) {
    let ht = if ht_index(ht).is_some() { ht } else { 0 };
    let changed = cap_with(h, |c| {
        if c.hot == ht {
            return false;
        }
        c.hot = ht;
        if ht == 0 {
            c.press = 0;
        }
        c.last = std::time::Instant::now();
        true
    });
    if changed {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(Some(h), FM_TIMER_CAP, 15, None);
        }
        cap_invalidate(h);
    }
}

/// Langkah animasi: sorotan menuju 1 (tombol aktif) atau 0 (lainnya). Timer berhenti bila selesai.
pub(crate) fn cap_tick(h: HWND) {
    let done = cap_with(h, |c| {
        let now = std::time::Instant::now();
        let dt = now.duration_since(c.last).as_secs_f32();
        c.last = now;
        let mut done = true;
        for i in 0..3 {
            let target = if ht_index(c.hot) == Some(i) { 1.0 } else { 0.0 };
            // Muncul 83 ms, hilang 167 ms (kurva Fluent "fast"/"normal").
            let step = dt / if target > c.a[i] { 0.083 } else { 0.167 };
            c.a[i] = if target > c.a[i] { (c.a[i] + step).min(1.0) } else { (c.a[i] - step).max(0.0) };
            if (c.a[i] - target).abs() > f32::EPSILON {
                done = false;
            }
        }
        done
    });
    if done {
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::KillTimer(Some(h), FM_TIMER_CAP) };
    }
    cap_invalidate(h);
}

pub(crate) fn cap_forget(h: HWND) {
    CAP_ANIM.with(|v| v.borrow_mut().retain(|c| c.h != h.0 as isize));
}

pub(crate) fn cap_refresh(h: HWND) {
    cap_invalidate(h);
}

fn blend(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let x = ((a >> s) & 0xFF) as f32;
        let y = ((b >> s) & 0xFF) as f32;
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

/// Gambar tiga tombol jendela pada DIB bilah atas.
fn paint_caption(h: HWND, dc: HDC, dark: bool, p: &Pal, f: &Fonts) {
    let Some(rects) = cap_rects(h) else { return };
    let (hot, press, a) = cap_with(h, |c| (c.hot, c.press, c.a));
    let active = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() } == h;
    let zoomed = unsafe { IsZoomed(h) }.as_bool();
    let (hover_bg, press_bg) = if dark { (0x2D2D2D, 0x292929) } else { (0xE9E9E9, 0xEDEDED) };
    let glyph_base = if active { p.text } else { p.dim };
    for (i, r) in rects.iter().enumerate() {
        let ht = [HTMINBUTTON, HTMAXBUTTON, HTCLOSE][i];
        let pressed = press == ht && hot == ht;
        let (bg, fg) = if i == 2 {
            let target = if pressed { 0xC83C31 } else { 0xC42B1C };
            (blend(p.tabrow, target, a[i]), blend(glyph_base, 0xFFFFFF, a[i]))
        } else {
            let target = if pressed { press_bg } else { hover_bg };
            let fg = if pressed { blend(glyph_base, p.dim, 0.5) } else { glyph_base };
            (blend(p.tabrow, target, a[i]), fg)
        };
        fill(dc, r, bg);
        let glyph = match i {
            0 => "\u{E921}",
            1 if zoomed => "\u{E923}",
            1 => "\u{E922}",
            _ => "\u{E8BB}",
        };
        text(dc, r, glyph, fg, f.small, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    }
}

/// WM_NCLBUTTONDOWN/UP/DBLCLK: perintah dijalankan saat tombol dilepas di tombol yang sama. None = bukan tombol.
pub(crate) unsafe fn nc_caption_button(h: HWND, msg: u32, wp: WPARAM) -> Option<LRESULT> {
    let ht = wp.0 as u32;
    if msg == WM_NCLBUTTONUP {
        // Tekanan selalu dilepas, juga bila tombol mouse dilepas di luar tombol.
        let pressed = cap_with(h, |c| std::mem::replace(&mut c.press, 0));
        if pressed != 0 {
            cap_invalidate(h);
        }
        if ht_index(ht).is_none() {
            return None;
        }
        if pressed == ht {
            let cmd = match ht {
                HTMINBUTTON => SC_MINIMIZE,
                HTMAXBUTTON if IsZoomed(h).as_bool() => SC_RESTORE,
                HTMAXBUTTON => SC_MAXIMIZE,
                _ => SC_CLOSE,
            };
            let _ = PostMessageW(Some(h), WM_SYSCOMMAND, WPARAM(cmd as usize), LPARAM(0));
        }
        return Some(LRESULT(0));
    }
    if ht_index(ht).is_none() || !matches!(msg, WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK) {
        return None;
    }
    cap_with(h, |c| {
        c.hot = ht;
        c.press = ht;
    });
    cap_invalidate(h);
    Some(LRESULT(0))
}

/// WM_NCHITTEST (setelah DwmDefWindowProc menangani tombol jendela): tepi atas, area kosong baris tab = judul.
pub(crate) unsafe fn nc_hittest(h: HWND, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let sx = (lp.0 & 0xFFFF) as u16 as i16 as i32;
    let sy = ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    if let Some(ht) = caption_button_at(h, sx, sy) {
        return LRESULT(ht as isize);
    }
    let r = DefWindowProcW(h, WM_NCHITTEST, wp, lp);
    if r.0 != HTCLIENT as isize {
        return r;
    }
    let mut pt = POINT { x: (lp.0 & 0xFFFF) as u16 as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32 };
    let _ = ScreenToClient(h, &mut pt);
    let mut rc = RECT::default();
    let _ = GetClientRect(h, &mut rc);
    if !IsZoomed(h).as_bool() {
        let fy = frame_y(h);
        if pt.y < fy {
            if pt.x < fy * 2 {
                return LRESULT(HTTOPLEFT as isize);
            }
            if pt.x >= rc.right - fy * 2 {
                return LRESULT(HTTOPRIGHT as isize);
            }
            return LRESULT(HTTOP as isize);
        }
    }
    if let Some(w) = fm_win_of(h) {
        let g = geo(w);
        if pt.y < g.tab_h && hit_test(w, pt.x, pt.y).kind == FM_HIT_NONE {
            return LRESULT(HTCAPTION as isize);
        }
    }
    LRESULT(HTCLIENT as isize)
}

/// Perluas bingkai DWM setinggi baris tab agar tombol jendela tampil di sana.
pub(crate) fn extend_frame(h: HWND) {
    let m = MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: px(FM_TABH_DIP, dpi_of(h)), cyBottomHeight: 0 };
    let _ = unsafe { DwmExtendFrameIntoClientArea(h, &m) };
}

// ---- breadcrumb ----

/// Susun ulang segmen breadcrumb dari PIDL tab aktif (akar Desktop tidak ikut, kecuali folder itu sendiri Desktop).
pub(crate) fn crumbs_refresh(w: &mut FmWin) {
    w.crumbs.clear();
    w.crumb_rects.clear();
    let cur = match fm_active(w) {
        Some(t) => t.cur,
        None => return,
    };
    if cur.is_null() {
        return;
    }
    let p = unsafe { ILClone(cur) };
    if p.is_null() {
        return;
    }
    let mut names: Vec<Vec<u16>> = Vec::new();
    loop {
        if names.len() >= 32 {
            break;
        }
        match unsafe { SHGetNameFromIDList(p, SIGDN_NORMALDISPLAY) } {
            Ok(n) => {
                let mut v = Vec::new();
                let mut i = 0usize;
                // SAFETY: n adalah PWSTR berakhir NUL dari Shell.
                unsafe {
                    while *n.0.add(i) != 0 {
                        v.push(*n.0.add(i));
                        i += 1;
                    }
                    CoTaskMemFree(Some(n.0 as *const c_void));
                }
                names.push(v);
            }
            Err(_) => names.push(Vec::new()),
        }
        // SAFETY: p adalah PIDL valid hasil ILClone.
        if unsafe { (*p).mkid.cb } == 0 {
            break;
        }
        if !unsafe { ILRemoveLastID(Some(p)) }.as_bool() {
            break;
        }
        if unsafe { (*p).mkid.cb } == 0 {
            break;
        }
    }
    free_pidl(p);
    names.reverse();
    w.crumbs = names;
}

/// Klik segmen breadcrumb `idx`: naik ke leluhur yang sesuai.
pub(crate) fn crumb_go(w: &mut FmWin, idx: i32) {
    let n = w.crumbs.len() as i32;
    if idx < 0 || idx >= n - 1 {
        return;
    }
    let ups = n - 1 - idx;
    let cur = match fm_active(w) {
        Some(t) => t.cur,
        None => return,
    };
    if cur.is_null() {
        return;
    }
    let p = unsafe { ILClone(cur) };
    if p.is_null() {
        return;
    }
    let mut ok = true;
    for _ in 0..ups {
        if !unsafe { ILRemoveLastID(Some(p)) }.as_bool() {
            ok = false;
            break;
        }
    }
    if ok {
        if let Some(t) = fm_active(w) {
            let _ = fm_browse(t, p);
        }
    }
    free_pidl(p);
}

/// Tampilkan atau sembunyikan kotak edit alamat (mode ketik) di atas breadcrumb.
pub(crate) fn addr_edit(w: &mut FmWin, on: bool) {
    if w.addr.0.is_null() {
        return;
    }
    if w.editing != on {
        w.editing = on;
        unsafe {
            let _ = ShowWindow(w.addr, if on { SW_SHOW } else { SW_HIDE });
        }
    }
    if on {
        unsafe {
            let _ = SetFocus(Some(w.addr));
            let _ = SendMessageW(w.addr, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
        }
    }
    unsafe {
        let _ = InvalidateRect(Some(w.h), None, false);
    }
}

// ---- gambar ----

struct Fonts {
    ui: HFONT,
    icon: HFONT,
    nav: HFONT,
    small: HFONT,
}

fn fonts(dpi: i32) -> Fonts {
    Fonts {
        ui: font(dpi, 9, "Segoe UI"),
        icon: font(dpi, 12, ICON_FACE),
        nav: font(dpi, 11, ICON_FACE),
        small: font(dpi, 7, ICON_FACE),
    }
}

fn fonts_free(f: &Fonts) {
    unsafe {
        for h in [f.ui, f.icon, f.nav, f.small] {
            let _ = DeleteObject(HGDIOBJ(h.0));
        }
    }
}

fn draw_icon(w: &FmWin, dc: HDC, idx: i32, x: i32, y: i32) {
    if idx < 0 || w.himl == 0 {
        return;
    }
    let _ = unsafe { ImageList_Draw(HIMAGELIST(w.himl), idx, dc, x, y, ILD_TRANSPARENT) };
}

fn btn_fill(dc: HDC, r: &RECT, hot: bool, pressed: bool, p: &Pal, dpi: i32) {
    if hot && pressed {
        round_fill(dc, r, px(4, dpi), p.press);
    } else if hot {
        round_fill(dc, r, px(4, dpi), p.hover);
    }
}

fn paint_tabs(w: &FmWin, dc: HDC, g: &Geo, p: &Pal, f: &Fonts) {
    let n = w.tabs.n.clamp(0, w.tab.len() as i32);
    let is_hot = |i: i32| (w.hot.kind == FM_HIT_TAB || w.hot.kind == FM_HIT_TABCLOSE) && w.hot.index == i;
    let icon_sz = px(16, g.dpi);
    for i in 0..n {
        let r = tab_rect(g, i);
        let active = i == w.tabs.active;
        let hot = is_hot(i);
        if active {
            // Sudut atas bulat; bagian bawah menyatu dengan baris alamat (digambar sesudahnya).
            let rr = RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom + px(8, g.dpi) };
            round_fill(dc, &rr, px(8, g.dpi), p.layer);
        } else if hot {
            let rr = RECT {
                left: r.left + px(2, g.dpi),
                top: r.top + px(2, g.dpi),
                right: r.right - px(2, g.dpi),
                bottom: r.bottom - px(4, g.dpi),
            };
            round_fill(dc, &rr, px(6, g.dpi), p.tab_hover);
        }
        let cy = (r.top + r.bottom) / 2 - if active { 0 } else { px(1, g.dpi) };
        let ix = r.left + px(12, g.dpi);
        draw_icon(w, dc, w.tab[i as usize].icon, ix, cy - icon_sz / 2);
        let cr = tab_close_rect(g, &r);
        let show_close = active || hot;
        let tr = RECT {
            left: ix + icon_sz + px(8, g.dpi),
            top: r.top,
            right: if show_close { cr.left - px(2, g.dpi) } else { r.right - px(8, g.dpi) },
            bottom: r.bottom,
        };
        let mut title = {
            let t = &w.tab[i as usize].title;
            let k = t.iter().position(|&c| c == 0).unwrap_or(t.len());
            if k == 0 {
                to_wide("Tab baru")
            } else {
                t[..k].to_vec()
            }
        };
        text_w(dc, &tr, &mut title, p.text, f.ui, DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
        if show_close {
            if w.hot.kind == FM_HIT_TABCLOSE && w.hot.index == i {
                round_fill(dc, &cr, px(4, g.dpi), p.hover);
            }
            text(dc, &cr, "\u{E711}", p.text, f.small, DT_CENTER | DT_VCENTER);
        }
        let next_plain = i + 1 < n && i + 1 != w.tabs.active && !is_hot(i + 1);
        if !active && !hot && next_plain {
            vline(dc, r.right, cy - px(8, g.dpi), cy + px(8, g.dpi), p.sep);
        }
    }
    let pr = plus_rect(g, n);
    btn_fill(dc, &pr, w.hot.kind == FM_HIT_PLUS, w.press.kind == FM_HIT_PLUS, p, g.dpi);
    text(dc, &pr, "\u{E710}", p.text, f.small, DT_CENTER | DT_VCENTER);
}

fn paint_nav(w: &FmWin, dc: HDC, g: &Geo, p: &Pal, f: &Fonts, enabled: [bool; 4]) {
    for i in 0..4 {
        let r = nav_rect(g, i);
        let hot = w.hot.kind == FM_HIT_NAV && w.hot.index == i;
        let pressed = w.press.kind == FM_HIT_NAV && w.press.index == i;
        if enabled[i as usize] {
            btn_fill(dc, &r, hot, pressed, p, g.dpi);
        }
        let c = if enabled[i as usize] { p.text } else { p.dim };
        text(dc, &r, NAV_GLYPH[i as usize], c, f.nav, DT_CENTER | DT_VCENTER);
    }
}

fn paint_box(dc: HDC, r: &RECT, p: &Pal, focused: bool, accent: COLORREF, dpi: i32) {
    round_fill(dc, r, px(4, dpi), p.box_line);
    let inner = RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 };
    round_fill(dc, &inner, px(4, dpi) - 1, p.box_fill);
    if focused {
        let a = accent.0;
        let c = ((a & 0xFF) << 16) | (a & 0xFF00) | ((a >> 16) & 0xFF);
        let u = RECT { left: r.left + px(4, dpi), top: r.bottom - px(2, dpi), right: r.right - px(4, dpi), bottom: r.bottom };
        fill(dc, &u, c);
    }
}

fn paint_addr(w: &mut FmWin, dc: HDC, g: &Geo, p: &Pal, f: &Fonts) {
    let ab = addr_box(g);
    paint_box(dc, &ab, p, w.editing, w.accent, g.dpi);
    w.crumb_rects.clear();
    w.chev_rects.clear();
    if w.editing || ab.right - ab.left < px(60, g.dpi) {
        return;
    }
    let cy = (ab.top + ab.bottom) / 2;
    let icon_sz = px(16, g.dpi);
    let mut x = ab.left + px(10, g.dpi);
    let icon = fm_active_icon(w);
    draw_icon(w, dc, icon, x, cy - icon_sz / 2);
    x += icon_sz + px(2, g.dpi);
    let chev = px(20, g.dpi);
    let pad = px(6, g.dpi);
    let widths: Vec<i32> = w.crumbs.iter().map(|s| text_width(dc, s, f.ui) + pad * 2).collect();
    let avail = ab.right - px(8, g.dpi) - x;
    let mut start = 0usize;
    let mut total: i32 = widths.iter().map(|v| v + chev).sum();
    while total > avail && start + 1 < widths.len() {
        total -= widths[start] + chev;
        start += 1;
    }
    let inner_top = ab.top + px(4, g.dpi);
    let inner_bot = ab.bottom - px(4, g.dpi);
    for k in start..w.crumbs.len() {
        paint_chev(w, dc, g, p, f, k as i32 - 1, RECT { left: x, top: inner_top, right: x + chev, bottom: inner_bot });
        x += chev;
        let right = (x + widths[k]).min(ab.right - px(4, g.dpi));
        if right <= x {
            break;
        }
        let sr = RECT { left: x, top: inner_top, right, bottom: inner_bot };
        let hot = w.hot.kind == FM_HIT_CRUMB && w.hot.index == k as i32;
        let pressed = w.press.kind == FM_HIT_CRUMB && w.press.index == k as i32;
        btn_fill(dc, &sr, hot, pressed, p, g.dpi);
        let mut s = w.crumbs[k].clone();
        text_w(dc, &sr, &mut s, p.text, f.ui, DT_CENTER | DT_VCENTER | DT_END_ELLIPSIS);
        w.crumb_rects.push((k as i32, sr));
        x = right;
    }
    // Chevron penutup: subfolder dari folder aktif.
    let last = w.crumbs.len() as i32 - 1;
    if last >= 0 && x + chev <= ab.right - px(4, g.dpi) {
        paint_chev(w, dc, g, p, f, last, RECT { left: x, top: inner_top, right: x + chev, bottom: inner_bot });
    }
}

/// Chevron breadcrumb yang bisa diklik (dropdown subfolder segmen `k`; -1 = akar).
fn paint_chev(w: &mut FmWin, dc: HDC, g: &Geo, p: &Pal, f: &Fonts, k: i32, r: RECT) {
    let hot = w.hot.kind == FM_HIT_CHEV && w.hot.index == k;
    let pressed = w.press.kind == FM_HIT_CHEV && w.press.index == k;
    btn_fill(dc, &r, hot, pressed, p, g.dpi);
    let glyph = if pressed { "\u{E70D}" } else { "\u{E76C}" };
    text(dc, &r, glyph, p.text, f.small, DT_CENTER | DT_VCENTER);
    w.chev_rects.push((k, r));
}

fn fm_active_icon(w: &FmWin) -> i32 {
    let a = w.tabs.active;
    if a >= 0 && (a as usize) < w.tab.len() {
        w.tab[a as usize].icon
    } else {
        -1
    }
}

fn paint_search(w: &FmWin, dc: HDC, g: &Geo, p: &Pal, f: &Fonts) {
    let sb = search_box(g);
    let focused = !w.search.0.is_null() && unsafe { GetFocus() } == w.search;
    paint_box(dc, &sb, p, focused, w.accent, g.dpi);
    let gr = RECT { left: sb.right - px(32, g.dpi), top: sb.top, right: sb.right, bottom: sb.bottom };
    text(dc, &gr, "\u{E721}", p.dim, f.small, DT_CENTER | DT_VCENTER);
}

fn paint_cmd(w: &FmWin, dc: HDC, g: &Geo, p: &Pal, f: &Fonts) {
    for c in 0..CMD_N {
        let idx = c as i32;
        let r = cmd_rect_g(g, c);
        let enabled = idx == CMD_MORE || w12_fm_cmd_enabled(idx, w.has_sel);
        let hot = w.hot.kind == FM_HIT_CMD && w.hot.index == idx;
        let pressed = w.press.kind == FM_HIT_CMD && w.press.index == idx;
        if enabled {
            btn_fill(dc, &r, hot, pressed, p, g.dpi);
        }
        let tc = if enabled { p.text } else { p.dim };
        if CMD_TEXT[c].is_empty() {
            text(dc, &r, CMD_GLYPH[c], tc, f.icon, DT_CENTER | DT_VCENTER);
        } else {
            let gr = RECT { left: r.left + px(8, g.dpi), top: r.top, right: r.left + px(30, g.dpi), bottom: r.bottom };
            text(dc, &gr, CMD_GLYPH[c], tc, f.icon, DT_CENTER | DT_VCENTER);
            let caret_w = if CMD_CARET[c] { px(18, g.dpi) } else { px(6, g.dpi) };
            let tr = RECT { left: gr.right + px(6, g.dpi), top: r.top, right: r.right - caret_w, bottom: r.bottom };
            text(dc, &tr, CMD_TEXT[c], tc, f.ui, DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
            if CMD_CARET[c] {
                let kr = RECT { left: r.right - px(20, g.dpi), top: r.top, right: r.right - px(4, g.dpi), bottom: r.bottom };
                text(dc, &kr, "\u{E70D}", tc, f.small, DT_CENTER | DT_VCENTER);
            }
        }
        if CMD_SEP_AFTER[c] && c + 1 < CMD_N {
            let x = r.right + px(9, g.dpi);
            let cy = (r.top + r.bottom) / 2;
            vline(dc, x, cy - px(10, g.dpi), cy + px(10, g.dpi), p.sep);
        }
    }
}

fn paint_status(w: &FmWin, dc: HDC, g: &Geo, p: &Pal, f: &Fonts) {
    if g.stat_h <= 0 || g.cw <= 0 || g.stat_top >= g.ch {
        return;
    }
    let mem = unsafe { CreateCompatibleDC(Some(dc)) };
    if mem.0.is_null() {
        return;
    }
    let bmp = unsafe { CreateCompatibleBitmap(dc, g.cw, g.stat_h) };
    if bmp.0.is_null() {
        let _ = unsafe { DeleteDC(mem) };
        return;
    }
    let old = unsafe { SelectObject(mem, HGDIOBJ(bmp.0)) };
    // Gambar dengan koordinat lokal (y dikurangi stat_top).
    let mut lg = *g;
    lg.stat_top = 0;
    fill(mem, &RECT { left: 0, top: 0, right: g.cw, bottom: g.stat_h }, p.status);
    let mut s = format!("{} item", w.n_items.max(0));
    if w.n_sel > 0 {
        s.push_str(&format!("     {} item dipilih", w.n_sel));
    }
    let k = w.status.iter().position(|&c| c == 0).unwrap_or(w.status.len());
    if k > 0 {
        s.push_str("     ");
        s.push_str(&String::from_utf16_lossy(&w.status[..k]));
    }
    let tr = RECT { left: px(12, g.dpi), top: 0, right: term_btn_rect(&lg).left - px(8, g.dpi), bottom: g.stat_h };
    let tc = if w.status_error && k > 0 { 0xD13438 } else { p.text };
    text(mem, &tr, &s, tc, f.ui, DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
    for i in 0..2 {
        let r = view_btn_rect(&lg, i);
        let on = (i == 0) == w.details;
        let hot = w.hot.kind == FM_HIT_VIEWBTN && w.hot.index == i;
        if on {
            round_fill(mem, &r, px(4, g.dpi), p.press);
        } else if hot {
            round_fill(mem, &r, px(4, g.dpi), p.hover);
        }
        text(mem, &r, VIEW_GLYPH[i as usize], p.text, f.small, DT_CENTER | DT_VCENTER);
    }
    {
        let r = term_btn_rect(&lg);
        let hot = w.hot.kind == FM_HIT_TERMBTN;
        if w.term_on {
            round_fill(mem, &r, px(4, g.dpi), p.press);
        } else if hot {
            round_fill(mem, &r, px(4, g.dpi), p.hover);
        }
        text(mem, &r, "\u{E756}", p.text, f.small, DT_CENTER | DT_VCENTER);
        let x = (r.right + view_btn_rect(&lg, 0).left) / 2;
        vline(mem, x, r.top + px(4, g.dpi), r.bottom - px(4, g.dpi), p.sep);
    }
    unsafe {
        let _ = BitBlt(dc, 0, g.stat_top, g.cw, g.stat_h, Some(mem), 0, 0, SRCCOPY);
        let _ = SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
    }
}

/// Gambar bilah atas lewat DIB 32-bit (alpha 0 di bawah tombol jendela, 255 di tempat lain) lalu bilah status.
pub(crate) fn paint(w: &mut FmWin, dc: HDC, enabled_nav: [bool; 4]) {
    let g = geo(w);
    if g.cw <= 0 || g.bar_h <= 0 {
        return;
    }
    let p = pal(w.dark);
    let f = fonts(g.dpi);
    let mut bi: BITMAPINFO = unsafe { zeroed() };
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = g.cw;
    bi.bmiHeader.biHeight = -g.bar_h;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB.0;
    let mut bits: *mut c_void = std::ptr::null_mut();
    let dib = unsafe { CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) };
    let dib = match dib {
        Ok(b) if !b.0.is_null() && !bits.is_null() => b,
        _ => {
            fonts_free(&f);
            return;
        }
    };
    let mem = unsafe { CreateCompatibleDC(Some(dc)) };
    if mem.0.is_null() {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(dib.0));
        }
        fonts_free(&f);
        return;
    }
    let old = unsafe { SelectObject(mem, HGDIOBJ(dib.0)) };
    fill(mem, &RECT { left: 0, top: 0, right: g.cw, bottom: g.tab_h }, p.tabrow);
    paint_tabs(w, mem, &g, &p, &f);
    fill(mem, &RECT { left: 0, top: g.tab_h, right: g.cw, bottom: g.bar_h }, p.layer);
    paint_nav(w, mem, &g, &p, &f, enabled_nav);
    paint_addr(w, mem, &g, &p, &f);
    paint_search(w, mem, &g, &p, &f);
    fill(mem, &RECT { left: 0, top: g.cmd_top, right: g.cw, bottom: g.cmd_top + 1 }, p.line);
    paint_cmd(w, mem, &g, &p, &f);
    fill(mem, &RECT { left: 0, top: g.bar_h - 1, right: g.cw, bottom: g.bar_h }, p.line);
    // Tombol jendela digambar sendiri (menutupi tombol DWM di bawahnya).
    paint_caption(w.h, mem, w.dark, &p, &f);
    // Alpha: GDI menulis alpha 0; seluruh bilah dibuat buram.
    // SAFETY: bits menunjuk cw*bar_h piksel 32-bit milik DIB yang masih dipilih di mem.
    unsafe {
        let px_all = std::slice::from_raw_parts_mut(bits as *mut u32, (g.cw * g.bar_h) as usize);
        for v in px_all.iter_mut() {
            *v |= 0xFF00_0000;
        }
        let _ = BitBlt(dc, 0, 0, g.cw, g.bar_h, Some(mem), 0, 0, SRCCOPY);
        let _ = SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(mem);
    }
    paint_status(w, dc, &g, &p, &f);
    // Pembatas panel terminal: latar status + garis tipis di atas (seperti tepi panel Explorer).
    if g.term_top > g.split_top {
        fill(dc, &RECT { left: 0, top: g.split_top, right: g.cw, bottom: g.term_top }, p.status);
        fill(dc, &RECT { left: 0, top: g.split_top, right: g.cw, bottom: g.split_top + 1 }, p.line);
    }
    fonts_free(&f);
}

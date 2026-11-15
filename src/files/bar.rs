//! Sumber: w12fm_bar.inc:1-840 (dipakai win12files.cpp). Status: PORT LENGKAP, belum diuji runtime.
//! Deviasi: GetPrivateProfileStringW lewat FFI kernel32 (fitur Win32_System_WindowsProgramming kini aktif, deklarasi tetap lokal).

use std::ffi::c_void;
use std::ptr::null_mut;

use windows::core::{Interface, BOOL, GUID, PCSTR, PCWSTR, PSTR, PWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, POINT, PROPERTYKEY, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetColorizationColor, DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, FillRect, GetStockObject, InvalidateRect, LineTo, MoveToEx, Polygon,
    SelectObject, SetBkMode, SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
    DRAW_TEXT_FORMAT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE,
    FW_NORMAL, HDC, HFONT, HGDIOBJ, NULL_PEN, OUT_DEFAULT_PRECIS, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::Com::IBindCtx;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::System::Registry::HKEY_CURRENT_USER;
use windows::Win32::UI::Controls::{
    SetWindowTheme, TTTOOLINFOW, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP, TTS_NOPREFIX,
    TOOLTIPS_CLASSW, WM_MOUSELEAVE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    FOLDERID_Profile, ILClone, ILRemoveLastID, IContextMenu, IFolderView, IFolderView2, IShellView, BHID_SFUIObject,
    CMF_NORMAL, CMINVOKECOMMANDINFO, FVM_DETAILS, FVM_ICON, FVM_LIST, FVM_TILE, GCS_VERBW, SBSP_ABSOLUTE,
    SHGetKnownFolderIDList, SHGetValueW, SORTCOLUMN, SORT_ASCENDING, SORT_DESCENDING, SVGIO_BACKGROUND,
    SVGIO_SELECTION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, DestroyWindow, GetClientRect, GetMenuItemCount, GetMenuItemInfoW,
    SendMessageW, CreateWindowExW, SetTimer, CW_USEDEFAULT, HMENU, MENUITEMINFOW, MF_CHECKED, MF_STRING, MIIM_ID,
    MIIM_SUBMENU, SW_SHOWNORMAL, TrackPopupMenu, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_TOPALIGN, WINDOW_STYLE,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MOUSEMOVE, WS_EX_TOPMOST, WS_POPUP,
};

use crate::files::app::{
    fm_active, fm_browse, fm_current_view, fm_focus_view, fm_layout, fm_navigate, fm_refresh_chrome, fm_status,
    fm_tab_destroy, fm_tab_empty, fm_tab_init, fm_tab_navigation_failed, free_pidl,
};
use crate::files::state::{
    FmGeo, FmHit, FmTab, FmWin, CMD_DIP, CMD_LABEL, CMD_TIP, FM_CMDH_DIP, FM_HIT_CMD, FM_HIT_NAV, FM_HIT_NONE,
    FM_HIT_PLUS, FM_HIT_TAB, FM_HIT_TABCLOSE, FM_HISTMAX, FM_NAVH_DIP, FM_TABH_DIP, FM_TIMER_SEL, FM_TOOL_ID, HOME_ADDRESS, NAV_TIP,
    SORT_LABEL, VIEW_LABEL,
};
use crate::fm_logic::{
    w12_fm_palette_for, w12_fm_tab_close_hit, w12_fm_tab_slot_at, w12_fm_tab_width, w12_fm_theme_is_dark,
    w12_fm_cmd_enabled, w12_fm_sort_next, w12_hist_back, w12_hist_can_back, w12_hist_can_forward, w12_hist_forward,
    w12_hist_push, w12_tabs_add, w12_tabs_close, w12_tabs_move, W12FC_COPY, W12FC_COUNT, W12FC_CUT, W12FC_DELETE,
    W12FC_NEWFOLDER, W12FC_PASTE, W12FC_RENAME, W12FC_SORT, W12FC_VIEW,
};
use crate::wide::{pcw, to_wide, to_wide_z};

// ---- util (sama dengan app.rs; fungsi privat di sana, jadi disalin di sini) ----

/// Pengganti MulDiv(a, b, c) dengan pembulatan sama untuk nilai positif.
fn mul_div(a: i32, b: i32, c: i32) -> i32 {
    if c == 0 {
        return -1;
    }
    let p = a as i64 * b as i64;
    let half = c as i64 / 2;
    let r = if (p < 0) != (c < 0) { (p - half) / c as i64 } else { (p + half) / c as i64 };
    r as i32
}

/// Px(dip, dpi) dari C: skala DIP ke piksel.
fn px(dip: i32, dpi: i32) -> i32 {
    mul_div(dip, dpi, 96)
}

/// DpiOf(h): 96 bila 0.
fn dpi_of(h: HWND) -> i32 {
    let d = unsafe { GetDpiForWindow(h) } as i32;
    if d == 0 {
        96
    } else {
        d
    }
}

/// Rect klien. Nol bila GetClientRect gagal.
fn client_rect(h: HWND) -> RECT {
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let _ = unsafe { GetClientRect(h, &mut rc) };
    rc
}

/// FmRgb: 0xRRGGBB menjadi COLORREF (makro RGB: R di byte rendah).
pub(crate) fn fm_rgb(rgb: u32) -> COLORREF {
    let r = (rgb >> 16) & 0xFF;
    let g = (rgb >> 8) & 0xFF;
    let b = rgb & 0xFF;
    COLORREF(r | (g << 8) | (b << 16))
}

/// FmNoHit: tidak ada sasaran.
fn fm_no_hit() -> FmHit {
    FmHit { kind: FM_HIT_NONE, index: -1 }
}

// ---- geometri (w12fm_bar.inc:22-75) ----

/// FmGeoOf: geometri bilah dalam piksel untuk DPI jendela.
fn fm_geo_of(w: &FmWin) -> FmGeo {
    let dpi = dpi_of(w.h);
    let tab_h = px(FM_TABH_DIP, dpi);
    let nav_h = px(FM_NAVH_DIP, dpi);
    let cmd_top = tab_h + nav_h;
    FmGeo {
        tab_h,
        nav_top: tab_h,
        nav_h,
        cmd_top,
        cmd_h: px(FM_CMDH_DIP, dpi),
        plus_w: px(36, dpi),
        nav_btn_w: px(40, dpi),
        close_size: px(16, dpi),
        close_pad: px(8, dpi),
        pad: px(4, dpi),
    }
}

/// FmTabW: lebar satu tab. Tanpa tab, 200 DIP.
fn fm_tab_w(w: &FmWin, g: &FmGeo, cw: i32) -> i32 {
    let dpi = dpi_of(w.h);
    if w.tabs.n < 1 {
        return px(200, dpi);
    }
    w12_fm_tab_width(w.tabs.n, cw - g.plus_w, px(80, dpi), px(200, dpi))
}

/// FmCmdRect: kotak tombol perintah ke-c pada baris perintah.
fn fm_cmd_rect(w: &FmWin, g: &FmGeo, c: usize) -> RECT {
    let dpi = dpi_of(w.h);
    let mut left = px(8, dpi);
    for &dip in CMD_DIP.iter().take(c) {
        left += px(dip, dpi) + g.pad;
    }
    RECT {
        left,
        top: g.cmd_top + g.pad,
        right: left + px(CMD_DIP[c], dpi),
        bottom: g.cmd_top + g.cmd_h - g.pad,
    }
}

/// FmNavRect: kotak tombol navigasi ke-i.
fn fm_nav_rect(g: &FmGeo, i: i32) -> RECT {
    RECT {
        left: i * g.nav_btn_w,
        top: g.nav_top,
        right: (i + 1) * g.nav_btn_w,
        bottom: g.nav_top + g.nav_h,
    }
}

// ---- uji titik dan sorotan (w12fm_bar.inc:76-149) ----

/// FmHitTest: uji titik klien pada tab, tombol x, "+", navigasi, dan perintah.
fn fm_hit_test(w: &FmWin, x: i32, y: i32) -> FmHit {
    let mut hit = fm_no_hit();
    if y < 0 {
        return hit;
    }
    let rc = client_rect(w.h);
    let g = fm_geo_of(w);
    if y < g.tab_h {
        let n = w.tabs.n;
        let tab_w = fm_tab_w(w, &g, rc.right);
        let slot = w12_fm_tab_slot_at(x, n, tab_w, g.plus_w);
        if slot < 0 {
            return hit;
        }
        if slot == n {
            hit.kind = FM_HIT_PLUS;
            return hit;
        }
        hit.index = slot;
        hit.kind = if w12_fm_tab_close_hit(x, slot * tab_w, tab_w, g.close_size, g.close_pad) {
            FM_HIT_TABCLOSE
        } else {
            FM_HIT_TAB
        };
        return hit;
    }
    if y < g.cmd_top {
        if x >= 0 && x < 4 * g.nav_btn_w {
            hit.kind = FM_HIT_NAV;
            hit.index = x / g.nav_btn_w;
        }
        return hit;
    }
    if y < g.cmd_top + g.cmd_h {
        for c in 0..W12FC_COUNT {
            let r = fm_cmd_rect(w, &g, c);
            if x >= r.left && x < r.right {
                hit.kind = FM_HIT_CMD;
                hit.index = c as i32;
                break;
            }
        }
    }
    hit
}

/// FmSlotAtX: indeks tab di bawah x pada baris tab (untuk seret). -1 bila di luar baris tab.
fn fm_slot_at_x(w: &FmWin, x: i32, y: i32) -> i32 {
    let g = fm_geo_of(w);
    if y < 0 || y >= g.tab_h {
        return -1;
    }
    let rc = client_rect(w.h);
    w12_fm_tab_slot_at(x, w.tabs.n, fm_tab_w(w, &g, rc.right), g.plus_w)
}

/// FmUpdateTip: perbarui teks tooltip untuk sorotan `hit` (satu alat untuk seluruh klien).
fn fm_update_tip(w: &FmWin, hit: FmHit) {
    if w.tip.0.is_null() {
        return;
    }
    let text: &str = match hit.kind {
        FM_HIT_NAV => NAV_TIP[hit.index as usize],
        FM_HIT_CMD => CMD_TIP[hit.index as usize],
        FM_HIT_PLUS => "Tab baru",
        FM_HIT_TABCLOSE => "Tutup tab",
        _ => "",
    };
    let mut buf = to_wide_z(text);
    let ti = TTTOOLINFOW {
        cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
        hwnd: w.h,
        uId: FM_TOOL_ID,
        lpszText: PWSTR(buf.as_mut_ptr()),
        ..Default::default()
    };
    unsafe {
        SendMessageW(w.tip, TTM_UPDATETIPTEXTW, None, Some(LPARAM(&ti as *const TTTOOLINFOW as isize)));
    }
}

/// FmSetHot: ganti sorotan kursor; memperbarui tooltip dan menggambar ulang.
fn fm_set_hot(w: &mut FmWin, hit: FmHit) {
    if hit == w.hot {
        return;
    }
    w.hot = hit;
    fm_update_tip(w, hit);
    unsafe {
        let _ = InvalidateRect(Some(w.h), None, false);
    }
}

/// FmMoveTab: pindahkan entri tab dari `from` ke `to`. Urutan sama dengan memmove di C.
/// FmTab tidak Copy (memegang IExplorerBrowser), jadi memakai rotate, bukan salinan.
fn fm_move_tab(w: &mut FmWin, from: i32, to: i32) {
    let n = w.tabs.n;
    if from == to || from < 0 || to < 0 || from >= n || to >= n {
        return;
    }
    let (a, b) = (from as usize, to as usize);
    if a < b {
        w.tab[a..=b].rotate_left(1);
    } else {
        w.tab[b..=a].rotate_right(1);
    }
}

// ---- bagian 2: fungsi aksi, paint, dan tetikus ----

/// FmRefreshSelection (w12fm_bar.inc:179).
pub(crate) fn fm_refresh_selection(w: &mut FmWin) {
    // Ada item terpilih di tampilan tab aktif: IFolderView::ItemCount(SVGIO_SELECTION) > 0.
    let mut has = false;
    if let Some(t) = fm_active(w) {
        if let Some(fv) = fm_current_view(t).and_then(|sv| sv.cast::<IFolderView>().ok()) {
            if let Ok(n) = unsafe { fv.ItemCount(SVGIO_SELECTION) } {
                has = n > 0;
            }
        }
    }
    if has != w.has_sel {
        w.has_sel = has;
        let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
    }
}

/// FmHistRecord (w12fm_bar.inc:163).
pub(crate) fn fm_hist_record(t: &mut FmTab, pidl: *const ITEMIDLIST) {
    // Entri maju (indeks cur+1..n) dibuang lebih dulu, seperti FmHistRecord di C.
    let start = usize::try_from(t.hist.cur + 1).unwrap_or(0);
    let end = usize::try_from(t.hist.n).unwrap_or(0).min(FM_HISTMAX);
    for i in start..end {
        free_pidl(t.hpidl[i]);
        t.hpidl[i] = null_mut();
    }
    if w12_hist_push(&mut t.hist, FM_HISTMAX as i32) != 0 {
        // Penuh: buang yang paling lama, lalu geser satu slot ke kiri.
        free_pidl(t.hpidl[0]);
        t.hpidl.copy_within(1..FM_HISTMAX, 0);
        t.hpidl[FM_HISTMAX - 1] = null_mut();
    }
    let slot = usize::try_from(t.hist.cur).ok().and_then(|i| t.hpidl.get_mut(i));
    if let Some(slot) = slot {
        free_pidl(*slot);
        // C memakai ILCloneFull; crate 0.61 hanya menyediakan ILClone (klon penuh). Null bila pidl null.
        *slot = if pidl.is_null() { null_mut() } else { unsafe { ILClone(pidl) } };
    }
}

/// FmApplySort (w12fm_bar.inc:222).
pub(crate) fn fm_apply_sort(w: &mut FmWin) {
    let Some(t) = fm_active(w) else { return };
    if t.sort.col < 0 {
        return;
    }
    let Some(fv) = fm_folder_view(t) else { return };
    let sc = SORTCOLUMN {
        propkey: fm_sort_key(t.sort.col),
        direction: if t.sort.desc { SORT_DESCENDING } else { SORT_ASCENDING },
    };
    let _ = unsafe { fv.SetSortColumns(&[sc]) };
}

/// Kunci properti kolom Urutkan (w12fm_bar.inc:200-208): 0 nama, 1 tanggal diubah, 2 ukuran, 3 jenis.
/// GUID dan pid dari propkey.h ditulis lokal karena fitur EnhancedStorage tidak aktif di Cargo.toml.
fn fm_sort_key(col: i32) -> PROPERTYKEY {
    let fmtid = GUID::from_u128(0xb725f130_47ef_101a_a5f1_02608c9eebac);
    let pid = match col {
        1 => 14, // PKEY_DateModified
        2 => 12, // PKEY_Size
        3 => 4,  // PKEY_ItemTypeText
        _ => 10, // PKEY_ItemNameDisplay
    };
    PROPERTYKEY { fmtid, pid }
}

/// IFolderView2 dari tampilan aktif tab (w12fm_bar.inc:211-219); None bila tampilan belum siap.
fn fm_folder_view(t: &FmTab) -> Option<IFolderView2> {
    fm_current_view(t).and_then(|sv| sv.cast::<IFolderView2>().ok())
}

/// FmHistGo (w12fm_bar.inc:236).
pub(crate) fn fm_hist_go(w: &mut FmWin, dir: i32) {
    let id = usize::try_from(w.tabs.active)
        .ok()
        .and_then(|i| w.tabs.id.get(i).copied())
        .unwrap_or(-1);
    let failed = match fm_active(w) {
        Some(t) if t.eb.is_some() => {
            let from = t.hist.cur;
            let idx = if dir < 0 {
                w12_hist_back(&mut t.hist)
            } else {
                w12_hist_forward(&mut t.hist)
            };
            let pidl = usize::try_from(idx)
                .ok()
                .and_then(|i| t.hpidl.get(i).copied())
                .unwrap_or(null_mut());
            if idx < 0 {
                false
            } else if pidl.is_null() {
                t.hist.cur = from;
                false
            } else {
                t.hist_from = from;
                t.hist_pending = true;
                match t.eb.as_ref() {
                    Some(eb) => unsafe { eb.BrowseToIDList(pidl, SBSP_ABSOLUTE) }.is_err(),
                    None => false,
                }
            }
        }
        _ => false,
    };
    if failed {
        fm_tab_navigation_failed(w, id);
    }
}

/// FmActivateTab (w12fm_bar.inc:254).
pub(crate) fn fm_activate_tab(w: &mut FmWin) {
    fm_layout(w);
    fm_refresh_chrome(w);
    fm_apply_sort(w);
    if let Some(t) = fm_active(w) {
        if t.eb.is_some() {
            fm_focus_view(t);
        }
    }
    let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
}

/// FmNavigateHome (w12fm_bar.inc:265).
pub(crate) fn fm_navigate_home(w: &mut FmWin) {
    if fm_navigate(w, HOME_ADDRESS) {
        return;
    }
    let Ok(home) = (unsafe { SHGetKnownFolderIDList(&FOLDERID_Profile, 0, None) }) else {
        return;
    };
    if home.is_null() {
        return;
    }
    let ok = match fm_active(w) {
        Some(t) => fm_browse(t, home),
        None => false,
    };
    if ok {
        fm_status(w, "", false);
    }
    free_pidl(home);
}

/// FmNewTab (w12fm_bar.inc:276).
pub(crate) fn fm_new_tab(w: &mut FmWin) {
    let id = w12_tabs_add(&mut w.tabs, false);
    if id < 0 {
        fm_status(w, "Jumlah tab sudah maksimum", true);
        return;
    }
    let pos = usize::try_from(w.tabs.active).unwrap_or(0);
    if !fm_tab_init(w, pos) {
        // C: ZeroMemory pada slot tab. Di Rust dipakai fm_tab_empty() agar hist.cur = -1 dan sort.col = -1.
        if let Some(slot) = w.tab.get_mut(pos) {
            *slot = fm_tab_empty();
        }
        let _ = w12_tabs_close(&mut w.tabs, pos as i32);
        fm_status(w, "Tab baru tidak dapat dibuat", true);
        fm_activate_tab(w);
        return;
    }
    fm_navigate_home(w);
    fm_activate_tab(w);
}

/// FmCloseTab (w12fm_bar.inc:296).
pub(crate) fn fm_close_tab(w: &mut FmWin, index: i32) {
    let n = w.tabs.n;
    if index < 0 || index >= n {
        return;
    }
    let idx = index as usize;
    let last = (n - 1) as usize;
    if let Some(t) = w.tab.get_mut(idx) {
        fm_tab_destroy(t);
    }
    if idx < last && last < w.tab.len() {
        // memmove di C: geser tab kanan ke kiri; tab yang sudah dihancurkan pindah ke slot terakhir.
        w.tab[idx..=last].rotate_left(1);
    }
    if let Some(slot) = w.tab.get_mut(last) {
        *slot = fm_tab_empty();
    }
    let _ = w12_tabs_close(&mut w.tabs, index);
    w.hot = fm_no_hit();
    w.press = fm_no_hit();
    if w.tabs.n == 0 {
        // Tab terakhir ditutup: jendela ikut ditutup, dan w tidak boleh dipakai lagi.
        let _ = unsafe { DestroyWindow(w.h) };
        return;
    }
    fm_activate_tab(w);
}

/// FmRunVerb (w12fm_bar.inc:353-388): jalankan verb kanonik (cut, copy, paste, delete, rename, NewFolder)
/// lewat menu konteks shell. `background` = menu latar folder; selain itu = item yang dipilih.
pub(crate) fn fm_run_verb(w: &mut FmWin, background: bool, verb: &[u16]) {
    let Some(sv) = fm_active(w).and_then(|t| fm_current_view(t)) else {
        return;
    };
    let cm = if background {
        unsafe { sv.GetItemObject::<IContextMenu>(SVGIO_BACKGROUND) }.ok()
    } else {
        fm_selection_menu(&sv)
    };
    let Some(cm) = cm else {
        let msg = if background {
            "Perintah tidak tersedia di folder ini"
        } else {
            "Pilih item terlebih dahulu"
        };
        fm_status(w, msg, true);
        return;
    };
    let menu = unsafe { CreatePopupMenu() }.ok();
    let mut id = 0u32;
    if let Some(menu) = menu {
        // HRESULT QueryContextMenu diabaikan, sama seperti C. Rentang id 1..=0x7FFF.
        let _ = unsafe { cm.QueryContextMenu(menu, 0, 1, 0x7FFF, CMF_NORMAL) };
        id = fm_find_verb(&cm, menu, verb);
    }
    let mut ok = false;
    if id != 0 {
        let mut ici: CMINVOKECOMMANDINFO = unsafe { std::mem::zeroed() }; // SAFETY: struct C POD; nol adalah nilai valid.
        ici.cbSize = std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32;
        ici.hwnd = w.h;
        // MAKEINTRESOURCEA(id - 1): offset relatif terhadap idCmdFirst = 1.
        ici.lpVerb = PCSTR((id - 1) as usize as *const u8);
        ici.nShow = SW_SHOWNORMAL.0;
        ok = unsafe { cm.InvokeCommand(&ici) }.is_ok();
    }
    if let Some(menu) = menu {
        let _ = unsafe { DestroyMenu(menu) };
    }
    drop(cm);
    drop(sv);
    if ok {
        fm_status(w, "", false);
    } else if id != 0 {
        fm_status(w, "Perintah gagal dijalankan", true);
    } else {
        fm_status(w, "Perintah tidak tersedia di folder ini", true);
    }
}

/// FmSelectionMenu (w12fm_bar.inc:337-351): menu konteks item yang dipilih; None bila tidak ada pilihan.
fn fm_selection_menu(sv: &IShellView) -> Option<IContextMenu> {
    let fv: IFolderView2 = sv.cast().ok()?;
    let psia = unsafe { fv.GetSelection(false) }.ok()?;
    if unsafe { psia.GetCount() }.unwrap_or(0) == 0 {
        return None;
    }
    unsafe { psia.BindToHandler::<_, IContextMenu>(None::<&IBindCtx>, &BHID_SFUIObject) }.ok()
}

/// FmFindVerb (w12fm_bar.inc:314-334): cari id perintah (1..=0x7FFF) yang verb kanoniknya sama dengan `verb`;
/// rekursif ke submenu. 0 bila tidak ada.
fn fm_find_verb(cm: &IContextMenu, menu: HMENU, verb: &[u16]) -> u32 {
    let n = unsafe { GetMenuItemCount(Some(menu)) };
    for i in 0..n {
        let mut mi: MENUITEMINFOW = unsafe { std::mem::zeroed() }; // SAFETY: struct C POD; nol adalah nilai valid.
        mi.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
        mi.fMask = MIIM_ID | MIIM_SUBMENU;
        if unsafe { GetMenuItemInfoW(menu, i as u32, true, &mut mi) }.is_err() {
            continue;
        }
        if !mi.hSubMenu.0.is_null() {
            let sub = fm_find_verb(cm, mi.hSubMenu, verb);
            if sub != 0 {
                return sub;
            }
            continue;
        }
        if !(1..=0x7FFF).contains(&mi.wID) {
            continue;
        }
        let mut buf = [0u16; 128];
        let got = unsafe {
            cm.GetCommandString((mi.wID - 1) as usize, GCS_VERBW, None, PSTR(buf.as_mut_ptr() as *mut u8), 128)
        };
        if got.is_ok() && wide_eq_icase(&buf, verb) {
            return mi.wID;
        }
    }
    0
}

/// Pembanding UTF-16 tanpa membedakan huruf ASCII (pengganti `_wcsicmp` untuk verb ASCII). NUL akhir diabaikan.
fn wide_eq_icase(buf: &[u16], verb: &[u16]) -> bool {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let verb = verb.strip_suffix(&[0u16]).unwrap_or(verb);
    buf[..len].iter().map(|&c| fold_ascii(c)).eq(verb.iter().map(|&c| fold_ascii(c)))
}

fn fold_ascii(c: u16) -> u16 {
    if (b'A' as u16..=b'Z' as u16).contains(&c) {
        c + 32
    } else {
        c
    }
}

// GetPrivateProfileStringW (kernel32) dideklarasikan langsung: fitur `Win32_System_WindowsProgramming`
// tidak diaktifkan di Cargo.toml, dan Cargo.toml tidak diubah tanpa persetujuan. Signature sama dengan windows 0.61.3.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetPrivateProfileStringW(
        lp_app_name: *const u16,
        lp_key_name: *const u16,
        lp_default: *const u16,
        lp_returned_string: *mut u16,
        n_size: u32,
        lp_file_name: *const u16,
    ) -> u32;
}

/// FmDarkShell (w12fm_bar.inc:410): mode gelap menu konteks dan kotak dialog shell lewat uxtheme.
/// Ordinal 135 dan 133 tidak berdokumentasi, jadi best effort. Modul uxtheme tidak dilepas (sudah dimuat sepanjang proses).
fn fm_dark_shell(w: &FmWin) {
    let dll = to_wide_z("uxtheme.dll");
    let ux = match unsafe { LoadLibraryW(pcw(&dll)) } {
        Ok(m) => m,
        Err(_) => return,
    };
    // MAKEINTRESOURCEA(n): ordinal diteruskan sebagai PCSTR yang berisi nilai kecil.
    let set_pref = unsafe { GetProcAddress(ux, PCSTR(135usize as *const u8)) };
    let allow = unsafe { GetProcAddress(ux, PCSTR(133usize as *const u8)) };
    if let Some(f) = set_pref {
        // SetPreferredAppMode(1) = AllowDark: ikut tema aplikasi, bukan ForceDark.
        let f: unsafe extern "system" fn(i32) -> i32 = unsafe { std::mem::transmute(f) };
        let _ = unsafe { f(1) };
    }
    if let Some(f) = allow {
        // AllowDarkModeForWindow(hwnd, TRUE).
        let f: unsafe extern "system" fn(HWND, BOOL) -> BOOL = unsafe { std::mem::transmute(f) };
        let _ = unsafe { f(w.h, BOOL(1)) };
    }
    if !w.addr.0.is_null() {
        let theme = to_wide_z("DarkMode_CFD");
        let _ = unsafe { SetWindowTheme(w.addr, pcw(&theme), PCWSTR::null()) };
    }
}

/// FmInitBar (w12fm_bar.inc:425): tema, warna, Mica, kuas edit, tooltip satu alat, dan timer status pilihan.
/// Dipanggil dari WM_CREATE setelah kotak alamat dibuat.
pub(crate) fn fm_init_bar(w: &mut FmWin) {
    // settings.ini di folder yang sama dengan exe (g_ini). Bila exe tak terbaca, pakai nama relatif seperti C.
    let ini = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("settings.ini")))
        .unwrap_or_else(|| std::path::PathBuf::from("settings.ini"));
    let ini_w = to_wide_z(&ini.to_string_lossy());
    let section = to_wide_z("shell");
    let key = to_wide_z("theme");
    let empty = to_wide_z("");
    let mut theme = [0u16; 32];
    let n = unsafe {
        GetPrivateProfileStringW(
            section.as_ptr(),
            key.as_ptr(),
            empty.as_ptr(),
            theme.as_mut_ptr(),
            theme.len() as u32,
            ini_w.as_ptr(),
        )
    } as usize;
    let theme_s = String::from_utf16_lossy(&theme[..n.min(theme.len())]);
    let theme_arg = if theme_s.is_empty() { None } else { Some(theme_s.as_str()) };

    // SystemUsesLightTheme (DWORD). Bila tak terbaca, dianggap terang seperti C.
    let mut system_light: u32 = 1;
    let mut sz: u32 = std::mem::size_of::<u32>() as u32;
    let sub = to_wide_z(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let val = to_wide_z("SystemUsesLightTheme");
    let _ = unsafe {
        SHGetValueW(
            HKEY_CURRENT_USER,
            pcw(&sub),
            pcw(&val),
            None,
            Some(&mut system_light as *mut u32 as *mut c_void),
            Some(&mut sz as *mut u32),
        )
    };
    w.dark = w12_fm_theme_is_dark(theme_arg, system_light != 0);
    w.pal = w12_fm_palette_for(w.dark);

    // Aksen: RGB(0, 120, 212) bila warna aksen tidak terbaca. ARGB dari DWM diubah ke COLORREF.
    w.accent = COLORREF(0x00D4_7800);
    let mut argb: u32 = 0;
    let mut opaque = BOOL(0);
    let hr = unsafe { DwmGetColorizationColor(&mut argb, &mut opaque) };
    if hr.is_ok() {
        let (r, g, b) = ((argb >> 16) & 0xFF, (argb >> 8) & 0xFF, argb & 0xFF);
        w.accent = COLORREF(r | (g << 8) | (b << 16));
    }

    // Kegagalan atribut DWM diabaikan, sama seperti C.
    let dark_attr = BOOL(w.dark as i32);
    let backdrop: i32 = 2; // DWMSBT_MAINWINDOW (Mica); gagal di Windows lama diabaikan
    unsafe {
        let _ = DwmSetWindowAttribute(
            w.h,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_attr as *const BOOL as *const c_void,
            std::mem::size_of::<BOOL>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            w.h,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }

    if !w.edit_brush.0.is_null() {
        let _ = unsafe { DeleteObject(HGDIOBJ(w.edit_brush.0)) };
    }
    w.edit_brush = unsafe { CreateSolidBrush(fm_rgb(w.pal.panel)) };
    if w.dark {
        fm_dark_shell(w);
    }

    // Tooltip satu alat untuk seluruh bilah. Teks diisi fm_update_tip saat kursor pindah.
    let hinst = unsafe { GetModuleHandleW(PCWSTR::null()) }.ok().map(|m| HINSTANCE(m.0));
    w.tip = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            TOOLTIPS_CLASSW,
            PCWSTR::null(),
            WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            Some(w.h),
            None,
            hinst,
            None,
        )
    }
    .unwrap_or_default();
    if !w.tip.0.is_null() {
        let mut empty_text = [0u16; 1];
        let ti = TTTOOLINFOW {
            cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_SUBCLASS,
            hwnd: w.h,
            uId: FM_TOOL_ID,
            rect: RECT {
                left: 0,
                top: 0,
                right: 32767,
                bottom: px(FM_TABH_DIP + FM_NAVH_DIP + FM_CMDH_DIP, dpi_of(w.h)),
            },
            lpszText: PWSTR(empty_text.as_mut_ptr()),
            ..Default::default()
        };
        unsafe {
            SendMessageW(w.tip, TTM_ADDTOOLW, None, Some(LPARAM(&ti as *const TTTOOLINFOW as isize)));
        }
    }

    unsafe {
        SetTimer(Some(w.h), FM_TIMER_SEL, 300, None);
    }
}

/// Garis solid dengan pena sementara (FmLine, w12fm_bar.inc).
fn fm_line(dc: HDC, x1: i32, y1: i32, x2: i32, y2: i32, c: COLORREF, pw: i32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, pw, c);
        if pen.0.is_null() {
            return;
        }
        let old = SelectObject(dc, HGDIOBJ(pen.0));
        let _ = MoveToEx(dc, x1, y1, None);
        let _ = LineTo(dc, x2, y2);
        let _ = SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// Isi persegi dengan warna solid (FmFill).
fn fm_fill(dc: HDC, r: &RECT, c: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(c);
        if brush.0.is_null() {
            return;
        }
        let _ = FillRect(dc, r, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

/// Teks satu baris; font dipilih lalu dipulihkan (FmText).
fn fm_text(dc: HDC, r: &RECT, s: &mut [u16], c: COLORREF, font: HFONT, fmt: DRAW_TEXT_FORMAT) {
    let mut rr = *r;
    unsafe {
        let old = SelectObject(dc, HGDIOBJ(font.0));
        let _ = SetTextColor(dc, c);
        let _ = SetBkMode(dc, TRANSPARENT);
        let _ = DrawTextW(dc, s, &mut rr, fmt | DT_SINGLELINE | DT_NOPREFIX);
        let _ = SelectObject(dc, old);
    }
}

/// Silang × di dalam persegi (FmCross).
fn fm_cross(dc: HDC, r: &RECT, c: COLORREF, pw: i32) {
    let ins = (r.right - r.left) / 4;
    fm_line(dc, r.left + ins, r.top + ins, r.right - ins, r.bottom - ins, c, pw);
    fm_line(dc, r.right - ins, r.top + ins, r.left + ins, r.bottom - ins, c, pw);
}

/// Tanda plus (FmPlus).
fn fm_plus(dc: HDC, r: &RECT, c: COLORREF, s: i32, pw: i32) {
    let cx = (r.left + r.right) / 2;
    let cy = (r.top + r.bottom) / 2;
    fm_line(dc, cx - s, cy, cx + s, cy, c, pw);
    fm_line(dc, cx, cy - s, cx, cy + s, c, pw);
}

/// Panah navigasi: 0 kembali, 1 maju, 2 naik (FmNavIcon).
fn fm_nav_icon(dc: HDC, r: &RECT, kind: i32, c: COLORREF, s: i32, pw: i32) {
    let cx = (r.left + r.right) / 2;
    let cy = (r.top + r.bottom) / 2;
    let hs = s / 2;
    if kind == 2 {
        fm_line(dc, cx, cy - s, cx, cy + s, c, pw);
        fm_line(dc, cx, cy - s, cx - hs, cy - hs, c, pw);
        fm_line(dc, cx, cy - s, cx + hs, cy - hs, c, pw);
        return;
    }
    let dir = if kind == 0 { -1 } else { 1 };
    fm_line(dc, cx - s, cy, cx + s, cy, c, pw);
    fm_line(dc, cx + dir * s, cy, cx + dir * hs, cy - hs, c, pw);
    fm_line(dc, cx + dir * s, cy, cx + dir * hs, cy + hs, c, pw);
}

/// Kursor segitiga untuk tombol Urutkan dan Tampilan (FmCaret).
fn fm_caret(dc: HDC, cx: i32, cy: i32, s: i32, c: COLORREF) {
    let pts = [
        POINT { x: cx - s, y: cy - s / 2 },
        POINT { x: cx + s, y: cy - s / 2 },
        POINT { x: cx, y: cy + s / 2 },
    ];
    unsafe {
        let brush = CreateSolidBrush(c);
        if brush.0.is_null() {
            return;
        }
        let old_b = SelectObject(dc, HGDIOBJ(brush.0));
        let old_p = SelectObject(dc, GetStockObject(NULL_PEN));
        let _ = Polygon(dc, &pts);
        let _ = SelectObject(dc, old_p);
        let _ = SelectObject(dc, old_b);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

/// Font GDI dari ukuran poin pada DPI tertentu (FmFont).
fn fm_font(dpi: i32, pt: i32, face: &str) -> HFONT {
    let name = to_wide_z(face);
    unsafe {
        CreateFontW(
            -mul_div(pt, dpi, 72),
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

/// Judul tab; kosong menjadi "Tab baru" seperti di C.
fn tab_title(t: &FmTab) -> Vec<u16> {
    let n = t.title.iter().position(|&c| c == 0).unwrap_or(t.title.len());
    if n == 0 {
        to_wide("Tab baru")
    } else {
        t.title[..n].to_vec()
    }
}

/// Baris tab: latar, garis aksen, tombol ×, judul, dan tombol "+" (FmPaintTabs).
fn fm_paint_tabs(w: &FmWin, dc: HDC, g: &FmGeo, cw: i32, font: HFONT, dpi: i32) {
    let p = w.pal;
    let n = w.tabs.n.clamp(0, w.tab.len() as i32);
    let tab_w = fm_tab_w(w, g, cw);
    for i in 0..n {
        let r = RECT { left: i * tab_w, top: 0, right: (i + 1) * tab_w, bottom: g.tab_h };
        let active = i == w.tabs.active;
        let hot = w.hot.kind == FM_HIT_TAB && w.hot.index == i;
        let pressed = w.press.kind == FM_HIT_TAB && w.press.index == i;
        let bg = if active {
            p.panel
        } else if pressed {
            p.press
        } else if hot {
            p.hover
        } else {
            p.bg
        };
        fm_fill(dc, &r, fm_rgb(bg));
        if active {
            let under = RECT { left: r.left, top: r.bottom - px(2, dpi), right: r.right, bottom: r.bottom };
            fm_fill(dc, &under, w.accent);
        }
        let cs = g.close_size;
        let cright = r.right - g.close_pad;
        let ctop = (g.tab_h - cs) / 2;
        let cr = RECT { left: cright - cs, top: ctop, right: cright, bottom: ctop + cs };
        if w.hot.kind == FM_HIT_TABCLOSE && w.hot.index == i {
            fm_fill(dc, &cr, fm_rgb(p.press));
        }
        fm_cross(dc, &cr, fm_rgb(p.text), px(1, dpi));
        let tr = RECT { left: r.left + g.pad * 2, top: 0, right: cr.left - g.pad, bottom: g.tab_h };
        let mut title = tab_title(&w.tab[i as usize]);
        fm_text(dc, &tr, &mut title, fm_rgb(p.text), font, DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
    }
    let pr = RECT { left: n * tab_w, top: 0, right: n * tab_w + g.plus_w, bottom: g.tab_h };
    if w.press.kind == FM_HIT_PLUS {
        fm_fill(dc, &pr, fm_rgb(p.press));
    } else if w.hot.kind == FM_HIT_PLUS {
        fm_fill(dc, &pr, fm_rgb(p.hover));
    }
    fm_plus(dc, &pr, fm_rgb(p.text), px(5, dpi), px(1, dpi));
}

/// Tombol navigasi: kembali, maju, naik, segarkan (FmPaintNav).
fn fm_paint_nav(w: &FmWin, dc: HDC, g: &FmGeo, icon_font: HFONT, enabled: [bool; 4], dpi: i32) {
    let p = w.pal;
    for i in 0..4 {
        let r = fm_nav_rect(g, i);
        let hot = w.hot.kind == FM_HIT_NAV && w.hot.index == i;
        let pressed = w.press.kind == FM_HIT_NAV && w.press.index == i;
        if hot && pressed {
            fm_fill(dc, &r, fm_rgb(p.press));
        } else if hot {
            fm_fill(dc, &r, fm_rgb(p.hover));
        }
        let c = fm_rgb(if enabled[i as usize] { p.text } else { p.dim });
        if i == 3 {
            // Glyph Segarkan (U+E72C) dari Segoe MDL2 Assets.
            let mut glyph = to_wide("\u{E72C}");
            fm_text(dc, &r, &mut glyph, c, icon_font, DT_CENTER | DT_VCENTER);
        } else {
            fm_nav_icon(dc, &r, i, c, px(5, dpi), px(1, dpi));
        }
    }
}

/// Tombol perintah bilah beserta kursornya (FmPaintCmd).
fn fm_paint_cmd(w: &FmWin, dc: HDC, g: &FmGeo, font: HFONT, dpi: i32) {
    let p = w.pal;
    for c in 0..W12FC_COUNT {
        let idx = c as i32;
        let r = fm_cmd_rect(w, g, c);
        let hot = w.hot.kind == FM_HIT_CMD && w.hot.index == idx;
        let pressed = w.press.kind == FM_HIT_CMD && w.press.index == idx;
        if hot && pressed {
            fm_fill(dc, &r, fm_rgb(p.press));
        } else if hot {
            fm_fill(dc, &r, fm_rgb(p.hover));
        }
        let enabled = w12_fm_cmd_enabled(idx, w.has_sel);
        let tc = fm_rgb(if enabled { p.text } else { p.dim });
        let caret = idx == W12FC_SORT || idx == W12FC_VIEW;
        let right = if caret { r.right - px(18, dpi) } else { r.right - g.pad * 2 };
        let tr = RECT { left: r.left + g.pad * 2, top: r.top, right, bottom: r.bottom };
        let mut label = to_wide(CMD_LABEL[c]);
        fm_text(dc, &tr, &mut label, tc, font, DT_CENTER | DT_VCENTER | DT_END_ELLIPSIS);
        if caret {
            fm_caret(dc, r.right - px(10, dpi), (r.top + r.bottom) / 2, px(3, dpi), tc);
        }
    }
}

/// Teks status di kanan bilah perintah (FmPaintStatus).
fn fm_paint_status(w: &FmWin, dc: HDC, g: &FmGeo, cw: i32, font: HFONT, dpi: i32) {
    let n = w.status.iter().position(|&c| c == 0).unwrap_or(w.status.len());
    if n == 0 {
        return;
    }
    let last = fm_cmd_rect(w, g, W12FC_COUNT - 1);
    let sr = RECT {
        left: last.right + px(12, dpi),
        top: g.cmd_top,
        right: cw - g.pad * 2,
        bottom: g.cmd_top + g.cmd_h,
    };
    if sr.right <= sr.left {
        return;
    }
    // Merah = RGB(0xD1, 0x34, 0x38) bila status galat.
    let c = if w.status_error { fm_rgb(0xD13438) } else { fm_rgb(w.pal.text) };
    let mut s = w.status[..n].to_vec();
    fm_text(dc, &sr, &mut s, c, font, DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
}

/// Gambar bilah lewat bitmap memori lalu salin ke DC (FmPaint).
pub(crate) fn fm_paint(w: &mut FmWin, dc: HDC) {
    let cw = client_rect(w.h).right;
    let g = fm_geo_of(w);
    let bar_h = g.cmd_top + g.cmd_h;
    if cw <= 0 || bar_h <= 0 {
        return;
    }
    let dpi = dpi_of(w.h);
    let (can_back, can_fwd) = match fm_active(w) {
        Some(t) => (w12_hist_can_back(&t.hist), w12_hist_can_forward(&t.hist)),
        None => (false, false),
    };
    let mem = unsafe { CreateCompatibleDC(Some(dc)) };
    if mem.0.is_null() {
        return;
    }
    let bmp = unsafe { CreateCompatibleBitmap(dc, cw, bar_h) };
    if bmp.0.is_null() {
        let _ = unsafe { DeleteDC(mem) };
        return;
    }
    let old_bmp = unsafe { SelectObject(mem, HGDIOBJ(bmp.0)) };
    let font = fm_font(dpi, 9, "Segoe UI");
    let icon_font = fm_font(dpi, 12, "Segoe MDL2 Assets");
    let all = RECT { left: 0, top: 0, right: cw, bottom: bar_h };
    fm_fill(mem, &all, fm_rgb(w.pal.bg));
    fm_paint_tabs(w, mem, &g, cw, font, dpi);
    fm_paint_nav(w, mem, &g, icon_font, [can_back, can_fwd, true, true], dpi);
    fm_paint_cmd(w, mem, &g, font, dpi);
    fm_paint_status(w, mem, &g, cw, font, dpi);
    let sep = RECT { left: 0, top: bar_h - 1, right: cw, bottom: bar_h };
    fm_fill(mem, &sep, fm_rgb(w.pal.hover));
    unsafe {
        let _ = BitBlt(dc, 0, 0, cw, bar_h, Some(mem), 0, 0, SRCCOPY);
        let _ = SelectObject(mem, old_bmp);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteObject(HGDIOBJ(icon_font.0));
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
    }
}

/// Tetikus bilah (FmMouse, w12fm_bar.inc:750-840): sorotan, tekan-lepas, seret tab,
/// klik tengah menutup tab, klik ganda pada ruang kosong = tab baru.
pub(crate) fn fm_mouse(w: &mut FmWin, msg: u32, lp: LPARAM) {
    let raw = lp.0 as u32;
    let x = (raw & 0xFFFF) as u16 as i16 as i32;
    let y = ((raw >> 16) & 0xFFFF) as u16 as i16 as i32;
    match msg {
        WM_MOUSEMOVE => {
            if !w.tracking {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: w.h,
                    dwHoverTime: 0,
                };
                let _ = unsafe { TrackMouseEvent(&mut tme) };
                w.tracking = true;
            }
            let hit = fm_hit_test(w, x, y);
            fm_set_hot(w, hit);
            if w.press.kind == FM_HIT_TAB {
                // Seret: tukar posisi tab dengan slot di bawah kursor.
                let slot = fm_slot_at_x(w, x, y);
                let from = w.drag_tab;
                if slot >= 0 && slot < w.tabs.n && slot != from {
                    w12_tabs_move(&mut w.tabs, from, slot);
                    fm_move_tab(w, from, slot);
                    w.drag_tab = slot;
                    w.press.index = slot;
                    w.dragging = true;
                    let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
                }
            }
        }
        WM_MOUSELEAVE => {
            w.tracking = false;
            fm_set_hot(w, fm_no_hit());
        }
        WM_LBUTTONDOWN => {
            let hit = fm_hit_test(w, x, y);
            fm_press_hit(w, hit);
        }
        WM_LBUTTONDBLCLK => {
            // Klik kedua datang sebagai pesan ini (CS_DBLCLKS).
            let hit = fm_hit_test(w, x, y);
            if hit.kind == FM_HIT_NONE && y < fm_geo_of(w).tab_h {
                fm_new_tab(w);
            } else {
                fm_press_hit(w, hit);
            }
        }
        WM_LBUTTONUP => fm_lbutton_up(w, x, y),
        WM_MBUTTONDOWN => {
            // Klik tengah pada tab = tutup tab.
            let hit = fm_hit_test(w, x, y);
            if hit.kind == FM_HIT_TAB || hit.kind == FM_HIT_TABCLOSE {
                fm_close_tab(w, hit.index);
            }
        }
        _ => {}
    }
}

/// Tekan tombol kiri pada bilah: simpan tekanan, tangkap kursor, sorot (FmPressHit).
fn fm_press_hit(w: &mut FmWin, hit: FmHit) {
    w.press = hit;
    w.dragging = false;
    w.drag_tab = if hit.kind == FM_HIT_TAB { hit.index } else { -1 };
    if hit.kind != FM_HIT_NONE {
        let _ = unsafe { SetCapture(w.h) };
    }
    fm_set_hot(w, hit);
    let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
}

/// Lepas tombol kiri: beraksi hanya bila dilepas di atas tombol yang sama dengan saat ditekan.
fn fm_lbutton_up(w: &mut FmWin, x: i32, y: i32) {
    let press = w.press;
    let hit = fm_hit_test(w, x, y);
    let cap = unsafe { GetCapture() };
    if cap.0 == w.h.0 {
        let _ = unsafe { ReleaseCapture() };
    }
    w.press = fm_no_hit();
    w.dragging = false;
    w.drag_tab = -1;
    if press.kind == FM_HIT_NONE || hit != press {
        // Lepas di luar tombol yang ditekan: tidak ada aksi.
        let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
        return;
    }
    match hit.kind {
        FM_HIT_TAB => {
            if w.tabs.active != hit.index {
                w.tabs.active = hit.index;
                fm_activate_tab(w);
            }
        }
        FM_HIT_TABCLOSE => {
            // Bisa menghancurkan jendela: jangan sentuh w sesudahnya.
            fm_close_tab(w, hit.index);
            return;
        }
        FM_HIT_PLUS => {
            fm_new_tab(w);
            return;
        }
        FM_HIT_NAV => match hit.index {
            0 => fm_hist_go(w, -1),
            1 => fm_hist_go(w, 1),
            2 => fm_nav_up(w),
            _ => fm_nav_refresh(w),
        },
        FM_HIT_CMD => fm_bar_command(w, hit.index),
        _ => {}
    }
    let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
}

/// Perintah tombol bilah bila tersedia (FmBarCommand).
fn fm_bar_command(w: &mut FmWin, c: i32) {
    if c < 0 || (c as usize) >= W12FC_COUNT || !w12_fm_cmd_enabled(c, w.has_sel) {
        return;
    }
    match c {
        W12FC_NEWFOLDER => fm_run_verb(w, true, &to_wide("NewFolder")),
        W12FC_CUT => fm_run_verb(w, false, &to_wide("cut")),
        W12FC_COPY => fm_run_verb(w, false, &to_wide("copy")),
        W12FC_PASTE => fm_run_verb(w, true, &to_wide("paste")),
        W12FC_RENAME => fm_run_verb(w, false, &to_wide("rename")),
        W12FC_DELETE => fm_run_verb(w, false, &to_wide("delete")),
        W12FC_SORT => fm_sort_menu(w),
        W12FC_VIEW => fm_view_menu(w),
        _ => {}
    }
    let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
}

/// Menu popup kecil di bawah tombol perintah; mengembalikan id pilihan (0 bila dibatalkan).
fn fm_popup(w: &FmWin, c: i32, labels: &[&str], checked: i32, base: i32) -> i32 {
    let menu: HMENU = match unsafe { CreatePopupMenu() } {
        Ok(m) => m,
        Err(_) => return 0,
    };
    for (i, &label) in labels.iter().enumerate() {
        let i = i as i32;
        let flags = if i == checked { MF_STRING | MF_CHECKED } else { MF_STRING };
        let text = to_wide_z(label);
        let _ = unsafe { AppendMenuW(menu, flags, (base + i) as usize, pcw(&text)) };
    }
    let g = fm_geo_of(w);
    let r = fm_cmd_rect(w, &g, c as usize);
    let mut pt = POINT { x: r.left, y: r.bottom };
    let _ = unsafe { ClientToScreen(w.h, &mut pt) };
    let cmd = unsafe { TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, None, w.h, None) }.0;
    let _ = unsafe { DestroyMenu(menu) };
    cmd
}

/// Menu urutkan kolom; pilihan langsung diterapkan ke folder aktif (FmBarCommand SORT).
fn fm_sort_menu(w: &mut FmWin) {
    let cur = match fm_active(w) {
        Some(t) => t.sort.col,
        None => return,
    };
    let id = fm_popup(w, W12FC_SORT, &SORT_LABEL, cur, 100);
    if !(100..104).contains(&id) {
        return;
    }
    let applied = match fm_active(w) {
        Some(t) => {
            t.sort = w12_fm_sort_next(t.sort, id - 100);
            true
        }
        None => false,
    };
    if applied {
        fm_apply_sort(w);
    }
}

/// Menu mode tampilan (FmBarCommand VIEW).
fn fm_view_menu(w: &mut FmWin) {
    let id = fm_popup(w, W12FC_VIEW, &VIEW_LABEL, -1, 200);
    if (200..205).contains(&id) {
        fm_apply_view(w, id - 200);
    }
}

/// Terapkan mode tampilan ke folder aktif (FmApplyView): 0 detail, 1 daftar, 2 ikon sedang, 3 ikon besar, 4 ubin.
fn fm_apply_view(w: &mut FmWin, mode: i32) {
    let dpi = dpi_of(w.h);
    let view = match fm_active(w) {
        Some(t) => fm_folder_view(t),
        None => None,
    };
    let Some(fv) = view else {
        return;
    };
    let (vm, size) = match mode {
        1 => (FVM_LIST, 0),
        2 => (FVM_ICON, px(48, dpi)),
        3 => (FVM_ICON, px(96, dpi)),
        4 => (FVM_TILE, 0),
        _ => (FVM_DETAILS, 0),
    };
    let _ = unsafe { fv.SetViewModeAndIconSize(vm, size) };
}

/// Naik satu tingkat (FmCommand UP): klon PIDL, buang ID terakhir, buka hasilnya bila berhasil.
pub(crate) fn fm_nav_up(w: &mut FmWin) {
    let cur = match fm_active(w) {
        Some(t) => t.cur,
        None => return,
    };
    if cur.is_null() {
        return;
    }
    let parent = unsafe { ILClone(cur) };
    if parent.is_null() {
        return;
    }
    let removed = unsafe { ILRemoveLastID(Some(parent)) }.0 != 0;
    if removed {
        if let Some(t) = fm_active(w) {
            let _ = fm_browse(t, parent);
        }
    }
    free_pidl(parent);
}

/// Segarkan tampilan folder aktif (FmCommand REFRESH): IShellView::Refresh.
pub(crate) fn fm_nav_refresh(w: &mut FmWin) {
    let Some(t) = fm_active(w) else {
        return;
    };
    if t.eb.is_none() {
        return;
    }
    if let Some(sv) = fm_current_view(t) {
        let _ = unsafe { sv.Refresh() };
    }
}

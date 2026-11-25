//! Sumber: w12fm_bar.inc:1-840 (dipakai win12files.cpp). Status: PORT LENGKAP, belum diuji runtime.
//! Deviasi: GetPrivateProfileStringW lewat FFI kernel32 (fitur Win32_System_WindowsProgramming kini aktif, deklarasi tetap lokal).

use std::ffi::c_void;
use std::ptr::null_mut;

use windows::core::{Interface, BOOL, GUID, PCSTR, PCWSTR, PSTR, PWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, POINT, PROPERTYKEY, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetColorizationColor, DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
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
    SVGIO_SELECTION, SVGIO_ALLVIEW, SVSI_SELECT, FOLDERVIEWMODE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, DestroyWindow, GetClientRect, GetMenuItemCount, GetMenuItemInfoW,
    SendMessageW, CreateWindowExW, SetTimer, CW_USEDEFAULT, HMENU, MENUITEMINFOW, MF_CHECKED, MF_GRAYED, MF_STRING, MIIM_ID,
    MIIM_SUBMENU, SW_SHOWNORMAL, TrackPopupMenu, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_TOPALIGN, WINDOW_STYLE,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MOUSEMOVE, WS_EX_TOPMOST, WS_POPUP,
};

use crate::files::app::{
    fm_active, fm_browse, fm_current_view, fm_focus_view, fm_layout, fm_navigate, fm_refresh_chrome, fm_status,
    fm_tab_destroy, fm_tab_empty, fm_tab_init, fm_tab_navigation_failed, free_pidl,
};
use crate::files::state::{
    FmHit, FmTab, FmWin, FM_HIT_ADDR, FM_HIT_CHEV, FM_HIT_CMD, FM_HIT_CRUMB, FM_HIT_SPLIT, FM_HIT_TERMBTN, FM_HIT_NAV, FM_HIT_NONE, FM_HIT_VIEWBTN,
    FM_HIT_PLUS, FM_HIT_TAB, FM_HIT_TABCLOSE, FM_HISTMAX, FM_TIMER_SEL, FM_TOOL_ID, HOME_ADDRESS, SORT_LABEL,
    VIEW_LABEL,
};
use crate::fm_logic::{
    w12_fm_palette_for, w12_fm_tab_close_hit, w12_fm_tab_slot_at, w12_fm_tab_width, w12_fm_theme_is_dark,
    w12_fm_cmd_enabled, w12_fm_sort_next, w12_hist_back, w12_hist_can_back, w12_hist_can_forward, w12_hist_forward,
    w12_hist_push, w12_tabs_add, w12_tabs_close, w12_tabs_move, W12FC_COPY, W12FC_COUNT, W12FC_CUT, W12FC_DELETE,
    W12FC_NEWFOLDER, W12FC_PASTE, W12FC_RENAME, W12FC_SORT, W12FC_VIEW,
};
use crate::files::chrome;
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
    let (mut n_sel, mut n_all, mut details) = (0, 0, w.details);
    if let Some(t) = fm_active(w) {
        if let Some(fv) = fm_current_view(t).and_then(|sv| sv.cast::<IFolderView>().ok()) {
            if let Ok(n) = unsafe { fv.ItemCount(SVGIO_SELECTION) } {
                has = n > 0;
                n_sel = n;
            }
            n_all = unsafe { fv.ItemCount(SVGIO_ALLVIEW) }.unwrap_or(0);
            if let Ok(fv2) = fv.cast::<IFolderView2>() {
                let mut vm = FOLDERVIEWMODE(0);
                let mut sz = 0i32;
                if unsafe { fv2.GetViewModeAndIconSize(&mut vm, &mut sz) }.is_ok() {
                    details = vm == FVM_DETAILS;
                }
            }
        }
    }
    if has != w.has_sel || n_sel != w.n_sel || n_all != w.n_items || details != w.details {
        w.has_sel = has;
        w.n_sel = n_sel;
        w.n_items = n_all;
        w.details = details;
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

// ---- menu klik kanan tab (2026-11-24) ----

thread_local! {
    /// Lokasi tab yang ditutup (terbaru di akhir), untuk "Buka ulang tab yang ditutup" (Ctrl+Shift+T).
    static CLOSED_TABS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Alamat parsing tab (jalur berkas atau shell:::{..}); None bila tab belum punya lokasi.
fn fm_tab_path(w: &FmWin, index: i32) -> Option<String> {
    use windows::Win32::UI::Shell::{SHGetNameFromIDList, SIGDN_DESKTOPABSOLUTEPARSING};
    let t = w.tab.get(usize::try_from(index).ok()?)?;
    if t.cur.is_null() {
        return None;
    }
    // SAFETY: pidl tab valid; string dibebaskan CoTaskMemFree.
    unsafe {
        let s = SHGetNameFromIDList(t.cur, SIGDN_DESKTOPABSOLUTEPARSING).ok()?;
        let out = String::from_utf16_lossy(s.as_wide());
        windows::Win32::System::Com::CoTaskMemFree(Some(s.0 as *const std::ffi::c_void));
        (!out.is_empty()).then_some(out)
    }
}

/// Tab baru yang langsung membuka `path`.
fn fm_new_tab_at(w: &mut FmWin, path: &str) {
    let before = w.tabs.n;
    fm_new_tab(w);
    if w.tabs.n > before {
        crate::files::app::fm_navigate_to(w, path, false);
    }
}

/// Ctrl+Shift+T / menu tab: buka lagi tab terakhir yang ditutup.
pub(crate) fn fm_reopen_closed_tab(w: &mut FmWin) {
    match CLOSED_TABS.with(|c| c.borrow_mut().pop()) {
        Some(p) => fm_new_tab_at(w, &p),
        None => fm_status(w, "Tidak ada tab yang baru ditutup", false),
    }
}

/// Menu klik kanan tab, seperti Explorer Windows 11 (ditambah buka ulang tab yang ditutup).
fn fm_tab_menu(w: &mut FmWin, index: i32, x: i32, y: i32) {
    use windows::Win32::UI::WindowsAndMessaging::MF_SEPARATOR;
    let n = w.tabs.n;
    let path = fm_tab_path(w, index);
    let has_closed = CLOSED_TABS.with(|c| !c.borrow().is_empty());
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else { return };
    let add = |id: usize, label: &str, on: bool| {
        let t = to_wide_z(label);
        let f = if on { MF_STRING } else { MF_STRING | MF_GRAYED };
        let _ = unsafe { AppendMenuW(menu, f, id, pcw(&t)) };
    };
    let sep = || {
        let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) };
    };
    add(1, "Tab baru\tCtrl+T", true);
    add(2, "Duplikat tab", path.is_some());
    add(3, "Pindahkan ke jendela baru", path.is_some());
    sep();
    add(4, "Tutup tab\tCtrl+W", true);
    add(5, "Tutup tab lain", n > 1);
    add(6, "Tutup tab di sebelah kanan", index < n - 1);
    sep();
    add(7, "Buka ulang tab yang ditutup\tCtrl+Shift+T", has_closed);
    let mut pt = POINT { x, y };
    let _ = unsafe { ClientToScreen(w.h, &mut pt) };
    let id = unsafe { TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, None, w.h, None) }.0;
    let _ = unsafe { DestroyMenu(menu) };
    match id {
        1 => fm_new_tab(w),
        2 => {
            if let Some(p) = path {
                fm_new_tab_at(w, &p);
            }
        }
        3 => {
            if let Some(p) = path {
                if crate::files::frame::fm_open(Some(&p), false).is_some() {
                    // Jendela baru sudah terbuka: tab asal ditutup (bisa menutup jendela ini bila tab terakhir).
                    fm_close_tab(w, index);
                    return;
                }
                fm_status(w, "Jendela baru tidak dapat dibuka", true);
            }
        }
        4 => {
            fm_close_tab(w, index);
            return;
        }
        5 => {
            for j in (index + 1..n).rev() {
                fm_close_tab(w, j);
            }
            for j in (0..index).rev() {
                fm_close_tab(w, j);
            }
        }
        6 => {
            for j in (index + 1..n).rev() {
                fm_close_tab(w, j);
            }
        }
        7 => fm_reopen_closed_tab(w),
        _ => {}
    }
    let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
}

/// FmCloseTab (w12fm_bar.inc:296).
pub(crate) fn fm_close_tab(w: &mut FmWin, index: i32) {
    let n = w.tabs.n;
    if index < 0 || index >= n {
        return;
    }
    if let Some(p) = fm_tab_path(w, index) {
        CLOSED_TABS.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() >= 20 {
                c.remove(0);
            }
            c.push(p);
        });
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
    if !w.term.0.is_null() {
        crate::files::term::term_set_dark(w.term, w.dark);
    }

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
    // Tanpa Mica: baris tab digambar warna solid; area tombol jendela memakai warna bilah judul yang sama.
    let backdrop: i32 = 1; // DWMSBT_NONE
    let cap = chrome::caption_color(w.dark);
    let cap_ref = fm_rgb(cap);
    unsafe {
        let _ = DwmSetWindowAttribute(
            w.h,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_attr as *const BOOL as *const c_void,
            std::mem::size_of::<BOOL>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            w.h,
            DWMWA_CAPTION_COLOR,
            &cap_ref as *const COLORREF as *const c_void,
            std::mem::size_of::<COLORREF>() as u32,
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
    w.edit_brush = unsafe { CreateSolidBrush(fm_rgb(chrome::box_color(w.dark))) };
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
                bottom: 32767,
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

/// Gambar bilah (chrome.rs, gaya File Explorer Windows 11).
pub(crate) fn fm_paint(w: &mut FmWin, dc: HDC) {
    let (can_back, can_fwd) = match fm_active(w) {
        Some(t) => (w12_hist_can_back(&t.hist), w12_hist_can_forward(&t.hist)),
        None => (false, false),
    };
    chrome::paint(w, dc, [can_back, can_fwd, true, true]);
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
            let hit = chrome::hit_test(w, x, y);
            chrome::set_hot(w, hit);
            if w.press.kind == FM_HIT_SPLIT {
                crate::files::extras::split_drag(w, y);
            }
            if w.press.kind == FM_HIT_TAB {
                // Seret: tukar posisi tab dengan slot di bawah kursor.
                let slot = chrome::slot_at_x(w, x, y);
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
            chrome::set_hot(w, fm_no_hit());
        }
        WM_LBUTTONDOWN => {
            let hit = chrome::hit_test(w, x, y);
            fm_press_hit(w, hit);
        }
        WM_LBUTTONDBLCLK => {
            // Klik kedua datang sebagai pesan ini (CS_DBLCLKS).
            let hit = chrome::hit_test(w, x, y);
            if hit.kind == FM_HIT_NONE && y < chrome::geo(w).tab_h {
                fm_new_tab(w);
            } else if hit.kind == FM_HIT_SPLIT {
                crate::files::extras::split_reset(w);
            } else {
                fm_press_hit(w, hit);
            }
        }
        WM_LBUTTONUP => fm_lbutton_up(w, x, y),
        0x0205 => {
            // WM_RBUTTONUP: menu tab.
            let hit = chrome::hit_test(w, x, y);
            if hit.kind == FM_HIT_TAB || hit.kind == FM_HIT_TABCLOSE {
                fm_tab_menu(w, hit.index, x, y);
            }
        }
        WM_MBUTTONDOWN => {
            // Klik tengah pada tab = tutup tab.
            let hit = chrome::hit_test(w, x, y);
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
    chrome::set_hot(w, hit);
    let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
}

/// Lepas tombol kiri: beraksi hanya bila dilepas di atas tombol yang sama dengan saat ditekan.
fn fm_lbutton_up(w: &mut FmWin, x: i32, y: i32) {
    let press = w.press;
    let hit = chrome::hit_test(w, x, y);
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
        FM_HIT_CRUMB => chrome::crumb_go(w, hit.index),
        FM_HIT_CHEV => {
            let r = w.chev_rects.iter().find(|(k, _)| *k == hit.index).map(|(_, r)| *r);
            if let Some(r) = r {
                crate::files::extras::chevron_menu(w, hit.index, r);
            }
        }
        FM_HIT_TERMBTN => crate::files::extras::term_toggle(w),
        FM_HIT_ADDR => {
            chrome::addr_edit(w, true);
            return;
        }
        FM_HIT_VIEWBTN => {
            fm_apply_view(w, if hit.index == 0 { 0 } else { 3 });
            fm_refresh_selection(w);
        }
        _ => {}
    }
    let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
}

/// Perintah tombol bilah bila tersedia (FmBarCommand).
fn fm_bar_command(w: &mut FmWin, c: i32) {
    if c == chrome::CMD_MORE {
        let r = chrome::cmd_rect(w, c as usize);
        crate::files::extras::more_menu(w, r);
        let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
        return;
    }
    if c < 0 || (c as usize) >= W12FC_COUNT || !w12_fm_cmd_enabled(c, w.has_sel) {
        return;
    }
    match c {
        W12FC_NEWFOLDER => {
            let r = chrome::cmd_rect(w, c as usize);
            crate::files::extras::new_menu(w, r);
        }
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
    let r = chrome::cmd_rect(w, c as usize);
    let mut pt = POINT { x: r.left, y: r.bottom };
    let _ = unsafe { ClientToScreen(w.h, &mut pt) };
    let cmd = unsafe { TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, None, w.h, None) }.0;
    let _ = unsafe { DestroyMenu(menu) };
    cmd
}

// Menu "Lainnya" pindah ke extras::more_menu (2026-11-19).

/// Menu urutkan kolom; pilihan langsung diterapkan ke folder aktif (FmBarCommand SORT).
/// 2026-11-24: seperti Explorer Windows 11, ditambah Naik/Turun dan submenu "Kelompokkan menurut" (IFolderView2::SetGroupBy).
fn fm_sort_menu(w: &mut FmWin) {
    use windows::Win32::UI::WindowsAndMessaging::{MF_POPUP, MF_SEPARATOR};
    let Some(t) = fm_active(w) else { return };
    let sort = t.sort;
    let fv = fm_folder_view(t);
    // Kelompok aktif: indeks kolom SORT_LABEL, -1 = tidak dikelompokkan (PKEY_Null) atau kolom lain.
    let mut group = -1i32;
    if let Some(fv) = fv.as_ref() {
        let mut key = PROPERTYKEY::default();
        let mut asc = windows::core::BOOL(1);
        if unsafe { fv.GetGroupBy(&mut key, Some(&mut asc)) }.is_ok() {
            group = (0..SORT_LABEL.len() as i32).find(|&c| fm_sort_key(c) == key).unwrap_or(-1);
        }
    }
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else { return };
    let Ok(gm) = (unsafe { CreatePopupMenu() }) else {
        let _ = unsafe { DestroyMenu(menu) };
        return;
    };
    let add = |m: HMENU, id: usize, label: &str, on: bool| {
        let t = to_wide_z(label);
        let f = if on { MF_STRING | MF_CHECKED } else { MF_STRING };
        let _ = unsafe { AppendMenuW(m, f, id, pcw(&t)) };
    };
    for (i, label) in SORT_LABEL.iter().enumerate() {
        add(menu, 100 + i, label, i as i32 == sort.col);
        add(gm, 120 + i, label, i as i32 == group);
    }
    let _ = unsafe { AppendMenuW(gm, MF_SEPARATOR, 0, PCWSTR::null()) };
    add(gm, 124, "(Tidak ada)", group < 0);
    let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) };
    add(menu, 110, "Naik", sort.col >= 0 && !sort.desc);
    add(menu, 111, "Turun", sort.col >= 0 && sort.desc);
    let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) };
    let gt = to_wide_z("Kelompokkan menurut");
    let _ = unsafe { AppendMenuW(menu, MF_POPUP, gm.0 as usize, pcw(&gt)) };
    let r = chrome::cmd_rect(w, W12FC_SORT as usize);
    let mut pt = POINT { x: r.left, y: r.bottom };
    let _ = unsafe { ClientToScreen(w.h, &mut pt) };
    let id = unsafe { TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, None, w.h, None) }.0;
    // DestroyMenu menu induk ikut menghancurkan submenu kelompok.
    let _ = unsafe { DestroyMenu(menu) };
    match id {
        100..=103 => {
            if let Some(t) = fm_active(w) {
                // Pilih kolom dari menu: arah tetap (Explorer); kolom baru mulai Naik.
                let col = id - 100;
                t.sort = if col == t.sort.col { t.sort } else { w12_fm_sort_next(t.sort, col) };
            }
            fm_apply_sort(w);
        }
        110 | 111 => {
            if let Some(t) = fm_active(w) {
                if t.sort.col < 0 {
                    t.sort.col = 0;
                }
                t.sort.desc = id == 111;
            }
            fm_apply_sort(w);
        }
        120..=124 => {
            if let Some(fv) = fv {
                let key = if id == 124 { PROPERTYKEY::default() } else { fm_sort_key(id - 120) };
                if unsafe { fv.SetGroupBy(&key, true) }.is_err() {
                    fm_status(w, "Pengelompokan tidak didukung di folder ini", true);
                }
            }
        }
        _ => {}
    }
}

/// Menu mode tampilan (FmBarCommand VIEW).
/// Sejak 2026-11-19 sama dengan menu Tampilan Explorer Windows 11: 8 mode, Tampilan ringkas, submenu Tampilkan.
fn fm_view_menu(w: &mut FmWin) {
    use crate::files::extras::{adv_on, pane_on, ADV_CHECK, ADV_COMPACT, ADV_EXT, ADV_HIDDEN};
    use windows::Win32::UI::Shell::{FVM_CONTENT, FVM_SMALLICON};
    use windows::Win32::UI::WindowsAndMessaging::{MF_POPUP, MF_SEPARATOR};
    const MODES: [(&str, FOLDERVIEWMODE, i32); 8] = [
        ("Ikon sangat besar", FVM_ICON, 256),
        ("Ikon besar", FVM_ICON, 96),
        ("Ikon sedang", FVM_ICON, 48),
        ("Ikon kecil", FVM_SMALLICON, 16),
        ("Daftar", FVM_LIST, 16),
        ("Detail", FVM_DETAILS, 16),
        ("Ubin", FVM_TILE, 48),
        ("Konten", FVM_CONTENT, 32),
    ];
    let dpi = dpi_of(w.h);
    let fv = fm_active(w).and_then(|t| fm_folder_view(t));
    let (cur_mode, cur_size) = match fv.as_ref() {
        Some(fv) => {
            let mut m = FOLDERVIEWMODE(0);
            let mut sz = 0i32;
            let _ = unsafe { fv.GetViewModeAndIconSize(&mut m, &mut sz) };
            (m, sz)
        }
        None => (FOLDERVIEWMODE(0), 0),
    };
    // Mode aktif: mode sama, untuk FVM_ICON ukuran terdekat.
    let mut checked = -1i32;
    for (i, (_, m, sz)) in MODES.iter().enumerate() {
        if *m == cur_mode {
            if *m != FVM_ICON {
                checked = i as i32;
                break;
            }
            let want = if cur_size >= 192 { 256 } else if cur_size >= 72 { 96 } else { 48 };
            if *sz == want {
                checked = i as i32;
                break;
            }
        }
    }
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else { return };
    let Ok(show) = (unsafe { CreatePopupMenu() }) else {
        let _ = unsafe { DestroyMenu(menu) };
        return;
    };
    let add = |m: HMENU, id: usize, label: &str, on: bool| {
        let t = to_wide_z(label);
        let f = if on { MF_STRING | MF_CHECKED } else { MF_STRING };
        let _ = unsafe { AppendMenuW(m, f, id, pcw(&t)) };
    };
    for (i, (label, _, _)) in MODES.iter().enumerate() {
        add(menu, 200 + i, label, i as i32 == checked);
    }
    let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) };
    add(menu, 220, "Tampilan ringkas", adv_on(ADV_COMPACT));
    add(show, 230, "Panel detail\tAlt+Shift+P", pane_on(true));
    add(show, 231, "Panel pratinjau\tAlt+P", pane_on(false));
    let _ = unsafe { AppendMenuW(show, MF_SEPARATOR, 0, PCWSTR::null()) };
    add(show, 232, "Kotak centang item", adv_on(ADV_CHECK));
    add(show, 233, "Ekstensi nama file", adv_on(ADV_EXT));
    add(show, 234, "Item tersembunyi", adv_on(ADV_HIDDEN));
    let st = to_wide_z("Tampilkan");
    let _ = unsafe { AppendMenuW(menu, MF_POPUP, show.0 as usize, pcw(&st)) };
    let r = chrome::cmd_rect(w, W12FC_VIEW as usize);
    let mut pt = POINT { x: r.left, y: r.bottom };
    let _ = unsafe { ClientToScreen(w.h, &mut pt) };
    let id = unsafe { TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, None, w.h, None) }.0;
    // DestroyMenu menu induk ikut menghancurkan submenu "Tampilkan".
    let _ = unsafe { DestroyMenu(menu) };
    match id {
        200..=207 => {
            let (_, m, sz) = MODES[(id - 200) as usize];
            if let Some(fv) = fv {
                // Dibatasi 256: dulu dikali DPI tanpa batas (384 px di 150%), melebihi ukuran maksimum ikon/gambar
                // mini sehingga gambar tampak melar (2026-11-19).
                let size = if m == FVM_ICON { px(sz, dpi).min(256) } else { 0 };
                let _ = unsafe { fv.SetViewModeAndIconSize(m, size) };
            }
            fm_refresh_selection(w);
            crate::files::viewstate::fill_soon(w.h);
        }
        220 => crate::files::extras::toggle_adv(w, ADV_COMPACT),
        230 => crate::files::extras::pane_toggle(w, true),
        231 => crate::files::extras::pane_toggle(w, false),
        232 => crate::files::extras::toggle_adv(w, ADV_CHECK),
        233 => crate::files::extras::toggle_adv(w, ADV_EXT),
        234 => crate::files::extras::toggle_adv(w, ADV_HIDDEN),
        _ => {}
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
        2 => (FVM_ICON, px(48, dpi).min(256)),
        3 => (FVM_ICON, px(96, dpi).min(256)),
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

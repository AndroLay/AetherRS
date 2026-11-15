//! Port win12files.cpp (helper jendela, tata letak, tab, navigasi). Status: ditulis, belum diuji runtime; WinMain, WndProc, dan loop pesan (FmRunLoop, FmHandleKey, FmTranslateView) ada di frame.rs dan wndproc.rs.
//! Catatan: MulDiv dan helper string lstrcpynW/lstrcatW diganti fungsi lokal agar tidak memerlukan fitur crate tambahan.

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows::core::{Interface, Ref, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, COLORREF, GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, S_OK, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DeleteObject, EndPaint, GetStockObject, InvalidateRect, SetBkColor, SetTextColor,
    COLOR_BTNFACE, DEFAULT_GUI_FONT, HBRUSH, HDC, HGDIOBJ, PAINTSTRUCT,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, IBindCtx, CLSCTX_INPROC_SERVER};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Controls::{InitCommonControlsEx, EM_SETSEL, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, WM_MOUSELEAVE};
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_RETURN,
    VK_SHIFT,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    ExplorerBrowser, IExplorerBrowser, IExplorerBrowserEvents, IExplorerBrowserEvents_Impl, IInputObject,
    IInputObject_Vtbl, IShellView, ILClone, ILFindLastID, ILRemoveLastID, SHGetFileInfoW, SHGetNameFromIDList,
    ShellExecuteW, EBO_ALWAYSNAVIGATE, EBO_NOBORDER, EBO_SHOWFRAMES, EXPLORER_BROWSER_OPTIONS, FOLDERSETTINGS,
    FVM_DETAILS, SBSP_ABSOLUTE, SHFILEINFOW, SHGFI_FLAGS, SHGFI_ICON, SHGFI_PIDL, SHGFI_SMALLICON,
    SIGDN_DESKTOPABSOLUTEEDITING, SIGDN_NORMALDISPLAY, SVSI_ENSUREVISIBLE, SVSI_FOCUSED, SVSI_SELECT,
    SVUIA_ACTIVATE_FOCUS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow, DispatchMessageW, GetAncestor, GetClientRect,
    GetMessageW, GetWindowTextW, IsIconic, KillTimer, LoadCursorW, MoveWindow, PostQuitMessage, RegisterClassExW,
    SendMessageW, SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, CREATESTRUCTW,
    CS_DBLCLKS, CW_USEDEFAULT, ES_AUTOHSCROLL, GA_ROOT, HICON, HMENU, HWND_MESSAGE, ICON_SMALL,
    IDC_ARROW, MSG, SWP_NOACTIVATE, SWP_NOZORDER, SW_RESTORE, SW_SHOW, SW_SHOWNORMAL, WM_APP, WM_COMMAND,
    WM_COPYDATA, WM_CREATE, WM_CTLCOLOREDIT, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDBLCLK,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
    WM_SETFONT, WM_SETICON, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN,
    WS_EX_CLIENTEDGE, WS_OVERLAPPEDWINDOW, WS_VISIBLE, WNDCLASS_STYLES, WINDOW_EX_STYLE, WINDOW_STYLE, HCURSOR, FindWindowExW,
};

use crate::fm_logic::{
    w12_fm_accel, w12_fm_explorer_args, w12_fm_normalize_address, w12_fm_parse_args, w12_tabs_add, w12_tabs_close,
    w12_tabs_cycle, W12FmPalette, W12FmSort, W12Hist, W12Tabs, W12FA_BACK, W12FA_CLOSE_TAB, W12FA_FOCUS_ADDRESS,
    W12FA_FOCUS_SEARCH, W12FA_FORWARD, W12FA_NEW_FOLDER, W12FA_NEW_TAB, W12FA_NEW_WINDOW, W12FA_NEXT_TAB,
    W12FA_NONE, W12FA_PREV_TAB, W12FA_REFRESH, W12FA_TAB_N, W12FA_UP, W12FM_MAXTABS,
};
use crate::files::state::{
    FmHit, FmTab, FmWin, COPY_TAG, CLASS_FRAME, CLASS_MSG, FM_CMDH_DIP, FM_HISTMAX, FM_MAX_WINDOWS, FM_NAVH_DIP,
    FM_SEARCH_PATH, FM_TABH_DIP, FM_TIMER_SEL, HOME_ADDRESS, MSG_SEARCH_DONE, MUTEX_NAME, WINS,
};
use crate::files::bar::{
    fm_activate_tab, fm_apply_sort, fm_close_tab, fm_hist_go, fm_hist_record, fm_init_bar, fm_mouse, fm_new_tab,
    fm_navigate_home, fm_paint, fm_refresh_selection, fm_run_verb,
};
use crate::files::search_ui::{
    fm_search_cancel, fm_search_done, fm_search_init, fm_search_shutdown, fm_search_start, fm_update_cue,
};

// ---- util (win12files.cpp:121-154; pengganti MulDiv, lstrcpynW, lstrcatW) ----

/// Pengganti MulDiv(a, b, c) dengan pembulatan sama untuk nilai positif. Tidak memerlukan fitur WindowsProgramming.
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
pub(crate) fn px(dip: i32, dpi: i32) -> i32 {
    mul_div(dip, dpi, 96)
}

/// DpiOf(h): 96 bila 0.
pub(crate) fn dpi_of(h: HWND) -> i32 {
    let d = unsafe { GetDpiForWindow(h) } as i32;
    if d == 0 { 96 } else { d }
}

/// RECT nol (pengganti ZeroMemory).
fn rect0() -> RECT {
    RECT { left: 0, top: 0, right: 0, bottom: 0 }
}

/// Teks wide dengan NUL di akhir (untuk Win32).
pub(crate) fn wide_z(s: &str) -> Vec<u16> {
    crate::wide::to_wide_z(s)
}

/// Panjang string wide sampai NUL (tanpa NUL).
fn wlen(s: &[u16]) -> usize {
    s.iter().position(|&c| c == 0).unwrap_or(s.len())
}

/// Pengganti lstrcpynW: salin paling banyak dst.len()-1 karakter lalu akhiri NUL.
fn copy_wide(dst: &mut [u16], src: &[u16]) {
    if dst.is_empty() {
        return;
    }
    let n = wlen(src).min(dst.len() - 1);
    dst[..n].copy_from_slice(&src[..n]);
    dst[n] = 0;
}

/// Pengganti lstrcatW dengan batas dst.len().
fn cat_wide(dst: &mut [u16], src: &[u16]) {
    let start = wlen(dst);
    if start >= dst.len().saturating_sub(1) {
        return;
    }
    let room = dst.len() - 1 - start;
    let n = wlen(src).min(room);
    dst[start..start + n].copy_from_slice(&src[..n]);
    dst[start + n] = 0;
}

/// Salin string wide NUL-terminated dari pointer Win32 ke Vec (tanpa NUL).
///
/// SAFETY: pemanggil menjamin `p` null atau menunjuk ke string wide yang diakhiri NUL dan masih valid.
unsafe fn pwstr_vec(p: *const u16) -> Vec<u16> {
    let mut v = Vec::new();
    if p.is_null() {
        return v;
    }
    let mut i = 0isize;
    loop {
        let c = *p.offset(i);
        if c == 0 {
            break;
        }
        v.push(c);
        i += 1;
    }
    v
}

/// Bebaskan PIDL dengan CoTaskMemFree bila tidak null (pengganti CoTaskMemFree bersyarat di C).
pub(crate) fn free_pidl(p: *mut ITEMIDLIST) {
    if !p.is_null() {
        unsafe { CoTaskMemFree(Some(p as *const c_void)) };
    }
}

// ---- akses jendela (win12files.cpp:121-154) ----

/// FmWinOf(h): cari FmWin dari slot WINS. Borrow RefCell dilepas sebelum pointer dipakai.
///
/// Catatan aliasing: pemanggil yang sudah memegang `&mut FmWin` tidak boleh memanggil ini untuk
/// jendela yang sama selama callback berjalan. Lihat laporan (risiko aliasing).
pub(crate) fn fm_win_of(h: HWND) -> Option<&'static mut FmWin> {
    let found = {
        let slots = WINS.borrow();
        slots.iter().copied().find(|&p| !p.is_null() && unsafe { (*p).h } == h)
    };
    // SAFETY: pointer berasal dari Box::into_raw di fm_open dan dilepas di WM_NCDESTROY.
    found.map(|p| unsafe { &mut *p })
}

/// Pengganti pola inline `t->eb->GetCurrentView(IID_PPV_ARGS(&sv))` (win12files.cpp:291, 468, 496, 506).
/// Bukan fungsi C tersendiri; dipakai untuk memilih item setelah navigasi (/select,).
pub(crate) fn fm_current_view(t: &FmTab) -> Option<IShellView> {
    let eb = t.eb.as_ref()?;
    unsafe { eb.GetCurrentView::<IShellView>() }.ok()
}

/// FmActive: tab aktif bila ada.
pub(crate) fn fm_active(w: &mut FmWin) -> Option<&mut FmTab> {
    if w.tabs.n <= 0 || w.tabs.active < 0 {
        return None;
    }
    let i = w.tabs.active as usize;
    w.tab.get_mut(i)
}

/// Posisi tab dengan id tertentu (pengganti pencarian id di C).
fn tab_pos(w: &FmWin, id: i32) -> Option<usize> {
    let n = w.tabs.n.max(0) as usize;
    (0..n.min(W12FM_MAXTABS)).find(|&i| w.tabs.id[i] == id)
}

/// FmPos (win12files.cpp:124-129): indeks tab dengan id tertentu, -1 bila tidak ada.
pub(crate) fn fm_pos(w: &mut FmWin, tab_id: i32) -> i32 {
    match tab_pos(w, tab_id) {
        Some(i) => i as i32,
        None => -1,
    }
}

/// FmStatus (win12files.cpp:127-134): teks bilah status dan flag error.
pub(crate) fn fm_status(w: &mut FmWin, text: &str, error: bool) {
    copy_wide(&mut w.status, &crate::wide::to_wide(text));
    w.status_error = error;
    let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
}

/// FmFocus (win12files.cpp:136-140): pulihkan bila minimize, lalu jadikan depan.
pub(crate) fn fm_focus(w: &FmWin) {
    unsafe {
        if IsIconic(w.h).0 != 0 {
            let _ = ShowWindow(w.h, SW_RESTORE);
        }
        let _ = SetForegroundWindow(w.h);
    }
}

/// FmFocusView (win12files.cpp:493-500): UIActivate(SVUIA_ACTIVATE_FOCUS) pada tampilan tab aktif.
pub(crate) fn fm_focus_view(t: &mut FmTab) {
    if let Some(sv) = fm_current_view(t) {
        // SVUIA_STATUS berbasis i32; UIActivate menerima u32 (nilai 2 sama).
        let _ = unsafe { sv.UIActivate(SVUIA_ACTIVATE_FOCUS.0 as u32) };
    }
}

// ---- tata letak (win12files.cpp:157-194) ----

/// FmBodyRect (win12files.cpp:157-164): area isi tab di bawah bilah.
fn fm_body_rect(h: HWND, dpi: i32) -> RECT {
    let mut rc = rect0();
    let _ = unsafe { GetClientRect(h, &mut rc) };
    let top = px(FM_TABH_DIP + FM_NAVH_DIP + FM_CMDH_DIP, dpi);
    if top < rc.bottom {
        rc.top = top;
    } else {
        rc.top = rc.bottom;
    }
    rc
}

/// FmLayout (win12files.cpp:167-194): tata letak kotak alamat, pencarian, dan area tab.
pub(crate) fn fm_layout(w: &mut FmWin) {
    let dpi = dpi_of(w.h);
    let mut rc = rect0();
    let _ = unsafe { GetClientRect(w.h, &mut rc) };
    let right = rc.right;
    let nav_top = px(FM_TABH_DIP, dpi);
    let nav_h = px(FM_NAVH_DIP, dpi);
    let pad = px(4, dpi);
    let btns = px(40, dpi) * 4;
    let search_w = px(240, dpi);
    let search_x = right - px(12, dpi) - search_w;
    let addr_x = btns + pad * 2;
    let addr_w = (search_x - px(8, dpi) - addr_x).max(0);
    let edit_y = nav_top + pad;
    let edit_h = nav_h - pad * 2;
    unsafe {
        let _ = MoveWindow(w.addr, addr_x, edit_y, addr_w, edit_h, true);
        let _ = MoveWindow(w.search, search_x, edit_y, search_w, edit_h, true);
    }
    let body = fm_body_rect(w.h, dpi);
    let active = w.tabs.active;
    for (i, t) in w.tab.iter_mut().enumerate() {
        if let Some(eb) = &t.eb {
            let r = if i as i32 == active { body } else { rect0() };
            let _ = unsafe { eb.SetRect(None, r) };
        }
    }
}

// ---- judul, alamat, dan ikon (win12files.cpp:197-222) ----

/// FmRefreshChrome: alamat (edit), ikon kecil, dan judul jendela mengikuti tab aktif.
pub(crate) fn fm_refresh_chrome(w: &mut FmWin) {
    let h = w.h;
    let (cur, title) = match fm_active(w) {
        Some(t) => (t.cur, t.title),
        None => return,
    };
    if !cur.is_null() {
        // Alamat: SIGDN_DESKTOPABSOLUTEEDITING, lalu bebaskan memori Shell.
        if let Ok(name) = unsafe { SHGetNameFromIDList(cur, SIGDN_DESKTOPABSOLUTEEDITING) } {
            let mut b = unsafe { pwstr_vec(name.0) };
            b.push(0);
            let _ = unsafe { SetWindowTextW(w.addr, crate::wide::pcw(&b)) };
            unsafe { CoTaskMemFree(Some(name.0 as *const c_void)) };
        }
        // Ikon kecil folder: WM_SETICON mengembalikan ikon lama yang harus di-DestroyIcon.
        let mut sfi: SHFILEINFOW = unsafe { zeroed() }; // SAFETY: struct C POD; nol adalah nilai valid.
        let flags = SHGFI_FLAGS(SHGFI_PIDL.0 | SHGFI_ICON.0 | SHGFI_SMALLICON.0);
        unsafe {
            SHGetFileInfoW(
                PCWSTR(cur as *const u16),
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut sfi),
                size_of::<SHFILEINFOW>() as u32,
                flags,
            );
        }
        let prev = unsafe {
            SendMessageW(
                h,
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(sfi.hIcon.0 as isize)),
            )
        };
        if prev.0 != 0 {
            let _ = unsafe { DestroyIcon(HICON(prev.0 as *mut c_void)) };
        }
    }
    // Judul: "Win12 Files", atau "<judul tab> - Win12 Files".
    let mut buf = [0u16; 256];
    copy_wide(&mut buf, &wide_z("Win12 Files"));
    if title[0] != 0 {
        let mut t = [0u16; 128];
        copy_wide(&mut t, &title);
        copy_wide(&mut buf, &t);
        cat_wide(&mut buf, &wide_z(" - Win12 Files"));
    }
    let _ = unsafe { SetWindowTextW(h, crate::wide::pcw(&buf)) };
    fm_update_cue(w);
    fm_refresh_selection(w);
    let _ = unsafe { InvalidateRect(Some(h), None, false) };
}

// ---- IExplorerBrowserEvents (callback navigasi dan tampilan per tab) ----

/// Penerima event satu tab. Menyimpan HWND sebagai isize dan id tab (bukan pointer ke FmWin).
#[windows::core::implement(IExplorerBrowserEvents)]
struct FmEvents {
    hwnd: isize,
    id: i32,
}

impl IExplorerBrowserEvents_Impl for FmEvents_Impl {
    fn OnNavigationPending(&self, _pidlfolder: *const ITEMIDLIST) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnViewCreated(&self, psv: Ref<'_, IShellView>) -> windows::core::Result<()> {
        if let Some(sv) = psv.as_ref() {
            let _ = unsafe { sv.UIActivate(SVUIA_ACTIVATE_FOCUS.0 as u32) };
        }
        Ok(())
    }

    fn OnNavigationComplete(&self, pidlfolder: *const ITEMIDLIST) -> windows::core::Result<()> {
        if let Some(w) = fm_win_of(HWND(self.hwnd as *mut c_void)) {
            fm_tab_navigated(w, self.id, pidlfolder);
        }
        Ok(())
    }

    fn OnNavigationFailed(&self, _pidlfolder: *const ITEMIDLIST) -> windows::core::Result<()> {
        if let Some(w) = fm_win_of(HWND(self.hwnd as *mut c_void)) {
            fm_tab_navigation_failed(w, self.id);
        }
        Ok(())
    }
}

// ---- tab (win12files.cpp: FmTabInit, FmTabDestroy, FmBrowse, FmNavigateTo) ----

/// Tab kosong (belum ada IExplorerBrowser). Kolom POD dinolkan, lalu kolom khusus diatur.
pub(crate) fn fm_tab_empty() -> FmTab {
    // SAFETY: W12Hist dan W12FmSort adalah POD (angka tanpa pointer); nilai nol valid.
    let mut hist: W12Hist = unsafe { zeroed() };
    hist.cur = -1;
    // SAFETY: lihat di atas.
    let mut sort: W12FmSort = unsafe { zeroed() };
    sort.col = -1;
    FmTab {
        eb: None,
        cookie: 0,
        title: [0u16; 128],
        cur: null_mut(),
        pending: null_mut(),
        hist,
        hpidl: [null_mut(); FM_HISTMAX],
        hist_pending: false,
        hist_from: 0,
        sort,
        searching: false,
    }
}

/// FmTabInit: buat IExplorerBrowser untuk tab pada posisi `pos`, lalu pasang event-nya.
pub(crate) fn fm_tab_init(w: &mut FmWin, pos: usize) -> bool {
    if pos >= W12FM_MAXTABS {
        return false;
    }
    let eb: IExplorerBrowser = match unsafe {
        CoCreateInstance(&ExplorerBrowser, None::<&windows::core::IUnknown>, CLSCTX_INPROC_SERVER)
    } {
        Ok(e) => e,
        Err(_) => return false,
    };
    let body = fm_body_rect(w.h, dpi_of(w.h));
    let fs = FOLDERSETTINGS {
        ViewMode: FVM_DETAILS.0 as u32,
        fFlags: 0,
    };
    if unsafe { eb.Initialize(w.h, &body, Some(&fs as *const FOLDERSETTINGS)) }.is_err() {
        return false;
    }
    let _ = unsafe {
        eb.SetOptions(EXPLORER_BROWSER_OPTIONS(
            EBO_SHOWFRAMES.0 | EBO_NOBORDER.0 | EBO_ALWAYSNAVIGATE.0,
        ))
    };
    let id = w.tabs.id[pos];
    let events: IExplorerBrowserEvents = FmEvents {
        hwnd: w.h.0 as isize,
        id,
    }
    .into();
    // windows 0.61: Advise(psink) mengembalikan cookie sebagai Result<u32> (pdwCookie menjadi nilai kembali).
    let cookie: u32 = unsafe { eb.Advise(&events) }.unwrap_or(0);
    let t = &mut w.tab[pos];
    t.eb = Some(eb);
    t.cookie = cookie;
    true
}

/// FmTabDestroy: Unadvise dan Destroy IExplorerBrowser, bebaskan PIDL, lalu kosongkan tab.
pub(crate) fn fm_tab_destroy(t: &mut FmTab) {
    if let Some(eb) = t.eb.take() {
        if t.cookie != 0 {
            let _ = unsafe { eb.Unadvise(t.cookie) };
        }
        let _ = unsafe { eb.Destroy() };
        // eb dilepas di akhir blok (drop = pengganti Release).
    }
    free_pidl(t.cur);
    free_pidl(t.pending);
    for p in t.hpidl.iter_mut() {
        free_pidl(*p);
        *p = null_mut();
    }
    *t = fm_tab_empty();
}

/// FmBrowse: arahkan tab ke PIDL (BrowseToIDList). False bila tab belum punya eb atau Windows menolak.
pub(crate) fn fm_browse(t: &mut FmTab, pidl: *const ITEMIDLIST) -> bool {
    t.hist_pending = false;
    let Some(eb) = t.eb.clone() else {
        return false;
    };
    unsafe { eb.BrowseToIDList(pidl, SBSP_ABSOLUTE).is_ok() }
}

/// Dipanggil dari OnNavigationComplete: sinkronkan PIDL, riwayat, judul, dan item pilihan tab.
fn fm_tab_navigated(w: &mut FmWin, id: i32, pidl: *const ITEMIDLIST) {
    let Some(pos) = tab_pos(w, id) else {
        return;
    };
    let active = w.tabs.active == pos as i32;
    let copy = unsafe { ILClone(pidl) };
    {
        let t = &mut w.tab[pos];
        free_pidl(t.cur);
        t.cur = copy;
        t.searching = false;
        if t.hist_pending {
            t.hist_pending = false;
        } else {
            fm_hist_record(t, pidl);
        }
        // Judul tab: nama tampilan folder (SIGDN_NORMALDISPLAY).
        t.title = [0u16; 128];
        if let Ok(name) = unsafe { SHGetNameFromIDList(pidl, SIGDN_NORMALDISPLAY) } {
            let mut b = unsafe { pwstr_vec(name.0) };
            b.push(0);
            copy_wide(&mut t.title, &b);
            unsafe { CoTaskMemFree(Some(name.0 as *const c_void)) };
        }
        // Item yang dipilih setelah naik ke induk (/select,).
        if !t.pending.is_null() {
            if let Some(sv) = fm_current_view(t) {
                let flags = (SVSI_SELECT.0 | SVSI_ENSUREVISIBLE.0 | SVSI_FOCUSED.0) as u32;
                let _ = unsafe { sv.SelectItem(t.pending, flags) };
            }
            free_pidl(t.pending);
            t.pending = null_mut();
        }
    }
    if active {
        fm_apply_sort(w);
        fm_refresh_chrome(w);
    } else {
        let _ = unsafe { InvalidateRect(Some(w.h), None, false) };
    }
}

/// Dipanggil dari OnNavigationFailed: pulihkan indeks riwayat bila navigasi riwayat gagal, lalu lapor ke bilah status.
pub(crate) fn fm_tab_navigation_failed(w: &mut FmWin, id: i32) {
    let Some(pos) = tab_pos(w, id) else {
        return;
    };
    let active = w.tabs.active == pos as i32;
    {
        let t = &mut w.tab[pos];
        if t.hist_pending {
            t.hist_pending = false;
            t.hist.cur = t.hist_from;
        }
        free_pidl(t.pending);
        t.pending = null_mut();
    }
    if active {
        fm_status(w, "Windows tidak dapat membuka lokasi itu", true);
    }
}

/// FmNavigateTo: terjemahkan teks alamat ke PIDL lalu buka di tab aktif.
/// `select` = pilih item terakhir setelah naik ke induk (/select,).
pub(crate) fn fm_navigate_to(w: &mut FmWin, text: &str, select: bool) -> bool {
    let mut norm = [0u16; FM_SEARCH_PATH];
    let _ = w12_fm_normalize_address(Some(text), &mut norm);
    // C: ExpandEnvironmentStringsW(norm, expanded, 1024) sebelum SHParseDisplayName (win12files.cpp:328).
    // Hasil kosong (gagal) membuat parse di bawah gagal, sama seperti C.
    let mut expanded = [0u16; FM_SEARCH_PATH];
    let _ = unsafe {
        windows::Win32::System::Environment::ExpandEnvironmentStringsW(
            crate::wide::pcw(&norm),
            Some(&mut expanded[..]),
        )
    };
    let shown = String::from_utf16_lossy(&norm[..wlen(&norm)]);
    let mut pidl: *mut ITEMIDLIST = null_mut();
    let parsed = unsafe {
        windows::Win32::UI::Shell::SHParseDisplayName(
            crate::wide::pcw(&expanded),
            None::<&IBindCtx>,
            &mut pidl,
            0,
            None,
        )
        .is_ok()
    };
    if !parsed || pidl.is_null() {
        fm_status(w, &format!("Windows tidak dapat menemukan '{}'", shown), true);
        return false;
    }
    if let Some(t) = fm_active(w) {
        free_pidl(t.pending);
        t.pending = null_mut();
    }
    let ok = if select {
        // /select,: buka induk, lalu pilih item terakhir setelah navigasi selesai.
        let parent = unsafe { ILClone(pidl) };
        let _ = unsafe { ILRemoveLastID(Some(parent)) };
        let child = unsafe { ILClone(ILFindLastID(pidl)) };
        let res = match fm_active(w) {
            Some(t) => {
                t.pending = child;
                fm_browse(t, parent)
            }
            None => {
                free_pidl(child);
                false
            }
        };
        free_pidl(parent);
        res
    } else {
        match fm_active(w) {
            Some(t) => fm_browse(t, pidl),
            None => false,
        }
    };
    free_pidl(pidl);
    if !ok {
        if let Some(t) = fm_active(w) {
            free_pidl(t.pending);
            t.pending = null_mut();
        }
        fm_status(w, "Windows tidak dapat membuka lokasi itu", true);
        return false;
    }
    fm_status(w, "", false);
    true
}

/// FmNavigate: buka teks alamat tanpa pemilihan item.
pub(crate) fn fm_navigate(w: &mut FmWin, text: &str) -> bool {
    fm_navigate_to(w, text, false)
}
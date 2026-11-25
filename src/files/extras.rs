//! Tambahan win12files 2026-11-19 (permintaan pengguna: "sempurnakan, mirip 100% Explorer, tambah fitur yang tidak
//! dimiliki Windows"):
//! - Menu "Baru" asli Windows (CLSID_NewMenu: Folder, Pintasan, Dokumen Teks, ...), lengkap dengan ikon dan mode ganti nama.
//! - Dropdown subfolder dari chevron breadcrumb (seperti Explorer).
//! - Panel Pratinjau (Alt+P) dan panel Detail (Alt+Shift+P) lewat IExplorerPaneVisibility.
//! - Menu "Lainnya" yang diperluas (pilih semua/batal/balik, salin jalur, buka di Terminal, opsi folder).
//! - Item tersembunyi dan ekstensi nama file dari menu Tampilan (pengaturan Explorer yang sama).
//! - Panel terminal F4 (term.rs; bukan fitur Explorer) dan pembatas yang bisa diseret.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{Interface, GUID, PCSTR, PCWSTR};
use windows::Win32::Foundation::{E_NOINTERFACE, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, DeleteObject, InvalidateRect, HBITMAP, HGDIOBJ};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, IBindCtx, IDataObject, IServiceProvider, IServiceProvider_Impl, CLSCTX_INPROC_SERVER};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::IObjectWithSite;
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
    KEY_SET_VALUE, REG_DWORD,
};
use windows::Win32::System::SystemServices::{SFGAO_FOLDER, SFGAO_HIDDEN, SFGAO_STREAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IContextMenu, IContextMenu2, IContextMenu3, IEnumShellItems, IExplorerBrowser, IExplorerPaneVisibility,
    IExplorerPaneVisibility_Impl, IFolderView2, IShellExtInit, IShellItem, IShellItemArray, IShellItemImageFactory,
    SHCreateItemFromIDList, SHGetIDListFromObject, SHGetPathFromIDListW, ShellExecuteW, BHID_EnumItems,
    CLSID_NewMenu, CMF_NORMAL, CMINVOKECOMMANDINFO, EPS_DEFAULT_OFF, EPS_DEFAULT_ON, EPS_DONTCARE, EPS_FORCE,
    EP_DetailsPane, EP_PreviewPane, SBSP_ABSOLUTE, SIGDN_FILESYSPATH, SIGDN_NORMALDISPLAY, SIIGBF_ICONONLY,
    SVGIO_ALLVIEW, SVSI_DESELECT, SVSI_SELECT, SHELLSTATEA, SHGetSetSettings, SSF_AUTOCHECKSELECT, SSF_MASK,
    SSF_SHOWALLOBJECTS, SSF_SHOWEXTENSIONS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, DestroyWindow, GetSubMenu, MoveWindow, SendMessageTimeoutW,
    SetMenuItemInfoW, ShowWindow, TrackPopupMenu, HMENU, HWND_BROADCAST, MENUITEMINFOW, MF_CHECKED, MF_GRAYED,
    MF_SEPARATOR, MF_STRING, MIIM_BITMAP, SMTO_ABORTIFHUNG, SW_HIDE, SW_SHOW, SW_SHOWNORMAL, TPM_LEFTALIGN,
    TPM_RETURNCMD, TPM_TOPALIGN, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_SETTINGCHANGE,
};

use crate::files::app::{fm_active, fm_browse, fm_current_view, fm_focus_view, fm_layout, fm_status, free_pidl};
use crate::files::state::{FmWin, FM_TERMH_DIP};
use crate::files::{chrome, term};
use crate::wide::{pcw, to_wide_z};

// ---- meneruskan pesan menu ke IContextMenu2/3 (ikon dan isi submenu "Baru") ----

thread_local! {
    static ACTIVE_MENU: RefCell<Option<IContextMenu>> = const { RefCell::new(None) };
}

/// Dipanggil wndproc untuk WM_INITMENUPOPUP/DRAWITEM/MEASUREITEM/MENUCHAR selama menu shell terbuka.
pub(crate) fn menu_msg(m: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    if !matches!(m, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
        return None;
    }
    let cm = ACTIVE_MENU.with(|c| c.borrow().clone())?;
    if let Ok(cm3) = cm.cast::<IContextMenu3>() {
        let mut r = LRESULT(0);
        if unsafe { cm3.HandleMenuMsg2(m, wp, lp, Some(&mut r)) }.is_ok() {
            return Some(r);
        }
        return None;
    }
    if m == WM_MENUCHAR {
        return None;
    }
    if let Ok(cm2) = cm.cast::<IContextMenu2>() {
        if unsafe { cm2.HandleMenuMsg(m, wp, lp) }.is_ok() {
            return Some(LRESULT(if m == WM_INITMENUPOPUP { 0 } else { 1 }));
        }
    }
    None
}

fn invoke(cm: &IContextMenu, id: u32, h: HWND) -> bool {
    let mut ici: CMINVOKECOMMANDINFO = unsafe { std::mem::zeroed() };
    ici.cbSize = std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32;
    ici.hwnd = h;
    ici.lpVerb = PCSTR(id as usize as *const u8);
    ici.nShow = SW_SHOWNORMAL.0;
    unsafe { cm.InvokeCommand(&ici) }.is_ok()
}

fn screen_pt(h: HWND, x: i32, y: i32) -> POINT {
    let mut pt = POINT { x, y };
    let _ = unsafe { ClientToScreen(h, &mut pt) };
    pt
}

// ---- menu "Baru" ----

/// Menu "Baru" seperti Explorer: isi dari CLSID_NewMenu (ShellNew di registri). Cadangan: buat folder baru.
pub(crate) fn new_menu(w: &mut FmWin, anchor: RECT) {
    let Some(t) = fm_active(w) else { return };
    let cur = t.cur;
    let view = fm_current_view(t);
    if cur.is_null() {
        return;
    }
    let cm: Option<IContextMenu> =
        unsafe { CoCreateInstance(&CLSID_NewMenu, None::<&windows::core::IUnknown>, CLSCTX_INPROC_SERVER) }.ok();
    let Some(cm) = cm else {
        crate::files::bar::fm_run_verb(w, true, &crate::wide::to_wide("NewFolder"));
        return;
    };
    if let Ok(init) = cm.cast::<IShellExtInit>() {
        let _ = unsafe { init.Initialize(Some(cur as *const ITEMIDLIST), None::<&IDataObject>, None) };
    }
    // Situs = tampilan folder: item baru langsung dipilih dan masuk mode ganti nama, seperti Explorer.
    let site = cm.cast::<IObjectWithSite>().ok();
    if let (Some(s), Some(v)) = (site.as_ref(), view.as_ref()) {
        let _ = unsafe { s.SetSite(v) };
    }
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else { return };
    let ok = unsafe { cm.QueryContextMenu(menu, 0, 1, 0x7FFF, CMF_NORMAL) }.is_ok();
    let sub = unsafe { GetSubMenu(menu, 0) };
    let target = if !ok {
        None
    } else if sub.is_invalid() {
        Some(menu)
    } else {
        Some(sub)
    };
    let mut done = false;
    if let Some(target) = target {
        ACTIVE_MENU.with(|c| *c.borrow_mut() = Some(cm.clone()));
        let pt = screen_pt(w.h, anchor.left, anchor.bottom);
        let id = unsafe {
            TrackPopupMenu(target, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, None, w.h, None)
        }
        .0;
        ACTIVE_MENU.with(|c| *c.borrow_mut() = None);
        if id > 0 {
            done = invoke(&cm, (id - 1) as u32, w.h);
            if !done {
                fm_status(w, "Item baru tidak dapat dibuat", true);
            }
        } else {
            done = true;
        }
    }
    let _ = unsafe { DestroyMenu(menu) };
    if let Some(s) = site {
        let _ = unsafe { s.SetSite(None::<&windows::core::IUnknown>) };
    }
    if !done && target.is_none() {
        crate::files::bar::fm_run_verb(w, true, &crate::wide::to_wide("NewFolder"));
    }
}

// ---- dropdown chevron breadcrumb ----

/// PIDL segmen breadcrumb `k` (-1 = akar Desktop). Pemanggil membebaskan dengan free_pidl.
fn crumb_pidl(w: &mut FmWin, k: i32) -> *mut ITEMIDLIST {
    let n = w.crumbs.len() as i32;
    let Some(t) = fm_active(w) else { return std::ptr::null_mut() };
    if t.cur.is_null() {
        return std::ptr::null_mut();
    }
    let p = unsafe { windows::Win32::UI::Shell::ILClone(t.cur) };
    if p.is_null() {
        return p;
    }
    for _ in 0..(n - 1 - k).max(0) {
        if !unsafe { windows::Win32::UI::Shell::ILRemoveLastID(Some(p)) }.as_bool() {
            break;
        }
    }
    p
}

fn item_name(it: &IShellItem) -> Vec<u16> {
    match unsafe { it.GetDisplayName(SIGDN_NORMALDISPLAY) } {
        Ok(s) => {
            let v = unsafe { s.as_wide() }.to_vec();
            unsafe { CoTaskMemFree(Some(s.0 as *const c_void)) };
            v
        }
        Err(_) => Vec::new(),
    }
}

fn item_icon(it: &IShellItem, px: i32) -> HBITMAP {
    match it.cast::<IShellItemImageFactory>() {
        Ok(f) => unsafe { f.GetImage(SIZE { cx: px, cy: px }, SIIGBF_ICONONLY) }.unwrap_or_default(),
        Err(_) => HBITMAP::default(),
    }
}

/// Klik chevron setelah segmen `k`: daftar subfolder segmen itu (maks. 64), pilih untuk membuka.
pub(crate) fn chevron_menu(w: &mut FmWin, k: i32, anchor: RECT) {
    let parent = crumb_pidl(w, k);
    if parent.is_null() {
        return;
    }
    let item: Option<IShellItem> = unsafe { SHCreateItemFromIDList(parent) }.ok();
    free_pidl(parent);
    let Some(item) = item else { return };
    let Ok(en) = (unsafe { item.BindToHandler::<_, IEnumShellItems>(None::<&IBindCtx>, &BHID_EnumItems) }) else {
        return;
    };
    let mut kids: Vec<(Vec<u16>, IShellItem)> = Vec::new();
    loop {
        let mut one = [None];
        let mut got = 0u32;
        if unsafe { en.Next(&mut one, Some(&mut got)) }.is_err() || got == 0 {
            break;
        }
        let Some(c) = one[0].take() else { break };
        let mask = SFGAO_FOLDER | SFGAO_HIDDEN | SFGAO_STREAM;
        let a = unsafe { c.GetAttributes(mask) }.unwrap_or_default();
        // Folder saja; berkas zip (folder + stream) dan item tersembunyi dilewati, seperti dropdown Explorer.
        if a.0 & SFGAO_FOLDER.0 == 0 || a.0 & SFGAO_STREAM.0 != 0 || a.0 & SFGAO_HIDDEN.0 != 0 {
            continue;
        }
        kids.push((item_name(&c), c));
        if kids.len() >= 64 {
            break;
        }
    }
    if kids.is_empty() {
        return;
    }
    // Folder biasa diurutkan seperti Explorer (tanpa beda huruf besar/kecil); akar Desktop mempertahankan urutan shell.
    if k >= 0 {
        kids.sort_by_key(|(n, _)| String::from_utf16_lossy(n).to_lowercase());
    }
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else { return };
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(w.h) } as i32;
    let icon_px = 16 * dpi.max(96) / 96;
    let mut bmps: Vec<HBITMAP> = Vec::new();
    for (i, (name, it)) in kids.iter().enumerate() {
        let mut s = name.clone();
        s.push(0);
        let _ = unsafe { AppendMenuW(menu, MF_STRING, i + 1, PCWSTR(s.as_ptr())) };
        let bmp = item_icon(it, icon_px);
        if !bmp.is_invalid() {
            let mii = MENUITEMINFOW {
                cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_BITMAP,
                hbmpItem: bmp,
                ..Default::default()
            };
            let _ = unsafe { SetMenuItemInfoW(menu, (i + 1) as u32, false, &mii) };
            bmps.push(bmp);
        }
    }
    let pt = screen_pt(w.h, anchor.left, anchor.bottom);
    let id = unsafe { TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, None, w.h, None) }.0;
    let _ = unsafe { DestroyMenu(menu) };
    for b in bmps {
        let _ = unsafe { DeleteObject(HGDIOBJ(b.0)) };
    }
    if id > 0 {
        if let Some((_, it)) = kids.get((id - 1) as usize) {
            if let Ok(p) = unsafe { SHGetIDListFromObject(it) } {
                if let Some(t) = fm_active(w) {
                    let _ = fm_browse(t, p);
                }
                free_pidl(p);
            }
        }
    }
}

// ---- panel Pratinjau / Detail ----

static PANE_PREVIEW: AtomicBool = AtomicBool::new(false);
static PANE_DETAILS: AtomicBool = AtomicBool::new(false);

#[windows::core::implement(IExplorerPaneVisibility)]
struct FmPanes;

impl IExplorerPaneVisibility_Impl for FmPanes_Impl {
    fn GetPaneState(&self, ep: *const GUID) -> windows::core::Result<u32> {
        let ep = unsafe { ep.as_ref() }.copied().unwrap_or_default();
        // Catat sekali per GUID: memastikan panel mana yang benar-benar ditanyakan DefView (2026-11-19, status dobel).
        {
            static SEEN: std::sync::Mutex<Vec<u128>> = std::sync::Mutex::new(Vec::new());
            if let Ok(mut s) = SEEN.lock() {
                if !s.contains(&ep.to_u128()) && s.len() < 32 {
                    s.push(ep.to_u128());
                    crate::log::log(&format!("files: panel ditanya {{{:?}}}", ep));
                }
            }
        }
        let on = if ep == EP_PreviewPane {
            Some(PANE_PREVIEW.load(Ordering::Relaxed))
        } else if ep == EP_DetailsPane {
            Some(PANE_DETAILS.load(Ordering::Relaxed))
        } else if ep == EP_STATUS_BAR || ep == EP_COMMANDS || ep == EP_COMMANDS_ORGANIZE || ep == EP_COMMANDS_VIEW {
            // Bilah status sendiri sudah menampilkan "N item, M dipilih"; bilah bawaan membuatnya dobel.
            // Bilah perintah lama (Organise / Include in library / New folder) menggandakan bilah alat sendiri.
            Some(false)
        } else {
            None
        };
        Ok(match on {
            Some(true) => (EPS_DEFAULT_ON.0 | EPS_FORCE.0) as u32,
            Some(false) => (EPS_DEFAULT_OFF.0 | EPS_FORCE.0) as u32,
            None => EPS_DONTCARE.0 as u32,
        })
    }
}

/// EP_StatusBar (shlguid.h).
const EP_STATUS_BAR: GUID = GUID::from_u128(0x65bcde4f_4f07_4f27_83a7_1afca4df7ddd);
/// EP_Commands, EP_Commands_Organize, EP_Commands_View (shlguid.h): bilah perintah gaya Windows 7.
const EP_COMMANDS: GUID = GUID::from_u128(0xd9745868_ca5f_4a76_91cd_f5a129fbb076);
const EP_COMMANDS_ORGANIZE: GUID = GUID::from_u128(0x72e81700_e3ec_4660_bf24_3c3b7b648806);
const EP_COMMANDS_VIEW: GUID = GUID::from_u128(0x21f7c32d_eeaa_439b_bb51_37b96fd6a943);

/// SID_ExplorerPaneVisibility (shlguid.h); tidak ada di crate windows.
const SID_EXPLORER_PANE_VISIBILITY: GUID = GUID::from_u128(0xe07010ec_bc17_44c0_97b0_46c7c95b9edc);

#[windows::core::implement(IServiceProvider)]
struct FmSite {
    vis: IExplorerPaneVisibility,
}

impl IServiceProvider_Impl for FmSite_Impl {
    fn QueryService(&self, sid: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> windows::core::Result<()> {
        if ppv.is_null() {
            return Err(E_NOINTERFACE.into());
        }
        unsafe { *ppv = std::ptr::null_mut() };
        if unsafe { sid.as_ref() } == Some(&SID_EXPLORER_PANE_VISIBILITY) {
            return unsafe { self.vis.query(riid, ppv) }.ok();
        }
        Err(E_NOINTERFACE.into())
    }
}

/// Pasang situs panel ke IExplorerBrowser (sebelum Initialize).
pub(crate) fn panes_attach(eb: &IExplorerBrowser) {
    let vis: IExplorerPaneVisibility = FmPanes.into();
    let site: IServiceProvider = FmSite { vis }.into();
    if let Ok(ows) = eb.cast::<IObjectWithSite>() {
        let _ = unsafe { ows.SetSite(&site) };
    }
}

/// Lepas situs (sebelum Destroy) agar objek situs ikut dilepas.
pub(crate) fn panes_detach(eb: &IExplorerBrowser) {
    if let Ok(ows) = eb.cast::<IObjectWithSite>() {
        let _ = unsafe { ows.SetSite(None::<&windows::core::IUnknown>) };
    }
}

pub(crate) fn pane_on(details: bool) -> bool {
    if details {
        PANE_DETAILS.load(Ordering::Relaxed)
    } else {
        PANE_PREVIEW.load(Ordering::Relaxed)
    }
}

/// Alt+P / Alt+Shift+P: panel pratinjau dan detail saling menggantikan, seperti Explorer.
pub(crate) fn pane_toggle(w: &mut FmWin, details: bool) {
    let (me, other) = if details { (&PANE_DETAILS, &PANE_PREVIEW) } else { (&PANE_PREVIEW, &PANE_DETAILS) };
    let on = !me.load(Ordering::Relaxed);
    me.store(on, Ordering::Relaxed);
    if on {
        other.store(false, Ordering::Relaxed);
    }
    pane_settings_save();
    // ExplorerBrowser membaca status panel saat navigasi: muat ulang folder aktif tanpa menambah riwayat.
    if let Some(t) = fm_active(w) {
        if let (Some(eb), false) = (t.eb.clone(), t.cur.is_null()) {
            t.hist_from = t.hist.cur;
            t.hist_pending = true;
            if unsafe { eb.BrowseToIDList(t.cur, SBSP_ABSOLUTE) }.is_err() {
                t.hist_pending = false;
            }
        }
    }
}

const PANE_KEY: &str = "Software\\Win12Shell\\Files";

/// Muat status panel tersimpan (HKCU\Software\Win12Shell\Files: Preview, Details).
pub(crate) fn pane_settings_load() {
    PANE_PREVIEW.store(reg_dword(HKEY_CURRENT_USER, PANE_KEY, "Preview").unwrap_or(0) == 1, Ordering::Relaxed);
    PANE_DETAILS.store(reg_dword(HKEY_CURRENT_USER, PANE_KEY, "Details").unwrap_or(0) == 1, Ordering::Relaxed);
}

fn pane_settings_save() {
    let key = to_wide_z(PANE_KEY);
    let mut hk = HKEY::default();
    let r = unsafe {
        windows::Win32::System::Registry::RegCreateKeyExW(
            HKEY_CURRENT_USER,
            pcw(&key),
            None,
            PCWSTR::null(),
            windows::Win32::System::Registry::REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut hk,
            None,
        )
    };
    if r.is_err() {
        return;
    }
    for (name, v) in [("Preview", &PANE_PREVIEW), ("Details", &PANE_DETAILS)] {
        let n = to_wide_z(name);
        let d: u32 = v.load(Ordering::Relaxed) as u32;
        let _ = unsafe { RegSetValueExW(hk, pcw(&n), None, REG_DWORD, Some(&d.to_le_bytes())) };
    }
    let _ = unsafe { RegCloseKey(hk) };
}

fn reg_dword(root: HKEY, path: &str, name: &str) -> Option<u32> {
    let key = to_wide_z(path);
    let n = to_wide_z(name);
    let mut hk = HKEY::default();
    if unsafe { RegOpenKeyExW(root, pcw(&key), None, KEY_QUERY_VALUE, &mut hk) }.is_err() {
        return None;
    }
    let mut buf = [0u8; 4];
    let mut len = 4u32;
    let r = unsafe { RegQueryValueExW(hk, pcw(&n), None, None, Some(buf.as_mut_ptr()), Some(&mut len)) };
    let _ = unsafe { RegCloseKey(hk) };
    if r.is_err() {
        return None;
    }
    Some(u32::from_le_bytes(buf))
}

// ---- item tersembunyi / ekstensi nama file (pengaturan Explorer yang sama) ----

const ADV_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";

/// Pengaturan Explorer\Advanced: (nama nilai, nilai saat aktif, nilai saat mati, bawaan).
pub(crate) const ADV_HIDDEN: usize = 0;
pub(crate) const ADV_EXT: usize = 1;
pub(crate) const ADV_CHECK: usize = 2;
pub(crate) const ADV_COMPACT: usize = 3;
const ADV: [(&str, u32, u32, u32); 4] =
    [("Hidden", 1, 2, 2), ("HideFileExt", 0, 1, 1), ("AutoCheckSelect", 1, 0, 0), ("UseCompactMode", 1, 0, 0)];

/// Bit SHELLSTATE untuk pengaturan yang punya SSF_*: (mask, bitfield 1 atau 2, bit). Ringkas (UseCompactMode) tidak punya.
fn adv_ssf(which: usize) -> Option<(SSF_MASK, bool, i32)> {
    match which {
        ADV_HIDDEN => Some((SSF_SHOWALLOBJECTS, true, 1 << 0)),
        ADV_EXT => Some((SSF_SHOWEXTENSIONS, true, 1 << 1)),
        ADV_CHECK => Some((SSF_AUTOCHECKSELECT, false, 1 << 3)),
        _ => None,
    }
}

pub(crate) fn adv_on(which: usize) -> bool {
    if let Some((mask, first, bit)) = adv_ssf(which) {
        let mut ss = SHELLSTATEA::default();
        unsafe { SHGetSetSettings(Some(&mut ss), mask, false) };
        let v = if first { ss._bitfield1 } else { ss._bitfield2 };
        return v & bit != 0;
    }
    let (name, on, _, def) = ADV[which.min(ADV.len() - 1)];
    reg_dword(HKEY_CURRENT_USER, ADV_KEY, name).unwrap_or(def) == on
}

/// Ubah pengaturan tampilan Explorer (item tersembunyi, ekstensi, kotak centang, ringkas) seperti menu Tampilan Explorer.
/// Yang punya SSF_* lewat SHGetSetSettings (registri + cache shell + siaran, sama seperti Explorer).
pub(crate) fn toggle_adv(w: &mut FmWin, which: usize) {
    let which = which.min(ADV.len() - 1);
    if let Some((mask, first, bit)) = adv_ssf(which) {
        let mut ss = SHELLSTATEA::default();
        unsafe { SHGetSetSettings(Some(&mut ss), mask, false) };
        if first {
            ss._bitfield1 ^= bit;
        } else {
            ss._bitfield2 ^= bit;
        }
        unsafe { SHGetSetSettings(Some(&mut ss), mask, true) };
        if let Some(sv) = fm_active(w).and_then(|t| fm_current_view(t)) {
            let _ = unsafe { sv.Refresh() };
        }
        return;
    }
    let (name, on, off, _) = ADV[which];
    let val = if adv_on(which) { off } else { on };
    let key = to_wide_z(ADV_KEY);
    let mut hk = HKEY::default();
    if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, pcw(&key), None, KEY_SET_VALUE, &mut hk) }.is_err() {
        fm_status(w, "Pengaturan tidak dapat diubah", true);
        return;
    }
    let n = to_wide_z(name);
    let _ = unsafe { RegSetValueExW(hk, pcw(&n), None, REG_DWORD, Some(&val.to_le_bytes())) };
    let _ = unsafe { RegCloseKey(hk) };
    // Explorer menyiarkan "ShellState"; tampilan folder (juga milik win12files) membaca ulang pengaturan.
    let s = to_wide_z("ShellState");
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(s.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            200,
            None,
        );
    }
    if let Some(sv) = fm_active(w).and_then(|t| fm_current_view(t)) {
        let _ = unsafe { sv.Refresh() };
    }
}

// ---- menu "Lainnya" ----

fn folder_view(w: &mut FmWin) -> Option<IFolderView2> {
    fm_active(w).and_then(|t| fm_current_view(t)).and_then(|v| v.cast::<IFolderView2>().ok())
}

fn select_all(w: &mut FmWin, mode: u8) {
    let Some(fv) = folder_view(w) else { return };
    let n = unsafe { fv.ItemCount(SVGIO_ALLVIEW) }.unwrap_or(0);
    for i in 0..n {
        let flag = match mode {
            0 => SVSI_SELECT.0,
            1 => SVSI_DESELECT.0,
            _ => {
                // Balik: cek status pilihan per item (PIDL anak relatif dari IFolderView::Item).
                let sel = match unsafe { fv.Item(i) } {
                    Ok(p) => {
                        let s = unsafe { fv.GetSelectionState(p) }.unwrap_or(0);
                        free_pidl(p);
                        s & SVSI_SELECT.0 as u32 != 0
                    }
                    Err(_) => false,
                };
                if sel {
                    SVSI_DESELECT.0
                } else {
                    SVSI_SELECT.0
                }
            }
        };
        let _ = unsafe { fv.SelectItem(i, flag as u32) };
    }
    crate::files::bar::fm_refresh_selection(w);
}

/// Jalur folder aktif (sistem berkas); kosong bila bukan folder berkas (Beranda, This PC).
pub(crate) fn cur_path(w: &mut FmWin) -> String {
    let Some(t) = fm_active(w) else { return String::new() };
    if t.cur.is_null() {
        return String::new();
    }
    let mut buf = [0u16; 260];
    if unsafe { SHGetPathFromIDListW(t.cur, &mut buf) }.as_bool() {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
        String::from_utf16_lossy(&buf[..n])
    } else {
        String::new()
    }
}

fn clip_set(h: HWND, s: &str) -> bool {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    unsafe {
        if OpenClipboard(Some(h)).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let mut ok = false;
        if let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, v.len() * 2) {
            let p = GlobalLock(hg) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(v.as_ptr(), p, v.len());
                let _ = GlobalUnlock(hg);
                ok = SetClipboardData(13, Some(HANDLE(hg.0))).is_ok();
            }
        }
        let _ = CloseClipboard();
        ok
    }
}

/// Ctrl+Shift+C: salin jalur item terpilih (berkutip, satu per baris) atau folder aktif, seperti "Salin sebagai jalur".
pub(crate) fn copy_path(w: &mut FmWin) {
    let mut out: Vec<String> = Vec::new();
    if let Some(fv) = folder_view(w) {
        if let Ok(arr) = unsafe { fv.GetSelection(false) } {
            let arr: IShellItemArray = arr;
            let n = unsafe { arr.GetCount() }.unwrap_or(0);
            for i in 0..n {
                if let Ok(it) = unsafe { arr.GetItemAt(i) } {
                    if let Ok(s) = unsafe { it.GetDisplayName(SIGDN_FILESYSPATH) } {
                        out.push(format!("\"{}\"", String::from_utf16_lossy(unsafe { s.as_wide() })));
                        unsafe { CoTaskMemFree(Some(s.0 as *const c_void)) };
                    }
                }
            }
        }
    }
    if out.is_empty() {
        let p = cur_path(w);
        if !p.is_empty() {
            out.push(format!("\"{}\"", p));
        }
    }
    if out.is_empty() {
        fm_status(w, "Lokasi ini tidak punya jalur berkas", true);
        return;
    }
    let n = out.len();
    if clip_set(w.h, &out.join("\r\n")) {
        fm_status(w, &if n == 1 { "Jalur disalin".to_string() } else { format!("{} jalur disalin", n) }, false);
    }
}

/// "Buka di Terminal": Windows Terminal bila ada, selain itu PowerShell, di folder aktif.
pub(crate) fn open_external_terminal(w: &mut FmWin) {
    let dir = cur_path(w);
    let dirw = to_wide_z(&dir);
    let dir_p = if dir.is_empty() { PCWSTR::null() } else { pcw(&dirw) };
    let open = to_wide_z("open");
    let wt = to_wide_z("wt.exe");
    let args = to_wide_z(&if dir.is_empty() { String::new() } else { format!("-d \"{}\"", dir) });
    let r = unsafe { ShellExecuteW(Some(w.h), pcw(&open), pcw(&wt), pcw(&args), dir_p, SW_SHOWNORMAL) };
    if (r.0 as isize) > 32 {
        return;
    }
    let ps = to_wide_z("powershell.exe");
    let none = to_wide_z("-NoLogo");
    let r = unsafe { ShellExecuteW(Some(w.h), pcw(&open), pcw(&ps), pcw(&none), dir_p, SW_SHOWNORMAL) };
    if (r.0 as isize) <= 32 {
        fm_status(w, "Terminal tidak dapat dibuka", true);
    }
}

fn folder_options(w: &mut FmWin) {
    let open = to_wide_z("open");
    let exe = to_wide_z("rundll32.exe");
    let args = to_wide_z("shell32.dll,Options_RunDLL 0");
    unsafe {
        let _ = ShellExecuteW(Some(w.h), pcw(&open), pcw(&exe), pcw(&args), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

fn add(menu: HMENU, id: usize, label: &str, checked: bool, enabled: bool) {
    let s = to_wide_z(label);
    let mut f = MF_STRING;
    if checked {
        f |= MF_CHECKED;
    }
    if !enabled {
        f |= MF_GRAYED;
    }
    let _ = unsafe { AppendMenuW(menu, f, id, pcw(&s)) };
}

fn sep(menu: HMENU) {
    let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) };
}

// ---- perintah kontekstual Explorer (2026-11-24): Batalkan, Bagikan, ZIP, Ekstrak, latar, putar ----

/// Jalur berkas item terpilih (tanpa kutip). Item tanpa jalur (virtual) dilewati.
fn selected_paths(w: &mut FmWin) -> Vec<String> {
    let mut out = Vec::new();
    let Some(fv) = folder_view(w) else { return out };
    let Ok(arr) = (unsafe { fv.GetSelection(false) }) else { return out };
    let arr: IShellItemArray = arr;
    let n = unsafe { arr.GetCount() }.unwrap_or(0);
    for i in 0..n {
        if let Ok(it) = unsafe { arr.GetItemAt(i) } {
            if let Ok(s) = unsafe { it.GetDisplayName(SIGDN_FILESYSPATH) } {
                out.push(String::from_utf16_lossy(unsafe { s.as_wide() }));
                unsafe { CoTaskMemFree(Some(s.0 as *const c_void)) };
            }
        }
    }
    out
}

fn ext_of(p: &str) -> String {
    std::path::Path::new(p).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

const IMAGE_EXTS: [&str; 9] = ["jpg", "jpeg", "png", "bmp", "gif", "tif", "tiff", "jfif", "webp"];

/// Batalkan (Ctrl+Z) lewat perintah DefView "Batalkan" (SFVIDM_EDIT_UNDO), sama dengan menu Edit Explorer lama.
fn undo(w: &mut FmWin) {
    use windows::Win32::System::Ole::IOleWindow;
    let Some(sv) = fm_active(w).and_then(|t| fm_current_view(t)) else { return };
    let Ok(h) = sv.cast::<IOleWindow>().and_then(|o| unsafe { o.GetWindow() }) else { return };
    // SAFETY: h jendela DefView milik proses ini.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            h,
            windows::Win32::UI::WindowsAndMessaging::WM_COMMAND,
            Some(WPARAM(0x701B)),
            Some(LPARAM(0)),
        );
    }
}

/// Kompres ke ZIP seperti Explorer: buat .zip kosong di folder aktif, lalu jatuhkan pilihan ke folder terkompresi
/// (penangan ZIP bawaan Windows menyalin dengan dialog progresnya sendiri).
fn compress_zip(w: &mut FmWin) {
    use windows::Win32::System::Ole::{IDropTarget, DROPEFFECT_COPY};
    use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
    use windows::Win32::UI::Shell::{SHCreateItemFromParsingName, BHID_DataObject, BHID_SFUIObject};
    let dir = cur_path(w);
    let paths = selected_paths(w);
    if dir.is_empty() || paths.is_empty() {
        fm_status(w, "Pilih berkas atau folder di folder biasa terlebih dahulu", true);
        return;
    }
    let base = if paths.len() == 1 {
        let p = std::path::Path::new(&paths[0]);
        p.file_stem().or_else(|| p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        std::path::Path::new(&dir).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Arsip".into())
    };
    let mut zip = std::path::Path::new(&dir).join(format!("{base}.zip"));
    let mut k = 2;
    while zip.exists() {
        zip = std::path::Path::new(&dir).join(format!("{base} ({k}).zip"));
        k += 1;
    }
    // Arsip ZIP kosong = rekaman akhir direktori saja.
    let mut empty = vec![0x50u8, 0x4B, 0x05, 0x06];
    empty.resize(22, 0);
    if std::fs::write(&zip, &empty).is_err() {
        fm_status(w, "Berkas ZIP tidak dapat dibuat di sini", true);
        return;
    }
    let zip_s = zip.to_string_lossy().into_owned();
    let ok = (|| -> Option<()> {
        let fv = folder_view(w)?;
        let arr: IShellItemArray = unsafe { fv.GetSelection(false) }.ok()?;
        let data: IDataObject = unsafe { arr.BindToHandler(None::<&IBindCtx>, &BHID_DataObject) }.ok()?;
        let zw = to_wide_z(&zip_s);
        let item: IShellItem = unsafe { SHCreateItemFromParsingName(pcw(&zw), None::<&IBindCtx>) }.ok()?;
        let dt: IDropTarget = unsafe { item.BindToHandler(None::<&IBindCtx>, &BHID_SFUIObject) }.ok()?;
        let pt = windows::Win32::Foundation::POINTL { x: 0, y: 0 };
        let mut eff = DROPEFFECT_COPY;
        unsafe {
            dt.DragEnter(&data, MODIFIERKEYS_FLAGS(0), pt, &mut eff).ok()?;
            eff = DROPEFFECT_COPY;
            dt.Drop(&data, MODIFIERKEYS_FLAGS(0), pt, &mut eff).ok()?;
        }
        Some(())
    })()
    .is_some();
    if ok {
        fm_status(w, &format!("Mengompres ke {}", zip.file_name().map(|s| s.to_string_lossy()).unwrap_or_default()), false);
    } else {
        let _ = std::fs::remove_file(&zip);
        fm_status(w, "Kompres ZIP gagal", true);
    }
}

/// Menu "Lainnya" (⋯) seperti Explorer Windows 11, ditambah perintah terminal dan jalur.
pub(crate) fn more_menu(w: &mut FmWin, anchor: RECT) {
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else { return };
    let has_sel = w.has_sel;
    let fs = !cur_path(w).is_empty();
    let sel = if has_sel { selected_paths(w) } else { Vec::new() };
    let all_ext = |set: &[&str]| !sel.is_empty() && sel.iter().all(|p| set.contains(&ext_of(p).as_str()));
    let zips = all_ext(&["zip"]);
    let images = all_ext(&IMAGE_EXTS);
    add(menu, 20, "Batalkan\tCtrl+Z", false, true);
    sep(menu);
    add(menu, 21, "Bagikan", false, has_sel);
    add(menu, 22, "Kompres ke berkas ZIP", false, fs && !sel.is_empty());
    add(menu, 23, "Ekstrak semua...", false, zips && sel.len() == 1);
    if images {
        add(menu, 24, "Atur sebagai latar belakang", false, sel.len() == 1);
        add(menu, 25, "Putar ke kanan", false, true);
        add(menu, 26, "Putar ke kiri", false, true);
    }
    sep(menu);
    add(menu, 1, "Sematkan ke Akses cepat", false, has_sel);
    sep(menu);
    add(menu, 2, "Pilih semua\tCtrl+A", false, true);
    add(menu, 3, "Kosongkan pilihan", false, has_sel);
    add(menu, 4, "Balikkan pilihan", false, true);
    sep(menu);
    add(menu, 5, "Salin jalur\tCtrl+Shift+C", false, true);
    add(menu, 6, "Buka di Terminal", false, fs);
    add(menu, 7, "Panel terminal\tF4", w.term_on, true);
    sep(menu);
    add(menu, 8, "Panel pratinjau\tAlt+P", pane_on(false), true);
    add(menu, 9, "Panel detail\tAlt+Shift+P", pane_on(true), true);
    sep(menu);
    add(menu, 10, "Properti\tAlt+Enter", false, true);
    add(menu, 11, "Opsi", false, true);
    let pt = screen_pt(w.h, anchor.right, anchor.bottom);
    let id = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | windows::Win32::UI::WindowsAndMessaging::TPM_RIGHTALIGN | TPM_TOPALIGN,
            pt.x,
            pt.y,
            None,
            w.h,
            None,
        )
    }
    .0;
    let _ = unsafe { DestroyMenu(menu) };
    match id {
        1 => crate::files::bar::fm_run_verb(w, false, &crate::wide::to_wide("pintohome")),
        2 => select_all(w, 0),
        3 => select_all(w, 1),
        4 => select_all(w, 2),
        5 => copy_path(w),
        6 => open_external_terminal(w),
        7 => term_toggle(w),
        8 => pane_toggle(w, false),
        9 => pane_toggle(w, true),
        10 => {
            let bg = !w.has_sel;
            crate::files::bar::fm_run_verb(w, bg, &crate::wide::to_wide("properties"));
        }
        11 => folder_options(w),
        20 => undo(w),
        21 => crate::files::bar::fm_run_verb(w, false, &crate::wide::to_wide("Windows.ModernShare")),
        22 => compress_zip(w),
        23 => crate::files::bar::fm_run_verb(w, false, &crate::wide::to_wide("extract")),
        24 => crate::files::bar::fm_run_verb(w, false, &crate::wide::to_wide("setdesktopwallpaper")),
        25 => crate::files::bar::fm_run_verb(w, false, &crate::wide::to_wide("Windows.rotateright")),
        26 => crate::files::bar::fm_run_verb(w, false, &crate::wide::to_wide("Windows.rotateleft")),
        _ => {}
    }
}

// ---- panel terminal F4 ----

/// F4: tampilkan/sembunyikan panel terminal (sesi tetap hidup saat disembunyikan, seperti Dolphin).
pub(crate) fn term_toggle(w: &mut FmWin) {
    if w.term.0.is_null() {
        let mut dir = cur_path(w);
        if dir.is_empty() {
            dir = std::env::var("USERPROFILE").unwrap_or_default();
        }
        match term::term_open(w.h, &dir, w.dark) {
            Some(h) => w.term = h,
            None => {
                fm_status(w, "Terminal tidak dapat dijalankan", true);
                return;
            }
        }
        w.term_on = true;
    } else {
        w.term_on = !w.term_on;
    }
    fm_layout(w);
    if w.term_on {
        term::term_focus(w.term);
        term_follow(w);
    } else if let Some(t) = fm_active(w) {
        fm_focus_view(t);
    }
    unsafe {
        let _ = InvalidateRect(Some(w.h), None, false);
    }
}

/// Shell di panel keluar (`exit`): tutup panel.
pub(crate) fn term_closed(w: &mut FmWin, h: HWND) {
    if w.term != h || w.term.0.is_null() {
        return;
    }
    let had_focus = unsafe { GetFocus() } == w.term;
    unsafe {
        let _ = DestroyWindow(w.term);
    }
    w.term = HWND::default();
    w.term_on = false;
    fm_layout(w);
    if had_focus {
        if let Some(t) = fm_active(w) {
            fm_focus_view(t);
        }
    }
    unsafe {
        let _ = InvalidateRect(Some(w.h), None, false);
    }
}

/// Folder aktif berubah: panel terminal ikut pindah (bila shell sedang menganggur).
pub(crate) fn term_follow(w: &mut FmWin) {
    if w.term.0.is_null() || !w.term_on {
        return;
    }
    let dir = cur_path(w);
    if !dir.is_empty() {
        term::term_sync(w.term, &dir);
    }
}

/// Shell di panel terminal pindah folder (cd, z, Set-Location): File Explorer ikut pindah (2026-11-19).
/// Fokus tetap di terminal agar pengguna bisa terus mengetik.
pub(crate) fn term_cwd(w: &mut FmWin, path: &str) {
    if w.term.0.is_null() || !w.term_on {
        return;
    }
    let norm = |s: &str| {
        let t = s.trim_end_matches('\\');
        if t.len() == 2 && t.ends_with(':') { format!("{}\\", t) } else { t.to_string() }
    };
    let want = norm(path);
    if want.is_empty() || norm(&cur_path(w)).eq_ignore_ascii_case(&want) {
        return;
    }
    if !std::path::Path::new(&want).is_dir() {
        return;
    }
    let had_focus = unsafe { GetFocus() } == w.term;
    if crate::files::app::fm_navigate(w, &want) && had_focus {
        unsafe {
            let _ = SetFocus(Some(w.term));
        }
    }
}

/// Posisikan panel terminal (dipanggil fm_layout).
pub(crate) fn term_layout(w: &FmWin) {
    if w.term.0.is_null() {
        return;
    }
    let g = chrome::geo(w);
    unsafe {
        if w.term_on && g.stat_top > g.term_top {
            let _ = MoveWindow(w.term, 0, g.term_top, g.cw, g.stat_top - g.term_top, true);
            let _ = ShowWindow(w.term, SW_SHOW);
        } else {
            let _ = ShowWindow(w.term, SW_HIDE);
        }
    }
}

/// Seret pembatas: tinggi panel = jarak kursor ke bilah status (DIP), dibatasi di chrome::geo.
pub(crate) fn split_drag(w: &mut FmWin, y: i32) {
    let g = chrome::geo(w);
    let dpi = g.dpi.max(96);
    let h_px = (g.stat_top - y).max(0);
    let dip = h_px * 96 / dpi;
    let max_dip = ((g.stat_top - g.bar_h) * 96 / dpi - 120).max(80);
    let v = dip.clamp(80, max_dip);
    if v != w.term_h {
        w.term_h = v;
        fm_layout(w);
        unsafe {
            let _ = InvalidateRect(Some(w.h), None, false);
        }
    }
}

/// Tinggi bawaan panel (klik ganda pembatas).
pub(crate) fn split_reset(w: &mut FmWin) {
    w.term_h = FM_TERMH_DIP;
    fm_layout(w);
}

/// Fokus ke panel terminal bila tampil (Ctrl+` dari tampilan folder).
pub(crate) fn term_has_focus(w: &FmWin) -> bool {
    !w.term.0.is_null() && unsafe { GetFocus() } == w.term
}

pub(crate) fn term_focus_toggle(w: &mut FmWin) {
    if w.term.0.is_null() || !w.term_on {
        term_toggle(w);
        return;
    }
    if term_has_focus(w) {
        if let Some(t) = fm_active(w) {
            fm_focus_view(t);
        }
    } else {
        unsafe {
            let _ = SetFocus(Some(w.term));
        }
    }
}

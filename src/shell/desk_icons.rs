//! Ikon desktop untuk mode shell (jendela "Progman" Rust, `core_c3b::desk_proc`).
//!
//! 2026-11-23: ditulis ulang agar setara Explorer (cadangan versi lama: %TEMP%\desk_icons-before-20261123.rs):
//! - Isi dari namespace Desktop shell (`SHGetDesktopFolder` + EnumObjects): folder Desktop pengguna dan publik
//!   tergabung, nama mengikuti pengaturan sembunyikan ekstensi, ikon sistem (Tempat Sampah, This PC, Jaringan,
//!   Control Panel, folder pengguna) mengikuti "Desktop icon settings" (HideDesktopIcons\NewStartPanel), dan
//!   "Tampilkan ikon desktop" (Advanced\HideIcons) dihormati.
//! - Ukuran ikon dan sel mengikuti DPI dan metrik sistem; ikon tajam dari IShellItemImageFactory (alpha
//!   premultiplied), panah pintasan dari ikon stok, font judul ikon sistem, sorotan transparan hover/pilih.
//! - Menu konteks shell asli per ikon (IContextMenu3: Buka dengan, Kirim ke, dll.), pilihan ganda (Ctrl/Shift,
//!   seret kotak), keyboard (Enter, Delete, F2, F5, panah, Ctrl+A/C/X/V, Alt+Enter, tombol menu), ganti nama
//!   langsung di tempat, seret ke luar (SHDoDragDrop) dan jatuhkan ke desktop/ikon (IDropTarget diteruskan).
//! - Pembaruan otomatis lewat SHChangeNotifyRegister (pemberitahuan shell; bukan ReadDirectoryChangesW buatan
//!   sendiri yang pernah membocorkan IRP dan membuat freeze, catatan 2026-11-13).
//! Posisi ikon selalu tersusun otomatis (kolom dari kiri atas, urut nama); posisi bebas belum didukung.

use super::hotkeys::shell_open;
use super::prelude::*;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::time::Instant;
use windows::core::{implement, PCSTR, PSTR};
use windows::Win32::Foundation::POINTL;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, IDataObject, CLSCTX_INPROC_SERVER};
use windows::Win32::System::DataExchange::{IsClipboardFormatAvailable, RegisterClipboardFormatW};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop, RevokeDragDrop, DROPEFFECT, DROPEFFECT_COPY,
    DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi, SystemParametersInfoForDpi};
use windows::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
use windows::Win32::UI::Shell::{
    CLSID_DragDropHelper, CLSID_NewMenu, DefSubclassProc, IContextMenu, IContextMenu2, IContextMenu3,
    IDropTargetHelper, IEnumIDList, IShellExtInit, IShellFolder, IShellItem, IShellItemImageFactory, ILClone,
    ILFree, RemoveWindowSubclass, SHBindToParent, SHChangeNotifyDeregister, SHChangeNotifyEntry,
    SHChangeNotifyRegister, SHCreateItemFromIDList, SHDoDragDrop, SHEmptyRecycleBinW, SHGetDesktopFolder,
    SHGetKnownFolderIDList, SHGetKnownFolderPath, SHGetPathFromIDListW, SHGetStockIconInfo, SHParseDisplayName,
    SetWindowSubclass, StrCmpLogicalW, StrRetToStrW, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
    FOLDERID_Desktop, FOLDERID_PublicDesktop, KF_FLAG_DEFAULT, SHCNRF_SOURCE, SHGDNF, SHGSI_ICONLOCATION,
    SHSTOCKICONINFO, SIID_LINK, SIIGBF_ICONONLY,
};

/// Alamat Tempat Sampah. win12files membukanya lewat SHParseDisplayName (files/app.rs fm_navigate_to), tanpa Explorer.
pub const RECYCLE_PATH: &str = "shell:::{645FF040-5081-101B-9F08-00AA002F954E}";
const RECYCLE_CLSID: &str = "{645FF040-5081-101B-9F08-00AA002F954E}";

/// Batas jumlah ikon, supaya grid tidak meluap jauh dari layar.
const MAX_ICONS: usize = 400;
/// Pesan pemberitahuan shell ke jendela desktop (SHChangeNotifyRegister).
pub const WM_DESK_NOTIFY: u32 = WM_APP + 0x3D1;
/// Timer jendela desktop: muat ulang tertunda setelah pemberitahuan (gabungan beberapa perubahan).
pub const T_DESK_RELOAD: usize = 0xD35C;
/// Id perintah menu konteks shell (idCmdFirst..idCmdLast).
const CM_FIRST: u32 = 1;
const CM_LAST: u32 = 0x7FFF;
/// Id menu latar desktop untuk menu "Baru" (CLSID_NewMenu).
const NEW_FIRST: u32 = 0x1000;
const NEW_LAST: u32 = 0x1FFF;

// SFGAO_* (nilai Win32), ditulis sebagai angka agar tidak bergantung pada tipe binding.
const SFGAO_LINK: u32 = 0x0001_0000;
const SFGAO_STREAM: u32 = 0x0040_0000;
const SFGAO_FOLDER: u32 = 0x2000_0000;
const SFGAO_FILESYSTEM: u32 = 0x4000_0000;
// SHGDN_*
const SHGDN_NORMAL: u32 = 0;
const SHGDN_INFOLDER: u32 = 1;
const SHGDN_FOREDITING: u32 = 0x1000;
const SHGDN_FORPARSING: u32 = 0x8000;
// SHCONTF_*
const SHCONTF_FOLDERS: u32 = 0x20;
const SHCONTF_NONFOLDERS: u32 = 0x40;
// CMF_* dan CMIC_MASK_*
const CMF_NORMAL: u32 = 0;
const CMF_DEFAULTONLY: u32 = 0x1;
const CMF_CANRENAME: u32 = 0x10;
const CMF_EXTENDEDVERBS: u32 = 0x100;
const CMIC_MASK_UNICODE: u32 = 0x4000;
const CMIC_MASK_SHIFT_DOWN: u32 = 0x1000_0000;
const CMIC_MASK_CONTROL_DOWN: u32 = 0x4000_0000;
const CMIC_MASK_PTINVOKE: u32 = 0x2000_0000;
const GCS_VERBW: u32 = 4;
// SHCNE_*
const SHCNE_CREATE: i32 = 0x2;
const SHCNE_MKDIR: i32 = 0x8;
const SHCNE_ALLEVENTS: i32 = 0x7FFF_FFFF;
const CF_HDROP: u32 = 15;

/// Satu ikon desktop. `pidl` relatif terhadap Desktop (juga absolut) dan dimiliki ikon ini.
struct DeskIcon {
    label: String,
    /// Nama penguraian: path file sistem, atau "::{CLSID}" untuk item virtual.
    path: String,
    pidl: *mut ITEMIDLIST,
    attrs: u32,
    /// Bitmap 32 bpp premultiplied seukuran ikon; dimiliki ikon ini.
    bmp: HBITMAP,
    selected: bool,
    x: i32,
    y: i32,
}

impl DeskIcon {
    fn is_folder(&self) -> bool {
        self.attrs & SFGAO_FOLDER != 0 && self.attrs & SFGAO_STREAM == 0
    }
    fn is_link(&self) -> bool {
        self.attrs & SFGAO_LINK != 0
    }
}

impl Drop for DeskIcon {
    fn drop(&mut self) {
        // SAFETY: pidl dan bmp dimiliki ikon ini dan tidak dipakai lagi setelah drop.
        unsafe {
            if !self.pidl.is_null() {
                ILFree(Some(self.pidl));
            }
            if !self.bmp.0.is_null() {
                let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            }
        }
    }
}

/// Ukuran turunan DPI. Dihitung ulang saat muat ulang.
#[derive(Clone, Copy)]
struct Metrics {
    icon_px: i32,
    cell_w: i32,
    cell_h: i32,
    line_h: i32,
    margin: i32,
    pad: i32,
    font: HFONT,
    /// Overlay panah pintasan seukuran ikon (dimiliki modul).
    link: HICON,
}

/// Seret kotak pilihan: titik awal, titik sekarang (koordinat klien), dan pilihan sebelum seret (Ctrl).
struct Marquee {
    start: POINT,
    cur: POINT,
    base: Vec<bool>,
}

/// Tekan tombol kiri pada ikon: dipakai untuk ambang seret dan klik tanpa seret.
struct Press {
    pt: POINT,
    idx: usize,
    mods: u32,
}

/// Ganti nama di tempat: kotak edit dan pidl item (salinan, dilepas saat selesai).
struct Rename {
    edit: HWND,
    pidl: *mut ITEMIDLIST,
    original: String,
}

struct DeskState {
    icons: Vec<DeskIcon>,
    m: Option<Metrics>,
    hover: i32,
    focus: i32,
    anchor: i32,
    marquee: Option<Marquee>,
    press: Option<Press>,
    rename: Option<Rename>,
    tracking_leave: bool,
    notify_id: u32,
    /// Setelah "Baru": item yang dibuat dalam 8 detik langsung masuk mode ganti nama.
    pending_new: Option<Instant>,
    rename_path: Option<String>,
    ole_ok: Option<bool>,
}

static DESK_STATE: SyncCell<RefCell<DeskState>> = SyncCell::new(RefCell::new(DeskState {
    icons: Vec::new(),
    m: None,
    hover: -1,
    focus: -1,
    anchor: -1,
    marquee: None,
    press: None,
    rename: None,
    tracking_leave: false,
    notify_id: 0,
    pending_new: None,
    rename_path: None,
    ole_ok: None,
}));

thread_local! {
    /// Menu shell yang sedang terbuka; pesan menu diteruskan ke sini (submenu Buka dengan, Kirim ke, Baru).
    static ACTIVE_CM: RefCell<Option<IContextMenu>> = const { RefCell::new(None) };
    /// Target jatuhkan yang terdaftar (RegisterDragDrop memegang referensinya sendiri).
    static DROP_TARGET: RefCell<Option<IDropTarget>> = const { RefCell::new(None) };
}

fn desk_hwnd() -> HWND {
    DESK.get()
}

fn scale(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

// ---------------------------------------------------------------- namespace Desktop

fn desktop_folder() -> Option<IShellFolder> {
    // SAFETY: tanpa prasyarat selain COM aktif di thread ini (RoInitialize di w_win_main).
    unsafe { SHGetDesktopFolder() }.ok()
}

fn display_name(sf: &IShellFolder, pidl: *const ITEMIDLIST, flags: u32) -> String {
    let mut sr: STRRET = unsafe { zeroed() };
    // SAFETY: pidl milik pemanggil dan hidup selama pemanggilan; string hasil dibebaskan CoTaskMemFree.
    unsafe {
        if sf.GetDisplayNameOf(pidl, SHGDNF(flags), &mut sr).is_err() {
            return String::new();
        }
        let mut p = PWSTR::null();
        if StrRetToStrW(&mut sr, Some(pidl), &mut p).is_err() || p.is_null() {
            return String::new();
        }
        let s = from_pcwstr(p.0);
        CoTaskMemFree(Some(p.0 as *const c_void));
        s
    }
}

fn known_folder_path(id: &GUID) -> Option<String> {
    // SAFETY: string hasil dibebaskan CoTaskMemFree.
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = from_pcwstr(p.0);
        CoTaskMemFree(Some(p.0 as *const c_void));
        (!s.is_empty()).then_some(s)
    }
}

fn desktop_dirs() -> Vec<String> {
    [&FOLDERID_Desktop, &FOLDERID_PublicDesktop]
        .into_iter()
        .filter_map(known_folder_path)
        .map(|s| s.trim_end_matches('\\').to_lowercase())
        .collect()
}

/// Ikon virtual tampil bila nilai HideDesktopIcons\NewStartPanel\{CLSID} = 0. Tanpa nilai: hanya Tempat Sampah
/// yang tampil (bawaan Windows).
fn virtual_visible(clsid: &str) -> bool {
    let k = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\HideDesktopIcons\\NewStartPanel";
    match reg::get_dword(k, clsid) {
        Some(v) => v == 0,
        None => clsid.eq_ignore_ascii_case(RECYCLE_CLSID),
    }
}

/// "Tampilkan ikon desktop" (Explorer\Advanced\HideIcons = 1 menyembunyikan semua).
pub fn desk_icons_hidden() -> bool {
    reg::get_dword("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced", "HideIcons") == Some(1)
}

pub fn desk_icons_set_hidden(hide: bool) {
    let _ = reg::set_dword(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced",
        "HideIcons",
        u32::from(hide),
    );
    desk_icons_reload();
}

/// Ukuran ikon logis (32 kecil, 48 sedang, 96 besar) di HKCU\Software\Win12Shell DeskIconSize.
pub fn desk_icon_size() -> i32 {
    match reg::our_dword("DeskIconSize") {
        32 => 32,
        96 => 96,
        _ => 48,
    }
}

pub fn desk_set_icon_size(px: i32) {
    let _ = reg::set_our_dword("DeskIconSize", px as u32);
    desk_icons_reload();
}

/// Enumerasi item Desktop sesuai aturan Explorer. Hasil: (label, path, pidl milik pemanggil, atribut).
fn enumerate() -> Vec<(String, String, *mut ITEMIDLIST, u32)> {
    let mut out = Vec::new();
    if desk_icons_hidden() {
        return out;
    }
    let Some(sf) = desktop_folder() else { return out };
    let dirs = desktop_dirs();
    let mut en: Option<IEnumIDList> = None;
    // SAFETY: en diisi oleh shell; HWND pemilik boleh nol.
    let hr = unsafe { sf.EnumObjects(desk_hwnd(), SHCONTF_FOLDERS | SHCONTF_NONFOLDERS, &mut en) };
    let Some(en) = en.filter(|_| hr.is_ok()) else { return out };
    loop {
        let mut one: [*mut ITEMIDLIST; 1] = [std::ptr::null_mut()];
        let mut got = 0u32;
        // SAFETY: satu slot; pidl hasil dimiliki pemanggil.
        if unsafe { en.Next(&mut one, Some(&mut got)) } != S_OK || got == 0 || one[0].is_null() {
            break;
        }
        let pidl = one[0];
        let path = display_name(&sf, pidl, SHGDN_FORPARSING);
        let keep = if let Some(rest) = path.strip_prefix("::") {
            virtual_visible(rest)
        } else {
            let parent = path.rsplit_once('\\').map(|(p, _)| p.to_lowercase()).unwrap_or_default();
            dirs.iter().any(|d| *d == parent)
        };
        if !keep || out.len() >= MAX_ICONS {
            // SAFETY: pidl milik kita dan tidak dipakai.
            unsafe { ILFree(Some(pidl)) };
            continue;
        }
        let mut attrs: u32 = SFGAO_FOLDER | SFGAO_STREAM | SFGAO_LINK | SFGAO_FILESYSTEM;
        let arr = [pidl as *const ITEMIDLIST];
        // SAFETY: satu pidl anak dari folder ini.
        if unsafe { sf.GetAttributesOf(&arr, &mut attrs) }.is_err() {
            attrs = 0;
        }
        let label = display_name(&sf, pidl, SHGDN_NORMAL);
        out.push((label, path, pidl, attrs));
    }
    // Ikon sistem dulu (seperti Explorer), lalu urut nama alami.
    out.sort_by(|a, b| {
        let va = a.1.starts_with("::");
        let vb = b.1.starts_with("::");
        vb.cmp(&va).then_with(|| {
            let wa = to_wide_z(&a.0);
            let wb = to_wide_z(&b.0);
            // SAFETY: dua string berakhir NUL yang hidup selama pemanggilan.
            unsafe { StrCmpLogicalW(pcw(&wa), pcw(&wb)) }.cmp(&0)
        })
    });
    out
}

// ---------------------------------------------------------------- metrik, ikon

fn build_metrics() -> Metrics {
    let h = desk_hwnd();
    // SAFETY: GetDpiForWindow menerima HWND apa pun; 0 bila tidak valid.
    let mut dpi = if h.0.is_null() { 0 } else { unsafe { GetDpiForWindow(h) } };
    if dpi == 0 {
        dpi = 96;
    }
    let icon_px = scale(desk_icon_size(), dpi);
    let mut lf: LOGFONTW = unsafe { zeroed() };
    // SAFETY: lf hidup selama pemanggilan; ukuran sesuai SPI_GETICONTITLELOGFONT.
    let ok = unsafe {
        SystemParametersInfoForDpi(
            SPI_GETICONTITLELOGFONT.0,
            size_of::<LOGFONTW>() as u32,
            Some(&mut lf as *mut LOGFONTW as *mut c_void),
            0,
            dpi,
        )
    }
    .is_ok();
    if !ok {
        lf.lfHeight = -scale(12, dpi);
        let face: Vec<u16> = "Segoe UI".encode_utf16().collect();
        lf.lfFaceName[..face.len()].copy_from_slice(&face);
    }
    // SAFETY: lf valid; HFONT dimiliki modul (dilepas di free_metrics).
    let font = unsafe { CreateFontIndirectW(&lf) };
    let mut line_h = scale(16, dpi);
    // SAFETY: DC layar sementara; font dipilih lalu dikembalikan.
    unsafe {
        let dc = GetDC(None);
        let old = SelectObject(dc, HGDIOBJ(font.0));
        let mut tm: TEXTMETRICW = zeroed();
        if GetTextMetricsW(dc, &mut tm).as_bool() {
            line_h = tm.tmHeight;
        }
        SelectObject(dc, old);
        ReleaseDC(None, dc);
    }
    // SAFETY: metrik sistem tanpa pointer.
    let spacing = unsafe { GetSystemMetricsForDpi(SM_CXICONSPACING, dpi) };
    let cell_w = spacing.max(icon_px + scale(30, dpi));
    let pad = scale(4, dpi);
    let cell_h = pad + icon_px + pad + 2 * line_h + pad * 2;
    Metrics { icon_px, cell_w, cell_h, line_h, margin: scale(4, dpi), pad, font, link: link_overlay(icon_px) }
}

fn free_metrics(m: &Metrics) {
    // SAFETY: objek dibuat build_metrics dan tidak dipakai lagi.
    unsafe {
        if !m.font.0.is_null() {
            let _ = DeleteObject(HGDIOBJ(m.font.0));
        }
        if !m.link.0.is_null() {
            let _ = DestroyIcon(m.link);
        }
    }
}

/// Overlay panah pintasan tepat seukuran ikon (lokasi ikon stok SIID_LINK lewat PrivateExtractIconsW).
fn link_overlay(px: i32) -> HICON {
    let mut sii: SHSTOCKICONINFO = unsafe { zeroed() };
    sii.cbSize = size_of::<SHSTOCKICONINFO>() as u32;
    // SAFETY: sii diisi cbSize.
    if unsafe { SHGetStockIconInfo(SIID_LINK, SHGSI_ICONLOCATION, &mut sii) }.is_err() {
        return HICON::default();
    }
    let mut hi = [HICON::default()];
    // SAFETY: szPath berakhir NUL; satu HICON hasil dimiliki pemanggil.
    let n = unsafe { PrivateExtractIconsW(&sii.szPath, sii.iIcon, px, px, Some(&mut hi), None, 0) };
    if n == 0 || n == u32::MAX {
        return HICON::default();
    }
    hi[0]
}

/// HBITMAP dari image factory -> DIB 32 bpp top-down premultiplied milik kita. `src` selalu dilepas.
fn to_premultiplied(src: HBITMAP) -> HBITMAP {
    let mut out = HBITMAP::default();
    // SAFETY: semua buffer berukuran sesuai header; src dilepas di akhir.
    unsafe {
        let mut bm: BITMAP = zeroed();
        if GetObjectW(HGDIOBJ(src.0), size_of::<BITMAP>() as i32, Some(&mut bm as *mut BITMAP as *mut c_void)) != 0
            && bm.bmWidth > 0
            && bm.bmHeight != 0
        {
            let w = bm.bmWidth;
            let h = bm.bmHeight.abs();
            let mut bi: BITMAPINFO = zeroed();
            bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w;
            bi.bmiHeader.biHeight = -h;
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            let mut px: Vec<u8> = vec![0; (w * h * 4) as usize];
            let dc = GetDC(None);
            let got = GetDIBits(dc, src, 0, h as u32, Some(px.as_mut_ptr() as *mut c_void), &mut bi, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            if got > 0 {
                let any_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
                if !any_alpha {
                    px.chunks_exact_mut(4).for_each(|p| p[3] = 255);
                } else if px.chunks_exact(4).any(|p| p[0] > p[3] || p[1] > p[3] || p[2] > p[3]) {
                    // Belum premultiplied: kalikan warna dengan alpha.
                    for p in px.chunks_exact_mut(4) {
                        let a = p[3] as u32;
                        p[0] = (p[0] as u32 * a / 255) as u8;
                        p[1] = (p[1] as u32 * a / 255) as u8;
                        p[2] = (p[2] as u32 * a / 255) as u8;
                    }
                }
                let mut bits: *mut c_void = std::ptr::null_mut();
                if let Ok(b) = CreateDIBSection(None, &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
                    if !bits.is_null() {
                        std::ptr::copy_nonoverlapping(px.as_ptr(), bits as *mut u8, px.len());
                        out = b;
                    } else {
                        let _ = DeleteObject(HGDIOBJ(b.0));
                    }
                }
            }
        }
        let _ = DeleteObject(HGDIOBJ(src.0));
    }
    out
}

fn icon_bitmap(pidl: *const ITEMIDLIST, px: i32) -> HBITMAP {
    // SAFETY: pidl absolut (anak Desktop) yang hidup selama pemanggilan.
    unsafe {
        let Ok(si) = SHCreateItemFromIDList::<IShellItem>(pidl) else { return HBITMAP::default() };
        let Ok(f) = si.cast::<IShellItemImageFactory>() else { return HBITMAP::default() };
        match f.GetImage(SIZE { cx: px, cy: px }, SIIGBF_ICONONLY) {
            Ok(hb) if !hb.0.is_null() => to_premultiplied(hb),
            _ => HBITMAP::default(),
        }
    }
}

// ---------------------------------------------------------------- muat ulang, tata letak

/// Muat ulang daftar, metrik, dan ikon; pilihan dipertahankan menurut path. Dipanggil dari shell_mode_start,
/// menu "Segarkan", F5, dan pemberitahuan shell.
pub fn desk_icons_reload() {
    if DESK_STATE.borrow().rename.is_some() {
        // Jangan mengganti daftar di tengah ganti nama; dicoba lagi setelah selesai.
        schedule_reload(400);
        return;
    }
    ensure_notify();
    ensure_drop_target();
    let m = build_metrics();
    let selected: Vec<String> = {
        let st = DESK_STATE.borrow();
        st.icons.iter().filter(|i| i.selected).map(|i| i.path.to_lowercase()).collect()
    };
    let found = enumerate();
    let fresh: Vec<DeskIcon> = found
        .into_iter()
        .map(|(label, path, pidl, attrs)| DeskIcon {
            bmp: icon_bitmap(pidl, m.icon_px),
            selected: selected.contains(&path.to_lowercase()),
            label,
            path,
            pidl,
            attrs,
            x: 0,
            y: 0,
        })
        .collect();
    let n = fresh.len();
    let (old_icons, old_m) = {
        let mut st = DESK_STATE.borrow_mut();
        let old_icons = std::mem::replace(&mut st.icons, fresh);
        let old_m = st.m.replace(m);
        st.hover = -1;
        if st.focus >= n as i32 {
            st.focus = -1;
        }
        (old_icons, old_m)
    };
    drop(old_icons);
    if let Some(om) = old_m {
        free_metrics(&om);
    }
    desk_icons_layout();
    invalidate_all();
    // Item baru dari menu "Baru": langsung ganti nama.
    let want = DESK_STATE.borrow_mut().rename_path.take();
    if let Some(p) = want {
        let idx = DESK_STATE.borrow().icons.iter().position(|i| i.path.eq_ignore_ascii_case(&p));
        if let Some(i) = idx {
            select_only(i);
            begin_rename(i);
        }
    }
    log(&format!("desk: {n} ikon desktop dimuat"));
}

/// Kosongkan ikon dan lepaskan sumber daya. Dipanggil dari shell_mode_stop.
pub fn desk_icons_clear() {
    cancel_rename();
    let (icons, m, id) = {
        let mut st = DESK_STATE.borrow_mut();
        let id = std::mem::take(&mut st.notify_id);
        (std::mem::take(&mut st.icons), st.m.take(), id)
    };
    if id != 0 {
        // SAFETY: id dari SHChangeNotifyRegister.
        let _ = unsafe { SHChangeNotifyDeregister(id) };
    }
    let h = desk_hwnd();
    if DROP_TARGET.with(|d| d.borrow_mut().take()).is_some() && !h.0.is_null() {
        // SAFETY: h terdaftar oleh ensure_drop_target.
        let _ = unsafe { RevokeDragDrop(h) };
    }
    drop(icons);
    if let Some(m) = m {
        free_metrics(&m);
    }
}

/// Area kerja layar utama (di atas bilah Win12 karena shell_work_area), koordinat layar.
fn work_area() -> RECT {
    let mut rc = RECT::default();
    // SAFETY: rc hidup selama pemanggilan.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rc as *mut RECT as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_err() {
        rc.right = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        rc.bottom = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    }
    rc
}

/// Grid kolom dari pojok kiri atas area kerja (seperti Explorer). Koordinat klien jendela desktop dimulai di
/// pojok virtual screen.
pub fn desk_icons_layout() {
    let wa = work_area();
    let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let mut st = DESK_STATE.borrow_mut();
    let Some(m) = st.m else { return };
    let ox = wa.left - vx + m.margin;
    let oy = wa.top - vy + m.margin;
    let rows = (((wa.bottom - wa.top) - 2 * m.margin) / m.cell_h).max(1);
    for (i, ic) in st.icons.iter_mut().enumerate() {
        let i = i as i32;
        ic.x = ox + (i / rows) * m.cell_w;
        ic.y = oy + (i % rows) * m.cell_h;
    }
}

fn rows_per_column() -> i32 {
    let wa = work_area();
    let st = DESK_STATE.borrow();
    let Some(m) = st.m else { return 1 };
    (((wa.bottom - wa.top) - 2 * m.margin) / m.cell_h).max(1)
}

/// Rect ikon termasuk teks yang meluap saat terpilih (5 baris).
fn icon_bounds(ic: &DeskIcon, m: &Metrics) -> RECT {
    RECT { left: ic.x, top: ic.y, right: ic.x + m.cell_w, bottom: ic.y + m.cell_h + 3 * m.line_h + m.pad }
}

fn invalidate_all() {
    let h = desk_hwnd();
    if !h.0.is_null() {
        // SAFETY: jendela desktop milik kita.
        let _ = unsafe { InvalidateRect(Some(h), None, false) };
    }
}

fn invalidate_icon(i: i32) {
    let h = desk_hwnd();
    if h.0.is_null() || i < 0 {
        return;
    }
    let rc = {
        let st = DESK_STATE.borrow();
        let (Some(m), Some(ic)) = (st.m, st.icons.get(i as usize)) else { return };
        icon_bounds(ic, &m)
    };
    // SAFETY: rc hidup selama pemanggilan.
    let _ = unsafe { InvalidateRect(Some(h), Some(&rc), false) };
}

fn invalidate_rect(rc: &RECT) {
    let h = desk_hwnd();
    if !h.0.is_null() {
        let _ = unsafe { InvalidateRect(Some(h), Some(rc), false) };
    }
}

fn schedule_reload(ms: u32) {
    let h = desk_hwnd();
    if !h.0.is_null() {
        // SAFETY: timer pada jendela desktop milik kita.
        unsafe { SetTimer(Some(h), T_DESK_RELOAD, ms, None) };
    }
}

// ---------------------------------------------------------------- menggambar

/// Isi persegi dengan warna `rgb` (0xRRGGBB) ber-alpha `a` lewat AlphaBlend 1x1.
fn fill_alpha(dc: HDC, rc: &RECT, rgb: u32, a: u8) {
    if rc.right <= rc.left || rc.bottom <= rc.top {
        return;
    }
    let r = ((rgb >> 16) & 0xFF) * a as u32 / 255;
    let g = ((rgb >> 8) & 0xFF) * a as u32 / 255;
    let b = (rgb & 0xFF) * a as u32 / 255;
    // SAFETY: DIB 1x1 dibuat, dipakai, lalu dilepas di fungsi ini.
    unsafe {
        let mut bi: BITMAPINFO = zeroed();
        bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = 1;
        bi.bmiHeader.biHeight = 1;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let mut bits: *mut c_void = std::ptr::null_mut();
        let Ok(bmp) = CreateDIBSection(None, &bi, DIB_RGB_COLORS, &mut bits, None, 0) else { return };
        if !bits.is_null() {
            *(bits as *mut u32) = (a as u32) << 24 | r << 16 | g << 8 | b;
            let mdc = CreateCompatibleDC(Some(dc));
            let old = SelectObject(mdc, HGDIOBJ(bmp.0));
            let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
            let _ = AlphaBlend(dc, rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top, mdc, 0, 0, 1, 1, bf);
            SelectObject(mdc, old);
            let _ = DeleteDC(mdc);
        }
        let _ = DeleteObject(HGDIOBJ(bmp.0));
    }
}

/// Kotak sorot: isi + garis tepi 1 px (keduanya transparan).
fn highlight(dc: HDC, rc: &RECT, rgb: u32, fill: u8, edge: u8) {
    fill_alpha(dc, &RECT { left: rc.left + 1, top: rc.top + 1, right: rc.right - 1, bottom: rc.bottom - 1 }, rgb, fill);
    fill_alpha(dc, &RECT { left: rc.left, top: rc.top, right: rc.right, bottom: rc.top + 1 }, rgb, edge);
    fill_alpha(dc, &RECT { left: rc.left, top: rc.bottom - 1, right: rc.right, bottom: rc.bottom }, rgb, edge);
    fill_alpha(dc, &RECT { left: rc.left, top: rc.top + 1, right: rc.left + 1, bottom: rc.bottom - 1 }, rgb, edge);
    fill_alpha(dc, &RECT { left: rc.right - 1, top: rc.top + 1, right: rc.right, bottom: rc.bottom - 1 }, rgb, edge);
}

/// Gambar ikon, label, sorotan, dan kotak seret. Dipanggil dari WM_PAINT desk_proc, setelah PaintDesktop.
pub fn desk_icons_paint(dc: HDC) {
    desk_icons_layout();
    let st = DESK_STATE.borrow();
    let Some(m) = st.m else { return };
    let renaming = st.rename.as_ref().map(|r| r.pidl);
    let fmt = DRAW_TEXT_FORMAT(DT_CENTER.0 | DT_WORDBREAK.0 | DT_NOPREFIX.0 | DT_END_ELLIPSIS.0 | DT_EDITCONTROL.0);
    // SAFETY: dc valid dari WM_PAINT; objek GDI dipilih lalu dikembalikan.
    unsafe {
        let old_font = SelectObject(dc, HGDIOBJ(m.font.0));
        SetBkMode(dc, TRANSPARENT);
        let mdc = CreateCompatibleDC(Some(dc));
        let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
        for (i, ic) in st.icons.iter().enumerate() {
            let hot = st.hover == i as i32;
            let tx_top = ic.y + m.pad + m.icon_px + m.pad;
            let lines = if ic.selected { 5 } else { 2 };
            let mut buf: Vec<u16> = ic.label.encode_utf16().collect();
            // Tinggi teks nyata (dibatasi jumlah baris) untuk kotak sorot.
            let mut calc = RECT { left: ic.x + m.pad, top: tx_top, right: ic.x + m.cell_w - m.pad, bottom: tx_top };
            let _ = DrawTextW(dc, &mut buf.clone(), &mut calc, DRAW_TEXT_FORMAT(fmt.0 | DT_CALCRECT.0));
            let text_h = (calc.bottom - calc.top).clamp(m.line_h, lines * m.line_h);
            if ic.selected || hot {
                let rc = RECT {
                    left: ic.x + 2,
                    top: ic.y + 2,
                    right: ic.x + m.cell_w - 2,
                    bottom: tx_top + text_h + m.pad,
                };
                if ic.selected {
                    highlight(dc, &rc, 0x99C9EF, if hot { 0x80 } else { 0x66 }, 0xB0);
                } else {
                    highlight(dc, &rc, 0xFFFFFF, 0x26, 0x50);
                }
            }
            if !ic.bmp.0.is_null() {
                let ix = ic.x + (m.cell_w - m.icon_px) / 2;
                let old = SelectObject(mdc, HGDIOBJ(ic.bmp.0));
                let _ = AlphaBlend(dc, ix, ic.y + m.pad, m.icon_px, m.icon_px, mdc, 0, 0, m.icon_px, m.icon_px, bf);
                SelectObject(mdc, old);
                if ic.is_link() && !m.link.0.is_null() {
                    let _ = DrawIconEx(dc, ix, ic.y + m.pad, m.link, m.icon_px, m.icon_px, 0, None, DI_NORMAL);
                }
            }
            if renaming == Some(ic.pidl) {
                continue;
            }
            let mut tr = RECT {
                left: ic.x + m.pad,
                top: tx_top,
                right: ic.x + m.cell_w - m.pad,
                bottom: tx_top + lines * m.line_h,
            };
            let mut sh = RECT { left: tr.left + 1, top: tr.top + 1, right: tr.right + 1, bottom: tr.bottom + 1 };
            SetTextColor(dc, COLORREF(0x0000_0000));
            let _ = DrawTextW(dc, &mut buf.clone(), &mut sh, fmt);
            SetTextColor(dc, COLORREF(0x00FF_FFFF));
            let _ = DrawTextW(dc, &mut buf, &mut tr, fmt);
        }
        let _ = DeleteDC(mdc);
        SelectObject(dc, old_font);
        if let Some(mq) = &st.marquee {
            highlight(dc, &norm_rect(mq.start, mq.cur), 0x3399FF, 0x40, 0xC0);
        }
    }
}

fn norm_rect(a: POINT, b: POINT) -> RECT {
    RECT { left: a.x.min(b.x), top: a.y.min(b.y), right: a.x.max(b.x) + 1, bottom: a.y.max(b.y) + 1 }
}

// ---------------------------------------------------------------- pilihan

fn hit(x: i32, y: i32) -> Option<usize> {
    let st = DESK_STATE.borrow();
    let m = st.m?;
    st.icons
        .iter()
        .position(|ic| x >= ic.x + 2 && x < ic.x + m.cell_w - 2 && y >= ic.y + 2 && y < ic.y + m.cell_h)
}

/// Terapkan pilihan baru dan gambar ulang hanya ikon yang berubah.
fn apply_selection(f: impl Fn(usize, bool) -> bool) {
    let changed: Vec<i32> = {
        let mut st = DESK_STATE.borrow_mut();
        let mut ch = Vec::new();
        for (k, ic) in st.icons.iter_mut().enumerate() {
            let want = f(k, ic.selected);
            if ic.selected != want {
                ic.selected = want;
                ch.push(k as i32);
            }
        }
        ch
    };
    for k in changed {
        invalidate_icon(k);
    }
}

fn set_focus_anchor(i: i32, anchor: bool) {
    let mut st = DESK_STATE.borrow_mut();
    st.focus = i;
    if anchor {
        st.anchor = i;
    }
}

fn select_only(i: usize) {
    apply_selection(|k, _| k == i);
    set_focus_anchor(i as i32, true);
}

fn clear_selection() {
    apply_selection(|_, _| false);
}

fn selected_indices() -> Vec<usize> {
    DESK_STATE.borrow().icons.iter().enumerate().filter(|(_, i)| i.selected).map(|(k, _)| k).collect()
}

/// Salinan pidl (dilepas saat drop): aman dipakai melewati loop pesan (menu, seret) yang bisa memuat ulang ikon.
struct Pidls(Vec<*mut ITEMIDLIST>);

impl Pidls {
    fn arr(&self) -> Vec<*const ITEMIDLIST> {
        self.0.iter().map(|p| *p as *const ITEMIDLIST).collect()
    }
}

impl Drop for Pidls {
    fn drop(&mut self) {
        for p in &self.0 {
            // SAFETY: salinan dari ILClone milik struktur ini.
            unsafe { ILFree(Some(*p)) };
        }
    }
}

fn selected_pidls() -> Pidls {
    let st = DESK_STATE.borrow();
    Pidls(
        st.icons
            .iter()
            .filter(|i| i.selected && !i.pidl.is_null())
            // SAFETY: pidl ikon valid selama borrow; salinannya dimiliki Pidls.
            .map(|i| unsafe { ILClone(i.pidl) })
            .filter(|p| !p.is_null())
            .collect(),
    )
}

// ---------------------------------------------------------------- menu shell

/// Diteruskan desk_proc untuk WM_INITMENUPOPUP/DRAWITEM/MEASUREITEM/MENUCHAR selama menu shell terbuka.
fn menu_msg(m: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    if !matches!(m, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
        return None;
    }
    let cm = ACTIVE_CM.with(|c| c.borrow().clone())?;
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

/// Tampilkan menu dengan notifikasi aktif (submenu shell diisi lewat WM_INITMENUPOPUP). Mengembalikan id.
fn track_shell_menu(hm: HMENU, pt: POINT, cm: Option<IContextMenu>) -> i32 {
    let h = desk_hwnd();
    ACTIVE_CM.with(|c| *c.borrow_mut() = cm);
    // SAFETY: hm menu valid; h jendela desktop milik kita.
    let id = unsafe {
        let _ = SetForegroundWindow(h);
        let r = TrackPopupMenuEx(
            hm,
            (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_TOPALIGN).0,
            pt.x,
            pt.y,
            h,
            None,
        )
        .0;
        let _ = PostMessageW(Some(h), WM_NULL, WPARAM(0), LPARAM(0));
        r
    };
    ACTIVE_CM.with(|c| *c.borrow_mut() = None);
    id
}

fn key_mods() -> u32 {
    let mut f = 0;
    // SAFETY: GetKeyState tanpa pointer.
    unsafe {
        if GetKeyState(VK_SHIFT.0 as i32) < 0 {
            f |= CMIC_MASK_SHIFT_DOWN;
        }
        if GetKeyState(VK_CONTROL.0 as i32) < 0 {
            f |= CMIC_MASK_CONTROL_DOWN;
        }
    }
    f
}

fn invoke_id(cm: &IContextMenu, id: usize, pt: Option<POINT>) -> bool {
    let mut ici: CMINVOKECOMMANDINFOEX = unsafe { zeroed() };
    ici.cbSize = size_of::<CMINVOKECOMMANDINFOEX>() as u32;
    ici.fMask = CMIC_MASK_UNICODE | key_mods() | if pt.is_some() { CMIC_MASK_PTINVOKE } else { 0 };
    ici.hwnd = desk_hwnd();
    ici.lpVerb = PCSTR(id as *const u8);
    ici.lpVerbW = PCWSTR(id as *const u16);
    ici.nShow = SW_SHOWNORMAL.0;
    if let Some(p) = pt {
        ici.ptInvoke = p;
    }
    // SAFETY: CMINVOKECOMMANDINFOEX diawali CMINVOKECOMMANDINFO; cbSize memberi tahu versi EX.
    unsafe { cm.InvokeCommand(&ici as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO) }.is_ok()
}

fn invoke_verb(cm: &IContextMenu, verb: &str) -> bool {
    let a: Vec<u8> = verb.bytes().chain(std::iter::once(0)).collect();
    let w = to_wide_z(verb);
    let mut ici: CMINVOKECOMMANDINFOEX = unsafe { zeroed() };
    ici.cbSize = size_of::<CMINVOKECOMMANDINFOEX>() as u32;
    ici.fMask = CMIC_MASK_UNICODE | key_mods();
    ici.hwnd = desk_hwnd();
    ici.lpVerb = PCSTR(a.as_ptr());
    ici.lpVerbW = pcw(&w);
    ici.nShow = SW_SHOWNORMAL.0;
    // SAFETY: a dan w hidup selama pemanggilan.
    unsafe { cm.InvokeCommand(&ici as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO) }.is_ok()
}

fn command_verb(cm: &IContextMenu, id: usize) -> String {
    let mut buf = [0u16; 80];
    // SAFETY: buffer 80 karakter wide; GCS_VERBW menulis string wide.
    let ok = unsafe { cm.GetCommandString(id, GCS_VERBW, None, PSTR(buf.as_mut_ptr() as *mut u8), 80) }.is_ok();
    if !ok {
        return String::new();
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n]).to_lowercase()
}

fn selection_menu(sf: &IShellFolder, pidls: &Pidls) -> Option<IContextMenu> {
    // SAFETY: pidl anak Desktop yang hidup selama pemanggilan.
    unsafe { sf.GetUIObjectOf::<IContextMenu>(desk_hwnd(), &pidls.arr(), None) }.ok()
}

/// Jalankan verb pada pilihan (delete, copy, cut, properties) tanpa menampilkan menu.
fn verb_on_selection(verb: &str) {
    let pidls = selected_pidls();
    if pidls.0.is_empty() {
        return;
    }
    let Some(sf) = desktop_folder() else { return };
    let Some(cm) = selection_menu(&sf, &pidls) else { return };
    // SAFETY: menu sementara untuk QueryContextMenu (sebagian handler butuh sebelum InvokeCommand).
    unsafe {
        if let Ok(hm) = CreatePopupMenu() {
            let _ = cm.QueryContextMenu(hm, 0, CM_FIRST, CM_LAST, CMF_NORMAL);
            let ok = invoke_verb(&cm, verb);
            let _ = DestroyMenu(hm);
            if !ok {
                log(&format!("desk: verb {verb} gagal"));
            }
        }
    }
}

/// Menu klik kanan untuk ikon terpilih: menu shell asli. "Ganti nama" dan "Buka" folder ditangani di sini.
fn item_menu(pt: POINT, extended: bool) {
    let pidls = selected_pidls();
    if pidls.0.is_empty() {
        return;
    }
    let Some(sf) = desktop_folder() else { return };
    let Some(cm) = selection_menu(&sf, &pidls) else { return };
    let Ok(hm) = (unsafe { CreatePopupMenu() }) else { return };
    let flags = CMF_NORMAL | CMF_CANRENAME | if extended { CMF_EXTENDEDVERBS } else { 0 };
    // SAFETY: hm menu baru; rentang id CM_FIRST..CM_LAST.
    if unsafe { cm.QueryContextMenu(hm, 0, CM_FIRST, CM_LAST, flags) }.is_err() {
        let _ = unsafe { DestroyMenu(hm) };
        return;
    }
    let cmd = track_shell_menu(hm, pt, Some(cm.clone()));
    let _ = unsafe { DestroyMenu(hm) };
    if cmd < CM_FIRST as i32 || cmd > CM_LAST as i32 {
        return;
    }
    let id = (cmd as u32 - CM_FIRST) as usize;
    let verb = command_verb(&cm, id);
    let sel = selected_indices();
    match verb.as_str() {
        "rename" => {
            let f = DESK_STATE.borrow().focus;
            let i = if f >= 0 && sel.contains(&(f as usize)) { Some(f as usize) } else { sel.first().copied() };
            if let Some(i) = i {
                begin_rename(i);
            }
        }
        // Folder dibuka di win12files (di mode shell tidak ada Explorer).
        "open" | "explore" | "opennewwindow" if sel.len() == 1 && DESK_STATE.borrow().icons[sel[0]].is_folder() => {
            open_icon(sel[0]);
        }
        _ => {
            if !invoke_id(&cm, id, Some(pt)) {
                log(&format!("desk: perintah menu {id} ({verb}) gagal"));
            }
        }
    }
}

/// IContextMenu atau IDropTarget folder Desktop pengguna sebagai item (untuk Tempel dan jatuhkan ke latar).
fn desktop_item_ui<T: Interface>() -> Option<T> {
    // SAFETY: pidl absolut dibebaskan di akhir; `last` menunjuk ke dalamnya dan dipakai sebelum ILFree.
    unsafe {
        let abs = SHGetKnownFolderIDList(&FOLDERID_Desktop, 0, None).ok()?;
        let mut last: *mut ITEMIDLIST = std::ptr::null_mut();
        let out = SHBindToParent::<IShellFolder>(abs, Some(&mut last))
            .ok()
            .and_then(|parent| parent.GetUIObjectOf::<T>(desk_hwnd(), &[last as *const ITEMIDLIST], None).ok());
        ILFree(Some(abs));
        out
    }
}

fn can_paste() -> bool {
    // SAFETY: tanpa pointer.
    unsafe {
        let idl = RegisterClipboardFormatW(w!("Shell IDList Array"));
        IsClipboardFormatAvailable(CF_HDROP).is_ok() || (idl != 0 && IsClipboardFormatAvailable(idl).is_ok())
    }
}

fn paste(verb: &str) {
    let Some(cm) = desktop_item_ui::<IContextMenu>() else { return };
    // SAFETY: menu sementara untuk QueryContextMenu.
    unsafe {
        if let Ok(hm) = CreatePopupMenu() {
            let _ = cm.QueryContextMenu(hm, 0, CM_FIRST, CM_LAST, CMF_NORMAL);
            if !invoke_verb(&cm, verb) {
                log(&format!("desk: {verb} gagal"));
            }
            let _ = DestroyMenu(hm);
        }
    }
}

/// Menu "Baru" (CLSID_NewMenu) untuk folder Desktop pengguna.
fn new_menu() -> Option<IContextMenu> {
    // SAFETY: objek COM in-proc; pidl folder dibebaskan setelah Initialize (NewMenu menyalinnya).
    unsafe {
        let cm: IContextMenu = CoCreateInstance(&CLSID_NewMenu, None::<&IUnknown>, CLSCTX_INPROC_SERVER).ok()?;
        let abs = SHGetKnownFolderIDList(&FOLDERID_Desktop, 0, None).ok()?;
        if let Ok(init) = cm.cast::<IShellExtInit>() {
            let _ = init.Initialize(Some(abs as *const ITEMIDLIST), None::<&IDataObject>, None);
        }
        ILFree(Some(abs));
        Some(cm)
    }
}

const ID_VIEW_LARGE: i32 = 60;
const ID_VIEW_MEDIUM: i32 = 61;
const ID_VIEW_SMALL: i32 = 62;
const ID_VIEW_SHOW: i32 = 63;
const ID_PASTE: i32 = 65;
const ID_PASTE_LINK: i32 = 66;

/// Menu klik kanan latar desktop: Tampilan, Segarkan, Tempel, Baru, wallpaper, Personalisasi, dll.
pub fn desk_background_menu(pt: POINT) {
    let Ok(hm) = (unsafe { CreatePopupMenu() }) else { return };
    let size = desk_icon_size();
    let hidden = desk_icons_hidden();
    let chk = |on: bool| if on { MF_STRING | MF_CHECKED } else { MF_STRING };
    // SAFETY: semua menu valid; submenu dimiliki hm dan ikut dilepas DestroyMenu(hm).
    unsafe {
        if let Ok(view) = CreatePopupMenu() {
            let _ = AppendMenuW(view, chk(size == 96), ID_VIEW_LARGE as usize, w!("Ikon besar"));
            let _ = AppendMenuW(view, chk(size == 48), ID_VIEW_MEDIUM as usize, w!("Ikon sedang"));
            let _ = AppendMenuW(view, chk(size == 32), ID_VIEW_SMALL as usize, w!("Ikon kecil"));
            let _ = AppendMenuW(view, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(view, chk(!hidden), ID_VIEW_SHOW as usize, w!("Tampilkan ikon desktop"));
            let _ = AppendMenuW(hm, MF_POPUP, view.0 as usize, w!("Tampilan"));
        }
        let _ = AppendMenuW(hm, MF_STRING, 4, w!("Segarkan"));
        let _ = AppendMenuW(hm, MF_SEPARATOR, 0, PCWSTR::null());
        let pf = if can_paste() { MF_STRING } else { MF_STRING | MF_GRAYED };
        let _ = AppendMenuW(hm, pf, ID_PASTE as usize, w!("Tempel"));
        let _ = AppendMenuW(hm, pf, ID_PASTE_LINK as usize, w!("Tempel pintasan"));
        let _ = AppendMenuW(hm, MF_SEPARATOR, 0, PCWSTR::null());
    }
    let nm = new_menu();
    if let Some(cm) = &nm {
        // SAFETY: NewMenu menyisipkan item "Baru" (dengan submenu) ke hm pada posisi akhir.
        unsafe {
            let pos = GetMenuItemCount(Some(hm)).max(0) as u32;
            if cm.QueryContextMenu(hm, pos, NEW_FIRST, NEW_LAST, CMF_NORMAL).is_ok() {
                let _ = AppendMenuW(hm, MF_SEPARATOR, 0, PCWSTR::null());
            }
        }
    }
    desk_wall_menu_add(hm);
    // SAFETY: hm valid.
    unsafe {
        let _ = AppendMenuW(hm, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(hm, MF_STRING, 5, w!("Kosongkan Tempat Sampah"));
        let _ = AppendMenuW(hm, MF_STRING, 1, w!("Personalisasi"));
        let _ = AppendMenuW(hm, MF_STRING, 2, w!("Pengaturan layar"));
        let _ = AppendMenuW(hm, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(hm, MF_STRING, 3, w!("Menu Win12 Shell"));
    }
    let c = track_shell_menu(hm, pt, nm.clone());
    let _ = unsafe { DestroyMenu(hm) };
    if c >= NEW_FIRST as i32 && c <= NEW_LAST as i32 {
        if let Some(cm) = &nm {
            DESK_STATE.borrow_mut().pending_new = Some(Instant::now());
            if !invoke_id(cm, (c as u32 - NEW_FIRST) as usize, Some(pt)) {
                log("desk: item baru gagal dibuat");
            }
        }
        return;
    }
    if desk_wall_cmd(c) {
        return;
    }
    match c {
        ID_VIEW_LARGE => desk_set_icon_size(96),
        ID_VIEW_MEDIUM => desk_set_icon_size(48),
        ID_VIEW_SMALL => desk_set_icon_size(32),
        ID_VIEW_SHOW => desk_icons_set_hidden(!hidden),
        ID_PASTE => paste("paste"),
        ID_PASTE_LINK => paste("pastelink"),
        1 => shell_open("ms-settings:personalization"),
        2 => shell_open("ms-settings:display"),
        3 => main_menu(MSG.get(), pt, false),
        4 => desk_icons_reload(),
        5 => desk_recycle_empty(),
        _ => {}
    }
}

/// Kosongkan Tempat Sampah. Flag 0: Windows menampilkan konfirmasi dan progres sendiri.
pub fn desk_recycle_empty() {
    let h = desk_hwnd();
    let owner = (!h.0.is_null()).then_some(h);
    // SAFETY: root null = semua drive.
    let r = unsafe { SHEmptyRecycleBinW(owner, PCWSTR::null(), 0) };
    log(&format!("desk: kosongkan Tempat Sampah -> {}", if r.is_ok() { "ok" } else { "batal/gagal" }));
    schedule_reload(200);
}

// ---------------------------------------------------------------- buka

fn open_icon(i: usize) {
    let (path, folder) = {
        let st = DESK_STATE.borrow();
        let Some(ic) = st.icons.get(i) else { return };
        (ic.path.clone(), ic.is_folder())
    };
    if folder {
        // Folder (juga virtual: This PC, Tempat Sampah) dibuka win12files tanpa Explorer.
        let p = match path.strip_prefix("::") {
            Some(rest) => format!("shell:::{rest}"),
            None => path,
        };
        open_folder_window(&p);
    } else if !path.starts_with("::") {
        shell_open(&path);
    } else {
        // Item virtual bukan folder (mis. Control Panel): verb bawaan dari menu shell.
        let single = {
            let st = DESK_STATE.borrow();
            // SAFETY: salinan pidl dimiliki Pidls.
            Pidls(vec![unsafe { ILClone(st.icons[i].pidl) }])
        };
        let Some(sf) = desktop_folder() else { return };
        let Some(cm) = selection_menu(&sf, &single) else { return };
        // SAFETY: menu sementara.
        unsafe {
            if let Ok(hm) = CreatePopupMenu() {
                if cm.QueryContextMenu(hm, 0, CM_FIRST, CM_LAST, CMF_DEFAULTONLY).is_ok() {
                    let def = GetMenuDefaultItem(hm, 0, GET_MENU_DEFAULT_ITEM_FLAGS(0));
                    if def != u32::MAX && def >= CM_FIRST {
                        invoke_id(&cm, (def - CM_FIRST) as usize, None);
                    }
                }
                let _ = DestroyMenu(hm);
            }
        }
    }
}

// ---------------------------------------------------------------- ganti nama

/// Pesan ke jendela desktop: selesai ganti nama (wParam 1 = simpan).
pub const WM_DESK_RENAME_END: u32 = WM_APP + 0x3D2;

fn begin_rename(i: usize) {
    if DESK_STATE.borrow().rename.is_some() {
        return;
    }
    let h = desk_hwnd();
    let Some(sf) = desktop_folder() else { return };
    let (pidl, text, rc, font, folder) = {
        let st = DESK_STATE.borrow();
        let (Some(m), Some(ic)) = (st.m, st.icons.get(i)) else { return };
        // SAFETY: pidl ikon valid; salinannya dimiliki Rename.
        let pidl = unsafe { ILClone(ic.pidl) };
        let text = display_name(&sf, ic.pidl, SHGDN_INFOLDER | SHGDN_FOREDITING);
        let top = ic.y + m.pad + m.icon_px + m.pad;
        let rc = RECT { left: ic.x + 1, top, right: ic.x + m.cell_w - 1, bottom: top + 3 * m.line_h + 6 };
        (pidl, text, rc, m.font, ic.is_folder())
    };
    if pidl.is_null() || text.is_empty() {
        unsafe { ILFree(Some(pidl)) };
        return;
    }
    let wt = to_wide_z(&text);
    // ES_CENTER | ES_MULTILINE | ES_AUTOVSCROLL
    let style = WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(0x1 | 0x4 | 0x40);
    // SAFETY: kotak edit anak jendela desktop; dilepas end_rename.
    let edit = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            pcw(&wt),
            style,
            rc.left,
            rc.top,
            rc.right - rc.left,
            rc.bottom - rc.top,
            Some(h),
            None,
            Some(HI.get()),
            None,
        )
    };
    let Ok(edit) = edit else {
        unsafe { ILFree(Some(pidl)) };
        return;
    };
    // Pilih nama tanpa ekstensi, seperti Explorer.
    let stem = if folder { None } else { text.rfind('.').filter(|&p| p > 0) };
    let end = stem.map(|p| text[..p].encode_utf16().count() as isize).unwrap_or(-1);
    // SAFETY: edit valid; subclass dilepas di WM_NCDESTROY.
    unsafe {
        SendMessageW(edit, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        SendMessageW(edit, 0x00B1, Some(WPARAM(0)), Some(LPARAM(end))); // EM_SETSEL
        let _ = SetWindowSubclass(edit, Some(rename_proc), 1, 0);
        let _ = SetFocus(Some(edit));
    }
    DESK_STATE.borrow_mut().rename = Some(Rename { edit, pidl, original: text });
    invalidate_icon(i as i32);
}

unsafe extern "system" fn rename_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    let desk = desk_hwnd();
    match m {
        WM_GETDLGCODE => return LRESULT(0x4), // DLGC_WANTALLKEYS
        WM_KEYDOWN if wp.0 == VK_RETURN.0 as usize || wp.0 == VK_ESCAPE.0 as usize => {
            let save = wp.0 == VK_RETURN.0 as usize;
            let _ = unsafe { PostMessageW(Some(desk), WM_DESK_RENAME_END, WPARAM(save as usize), LPARAM(0)) };
            return LRESULT(0);
        }
        WM_CHAR if matches!(wp.0, 0x0D | 0x0A | 0x1B) => return LRESULT(0),
        WM_KILLFOCUS => {
            let _ = unsafe { PostMessageW(Some(desk), WM_DESK_RENAME_END, WPARAM(1), LPARAM(0)) };
        }
        WM_NCDESTROY => {
            let _ = unsafe { RemoveWindowSubclass(h, Some(rename_proc), 1) };
        }
        _ => {}
    }
    unsafe { DefSubclassProc(h, m, wp, lp) }
}

fn end_rename(save: bool) {
    let Some(r) = DESK_STATE.borrow_mut().rename.take() else { return };
    if save {
        // SAFETY: edit masih hidup (dihancurkan di bawah).
        let text = unsafe {
            let n = GetWindowTextLengthW(r.edit).max(0) as usize;
            let mut buf = vec![0u16; n + 1];
            let got = GetWindowTextW(r.edit, &mut buf).max(0) as usize;
            String::from_utf16_lossy(&buf[..got])
        };
        let text = text.trim().to_string();
        if !text.is_empty() && text != r.original {
            if let Some(sf) = desktop_folder() {
                let wt = to_wide_z(&text);
                // SAFETY: pidl salinan milik Rename; nama berakhir NUL.
                let res = unsafe {
                    sf.SetNameOf(Some(desk_hwnd()), r.pidl, pcw(&wt), SHGDNF(SHGDN_INFOLDER | SHGDN_FOREDITING), None)
                };
                if res.is_err() {
                    log("desk: ganti nama gagal");
                }
            }
        }
    }
    // SAFETY: edit dan pidl milik Rename.
    unsafe {
        let _ = DestroyWindow(r.edit);
        ILFree(Some(r.pidl));
        let _ = SetFocus(Some(desk_hwnd()));
    }
    invalidate_all();
    schedule_reload(150);
}

fn cancel_rename() {
    end_rename(false);
}

// ---------------------------------------------------------------- pemberitahuan shell

fn ensure_notify() {
    let h = desk_hwnd();
    if h.0.is_null() || DESK_STATE.borrow().notify_id != 0 {
        return;
    }
    let mut entries: Vec<SHChangeNotifyEntry> = Vec::new();
    // SAFETY: pidl dibiarkan hidup selama proses (SHChangeNotifyRegister merujuknya); hanya tiga.
    unsafe {
        for id in [&FOLDERID_Desktop, &FOLDERID_PublicDesktop] {
            if let Ok(p) = SHGetKnownFolderIDList(id, 0, None) {
                entries.push(SHChangeNotifyEntry { pidl: p, fRecursive: false.into() });
            }
        }
        let w = to_wide_z(&format!("::{RECYCLE_CLSID}"));
        let mut p: *mut ITEMIDLIST = std::ptr::null_mut();
        if SHParseDisplayName(pcw(&w), None, &mut p, 0, None).is_ok() && !p.is_null() {
            entries.push(SHChangeNotifyEntry { pidl: p, fRecursive: false.into() });
        }
        if entries.is_empty() {
            return;
        }
        // SHCNRF_InterruptLevel | SHCNRF_ShellLevel: perubahan dari proses mana pun ikut terlapor.
        let id = SHChangeNotifyRegister(h, SHCNRF_SOURCE(0x1 | 0x2), SHCNE_ALLEVENTS, WM_DESK_NOTIFY, entries.len() as i32, entries.as_ptr());
        if id == 0 {
            log("desk: SHChangeNotifyRegister gagal; desktop diperbarui lewat Segarkan/F5");
        }
        DESK_STATE.borrow_mut().notify_id = id;
    }
}

fn on_notify(wp: WPARAM, lp: LPARAM) {
    let event = lp.0 as i32;
    let recent = DESK_STATE.borrow().pending_new.is_some_and(|t| t.elapsed().as_secs() < 8);
    if recent && event & (SHCNE_CREATE | SHCNE_MKDIR) != 0 && wp.0 != 0 {
        // Tanpa SHCNRF_NewDelivery: wParam menunjuk dua pidl absolut, berlaku selama pesan ini.
        let first = unsafe { *(wp.0 as *const *const ITEMIDLIST) };
        if !first.is_null() {
            let mut buf = [0u16; 260];
            // SAFETY: first valid selama pemrosesan pesan; buf 260 karakter.
            if unsafe { SHGetPathFromIDListW(first, &mut buf) }.as_bool() {
                let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                let mut st = DESK_STATE.borrow_mut();
                st.rename_path = Some(String::from_utf16_lossy(&buf[..n]));
                st.pending_new = None;
            }
        }
    }
    schedule_reload(250);
}

// ---------------------------------------------------------------- seret dan jatuhkan

fn ensure_ole() -> bool {
    if let Some(v) = DESK_STATE.borrow().ole_ok {
        return v;
    }
    // SAFETY: thread utama STA (RoInitialize); OleInitialize menambah hitungan referensi, tidak dilepas.
    let ok = unsafe { OleInitialize(None) }.is_ok();
    if !ok {
        log("desk: OleInitialize gagal; seret-lepas desktop nonaktif");
    }
    DESK_STATE.borrow_mut().ole_ok = Some(ok);
    ok
}

fn start_drag() {
    let pidls = selected_pidls();
    if pidls.0.is_empty() || !ensure_ole() {
        return;
    }
    let Some(sf) = desktop_folder() else { return };
    // SAFETY: pidl salinan hidup selama pemanggilan.
    let Ok(data) = (unsafe { sf.GetUIObjectOf::<IDataObject>(desk_hwnd(), &pidls.arr(), None) }) else { return };
    // SHDoDragDrop menyediakan sumber seret dan gambar seret bawaan shell.
    let _ = unsafe {
        SHDoDragDrop(
            Some(desk_hwnd()),
            &data,
            None::<&windows::Win32::System::Ole::IDropSource>,
            DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK,
        )
    };
}

struct DropSt {
    data: Option<IDataObject>,
    cur: Option<IDropTarget>,
    /// Ikon target (-1 = latar desktop).
    cur_idx: i32,
    helper: Option<IDropTargetHelper>,
}

#[implement(IDropTarget)]
struct DeskDrop {
    st: RefCell<DropSt>,
}

fn client_pt(pt: &POINTL) -> POINT {
    let mut p = POINT { x: pt.x, y: pt.y };
    // SAFETY: jendela desktop milik kita.
    let _ = unsafe { ScreenToClient(desk_hwnd(), &mut p) };
    p
}

/// Target untuk titik ini: ikon yang menerima jatuhan (folder, aplikasi, Tempat Sampah), selain itu latar.
fn target_at(pt: &POINTL) -> (i32, Option<IDropTarget>) {
    let p = client_pt(pt);
    if let Some(i) = hit(p.x, p.y) {
        let (pidl, sel) = {
            let st = DESK_STATE.borrow();
            let ic = &st.icons[i];
            // SAFETY: salinan pidl dimiliki Pidls.
            (Pidls(vec![unsafe { ILClone(ic.pidl) }]), ic.selected)
        };
        // Item yang sedang diseret tidak menjadi target dirinya sendiri.
        if !sel {
            if let Some(sf) = desktop_folder() {
                // SAFETY: pidl valid selama pemanggilan.
                if let Ok(t) = unsafe { sf.GetUIObjectOf::<IDropTarget>(desk_hwnd(), &pidl.arr(), None) } {
                    return (i as i32, Some(t));
                }
            }
        }
    }
    (-1, desktop_item_ui::<IDropTarget>())
}

fn drop_hover(idx: i32) {
    let old = {
        let mut st = DESK_STATE.borrow_mut();
        std::mem::replace(&mut st.hover, idx)
    };
    if old != idx {
        invalidate_icon(old);
        invalidate_icon(idx);
    }
}

impl IDropTarget_Impl for DeskDrop_Impl {
    fn DragEnter(
        &self,
        pdataobj: windows_core::Ref<'_, IDataObject>,
        grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows_core::Result<()> {
        let data: Option<IDataObject> = pdataobj.ok().ok().cloned();
        let (idx, t) = target_at(pt);
        let mut st = self.st.borrow_mut();
        if st.helper.is_none() {
            st.helper = unsafe { CoCreateInstance(&CLSID_DragDropHelper, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }.ok();
        }
        // SAFETY: pdweffect dari OLE, valid selama panggilan.
        unsafe {
            let mut eff = *pdweffect;
            if let (Some(t), Some(d)) = (&t, &data) {
                if t.DragEnter(d, grfkeystate, *pt, &mut eff).is_err() {
                    eff = DROPEFFECT_NONE;
                }
            } else {
                eff = DROPEFFECT_NONE;
            }
            if let (Some(hp), Some(d)) = (&st.helper, &data) {
                let p = POINT { x: pt.x, y: pt.y };
                let _ = hp.DragEnter(desk_hwnd(), d, &p, eff);
            }
            *pdweffect = eff;
        }
        st.data = data;
        st.cur = t;
        st.cur_idx = idx;
        drop(st);
        drop_hover(if idx >= 0 { idx } else { -1 });
        Ok(())
    }

    fn DragOver(&self, grfkeystate: MODIFIERKEYS_FLAGS, pt: &POINTL, pdweffect: *mut DROPEFFECT) -> windows_core::Result<()> {
        let (idx, t) = target_at(pt);
        let mut st = self.st.borrow_mut();
        // SAFETY: pdweffect dari OLE.
        unsafe {
            let mut eff = *pdweffect;
            if idx != st.cur_idx {
                if let Some(old) = st.cur.take() {
                    let _ = old.DragLeave();
                }
                st.cur = t;
                st.cur_idx = idx;
                match (&st.cur, &st.data) {
                    (Some(c), Some(d)) => {
                        if c.DragEnter(d, grfkeystate, *pt, &mut eff).is_err() {
                            eff = DROPEFFECT_NONE;
                        }
                    }
                    _ => eff = DROPEFFECT_NONE,
                }
            } else if let Some(c) = &st.cur {
                if c.DragOver(grfkeystate, *pt, &mut eff).is_err() {
                    eff = DROPEFFECT_NONE;
                }
            } else {
                eff = DROPEFFECT_NONE;
            }
            if let Some(hp) = &st.helper {
                let p = POINT { x: pt.x, y: pt.y };
                let _ = hp.DragOver(&p, eff);
            }
            *pdweffect = eff;
        }
        drop(st);
        drop_hover(if idx >= 0 { idx } else { -1 });
        Ok(())
    }

    fn DragLeave(&self) -> windows_core::Result<()> {
        let mut st = self.st.borrow_mut();
        // SAFETY: objek COM valid.
        unsafe {
            if let Some(c) = st.cur.take() {
                let _ = c.DragLeave();
            }
            if let Some(hp) = &st.helper {
                let _ = hp.DragLeave();
            }
        }
        st.data = None;
        st.cur_idx = -1;
        drop(st);
        drop_hover(-1);
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: windows_core::Ref<'_, IDataObject>,
        grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows_core::Result<()> {
        let data: Option<IDataObject> = pdataobj.ok().ok().cloned();
        let (cur, helper) = {
            let mut st = self.st.borrow_mut();
            st.data = None;
            st.cur_idx = -1;
            (st.cur.take(), st.helper.clone())
        };
        // SAFETY: pdweffect dari OLE.
        unsafe {
            let mut eff = *pdweffect;
            match (&cur, &data) {
                (Some(c), Some(d)) => {
                    if c.Drop(d, grfkeystate, *pt, &mut eff).is_err() {
                        eff = DROPEFFECT_NONE;
                    }
                }
                _ => eff = DROPEFFECT_NONE,
            }
            if let (Some(hp), Some(d)) = (&helper, &data) {
                let p = POINT { x: pt.x, y: pt.y };
                let _ = hp.Drop(d, &p, eff);
            }
            *pdweffect = eff;
        }
        drop_hover(-1);
        schedule_reload(300);
        Ok(())
    }
}

fn ensure_drop_target() {
    let h = desk_hwnd();
    if h.0.is_null() || DROP_TARGET.with(|d| d.borrow().is_some()) || !ensure_ole() {
        return;
    }
    let t: IDropTarget = DeskDrop {
        st: RefCell::new(DropSt { data: None, cur: None, cur_idx: -1, helper: None }),
    }
    .into();
    // SAFETY: h jendela desktop milik kita; dicabut di desk_icons_clear.
    if unsafe { RegisterDragDrop(h, &t) }.is_ok() {
        DROP_TARGET.with(|d| *d.borrow_mut() = Some(t));
    } else {
        log("desk: RegisterDragDrop gagal");
    }
}

// ---------------------------------------------------------------- pesan jendela desktop

fn lparam_pt(lp: LPARAM) -> POINT {
    POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 }
}

fn to_screen(p: POINT) -> POINT {
    let mut s = p;
    let _ = unsafe { ClientToScreen(desk_hwnd(), &mut s) };
    s
}

fn icon_center(i: usize) -> POINT {
    let st = DESK_STATE.borrow();
    match (st.m, st.icons.get(i)) {
        (Some(m), Some(ic)) => POINT { x: ic.x + m.cell_w / 2, y: ic.y + m.cell_h / 2 },
        _ => POINT { x: 0, y: 0 },
    }
}

fn on_lbutton_down(p: POINT, mods: u32) {
    let h = desk_hwnd();
    if DESK_STATE.borrow().rename.is_some() {
        end_rename(true);
    }
    // SAFETY: jendela milik kita.
    unsafe {
        let _ = SetFocus(Some(h));
    }
    let ctrl = mods & 0x8 != 0;
    let shift = mods & 0x4 != 0;
    if let Some(i) = hit(p.x, p.y) {
        if ctrl {
            apply_selection(|k, s| if k == i { !s } else { s });
            set_focus_anchor(i as i32, true);
        } else if shift {
            let a = DESK_STATE.borrow().anchor;
            let a = if a < 0 { i } else { a as usize };
            let (lo, hi) = (a.min(i), a.max(i));
            apply_selection(|k, _| k >= lo && k <= hi);
            set_focus_anchor(i as i32, false);
        } else if !DESK_STATE.borrow().icons[i].selected {
            select_only(i);
        } else {
            set_focus_anchor(i as i32, true);
        }
        DESK_STATE.borrow_mut().press = Some(Press { pt: p, idx: i, mods });
    } else {
        if !ctrl {
            clear_selection();
        }
        let base: Vec<bool> = DESK_STATE.borrow().icons.iter().map(|i| i.selected).collect();
        DESK_STATE.borrow_mut().marquee = Some(Marquee { start: p, cur: p, base });
    }
    unsafe { SetCapture(h) };
}

fn on_mouse_move(p: POINT, mods: u32) {
    let h = desk_hwnd();
    // Ambang seret dari ikon.
    let drag = {
        let st = DESK_STATE.borrow();
        st.press.as_ref().map(|pr| {
            let cx = unsafe { GetSystemMetrics(SM_CXDRAG) };
            let cy = unsafe { GetSystemMetrics(SM_CYDRAG) };
            (p.x - pr.pt.x).abs() > cx || (p.y - pr.pt.y).abs() > cy
        })
    };
    if let Some(moved) = drag {
        if moved {
            DESK_STATE.borrow_mut().press = None;
            let _ = unsafe { ReleaseCapture() };
            start_drag();
        }
        return;
    }
    let mq = {
        let mut st = DESK_STATE.borrow_mut();
        st.marquee.as_mut().map(|mq| {
            let old = norm_rect(mq.start, mq.cur);
            mq.cur = p;
            (old, norm_rect(mq.start, mq.cur), mq.base.clone())
        })
    };
    if let Some((old, new, base)) = mq {
        let ctrl = mods & 0x8 != 0;
        let cells: Vec<RECT> = {
            let st = DESK_STATE.borrow();
            let m = st.m;
            st.icons
                .iter()
                .map(|ic| match m {
                    Some(m) => RECT { left: ic.x + 2, top: ic.y + 2, right: ic.x + m.cell_w - 2, bottom: ic.y + m.cell_h },
                    None => RECT::default(),
                })
                .collect()
        };
        let inter = |a: &RECT, b: &RECT| a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
        apply_selection(|k, _| {
            let hitk = inter(&cells[k], &new);
            let b = base.get(k).copied().unwrap_or(false);
            if ctrl { b ^ hitk } else { b || hitk }
        });
        let u = RECT {
            left: old.left.min(new.left) - 1,
            top: old.top.min(new.top) - 1,
            right: old.right.max(new.right) + 1,
            bottom: old.bottom.max(new.bottom) + 1,
        };
        invalidate_rect(&u);
        return;
    }
    let new = hit(p.x, p.y).map_or(-1, |i| i as i32);
    let (old, track) = {
        let mut st = DESK_STATE.borrow_mut();
        let old = std::mem::replace(&mut st.hover, new);
        let track = !st.tracking_leave;
        st.tracking_leave = true;
        (old, track)
    };
    if old != new {
        invalidate_icon(old);
        invalidate_icon(new);
    }
    if track {
        let mut tme = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: h,
            dwHoverTime: 0,
        };
        let _ = unsafe { TrackMouseEvent(&mut tme) };
    }
}

fn on_lbutton_up() {
    let (press, mq) = {
        let mut st = DESK_STATE.borrow_mut();
        (st.press.take(), st.marquee.take())
    };
    if let Some(pr) = press {
        if pr.mods & (0x8 | 0x4) == 0 {
            select_only(pr.idx);
        }
    }
    if let Some(mq) = mq {
        let r = norm_rect(mq.start, mq.cur);
        invalidate_rect(&RECT { left: r.left - 1, top: r.top - 1, right: r.right + 1, bottom: r.bottom + 1 });
    }
    let _ = unsafe { ReleaseCapture() };
}

fn move_focus(dx: i32, dy: i32) {
    let n = DESK_STATE.borrow().icons.len() as i32;
    if n == 0 {
        return;
    }
    let rows = rows_per_column();
    let f = DESK_STATE.borrow().focus;
    let next = if f < 0 { 0 } else { (f + dy + dx * rows).clamp(0, n - 1) };
    select_only(next as usize);
}

fn on_key(vk: u32) -> bool {
    let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
    match vk {
        v if v == VK_F5.0 as u32 => desk_icons_reload(),
        v if v == VK_F2.0 as u32 => {
            let f = DESK_STATE.borrow().focus;
            let sel = selected_indices();
            let i = if f >= 0 && sel.contains(&(f as usize)) { Some(f as usize) } else { sel.first().copied() };
            if let Some(i) = i {
                begin_rename(i);
            }
        }
        v if v == VK_RETURN.0 as u32 => {
            for i in selected_indices().into_iter().take(15) {
                open_icon(i);
            }
        }
        v if v == VK_DELETE.0 as u32 => verb_on_selection("delete"),
        v if v == VK_UP.0 as u32 => move_focus(0, -1),
        v if v == VK_DOWN.0 as u32 => move_focus(0, 1),
        v if v == VK_LEFT.0 as u32 => move_focus(-1, 0),
        v if v == VK_RIGHT.0 as u32 => move_focus(1, 0),
        0x41 if ctrl => apply_selection(|_, _| true),
        0x43 if ctrl => verb_on_selection("copy"),
        0x58 if ctrl => verb_on_selection("cut"),
        0x56 if ctrl => paste("paste"),
        _ => return false,
    }
    true
}

/// Pesan jendela desktop yang ditangani modul ini (mouse, keyboard, menu, ganti nama, pemberitahuan).
/// `None` = teruskan ke desk_proc/DefWindowProc.
pub fn desk_icons_on_message(_h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    if let Some(r) = menu_msg(m, wp, lp) {
        return Some(r);
    }
    match m {
        WM_LBUTTONDOWN => on_lbutton_down(lparam_pt(lp), wp.0 as u32),
        WM_MOUSEMOVE => on_mouse_move(lparam_pt(lp), wp.0 as u32),
        WM_LBUTTONUP => on_lbutton_up(),
        WM_CAPTURECHANGED => {
            let mq = {
                let mut st = DESK_STATE.borrow_mut();
                st.press = None;
                st.marquee.take()
            };
            if mq.is_some() {
                invalidate_all();
            }
        }
        0x02A3 => {
            // WM_MOUSELEAVE
            let old = {
                let mut st = DESK_STATE.borrow_mut();
                st.tracking_leave = false;
                std::mem::replace(&mut st.hover, -1)
            };
            invalidate_icon(old);
        }
        WM_LBUTTONDBLCLK => {
            let p = lparam_pt(lp);
            if let Some(i) = hit(p.x, p.y) {
                select_only(i);
                open_icon(i);
            }
        }
        WM_RBUTTONDOWN => {
            let p = lparam_pt(lp);
            let _ = unsafe { SetFocus(Some(desk_hwnd())) };
            if let Some(i) = hit(p.x, p.y) {
                if !DESK_STATE.borrow().icons[i].selected {
                    select_only(i);
                }
            }
        }
        WM_RBUTTONUP => {
            let p = lparam_pt(lp);
            let shift = wp.0 as u32 & 0x4 != 0;
            if let Some(i) = hit(p.x, p.y) {
                if !DESK_STATE.borrow().icons[i].selected {
                    select_only(i);
                }
                item_menu(to_screen(p), shift);
            } else {
                clear_selection();
                desk_background_menu(to_screen(p));
            }
        }
        WM_CONTEXTMENU => {
            // Hanya dari keyboard (tombol menu / Shift+F10); klik kanan ditangani WM_RBUTTONUP.
            if lp.0 as i32 != -1 {
                return Some(LRESULT(0));
            }
            let sel = selected_indices();
            let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
            if let Some(&i) = sel.first() {
                item_menu(to_screen(icon_center(i)), shift);
            } else {
                let wa = work_area();
                desk_background_menu(POINT { x: wa.left + 40, y: wa.top + 40 });
            }
        }
        WM_KEYDOWN => {
            if !on_key(wp.0 as u32) {
                return None;
            }
        }
        WM_SYSKEYDOWN if wp.0 == VK_RETURN.0 as usize => verb_on_selection("properties"),
        WM_DESK_NOTIFY => on_notify(wp, lp),
        WM_DESK_RENAME_END => end_rename(wp.0 != 0),
        WM_TIMER if wp.0 == T_DESK_RELOAD => {
            let _ = unsafe { KillTimer(Some(desk_hwnd()), T_DESK_RELOAD) };
            desk_icons_reload();
        }
        WM_DPICHANGED => {
            schedule_reload(100);
            return None;
        }
        WM_SETTINGCHANGE => {
            // Font judul ikon, metrik ikon, atau ShellState (sembunyikan ekstensi) berubah.
            let w = wp.0 as u32;
            if w == SPI_SETICONTITLELOGFONT.0 || w == SPI_SETICONMETRICS.0 || w == 0 {
                schedule_reload(300);
            }
            return None;
        }
        _ => return None,
    }
    Some(LRESULT(0))
}

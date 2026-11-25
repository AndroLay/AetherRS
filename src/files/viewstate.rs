//! Tampilan per folder (2026-11-19, permintaan pengguna) dan kolom Detail yang mengisi lebar tampilan.
//!
//! Seperti Explorer/Dolphin, tiap folder mengingat mode tampilan, ukuran ikon, urutan, dan pengelompokan.
//! Disimpan di `HKCU\Software\Win12Shell\Files\Views`, nama nilai = jalur parsing folder (huruf kecil).
//! Folder yang belum pernah diubah memakai bawaan jenis foldernya (FVM_AUTO), jadi This PC tampil sebagai ubin
//! drive dengan bilah kapasitas dan folder Gambar sebagai ikon, sama seperti Explorer.
//!
//! Format nilai: `v1|<mode>|<ukuran>|g:<fmtid hex>,<pid>,<naik 0/1>|s:<fmtid>,<pid>,<arah>;...`

use windows::core::{Interface, GUID, PWSTR};
use windows::Win32::Foundation::{HWND, PROPERTYKEY, RECT};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IColumnManager, IFolderView2, IPersistFolder2, IShellView, SHGetNameFromIDList, CM_COLUMNINFO, CM_ENUM_VISIBLE,
    CM_MASK_WIDTH, FOLDERVIEWMODE, FVM_DETAILS, SIGDN_DESKTOPABSOLUTEPARSING, SORTCOLUMN, SORTDIRECTION,
};
use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, SM_CXVSCROLL};

use crate::files::state::FmTab;

const VIEWS_KEY: &str = r"Software\Win12Shell\Files\Views";
/// Timer jendela: isi ulang lebar kolom sesaat setelah navigasi (DefView kadang memasang lebar belakangan).
pub(crate) const FM_TIMER_FILL: usize = 8;
/// FVM_AUTO (-1): biarkan folder memilih tampilan bawaan jenisnya.
pub(crate) const FVM_AUTO_U32: u32 = 0xFFFF_FFFF;
/// FWF_NOHEADERINALLVIEWS: header kolom hanya di Detail.
pub(crate) const FWF_NOHEADERINALLVIEWS_U32: u32 = 0x1000_0000;

/// Kunci folder: jalur parsing (C:\..., ::{CLSID} untuk This PC, dst.), huruf kecil.
pub(crate) fn key_of_pidl(pidl: *const ITEMIDLIST) -> Option<String> {
    if pidl.is_null() {
        return None;
    }
    // SAFETY: pidl valid selama pemanggilan; string hasil dibebaskan dengan CoTaskMemFree.
    unsafe {
        let p: PWSTR = SHGetNameFromIDList(pidl, SIGDN_DESKTOPABSOLUTEPARSING).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.filter(|s| !s.is_empty()).map(|s| s.to_lowercase())
    }
}

fn key_of_view(sv: &IShellView) -> Option<String> {
    let fv: IFolderView2 = sv.cast().ok()?;
    // SAFETY: GetCurFolder mengembalikan PIDL milik pemanggil (dibebaskan di bawah).
    unsafe {
        let pf: IPersistFolder2 = fv.GetFolder().ok()?;
        let pidl = pf.GetCurFolder().ok()?;
        let k = key_of_pidl(pidl);
        CoTaskMemFree(Some(pidl as *const _));
        k
    }
}

fn pk_str(k: &PROPERTYKEY) -> String {
    format!("{:032x},{}", k.fmtid.to_u128(), k.pid)
}

fn pk_parse(s: &str) -> Option<PROPERTYKEY> {
    let (g, p) = s.split_once(',')?;
    Some(PROPERTYKEY { fmtid: GUID::from_u128(u128::from_str_radix(g, 16).ok()?), pid: p.parse().ok()? })
}

/// Keadaan tampilan sekarang sebagai teks (None bila tampilan belum siap).
pub(crate) fn capture(sv: &IShellView) -> Option<String> {
    let fv: IFolderView2 = sv.cast().ok()?;
    let mut vm = FOLDERVIEWMODE(0);
    let mut size = 0i32;
    // SAFETY: keluaran ke variabel lokal.
    unsafe { fv.GetViewModeAndIconSize(&mut vm, &mut size) }.ok()?;
    let mut out = format!("v1|{}|{}", vm.0, size);
    let mut gk = PROPERTYKEY::default();
    let mut asc = windows::core::BOOL(1);
    // SAFETY: keluaran ke variabel lokal.
    if unsafe { fv.GetGroupBy(&mut gk, Some(&mut asc)) }.is_ok() && gk.fmtid != GUID::zeroed() {
        out.push_str(&format!("|g:{},{}", pk_str(&gk), asc.0 != 0));
    }
    let n = unsafe { fv.GetSortColumnCount() }.unwrap_or(0).clamp(0, 4) as usize;
    if n > 0 {
        let mut cols = vec![SORTCOLUMN::default(); n];
        // SAFETY: buffer berukuran n sesuai GetSortColumnCount.
        if unsafe { fv.GetSortColumns(&mut cols) }.is_ok() {
            let s: Vec<String> = cols.iter().map(|c| format!("{},{}", pk_str(&c.propkey), c.direction.0)).collect();
            out.push_str(&format!("|s:{}", s.join(";")));
        }
    }
    Some(out)
}

/// Mode tampilan dari teks tersimpan (untuk SetFolderSettings sebelum tampilan baru dibuat).
pub(crate) fn saved_mode(s: &str) -> Option<u32> {
    let mut it = s.split('|');
    if it.next()? != "v1" {
        return None;
    }
    it.next()?.parse::<i32>().ok().map(|v| v as u32)
}

/// Terapkan keadaan tersimpan ke tampilan.
pub(crate) fn apply(sv: &IShellView, s: &str) {
    let Ok(fv) = sv.cast::<IFolderView2>() else { return };
    let parts: Vec<&str> = s.split('|').collect();
    if parts.len() < 3 || parts[0] != "v1" {
        return;
    }
    let (Ok(vm), Ok(size)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>()) else { return };
    // SAFETY: nilai yang sebelumnya dibaca dari DefView sendiri.
    let _ = unsafe { fv.SetViewModeAndIconSize(FOLDERVIEWMODE(vm), size) };
    for p in &parts[3..] {
        if let Some(g) = p.strip_prefix("g:") {
            if let Some((k, a)) = g.rsplit_once(',') {
                if let Some(pk) = pk_parse(k) {
                    let _ = unsafe { fv.SetGroupBy(&pk, a == "true") };
                }
            }
        } else if let Some(list) = p.strip_prefix("s:") {
            let cols: Vec<SORTCOLUMN> = list
                .split(';')
                .filter_map(|c| {
                    let (k, d) = c.rsplit_once(',')?;
                    Some(SORTCOLUMN { propkey: pk_parse(k)?, direction: SORTDIRECTION(d.parse().ok()?) })
                })
                .collect();
            if !cols.is_empty() {
                let _ = unsafe { fv.SetSortColumns(&cols) };
            }
        }
    }
}

/// Keadaan tersimpan untuk folder `key`.
pub(crate) fn load(key: &str) -> Option<String> {
    crate::reg::get_string(VIEWS_KEY, key).filter(|s| s.starts_with("v1|"))
}

/// Simpan keadaan tampilan tab bila berubah sejak folder dibuka (dipanggil sebelum pergi / tab ditutup).
pub(crate) fn save(t: &FmTab) {
    let Some(sv) = crate::files::app::fm_current_view(t) else { return };
    let (Some(key), Some(cur)) = (key_of_view(&sv), capture(&sv)) else { return };
    if t.searching || cur == t.vs_init {
        return;
    }
    let _ = crate::reg::set_string(VIEWS_KEY, &key, &cur);
}

/// Kolom Detail mengisi lebar tampilan seperti Dolphin: sisa ruang ditambahkan ke kolom Nama.
/// Bila pengguna mengubah lebar Nama sendiri, lebar itu jadi dasar baru; saat jendela mengecil, tambahan dikurangi lagi.
pub(crate) fn fill_columns(t: &mut FmTab) {
    let Some(sv) = crate::files::app::fm_current_view(t) else { return };
    let Ok(fv) = sv.cast::<IFolderView2>() else { return };
    let mut vm = FOLDERVIEWMODE(0);
    let mut size = 0i32;
    if unsafe { fv.GetViewModeAndIconSize(&mut vm, &mut size) }.is_err() || vm != FVM_DETAILS {
        return;
    }
    let Ok(cm) = sv.cast::<IColumnManager>() else { return };
    let Ok(hv) = (unsafe { sv.GetWindow() }) else { return };
    let n = unsafe { cm.GetColumnCount(CM_ENUM_VISIBLE) }.unwrap_or(0) as usize;
    if n == 0 {
        return;
    }
    let mut keys = vec![PROPERTYKEY::default(); n];
    if unsafe { cm.GetColumns(CM_ENUM_VISIBLE, &mut keys) }.is_err() {
        return;
    }
    let width_of = |k: &PROPERTYKEY| -> i32 {
        let mut ci = CM_COLUMNINFO { cbSize: size_of::<CM_COLUMNINFO>() as u32, dwMask: CM_MASK_WIDTH.0 as u32, ..Default::default() };
        // SAFETY: ci berukuran benar; hanya lebar yang diminta.
        if unsafe { cm.GetColumnInfo(k, &mut ci) }.is_ok() {
            ci.uWidth as i32
        } else {
            0
        }
    };
    let widths: Vec<i32> = keys.iter().map(width_of).collect();
    let name = widths[0];
    let others: i32 = widths[1..].iter().sum();
    let mut rc = RECT::default();
    let _ = unsafe { GetClientRect(hv, &mut rc) };
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hv) }.max(96);
    // ruang batang gulir tegak + sedikit sela agar batang gulir mendatar tidak muncul
    let avail = rc.right - rc.left - unsafe { GetSystemMetricsForDpi(SM_CXVSCROLL, dpi) } - (8 * dpi as i32 / 96);
    if avail <= 0 {
        return;
    }
    let base = if name == t.fill_w { name - t.fill_extra } else { name };
    let extra = (avail - others - base).max(0);
    let want = base + extra;
    t.fill_extra = extra;
    t.fill_w = want;
    if want != name {
        let mut ci = CM_COLUMNINFO {
            cbSize: size_of::<CM_COLUMNINFO>() as u32,
            dwMask: CM_MASK_WIDTH.0 as u32,
            uWidth: want.max(0) as u32,
            ..Default::default()
        };
        // SAFETY: ci berukuran benar; hanya lebar yang diubah.
        let _ = unsafe { cm.SetColumnInfo(&keys[0], &mut ci) };
    }
}

/// Jadwalkan pengisian kolom sesaat lagi (setelah navigasi, ubah ukuran, atau ganti mode).
pub(crate) fn fill_soon(h: HWND) {
    // SAFETY: timer pada jendela frame milik thread ini.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(Some(h), FM_TIMER_FILL, 120, None);
    }
}

use std::mem::size_of;

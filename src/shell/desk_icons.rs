//! Ikon desktop untuk mode shell (jendela "Progman" Rust, `core_c3b::desk_proc`).
//!
//! Isi: file dan folder dari folder Desktop pengguna dan Public Desktop (SHGetKnownFolderPath), disusun grid
//! mulai dari pojok kiri atas area kerja. Klik memilih, klik ganda membuka (`shell_open`).
//! Dimuat saat start (`shell_mode_start`), saat WM_DISPLAYCHANGE (tata letak saja), dan lewat menu "Segarkan".
//! Tanpa pemantau folder (ReadDirectoryChangesW): pemantau itu pernah membocorkan IRP dan membuat freeze
//! (catatan 2026-11-13 di w12filesearch.inc). Tidak ada seret-lepas, pilihan ganda, atau menu konteks per ikon.
//! C++ menggambar ikon desktop lewat Explorer; mode shell tidak menjalankan Explorer, jadi modul ini mengisinya.

use super::hotkeys::shell_open;
use super::prelude::*;
use std::ffi::c_void;
use std::mem::{replace, size_of, zeroed};
use std::os::windows::fs::MetadataExt;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    SHGetFileInfoW, SHGetKnownFolderPath, FOLDERID_Desktop, FOLDERID_PublicDesktop, KF_FLAG_DEFAULT, SHFILEINFOW,
    SHGFI_FLAGS, SHGFI_ICON, SHGFI_LARGEICON,
};

/// Lebar dan tinggi satu sel grid (ikon + label dua baris).
const CELL_W: i32 = 96;
const CELL_H: i32 = 92;
/// Ikon besar shell (SHGFI_LARGEICON) selalu 32 px.
const ICON_PX: i32 = 32;
/// Jarak dari tepi area kerja.
const MARGIN: i32 = 12;
/// Batas jumlah ikon, supaya grid tidak meluap jauh dari layar.
const MAX_ICONS: usize = 150;
/// FILE_ATTRIBUTE_HIDDEN (0x2). Item tersembunyi tidak ditampilkan, seperti Explorer.
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
/// Aksen #0078D4 dalam urutan COLORREF (0x00BBGGRR).
const ACCENT: u32 = 0x00D4_7800;

struct DeskIcon {
    label: String,
    path: String,
    /// Dimiliki modul ini; di-DestroyIcon saat dimuat ulang atau saat mode shell berhenti.
    hicon: HICON,
    /// Posisi pojok kiri atas sel, koordinat klien jendela desktop.
    x: i32,
    y: i32,
}

/// Ikon yang sedang tampil.
static DESK_ICONS: SyncCell<RefCell<Vec<DeskIcon>>> = SyncCell::new(RefCell::new(Vec::new()));
/// Indeks ikon yang terpilih, -1 bila tidak ada.
static DESK_SEL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));

/// Folder desktop: milik pengguna dulu, lalu Public Desktop. Folder yang gagal diambil dilewati.
fn desktop_dirs() -> Vec<String> {
    let mut out = Vec::new();
    for id in [&FOLDERID_Desktop, &FOLDERID_PublicDesktop] {
        if let Ok(p) = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) } {
            // SAFETY: p adalah string COM yang dialokasikan shell; dibebaskan dengan CoTaskMemFree di bawah.
            let s = unsafe { from_pcwstr(p.0) };
            unsafe { CoTaskMemFree(Some(p.0 as *const c_void)) };
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    out
}

/// Tambahkan isi satu folder ke `out` sebagai (label, path). Item tersembunyi, desktop.ini, dan Thumbs.db dilewati.
fn scan_dir(dir: &str, out: &mut Vec<(String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let Ok(meta) = ent.metadata() else {
            continue;
        };
        if meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
            continue;
        }
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("thumbs.db") {
            continue;
        }
        let path = ent.path();
        // Explorer menyembunyikan ekstensi file yang dikenal (HideFileExt); folder tampil dengan nama penuh.
        let label = if meta.is_file() {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| name.clone())
        } else {
            name.clone()
        };
        out.push((label, path.to_string_lossy().into_owned()));
    }
}

/// Ikon besar shell untuk satu path. Mengembalikan HICON nol bila gagal; hasil tidak nol wajib di-DestroyIcon.
fn icon_for(path: &str) -> HICON {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: SHFILEINFOW adalah struct C POD; nol adalah nilai awal yang valid.
    let mut sfi: SHFILEINFOW = unsafe { zeroed() };
    let flags = SHGFI_FLAGS(SHGFI_ICON.0 | SHGFI_LARGEICON.0);
    // SAFETY: wide berakhir NUL dan hidup selama pemanggilan; sfi ditulis oleh shell.
    let got = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut sfi),
            size_of::<SHFILEINFOW>() as u32,
            flags,
        )
    };
    if got == 0 {
        return HICON::default();
    }
    sfi.hIcon
}

/// Area kerja layar utama (di atas bilah Win12 karena shell_work_area), dalam koordinat layar.
fn work_area() -> RECT {
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: rc hidup selama pemanggilan; ukuran RECT sesuai untuk SPI_GETWORKAREA.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rc as *mut RECT as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_err() {
        // SAFETY: GetSystemMetrics tanpa parameter pointer.
        rc.right = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        rc.bottom = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    }
    rc
}

/// Tata letak grid dari pojok kiri atas area kerja, lalu ke kanan. Koordinat klien jendela desktop
/// mulai di pojok virtual screen, jadi posisi area kerja dikurangi SM_XVIRTUALSCREEN/SM_YVIRTUALSCREEN.
pub fn desk_icons_layout() {
    let wa = work_area();
    // SAFETY: GetSystemMetrics tanpa parameter pointer.
    let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    // SAFETY: sama seperti di atas.
    let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let ox = wa.left - vx + MARGIN;
    let oy = wa.top - vy + MARGIN;
    let rows = (((wa.bottom - wa.top) - 2 * MARGIN) / CELL_H).max(1);
    let mut v = DESK_ICONS.borrow_mut();
    for (i, ic) in v.iter_mut().enumerate() {
        let i = i as i32;
        ic.x = ox + (i / rows) * CELL_W;
        ic.y = oy + (i % rows) * CELL_H;
    }
}

/// Minta gambar ulang jendela desktop (bila ada).
fn invalidate() {
    let h = DESK.get();
    if !h.0.is_null() {
        // SAFETY: h adalah jendela desktop milik kita.
        let _ = unsafe { InvalidateRect(Some(h), None, true) };
    }
}

/// Lepaskan semua HICON dari daftar lama.
fn destroy_all(old: Vec<DeskIcon>) {
    for ic in old {
        if !ic.hicon.0.is_null() {
            // SAFETY: HICON ini dibuat SHGetFileInfoW dan belum pernah dilepas.
            let _ = unsafe { DestroyIcon(ic.hicon) };
        }
    }
}

/// Muat ulang daftar dari folder desktop, buat ulang ikon, lalu gambar ulang. Dipanggil dari shell_mode_start
/// dan menu "Segarkan".
pub fn desk_icons_reload() {
    let mut found: Vec<(String, String)> = Vec::new();
    for d in desktop_dirs() {
        scan_dir(&d, &mut found);
    }
    found.sort_by_key(|(label, _)| label.to_lowercase());
    found.truncate(MAX_ICONS);
    let fresh: Vec<DeskIcon> = found
        .into_iter()
        .map(|(label, path)| DeskIcon {
            hicon: icon_for(&path),
            label,
            path,
            x: 0,
            y: 0,
        })
        .collect();
    let n = fresh.len();
    let old = replace(&mut *DESK_ICONS.borrow_mut(), fresh);
    destroy_all(old);
    DESK_SEL.set(-1);
    desk_icons_layout();
    invalidate();
    log(&format!("desk: {n} ikon desktop dimuat"));
}

/// Kosongkan ikon dan lepaskan HICON-nya. Dipanggil dari shell_mode_stop.
pub fn desk_icons_clear() {
    let old = replace(&mut *DESK_ICONS.borrow_mut(), Vec::new());
    destroy_all(old);
    DESK_SEL.set(-1);
}

/// Bingkai aksen 2 px di sekeliling sel terpilih.
fn frame_accent(dc: HDC, cell: &RECT) {
    let inner = RECT {
        left: cell.left + 1,
        top: cell.top + 1,
        right: cell.right - 1,
        bottom: cell.bottom - 1,
    };
    // SAFETY: dc adalah HDC yang valid dari WM_PAINT; brush dibuat dan dihapus di fungsi ini.
    unsafe {
        let br = CreateSolidBrush(COLORREF(ACCENT));
        let _ = FrameRect(dc, cell, br);
        let _ = FrameRect(dc, &inner, br);
        let _ = DeleteObject(HGDIOBJ(br.0));
    }
}

/// Gambar ikon dan label. Dipanggil dari WM_PAINT desk_proc, setelah PaintDesktop.
pub fn desk_icons_paint(dc: HDC) {
    // Hitung ulang tata letak tiap gambar: area kerja bisa berubah (bilah Win12) tanpa pergantian layar.
    // Dipanggil sebelum borrow() agar tidak bertabrakan dengan borrow_mut() di desk_icons_layout.
    desk_icons_layout();
    let items = DESK_ICONS.borrow();
    if items.is_empty() {
        return;
    }
    let sel = DESK_SEL.get();
    let text_fmt = DRAW_TEXT_FORMAT(DT_CENTER.0 | DT_WORDBREAK.0 | DT_NOPREFIX.0);
    // SAFETY: dc adalah HDC yang valid dari WM_PAINT; objek font dipilih lalu dikembalikan.
    unsafe {
        let old_font = SelectObject(dc, GetStockObject(DEFAULT_GUI_FONT));
        SetBkMode(dc, TRANSPARENT);
        for (i, ic) in items.iter().enumerate() {
            if !ic.hicon.0.is_null() {
                let ix = ic.x + (CELL_W - ICON_PX) / 2;
                let _ = DrawIconEx(dc, ix, ic.y + 6, ic.hicon, ICON_PX, ICON_PX, 0, None, DI_NORMAL);
            }
            let mut buf: Vec<u16> = ic.label.encode_utf16().collect();
            let mut tr = RECT {
                left: ic.x + 2,
                top: ic.y + 6 + ICON_PX + 4,
                right: ic.x + CELL_W - 2,
                bottom: ic.y + CELL_H,
            };
            // Bayangan 1 px hitam lalu teks putih, supaya terbaca di atas wallpaper terang.
            let mut sh = RECT {
                left: tr.left + 1,
                top: tr.top + 1,
                right: tr.right + 1,
                bottom: tr.bottom + 1,
            };
            SetTextColor(dc, COLORREF(0x0000_0000));
            let _ = DrawTextW(dc, &mut buf, &mut sh, text_fmt);
            SetTextColor(dc, COLORREF(0x00FF_FFFF));
            let _ = DrawTextW(dc, &mut buf, &mut tr, text_fmt);
            if sel == i as i32 {
                let cell = RECT {
                    left: ic.x,
                    top: ic.y,
                    right: ic.x + CELL_W,
                    bottom: ic.y + CELL_H,
                };
                frame_accent(dc, &cell);
            }
        }
        SelectObject(dc, old_font);
    }
}

/// Klik pada desktop: pilih ikon di bawah kursor (atau hapus pilihan bila di luar ikon). Klik ganda membuka ikon.
/// `x` dan `y` adalah koordinat klien. Dipanggil dari desk_proc untuk WM_LBUTTONDOWN dan WM_LBUTTONDBLCLK.
pub fn desk_icons_click(x: i32, y: i32, dbl: bool) {
    let hit = DESK_ICONS
        .borrow()
        .iter()
        .position(|ic| x >= ic.x && x < ic.x + CELL_W && y >= ic.y && y < ic.y + CELL_H);
    let new_sel = hit.map_or(-1, |i| i as i32);
    if DESK_SEL.get() != new_sel {
        DESK_SEL.set(new_sel);
        invalidate();
    }
    if dbl {
        let target = match hit {
            Some(i) => DESK_ICONS.borrow().get(i).map(|ic| ic.path.clone()),
            None => None,
        };
        if let Some(p) = target {
            shell_open(&p);
        }
    }
}

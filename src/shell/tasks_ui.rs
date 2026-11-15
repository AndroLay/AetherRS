//! Sumber: w12tasks.inc (seluruh file). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H12.
use super::prelude::*;

use std::ffi::c_void;
use std::sync::atomic::Ordering;

use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGR, IWICBitmapSource, IWICImagingFactory,
    IWICPalette, WICBitmapCacheOnLoad, WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant,
    WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand, WICRect,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH};

// ================================================================ wallpaper desktop
// Gaya seperti Pengaturan Windows (HKCU\Control Panel\Desktop): 10 Isi, 6 Paskan, 2 Rentangkan, 0 Tengah,
// 22 Bentang (satu gambar melintasi semua layar), TileWallpaper=1 berarti Ubin.
// Warna latar: Control Panel\Colors\Background.

const DESK_KEY: &str = "Control Panel\\Desktop";
const COLORS_KEY: &str = "Control Panel\\Colors";
const ZERO_RECT: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };

/// Pekerjaan wallpaper untuk thread latar (C++ `WallJob`). Tanpa pointer mentah agar aman dikirim ke thread.
struct WallJob {
    gen: i32,
    style: i32,
    nmon: usize,
    /// Warna latar sebagai piksel 0x00RRGGBB.
    bg: u32,
    virt: RECT,
    mon: [RECT; 8],
    path: String,
}

/// ID menu klik kanan desktop: "Ganti wallpaper..." dan awal ID submenu gaya.
pub const DW_PICK: i32 = 40;
pub const DW_STYLE: i32 = 41;

/// Pilihan gaya di submenu (C++ `kWallStyles`).
const WALL_STYLES: [(i32, &str); 6] = [
    (10, "Isi"),
    (6, "Paskan"),
    (2, "Rentangkan"),
    (0, "Tengah"),
    (-1, "Ubin"),
    (22, "Bentang (semua layar)"),
];

/// Seperti `_wtoi`: angka di depan string (spasi awal dilewati), 0 bila tidak ada.
fn wtoi(s: &str) -> i32 {
    let t = s.trim_start();
    let mut end = 0usize;
    for (i, c) in t.char_indices() {
        let sign = i == 0 && (c == '-' || c == '+');
        if !(c.is_ascii_digit() || sign) {
            break;
        }
        end = i + c.len_utf8();
    }
    t[..end].parse::<i32>().unwrap_or(0)
}

unsafe extern "system" fn wall_mon_proc(m: HMONITOR, _dc: HDC, _rc: *mut RECT, lp: LPARAM) -> BOOL {
    let j = &mut *(lp.0 as *mut WallJob);
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        rcMonitor: ZERO_RECT,
        rcWork: ZERO_RECT,
        dwFlags: 0,
    };
    if j.nmon < 8 && GetMonitorInfoW(m, &mut mi).as_bool() {
        j.mon[j.nmon] = mi.rcMonitor;
        j.nmon += 1;
    }
    BOOL(1)
}

/// Gaya dari registri. -1 berarti Ubin (WallpaperStyle 0 dan TileWallpaper 1).
fn desk_wall_style() -> i32 {
    let st = reg::get_string(DESK_KEY, "WallpaperStyle").map(|v| wtoi(&v)).unwrap_or(0);
    let tile = reg::get_string(DESK_KEY, "TileWallpaper").map(|v| wtoi(&v)).unwrap_or(0);
    if st == 0 && tile == 1 {
        -1
    } else {
        st
    }
}

/// Mengisi `j`: path, gaya, warna latar, layar virtual, dan daftar monitor.
fn desk_wall_read(j: &mut WallJob) {
    let mut path = reg::get_string(DESK_KEY, "WallPaper").unwrap_or_default();
    if path.is_empty() {
        let mut buf = [0u16; MAX_PATH as usize];
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETDESKWALLPAPER,
                MAX_PATH,
                Some(buf.as_mut_ptr() as *mut c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .is_ok();
        if ok {
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            path = from_wide(&buf[..end]);
        }
    }
    // File asli hilang (path ada tetapi tidak bisa dibuka): pakai salinan milik Windows.
    if !path.is_empty() && !std::path::Path::new(&path).exists() {
        if let Ok(ad) = std::env::var("APPDATA") {
            path = format!("{ad}\\Microsoft\\Windows\\Themes\\TranscodedWallpaper");
        }
    }
    j.path = path;
    j.style = desk_wall_style();

    let c = reg::get_string(COLORS_KEY, "Background").unwrap_or_default();
    let mut rgb = [0i32; 3];
    for (i, t) in c.split_whitespace().take(3).enumerate() {
        match t.parse::<i32>() {
            Ok(v) => rgb[i] = v,
            Err(_) => break,
        }
    }
    // Makro RGB() di C memotong tiap komponen ke byte.
    j.bg = ((rgb[0] as u8 as u32) << 16) | ((rgb[1] as u8 as u32) << 8) | (rgb[2] as u8 as u32);

    let (x, y, cx, cy) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    j.virt = RECT { left: x, top: y, right: x + cx, bottom: y + cy };
    j.nmon = 0;
    if j.style != 22 {
        let lp = LPARAM(j as *mut WallJob as isize);
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(wall_mon_proc), lp);
        }
    }
    if j.nmon == 0 {
        j.nmon = 1;
        j.mon[0] = j.virt;
    }
}

/// Menyalin bagian gambar yang tampak ke DIB pada koordinat layar virtual.
/// `d` = (lebar, tinggi, dx, dy) gambar hasil skala terhadap monitor `m`.
fn wall_copy(
    src: &IWICBitmapSource,
    d: (i32, i32, i32, i32),
    m: &RECT,
    virt: &RECT,
    base: *mut u8,
    stride: i32,
) {
    let (dw, dh, dx, dy) = d;
    let mw = m.right - m.left;
    let mh = m.bottom - m.top;
    let x0 = if dx < 0 { -dx } else { 0 };
    let y0 = if dy < 0 { -dy } else { 0 };
    let x1 = dw.min(mw - dx);
    let y1 = dh.min(mh - dy);
    if x1 > x0 && y1 > y0 {
        let r = WICRect { X: x0, Y: y0, Width: x1 - x0, Height: y1 - y0 };
        let len = (stride * (r.Height - 1) + r.Width * 4) as usize;
        let off = ((m.top - virt.top + dy + y0) as isize) * (stride as isize)
            + ((m.left - virt.left + dx + x0) as isize) * 4;
        let buf = unsafe { std::slice::from_raw_parts_mut(base.offset(off), len) };
        let _ = unsafe { src.CopyPixels(&r as *const WICRect, stride as u32, buf) };
    }
}

/// Skala gambar sesuai gaya (bila perlu), lalu salin ke satu monitor. Skala gagal berarti monitor dilewati.
fn wall_place(
    f: &IWICImagingFactory,
    src: &IWICBitmapSource,
    sw: u32,
    sh: u32,
    style: i32,
    m: &RECT,
    virt: &RECT,
    base: *mut u8,
    stride: i32,
) {
    let mw = m.right - m.left;
    let mh = m.bottom - m.top;
    let (mut dw, mut dh, mut dx, mut dy) = (0i32, 0i32, 0i32, 0i32);
    w12_wall_rect(style, sw as i32, sh as i32, mw, mh, &mut dw, &mut dh, &mut dx, &mut dy);
    if dw != sw as i32 || dh != sh as i32 {
        let Ok(scl) = (unsafe { f.CreateBitmapScaler() }) else {
            return;
        };
        if unsafe { scl.Initialize(src, dw as u32, dh as u32, WICBitmapInterpolationModeFant) }
            .is_err()
        {
            return;
        }
        wall_copy(&scl, (dw, dh, dx, dy), m, virt, base, stride);
    } else {
        wall_copy(src, (dw, dh, dx, dy), m, virt, base, stride);
    }
}

/// Mendekode gambar wallpaper ke DIB. Bila ada tahap yang gagal, DIB tetap berisi warna latar.
fn wall_decode(job: &WallJob, base: *mut u8, vw: i32, vh: i32, stride: i32) {
    if job.path.is_empty() {
        return;
    }
    let f: IWICImagingFactory = match unsafe {
        CoCreateInstance(&CLSID_WICImagingFactory, None::<&IUnknown>, CLSCTX_INPROC_SERVER)
    } {
        Ok(v) => v,
        Err(_) => return,
    };
    let wpath = to_wide_z(&job.path);
    let Ok(dec) = (unsafe {
        f.CreateDecoderFromFilename(
            pcw(&wpath),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )
    }) else {
        return;
    };
    let Ok(fr) = (unsafe { dec.GetFrame(0) }) else {
        return;
    };
    let fsrc: &IWICBitmapSource = &fr;
    let Ok(cv) = (unsafe { f.CreateFormatConverter() }) else {
        return;
    };
    let init = unsafe {
        cv.Initialize(
            fsrc,
            &GUID_WICPixelFormat32bppBGR,
            WICBitmapDitherTypeNone,
            None::<&IWICPalette>,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
    };
    if init.is_err() {
        return;
    }
    let (mut sw, mut sh) = (0u32, 0u32);
    if unsafe { cv.GetSize(&mut sw, &mut sh) }.is_err() || sw == 0 || sh == 0 {
        return;
    }
    let src: &IWICBitmapSource = &cv;

    if job.style == -1 {
        // Ubin: sumber dibuat bitmap sekali, lalu ditempel berulang dari pojok kiri atas layar virtual.
        let Ok(mem) = (unsafe { f.CreateBitmapFromSource(src, WICBitmapCacheOnLoad) }) else {
            return;
        };
        let (sw_i, sh_i) = (sw as i32, sh as i32);
        let mut ty = 0i32;
        while ty < vh {
            let mut tx = 0i32;
            while tx < vw {
                let r = WICRect {
                    X: 0,
                    Y: 0,
                    Width: sw_i.min(vw - tx),
                    Height: sh_i.min(vh - ty),
                };
                let len = (stride * (r.Height - 1) + r.Width * 4) as usize;
                let off = (ty * stride + tx * 4) as isize;
                let buf = unsafe { std::slice::from_raw_parts_mut(base.offset(off), len) };
                let _ = unsafe { mem.CopyPixels(&r as *const WICRect, stride as u32, buf) };
                tx += sw_i;
            }
            ty += sh_i;
        }
    } else {
        for m in &job.mon[..job.nmon] {
            wall_place(&f, src, sw, sh, job.style, m, &job.virt, base, stride);
        }
    }
}

/// Membuat DIB 32 bit top-down seukuran layar virtual lalu mengisinya. None bila DIB gagal dibuat.
fn wall_render(job: &WallJob) -> Option<HBITMAP> {
    let vw = job.virt.right - job.virt.left;
    let vh = job.virt.bottom - job.virt.top;
    if vw <= 0 || vh <= 0 {
        return None;
    }
    // BITMAPINFO di windows 0.61 tidak derive Default; struct POD ini valid di-zero-kan.
    let mut bi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = vw;
    bi.bmiHeader.biHeight = -vh;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bmp = unsafe { CreateDIBSection(None, &bi, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?;
    let n = (vw as usize) * (vh as usize);
    let stride = vw * 4;
    let base = bits as *mut u8;

    // Latar dulu, lalu gambar, lalu alpha penuh.
    {
        let px = unsafe { std::slice::from_raw_parts_mut(bits as *mut u32, n) };
        px.fill(job.bg);
    }
    wall_decode(job, base, vw, vh, stride);
    {
        let px = unsafe { std::slice::from_raw_parts_mut(bits as *mut u32, n) };
        for p in px.iter_mut() {
            *p |= 0xFF00_0000;
        }
    }
    unsafe {
        let _ = GdiFlush();
    }
    Some(bmp)
}

/// Isi thread latar: render wallpaper lalu kirim HBITMAP ke jendela tasks lewat WM_W12_WALL.
fn wall_thread(job: Box<WallJob>) {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
    let com_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let bmp = wall_render(&job);
    // HBITMAP dikirim sebagai isize; pemilik akhirnya jendela tasks (desk_wall_apply).
    let lp = bmp.map(|b| b.0 as isize).unwrap_or(0);
    let tk = TK_MSG.get();
    let posted = !tk.0.is_null()
        && unsafe {
            PostMessageW(Some(tk), WM_W12_WALL, WPARAM(job.gen as u32 as usize), LPARAM(lp))
        }
        .is_ok();
    if !posted {
        if let Some(b) = bmp {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(b.0));
            }
        }
    }
    if com_ok {
        unsafe { CoUninitialize() };
    }
}

/// Memulai pemuatan wallpaper di thread latar. Generasi baru membuat hasil generasi lama diabaikan.
pub fn desk_wall_load() {
    if DESK.get().0.is_null() || TK_MSG.get().0.is_null() {
        return;
    }
    let mut job = Box::new(WallJob {
        gen: 0,
        style: 0,
        nmon: 0,
        bg: 0,
        virt: ZERO_RECT,
        mon: [ZERO_RECT; 8],
        path: String::new(),
    });
    desk_wall_read(&mut job);
    // InterlockedIncrement mengembalikan nilai baru = nilai lama + 1.
    job.gen = WALL_GEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    // Bila spawn gagal, closure (dan job) dibuang; sama dengan free(j) di C++.
    let _ = std::thread::Builder::new().spawn(move || wall_thread(job));
}

/// Menerapkan hasil thread latar ke jendela desktop bila generasinya masih terbaru.
pub fn desk_wall_apply(wp: WPARAM, b: HBITMAP) {
    let desk = DESK.get();
    let gen_ok = (wp.0 as u32 as i32) == WALL_GEN.load(Ordering::SeqCst);
    if !b.0.is_null() && gen_ok && !desk.0.is_null() {
        unsafe {
            let mut bm = BITMAP::default();
            GetObjectW(
                HGDIOBJ(b.0),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut BITMAP as *mut c_void),
            );
            let dc = CreateCompatibleDC(None);
            let old = SelectObject(dc, HGDIOBJ(b.0));
            let dst = POINT {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            };
            let sz = SIZE { cx: bm.bmWidth, cy: bm.bmHeight };
            let src = POINT { x: 0, y: 0 };
            let bf = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                desk,
                None,
                Some(&dst),
                Some(&sz),
                Some(dc),
                Some(&src),
                COLORREF(0),
                Some(&bf),
                ULW_ALPHA,
            );
            SelectObject(dc, old);
            let _ = DeleteDC(dc);
            let _ = SetWindowPos(
                desk,
                Some(HWND_BOTTOM),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
    if !b.0.is_null() {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(b.0));
        }
    }
    trim_soon();
}

/// Menulis pilihan ke registri (path bila ada, gaya bila bukan -100), lalu memberi tahu sistem dan memuat ulang.
pub fn desk_wall_set(path: Option<&str>, style: i32) {
    if let Some(p) = path {
        reg::set_string(DESK_KEY, "WallPaper", p);
    }
    if style != -100 {
        let st = style.max(0);
        reg::set_string(DESK_KEY, "WallpaperStyle", &st.to_string());
        reg::set_string(DESK_KEY, "TileWallpaper", if style == -1 { "1" } else { "0" });
    }
    let cur = reg::get_string(DESK_KEY, "WallPaper").unwrap_or_default();
    let mut wz = to_wide_z(&cur);
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            Some(wz.as_mut_ptr() as *mut c_void),
            SPIF_SENDCHANGE,
        );
    }
    desk_wall_load();
}

/// Mengubah PWSTR milik COM menjadi String. Pemanggil yang membebaskan memorinya.
unsafe fn pwstr_to_string(p: PWSTR) -> String {
    if p.0.is_null() {
        return String::new();
    }
    let mut n = 0usize;
    while *p.0.add(n) != 0 {
        n += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p.0, n))
}

/// Menambah item wallpaper ke menu klik kanan desktop. Submenu gaya diberi centang pada gaya aktif.
pub fn desk_wall_menu_add(m: HMENU) {
    unsafe {
        let _ = AppendMenuW(m, MF_STRING, DW_PICK as usize, w!("Ganti wallpaper..."));
    }
    let cur = desk_wall_style();
    if let Ok(sub) = unsafe { CreatePopupMenu() } {
        for (i, (style, name)) in WALL_STYLES.iter().enumerate() {
            let wn = to_wide_z(name);
            let flags = if *style == cur { MF_STRING | MF_CHECKED } else { MF_STRING };
            unsafe {
                let _ = AppendMenuW(sub, flags, (DW_STYLE + i as i32) as usize, pcw(&wn));
            }
        }
        unsafe {
            let _ = AppendMenuW(m, MF_POPUP, sub.0 as usize, w!("Posisi wallpaper"));
        }
    }
}

/// Menangani perintah menu wallpaper. Mengembalikan true bila `c` milik wallpaper.
pub fn desk_wall_cmd(c: i32) -> bool {
    if c == DW_PICK {
        let dlg: IFileOpenDialog = match unsafe {
            CoCreateInstance(&FileOpenDialog, None::<&IUnknown>, CLSCTX_INPROC_SERVER)
        } {
            Ok(d) => d,
            Err(_) => return true,
        };
        let filt = [COMDLG_FILTERSPEC {
            pszName: w!("Gambar"),
            pszSpec: w!("*.jpg;*.jpeg;*.png;*.bmp;*.gif;*.tif;*.tiff;*.jxr;*.heic;*.webp;*.avif"),
        }];
        let mut picked: Option<String> = None;
        unsafe {
            let _ = dlg.SetFileTypes(&filt);
            let _ = dlg.SetTitle(w!("Pilih wallpaper"));
            // PORT: IFileDialog::Show menerima Option<HWND> di binding windows 0.61.
            if dlg.Show(Some(DESK.get())).is_ok() {
                if let Ok(si) = dlg.GetResult() {
                    if let Ok(p) = si.GetDisplayName(SIGDN_FILESYSPATH) {
                        picked = Some(pwstr_to_string(p));
                        CoTaskMemFree(Some(p.0 as *const c_void));
                    }
                }
            }
        }
        if let Some(path) = picked {
            desk_wall_set(Some(&path), -100);
        }
        trim_soon();
        return true;
    }
    if c >= DW_STYLE && c < DW_STYLE + WALL_STYLES.len() as i32 {
        desk_wall_set(None, WALL_STYLES[(c - DW_STYLE) as usize].0);
        return true;
    }
    false
}

// ================================================================ desktop virtual
// Jendela desktop lain disembunyikan (ShowWindowAsync SW_HIDE, tidak bisa menggantung pada aplikasi macet) dan
// ditampilkan lagi saat kembali. Daftar yang tersembunyi ditulis ke vdesk-hidden.txt (hwnd + pid) agar dipulihkan
// bila shell berhenti mendadak (oleh shell berikutnya atau win12boot sebelum Explorer cadangan).
// Pemetaan: g_vd->VD, g_vdN->VD_N (state.rs), g_vdCount->VD_COUNT (state.rs), g_vdCur->VD_CUR, g_vdFocus->VD_FOCUS.

/// Satu jendela milik desktop virtual (C++ `VdWin`): `d` = indeks desktop, `hid` = sedang disembunyikan.
#[derive(Clone, Copy)]
pub struct VdWin {
    pub h: HWND,
    pub d: i32,
    pub hid: bool,
}

const VD_EMPTY: VdWin = VdWin { h: HWND(std::ptr::null_mut()), d: 0, hid: false };

/// Daftar jendela desktop virtual; jumlah yang terpakai ada di `VD_N` (state.rs).
pub static VD: SyncCell<RefCell<[VdWin; VD_WINS as usize]>> =
    SyncCell::new(RefCell::new([VD_EMPTY; VD_WINS as usize]));
/// Indeks desktop aktif (C++ `g_vdCur`).
pub static VD_CUR: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
/// Jendela fokus terakhir tiap desktop (C++ `g_vdFocus`).
static VD_FOCUS: SyncCell<RefCell<[HWND; VD_MAX as usize]>> =
    SyncCell::new(RefCell::new([HWND(std::ptr::null_mut()); VD_MAX as usize]));

fn top_window() -> HWND {
    unsafe { GetTopWindow(None) }.unwrap_or_default()
}

fn next_window(w: HWND) -> HWND {
    unsafe { GetWindow(w, GW_HWNDNEXT) }.unwrap_or_default()
}

fn vd_find(h: HWND) -> i32 {
    let n = VD_N.get().clamp(0, VD_WINS) as usize;
    let vd = VD.borrow();
    for i in 0..n {
        if vd[i].h == h {
            return i as i32;
        }
    }
    -1
}

fn vd_add(h: HWND, d: i32, hid: bool) {
    let n = VD_N.get();
    if n >= VD_WINS {
        return;
    }
    VD.borrow_mut()[n as usize] = VdWin { h, d, hid };
    VD_N.set(n + 1);
}

fn vd_path() -> String {
    format!("{}\\vdesk-hidden.txt", DIR.borrow().as_str())
}

/// Tulis daftar jendela tersembunyi (hwnd + pid). Daftar kosong berarti berkas dihapus.
pub fn vd_save() {
    let p = vd_path();
    let mut out = String::new();
    {
        let vd = VD.borrow();
        let n = VD_N.get().clamp(0, VD_WINS) as usize;
        for e in vd.iter().take(n) {
            if !e.hid {
                continue;
            }
            let mut pid: u32 = 0;
            unsafe { GetWindowThreadProcessId(e.h, Some(&mut pid as *mut u32)) };
            out.push_str(&format!("{:x} {}\r\n", e.h.0 as usize, pid));
        }
    }
    if out.is_empty() {
        let _ = std::fs::remove_file(&p);
        return;
    }
    let _ = std::fs::write(&p, out.as_bytes());
}

/// Shell sebelumnya berhenti saat ada jendela desktop lain tersembunyi: tampilkan lagi lalu hapus berkas.
pub fn vd_recover() {
    let p = vd_path();
    let Ok(bytes) = std::fs::read(&p) else {
        return;
    };
    let mut cur: &[u8] = &bytes;
    loop {
        let mut hv: u64 = 0;
        let mut pid: u32 = 0;
        if !w12_vd_parse_next(&mut cur, &mut hv, &mut pid) {
            // baris rusak dilewati; baris sahih sesudahnya tetap dipulihkan
            match cur.iter().position(|&b| b == b'\n') {
                Some(nl) => cur = &cur[nl + 1..],
                None => break,
            }
            continue;
        }
        let h = HWND(hv as usize as *mut c_void);
        let mut wp: u32 = 0;
        unsafe {
            if IsWindow(Some(h)).as_bool()
                && GetWindowThreadProcessId(h, Some(&mut wp as *mut u32)) != 0
                && wp == pid
                && !IsWindowVisible(h).as_bool()
            {
                let _ = ShowWindowAsync(h, SW_SHOWNA);
            }
        }
    }
    let _ = std::fs::remove_file(&p);
}

/// Segarkan daftar: buang jendela yang sudah tertutup, catat jendela tugas baru ke desktop aktif.
pub fn vd_scan() {
    {
        let mut vd = VD.borrow_mut();
        let n = VD_N.get().clamp(0, VD_WINS) as usize;
        let mut j = 0usize;
        for i in 0..n {
            let e = vd[i];
            if unsafe { IsWindow(Some(e.h)) }.as_bool() {
                vd[j] = e;
                j += 1;
            }
        }
        VD_N.set(j as i32);
    }
    let cur = VD_CUR.get();
    let mut w = top_window();
    while !w.0.is_null() {
        if is_task_window(w) {
            let i = vd_find(w);
            if i < 0 {
                vd_add(w, cur, false);
            } else {
                // tampil = milik desktop aktif (mis. aplikasi memunculkan diri sendiri)
                let mut vd = VD.borrow_mut();
                vd[i as usize].d = cur;
                vd[i as usize].hid = false;
            }
        }
        w = next_window(w);
    }
}

/// Sembunyikan jendela desktop `d` beserta dialog miliknya. `only` = batasi ke jendela itu (dan dialognya).
fn vd_hide(d: i32, only: Option<HWND>) {
    // dialog/jendela milik (owned) ikut disembunyikan bersama pemiliknya
    let mut w = top_window();
    while !w.0.is_null() {
        if unsafe { IsWindowVisible(w) }.as_bool() && !is_our_window(w) {
            let o = unsafe { GetWindow(w, GW_OWNER) }.unwrap_or_default();
            let oi = if o.0.is_null() { -1 } else { vd_find(o) };
            if oi >= 0
                && VD.borrow()[oi as usize].d == d
                && only.map_or(true, |x| o == x)
                && vd_find(w) < 0
            {
                vd_add(w, d, false);
            }
        }
        w = next_window(w);
    }
    let n = VD_N.get().clamp(0, VD_WINS) as usize;
    for i in 0..n {
        let e = VD.borrow()[i];
        if e.d != d || e.hid || !unsafe { IsWindowVisible(e.h) }.as_bool() {
            continue;
        }
        if let Some(x) = only {
            if e.h != x && unsafe { GetWindow(e.h, GW_OWNER) }.unwrap_or_default() != x {
                continue;
            }
        }
        unsafe {
            let _ = ShowWindowAsync(e.h, SW_HIDE);
        }
        VD.borrow_mut()[i].hid = true;
    }
}

/// Tampilkan lagi jendela tersembunyi milik desktop `d`.
fn vd_show(d: i32) {
    let n = VD_N.get().clamp(0, VD_WINS) as usize;
    for i in 0..n {
        let e = VD.borrow()[i];
        if e.d == d && e.hid {
            unsafe {
                let _ = ShowWindowAsync(e.h, SW_SHOWNA);
            }
            VD.borrow_mut()[i].hid = false;
        }
    }
}

fn vd_focus_get(i: i32) -> HWND {
    VD_FOCUS.borrow().get(i as usize).copied().unwrap_or_default()
}

fn vd_focus_set(i: i32, h: HWND) {
    if (0..VD_MAX).contains(&i) {
        VD_FOCUS.borrow_mut()[i as usize] = h;
    }
}

/// Fokuskan jendela terakhir di desktop aktif; bila tidak ada, jendela tugas pertama yang tidak minimize.
pub fn vd_focus_now() {
    let mut f = vd_focus_get(VD_CUR.get());
    let unusable = f.0.is_null()
        || !unsafe { IsWindow(Some(f)) }.as_bool()
        || !unsafe { IsWindowVisible(f) }.as_bool()
        || unsafe { IsIconic(f) }.as_bool();
    if unusable {
        f = HWND(std::ptr::null_mut());
        let mut w = top_window();
        while !w.0.is_null() {
            if is_task_window(w) && !unsafe { IsIconic(w) }.as_bool() {
                f = w;
                break;
            }
            w = next_window(w);
        }
    }
    if !f.0.is_null() {
        activate_window(f);
    } else {
        let desk = DESK.get();
        if !desk.0.is_null() {
            let _ = unsafe { SetForegroundWindow(desk) };
        }
    }
    refresh_windows();
}

/// Pindah ke desktop `n`. `focus` = fokuskan jendela desktop tujuan setelah pindah.
pub fn vd_go(n: i32, focus: bool) {
    if n < 0 || n >= VD_COUNT.get() {
        return;
    }
    let cur = VD_CUR.get();
    if n == cur {
        if focus {
            vd_hud();
        }
        return;
    }
    vd_scan();
    let fg = unsafe { GetForegroundWindow() };
    let keep = if vd_find(fg) >= 0 { fg } else { HWND(std::ptr::null_mut()) };
    vd_focus_set(cur, keep);
    vd_hide(cur, None);
    VD_CUR.set(n);
    vd_show(n);
    vd_save();
    if !tv_is_open() {
        vd_hud();
    }
    if focus {
        // setelah ShowWindowAsync selesai diproses aplikasi
        unsafe { SetTimer(Some(TK_MSG.get()), TKT_FOCUS as usize, 90, None) };
    }
}

/// Tambah desktop baru di kanan; `go` = langsung pindah ke sana.
pub fn vd_new(go: bool) {
    let count = VD_COUNT.get();
    if count >= VD_MAX {
        vd_hud();
        return;
    }
    vd_focus_set(count, HWND(std::ptr::null_mut()));
    VD_COUNT.set(count + 1);
    if go {
        vd_go(count, true);
    } else if !tv_is_open() {
        vd_hud();
    }
}

/// Tutup desktop `n`: jendelanya pindah ke desktop di sebelah kirinya (atau kanan bila `n` = 0).
pub fn vd_close(n: i32) {
    let count = VD_COUNT.get();
    if count <= 1 || n < 0 || n >= count {
        return;
    }
    vd_scan();
    let dest = w12_vd_close_dest(n, count);
    if n == VD_CUR.get() {
        vd_go(dest, !tv_is_open());
    }
    let total = VD_N.get().clamp(0, VD_WINS) as usize;
    for i in 0..total {
        let e = VD.borrow()[i];
        if e.d != n {
            continue;
        }
        VD.borrow_mut()[i].d = dest;
        if dest == VD_CUR.get() && e.hid {
            unsafe {
                let _ = ShowWindowAsync(e.h, SW_SHOWNA);
            }
            VD.borrow_mut()[i].hid = false;
        }
    }
    for i in 0..total {
        let d = VD.borrow()[i].d;
        if d > n {
            VD.borrow_mut()[i].d = d - 1;
        }
    }
    for k in n..(count - 1) {
        let v = vd_focus_get(k + 1);
        vd_focus_set(k, v);
    }
    VD_CUR.set(w12_vd_index_after_close(VD_CUR.get(), n));
    VD_COUNT.set(count - 1);
    vd_save();
    if !tv_is_open() {
        vd_hud();
    }
}

/// Pindahkan jendela `h` ke desktop `n`; `n` sama dengan jumlah desktop berarti buat desktop baru.
pub fn vd_move(h: HWND, n: i32) {
    if h.0.is_null() {
        return;
    }
    let count = VD_COUNT.get();
    if n < 0 || n > count {
        return;
    }
    if n == count {
        if count >= VD_MAX {
            return;
        }
        vd_focus_set(count, HWND(std::ptr::null_mut()));
        VD_COUNT.set(count + 1);
    }
    vd_scan();
    let i = vd_find(h);
    if i < 0 {
        return;
    }
    let from = VD.borrow()[i as usize].d;
    if from == n {
        return;
    }
    if from == VD_CUR.get() {
        vd_hide(from, Some(h)); // sembunyikan h (dan dialognya) sebelum dipindah
    }
    let total = VD_N.get().clamp(0, VD_WINS) as usize;
    for k in 0..total {
        let kh = VD.borrow()[k].h;
        let owner = unsafe { GetWindow(kh, GW_OWNER) }.unwrap_or_default();
        if kh == h || owner == h {
            VD.borrow_mut()[k].d = n;
        }
    }
    if n == VD_CUR.get() {
        vd_show(n);
    }
    vd_save();
    unsafe { SetTimer(Some(TK_MSG.get()), TKT_FOCUS as usize, 90, None) };
}

/// HUD singkat "Desktop N" + titik per desktop di tengah atas; tembus klik, hilang sendiri (TKT_HUD).
pub fn vd_hud() {
    let tk = TK_MSG.get();
    if tk.0.is_null() {
        return;
    }
    // PORT: s() menerima f32 (core_c1.rs:253); literal diberi bentuk float.
    let bw = s(220.0);
    let bh = s(66.0);
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let mon = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
        let _ = GetMonitorInfoW(mon, &mut mi);
    }
    let left = (mi.rcWork.left + mi.rcWork.right - bw) / 2;
    let top = mi.rcWork.top + s(28.0);
    let r = RECT { left, top, right: left + bw, bottom: top + bh };
    if HUD.get().0.is_null() {
        let created = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                w!("Win12Tasks"),
                w!(""),
                WS_POPUP,
                left,
                top,
                bw,
                bh,
                None,
                None,
                Some(HI.get()),
                None,
            )
        };
        if let Ok(x) = created {
            HUD.set(x);
        }
    }
    let hud = HUD.get();
    if hud.0.is_null() {
        return;
    }
    let Some(rt) = RT.borrow().as_ref().cloned() else {
        return;
    };
    ensure_mem(bw, bh);
    let brc = RECT { left: 0, top: 0, right: bw, bottom: bh };
    if unsafe { rt.BindDC(MEMDC.get(), &brc) }.is_err() {
        return;
    }
    unsafe {
        rt.BeginDraw();
        rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
    }
    let fw = bw as f32;
    let fh = bh as f32;
    let panel = if LIGHT.get() {
        rgba(249, 249, 249, 0.97)
    } else {
        rgba(36, 36, 36, 0.96)
    };
    fill_rr(0.5, 0.5, fw - 0.5, fh - 0.5, sf(16.0), panel, 1.0);
    let rr = D2D1_ROUNDED_RECT {
        rect: rf(0.5, 0.5, fw - 0.5, fh - 0.5),
        radiusX: sf(16.0),
        radiusY: sf(16.0),
    };
    set_color(PALETTE.borrow().line, 1.0);
    {
        let brg = BR.borrow();
        if let Some(b) = brg.as_ref() {
            let brush: &ID2D1Brush = b;
            unsafe { rt.DrawRoundedRectangle(&rr, brush, 1.0, None::<&ID2D1StrokeStyle>) };
        }
    }
    let label = format!("Desktop {}", VD_CUR.get() + 1);
    let ink = PALETTE.borrow().text;
    text(&label, TF_MEDC.borrow().as_ref(), 0.0, sf(8.0), fw, sf(36.0), ink, 1.0);
    let dot = sf(6.0);
    let gap = sf(8.0);
    let act = sf(18.0);
    let cy = sf(48.0);
    let count = VD_COUNT.get();
    let cur = VD_CUR.get();
    let tw = (count - 1) as f32 * (dot + gap) + act;
    let mut x = (fw - tw) / 2.0;
    for k in 0..count {
        let (col, mul) = if k == cur {
            (PALETTE.borrow().accent, 1.0)
        } else {
            (PALETTE.borrow().sub, 0.6)
        };
        let dw = if k == cur { act } else { dot };
        fill_rr(x, cy - dot / 2.0, x + dw, cy + dot / 2.0, dot / 2.0, col, mul);
        x += dw + gap;
    }
    unsafe {
        let _ = rt.EndDraw(None, None);
    }
    layer_present(hud, &r, 255);
    unsafe {
        let _ = SetWindowPos(
            hud,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        SetTimer(Some(tk), TKT_HUD as usize, 1100, None);
    }
}


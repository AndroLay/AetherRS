//! Gambar mini sendiri (Rust + WIC) untuk berkas gambar yang tidak bisa dibuatkan gambar mini oleh Windows
//! (2026-11-19, permintaan pengguna).
//!
//! Latar: folder Gambar ada di sync root OneDrive; Windows meminta gambar mini ke penyedia OneDrive
//! ({021E4F06-...}), yang gagal (WTS_E_FAILEDEXTRACTION) bila OneDrive.exe tidak berjalan.
//!
//! Cara kerja: tampilan memakai FVO_VISTALAYOUT (SysListView32, bukan DirectUI ItemsView) supaya custom draw bisa
//! dipakai. SHELLDLL_DefView di-subclass; pada CDDS_ITEMPOSTPAINT, untuk berkas gambar ber-reparse-point (OneDrive)
//! saat OneDrive mati, kotak ikon ditimpa gambar mini buatan sendiri. Gambar mini dibuat di satu thread latar
//! (WIC, skala Fant, PBGRA) lalu dikirim sebagai HBITMAP; cache di thread UI dibatasi CACHE_BYTES (LRU).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::{Condvar, Mutex, OnceLock};

use windows::core::{w, Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{GENERIC_READ, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, CreateCompatibleDC, CreateDIBSection, CreateSolidBrush, DeleteDC, DeleteObject, FillRect, GetPixel,
    InvalidateRect, SelectObject, AC_SRC_ALPHA, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, IWICPalette,
    WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::Storage::FileSystem::GetFileAttributesW;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Controls::{
    CDDS_ITEMPOSTPAINT, CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDRF_NOTIFYITEMDRAW, CDRF_NOTIFYPOSTPAINT, LVIR_ICON,
    LVM_GETITEMRECT, LVM_GETVIEW, LV_VIEW_ICON, LV_VIEW_TILE, NMHDR, NMLVCUSTOMDRAW, NM_CUSTOMDRAW,
};
use windows::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
use windows::Win32::UI::Shell::{
    DefSubclassProc, IFolderView, IPersistFolder2, IShellFolder, IShellView, RemoveWindowSubclass, SHGetPathFromIDListW,
    SetWindowSubclass, StrRetToStrW, SHGDNF, SHGDN_FORPARSING, SHGDN_INFOLDER,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, PostMessageW, RegisterWindowMessageW, SendMessageW, WM_NCDESTROY, WM_NOTIFY,
};

use crate::sync::SyncCell;

const SUBCLASS_ID: usize = 0x5731_3254;
const CACHE_BYTES: usize = 16 << 20;
const QUEUE_MAX: usize = 48;
const EXTS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "webp", "heic", "heif", "tif", "tiff", "ico", "jfif"];

enum Entry {
    Skip,
    Failed,
    Pending(u64),
    Ready { bmp: HBITMAP, w: i32, h: i32, used: u64 },
}

struct View {
    hwnd: HWND,
    fv: IFolderView,
    sf: IShellFolder,
    dir: String,
}

static VIEWS: SyncCell<RefCell<Vec<View>>> = SyncCell::new(RefCell::new(Vec::new()));
static CACHE: SyncCell<RefCell<(BTreeMap<String, Entry>, usize)>> = SyncCell::new(RefCell::new((BTreeMap::new(), 0)));
static ONEDRIVE: SyncCell<RefCell<(u64, bool)>> = SyncCell::new(RefCell::new((0, false)));

struct Req {
    key: String,
    path: String,
    size: i32,
    hwnd: isize,
}
struct Res {
    key: String,
    bmp: isize,
    w: i32,
    h: i32,
}

static QUEUE: OnceLock<(Mutex<Vec<Req>>, Condvar)> = OnceLock::new();

fn ready_msg() -> u32 {
    static M: OnceLock<u32> = OnceLock::new();
    // SAFETY: nama pesan berakhir NUL.
    *M.get_or_init(|| unsafe { RegisterWindowMessageW(w!("W12FilesThumbReady")) })
}

fn now_ms() -> u64 {
    // SAFETY: tanpa prasyarat.
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
}

/// OneDrive.exe berjalan? Hasil disimpan 3 detik (snapshot proses tidak murah).
fn onedrive_running() -> bool {
    let mut c = ONEDRIVE.borrow_mut();
    let t = now_ms();
    if t.saturating_sub(c.0) > 3000 || c.0 == 0 {
        c.0 = t;
        c.1 = crate::files::frame::process_running("onedrive.exe");
    }
    c.1
}

/// Pasang di tampilan baru (dipanggil dari OnViewCreated). Hanya folder sistem berkas.
pub(crate) fn attach(sv: &IShellView) {
    // SAFETY: semua antarmuka dari tampilan yang hidup; PIDL dibebaskan.
    unsafe {
        let Ok(hwnd) = sv.GetWindow() else { return };
        let Ok(fv) = sv.cast::<IFolderView>() else { return };
        let Ok(sf) = fv.GetFolder::<IShellFolder>() else { return };
        let Ok(pf) = fv.GetFolder::<IPersistFolder2>() else { return };
        let Ok(pidl) = pf.GetCurFolder() else { return };
        let mut buf = [0u16; 260];
        let ok = SHGetPathFromIDListW(pidl, &mut buf).as_bool();
        CoTaskMemFree(Some(pidl as *const c_void));
        if !ok || buf[0] == 0 {
            return;
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let dir = String::from_utf16_lossy(&buf[..n]);
        if VIEWS.borrow().iter().any(|v| v.hwnd == hwnd) {
            return;
        }
        if SetWindowSubclass(hwnd, Some(subproc), SUBCLASS_ID, 0).as_bool() {
            VIEWS.borrow_mut().push(View { hwnd, fv, sf, dir });
        }
    }
}

unsafe extern "system" fn subproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM, _id: usize, _ref: usize) -> LRESULT {
    if m == WM_NOTIFY && lp.0 != 0 {
        let nm = &*(lp.0 as *const NMHDR);
        if nm.code == NM_CUSTOMDRAW {
            let r = DefSubclassProc(h, m, wp, lp);
            let cd = &*(lp.0 as *const NMLVCUSTOMDRAW);
            let stage = cd.nmcd.dwDrawStage;
            if stage == CDDS_PREPAINT {
                return LRESULT(r.0 | CDRF_NOTIFYITEMDRAW as isize);
            }
            if stage == CDDS_ITEMPREPAINT {
                return LRESULT(r.0 | CDRF_NOTIFYPOSTPAINT as isize);
            }
            if stage == CDDS_ITEMPOSTPAINT {
                draw_item(h, nm.hwndFrom, cd.nmcd.hdc, cd.nmcd.dwItemSpec as i32);
            }
            return r;
        }
    } else if m == ready_msg() && lp.0 != 0 {
        let res = Box::from_raw(lp.0 as *mut Res);
        on_ready(h, *res);
        return LRESULT(0);
    } else if m == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(h, Some(subproc), SUBCLASS_ID);
        VIEWS.borrow_mut().retain(|v| v.hwnd != h);
    }
    DefSubclassProc(h, m, wp, lp)
}

/// Jalur lengkap item ke-`i` tampilan `h`.
unsafe fn item_path(h: HWND, i: i32) -> Option<String> {
    let views = VIEWS.borrow();
    let v = views.iter().find(|v| v.hwnd == h)?;
    let pidl: *mut ITEMIDLIST = v.fv.Item(i).ok()?;
    let mut sr = STRRET::default();
    let mut out = None;
    if v.sf.GetDisplayNameOf(pidl, SHGDNF(SHGDN_INFOLDER.0 | SHGDN_FORPARSING.0), &mut sr).is_ok() {
        let mut p = PWSTR::null();
        if StrRetToStrW(&mut sr, Some(pidl), &mut p).is_ok() {
            out = p.to_string().ok().map(|n| format!("{}\\{}", v.dir.trim_end_matches('\\'), n));
            CoTaskMemFree(Some(p.0 as *const c_void));
        }
    }
    CoTaskMemFree(Some(pidl as *const c_void));
    out
}

/// Perlu gambar mini sendiri? Berkas gambar, placeholder/reparse OneDrive, dan OneDrive tidak berjalan.
fn needs_own(path: &str) -> bool {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if !EXTS.contains(&ext.as_str()) {
        return false;
    }
    let wz = crate::wide::to_wide_z(path);
    // SAFETY: wz berakhir NUL.
    let a = unsafe { GetFileAttributesW(PCWSTR(wz.as_ptr())) };
    a != u32::MAX && a & 0x400 != 0 && a & 0x10 == 0 && !onedrive_running()
}

unsafe fn draw_item(h: HWND, lv: HWND, hdc: HDC, i: i32) {
    let view = SendMessageW(lv, LVM_GETVIEW, None, None).0 as u32;
    if view != LV_VIEW_ICON && view != LV_VIEW_TILE {
        return;
    }
    let mut rc = RECT { left: LVIR_ICON as i32, ..Default::default() };
    if SendMessageW(lv, LVM_GETITEMRECT, Some(WPARAM(i as usize)), Some(LPARAM(&mut rc as *mut RECT as isize))).0 == 0 {
        return;
    }
    let size = (rc.right - rc.left).min(rc.bottom - rc.top);
    if size < 40 {
        return;
    }
    let Some(path) = item_path(h, i) else { return };
    let key = format!("{}|{}", path.to_lowercase(), size);
    let mut c = CACHE.borrow_mut();
    let t = now_ms();
    match c.0.get_mut(&key) {
        Some(Entry::Skip) | Some(Entry::Failed) => {}
        Some(Entry::Ready { bmp, w, h: bh, used }) => {
            *used = t;
            paint(hdc, &rc, *bmp, *w, *bh);
        }
        Some(Entry::Pending(at)) => {
            if t.saturating_sub(*at) > 4000 {
                *at = t;
                enqueue(Req { key, path, size, hwnd: h.0 as isize });
            }
        }
        None => {
            if needs_own(&path) {
                c.0.insert(key.clone(), Entry::Pending(t));
                enqueue(Req { key, path, size, hwnd: h.0 as isize });
            } else {
                c.0.insert(key, Entry::Skip);
            }
        }
    }
}

/// Tutup ikon generik dengan warna latar kotak (diambil dari sudutnya, jadi sorotan pilihan ikut), lalu gambar
/// mini di tengah bawah seperti Explorer.
unsafe fn paint(hdc: HDC, rc: &RECT, bmp: HBITMAP, w: i32, h: i32) {
    let bg = GetPixel(hdc, rc.left + 1, rc.top + 1);
    let br = CreateSolidBrush(bg);
    FillRect(hdc, rc, br);
    let _ = DeleteObject(HGDIOBJ(br.0));
    let mem = CreateCompatibleDC(Some(hdc));
    if mem.0.is_null() {
        return;
    }
    let old = SelectObject(mem, HGDIOBJ(bmp.0));
    let x = rc.left + (rc.right - rc.left - w) / 2;
    let y = rc.bottom - h;
    let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
    let _ = AlphaBlend(hdc, x, y, w, h, mem, 0, 0, w, h, bf);
    SelectObject(mem, old);
    let _ = DeleteDC(mem);
}

fn on_ready(h: HWND, r: Res) {
    let mut c = CACHE.borrow_mut();
    let bytes = (r.w.max(0) as usize) * (r.h.max(0) as usize) * 4;
    let e = if r.bmp == 0 {
        Entry::Failed
    } else {
        c.1 += bytes;
        Entry::Ready { bmp: HBITMAP(r.bmp as *mut c_void), w: r.w, h: r.h, used: now_ms() }
    };
    if let Some(Entry::Ready { bmp, w, h, .. }) = c.0.insert(r.key, e) {
        c.1 = c.1.saturating_sub((w * h * 4) as usize);
        // SAFETY: bitmap lama milik cache.
        let _ = unsafe { DeleteObject(HGDIOBJ(bmp.0)) };
    }
    // LRU: buang yang paling lama tidak dipakai sampai di bawah batas.
    while c.1 > CACHE_BYTES {
        let oldest = c
            .0
            .iter()
            .filter_map(|(k, e)| if let Entry::Ready { used, .. } = e { Some((k.clone(), *used)) } else { None })
            .min_by_key(|x| x.1);
        let Some((k, _)) = oldest else { break };
        if let Some(Entry::Ready { bmp, w, h, .. }) = c.0.remove(&k) {
            c.1 = c.1.saturating_sub((w * h * 4) as usize);
            // SAFETY: bitmap milik cache.
            let _ = unsafe { DeleteObject(HGDIOBJ(bmp.0)) };
        }
    }
    if c.0.len() > 20_000 {
        c.0.retain(|_, e| matches!(e, Entry::Ready { .. }));
    }
    drop(c);
    // SAFETY: jendela daftar anak tampilan ini.
    unsafe {
        if let Ok(lv) = FindWindowExW(Some(h), None, w!("SysListView32"), PCWSTR::null()) {
            let _ = InvalidateRect(Some(lv), None, false);
        }
    }
}

fn enqueue(r: Req) {
    let q = QUEUE.get_or_init(|| {
        let _ = std::thread::Builder::new().name("w12-thumbs".into()).spawn(worker);
        (Mutex::new(Vec::new()), Condvar::new())
    });
    if let Ok(mut v) = q.0.lock() {
        if v.len() >= QUEUE_MAX {
            v.remove(0); // tertua dibuang; dijadwal ulang saat item itu dilukis lagi (Pending > 4 dtk)
        }
        v.push(r);
        q.1.notify_one();
    }
}

fn worker() {
    // SAFETY: inisialisasi COM thread ini sekali.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let wic: Option<IWICImagingFactory> = unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }.ok();
    let Some((m, cv)) = QUEUE.get() else { return };
    loop {
        let req = {
            let Ok(mut v) = m.lock() else { return };
            while v.is_empty() {
                v = match cv.wait(v) {
                    Ok(g) => g,
                    Err(_) => return,
                };
            }
            v.pop().unwrap() // terbaru dulu: item yang sedang terlihat setelah gulir cepat
        };
        let (bmp, w, h) = wic.as_ref().and_then(|f| make(f, &req.path, req.size)).unwrap_or((0, 0, 0));
        let res = Box::into_raw(Box::new(Res { key: req.key, bmp, w, h }));
        // SAFETY: hwnd tampilan; bila gagal kirim, sumber daya dibebaskan di sini.
        unsafe {
            if PostMessageW(Some(HWND(req.hwnd as *mut c_void)), ready_msg(), WPARAM(0), LPARAM(res as isize)).is_err() {
                let r = Box::from_raw(res);
                if r.bmp != 0 {
                    let _ = DeleteObject(HGDIOBJ(r.bmp as *mut c_void));
                }
            }
        }
    }
}

/// Dekode `path` dan skalakan agar muat dalam kotak `size` (tanpa diperbesar). Hasil: HBITMAP PBGRA atas-ke-bawah.
fn make(f: &IWICImagingFactory, path: &str, size: i32) -> Option<(isize, i32, i32)> {
    let wz = crate::wide::to_wide_z(path);
    // SAFETY: antarmuka WIC dipakai di thread ini saja; buffer DIB berukuran w*h*4.
    unsafe {
        let dec = f.CreateDecoderFromFilename(PCWSTR(wz.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let (mut sw, mut sh) = (0u32, 0u32);
        frame.GetSize(&mut sw, &mut sh).ok()?;
        if sw == 0 || sh == 0 {
            return None;
        }
        let k = (size as f64 / sw.max(sh) as f64).min(1.0);
        let w = ((sw as f64 * k).round() as u32).max(1);
        let h = ((sh as f64 * k).round() as u32).max(1);
        let sc = f.CreateBitmapScaler().ok()?;
        sc.Initialize(&frame, w, h, WICBitmapInterpolationModeFant).ok()?;
        let cv = f.CreateFormatConverter().ok()?;
        cv.Initialize(&sc, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None::<&IWICPalette>, 0.0, WICBitmapPaletteTypeCustom)
            .ok()?;
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w as i32;
        bi.bmiHeader.biHeight = -(h as i32);
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB.0;
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(None, &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            return None;
        }
        let buf = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
        if cv.CopyPixels(std::ptr::null(), w * 4, buf).is_err() {
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            return None;
        }
        Some((bmp.0 as isize, w as i32, h as i32))
    }
}

use std::mem::size_of;

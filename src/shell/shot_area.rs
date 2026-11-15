//! Tangkapan area native untuk Win+Shift+S di mode shell, tanpa Snipping Tool (`ms-screenclip:`),
//! supaya tidak bergantung pada Explorer.
//!
//! Alur: layar virtual ditangkap dulu (sebelum overlay muncul, agar overlay tidak ikut terekam).
//! Overlay topmost menampilkan hasil tangkapan yang digelapkan. Seret untuk memilih area, lepas untuk
//! menyimpan lewat `hk_publish_shot` (clipboard + PNG di Gambar\Screenshots). Esc atau klik kanan batal.
//! Klik tanpa seret juga batal. Warna bingkai memakai aksen #0078D4 seperti sisa shell.

use super::hotkeys::hk_publish_shot;
use super::prelude::*;
use std::ffi::c_void;

/// Hasil tangkapan layar virtual (BGRA atas-ke-bawah, alfa 255) untuk area yang dipilih.
static SA_BRIGHT: SyncCell<Cell<HBITMAP>> = SyncCell::new(Cell::new(HBITMAP(std::ptr::null_mut())));
/// Salinan tangkapan yang digelapkan (setengah terang) untuk latar overlay.
static SA_DIM: SyncCell<Cell<HBITMAP>> = SyncCell::new(Cell::new(HBITMAP(std::ptr::null_mut())));
/// Penunjuk piksel `SA_BRIGHT`; hidup sampai `sa_free_all`.
static SA_BITS: SyncCell<Cell<*mut u8>> = SyncCell::new(Cell::new(std::ptr::null_mut()));
static SA_WND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
/// Ukuran layar virtual (lebar, tinggi) dalam piksel.
static SA_SIZE: SyncCell<Cell<(i32, i32)>> = SyncCell::new(Cell::new((0, 0)));
/// Titik awal seret (koordinat klien); `None` bila belum ada seretan.
static SA_DRAG: SyncCell<Cell<Option<(i32, i32)>>> = SyncCell::new(Cell::new(None));
static SA_CUR: SyncCell<Cell<(i32, i32)>> = SyncCell::new(Cell::new((0, 0)));
static SA_REG: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

/// Win+Shift+S di mode shell: mulai pemilihan area. Tidak melakukan apa-apa bila sudah berjalan.
pub fn shot_area_start() {
    if !SA_WND.get().0.is_null() {
        return;
    }
    // SAFETY: GetSystemMetrics hanya membaca metrik sistem.
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if vw <= 0 || vh <= 0 || !sa_register() {
        return;
    }
    let n = (vw as usize) * (vh as usize) * 4;
    // Tangkap dulu, sebelum overlay muncul.
    let Some((bright, bits)) = sa_dib(vw, vh) else {
        hk_toast("Tangkapan area gagal", 0xE783);
        return;
    };
    if !sa_capture(bright, vx, vy, vw, vh) {
        sa_del(bright);
        hk_toast("Tangkapan area gagal", 0xE783);
        return;
    }
    // SAFETY: bits menunjuk DIB 32bpp vw*vh yang baru dibuat dan hidup sampai sa_free_all.
    let px = unsafe { std::slice::from_raw_parts_mut(bits, n) };
    for p in px.chunks_exact_mut(4) {
        p[3] = 255; // BitBlt tidak mengisi alfa
    }
    let Some((dim, dim_bits)) = sa_dib(vw, vh) else {
        sa_del(bright);
        hk_toast("Tangkapan area gagal", 0xE783);
        return;
    };
    sa_dim(bits, dim_bits, n);
    SA_BRIGHT.set(bright);
    SA_DIM.set(dim);
    SA_BITS.set(bits);
    SA_SIZE.set((vw, vh));
    SA_DRAG.set(None);
    SA_CUR.set((0, 0));
    let hi = HI.get();
    // SAFETY: kelas Win12ShotArea sudah didaftarkan; jendela top-level tanpa parent.
    let made = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("Win12ShotArea"),
            w!(""),
            WS_POPUP | WS_VISIBLE,
            vx,
            vy,
            vw,
            vh,
            None,
            None,
            Some(hi),
            None,
        )
    };
    let hwnd = match made {
        Ok(h) => h,
        Err(_) => {
            sa_free_all();
            return;
        }
    };
    SA_WND.set(hwnd);
    // SAFETY: hwnd adalah jendela yang baru dibuat. Gagal diam-diam: klik dan klik kanan tetap berfungsi.
    let _ = unsafe { SetForegroundWindow(hwnd) };
}

/// Daftarkan kelas jendela overlay sekali. Mengembalikan `false` bila gagal.
fn sa_register() -> bool {
    if SA_REG.get() {
        return true;
    }
    // SAFETY: WNDCLASSEXW adalah struct POD; nol adalah nilai awal yang valid.
    let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(sa_proc);
    wc.hInstance = HI.get();
    // SAFETY: IDC_CROSS adalah kursor sistem.
    if let Ok(cur) = unsafe { LoadCursorW(None, IDC_CROSS) } {
        wc.hCursor = cur;
    }
    wc.lpszClassName = w!("Win12ShotArea");
    // SAFETY: wc hidup selama pemanggilan.
    let ok = unsafe { RegisterClassExW(&wc) } != 0;
    SA_REG.set(ok);
    ok
}

/// Prosedur jendela overlay. State hanya lewat `Cell`, jadi panggilan balik yang masuk lagi aman.
unsafe extern "system" fn sa_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_ERASEBKGND => LRESULT(1), // latar digambar penuh di WM_PAINT
        WM_PAINT => {
            sa_paint(h);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let p = sa_pt(lp);
            SA_DRAG.set(Some(p));
            SA_CUR.set(p);
            // SAFETY: h adalah jendela overlay yang hidup; capture dilepas di WM_LBUTTONUP.
            let _ = unsafe { SetCapture(h) };
            sa_dirty(h, None, sa_rect());
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if SA_DRAG.get().is_some() {
                let old = sa_rect();
                SA_CUR.set(sa_pt(lp));
                sa_dirty(h, old, sa_rect());
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if SA_DRAG.get().is_some() {
                SA_CUR.set(sa_pt(lp));
                let sel = sa_rect();
                SA_DRAG.set(None);
                // SAFETY: melepas capture yang dipasang WM_LBUTTONDOWN.
                let _ = unsafe { ReleaseCapture() };
                match sel {
                    Some((l, t, r, b)) if r - l >= 4 && b - t >= 4 => {
                        // Salin dulu sebelum jendela ditutup (bitmap dilepas di WM_DESTROY).
                        let out = sa_crop(l, t, r, b);
                        sa_close(h);
                        hk_publish_shot(&out, r - l, b - t);
                    }
                    _ => sa_close(h),
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            sa_close(h);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 == VK_ESCAPE.0 as usize {
                sa_close(h);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            SA_WND.set(HWND(std::ptr::null_mut()));
            SA_DRAG.set(None);
            sa_free_all();
            LRESULT(0)
        }
        // SAFETY: argumen diteruskan apa adanya dari sistem.
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

/// Gambar latar gelap, lalu area terpilih dalam terang, dengan bingkai aksen.
fn sa_paint(h: HWND) {
    let (w, hg) = SA_SIZE.get();
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: ps hidup selama pemanggilan; EndPaint di bawah memakai pasangan yang sama.
    let hdc = unsafe { BeginPaint(h, &mut ps) };
    // SAFETY: mdc dibuat dan dihapus di sini; bitmap dipilih sementara lalu dikembalikan ke semula.
    unsafe {
        let mdc = CreateCompatibleDC(Some(hdc));
        let old = SelectObject(mdc, HGDIOBJ(SA_DIM.get().0));
        let _ = BitBlt(hdc, 0, 0, w, hg, Some(mdc), 0, 0, SRCCOPY);
        if let Some((l, t, r, b)) = sa_rect() {
            let _ = SelectObject(mdc, HGDIOBJ(SA_BRIGHT.get().0));
            let _ = BitBlt(hdc, l, t, r - l, b - t, Some(mdc), l, t, SRCCOPY);
            let brush = CreateSolidBrush(COLORREF(0x00D4_7800)); // #0078D4 dalam urutan BGR
            let rc = RECT { left: l, top: t, right: r, bottom: b };
            let rc2 = RECT { left: l - 1, top: t - 1, right: r + 1, bottom: b + 1 };
            let _ = FrameRect(hdc, &rc, brush);
            let _ = FrameRect(hdc, &rc2, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
        }
        let _ = SelectObject(mdc, old);
        let _ = DeleteDC(mdc);
        let _ = EndPaint(h, &ps);
    }
}

/// Kotak seleksi dari titik seret dan titik kini, dalam koordinat klien (kiri, atas, kanan, bawah).
fn sa_rect() -> Option<(i32, i32, i32, i32)> {
    let d = SA_DRAG.get()?;
    let c = SA_CUR.get();
    Some((d.0.min(c.0), d.1.min(c.1), d.0.max(c.0), d.1.max(c.1)))
}

/// Koordinat klien dari `lParam` (x di kata rendah, y di kata tinggi, bertanda).
fn sa_pt(lp: LPARAM) -> (i32, i32) {
    (
        (lp.0 & 0xFFFF) as u16 as i16 as i32,
        ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32,
    )
}

/// Tandai area yang berubah (lama dan baru) sebagai kotor, dengan margin untuk bingkai.
fn sa_dirty(h: HWND, a: Option<(i32, i32, i32, i32)>, b: Option<(i32, i32, i32, i32)>) {
    let mut u: Option<(i32, i32, i32, i32)> = None;
    for r in [a, b].into_iter().flatten() {
        u = Some(match u {
            None => r,
            Some(p) => (p.0.min(r.0), p.1.min(r.1), p.2.max(r.2), p.3.max(r.3)),
        });
    }
    if let Some((l, t, r, b)) = u {
        let rc = RECT { left: l - 3, top: t - 3, right: r + 3, bottom: b + 3 };
        // SAFETY: h adalah jendela overlay; rc hidup selama pemanggilan.
        let _ = unsafe { InvalidateRect(Some(h), Some(&rc as *const RECT), false) };
    }
}

/// Salin area (l, t, r, b) dari tangkapan terang ke buffer BGRA atas-ke-bawah baru, alfa 255.
fn sa_crop(l: i32, t: i32, r: i32, b: i32) -> Vec<u8> {
    let (vw, vh) = SA_SIZE.get();
    let bits = SA_BITS.get();
    let (cw, ch) = ((r - l) as usize, (b - t) as usize);
    let row_out = cw * 4;
    let mut out = vec![0u8; row_out * ch];
    if bits.is_null() || l < 0 || t < 0 || r > vw || b > vh || row_out == 0 {
        return out;
    }
    // SAFETY: bits menunjuk DIB 32bpp vw*vh yang hidup sampai WM_DESTROY (sa_free_all).
    let src = unsafe { std::slice::from_raw_parts(bits as *const u8, (vw as usize) * (vh as usize) * 4) };
    for (i, dst) in out.chunks_exact_mut(row_out).enumerate() {
        let s = ((t as usize + i) * vw as usize + l as usize) * 4;
        dst.copy_from_slice(&src[s..s + row_out]);
    }
    for px in out.chunks_exact_mut(4) {
        px[3] = 255;
    }
    out
}

/// Tutup overlay. WM_DESTROY melepas bitmap dan mengatur ulang state.
fn sa_close(h: HWND) {
    // SAFETY: h adalah jendela overlay yang hidup.
    let _ = unsafe { DestroyWindow(h) };
}

/// Buat DIB 32bpp atas-ke-bawah berukuran `w`x`h`. Mengembalikan handle dan penunjuk piksel.
fn sa_dib(w: i32, h: i32) -> Option<(HBITMAP, *mut u8)> {
    // SAFETY: BITMAPINFO adalah struct POD; nol adalah nilai awal yang valid.
    let mut bi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = -h; // atas-ke-bawah
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB.0;
    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: GetDC(None) dilepas dengan ReleaseDC di blok yang sama; bi dan bits hidup selama pemanggilan.
    let made = unsafe {
        let sdc = GetDC(None);
        let made = CreateDIBSection(Some(sdc), &bi, DIB_RGB_COLORS, &mut bits, None, 0);
        let _ = ReleaseDC(None, sdc);
        made
    };
    let b = made.ok()?;
    if bits.is_null() {
        sa_del(b);
        return None;
    }
    Some((b, bits as *mut u8))
}

/// Salin layar virtual (vx, vy, vw, vh) ke `bmp` lewat BitBlt dengan CAPTUREBLT (kursor tidak ikut).
fn sa_capture(bmp: HBITMAP, vx: i32, vy: i32, vw: i32, vh: i32) -> bool {
    // SAFETY: sdc dan mdc dilepas sebelum fungsi kembali; bmp milik pemanggil.
    unsafe {
        let sdc = GetDC(None);
        let mdc = CreateCompatibleDC(Some(sdc));
        let old = SelectObject(mdc, HGDIOBJ(bmp.0));
        let ok = BitBlt(mdc, 0, 0, vw, vh, Some(sdc), vx, vy, ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0)).is_ok();
        let _ = GdiFlush();
        let _ = SelectObject(mdc, old);
        let _ = DeleteDC(mdc);
        let _ = ReleaseDC(None, sdc);
        ok
    }
}

/// Isi `dst` dengan `src` yang digelapkan (setengah kecerahan), alfa 255. Keduanya `n` byte.
fn sa_dim(src: *const u8, dst: *mut u8, n: usize) {
    // SAFETY: kedua buffer berukuran n byte, hidup selama pemanggilan, dan tidak tumpang tindih.
    let s = unsafe { std::slice::from_raw_parts(src, n) };
    let d = unsafe { std::slice::from_raw_parts_mut(dst, n) };
    for (dp, sp) in d.chunks_exact_mut(4).zip(s.chunks_exact(4)) {
        dp[0] = sp[0] >> 1;
        dp[1] = sp[1] >> 1;
        dp[2] = sp[2] >> 1;
        dp[3] = 255;
    }
}

/// Lepas satu bitmap GDI.
fn sa_del(b: HBITMAP) {
    // SAFETY: b adalah HBITMAP dari CreateDIBSection dan tidak dipakai lagi setelah ini.
    let _ = unsafe { DeleteObject(HGDIOBJ(b.0)) };
}

/// Lepas kedua bitmap overlay dan kosongkan penunjuknya. Aman dipanggil berulang.
fn sa_free_all() {
    let (b, d) = (SA_BRIGHT.get(), SA_DIM.get());
    if !b.0.is_null() {
        sa_del(b);
    }
    if !d.0.is_null() {
        sa_del(d);
    }
    SA_BRIGHT.set(HBITMAP(std::ptr::null_mut()));
    SA_DIM.set(HBITMAP(std::ptr::null_mut()));
    SA_BITS.set(std::ptr::null_mut());
}

//! Sumber: w12overlay.inc (seluruh file). Status: kode lengkap; cargo check lib+bins dan --tests lolos (0 error, 0 warning) 2026-11-14; belum diuji runtime. Pemilik: H11.
//! Mode shell: Alt+Tab (SWITCH) dan Task View (TV). Tiga jendela dibuat saat dibuka lalu dihancurkan:
//! OV_BG (thumbnail wallpaper, hanya Task View), OV_UI (berlapis, menerima mouse/keyboard),
//! OV_TH (thumbnail DWM jendela, tembus klik, paling atas).
use super::prelude::*;
use windows::Win32::Graphics::Dwm::{
    DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmUnregisterThumbnail,
    DwmUpdateThumbnailProperties, DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
    DWM_TNP_RECTSOURCE, DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};
use std::sync::atomic::Ordering;
use std::time::Instant;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHGetImageList, SHFILEINFOW, SHGFI_SYSICONINDEX, SHIL_JUMBO};
use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;
use windows::Win32::Graphics::Imaging::IWICBitmapSource;
use windows::core::BSTR;
use windows::Win32::System::Com::{CoInitializeEx, IDispatch, IDispatch_Impl, ITypeInfo, COINIT_APARTMENTTHREADED, DISPATCH_FLAGS, DISPPARAMS, EXCEPINFO};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4};
use windows::Win32::UI::Accessibility::{IAccessible, IAccessible_Impl, LresultFromObject, NotifyWinEvent};

/// Jumlah maksimum kartu jendela (`OV_MAX` di C). Dinaikkan ke 64 agar semua jendela tugas tampil (Alt+Tab dan Task View).
const OV_ITEM_MAX: usize = 64;

/// Satu kartu jendela (`OvItem` di C). `th` = handle thumbnail DWM (0 bila belum ada).
#[derive(Clone, Copy)]
struct OvItem {
    h: HWND,
    app: i32,
    card: RECT,
    thumb: RECT,
    th: isize,
    asp: f32,
    title: [u16; 128],
}

// Pengganti g_ov* (state Alt+Tab / Task View). Semua privat ke modul ini.
static OV_ITEMS: SyncCell<RefCell<Vec<OvItem>>> = SyncCell::new(RefCell::new(Vec::new()));
static OV_SEL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
static OV_HOT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
static OV_HOT_X: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static OV_MODE: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(OV_NONE));
static OV_SHOWN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static OV_TRACK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static OV_UI: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static OV_TH: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static OV_BG: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
/// Area layar (koordinat layar) yang ditutupi overlay.
static OV_RC: SyncCell<Cell<RECT>> = SyncCell::new(Cell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }));
/// Kartu desktop Task View; indeks `VD_COUNT` = "Desktop baru".
static OV_DK: SyncCell<RefCell<[RECT; VD_MAX as usize + 1]>> =
    SyncCell::new(RefCell::new([RECT { left: 0, top: 0, right: 0, bottom: 0 }; VD_MAX as usize + 1]));
static OV_DK_TH: SyncCell<RefCell<[isize; VD_MAX as usize]>> = SyncCell::new(RefCell::new([0; VD_MAX as usize]));
static OV_BG_TH: SyncCell<Cell<isize>> = SyncCell::new(Cell::new(0));
static OV_DK_HOT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
static OV_DK_HOT_X: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
/// Status Alt+Tab (`g_atState`, didefinisikan di dekat TasksStop di C).
static AT_STATE: SyncCell<Cell<W12AltTab>> = SyncCell::new(Cell::new(W12AltTab { state: 0, alt_vk: 0 }));

// Alt+Tab gaya Windows (`ov_render_switch`): kartu per aplikasi (judul + ikon di atas, lalu thumbnail DWM) yang mengalir
// per baris di tengah layar, susunan sama dengan Task View (`ov_layout`). Navigasi: Tab / panah kanan maju,
// Shift+Tab / panah kiri mundur (±1); panah atas/bawah berpindah baris secara spasial (`sw_vstep`). Sorotan meluncur
// (`SW_GLIDE`) dan latar blur (`SW_BLUR`). Nilai dasar dalam DIP (dikali `sf`).
/// Tinggi thumbnail awal pada skala 100%; `w12_flow_layout` memperkecilnya bila kartu tidak muat di layar.
const SW_THUMB_H0: f32 = 180.0;
/// Batas panel terhadap monitor (fraksi lebar dan tinggi).
const SW_MAX_W_FRAC: f32 = 0.92;
const SW_MAX_H_FRAC: f32 = 0.70;
/// Padding panel pada skala 100%.
const SW_PAD0: f32 = 12.0;
/// Jarak ikon ke judul dan jarak tepi kartu ke isi.
const SW_TEXT_GAP0: f32 = 8.0;
/// Ikon kecil di baris judul kartu.
const SW_HEAD_ICON0: f32 = 20.0;
/// Ikon besar: di tengah thumbnail untuk jendela tanpa thumbnail (mis. diminimalkan); juga ukuran cache ikon.
const SW_BIG_ICON0: f32 = 64.0;
/// Radius sudut panel.
const SW_PANEL_RAD0: f32 = 8.0;
/// Radius sudut kartu dan sorotan.
const SW_ROW_RAD0: f32 = 8.0;
/// Durasi fade-in panel (ms) dan jeda antar langkah timer fade.
const SW_FADE_MS: u32 = 120;
const SW_FADE_STEP_MS: u32 = 16;
/// Timer fade-in Alt+Tab (lokal, id 1–4 dipakai `state.rs`).
const TKT_SWFADE: i32 = 5;
/// Timer animasi sorotan Alt+Tab (`SW_GLIDE`).
const TKT_SWGLIDE: i32 = 6;
/// Durasi animasi sorotan (ms) dan jeda antar langkah timer.
const SW_GLIDE_MS: u128 = 140;
const SW_GLIDE_STEP_MS: u32 = 16;
/// Nilai `lp` untuk `TK_SW_STEP` saat panah atas/bawah (langkah ±1 memakai ±1).
const SW_VSTEP_UP: isize = 100;
const SW_VSTEP_DOWN: isize = 101;
/// Toleransi (px) agar dua kartu dianggap satu baris.
const SW_ROW_TOL: i32 = 4;
/// Sorotan yang sedang bergeser dari kartu lama ke kartu baru (koordinat relatif panel).
#[derive(Clone, Copy)]
struct SwGlide {
    from: RECT,
    to: RECT,
    t0: Instant,
}
static SW_GLIDE: SyncCell<Cell<Option<SwGlide>>> = SyncCell::new(Cell::new(None));
/// Latar panel Alt+Tab yang sudah diburamkan (BGRA premultiplied, ukuran `OV_RC`). Kosong = tanpa blur.
static SW_BLUR: SyncCell<RefCell<Vec<u8>>> = SyncCell::new(RefCell::new(Vec::new()));
/// Ikon besar per kunci (exe aplikasi, atau `#<hwnd>` untuk jendela tanpa aplikasi). Diisi saat overlay dibuka; bila tidak ada, pakai ikon taskbar `App.icon`.
static SW_ICONS: SyncCell<RefCell<Vec<(String, Icon)>>> = SyncCell::new(RefCell::new(Vec::new()));

fn ov_rc(l: i32, t: i32, r: i32, b: i32) -> RECT {
    RECT { left: l, top: t, right: r, bottom: b }
}

/// `OffsetRect` (tidak ada di crate windows).
fn ov_rc_off(rc: &mut RECT, dx: i32, dy: i32) {
    rc.left += dx;
    rc.right += dx;
    rc.top += dy;
    rc.bottom += dy;
}

/// `IsRectEmpty` (tidak ada di crate windows).
fn ov_rc_empty(rc: &RECT) -> bool {
    rc.right <= rc.left || rc.bottom <= rc.top
}

/// `PtInRect` (tidak ada di crate windows): batas kiri/atas inklusif, kanan/bawah eksklusif.
fn ov_pt_in(rc: &RECT, x: i32, y: i32) -> bool {
    x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom
}

/// `OvAppOf`: indeks app yang memuat jendela `h`, atau -1.
fn ov_app_of(h: HWND) -> i32 {
    let apps = APPS.borrow();
    for (a, app) in apps.iter().enumerate() {
        if app.wins.contains(&h) {
            return a as i32;
        }
    }
    -1
}

/// `OvAspect`: rasio lebar/tinggi (dijepit 0.6..2.4). Jendela diminimalkan memakai rcNormalPosition.
fn ov_aspect(h: HWND) -> f32 {
    let mut r = RECT::default();
    let mut got = false;
    if unsafe { IsIconic(h) }.as_bool() {
        let mut wp = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        if unsafe { GetWindowPlacement(h, &mut wp) }.is_ok() {
            r = wp.rcNormalPosition;
            got = true;
        }
    }
    if !got {
        let _ = unsafe { GetWindowRect(h, &mut r) };
    }
    let w = (r.right - r.left) as f32;
    let ht = (r.bottom - r.top) as f32;
    if w < 1.0 || ht < 1.0 {
        return 1.6;
    }
    let a = w / ht;
    if a < 0.6 {
        0.6
    } else if a > 2.4 {
        2.4
    } else {
        a
    }
}

/// `OvUnthumb`: lepas semua thumbnail jendela dan desktop.
fn ov_unthumb() {
    {
        let mut items = OV_ITEMS.borrow_mut();
        for it in items.iter_mut() {
            if it.th != 0 {
                let _ = unsafe { DwmUnregisterThumbnail(it.th) };
                it.th = 0;
            }
        }
    }
    let mut dk = OV_DK_TH.borrow_mut();
    for t in dk.iter_mut() {
        if *t != 0 {
            let _ = unsafe { DwmUnregisterThumbnail(*t) };
            *t = 0;
        }
    }
}

/// `OvCollect`: isi daftar kartu dari jendela tugas (urutan Z). Jendela aktif dipindah ke indeks 0.
fn ov_collect() {
    let fg = unsafe { GetForegroundWindow() };
    let mut v: Vec<OvItem> = Vec::new();
    let mut w = unsafe { GetTopWindow(None) }.unwrap_or_default();
    while !w.0.is_null() && v.len() < OV_ITEM_MAX {
        if is_task_window(w) {
            let mut title = [0u16; 128];
            unsafe { GetWindowTextW(w, &mut title) };
            v.push(OvItem {
                h: w,
                app: ov_app_of(w),
                card: RECT::default(),
                thumb: RECT::default(),
                th: 0,
                asp: ov_aspect(w),
                title,
            });
        }
        w = unsafe { GetWindow(w, GW_HWNDNEXT) }.unwrap_or_default();
    }
    // Jendela aktif selalu pertama (jendela topmost lain bisa ada di atasnya).
    if let Some(i) = v.iter().skip(1).position(|it| it.h == fg).map(|p| p + 1) {
        let t = v.remove(i);
        v.insert(0, t);
    }
    *OV_ITEMS.borrow_mut() = v;
}

/// `OvLayout`: kartu mengalir per baris, rata tengah; tinggi thumbnail diperkecil sampai semua muat.
/// Koordinat relatif (0,0). Mengembalikan (outW, outH).
fn ov_layout(max_w: f32, max_h: f32, th0: f32) -> (f32, f32) {
    let mut items = OV_ITEMS.borrow_mut();
    let n = items.len().min(OV_ITEM_MAX);
    let mut asp = [0f32; OV_ITEM_MAX];
    for (i, it) in items.iter().take(n).enumerate() {
        asp[i] = it.asp;
    }
    let p = W12FlowParams {
        max_w,
        max_h,
        th0,
        header: sf(34.0),
        pad: sf(6.0),
        gap: sf(14.0),
        min_th: sf(56.0),
        shrink: 0.88,
    };
    let mut card = [W12RectF::default(); OV_ITEM_MAX];
    let mut thumb = [W12RectF::default(); OV_ITEM_MAX];
    let (mut out_w, mut out_h) = (0f32, 0f32);
    w12_flow_layout(
        &asp[..n],
        n as i32,
        &p,
        &mut card[..n],
        &mut thumb[..n],
        &mut out_w,
        &mut out_h,
    );
    for (i, it) in items.iter_mut().take(n).enumerate() {
        let c = card[i];
        let t = thumb[i];
        it.card = ov_rc(c.l as i32, c.t as i32, c.r as i32, c.b as i32);
        it.thumb = ov_rc(t.l as i32, t.t as i32, t.r as i32, t.b as i32);
    }
    (out_w, out_h)
}

/// `OvOffset`: geser semua kartu dan thumbnail.
fn ov_offset(dx: i32, dy: i32) {
    for it in OV_ITEMS.borrow_mut().iter_mut() {
        ov_rc_off(&mut it.card, dx, dy);
        ov_rc_off(&mut it.thumb, dx, dy);
    }
}

/// `OvThumbProps`: atur thumbnail DWM. `fit` = letterbox ke ukuran sumber (menjaga rasio).
fn ov_thumb_props(th: isize, mut dst: RECT, src: Option<&RECT>, fit: bool, visible: bool) {
    if fit {
        if let Ok(sz) = unsafe { DwmQueryThumbnailSourceSize(th) } {
            if sz.cx > 0 && sz.cy > 0 {
                let bw = (dst.right - dst.left) as f32;
                let bh = (dst.bottom - dst.top) as f32;
                let sx = sz.cx as f32;
                let sy = sz.cy as f32;
                let k = if bw / sx < bh / sy { bw / sx } else { bh / sy };
                let w = sx * k;
                let h = sy * k;
                dst.left += ((bw - w) / 2.0) as i32;
                dst.top += ((bh - h) / 2.0) as i32;
                dst.right = dst.left + w as i32;
                dst.bottom = dst.top + h as i32;
            }
        }
    }
    let mut p = DWM_THUMBNAIL_PROPERTIES::default();
    let mut flags = DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY;
    if src.is_some() {
        flags |= DWM_TNP_RECTSOURCE;
    }
    p.dwFlags = flags;
    p.rcDestination = dst;
    if let Some(s) = src {
        p.rcSource = *s;
    }
    p.fVisible = BOOL::from(visible);
    p.opacity = 255;
    p.fSourceClientAreaOnly = BOOL(0);
    let _ = unsafe { DwmUpdateThumbnailProperties(th, &p) };
}

/// `OvThumbs`: daftarkan dan posisikan thumbnail jendela; pada Task View juga thumbnail desktop.
fn ov_thumbs() {
    let th = OV_TH.get();
    if th.0.is_null() {
        return;
    }
    let n = OV_ITEMS.borrow().len();
    for i in 0..n {
        let (h, thumb, mut th_i) = {
            let items = OV_ITEMS.borrow();
            let it = &items[i];
            (it.h, it.thumb, it.th)
        };
        // Task View: jendela diminimalkan tetap ikon besar (digambar OvRender).
        // Alt+Tab: jendela diminimalkan juga diberi thumbnail DWM seperti Windows (bisa kosong bila app tidak menyimpan frame).
        if OV_MODE.get() != OV_SWITCH && unsafe { IsIconic(h) }.as_bool() {
            continue;
        }
        if th_i == 0 {
            match unsafe { DwmRegisterThumbnail(th, h) } {
                Ok(v) => th_i = v,
                Err(_) => {
                    OV_ITEMS.borrow_mut()[i].th = 0;
                    continue;
                }
            }
            OV_ITEMS.borrow_mut()[i].th = th_i;
        }
        ov_thumb_props(th_i, thumb, None, true, true);
    }
    if OV_MODE.get() != OV_TV {
        return;
    }
    let desk = DESK.get();
    if desk.0.is_null() {
        return;
    }
    let mut ds = RECT::default();
    let _ = unsafe { GetWindowRect(desk, &mut ds) };
    let rc = OV_RC.get();
    let src = ov_rc(rc.left - ds.left, rc.top - ds.top, rc.right - ds.left, rc.bottom - ds.top);
    let lim = VD_COUNT.get().min(VD_MAX).max(0) as usize;
    for k in 0..lim {
        let mut th_k = OV_DK_TH.borrow()[k];
        if th_k == 0 {
            match unsafe { DwmRegisterThumbnail(th, desk) } {
                Ok(v) => th_k = v,
                Err(_) => {
                    OV_DK_TH.borrow_mut()[k] = 0;
                    continue;
                }
            }
            OV_DK_TH.borrow_mut()[k] = th_k;
        }
        let mut d = OV_DK.borrow()[k];
        d.left += s(6.0);
        d.top += s(6.0);
        d.right -= s(6.0);
        d.bottom -= s(28.0);
        ov_thumb_props(th_k, d, Some(&src), false, true);
    }
}

/// `OvCloseRect`: area tombol X (pojok kanan atas kartu).
fn ov_close_rect(card: &RECT) -> RECT {
    ov_rc(card.right - s(32.0), card.top + s(5.0), card.right - s(6.0), card.top + s(29.0))
}

/// Judul kartu (u16 ber-NUL) menjadi String.
fn ov_title(t: &[u16; 128]) -> String {
    let end = t.iter().position(|&c| c == 0).unwrap_or(t.len());
    String::from_utf16_lossy(&t[..end])
}

/// Kotak membulat dengan radius sama (`RF` + `D2D1_ROUNDED_RECT` di C).
fn ov_rr(l: f32, t: f32, r: f32, b: f32, rad: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT { rect: rf(l, t, r, b), radiusX: rad, radiusY: rad }
}

/// `DrawRoundedRectangle` dengan kuas `BR` (warna sudah diset lewat `set_color`).
fn ov_outline(rr: &D2D1_ROUNDED_RECT, width: f32) {
    let rt_g = RT.borrow();
    let br_g = BR.borrow();
    if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
        let brush: &ID2D1Brush = br;
        unsafe { rt.DrawRoundedRectangle(rr, brush, width, None::<&ID2D1StrokeStyle>) };
    }
}

/// HWND kosong (pengganti `nullptr`).
fn ov_null() -> HWND {
    HWND(std::ptr::null_mut())
}

/// `OvRender`: gambar overlay ke DIB lalu tampilkan lewat `layer_present`.
fn ov_render() {
    if OV_MODE.get() == OV_SWITCH {
        ov_render_switch();
        return;
    }
    let ui = OV_UI.get();
    let Some(rt) = RT.borrow().as_ref().cloned() else { return };
    if ui.0.is_null() {
        return;
    }
    let rc = OV_RC.get();
    let wi = rc.right - rc.left;
    let hi = rc.bottom - rc.top;
    let (wf, hf) = (wi as f32, hi as f32);
    ensure_mem(wi, hi);
    let brc = RECT { left: 0, top: 0, right: wi, bottom: hi };
    if unsafe { rt.BindDC(MEMDC.get(), &brc) }.is_err() {
        return;
    }
    // SAFETY: rt adalah render target D2D yang valid; BindDC sudah berhasil di atas.
    unsafe {
        rt.BeginDraw();
        rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
    }
    let light = LIGHT.get();
    let c = *PALETTE.borrow();
    let panel = if light { rgba(243, 243, 243, 0.97) } else { rgba(32, 32, 32, 0.95) };
    let hot = if light { rgba(0, 0, 0, 0.06) } else { rgba(255, 255, 255, 0.08) };
    let mode = OV_MODE.get();
    let sel = OV_SEL.get();
    let hot_i = OV_HOT.get();
    if mode == OV_TV {
        let bg_fill = if OV_BG_TH.get() != 0 {
            if light { rgba(238, 238, 238, 0.45) } else { rgba(0, 0, 0, 0.42) }
        } else {
            panel
        };
        fill_rr(0.0, 0.0, wf, hf, 0.0, bg_fill, 1.0);
        let cnt = VD_COUNT.get();
        let nd = if cnt < VD_MAX { cnt + 1 } else { cnt };
        let dk_hot = OV_DK_HOT.get();
        let dk_hot_x = OV_DK_HOT_X.get();
        let cur = VD_CUR.get();
        for k in 0..nd.max(0) as usize {
            let r = OV_DK.borrow()[k];
            let is_new = k as i32 == cnt;
            let fill = if k as i32 == dk_hot {
                if light { rgba(255, 255, 255, 0.95) } else { rgba(60, 60, 60, 0.95) }
            } else {
                panel
            };
            fill_rr(
                r.left as f32,
                r.top as f32,
                r.right as f32,
                r.bottom as f32,
                sf(10.0),
                fill,
                if is_new { 0.75 } else { 1.0 },
            );
            if !is_new && k as i32 == cur {
                let rr = ov_rr(
                    r.left as f32 + 1.0,
                    r.top as f32 + 1.0,
                    r.right as f32 - 1.0,
                    r.bottom as f32 - 1.0,
                    sf(10.0),
                );
                set_color(c.accent, 1.0);
                ov_outline(&rr, sf(2.0));
            }
            if is_new {
                glyph(
                    0xE710,
                    TF_GLYPH.borrow().as_ref(),
                    (r.left + r.right) as f32 / 2.0,
                    (r.top + r.bottom) as f32 / 2.0 - sf(10.0),
                    sf(16.0),
                    c.text,
                    1.0,
                );
                text(
                    "Desktop baru",
                    TF_SC.borrow().as_ref(),
                    r.left as f32,
                    r.bottom as f32 - sf(30.0),
                    r.right as f32,
                    r.bottom as f32 - sf(6.0),
                    c.text,
                    1.0,
                );
            } else {
                let label = format!("Desktop {}", k + 1);
                text(
                    &label,
                    TF_SC.borrow().as_ref(),
                    r.left as f32 + sf(8.0),
                    r.bottom as f32 - sf(28.0),
                    r.right as f32 - sf(8.0),
                    r.bottom as f32 - sf(4.0),
                    c.text,
                    1.0,
                );
                if k as i32 == dk_hot && cnt > 1 {
                    glyph(
                        0xE711,
                        TF_GLYPH_SM.borrow().as_ref(),
                        r.right as f32 - sf(15.0),
                        r.bottom as f32 - sf(16.0),
                        sf(10.0),
                        if dk_hot_x { c.accent } else { c.sub },
                        1.0,
                    );
                }
            }
        }
        if OV_ITEMS.borrow().is_empty() {
            let col = if light { c.text } else { rgba(255, 255, 255, 0.9) };
            text(
                "Tidak ada jendela di desktop ini",
                TF_SC.borrow().as_ref(),
                0.0,
                hf / 2.0 - sf(20.0),
                wf,
                hf / 2.0 + sf(20.0),
                col,
                1.0,
            );
        }
    } else {
        fill_rr(0.5, 0.5, wf - 0.5, hf - 0.5, sf(14.0), panel, 1.0);
        let rr = ov_rr(0.5, 0.5, wf - 0.5, hf - 0.5, sf(14.0));
        set_color(c.line, 1.0);
        ov_outline(&rr, 1.0);
    }
    let items_n = OV_ITEMS.borrow().len();
    for i in 0..items_n {
        let it = OV_ITEMS.borrow()[i];
        let cr = it.card;
        let is_sel = i as i32 == sel;
        let hv = i as i32 == hot_i;
        // Alt+Tab: pilihan = isian tipis + cincin netral; Task View tetap aksen.
        let sw = mode == OV_SWITCH;
        if mode == OV_TV {
            let mul = if is_sel || hv { 1.0 } else { 0.92 };
            fill_rr(cr.left as f32, cr.top as f32, cr.right as f32, cr.bottom as f32, sf(10.0), panel, mul);
        }
        if is_sel || hv {
            let fill = if is_sel && !sw { c.accent_soft } else { hot };
            fill_rr(cr.left as f32, cr.top as f32, cr.right as f32, cr.bottom as f32, sf(10.0), fill, 1.0);
        }
        if is_sel {
            let inset = if sw { sf(1.5) } else { 1.0 };
            let rr = ov_rr(
                cr.left as f32 + inset,
                cr.top as f32 + inset,
                cr.right as f32 - inset,
                cr.bottom as f32 - inset,
                sf(10.0),
            );
            let col = if sw {
                if light { rgba(0, 0, 0, 0.85) } else { rgba(255, 255, 255, 0.92) }
            } else {
                c.accent
            };
            set_color(col, 1.0);
            ov_outline(&rr, if sw { sf(3.0) } else { sf(2.0) });
        }
        let mut tx = cr.left as f32 + sf(10.0);
        if it.app >= 0 {
            if let Some(app) = APPS.borrow_mut().get_mut(it.app as usize) {
                draw_icon_at(&mut app.icon, tx, cr.top as f32 + sf(9.0), sf(16.0), 1.0);
            }
            tx += sf(22.0);
        }
        let tr = if mode == OV_TV && hv { cr.right as f32 - sf(34.0) } else { cr.right as f32 - sf(10.0) };
        let title = ov_title(&it.title);
        text(
            &title,
            TF_S.borrow().as_ref(),
            tx,
            cr.top as f32 + sf(4.0),
            tr,
            cr.top as f32 + sf(30.0),
            c.text,
            1.0,
        );
        if mode == OV_TV && hv {
            let x = ov_close_rect(&cr);
            let hx = OV_HOT_X.get();
            if hx {
                fill_rr(x.left as f32, x.top as f32, x.right as f32, x.bottom as f32, sf(5.0), rgba(196, 43, 28, 1.0), 1.0);
            }
            let gc = if hx { rgba(255, 255, 255, 1.0) } else { c.text };
            glyph(
                0xE711,
                TF_GLYPH_SM.borrow().as_ref(),
                (x.left + x.right) as f32 / 2.0,
                (x.top + x.bottom) as f32 / 2.0,
                sf(10.0),
                gc,
                1.0,
            );
        }
        // Diminimalkan atau thumbnail tidak tersedia: ikon besar.
        if it.th == 0 {
            let t = it.thumb;
            fill_rr(t.left as f32, t.top as f32, t.right as f32, t.bottom as f32, sf(6.0), hot, 1.0);
            let sz = sf(48.0);
            if it.app >= 0 {
                if let Some(app) = APPS.borrow_mut().get_mut(it.app as usize) {
                    draw_icon_at(
                        &mut app.icon,
                        ((t.left + t.right) as f32 - sz) / 2.0,
                        ((t.top + t.bottom) as f32 - sz) / 2.0,
                        sz,
                        1.0,
                    );
                }
            }
        }
    }
    let _ = unsafe { rt.EndDraw(None, None) };
    layer_present(ui, &rc, 255);
}

/// `OvMakeWnd`: jendela overlay (WS_POPUP) seukuran area overlay.
fn ov_make_wnd(ex: WINDOW_EX_STYLE) -> HWND {
    let rc = OV_RC.get();
    unsafe {
        CreateWindowExW(
            ex | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("Win12Tasks"),
            w!(""),
            WS_POPUP,
            rc.left,
            rc.top,
            rc.right - rc.left,
            rc.bottom - rc.top,
            None,
            None,
            Some(HI.get()),
            None,
        )
    }
    .unwrap_or_default()
}

/// `OvClose`: tutup Alt+Tab / Task View dan lepas semua sumber daya overlay.
fn ov_close() {
    let tk = TK_MSG.get();
    if !tk.0.is_null() {
        unsafe {
            let _ = KillTimer(Some(tk), TKT_SWSHOW as usize);
            let _ = KillTimer(Some(tk), TKT_SWFADE as usize);
            let _ = KillTimer(Some(tk), TKT_SWGLIDE as usize);
            let _ = KillTimer(Some(tk), TKT_TVREBUILD as usize);
        }
    }
    SW_ALPHA.set(255);
    SW_GLIDE.set(None);
    SW_BLUR.borrow_mut().clear();
    if OV_MODE.get() == OV_NONE {
        return;
    }
    ov_unthumb();
    let bgth = OV_BG_TH.get();
    if bgth != 0 {
        let _ = unsafe { DwmUnregisterThumbnail(bgth) };
        OV_BG_TH.set(0);
    }
    OV_MODE.set(OV_NONE);
    OV_ITEMS.borrow_mut().clear();
    OV_SHOWN.set(false);
    OV_TRACK.set(false);
    OV_HOT.set(-1);
    OV_DK_HOT.set(-1);
    let ws = [OV_TH.get(), OV_UI.get(), OV_BG.get()];
    OV_TH.set(ov_null());
    OV_UI.set(ov_null());
    OV_BG.set(ov_null());
    for w in ws {
        if !w.0.is_null() {
            let _ = unsafe { DestroyWindow(w) };
        }
    }
    release_mem(); // DIB seukuran layar tidak disimpan; dok membuat DIB kecilnya lagi saat menggambar
    trim_soon();
}

/// Input tombol untuk `SendInput` (`in[i].ki` di C).
fn ov_key_input(vk: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: W12_KBSIG,
            },
        },
    }
}

// ---------------------------------------------------------------- Alt+Tab

/// `SwShow`: buat jendela overlay Alt+Tab dan tampilkan.
fn sw_show() {
    if OV_MODE.get() != OV_SWITCH || OV_SHOWN.get() {
        return;
    }
    let first = {
        let items = OV_ITEMS.borrow();
        match items.first() {
            Some(it) => it.h,
            None => return,
        }
    };
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GetMonitorInfoW(MonitorFromWindow(first, MONITOR_DEFAULTTOPRIMARY), &mut mi);
    }
    let m = mi.rcMonitor;
    let mw = (m.right - m.left) as f32;
    let mh = (m.bottom - m.top) as f32;
    // Alt+Tab gaya Windows: kartu (judul + thumbnail DWM) mengalir di tengah layar.
    let (bw, bh) = sw_layout(mw, mh);
    let left = m.left + ((mw - bw) / 2.0) as i32;
    let top = m.top + ((mh - bh) / 2.0) as i32;
    OV_RC.set(ov_rc(left, top, left + bw.round() as i32, top + bh.round() as i32));
    // Blur latar dihitung sebelum jendela overlay dibuat, agar layar di bawah panel belum tertutup.
    sw_blur_capture(OV_RC.get());
    let ui = ov_make_wnd(WS_EX_LAYERED | WS_EX_NOACTIVATE);
    let th = ov_make_wnd(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE);
    OV_UI.set(ui);
    OV_TH.set(th);
    if ui.0.is_null() || th.0.is_null() {
        ov_close();
        at_reset_sticky();
        return;
    }
    OV_SHOWN.set(true);
    sw_load_icons(); // ikon besar per item (cache SW_ICONS) untuk jendela tanpa thumbnail
    // Fade-in: mulai dari alpha 0, naik lewat `TKT_SWFADE`. Bila timer gagal dipasang, langsung tampil penuh.
    SW_ALPHA.set(0);
    ov_render();
    ov_thumbs(); // thumbnail DWM di atas panel (OV_TH); opacity-nya ikut fade panel
    sw_thumb_alpha(SW_ALPHA.get());
    unsafe {
        let _ = ShowWindow(ui, SW_SHOWNA);
        let _ = ShowWindow(th, SW_SHOWNA);
        let _ = SetWindowPos(th, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    let armed = unsafe { SetTimer(Some(TK_MSG.get()), TKT_SWFADE as usize, SW_FADE_STEP_MS, None) } != 0;
    if !armed {
        SW_ALPHA.set(255);
        layer_present(ui, &OV_RC.get(), 255);
        sw_thumb_alpha(255);
    }
}

/// Satu langkah fade-in Alt+Tab (`TKT_SWFADE`): naikkan alpha dan tampilkan ulang bitmap yang sudah digambar.
/// Tidak menggambar ulang isi panel. Timer dipasang ulang sampai alpha penuh.
fn sw_fade_tick() {
    if !OV_SHOWN.get() || OV_MODE.get() != OV_SWITCH {
        return;
    }
    let ui = OV_UI.get();
    if ui.0.is_null() {
        return;
    }
    let step = (255 * SW_FADE_STEP_MS / SW_FADE_MS).max(1) as u8;
    let a = SW_ALPHA.get().saturating_add(step);
    SW_ALPHA.set(a);
    layer_present(ui, &OV_RC.get(), a);
    sw_thumb_alpha(a);
    if a < 255 {
        unsafe { SetTimer(Some(TK_MSG.get()), TKT_SWFADE as usize, SW_FADE_STEP_MS, None) };
    }
}

/// `SwStep`: langkah pilihan Alt+Tab (`dir` = +1 / -1). `kb` = dipicu keyboard (tampil tertunda 110 ms).
fn sw_step(dir: i32, kb: bool) {
    let mode = OV_MODE.get();
    if mode == OV_TV {
        // Alt+Tab di Task View: pindahkan pilihan.
        let n = OV_ITEMS.borrow().len() as i32;
        if n > 0 {
            let cur = OV_SEL.get().max(0);
            OV_SEL.set((cur + dir + n) % n);
            ov_render();
        }
        return;
    }
    if mode != OV_SWITCH {
        // Per jendela (seperti Windows): setiap jendela tugas punya kartu sendiri, termasuk jendela tanpa aplikasi.
        ov_collect();
        let n = OV_ITEMS.borrow().len() as i32;
        if n == 0 {
            at_reset_sticky(); // Ctrl+Alt+Tab tanpa jendela: tidak ada panel, jangan tinggalkan mode tetap
            return;
        }
        OV_MODE.set(OV_SWITCH);
        OV_SHOWN.set(false);
        OV_SEL.set(if n > 1 {
            if dir > 0 { 1 } else { n - 1 }
        } else {
            0
        });
        // Alt+Tab cepat (dilepas < 110 ms) berpindah tanpa menggambar apa pun; gestur langsung menampilkan.
        if kb {
            unsafe { SetTimer(Some(TK_MSG.get()), TKT_SWSHOW as usize, 110, None) };
        } else {
            sw_show();
        }
        return;
    }
    let n = OV_ITEMS.borrow().len() as i32;
    if n > 0 {
        sw_select((OV_SEL.get() + dir + n) % n);
    }
    if OV_SHOWN.get() {
        ov_render();
    }
}

/// Pindah pilihan ke `to`. Bila panel sudah tampil, sorotan meluncur dari kartu lama (`SW_GLIDE`). Pemanggil menggambar ulang.
fn sw_select(to: i32) {
    let from = OV_SEL.get();
    OV_SEL.set(to);
    ov_acc_sel_changed();
    if !OV_SHOWN.get() || from == to || from < 0 || to < 0 {
        return;
    }
    let items = OV_ITEMS.borrow();
    if let (Some(a), Some(b)) = (items.get(from as usize), items.get(to as usize)) {
        SW_GLIDE.set(Some(SwGlide { from: a.card, to: b.card, t0: Instant::now() }));
        unsafe { SetTimer(Some(TK_MSG.get()), TKT_SWGLIDE as usize, SW_GLIDE_STEP_MS, None) };
    }
}

/// Navigasi spasial (panah atas/bawah): pindah ke baris berikutnya di arah itu, pilih kartu dengan pusat X terdekat.
/// Tanpa baris di arah itu, bungkus ke baris ujung di arah berlawanan. Hanya satu baris: langkah linear
/// (bawah = kartu berikutnya, atas = kartu sebelumnya), supaya keempat panah selalu memindahkan pilihan.
/// Bila panel belum tampil (masih dalam jeda 110 ms), pilihan tetap diperbarui; panel tampil dengan pilihan itu.
fn sw_vstep(down: bool) {
    if OV_MODE.get() != OV_SWITCH {
        return;
    }
    let cur = OV_SEL.get();
    let pick = {
        let items = OV_ITEMS.borrow();
        let n = items.len();
        if cur < 0 || (cur as usize) >= n {
            return;
        }
        let c = items[cur as usize].card;
        // Jarak vertikal searah panah (positif = di arah itu).
        let dy_of = |r: &RECT| if down { r.top - c.top } else { c.top - r.top };
        let mut row = i32::MAX;
        for it in items.iter() {
            let dy = dy_of(&it.card);
            if dy > SW_ROW_TOL && dy < row {
                row = dy;
            }
        }
        // `top` baris sasaran: baris berikutnya; bila tidak ada, baris ujung di arah berlawanan (bungkus).
        let target_top = if row != i32::MAX {
            Some(if down { c.top + row } else { c.top - row })
        } else {
            let edge = if down {
                items.iter().map(|it| it.card.top).min()
            } else {
                items.iter().map(|it| it.card.top).max()
            };
            edge.filter(|t| (t - c.top).abs() > SW_ROW_TOL)
        };
        match target_top {
            Some(t) => {
                let ccx = c.left + c.right;
                let mut best: Option<(i32, usize)> = None;
                for (i, it) in items.iter().enumerate() {
                    if (it.card.top - t).abs() > SW_ROW_TOL {
                        continue;
                    }
                    let dx = ((it.card.left + it.card.right) - ccx).abs();
                    if best.map_or(true, |(bd, _)| dx < bd) {
                        best = Some((dx, i));
                    }
                }
                best.map(|(_, i)| i)
            }
            // Satu baris saja: atas/bawah berfungsi seperti kiri/kanan.
            None => Some(if down { (cur as usize + 1) % n } else { (cur as usize + n - 1) % n }),
        }
    };
    if let Some(i) = pick {
        sw_select(i as i32);
        if OV_SHOWN.get() {
            ov_render();
        }
    }
}

/// Satu langkah animasi sorotan (`TKT_SWGLIDE`): gambar ulang panel; timer dipasang ulang sampai `SW_GLIDE_MS` terlewati.
fn sw_glide_tick() {
    let Some(g) = SW_GLIDE.get() else { return };
    if !OV_SHOWN.get() || OV_MODE.get() != OV_SWITCH {
        SW_GLIDE.set(None);
        return;
    }
    if g.t0.elapsed().as_millis() >= SW_GLIDE_MS {
        SW_GLIDE.set(None);
    } else {
        unsafe { SetTimer(Some(TK_MSG.get()), TKT_SWGLIDE as usize, SW_GLIDE_STEP_MS, None) };
    }
    ov_render();
}

/// Blur latar panel Alt+Tab: tangkap layar di bawah `rc` (ukuran panel), kecilkan 4×, box blur, besarkan lagi (bilinear),
/// lalu kalikan dengan bentuk rounded-rect agar sudut membulat. Hasil di `SW_BLUR` (BGRA premultiplied). Gagal = kosong.
fn sw_blur_capture(rc: RECT) {
    const DS: usize = 4;
    const BLUR_R: usize = 3;
    SW_BLUR.borrow_mut().clear();
    let w = (rc.right - rc.left).max(1) as usize;
    let h = (rc.bottom - rc.top).max(1) as usize;
    let sw = (w / DS).max(1);
    let sh = (h / DS).max(1);
    let mut small = vec![0u8; sw * sh * 4];
    let mut ok = false;
    // SAFETY: GDI dipakai seperti `mica_grab`: DC dan DIB dilepas di semua jalur.
    unsafe {
        let screen = GetDC(None);
        if screen.0.is_null() {
            return;
        }
        let sample = CreateCompatibleDC(Some(screen));
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = sw as i32;
        bi.bmiHeader.biHeight = -(sh as i32); // top-down
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB.0;
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        if let Ok(dib) = CreateDIBSection(Some(screen), &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
            if !sample.0.is_null() && !bits.is_null() {
                let old = SelectObject(sample, HGDIOBJ(dib.0));
                SetStretchBltMode(sample, HALFTONE);
                ok = StretchBlt(
                    sample,
                    0,
                    0,
                    sw as i32,
                    sh as i32,
                    Some(screen),
                    rc.left,
                    rc.top,
                    w as i32,
                    h as i32,
                    ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
                )
                .as_bool();
                if ok {
                    std::ptr::copy_nonoverlapping(bits as *const u8, small.as_mut_ptr(), small.len());
                }
                SelectObject(sample, old);
            }
            let _ = DeleteObject(HGDIOBJ(dib.0));
        }
        if !sample.0.is_null() {
            let _ = DeleteDC(sample);
        }
        ReleaseDC(None, screen);
    }
    if !ok {
        return;
    }
    let blurred = ov_box_blur(&small, sw, sh, BLUR_R);
    let (wf, hf) = (w as f32, h as f32);
    let (hw, hh) = (wf / 2.0, hf / 2.0);
    let rad = sf(SW_PANEL_RAD0).min(hw.min(hh));
    let mut out = vec![0u8; w * h * 4];
    let (sxk, syk) = (sw as f32 / wf, sh as f32 / hf);
    for y in 0..h {
        // Koordinat kecil (bilinear); tepi dijepit.
        let sy = ((y as f32 + 0.5) * syk - 0.5).clamp(0.0, (sh - 1) as f32);
        let (y0, ty) = (sy.floor() as usize, sy - sy.floor());
        let y1 = (y0 + 1).min(sh - 1);
        // Cakupan rounded-rect (jarak bertanda, anti-alias 1 px).
        let py = y as f32 + 0.5 - hh;
        let qy = py.abs() - (hh - rad);
        for x in 0..w {
            let sx = ((x as f32 + 0.5) * sxk - 0.5).clamp(0.0, (sw - 1) as f32);
            let (x0, tx) = (sx.floor() as usize, sx - sx.floor());
            let x1 = (x0 + 1).min(sw - 1);
            let px = x as f32 + 0.5 - hw;
            let qx = px.abs() - (hw - rad);
            let dist = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rad;
            let cov = (0.5 - dist).clamp(0.0, 1.0);
            for c in 0..3 {
                let at = |xx: usize, yy: usize| blurred[(yy * sw + xx) * 4 + c] as f32;
                let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
                let bot = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
                let v = top + (bot - top) * ty;
                out[(y * w + x) * 4 + c] = (v * cov).round() as u8;
            }
            out[(y * w + x) * 4 + 3] = (255.0 * cov).round() as u8;
        }
    }
    *SW_BLUR.borrow_mut() = out;
}

/// Box blur terpisah (horizontal lalu vertikal) dengan radius `r` piksel; tepi dijepit. Masukan dan keluaran BGRA.
fn ov_box_blur(src: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    let mut tmp = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let x0 = x.saturating_sub(r);
            let x1 = (x + r).min(w - 1);
            let n = (x1 - x0 + 1) as u32;
            for c in 0..4 {
                let s: u32 = (x0..=x1).map(|xx| src[(y * w + xx) * 4 + c] as u32).sum();
                tmp[(y * w + x) * 4 + c] = (s / n) as u8;
            }
        }
    }
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let y0 = y.saturating_sub(r);
        let y1 = (y + r).min(h - 1);
        let n = (y1 - y0 + 1) as u32;
        for x in 0..w {
            for c in 0..4 {
                let s: u32 = (y0..=y1).map(|yy| tmp[(yy * w + x) * 4 + c] as u32).sum();
                out[(y * w + x) * 4 + c] = (s / n) as u8;
            }
        }
    }
    out
}

/// Klik mouse memilih lalu menutup panel: status Alt+Tab kembali diam (state 1 = Alt masih ditahan, 3 = mode tetap).
fn at_reset_active() {
    let mut st = AT_STATE.get();
    if st.state == 1 || st.state == 3 {
        st.state = 0;
        AT_STATE.set(st);
    }
}

/// Mode tetap (state 3) tanpa panel yang tampil (tanpa jendela, Win+Tab, gagal buat jendela): kembali diam.
/// Tanpa ini kait terus menelan semua tombol sampai Enter/Spasi/Esc. State 1 (Alt ditahan) sengaja tidak disentuh.
fn at_reset_sticky() {
    let mut st = AT_STATE.get();
    if st.state == 3 {
        st.state = 0;
        AT_STATE.set(st);
    }
}

/// `SwCommit`: aktifkan pilihan. `alt_vk` != 0 = Alt yang ditahan kait harus dilepas (lihat komentar C).
fn sw_commit(alt_vk: usize) {
    let mode = OV_MODE.get();
    let sel = OV_SEL.get();
    let target = {
        let items = OV_ITEMS.borrow();
        if mode == OV_SWITCH && sel >= 0 && (sel as usize) < items.len() {
            items[sel as usize].h
        } else {
            ov_null()
        }
    };
    if mode == OV_SWITCH {
        ov_close();
    }
    // Tombol kosong 0xE8 dulu: aplikasi tidak membuka bilah menunya, dan proses ini boleh memindah fokus.
    // Dikirim juga tanpa Alt (mode tetap, Enter/Spasi), karena tanpa Alt Windows menolak pindah fokus.
    let pad = [ov_key_input(0xE8, KEYBD_EVENT_FLAGS(0)), ov_key_input(0xE8, KEYEVENTF_KEYUP)];
    unsafe { SendInput(&pad, std::mem::size_of::<INPUT>() as i32) };
    if !target.0.is_null() && unsafe { IsWindow(Some(target)) }.as_bool() {
        activate_window(target);
    }
    if alt_vk != 0 {
        let ext = if alt_vk == VK_RMENU.0 as usize {
            KEYEVENTF_EXTENDEDKEY
        } else {
            KEYBD_EVENT_FLAGS(0)
        };
        unsafe { SendInput(&[ov_key_input(alt_vk as u16, KEYEVENTF_KEYUP | ext)], std::mem::size_of::<INPUT>() as i32) };
    }
}

/// Alpha panel Alt+Tab saat fade-in (0–255). Diatur `sw_show` / `sw_fade_tick`, dipakai `ov_render_switch`.
static SW_ALPHA: SyncCell<Cell<u8>> = SyncCell::new(Cell::new(255));

/// Judul kartu Alt+Tab: judul jendela (seperti Windows). Bila judul kosong, pakai nama aplikasi dari `APPS`;
/// bila jendela tidak terdaftar di `APPS` (`app` = -1), pakai nama berkas exe-nya.
/// PORT: cadangan exe ini tidak ada di C. Ditambahkan agar kartu tidak tampil tanpa nama; penyebab nama kosong
/// di build lama belum terbukti (belum diuji runtime).
fn sw_name(it: &OvItem) -> String {
    let t = ov_title(&it.title);
    if !t.is_empty() {
        return t;
    }
    if let Some(n) = APPS.borrow().get(it.app as usize).map(|a| a.name.clone()) {
        if !n.is_empty() {
            return n;
        }
    }
    ov_exe_stem(it.h)
}

/// Nama berkas exe (tanpa ekstensi) proses pemilik jendela `h`; kosong bila gagal.
fn ov_exe_stem(h: HWND) -> String {
    let mut pid: u32 = 0;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(h, Some(&mut pid as *mut u32));
    }
    if pid == 0 {
        return String::new();
    }
    let Ok(hproc) = (unsafe {
        windows::Win32::System::Threading::OpenProcess(
            windows::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        )
    }) else {
        return String::new();
    };
    let mut buf = [0u16; 520];
    let mut n: u32 = buf.len() as u32;
    let ok = unsafe {
        windows::Win32::System::Threading::QueryFullProcessImageNameW(
            hproc,
            windows::Win32::System::Threading::PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut n,
        )
    };
    let _ = unsafe { CloseHandle(hproc) };
    if ok.is_err() {
        return String::new();
    }
    std::path::Path::new(&from_wide(&buf))
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Ukuran panel Alt+Tab `(bw, bh)` untuk monitor `mw` x `mh`. Kartu memakai susunan Task View (`ov_layout`)
/// dengan batas fraksi monitor; `ov_offset` menggeser kartu dan thumbnail ke dalam padding panel.
fn sw_layout(mw: f32, mh: f32) -> (f32, f32) {
    let pad = sf(SW_PAD0);
    let (gw, gh) = ov_layout(mw * SW_MAX_W_FRAC - 2.0 * pad, mh * SW_MAX_H_FRAC - 2.0 * pad, sf(SW_THUMB_H0));
    ov_offset(pad.round() as i32, pad.round() as i32);
    (gw + 2.0 * pad, gh + 2.0 * pad)
}

/// Alpha thumbnail DWM Alt+Tab (0–255), mengikuti fade panel. Hanya kartu yang punya thumbnail.
fn sw_thumb_alpha(a: u8) {
    for it in OV_ITEMS.borrow().iter() {
        if it.th != 0 {
            let p = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_OPACITY,
                opacity: a,
                ..Default::default()
            };
            let _ = unsafe { DwmUpdateThumbnailProperties(it.th, &p) };
        }
    }
}

/// Ikon besar satu exe (SHIL_JUMBO, 256 px), di-scale ke `px`. None bila gagal (mis. UWP atau exe kosong).
fn sw_big_icon(exe: &str, px: i32) -> Option<IWICBitmapSource> {
    if exe.is_empty() {
        return None;
    }
    // SAFETY: SHFILEINFOW hanya berisi bilangan bulat dan array, sehingga pola bit nol valid.
    let mut sfi: SHFILEINFOW = unsafe { std::mem::zeroed() };
    let wide = to_wide_z(exe);
    // SAFETY: wide hidup sampai panggilan selesai; sfi adalah buffer keluaran yang valid.
    let got = unsafe {
        SHGetFileInfoW(
            pcw(&wide),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut sfi as *mut SHFILEINFOW),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_SYSICONINDEX,
        )
    };
    if got == 0 {
        return None;
    }
    // SAFETY: SHGetImageList mengembalikan IImageList ber-AddRef; Drop melepasnya.
    let il: IImageList = unsafe { SHGetImageList::<IImageList>(SHIL_JUMBO as i32) }.ok()?;
    // SAFETY: il valid; sfi.iIcon diisi SHGetFileInfoW di atas.
    let hi = unsafe { il.GetIcon(sfi.iIcon, ILD_TRANSPARENT.0) }.ok()?;
    if hi.0.is_null() {
        return None;
    }
    let src = icon_from_hicon(hi, px);
    // SAFETY: hi dibuat GetIcon dan tidak dipakai lagi; icon_from_hicon tidak menghancurkannya.
    let _ = unsafe { DestroyIcon(hi) };
    src
}

/// Kunci cache `SW_ICONS` dan exe item `it`. Kunci = exe bila jendela punya aplikasi dengan exe; selain itu `#<hwnd>`.
fn sw_icon_key(it: &OvItem) -> (String, String) {
    let exe = if it.app < 0 {
        String::new()
    } else {
        APPS.borrow().get(it.app as usize).map(|a| a.exe.clone()).unwrap_or_default()
    };
    let key = if exe.is_empty() { format!("#{}", it.h.0 as usize) } else { exe.clone() };
    (key, exe)
}

/// Muat ikon besar untuk item `OV_ITEMS` yang belum ada di `SW_ICONS`, dan buang entri yang tidak dipakai lagi.
/// Ikon dari exe (`sw_big_icon`), selain itu ikon jendela (`window_icon_src`), jadi jendela tanpa aplikasi tetap berikon.
/// Hanya dari thread UI.
fn sw_load_icons() {
    let px = sf(SW_BIG_ICON0).round() as i32;
    let want: Vec<(String, String, HWND)> = OV_ITEMS
        .borrow()
        .iter()
        .map(|it| {
            let (key, exe) = sw_icon_key(it);
            (key, exe, it.h)
        })
        .collect();
    let mut cache = SW_ICONS.borrow_mut();
    let mut i = 0;
    while i < cache.len() {
        // Entri `#<hwnd>` selalu diambil ulang: handle jendela bisa dipakai lagi untuk jendela lain.
        if !cache[i].0.starts_with('#') && want.iter().any(|(k, _, _)| *k == cache[i].0) {
            i += 1;
        } else {
            let (_, mut ic) = cache.remove(i);
            icon_free(&mut ic);
        }
    }
    for (key, exe, h) in want {
        if cache.iter().any(|(k, _)| *k == key) {
            continue;
        }
        if let Some(src) = sw_big_icon(&exe, px).or_else(|| window_icon_src(h, px)) {
            let mut ic = Icon::default();
            ic.src = Some(src);
            cache.push((key, ic));
        }
    }
}

/// Gambar ikon item `it` di kotak (x, y, sz): ikon besar dari cache `SW_ICONS` bila ada; selain itu ikon taskbar aplikasinya.
fn sw_draw_item_icon(it: &OvItem, x: f32, y: f32, sz: f32) {
    let (key, _) = sw_icon_key(it);
    let pos = SW_ICONS.borrow().iter().position(|(k, _)| *k == key);
    match pos {
        Some(p) => draw_icon_at(&mut SW_ICONS.borrow_mut()[p].1, x, y, sz, 1.0),
        None => {
            if it.app < 0 {
                return;
            }
            let mut apps = APPS.borrow_mut();
            let Some(a) = apps.get_mut(it.app as usize) else { return };
            draw_icon_at(&mut a.icon, x, y, sz, 1.0);
        }
    }
}

/// Gambar Alt+Tab gaya Windows: panel membulat translucent, kartu per aplikasi (judul di atas, thumbnail DWM
/// di bawahnya), sorotan pilihan. Tanpa blur sejati (hanya isian translucent). Dipanggil dari `ov_render` bila mode SWITCH.
fn ov_render_switch() {
    let ui = OV_UI.get();
    let Some(rt) = RT.borrow().as_ref().cloned() else { return };
    if ui.0.is_null() {
        return;
    }
    let rc = OV_RC.get();
    let wi = rc.right - rc.left;
    let hi = rc.bottom - rc.top;
    let (wf, hf) = (wi as f32, hi as f32);
    ensure_mem(wi, hi);
    let brc = RECT { left: 0, top: 0, right: wi, bottom: hi };
    if unsafe { rt.BindDC(MEMDC.get(), &brc) }.is_err() {
        return;
    }
    // SAFETY: rt adalah render target D2D yang valid; BindDC sudah berhasil di atas.
    unsafe {
        rt.BeginDraw();
        rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
    }
    let light = LIGHT.get();
    let c = *PALETTE.borrow();
    let sel = OV_SEL.get();
    let hot_i = OV_HOT.get();
    let rad = sf(SW_PANEL_RAD0);
    let trad = sf(SW_ROW_RAD0);
    // Blur latar (`sw_blur_capture`) digambar dulu; dengan blur, tint panel dibuat lebih tembus agar blur terlihat.
    let blur_ok = {
        let b = SW_BLUR.borrow();
        !b.is_empty() && b.len() == (wi.max(0) as usize) * (hi.max(0) as usize) * 4
    };
    if blur_ok {
        let b = SW_BLUR.borrow();
        let dst = rf(0.0, 0.0, wf, hf);
        if let Some(bmp) = d2d_bitmap_bgra(&rt, wi as u32, hi as u32, b.as_slice(), (wi * 4) as u32) {
            unsafe {
                rt.DrawBitmap(
                    &bmp,
                    Some(&dst as *const D2D_RECT_F),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                )
            };
        }
    }
    let (pa_d, pa_l): (f32, f32) = if blur_ok { (0.55, 0.60) } else { (0.82, 0.88) };
    let panel = if light { rgba(245, 245, 245, pa_l) } else { rgba(30, 30, 30, pa_d) };
    let edge = if light { rgba(0, 0, 0, 0.12) } else { rgba(255, 255, 255, 0.10) };
    let sel_fill = if light { rgba(0, 0, 0, 0.06) } else { rgba(255, 255, 255, 0.10) };
    let hot_fill = if light { rgba(0, 0, 0, 0.03) } else { rgba(255, 255, 255, 0.06) };
    fill_rr(0.0, 0.0, wf, hf, rad, panel, 1.0);
    let rr = ov_rr(0.5, 0.5, wf - 0.5, hf - 0.5, rad);
    set_color(edge, 1.0);
    ov_outline(&rr, 1.0);

    // Satu kartu per aplikasi: judul (ikon kecil + nama) di pita atas, lalu area thumbnail. Thumbnail DWM
    // digambar `ov_thumbs` di atas panel; di sini hanya sorotan, placeholder, dan ikon besar untuk jendela
    // tanpa thumbnail (diminimalkan).
    let items_n = OV_ITEMS.borrow().len();
    let thumb_bg = if light { rgba(0, 0, 0, 0.06) } else { rgba(0, 0, 0, 0.25) };
    let head_isz = sf(SW_HEAD_ICON0);
    let big_isz = sf(SW_BIG_ICON0);
    // Sorotan animasi (`sw_select`): meluncur dari kartu lama ke kartu baru dengan ease-out cubic.
    // Selama meluncur, sorotan kartu terpilih tidak digambar dulu; yang tampil hanya persegi peluncur.
    let glide_rc: Option<RECT> = SW_GLIDE.get().map(|g| {
        let el = g.t0.elapsed().as_millis();
        let t = (el as f32 / SW_GLIDE_MS as f32).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - t).powi(3);
        let mix = |a: i32, b: i32| (a as f32 + (b - a) as f32 * e).round() as i32;
        RECT {
            left: mix(g.from.left, g.to.left),
            top: mix(g.from.top, g.to.top),
            right: mix(g.from.right, g.to.right),
            bottom: mix(g.from.bottom, g.to.bottom),
        }
    });
    if let Some(g) = glide_rc {
        fill_rr(g.left as f32, g.top as f32, g.right as f32, g.bottom as f32, trad, sel_fill, 1.0);
    }
    for i in 0..items_n {
        let it = OV_ITEMS.borrow()[i];
        let cr = it.card;
        let (l, t) = (cr.left as f32, cr.top as f32);
        let (r, b) = (cr.right as f32, cr.bottom as f32);
        if b <= 0.0 || t >= hf {
            continue; // kartu di luar panel
        }
        if i as i32 == sel {
            if glide_rc.is_none() {
                fill_rr(l, t, r, b, trad, sel_fill, 1.0);
            }
        } else if i as i32 == hot_i {
            fill_rr(l, t, r, b, trad, hot_fill, 1.0);
        }
        let tr = it.thumb;
        let (tl, tt) = (tr.left as f32, tr.top as f32);
        let (tx, tb) = (tr.right as f32, tr.bottom as f32);
        // Pita judul: ikon kecil di kiri, judul di kanannya (tengah vertikal, lihat `text`).
        let iy = t + (tt - t - head_isz) / 2.0;
        sw_draw_item_icon(&it, tl, iy, head_isz);
        let name = sw_name(&it);
        text(&name, TF_S.borrow().as_ref(), tl + head_isz + sf(SW_TEXT_GAP0), t, tx, tt, c.text, 1.0);
        // Area thumbnail: placeholder; thumbnail DWM menutupinya bila ada.
        fill_rr(tl, tt, tx, tb, trad, thumb_bg, 1.0);
        if it.th == 0 {
            // Tanpa thumbnail (diminimalkan): ikon besar di tengah area.
            let cx = (tl + tx) / 2.0 - big_isz / 2.0;
            let cy = (tt + tb) / 2.0 - big_isz / 2.0;
            sw_draw_item_icon(&it, cx, cy, big_isz);
        }
    }
    let _ = unsafe { rt.EndDraw(None, None) };
    layer_present(ui, &rc, SW_ALPHA.get());
}

// ---------------------------------------------------------------- Task View

/// `TvRelayout`: hitung ulang tata letak kartu dan kartu desktop Task View.
fn tv_relayout() {
    if OV_MODE.get() != OV_TV {
        return;
    }
    ov_unthumb();
    vd_scan();
    ov_collect();
    let rc = OV_RC.get();
    let wi = rc.right - rc.left;
    let hi = rc.bottom - rc.top;
    let cw = s(176.0);
    let gap = s(14.0);
    let ih = ((cw - s(12.0)) as f32 * hi as f32 / wi as f32) as i32;
    let ch = ih + s(6.0) + s(28.0);
    let cnt = VD_COUNT.get();
    let nd = if cnt < VD_MAX { cnt + 1 } else { cnt };
    let mut x = (wi - (nd * cw + (nd - 1) * gap)) / 2;
    let y = s(26.0);
    {
        let mut dk = OV_DK.borrow_mut();
        for r in dk.iter_mut() {
            *r = ov_rc(0, 0, 0, 0); // SetRectEmpty
        }
        for k in 0..nd.max(0) as usize {
            dk[k] = ov_rc(x, y, x + cw, y + ch);
            x += cw + gap;
        }
    }
    let top = (y + ch + s(34.0)) as f32;
    let side = sf(56.0);
    let bottom = sf(48.0);
    let (gw, gh) = ov_layout(wi as f32 - 2.0 * side, hi as f32 - top - bottom, sf(240.0));
    ov_offset(
        ((wi as f32 - gw) / 2.0) as i32,
        (top + (hi as f32 - top - bottom - gh) / 2.0) as i32,
    );
    let n = OV_ITEMS.borrow().len() as i32;
    if OV_SEL.get() >= n {
        OV_SEL.set(n - 1);
    }
    OV_HOT.set(-1);
    ov_thumbs();
    ov_render();
}

/// `TvOpen`: buka Task View pada monitor jendela aktif.
fn tv_open() {
    if OV_MODE.get() == OV_SWITCH {
        ov_close();
        at_reset_sticky(); // Win+Tab menutup panel Alt+Tab tetap: status mode tetap ikut diatur ulang
    }
    if OV_MODE.get() == OV_TV {
        return;
    }
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let fg = unsafe { GetForegroundWindow() };
    unsafe {
        let _ = GetMonitorInfoW(MonitorFromWindow(fg, MONITOR_DEFAULTTOPRIMARY), &mut mi);
    }
    OV_RC.set(mi.rcMonitor);
    OV_MODE.set(OV_TV);
    OV_SEL.set(0);
    OV_HOT.set(-1);
    OV_DK_HOT.set(-1);
    let bg = ov_make_wnd(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE);
    let ui = ov_make_wnd(WS_EX_LAYERED);
    let th = ov_make_wnd(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE);
    OV_BG.set(bg);
    OV_UI.set(ui);
    OV_TH.set(th);
    if ui.0.is_null() || th.0.is_null() {
        ov_close();
        return;
    }
    let desk = DESK.get();
    let mut bg_th: isize = 0;
    if !bg.0.is_null() && !desk.0.is_null() {
        if let Ok(v) = unsafe { DwmRegisterThumbnail(bg, desk) } {
            bg_th = v;
        }
    }
    OV_BG_TH.set(bg_th);
    if bg_th != 0 {
        let mut ds = RECT::default();
        let _ = unsafe { GetWindowRect(desk, &mut ds) };
        let rc = OV_RC.get();
        let src = ov_rc(rc.left - ds.left, rc.top - ds.top, rc.right - ds.left, rc.bottom - ds.top);
        let dst = ov_rc(0, 0, rc.right - rc.left, rc.bottom - rc.top);
        ov_thumb_props(bg_th, dst, Some(&src), false, true);
    }
    if !bg.0.is_null() {
        let _ = unsafe { ShowWindow(bg, SW_SHOWNA) };
    }
    tv_relayout();
    unsafe {
        let _ = ShowWindow(ui, SW_SHOW);
        let _ = ShowWindow(th, SW_SHOWNA);
        let _ = SetWindowPos(th, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        let _ = SetForegroundWindow(ui); // Esc, panah, Enter
    }
}

/// `TvClose`: tutup overlay; fokus ke `activate` bila ada, atau ke jendela sebelumnya (TKT_FOCUS).
fn tv_close(activate: Option<HWND>) {
    ov_close();
    match activate {
        Some(a) if !a.0.is_null() && unsafe { IsWindow(Some(a)) }.as_bool() => activate_window(a),
        _ => {
            let tk = TK_MSG.get();
            if !tk.0.is_null() {
                unsafe { SetTimer(Some(tk), TKT_FOCUS as usize, 30, None) };
            }
        }
    }
}

/// `TvToggle`: buka/tutup Task View (dipanggil dari core_c3 saat tombol Win+Tab / gestur).
pub fn tv_toggle() {
    if !SHELL_MODE.get() || TK_MSG.get().0.is_null() {
        return;
    }
    if OV_MODE.get() == OV_TV {
        tv_close(None);
    } else {
        tv_open();
    }
}

/// Apakah Task View sedang terbuka (`TvIsOpen`).
pub fn tv_is_open() -> bool {
    OV_MODE.get() == OV_TV
}

/// Mode overlay saat ini (`OV_NONE` / `OV_SWITCH` / `OV_TV`); dibaca diagnostik (`g_ovMode` di C).
pub fn ov_mode() -> i32 {
    OV_MODE.get()
}

/// `OvHitItem`: indeks kartu jendela di titik (x, y) dan apakah titik di tombol X.
fn ov_hit_item(x: i32, y: i32) -> (i32, bool) {
    let items = OV_ITEMS.borrow();
    for (i, it) in items.iter().enumerate() {
        if !ov_pt_in(&it.card, x, y) {
            continue;
        }
        let xr = ov_close_rect(&it.card);
        let on_x = OV_MODE.get() == OV_TV && ov_pt_in(&xr, x, y);
        return (i as i32, on_x);
    }
    (-1, false)
}

/// `OvHitDesk`: indeks kartu desktop (0..=VD_MAX) di titik (x, y) dan apakah titik di tombol X.
fn ov_hit_desk(x: i32, y: i32) -> (i32, bool) {
    if OV_MODE.get() != OV_TV {
        return (-1, false);
    }
    let dk = OV_DK.borrow();
    for k in 0..=(VD_MAX as usize) {
        if ov_rc_empty(&dk[k]) || !ov_pt_in(&dk[k], x, y) {
            continue;
        }
        let r = dk[k];
        let xr = ov_rc(r.right - s(28.0), r.bottom - s(28.0), r.right - s(2.0), r.bottom - s(2.0));
        let cnt = VD_COUNT.get();
        let on_x = (k as i32) < cnt && cnt > 1 && ov_pt_in(&xr, x, y);
        return (k as i32, on_x);
    }
    (-1, false)
}

/// Handle jendela kartu ke-`i` (null bila indeks tidak ada).
fn ov_item_h(i: i32) -> HWND {
    if i < 0 {
        return ov_null();
    }
    let items = OV_ITEMS.borrow();
    match items.get(i as usize) {
        Some(it) => it.h,
        None => ov_null(),
    }
}

/// Minta jendela kartu ditutup lalu jadwalkan penyusunan ulang Task View (350 ms).
fn ov_post_close(h: HWND) {
    unsafe {
        let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
        SetTimer(Some(TK_MSG.get()), TKT_TVREBUILD as usize, 350, None);
    }
}

/// `TvMenu`: menu konteks kartu jendela (pindah ke desktop lain, atau tutup jendela).
fn tv_menu(i: i32, pt: POINT) {
    let h = ov_item_h(i);
    let Ok(m) = (unsafe { CreatePopupMenu() }) else {
        return;
    };
    let Ok(sub) = (unsafe { CreatePopupMenu() }) else {
        return;
    };
    let cnt = VD_COUNT.get();
    let cur = VD_CUR.get();
    for k in 0..cnt {
        if k == cur {
            continue;
        }
        let t = to_wide_z(&format!("Desktop {}", k + 1));
        let _ = unsafe { AppendMenuW(sub, MF_STRING, (100 + k) as usize, pcw(&t)) };
    }
    if cnt < VD_MAX {
        let t = to_wide_z("Desktop baru");
        let _ = unsafe { AppendMenuW(sub, MF_STRING, (100 + cnt) as usize, pcw(&t)) };
    }
    let t_pindah = to_wide_z("Pindahkan ke");
    let _ = unsafe { AppendMenuW(m, MF_POPUP, sub.0 as usize, pcw(&t_pindah)) };
    let _ = unsafe { AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null()) };
    let t_tutup = to_wide_z("Tutup jendela");
    let _ = unsafe { AppendMenuW(m, MF_STRING, 1, pcw(&t_tutup)) };
    let c = track_menu(m, OV_UI.get(), pt, false);
    let _ = unsafe { DestroyMenu(m) };
    if OV_MODE.get() != OV_TV {
        return;
    }
    if c == 1 {
        ov_post_close(h);
    } else if c >= 100 {
        vd_move(h, c - 100);
        let _ = unsafe { KillTimer(Some(TK_MSG.get()), TKT_FOCUS as usize) };
        tv_relayout();
    }
}

/// `OvMouse`: pesan mouse di jendela UI overlay; (`x`, `y`) relatif ke jendela.
fn ov_mouse(m: u32, x: i32, y: i32) {
    let (hi, ix) = ov_hit_item(x, y);
    let (hd, dx) = ov_hit_desk(x, y);
    if m == WM_MOUSEMOVE {
        if !OV_TRACK.get() {
            let mut t = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: OV_UI.get(),
                dwHoverTime: 0,
            };
            OV_TRACK.set(unsafe { TrackMouseEvent(&mut t) }.is_ok());
        }
        if hi != OV_HOT.get() || ix != OV_HOT_X.get() || hd != OV_DK_HOT.get() || dx != OV_DK_HOT_X.get() {
            OV_HOT.set(hi);
            OV_HOT_X.set(ix);
            OV_DK_HOT.set(hd);
            OV_DK_HOT_X.set(dx);
            if OV_MODE.get() == OV_SWITCH && hi >= 0 && hi != OV_SEL.get() {
                OV_SEL.set(hi);
                SW_GLIDE.set(None); // hover langsung pindah, tanpa animasi
                ov_acc_sel_changed();
            }
            ov_render();
        }
        return;
    }
    if m == WM_LBUTTONUP {
        if OV_MODE.get() == OV_SWITCH {
            if hi >= 0 {
                OV_SEL.set(hi);
                at_reset_active();
                sw_commit(0);
            }
            return;
        }
        if hi >= 0 && ix {
            ov_post_close(ov_item_h(hi));
            return;
        }
        if hi >= 0 {
            tv_close(Some(ov_item_h(hi)));
            return;
        }
        if hd >= 0 && dx {
            vd_close(hd);
            tv_relayout();
            return;
        }
        if hd >= 0 && hd == VD_COUNT.get() {
            vd_new(false);
            tv_relayout();
            return;
        }
        if hd >= 0 {
            vd_go(hd, false);
            tv_relayout();
            return;
        }
        tv_close(None);
        return;
    }
    if m == WM_MBUTTONUP && OV_MODE.get() == OV_TV && hi >= 0 {
        ov_post_close(ov_item_h(hi));
        return;
    }
    if m == WM_RBUTTONUP && OV_MODE.get() == OV_TV && hi >= 0 {
        let rc = OV_RC.get();
        let pt = POINT { x: x + rc.left, y: y + rc.top };
        tv_menu(hi, pt);
    }
}

/// `TvKey`: tombol di jendela UI Task View (Esc, panah, Tab, Enter, Spasi, Delete).
fn tv_key(vk: u16) {
    if OV_MODE.get() != OV_TV {
        return;
    }
    if vk == VK_ESCAPE.0 {
        tv_close(None);
        return;
    }
    let n = OV_ITEMS.borrow().len() as i32;
    if n == 0 {
        return;
    }
    let sel = OV_SEL.get();
    if vk == VK_RIGHT.0 || vk == VK_DOWN.0 || vk == VK_TAB.0 {
        OV_SEL.set((sel + 1) % n);
        ov_render();
    } else if vk == VK_LEFT.0 || vk == VK_UP.0 {
        OV_SEL.set((sel - 1 + n) % n);
        ov_render();
    } else if (vk == VK_RETURN.0 || vk == VK_SPACE.0) && sel >= 0 {
        tv_close(Some(ov_item_h(sel)));
    } else if vk == VK_DELETE.0 && sel >= 0 {
        ov_post_close(ov_item_h(sel));
    }
}

/// `TkTask`: pesan tugas (WM_W12_TASK) yang dijalankan di thread UI.
fn tk_task(k: i32, lp: isize) {
    match k {
        TK_SW_STEP if lp == SW_VSTEP_UP => sw_vstep(false),
        TK_SW_STEP if lp == SW_VSTEP_DOWN => sw_vstep(true),
        TK_SW_STEP => sw_step(if lp > 0 { 1 } else { -1 }, lp == 1 || lp == -1),
        TK_SW_COMMIT => sw_commit(lp as usize),
        TK_SW_CANCEL => {
            if OV_MODE.get() == OV_SWITCH {
                ov_close();
            }
        }
        TK_TASKVIEW => {
            if lp == 0 {
                tv_toggle();
            } else if lp == 1 {
                tv_open();
            } else if tv_is_open() {
                tv_close(None);
            }
        }
        TK_DESK_GO => {
            let n = w12_vd_step(VD_CUR.get(), VD_COUNT.get(), lp as i32);
            if tv_is_open() {
                vd_go(n, false);
                tv_relayout();
            } else if n != VD_CUR.get() {
                vd_go(n, true);
            } else {
                vd_hud();
            }
        }
        TK_DESK_NEW => {
            if tv_is_open() {
                vd_new(false);
                tv_relayout();
            } else {
                vd_new(true);
            }
        }
        TK_DESK_CLOSE => {
            if VD_COUNT.get() > 1 {
                vd_close(VD_CUR.get());
                if tv_is_open() {
                    tv_relayout();
                }
            }
        }
        TK_MOVE_WIN => {
            let fg = unsafe { GetForegroundWindow() };
            if fg.0.is_null() || !is_task_window(fg) {
                return;
            }
            let mut n = VD_CUR.get() + lp as i32;
            if n < 0 {
                return;
            }
            if n >= VD_COUNT.get() {
                if VD_COUNT.get() >= VD_MAX {
                    return;
                }
                n = VD_COUNT.get();
            }
            vd_move(fg, n);
            vd_go(n, true);
        }
        TK_GEST_UP => tv_open(),
        TK_GEST_DOWN => {
            if tv_is_open() {
                tv_close(None);
            } else {
                let _ = unsafe {
                    PostMessageW(Some(MSG.get()), WM_W12_SHELLKEY, WPARAM('D' as usize), LPARAM(0))
                };
            }
        }
        _ => {}
    }
}

/// Kirim pesan tugas (WM_W12_TASK) ke jendela pesan tersembunyi; aman dipanggil dari thread kait.
fn tk_post(k: i32, lp: isize) {
    let _ = unsafe { PostMessageW(Some(TK_MSG.get()), WM_W12_TASK, WPARAM(k as usize), LPARAM(lp)) };
}

// ---------- Aksesibilitas panel Alt+Tab (MSAA / IAccessible) ----------
// Anak ID 0 = panel (LIST); anak 1..=n = kartu (LISTITEM) sesuai urutan OV_ITEMS.
// Aktif hanya di mode SWITCH. Klien (mis. pembaca layar) mengambilnya lewat WM_GETOBJECT.

const ACC_OBJID_CLIENT: i32 = -4;
const ACC_CHILDID_SELF: i32 = 0;
const ACC_ROLE_LIST: i32 = 0x21;
const ACC_ROLE_LISTITEM: i32 = 0x22;
const ACC_ST_SELECTED: i32 = 0x2;
const ACC_ST_FOCUSED: i32 = 0x4;
const ACC_ST_SELECTABLE: i32 = 0x0020_0000;
const ACC_ST_OFFSCREEN: i32 = 0x0001_0000;
const ACC_NAV_NEXT: i32 = 5;
const ACC_NAV_PREV: i32 = 6;
const ACC_NAV_FIRST: i32 = 7;
const ACC_NAV_LAST: i32 = 8;
const ACC_SELFLAG_TAKESELECTION: i32 = 2;
const ACC_EVT_SELECTION: u32 = 0x8006;

/// 0 = belum dicoba, 1 = siap (STA), 2 = gagal (thread sudah MTA; aksesibilitas dinonaktifkan).
static ACC_COM: SyncCell<Cell<u8>> = SyncCell::new(Cell::new(0));

/// Masuk STA sekali untuk thread UI. Tidak ada CoUninitialize: thread UI hidup sepanjang proses.
fn ov_acc_com_ready() -> bool {
    if ACC_COM.get() == 0 {
        let ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        ACC_COM.set(if ok { 1 } else { 2 });
    }
    ACC_COM.get() == 1
}

/// VARIANT berisi VT_I4.
fn acc_var_i4(v: i32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_I4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { lVal: v },
            }),
        },
    }
}

/// ID anak yang diminta klien. Selain VT_I4 dianggap diri sendiri (0).
fn acc_child_of(v: &VARIANT) -> i32 {
    unsafe {
        let inner = &v.Anonymous.Anonymous;
        if inner.vt == VT_I4 {
            inner.Anonymous.lVal
        } else {
            ACC_CHILDID_SELF
        }
    }
}

/// Jumlah anak yang berlaku. Nol bila bukan mode SWITCH.
fn acc_count() -> i32 {
    if OV_MODE.get() == OV_SWITCH {
        OV_ITEMS.borrow().len() as i32
    } else {
        0
    }
}

/// Apakah `c` menunjuk panel (0) atau kartu yang berlaku (1..=n).
fn acc_ok(c: i32) -> bool {
    c == ACC_CHILDID_SELF || (c >= 1 && c <= acc_count())
}

fn acc_err() -> windows::core::Error {
    windows::core::Error::from(E_INVALIDARG)
}

/// Rectangle layar anak `c`: panel = OV_RC; kartu = OV_RC + posisi kartu (window-relatif).
fn acc_rect(c: i32) -> Option<RECT> {
    if !acc_ok(c) {
        return None;
    }
    let base = OV_RC.get();
    if c == ACC_CHILDID_SELF {
        return Some(base);
    }
    let items = OV_ITEMS.borrow();
    let it = items.get((c - 1) as usize)?;
    Some(RECT {
        left: base.left + it.card.left,
        top: base.top + it.card.top,
        right: base.left + it.card.right,
        bottom: base.top + it.card.bottom,
    })
}

fn acc_name(c: i32) -> Option<String> {
    if c == ACC_CHILDID_SELF {
        return Some("Pengalih aplikasi".to_string());
    }
    if !acc_ok(c) {
        return None;
    }
    let items = OV_ITEMS.borrow();
    items.get((c - 1) as usize).map(sw_name)
}

/// Anak yang terpilih sebagai VT_I4 (ID 1-based), atau VT_EMPTY bila tidak ada.
fn acc_sel_var() -> VARIANT {
    let s = OV_SEL.get();
    if s >= 0 && s < acc_count() {
        acc_var_i4(s + 1)
    } else {
        VARIANT::default()
    }
}

/// Objek IAccessible panel Alt+Tab. Tanpa state: semua data dibaca dari OV_* saat dipanggil.
#[windows::core::implement(IAccessible)]
struct OvAcc;

impl IDispatch_Impl for OvAcc_Impl {
    fn GetTypeInfoCount(&self) -> windows::core::Result<u32> {
        Ok(0)
    }
    fn GetTypeInfo(&self, _itinfo: u32, _lcid: u32) -> windows::core::Result<ITypeInfo> {
        Err(E_NOTIMPL.into())
    }
    fn GetIDsOfNames(
        &self,
        _riid: *const GUID,
        _rgsznames: *const PCWSTR,
        _cnames: u32,
        _lcid: u32,
        _rgdispid: *mut i32,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn Invoke(
        &self,
        _dispidmember: i32,
        _riid: *const GUID,
        _lcid: u32,
        _wflags: DISPATCH_FLAGS,
        _pdispparams: *const DISPPARAMS,
        _pvarresult: *mut VARIANT,
        _pexcepinfo: *mut EXCEPINFO,
        _puargerr: *mut u32,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
}

impl IAccessible_Impl for OvAcc_Impl {
    fn accParent(&self) -> windows::core::Result<IDispatch> {
        Err(E_NOTIMPL.into())
    }
    fn accChildCount(&self) -> windows::core::Result<i32> {
        Ok(acc_count())
    }
    fn get_accChild(&self, _varchild: &VARIANT) -> windows::core::Result<IDispatch> {
        Err(E_NOTIMPL.into())
    }
    fn get_accName(&self, varchild: &VARIANT) -> windows::core::Result<BSTR> {
        acc_name(acc_child_of(varchild)).map(BSTR::from).ok_or_else(acc_err)
    }
    fn get_accValue(&self, _varchild: &VARIANT) -> windows::core::Result<BSTR> {
        Err(E_NOTIMPL.into())
    }
    fn get_accDescription(&self, _varchild: &VARIANT) -> windows::core::Result<BSTR> {
        Err(E_NOTIMPL.into())
    }
    fn get_accRole(&self, varchild: &VARIANT) -> windows::core::Result<VARIANT> {
        let c = acc_child_of(varchild);
        if !acc_ok(c) {
            return Err(acc_err());
        }
        Ok(acc_var_i4(if c == ACC_CHILDID_SELF { ACC_ROLE_LIST } else { ACC_ROLE_LISTITEM }))
    }
    fn get_accState(&self, varchild: &VARIANT) -> windows::core::Result<VARIANT> {
        let c = acc_child_of(varchild);
        if !acc_ok(c) {
            return Err(acc_err());
        }
        if c == ACC_CHILDID_SELF {
            return Ok(acc_var_i4(0));
        }
        let mut st = ACC_ST_SELECTABLE;
        if OV_SEL.get() == c - 1 {
            st |= ACC_ST_SELECTED | ACC_ST_FOCUSED;
        }
        if let Some(r) = acc_rect(c) {
            let b = OV_RC.get();
            if r.right <= b.left || r.left >= b.right || r.bottom <= b.top || r.top >= b.bottom {
                st |= ACC_ST_OFFSCREEN;
            }
        }
        Ok(acc_var_i4(st))
    }
    fn get_accHelp(&self, _varchild: &VARIANT) -> windows::core::Result<BSTR> {
        Err(E_NOTIMPL.into())
    }
    fn get_accHelpTopic(&self, _pszhelpfile: *mut BSTR, _varchild: &VARIANT) -> windows::core::Result<i32> {
        Err(E_NOTIMPL.into())
    }
    fn get_accKeyboardShortcut(&self, _varchild: &VARIANT) -> windows::core::Result<BSTR> {
        Err(E_NOTIMPL.into())
    }
    fn get_accDefaultAction(&self, varchild: &VARIANT) -> windows::core::Result<BSTR> {
        let c = acc_child_of(varchild);
        if !acc_ok(c) {
            return Err(acc_err());
        }
        if c == ACC_CHILDID_SELF {
            return Err(E_NOTIMPL.into());
        }
        Ok(BSTR::from("Pilih"))
    }
    fn accFocus(&self) -> windows::core::Result<VARIANT> {
        Ok(acc_sel_var())
    }
    fn accSelection(&self) -> windows::core::Result<VARIANT> {
        Ok(acc_sel_var())
    }
    fn accSelect(&self, flagsselect: i32, varchild: &VARIANT) -> windows::core::Result<()> {
        let c = acc_child_of(varchild);
        if (flagsselect & ACC_SELFLAG_TAKESELECTION) == 0 || c < 1 || !acc_ok(c) {
            return Err(acc_err());
        }
        sw_select(c - 1);
        if OV_SHOWN.get() {
            ov_render();
        }
        Ok(())
    }
    fn accLocation(
        &self,
        pxleft: *mut i32,
        pytop: *mut i32,
        pcxwidth: *mut i32,
        pcyheight: *mut i32,
        varchild: &VARIANT,
    ) -> windows::core::Result<()> {
        let r = acc_rect(acc_child_of(varchild)).ok_or_else(acc_err)?;
        unsafe {
            if !pxleft.is_null() {
                *pxleft = r.left;
            }
            if !pytop.is_null() {
                *pytop = r.top;
            }
            if !pcxwidth.is_null() {
                *pcxwidth = r.right - r.left;
            }
            if !pcyheight.is_null() {
                *pcyheight = r.bottom - r.top;
            }
        }
        Ok(())
    }
    fn accNavigate(&self, navdir: i32, varstart: &VARIANT) -> windows::core::Result<VARIANT> {
        let c = acc_child_of(varstart);
        if !acc_ok(c) {
            return Err(acc_err());
        }
        let n = acc_count();
        let r = match navdir {
            ACC_NAV_FIRST if c == ACC_CHILDID_SELF && n > 0 => acc_var_i4(1),
            ACC_NAV_LAST if c == ACC_CHILDID_SELF && n > 0 => acc_var_i4(n),
            ACC_NAV_NEXT if c >= 1 && c < n => acc_var_i4(c + 1),
            ACC_NAV_PREV if c > 1 => acc_var_i4(c - 1),
            _ => VARIANT::default(),
        };
        Ok(r)
    }
    fn accHitTest(&self, xleft: i32, ytop: i32) -> windows::core::Result<VARIANT> {
        let b = OV_RC.get();
        if OV_MODE.get() != OV_SWITCH || !ov_pt_in(&b, xleft, ytop) {
            return Ok(VARIANT::default());
        }
        let (hi, _) = ov_hit_item(xleft - b.left, ytop - b.top);
        Ok(if hi >= 0 { acc_var_i4(hi + 1) } else { acc_var_i4(ACC_CHILDID_SELF) })
    }
    fn accDoDefaultAction(&self, varchild: &VARIANT) -> windows::core::Result<()> {
        let c = acc_child_of(varchild);
        if !acc_ok(c) {
            return Err(acc_err());
        }
        if c == ACC_CHILDID_SELF {
            return Err(E_NOTIMPL.into());
        }
        OV_SEL.set(c - 1);
        at_reset_active();
        sw_commit(0);
        Ok(())
    }
    fn put_accName(&self, _varchild: &VARIANT, _szname: &BSTR) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn put_accValue(&self, _varchild: &VARIANT, _szvalue: &BSTR) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
}

/// Jawaban WM_GETOBJECT untuk objek klien panel. None = tidak ditangani (diteruskan ke DefWindowProcW).
fn ov_wm_getobject(wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    if OV_MODE.get() != OV_SWITCH || lp.0 as i32 != ACC_OBJID_CLIENT || !ov_acc_com_ready() {
        return None;
    }
    let acc: IAccessible = OvAcc.into();
    Some(unsafe { LresultFromObject(&IAccessible::IID, wp, &acc) })
}

/// Beri tahu klien aksesibilitas bahwa kartu terpilih berubah (EVENT_OBJECT_SELECTION).
/// Jangan dipanggil saat ada RefCell yang sedang dipinjam mutable: klien bisa memanggil balik lewat WM_GETOBJECT.
fn ov_acc_sel_changed() {
    let ui = OV_UI.get();
    if OV_MODE.get() != OV_SWITCH || !OV_SHOWN.get() || ui.0.is_null() {
        return;
    }
    let sel = OV_SEL.get();
    if sel < 0 {
        return;
    }
    unsafe { NotifyWinEvent(ACC_EVT_SELECTION, ui, ACC_OBJID_CLIENT, sel + 1) };
}

/// `TkProc`: prosedur jendela kelas `Win12Tasks` (jendela pesan, overlay UI, lapisan tembus, HUD).
unsafe extern "system" fn tk_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_GETOBJECT && h == OV_UI.get() {
        if let Some(r) = ov_wm_getobject(wp, lp) {
            return r;
        }
    }
    match m {
        WM_NCHITTEST => {
            if h == OV_TH.get() || h == OV_BG.get() || h == HUD.get() {
                return LRESULT(HTTRANSPARENT as isize); // klik tembus ke OV_UI
            }
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let _ = unsafe { ValidateRect(Some(h), None) };
            return LRESULT(0);
        }
        WM_MOUSEMOVE | WM_LBUTTONUP | WM_MBUTTONUP | WM_RBUTTONUP => {
            if h == OV_UI.get() {
                let x = (lp.0 & 0xFFFF) as i16 as i32; // GET_X_LPARAM
                let y = ((lp.0 >> 16) & 0xFFFF) as i16 as i32; // GET_Y_LPARAM
                ov_mouse(m, x, y);
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            if h == OV_UI.get() {
                OV_TRACK.set(false);
                OV_HOT.set(-1);
                OV_DK_HOT.set(-1);
                ov_render();
            }
            return LRESULT(0);
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if h == OV_UI.get() {
                tv_key(wp.0 as u16);
            }
            return LRESULT(0);
        }
        WM_ACTIVATE => {
            if h == OV_UI.get() && (wp.0 & 0xFFFF) as u32 == WA_INACTIVE && OV_MODE.get() == OV_TV {
                tk_post(TK_TASKVIEW, 2);
            }
            return LRESULT(0);
        }
        WM_W12_WALL => {
            desk_wall_apply(wp, HBITMAP(lp.0 as *mut std::ffi::c_void));
            return LRESULT(0);
        }
        WM_W12_TASK => {
            tk_task(wp.0 as i32, lp.0);
            return LRESULT(0);
        }
        WM_TIMER => {
            // Deviasi dari C (KillTimer(h, wp)): timer selalu dipasang pada TK_MSG, jadi id dihapus dari sana.
            let id = wp.0;
            let _ = unsafe { KillTimer(Some(TK_MSG.get()), id) };
            if id == TKT_SWSHOW as usize {
                sw_show();
            } else if id == TKT_SWFADE as usize {
                sw_fade_tick();
            } else if id == TKT_SWGLIDE as usize {
                sw_glide_tick();
            } else if id == TKT_FOCUS as usize {
                vd_focus_now();
            } else if id == TKT_HUD as usize {
                let hud = HUD.get();
                if !hud.0.is_null() {
                    let _ = unsafe { DestroyWindow(hud) };
                    HUD.set(ov_null());
                }
                if OV_MODE.get() == OV_NONE {
                    release_mem();
                    trim_soon();
                }
            } else if id == TKT_TVREBUILD as usize {
                tv_relayout();
            }
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

/// `TasksStart`: daftarkan kelas dan buat jendela pesan tersembunyi, lalu muat keadaan desktop virtual.
pub fn tasks_start() {
    if !TK_MSG.get().0.is_null() {
        return;
    }
    // SAFETY: WNDCLASSEXW adalah struct POD; nol berarti None/null untuk setiap pointer dan handle.
    let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(tk_proc);
    wc.hInstance = HI.get();
    wc.lpszClassName = w!("Win12Tasks");
    // SAFETY: IDC_ARROW adalah kursor sistem.
    if let Ok(cur) = unsafe { LoadCursorW(None, IDC_ARROW) } {
        wc.hCursor = cur;
    }
    // Gagal bila kelas sudah terdaftar dari mode shell sebelumnya: diabaikan (sama seperti C).
    let _ = unsafe { RegisterClassExW(&wc) };
    let tk = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("Win12Tasks"),
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(HI.get()),
            None,
        )
    }
    .unwrap_or_default();
    TK_MSG.set(tk);
    vd_recover();
    vd_scan();
    desk_wall_load();
    gest_start(); // gestur touchpad: no-op bila [gestures] enabled=0 atau tanpa touch pad
}

/// `TasksStop`: hentikan gestur, tutup overlay, tampilkan lagi jendela yang disembunyikan, lalu bersihkan.
pub fn tasks_stop() {
    gest_stop(); // dulu: tidak ada gestur baru saat teardown
    ov_close();
    let n = VD_N.get().max(0) as usize;
    {
        // VD: array VdWin (field h: HWND, hid: bool), belum dideklarasikan di Rust (lihat laporan).
        let mut vd = VD.borrow_mut();
        for i in 0..n {
            if vd[i].hid {
                let _ = unsafe { ShowWindowAsync(vd[i].h, SW_SHOWNA) };
                vd[i].hid = false;
            }
        }
    }
    VD_N.set(0);
    VD_COUNT.set(1);
    VD_CUR.set(0);
    vd_save(); // tanpa entri tersembunyi: berkas vdesk-hidden dihapus
    WALL_GEN.fetch_add(1, Ordering::SeqCst); // hasil wallpaper yang terlambat diabaikan (desk_wall_apply)
    AT_STATE.set(W12AltTab::default());
    let hud = HUD.get();
    if !hud.0.is_null() {
        let _ = unsafe { DestroyWindow(hud) };
        HUD.set(ov_null());
    }
    let tk = TK_MSG.get();
    if !tk.0.is_null() {
        let _ = unsafe { DestroyWindow(tk) };
        TK_MSG.set(ov_null());
    }
}

/// `TasksKey`: dipanggil dari kait keyboard (thread kait); hanya membaca state dan memposting WM_W12_TASK.
/// Hasil: 0 = lewatkan, 1 = telan, 2 = telan + kombinasi Win (pemanggil mengatur agar Start tidak terbuka).
pub fn tasks_key(k: &KBDLLHOOKSTRUCT, wp: usize, win_down: bool) -> i32 {
    if TK_MSG.get().0.is_null() {
        return 0;
    }
    let vk = k.vkCode;
    let down = wp == WM_KEYDOWN as usize || wp == WM_SYSKEYDOWN as usize;
    // SAFETY: GetAsyncKeyState hanya membaca status tombol.
    let ctrl = unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) } < 0;
    let shift = unsafe { GetAsyncKeyState(VK_SHIFT.0 as i32) } < 0;
    let alt = (k.flags.0 & LLKHF_ALTDOWN.0) != 0;
    if win_down && down {
        let a = w12_win_task_key(vk, ctrl, shift, alt);
        if a != W12WK_NONE {
            match a {
                W12WK_TASKVIEW => tk_post(TK_TASKVIEW, 0),
                W12WK_DESK_PREV => tk_post(TK_DESK_GO, -1),
                W12WK_DESK_NEXT => tk_post(TK_DESK_GO, 1),
                W12WK_DESK_NEW => tk_post(TK_DESK_NEW, 0),
                W12WK_DESK_CLOSE => tk_post(TK_DESK_CLOSE, 0),
                W12WK_MOVE_PREV => tk_post(TK_MOVE_WIN, -1),
                W12WK_MOVE_NEXT => tk_post(TK_MOVE_WIN, 1),
                _ => {}
            }
            return 2;
        }
    }
    let mut st = AT_STATE.get();
    let alt_vk = st.alt_vk; // dibaca dulu: w12_alt_tab_key mereset state saat COMMIT
    let a = w12_alt_tab_key(&mut st, vk, down, alt, shift, ctrl);
    AT_STATE.set(st);
    match a {
        W12AT_STEP_FWD => tk_post(TK_SW_STEP, 1),
        W12AT_STEP_BACK => tk_post(TK_SW_STEP, -1),
        W12AT_UP => tk_post(TK_SW_STEP, SW_VSTEP_UP),
        W12AT_DOWN => tk_post(TK_SW_STEP, SW_VSTEP_DOWN),
        W12AT_CANCEL => tk_post(TK_SW_CANCEL, 0),
        W12AT_COMMIT => tk_post(TK_SW_COMMIT, alt_vk as isize),
        _ => {}
    }
    if a == W12AT_PASS {
        0
    } else {
        1
    }
}

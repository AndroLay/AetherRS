//! Sumber: win12shell.cpp:1330–2635 (owner C2: media, item dan pil, mica, gambar pil).
//! Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Stub lintas-owner sudah diganti fungsi pub (widgets.rs, notch.rs).

use super::prelude::*;

use std::cell::{Cell, RefCell};

use windows::core::Interface;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::SystemInformation::GetLocalTime;

use crate::log::log;
use crate::sync::SyncCell;

use super::core_c1::{decode_art_bytes, icon_free, kill_timer, s, set_timer, sf, wstr_eq, MEDIA};
use super::media_worker;
use super::notch::notch_invalidate;
use super::state::{
    ART_PX, BAR_H, BAR_H_SMALL, G_COUNT, MAXITEMS, MAXPILLS, MODE, MODE_DOCK, MSG, NITEMS, P_CAL, P_SYS,
    SLOT, SLOT_SMALL, SMALL, TRAY_SLOT, TRAY_SLOT_SMALL, T_MEDIA, WG_SCALE,
};
use super::core_c1::{measure_text, APPS, SEARCH, TRAY_CLOCK};
use super::state::{
    DATE, DATE_SHORT, DESK_LAYER, DESK_Z_DIRTY, FG_VALID, FLY_OPEN, GAP, G_CENTER, G_LEFT, G_NOTE,
    G_RIGHT, G_TRAY, HAS_BAT, IT_APP, IT_BAT, IT_BELL, IT_CHEV, IT_CLOCK, IT_MEDIA_MAIN,
    IT_MEDIA_NEXT, IT_MEDIA_PLAY, IT_MEDIA_PREV, IT_NET, IT_SEARCH, IT_START, IT_TASKVIEW,
    IT_TCLOCK, IT_USB, IT_VOL, MARGIN_B, MODE_CLASSIC, MODE_SEGMENTED, NBOTTOM, PANEL_OPEN,
    PEEK, PILL_PAD, POP_OPEN, P_CLOCK, P_MEDIA, P_TOP, QUIET, TF_B, TF_HUGE, TF_SS, TF_TRAY, TF_TZ,
    TF_XS, TIME, TIME_TZ, TZ, USB_COUNT,
};
use windows::core::{w, PCWSTR};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetClassNameW, GetSystemMetrics, GetWindow, GetWindowLongPtrW, SetWindowPos,
    GWL_EXSTYLE, GW_HWNDPREV, HWND_BOTTOM, HWND_TOP, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, WS_EX_TOPMOST,
};
use windows::core::BOOL;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, ShowWindow, HWND_TOPMOST, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE,
};
use super::core_c1::{
    add_running_app, is_task_window, match_app, w12_invalidate, window_identity, WC,
};
use super::state::{
    DRAG_CAND, DRAG_ITEM, DRAG_ON, FULLSCREEN, FULLSCREEN_START_PEEK, HOT, MAXW, QUITTING, T_ANIM,
};
use crate::ui_logic::w12_should_show_pill;

// impor batch gambar (C++ 1854-2003)
use super::core_c1::{
    d2d_bitmap_bgra, draw_icon_at, fill_rr, glyph, icon_bitmap, rf, rgba, set_color, src_px,
    tile_margin, Icon, PALETTE, WIFI_Q,
};
use super::state::{
    BAT, BR, CHARGING, D2D, LIGHT, MUTE, NET, NET_BOTH, NET_ETH, NET_WIFI, RT, RT_GEN, START_BRUSH,
    TF_GLYPH, TF_GLYPH_MD, VOL, WIC,
};
use windows::Win32::Graphics::Imaging::{
    GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, IWICPalette,
    WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
};
// PORT: `windows::Foundation::Numerics` butuh fitur Cargo `Foundation_Numerics` (kini aktif di Cargo.toml).
// `Vec2` dan `Mat32` di bawah ber-`#[repr(C)]` dengan field dan urutan sama dengan
// `windows_numerics::Vector2` dan `Matrix3x2` (windows-numerics 0.2.0, bindings.rs). Vektor dikonversi ke tipe
// API D2D lewat `nv2!` (transmute; ukuran diperiksa kompiler), matriks lewat pointer cast.
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D_RECT_F,
};
use windows::Win32::Graphics::Direct2D::{
    ID2D1Bitmap, ID2D1Brush, ID2D1Geometry, ID2D1StrokeStyle, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
    D2D1_ROUNDED_RECT,
};

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Vec2 {
    pub X: f32,
    pub Y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Mat32 {
    pub M11: f32,
    pub M12: f32,
    pub M21: f32,
    pub M22: f32,
    pub M31: f32,
    pub M32: f32,
}

/// Ubah `Vec2` lokal ke tipe `windows_numerics::Vector2` yang diminta API D2D (layout sama, 8 byte).
/// Harus dipanggil di dalam blok `unsafe` (pemanggil sudah `unsafe`, jadi macro tidak membungkus lagi).
macro_rules! nv2 {
    ($v:expr) => {
        std::mem::transmute::<Vec2, _>($v)
    };
}

// ---- tipe item dan pil (win12shell.cpp:1483–1484) ----

/// Sumber: `struct Item` (:1483). Field `type` (kata kunci Rust) menjadi `ty`.
#[derive(Clone, Copy, Default)]
pub struct Item {
    pub ty: i32,
    pub app: i32,
    pub group: i32,
    pub pill: i32,
    pub x: f32,
    pub w: f32,
    pub hov: f32,
    pub dx: f32,
    pub act: f32,
}

/// Sumber: `struct Pill` (:1484).
#[derive(Clone, Copy, Default)]
pub struct Pill {
    pub h: HWND,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub ht: i32,
    pub want: bool,
    pub shown: bool,
    pub topped: bool,
    pub applied: RECT,
}

/// Sumber: `g_items[MAXITEMS]` (:1485). Panjang Vec menjadi MAXITEMS setelah `items_ensure()`;
/// jumlah entri aktif ada di `NITEMS` (sama seperti `g_nitems`).
pub static ITEMS: SyncCell<RefCell<Vec<Item>>> = SyncCell::new(RefCell::new(Vec::new()));

/// Sumber: `g_pills[MAXPILLS]` (:1487). Panjang Vec menjadi MAXPILLS setelah `pills_ensure()`.
pub static PILLS: SyncCell<RefCell<Vec<Pill>>> = SyncCell::new(RefCell::new(Vec::new()));

/// Pastikan `ITEMS` berisi MAXITEMS entri, setara array tetap `g_items`. Idempoten.
/// Pemakai harus memanggilnya sebelum mengindeks `ITEMS`.
pub fn items_ensure() {
    let mut v = ITEMS.borrow_mut();
    if v.len() < MAXITEMS as usize {
        v.resize(MAXITEMS as usize, Item::default());
    }
}

/// Pastikan `PILLS` berisi MAXPILLS entri. Idempoten. Pemakai harus memanggilnya sebelum mengindeks `PILLS`.
pub fn pills_ensure() {
    let mut v = PILLS.borrow_mut();
    if v.len() < MAXPILLS as usize {
        v.resize(MAXPILLS as usize, Pill::default());
    }
}

// ---- media (win12shell.cpp:1330–1475) ----

/// Sumber: `static int s_lines` di MediaLog (:1456).
static S_LINES: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));

/// Sumber: MediaClear (:1330). Dipanggil saat shell keluar: pekerja GSMTC dihentikan dan status dikosongkan.
pub fn media_clear() {
    media_worker::stop();
    let had = MEDIA.borrow().has;
    if had {
        {
            let mut m = MEDIA.borrow_mut();
            m.has = false;
            m.title[0] = 0;
            m.artist[0] = 0;
            m.src[0] = 0;
            m.status = 0;
            m.art_seq = 0;
            icon_free(&mut m.art);
        }
        media_changed();
    }
}

/// Sumber: MediaTick (:1343). 2026-11-15: GSMTC tidak lagi dipoll di thread UI. Fungsi ini hanya memastikan
/// pekerja GSMTC (media_worker.rs) berjalan, lalu menerapkan snapshot yang sudah ada.
/// T_MEDIA berhenti setelah pekerja berjalan; bila pembuatan thread gagal, T_MEDIA (3 dtk) mencoba lagi.
pub fn media_tick() {
    if media_worker::start(MSG.get()) {
        kill_timer(T_MEDIA);
    }
    media_apply();
}

fn wide_into(s: &str, out: &mut [u16]) {
    if out.is_empty() {
        return;
    }
    let mut n = 0usize;
    for c in s.encode_utf16() {
        if n + 1 >= out.len() {
            break;
        }
        out[n] = c;
        n += 1;
    }
    out[n] = 0;
}

/// Menerapkan snapshot pekerja GSMTC ke `MEDIA` (dipanggil dari WM_W12_MEDIAEV lewat `media_event`).
/// Perubahan judul, artis, status, atau sampul: relayout + notch. Perubahan garis waktu saja: notch digambar ulang.
pub fn media_apply() {
    let Some(sn) = media_worker::take() else {
        return;
    };
    let mut meta = false;
    let mut tl = false;
    {
        let mut m = MEDIA.borrow_mut();
        if !sn.has {
            if m.has {
                m.has = false;
                m.title[0] = 0;
                m.artist[0] = 0;
                m.src[0] = 0;
                m.status = 0;
                m.art_seq = 0;
                icon_free(&mut m.art);
                meta = true;
            }
        } else {
            let mut t = [0u16; 200];
            let mut a = [0u16; 200];
            let mut src = [0u16; 256];
            wide_into(&sn.title, &mut t);
            wide_into(&sn.artist, &mut a);
            wide_into(&sn.app, &mut src);
            if !wstr_eq(&t, &m.title) || !wstr_eq(&a, &m.artist) || !wstr_eq(&src, &m.src) {
                m.title = t;
                m.artist = a;
                m.src = src;
                meta = true;
            }
            if m.status != sn.status || !m.has {
                m.status = sn.status;
                m.has = true;
                meta = true;
            }
            if m.tl_start != sn.tl_start || m.tl_end != sn.tl_end || m.tl_pos != sn.tl_pos || m.tl_upd != sn.tl_upd {
                m.tl_start = sn.tl_start;
                m.tl_end = sn.tl_end;
                m.tl_pos = sn.tl_pos;
                m.tl_upd = sn.tl_upd;
                tl = true;
            }
            m.can_prev = sn.can_prev;
            m.can_next = sn.can_next;
            m.can_seek = sn.can_seek;
        }
    }
    if sn.has && sn.art_seq != MEDIA.borrow().art_seq {
        ART_PX.set(s(56.0));
        let src = if sn.art_seq == 0 {
            None
        } else {
            media_worker::take_art(sn.art_seq).and_then(|b| decode_art_bytes(&b, ART_PX.get()))
        };
        {
            let mut m = MEDIA.borrow_mut();
            icon_free(&mut m.art);
            m.art.src = src;
            m.art_seq = sn.art_seq;
        }
        meta = true;
    }
    if meta {
        media_changed();
    } else if tl {
        notch_invalidate();
    }
}

/// Sumber: MediaChanged (win12shell.cpp:1791) = Relayout(); NotchMedia(). `notch_media` ada di notch.rs.
pub fn media_changed() {
    relayout();
    notch_media();
}

/// Sumber: MediaLog (:1454). Catatan dibatasi 60 baris per sesi; cmd 1 tidak dicatat.
pub(crate) fn media_log(cmd: i32, src: &str) {
    if cmd == 1 || S_LINES.get() >= 60 {
        return;
    }
    S_LINES.set(S_LINES.get() + 1);
    // Win32 GetLocalTime bersifat unsafe; hanya membaca jam sistem.
    let t = unsafe { GetLocalTime() };
    let line = format!(
        "{:02}:{:02}:{:02}.{:03} media {} dari {}{}",
        t.wHour,
        t.wMinute,
        t.wSecond,
        t.wMilliseconds,
        if cmd == 0 { "sebelumnya" } else { "berikutnya" },
        src,
        if S_LINES.get() == 60 {
            " (batas catatan tercapai)"
        } else {
            ""
        }
    );
    log(&line);
}

/// Sumber: MediaCmd (:1467). cmd: 0 sebelumnya, 1 putar/jeda, 2 berikutnya.
/// 2026-11-15: perintah diantre ke pekerja GSMTC (media_worker.rs). Putar/jeda langsung dibalik di UI
/// agar tombol terasa responsif; status asli menyusul lewat event PlaybackInfoChanged.
pub fn media_cmd(cmd: i32, src: &str) {
    media_log(cmd, src);
    if !MEDIA.borrow().has {
        return;
    }
    let c = match cmd {
        0 => media_worker::Cmd::Prev,
        1 => media_worker::Cmd::Toggle,
        _ => media_worker::Cmd::Next,
    };
    media_worker::send(c);
    if cmd == 1 {
        let flipped = {
            let mut m = MEDIA.borrow_mut();
            match m.status {
                4 => {
                    m.status = 5;
                    true
                }
                5 => {
                    m.status = 4;
                    true
                }
                _ => false,
            }
        };
        if flipped {
            media_changed();
        }
    }
}

/// Seek ke posisi `frac` (0..1) pada garis waktu sesi aktif (dijalankan pekerja GSMTC).
pub fn media_seek(frac: f32) {
    if !MEDIA.borrow().has {
        return;
    }
    media_worker::send(media_worker::Cmd::Seek(frac));
}

// ---- widget dan ukuran dok (win12shell.cpp:1507–1526) ----

/// Sumber: WgIdx (:1510). Indeks 0 = Sistem, 1 = Kalender, -1 selain itu.
pub fn wg_idx(pi: i32) -> i32 {
    if pi == P_SYS {
        0
    } else if pi == P_CAL {
        1
    } else {
        -1
    }
}

/// Sumber: WgScale (:1511).
pub fn wg_scale(pi: i32) -> f32 {
    let k = wg_idx(pi);
    if k < 0 {
        1.0
    } else {
        WG_SCALE.get()[k as usize]
    }
}

/// Sumber: BarH (:1524).
pub fn bar_h() -> i32 {
    let v: i32 = if SMALL.get() {
        BAR_H_SMALL
    } else if MODE.get() == MODE_DOCK {
        60
    } else {
        BAR_H
    };
    s(v as f32)
}

/// Sumber: SlotW (:1525).
pub fn slot_w() -> f32 {
    let v: i32 = if SMALL.get() {
        SLOT_SMALL
    } else if MODE.get() == MODE_DOCK {
        48
    } else {
        SLOT
    };
    sf(v as f32)
}

/// Sumber: TrayW (:1526).
pub fn tray_w() -> f32 {
    let v: i32 = if SMALL.get() {
        TRAY_SLOT_SMALL
    } else if MODE.get() == MODE_DOCK {
        36
    } else {
        TRAY_SLOT
    };
    sf(v as f32)
}

// ---- tata letak dok dan pil (win12shell.cpp:1527–1722) ----

/// Sumber: `g_nsep` (:1488). Jumlah garis pemisah dok yang dipasang `place_bottom`.
pub static NSEP: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));

/// Sumber: `g_sepX[G_COUNT]` (:1489). Posisi X garis pemisah dok, relatif ke pil dok.
pub static SEP_X: SyncCell<Cell<[f32; G_COUNT as usize]>> =
    SyncCell::new(Cell::new([0.0; G_COUNT as usize]));

/// Sumber: `MixC` (:1528). Campuran linear dua warna dengan faktor `t`.
pub fn mix_c(a: D2D1_COLOR_F, b: D2D1_COLOR_F, t: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// Status fokus satu app (`g_apps[i].fg`). Indeks di luar daftar dianggap tidak fokus.
fn app_fg(i: i32) -> bool {
    let apps = APPS.borrow();
    usize::try_from(i).ok().and_then(|k| apps.get(k)).map_or(false, |a| a.fg)
}

/// Pin (`-1` bebas, `0` kiri, `1` kanan) dan jumlah jendela (`nw`) satu app. Borrow `APPS` dilepas sebelum kembali.
/// Indeks di luar daftar menghasilkan `(-1, 0)`, sehingga app itu tidak ditampilkan.
fn app_pin_nw(i: i32) -> (i32, i32) {
    let apps = APPS.borrow();
    match usize::try_from(i).ok().and_then(|k| apps.get(k)) {
        Some(a) => (a.pin, a.wins.len() as i32),
        None => (-1, 0),
    }
}

/// Sumber: `AddItem` (:1534). Menambah satu item ke `ITEMS`; diabaikan bila sudah penuh (`MAXITEMS`).
pub fn add_item(ty: i32, app: i32, group: i32, w: f32) {
    items_ensure();
    let n = NITEMS.get();
    if n >= MAXITEMS {
        return;
    }
    // indikator langsung sesuai status fokus; tata letak ulang tidak memicu animasi dari nol
    let act = if ty == IT_APP && app >= 0 && app < apps_count() && app_fg(app) { 1.0 } else { 0.0 };
    ITEMS.borrow_mut()[n as usize] = Item { ty, app, group, pill: -1, x: 0.0, w, hov: 0.0, dx: 0.0, act };
    NITEMS.set(n + 1);
}

/// Sumber: `NetSlotW` (:1543). Satu ikon jaringan saja, juga saat kabel + Wi-Fi (lihat DrawNet).
pub fn net_slot_w() -> f32 {
    tray_w()
}

/// Sumber: `BuildBottom` (:1548). Mengisi item dok bawah sesuai mode dan daftar app.
pub fn build_bottom() {
    let seg = MODE.get() == MODE_SEGMENTED;
    // Dock/Classic: Start, Search, Task View paling kiri (seperti gambar referensi)
    let gs = if seg { G_CENTER } else { G_NOTE };
    let slot = slot_w();
    add_item(IT_START, -1, gs, slot);
    if SEARCH.get() {
        add_item(IT_SEARCH, -1, gs, slot);
    }
    add_item(IT_TASKVIEW, -1, gs, slot);
    let napps = apps_count();
    if seg {
        for i in 0..napps {
            if app_pin_nw(i).0 == 0 {
                add_item(IT_APP, i, G_LEFT, slot);
            }
        }
        for i in 0..napps {
            if app_pin_nw(i).0 == 1 {
                add_item(IT_APP, i, G_RIGHT, slot);
            }
        }
        // 2026-11-15: pengelompokan segmented. `[shell] segrun=1` (bawaan): aplikasi berjalan yang tidak disematkan
        // mendapat pil sendiri (G_RUN, seperti C++ :1558). segrun=0: digabung ke pil kanan (perilaku port sebelumnya).
        let run_group = if super::core_c1::ini_int("shell", "segrun", 1) != 0 { super::state::G_RUN } else { G_RIGHT };
        for i in 0..napps {
            let (pin, nw) = app_pin_nw(i);
            if pin < 0 && nw > 0 {
                add_item(IT_APP, i, run_group, slot);
            }
        }
    } else {
        // satu deret aplikasi mengikuti urutan (bisa digeser).
        // 2026-11-15 Dock: aplikasi berjalan yang tidak disematkan dipisah garis (G_RUN), seperti dok macOS.
        let dock_split = MODE.get() == MODE_DOCK && super::core_c1::ini_int("shell", "dockrun", 1) != 0;
        for i in 0..napps {
            let (pin, nw) = app_pin_nw(i);
            if pin >= 0 || (nw > 0 && !dock_split) {
                add_item(IT_APP, i, G_LEFT, slot);
            }
        }
        if dock_split {
            for i in 0..napps {
                let (pin, nw) = app_pin_nw(i);
                if pin < 0 && nw > 0 {
                    add_item(IT_APP, i, super::state::G_RUN, slot);
                }
            }
        }
    }
    add_item(IT_CHEV, -1, G_TRAY, sf(if SMALL.get() { 24.0 } else { 28.0 }));
    add_item(IT_NET, -1, G_TRAY, net_slot_w());
    add_item(IT_VOL, -1, G_TRAY, tray_w());
    if HAS_BAT.get() {
        let w = tray_w() + measure_text("100%", TF_TRAY.borrow().as_ref()) + sf(6.0);
        add_item(IT_BAT, -1, G_TRAY, w);
    }
    if USB_COUNT.get() > 0 {
        add_item(IT_USB, -1, G_TRAY, tray_w());
    }
    add_item(IT_BELL, -1, G_TRAY, tray_w() + sf(2.0));
    if TRAY_CLOCK.get() {
        // urutan desain: lonceng lalu jam dua baris "21.41 WIB" / "Kam, 5 Nov"
        let tw = measure_text(TIME_TZ.borrow().as_str(), TF_SS.borrow().as_ref());
        let dw = measure_text(DATE_SHORT.borrow().as_str(), TF_XS.borrow().as_ref());
        add_item(IT_TCLOCK, -1, G_TRAY, tw.max(dw) + sf(20.0));
    }
}

/// Sumber: `BuildTop` (:1574). Item pil atas: jam, lalu kontrol media bila ada judul atau artis.
pub fn build_top() {
    add_item(IT_CLOCK, -1, -1, 0.0);
    let show = {
        let m = MEDIA.borrow();
        m.has && (m.title[0] != 0 || m.artist[0] != 0)
    };
    if show {
        add_item(IT_MEDIA_MAIN, -1, -1, 0.0);
        add_item(IT_MEDIA_PREV, -1, -1, sf(34.0));
        add_item(IT_MEDIA_PLAY, -1, -1, sf(42.0));
        add_item(IT_MEDIA_NEXT, -1, -1, sf(34.0));
    }
}

/// Garis pemisah dok dipasang sebelum grup Left dan Tray (`sepBefore` di C, :1636).
fn dock_sep_before(g: i32) -> bool {
    g == G_TRAY || g == G_LEFT || g == super::state::G_RUN
}

/// Menaruh item grup `g` berurutan mulai `start` di pil `pill` (loop `g_items` di C). Mengembalikan X akhir.
fn place_group(g: i32, pill: i32, start: f32) -> f32 {
    let n = NITEMS.get().max(0) as usize;
    let mut items = ITEMS.borrow_mut();
    let mut ix = start;
    for it in items.iter_mut().take(n) {
        if it.group == g {
            it.pill = pill;
            it.x = ix;
            ix += it.w;
        }
    }
    ix
}

/// Sumber: `PlaceBottom` (:1585). Menghitung pil dan posisi item dok bawah: Classic (satu bar penuh),
/// Segmented (satu pil per grup), atau Dock (satu kapsul dengan garis pemisah). Mengisi `NSEP` dan `SEP_X`.
pub fn place_bottom() {
    items_ensure();
    pills_ensure();
    {
        let mut pills = PILLS.borrow_mut();
        for p in pills.iter_mut().take(NBOTTOM as usize) {
            p.want = false;
        }
    }
    NSEP.set(0);
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let mode = MODE.get();
    let h = bar_h();
    let y = sh - s(if mode == MODE_DOCK { 12.0 } else { MARGIN_B as f32 }) - h;
    let pad = sf(if mode == MODE_DOCK { 10.0 } else { PILL_PAD as f32 });
    let gap = sf(GAP as f32);
    let gc = G_COUNT as usize;
    let mut gw = [0.0f32; G_COUNT as usize];
    let mut has = [false; G_COUNT as usize];
    {
        let nitems = NITEMS.get().max(0) as usize;
        let items = ITEMS.borrow();
        for it in items.iter().take(nitems) {
            if it.group >= 0 && (it.group as usize) < gc {
                gw[it.group as usize] += it.w;
                has[it.group as usize] = true;
            }
        }
    }
    let ng = has.iter().filter(|&&b| b).count();
    if ng == 0 {
        return;
    }
    if mode == MODE_CLASSIC {
        // bar penuh menempel di bawah: grup di tengah, tray di kanan
        {
            let mut pills = PILLS.borrow_mut();
            let p0 = &mut pills[0];
            p0.x = 0;
            p0.y = sh - h;
            p0.w = sw;
            p0.ht = h;
            p0.want = true;
        }
        let gapc = sf(8.0);
        let mut block = 0.0f32;
        let mut nb = 0usize;
        for g in 0..G_TRAY as usize {
            if has[g] {
                block += gw[g];
                nb += 1;
            }
        }
        if nb > 1 {
            block += gapc * (nb - 1) as f32;
        }
        let mut ix = ((sw as f32 - block) / 2.0).floor();
        let mut k = 0usize;
        for g in 0..G_TRAY as usize {
            if !has[g] {
                continue;
            }
            if k > 0 {
                ix += gapc;
            }
            k += 1;
            ix = place_group(g as i32, 0, ix);
        }
        let tx = sw as f32 - pad - gw[G_TRAY as usize];
        place_group(G_TRAY, 0, tx);
        return;
    }
    if mode == MODE_SEGMENTED {
        let mut total = gap * (ng - 1) as f32;
        for g in 0..gc {
            if has[g] {
                total += (gw[g] + 2.0 * pad).ceil();
            }
        }
        let mut x = ((sw as f32 - total) / 2.0).floor();
        let mut p = 0usize;
        for g in 0..gc {
            if p >= NBOTTOM as usize {
                break;
            }
            if !has[g] {
                continue;
            }
            let pw = (gw[g] + 2.0 * pad).ceil() as i32;
            {
                let mut pills = PILLS.borrow_mut();
                let pl = &mut pills[p];
                pl.x = x as i32;
                pl.y = y;
                pl.w = pw;
                pl.ht = h;
                pl.want = true;
            }
            place_group(g as i32, p as i32, pad);
            x += pw as f32 + gap;
            p += 1;
        }
        return;
    }
    // Dock: satu kapsul; garis pemisah setelah tombol sistem dan sebelum tray (desain)
    let sepw = sf(17.0);
    let gapg = sf(2.0);
    let mut total = 2.0 * pad;
    let mut kk = 0usize;
    for g in 0..gc {
        if has[g] {
            total += gw[g];
            if kk > 0 {
                total += if dock_sep_before(g as i32) { sepw } else { gapg };
            }
            kk += 1;
        }
    }
    let pw = total.ceil() as i32;
    {
        let mut pills = PILLS.borrow_mut();
        let p0 = &mut pills[0];
        p0.w = pw;
        p0.ht = h;
        p0.x = (sw - pw) / 2;
        p0.y = y;
        p0.want = true;
    }
    let mut ix = pad;
    let mut k = 0usize;
    let mut sep_x = SEP_X.get();
    let mut nsep = 0usize;
    for g in 0..gc {
        if !has[g] {
            continue;
        }
        if k > 0 {
            if dock_sep_before(g as i32) {
                if nsep < gc {
                    sep_x[nsep] = ix + sepw / 2.0;
                    nsep += 1;
                }
                ix += sepw;
            } else {
                ix += gapg;
            }
        }
        k += 1;
        ix = place_group(g as i32, 0, ix);
    }
    SEP_X.set(sep_x);
    NSEP.set(nsep as i32);
}

/// Sumber: `PlaceTop` (:1653). Jam kiri atas dan kontrol media kanan atas, lalu `widgets_place`.
/// Catatan: loop juga menyentuh `IT_USB` (16 > `IT_MEDIA_MAIN`) seperti di C; lihat laporan.
pub fn place_top() {
    items_ensure();
    pills_ensure();
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let m = s(22.0);
    {
        let mut pills = PILLS.borrow_mut();
        pills[P_CLOCK as usize].want = false;
        pills[P_MEDIA as usize].want = false;
    }
    let mut x = 0.0f32;
    let n = NITEMS.get().max(0) as usize;
    {
        let mut items = ITEMS.borrow_mut();
        for it in items.iter_mut().take(n) {
            if it.ty == IT_CLOCK {
                let tw = measure_text(TIME.borrow().as_str(), TF_HUGE.borrow().as_ref())
                    + sf(7.0)
                    + measure_text(TZ.borrow().as_str(), TF_TZ.borrow().as_ref());
                let dw = measure_text(DATE.borrow().as_str(), TF_B.borrow().as_ref());
                let pw = (sf(22.0) + tw.max(dw) + sf(26.0)).ceil() as i32;
                let mut pills = PILLS.borrow_mut();
                let p = &mut pills[P_CLOCK as usize];
                p.w = pw;
                p.ht = s(86.0);
                p.x = m;
                p.y = m;
                p.want = true;
                it.pill = P_CLOCK;
                it.x = 0.0;
                it.w = pw as f32;
            } else if it.ty == IT_MEDIA_MAIN {
                let mut pills = PILLS.borrow_mut();
                let p = &mut pills[P_MEDIA as usize];
                p.w = s(392.0);
                p.ht = s(84.0);
                p.x = sw - m - p.w;
                p.y = m;
                p.want = true;
                let ctl = sf(34.0) + sf(42.0) + sf(34.0) + sf(12.0);
                it.pill = P_MEDIA;
                it.x = 0.0;
                it.w = p.w as f32 - ctl;
                x = it.w;
            } else if it.ty > IT_MEDIA_MAIN {
                it.pill = P_MEDIA;
                it.x = x;
                x += it.w;
            }
        }
    }
    widgets_place(sw, sh);
}

/// Sumber: `TopBlocked` (:1680). Pil atas hanya tampil saat desktop aktif. `PEEK` (uji --peek) selalu menampilkan.
pub fn top_blocked() -> bool {
    if PEEK.get() {
        return false;
    }
    FG_VALID.get() || POP_OPEN.get() || FLY_OPEN.get() || QUIET.get() || PANEL_OPEN.get() >= 0
}

/// Sumber: `DeskLayer` (:1690). Pil atas (indeks >= `P_TOP`) ikut lapisan desktop bila `DESK_LAYER` aktif.
pub fn desk_layer(i: i32) -> bool {
    DESK_LAYER.get() && i >= P_TOP
}

/// Sumber: `IsTopPillWnd` (:1693). True bila `h` adalah jendela pil atas (`P_TOP..MAXPILLS`).
pub fn is_top_pill_wnd(h: HWND) -> bool {
    if h.0.is_null() {
        return false;
    }
    let pills = PILLS.borrow();
    (P_TOP..MAXPILLS).any(|i| pills.get(i as usize).map_or(false, |p| p.h.0 == h.0))
}

/// Sumber: `PillsToDesk` (:1699). Menyisipkan pil atas tepat di bawah jendela pertama di atas blok desktop
/// (Progman plus WorkerW yang menempel), atau paling bawah bila tidak ada. `None` = nullptr di C.
pub fn pills_to_desk(desk: Option<HWND>) {
    if !DESK_LAYER.get() {
        return;
    }
    // Salin handle lalu lepas borrow PILLS: SetWindowPos bisa mengirim pesan sinkron ke wndproc yang juga memakai PILLS.
    let handles: Vec<HWND> = {
        let pills = PILLS.borrow();
        (P_TOP..MAXPILLS)
            .filter_map(|i| pills.get(i as usize).map(|p| p.h))
            .filter(|h| !h.0.is_null())
            .collect()
    };
    if handles.is_empty() {
        return; // pil belum dibuat: tanda tetap terpasang untuk pemanggilan berikutnya
    }
    DESK_Z_DIRTY.set(false);
    let desk: HWND = match desk {
        Some(d) if !d.0.is_null() => d,
        _ => unsafe { FindWindowW(w!("Progman"), PCWSTR::null()) }.unwrap_or_default(),
    };
    let mut after = HWND_BOTTOM;
    if !desk.0.is_null() {
        // Meniru `for (; a; a = GetWindow(a, GW_HWNDPREV))` di C. Pil sendiri dilewati; berhenti di
        // jendela pertama yang kelasnya bukan "WorkerW".
        let mut a = unsafe { GetWindow(desk, GW_HWNDPREV) }.unwrap_or_default();
        while !a.0.is_null() {
            if is_top_pill_wnd(a) {
                a = unsafe { GetWindow(a, GW_HWNDPREV) }.unwrap_or_default();
                continue;
            }
            let mut cls = [0u16; 16];
            let n = unsafe { GetClassNameW(a, &mut cls) }.clamp(0, cls.len() as i32) as usize;
            if !cls[..n].iter().copied().eq("WorkerW".encode_utf16()) {
                break;
            }
            a = unsafe { GetWindow(a, GW_HWNDPREV) }.unwrap_or_default();
        }
        let topmost = !a.0.is_null() && {
            let ex = unsafe { GetWindowLongPtrW(a, GWL_EXSTYLE) };
            ((ex as u32) & WS_EX_TOPMOST.0) != 0
        };
        after = if a.0.is_null() || topmost { HWND_TOP } else { a };
    }
    for h in handles {
        let _ = unsafe {
            SetWindowPos(h, Some(after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
        };
    }
}

/// Sumber: `g_pressed` (win12shell.cpp:1490). Indeks item yang sedang ditekan; `-1` bila tidak ada.
pub static PRESSED: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));

/// Sumber: `static bool g_layoutDirty` (win12shell.cpp:1794). Diset bila susunan app/item berubah dan perlu `relayout`.
pub static LAYOUT_DIRTY: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

/// Pengganti `g_napps`. Sumber kebenaran tunggal adalah `APPS.len()`; `NAPPS` (state.rs) tidak pernah diisi.
pub fn apps_count() -> i32 {
    APPS.borrow().len() as i32
}

/// Pengganti `EqualRect`.
pub fn rect_eq(a: &RECT, b: &RECT) -> bool {
    a.left == b.left && a.top == b.top && a.right == b.right && a.bottom == b.bottom
}

// GlowUpdate (win12shell.cpp:2579) sekarang diport di bagian akhir file ini (`glow_update`).

/// Sumber: `ApplyPills` (win12shell.cpp:1724). Menampilkan/menyembunyikan pil dan memasang posisinya.
pub fn apply_pills(repaint: bool) {
    if QUITTING.get() {
        return;
    }
    pills_ensure();
    if DESK_Z_DIRTY.get() {
        pills_to_desk(None); // sebelum pil pertama kali tampil
    }
    let mut repaint = repaint;
    for i in 0..MAXPILLS {
        let iu = i as usize;
        // Salinan (Pill: Copy) agar tidak ada borrow `PILLS` yang aktif saat memanggil API Win32.
        let p = PILLS.borrow()[iu];
        if p.h.0.is_null() {
            continue;
        }
        let mut vis = w12_should_show_pill(
            p.want,
            FULLSCREEN.get(),
            FULLSCREEN_START_PEEK.get(),
            i < NBOTTOM,
        );
        if vis && i >= P_TOP && !DESK_LAYER.get() {
            vis = !top_blocked();
        }
        if vis {
            let r = RECT { left: p.x, top: p.y, right: p.x + p.w, bottom: p.y + p.ht };
            if !p.shown || !rect_eq(&r, &p.applied) {
                // Naik ke puncak band topmost hanya saat pertama kali tampil. Tampil ulang/ubah ukuran berikutnya
                // memakai SWP_NOZORDER agar pil tidak melompati Quick Settings atau flyout yang sudah terbuka.
                // Pil lapisan desktop tidak pernah diubah urutan Z-nya di sini (PillsToDesk yang mengatur).
                let mut fl = SWP_NOACTIVATE | SWP_SHOWWINDOW;
                if p.topped || desk_layer(i) {
                    fl = fl | SWP_NOZORDER;
                }
                let _ = unsafe { SetWindowPos(p.h, Some(HWND_TOPMOST), p.x, p.y, p.w, p.ht, fl) };
                {
                    let mut pills = PILLS.borrow_mut();
                    pills[iu].topped = true;
                    pills[iu].applied = r;
                }
                repaint = true;
            }
            if repaint {
                w12_invalidate(p.h, None, false);
            }
        } else if p.shown {
            let _ = unsafe { ShowWindow(p.h, SW_HIDE) };
        }
        PILLS.borrow_mut()[iu].shown = vis;
    }
    widgets_visibility();
    glow_update();
    notch_sync();
}

/// Sumber: `InvalidatePills` (win12shell.cpp:1757). Menggambar ulang pil yang sedang tampil.
pub fn invalidate_pills(bottom: bool, top: bool) {
    pills_ensure();
    for i in 0..MAXPILLS {
        let want = if i < NBOTTOM { bottom } else { i >= P_TOP && top };
        let p = PILLS.borrow()[i as usize];
        if want && !p.h.0.is_null() && p.shown {
            w12_invalidate(p.h, None, false);
        }
    }
}

/// Sumber: `Relayout` (win12shell.cpp:1765). Menyusun ulang item dan pil, lalu memasangnya.
pub fn relayout() {
    if QUITTING.get() {
        return;
    }
    items_ensure();
    // Simpan hover dan tipe/app item HOT lebih dulu (pengganti `saved[]`, `hotType`, `hotApp`), karena NITEMS direset.
    let hot = HOT.get();
    let n_old = NITEMS.get();
    let mut hot_type = -1;
    let mut hot_app = -1;
    let mut saved: Vec<(i32, i32, f32)> = Vec::new();
    {
        let items = ITEMS.borrow();
        if hot >= 0 && hot < n_old {
            hot_type = items[hot as usize].ty;
            hot_app = items[hot as usize].app;
        }
        for it in items.iter().take(n_old as usize) {
            if it.hov > 0.001 {
                saved.push((it.ty, it.app, it.hov));
            }
        }
    }
    NITEMS.set(0);
    build_bottom();
    build_top();
    place_bottom();
    place_top();
    let n = NITEMS.get() as usize;
    {
        let mut items = ITEMS.borrow_mut();
        for it in items.iter_mut().take(n) {
            it.dx = it.x;
        }
    }
    DRAG_ON.set(false);
    DRAG_CAND.set(false);
    DRAG_ITEM.set(-1);
    HOT.set(-1);
    PRESSED.set(-1);
    {
        let mut items = ITEMS.borrow_mut();
        for i in 0..n {
            let it = &mut items[i];
            for &(t, a, h) in &saved {
                if it.ty == t && it.app == a {
                    it.hov = h;
                }
            }
            if it.ty == hot_type && it.app == hot_app {
                HOT.set(i as i32);
            }
        }
    }
    apply_pills(true);
}

/// Sumber: `EnumWin` (win12shell.cpp:1795). Callback `EnumWindows`: mengaitkan jendela tugas ke app.
/// `lp` berisi HWND jendela latar depan (pengganti parameter `fgp`).
unsafe extern "system" fn enum_win(h: HWND, lp: LPARAM) -> BOOL {
    if !is_task_window(h) {
        return BOOL(1);
    }
    let Some(w) = window_identity(h) else {
        return BOOL(1);
    };
    // Tidak ada borrow APPS yang ditahan saat memanggil match/add: keduanya membaca dan menulis APPS sendiri.
    let i = match match_app(&w) {
        Some(i) => i,
        None => match add_running_app(&w, h) {
            Some(i) => {
                LAYOUT_DIRTY.set(true);
                i
            }
            None => return BOOL(1),
        },
    };
    let mut apps = APPS.borrow_mut();
    if let Some(a) = apps.get_mut(i) {
        if (a.wins.len() as i32) < MAXW {
            a.wins.push(h);
        }
        if h.0 as isize == lp.0 {
            a.fg = true;
        }
    }
    BOOL(1)
}

/// Sumber: `AppsSignature` (win12shell.cpp:1812). Sidik jari keadaan app untuk mendeteksi perubahan.
pub fn apps_signature() -> u32 {
    let apps = APPS.borrow();
    let mut h: u32 = 2166136261u32 ^ (apps.len() as u32);
    for a in apps.iter() {
        let bits = ((a.wins.len() as u32) << 2) | (a.fg as u32) | ((a.flash as u32) << 1);
        h = (h ^ bits).wrapping_mul(16777619u32);
    }
    h
}

/// Sumber: `RefreshWindows` (win12shell.cpp:1820). Memindai jendela tugas, menyusun ulang app, lalu memperbarui tampilan.
pub fn refresh_windows() {
    let sig0 = apps_signature();
    {
        let mut wc = WC.borrow_mut();
        for e in wc.iter_mut() {
            e.seen = false;
        }
    }
    // Catat status "berjalan" lalu kosongkan daftar jendela; `enum_win` mengisinya lagi.
    let was_running: Vec<bool> = {
        let mut apps = APPS.borrow_mut();
        let v: Vec<bool> = apps.iter().map(|a| !a.wins.is_empty()).collect();
        for a in apps.iter_mut() {
            a.wins.clear();
            a.fg = false;
        }
        v
    };
    let before = was_running.len();
    let fg = unsafe { GetForegroundWindow() };
    let _ = unsafe { EnumWindows(Some(enum_win), LPARAM(fg.0 as isize)) };
    {
        let apps = APPS.borrow();
        for i in 0..before.min(apps.len()) {
            if apps[i].pin < 0 && was_running[i] != !apps[i].wins.is_empty() {
                LAYOUT_DIRTY.set(true);
            }
        }
    }
    // Buang app yang tidak berjalan, kecuali pin Start (pin >= 0 atau spin != 0). Indeks tidak dinaikkan saat hapus.
    let mut i = 0usize;
    loop {
        if i >= APPS.borrow().len() {
            break;
        }
        let remove = {
            let mut apps = APPS.borrow_mut();
            let a = &mut apps[i];
            if a.fg || a.wins.is_empty() {
                a.flash = false;
            }
            a.pin < 0 && a.spin == 0 && a.wins.is_empty()
        };
        if remove {
            let mut gone = APPS.borrow_mut().remove(i);
            icon_free(&mut gone.icon);
            LAYOUT_DIRTY.set(true);
        } else {
            i += 1;
        }
    }
    WC.borrow_mut().retain(|e| e.seen);
    let sig_changed = apps_signature() != sig0;
    if LAYOUT_DIRTY.get() {
        LAYOUT_DIRTY.set(false);
        relayout();
    } else if sig_changed {
        invalidate_pills(true, false); // gambar ulang hanya bila ada perubahan
    }
    if sig_changed {
        start_lists_invalidate();
        set_timer(T_ANIM, 15); // indikator aktif ikut dianimasikan
    }
}

// ---------------------------------------------------------------- gambar (C++ 1854-2002)

/// `DrawStartLogo` (:1855-1873): empat kotak membulat logo Start. Pakai kuas gradien `START_BRUSH` bila ada;
/// bila tidak, pakai warna aksen pada kuas `BR`.
pub fn draw_start_logo(cx: f32, cy: f32, s: f32) {
    let g = (s * 0.08).round();
    let q = (s - g) / 2.0;
    let r = s * 0.08;
    let x0 = (cx - s / 2.0).round();
    let y0 = (cy - s / 2.0).round();
    let rt_g = RT.borrow();
    let Some(rt) = rt_g.as_ref() else { return };
    let grad_g = START_BRUSH.borrow();
    let br_g = BR.borrow();
    let brush: &ID2D1Brush = if let Some(grad) = grad_g.as_ref() {
        unsafe {
            grad.SetStartPoint(nv2!(Vec2 { X: x0, Y: y0 }));
            grad.SetEndPoint(nv2!(Vec2 { X: x0 + s, Y: y0 + s }));
        }
        let gb: &ID2D1Brush = grad;
        gb
    } else {
        set_color(PALETTE.borrow().accent, 1.0);
        let Some(bs) = br_g.as_ref() else { return };
        let bb: &ID2D1Brush = bs;
        bb
    };
    for i in 0..4u32 {
        let x = x0 + (i % 2) as f32 * (q + g);
        let y = y0 + (i / 2) as f32 * (q + g);
        let rr = D2D1_ROUNDED_RECT { rect: rf(x, y, x + q, y + q), radiusX: r, radiusY: r };
        unsafe { rt.FillRoundedRectangle(&rr, brush) };
    }
}

/// `DrawTaskView` (:1875-1887): tiga jendela sejajar (gaya Mission Control). Warna mengikuti `PALETTE.glyph`.
pub fn draw_task_view(cx: f32, cy: f32, s: f32) {
    let sz = s.round();
    let x0 = (cx - s / 2.0).round();
    let y0 = (cy - s / 2.0).round();
    let rad = sz * 0.13;
    let sw = (sz * 0.16).round();
    let sr = rad.min(sw * 0.4);
    let mx = (sz * 0.24).round();
    let my = (sz * 0.20).round();
    let sy = (sz * 0.30).round();
    let c = PALETTE.borrow().glyph;
    fill_rr(x0, y0 + sy, x0 + sw, y0 + sz - sy, sr, c, 0.45);
    fill_rr(x0 + sz - sw, y0 + sy, x0 + sz, y0 + sz - sy, sr, c, 0.45);
    fill_rr(x0 + mx, y0 + my, x0 + sz - mx, y0 + sz - my, rad, c, 1.0);
}

/// `DrawNet` (:1889-1902): ikon jaringan. LAN/keduanya = glyph kabel; Wi-Fi = batang sesuai `WIFI_Q`.
pub fn draw_net(cx: f32, cy: f32) {
    let net = NET.get();
    let tf = TF_GLYPH.borrow();
    let c = PALETTE.borrow().glyph;
    if net == NET_ETH || net == NET_BOTH {
        glyph(0xE839, tf.as_ref(), cx, cy, sf(16.0), c, 1.0);
    } else if net == NET_WIFI {
        let q = WIFI_Q.get();
        let bars: usize = if q >= 75 {
            4
        } else if q >= 50 {
            3
        } else if q >= 25 {
            2
        } else {
            1
        };
        const LV: [u16; 5] = [0, 0xE872, 0xE873, 0xE874, 0xE701];
        if bars < 4 {
            glyph(0xE701, tf.as_ref(), cx, cy, sf(16.0), c, 0.3);
        }
        glyph(LV[bars], tf.as_ref(), cx, cy, sf(16.0), c, 1.0);
    } else {
        glyph(0xF384, tf.as_ref(), cx, cy, sf(16.0), c, 1.0);
    }
}

/// `VolGlyph` (:1904-1909): glyph volume sesuai status diam dan persentase.
pub fn vol_glyph() -> u16 {
    let v = VOL.get();
    if v < 0 || MUTE.get() {
        return 0xE74F;
    }
    if v == 0 {
        return 0xE992;
    }
    if v < 34 {
        0xE993
    } else if v < 67 {
        0xE994
    } else {
        0xE995
    }
}

/// `DrawBattery` (:1914-1949): ikon baterai digambar sendiri (glyph font tampil blok di Windows ini).
/// `bw` = lebar badan, (cx, cy) = pusat ikon. Merah bila <= 15% dan tidak mengisi daya; saat mengisi,
/// petir dengan celah latar.
pub fn draw_battery(cx: f32, cy: f32, bw: f32, ink: D2D1_COLOR_F, bg: D2D1_COLOR_F) {
    let bw = bw.round();
    let bh = (bw * 0.52).round();
    let nub = if (bw * 0.08).round() < 2.0 { 2.0 } else { (bw * 0.08).round() };
    let sw = if sf(1.0) < 1.0 { 1.0 } else { sf(1.0).round() };
    let l = (cx - (bw + nub + 1.0) / 2.0).round();
    let t = (cy - bh / 2.0).round();
    let r = l + bw;
    let b = t + bh;
    let rad = (bh * 0.26).round();
    let body = D2D1_ROUNDED_RECT {
        rect: rf(l + sw / 2.0, t + sw / 2.0, r - sw / 2.0, b - sw / 2.0),
        radiusX: rad,
        radiusY: rad,
    };
    set_color(ink, 1.0);
    {
        let rt_g = RT.borrow();
        let br_g = BR.borrow();
        if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
            let brush: &ID2D1Brush = br;
            unsafe { rt.DrawRoundedRectangle(&body, brush, sw, None::<&ID2D1StrokeStyle>) };
        }
    }
    fill_rr(r + 1.0, cy - (bh * 0.2).round(), r + 1.0 + nub, cy + (bh * 0.2).round(), nub * 0.5, ink, 1.0);
    let bat = BAT.get();
    let lv: i32 = if (0..=100).contains(&bat) { bat } else { 100 };
    let charging = CHARGING.get();
    let inset = sw + if bw >= sf(24.0) { sf(2.0) } else { sf(1.5) };
    let x0 = l + inset;
    let x1 = x0 + (r - inset - x0) * lv as f32 / 100.0;
    let fill = if !charging && lv <= 15 { rgba(232, 72, 85, 1.0) } else { ink };
    if x1 - x0 > 0.75 {
        let fr = if rad > inset { rad - inset } else { 1.0 };
        fill_rr(x0, t + inset, x1, b - inset, fr, fill, 1.0);
    }
    if !charging {
        return;
    }
    // petir: celah selebar 2 px warna latar, lalu isi warna tinta (C++ :1931-1948)
    let s = bh * 1.15;
    let ox = cx - s * 0.36;
    let oy = cy - s * 0.5;
    const P: [[f32; 2]; 7] = [
        [0.55, 0.0],
        [0.08, 0.56],
        [0.40, 0.56],
        [0.30, 1.0],
        [0.80, 0.42],
        [0.48, 0.42],
        [0.62, 0.0],
    ];
    let pt = |i: usize| Vec2 { X: ox + P[i][0] * s, Y: oy + P[i][1] * s };
    let pg = {
        let d = D2D.borrow();
        let Some(f) = d.as_ref() else { return };
        match unsafe { f.CreatePathGeometry() } {
            Ok(p) => p,
            Err(_) => return,
        }
    };
    let Ok(sk) = (unsafe { pg.Open() }) else { return };
    unsafe {
        sk.BeginFigure(nv2!(pt(0)), D2D1_FIGURE_BEGIN_FILLED);
        for i in 1..7 {
            sk.AddLine(nv2!(pt(i)));
        }
        sk.EndFigure(D2D1_FIGURE_END_CLOSED);
        let _ = sk.Close();
    }
    let geo: &ID2D1Geometry = &pg;
    set_color(bg, 1.0);
    {
        let rt_g = RT.borrow();
        let br_g = BR.borrow();
        if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
            let brush: &ID2D1Brush = br;
            unsafe { rt.DrawGeometry(geo, brush, sf(2.0), None::<&ID2D1StrokeStyle>) };
        }
    }
    set_color(ink, 1.0);
    {
        let rt_g = RT.borrow();
        let br_g = BR.borrow();
        if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
            let brush: &ID2D1Brush = br;
            unsafe { rt.FillGeometry(geo, brush, None::<&ID2D1Brush>) };
        }
    }
}

/// `DrawArt` (:1951-1969): sampul media dalam kotak membulat. Tanpa bitmap: kotak redup + glyph musik.
pub fn draw_art(x: f32, y: f32, s: f32) {
    let bmp = {
        let mut m = MEDIA.borrow_mut();
        icon_bitmap(&mut m.art)
    };
    let art_px = ART_PX.get();
    let b = match bmp {
        Some(b) if art_px != 0 => b,
        _ => {
            let c = if LIGHT.get() { rgba(0, 0, 0, 0.07) } else { rgba(255, 255, 255, 0.10) };
            fill_rr(x, y, x + s, y + s, sf(12.0), c, 1.0);
            let sub = PALETTE.borrow().sub;
            let tf = TF_GLYPH_MD.borrow();
            glyph(0xE8D6, tf.as_ref(), (x + s / 2.0).round(), (y + s / 2.0).round(), s, sub, 1.0);
            return;
        }
    };
    let rt_g = RT.borrow();
    let Some(rt) = rt_g.as_ref() else { return };
    let Ok(bb) = (unsafe { rt.CreateBitmapBrush(&b, None, None) }) else { return };
    let k = s / art_px as f32;
    let mx = Mat32 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: x, M32: y };
    unsafe {
        bb.SetTransform(&mx as *const Mat32 as *const _);
        bb.SetInterpolationMode(D2D1_BITMAP_INTERPOLATION_MODE_LINEAR);
    }
    let rr = D2D1_ROUNDED_RECT { rect: rf(x, y, x + s, y + s), radiusX: sf(12.0), radiusY: sf(12.0) };
    let brush: &ID2D1Brush = &bb;
    unsafe { rt.FillRoundedRectangle(&rr, brush) };
}

/// `DrawBitmapAt` (:1971-1975): gambar bitmap ke kotak (x, y)-(x+size, y+size), opasitas penuh.
pub fn draw_bitmap_at(b: &ID2D1Bitmap, x: f32, y: f32, size: f32) {
    let rc = rf(x, y, x + size, y + size);
    let rt_g = RT.borrow();
    if let Some(rt) = rt_g.as_ref() {
        unsafe {
            rt.DrawBitmap(
                b,
                Some(&rc as *const D2D_RECT_F),
                1.0,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                None,
            )
        };
    }
}

/// Isi `buf` (= `px[start..]`) dengan skala Fant ke `b` px lalu konversi ke PBGRA (C :1993-1997).
/// Mengembalikan false bila salah satu langkah WIC gagal; hasil gagal itu lalu di-cache oleh `icon_tile`.
unsafe fn icon_tile_fill(
    wic: &IWICImagingFactory,
    src: &IWICBitmapSource,
    b: i32,
    n: i32,
    buf: &mut [u8],
) -> bool {
    let Ok(sc) = wic.CreateBitmapScaler() else {
        return false;
    };
    if sc.Initialize(src, b as u32, b as u32, WICBitmapInterpolationModeFant).is_err() {
        return false;
    }
    let Ok(cv) = wic.CreateFormatConverter() else {
        return false;
    };
    let init = cv.Initialize(
        &*sc,
        &GUID_WICPixelFormat32bppPBGRA,
        WICBitmapDitherTypeNone,
        None::<&IWICPalette>,
        0.0,
        WICBitmapPaletteTypeCustom,
    );
    if init.is_err() {
        return false;
    }
    cv.CopyPixels(std::ptr::null(), (n * 4) as u32, buf).is_ok()
}

/// `IconTile` (win12shell.cpp:1980-2010): ubin PBGRA `b` px yang berbingkai `TileMargin()` di sekelilingnya.
/// Hasil di-cache per `Icon` lewat (`tile_sz`, `tile_gen`), termasuk hasil gagal (None), seperti C (:1983, :2008).
pub fn icon_tile(ic: &mut Icon, b: i32) -> Option<ID2D1Bitmap> {
    let src = ic.src.as_ref()?;
    let wic_g = WIC.borrow();
    let wic = wic_g.as_ref()?;
    let rt_g = RT.borrow();
    let rt = rt_g.as_ref()?;
    if b < 8 {
        return None;
    }
    if ic.tile_gen == RT_GEN.get() && ic.tile_sz == b {
        return ic.tile.clone();
    }
    ic.tile = None;
    ic.tile_gen = RT_GEN.get();
    ic.tile_sz = b;
    let m = tile_margin();
    let n = b + 2 * m;
    let total = (n as usize) * (n as usize) * 4;
    let start = ((m as usize) * (n as usize) + m as usize) * 4;
    let mut px = vec![0u8; total];
    let ok = unsafe { icon_tile_fill(wic, src, b, n, &mut px[start..]) };
    let out = if ok {
        d2d_bitmap_bgra(rt, n as u32, n as u32, &px, (n * 4) as u32)
    } else {
        None
    };
    ic.tile = out.clone();
    out
}

/// `DrawIconTile` (win12shell.cpp:2013-2022): ikon asli Windows, badan `B` px berpusat di (cx, cy).
/// Bila ubin tersedia digambar 1:1 dengan bingkai `TileMargin`; bila tidak, cadangan `draw_icon_at`.
pub fn draw_icon_tile(ic: &mut Icon, cx: f32, cy: f32, bb: i32) {
    let mut b = bb;
    if b < 8 {
        b = 8;
    }
    if b > src_px() {
        b = src_px();
    }
    let m = tile_margin();
    let bx = (cx - b as f32 / 2.0).round();
    let by = (cy - b as f32 / 2.0).round();
    match icon_tile(ic, b) {
        Some(tb) => draw_bitmap_at(&tb, bx - m as f32, by - m as f32, (b + 2 * m) as f32),
        None => draw_icon_at(ic, bx, by, b as f32, 1.0),
    }
}

// ---------------------------------------------------------------- Mica: wallpaper buram sebagai kaca
// Sumber: win12shell.cpp:2024-2164 (MicaBlur, MicaResolveWallpaperPath, MicaGetWallpaperStamp,
// MicaRememberWallpaper, MicaLoad). Mica(b) diport: MicaCaptureFullscreen (C++ 2166-2235),
// MicaRestoreWallpaper (2237-2243), MicaBegin/MicaMoved (2246-2261), MicaFill (2264-2292), Surface (2295-2332).
// PORT: `RegGetValueW` -> `reg::get_string` (HKCU); `GetEnvironmentVariableW` -> `std::env::var_os`
// (dipilih `std::env` agar tidak memanggil API; fitur `Win32_System_Environment` sudah aktif); `malloc` -> `Vec` dengan `try_reserve_exact`.
// PORT: nilai `INT_PTR` pada `SetPropW`/`GetPropW` (MicaBegin/MicaMoved) diwakili `prop_value` (HANDLE dari isize).

use std::ffi::c_void;

use windows::Win32::Foundation::{GENERIC_READ, HANDLE, POINT};
use windows::Win32::Graphics::Direct2D::Common::D2D1_GRADIENT_STOP;
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_BRUSH_PROPERTIES, D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2,
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
};
use windows::Win32::Graphics::Direct2D::ID2D1BitmapBrush;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetSysColor, ReleaseDC,
    SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, CAPTUREBLT, COLOR_DESKTOP, DIB_RGB_COLORS, HALFTONE, HGDIOBJ, ROP_CODE, SRCCOPY,
};
use windows::Win32::Graphics::Imaging::{IWICBitmap, WICDecodeMetadataCacheOnDemand};
use windows::Win32::Storage::FileSystem::{
    GetFileAttributesExW, GetFileAttributesW, GetFileExInfoStandard, INVALID_FILE_ATTRIBUTES,
    WIN32_FILE_ATTRIBUTE_DATA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetPropW, SetPropW, SystemParametersInfoW, SPI_GETDESKWALLPAPER,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WINDOWPOS,
};

use crate::reg;
use crate::ui_logic::w12_average_rgb;
use crate::wide::{pcw, to_wide_z};

use super::core_c1::{apply_wallpaper_surface_tint, d2d_bitmap_from_wic, WALLPAPER_TINT};
use super::state::{
    GLASS, GLASS_OK, MICA_FILE_SIZE, MICA_FILE_WRITE, MICA_FULLSCREEN_SNAPSHOT, MICA_PATH,
};

/// `MICA_K` (C++ 2025): faktor perkecil; 1920x1200 -> 120x75 px.
pub const MICA_K: i32 = 16;
/// `MAX_PATH` (C++ 2032): kapasitas jalur wallpaper termasuk NUL.
const MICA_PATH_MAX: usize = 260;
/// Kunci pengaturan wallpaper di HKCU (C++ 2101-2103).
const DESKTOP_KEY: &str = "Control Panel\\Desktop";

/// `g_micaSrc` (C++ 2026): wallpaper monitor utama, diperkecil dan diburamkan.
pub static MICA_SRC: SyncCell<RefCell<Option<IWICBitmap>>> = SyncCell::new(RefCell::new(None));
/// `g_micaBmp` (C++ 2027): ID2D1Bitmap dari `MICA_SRC` (dibuat di `mica_fill` bila `MICA_GEN` tidak cocok).
pub static MICA_BMP: SyncCell<RefCell<Option<ID2D1Bitmap>>> = SyncCell::new(RefCell::new(None));
/// `g_micaBr` (C++ 2028): kuas bitmap dari `MICA_BMP` (dibuat di `mica_fill`).
pub static MICA_BR: SyncCell<RefCell<Option<ID2D1BitmapBrush>>> = SyncCell::new(RefCell::new(None));
/// `g_micaGen` (C++ 2029): `RT_GEN` saat bitmap/kuas dibuat; -1 = belum.
pub static MICA_GEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
/// `g_micaMon` (C++ 2030): area layar (px) yang diwakili bitmap.
pub static MICA_MON: SyncCell<Cell<RECT>> =
    SyncCell::new(Cell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }));
/// `g_micaOrg` (C++ 2031): posisi layar titik (0,0) render target saat `mica_begin` (diisi di sana).
pub static MICA_ORG: SyncCell<Cell<POINT>> = SyncCell::new(Cell::new(POINT { x: 0, y: 0 }));

/// `mica_alloc`: buffer `n` byte berisi nol; `None` bila alokasi gagal (padanan `malloc` NULL di C++).
fn mica_alloc(n: usize) -> Option<Vec<u8>> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).ok()?;
    v.resize(n, 0);
    Some(v)
}

/// Pemotongan jalur seperti `wcsncpy(g_micaPath, path, MAX_PATH - 1)` (C++ 2083-2084).
fn mica_clip(s: &str) -> String {
    let units: Vec<u16> = s.encode_utf16().take(MICA_PATH_MAX - 1).collect();
    String::from_utf16_lossy(&units)
}

/// `_wtoi` (CRT): lewati spasi awal, tanda opsional, lalu digit desimal; selain itu 0.
fn wtoi(s: &str) -> i32 {
    let t = s.trim_start_matches([' ', '\t', '\n', '\u{b}', '\u{c}', '\r']);
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let mut v: i64 = 0;
    for b in digits.bytes() {
        if !b.is_ascii_digit() {
            break;
        }
        v = (v * 10 + (b - b'0') as i64).min(i32::MAX as i64 + 1);
    }
    (if neg { -v } else { v }).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// `MicaBlur` (C++ 2035-2057): box blur terpisah, dua lintasan (horizontal lalu vertikal); indeks dijepit di tepi.
fn mica_blur(px: &mut [u8], w: i32, h: i32, r: i32) {
    let (wu, hu) = (w as usize, h as usize);
    let Some(mut tmp) = mica_alloc(wu * hu * 4) else {
        return;
    };
    let n = 2 * r + 1;
    for _ in 0..2 {
        for y in 0..hu {
            for x in 0..wu {
                for c in 0..4 {
                    let mut s = 0i32;
                    for k in -r..=r {
                        let xx = (x as i32 + k).clamp(0, w - 1) as usize;
                        s += px[(y * wu + xx) * 4 + c] as i32;
                    }
                    tmp[(y * wu + x) * 4 + c] = (s / n) as u8;
                }
            }
        }
        for y in 0..hu {
            for x in 0..wu {
                for c in 0..4 {
                    let mut s = 0i32;
                    for k in -r..=r {
                        let yy = (y as i32 + k).clamp(0, h - 1) as usize;
                        s += tmp[(yy * wu + x) * 4 + c] as i32;
                    }
                    px[(y * wu + x) * 4 + c] = (s / n) as u8;
                }
            }
        }
    }
}

/// `MicaResolveWallpaperPath` (C++ 2059-2069): wallpaper aktif; bila berkas tidak ada, pakai salinan
/// `TranscodedWallpaper` di `%APPDATA%` (Slideshow/Spotlight).
pub(crate) fn mica_resolve_wallpaper_path() -> String {
    let mut buf = [0u16; MICA_PATH_MAX];
    // SAFETY: buf berukuran MICA_PATH_MAX; uiparam = kapasitas dalam karakter (C++ 2063).
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            MICA_PATH_MAX as u32,
            Some(buf.as_mut_ptr() as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let path = String::from_utf16_lossy(&buf[..len]);
    let missing = path.is_empty() || {
        let w = to_wide_z(&path);
        // SAFETY: w berakhir NUL.
        unsafe { GetFileAttributesW(pcw(&w)) == INVALID_FILE_ATTRIBUTES }
    };
    if missing {
        if let Some(ad) = std::env::var_os("APPDATA") {
            let ad = ad.to_string_lossy().into_owned();
            // GetEnvironmentVariableW (C++ 2066) gagal bila nilai kosong atau tidak muat di MAX_PATH.
            if !ad.is_empty() && ad.encode_utf16().count() < MICA_PATH_MAX {
                return mica_clip(&format!(
                    "{ad}\\Microsoft\\Windows\\Themes\\TranscodedWallpaper"
                ));
            }
        }
    }
    path
}

/// `MicaGetWallpaperStamp` (C++ 2071-2079): ukuran dan waktu tulis berkas; (0, 0) bila jalur kosong atau gagal.
pub(crate) fn mica_wallpaper_stamp(path: &str) -> (u64, u64) {
    if path.is_empty() {
        return (0, 0);
    }
    let w = to_wide_z(path);
    let mut data = WIN32_FILE_ATTRIBUTE_DATA::default();
    // SAFETY: data bertipe WIN32_FILE_ATTRIBUTE_DATA, sesuai level GetFileExInfoStandard.
    let ok = unsafe {
        GetFileAttributesExW(
            pcw(&w),
            GetFileExInfoStandard,
            &mut data as *mut WIN32_FILE_ATTRIBUTE_DATA as *mut c_void,
        )
        .is_ok()
    };
    if !ok {
        return (0, 0);
    }
    let size = ((data.nFileSizeHigh as u64) << 32) | data.nFileSizeLow as u64;
    let write = ((data.ftLastWriteTime.dwHighDateTime as u64) << 32)
        | data.ftLastWriteTime.dwLowDateTime as u64;
    (size, write)
}

/// `MicaRememberWallpaper` (C++ 2081-2086): simpan jalur (dipotong) beserta stempel berkasnya.
fn mica_remember_wallpaper(path: &str) {
    let clipped = mica_clip(path);
    let (size, write) = mica_wallpaper_stamp(&clipped);
    MICA_PATH.replace(clipped);
    MICA_FILE_SIZE.set(size);
    MICA_FILE_WRITE.set(write);
}

/// Nilai REG_SZ di `Control Panel\Desktop` (HKCU) seperti `RegGetValueW` ke buffer `st[8]` (C++ 2099-2103):
/// nilai yang lebih dari 7 karakter tidak muat, sehingga nilai awal dipakai.
fn reg_wallpaper_value(name: &str, default: &str) -> String {
    reg::get_string(DESKTOP_KEY, name)
        .filter(|v| v.encode_utf16().count() <= 7)
        .unwrap_or_else(|| default.to_string())
}

/// Bagian decode-skala-salin `MicaLoad` (C++ 2118-2144). Piksel wallpaper menimpa `px` tanpa blend.
fn mica_paint_wallpaper(
    wic: &IWICImagingFactory,
    path: &str,
    style: i32,
    mw: i32,
    mh: i32,
    tw: i32,
    th: i32,
    px: &mut [u8],
) {
    if path.is_empty() {
        return;
    }
    let wz = to_wide_z(path);
    // SAFETY: wz berakhir NUL; antarmuka WIC dipakai di utas UI yang sama.
    unsafe {
        let Ok(dec) = wic.CreateDecoderFromFilename(
            pcw(&wz),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        ) else {
            return;
        };
        let Ok(fr) = dec.GetFrame(0) else {
            return;
        };
        let (mut iw, mut ih) = (0u32, 0u32);
        if fr.GetSize(&mut iw, &mut ih).is_err() || iw == 0 || ih == 0 {
            return;
        }
        let (mut sx, mut sy) = (mw as f32 / iw as f32, mh as f32 / ih as f32);
        if style == 6 {
            // pas
            let m = sx.min(sy);
            sx = m;
            sy = m;
        } else if style == 0 {
            // tengah
            sx = 1.0;
            sy = 1.0;
        } else if style != 2 {
            // isi, bentang (regang = sx/sy apa adanya)
            let m = sx.max(sy);
            sx = m;
            sy = m;
        }
        let dw = ((iw as f32 * sx / MICA_K as f32).round() as i32).clamp(1, 4 * tw);
        let dh = ((ih as f32 * sy / MICA_K as f32).round() as i32).clamp(1, 4 * th);
        let Some(mut im) = mica_alloc(dw as usize * dh as usize * 4) else {
            return;
        };
        let Ok(sc) = wic.CreateBitmapScaler() else {
            return;
        };
        if sc
            .Initialize(&*fr, dw as u32, dh as u32, WICBitmapInterpolationModeFant)
            .is_err()
        {
            return;
        }
        let Ok(cv) = wic.CreateFormatConverter() else {
            return;
        };
        if cv
            .Initialize(
                &*sc,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None::<&IWICPalette>,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .is_err()
        {
            return;
        }
        if cv.CopyPixels(std::ptr::null(), (dw * 4) as u32, &mut im).is_err() {
            return;
        }
        let ox = (tw - dw) / 2;
        let oy = (th - dh) / 2;
        for y in 0..dh {
            let ty = y + oy;
            if ty < 0 || ty >= th {
                continue;
            }
            for x in 0..dw {
                let tx = x + ox;
                if tx < 0 || tx >= tw {
                    continue;
                }
                let s = ((y * dw + x) * 4) as usize;
                let d = ((ty * tw + tx) * 4) as usize;
                px[d..d + 4].copy_from_slice(&im[s..s + 4]);
            }
        }
    }
}

/// `MicaLoad` (C++ 2089-2164): muat wallpaper, perkecil 1/MICA_K, buramkan. `false` = kaca mati.
/// Mengisi `MICA_SRC`, `MICA_MON`, dan `WALLPAPER_TINT` (C++ 2157).
pub fn mica_load() -> bool {
    // SafeRelease(g_micaBr, g_micaBmp, g_micaSrc) (C++ 2091-2094)
    MICA_BR.replace(None);
    MICA_BMP.replace(None);
    MICA_SRC.replace(None);
    MICA_GEN.set(-1);
    let Some(wic) = WIC.borrow().as_ref().cloned() else {
        return false;
    };
    let path = mica_resolve_wallpaper_path();
    mica_remember_wallpaper(&path);
    let st = reg_wallpaper_value("WallpaperStyle", "10");
    let tl = reg_wallpaper_value("TileWallpaper", "0");
    let style = if wtoi(&tl) == 1 { 10 } else { wtoi(&st) }; // ubin didekati sebagai isi (C++ 2104)
    let (mw, mh) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    MICA_MON.set(RECT {
        left: 0,
        top: 0,
        right: mw,
        bottom: mh,
    });
    let tw = (mw + MICA_K - 1) / MICA_K;
    let th = (mh + MICA_K - 1) / MICA_K;
    if tw <= 0 || th <= 0 {
        return false;
    }
    let Some(mut px) = mica_alloc(tw as usize * th as usize * 4) else {
        return false;
    };
    // COLORREF: byte rendah = R, lalu G, lalu B (C++ 2111-2112)
    let bg = unsafe { GetSysColor(COLOR_DESKTOP) };
    let (r, g, b) = (
        (bg & 0xff) as u8,
        ((bg >> 8) & 0xff) as u8,
        ((bg >> 16) & 0xff) as u8,
    );
    for p in px.chunks_exact_mut(4) {
        p[0] = b;
        p[1] = g;
        p[2] = r;
        p[3] = 255;
    }
    mica_paint_wallpaper(&wic, &path, style, mw, mh, tw, th, &mut px);
    // Tint: rata-rata warna wallpaper di atas latar desktop (C++ 2149-2157)
    let (mut red, mut green, mut blue) = (0u64, 0u64, 0u64);
    for p in px.chunks_exact(4) {
        let inv = 255 - p[3] as u32;
        red += ((p[2] as u32 + r as u32 * inv + 127) / 255) as u64;
        green += ((p[1] as u32 + g as u32 * inv + 127) / 255) as u64;
        blue += ((p[0] as u32 + b as u32 * inv + 127) / 255) as u64;
    }
    WALLPAPER_TINT.set(w12_average_rgb(red, green, blue, (tw * th) as u32));
    if !GLASS.get() {
        return false; // C++ 2158
    }
    for p in px.chunks_exact_mut(4) {
        p[3] = 255; // kaca selalu padat; transparansi diatur lapisan warna di atasnya (C++ 2159)
    }
    mica_blur(&mut px, tw, th, 2);
    let src = unsafe {
        wic.CreateBitmapFromMemory(
            tw as u32,
            th as u32,
            &GUID_WICPixelFormat32bppPBGRA,
            (tw * 4) as u32,
            &px,
        )
    };
    MICA_SRC.replace(src.ok());
    let ok = MICA_SRC.borrow().is_some();
    ok
}

// ---------------------------------------------------------------------------
// Mica(b): tangkapan layar penuh dan posisi jendela (C++ 2166–2261)
// ---------------------------------------------------------------------------

/// `(HANDLE)(INT_PTR)(v)` untuk `SetPropW` (C++ 2250–2251). Hanya penanda bilangan bulat.
fn prop_value(v: i32) -> HANDLE {
    HANDLE((v as isize) as *mut std::ffi::c_void)
}

/// Salin piksel DIB 32-bit hasil `StretchBlt` dari layar (C++ 2173–2196). `None` bila gagal.
/// Deviasi: C++ mengolah piksel DIB di tempat; port menyalinnya ke `Vec`, lalu DIB dibebaskan.
fn mica_grab(tw: i32, th: i32, mw: i32, mh: i32) -> Option<Vec<u8>> {
    unsafe {
        let mut bi: BITMAPINFO = std::mem::zeroed(); // C++ `= {}`
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = tw;
        bi.bmiHeader.biHeight = -th; // top-down: koordinat bitmap sama dengan layar
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB.0;

        let screen = GetDC(None);
        if screen.0.is_null() {
            return None;
        }
        let sample = CreateCompatibleDC(Some(screen));
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib = match CreateDIBSection(Some(screen), &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(d) => d,
            Err(_) => {
                if !sample.0.is_null() {
                    let _ = DeleteDC(sample);
                }
                ReleaseDC(None, screen);
                return None;
            }
        };
        if sample.0.is_null() || bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(dib.0));
            if !sample.0.is_null() {
                let _ = DeleteDC(sample);
            }
            ReleaseDC(None, screen);
            return None;
        }

        let old = SelectObject(sample, HGDIOBJ(dib.0));
        SetStretchBltMode(sample, HALFTONE);
        let _ = SetBrushOrgEx(sample, 0, 0, None);
        let copied = StretchBlt(
            sample,
            0,
            0,
            tw,
            th,
            Some(screen),
            0,
            0,
            mw,
            mh,
            ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
        )
        .as_bool();
        let out = if copied {
            let n = tw as usize * th as usize * 4;
            mica_alloc(n).map(|mut v| {
                std::ptr::copy_nonoverlapping(bits as *const u8, v.as_mut_ptr(), n);
                v
            })
        } else {
            None
        };
        SelectObject(sample, old);
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(sample);
        ReleaseDC(None, screen);
        out
    }
}

/// `MicaCaptureFullscreen` (C++ 2166–2235): ambil wallpaper dari layar penuh sebagai kaca.
/// `true` bila kaca diganti. Dipanggil saat mode layar penuh aktif.
pub fn mica_capture_fullscreen() -> bool {
    if !GLASS.get() {
        return false; // C++ 2168
    }
    let Some(wic) = WIC.borrow().as_ref().cloned() else {
        return false;
    };
    let (mw, mh) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let tw = (mw + MICA_K - 1) / MICA_K;
    let th = (mh + MICA_K - 1) / MICA_K;
    if mw <= 0 || mh <= 0 || tw <= 0 || th <= 0 {
        return false;
    }
    let Some(mut px) = mica_grab(tw, th, mw, mh) else {
        return false;
    };
    let (mut red, mut green, mut blue) = (0u64, 0u64, 0u64);
    // Mode layar penuh eksklusif kadang memberi frame hitam melalui GDI (C++ 2202).
    let mut has_signal = false;
    for p in px.chunks_exact_mut(4) {
        p[3] = 255;
        blue += p[0] as u64;
        green += p[1] as u64;
        red += p[2] as u64;
        if p[0] > 2 || p[1] > 2 || p[2] > 2 {
            has_signal = true;
        }
    }
    if !has_signal {
        return false;
    }
    let tint = w12_average_rgb(red, green, blue, (tw * th) as u32);
    mica_blur(&mut px, tw, th, 2);
    let src = unsafe {
        wic.CreateBitmapFromMemory(
            tw as u32,
            th as u32,
            &GUID_WICPixelFormat32bppPBGRA,
            (tw * 4) as u32,
            &px,
        )
    };
    let Ok(captured) = src else {
        return false;
    };
    // SafeRelease(g_micaBr, g_micaBmp, g_micaSrc) lalu ganti sumber (C++ 2216–2224).
    MICA_BR.replace(None);
    MICA_BMP.replace(None);
    MICA_SRC.replace(Some(captured));
    MICA_GEN.set(-1);
    MICA_MON.set(RECT {
        left: 0,
        top: 0,
        right: mw,
        bottom: mh,
    });
    WALLPAPER_TINT.set(tint);
    apply_wallpaper_surface_tint();
    true
}

/// `MicaRestoreWallpaper` (C++ 2237–2243): kembalikan kaca dari wallpaper setelah layar penuh selesai.
pub fn mica_restore_wallpaper() {
    if !MICA_FULLSCREEN_SNAPSHOT.get() {
        return;
    }
    MICA_FULLSCREEN_SNAPSHOT.set(false);
    GLASS_OK.set(mica_load());
    apply_wallpaper_surface_tint();
}

/// `MicaBegin` (C++ 2246–2252): catat posisi layar jendela yang sedang dirender.
pub fn mica_begin(h: HWND, wr: &RECT) {
    MICA_ORG.set(POINT {
        x: wr.left,
        y: wr.top,
    });
    unsafe {
        let _ = SetPropW(h, w!("w12mx"), Some(prop_value(wr.left + 0x40000)));
        let _ = SetPropW(h, w!("w12my"), Some(prop_value(wr.top + 0x40000)));
    }
}

/// `MicaMoved` (C++ 2255–2261): jendela pindah dari posisi render terakhir, jadi gambar ulang.
pub fn mica_moved(h: HWND, lp: LPARAM) {
    if !GLASS_OK.get() || lp.0 == 0 {
        return;
    }
    // SAFETY: lp dari WM_WINDOWPOSCHANGED menunjuk ke WINDOWPOS yang valid selama pesan diproses.
    let p = unsafe { &*(lp.0 as *const WINDOWPOS) };
    if (p.flags.0 & SWP_NOMOVE.0) != 0 {
        return;
    }
    let mx = unsafe { GetPropW(h, w!("w12mx")) };
    if mx.0.is_null() {
        return;
    }
    let my = unsafe { GetPropW(h, w!("w12my")) };
    if (mx.0 as isize) != ((p.x + 0x40000) as isize)
        || (my.0 as isize) != ((p.y + 0x40000) as isize)
    {
        w12_invalidate(h, None, false);
    }
}

// ---------------------------------------------------------------------------
// Sisa C2 (C++ 2264–2634): MicaFill, Surface, TextPair, MediaSourceName, DrawItem, DockRim,
// LayerPresent, RenderPill, GlowUpdate, StartAnim, DragTargetX. Ditulis integrator (Tahap B).
// ---------------------------------------------------------------------------

/// `g_wgPill` (:1509): widget yang sedang ditekan/digeser; -1 = tidak ada.
pub static WG_PILL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
/// `g_dragPill`, `g_dragFrom`, `g_dragTo`, `g_dragDownX`, `g_dragX` (:1517).
pub static DRAG_PILL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static DRAG_FROM: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static DRAG_TO: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static DRAG_DOWN_X: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static DRAG_X: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
/// `g_nslots`, `g_slots[MAXITEMS]` (:1517): urutan indeks item aplikasi yang bisa digeser.
pub static NSLOTS: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static SLOTS: SyncCell<Cell<[i32; MAXITEMS as usize]>> = SyncCell::new(Cell::new([0; MAXITEMS as usize]));
/// `g_glowApplied` (:2577).
static GLOW_APPLIED: SyncCell<Cell<RECT>> = SyncCell::new(Cell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }));

fn v2(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

fn gstop(position: f32, color: D2D1_COLOR_F) -> D2D1_GRADIENT_STOP {
    D2D1_GRADIENT_STOP { position, color }
}

fn rrect(l: f32, t: f32, r: f32, b: f32, rad: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT { rect: rf(l, t, r, b), radiusX: rad, radiusY: rad }
}

/// `PathFindFileNameW`: bagian setelah pemisah terakhir (`\`, `/`, `:`).
pub fn path_file_part(p: &str) -> &str {
    match p.rfind(['\\', '/', ':']) {
        Some(i) => &p[i + 1..],
        None => p,
    }
}

/// `MicaFill` (C++ 2264–2292): isi bentuk dengan wallpaper buram sesuai posisi layarnya.
pub fn mica_fill(l: f32, t: f32, r: f32, b: f32, rad: f32) -> bool {
    if !GLASS_OK.get() {
        return false;
    }
    let Some(src) = MICA_SRC.borrow().as_ref().cloned() else { return false };
    let Some(rt) = RT.borrow().as_ref().cloned() else { return false };
    if MICA_GEN.get() != RT_GEN.get() || MICA_BR.borrow().is_none() {
        MICA_BR.replace(None);
        MICA_BMP.replace(None);
        let Ok(bmp) = (unsafe { rt.CreateBitmapFromWicBitmap(&src, None) }) else { return false };
        MICA_BMP.replace(Some(bmp.clone()));
        let bp = D2D1_BITMAP_BRUSH_PROPERTIES {
            extendModeX: D2D1_EXTEND_MODE_CLAMP,
            extendModeY: D2D1_EXTEND_MODE_CLAMP,
            interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
        };
        let Ok(br) = (unsafe { rt.CreateBitmapBrush(&bmp, Some(&bp as *const _), None) }) else { return false };
        MICA_BR.replace(Some(br));
        MICA_GEN.set(RT_GEN.get());
    }
    let Some(br) = MICA_BR.borrow().as_ref().cloned() else { return false };
    let mut m = Matrix3x2::default();
    unsafe { rt.GetTransform(&mut m) };
    if m.M11.abs() < 1e-4 || m.M22.abs() < 1e-4 {
        return false;
    }
    // ruang kuas -> dunia (B) -> perangkat (M) harus = piksel bitmap * K + monitor - asal jendela
    let k = MICA_K as f32;
    let mon = MICA_MON.get();
    let org = MICA_ORG.get();
    let bm = Matrix3x2 {
        M11: k / m.M11,
        M12: 0.0,
        M21: 0.0,
        M22: k / m.M22,
        M31: ((mon.left - org.x) as f32 - m.M31) / m.M11,
        M32: ((mon.top - org.y) as f32 - m.M32) / m.M22,
    };
    unsafe {
        br.SetTransform(&bm);
        if rad > 0.0 {
            rt.FillRoundedRectangle(&rrect(l, t, r, b, rad), &br);
        } else {
            rt.FillRectangle(&rf(l, t, r, b), &br);
        }
    }
    true
}

/// `Surface` (C++ 2295–2332): kaca Mica (bila aktif), lapisan warna, dan garis tepi 1 px.
pub fn surface(l: f32, t: f32, r: f32, b: f32, rad: f32, fill: D2D1_COLOR_F) {
    mica_fill(l, t, r, b, rad);
    fill_rr(l, t, r, b, rad, fill, 1.0);
    let rr = if rad > 0.5 { rad - 0.5 } else { 0.0 };
    let ed = rrect(l + 0.5, t + 0.5, r - 0.5, b - 0.5, rr);
    let Some(rt) = RT.borrow().as_ref().cloned() else { return };
    if GLASS_OK.get() && !LIGHT.get() {
        // Kaca gelap: kilau tipis di separuh atas dan tepi yang sedikit lebih terang di atas daripada di bawah.
        let sh = [gstop(0.0, rgba(255, 255, 255, 0.055)), gstop(1.0, rgba(255, 255, 255, 0.0))];
        unsafe {
            if let Ok(sc) = rt.CreateGradientStopCollection(&sh, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) {
                let p = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: v2(l, t), endPoint: v2(l, t + (b - t) * 0.55) };
                if let Ok(lb) = rt.CreateLinearGradientBrush(&p, None, &sc) {
                    rt.FillRoundedRectangle(&rrect(l, t, r, b, rad), &lb);
                }
            }
            let rim = [gstop(0.0, rgba(255, 255, 255, 0.20)), gstop(1.0, rgba(255, 255, 255, 0.06))];
            if let Ok(sc) = rt.CreateGradientStopCollection(&rim, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) {
                let p = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: v2(l, t), endPoint: v2(l, b) };
                if let Ok(lb) = rt.CreateLinearGradientBrush(&p, None, &sc) {
                    rt.DrawRoundedRectangle(&ed, &lb, 1.0, None::<&ID2D1StrokeStyle>);
                    return;
                }
            }
        }
    }
    set_color(PALETTE.borrow().line, 1.0);
    if let Some(br) = BR.borrow().as_ref() {
        unsafe { rt.DrawRoundedRectangle(&ed, br, 1.0, None::<&ID2D1StrokeStyle>) };
    }
}

/// `TextPair` (C++ 2335–2354): dua teks berbeda ukuran pada satu garis dasar ("21.41" besar + "WIB" kecil).
#[allow(clippy::too_many_arguments)]
pub fn text_pair(
    a: &str,
    fa: Option<&IDWriteTextFormat>,
    b: &str,
    fb: Option<&IDWriteTextFormat>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    ca: D2D1_COLOR_F,
    cb: D2D1_COLOR_F,
) {
    let (Some(fa), Some(fb)) = (fa, fb) else { return };
    let Some(dw) = DW.borrow().as_ref().cloned() else { return };
    let Some(rt) = RT.borrow().as_ref().cloned() else { return };
    let s = to_wide(&copy_wide(&format!("{} {}", a, b), 96));
    let n = s.len() as u32;
    let la = (to_wide(a).len() as u32).min(n);
    let Ok(l) = (unsafe { dw.CreateTextLayout(&s, fa, w, h) }) else { return };
    let rb = DWRITE_TEXT_RANGE { startPosition: la, length: n - la };
    unsafe {
        let _ = l.SetFontSize(fb.GetFontSize(), rb);
        let _ = l.SetFontWeight(fb.GetFontWeight(), rb);
        if let Ok(b2) = rt.CreateSolidColorBrush(&cb, None) {
            let _ = l.SetDrawingEffect(&b2, rb);
        }
    }
    set_color(ca, 1.0);
    if let Some(br) = BR.borrow().as_ref() {
        unsafe { rt.DrawTextLayout(v2(x, y), &l, br, D2D1_DRAW_TEXT_OPTIONS_NONE) };
    }
}

/// `MediaSourceName` (C++ 2357–2371): nama aplikasi sumber media ("Spotify"). Hasil dipotong ke `cch - 1` karakter.
pub fn media_source_name(cch: usize) -> String {
    let s = from_wide(&MEDIA.borrow().src);
    if s.is_empty() {
        return String::new();
    }
    let ls = s.to_lowercase();
    for a in APPS.borrow().iter() {
        if (!a.aumid.is_empty() && a.aumid.to_lowercase() == ls)
            || (!a.exe.is_empty() && path_file_part(&a.exe).to_lowercase() == ls)
        {
            return copy_wide(&a.name, cch);
        }
    }
    // Sumber video/musik umum yang AUMID-nya tidak terbaca manusia (browser memakai AUMID pendek atau hash).
    let known: &[(&str, &str)] = &[
        ("msedge", "Microsoft Edge"),
        ("chrome", "Google Chrome"),
        ("308046b0af4a39cb", "Firefox"),
        ("brave", "Brave"),
        ("opera", "Opera"),
        ("vlc", "VLC"),
        ("microsoft.zunevideo", "Film & TV"),
        ("microsoft.zunemusic", "Media Player"),
        ("microsoft.media.player", "Media Player"),
    ];
    for (k, name) in known {
        if ls == *k || ls.starts_with(&format!("{}_", k)) || ls.starts_with(&format!("{}.exe", k)) {
            return copy_wide(name, cch);
        }
    }
    if ls.contains("zen") && (ls.len() == 16 || ls.ends_with("zen.exe")) {
        return copy_wide("Zen", cch);
    }
    let base = match s.rfind('!') {
        Some(i) => &s[i + 1..],
        None => path_file_part(&s),
    };
    let mut out = copy_wide(base, cch);
    if let Some(d) = out.rfind('.') {
        if out[d..].eq_ignore_ascii_case(".exe") {
            out.truncate(d);
        }
    }
    if let Some(c) = out.chars().next() {
        let up: String = c.to_uppercase().collect();
        out.replace_range(..c.len_utf8(), &up);
    }
    out
}

/// `DrawItem` (C++ 2373–2469): gambar satu item bilah (aplikasi, Start, jam, media, ikon tray).
pub fn draw_item(idx: i32, h: f32) {
    let (it, prev, next) = {
        let v = ITEMS.borrow();
        let Some(it) = v.get(idx as usize).copied() else { return };
        let prev = if idx > 0 { v.get(idx as usize - 1).copied() } else { None };
        let next = v.get(idx as usize + 1).copied();
        (it, prev, next)
    };
    let pal = *PALETTE.borrow();
    let x = it.dx;
    let w = it.w;
    let cx = (x + w / 2.0).round();
    let cy = (h / 2.0).round();
    let ic = icon_size() as f32;
    let top = it.pill >= P_TOP;
    let pressed = PRESSED.get() == idx;
    let dock = MODE.get() == MODE_DOCK && !top;
    let ins = if top { sf(3.0) } else { sf(1.0) };
    let vins = if dock { ((h - sf(42.0)) / 2.0).round() } else { sf(4.0) };
    let bg = rf(x + ins, vins, x + w - ins, h - vins);
    let rad = if dock { sf(12.0) } else { sf(5.0) };
    if it.ty == IT_APP && APPS.borrow().get(it.app as usize).is_some_and(|a| a.flash) {
        fill_rr(bg.left, bg.top, bg.right, bg.bottom, rad, pal.flash, 1.0);
    }
    let hover_bg = !top && (it.ty != IT_APP || dock);
    if it.hov > 0.01 && hover_bg {
        fill_rr(bg.left, bg.top, bg.right, bg.bottom, rad, if pressed { pal.press } else { pal.hover }, it.hov);
    }
    let small = SMALL.get();
    match it.ty {
        IT_APP => {
            let Some((launch, nw)) = APPS.borrow().get(it.app as usize).map(|a| (a.launch.clone(), a.wins.len())) else {
                return;
            };
            // efek dok: ikon membesar sedikit saat disorot (tetangga ikut sedikit), memantul saat aplikasi dibuka
            let mut mag = 0.0f32;
            if !small && it.pill < NBOTTOM {
                // Dock: magnifikasi lebih terasa (ala macOS); mode lain tetap halus.
                let (k0, k1) = if dock { (0.24, 0.09) } else { (0.16, 0.05) };
                mag = it.hov * k0;
                if let Some(p) = prev {
                    if p.ty == IT_APP && p.pill == it.pill {
                        mag = mag.max(p.hov * k1);
                    }
                }
                if idx + 1 < NITEMS.get() {
                    if let Some(n) = next {
                        if n.ty == IT_APP && n.pill == it.pill {
                            mag = mag.max(n.hov * k1);
                        }
                    }
                }
            }
            if DRAG_ON.get() && idx == DRAG_ITEM.get() {
                mag = mag.max(0.18);
            } else if SETTLE_K.get() > 0.0 && it.app == SETTLE_APP.get() {
                mag = mag.max(0.18 * SETTLE_K.get());
            }
            let mut s = ic * (1.0 + mag);
            if pressed && !DRAG_ON.get() {
                s -= sf(2.0);
            }
            let mut lift = (s - ic) * 0.55;
            let bouncing = {
                let bk = BOUNCE_KEY.borrow();
                !bk.is_empty() && *bk == launch && nw == 0
            };
            if bouncing {
                let t = unsafe { windows::Win32::System::SystemInformation::GetTickCount() }.wrapping_sub(BOUNCE_START.get());
                if t < 1500 {
                    lift += sf(8.0) * ((t as f32) * 3.14159 / 300.0).sin().abs() * (1.0 - t as f32 / 1500.0);
                }
            }
            if let Some(a) = APPS.borrow_mut().get_mut(it.app as usize) {
                draw_icon_tile(&mut a.icon, cx, cy - sf(1.0) - lift, s.round() as i32);
            }
            if nw > 0 {
                // titik aplikasi berjalan; melebar menjadi garis aksen saat jendelanya fokus
                let iw = sf(5.0) + (sf(14.0) - sf(5.0)) * it.act;
                let ih = sf(5.0) - sf(1.5) * it.act;
                let by = h - sf(if dock { 6.0 } else { 4.0 });
                fill_rr(cx - iw / 2.0, by - ih, cx + iw / 2.0, by, ih / 2.0, mix_c(pal.dot, pal.accent, it.act), 1.0);
            }
        }
        IT_NOTE => {
            let ix = x + sf(10.0);
            let tx = ix + ic + sf(10.0);
            draw_icon_tile(&mut NOTE_ICON.borrow_mut(), ix + ic / 2.0, cy, ic.round() as i32);
            text(NOTE_T1, TF_TITLE.borrow().as_ref(), tx, cy - sf(17.0), x + w - sf(2.0), cy + sf(1.0), pal.text, 1.0);
            text(NOTE_T2, TF_SUB.borrow().as_ref(), tx, cy - sf(1.0), x + w - sf(2.0), cy + sf(15.0), pal.sub, 1.0);
        }
        IT_SEARCH => glyph(0xE721, TF_GLYPH.borrow().as_ref(), cx, cy, sf(16.0), pal.glyph, 1.0),
        IT_START => draw_start_logo(cx, cy, if small { sf(17.0) } else { sf(22.0) }),
        IT_TASKVIEW => draw_task_view(cx, cy, if small { sf(16.0) } else { sf(19.0) }),
        IT_NET => draw_net(cx, cy),
        IT_VOL => glyph(vol_glyph(), TF_GLYPH.borrow().as_ref(), cx, cy, sf(16.0), pal.glyph, 1.0),
        IT_BAT => {
            let bg_c = if LIGHT.get() { rgba(251, 250, 254, 1.0) } else { rgba(18, 22, 50, 1.0) };
            draw_battery((x + tray_w() / 2.0).round(), cy, sf(19.0), pal.glyph, bg_c);
            let pct = format!("{}%", if BAT.get() < 0 { 100 } else { BAT.get() });
            text(&pct, TF_TRAY.borrow().as_ref(), x + tray_w() - sf(4.0), 0.0, x + w, h, pal.text, 1.0);
        }
        IT_TCLOCK => {
            // jam dua baris: "21.41 WIB" / "Kam, 5 Nov"
            text(&TIME_TZ.borrow(), TF_SS.borrow().as_ref(), x + sf(10.0), cy - sf(17.0), x + w, cy + sf(1.0), pal.text, 1.0);
            text(&DATE_SHORT.borrow(), TF_XS.borrow().as_ref(), x + sf(10.0), cy, x + w, cy + sf(17.0), pal.sub, 1.0);
        }
        IT_BELL => glyph(0xEA8F, TF_GLYPH.borrow().as_ref(), cx, cy, sf(16.0), pal.glyph, 1.0),
        IT_USB => glyph(0xE88E, TF_GLYPH.borrow().as_ref(), cx, cy, sf(16.0), pal.glyph, 1.0),
        IT_CHEV => glyph(
            if FLY_OPEN.get() { 0xE70D } else { 0xE70E },
            TF_GLYPH_SM.borrow().as_ref(),
            cx,
            cy,
            sf(12.0),
            pal.glyph,
            1.0,
        ),
        IT_CLOCK => {
            // pil jam: "21.41" besar + "WIB", tanggal lengkap di bawahnya
            let (time, tz) = (TIME.borrow().clone(), TZ.borrow().clone());
            text_pair(
                &time,
                TF_HUGE.borrow().as_ref(),
                &tz,
                TF_TZ.borrow().as_ref(),
                x + sf(22.0),
                sf(10.0),
                w - sf(30.0),
                sf(50.0),
                pal.text,
                pal.sub,
            );
            text(&DATE.borrow(), TF_B.borrow().as_ref(), x + sf(22.0), sf(56.0), x + w - sf(12.0), sf(76.0), pal.sub, 1.0);
        }
        IT_MEDIA_MAIN => {
            let art = sf(56.0);
            let ax = x + sf(14.0);
            let tx = ax + art + sf(14.0);
            draw_art(ax, cy - art / 2.0, art);
            let src = media_source_name(64);
            let (artist, title) = {
                let m = MEDIA.borrow();
                (from_wide(&m.artist), from_wide(&m.title))
            };
            let sub = if !artist.is_empty() && !src.is_empty() {
                copy_wide(&format!("{} \u{00B7} {}", artist, src), 300)
            } else {
                copy_wide(if !artist.is_empty() { &artist } else { &src }, 300)
            };
            text(&title, TF_T2.borrow().as_ref(), tx, cy - sf(21.0), x + w - sf(4.0), cy, pal.text, 1.0);
            let sub_s: &str = if sub.is_empty() { " " } else { &sub };
            text(sub_s, TF_S.borrow().as_ref(), tx, cy + sf(1.0), x + w - sf(4.0), cy + sf(20.0), pal.sub, 1.0);
        }
        IT_MEDIA_PREV | IT_MEDIA_NEXT => {
            let r = sf(16.0);
            if it.hov > 0.01 {
                fill_rr(cx - r, cy - r, cx + r, cy + r, r, if pressed { pal.press } else { pal.hover }, it.hov);
            }
            let g = if it.ty == IT_MEDIA_PREV { 0xE892 } else { 0xE893 };
            glyph(g, TF_GLYPH_MD.borrow().as_ref(), cx, cy, sf(14.0), pal.glyph, 1.0);
        }
        IT_MEDIA_PLAY => {
            let r = sf(19.0);
            fill_rr(cx - r, cy - r, cx + r, cy + r, r, mix_c(pal.accent, rgba(255, 255, 255, 1.0), it.hov * 0.12), 1.0);
            let g = if MEDIA.borrow().status == 4 { 0xE769 } else { 0xE768 };
            glyph(g, TF_GLYPH_MD.borrow().as_ref(), cx, cy, sf(14.0), rgba(255, 255, 255, 1.0), 1.0);
        }
        _ => {}
    }
}

/// `DockRim` (C++ 2472–2507): kilau bibir dok mengikuti kursor, ditambah pendar lembut di dalam.
pub fn dock_rim(w: f32, h: f32, rad: f32) {
    let Some(rt) = RT.borrow().as_ref().cloned() else { return };
    let pal = *PALETTE.borrow();
    let light = LIGHT.get();
    let hx = DOCK_HX.get();
    let mut hot = if light { pal.accent } else { rgba(255, 255, 255, 1.0) };
    hot.a = if light { 0.65 } else { 0.90 };
    let mut mid = if light { pal.accent } else { pal.accent_ink };
    mid.a = if light { 0.30 } else { 0.50 };
    let mut transparent_accent = pal.accent_ink;
    transparent_accent.a = 0.0;
    let st = [gstop(0.0, hot), gstop(0.42, mid), gstop(1.0, transparent_accent)];
    unsafe {
        if let Ok(sc) = rt.CreateGradientStopCollection(&st, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) {
            let p = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                center: v2(hx, 0.0),
                gradientOriginOffset: v2(0.0, 0.0),
                radiusX: sf(130.0),
                radiusY: sf(50.0),
            };
            if let Ok(rb) = rt.CreateRadialGradientBrush(&p, None, &sc) {
                let e = rrect(0.5, 0.5, w - 0.5, h - 0.5, rad - 0.5);
                rt.DrawRoundedRectangle(&e, &rb, sf(1.2), None::<&ID2D1StrokeStyle>);
            }
        }
        let mut glow = pal.accent_ink;
        glow.a = if light { 0.10 } else { 0.16 };
        let mut clear_glow = pal.accent_ink;
        clear_glow.a = 0.0;
        let s2 = [gstop(0.0, glow), gstop(1.0, clear_glow)];
        if let Ok(sc) = rt.CreateGradientStopCollection(&s2, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) {
            let p = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                center: v2(hx, 0.0),
                gradientOriginOffset: v2(0.0, 0.0),
                radiusX: sf(150.0),
                radiusY: sf(64.0),
            };
            if let Ok(rb) = rt.CreateRadialGradientBrush(&p, None, &sc) {
                rt.FillRoundedRectangle(&rrect(0.0, 0.0, w, h, rad), &rb);
            }
        }
    }
}

/// `LayerPresent` (C++ 2510–2516): pasang isi DIB bersama (`MEMDC`) ke jendela berlapis. C++ default `alpha` = 255.
pub fn layer_present(h: HWND, wr: &RECT, alpha: u8) {
    let src = POINT { x: 0, y: 0 };
    let dst = POINT { x: wr.left, y: wr.top };
    let sz = SIZE { cx: wr.right - wr.left, cy: wr.bottom - wr.top };
    let bf = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: alpha,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    unsafe {
        let _ = UpdateLayeredWindow(h, None, Some(&dst), Some(&sz), Some(MEMDC.get()), Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
    }
}

/// `RenderPill` (C++ 2518–2574): gambar satu pil (bilah, dok, pil atas, atau widget) ke jendela berlapisnya.
pub fn render_pill(h: HWND, pi: i32) {
    unsafe {
        let _ = RemovePropW(h, w!("w12p"));
    }
    let mut wr = RECT::default();
    unsafe {
        let _ = GetWindowRect(h, &mut wr);
    }
    let w = wr.right - wr.left;
    let ht = wr.bottom - wr.top;
    let Some(rt) = RT.borrow().as_ref().cloned() else { return };
    if w <= 0 || ht <= 0 || !unsafe { IsWindowVisible(h) }.as_bool() {
        return;
    }
    ensure_mem(w, ht);
    let brc = RECT { left: 0, top: 0, right: w, bottom: ht };
    if unsafe { rt.BindDC(MEMDC.get(), &brc) }.is_err() {
        return;
    }
    unsafe {
        rt.BeginDraw();
        rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
    }
    let fw = w as f32;
    let fh = ht as f32;
    let widget = pi == P_SYS || pi == P_CAL;
    // Lengkungan dok dan pil Segmented ~1/3 tinggi; Classic tetap bilah lurus tanpa sudut.
    let classic = MODE.get() == MODE_CLASSIC && pi == 0;
    let ws = if widget { wg_scale(pi) } else { 1.0 };
    let rad = if widget {
        sf(18.0) * ws
    } else if pi >= P_TOP {
        sf(18.0)
    } else if classic {
        0.0
    } else {
        (fh * 0.34).round()
    };
    mica_begin(h, &wr);
    let pal = *PALETTE.borrow();
    if classic {
        mica_fill(0.0, 0.0, fw, fh, 0.0);
        fill_rr(0.0, 0.0, fw, fh, 0.0, pal.veil, 1.0);
        let line = if GLASS_OK.get() && !LIGHT.get() { rgba(255, 255, 255, 0.14) } else { pal.line };
        fill_rr(0.0, 0.0, fw, 1.0, 0.0, line, 1.0);
    } else {
        surface(0.0, 0.0, fw, fh, rad, pal.veil);
    }
    if widget {
        // Isi digambar pada ukuran dasar lalu diskalakan transformasi D2D: teks DirectWrite tetap tajam.
        let sc = Matrix3x2 { M11: ws, M12: 0.0, M21: 0.0, M22: ws, M31: 0.0, M32: 0.0 };
        let id = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 };
        unsafe { rt.SetTransform(&sc) };
        widget_draw(pi, fw / ws, fh / ws);
        unsafe { rt.SetTransform(&id) };
        if WG_HOVER.get() == pi || WG_PILL.get() == pi {
            // pegangan ubah ukuran di sudut kanan bawah, hanya saat disorot
            let gx = fw - sf(9.0);
            let gy = fh - sf(9.0);
            for k in 0..3 {
                for j in 0..=k {
                    let dx = gx - (k - j) as f32 * sf(4.5);
                    let dy = gy - j as f32 * sf(4.5);
                    fill_rr(dx - sf(1.2), dy - sf(1.2), dx + sf(1.2), dy + sf(1.2), sf(1.2), pal.sub, 1.0);
                }
            }
        }
    } else {
        let n = NITEMS.get();
        let pills: Vec<i32> = {
            let v = ITEMS.borrow();
            (0..n).map(|i| v.get(i as usize).map_or(-1, |it| it.pill)).collect()
        };
        for i in 0..n {
            if pills[i as usize] == pi && !(DRAG_ON.get() && i == DRAG_ITEM.get()) {
                draw_item(i, fh);
            }
        }
        if DRAG_ON.get() && DRAG_PILL.get() == pi && DRAG_ITEM.get() >= 0 {
            draw_item(DRAG_ITEM.get(), fh); // ikon yang digeser di atas
        }
        if pi == 0 && MODE.get() == MODE_DOCK {
            let sep = SEP_X.get();
            for s in 0..NSEP.get() {
                let sx = sep[s as usize].round();
                fill_rr(sx, (fh * 0.25).round(), sx + 1.0, (fh * 0.75).round(), 0.0, pal.sep, 1.0);
            }
            if DOCK_HX.get() >= 0.0 {
                dock_rim(fw, fh, rad);
            }
        }
    }
    if let Err(e) = unsafe { rt.EndDraw(None, None) } {
        if e.code() == D2DERR_RECREATE_TARGET {
            create_target();
            w12_invalidate(h, None, false);
            return;
        }
    }
    layer_present(h, &wr, 255);
}

/// `GlowUpdate` (C++ 2579–2615): cahaya lembut di belakang dok (jendela berlapis tembus klik di bawah dok).
pub fn glow_update() {
    let glow = GLOW.get();
    if glow.0.is_null() {
        return;
    }
    pills_ensure();
    let d = PILLS.borrow()[0];
    let vis = MODE.get() == MODE_DOCK && !d.h.0.is_null() && d.shown && !FULLSCREEN.get();
    if !vis {
        if unsafe { IsWindowVisible(glow) }.as_bool() {
            unsafe {
                let _ = ShowWindow(glow, SW_HIDE);
            }
        }
        return;
    }
    // Cahaya hanya di area dok: jendela glow TOPMOST, jadi tidak boleh melewati tepi dok
    // (tidak tembus ke aplikasi di sekitarnya; permintaan 2026-11-14, revisi kedua).
    let r = RECT { left: d.x, top: d.y, right: d.x + d.w, bottom: d.y + d.ht };
    if unsafe { IsWindowVisible(glow) }.as_bool() && rect_eq(&r, &GLOW_APPLIED.get()) && !GLOW_DIRTY.get() {
        return;
    }
    let w = r.right - r.left;
    let ht = r.bottom - r.top;
    ensure_mem(w, ht);
    let brc = RECT { left: 0, top: 0, right: w, bottom: ht };
    let Some(rt) = RT.borrow().as_ref().cloned() else { return };
    if unsafe { rt.BindDC(MEMDC.get(), &brc) }.is_err() {
        return;
    }
    let c0 = PALETTE.borrow().glow;
    let mut c1 = c0;
    c1.a = 0.0;
    let st = [gstop(0.0, c0), gstop(0.55, D2D1_COLOR_F { r: c0.r, g: c0.g, b: c0.b, a: c0.a * 0.45 }), gstop(1.0, c1)];
    unsafe {
        rt.BeginDraw();
        rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
        if let Ok(sc) = rt.CreateGradientStopCollection(&st, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) {
            // Pusat di tengah dok; radius = setengah dok, sehingga alpha sudah 0 di tepi jendela.
            let cxg = w as f32 / 2.0;
            let cyg = ht as f32 / 2.0;
            let p = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                center: v2(cxg, cyg),
                gradientOriginOffset: v2(0.0, 0.0),
                radiusX: w as f32 / 2.0,
                radiusY: ht as f32 / 2.0,
            };
            if let Ok(rb) = rt.CreateRadialGradientBrush(&p, None, &sc) {
                rt.FillRectangle(&rf(0.0, 0.0, w as f32, ht as f32), &rb);
            }
        }
        if rt.EndDraw(None, None).is_err() {
            return;
        }
        let _ = SetWindowPos(glow, Some(d.h), r.left, r.top, w, ht, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
    layer_present(glow, &r, 255);
    GLOW_APPLIED.set(r);
    GLOW_DIRTY.set(false);
}

/// `StartAnim` (C++ 2618): jalankan timer animasi hover bila belum berjalan.
pub fn start_anim() {
    if !ANIM_ON.get() {
        ANIM_ON.set(true);
        set_timer(T_ANIM, 15);
    }
}

/// `DragTargetX` (C++ 2621–2634): posisi tujuan item saat aplikasi digeser.
pub fn drag_target_x(i: i32) -> f32 {
    let items = ITEMS.borrow();
    let xi = |k: i32| items.get(k as usize).map_or(0.0, |it| it.x);
    let slots = SLOTS.get();
    let ns = NSLOTS.get();
    if i == DRAG_ITEM.get() {
        if ns <= 0 {
            return xi(i);
        }
        let lo = xi(slots[0]);
        let hi = xi(slots[(ns - 1) as usize]);
        let x = DRAG_X.get() as f32 - items.get(i as usize).map_or(0.0, |it| it.w) / 2.0;
        return if x < lo { lo } else if x > hi { hi } else { x };
    }
    let mut k = -1;
    for s in 0..ns {
        if slots[s as usize] == i {
            k = s;
        }
    }
    if k < 0 {
        return xi(i);
    }
    let mut e = k;
    let (from, to) = (DRAG_FROM.get(), DRAG_TO.get());
    if from < to && k > from && k <= to {
        e = k - 1;
    } else if from > to && k >= to && k < from {
        e = k + 1;
    }
    xi(slots[e as usize])
}

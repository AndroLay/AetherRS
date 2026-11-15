//! Sumber: w12widgets.inc. Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime.
use super::prelude::*;
use std::ffi::c_void;
use windows::Win32::Globalization::{GetDateFormatEx, ENUM_DATE_FORMATS_FLAGS};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
    PDH_MORE_DATA,
};
use windows::Win32::System::SystemInformation::{
    GetLocalTime, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToFileTime};
use windows::Win32::UI::Shell::ShellExecuteW;

// Sumber: w12widgets.inc:1-12 (global), 120-125 (posisi dan skala).
/// Posisi belum disimpan (`WG_NOPOS` di C++).
pub const WG_NOPOS: i32 = -0x7FFF_FFFF;

pub static RAM: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(-1.0));
pub static GPU: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(-1.0));
pub static CPU_IDLE: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
pub static CPU_TOTAL: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
pub static PDH_Q: SyncCell<Cell<*mut c_void>> = SyncCell::new(Cell::new(std::ptr::null_mut()));
pub static PDH_GPU: SyncCell<Cell<*mut c_void>> = SyncCell::new(Cell::new(std::ptr::null_mut()));
pub static PDH_FAILED: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static CAL_OFF: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static CAL_PREV: SyncCell<Cell<D2D_RECT_F>> = SyncCell::new(Cell::new(D2D_RECT_F {
    left: 0.0,
    top: 0.0,
    right: 0.0,
    bottom: 0.0,
}));
pub static CAL_NEXT: SyncCell<Cell<D2D_RECT_F>> = SyncCell::new(Cell::new(D2D_RECT_F {
    left: 0.0,
    top: 0.0,
    right: 0.0,
    bottom: 0.0,
}));
pub static CAL_HEAD: SyncCell<Cell<D2D_RECT_F>> = SyncCell::new(Cell::new(D2D_RECT_F {
    left: 0.0,
    top: 0.0,
    right: 0.0,
    bottom: 0.0,
}));
pub static WG_X: SyncCell<Cell<[i32; 2]>> = SyncCell::new(Cell::new([WG_NOPOS; 2]));
pub static WG_Y: SyncCell<Cell<[i32; 2]>> = SyncCell::new(Cell::new([WG_NOPOS; 2]));
pub static WG_DOWN: SyncCell<Cell<POINT>> = SyncCell::new(Cell::new(POINT { x: 0, y: 0 }));
pub static WG_START_RC: SyncCell<Cell<RECT>> = SyncCell::new(Cell::new(RECT {
    left: 0,
    top: 0,
    right: 0,
    bottom: 0,
}));
pub static WG_START_SCALE: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(1.0));
static S_GPU_TICK: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));

// Sumber: blok extern INI di w12widgets.inc (GetPrivateProfileString / WritePrivateProfileString).
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetPrivateProfileStringW(
        lp_app_name: *const u16,
        lp_key_name: *const u16,
        lp_default: *const u16,
        lp_returned_string: *mut u16,
        n_size: u32,
        lp_file_name: *const u16,
    ) -> u32;
    fn WritePrivateProfileStringW(
        lp_app_name: *const u16,
        lp_key_name: *const u16,
        lp_string: *const u16,
        lp_file_name: *const u16,
    ) -> i32;
}

/// Sumber: `FtU(f)` (w12widgets.inc:14).
fn ft_u(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | (f.dwLowDateTime as u64)
}

/// Sumber: `SysSample()` (w12widgets.inc:16–56).
pub(crate) fn sys_sample() {
    let mut fi = FILETIME::default();
    let mut fk = FILETIME::default();
    let mut fu = FILETIME::default();
    let got = unsafe {
        GetSystemTimes(
            Some(&mut fi as *mut FILETIME),
            Some(&mut fk as *mut FILETIME),
            Some(&mut fu as *mut FILETIME),
        )
    };
    if got.is_ok() {
        let idle = ft_u(fi);
        let total = ft_u(fk).wrapping_add(ft_u(fu));
        let prev_total = CPU_TOTAL.get();
        if prev_total != 0 && total > prev_total {
            let di = idle.wrapping_sub(CPU_IDLE.get()) as f64;
            let dt = total.wrapping_sub(prev_total) as f64;
            let v = 100.0 * (1.0 - di / dt);
            CPU.set(v.clamp(0.0, 100.0) as f32);
        }
        CPU_IDLE.set(idle);
        CPU_TOTAL.set(total);
    }

    let mut ms = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    if unsafe { GlobalMemoryStatusEx(&mut ms) }.is_ok() {
        RAM.set(ms.dwMemoryLoad as f32);
    }

    if PDH_FAILED.get() {
        return;
    }
    if PDH_Q.get().is_null() {
        let mut q = PDH_HQUERY(std::ptr::null_mut());
        let mut c = PDH_HCOUNTER(std::ptr::null_mut());
        let path = to_wide_z(r"\GPU Engine(*engtype_3D)\Utilization Percentage");
        let open_ok = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut q) } == 0;
        let add_ok = open_ok && unsafe { PdhAddEnglishCounterW(q, pcw(&path), 0, &mut c) } == 0;
        if !add_ok {
            PDH_FAILED.set(true);
            if !q.0.is_null() {
                let _ = unsafe { PdhCloseQuery(q) };
            }
            return;
        }
        PDH_Q.set(q.0);
        PDH_GPU.set(c.0);
        let _ = unsafe { PdhCollectQueryData(q) };
        return;
    }

    // Sampel GPU hanya setiap dua tik (`s_gpuTick & 1` di C++).
    let tick = S_GPU_TICK.get();
    S_GPU_TICK.set(tick.wrapping_add(1));
    if (tick & 1) != 0 {
        return;
    }
    let q = PDH_HQUERY(PDH_Q.get());
    let c = PDH_HCOUNTER(PDH_GPU.get());
    let _ = unsafe { PdhCollectQueryData(q) };
    let mut buf_size: u32 = 0;
    let mut item_count: u32 = 0;
    let st = unsafe {
        PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut buf_size, &mut item_count, None)
    };
    if st != PDH_MORE_DATA || buf_size == 0 {
        return;
    }
    // Buffer `u64` agar alignment 8 byte sesuai struct item PDH.
    let mut buf: Vec<u64> = vec![0u64; (buf_size as usize).div_ceil(8)];
    let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
    let st = unsafe {
        PdhGetFormattedCounterArrayW(
            c,
            PDH_FMT_DOUBLE,
            &mut buf_size,
            &mut item_count,
            Some(items),
        )
    };
    if st != 0 {
        return;
    }
    let mut sum = 0.0f64;
    let list = unsafe { std::slice::from_raw_parts(items, item_count as usize) };
    for it in list {
        if it.FmtValue.CStatus == 0 {
            sum += unsafe { it.FmtValue.Anonymous.doubleValue };
        }
    }
    GPU.set(sum.min(100.0) as f32);
}

/// Sumber: `SysRelease()` (w12widgets.inc:58–64).
pub(crate) fn sys_release() {
    let q = PDH_Q.get();
    if !q.is_null() {
        let _ = unsafe { PdhCloseQuery(PDH_HQUERY(q)) };
    }
    PDH_Q.set(std::ptr::null_mut());
    CPU_TOTAL.set(0);
}

/// Sumber: `WidgetCovered(p)` (w12widgets.inc:66–70).
fn widget_covered(p: &Pill) -> bool {
    if !DESK_LAYER.get() || !FG_VALID.get() {
        return false;
    }
    let fg = FG_RECT.get();
    fg.left <= p.x && fg.top <= p.y && fg.right >= p.x + p.w && fg.bottom >= p.y + p.ht
}

/// Sumber: `WidgetsVisibility()` (w12widgets.inc:73–85).
pub fn widgets_visibility() {
    pills_ensure();
    let p = PILLS.borrow()[P_SYS as usize];
    let on = !p.h.0.is_null() && p.shown && !widget_covered(&p);
    if on && !SYS_TIMER.get() {
        SYS_TIMER.set(true);
        sys_sample();
        set_timer(T_SYSW, 3000);
    } else if !on && SYS_TIMER.get() {
        SYS_TIMER.set(false);
        kill_timer(T_SYSW);
        sys_release();
    }
}

// Sumber: kalender (w12widgets.inc:87–114).

/// Sumber: `CalMonth()`. Mengembalikan (tahun, bulan 1–12) setelah digeser `CAL_OFF`.
fn cal_month() -> (i32, i32) {
    let st = unsafe { GetLocalTime() };
    let t = st.wYear as i32 * 12 + (st.wMonth as i32 - 1) + CAL_OFF.get();
    (t.div_euclid(12), t.rem_euclid(12) + 1)
}

/// Sumber: `DaysIn(y, m)` (tabel bulan dengan kabisat).
fn days_in(y: i32, m: i32) -> i32 {
    match m {
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Sumber: `FirstDow(y, m)`. Lewat SystemTimeToFileTime lalu FileTimeToSystemTime; 0 = Minggu.
fn first_dow(y: i32, m: i32) -> i32 {
    let st = SYSTEMTIME {
        wYear: y as u16,
        wMonth: m as u16,
        wDay: 1,
        ..Default::default()
    };
    let mut ft = FILETIME::default();
    let mut back = SYSTEMTIME::default();
    if unsafe { SystemTimeToFileTime(&st, &mut ft) }.is_ok() {
        let _ = unsafe { FileTimeToSystemTime(&ft, &mut back) };
    }
    back.wDayOfWeek as i32
}

/// Sumber: `CalRows()` = (FirstDow + DaysIn + 6) / 7.
fn cal_rows() -> i32 {
    let (y, m) = cal_month();
    (first_dow(y, m) + days_in(y, m) + 6) / 7
}

// ---------------------------------------------------------------- tata letak widget yang diatur pengguna
// Sumber: w12widgets.inc:116–203. Posisi (px layar) dan skala disimpan di widgets.ini di folder proyek.

/// Sumber: `WgSec(k)` (w12widgets.inc:127).
fn wg_sec(k: usize) -> &'static str {
    if k == 0 {
        "sistem"
    } else {
        "kalender"
    }
}

/// Sumber: `WgIni(p)` (w12widgets.inc:128). Berkas `widgets.ini` di folder `DIR`.
fn wg_ini() -> Vec<u16> {
    let dir = DIR.borrow().clone();
    to_wide_z(&format!("{}\\widgets.ini", dir))
}

/// Sumber: `WgClampScale(s)` (w12widgets.inc:129). Dibulatkan per 5%, dibatasi 70–180%.
fn wg_clamp_scale(sc: f32) -> f32 {
    ((sc * 20.0).round() / 20.0).clamp(0.7, 1.8)
}

/// Port `_wtoi`: lewati spasi di depan, lalu tanda, lalu digit; berhenti di karakter non-digit.
fn wg_wtoi(s: &str) -> i32 {
    let b = s.trim_start().as_bytes();
    let mut i = 0usize;
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut v: i64 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        v = (v * 10 + (b[i] - b'0') as i64).min(i32::MAX as i64 + 1);
        i += 1;
    }
    let v = if neg { -v } else { v };
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// Sumber: `WgReadInt(sec, key, def, ini)` (w12widgets.inc:131–136). Dibaca sebagai teks; kosong = `def`.
fn wg_read_int(sec: &str, key: &str, def: i32, ini: &[u16]) -> i32 {
    let sec_w = to_wide_z(sec);
    let key_w = to_wide_z(key);
    let def_w = to_wide_z("");
    let mut buf = [0u16; 24];
    let _ = unsafe {
        GetPrivateProfileStringW(
            sec_w.as_ptr(),
            key_w.as_ptr(),
            def_w.as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            ini.as_ptr(),
        )
    };
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    if end == 0 {
        def
    } else {
        wg_wtoi(&String::from_utf16_lossy(&buf[..end]))
    }
}

/// Menulis satu key INI. `None` menghapus key (lpString NULL di C++).
fn wg_write(sec: &str, key: &str, val: Option<String>, ini: &[u16]) {
    let sec_w = to_wide_z(sec);
    let key_w = to_wide_z(key);
    let val_w = val.map(|v| to_wide_z(&v));
    let val_ptr = match &val_w {
        Some(v) => v.as_ptr(),
        None => std::ptr::null(),
    };
    let _ = unsafe {
        WritePrivateProfileStringW(sec_w.as_ptr(), key_w.as_ptr(), val_ptr, ini.as_ptr())
    };
}

/// Sumber: `WidgetsLoad()` (w12widgets.inc:138–148).
pub fn widgets_load() {
    let ini = wg_ini();
    let mut xs = [WG_NOPOS; 2];
    let mut ys = [WG_NOPOS; 2];
    let mut sc = WG_SCALE.get();
    for k in 0..2usize {
        let sec = wg_sec(k);
        let mut x = wg_read_int(sec, "x", WG_NOPOS, &ini);
        let mut y = wg_read_int(sec, "y", WG_NOPOS, &ini);
        if x == WG_NOPOS || y == WG_NOPOS {
            x = WG_NOPOS;
            y = WG_NOPOS;
        }
        xs[k] = x;
        ys[k] = y;
        sc[k] = wg_clamp_scale(wg_read_int(sec, "scale", 100, &ini) as f32 / 100.0);
    }
    WG_X.set(xs);
    WG_Y.set(ys);
    WG_SCALE.set(sc);
}

/// Sumber: `WidgetsSave()` (w12widgets.inc:150–165).
pub fn widgets_save() {
    let ini = wg_ini();
    let xs = WG_X.get();
    let ys = WG_Y.get();
    let sc = WG_SCALE.get();
    for k in 0..2usize {
        let sec = wg_sec(k);
        if xs[k] == WG_NOPOS {
            wg_write(sec, "x", None, &ini);
            wg_write(sec, "y", None, &ini);
        } else {
            wg_write(sec, "x", Some(xs[k].to_string()), &ini);
            wg_write(sec, "y", Some(ys[k].to_string()), &ini);
        }
        let pct = (sc[k] * 100.0).round() as i32;
        wg_write(sec, "scale", Some(pct.to_string()), &ini);
    }
}

/// Sumber: `WgBase(pi)` (w12widgets.inc:167–171). Ukuran pada skala 100%.
fn wg_base(pi: i32) -> (i32, i32) {
    let w = s(300.0);
    let h = if pi == P_SYS {
        s(138.0)
    } else {
        s(78.0) + cal_rows() * s(26.0)
    };
    (w, h)
}

/// Sumber: `WgClamp(p)` (w12widgets.inc:173–183). Menjaga pill tetap di area kerja monitor terdekat.
fn wg_clamp(p: &mut Pill) {
    let c = POINT {
        x: p.x + p.w / 2,
        y: p.y + p.ht / 2,
    };
    let mon = unsafe { MonitorFromPoint(c, MONITOR_DEFAULTTONEAREST) };
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(mon, &mut mi) }.as_bool() {
        return;
    }
    let wa = mi.rcWork;
    if p.x + p.w > wa.right {
        p.x = wa.right - p.w;
    }
    if p.y + p.ht > wa.bottom {
        p.y = wa.bottom - p.ht;
    }
    if p.x < wa.left {
        p.x = wa.left;
    }
    if p.y < wa.top {
        p.y = wa.top;
    }
}

/// Sumber: `WidgetsPlace(sw, sh)` (w12widgets.inc:185–203).
/// Pemanggil tidak boleh memegang borrow `PILLS` saat memanggil fungsi ini.
pub fn widgets_place(sw: i32, sh: i32) {
    pills_ensure();
    let want = WIDGETS.get();
    let scale = WG_SCALE.get();
    let wx = WG_X.get();
    let wy = WG_Y.get();
    let media = PILLS.borrow()[P_MEDIA as usize];
    let ia = P_SYS as usize;
    let ib = P_CAL as usize;
    let mut v = PILLS.borrow_mut();
    v[ia].want = want;
    v[ib].want = want;

    let (bw, bh) = wg_base(P_SYS);
    v[ia].w = (bw as f32 * scale[0]).round() as i32;
    v[ia].ht = (bh as f32 * scale[0]).round() as i32;
    v[ia].x = sw - s(22.0) - v[ia].w;
    v[ia].y = if media.want {
        media.y + media.ht + s(12.0)
    } else {
        s(22.0)
    };

    let (bw, bh) = wg_base(P_CAL);
    v[ib].w = (bw as f32 * scale[1]).round() as i32;
    v[ib].ht = (bh as f32 * scale[1]).round() as i32;
    v[ib].x = s(22.0);
    v[ib].y = (sh as f32 * 0.40) as i32;

    for k in 0..2usize {
        let pi = if k == 0 { ia } else { ib };
        if wx[k] != WG_NOPOS {
            v[pi].x = wx[k];
            v[pi].y = wy[k];
        }
        wg_clamp(&mut v[pi]);
    }
}

// ---------------------------------------------------------------- gambar

/// Sumber: `InRF(r, x, y)` (w12widgets.inc:288).
fn in_rf(r: &D2D_RECT_F, x: i32, y: i32) -> bool {
    let (xf, yf) = (x as f32, y as f32);
    xf >= r.left && xf < r.right && yf >= r.top && yf < r.bottom
}

/// Menggambar elips satu lingkaran dengan brush `BR` (pengganti `g_rt->DrawEllipse(&e, g_br, w)`).
fn draw_ellipse_br(e: &D2D1_ELLIPSE, width: f32) {
    let rt_g = RT.borrow();
    let br_g = BR.borrow();
    if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
        let brush: &ID2D1Brush = br;
        unsafe { rt.DrawEllipse(e, brush, width, None::<&ID2D1StrokeStyle>) };
    }
}

/// Sumber: `Ring(cx, cy, r, v)` (w12widgets.inc:206–232). Lintasan redup penuh + busur aksen searah jarum jam dari atas.
fn ring(cx: f32, cy: f32, r: f32, v: f32) {
    let pal = *PALETTE.borrow();
    let e = D2D1_ELLIPSE {
        point: Vector2 { X: cx, Y: cy },
        radiusX: r,
        radiusY: r,
    };
    set_color(pal.track, 1.0);
    draw_ellipse_br(&e, sf(5.0));
    if v <= 0.5 {
        return;
    }
    if v >= 99.5 {
        set_color(pal.accent_ink, 1.0);
        draw_ellipse_br(&e, sf(5.0));
        return;
    }

    let pg = {
        let d = D2D.borrow();
        let Some(f) = d.as_ref() else {
            return;
        };
        match unsafe { f.CreatePathGeometry() } {
            Ok(p) => p,
            Err(_) => return,
        }
    };
    let Ok(sk) = (unsafe { pg.Open() }) else {
        return;
    };
    let a = (v / 100.0) * 6.2831853 - 1.5707963;
    let p0 = Vector2 {
        X: cx,
        Y: cy - r,
    };
    let p1 = Vector2 {
        X: cx + r * a.cos(),
        Y: cy + r * a.sin(),
    };
    let arc = D2D1_ARC_SEGMENT {
        point: p1,
        size: D2D_SIZE_F {
            width: r,
            height: r,
        },
        rotationAngle: 0.0,
        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
        arcSize: if v > 50.0 {
            D2D1_ARC_SIZE_LARGE
        } else {
            D2D1_ARC_SIZE_SMALL
        },
    };
    unsafe {
        sk.BeginFigure(p0, D2D1_FIGURE_BEGIN_HOLLOW);
        sk.AddArc(&arc);
        sk.EndFigure(D2D1_FIGURE_END_OPEN);
        let _ = sk.Close();
    }

    let props = D2D1_STROKE_STYLE_PROPERTIES {
        startCap: D2D1_CAP_STYLE_ROUND,
        endCap: D2D1_CAP_STYLE_ROUND,
        dashCap: D2D1_CAP_STYLE_ROUND,
        lineJoin: D2D1_LINE_JOIN_ROUND,
        miterLimit: 10.0,
        dashStyle: D2D1_DASH_STYLE_SOLID,
        dashOffset: 0.0,
    };
    let ss = {
        let d = D2D.borrow();
        let Some(f) = d.as_ref() else {
            return;
        };
        match unsafe { f.CreateStrokeStyle(&props, None) } {
            Ok(s) => s,
            Err(_) => return,
        }
    };

    set_color(pal.accent_ink, 1.0);
    let rt_g = RT.borrow();
    let br_g = BR.borrow();
    if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
        let geo: &ID2D1Geometry = &pg;
        let brush: &ID2D1Brush = br;
        let _ = unsafe { rt.DrawGeometry(geo, brush, sf(5.0), &ss) };
    }
}

/// Sumber: `WidgetDraw(pi, w, h)` (w12widgets.inc:234–286). Dipanggil `render_pill` (core_c2).
pub fn widget_draw(pi: i32, w: f32, h: f32) {
    let _ = h;
    let pal = *PALETTE.borrow();
    let pad = sf(16.0);

    if pi == P_SYS {
        text(
            "Sistem",
            TF_H.borrow().as_ref(),
            pad,
            sf(14.0),
            w - pad,
            sf(34.0),
            pal.text,
            1.0,
        );
        let lab = ["CPU", "RAM", "GPU"];
        let val = [CPU.get(), RAM.get(), GPU.get()];
        let colw = (w - 2.0 * pad) / 3.0;
        for i in 0..3usize {
            let cx = (pad + colw * (i as f32 + 0.5)).round();
            let cy = sf(44.0) + sf(30.0);
            ring(cx, cy, sf(26.0), if val[i] < 0.0 { 0.0 } else { val[i] });
            let t = if val[i] < 0.0 {
                "\u{2013}".to_string()
            } else {
                format!("{}%", val[i].round() as i32)
            };
            text(
                &t,
                TF_MEDC.borrow().as_ref(),
                cx - sf(30.0),
                cy - sf(12.0),
                cx + sf(30.0),
                cy + sf(12.0),
                pal.text,
                1.0,
            );
            text(
                lab[i],
                TF_SC.borrow().as_ref(),
                cx - sf(30.0),
                cy + sf(33.0),
                cx + sf(30.0),
                cy + sf(51.0),
                pal.sub,
                1.0,
            );
        }
        return;
    }

    // Kalender: judul bulan, panah ganti bulan, Min..Sab, hari ini berlatar aksen.
    let (y, m) = cal_month();
    let st = SYSTEMTIME {
        wYear: y as u16,
        wMonth: m as u16,
        wDay: 1,
        ..Default::default()
    };
    let loc = to_wide_z("id-ID");
    let fmt = to_wide_z("MMMM yyyy");
    let mut out = [0u16; 48];
    let n = unsafe {
        GetDateFormatEx(
            pcw(&loc),
            ENUM_DATE_FORMATS_FLAGS(0),
            Some(&st as *const SYSTEMTIME),
            pcw(&fmt),
            Some(&mut out[..]),
            PCWSTR::null(),
        )
    };
    let title = if n > 0 {
        let end = out.iter().position(|&c| c == 0).unwrap_or(out.len());
        String::from_utf16_lossy(&out[..end])
    } else {
        format!("{}/{}", m, y)
    };
    text(
        &title,
        TF_H.borrow().as_ref(),
        pad,
        sf(14.0),
        w - sf(80.0),
        sf(34.0),
        pal.text,
        1.0,
    );

    CAL_HEAD.set(rf(pad, sf(10.0), w - sf(80.0), sf(38.0)));
    let bx = w - pad - sf(56.0);
    CAL_PREV.set(rf(bx, sf(11.0), bx + sf(26.0), sf(37.0)));
    CAL_NEXT.set(rf(bx + sf(30.0), sf(11.0), bx + sf(56.0), sf(37.0)));
    let pr = CAL_PREV.get();
    let nx = CAL_NEXT.get();
    glyph(
        0xE76B,
        TF_GLYPH_SM.borrow().as_ref(),
        (pr.left + pr.right) / 2.0,
        (pr.top + pr.bottom) / 2.0,
        sf(12.0),
        pal.sub,
        1.0,
    );
    glyph(
        0xE76C,
        TF_GLYPH_SM.borrow().as_ref(),
        (nx.left + nx.right) / 2.0,
        (nx.top + nx.bottom) / 2.0,
        sf(12.0),
        pal.sub,
        1.0,
    );

    const WD: [&str; 7] = ["Min", "Sen", "Sel", "Rab", "Kam", "Jum", "Sab"];
    let gx = pad - sf(4.0);
    let gw = w - 2.0 * gx;
    let cw = gw / 7.0;
    let top = sf(44.0);
    for (i, name) in WD.iter().enumerate() {
        let fi = i as f32;
        text(
            name,
            TF_XSC.borrow().as_ref(),
            gx + fi * cw,
            top,
            gx + (fi + 1.0) * cw,
            top + sf(20.0),
            pal.sub2,
            1.0,
        );
    }

    let now = unsafe { GetLocalTime() };
    let first = first_dow(y, m);
    let days = days_in(y, m);
    for d in 1..=days {
        let cell = first + d - 1;
        let r = cell / 7;
        let c = cell % 7;
        let cx = (gx + (c as f32 + 0.5) * cw).round();
        let cy = (top + sf(20.0) + r as f32 * sf(26.0) + sf(13.0)).round();
        let t = d.to_string();
        let today = now.wYear as i32 == y && now.wMonth as i32 == m && now.wDay as i32 == d;
        if today {
            let rr = sf(12.0);
            fill_rr(cx - rr, cy - rr, cx + rr, cy + rr, rr, pal.accent, 1.0);
            text(
                &t,
                TF_SSC.borrow().as_ref(),
                cx - sf(16.0),
                cy - sf(12.0),
                cx + sf(16.0),
                cy + sf(12.0),
                rgba(255, 255, 255, 1.0),
                1.0,
            );
        } else {
            text(
                &t,
                TF_SC.borrow().as_ref(),
                cx - sf(16.0),
                cy - sf(12.0),
                cx + sf(16.0),
                cy + sf(12.0),
                pal.text,
                1.0,
            );
        }
    }
}

// ---------------------------------------------------------------- klik, geser, ubah ukuran

/// Sumber: `WidgetClick(pi, x, y)` (w12widgets.inc:290–301). Dipanggil dari PillProc (belum ada).
pub fn widget_click(pi: i32, x: i32, y: i32) -> bool {
    if pi == P_SYS {
        let f = to_wide_z("taskmgr.exe");
        let _ = unsafe {
            ShellExecuteW(
                None,
                PCWSTR::null(),
                pcw(&f),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        return true;
    }
    if pi != P_CAL {
        return false;
    }
    let prev = CAL_PREV.get();
    let next = CAL_NEXT.get();
    let head = CAL_HEAD.get();
    if in_rf(&prev, x, y) {
        CAL_OFF.set(CAL_OFF.get() - 1);
    } else if in_rf(&next, x, y) {
        CAL_OFF.set(CAL_OFF.get() + 1);
    } else if in_rf(&head, x, y) {
        CAL_OFF.set(0);
    } else {
        return false;
    }
    relayout(); // jumlah minggu bisa berubah: tinggi widget ikut
    let h = PILLS.borrow().get(pi as usize).map(|p| p.h);
    if let Some(h) = h {
        w12_invalidate(h, None, false);
    }
    true
}

// ---------------------------------------------------------------- geser dan ubah ukuran (dipanggil PillProc)

/// Sumber: `WgInGrip(h, x, y)` (w12widgets.inc:304–309). Gagang ubah ukuran di pojok kanan bawah.
pub fn wg_in_grip(h: HWND, x: i32, y: i32) -> bool {
    let mut rc = RECT::default();
    let _ = unsafe { GetClientRect(h, &mut rc) };
    x >= rc.right - s(22.0) && y >= rc.bottom - s(22.0)
}

/// Sumber: `WgApply(pi, x, y, s, resize)` (w12widgets.inc:312–332). Pasang posisi/skala baru.
/// SetWindowPos memicu PillProc secara sinkron, jadi borrow PILLS sudah dilepas sebelum pemanggilan itu.
fn wg_apply(pi: i32, x: i32, y: i32, sc: f32, resize: bool) {
    let k = wg_idx(pi);
    if k < 0 {
        return;
    }
    let ku = k as usize;
    let mut p = match PILLS.borrow().get(pi as usize) {
        Some(p) => *p,
        None => return,
    };
    if p.h.0.is_null() {
        return;
    }
    if resize {
        let mut scales = WG_SCALE.get();
        scales[ku] = wg_clamp_scale(sc);
        WG_SCALE.set(scales);
        let (bw, bh) = wg_base(pi);
        p.w = (bw as f32 * scales[ku]).round() as i32;
        p.ht = (bh as f32 * scales[ku]).round() as i32;
    }
    p.x = x;
    p.y = y;
    wg_clamp(&mut p);
    let mut xs = WG_X.get();
    xs[ku] = p.x;
    WG_X.set(xs);
    let mut ys = WG_Y.get();
    ys[ku] = p.y;
    WG_Y.set(ys);
    if let Some(slot) = PILLS.borrow_mut().get_mut(pi as usize) {
        *slot = p;
    }
    let ukuran = if resize {
        SET_WINDOW_POS_FLAGS(0)
    } else {
        SWP_NOSIZE
    };
    let _ = unsafe {
        SetWindowPos(
            p.h,
            None,
            p.x,
            p.y,
            p.w,
            p.ht,
            SWP_NOZORDER | SWP_NOACTIVATE | ukuran,
        )
    };
    if let Some(slot) = PILLS.borrow_mut().get_mut(pi as usize) {
        slot.applied = RECT {
            left: p.x,
            top: p.y,
            right: p.x + p.w,
            bottom: p.y + p.ht,
        };
    }
    if resize {
        w12_invalidate(p.h, None, false);
    }
}

/// Sumber: `WgDown(h, pi, x, y)` (w12widgets.inc:334–342). Mulai geser (mode 1) atau ubah ukuran (mode 3).
pub fn wg_down(h: HWND, pi: i32, x: i32, y: i32) {
    WG_PILL.set(pi);
    WG_MODE.set(if wg_in_grip(h, x, y) { 3 } else { 1 });
    let mut down = WG_DOWN.get();
    let _ = unsafe { GetCursorPos(&mut down) };
    WG_DOWN.set(down);
    let mut rc = WG_START_RC.get();
    let _ = unsafe { GetWindowRect(h, &mut rc) };
    WG_START_RC.set(rc);
    WG_START_SCALE.set(wg_scale(pi));
    let _ = unsafe { SetCapture(h) };
}

/// Sumber: `WgDrag(pi)` (w12widgets.inc:344–362). Dipanggil saat kursor bergerak (PillProc).
pub fn wg_drag(pi: i32) {
    if WG_PILL.get() != pi || WG_MODE.get() == 0 {
        return;
    }
    let mut c = POINT::default();
    let _ = unsafe { GetCursorPos(&mut c) };
    let down = WG_DOWN.get();
    let dx = c.x - down.x;
    let dy = c.y - down.y;
    let mut mode = WG_MODE.get();
    if mode == 1 || mode == 3 {
        // ambang 4 px agar klik biasa (bulan, Task Manager) tidak dianggap geser
        if dx.abs() <= s(4.0) && dy.abs() <= s(4.0) {
            return;
        }
        mode += 1;
        WG_MODE.set(mode);
    }
    let start = WG_START_RC.get();
    if mode == 2 {
        wg_apply(pi, start.left + dx, start.top + dy, 0.0, false);
        return;
    }
    // ubah ukuran proporsional mengikuti perubahan relatif terbesar; sudut kiri atas tetap
    let w0 = (start.right - start.left) as f32;
    let h0 = (start.bottom - start.top) as f32;
    let fx = (w0 + dx as f32) / w0;
    let fy = (h0 + dy as f32) / h0;
    let besar = if fx > fy { fx } else { fy };
    wg_apply(pi, start.left, start.top, WG_START_SCALE.get() * besar, true);
}

/// Sumber: `WgUp()` (w12widgets.inc:364–372). true bila tadi menggeser/mengubah ukuran (bukan klik).
pub fn wg_up() -> bool {
    let mode = WG_MODE.get();
    let moved = mode == 2 || mode == 4;
    WG_MODE.set(0);
    WG_PILL.set(-1);
    let cap = unsafe { GetCapture() };
    if !cap.0.is_null() {
        let _ = unsafe { ReleaseCapture() };
    }
    if moved {
        widgets_save();
    }
    moved
}

/// Sumber: `WgStep(pi, d)` (w12widgets.inc:374–380). Perbesar/Perkecil dari menu atau roda Ctrl.
pub fn wg_step(pi: i32, d: f32) {
    let k = wg_idx(pi);
    if k < 0 {
        return;
    }
    let (x, y) = match PILLS.borrow().get(pi as usize) {
        Some(p) => (p.x, p.y),
        None => return,
    };
    let sc = WG_SCALE.get()[k as usize] + d;
    wg_apply(pi, x, y, sc, true);
    widgets_save();
}

/// Sumber: `WgMenu(h, pi, pt)` (w12widgets.inc:382–399). Menu konteks widget.
pub fn wg_menu(h: HWND, pi: i32, pt: POINT) {
    let k = wg_idx(pi);
    if k < 0 {
        return;
    }
    let ku = k as usize;
    let sc = WG_SCALE.get()[ku];
    let xs = WG_X.get();
    let gr = |gray: bool| if gray { MF_GRAYED } else { MF_ENABLED };
    let t1 = to_wide_z("Perbesar\tCtrl+Roda ke atas");
    let t2 = to_wide_z("Perkecil\tCtrl+Roda ke bawah");
    let t3 = to_wide_z("Ukuran normal");
    let t4 = to_wide_z("Kembalikan posisi");
    let t5 = to_wide_z("Menu Win12 Shell");
    let m = match unsafe { CreatePopupMenu() } {
        Ok(m) => m,
        Err(_) => return,
    };
    unsafe {
        let _ = AppendMenuW(m, MF_STRING | gr(sc >= 1.8), 1, pcw(&t1));
        let _ = AppendMenuW(m, MF_STRING | gr(sc <= 0.7), 2, pcw(&t2));
        let _ = AppendMenuW(m, MF_STRING | gr(sc == 1.0), 3, pcw(&t3));
        let _ = AppendMenuW(m, MF_STRING | gr(xs[ku] == WG_NOPOS), 4, pcw(&t4));
        let _ = AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(m, MF_STRING, 5, pcw(&t5));
    }
    let cmd = track_menu(m, h, pt, true);
    unsafe {
        let _ = DestroyMenu(m);
    }
    match cmd {
        1 => wg_step(pi, 0.1),
        2 => wg_step(pi, -0.1),
        3 => wg_step(pi, 1.0 - WG_SCALE.get()[ku]),
        4 => {
            let mut xs2 = WG_X.get();
            xs2[ku] = WG_NOPOS;
            WG_X.set(xs2);
            let mut ys = WG_Y.get();
            ys[ku] = WG_NOPOS;
            WG_Y.set(ys);
            widgets_save();
            relayout();
        }
        5 => main_menu(h, pt, true),
        _ => {}
    }
}

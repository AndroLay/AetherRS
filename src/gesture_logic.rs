//! Port `w12gesture_logic.h` (baris 1–73): keputusan gestur precision touchpad (murni, tanpa Win32). Status: lengkap.
//! Dipakai `w12gesture.inc` (thread Raw Input) di shell. Tes di `tests/gesture_logic.rs`.

/// Konfigurasi ambang gestur. Jarak dalam pecahan lebar/tinggi pad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct W12GestCfg {
    pub start_dist: f32,
    pub step_dist: f32,
    pub vert_dist: f32,
    pub axis_ratio: f32,
    pub lift_ms: u32,
}

/// Port `kW12GestDefault`.
pub const W12_GEST_DEFAULT: W12GestCfg = W12GestCfg {
    start_dist: 0.06,
    step_dist: 0.14,
    vert_dist: 0.10,
    axis_ratio: 1.5,
    lift_ms: 250,
};

pub const W12G_NONE: i32 = 0;
pub const W12G_SW_STEP_FWD: i32 = 1;
pub const W12G_SW_STEP_BACK: i32 = 2;
pub const W12G_SW_COMMIT: i32 = 3;
pub const W12G_DESK_NEXT: i32 = 4;
pub const W12G_DESK_PREV: i32 = 5;
pub const W12G_UP: i32 = 6;
pub const W12G_DOWN: i32 = 7;

/// Status gestur. `mode`: 0 belum, 1 alih aplikasi, 2 desktop, 3 vertikal.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct W12Gest {
    pub fingers: i32,
    pub mode: i32,
    pub x0: f32,
    pub y0: f32,
    pub last_step_x: f32,
}

pub fn w12_gest_abs(v: f32) -> f32 {
    if v < 0.0 {
        -v
    } else {
        v
    }
}

/// Semua jari diangkat / timeout: mode alih aplikasi menghasilkan `W12G_SW_COMMIT`, lalu semua field di-reset.
pub fn w12_gest_lift(g: &mut W12Gest) -> i32 {
    let r = if g.mode == 1 { W12G_SW_COMMIT } else { W12G_NONE };
    g.fingers = 0;
    g.mode = 0;
    g.x0 = 0.0;
    g.y0 = 0.0;
    g.last_step_x = 0.0;
    r
}

/// `cx`, `cy` = rata-rata 0..1 (y ke bawah). Mengembalikan paling banyak satu event per panggilan.
pub fn w12_gest_feed(g: &mut W12Gest, c: &W12GestCfg, fingers: i32, cx: f32, cy: f32) -> i32 {
    if fingers < 3 {
        return if g.fingers >= 3 { w12_gest_lift(g) } else { W12G_NONE };
    }
    if g.fingers == 0 || (g.mode == 0 && fingers != g.fingers) {
        g.fingers = fingers;
        g.x0 = cx;
        g.y0 = cy;
        g.mode = 0;
        return W12G_NONE;
    }
    if g.mode == 0 {
        let dx = cx - g.x0;
        let dy = cy - g.y0;
        let adx = w12_gest_abs(dx);
        let ady = w12_gest_abs(dy);
        if adx >= c.start_dist && adx >= c.axis_ratio * ady {
            g.last_step_x = cx;
            if fingers >= 4 {
                g.mode = 2;
                return if dx < 0.0 { W12G_DESK_NEXT } else { W12G_DESK_PREV };
            }
            g.mode = 1;
            return if dx > 0.0 { W12G_SW_STEP_FWD } else { W12G_SW_STEP_BACK };
        }
        if ady >= c.vert_dist && ady >= c.axis_ratio * adx {
            g.mode = 3;
            return if dy < 0.0 { W12G_UP } else { W12G_DOWN };
        }
        return W12G_NONE;
    }
    if g.mode == 1 {
        let d = cx - g.last_step_x;
        if d >= c.step_dist {
            g.last_step_x += c.step_dist;
            return W12G_SW_STEP_FWD;
        }
        if d <= -c.step_dist {
            g.last_step_x -= c.step_dist;
            return W12G_SW_STEP_BACK;
        }
    }
    W12G_NONE // mode 2/3: sekali per gestur
}

/// Satu kontak laporan HID precision touchpad.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12PtpContact {
    pub x: i32,
    pub y: i32,
    pub tip: bool,
    pub confident: bool,
}

/// Bingkai laporan yang sedang dikumpulkan (`sx`, `sy` = jumlah koordinat kontak tip dan confident).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12PtpFrame {
    pub expected: i32,
    pub got: i32,
    pub fingers: i32,
    pub sx: i64,
    pub sy: i64,
}

/// Laporan HID: `contact_count_field` > 0 memulai bingkai baru; `n` kontak dari `c` ditambahkan
/// (`n` dipotong ke panjang `c`; sumber C tidak memeriksa batas).
/// Mengembalikan 1 = bingkai lengkap (`fingers`, `sx`, `sy` dari kontak tip && confident),
/// 0 = tunggu laporan lanjutan.
pub fn w12_ptp_add(f: &mut W12PtpFrame, contact_count_field: i32, c: &[W12PtpContact], n: i32) -> i32 {
    if contact_count_field > 0 {
        f.expected = contact_count_field;
        f.got = 0;
        f.fingers = 0;
        f.sx = 0;
        f.sy = 0;
    } else if f.expected == 0 {
        return 0; // laporan yatim: tidak ada bingkai yang sedang dikumpulkan
    }
    let n = n.clamp(0, c.len() as i32) as usize;
    for k in &c[..n] {
        f.got += 1;
        if k.tip && k.confident {
            f.fingers += 1;
            f.sx += i64::from(k.x);
            f.sy += i64::from(k.y);
        }
    }
    if f.expected > 0 && f.got >= f.expected {
        f.expected = 0;
        return 1;
    }
    0
}

//! Port `w12tasks_logic.h` (baris 1–196): logika murni tugas (Alt+Tab, tombol Win, desktop virtual, wallpaper, kartu
//! Task View, snap Win+panah). Status: lengkap. Tes di `tests/tasks_logic.rs`. Deviasi: `w12_flow_layout` memotong `n`
//! ke panjang slice dan berhenti pada NaN (C berputar tanpa henti).

/// Persegi float (`W12RectF`): kiri, atas, kanan, bawah.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct W12RectF {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

pub const W12AT_PASS: i32 = 0;
pub const W12AT_SWALLOW: i32 = 1;
pub const W12AT_STEP_FWD: i32 = 2;
pub const W12AT_STEP_BACK: i32 = 3;
pub const W12AT_COMMIT: i32 = 4;
pub const W12AT_CANCEL: i32 = 5;
/// Panah atas / bawah: pindah ke baris atas / bawah (navigasi spasial, `sw_vstep` di overlay).
pub const W12AT_UP: i32 = 6;
pub const W12AT_DOWN: i32 = 7;

/// Status Alt+Tab (`W12AltTab`). `state`: 0 diam, 1 aktif, 2 dibatalkan (menunggu Alt dilepas),
/// 3 mode tetap (Ctrl+Alt+Tab: panel tetap terbuka setelah Alt dilepas, sampai Enter/Spasi/Esc/klik).
/// `alt_vk`: VK Alt yang ditekan (0xA4 atau 0xA5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12AltTab {
    pub state: i32,
    pub alt_vk: u32,
}

pub const W12WK_NONE: i32 = 0;
pub const W12WK_TASKVIEW: i32 = 1;
pub const W12WK_DESK_PREV: i32 = 2;
pub const W12WK_DESK_NEXT: i32 = 3;
pub const W12WK_DESK_NEW: i32 = 4;
pub const W12WK_DESK_CLOSE: i32 = 5;
pub const W12WK_MOVE_PREV: i32 = 6;
pub const W12WK_MOVE_NEXT: i32 = 7;

/// VK Alt: Alt umum (0x12), Alt kiri (0xA4), Alt kanan (0xA5).
pub fn w12_is_alt_vk(vk: u32) -> bool {
    vk == 0x12 || vk == 0xA4 || vk == 0xA5
}

/// Mesin status Alt+Tab. Urutan aturan sama dengan `docs\plans\01-desktop-tasks.md` (Task 1), nomor di komentar.
pub fn w12_alt_tab_key(
    s: &mut W12AltTab,
    vk: u32,
    down: bool,
    alt_held: bool,
    shift_held: bool,
    ctrl_held: bool,
) -> i32 {
    // 0: mode tetap (state 3) punya aturan sendiri.
    if s.state == 3 {
        return w12_alt_tab_sticky_key(s, vk, down, shift_held);
    }
    // 1: Alt turun
    if w12_is_alt_vk(vk) && down {
        if s.state != 0 {
            return W12AT_SWALLOW;
        }
        s.alt_vk = if vk == 0x12 { 0xA4 } else { vk };
        return W12AT_PASS;
    }
    // 2: Tab (juga dari state 2). Ctrl+Alt+Tab masuk mode tetap (state 3).
    if vk == 0x09 && down && alt_held {
        s.state = if ctrl_held { 3 } else { 1 };
        return if shift_held { W12AT_STEP_BACK } else { W12AT_STEP_FWD };
    }
    // 3
    if s.state == 0 {
        return W12AT_PASS;
    }
    // 4: panah dan Esc
    if s.state == 1 && down {
        // Kiri/kanan = mundur/maju satu kartu; atas/bawah = pindah baris (navigasi spasial, permintaan 2026-11-14).
        if vk == 0x25 {
            return W12AT_STEP_BACK;
        }
        if vk == 0x27 {
            return W12AT_STEP_FWD;
        }
        if vk == 0x26 {
            return W12AT_UP;
        }
        if vk == 0x28 {
            return W12AT_DOWN;
        }
        if vk == 0x1B {
            s.state = 2;
            return W12AT_CANCEL;
        }
    }
    // 5: Alt naik
    if w12_is_alt_vk(vk) && !down {
        s.state = 0;
        return W12AT_COMMIT;
    }
    // 6: tombol turun tanpa Alt
    if down && !alt_held {
        s.state = 0;
        return W12AT_COMMIT;
    }
    // 7
    W12AT_SWALLOW
}

/// Mode tetap (state 3, Ctrl+Alt+Tab): panel tetap terbuka walau Alt sudah dilepas.
/// Tab / Shift+Tab / panah = langkah; Enter atau Spasi = pilih; Esc = batal. Klik mouse ditangani `sw_commit`.
fn w12_alt_tab_sticky_key(s: &mut W12AltTab, vk: u32, down: bool, shift_held: bool) -> i32 {
    // Alt di mode ini tidak memilih. Tekan ulang diabaikan; lepas hanya mengosongkan alt_vk.
    if w12_is_alt_vk(vk) {
        if down {
            return W12AT_SWALLOW;
        }
        s.alt_vk = 0;
        return W12AT_PASS;
    }
    // Tombol naik dan modifier diteruskan agar status keyboard Windows tetap benar.
    let modifier = matches!(vk, 0x10 | 0x11 | 0x5B | 0x5C | 0xA0 | 0xA1 | 0xA2 | 0xA3);
    if !down || modifier {
        return W12AT_PASS;
    }
    match vk {
        0x09 => {
            if shift_held {
                W12AT_STEP_BACK
            } else {
                W12AT_STEP_FWD
            }
        }
        0x25 => W12AT_STEP_BACK,
        0x27 => W12AT_STEP_FWD,
        0x26 => W12AT_UP,
        0x28 => W12AT_DOWN,
        0x0D | 0x20 => {
            s.state = 0;
            W12AT_COMMIT
        }
        0x1B => {
            s.state = 0;
            W12AT_CANCEL
        }
        _ => W12AT_SWALLOW,
    }
}

/// Tombol Win ditahan (`W12WinTaskKey`). `vk`: Tab 0x09, ← 0x25, → 0x27, F4 0x73, 'D' 0x44.
pub fn w12_win_task_key(vk: u32, ctrl: bool, shift: bool, alt: bool) -> i32 {
    if vk == 0x09 && !ctrl && !alt {
        return W12WK_TASKVIEW;
    }
    if !ctrl || alt {
        return W12WK_NONE;
    }
    if !shift {
        return match vk {
            0x25 => W12WK_DESK_PREV,
            0x27 => W12WK_DESK_NEXT,
            0x44 => W12WK_DESK_NEW, // 'D'
            0x73 => W12WK_DESK_CLOSE,
            _ => W12WK_NONE,
        };
    }
    match vk {
        0x25 => W12WK_MOVE_PREV,
        0x27 => W12WK_MOVE_NEXT,
        _ => W12WK_NONE,
    }
}

/// Pindah desktop tanpa putar balik: dijepit ke [0, count-1].
pub fn w12_vd_step(cur: i32, count: i32, dir: i32) -> i32 {
    if count <= 0 {
        return 0;
    }
    cur.saturating_add(dir).clamp(0, count - 1)
}

/// Tujuan jendela desktop `n` saat desktop itu ditutup: `n > 0 ? n - 1 : 1`. Tidak bisa ditutup bila `count < 2` (-1).
pub fn w12_vd_close_dest(n: i32, count: i32) -> i32 {
    if count < 2 {
        return -1;
    }
    if n > 0 {
        n - 1
    } else {
        1
    }
}

/// Indeks desktop aktif setelah desktop `n` dihapus.
pub fn w12_vd_index_after_close(idx: i32, n: i32) -> i32 {
    if idx > n {
        return idx - 1;
    }
    if idx == n {
        return if n > 0 { n - 1 } else { 0 };
    }
    idx
}

/// Ukuran dan posisi wallpaper di monitor. `style`: 0 tengah (ukuran asli), 2 rentang, 6 muat, 10 isi, lainnya isi.
/// Penjaga: `img_w`/`img_h` < 1 dianggap 1 (mencegah pembagian nol). Keluaran: `dw`/`dh` ukuran, `dx`/`dy` posisi.
pub fn w12_wall_rect(
    style: i32,
    img_w: i32,
    img_h: i32,
    mon_w: i32,
    mon_h: i32,
    dw: &mut i32,
    dh: &mut i32,
    dx: &mut i32,
    dy: &mut i32,
) {
    let img_w = img_w.max(1);
    let img_h = img_h.max(1);
    let mut w = img_w;
    let mut h = img_h;
    if style == 2 {
        w = mon_w;
        h = mon_h;
    } else if style != 0 {
        let sx = f64::from(mon_w) / f64::from(img_w);
        let sy = f64::from(mon_h) / f64::from(img_h);
        let sc = if style == 6 {
            if sx < sy { sx } else { sy }
        } else if sx > sy {
            sx
        } else {
            sy
        };
        // `as i32` menjenuhkan nilai di luar rentang (C: perilaku tidak terdefinisi).
        w = (f64::from(img_w) * sc + 0.5) as i32;
        h = (f64::from(img_h) * sc + 0.5) as i32;
    }
    w = w.max(1);
    h = h.max(1);
    *dw = w;
    *dh = h;
    *dx = (mon_w - w) / 2;
    *dy = (mon_h - h) / 2;
}

/// Parameter tata letak kartu (`W12FlowParams`). Field `maxW`, `maxH`, `minTh` menjadi `max_w`, `max_h`, `min_th`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct W12FlowParams {
    pub max_w: f32,
    pub max_h: f32,
    pub th0: f32,
    pub header: f32,
    pub pad: f32,
    pub gap: f32,
    pub min_th: f32,
    pub shrink: f32,
}

/// Jumlah kartu maksimal (`W12FLOW_MAX`). Tipe `usize` agar bisa dipakai sebagai panjang array.
pub const W12FLOW_MAX: usize = 64;

/// Kartu mengalir per baris, rata tengah (`W12FlowLayout`). Tinggi thumbnail dikali `shrink` sampai tinggi total
/// <= `max_h` atau tinggi thumbnail < `min_th`. Koordinat relatif (0,0). `n` dipotong ke `W12FLOW_MAX` dan panjang
/// slice terpendek (C tidak memeriksa batas). `out_w`/`out_h` = 0 bila tidak ada kartu.
pub fn w12_flow_layout(
    aspect: &[f32],
    n: i32,
    p: &W12FlowParams,
    card: &mut [W12RectF],
    thumb: &mut [W12RectF],
    out_w: &mut f32,
    out_h: &mut f32,
) {
    let n = (n.clamp(0, W12FLOW_MAX as i32) as usize)
        .min(aspect.len())
        .min(card.len())
        .min(thumb.len());
    let max_w = p.max_w;
    let max_h = p.max_h;
    let mut th = p.th0;
    let mut row_w = [0.0f32; W12FLOW_MAX + 1];
    let mut row_start = [0usize; W12FLOW_MAX + 2];
    let nr = loop {
        let mut nr = 0usize;
        let mut x = 0.0f32;
        for i in 0..n {
            let mut cw = th * aspect[i] + 2.0 * p.pad;
            if cw > max_w {
                cw = max_w;
            }
            if i == 0 || x + p.gap + cw > max_w {
                row_start[nr] = i;
                row_w[nr] = cw;
                nr += 1;
                x = cw;
            } else {
                x += p.gap + cw;
                row_w[nr - 1] = x;
            }
        }
        let gaps = if nr > 0 { (nr - 1) as f32 } else { 0.0 };
        let h = nr as f32 * (p.header + th + 2.0 * p.pad) + gaps * p.gap;
        // Penjaga hang (ruling): th <= 0 atau shrink >= 1 tidak pernah mengecil. Pembanding `!(x > 0)` dan
        // `!(x < 1)` juga berhenti pada NaN, yang di C membuat loop berputar tanpa henti.
        if h <= max_h || th < p.min_th || !(th > 0.0) || !(p.shrink < 1.0) {
            break nr;
        }
        th *= p.shrink;
    };
    row_start[nr] = n;
    let mut w = 0.0f32;
    for &rw in &row_w[..nr] {
        if rw > w {
            w = rw;
        }
    }
    let row_h = p.header + th + 2.0 * p.pad;
    for r in 0..nr {
        let mut x = (w - row_w[r]) / 2.0;
        let y = r as f32 * (row_h + p.gap);
        for i in row_start[r]..row_start[r + 1] {
            let mut cw = th * aspect[i] + 2.0 * p.pad;
            if cw > max_w {
                cw = max_w;
            }
            card[i] = W12RectF {
                l: x,
                t: y,
                r: x + cw,
                b: y + row_h,
            };
            thumb[i] = W12RectF {
                l: x + p.pad,
                t: y + p.header,
                r: x + cw - p.pad,
                b: y + p.header + th,
            };
            x += cw + p.gap;
        }
    }
    *out_w = w;
    *out_h = if nr > 0 {
        nr as f32 * row_h + (nr - 1) as f32 * p.gap
    } else {
        0.0
    };
}

// Fase 5 Task 3: Win+panah men-snap jendela. Logika murni; status per jendela dikelola di `w12snap.inc`.
pub const W12SN_RESTORED: i32 = 0;
pub const W12SN_LEFT: i32 = 1;
pub const W12SN_RIGHT: i32 = 2;
pub const W12SN_MAX: i32 = 3;
pub const W12SN_MIN: i32 = 4;
/// Kuadran (2026-11-19, seperti Windows 11): kiri atas, kiri bawah, kanan atas, kanan bawah.
pub const W12SN_TL: i32 = 5;
pub const W12SN_BL: i32 = 6;
pub const W12SN_TR: i32 = 7;
pub const W12SN_BR: i32 = 8;

/// Fraksi area kerja (kiri, atas, kanan, bawah) untuk keadaan snap; None bila bukan keadaan ber-area (RESTORED/MAX/MIN).
pub fn w12_snap_frac(state: i32) -> Option<[f32; 4]> {
    Some(match state {
        W12SN_LEFT => [0.0, 0.0, 0.5, 1.0],
        W12SN_RIGHT => [0.5, 0.0, 1.0, 1.0],
        W12SN_TL => [0.0, 0.0, 0.5, 0.5],
        W12SN_BL => [0.0, 0.5, 0.5, 1.0],
        W12SN_TR => [0.5, 0.0, 1.0, 0.5],
        W12SN_BR => [0.5, 0.5, 1.0, 1.0],
        _ => return None,
    })
}

pub const W12SK_LEFT: i32 = 0;
pub const W12SK_RIGHT: i32 = 1;
pub const W12SK_UP: i32 = 2;
pub const W12SK_DOWN: i32 = 3;

/// Persegi integer (`W12RectI`): kiri, atas, kanan, bawah.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12RectI {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

/// Keadaan berikutnya (`W12SnapNext`). Baris: keadaan (RESTORED, LEFT, RIGHT, MAX, MIN); kolom: tombol (←, →, ↑, ↓).
/// MIN memakai baris RESTORED (tidak ada di tabel plan; ruling 5l). Nilai di luar rentang: RESTORED.
pub fn w12_snap_next(state: i32, key: i32) -> i32 {
    // 2026-11-19: dari setengah kiri/kanan, ↑/↓ menjadi kuadran (seperti Windows 11); dari kuadran ←/→ pindah sisi.
    const NEXT: [[i32; 4]; 9] = [
        [W12SN_LEFT, W12SN_RIGHT, W12SN_MAX, W12SN_MIN],      // RESTORED
        [W12SN_LEFT, W12SN_RESTORED, W12SN_TL, W12SN_BL],     // LEFT
        [W12SN_RESTORED, W12SN_RIGHT, W12SN_TR, W12SN_BR],    // RIGHT
        [W12SN_LEFT, W12SN_RIGHT, W12SN_MAX, W12SN_RESTORED], // MAX
        [W12SN_LEFT, W12SN_RIGHT, W12SN_MAX, W12SN_MIN],      // MIN (sama dengan RESTORED)
        [W12SN_TL, W12SN_TR, W12SN_MAX, W12SN_LEFT],          // TL
        [W12SN_BL, W12SN_BR, W12SN_LEFT, W12SN_MIN],          // BL
        [W12SN_TL, W12SN_TR, W12SN_MAX, W12SN_RIGHT],         // TR
        [W12SN_BL, W12SN_BR, W12SN_RIGHT, W12SN_MIN],         // BR
    ];
    if !(W12SN_RESTORED..=W12SN_BR).contains(&state) || !(W12SK_LEFT..=W12SK_DOWN).contains(&key) {
        return W12SN_RESTORED;
    }
    NEXT[state as usize][key as usize]
}

/// Rect target dalam koordinat `GetWindowRect` (`W12SnapRect`). LEFT/RIGHT: separuh kiri/kanan area kerja
/// (lebar ganjil: sisa 1 px ke kanan). Keadaan lain: area kerja penuh. `frame_inset` = bingkai tak terlihat
/// (`GetWindowRect` dikurangi `DWMWA_EXTENDED_FRAME_BOUNDS`).
pub fn w12_snap_rect(work: W12RectI, state: i32, frame_inset: W12RectI) -> W12RectI {
    let half = (work.r - work.l) / 2;
    let half_h = (work.b - work.t) / 2;
    let mut rc = work;
    if state == W12SN_LEFT || state == W12SN_TL || state == W12SN_BL {
        rc.r = work.l + half;
    } else if state == W12SN_RIGHT || state == W12SN_TR || state == W12SN_BR {
        rc.l = work.l + half;
    }
    if state == W12SN_TL || state == W12SN_TR {
        rc.b = work.t + half_h;
    } else if state == W12SN_BL || state == W12SN_BR {
        rc.t = work.t + half_h;
    }
    rc.l -= frame_inset.l;
    rc.t -= frame_inset.t;
    rc.r += frame_inset.r;
    rc.b += frame_inset.b;
    rc
}

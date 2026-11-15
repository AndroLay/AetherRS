//! Tes dari `w12tasks_logic_test.cpp` (baris 1–155): Alt+Tab, tombol Win, desktop virtual, wallpaper, tata letak alur, snap, dan parser `w12vdfile.h` (TestVdFile dari berkas yang sama). Status: ditulis; hasil uji ada di laporan.
//! Dijalankan dengan `cargo test --test tasks_logic`.
use w12::tasks_logic::{
    w12_alt_tab_key, w12_flow_layout, w12_snap_next, w12_snap_rect, w12_vd_close_dest,
    w12_vd_index_after_close, w12_vd_step, w12_wall_rect, w12_win_task_key, W12AltTab, W12FlowParams,
    W12RectF, W12RectI, W12AT_CANCEL, W12AT_COMMIT, W12AT_PASS, W12AT_STEP_BACK, W12AT_STEP_FWD,
    W12AT_SWALLOW, W12FLOW_MAX, W12SK_DOWN, W12SK_LEFT, W12SK_RIGHT, W12SK_UP, W12SN_LEFT, W12SN_MAX,
    W12SN_MIN, W12SN_RESTORED, W12SN_RIGHT, W12WK_DESK_CLOSE, W12WK_DESK_NEW, W12WK_DESK_NEXT,
    W12WK_DESK_PREV, W12WK_MOVE_NEXT, W12WK_MOVE_PREV, W12WK_NONE, W12WK_TASKVIEW,
};
use w12::vdfile::w12_vd_parse_next;

#[test]
fn alt_tab() {
    let mut s = W12AltTab::default();
    assert_eq!(w12_alt_tab_key(&mut s, 0xA4, true, true, false, false), W12AT_PASS);
    assert_eq!(s.alt_vk, 0xA4);
    assert_eq!(w12_alt_tab_key(&mut s, 0x09, true, true, false, false), W12AT_STEP_FWD);
    assert_eq!(s.state, 1);
    assert_eq!(w12_alt_tab_key(&mut s, 0x09, false, true, false, false), W12AT_SWALLOW);
    assert_eq!(w12_alt_tab_key(&mut s, 0x09, true, true, true, false), W12AT_STEP_BACK);
    assert_eq!(w12_alt_tab_key(&mut s, 0x27, true, true, false, false), W12AT_STEP_FWD);
    assert_eq!(w12_alt_tab_key(&mut s, 0x25, true, true, false, false), W12AT_STEP_BACK);
    assert_eq!(w12_alt_tab_key(&mut s, 0x1B, true, true, false, false), W12AT_CANCEL);
    assert_eq!(s.state, 2);
    assert_eq!(w12_alt_tab_key(&mut s, 0x41, true, true, false, false), W12AT_SWALLOW);
    assert_eq!(w12_alt_tab_key(&mut s, 0x09, true, true, false, false), W12AT_STEP_FWD);
    assert_eq!(s.state, 1);
    assert_eq!(w12_alt_tab_key(&mut s, 0x1B, true, true, false, false), W12AT_CANCEL);
    assert_eq!(s.state, 2);
    assert_eq!(w12_alt_tab_key(&mut s, 0xA4, false, false, false, false), W12AT_COMMIT);
    assert_eq!(s.state, 0);
    assert_eq!(w12_alt_tab_key(&mut s, 0x09, true, true, false, true), W12AT_STEP_FWD);
    assert_eq!(w12_alt_tab_key(&mut s, 0xA4, false, false, false, false), W12AT_PASS);
    let mut t = W12AltTab { state: 1, alt_vk: 0xA4 };
    assert_eq!(w12_alt_tab_key(&mut t, 0x41, true, false, false, false), W12AT_COMMIT);
    assert_eq!(t.state, 0);
    let mut u = W12AltTab::default();
    assert_eq!(w12_alt_tab_key(&mut u, 0x12, true, true, false, false), W12AT_PASS);
    assert_eq!(u.alt_vk, 0xA4);
}

#[test]
fn win_task_key() {
    assert_eq!(w12_win_task_key(0x09, false, false, false), W12WK_TASKVIEW);
    assert_eq!(w12_win_task_key(0x09, true, false, false), W12WK_NONE);
    assert_eq!(w12_win_task_key(0x27, true, false, false), W12WK_DESK_NEXT);
    assert_eq!(w12_win_task_key(0x25, true, false, false), W12WK_DESK_PREV);
    assert_eq!(w12_win_task_key(0x44, true, false, false), W12WK_DESK_NEW); // 'D'
    assert_eq!(w12_win_task_key(0x73, true, false, false), W12WK_DESK_CLOSE);
    assert_eq!(w12_win_task_key(0x25, true, true, false), W12WK_MOVE_PREV);
    assert_eq!(w12_win_task_key(0x27, true, true, false), W12WK_MOVE_NEXT);
    assert_eq!(w12_win_task_key(0x44, false, false, false), W12WK_NONE); // 'D'
}

#[test]
fn desktops() {
    assert_eq!(w12_vd_step(0, 3, -1), 0);
    assert_eq!(w12_vd_step(2, 3, 1), 2);
    assert_eq!(w12_vd_step(1, 3, 1), 2);
    assert_eq!(w12_vd_close_dest(0, 3), 1);
    assert_eq!(w12_vd_close_dest(2, 3), 1);
    assert_eq!(w12_vd_close_dest(0, 1), -1);
    assert_eq!(w12_vd_index_after_close(3, 1), 2);
    assert_eq!(w12_vd_index_after_close(0, 1), 0);
    assert_eq!(w12_vd_index_after_close(1, 1), 0);
}

#[test]
fn wall_rect() {
    let (mut dw, mut dh, mut dx, mut dy) = (0i32, 0i32, 0i32, 0i32);
    w12_wall_rect(10, 1920, 1080, 1920, 1200, &mut dw, &mut dh, &mut dx, &mut dy);
    assert_eq!((dw, dh, dx, dy), (2133, 1200, -106, 0));
    w12_wall_rect(6, 1920, 1080, 1920, 1200, &mut dw, &mut dh, &mut dx, &mut dy);
    assert_eq!((dw, dh, dx, dy), (1920, 1080, 0, 60));
    w12_wall_rect(2, 800, 600, 1920, 1200, &mut dw, &mut dh, &mut dx, &mut dy);
    assert_eq!((dw, dh, dx, dy), (1920, 1200, 0, 0));
    w12_wall_rect(0, 800, 600, 1920, 1200, &mut dw, &mut dh, &mut dx, &mut dy);
    assert_eq!((dw, dh, dx, dy), (800, 600, 560, 300));
    w12_wall_rect(10, 7680, 4320, 1920, 1080, &mut dw, &mut dh, &mut dx, &mut dy);
    assert_eq!((dw, dh, dx, dy), (1920, 1080, 0, 0));
}

#[test]
fn flow_layout() {
    let p = W12FlowParams {
        max_w: 1000.0,
        max_h: 1000.0,
        th0: 150.0,
        header: 34.0,
        pad: 6.0,
        gap: 14.0,
        min_th: 56.0,
        shrink: 0.88,
    };
    let a1 = [1.6f32];
    let mut card = [W12RectF::default(); W12FLOW_MAX];
    let mut th = [W12RectF::default(); W12FLOW_MAX];
    let mut w = 0.0f32;
    let mut h = 0.0f32;
    w12_flow_layout(&a1, 1, &p, &mut card, &mut th, &mut w, &mut h);
    assert_eq!((w, h), (252.0, 196.0));
    assert_eq!((th[0].l, th[0].t, th[0].r, th[0].b), (6.0, 34.0, 246.0, 184.0));
    w12_flow_layout(&a1, 0, &p, &mut card, &mut th, &mut w, &mut h);
    assert_eq!((w, h), (0.0, 0.0));

    let a40 = [1.6f32; 40];
    let q = W12FlowParams {
        max_w: 1200.0,
        max_h: 600.0,
        th0: 150.0,
        header: 34.0,
        pad: 6.0,
        gap: 14.0,
        min_th: 56.0,
        shrink: 0.88,
    };
    w12_flow_layout(&a40, 40, &q, &mut card, &mut th, &mut w, &mut h);
    assert!(w <= 1200.01 && h <= 600.01);
    for c in card.iter().take(40) {
        assert!(c.r <= w + 0.01);
    }
}

#[test]
fn vd_file() {
    // Berkas desktop virtual: "hwnd_hex pid" per baris, CRLF.
    let f: &[u8] = b"1a2b 345\r\nff 7\r\n";
    let mut cur: &[u8] = f;
    let mut hw: u64 = 0;
    let mut pid: u32 = 0;
    assert!(w12_vd_parse_next(&mut cur, &mut hw, &mut pid));
    assert!(hw == 0x1a2b && pid == 345);
    assert!(w12_vd_parse_next(&mut cur, &mut hw, &mut pid));
    assert!(hw == 0xff && pid == 7);
    assert!(!w12_vd_parse_next(&mut cur, &mut hw, &mut pid));
    let mut bad: &[u8] = b"zz\r\n";
    assert!(!w12_vd_parse_next(&mut bad, &mut hw, &mut pid));
    let mut b2: &[u8] = b"12\r\n";
    assert!(!w12_vd_parse_next(&mut b2, &mut hw, &mut pid));
    let mut last: &[u8] = b"2f 9";
    assert!(w12_vd_parse_next(&mut last, &mut hw, &mut pid));
    assert!(hw == 0x2f && pid == 9);
}

// Fase 5 Task 3: Win+panah men-snap jendela. Baris = keadaan, kolom = ← → ↑ ↓ (MIN memakai baris RESTORED; ruling 5l).
#[test]
fn snap_next() {
    let kasus = [
        (W12SN_RESTORED, W12SK_LEFT, W12SN_LEFT),
        (W12SN_RESTORED, W12SK_RIGHT, W12SN_RIGHT),
        (W12SN_RESTORED, W12SK_UP, W12SN_MAX),
        (W12SN_RESTORED, W12SK_DOWN, W12SN_MIN),
        (W12SN_LEFT, W12SK_LEFT, W12SN_LEFT),
        (W12SN_LEFT, W12SK_RIGHT, W12SN_RESTORED),
        (W12SN_LEFT, W12SK_UP, W12SN_MAX),
        (W12SN_LEFT, W12SK_DOWN, W12SN_RESTORED),
        (W12SN_RIGHT, W12SK_LEFT, W12SN_RESTORED),
        (W12SN_RIGHT, W12SK_RIGHT, W12SN_RIGHT),
        (W12SN_RIGHT, W12SK_UP, W12SN_MAX),
        (W12SN_RIGHT, W12SK_DOWN, W12SN_RESTORED),
        (W12SN_MAX, W12SK_LEFT, W12SN_LEFT),
        (W12SN_MAX, W12SK_RIGHT, W12SN_RIGHT),
        (W12SN_MAX, W12SK_UP, W12SN_MAX),
        (W12SN_MAX, W12SK_DOWN, W12SN_RESTORED),
        (W12SN_MIN, W12SK_LEFT, W12SN_LEFT),
        (W12SN_MIN, W12SK_RIGHT, W12SN_RIGHT),
        (W12SN_MIN, W12SK_UP, W12SN_MAX),
        (W12SN_MIN, W12SK_DOWN, W12SN_MIN),
    ];
    for (state, key, expect) in kasus {
        assert_eq!(w12_snap_next(state, key), expect, "keadaan {state}, tombol {key}");
    }
}

#[test]
fn snap_rect() {
    let work = W12RectI { l: 0, t: 0, r: 1921, b: 1150 }; // lebar ganjil: sisa 1 px ke kanan
    let inset = W12RectI { l: 7, t: 0, r: 7, b: 7 }; // bingkai tak terlihat: kiri, atas, kanan, bawah
    assert_eq!(
        w12_snap_rect(work, W12SN_LEFT, inset),
        W12RectI { l: -7, t: 0, r: 960 + 7, b: 1150 + 7 }
    );
    assert_eq!(
        w12_snap_rect(work, W12SN_RIGHT, inset),
        W12RectI { l: 960 - 7, t: 0, r: 1921 + 7, b: 1150 + 7 }
    );
    let even = W12RectI { l: 0, t: 0, r: 1920, b: 1080 };
    let none = W12RectI { l: 0, t: 0, r: 0, b: 0 };
    assert_eq!(w12_snap_rect(even, W12SN_LEFT, none), W12RectI { l: 0, t: 0, r: 960, b: 1080 });
    assert_eq!(w12_snap_rect(even, W12SN_RIGHT, none), W12RectI { l: 960, t: 0, r: 1920, b: 1080 });
}

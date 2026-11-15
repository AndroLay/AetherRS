//! Tes dari `w12gesture_logic_test.cpp` (baris 1–55) untuk gestur touchpad dan bingkai HID PTP. Status: ditulis; hasil uji ada di laporan.
//! Dijalankan dengan `cargo test --test gesture_logic`.
use w12::gesture_logic::{
    w12_gest_feed, w12_gest_lift, w12_ptp_add, W12Gest, W12PtpContact, W12PtpFrame, W12G_DESK_NEXT,
    W12G_DESK_PREV, W12G_NONE, W12G_SW_COMMIT, W12G_SW_STEP_BACK, W12G_SW_STEP_FWD, W12G_UP,
    W12_GEST_DEFAULT,
};

#[test]
fn gestur_sesuai_sumber() {
    let c = &W12_GEST_DEFAULT;

    // Alih aplikasi: tiga jari geser ke kanan, lalu ke kiri, lalu semua jari diangkat (commit).
    let mut g = W12Gest::default();
    assert_eq!(w12_gest_feed(&mut g, c, 3, 0.50, 0.50), W12G_NONE);
    assert_eq!(w12_gest_feed(&mut g, c, 3, 0.57, 0.51), W12G_SW_STEP_FWD);
    assert_eq!(g.mode, 1);
    assert_eq!(w12_gest_feed(&mut g, c, 3, 0.70, 0.51), W12G_NONE);
    assert_eq!(w12_gest_feed(&mut g, c, 3, 0.72, 0.52), W12G_SW_STEP_FWD);
    assert_eq!(w12_gest_feed(&mut g, c, 3, 0.56, 0.52), W12G_SW_STEP_BACK);
    assert_eq!(w12_gest_feed(&mut g, c, 0, 0.0, 0.0), W12G_SW_COMMIT);
    assert!(g.mode == 0 && g.fingers == 0);

    // Pindah desktop: empat jari, sekali per gestur.
    let mut d = W12Gest::default();
    assert_eq!(w12_gest_feed(&mut d, c, 4, 0.5, 0.5), W12G_NONE);
    assert_eq!(w12_gest_feed(&mut d, c, 4, 0.40, 0.5), W12G_DESK_NEXT);
    assert_eq!(w12_gest_feed(&mut d, c, 4, 0.20, 0.5), W12G_NONE); // sekali per gestur
    assert_eq!(w12_gest_lift(&mut d), W12G_NONE);

    // Vertikal ke atas dengan tiga jari.
    let mut v = W12Gest::default();
    assert_eq!(w12_gest_feed(&mut v, c, 3, 0.5, 0.5), W12G_NONE);
    assert_eq!(w12_gest_feed(&mut v, c, 3, 0.52, 0.38), W12G_UP);

    // Diagonal: belum ada sumbu dominan.
    let mut diag = W12Gest::default();
    assert_eq!(w12_gest_feed(&mut diag, c, 3, 0.5, 0.5), W12G_NONE);
    assert_eq!(w12_gest_feed(&mut diag, c, 3, 0.57, 0.56), W12G_NONE);

    // Scroll dua jari tidak menghasilkan gestur.
    let mut two = W12Gest::default();
    assert_eq!(w12_gest_feed(&mut two, c, 2, 0.5, 0.5), W12G_NONE);
    assert_eq!(w12_gest_feed(&mut two, c, 2, 0.9, 0.5), W12G_NONE);

    // Tiga ke empat jari sebelum mode: mulai ulang sebagai empat jari.
    let mut grow = W12Gest::default();
    assert_eq!(w12_gest_feed(&mut grow, c, 3, 0.5, 0.5), W12G_NONE);
    assert_eq!(w12_gest_feed(&mut grow, c, 4, 0.5, 0.5), W12G_NONE);
    assert_eq!(grow.fingers, 4);
    assert_eq!(w12_gest_feed(&mut grow, c, 4, 0.62, 0.5), W12G_DESK_PREV);

    // Bingkai hybrid: empat kontak dalam dua laporan, satu telapak tidak yakin.
    let mut f = W12PtpFrame::default();
    let a = [
        W12PtpContact { x: 100, y: 200, tip: true, confident: true },
        W12PtpContact { x: 300, y: 200, tip: true, confident: true },
    ];
    let b = [
        W12PtpContact { x: 500, y: 200, tip: true, confident: true },
        W12PtpContact { x: 700, y: 900, tip: true, confident: false },
    ];
    assert_eq!(w12_ptp_add(&mut f, 4, &a, 2), 0);
    assert_eq!(w12_ptp_add(&mut f, 0, &b, 2), 1);
    assert!(f.fingers == 3 && f.sx == 900 && f.sy == 600);
    assert_eq!(w12_ptp_add(&mut f, 0, &b, 2), 0); // laporan yatim diabaikan
}

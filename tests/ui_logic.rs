//! Tes dari `w12ui_logic_test.cpp` (baris 1–141) untuk helper UI murni: warna, pintasan Win/Start, pil, baterai, teks USB. Status: ditulis; hasil uji ada di laporan.
//! Dijalankan dengan `cargo test --test ui_logic`.
use w12::ui_logic::{
    w12_average_rgb, w12_battery_alert, w12_can_capture_win_key, w12_can_trim_native_start_working_set,
    w12_decode_windows_accent, w12_should_show_pill, w12_start_context_from_hit, w12_start_shortcut_action,
    w12_tint_surface, w12_usb_eject_fail_text, W12Rgb8, W12StartContextRef, W12_CTX_APP, W12_CTX_NONE,
    W12_CTX_SEARCH_RESULT, W12_STARTKEY_NONE, W12_STARTKEY_SEARCH, W12_STARTKEY_TOGGLE,
};

/// Mengubah buffer UTF-16 ber-NUL menjadi `String` (sampai NUL pertama atau akhir buffer).
fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[test]
fn windows_accent_urutan_byte() {
    // DWORD 0xAABBGGRR: 0xFFD47800 menjadi r=0, g=120, b=212.
    assert_eq!(w12_decode_windows_accent(0xFFD4_7800), W12Rgb8 { r: 0, g: 120, b: 212 });
}

#[test]
fn tint_wallpaper_menjaga_kecerahan_tema() {
    let dark = w12_tint_surface(W12Rgb8 { r: 18, g: 22, b: 50 }, W12Rgb8 { r: 152, g: 184, b: 208 }, 72);
    assert_eq!(dark, W12Rgb8 { r: 56, g: 68, b: 95 });
    assert!(dark.r < 100 && dark.g < 100 && dark.b < 110);

    let light = w12_tint_surface(W12Rgb8 { r: 252, g: 251, b: 255 }, W12Rgb8 { r: 152, g: 184, b: 208 }, 32);
    assert_eq!(light, W12Rgb8 { r: 239, g: 243, b: 249 });
    assert!(light.r > 220 && light.g > 220 && light.b > 220);

    let untouched = w12_tint_surface(W12Rgb8 { r: 18, g: 22, b: 50 }, W12Rgb8 { r: 152, g: 184, b: 208 }, 0);
    assert_eq!(untouched, W12Rgb8 { r: 18, g: 22, b: 50 });
}

#[test]
fn rata_rata_wallpaper() {
    assert_eq!(w12_average_rgb(300, 600, 750, 3), W12Rgb8 { r: 100, g: 200, b: 250 });
}

#[test]
fn tombol_win_termasuk_layar_penuh() {
    assert!(w12_can_capture_win_key(true, true, true, false, true));
    assert!(w12_can_capture_win_key(true, true, false, false, true));
    assert!(!w12_can_capture_win_key(false, true, true, false, true));
    assert!(!w12_can_capture_win_key(true, false, true, false, true));
    assert!(!w12_can_capture_win_key(true, true, true, true, true));
    assert!(!w12_can_capture_win_key(true, true, true, false, false));
}

#[test]
fn pintasan_start_diarahkan_ke_win12() {
    assert_eq!(w12_start_shortcut_action(true, false, false, true, false, true), W12_STARTKEY_TOGGLE);
    assert_eq!(w12_start_shortcut_action(false, true, false, true, false, true), W12_STARTKEY_SEARCH);
    assert_eq!(w12_start_shortcut_action(false, false, true, true, false, true), W12_STARTKEY_TOGGLE);
    assert_eq!(w12_start_shortcut_action(false, false, false, true, false, true), W12_STARTKEY_NONE);
    assert_eq!(w12_start_shortcut_action(false, true, false, false, false, true), W12_STARTKEY_NONE);
    assert_eq!(w12_start_shortcut_action(false, false, true, true, true, true), W12_STARTKEY_NONE);
    assert_eq!(w12_start_shortcut_action(true, false, false, true, false, false), W12_STARTKEY_NONE);
}

#[test]
fn trim_start_native_hanya_saat_win12_memiliki_pintasan() {
    assert!(!w12_can_trim_native_start_working_set(false, true, false, true));
    assert!(!w12_can_trim_native_start_working_set(true, false, false, true));
    assert!(!w12_can_trim_native_start_working_set(true, true, true, true));
    assert!(!w12_can_trim_native_start_working_set(true, true, false, false));
    assert!(w12_can_trim_native_start_working_set(true, true, false, true));
}

#[test]
fn pil_layar_penuh_hanya_saat_peek_start() {
    assert!(w12_should_show_pill(true, false, false, true));
    assert!(!w12_should_show_pill(true, true, false, true));
    assert!(w12_should_show_pill(true, true, true, true));
    assert!(!w12_should_show_pill(true, true, true, false));
    assert!(!w12_should_show_pill(false, true, true, true));
}

#[test]
fn konteks_menunjuk_item_start_yang_ada() {
    assert_eq!(
        w12_start_context_from_hit(1003, 1000, 2000, 3000, 8, 4),
        W12StartContextRef { kind: W12_CTX_APP, index: 3 }
    );
    assert_eq!(
        w12_start_context_from_hit(2005, 1000, 2000, 3000, 8, 4),
        W12StartContextRef { kind: W12_CTX_APP, index: 5 }
    );
    assert_eq!(
        w12_start_context_from_hit(3002, 1000, 2000, 3000, 8, 4),
        W12StartContextRef { kind: W12_CTX_SEARCH_RESULT, index: 2 }
    );
    assert_eq!(
        w12_start_context_from_hit(3004, 1000, 2000, 3000, 8, 4),
        W12StartContextRef { kind: W12_CTX_NONE, index: -1 }
    );
    assert_eq!(
        w12_start_context_from_hit(999, 1000, 2000, 3000, 8, 4),
        W12StartContextRef { kind: W12_CTX_NONE, index: -1 }
    );
}

#[test]
fn peringatan_baterai_sekali_per_ambang() {
    let mut armed = 100;
    assert_eq!(w12_battery_alert(25, false, Some(&mut armed)), 0);
    assert_eq!(w12_battery_alert(20, false, Some(&mut armed)), 20);
    assert_eq!(armed, 20);
    assert_eq!(w12_battery_alert(19, false, Some(&mut armed)), 0);
    assert_eq!(w12_battery_alert(10, false, Some(&mut armed)), 10);
    assert_eq!(armed, 10);
    assert_eq!(w12_battery_alert(4, false, Some(&mut armed)), 5);
    assert_eq!(armed, 5);
    assert_eq!(w12_battery_alert(4, true, Some(&mut armed)), 0);
    assert_eq!(armed, 100);
    assert_eq!(w12_battery_alert(21, false, Some(&mut armed)), 0);
    assert_eq!(w12_battery_alert(20, false, Some(&mut armed)), 20);
    // pulih sampai armed + 5 (tanpa mengisi daya) mengatur ulang dulu, lalu pembacaan yang sama dievaluasi
    // ulang: 10 sudah di ambang dan belum diperingatkan setelah reset, jadi peringatan 10 tampil lagi
    armed = 5;
    assert_eq!(w12_battery_alert(10, false, Some(&mut armed)), 10);
    assert_eq!(armed, 10);
    // pembacaan tidak valid (-1) tidak mengubah apa pun
    armed = 100;
    assert_eq!(w12_battery_alert(-1, false, Some(&mut armed)), 0);
    assert_eq!(armed, 100);
}

#[test]
fn teks_gagal_usb_menyebut_pengambil_veto() {
    let mut t = [0u16; 128];
    w12_usb_eject_fail_text(true, 0x17, Some("Explorer"), &mut t);
    assert_eq!(wide_to_string(&t), "Perangkat sedang dipakai oleh Explorer");
    w12_usb_eject_fail_text(true, 0x17, Some(""), &mut t);
    assert_eq!(wide_to_string(&t), "Perangkat sedang dipakai");
    w12_usb_eject_fail_text(false, 31, None, &mut t);
    assert_eq!(wide_to_string(&t), "Gagal mengeluarkan (CM 31)");
}

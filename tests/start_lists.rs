//! Tes dari `tests/w12start_lists_test.cpp` (baris 1–92) untuk `w12::start_lists`. Status: ditulis, belum dijalankan.
//! Hanya fungsi murni: tidak menyentuh registry, file, atau proses. Perintah: `cargo test --test start_lists`.
use w12::start_lists::{
    w12_start_build_pin_indices, w12_start_query_clear_selection, w12_start_query_insert_char,
    w12_start_query_select_all, w12_start_visible_items, AppLike, W12StartVisibleRange,
};

// Padanan `struct TestApp { int spin, pin; };`.
#[derive(Clone, Copy)]
struct TestApp {
    spin: i32,
    pin: i32,
}

impl AppLike for TestApp {
    fn spin(&self) -> i32 {
        self.spin
    }

    fn pin(&self) -> i32 {
        self.pin
    }
}

// Padanan literal `L"..."` ke buffer tetap; sisa elemen bernilai nol.
fn wide<const N: usize>(s: &str) -> [u16; N] {
    let mut buf = [0u16; N];
    for (dst, src) in buf.iter_mut().zip(s.encode_utf16()) {
        *dst = src;
    }
    buf
}

// Isi buffer sampai NUL pertama, seperti `wcscmp` pada C++.
fn cstr(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn range(first: i32, last: i32) -> W12StartVisibleRange {
    W12StartVisibleRange { first, last }
}

// Padanan `TestExplicitOrderKeepsDuplicateRanksStable`.
#[test]
fn explicit_order_keeps_duplicate_ranks_stable() {
    let apps = [
        TestApp { spin: 3, pin: -1 },
        TestApp { spin: 1, pin: 0 },
        TestApp { spin: 2, pin: 1 },
        TestApp { spin: 2, pin: 0 },
        TestApp { spin: 0, pin: 1 },
        TestApp { spin: 5, pin: -1 },
        TestApp { spin: 0, pin: -1 },
        TestApp { spin: 0, pin: -1 },
    ];
    let mut out = [0i32; 3];
    let n = w12_start_build_pin_indices(&apps, 8, 3, &mut out);
    assert_eq!(n, 3);
    assert_eq!(out, [1, 2, 3]);
}

// Padanan `TestSparseOrdersAndLimit`.
#[test]
fn sparse_orders_and_limit() {
    let apps = [
        TestApp { spin: 10, pin: -1 },
        TestApp { spin: 2, pin: -1 },
        TestApp { spin: 5, pin: -1 },
        TestApp { spin: 0, pin: -1 },
        TestApp { spin: 0, pin: -1 },
    ];
    let mut out = [0i32; 2];
    let n = w12_start_build_pin_indices(&apps, 5, 2, &mut out);
    assert_eq!(n, 2);
    assert_eq!(out, [1, 2]);
}

// Padanan `TestDockFallbackKeepsLeftThenRight`.
#[test]
fn dock_fallback_keeps_left_then_right() {
    let apps = [
        TestApp { spin: 0, pin: 1 },
        TestApp { spin: 0, pin: 0 },
        TestApp { spin: 0, pin: 1 },
        TestApp { spin: 0, pin: 0 },
        TestApp { spin: 0, pin: -1 },
    ];
    let mut out = [0i32; 3];
    let n = w12_start_build_pin_indices(&apps, 5, 3, &mut out);
    assert_eq!(n, 3);
    assert_eq!(out, [1, 3, 0]);
}

// Padanan `TestNoPinsReturnsEmpty`.
#[test]
fn no_pins_returns_empty() {
    let apps = [TestApp { spin: 0, pin: -1 }; 3];
    let mut out = [0i32; 3];
    assert_eq!(w12_start_build_pin_indices(&apps, 3, 3, &mut out), 0);
}

// Padanan `TestSelectAllQueryTypingReplacesSelection`.
#[test]
fn select_all_query_typing_replaces_selection() {
    let mut query = wide::<8>("alpha");
    assert!(w12_start_query_select_all(5));
    assert!(!w12_start_query_select_all(0));
    let mut length = w12_start_query_insert_char(&mut query, 5, true, u16::from(b'b'));
    assert_eq!(length, 1);
    assert_eq!(cstr(&query), "b");
    length = w12_start_query_insert_char(&mut query, length, false, u16::from(b'c'));
    assert_eq!(length, 2);
    assert_eq!(cstr(&query), "bc");
    length = w12_start_query_clear_selection(&mut query, length, true);
    assert_eq!(length, 0);
    assert_eq!(query[0], 0);
}

// Padanan `TestAllAppsVisibleRangeIsVirtualized`.
#[test]
fn all_apps_visible_range_is_virtualized() {
    assert_eq!(w12_start_visible_items(100, 0, 40, 100), range(0, 3));
    assert_eq!(w12_start_visible_items(100, 120, 40, 100), range(3, 6));
    assert_eq!(w12_start_visible_items(5, 0, 40, 100), range(0, 3));
    assert_eq!(w12_start_visible_items(5, -10, 40, 100), range(0, 3));
    assert_eq!(w12_start_visible_items(5, 0, 0, 100), range(0, 0));
}

//! Tes dari `w12stophosts_test.cpp` (baris 1–103) untuk whitelist, indeks dan nama, parser `stopHosts=`, serta backoff. Status: ditulis; hasil uji ada di laporan.
//! Hanya data in-memory: tidak menyentuh file hosts asli. Dijalankan dengan `cargo test --test stophosts`.
use w12::stophosts::{
    w12_stop_allowed, w12_stop_decide, w12_stop_index, w12_stop_name, w12_stop_parse, w12_stop_reset,
    W12StopState, W12STOP_COUNT, W12STOP_DISABLE, W12STOP_SKIP, W12STOP_TERMINATE, W12STOP_WINDOW_MS,
};

/// Mengubah teks menjadi UTF-16 (padanan literal `L"..."` di C).
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[test]
fn whitelist() {
    assert!(w12_stop_allowed(&wide("SearchHost.exe")));
    assert!(w12_stop_allowed(&wide("searchhost.EXE")));
    assert!(w12_stop_allowed(&wide("StartMenuExperienceHost.exe")));
    assert!(w12_stop_allowed(&wide("ShellHost.exe")));
    assert!(w12_stop_allowed(&wide("ShellExperienceHost.exe")));
    assert!(!w12_stop_allowed(&wide("explorer.exe")));
    assert!(!w12_stop_allowed(&wide("svchost.exe")));
    assert!(!w12_stop_allowed(&wide("msedgewebview2.exe")));
    assert!(!w12_stop_allowed(&wide("TextInputHost.exe")));
    assert!(!w12_stop_allowed(&wide("dwm.exe")));
    assert!(!w12_stop_allowed(&wide("SearchHost")));
    assert!(!w12_stop_allowed(&wide("SearchHost.exe ")));
    assert!(!w12_stop_allowed(&wide("C:\\Windows\\SearchHost.exe")));
    assert!(!w12_stop_allowed(&wide("")));
    // nullptr di C dipetakan ke slice kosong, karena `&[u16]` tidak bisa null.
    assert!(!w12_stop_allowed(&[]));
}

#[test]
fn index_dan_nama() {
    assert_eq!(w12_stop_index(&wide("SearchHost.exe")), 0);
    assert_eq!(w12_stop_index(&wide("StartMenuExperienceHost.exe")), 1);
    assert_eq!(w12_stop_index(&wide("ShellHost.exe")), 2);
    assert_eq!(w12_stop_index(&wide("ShellExperienceHost.exe")), 3);
    assert_eq!(w12_stop_index(&wide("explorer.exe")), -1);
    // nullptr di C dipetakan ke slice kosong.
    assert_eq!(w12_stop_index(&[]), -1);
    assert_eq!(w12_stop_name(0), Some("SearchHost.exe"));
    assert_eq!(w12_stop_name(3), Some("ShellExperienceHost.exe"));
    assert_eq!(w12_stop_name(4), Some("CrossDeviceResume.exe"));
    assert_eq!(w12_stop_name(7), Some("SpotifyLauncher.exe"));
    assert_eq!(w12_stop_name(W12STOP_COUNT as i32), None);
    assert_eq!(w12_stop_name(-1), None);
}

#[test]
fn parse() {
    let mut out = [0i32; W12STOP_COUNT];
    let mut rej = -1;
    // nullptr di C dipetakan ke slice kosong.
    assert_eq!(w12_stop_parse(&[], &mut out, Some(&mut rej)), 0);
    assert_eq!(rej, 0);
    assert_eq!(w12_stop_parse(&wide(""), &mut out, Some(&mut rej)), 0);
    assert_eq!(w12_stop_parse(&wide("   "), &mut out, Some(&mut rej)), 0);
    assert_eq!(w12_stop_parse(&wide("SearchHost.exe"), &mut out, Some(&mut rej)), 1);
    assert!(out[0] == 0 && rej == 0);
    assert_eq!(
        w12_stop_parse(
            &wide(" searchhost.exe ,  ShellHost.exe;StartMenuExperienceHost.exe"),
            &mut out,
            Some(&mut rej)
        ),
        3
    );
    assert!(out[0] == 0 && out[1] == 2 && out[2] == 1 && rej == 0);
    assert_eq!(
        w12_stop_parse(&wide("explorer.exe,SearchHost.exe,dwm.exe"), &mut out, Some(&mut rej)),
        1
    );
    assert!(out[0] == 0 && rej == 2);
    assert_eq!(
        w12_stop_parse(&wide("SearchHost.exe,searchhost.EXE,SearchHost.exe"), &mut out, Some(&mut rej)),
        1
    );
    assert_eq!(rej, 0);
    assert_eq!(
        w12_stop_parse(&wide("ShellHost.exe,,;;ShellExperienceHost.exe"), &mut out, Some(&mut rej)),
        2
    );
    assert!(out[0] == 2 && out[1] == 3 && rej == 0);
    assert_eq!(
        w12_stop_parse(
            &wide("SearchHost.exe,StartMenuExperienceHost.exe,ShellHost.exe,ShellExperienceHost.exe"),
            &mut out,
            Some(&mut rej)
        ),
        4
    );
}

#[test]
fn backoff() {
    let mut st = W12StopState::default();
    w12_stop_reset(&mut st);
    assert_eq!(w12_stop_decide(&mut st, 0, 1000), W12STOP_TERMINATE);
    assert_eq!(w12_stop_decide(&mut st, 0, 2000), W12STOP_TERMINATE);
    assert_eq!(w12_stop_decide(&mut st, 0, 3000), W12STOP_DISABLE);
    assert_eq!(w12_stop_decide(&mut st, 0, 4000), W12STOP_SKIP);
    assert_eq!(w12_stop_decide(&mut st, 0, 5 * W12STOP_WINDOW_MS), W12STOP_SKIP);
    assert_eq!(w12_stop_decide(&mut st, 1, 3000), W12STOP_TERMINATE);
    assert_eq!(w12_stop_decide(&mut st, -1, 3000), W12STOP_SKIP);
    assert_eq!(w12_stop_decide(&mut st, W12STOP_COUNT as i32, 3000), W12STOP_SKIP);
    // Kasus `st == nullptr` di C tidak punya padanan: `&mut W12StopState` di Rust tidak bisa null.
}

#[test]
fn backoff_window() {
    let mut st = W12StopState::default();
    w12_stop_reset(&mut st);
    assert_eq!(w12_stop_decide(&mut st, 0, 0), W12STOP_TERMINATE);
    assert_eq!(w12_stop_decide(&mut st, 0, 1000), W12STOP_TERMINATE);
    // dua kemunculan pertama sudah di luar jendela 10 menit: tidak dihitung lagi
    assert_eq!(w12_stop_decide(&mut st, 0, W12STOP_WINDOW_MS + 5000), W12STOP_TERMINATE);
    assert_eq!(w12_stop_decide(&mut st, 0, W12STOP_WINDOW_MS + 6000), W12STOP_TERMINATE);
    assert_eq!(w12_stop_decide(&mut st, 0, W12STOP_WINDOW_MS + 7000), W12STOP_DISABLE);
    w12_stop_reset(&mut st);
    // tepat 10 menit setelah kemunculan pertama: sudah di luar jendela
    assert_eq!(w12_stop_decide(&mut st, 0, 0), W12STOP_TERMINATE);
    assert_eq!(w12_stop_decide(&mut st, 0, 1), W12STOP_TERMINATE);
    assert_eq!(w12_stop_decide(&mut st, 0, W12STOP_WINDOW_MS), W12STOP_TERMINATE);
}

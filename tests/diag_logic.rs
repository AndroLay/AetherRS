//! Tes dari `w12diag_logic_test.cpp` (baris 1–105) untuk parser `--diag-cycle`, nama komponen, MB persepuluh, serta format baris dan catatan. Status: ditulis; hasil uji ada di laporan.
//! Tidak memanggil `log::log`: tes hanya memakai data in-memory. Dijalankan dengan `cargo test --test diag_logic`.
use w12::diag::{
    w12_diag_comp_name, w12_diag_format_note, w12_diag_format_row, w12_diag_parse_cmd, w12_diag_parse_comp,
    w12_diag_parse_n, w12_diag_tenths_mb, W12DiagSample, W12DIAG_COUNT, W12DIAG_FLYOUT, W12DIAG_PANEL,
    W12DIAG_QS, W12DIAG_START, W12DIAG_TASKVIEW,
};

/// Mengubah teks menjadi UTF-16 (padanan literal `L"..."` di C).
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Mengubah buffer UTF-16 ber-NUL menjadi `String` (sampai NUL pertama atau akhir buffer).
fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[test]
fn parse_comp_sesuai_sumber() {
    assert_eq!(w12_diag_parse_comp(Some(&wide("start"))), W12DIAG_START);
    assert_eq!(w12_diag_parse_comp(Some(&wide("panel"))), W12DIAG_PANEL);
    assert_eq!(w12_diag_parse_comp(Some(&wide("qs"))), W12DIAG_QS);
    assert_eq!(w12_diag_parse_comp(Some(&wide("flyout"))), W12DIAG_FLYOUT);
    assert_eq!(w12_diag_parse_comp(Some(&wide("taskview"))), W12DIAG_TASKVIEW);
    assert_eq!(w12_diag_parse_comp(Some(&wide("Start"))), -1);
    assert_eq!(w12_diag_parse_comp(Some(&wide("star"))), -1);
    assert_eq!(w12_diag_parse_comp(Some(&wide("start "))), -1);
    assert_eq!(w12_diag_parse_comp(Some(&wide(""))), -1);
    // nullptr di C dipetakan ke `None`.
    assert_eq!(w12_diag_parse_comp(None), -1);
    assert_eq!(w12_diag_comp_name(W12DIAG_TASKVIEW), "taskview");
    // Di luar rentang menghasilkan "?"; `W12DIAG_COUNT` bertipe usize, jadi dikonversi ke i32.
    assert_eq!(w12_diag_comp_name(W12DIAG_COUNT as i32), "?");
}

#[test]
fn parse_n_sesuai_sumber() {
    assert_eq!(w12_diag_parse_n(Some(&wide("1"))), 1);
    assert_eq!(w12_diag_parse_n(Some(&wide("20"))), 20);
    assert_eq!(w12_diag_parse_n(Some(&wide("100"))), 100);
    assert_eq!(w12_diag_parse_n(Some(&wide("0000020"))), 20);
    assert_eq!(w12_diag_parse_n(Some(&wide("0"))), -1);
    assert_eq!(w12_diag_parse_n(Some(&wide("101"))), -1);
    assert_eq!(w12_diag_parse_n(Some(&wide("99999999999999999999"))), -1);
    assert_eq!(w12_diag_parse_n(Some(&wide(""))), -1);
    assert_eq!(w12_diag_parse_n(Some(&wide("-5"))), -1);
    assert_eq!(w12_diag_parse_n(Some(&wide("2x"))), -1);
    assert_eq!(w12_diag_parse_n(None), -1);
}

#[test]
fn parse_cmd_sesuai_sumber() {
    let mut comp = -9;
    let mut n = -9;
    assert_eq!(w12_diag_parse_cmd(Some(&wide("win12shell.exe --shell")), &mut comp, &mut n), 0);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("win12shell.exe --diag-cycles start 20")), &mut comp, &mut n), 0);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("xx--diag-cycle start 20")), &mut comp, &mut n), 0);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("win12shell.exe --diag-cycle start 20")), &mut comp, &mut n), 1);
    assert!(comp == W12DIAG_START && n == 20);
    assert_eq!(
        w12_diag_parse_cmd(Some(&wide("\"C:\\w12\\win12shell.exe\" --diag-cycle taskview 100")), &mut comp, &mut n),
        1
    );
    assert!(comp == W12DIAG_TASKVIEW && n == 100);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("win12shell.exe --diag-cycle\tqs\t5")), &mut comp, &mut n), 1);
    assert!(comp == W12DIAG_QS && n == 5);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("--diag-cycles x --diag-cycle panel 3")), &mut comp, &mut n), 1);
    assert!(comp == W12DIAG_PANEL && n == 3);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("win12shell.exe --diag-cycle")), &mut comp, &mut n), -1);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("win12shell.exe --diag-cycle start")), &mut comp, &mut n), -1);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("win12shell.exe --diag-cycle bogus 20")), &mut comp, &mut n), -1);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("win12shell.exe --diag-cycle start 0")), &mut comp, &mut n), -1);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("win12shell.exe --diag-cycle start 101")), &mut comp, &mut n), -1);
    assert_eq!(w12_diag_parse_cmd(Some(&wide("win12shell.exe --diag-cycle flyout 2x")), &mut comp, &mut n), -1);
}

#[test]
fn tenths_sesuai_sumber() {
    assert_eq!(w12_diag_tenths_mb(0), 0);
    assert_eq!(w12_diag_tenths_mb(1048576), 10);
    assert_eq!(w12_diag_tenths_mb(49283072), 470);
    assert_eq!(w12_diag_tenths_mb(49492788), 472);
    assert_eq!(w12_diag_tenths_mb(1073741824), 10240);
}

#[test]
fn format_row_sesuai_sumber() {
    let a = W12DiagSample { handles: 965, gdi: 84, user: 65, priv_: 49283072 };
    let b = W12DiagSample { handles: 968, gdi: 85, user: 66, priv_: 49492788 };
    let mut buf = [0u16; 256];
    let len = w12_diag_format_row(&mut buf, W12DIAG_START, 20, &a, &b);
    assert!(len > 0);
    assert_eq!(
        wide_to_string(&buf),
        "diag start 20 handle 965\u{2192}968 gdi 84\u{2192}85 user 65\u{2192}66 privat 47.0\u{2192}47.2 MB"
    );
    // Buffer terlalu kecil (cap 10 di C) menghasilkan -1; panjang buffer kini diambil dari `buf.len()`.
    let mut kecil = [0u16; 10];
    assert_eq!(w12_diag_format_row(&mut kecil, W12DIAG_START, 20, &a, &b), -1);
}

#[test]
fn format_note_sesuai_sumber() {
    let mut buf = [0u16; 64];
    assert!(w12_diag_format_note(&mut buf, W12DIAG_FLYOUT, Some("tidak tersedia")) > 0);
    assert_eq!(wide_to_string(&buf), "diag flyout tidak tersedia");
    let mut kecil = [0u16; 4];
    assert_eq!(w12_diag_format_note(&mut kecil, W12DIAG_QS, Some("x")), -1);
}

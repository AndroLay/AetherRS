//! Port `tests/w12query_test.cpp` (42 baris non-kosong). Status: lengkap.
//! Tes murni: tanpa registry, file, atau proses. Jalankan dengan `cargo test --test query`.

use w12::query::{
    w12_calc_eval, w12_calc_format, w12_calc_looks_like, w12_query_classify, w12_url_encode,
    W12Q_CALC, W12Q_CMD, W12Q_FILES, W12Q_NORMAL, W12Q_WEB,
};

fn eval(expr: &str) -> Option<f64> {
    let mut r = 0.0;
    if w12_calc_eval(expr, &mut r) {
        Some(r)
    } else {
        None
    }
}

fn eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn ok_eq(expr: &str, want: f64) -> bool {
    matches!(eval(expr), Some(v) if eq(v, want))
}

fn fmt(v: f64) -> String {
    let mut buf = [0u16; 64];
    w12_calc_format(v, &mut buf);
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn url(s: &str, cap: usize) -> (i32, String) {
    let mut buf = vec![0u8; cap];
    let n = w12_url_encode(s, &mut buf);
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    (n, String::from_utf8_lossy(&buf[..end]).into_owned())
}

#[test]
fn kalkulator_hasil() {
    assert!(ok_eq("2+3*4", 14.0));
    assert!(ok_eq("(2+3)*4", 20.0));
    assert!(ok_eq("2^3^2", 512.0));
    assert!(ok_eq("-2^2", -4.0));
    assert!(ok_eq("2,5*2", 5.0));
    assert!(ok_eq("6 x 7", 42.0));
    assert!(ok_eq("8:2", 4.0));
    assert!(ok_eq("sqrt(16)+1", 5.0));
    assert!(ok_eq("10÷4", 2.5));
}

#[test]
fn kalkulator_gagal() {
    assert!(eval("1/0").is_none());
    assert!(eval("2+").is_none());
    assert!(eval("abc").is_none());
    assert!(eval("1/(1/0)").is_none()); // pembagian nol tersembunyi tetap gagal
    assert!(eval("sqrt(-1)").is_none());
}

#[test]
fn kalkulator_pangkat_negatif() {
    assert!(ok_eq("2^-1", 0.5));
}

#[test]
fn format_angka() {
    assert_eq!(fmt(2.5), "2,5");
    assert_eq!(fmt(0.1 + 0.2), "0,3");
    assert_eq!(fmt(1e21), "1e+21");
    assert_eq!(fmt(-0.0), "0"); // -0 dinormalkan
}

#[test]
fn kalkulator_terdeteksi() {
    assert!(!w12_calc_looks_like("12"));
    assert!(w12_calc_looks_like("12+3"));
    assert!(!w12_calc_looks_like("kalkulator"));
    assert!(!w12_calc_looks_like("x+1"));
    assert!(w12_calc_looks_like("6 X 7")); // X kapital di antara dua angka
}

#[test]
fn klasifikasi_prefiks() {
    let mut rest: &str = "";
    assert_eq!(w12_query_classify("=1+1", Some(&mut rest)), W12Q_CALC);
    assert_eq!(rest, "1+1");
    assert_eq!(w12_query_classify("> cmd /k", Some(&mut rest)), W12Q_CMD);
    assert_eq!(rest, "cmd /k");
    assert_eq!(w12_query_classify("FILE:laporan", Some(&mut rest)), W12Q_FILES);
    assert_eq!(rest, "laporan");
    assert_eq!(w12_query_classify("?cuaca", Some(&mut rest)), W12Q_WEB);
    assert_eq!(w12_query_classify("laporan", Some(&mut rest)), W12Q_NORMAL);
    assert_eq!(rest, "laporan");
    assert_eq!(w12_query_classify("  =1+1", Some(&mut rest)), W12Q_CALC); // spasi depan diabaikan
    assert_eq!(rest, "1+1");
}

#[test]
fn url_encode() {
    let (n, s) = url("kucing lucu", 64);
    assert!(n > 0 && s == "kucing+lucu");
    let (n, s) = url("a&b", 64);
    assert!(n > 0 && s == "a%26b");
    let (n, s) = url("ä", 64);
    assert!(n > 0 && s == "%C3%A4");
    assert_eq!(url("abcdef", 4).0, -1);
}

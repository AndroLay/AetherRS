//! Port tests\w12fm_logic_test.cpp (157 baris): tes logika murni Win12 Files, in-memory (tanpa registry, file, atau proses).
//! Status: lengkap. 10 #[test] setara TestXxx di C++ (baris 8-141); main (143-157) diganti harness cargo test.

use w12::fm_logic::*;

/// Setara CHECK di C++: gagal bila ekspresi false, dengan teks ekspresi dan baris sumber.
macro_rules! check {
    ($e:expr) => {
        if !($e) {
            panic!("FAIL: {} (baris {})", stringify!($e), line!());
        }
    };
}

/// Mengubah buffer UTF-16 menjadi String sampai NUL pertama (meniru perbandingan wcscmp dengan literal).
fn units_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[test]
fn test_tabs() {
    let mut t = W12Tabs::default();
    check!(w12_tabs_add(&mut t, false) == 0 && w12_tabs_add(&mut t, false) == 1 && w12_tabs_add(&mut t, false) == 2 && t.active == 2);
    t.active = 0;
    check!(w12_tabs_add(&mut t, true) == 3 && t.id[1] == 3 && t.active == 1); // sisip kanan tab aktif
    check!(w12_tabs_close(&mut t, 1) == 1 && t.n == 3 && t.id[1] == 1); // aktif -> tetangga kanan
    t.active = 2;
    check!(w12_tabs_close(&mut t, 2) == 1 && t.n == 2); // aktif paling kanan -> kiri
    w12_tabs_cycle(&mut t, 1);
    check!(t.active == 0);
    w12_tabs_cycle(&mut t, -1);
    check!(t.active == 1);
    w12_tabs_move(&mut t, 1, 0);
    check!(t.id[0] == 1 && t.active == 0);
    let mut full = W12Tabs::default();
    for _ in 0..12 {
        w12_tabs_add(&mut full, false);
    }
    check!(w12_tabs_add(&mut full, false) == -1);
    let mut one = W12Tabs::default();
    w12_tabs_add(&mut one, false);
    check!(w12_tabs_close(&mut one, 0) == -1 && one.n == 0);
}

#[test]
fn test_accel() {
    let mut n = -1;
    check!(w12_fm_accel(W12FK_LEFT, false, false, true, false, Some(&mut n)) == W12FA_BACK);
    check!(w12_fm_accel(W12FK_BACK, false, false, false, true, Some(&mut n)) == W12FA_NONE
        && w12_fm_accel(W12FK_BACK, false, false, false, false, Some(&mut n)) == W12FA_UP);
    check!(w12_fm_accel(b'N' as u32, true, true, false, false, Some(&mut n)) == W12FA_NEW_FOLDER
        && w12_fm_accel(b'N' as u32, true, false, false, false, Some(&mut n)) == W12FA_NEW_WINDOW);
    check!(w12_fm_accel(b'3' as u32, true, false, false, false, Some(&mut n)) == W12FA_TAB_N && n == 2);
    check!(w12_fm_accel(b'9' as u32, true, false, false, false, Some(&mut n)) == W12FA_TAB_N && n == -1);
    check!(w12_fm_accel(b'C' as u32, true, false, false, false, Some(&mut n)) == W12FA_NONE); // salin = milik DefView
}

#[test]
fn test_address_and_args() {
    let mut o = [0u16; 260];
    check!(w12_fm_normalize_address(Some("  \"C:/Users/u/\"  "), &mut o) != 0 && units_to_string(&o) == "C:\\Users\\u");
    check!(w12_fm_normalize_address(Some("C:\\"), &mut o) != 0 && units_to_string(&o) == "C:\\");
    check!(w12_fm_normalize_address(Some("shell:Downloads"), &mut o) != 0 && units_to_string(&o) == "shell:Downloads");
    check!(w12_fm_normalize_address(Some("   "), &mut o) == 0);
    let mut sel = false;
    check!(w12_fm_parse_args(Some("\"C:\\x\\win12files.exe\" /select,\"C:\\a b\\c.txt\""), &mut o, Some(&mut sel)) != 0
        && sel && units_to_string(&o) == "C:\\a b\\c.txt");
    check!(w12_fm_parse_args(Some("win12files.exe D:\\Data"), &mut o, Some(&mut sel)) != 0
        && !sel && units_to_string(&o) == "D:\\Data");
    check!(w12_fm_parse_args(Some("win12files.exe"), &mut o, Some(&mut sel)) == 0 && o[0] == 0);
}

#[test]
fn test_history() {
    let mut h = W12Hist { n: 0, cur: -1 };
    check!(w12_hist_push(&mut h, 3) == 0 && h.n == 1 && h.cur == 0);
    check!(w12_hist_push(&mut h, 3) == 0 && h.n == 2 && h.cur == 1);
    check!(w12_hist_push(&mut h, 3) == 0 && h.n == 3 && h.cur == 2);
    check!(w12_hist_push(&mut h, 3) == 1 && h.n == 3 && h.cur == 2); // penuh: entri terlama dibuang
    check!(w12_hist_can_back(&h) && !w12_hist_can_forward(&h));
    check!(w12_hist_back(&mut h) == 1 && w12_hist_can_forward(&h) && w12_hist_forward(&mut h) == 2);
    check!(w12_hist_forward(&mut h) == -1);
    w12_hist_back(&mut h); // cur = 1
    check!(w12_hist_push(&mut h, 3) == 0 && h.n == 3 && h.cur == 2); // entri maju dibuang, lalu entri baru
    let mut e = W12Hist::default();
    check!(!w12_hist_can_back(&e) && !w12_hist_can_forward(&e) && w12_hist_back(&mut e) == -1);
}

#[test]
fn test_tab_geometry() {
    check!(w12_fm_tab_width(1, 1000, 80, 200) == 200); // maks
    check!(w12_fm_tab_width(4, 600, 80, 200) == 150); // dibagi rata
    check!(w12_fm_tab_width(12, 600, 80, 200) == 80); // minimum (boleh melampaui bilah)
    check!(w12_fm_tab_slot_at(0, 3, 100, 36) == 0 && w12_fm_tab_slot_at(99, 3, 100, 36) == 0);
    check!(w12_fm_tab_slot_at(100, 3, 100, 36) == 1);
    check!(w12_fm_tab_slot_at(300, 3, 100, 36) == 3 && w12_fm_tab_slot_at(335, 3, 100, 36) == 3); // tombol +
    check!(w12_fm_tab_slot_at(336, 3, 100, 36) == -1 && w12_fm_tab_slot_at(-1, 3, 100, 36) == -1);
    check!(w12_fm_tab_close_hit(180, 100, 100, 16, 8)); // kotak x: [176, 192)
    check!(!w12_fm_tab_close_hit(175, 100, 100, 16, 8) && !w12_fm_tab_close_hit(192, 100, 100, 16, 8));
}

#[test]
fn test_commands() {
    check!(w12_fm_cmd_needs_selection(W12FC_CUT) && w12_fm_cmd_needs_selection(W12FC_COPY));
    check!(w12_fm_cmd_needs_selection(W12FC_RENAME) && w12_fm_cmd_needs_selection(W12FC_DELETE));
    check!(!w12_fm_cmd_needs_selection(W12FC_PASTE) && !w12_fm_cmd_needs_selection(W12FC_NEWFOLDER));
    check!(!w12_fm_cmd_enabled(W12FC_CUT, false) && w12_fm_cmd_enabled(W12FC_CUT, true));
    check!(w12_fm_cmd_enabled(W12FC_PASTE, false) && w12_fm_cmd_enabled(W12FC_SORT, false) && w12_fm_cmd_enabled(W12FC_VIEW, false));
}

#[test]
fn test_theme_and_palette() {
    check!(!w12_fm_theme_is_dark(Some("light"), false) && !w12_fm_theme_is_dark(Some("LIGHT"), false));
    check!(w12_fm_theme_is_dark(Some("dark"), true) && w12_fm_theme_is_dark(None, true)); // bawaan gelap
    check!(w12_fm_theme_is_dark(Some("gelap"), false)); // nilai lain = gelap
    check!(w12_fm_theme_is_dark(Some("system"), false) && !w12_fm_theme_is_dark(Some("system"), true));
    let d = w12_fm_palette_for(true);
    let l = w12_fm_palette_for(false);
    check!(d.bg == 0x202020 && d.panel == 0x2B2B2B && d.text == 0xFFFFFF);
    check!(l.bg == 0xF3F3F3 && l.panel == 0xFFFFFF && l.text == 0x1A1A1A);
    check!(d.hover != d.bg && d.press != d.hover && d.dim != d.text);
    check!(l.hover != l.bg && l.press != l.hover && l.dim != l.text);
}

#[test]
fn test_sort() {
    let mut s = W12FmSort { col: 0, desc: false };
    s = w12_fm_sort_next(s, 0);
    check!(s.col == 0 && s.desc); // klik ulang: balik arah
    s = w12_fm_sort_next(s, 0);
    check!(s.col == 0 && !s.desc);
    s = w12_fm_sort_next(s, 2);
    check!(s.col == 2 && !s.desc); // kolom lain: naik
}

#[test]
fn test_search_scope() {
    check!(w12_fm_under_folder("C:\\Users\\u\\Documents\\a.txt", Some("C:\\Users\\u\\Documents")));
    check!(w12_fm_under_folder("C:\\Users\\u\\Documents\\x\\y.txt", Some("C:\\Users\\u\\Documents"))); // turunan
    check!(w12_fm_under_folder("c:\\USERS\\u\\documents\\a.txt", Some("C:\\Users\\u\\Documents"))); // tanpa beda huruf
    check!(!w12_fm_under_folder("C:\\Users\\u\\Documents2\\a.txt", Some("C:\\Users\\u\\Documents"))); // batas '\'
    check!(!w12_fm_under_folder("C:\\Users\\u\\Documents", Some("C:\\Users\\u\\Documents"))); // folder itu sendiri
    check!(!w12_fm_under_folder("C:\\Windows\\a.txt", Some("C:\\Users\\u\\Documents")));
    check!(w12_fm_under_folder("C:\\Windows\\a.txt", Some("C:\\"))); // akar
    check!(!w12_fm_under_folder("D:\\a.txt", Some("C:\\")));
    check!(w12_fm_under_folder("C:\\Users\\u\\Documents\\a.txt", Some("C:\\Users\\u\\Documents\\"))); // folder berakhir '\'
    check!(!w12_fm_under_folder("C:\\Users\\u\\Documents", Some("C:\\Users\\u\\Documents\\")));
    check!(w12_fm_under_folder("C:\\a\\b.txt", Some(""))); // tanpa filter
    check!(!w12_fm_under_folder("", Some("C:\\")));
}

#[test]
fn test_explorer_args() {
    let mut o = [0u16; 260];
    check!(w12_fm_explorer_args(Some("C:\\a b"), false, &mut o) && units_to_string(&o) == "\"C:\\a b\"");
    check!(w12_fm_explorer_args(Some("C:\\a b\\c.txt"), true, &mut o) && units_to_string(&o) == "/select,\"C:\\a b\\c.txt\"");
    check!(w12_fm_explorer_args(Some(""), false, &mut o) && o[0] == 0); // tanpa jalur: explorer biasa
    check!(w12_fm_explorer_args(None, false, &mut o) && o[0] == 0);
    check!(!w12_fm_explorer_args(Some("C:\\x\"y"), false, &mut o)); // kutip di jalur: tolak
    check!(!w12_fm_explorer_args(Some("C:\\abc"), false, &mut o[..4])); // muat tidak cukup
}

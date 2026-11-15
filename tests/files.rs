//! Port `tests\w12files_test.cpp` (baris 1–282): format, skip entry, bonus rank, dedupe akar, induk pendek,
//! lower-into, indeks rusak, build+cari, dan batas kedalaman. Status: lengkap sebagai kode tes, belum dijalankan.
//! Tes bergantung FX (`w12_fx_*` masih `todo!()` di `src\files\model.rs`): indeks rusak, build+cari, batas kedalaman.

use std::ffi::c_void;
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::sync::atomic::AtomicI32;

use windows::Win32::Foundation::HANDLE;

use w12::files::model::{
    w12_file_skip_entry, w12_fx_build, w12_fx_close, w12_fx_dedupe_roots, w12_fx_open, w12_fx_path,
    w12_fx_rank_bonus, w12_fx_search, w12_fx_short_parent, W12FxBuildStats, W12FxEntry, W12FxHeader,
    W12FxView, W12FX_DIR, W12FX_RECENT, W12FX_ROOT,
};
use w12::search::{w12_search_lower_into, w12_search_make_query, W12SearchHit, W12SearchQuery};

/// Pengganti `CHECK` C++: gagal -> cetak `FAIL: <ekspresi>` ke stderr, lalu `return false` dari fungsi tes.
macro_rules! check {
    ($c:expr) => {
        if !($c) {
            eprintln!("FAIL: {}", stringify!($c));
            return false;
        }
    };
}

const H: u64 = 36_000_000_000; // 1 jam dalam satuan FILETIME (100 ns)
const D: u64 = 864_000_000_000; // 1 hari
const NOW: u64 = 1000 * D;
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
/// Pola `memset(&v, 0xAB, ...)` untuk pegangan palsu (non-null) pada `W12FxView`.
const POISON: usize = usize::from_ne_bytes([0xAB; std::mem::size_of::<usize>()]);

/// Panjang string UTF-16 sampai NUL pertama (`wcslen`); tanpa NUL, panjang buffer.
fn wide_len(buf: &[u16]) -> usize {
    buf.iter().position(|&c| c == 0).unwrap_or(buf.len())
}

/// Isi buffer UTF-16 sampai NUL pertama sebagai `String`.
fn wide_str(buf: &[u16]) -> String {
    String::from_utf16_lossy(&buf[..wide_len(buf)])
}

/// Folder sementara tanpa '\' di akhir (`GetTempPathW` memberi '\' di akhir).
fn temp_dir_str() -> String {
    std::env::temp_dir().to_string_lossy().trim_end_matches('\\').to_string()
}

/// Nama 30 karakter: "lapisan_NN_" lalu dipadatkan dengan 'x' (sumber: MakeLongName).
fn make_long_name(level: i32) -> String {
    let mut s = format!("lapisan_{level:02}_");
    while s.len() < 30 {
        s.push('x');
    }
    s
}

/// Berkas kosong dengan CREATE_NEW; `attrs` != 0 dipasang saat pembuatan (sumber: TouchFile).
fn touch_file(path: &str, attrs: u32) -> bool {
    let mut opt = std::fs::OpenOptions::new();
    opt.write(true).create_new(true);
    if attrs != 0 {
        opt.attributes(attrs);
    }
    opt.open(path).is_ok()
}

/// Pohon uji (sumber: BuildTree); `long_root` berawalan `\\?\`.
fn build_tree(long_root: &str) -> bool {
    let docs = format!("{long_root}\\Docs");
    check!(std::fs::create_dir(long_root).is_ok()); // akar uji; tidak pernah dibuat sebelumnya
    check!(std::fs::create_dir(&docs).is_ok());
    check!(touch_file(&format!("{docs}\\Laporan Akhir.docx"), 0));
    check!(std::fs::create_dir(format!("{docs}\\Proyek")).is_ok());
    check!(touch_file(&format!("{docs}\\Proyek\\anggaran.xlsx"), 0));
    check!(std::fs::create_dir(format!("{docs}\\node_modules")).is_ok());
    check!(touch_file(&format!("{docs}\\node_modules\\x.js"), 0));
    check!(std::fs::create_dir(format!("{docs}\\.git")).is_ok());
    check!(touch_file(&format!("{docs}\\.git\\config"), 0));
    check!(touch_file(&format!("{docs}\\rahasia.txt"), FILE_ATTRIBUTE_HIDDEN));
    check!(touch_file(&format!("{docs}\\\u{00dc}n\u{00ef}code \u{00e4}.txt"), 0));
    check!(touch_file(&format!("{docs}\\~$kunci.docx"), 0));
    let mut p = docs.clone();
    for i in 0..9 {
        p = format!("{p}\\{}", make_long_name(i));
        check!(std::fs::create_dir(&p).is_ok());
    }
    check!(touch_file(&format!("{p}\\dalam.txt"), 0));
    true
}

/// Pegangan kosong untuk `w12_fx_open` (`W12FxView` tidak punya Default).
fn empty_view() -> W12FxView {
    W12FxView {
        file: HANDLE(std::ptr::null_mut()),
        map: HANDLE(std::ptr::null_mut()),
        h: std::ptr::null(),
        e: std::ptr::null(),
        pool: std::ptr::null(),
    }
}

/// Jumlah hasil pencarian indeks (sumber: Count), kapasitas 64. Bergantung FX.
fn count(v: &W12FxView, q: &str) -> i32 {
    let mut query = W12SearchQuery::default();
    w12_search_make_query(q, &mut query);
    let mut hits = [W12SearchHit::default(); 64];
    w12_fx_search(v, &query, NOW, &mut hits)
}

/// Jalur hasil teratas untuk `q` (sumber: TopPath). Bergantung FX.
fn top_path(v: &W12FxView, q: &str, out: &mut [u16]) -> bool {
    let mut query = W12SearchQuery::default();
    w12_search_make_query(q, &mut query);
    let mut hits = [W12SearchHit::default(); 64];
    let n = w12_fx_search(v, &query, NOW, &mut hits);
    n > 0 && w12_fx_path(v, hits[0].reference as u32, out) > 0
}

/// Verifikasi indeks hasil build (sumber: CheckIndex, 13 CHECK). Bergantung FX.
fn check_index(v: &W12FxView, root: &str) -> bool {
    let mut path = [0u16; 4096];
    let expect: Vec<u16> = format!("{root}\\Docs\\Laporan Akhir.docx").encode_utf16().collect();
    check!(top_path(v, "laporan", &mut path));
    check!(path[..wide_len(&path)] == expect[..]);
    check!(count(v, "w12fx") == 0); // akar tidak pernah menjadi hasil
    check!(count(v, "x.js") == 0); // node_modules dilewati
    check!(count(v, "config") == 0); // .git dilewati
    check!(count(v, "rahasia") == 0); // HIDDEN dilewati
    check!(count(v, "kunci") == 0); // ~$ dilewati
    check!(count(v, "\u{00e4}") == 1); // nama non-ASCII terindeks
    check!(count(v, "anggaran") == 1);
    check!(count(v, "proyek") == 1); // folder ikut terindeks
    check!(count(v, "dalam") == 1);
    check!(top_path(v, "dalam", &mut path));
    check!(wide_len(&path) > 260); // jalur lebih dari 260 karakter
    true
}

/// Build indeks dari pohon uji, lalu buka dan verifikasi (sumber: RunIndexCheck, 4 CHECK). Bergantung FX.
fn run_index_check(root: &str, long_root: &str, idx: &str) -> bool {
    let mut st = W12FxBuildStats::default();
    let cancel = AtomicI32::new(0);
    check!(build_tree(long_root));
    check!(w12_fx_build(&[Some(root)], None, idx, Some(&cancel), Some(&mut st)));
    check!(!st.truncated);
    let mut v = empty_view();
    check!(w12_fx_open(idx, &mut v));
    let ok = check_index(&v, root);
    w12_fx_close(&mut v);
    ok
}

/// Ukuran struct dan konstanta format (sumber: TestFormatSizes, 3 CHECK).
fn test_format_sizes() -> bool {
    check!(std::mem::size_of::<W12FxHeader>() == 32);
    check!(std::mem::size_of::<W12FxEntry>() == 24);
    check!(W12FX_DIR == 1 && W12FX_ROOT == 2 && W12FX_RECENT == 4);
    true
}

/// Aturan lewati entri direktori (sumber: TestSkipEntry, 10 CHECK).
fn test_skip_entry() -> bool {
    check!(w12_file_skip_entry(Some("j"), 0x410, 0xA000_0003, true)); // junction
    check!(!w12_file_skip_entry(Some("c.pdf"), 0x400400, 0x9000_001A, false)); // file awan: tetap diindeks
    check!(w12_file_skip_entry(Some("."), 0x10, 0, true));
    check!(w12_file_skip_entry(Some(".."), 0x10, 0, true));
    check!(w12_file_skip_entry(Some("NODE_MODULES"), 0x10, 0, true));
    check!(w12_file_skip_entry(Some(".git"), 0x10, 0, true));
    check!(w12_file_skip_entry(Some("rahasia.txt"), 0x2, 0, false)); // HIDDEN
    check!(w12_file_skip_entry(Some("~$kunci.docx"), 0x20, 0, false));
    check!(w12_file_skip_entry(Some("unduhan.tmp"), 0x20, 0, false));
    check!(!w12_file_skip_entry(Some("Laporan Akhir.docx"), 0x20, 0, false));
    true
}

/// Bonus peringkat berdasarkan umur, folder, dan penanda baru (sumber: TestRankBonus, 5 CHECK).
fn test_rank_bonus() -> bool {
    check!(w12_fx_rank_bonus(NOW - H, NOW, 0) == -80);
    check!(w12_fx_rank_bonus(NOW - 10 * D, NOW, W12FX_DIR) == -90);
    check!(w12_fx_rank_bonus(NOW - 2 * D, NOW, W12FX_RECENT) == -35);
    check!(w12_fx_rank_bonus(NOW + D, NOW, 0) == -80); // mtime di masa depan dianggap baru
    check!(w12_fx_rank_bonus(NOW - 40 * D, NOW, 0) == -120);
    true
}

/// Dedupe akar: induk/anak dan duplikat tanpa memperhatikan huruf besar-kecil (sumber: TestDedupeRoots, 4 CHECK).
fn test_dedupe_roots() -> bool {
    let mut r: Vec<String> = [
        "C:\\U\\a\\OneDrive",
        "C:\\U\\a\\OneDrive\\Dokumen",
        "C:\\U\\a\\Desktop",
        "c:\\u\\a\\desktop\\",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    check!(w12_fx_dedupe_roots(&mut r) == 2);
    check!(r.first().map(String::as_str) == Some("C:\\U\\a\\OneDrive"));
    check!(r.get(1).map(String::as_str) == Some("C:\\U\\a\\Desktop"));
    let mut s: Vec<String> = vec!["C:\\U\\a\\One".to_string(), "C:\\U\\a\\OneDrive".to_string()];
    check!(w12_fx_dedupe_roots(&mut s) == 2);
    true
}

/// Induk relatif terhadap profil (sumber: TestShortParent, 2 CHECK).
fn test_short_parent() -> bool {
    let mut out = [0u16; 260];
    w12_fx_short_parent(Some("C:\\Users\\u\\Documents\\Proyek\\a.xlsx"), Some("C:\\Users\\u"), &mut out);
    check!(wide_str(&out) == "Documents\\Proyek");
    w12_fx_short_parent(Some("E:\\Data\\b.txt"), Some("C:\\Users\\u"), &mut out);
    check!(wide_str(&out) == "E:\\Data");
    true
}

/// Pengecilan huruf ke buffer terbatas (sumber: TestLowerInto, 2 CHECK).
fn test_lower_into() -> bool {
    let mut buf = [0u16; 8];
    w12_search_lower_into("ABC\u{00c4}DEF", &mut buf, 7);
    check!(wide_str(&buf) == "abc\u{00e4}def");
    w12_search_lower_into("ABCDEF", &mut buf, 3); // dibatasi n karakter
    check!(wide_str(&buf) == "abc");
    true
}

/// Indeks rusak (header terpotong) harus ditolak dan semua pegangan dikosongkan
/// (sumber: TestBrokenIndexRejected, 5 CHECK). Bergantung FX: `w12_fx_open` belum diport.
fn test_broken_index_rejected() -> bool {
    const JUNK: &[u8; 16] = b"WFX1-dipotong\0\0\0"; // header 32 B terpotong
    check!(std::env::temp_dir().is_dir()); // GetTempPathW > 0
    let tmp = temp_dir_str();
    let bad = format!("{tmp}\\w12fx_bad.idx");
    let created = std::fs::File::create(&bad);
    check!(created.is_ok());
    let Ok(mut f) = created else {
        return false;
    };
    let wrote = f.write_all(JUNK).is_ok();
    drop(f);
    check!(wrote);
    // memset(&v, 0xAB) di sumber: semua pegangan diisi pola non-null
    let mut v = W12FxView {
        file: HANDLE(POISON as *mut c_void),
        map: HANDLE(POISON as *mut c_void),
        h: POISON as *const W12FxHeader,
        e: POISON as *const W12FxEntry,
        pool: POISON as *const u16,
    };
    let opened = w12_fx_open(&bad, &mut v);
    let _ = std::fs::remove_file(&bad); // DeleteFileW di sumber (berkas uji sendiri)
    check!(!opened);
    check!(v.h.is_null() && v.e.is_null() && v.pool.is_null() && v.file.0.is_null() && v.map.0.is_null());
    true
}

/// Build lalu cari pada pohon uji (sumber: TestBuildAndSearch, 1 CHECK; RunIndexCheck 4 CHECK).
/// Bergantung FX: `w12_fx_build` belum diport.
fn test_build_and_search() -> bool {
    check!(std::env::temp_dir().is_dir()); // GetTempPathW > 0
    let tmp = temp_dir_str();
    let root = format!("{tmp}\\w12fx_test");
    let idx = format!("{tmp}\\w12fx_test.idx");
    let long_root = format!("\\\\?\\{root}");
    let _ = std::fs::remove_dir_all(&long_root); // RemoveTree di sumber
    let _ = std::fs::remove_file(&idx); // DeleteFileW di sumber
    let ok = run_index_check(&root, &long_root, &idx);
    let _ = std::fs::remove_dir_all(&long_root);
    let _ = std::fs::remove_file(&idx);
    ok
}

/// Batas kedalaman indeks: berkas di kedalaman 16 terindeks, di kedalaman 17 tidak
/// (sumber: TestDepthLimit, 9 CHECK). Bergantung FX: `w12_fx_build`, `w12_fx_open`, `w12_fx_search` belum diport.
fn test_depth_limit() -> bool {
    check!(std::env::temp_dir().is_dir()); // GetTempPathW > 0
    let tmp = temp_dir_str();
    let root = format!("{tmp}\\w12fx_depth");
    let idx = format!("{tmp}\\w12fx_depth.idx");
    let long_root = format!("\\\\?\\{root}");
    let _ = std::fs::remove_dir_all(&long_root); // RemoveTree di sumber
    let _ = std::fs::remove_file(&idx); // DeleteFileW di sumber
    check!(std::fs::create_dir(&long_root).is_ok());
    let mut p = long_root.clone();
    for level in 1..=16 {
        let next = format!("{p}\\L{level}");
        check!(std::fs::create_dir(&next).is_ok());
        p = next;
        if level == 15 {
            check!(touch_file(&format!("{p}\\kedalaman16.txt"), 0));
        }
        if level == 16 {
            check!(touch_file(&format!("{p}\\kedalaman17.txt"), 0));
        }
    }
    let roots = [Some(root.as_str())];
    let mut st = W12FxBuildStats::default();
    let cancel = AtomicI32::new(0);
    check!(w12_fx_build(&roots, None, &idx, Some(&cancel), Some(&mut st)));
    let mut v = empty_view();
    check!(w12_fx_open(&idx, &mut v));
    let in16 = count(&v, "kedalaman16");
    let in17 = count(&v, "kedalaman17");
    w12_fx_close(&mut v);
    let _ = std::fs::remove_dir_all(&long_root);
    let _ = std::fs::remove_file(&idx);
    check!(in16 == 1); // kedalaman 16 terindeks
    check!(in17 == 0); // kedalaman 17 tidak terindeks
    true
}

/// Harness: urutan sama dengan `main` di sumber (`&&` berhenti pada tes pertama yang gagal).
#[test]
fn files_semua_uji() {
    assert!(
        test_format_sizes()
            && test_skip_entry()
            && test_rank_bonus()
            && test_dedupe_roots()
            && test_short_parent()
            && test_lower_into()
            && test_broken_index_rejected()
            && test_build_and_search()
            && test_depth_limit(),
        "w12files: salah satu tes gagal (lihat baris FAIL di stderr)"
    );
}

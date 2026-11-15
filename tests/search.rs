//! Sumber: `tests/w12search_test.cpp` (1-58). Status: lengkap; `W12SearchFreeFields` tidak diuji karena tidak diport.
//! Tes murni: tanpa registry, file, atau proses. Jalankan dengan `cargo test --test search`.

use w12::search::{
    w12_search_backspace, w12_search_build_fields, w12_search_make_query, w12_search_score,
    w12_search_top_add, w12_search_top_sort, W12SearchHit, W12SearchQuery,
};

fn hit(reference: i32, score: i32, order: i32) -> W12SearchHit {
    W12SearchHit {
        kind: 0,
        reference,
        score,
        match_pos: 0,
        match_len: 1,
        order,
    }
}

fn as_text(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[test]
fn ranking_and_matching() {
    let f = w12_search_build_fields("Visual Studio Code").expect("build fields");
    let mut q = W12SearchQuery::default();
    let (mut pos, mut len) = (-2i32, -2i32);

    w12_search_make_query("visual", &mut q);
    let s = w12_search_score(&f.lower, &f.initials, Some(&q), Some(&mut pos), Some(&mut len));
    assert!(s > 800 && pos == 0);

    w12_search_make_query("vsc", &mut q);
    let s = w12_search_score(&f.lower, &f.initials, Some(&q), Some(&mut pos), Some(&mut len));
    assert!(s == 650 && pos == -1);

    w12_search_make_query("code visual", &mut q);
    let s = w12_search_score(&f.lower, &f.initials, Some(&q), Some(&mut pos), Some(&mut len));
    assert!(s == 500 && pos == -1);
}

#[test]
fn stable_top_results() {
    let mut heap = [W12SearchHit::default(); 3];
    let mut count = 0i32;
    let candidates = [
        hit(0, 80, 0),
        hit(1, 100, 1),
        hit(2, 90, 2),
        hit(3, 70, 3),
        hit(4, 100, 4),
    ];
    for c in &candidates {
        w12_search_top_add(&mut heap, Some(&mut count), Some(c));
    }
    w12_search_top_sort(&mut heap[..count as usize]);
    assert_eq!(count, 3);
    assert!(heap[0].reference == 1 && heap[0].score == 100);
    assert!(heap[1].reference == 4 && heap[1].score == 100);
    assert!(heap[2].reference == 2 && heap[2].score == 90);
}

#[test]
fn backspace_by_word() {
    let mut text = [0u16; 32];
    let src: Vec<u16> = "hello world".encode_utf16().collect();
    text[..src.len()].copy_from_slice(&src);
    let mut n = w12_search_backspace(&mut text, src.len(), true);
    assert!(n == 6 && as_text(&text) == "hello ");
    n = w12_search_backspace(&mut text, n, true);
    assert!(n == 0 && text[0] == 0);

    text = [0u16; 32];
    let src: Vec<u16> = "hello".encode_utf16().collect();
    text[..src.len()].copy_from_slice(&src);
    n = w12_search_backspace(&mut text, src.len(), false);
    assert!(n == 4 && as_text(&text) == "hell");
}

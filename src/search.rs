//! Sumber: `w12search.cpp` (1-178) dan `w12search.h` (1-27). Status: lengkap, kecuali `W12SearchFreeFields` (tidak diport: memori dikelola `Vec`) dan `wordStorage` (diganti rentang kata di `lower`).
//! Posisi dan batas dihitung dalam unit UTF-16 seperti C. Input `&str` diubah ke UTF-16 hanya sampai NUL pertama, seperti `wcslen`.

use windows::Win32::UI::WindowsAndMessaging::CharLowerBuffW;

pub const W12_SEARCH_QUERY_MAX: i32 = 63;
pub const W12_SEARCH_MAX_WORDS: i32 = 32;

const QUERY_UNITS: usize = W12_SEARCH_QUERY_MAX as usize + 1;
const WORD_SLOTS: usize = W12_SEARCH_MAX_WORDS as usize;
const NAME_MAX: usize = 159; // sama dengan batas nama lama StScore
const INITIALS_MAX: usize = 39;
const NO_POS: i32 = -1;
const SPACE: u16 = 0x20;

/// Satu hasil pencarian. `reference` = field C `ref`; `match_pos`/`match_len` = `mpos`/`mlen`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12SearchHit {
    pub kind: i32,
    pub reference: i32,
    pub score: i32,
    pub match_pos: i32,
    pub match_len: i32,
    pub order: i32,
}

/// Query yang sudah diproses. Hanya `length` unit pertama di `lower` yang terisi.
/// `words` berisi rentang (awal, panjang) setiap kata di `lower`, maks 32 kata.
#[derive(Clone, Copy, Debug)]
pub struct W12SearchQuery {
    pub lower: [u16; QUERY_UNITS],
    pub words: [(usize, usize); WORD_SLOTS],
    pub length: i32,
    pub word_count: i32,
}

impl Default for W12SearchQuery {
    fn default() -> Self {
        Self {
            lower: [0; QUERY_UNITS],
            words: [(0, 0); WORD_SLOTS],
            length: 0,
            word_count: 0,
        }
    }
}

/// Pengganti pasangan `wchar_t*` dari `W12SearchBuildFields` (kepemilikan oleh `Vec`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct W12SearchFields {
    /// Nama huruf kecil, maks 159 unit.
    pub lower: Vec<u16>,
    /// Inisial awal kata, maks 39 unit.
    pub initials: Vec<u16>,
}

/// Separator kata, sama dengan `W12SearchSep` di C.
fn is_sep(c: u16) -> bool {
    c < 128 && matches!(c as u8, b' ' | b'-' | b'_' | b'.' | b'(' | b'/' | b'&')
}

/// Spasi seperti `iswspace` pada locale C (ASCII).
fn is_space(c: u16) -> bool {
    c < 128 && matches!(c as u8, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

/// Unit UTF-16 dari `s` sampai NUL pertama (seperti `wcslen`), dibatasi `max` unit.
fn units_until_nul(s: &str, max: usize) -> Vec<u16> {
    s.encode_utf16().take_while(|&c| c != 0).take(max).collect()
}

/// Potong slice di NUL pertama (perilaku string C).
fn until_nul(s: &[u16]) -> &[u16] {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    &s[..end]
}

/// Cari `needle` di `hay` seperti `wcsstr`. Needle kosong selalu cocok di 0.
fn find_sub(hay: &[u16], needle: &[u16]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Lowercase di tempat dengan `CharLowerBuffW`, sama dengan sumber C.
fn lower_units(buf: &mut [u16]) {
    if buf.is_empty() {
        return;
    }
    // SAFETY: CharLowerBuffW hanya membaca dan menulis di dalam batas `buf`.
    let _ = unsafe { CharLowerBuffW(buf) };
}

/// `Better` di C: skor lebih tinggi menang; skor sama, `order` lebih kecil menang.
fn better(a: &W12SearchHit, b: &W12SearchHit) -> bool {
    if a.score != b.score {
        a.score > b.score
    } else {
        a.order < b.order
    }
}

/// `Worse` di C: kebalikan dari `better`.
fn worse(a: &W12SearchHit, b: &W12SearchHit) -> bool {
    if a.score != b.score {
        a.score < b.score
    } else {
        a.order > b.order
    }
}

/// `W12SearchBuildFields`. `None` bila nama kosong (sama dengan `false` di C).
pub fn w12_search_build_fields(name: &str) -> Option<W12SearchFields> {
    let mut lower = units_until_nul(name, NAME_MAX);
    if lower.is_empty() {
        return None;
    }
    lower_units(&mut lower);
    let initials: Vec<u16> = lower
        .iter()
        .enumerate()
        .filter(|&(i, &c)| !is_sep(c) && (i == 0 || is_sep(lower[i - 1])))
        .map(|(_, &c)| c)
        .take(INITIALS_MAX)
        .collect();
    Some(W12SearchFields { lower, initials })
}

/// `W12SearchMakeQuery`. `query` kosong setara NULL di C: hasilnya query kosong.
pub fn w12_search_make_query(query: &str, out: &mut W12SearchQuery) {
    *out = W12SearchQuery::default();
    let mut units = units_until_nul(query, W12_SEARCH_QUERY_MAX as usize);
    let n = units.len();
    lower_units(&mut units);
    out.lower[..n].copy_from_slice(&units);
    out.length = n as i32;

    // Kata dipisah oleh spasi; tidak ada kata kosong; maks 32 kata.
    let mut word_count = 0usize;
    let mut start: Option<usize> = None;
    for i in 0..=n {
        if i == n || out.lower[i] == SPACE {
            if let Some(s) = start.take() {
                out.words[word_count] = (s, i - s);
                word_count += 1;
            }
        } else if start.is_none() && word_count < WORD_SLOTS {
            start = Some(i);
        }
    }
    out.word_count = word_count as i32;
}

/// Logika inti `W12SearchScore`: mengembalikan (skor, posisi cocok; -1 bila tidak ada).
fn score_inner(name: &[u16], initials: &[u16], q: Option<&W12SearchQuery>) -> (i32, i32) {
    let Some(q) = q else {
        return (0, NO_POS);
    };
    let name = until_nul(name);
    if q.length <= 0 || name.is_empty() {
        return (0, NO_POS);
    }
    let qn = q.length as usize;
    let lower = &q.lower[..qn];
    let nl = name.len();
    if name == lower {
        return (1000, 0);
    }
    if nl >= qn && &name[..qn] == lower {
        return (900 - nl.min(60) as i32, 0);
    }
    for i in 1..nl {
        if is_sep(name[i - 1]) && !is_sep(name[i]) && name[i..].starts_with(lower) {
            return (750 - i.min(60) as i32, i as i32);
        }
    }
    let initials = until_nul(initials);
    if qn >= 2 && initials.starts_with(lower) {
        return (650, NO_POS);
    }
    if lower.contains(&SPACE) && q.word_count > 0 {
        let all = (0..q.word_count as usize).all(|k| {
            let (start, len) = q.words[k];
            find_sub(name, &q.lower[start..start + len]).is_some()
        });
        if all {
            return (500, NO_POS);
        }
    }
    match find_sub(name, lower) {
        Some(p) => (400 - p.min(60) as i32, p as i32),
        None => (0, NO_POS),
    }
}

/// `W12SearchScore`. `name` dan `initials` berupa hasil `w12_search_build_fields`; slice kosong setara NULL di C. `q` None setara NULL.
/// `match_pos` diisi -1 bila tidak ada posisi; `match_len` diisi panjang query (0 bila `q` None).
pub fn w12_search_score(
    name: &[u16],
    initials: &[u16],
    q: Option<&W12SearchQuery>,
    match_pos: Option<&mut i32>,
    match_len: Option<&mut i32>,
) -> i32 {
    let (score, pos) = score_inner(name, initials, q);
    if let Some(p) = match_pos {
        *p = pos;
    }
    if let Some(l) = match_len {
        *l = q.map_or(0, |q| q.length);
    }
    score
}

/// `W12SearchTopAdd`: heap min (hasil terburuk di akar) berkapasitas `heap.len()`.
/// `count` berisi jumlah elemen terisi, dijepit ke [0, heap.len()] (C tidak menjepit); `None` pada `count` atau `candidate` berarti tidak ada yang dilakukan.
pub fn w12_search_top_add(
    heap: &mut [W12SearchHit],
    count: Option<&mut i32>,
    candidate: Option<&W12SearchHit>,
) {
    let (Some(count), Some(candidate)) = (count, candidate) else {
        return;
    };
    let capacity = heap.len();
    if capacity == 0 {
        return;
    }
    let mut n = (*count).clamp(0, capacity as i32) as usize;
    if n < capacity {
        let mut i = n;
        n += 1;
        heap[i] = *candidate;
        while i > 0 {
            let parent = (i - 1) / 2;
            if !worse(&heap[i], &heap[parent]) {
                break;
            }
            heap.swap(i, parent);
            i = parent;
        }
        *count = n as i32;
        return;
    }
    if !better(candidate, &heap[0]) {
        return;
    }
    heap[0] = *candidate;
    let mut i = 0usize;
    loop {
        let left = i * 2 + 1;
        let right = left + 1;
        if left >= n {
            break;
        }
        let mut worst = left;
        if right < n && worse(&heap[right], &heap[left]) {
            worst = right;
        }
        if worse(&heap[i], &heap[worst]) {
            break;
        }
        heap.swap(i, worst);
        i = worst;
    }
}

/// `W12SearchTopSort`: urutkan `hits` dengan hasil terbaik di depan (insertion sort, sama dengan C).
/// Panjang slice adalah `count` di C.
pub fn w12_search_top_sort(hits: &mut [W12SearchHit]) {
    for i in 1..hits.len() {
        let value = hits[i];
        let mut j = i;
        while j > 0 && better(&value, &hits[j - 1]) {
            hits[j] = hits[j - 1];
            j -= 1;
        }
        hits[j] = value;
    }
}

/// `W12SearchBackspace`: hapus satu karakter, atau satu kata bila `control_down`.
/// Mengembalikan panjang baru dan menulis NUL di posisi itu. `length` dibatasi panjang `text`.
pub fn w12_search_backspace(text: &mut [u16], length: usize, control_down: bool) -> usize {
    let length = length.min(text.len());
    if length == 0 {
        return 0;
    }
    let mut cut = length;
    if control_down {
        while cut > 0 && is_space(text[cut - 1]) {
            cut -= 1;
        }
        while cut > 0 && !is_space(text[cut - 1]) {
            cut -= 1;
        }
    } else {
        cut -= 1;
    }
    text[cut] = 0;
    cut
}

/// `W12SearchLowerInto`: salin hingga `n` unit dari `src` ke `dst`, beri NUL, lalu lowercase.
/// Panjang dibatasi `dst.len() - 1` agar ruang untuk NUL selalu ada.
pub fn w12_search_lower_into(src: &str, dst: &mut [u16], n: i32) {
    if dst.is_empty() {
        return;
    }
    let units = units_until_nul(src, n.max(0) as usize);
    let len = units.len().min(dst.len() - 1);
    dst[..len].copy_from_slice(&units[..len]);
    dst[len] = 0;
    lower_units(&mut dst[..len]);
}

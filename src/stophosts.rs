//! Port `w12stophosts.h` (baris 1–126): whitelist host Windows yang boleh dihentikan, parser `stopHosts=`, dan backoff
//! 10 menit. Status: lengkap. Tanpa API Windows (pemindaian proses dan TerminateProcess ada di `w12stophosts.inc`).
//! Tes di `tests/stophosts.rs` memakai data in-memory. Nama kanonik dikembalikan sebagai `&str` (ASCII).

/// Jumlah nama di whitelist (`W12STOP_COUNT`). Tipe `usize` karena dipakai sebagai ukuran array.
pub const W12STOP_COUNT: usize = 4;
/// Jumlah kemunculan yang dicatat per nama (`W12STOP_LIMIT`). Tipe `usize` karena dipakai sebagai ukuran array.
pub const W12STOP_LIMIT: usize = 3;

pub const W12STOP_SKIP: i32 = 0;
pub const W12STOP_TERMINATE: i32 = 1;
pub const W12STOP_DISABLE: i32 = 2;

/// Jendela backoff 10 menit dalam milidetik (`W12STOP_WINDOW_MS`).
pub const W12STOP_WINDOW_MS: u64 = 10 * 60 * 1000;

/// Daftar yang diizinkan. Nama di luar daftar tidak pernah diterminasi, apa pun isi `settings.ini`.
pub const W12_STOP_NAME_TABLE: [&str; W12STOP_COUNT] = [
    "SearchHost.exe",
    "StartMenuExperienceHost.exe",
    "ShellHost.exe",
    "ShellExperienceHost.exe",
];

/// Huruf ASCII A..Z menjadi a..z; unit lain tidak diubah (`W12StopLower`).
pub fn w12_stop_lower(c: u16) -> u16 {
    if (u16::from(b'A')..=u16::from(b'Z')).contains(&c) {
        c + u16::from(b'a' - b'A')
    } else {
        c
    }
}

/// Memotong `s` di NUL pertama (atau akhir slice), meniru string ber-NUL di C.
fn until_nul(s: &[u16]) -> &[u16] {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    &s[..end]
}

/// Menghapus spasi dan tab di kedua ujung (bagian dari `W12StopParse`).
fn trim_blank(s: &[u16]) -> &[u16] {
    let is_blank = |c: u16| c == u16::from(b' ') || c == u16::from(b'\t');
    let start = s.iter().position(|&c| !is_blank(c)).unwrap_or(s.len());
    let end = s.iter().rposition(|&c| !is_blank(c)).map_or(start, |i| i + 1);
    &s[start..end]
}

/// Perbandingan tanpa membedakan huruf (ASCII) antara `a` (UTF-16, dipotong di NUL) dan `b` (`W12StopEqualI`).
pub fn w12_stop_equal_i(a: &[u16], b: &str) -> bool {
    until_nul(a)
        .iter()
        .copied()
        .map(w12_stop_lower)
        .eq(b.encode_utf16().map(w12_stop_lower))
}

/// Indeks whitelist untuk nama exe (tanpa membedakan huruf). `-1` bila kosong atau tidak diizinkan (`W12StopIndex`).
pub fn w12_stop_index(name: &[u16]) -> i32 {
    let name = until_nul(name);
    if name.is_empty() {
        return -1;
    }
    for (i, entry) in W12_STOP_NAME_TABLE.iter().enumerate() {
        if w12_stop_equal_i(name, *entry) {
            return i as i32;
        }
    }
    -1
}

/// Nama termasuk whitelist (`W12StopAllowed`).
pub fn w12_stop_allowed(name: &[u16]) -> bool {
    w12_stop_index(name) >= 0
}

/// Nama kanonik (ejaan whitelist) untuk indeks; `None` bila di luar rentang (`W12StopName`).
pub fn w12_stop_name(idx: i32) -> Option<&'static str> {
    usize::try_from(idx)
        .ok()
        .and_then(|i| W12_STOP_NAME_TABLE.get(i).copied())
}

/// Mengurai nilai `stopHosts=` (dipisah koma atau titik koma; spasi dan tab di sekitar nama diabaikan).
/// `out` berisi indeks whitelist tanpa duplikat, urutan kemunculan. Entri kosong diabaikan. Entri di luar whitelist
/// (atau 64 unit ke atas) dihitung di `rejected`. Mengembalikan jumlah indeks yang diterima (`W12StopParse`).
pub fn w12_stop_parse(value: &[u16], out: &mut [i32; W12STOP_COUNT], rejected: Option<&mut i32>) -> i32 {
    let mut n = 0usize;
    let mut rej = 0i32;
    for seg in until_nul(value).split(|&c| c == u16::from(b',') || c == u16::from(b';')) {
        let tok = trim_blank(seg);
        if tok.is_empty() {
            continue;
        }
        // C: `tok[64]` memuat paling banyak 63 unit dan NUL. Token yang lebih panjang ditolak.
        let idx = if tok.len() >= 64 { -1 } else { w12_stop_index(tok) };
        if idx < 0 {
            rej += 1;
            continue;
        }
        if !out[..n].contains(&idx) {
            out[n] = idx;
            n += 1;
        }
    }
    if let Some(r) = rejected {
        *r = rej;
    }
    n as i32
}

/// Status backoff per nama. Setiap kemunculan dalam jendela 10 menit dicatat. Kemunculan ke-3 dalam jendela
/// menonaktifkan nama (`DISABLE`, tidak diterminasi). Nama nonaktif selalu `SKIP` sampai `w12_stop_reset`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12StopState {
    pub at: [[u64; W12STOP_LIMIT]; W12STOP_COUNT],
    pub used: [usize; W12STOP_COUNT],
    pub off: [bool; W12STOP_COUNT],
}

/// Mengosongkan status backoff (`W12StopReset`).
pub fn w12_stop_reset(s: &mut W12StopState) {
    *s = W12StopState::default();
}

/// Keputusan untuk satu kemunculan nama `idx` pada waktu `now_ms` (ms, monoton) (`W12StopDecide`).
pub fn w12_stop_decide(s: &mut W12StopState, idx: i32, now_ms: u64) -> i32 {
    if idx < 0 || (idx as usize) >= W12STOP_COUNT {
        return W12STOP_SKIP;
    }
    let i = idx as usize;
    if s.off[i] {
        return W12STOP_SKIP;
    }
    let mut keep = [0u64; W12STOP_LIMIT];
    let mut n = 0usize;
    for k in 0..s.used[i].min(W12STOP_LIMIT) {
        let t = s.at[i][k];
        if now_ms >= t && now_ms - t < W12STOP_WINDOW_MS {
            keep[n] = t;
            n += 1;
        }
    }
    if n + 1 >= W12STOP_LIMIT {
        s.off[i] = true;
        s.used[i] = 0;
        return W12STOP_DISABLE;
    }
    s.at[i][..n].copy_from_slice(&keep[..n]);
    s.at[i][n] = now_ms;
    s.used[i] = n + 1;
    W12STOP_TERMINATE
}

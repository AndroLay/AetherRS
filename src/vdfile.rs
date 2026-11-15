//! Port `w12vdfile.h` (baris 1–44): parser berkas desktop virtual `vdesk-hidden.txt`. Status: lengkap.
//! Tiap baris `<hwnd hex> <pid desimal>` dipisah CRLF. Buffer diperlakukan seperti string ber-NUL:
//! indeks di luar panjang slice dibaca sebagai 0. Dipakai `win12boot` (B) lewat `crate::vdfile`.

/// Bayte ke-`i`, atau 0 bila sudah habis (setara NUL terminator di sumber C).
fn at(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

/// Membaca satu baris dari `*cur`. Sukses (`true`): isi `*hwnd` dan `*pid`, lalu geser `*cur` melewati CRLF.
/// Gagal (`false`): `*cur` tidak diubah. Gagal bila: tanpa digit hex, lebih dari 16 digit hex, tanpa spasi,
/// tanpa digit pid, pid lebih dari 0xFFFFFFFF, atau karakter lain setelah pid.
/// Port `W12VdParseNext` (nilai kembali `int` 1/0 menjadi `bool`).
pub fn w12_vd_parse_next(cur: &mut &[u8], hwnd: &mut u64, pid: &mut u32) -> bool {
    let s: &[u8] = *cur;
    let mut p = 0usize;
    let mut h: u64 = 0;
    let mut d: u64 = 0;
    let mut nh = 0u32;
    let mut nd = 0u32;

    while matches!(at(s, p), b'\r' | b'\n') {
        p += 1;
    }
    loop {
        let c = at(s, p);
        let v = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => break,
        };
        nh += 1;
        if nh > 16 {
            return false;
        }
        h = h * 16 + u64::from(v);
        p += 1;
    }
    if nh == 0 || at(s, p) != b' ' {
        return false;
    }
    p += 1;
    while at(s, p).is_ascii_digit() {
        d = d * 10 + u64::from(at(s, p) - b'0');
        if d > 0xFFFF_FFFF {
            return false;
        }
        nd += 1;
        p += 1;
    }
    if nd == 0 {
        return false;
    }
    let c = at(s, p);
    if c != b'\r' && c != b'\n' && c != 0 {
        return false;
    }
    if at(s, p) == b'\r' {
        p += 1;
    }
    if at(s, p) == b'\n' {
        p += 1;
    }
    *hwnd = h;
    *pid = d as u32;
    *cur = &s[p..];
    true
}

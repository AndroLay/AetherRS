//! Konversi string UTF-8 <-> UTF-16 (wide) untuk Win32.

use windows::core::PCWSTR;

/// UTF-16 tanpa terminator NUL.
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// UTF-16 dengan terminator NUL (dipakai sebagai PCWSTR).
pub fn to_wide_z(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

/// Buffer wide kosong berukuran `n` (untuk API yang mengisi buffer).
pub fn buf(n: usize) -> Vec<u16> {
    vec![0u16; n]
}

/// Potong pada NUL pertama lalu ubah menjadi String.
pub fn from_wide(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

/// Baca string wide ber-NUL dari pointer mentah.
///
/// # Safety
/// `p` harus null atau menunjuk ke string UTF-16 yang diakhiri NUL dan dapat dibaca.
pub unsafe fn from_pcwstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

/// PCWSTR dari buffer yang sudah ber-NUL (hasil `to_wide_z`).
/// Buffer harus tetap hidup selama PCWSTR dipakai.
pub fn pcw(buf: &[u16]) -> PCWSTR {
    PCWSTR(buf.as_ptr())
}

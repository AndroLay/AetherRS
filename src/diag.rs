//! Port `w12diag.h` (baris 1–203): logika murni perintah `--diag-cycle <komponen> <N>` dan format baris log. Status: lengkap.
//! Tanpa WinAPI. `DiagLog` tidak ada di header ini (dipakai `w12diag.inc`), jadi tidak ada panggilan log di sini.
//! Buffer `wchar_t` keluaran menjadi `&mut [u16]` (kapasitas = panjang slice). Tes di `tests/diag_logic.rs`.

/// Komponen yang bisa diukur (`enum W12DiagComp`). Konstanta enum menjadi `i32`.
pub const W12DIAG_START: i32 = 0;
pub const W12DIAG_PANEL: i32 = 1;
pub const W12DIAG_QS: i32 = 2;
pub const W12DIAG_FLYOUT: i32 = 3;
pub const W12DIAG_TASKVIEW: i32 = 4;
/// Jumlah komponen (`W12DIAG_COUNT`). Tipe `usize` karena dipakai sebagai ukuran array.
pub const W12DIAG_COUNT: usize = 5;

/// Nama komponen untuk log (`kNames`), urut sesuai konstanta `W12DIAG_*`.
const NAMES: [&str; W12DIAG_COUNT] = ["start", "panel", "qs", "flyout", "taskview"];

/// Satu pengukuran: handle proses, objek GDI, objek USER, dan memori privat (byte).
/// Field `priv` menjadi `priv_` karena `priv` adalah kata cadangan Rust.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12DiagSample {
    pub handles: u32,
    pub gdi: u32,
    pub user: u32,
    pub priv_: u64,
}

/// Memotong slice di NUL pertama (atau akhir slice), meniru string ber-NUL di C.
fn until_nul(s: &[u16]) -> &[u16] {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    &s[..end]
}

/// Unit spasi atau tab (`W12DiagIsSep`).
pub fn w12_diag_is_sep(c: u16) -> bool {
    c == u16::from(b' ') || c == u16::from(b'\t')
}

/// Perbandingan dua string sampai NUL (`W12DiagEq`). Salah satu `None` menghasilkan false.
pub fn w12_diag_eq(a: Option<&[u16]>, b: Option<&[u16]>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => until_nul(a) == until_nul(b),
        _ => false,
    }
}

/// Nama komponen untuk log (`W12DiagCompName`). Indeks di luar rentang menghasilkan "?".
pub fn w12_diag_comp_name(comp: i32) -> &'static str {
    usize::try_from(comp)
        .ok()
        .and_then(|i| NAMES.get(i).copied())
        .unwrap_or("?")
}

/// "start" menjadi `W12DIAG_START`. Case-sensitive, harus cocok penuh. Gagal (termasuk `None`) menghasilkan -1.
pub fn w12_diag_parse_comp(s: Option<&[u16]>) -> i32 {
    let Some(s) = s else {
        return -1;
    };
    let s = until_nul(s);
    for (i, name) in NAMES.iter().enumerate() {
        if s.iter().copied().eq(name.encode_utf16()) {
            return i as i32;
        }
    }
    -1
}

/// Hanya digit 1..100 (boleh diawali nol). Selain itu, termasuk `None` dan kosong, menghasilkan -1 (`W12DiagParseN`).
pub fn w12_diag_parse_n(s: Option<&[u16]>) -> i32 {
    let Some(s) = s else {
        return -1;
    };
    let s = until_nul(s);
    if s.is_empty() {
        return -1;
    }
    let mut v: i32 = 0;
    for &c in s {
        if !(u16::from(b'0')..=u16::from(b'9')).contains(&c) {
            return -1;
        }
        v = v * 10 + i32::from(c - u16::from(b'0'));
        if v > 100 {
            return -1;
        }
    }
    if v >= 1 {
        v
    } else {
        -1
    }
}

/// Unit di posisi kursor, atau 0 bila sudah habis (meniru `*p` di C).
fn peek(cur: &[u16]) -> u16 {
    cur.first().copied().unwrap_or(0)
}

/// Salin token berikutnya (dipisah spasi/tab) ke `tok` lalu NUL-kan. Gagal bila kosong atau tidak muat
/// (maksimal `tok.len() - 1` unit). `cur` adalah kursor yang dimajukan (`W12DiagNextToken`).
pub fn w12_diag_next_token(cur: &mut &[u16], tok: &mut [u16]) -> bool {
    while peek(*cur) != 0 && w12_diag_is_sep(peek(*cur)) {
        let s: &[u16] = *cur;
        *cur = &s[1..];
    }
    if peek(*cur) == 0 || tok.len() < 2 {
        return false;
    }
    let mut n = 0usize;
    while peek(*cur) != 0 && !w12_diag_is_sep(peek(*cur)) {
        if n + 1 >= tok.len() {
            return false;
        }
        tok[n] = peek(*cur);
        n += 1;
        let s: &[u16] = *cur;
        *cur = &s[1..];
    }
    tok[n] = 0;
    true
}

/// Mengurai "--diag-cycle <komponen> <N>" (`W12DiagParseCmd`). Flag harus berdiri sendiri: di awal atau didahului
/// spasi/tab/kutip, dan diikuti akhir/spasi/tab, sehingga "--diag-cycles" tidak cocok.
/// Mengembalikan 0 = flag tidak ada (termasuk `None`); 1 = valid (`comp` dan `n` diisi); -1 = flag ada tetapi argumen tidak valid.
pub fn w12_diag_parse_cmd(cmd: Option<&[u16]>, comp: &mut i32, n: &mut i32) -> i32 {
    let Some(cmd) = cmd else {
        return 0;
    };
    let cmd = until_nul(cmd);
    let flag: Vec<u16> = "--diag-cycle".encode_utf16().collect();
    let mut start = 0usize;
    while let Some(rel) = cmd[start..].windows(flag.len()).position(|w| w == flag.as_slice()) {
        let hit = start + rel;
        let before = hit == 0 || w12_diag_is_sep(cmd[hit - 1]) || cmd[hit - 1] == u16::from(b'"');
        let after = cmd.get(hit + flag.len()).copied().unwrap_or(0);
        if before && (after == 0 || w12_diag_is_sep(after)) {
            let mut q: &[u16] = &cmd[hit + flag.len()..];
            let mut tok = [0u16; 32];
            if !w12_diag_next_token(&mut q, &mut tok) {
                return -1;
            }
            let c = w12_diag_parse_comp(Some(&tok[..]));
            if c < 0 {
                return -1;
            }
            if !w12_diag_next_token(&mut q, &mut tok) {
                return -1;
            }
            let v = w12_diag_parse_n(Some(&tok[..]));
            if v < 0 {
                return -1;
            }
            *comp = c;
            *n = v;
            return 1;
        }
        start = hit + 1;
    }
    0
}

/// Byte menjadi persepuluh MB (1 MB = 2^20), dibulatkan ke bawah (`W12DiagTenthsMB`).
pub fn w12_diag_tenths_mb(bytes: u64) -> u64 {
    (bytes >> 20) * 10 + (((bytes & 0xFFFFF) * 10) >> 20)
}

/// Penulis buffer wide-char tanpa format (`W12DiagOut`). Kapasitas = panjang `buf`, sudah termasuk NUL.
struct DiagOut<'a> {
    buf: &'a mut [u16],
    len: usize,
    ok: bool,
}

impl DiagOut<'_> {
    fn put_c(&mut self, c: u16) {
        if !self.ok {
            return;
        }
        if self.len + 1 >= self.buf.len() {
            self.ok = false;
            return;
        }
        self.buf[self.len] = c;
        self.len += 1;
    }

    fn put_s(&mut self, s: &str) {
        for c in s.encode_utf16() {
            self.put_c(c);
        }
    }

    fn put_u(&mut self, v: u64) {
        for c in v.to_string().encode_utf16() {
            self.put_c(c);
        }
    }

    fn put_tenths(&mut self, t: u64) {
        self.put_u(t / 10);
        self.put_c(u16::from(b'.'));
        self.put_c(u16::from(b'0') + (t % 10) as u16);
    }

    /// Menutup string. Bila gagal, buffer dinolkan di indeks 0 dan mengembalikan -1 (`W12DiagEnd`).
    fn end(self) -> i32 {
        if !self.ok {
            if let Some(first) = self.buf.first_mut() {
                *first = 0;
            }
            return -1;
        }
        match self.buf.get_mut(self.len) {
            Some(z) => {
                *z = 0;
                self.len as i32
            }
            None => -1,
        }
    }
}

/// Baris log: "diag <komponen> <N> handle a→b gdi a→b user a→b privat x.x→y.y MB" (`W12DiagFormatRow`).
/// Mengembalikan jumlah unit tanpa NUL, atau -1 bila `buf` kurang dari 2 unit atau tidak muat.
pub fn w12_diag_format_row(buf: &mut [u16], comp: i32, n: i32, a: &W12DiagSample, b: &W12DiagSample) -> i32 {
    if buf.len() < 2 {
        return -1;
    }
    let mut o = DiagOut { buf, len: 0, ok: true };
    o.put_s("diag ");
    o.put_s(w12_diag_comp_name(comp));
    o.put_c(u16::from(b' '));
    o.put_u(n as u64);
    o.put_s(" handle ");
    o.put_u(u64::from(a.handles));
    o.put_s("→");
    o.put_u(u64::from(b.handles));
    o.put_s(" gdi ");
    o.put_u(u64::from(a.gdi));
    o.put_s("→");
    o.put_u(u64::from(b.gdi));
    o.put_s(" user ");
    o.put_u(u64::from(a.user));
    o.put_s("→");
    o.put_u(u64::from(b.user));
    o.put_s(" privat ");
    o.put_tenths(w12_diag_tenths_mb(a.priv_));
    o.put_s("→");
    o.put_tenths(w12_diag_tenths_mb(b.priv_));
    o.put_s(" MB");
    o.end()
}

/// Baris catatan: "diag <komponen> <teks>" (mis. komponen tidak tersedia) (`W12DiagFormatNote`).
/// `text` `None` diperlakukan seperti teks kosong. Mengembalikan jumlah unit tanpa NUL, atau -1 bila gagal.
pub fn w12_diag_format_note(buf: &mut [u16], comp: i32, text: Option<&str>) -> i32 {
    if buf.len() < 2 {
        return -1;
    }
    let mut o = DiagOut { buf, len: 0, ok: true };
    o.put_s("diag ");
    o.put_s(w12_diag_comp_name(comp));
    o.put_c(u16::from(b' '));
    if let Some(t) = text {
        o.put_s(t);
    }
    o.end()
}

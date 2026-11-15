//! Sumber: `w12query.cpp` (267 baris) dan `w12query.h` (24 baris). Status: lengkap.
//! Alat kueri Start: prefiks `=`, `>`, `?`, `file:` dan kalkulator kecil. Fungsi murni, tanpa state global.
//! `iswspace` locale C dipetakan ke spasi ASCII. Input `wchar_t*` menjadi `&str`; output `wchar_t*` + cap
//! menjadi `&mut [u16]` (panjang slice = cap termasuk NUL).

/// Jenis kueri: pencarian biasa.
pub const W12Q_NORMAL: i32 = 0;
/// Jenis kueri: kalkulator (prefiks `=`).
pub const W12Q_CALC: i32 = 1;
/// Jenis kueri: perintah (prefiks `>`).
pub const W12Q_CMD: i32 = 2;
/// Jenis kueri: pencarian web (prefiks `?`).
pub const W12Q_WEB: i32 = 3;
/// Jenis kueri: pencarian file (prefiks `file:`).
pub const W12Q_FILES: i32 = 4;

/// Batas panjang ekspresi dalam unit UTF-16 (seperti `wcslen`); sekaligus membatasi kedalaman rekursi.
const MAX_EXPR_LEN: usize = 256;
const PI: f64 = std::f64::consts::PI;
const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

/// Spasi seperti `iswspace` pada locale C: spasi ASCII (termasuk \v dan \f).
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r')
}

/// Memotong pada NUL pertama, seperti string C yang berakhir NUL.
fn c_str(s: &str) -> &str {
    match s.find('\0') {
        Some(i) => &s[..i],
        None => s,
    }
}

fn is_decimal_sep(c: char) -> bool {
    c == ',' || c == '.'
}

/// Status penguraian rekursif turun (C: `struct Parser { p, ok }`).
/// Karakter di luar akhir dibaca sebagai '\0', sama dengan NUL di C.
struct Parser {
    chars: Vec<char>,
    pos: usize,
    ok: bool,
}

impl Parser {
    fn new(s: &str) -> Self {
        Parser {
            chars: s.chars().collect(),
            pos: 0,
            ok: true,
        }
    }

    /// Karakter pada posisi `pos + off`, atau '\0' bila sudah di akhir.
    fn at(&self, off: usize) -> char {
        self.chars.get(self.pos + off).copied().unwrap_or('\0')
    }

    fn skip_spaces(&mut self) {
        while is_space(self.at(0)) {
            self.pos += 1;
        }
    }
}

/// Angka: digit, lalu opsional pemisah desimal (',' atau '.') diikuti digit.
/// Diurai manual agar tidak bergantung pada locale.
fn parse_number(ps: &mut Parser) -> f64 {
    let mut v: f64 = 0.0;
    while ps.at(0).is_ascii_digit() {
        v = v * 10.0 + f64::from(ps.at(0) as u8 - b'0');
        ps.pos += 1;
    }
    if is_decimal_sep(ps.at(0)) && ps.at(1).is_ascii_digit() {
        ps.pos += 1;
        let mut scale: f64 = 1.0;
        while ps.at(0).is_ascii_digit() {
            v = v * 10.0 + f64::from(ps.at(0) as u8 - b'0');
            scale *= 10.0;
            ps.pos += 1;
        }
        v /= scale;
    }
    v
}

/// Primer: angka, '(' ekspresi ')', sqrt(ekspresi), atau pi.
fn parse_primary(ps: &mut Parser) -> f64 {
    ps.skip_spaces();
    if ps.at(0) == '(' {
        ps.pos += 1;
        let v = parse_expr(ps);
        ps.skip_spaces();
        if !ps.ok || ps.at(0) != ')' {
            ps.ok = false;
            return 0.0;
        }
        ps.pos += 1;
        return v;
    }
    if ps.at(0).is_ascii_digit() || (is_decimal_sep(ps.at(0)) && ps.at(1).is_ascii_digit()) {
        return parse_number(ps);
    }
    if ps.at(0).is_ascii_alphabetic() {
        // Nama dibaca paling banyak 7 huruf ASCII, huruf kecil (seperti buffer C `name[8]`).
        let mut name = String::new();
        while ps.at(0).is_ascii_alphabetic() && name.len() < 7 {
            name.push(ps.at(0).to_ascii_lowercase());
            ps.pos += 1;
        }
        if name == "pi" {
            return PI;
        }
        if name != "sqrt" {
            ps.ok = false;
            return 0.0;
        }
        ps.skip_spaces();
        if ps.at(0) != '(' {
            ps.ok = false;
            return 0.0;
        }
        ps.pos += 1;
        let v = parse_expr(ps);
        ps.skip_spaces();
        if !ps.ok || ps.at(0) != ')' || v < 0.0 {
            ps.ok = false;
            return 0.0;
        }
        ps.pos += 1;
        return v.sqrt();
    }
    ps.ok = false;
    0.0
}

/// Pangkat: '^' kanan-asosiatif, eksponen boleh bertanda minus. Lebih kuat dari unary minus: -2^2 = -4.
fn parse_power(ps: &mut Parser) -> f64 {
    let base = parse_primary(ps);
    ps.skip_spaces();
    if !ps.ok || ps.at(0) != '^' {
        return base;
    }
    ps.pos += 1;
    let e = parse_unary(ps);
    if !ps.ok {
        return 0.0;
    }
    base.powf(e)
}

/// Unary: '-' dan '+' di depan. Operand suku boleh bertanda, jadi 2*-3 = -6.
fn parse_unary(ps: &mut Parser) -> f64 {
    ps.skip_spaces();
    if ps.at(0) == '-' {
        ps.pos += 1;
        return -parse_unary(ps);
    }
    if ps.at(0) == '+' {
        ps.pos += 1;
        return parse_unary(ps);
    }
    parse_power(ps)
}

/// Suku: kali (* x X ×) dan bagi (/ : ÷). Pembagi nol membuat ekspresi gagal.
fn parse_term(ps: &mut Parser) -> f64 {
    let mut v = parse_unary(ps);
    loop {
        ps.skip_spaces();
        if !ps.ok {
            return 0.0;
        }
        match ps.at(0) {
            '*' | 'x' | 'X' | '\u{00D7}' => {
                ps.pos += 1;
                v *= parse_unary(ps);
            }
            '/' | ':' | '\u{00F7}' => {
                ps.pos += 1;
                let d = parse_unary(ps);
                if !ps.ok || d == 0.0 {
                    ps.ok = false;
                    return 0.0;
                }
                v /= d;
            }
            _ => break,
        }
    }
    if ps.ok {
        v
    } else {
        0.0
    }
}

/// Jumlah dan selisih: '+' dan '-'.
fn parse_expr(ps: &mut Parser) -> f64 {
    let mut v = parse_term(ps);
    loop {
        ps.skip_spaces();
        if !ps.ok {
            return 0.0;
        }
        match ps.at(0) {
            '+' => {
                ps.pos += 1;
                v += parse_term(ps);
            }
            '-' => {
                ps.pos += 1;
                v -= parse_term(ps);
            }
            _ => break,
        }
    }
    if ps.ok {
        v
    } else {
        0.0
    }
}

/// Membuang nol di belakang koma (dan titik bila ikut tersisa), seperti `%g` tanpa flag `#`.
fn trim_frac_zeros(s: &str) -> &str {
    if !s.contains('.') {
        return s;
    }
    s.trim_end_matches('0').trim_end_matches('.')
}

/// Memformat `v` seperti `printf("%.12g")` pada locale C. Titik desimal belum diganti.
fn format_g12(v: f64) -> String {
    const P: i32 = 12;
    if v == 0.0 {
        return "0".to_string();
    }
    // Eksponen X dihitung dari pembulatan ke P digit signifikan, sesuai standar C.
    let sci = format!("{:.*e}", (P - 1) as usize, v);
    let (mant, exp) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let x: i32 = exp.parse().unwrap_or(0);
    if x < -4 || x >= P {
        let sign = if x < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", trim_frac_zeros(mant), sign, x.unsigned_abs())
    } else {
        let prec = (P - 1 - x) as usize;
        trim_frac_zeros(&format!("{:.*}", prec, v)).to_string()
    }
}

/// Menyalin `text` ke `out` bila muat, dengan satu byte disisakan untuk NUL (seperti `PutChars`).
fn put_bytes(out: &mut [u8], n: &mut usize, text: &[u8]) -> bool {
    if *n + text.len() + 1 > out.len() {
        return false;
    }
    out[*n..*n + text.len()].copy_from_slice(text);
    *n += text.len();
    true
}

/// Karakter yang tidak di-persen-kode pada URL: huruf, digit, dan `-_.~`.
fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~')
}

/// Jenis kueri dari awal teks (spasi depan diabaikan). `rest` menerima sisa teks tanpa spasi depan.
/// Untuk `W12Q_NORMAL`, sisanya adalah seluruh teks. Teks kosong setara dengan `q` NULL di C.
/// `rest` opsional (`None` setara NULL di C).
pub fn w12_query_classify<'a>(q: &'a str, rest: Option<&mut &'a str>) -> i32 {
    let q = q.trim_start_matches(is_space);
    let (kind, r) = if let Some(t) = q.strip_prefix('=') {
        (W12Q_CALC, t)
    } else if let Some(t) = q.strip_prefix('>') {
        (W12Q_CMD, t)
    } else if let Some(t) = q.strip_prefix('?') {
        (W12Q_WEB, t)
    } else if q.len() >= 5 && q.as_bytes()[..5].eq_ignore_ascii_case(b"file:") {
        (W12Q_FILES, &q[5..])
    } else {
        (W12Q_NORMAL, q)
    };
    if let Some(slot) = rest {
        *slot = r.trim_start_matches(is_space);
    }
    kind
}

/// Benar bila teks berisi angka dan operator serta hanya karakter kalkulator; 'x' hanya di antara dua angka.
pub fn w12_calc_looks_like(q: &str) -> bool {
    let q: Vec<char> = c_str(q).chars().collect();
    let n = q.len();
    let mut digit = false;
    let mut op = false;
    for (i, &c) in q.iter().enumerate() {
        if c.is_ascii_digit() {
            digit = true;
        } else if matches!(c, '+' | '-' | '*' | '/' | '^' | ':' | '\u{00D7}' | '\u{00F7}') {
            op = true;
        } else if matches!(c, '(' | ')') || is_decimal_sep(c) || is_space(c) {
            // Kurung, pemisah desimal, dan spasi dilewati.
        } else if c == 'x' || c == 'X' {
            // 'x' hanya sah bila diapit angka; spasi di sekitarnya boleh.
            let mut a = i;
            while a > 0 && is_space(q[a - 1]) {
                a -= 1;
            }
            let mut b = i + 1;
            while b < n && is_space(q[b]) {
                b += 1;
            }
            if a == 0 || b >= n || !q[a - 1].is_ascii_digit() || !q[b].is_ascii_digit() {
                return false;
            }
            op = true;
        } else {
            return false;
        }
    }
    digit && op
}

/// Menghitung ekspresi. Gagal (`false`, `out` tidak ditulis) bila sintaksnya salah, pembagi nol,
/// akar negatif, atau hasilnya tidak hingga. Gagal juga bila panjang lebih dari 256 unit UTF-16.
pub fn w12_calc_eval(expr: &str, out: &mut f64) -> bool {
    let expr = c_str(expr);
    if expr.encode_utf16().count() > MAX_EXPR_LEN {
        return false;
    }
    let mut ps = Parser::new(expr);
    let v = parse_expr(&mut ps);
    ps.skip_spaces();
    if !ps.ok || ps.at(0) != '\0' || !v.is_finite() {
        return false;
    }
    *out = v;
    true
}

/// Menulis `v` dengan `%.12g` dan titik desimal diganti koma, ke `out` (panjang slice = cap, termasuk NUL).
/// Nilai tidak hingga menghasilkan string kosong. Keluaran dipotong bila tidak muat.
pub fn w12_calc_format(v: f64, out: &mut [u16]) {
    if out.is_empty() {
        return;
    }
    if !v.is_finite() {
        out[0] = 0;
        return;
    }
    let text = format_g12(v).replace('.', ",");
    let mut n = 0usize;
    for b in text.bytes().take(out.len() - 1) {
        out[n] = u16::from(b);
        n += 1;
    }
    out[n] = 0;
}

/// Mengodekan `s` ke UTF-8 lalu persen-kode untuk URL (spasi menjadi '+'). Keluaran ditulis ke `out`
/// dengan NUL di akhir. Mengembalikan panjang keluaran, atau -1 bila tidak muat (panjang slice termasuk NUL).
pub fn w12_url_encode(s: &str, out: &mut [u8]) -> i32 {
    if out.is_empty() {
        return -1;
    }
    let mut n = 0usize;
    for ch in c_str(s).chars() {
        if ch == ' ' {
            if !put_bytes(out, &mut n, b"+") {
                return -1;
            }
            continue;
        }
        let mut utf8 = [0u8; 4];
        for &b in ch.encode_utf8(&mut utf8).as_bytes() {
            let ok = if is_unreserved(b) {
                put_bytes(out, &mut n, &[b])
            } else {
                let esc = [
                    b'%',
                    HEX_DIGITS[usize::from(b >> 4)],
                    HEX_DIGITS[usize::from(b & 0x0F)],
                ];
                put_bytes(out, &mut n, &esc)
            };
            if !ok {
                return -1;
            }
        }
    }
    out[n] = 0;
    n as i32
}

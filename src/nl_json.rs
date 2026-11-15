//! Sumber: `w12nl_json.cpp` (1-463) dan `w12nl_json.h` (1-26). Status: lengkap (parser JSON manual); parameter `truncated` ReadString tidak diport karena sumber selalu mengirim nullptr.
//! Buffer `char` menjadi `[u8; N]` ber-NUL; pemotongan token, penulisan volatile, dan saturasi `strtol` (long 32-bit di Windows) dipertahankan.
//! Kontrak: `NlJsonParse*` memakai `&mut` untuk out (C menolak out NULL); `NlJsonReadBool` memakai `Option<&mut bool>` (out NULL diizinkan).

use std::ptr;

const MAX_DEVICES_JSON: usize = 32768;
const MAX_TOKEN_JSON: usize = 4096;
const MAX_BOOL_JSON: usize = 65536;
const MAX_SKIP_DEPTH: u32 = 24;
const KEY_CAP: usize = 48;
const ROOT_KEY_CAP: usize = 64;
const BATTERY_KEY_CAP: usize = 32;
const PLATFORM_CAP: usize = 24;

/// `NlJsonDevice`. `name` (193) dan `link` (16) adalah buffer ber-NUL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NlJsonDevice {
    pub name: [u8; 193],
    pub link: [u8; 16],
    pub battery_level: i32,
    pub has_battery: bool,
    pub charging: bool,
    pub online: bool,
}

impl Default for NlJsonDevice {
    fn default() -> Self {
        Self {
            name: [0; 193],
            link: [0; 16],
            battery_level: 0,
            has_battery: false,
            charging: false,
            online: false,
        }
    }
}

/// `NlJsonDeviceList`: jumlah perangkat Android dan perangkat utama terpilih.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NlJsonDeviceList {
    pub android_count: i32,
    pub has_android: bool,
    pub primary: NlJsonDevice,
}

/// `NlJsonApiToken`: port dan token (64 hex dalam buffer 65 dengan NUL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NlJsonApiToken {
    pub port: u16,
    pub token: [u8; 65],
}

impl Default for NlJsonApiToken {
    fn default() -> Self {
        Self {
            port: 0,
            token: [0; 65],
        }
    }
}

/// `strlen` untuk buffer ber-NUL: potong di NUL pertama.
fn c_str(buf: &[u8]) -> &[u8] {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    &buf[..end]
}

/// `strcmp(buf, want) == 0` untuk buffer ber-NUL.
fn c_eq(buf: &[u8], want: &[u8]) -> bool {
    c_str(buf) == want
}

/// `SecureClear` di C: nolkan dengan penulisan volatile agar tidak dihapus kompilator.
fn secure_clear(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        // SAFETY: `b` adalah referensi valid yang dapat ditulis.
        unsafe { ptr::write_volatile(b as *mut u8, 0) };
    }
}

/// Kosongkan `out` setelah gagal, sama dengan `Fail(NlJsonApiToken *)` di C.
fn wipe_token(out: &mut NlJsonApiToken) {
    secure_clear(&mut out.token);
    // SAFETY: `out.port` adalah field valid dalam referensi yang sama.
    unsafe { ptr::write_volatile(&mut out.port as *mut u16, 0) };
}

/// `Append` di C: tambahkan `bytes` bila masih muat beserta NUL di akhir; bila tidak, dilewati.
fn append(out: &mut [u8], length: &mut usize, bytes: &[u8]) {
    let cap = out.len();
    if cap == 0 {
        return;
    }
    let end = *length + bytes.len();
    if end < cap {
        out[*length..end].copy_from_slice(bytes);
        *length = end;
        out[end] = 0;
    }
}

/// `EncodeUtf8` di C: tulis `cp` sebagai UTF-8 ke `bytes`, kembalikan jumlah byte.
fn encode_utf8(cp: u32, bytes: &mut [u8; 4]) -> usize {
    if cp <= 0x7f {
        bytes[0] = cp as u8;
        1
    } else if cp <= 0x7ff {
        bytes[0] = (0xc0 | (cp >> 6)) as u8;
        bytes[1] = (0x80 | (cp & 0x3f)) as u8;
        2
    } else if cp <= 0xffff {
        bytes[0] = (0xe0 | (cp >> 12)) as u8;
        bytes[1] = (0x80 | ((cp >> 6) & 0x3f)) as u8;
        bytes[2] = (0x80 | (cp & 0x3f)) as u8;
        3
    } else {
        bytes[0] = (0xf0 | (cp >> 18)) as u8;
        bytes[1] = (0x80 | ((cp >> 12) & 0x3f)) as u8;
        bytes[2] = (0x80 | ((cp >> 6) & 0x3f)) as u8;
        bytes[3] = (0x80 | (cp & 0x3f)) as u8;
        4
    }
}

/// `IsHexToken` di C: 64 karakter pertama harus hex.
fn is_hex_token(s: &[u8]) -> bool {
    s.len() >= 64 && s[..64].iter().all(|c| c.is_ascii_hexdigit())
}

/// Parser JSON minimal, setara `NlJsonParser` di C. `pos` adalah indeks ke `text`.
struct Parser<'a> {
    text: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a [u8]) -> Self {
        Self { text, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.pos).copied()
    }

    fn at_end(&self) -> bool {
        self.pos >= self.text.len()
    }

    /// `White`: lewati spasi JSON (spasi, tab, CR, LF).
    fn white(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
    }

    /// `Take`: cocokkan satu karakter, lalu lewati spasi di kedua sisi.
    fn take(&mut self, c: u8) -> bool {
        self.white();
        if self.peek() != Some(c) {
            return false;
        }
        self.pos += 1;
        self.white();
        true
    }

    /// `Literal`: cocokkan teks tepat (setelah spasi).
    fn literal(&mut self, s: &[u8]) -> bool {
        self.white();
        let end = self.pos + s.len();
        if end > self.text.len() || &self.text[self.pos..end] != s {
            return false;
        }
        self.pos = end;
        true
    }

    fn read_null(&mut self) -> bool {
        self.literal(b"null")
    }

    /// `ReadBool`: `true` atau `false` setelah spasi.
    fn read_bool(&mut self) -> Option<bool> {
        self.white();
        if self.literal(b"true") {
            Some(true)
        } else if self.literal(b"false") {
            Some(false)
        } else {
            None
        }
    }

    /// `ReadHex4`: empat digit hex. Posisi tetap maju walau gagal, sama dengan C.
    fn read_hex4(&mut self) -> Option<u32> {
        if self.text.len().saturating_sub(self.pos) < 4 {
            return None;
        }
        let mut value = 0u32;
        for _ in 0..4 {
            let c = self.text[self.pos];
            self.pos += 1;
            let digit = match c {
                b'0'..=b'9' => u32::from(c - b'0'),
                b'a'..=b'f' => u32::from(c - b'a' + 10),
                b'A'..=b'F' => u32::from(c - b'A' + 10),
                _ => return None,
            };
            value = (value << 4) | digit;
        }
        Some(value)
    }

    /// `ReadString`: baca string JSON ke `out` (dipotong bila penuh, NUL di akhir). `None` hanya memvalidasi.
    fn read_string(&mut self, mut out: Option<&mut [u8]>) -> bool {
        self.white();
        if self.pos >= self.text.len() {
            return false;
        }
        let quote = self.text[self.pos];
        self.pos += 1;
        if quote != b'"' {
            return false;
        }
        let mut length = 0usize;
        if let Some(buf) = out.as_deref_mut() {
            if let Some(first) = buf.first_mut() {
                *first = 0;
            }
        }
        while self.pos < self.text.len() {
            let c = self.text[self.pos];
            self.pos += 1;
            if c == b'"' {
                return true;
            }
            let mut bytes = [0u8; 4];
            let mut count = 1usize;
            if c == b'\\' {
                if self.pos >= self.text.len() {
                    return false;
                }
                let esc = self.text[self.pos];
                self.pos += 1;
                match esc {
                    b'"' => bytes[0] = b'"',
                    b'\\' => bytes[0] = b'\\',
                    b'/' => bytes[0] = b'/',
                    b'b' | b'f' | b'n' | b'r' | b't' => bytes[0] = b' ',
                    b'u' => {
                        let Some(mut cp) = self.read_hex4() else {
                            return false;
                        };
                        if (0xd800..=0xdbff).contains(&cp) {
                            if self.text.len() - self.pos < 6
                                || self.text[self.pos] != b'\\'
                                || self.text[self.pos + 1] != b'u'
                            {
                                return false;
                            }
                            self.pos += 2;
                            let Some(low) = self.read_hex4() else {
                                return false;
                            };
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return false;
                            }
                            cp = 0x10000 + ((cp - 0xd800) << 10) + (low - 0xdc00);
                        } else if (0xdc00..=0xdfff).contains(&cp) {
                            return false;
                        }
                        if cp < 0x20 || (0x7f..=0x9f).contains(&cp) {
                            cp = u32::from(b' ');
                        }
                        count = encode_utf8(cp, &mut bytes);
                    }
                    _ => return false,
                }
            } else {
                if c < 0x20 {
                    return false;
                }
                bytes[0] = c;
            }
            if let Some(buf) = out.as_deref_mut() {
                append(buf, &mut length, &bytes[..count]);
            }
        }
        false
    }

    /// `ReadNumberToken`: validasi token angka JSON. Mengembalikan rentang (awal, akhir) di `text`.
    fn read_number_token(&mut self) -> Option<(usize, usize)> {
        self.white();
        let start = self.pos;
        let n = self.text.len();
        if self.pos < n && self.text[self.pos] == b'-' {
            self.pos += 1;
        }
        if self.pos >= n {
            return None;
        }
        if self.text[self.pos] == b'0' {
            self.pos += 1;
            if self.pos < n && self.text[self.pos].is_ascii_digit() {
                return None;
            }
        } else if (b'1'..=b'9').contains(&self.text[self.pos]) {
            while self.pos < n && self.text[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        } else {
            return None;
        }
        if self.pos < n && self.text[self.pos] == b'.' {
            self.pos += 1;
            let digits = self.pos;
            while self.pos < n && self.text[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if digits == self.pos {
                return None;
            }
        }
        if self.pos < n && matches!(self.text[self.pos], b'e' | b'E') {
            self.pos += 1;
            if self.pos < n && matches!(self.text[self.pos], b'+' | b'-') {
                self.pos += 1;
            }
            let digits = self.pos;
            while self.pos < n && self.text[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if digits == self.pos {
                return None;
            }
        }
        Some((start, self.pos))
    }

    /// `ReadInt`: bilangan bulat JSON. Token dengan titik, eksponen, atau panjang >= 32 gagal.
    /// Nilai di luar `int` dijenuhkan, seperti strtol dengan `long` 32-bit di Windows.
    fn read_int(&mut self) -> Option<i32> {
        let (start, finish) = self.read_number_token()?;
        let token = &self.text[start..finish];
        if token.iter().any(|&b| matches!(b, b'.' | b'e' | b'E')) {
            return None;
        }
        if token.is_empty() || token.len() >= 32 {
            return None;
        }
        let (negative, digits) = if token.first() == Some(&b'-') {
            (true, &token[1..])
        } else {
            (false, token)
        };
        let mut magnitude: u64 = 0;
        for &d in digits {
            magnitude = magnitude
                .saturating_mul(10)
                .saturating_add(u64::from(d - b'0'));
        }
        let signed = if negative {
            -(magnitude.min(1u64 << 31) as i64)
        } else {
            magnitude.min(i32::MAX as u64) as i64
        };
        Some(signed as i32)
    }

    /// `ReadDevice`: satu objek perangkat. Mengembalikan (perangkat, apakah platform = "android").
    fn read_device(&mut self) -> Option<(NlJsonDevice, bool)> {
        let mut d = NlJsonDevice::default();
        let mut platform = [0u8; PLATFORM_CAP];
        let mut saw_platform = false;
        if !self.take(b'{') {
            return None;
        }
        self.white();
        if self.take(b'}') {
            return None;
        }
        loop {
            let mut key = [0u8; KEY_CAP];
            if !self.read_string(Some(&mut key[..])) {
                return None;
            }
            if !self.take(b':') {
                return None;
            }
            if c_eq(&key, b"name") {
                if !self.read_string(Some(&mut d.name[..])) {
                    return None;
                }
            } else if c_eq(&key, b"platform") {
                if saw_platform || !self.read_string(Some(&mut platform[..])) {
                    return None;
                }
                saw_platform = true;
            } else if c_eq(&key, b"online") {
                d.online = self.read_bool()?;
            } else if c_eq(&key, b"link") {
                if !self.read_string(Some(&mut d.link[..])) {
                    return None;
                }
            } else if c_eq(&key, b"battery") {
                if !self.read_battery(&mut d) {
                    return None;
                }
            } else if !self.skip_value(0) {
                return None;
            }
            self.white();
            if self.take(b'}') {
                break;
            }
            if !self.take(b',') {
                return None;
            }
            self.white();
        }
        if !saw_platform {
            return None;
        }
        let android = c_eq(&platform, b"android");
        Some((d, android))
    }

    /// `ReadBattery`: `null` berarti tanpa baterai; objek kosong tidak mengubah `has_battery`.
    fn read_battery(&mut self, device: &mut NlJsonDevice) -> bool {
        self.white();
        if self.read_null() {
            device.has_battery = false;
            return true;
        }
        if !self.take(b'{') {
            return false;
        }
        let mut saw_level = false;
        self.white();
        if self.take(b'}') {
            return true;
        }
        loop {
            let mut key = [0u8; BATTERY_KEY_CAP];
            if !self.read_string(Some(&mut key[..])) {
                return false;
            }
            if !self.take(b':') {
                return false;
            }
            if c_eq(&key, b"level") {
                if saw_level {
                    return false;
                }
                let Some(level) = self.read_int() else {
                    return false;
                };
                if !(0..=100).contains(&level) {
                    return false;
                }
                device.battery_level = level;
                saw_level = true;
            } else if c_eq(&key, b"charging") {
                let Some(v) = self.read_bool() else {
                    return false;
                };
                device.charging = v;
            } else if !self.skip_value(0) {
                return false;
            }
            self.white();
            if self.take(b'}') {
                break;
            }
            if !self.take(b',') {
                return false;
            }
            self.white();
        }
        device.has_battery = saw_level;
        true
    }

    /// `ReadDevices`: array perangkat. Perangkat Android dihitung; utama = online (prioritas 2) lalu offline (1), seri ambil yang pertama.
    fn read_devices(&mut self, out: &mut NlJsonDeviceList) -> bool {
        if !self.take(b'[') {
            return false;
        }
        self.white();
        if self.take(b']') {
            return true;
        }
        let mut chosen_priority = 0;
        loop {
            let Some((current, android)) = self.read_device() else {
                return false;
            };
            if android {
                out.android_count += 1;
                let priority = if current.online { 2 } else { 1 };
                if !out.has_android || priority > chosen_priority {
                    out.primary = current;
                    out.has_android = true;
                    chosen_priority = priority;
                }
            }
            self.white();
            if self.take(b']') {
                break;
            }
            if !self.take(b',') {
                return false;
            }
            self.white();
        }
        true
    }

    /// `SkipValue`: lewati satu nilai JSON tanpa menyimpan isinya (kedalaman maksimal 24).
    fn skip_value(&mut self, depth: u32) -> bool {
        if depth > MAX_SKIP_DEPTH {
            return false;
        }
        self.white();
        match self.peek() {
            None => false,
            Some(b'"') => self.read_string(None),
            Some(b'{') => {
                if !self.take(b'{') {
                    return false;
                }
                self.white();
                if self.take(b'}') {
                    return true;
                }
                loop {
                    if !self.read_string(None) || !self.take(b':') || !self.skip_value(depth + 1) {
                        return false;
                    }
                    self.white();
                    if self.take(b'}') {
                        return true;
                    }
                    if !self.take(b',') {
                        return false;
                    }
                }
            }
            Some(b'[') => {
                if !self.take(b'[') {
                    return false;
                }
                self.white();
                if self.take(b']') {
                    return true;
                }
                loop {
                    if !self.skip_value(depth + 1) {
                        return false;
                    }
                    self.white();
                    if self.take(b']') {
                        return true;
                    }
                    if !self.take(b',') {
                        return false;
                    }
                }
            }
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            Some(_) => self.read_number_token().is_some(),
        }
    }

    /// `ReadRootBool`: cari kunci bool di objek akar; kunci lain dilewati. `out` hanya diisi bila berhasil.
    fn read_root_bool(&mut self, wanted: &[u8], out: Option<&mut bool>) -> bool {
        let mut found = false;
        let mut value = false;
        if !self.take(b'{') {
            return false;
        }
        self.white();
        if self.take(b'}') {
            return false;
        }
        loop {
            let mut key = [0u8; ROOT_KEY_CAP];
            if !self.read_string(Some(&mut key[..])) {
                return false;
            }
            if !self.take(b':') {
                return false;
            }
            if c_eq(&key, wanted) {
                if found {
                    return false;
                }
                let Some(v) = self.read_bool() else {
                    return false;
                };
                value = v;
                found = true;
            } else if !self.skip_value(0) {
                return false;
            }
            self.white();
            if self.take(b'}') {
                break;
            }
            if !self.take(b',') {
                return false;
            }
            self.white();
        }
        self.white();
        if !self.at_end() || !found {
            return false;
        }
        if let Some(out) = out {
            *out = value;
        }
        true
    }

    /// `ParseDevices`: objek akar wajib memuat `ok` (true) dan `devices`; tidak boleh ada data sisa.
    fn parse_devices(&mut self, out: &mut NlJsonDeviceList) -> bool {
        let mut result = NlJsonDeviceList::default();
        let mut saw_ok = false;
        let mut ok = false;
        let mut saw_devices = false;
        if !self.take(b'{') {
            return fail_devices(out);
        }
        self.white();
        if self.take(b'}') {
            return fail_devices(out);
        }
        loop {
            let mut key = [0u8; KEY_CAP];
            if !self.read_string(Some(&mut key[..])) {
                return fail_devices(out);
            }
            if !self.take(b':') {
                return fail_devices(out);
            }
            if c_eq(&key, b"ok") {
                if saw_ok {
                    return fail_devices(out);
                }
                let Some(v) = self.read_bool() else {
                    return fail_devices(out);
                };
                ok = v;
                saw_ok = true;
            } else if c_eq(&key, b"devices") {
                if saw_devices || !self.read_devices(&mut result) {
                    return fail_devices(out);
                }
                saw_devices = true;
            } else if !self.skip_value(0) {
                return fail_devices(out);
            }
            self.white();
            if self.take(b'}') {
                break;
            }
            if !self.take(b',') {
                return fail_devices(out);
            }
            self.white();
        }
        self.white();
        if !self.at_end() || !saw_ok || !ok || !saw_devices {
            return fail_devices(out);
        }
        *out = result;
        true
    }

    /// `ParseApiToken`: objek wajib memuat `port` (1024..=65535) dan `token` (64 hex). Token lebih panjang dipotong ke 64 karakter pertama.
    fn parse_api_token(&mut self, out: &mut NlJsonApiToken) -> bool {
        // Token ditulis langsung ke `out` (bukan salinan lokal) agar tidak ada sisa token di stack; gagal -> wipe.
        wipe_token(out);
        let result = &mut *out;
        let mut saw_port = false;
        let mut saw_token = false;
        let mut port: i32 = 0;
        if !self.take(b'{') {
            return fail_token(out);
        }
        self.white();
        if self.take(b'}') {
            return fail_token(out);
        }
        loop {
            let mut key = [0u8; KEY_CAP];
            if !self.read_string(Some(&mut key[..])) {
                return fail_token(out);
            }
            if !self.take(b':') {
                return fail_token(out);
            }
            if c_eq(&key, b"port") {
                if saw_port {
                    return fail_token(out);
                }
                let Some(v) = self.read_int() else {
                    return fail_token(out);
                };
                port = v;
                saw_port = true;
            } else if c_eq(&key, b"token") {
                if saw_token || !self.read_string(Some(&mut result.token[..])) {
                    return fail_token(out);
                }
                saw_token = true;
            } else if !self.skip_value(0) {
                return fail_token(out);
            }
            self.white();
            if self.take(b'}') {
                break;
            }
            if !self.take(b',') {
                return fail_token(out);
            }
            self.white();
        }
        self.white();
        if !self.at_end()
            || !saw_port
            || !saw_token
            || !(1024..=65535).contains(&port)
            || c_str(&result.token).len() != 64
            || !is_hex_token(c_str(&result.token))
        {
            return fail_token(out);
        }
        result.port = port as u16;
        true
    }
}

/// `Fail(NlJsonDeviceList *)` di C: kosongkan `out` lalu kembalikan `false`.
fn fail_devices(out: &mut NlJsonDeviceList) -> bool {
    *out = NlJsonDeviceList::default();
    false
}

/// `Fail(NlJsonApiToken *)` di C: kosongkan `out` secara volatile lalu kembalikan `false`.
fn fail_token(out: &mut NlJsonApiToken) -> bool {
    wipe_token(out);
    false
}

/// `NlJsonParseDevices`: parse daftar perangkat. Gagal bila `json` kosong atau lebih dari 32768 byte; `out` dikosongkan.
pub fn nl_json_parse_devices(json: &[u8], out: &mut NlJsonDeviceList) -> bool {
    if json.is_empty() || json.len() > MAX_DEVICES_JSON {
        return fail_devices(out);
    }
    Parser::new(json).parse_devices(out)
}

/// `NlJsonParseApiToken`: parse port dan token. Gagal bila panjang di luar 1..=4096; `out` dikosongkan secara volatile.
pub fn nl_json_parse_api_token(json: &[u8], out: &mut NlJsonApiToken) -> bool {
    if json.is_empty() || json.len() > MAX_TOKEN_JSON {
        return fail_token(out);
    }
    Parser::new(json).parse_api_token(out)
}

/// `NlJsonReadBool`: baca bool dari kunci di objek akar. `out` tidak disentuh bila gagal.
pub fn nl_json_read_bool(json: &[u8], key: &str, out: Option<&mut bool>) -> bool {
    if json.is_empty() || json.len() > MAX_BOOL_JSON {
        return false;
    }
    Parser::new(json).read_root_bool(c_str(key.as_bytes()), out)
}

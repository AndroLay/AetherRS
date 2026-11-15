//! Port w12fm_logic.h (318 baris): logika murni Win12 Files (tab, pintasan, alamat, argumen, riwayat, geometri, palet, urutan, cakupan).
//! Status: lengkap, tanpa Win32 dan tanpa registry. Tes: tests/fm_logic.rs (port tests\w12fm_logic_test.cpp, 157 baris).

/// Jumlah tab maksimum per jendela (C: W12FM_MAXTABS). usize karena dipakai sebagai panjang array.
pub const W12FM_MAXTABS: usize = 12;

// Kode tombol sama dengan VK_* Windows (C: W12FK_*). Bertipe u32 seperti parameter vk.
pub const W12FK_BACK: u32 = 0x08;
pub const W12FK_TAB: u32 = 0x09;
pub const W12FK_F3: u32 = 0x72;
pub const W12FK_F4: u32 = 0x73;
pub const W12FK_F5: u32 = 0x74;
pub const W12FK_LEFT: u32 = 0x25;
pub const W12FK_UP: u32 = 0x26;
pub const W12FK_RIGHT: u32 = 0x27;

// Kode perintah pintasan (C: W12FA_*).
pub const W12FA_NONE: i32 = 0;
pub const W12FA_BACK: i32 = 1;
pub const W12FA_FORWARD: i32 = 2;
pub const W12FA_UP: i32 = 3;
pub const W12FA_REFRESH: i32 = 4;
pub const W12FA_FOCUS_ADDRESS: i32 = 5;
pub const W12FA_FOCUS_SEARCH: i32 = 6;
pub const W12FA_NEW_TAB: i32 = 7;
pub const W12FA_CLOSE_TAB: i32 = 8;
pub const W12FA_NEXT_TAB: i32 = 9;
pub const W12FA_PREV_TAB: i32 = 10;
pub const W12FA_NEW_WINDOW: i32 = 11;
pub const W12FA_NEW_FOLDER: i32 = 12;
pub const W12FA_TAB_N: i32 = 13;

// Kode tombol bilah perintah (C: W12FC_*). W12FC_COUNT usize karena dipakai sebagai panjang array di files::state.
pub const W12FC_NEWFOLDER: i32 = 0;
pub const W12FC_CUT: i32 = 1;
pub const W12FC_COPY: i32 = 2;
pub const W12FC_PASTE: i32 = 3;
pub const W12FC_RENAME: i32 = 4;
pub const W12FC_DELETE: i32 = 5;
pub const W12FC_SORT: i32 = 6;
pub const W12FC_VIEW: i32 = 7;
pub const W12FC_COUNT: usize = 8;

// Unit UTF-16 yang dipakai fungsi teks dan jalur.
const U_TAB: u16 = b'\t' as u16;
const U_SPACE: u16 = b' ' as u16;
const U_QUOTE: u16 = b'"' as u16;
const U_SLASH: u16 = b'/' as u16;
const U_BACKSLASH: u16 = b'\\' as u16;
const U_COLON: u16 = b':' as u16;

// Teks ke unit UTF-16, berhenti di NUL pertama (meniru wchar_t* yang diakhiri '\0').
fn to_units(s: &str) -> Vec<u16> {
    s.encode_utf16().take_while(|&c| c != 0).collect()
}

// Unit pada indeks i, atau 0 bila di luar batas (meniru terminator '\0' di C).
fn unit_at(s: &[u16], i: usize) -> u16 {
    s.get(i).copied().unwrap_or(0)
}

// Spasi atau tab: pemisah yang dilewati fungsi jalur.
fn is_blank(c: u16) -> bool {
    c == U_SPACE || c == U_TAB
}

// Lipat huruf besar ASCII A-Z ke a-z (C: W12FmFoldAscii). Karakter lain tidak berubah.
pub fn w12_fm_fold_ascii(c: u16) -> u16 {
    if ((b'A' as u16)..=(b'Z' as u16)).contains(&c) {
        c + (b'a' - b'A') as u16
    } else {
        c
    }
}

/// Daftar tab satu jendela (C: W12Tabs). Invarian: 0 <= n <= 12; active < n bila n > 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12Tabs {
    pub n: i32,
    pub active: i32,
    pub id: [i32; W12FM_MAXTABS],
    pub next_id: i32,
}

// Id tab pada indeks i, atau -1 bila di luar batas.
fn tab_id_at(t: &W12Tabs, i: i32) -> i32 {
    if i < 0 {
        return -1;
    }
    t.id.get(i as usize).copied().unwrap_or(-1)
}

/// Tambah tab. after_active: sisip tepat di kanan tab aktif (bila kosong, di awal). Tab baru jadi aktif.
/// Mengembalikan id baru, atau -1 bila sudah penuh.
pub fn w12_tabs_add(t: &mut W12Tabs, after_active: bool) -> i32 {
    if t.n >= W12FM_MAXTABS as i32 {
        return -1;
    }
    let mut pos = t.n;
    if after_active && t.n > 0 {
        pos = t.active + 1;
    }
    // Penjagaan: posisi di luar 0..=n hanya terjadi bila invarian rusak (C tidak terdefinisi), jadi ditolak.
    if pos < 0 || pos > t.n {
        return -1;
    }
    let mut i = t.n;
    while i > pos {
        t.id[i as usize] = t.id[(i - 1) as usize];
        i -= 1;
    }
    t.id[pos as usize] = t.next_id;
    t.n += 1;
    t.active = pos;
    let id = t.next_id;
    // Dibungkus agar tidak panic bila nomor habis (C: overflow int tidak terdefinisi).
    t.next_id = t.next_id.wrapping_add(1);
    id
}

/// Tutup tab index. Tab aktif yang ditutup digantikan tetangga kanan; bila tidak ada, tetangga kiri.
/// Mengembalikan id tab aktif baru, atau -1 bila tidak ada tab lagi (jendela ditutup).
pub fn w12_tabs_close(t: &mut W12Tabs, index: i32) -> i32 {
    if index < 0 || index >= t.n {
        return if t.n > 0 { tab_id_at(t, t.active) } else { -1 };
    }
    let mut i = index;
    while i < t.n - 1 {
        t.id[i as usize] = t.id[(i + 1) as usize];
        i += 1;
    }
    t.n -= 1;
    if t.n == 0 {
        t.active = 0;
        return -1;
    }
    if t.active > index {
        t.active -= 1;
    } else if t.active == index && index >= t.n {
        t.active = t.n - 1;
    }
    tab_id_at(t, t.active)
}

/// Ctrl+Tab (dir = 1) dan Ctrl+Shift+Tab (dir = -1). Berputar di ujung.
pub fn w12_tabs_cycle(t: &mut W12Tabs, dir: i32) {
    if t.n <= 0 {
        return;
    }
    // rem_euclid sama dengan ((a % n) + n) % n di C untuk n > 0.
    t.active = (t.active + dir).rem_euclid(t.n);
}

/// Seret tab dari from ke to. Tab aktif tetap tab yang sama; indeksnya ikut berpindah.
pub fn w12_tabs_move(t: &mut W12Tabs, from: i32, to: i32) {
    if t.n <= 0 || from < 0 || from >= t.n || to < 0 || to >= t.n || from == to {
        return;
    }
    let active_id = tab_id_at(t, t.active);
    let moved = tab_id_at(t, from);
    let (f, g) = (from as usize, to as usize);
    if from < to {
        for i in f..g {
            t.id[i] = t.id[i + 1];
        }
    } else {
        for i in ((g + 1)..=f).rev() {
            t.id[i] = t.id[i - 1];
        }
    }
    t.id[g] = moved;
    for i in 0..t.n {
        if t.id[i as usize] == active_id {
            t.active = i;
            break;
        }
    }
}

/// Memetakan tombol ke perintah Win12 Files. Modifier harus tepat. Backspace naik folder hanya bila fokus tidak di kotak edit.
/// tab_n: 0..8 untuk Ctrl+1..8, -1 untuk Ctrl+9 (tab terakhir). Selain tabel: W12FA_NONE (diteruskan ke DefView).
pub fn w12_fm_accel(vk: u32, ctrl: bool, shift: bool, alt: bool, focus_in_edit: bool, tab_n: Option<&mut i32>) -> i32 {
    let none = !ctrl && !shift && !alt;
    let only_alt = alt && !ctrl && !shift;
    let only_ctrl = ctrl && !shift && !alt;
    let ctrl_shift = ctrl && shift && !alt;
    if only_alt {
        if vk == W12FK_LEFT {
            return W12FA_BACK;
        }
        if vk == W12FK_RIGHT {
            return W12FA_FORWARD;
        }
        if vk == W12FK_UP {
            return W12FA_UP;
        }
        if vk == 'D' as u32 {
            return W12FA_FOCUS_ADDRESS;
        }
        return W12FA_NONE;
    }
    if none {
        if vk == W12FK_BACK {
            return if focus_in_edit { W12FA_NONE } else { W12FA_UP };
        }
        if vk == W12FK_F5 {
            return W12FA_REFRESH;
        }
        if vk == W12FK_F4 {
            return W12FA_FOCUS_ADDRESS;
        }
        if vk == W12FK_F3 {
            return W12FA_FOCUS_SEARCH;
        }
        return W12FA_NONE;
    }
    if ctrl_shift {
        if vk == W12FK_TAB {
            return W12FA_PREV_TAB;
        }
        if vk == 'N' as u32 {
            return W12FA_NEW_FOLDER;
        }
        return W12FA_NONE;
    }
    if only_ctrl {
        if vk == 'L' as u32 {
            return W12FA_FOCUS_ADDRESS;
        }
        if vk == 'F' as u32 || vk == 'E' as u32 {
            return W12FA_FOCUS_SEARCH;
        }
        if vk == 'T' as u32 {
            return W12FA_NEW_TAB;
        }
        if vk == 'W' as u32 || vk == W12FK_F4 {
            return W12FA_CLOSE_TAB;
        }
        if vk == W12FK_TAB {
            return W12FA_NEXT_TAB;
        }
        if vk == 'N' as u32 {
            return W12FA_NEW_WINDOW;
        }
        if (('1' as u32)..=('9' as u32)).contains(&vk) {
            if let Some(n) = tab_n {
                *n = if vk == '9' as u32 { -1 } else { (vk - '1' as u32) as i32 };
            }
            return W12FA_TAB_N;
        }
    }
    W12FA_NONE
}

/// Normalisasi teks alamat ke out (panjang out = cap, termasuk NUL): buang spasi tepi, kutip luar, '/' menjadi '\',
/// lalu buang '\' akhir kecuali akar drive "C:\". Mengembalikan panjang hasil; 0 bila kosong atau tidak muat.
pub fn w12_fm_normalize_address(input: Option<&str>, out: &mut [u16]) -> i32 {
    if out.is_empty() {
        return 0;
    }
    out[0] = 0;
    let src = match input {
        Some(s) => to_units(s),
        None => return 0,
    };
    // Lewati spasi/tab depan, lalu buang spasi/tab belakang.
    let mut start = 0usize;
    while start < src.len() && is_blank(src[start]) {
        start += 1;
    }
    let body = &src[start..];
    let mut len = body.len();
    while len > 0 && is_blank(body[len - 1]) {
        len -= 1;
    }
    if len >= out.len() {
        return 0;
    }
    out[..len].copy_from_slice(&body[..len]);
    out[len] = 0;
    // Buang kutip luar bila berpasangan.
    if len >= 2 && out[0] == U_QUOTE && out[len - 1] == U_QUOTE {
        for i in 0..len - 2 {
            out[i] = out[i + 1];
        }
        len -= 2;
    }
    // Buang spasi/tab yang tersisa di tepi dalam kutip.
    let mut s = 0usize;
    while s < len && is_blank(out[s]) {
        s += 1;
    }
    while len > s && is_blank(out[len - 1]) {
        len -= 1;
    }
    for i in 0..len - s {
        out[i] = out[i + s];
    }
    len -= s;
    // '/' menjadi '\'.
    for c in out[..len].iter_mut() {
        if *c == U_SLASH {
            *c = U_BACKSLASH;
        }
    }
    // Buang '\' akhir, kecuali akar "X:\" (panjang 3 dengan ':' di indeks 1).
    if len > 1 && out[len - 1] == U_BACKSLASH && !(len == 3 && out[1] == U_COLON) {
        len -= 1;
    }
    out[len] = 0;
    len as i32
}

/// Awalan "/select," tanpa membedakan huruf besar-kecil (ASCII). p yang lebih pendek dianggap tidak cocok.
pub fn w12_fm_select_prefix(p: &[u16]) -> bool {
    const K_SEL: &[u8; 8] = b"/select,";
    for (i, &k) in K_SEL.iter().enumerate() {
        if w12_fm_fold_ascii(unit_at(p, i)) != k as u16 {
            return false;
        }
    }
    true
}

/// Argumen baris perintah "<exe> [/select,]<jalur>". Nama exe boleh dalam kutip. Jalur dalam kutip, atau sisa baris.
/// Mengembalikan panjang jalur; 0 bila tidak ada jalur atau tidak muat. select: diisi true bila ada "/select,".
pub fn w12_fm_parse_args(cmd_line: Option<&str>, path: &mut [u16], mut select: Option<&mut bool>) -> i32 {
    if let Some(s) = select.as_deref_mut() {
        *s = false;
    }
    if path.is_empty() {
        return 0;
    }
    path[0] = 0;
    let cmd = match cmd_line {
        Some(s) => to_units(s),
        None => return 0,
    };
    // Lewati nama exe: dalam kutip sampai kutip penutup (ikut dilewati), atau sampai spasi/tab.
    let mut p = 0usize;
    if unit_at(&cmd, p) == U_QUOTE {
        p += 1;
        while unit_at(&cmd, p) != 0 && unit_at(&cmd, p) != U_QUOTE {
            p += 1;
        }
        if unit_at(&cmd, p) == U_QUOTE {
            p += 1;
        }
    } else {
        while unit_at(&cmd, p) != 0 && !is_blank(unit_at(&cmd, p)) {
            p += 1;
        }
    }
    while is_blank(unit_at(&cmd, p)) {
        p += 1;
    }
    // Penanda "/select,": tandai, lalu lewati awalan dan spasi sesudahnya.
    if w12_fm_select_prefix(cmd.get(p..).unwrap_or(&[])) {
        if let Some(s) = select.as_deref_mut() {
            *s = true;
        }
        p += 8;
        while is_blank(unit_at(&cmd, p)) {
            p += 1;
        }
    }
    // Salin jalur. Dalam kutip: apa adanya sampai kutip penutup. Tanpa kutip: sampai akhir, lalu buang spasi/tab belakang.
    let cap = path.len();
    let mut n = 0usize;
    if unit_at(&cmd, p) == U_QUOTE {
        p += 1;
        while unit_at(&cmd, p) != 0 && unit_at(&cmd, p) != U_QUOTE {
            if n >= cap - 1 {
                path[0] = 0;
                return 0;
            }
            path[n] = unit_at(&cmd, p);
            n += 1;
            p += 1;
        }
    } else {
        while unit_at(&cmd, p) != 0 {
            if n >= cap - 1 {
                path[0] = 0;
                return 0;
            }
            path[n] = unit_at(&cmd, p);
            n += 1;
            p += 1;
        }
        while n > 0 && is_blank(path[n - 1]) {
            n -= 1;
        }
    }
    path[n] = 0;
    n as i32
}

/// Riwayat navigasi per tab (maju/mundur). Entri 0..n-1; cur = indeks entri aktif, -1 bila kosong.
/// Pemanggil memegang data entrinya sendiri (C: W12Hist di fm_logic.h baris 188).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct W12Hist {
    pub n: i32,
    pub cur: i32,
}

impl Default for W12Hist {
    /// Sama dengan inisialisasi kosong di C++: n = 0, cur = -1.
    fn default() -> Self {
        W12Hist { n: 0, cur: -1 }
    }
}

/// Entri baru ditaruh sesudah cur; entri maju ikut terbuang. Bila penuh (n == cap), entri terlama dibuang
/// dan mengembalikan 1 agar pemanggil menggeser datanya satu slot ke kiri. Entri baru ada di indeks h.cur.
pub fn w12_hist_push(h: &mut W12Hist, cap: i32) -> i32 {
    let mut n = h.cur + 1;
    let mut dropped = 0;
    if n >= cap {
        dropped = 1;
        n -= 1;
    }
    h.cur = n;
    h.n = n + 1;
    dropped
}

/// Bisa mundur bila cur > 0.
pub fn w12_hist_can_back(h: &W12Hist) -> bool {
    h.cur > 0
}

/// Bisa maju bila masih ada entri sesudah cur.
pub fn w12_hist_can_forward(h: &W12Hist) -> bool {
    h.cur + 1 < h.n
}

/// Mundur satu entri. Mengembalikan indeks baru, atau -1 bila tidak bisa bergerak.
pub fn w12_hist_back(h: &mut W12Hist) -> i32 {
    if w12_hist_can_back(h) {
        h.cur -= 1;
        h.cur
    } else {
        -1
    }
}

/// Maju satu entri. Mengembalikan indeks baru, atau -1 bila tidak bisa bergerak.
pub fn w12_hist_forward(h: &mut W12Hist) -> i32 {
    if w12_hist_can_forward(h) {
        h.cur += 1;
        h.cur
    } else {
        -1
    }
}

/// Lebar satu tab (satuan bebas, mis. piksel): bilah dibagi rata lalu dijepit ke [min_w, max_w].
pub fn w12_fm_tab_width(n: i32, avail: i32, min_w: i32, max_w: i32) -> i32 {
    let mut w = if n > 0 { avail / n } else { max_w };
    if w < min_w {
        w = min_w;
    }
    if w > max_w {
        w = max_w;
    }
    w
}

/// Slot di x: 0..n-1 = tab, n = tombol "+" (tepat sesudah tab terakhir), -1 = tidak ada.
pub fn w12_fm_tab_slot_at(x: i32, n: i32, tab_w: i32, plus_w: i32) -> i32 {
    if x < 0 || tab_w <= 0 {
        return -1;
    }
    let span = n.saturating_mul(tab_w);
    if x < span {
        return x / tab_w;
    }
    if x < span.saturating_add(plus_w) {
        return n;
    }
    -1
}

/// Kotak tutup (x) di tepi kanan tab: [kanan - pad - size, kanan - pad).
pub fn w12_fm_tab_close_hit(x: i32, tab_left: i32, tab_w: i32, size: i32, pad: i32) -> bool {
    let right = tab_left + tab_w - pad;
    x >= right - size && x < right
}

/// Perintah yang butuh seleksi (abu-abu bila kosong): Potong, Salin, Ganti nama, Hapus.
pub fn w12_fm_cmd_needs_selection(cmd: i32) -> bool {
    cmd == W12FC_CUT || cmd == W12FC_COPY || cmd == W12FC_RENAME || cmd == W12FC_DELETE
}

/// Perintah aktif bila ada seleksi, atau bila perintah itu memang tidak butuh seleksi.
pub fn w12_fm_cmd_enabled(cmd: i32, has_selection: bool) -> bool {
    has_selection || !w12_fm_cmd_needs_selection(cmd)
}

/// Pembanding teks tanpa beda huruf besar-kecil (ASCII). Sama bila keduanya berakhir di posisi yang sama.
pub fn w12_fm_word_eq(a: &str, b: &str) -> bool {
    let ua = to_units(a);
    let ub = to_units(b);
    let mut i = 0usize;
    loop {
        let x = w12_fm_fold_ascii(unit_at(&ua, i));
        let y = w12_fm_fold_ascii(unit_at(&ub, i));
        if x != y {
            return false;
        }
        if x == 0 {
            return true;
        }
        i += 1;
    }
}

/// Tema dari settings.ini: "light" = terang; "system" = ikuti Windows (system_light); lainnya (dark, bawaan, kosong) = gelap.
pub fn w12_fm_theme_is_dark(value: Option<&str>, system_light: bool) -> bool {
    match value {
        None => true,
        Some(v) => {
            if w12_fm_word_eq(v, "light") {
                false
            } else if w12_fm_word_eq(v, "system") {
                !system_light
            } else {
                true
            }
        }
    }
}

/// Palet bingkai (0xRRGGBB). hover, press, dim ditetapkan di sini (keputusan penulis header, bukan dari desain).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12FmPalette {
    pub bg: u32,
    pub panel: u32,
    pub text: u32,
    pub hover: u32,
    pub press: u32,
    pub dim: u32,
}

/// Palet gelap (#202020 / #2B2B2B / #FFFFFF) atau terang (#F3F3F3 / #FFFFFF / #1A1A1A).
pub fn w12_fm_palette_for(dark: bool) -> W12FmPalette {
    if dark {
        W12FmPalette { bg: 0x202020, panel: 0x2B2B2B, text: 0xFFFFFF, hover: 0x3A3A3A, press: 0x454545, dim: 0x7A7A7A }
    } else {
        W12FmPalette { bg: 0xF3F3F3, panel: 0xFFFFFF, text: 0x1A1A1A, hover: 0xE5E5E5, press: 0xD6D6D6, dim: 0x9A9A9A }
    }
}

/// Urutan kolom: klik pada kolom yang sedang aktif membalik arah; kolom lain dimulai dengan naik.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12FmSort {
    pub col: i32,
    pub desc: bool,
}

/// Status urutan berikutnya setelah kolom `clicked` diklik.
pub fn w12_fm_sort_next(cur: W12FmSort, clicked: i32) -> W12FmSort {
    let mut s = W12FmSort { col: clicked, desc: false };
    if clicked == cur.col {
        s.desc = !cur.desc;
    }
    s
}

/// Jalur berada DI DALAM folder (turunan; folder itu sendiri tidak termasuk). Tanpa beda huruf (ASCII), batas di '\'.
/// Folder None atau kosong = tanpa filter. Folder berakhiran '\' (mis. "C:\") dianggap induk langsung.
pub fn w12_fm_under_folder(path: &str, folder: Option<&str>) -> bool {
    let f = match folder {
        Some(s) => to_units(s),
        None => return true,
    };
    if f.is_empty() {
        return true;
    }
    let p = to_units(path);
    let flen = f.len();
    if p.len() <= flen {
        return false;
    }
    for i in 0..flen {
        if w12_fm_fold_ascii(p[i]) != w12_fm_fold_ascii(f[i]) {
            return false;
        }
    }
    let ends_sep = f[flen - 1] == U_BACKSLASH;
    ends_sep || p[flen] == U_BACKSLASH
}

/// Argumen explorer.exe untuk pengaman putaran: "<jalur>" atau /select,"<jalur>". Jalur kosong = tanpa argumen.
/// Jalur yang memuat kutip ditolak (false). out harus muat termasuk NUL; bila tidak muat, false.
pub fn w12_fm_explorer_args(path: Option<&str>, select: bool, out: &mut [u16]) -> bool {
    if out.is_empty() {
        return false;
    }
    out[0] = 0;
    let units = match path {
        Some(s) => to_units(s),
        None => return true,
    };
    if units.is_empty() {
        return true;
    }
    if units.contains(&U_QUOTE) {
        return false;
    }
    let mut buf: Vec<u16> = Vec::with_capacity(units.len() + 10);
    if select {
        buf.extend_from_slice(&to_units("/select,\""));
    } else {
        buf.push(U_QUOTE);
    }
    buf.extend_from_slice(&units);
    buf.push(U_QUOTE);
    // Muat bila panjang buf ditambah NUL tidak melebihi out.
    if buf.len() + 1 > out.len() {
        return false;
    }
    out[..buf.len()].copy_from_slice(&buf);
    out[buf.len()] = 0;
    true
}

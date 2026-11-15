//! Logika indeks berkas F1. Sumber: w12files.cpp:1-535, w12files.h:1-38.
//! Status: lengkap (semua fungsi w12files.cpp:1-535 diport, tanpa todo!()). Build, open, close, path, search diisi; tes lulus pada run terakhir (sebelum port bar.rs; belum dijalankan ulang).
//! Penyimpangan: cancel memakai AtomicI32; pointer nullable memakai Option; nama fungsi memakai prefiks w12_.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE};
use windows::Win32::Globalization::{CompareStringOrdinal, CSTR_EQUAL};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindNextFileW,
    GetFileSizeEx, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ,
    FIND_FIRST_EX_LARGE_FETCH, OPEN_EXISTING, WIN32_FIND_DATAW,
};
use windows::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS,
    PAGE_READONLY,
};
use windows::Win32::UI::WindowsAndMessaging::CharLowerBuffW;

use crate::search::{
    w12_search_score, w12_search_top_add, w12_search_top_sort, W12SearchHit, W12SearchQuery,
};

// ---- konstanta (w12files.h, w12files.cpp:11-15) ----

pub const W12FX_MAGIC: u32 = 0x3158_4657;
pub const W12FX_VERSION: u32 = 1;
pub const W12FX_NO_PARENT: u32 = 0xFFFF_FFFF;
pub const W12FX_DIR: u32 = 1;
pub const W12FX_ROOT: u32 = 2;
pub const W12FX_RECENT: u32 = 4;

const FX_PATH_MAX: usize = 32000;
const FX_MAX_ENTRIES: usize = 500_000;
const FX_MAX_CHARS: usize = 32_000_000;
const FX_MAX_DEPTH: usize = 16;
const FX_DAY: u64 = 864_000_000_000;
const FX_SKIP_DIRS: [&str; 7] = [
    "node_modules",
    ".git",
    ".svn",
    ".hg",
    "__pycache__",
    ".venv",
    "$RECYCLE.BIN",
];
const ATTR_HIDDEN: u32 = 0x2;
const ATTR_SYSTEM: u32 = 0x4;
const ATTR_REPARSE_POINT: u32 = 0x400;
const DOT: u16 = b'.' as u16;
const BACKSLASH: u16 = b'\\' as u16;
const COLON: u16 = b':' as u16;
const TILDE: u16 = b'~' as u16;
const DOLLAR: u16 = b'$' as u16;

// ---- struktur file indeks (pack 4, sama dengan C) ----

/// Header file indeks: 32 byte.
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct W12FxHeader {
    pub magic: u32,
    pub version: u32,
    pub count: u32,
    pub chars: u32,
    pub built: u64,
    pub roots: u32,
    pub reserved: u32,
}

/// Satu entri indeks: 24 byte.
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct W12FxEntry {
    pub parent: u32,
    pub name: u32,
    pub lower: u32,
    pub name_len: u16,
    pub flags: u16,
    pub mtime: u64,
}

const _: () = assert!(std::mem::size_of::<W12FxHeader>() == 32);
const _: () = assert!(std::mem::size_of::<W12FxEntry>() == 24);

/// Tampilan file indeks yang dipetakan. Tanpa Drop: tutup dengan w12_fx_close.
pub struct W12FxView {
    pub file: HANDLE,
    pub map: HANDLE,
    pub h: *const W12FxHeader,
    pub e: *const W12FxEntry,
    pub pool: *const u16,
}

/// Statistik pembangunan indeks.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct W12FxBuildStats {
    pub dirs: u32,
    pub files: u32,
    pub skipped: u32,
    pub truncated: bool,
}

// ---- helper string (UTF-16, meniru wchar_t*) ----

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Ubah ke UTF-16 lalu potong pada NUL pertama (perilaku wchar_t* di C).
fn wide_c(s: &str) -> Vec<u16> {
    let mut w = utf16(s);
    if let Some(p) = w.iter().position(|&c| c == 0) {
        w.truncate(p);
    }
    w
}

/// SameText di sumber: panjang sama dan CompareStringOrdinal (abaikan huruf) == CSTR_EQUAL.
fn same_text(a: &[u16], b: &[u16]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    if a.is_empty() {
        return true;
    }
    unsafe { CompareStringOrdinal(a, b, true) == CSTR_EQUAL }
}

fn is_dot_name(name: &[u16]) -> bool {
    matches!(name, [DOT] | [DOT, DOT])
}

/// W12FileSkipEntry (w12files.cpp:27-41) untuk nama UTF-16.
fn skip_entry_w(name: &[u16], attrs: u32, reparse_tag: u32, is_dir: bool) -> bool {
    if name.is_empty() || is_dot_name(name) {
        return true;
    }
    if (attrs & (ATTR_HIDDEN | ATTR_SYSTEM)) != 0 {
        return true;
    }
    if is_dir {
        if (attrs & ATTR_REPARSE_POINT) != 0 && (reparse_tag & 0x2000_0000) != 0 {
            return true;
        }
        return FX_SKIP_DIRS.iter().any(|d| same_text(name, &utf16(d)));
    }
    if name.len() >= 2 && name[0] == TILDE && name[1] == DOLLAR {
        return true;
    }
    name.len() >= 4 && same_text(&name[name.len() - 4..], &utf16(".tmp"))
}

/// W12FileSkipEntry: apakah entri dilewati saat pemindaian.
pub fn w12_file_skip_entry(name: Option<&str>, attrs: u32, reparse_tag: u32, is_dir: bool) -> bool {
    let w = match name {
        Some(s) => wide_c(s),
        None => Vec::new(),
    };
    skip_entry_w(&w, attrs, reparse_tag, is_dir)
}

/// W12FxRankBonus (w12files.cpp:43-52).
pub fn w12_fx_rank_bonus(mtime: u64, now: u64, flags: u32) -> i32 {
    let basis: i32 = if (flags & W12FX_RECENT) != 0 {
        -60
    } else if (flags & W12FX_DIR) != 0 {
        -100
    } else {
        -120
    };
    if mtime >= now {
        return basis + 40;
    }
    let age = now - mtime;
    if age <= FX_DAY {
        basis + 40
    } else if age <= 7 * FX_DAY {
        basis + 25
    } else if age <= 30 * FX_DAY {
        basis + 10
    } else {
        basis
    }
}

/// DedupeLen (w12files.cpp:55-60): panjang tanpa '\' di ujung, minimal 1.
fn dedupe_len(w: &[u16]) -> usize {
    let mut n = w.len();
    while n > 1 && w[n - 1] == BACKSLASH {
        n -= 1;
    }
    n
}

/// WithinRoot (w12files.cpp:63-67).
fn within_root(p: &[u16], pn: usize, q: &[u16], qn: usize) -> bool {
    if pn < qn || !same_text(&p[..qn], &q[..qn]) {
        return false;
    }
    pn == qn || p[qn] == BACKSLASH
}

/// Inti W12FxDedupeRoots (w12files.cpp:69-87): true = akar dipertahankan.
fn dedupe_alive(w: &[Vec<u16>]) -> Vec<bool> {
    let n = w.len();
    let mut alive: Vec<bool> = w.iter().map(|r| !r.is_empty()).collect();
    for i in 0..n {
        if !alive[i] {
            continue;
        }
        let pi = dedupe_len(&w[i]);
        for j in 0..n {
            if j == i || !alive[j] {
                continue;
            }
            let pj = dedupe_len(&w[j]);
            if within_root(&w[i], pi, &w[j], pj) && (pi > pj || j < i) {
                alive[i] = false;
                break;
            }
        }
    }
    alive
}

/// W12FxDedupeRoots: buang akar kosong dan akar yang berada di dalam akar lain. Urutan dipertahankan.
pub fn w12_fx_dedupe_roots(roots: &mut Vec<String>) -> usize {
    let wide: Vec<Vec<u16>> = roots.iter().map(|s| wide_c(s)).collect();
    let alive = dedupe_alive(&wide);
    let mut k = 0usize;
    roots.retain(|_| {
        let keep = alive[k];
        k += 1;
        keep
    });
    roots.len()
}

/// W12FxShortParent (w12files.cpp:89-108): induk relatif terhadap profil; kosong bila tidak cocok.
pub fn w12_fx_short_parent(full_path: Option<&str>, profile: Option<&str>, out: &mut [u16]) {
    if out.is_empty() {
        return;
    }
    out[0] = 0;
    let full = match full_path {
        Some(s) => wide_c(s),
        None => return,
    };
    let cut = match full.iter().rposition(|&c| c == BACKSLASH) {
        Some(p) => p + 1,
        None => return,
    };
    let mut parent_len = cut - 1;
    if parent_len == 2 && full[1] == COLON {
        parent_len = cut;
    }
    let prof = match profile {
        Some(s) => wide_c(s),
        None => Vec::new(),
    };
    let mut pl = prof.len();
    while pl > 1 && prof[pl - 1] == BACKSLASH {
        pl -= 1;
    }
    let mut start = 0usize;
    if pl > 0
        && parent_len >= pl
        && same_text(&full[..pl], &prof[..pl])
        && (parent_len == pl || full[pl] == BACKSLASH)
    {
        start = if parent_len == pl { parent_len } else { pl + 1 };
    }
    let len = parent_len - start;
    if len >= out.len() {
        return;
    }
    out[..len].copy_from_slice(&full[start..start + len]);
    out[len] = 0;
}

// ---- FX: indeks berkas (port w12files.cpp:131-534) ----

const ATTR_DIRECTORY: u32 = 0x10;
const FX_HDR_BYTES: usize = 32;
const FX_ENT_BYTES: usize = 24;
const FX_OPEN_MIN_BYTES: i64 = 32;
const FX_OPEN_MAX_BYTES: i64 = 70_000_032;
const FX_WRITE_CHUNK: usize = 1 << 20;
const FX_STAR: u16 = b'*' as u16;
const FX_LNK: [u16; 4] = [b'.' as u16, b'l' as u16, b'n' as u16, b'k' as u16];

/// Kode hasil internal (FX_OK, FX_FAIL, FX_FULL, FX_CANCEL di C).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FxRc {
    Ok,
    Fail,
    Full,
    Cancel,
}

/// Folder yang menunggu dipindai (antrean BFS).
struct FxDirItem {
    entry: u32,
    path: Vec<u16>,
}

/// Keadaan pembangunan indeks (padanan FxBuilder di C).
struct FxBuilder {
    entries: Vec<W12FxEntry>,
    pool: Vec<u16>,
    root_count: u32,
    queue: VecDeque<FxDirItem>,
    stats: W12FxBuildStats,
}

impl FxBuilder {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            pool: Vec::new(),
            root_count: 0,
            queue: VecDeque::new(),
            stats: W12FxBuildStats::default(),
        }
    }

    /// FxAddEntry (w12files.cpp:131-170): nama + NUL, lalu salinan huruf kecil + NUL.
    fn add_entry(&mut self, parent: u32, name: &[u16], flags: u16, mtime: u64) -> (FxRc, u32) {
        if self.entries.len() >= FX_MAX_ENTRIES {
            return (FxRc::Full, 0);
        }
        let nl = name.len();
        if nl > 0xFFFF {
            return (FxRc::Fail, 0);
        }
        let need = nl + 1;
        if self.pool.len() + 2 * need > FX_MAX_CHARS {
            return (FxRc::Full, 0);
        }
        let off = self.pool.len();
        self.pool.extend_from_slice(name);
        self.pool.push(0);
        let lower = self.pool.len();
        self.pool.extend_from_slice(name);
        self.pool.push(0);
        if nl > 0 {
            // SAFETY: CharLowerBuffW hanya membaca dan menulis di dalam batas slice.
            let _ = unsafe { CharLowerBuffW(&mut self.pool[lower..lower + nl]) };
        }
        let idx = self.entries.len() as u32;
        self.entries.push(W12FxEntry {
            parent,
            name: off as u32,
            lower: lower as u32,
            name_len: nl as u16,
            flags,
            mtime,
        });
        (FxRc::Ok, idx)
    }
}

/// Kondisi batal: nilai bukan nol pada `cancel` (padanan `cancel && *cancel` di C).
fn fx_cancelled(cancel: Option<&AtomicI32>) -> bool {
    cancel.is_some_and(|c| c.load(Ordering::Relaxed) != 0)
}

/// Baca u32 little-endian pada offset `at`; None bila di luar batas.
fn rd_u32(b: &[u8], at: usize) -> Option<u32> {
    let s: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(s))
}

/// Baca u16 little-endian pada offset `at`; None bila di luar batas.
fn rd_u16(b: &[u8], at: usize) -> Option<u16> {
    let s: [u8; 2] = b.get(at..at + 2)?.try_into().ok()?;
    Some(u16::from_le_bytes(s))
}

/// FileTimeToU64 (w12files.cpp:17-20): (high << 32) | low.
fn fx_filetime(low: u32, high: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

/// Waktu sekarang dalam satuan FILETIME (setara GetSystemTimeAsFileTime).
fn fx_now_filetime() -> u64 {
    let ticks = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_nanos() / 100) as u64)
        .unwrap_or(0);
    ticks + 116_444_736_000_000_000
}

/// Unit sampai NUL pertama (setara wcslen pada buffer WIN32_FIND_DATAW).
fn fx_name_units(buf: &[u16]) -> &[u16] {
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    &buf[..n]
}

/// Nama berakhiran ".lnk" tanpa membedakan huruf, dan lebih dari 4 unit (w12files.cpp:284-285).
fn fx_ends_lnk(name: &[u16]) -> bool {
    let nl = name.len();
    if nl <= 4 {
        return false;
    }
    // SAFETY: kedua slice berisi tepat 4 unit.
    unsafe { CompareStringOrdinal(&name[nl - 4..], &FX_LNK, true) == CSTR_EQUAL }
}

/// FxDirPrefix (w12files.cpp:194-208): awalan `\\?\` (atau `\\?\UNC\` untuk UNC), lalu folder, diakhiri `\`.
fn fx_dir_prefix(dir: &[u16]) -> Vec<u16> {
    let (mut out, skip) = if dir.len() >= 2 && dir[0] == BACKSLASH && dir[1] == BACKSLASH {
        (utf16("\\\\?\\UNC\\"), 2)
    } else {
        (utf16("\\\\?\\"), 0)
    };
    out.extend_from_slice(&dir[skip..]);
    if out.last() != Some(&BACKSLASH) {
        out.push(BACKSLASH);
    }
    out
}

/// FxTrimLen (w12files.cpp:211-218): buang `\` di akhir (sisakan minimal 1 unit); "X:\" tetap 3 unit.
fn fx_trim_len(s: &[u16]) -> usize {
    let n = s.len();
    let mut t = n;
    while t > 1 && s[t - 1] == BACKSLASH {
        t -= 1;
    }
    if t == 2 && s[1] == COLON && n > 2 {
        return 3;
    }
    t
}

/// FxChainLen (w12files.cpp:221-226): jumlah entri dari `idx` sampai akar (ikut dihitung), berhenti di 65.
fn fx_chain_len(entries: &[W12FxEntry], idx: u32) -> usize {
    let mut n: usize = 0;
    let mut i = idx;
    while i != W12FX_NO_PARENT && n < 65 {
        n += 1;
        match entries.get(i as usize) {
            Some(e) => i = e.parent,
            None => break,
        }
    }
    n
}

/// Validasi seluruh pemetaan berkas indeks (w12files.cpp:439-452). `b` berisi seluruh berkas.
fn fx_validate(b: &[u8]) -> bool {
    let (Some(magic), Some(version), Some(count), Some(chars)) =
        (rd_u32(b, 0), rd_u32(b, 4), rd_u32(b, 8), rd_u32(b, 12))
    else {
        return false;
    };
    if magic != W12FX_MAGIC || version != W12FX_VERSION {
        return false;
    }
    let count = count as usize;
    let chars = chars as usize;
    if count > FX_MAX_ENTRIES || chars > FX_MAX_CHARS {
        return false;
    }
    let pool = FX_HDR_BYTES + FX_ENT_BYTES * count;
    if b.len() != pool + 2 * chars {
        return false;
    }
    let nul_at = |u: usize| rd_u16(b, pool + 2 * u) == Some(0);
    for i in 0..count {
        let eb = FX_HDR_BYTES + FX_ENT_BYTES * i;
        let (Some(parent), Some(name), Some(lower), Some(nl)) =
            (rd_u32(b, eb), rd_u32(b, eb + 4), rd_u32(b, eb + 8), rd_u16(b, eb + 12))
        else {
            return false;
        };
        if parent != W12FX_NO_PARENT && parent as usize >= i {
            return false;
        }
        let nl = usize::from(nl);
        if nl == 0 {
            return false;
        }
        let (name, lower) = (name as usize, lower as usize);
        if name + nl + 1 > chars || lower + nl + 1 > chars {
            return false;
        }
        if !nul_at(name + nl) || !nul_at(lower + nl) {
            return false;
        }
    }
    true
}

/// Keadaan tampilan kosong (setara W12FxView yang direset di C).
fn fx_view_null() -> W12FxView {
    W12FxView {
        file: HANDLE(std::ptr::null_mut()),
        map: HANDLE(std::ptr::null_mut()),
        h: std::ptr::null(),
        e: std::ptr::null(),
        pool: std::ptr::null(),
    }
}

/// FindFirstFileExW (FindExInfoBasic, FIND_FIRST_EX_LARGE_FETCH; w12files.cpp:237-240). None bila gagal.
fn fx_find_first(pat: &[u16], data: &mut WIN32_FIND_DATAW) -> Option<HANDLE> {
    // SAFETY: pat berakhir NUL dan hidup selama panggilan; data adalah buffer WIN32_FIND_DATAW yang valid.
    unsafe {
        FindFirstFileExW(
            PCWSTR(pat.as_ptr()),
            FindExInfoBasic,
            (data as *mut WIN32_FIND_DATAW).cast::<c_void>(),
            FindExSearchNameMatch,
            None,
            FIND_FIRST_EX_LARGE_FETCH,
        )
        .ok()
    }
}

/// FindNextFileW. false bila tidak ada entri lagi atau gagal.
fn fx_find_next(h: HANDLE, data: &mut WIN32_FIND_DATAW) -> bool {
    // SAFETY: h adalah handle pencarian dari fx_find_first; data adalah buffer yang valid.
    unsafe { FindNextFileW(h, data).is_ok() }
}

/// FindClose untuk handle pencarian.
fn fx_find_close(h: HANDLE) {
    // SAFETY: h adalah handle pencarian yang belum ditutup.
    unsafe {
        let _ = FindClose(h);
    }
}

/// CloseHandle bila handle tidak null.
fn fx_close_handle(h: HANDLE) {
    if h.0.is_null() {
        return;
    }
    // SAFETY: h dibuka oleh pemanggil dan belum ditutup.
    unsafe {
        let _ = CloseHandle(h);
    }
}

/// UnmapViewOfFile untuk alamat dasar hasil MapViewOfFile.
fn fx_unmap(base: *const u8) {
    // SAFETY: base berasal dari MapViewOfFile dan belum di-unmap.
    unsafe {
        let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
            Value: base as *mut c_void,
        });
    }
}

/// FxScanDir (w12files.cpp:229-267): memindai satu folder, menambah entri, dan mengantrikan subfolder.
fn fx_scan_dir(b: &mut FxBuilder, dir: &FxDirItem, cancel: Option<&AtomicI32>) -> FxRc {
    let mut pat = fx_dir_prefix(&dir.path);
    pat.push(FX_STAR);
    pat.push(0);
    // SAFETY: WIN32_FIND_DATAW adalah struct POD tanpa invarian; seluruh bit nol valid.
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let Some(h) = fx_find_first(&pat, &mut data) else {
        b.stats.skipped += 1;
        return FxRc::Ok;
    };
    let mut rc = FxRc::Ok;
    let mut first = true;
    loop {
        if first {
            first = false;
        } else if !fx_find_next(h, &mut data) {
            break;
        }
        if fx_cancelled(cancel) {
            rc = FxRc::Cancel;
            break;
        }
        let name = fx_name_units(&data.cFileName);
        if is_dot_name(name) {
            continue;
        }
        let attrs = data.dwFileAttributes;
        let is_dir = (attrs & ATTR_DIRECTORY) != 0;
        let tag = if (attrs & ATTR_REPARSE_POINT) != 0 {
            data.dwReserved0
        } else {
            0
        };
        if skip_entry_w(name, attrs, tag, is_dir) {
            b.stats.skipped += 1;
            continue;
        }
        let mtime = fx_filetime(
            data.ftLastWriteTime.dwLowDateTime,
            data.ftLastWriteTime.dwHighDateTime,
        );
        let flags = if is_dir { W12FX_DIR as u16 } else { 0 };
        let (r, idx) = b.add_entry(dir.entry, name, flags, mtime);
        if r != FxRc::Ok {
            rc = r;
            break;
        }
        if !is_dir {
            b.stats.files += 1;
            continue;
        }
        b.stats.dirs += 1;
        let sep = usize::from(!dir.path.is_empty() && dir.path[dir.path.len() - 1] != BACKSLASH);
        let cl = dir.path.len() + sep + name.len();
        if cl >= FX_PATH_MAX || fx_chain_len(&b.entries, idx) > FX_MAX_DEPTH {
            b.stats.skipped += 1;
            continue;
        }
        let mut child = Vec::with_capacity(cl);
        child.extend_from_slice(&dir.path);
        if sep == 1 {
            child.push(BACKSLASH);
        }
        child.extend_from_slice(name);
        b.queue.push_back(FxDirItem {
            entry: idx,
            path: child,
        });
    }
    fx_find_close(h);
    rc
}

/// FxScanRecent (w12files.cpp:270-293): memindai `*.lnk` di folder Recent sebagai entri RECENT di bawah akar.
fn fx_scan_recent(
    b: &mut FxBuilder,
    root_idx: u32,
    dir: &[u16],
    cancel: Option<&AtomicI32>,
) -> FxRc {
    let mut pat = fx_dir_prefix(dir);
    pat.push(FX_STAR);
    pat.extend_from_slice(&FX_LNK);
    pat.push(0);
    // SAFETY: WIN32_FIND_DATAW adalah struct POD; seluruh bit nol valid.
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let Some(h) = fx_find_first(&pat, &mut data) else {
        return FxRc::Ok;
    };
    let mut rc = FxRc::Ok;
    let mut first = true;
    loop {
        if first {
            first = false;
        } else if !fx_find_next(h, &mut data) {
            break;
        }
        if fx_cancelled(cancel) {
            rc = FxRc::Cancel;
            break;
        }
        if (data.dwFileAttributes & ATTR_DIRECTORY) != 0 {
            continue;
        }
        let name = fx_name_units(&data.cFileName);
        if !fx_ends_lnk(name) {
            continue;
        }
        let mtime = fx_filetime(
            data.ftLastWriteTime.dwLowDateTime,
            data.ftLastWriteTime.dwHighDateTime,
        );
        let stem = &name[..name.len() - 4];
        let (r, _) = b.add_entry(root_idx, stem, W12FX_RECENT as u16, mtime);
        if r != FxRc::Ok {
            rc = r;
            break;
        }
        b.stats.files += 1;
    }
    fx_find_close(h);
    rc
}

/// FxRunQueue (w12files.cpp:296-306): memproses antrean folder secara FIFO sampai kosong atau gagal.
fn fx_run_queue(b: &mut FxBuilder, cancel: Option<&AtomicI32>) -> FxRc {
    while let Some(item) = b.queue.pop_front() {
        let rc = fx_scan_dir(b, &item, cancel);
        if rc != FxRc::Ok {
            return rc;
        }
    }
    FxRc::Ok
}

/// FxBuildCore (w12files.cpp:309-345): mengisi `b` dari akar, folder Recent, lalu antrean folder.
fn fx_build_core(
    b: &mut FxBuilder,
    roots: &mut Vec<String>,
    recent_dir: Option<&str>,
    cancel: Option<&AtomicI32>,
) -> FxRc {
    if fx_cancelled(cancel) {
        return FxRc::Cancel;
    }
    let _ = w12_fx_dedupe_roots(roots);
    let mut rc = FxRc::Ok;
    for root in roots.iter() {
        let units = utf16(root);
        let len = fx_trim_len(&units);
        if len >= FX_PATH_MAX {
            b.stats.skipped += 1;
            continue;
        }
        let (r, idx) = b.add_entry(
            W12FX_NO_PARENT,
            &units[..len],
            (W12FX_ROOT | W12FX_DIR) as u16,
            0,
        );
        rc = r;
        if rc != FxRc::Ok {
            break;
        }
        b.root_count += 1;
        b.queue.push_back(FxDirItem {
            entry: idx,
            path: units[..len].to_vec(),
        });
    }
    if rc == FxRc::Ok {
        if let Some(rd) = recent_dir.filter(|s| !s.is_empty()) {
            let units = utf16(rd);
            let rl = fx_trim_len(&units);
            if rl >= FX_PATH_MAX {
                b.stats.skipped += 1;
            } else {
                let (r, idx) = b.add_entry(
                    W12FX_NO_PARENT,
                    &units[..rl],
                    (W12FX_ROOT | W12FX_DIR) as u16,
                    0,
                );
                rc = r;
                if rc == FxRc::Ok {
                    b.root_count += 1;
                    rc = fx_scan_recent(b, idx, &units[..rl], cancel);
                }
            }
        }
    }
    if rc == FxRc::Ok {
        rc = fx_run_queue(b, cancel);
    }
    rc
}

/// Pembungkus W12FxBuild (w12files.cpp:385-406): membangun lalu menyimpan indeks ke `out_path`.
fn fx_build_impl(
    b: &mut FxBuilder,
    roots: &mut Vec<String>,
    recent_dir: Option<&str>,
    out_path: &str,
    cancel: Option<&AtomicI32>,
) -> bool {
    if out_path.is_empty() {
        return false;
    }
    match fx_build_core(b, roots, recent_dir, cancel) {
        FxRc::Ok => fx_save(b, out_path),
        FxRc::Full => {
            b.stats.truncated = true;
            fx_save(b, out_path)
        }
        FxRc::Fail | FxRc::Cancel => false,
    }
}

/// Menulis `bytes` ke berkas dalam potongan 1 MB (w12files.cpp:348-360).
fn fx_write_all(file: &mut std::fs::File, bytes: &[u8]) -> bool {
    bytes
        .chunks(FX_WRITE_CHUNK)
        .all(|c| file.write_all(c).is_ok())
}

/// FxSave (w12files.cpp:363-382): menulis header, entri, lalu pool. Bila gagal, berkas dihapus.
fn fx_save(b: &FxBuilder, out_path: &str) -> bool {
    let mut file = match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .share_mode(0)
        .open(out_path)
    {
        Ok(f) => f,
        Err(_) => return false,
    };
    let count = b.entries.len() as u32;
    let chars = b.pool.len() as u32;
    let mut header = [0u8; FX_HDR_BYTES];
    header[0..4].copy_from_slice(&W12FX_MAGIC.to_le_bytes());
    header[4..8].copy_from_slice(&W12FX_VERSION.to_le_bytes());
    header[8..12].copy_from_slice(&count.to_le_bytes());
    header[12..16].copy_from_slice(&chars.to_le_bytes());
    header[16..24].copy_from_slice(&fx_now_filetime().to_le_bytes());
    header[24..28].copy_from_slice(&b.root_count.to_le_bytes());
    header[28..32].copy_from_slice(&0u32.to_le_bytes());
    let mut ok = fx_write_all(&mut file, &header);
    let mut buf: Vec<u8> = Vec::with_capacity(FX_WRITE_CHUNK + FX_ENT_BYTES);
    for e in &b.entries {
        if !ok {
            break;
        }
        buf.extend_from_slice(&e.parent.to_le_bytes());
        buf.extend_from_slice(&e.name.to_le_bytes());
        buf.extend_from_slice(&e.lower.to_le_bytes());
        buf.extend_from_slice(&e.name_len.to_le_bytes());
        buf.extend_from_slice(&e.flags.to_le_bytes());
        buf.extend_from_slice(&e.mtime.to_le_bytes());
        if buf.len() >= FX_WRITE_CHUNK {
            ok = fx_write_all(&mut file, &buf);
            buf.clear();
        }
    }
    if ok && !buf.is_empty() {
        ok = fx_write_all(&mut file, &buf);
    }
    for part in b.pool.chunks(FX_WRITE_CHUNK / 2) {
        if !ok {
            break;
        }
        buf.clear();
        for u in part {
            buf.extend_from_slice(&u.to_le_bytes());
        }
        ok = fx_write_all(&mut file, &buf);
    }
    drop(file);
    if !ok {
        let _ = std::fs::remove_file(out_path);
        return false;
    }
    true
}

/// W12FxBuild (w12files.cpp:385-406): membangun indeks dari akar-akar lalu menyimpannya ke `out_path`.
pub fn w12_fx_build(
    roots: &[Option<&str>],
    recent_dir: Option<&str>,
    out_path: &str,
    cancel: Option<&AtomicI32>,
    st: Option<&mut W12FxBuildStats>,
) -> bool {
    let mut owned: Vec<String> = roots
        .iter()
        .flatten()
        .filter(|r| !r.is_empty())
        .map(|r| (*r).to_string())
        .collect();
    let mut b = FxBuilder::new();
    let ok = fx_build_impl(&mut b, &mut owned, recent_dir, out_path, cancel);
    if let Some(s) = st {
        *s = b.stats;
    }
    ok
}

/// W12FxOpen (w12files.cpp:409-465): membuka dan memetakan indeks baca-saja, lalu memvalidasinya.
pub fn w12_fx_open(path: &str, v: &mut W12FxView) -> bool {
    *v = fx_view_null();
    if path.is_empty() {
        return false;
    }
    let mut wpath = wide_c(path);
    wpath.push(0);
    // SAFETY: wpath berakhir NUL dan hidup selama panggilan.
    let file = match unsafe {
        CreateFileW(
            PCWSTR(wpath.as_ptr()),
            GENERIC_READ.0,
            FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_DELETE.0),
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    } {
        Ok(h) => h,
        Err(_) => return false,
    };
    let mut size: i64 = 0;
    // SAFETY: file adalah handle yang valid; size adalah tujuan i64 yang valid.
    let size_ok = unsafe { GetFileSizeEx(file, &mut size).is_ok() };
    if !size_ok || !(FX_OPEN_MIN_BYTES..=FX_OPEN_MAX_BYTES).contains(&size) {
        fx_close_handle(file);
        return false;
    }
    // SAFETY: file valid; pemetaan baca-saja tanpa nama dan tanpa ukuran eksplisit.
    let map = match unsafe { CreateFileMappingW(file, None, PAGE_READONLY, 0, 0, PCWSTR::null()) } {
        Ok(h) => h,
        Err(_) => {
            fx_close_handle(file);
            return false;
        }
    };
    // SAFETY: map adalah pemetaan yang valid; akses baca penuh.
    let view = unsafe { MapViewOfFile(map, FILE_MAP_READ, 0, 0, 0) };
    let base = view.Value as *const u8;
    if base.is_null() {
        fx_close_handle(map);
        fx_close_handle(file);
        return false;
    }
    let len = size as usize;
    // SAFETY: base memetakan tepat `len` byte baca-saja; slice dipakai sebelum fx_unmap atau sebelum view disimpan.
    let bytes: &[u8] = unsafe { std::slice::from_raw_parts(base, len) };
    let count = match rd_u32(bytes, 8) {
        Some(c) if fx_validate(bytes) => c as usize,
        _ => {
            fx_unmap(base);
            fx_close_handle(map);
            fx_close_handle(file);
            return false;
        }
    };
    // SAFETY: fx_validate memastikan ukuran berkas = header + count*entri + pool, jadi offset ini berada di dalam pemetaan.
    let (entries, pool) = unsafe {
        (
            base.add(FX_HDR_BYTES).cast::<W12FxEntry>(),
            base.add(FX_HDR_BYTES + FX_ENT_BYTES * count).cast::<u16>(),
        )
    };
    v.file = file;
    v.map = map;
    v.h = base.cast::<W12FxHeader>();
    v.e = entries;
    v.pool = pool;
    true
}

/// W12FxClose (w12files.cpp:467-478): melepas pemetaan dan handle, lalu mengosongkan tampilan.
pub fn w12_fx_close(v: &mut W12FxView) {
    if !v.h.is_null() {
        fx_unmap(v.h.cast::<u8>());
    }
    fx_close_handle(v.map);
    fx_close_handle(v.file);
    *v = fx_view_null();
}

/// W12FxPath (w12files.cpp:481-509): menyusun path lengkap entri `idx` dari akar ke daun ke `out`.
/// Mengembalikan panjang dalam unit tanpa NUL; 0 bila gagal atau `out` tidak muat.
pub fn w12_fx_path(v: &W12FxView, idx: u32, out: &mut [u16]) -> i32 {
    if v.h.is_null() || v.e.is_null() || v.pool.is_null() || out.is_empty() {
        return 0;
    }
    // SAFETY: v.h/e/pool menunjuk ke pemetaan hidup yang divalidasi oleh w12_fx_open.
    let count = unsafe { (*v.h).count } as usize;
    if idx as usize >= count {
        return 0;
    }
    let mut chain = [0u32; 64];
    let mut depth = 0usize;
    let mut cur = idx;
    loop {
        if depth == chain.len() {
            out[0] = 0;
            return 0;
        }
        chain[depth] = cur;
        depth += 1;
        // SAFETY: cur < count; induk selalu lebih kecil dari anak (divalidasi saat open).
        let parent = unsafe { (*v.e.add(cur as usize)).parent };
        if parent == W12FX_NO_PARENT {
            break;
        }
        cur = parent;
    }
    let mut pos = 0usize;
    for k in (0..depth).rev() {
        let i = chain[k] as usize;
        // SAFETY: i < count; field dibaca sebagai salinan nilai.
        let (name_len, name_off, flags) = unsafe {
            let p = v.e.add(i);
            ((*p).name_len as usize, (*p).name as usize, (*p).flags)
        };
        let is_recent = (u32::from(flags) & W12FX_RECENT) != 0;
        let sep = usize::from(pos > 0 && out[pos - 1] != BACKSLASH);
        let ext = if is_recent { 4 } else { 0 };
        let need = sep + name_len + ext;
        if need + pos + 1 > out.len() {
            out[0] = 0;
            return 0;
        }
        if sep == 1 {
            out[pos] = BACKSLASH;
            pos += 1;
        }
        // SAFETY: name_off + name_len berada di dalam pool (divalidasi oleh fx_validate); nama asli, bukan lower (w12files.cpp:500).
        let name = unsafe { std::slice::from_raw_parts(v.pool.add(name_off), name_len) };
        out[pos..pos + name_len].copy_from_slice(name);
        pos += name_len;
        if is_recent {
            out[pos..pos + 4].copy_from_slice(&FX_LNK);
            pos += 4;
        }
    }
    out[pos] = 0;
    i32::try_from(pos).unwrap_or(0)
}

/// W12FxSearch (w12files.cpp:512-534): mencari entri yang cocok dengan `q` dan menyimpan hasil teratas ke `out`.
/// Mengembalikan jumlah hit yang terkumpul; `out[..n]` sudah diurutkan.
pub fn w12_fx_search(v: &W12FxView, q: &W12SearchQuery, now: u64, out: &mut [W12SearchHit]) -> i32 {
    if v.h.is_null() || v.e.is_null() || v.pool.is_null() || out.is_empty() || q.length <= 0 {
        return 0;
    }
    // SAFETY: v.h valid (diset oleh w12_fx_open).
    let count = unsafe { (*v.h).count } as usize;
    let mut found: i32 = 0;
    for i in 0..count {
        // SAFETY: i < count; entri dan pool berada di dalam pemetaan.
        let (flags, name_len, lower, mtime) = unsafe {
            let p = v.e.add(i);
            ((*p).flags, (*p).name_len as usize, (*p).lower as usize, (*p).mtime)
        };
        if (u32::from(flags) & W12FX_ROOT) != 0 {
            continue;
        }
        // SAFETY: lower + name_len berada di dalam pool.
        let name = unsafe { std::slice::from_raw_parts(v.pool.add(lower), name_len) };
        let mut mpos: i32 = -1;
        let mut mlen: i32 = 0;
        let s = w12_search_score(name, &[], Some(q), Some(&mut mpos), Some(&mut mlen));
        if s <= 0 {
            continue;
        }
        let score = (s + w12_fx_rank_bonus(mtime, now, u32::from(flags))).max(1);
        let hit = W12SearchHit {
            kind: i32::from(flags),
            reference: i as i32,
            score,
            match_pos: mpos,
            match_len: mlen,
            order: i as i32,
        };
        w12_search_top_add(out, Some(&mut found), Some(&hit));
    }
    let n = usize::try_from(found).unwrap_or(0).min(out.len());
    w12_search_top_sort(&mut out[..n]);
    found
}

//! Sumber: w12filesearch.inc. Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime.
use super::prelude::*;

use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, MutexGuard};

use windows::core::{HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_NOTIFY_ENUM_DIR, HANDLE, WAIT_FAILED,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    SHGetKnownFolderPath, FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music,
    FOLDERID_Pictures, FOLDERID_Videos, KNOWN_FOLDER_FLAG,
};
use crate::files::model::{
    w12_fx_build, w12_fx_close, w12_fx_dedupe_roots, w12_fx_open, w12_fx_path, w12_fx_search,
    w12_fx_short_parent, W12FxBuildStats, W12FxEntry, W12FxView, W12FX_DIR, W12FX_RECENT,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::SystemInformation::{GetSystemTimeAsFileTime, GetTickCount64};
use windows::Win32::System::Threading::{
    CreateEventW, CreateThread, GetCurrentThread, ResetEvent, SetEvent, SetThreadPriority,
    WaitForMultipleObjects, WaitForSingleObject, INFINITE, THREAD_CREATION_FLAGS,
    THREAD_MODE_BACKGROUND_BEGIN,
};

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Graphics::Imaging::IWICBitmapSource;
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, MoveFileExW, MOVEFILE_REPLACE_EXISTING,
};
use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
use windows::Win32::UI::Shell::{
    SHGetFileInfoW, SHGetImageList, SHFILEINFOW, SHGFI_SYSICONINDEX, SHGFI_USEFILEATTRIBUTES,
    SHIL_EXTRALARGE, SHIL_LARGE,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, PostMessageW};

// ---- Konstanta (`enum` di w12filesearch.inc:10-23) ----
const FS_MAXROOTS: usize = 8; // akar pemindaian: folder pengguna + OneDrive
const FS_MAXQ: usize = 63; // kueri dipotong ke panjang ini (unit UTF-16)
const FS_MAXWATCH: usize = 9; // 8 akar + 1 Recent
const FS_RECENT_IDX: usize = 8; // indeks pemantau Recent (tanpa subfolder)
const FS_FOLDER_SLOT: usize = 32; // slot ikon folder
const FS_ICON_SLOTS: usize = 33; // 32 ekstensi + folder
const FS_RECENT_BONUS: i32 = 60; // bonus untuk FILE yang juga ada di Recent
const FS_QUIET_MS: u64 = 30_000; // bangun setelah 30 detik tenang
const FS_MAXDELAY_MS: u64 = 300_000; // paling lambat 5 menit sejak perubahan pertama
const FS_OLDIDX_MS: u64 = 600_000; // indeks lebih tua dari 10 menit: bangun ulang saat mulai
const FS_STARTBUILD_MS: u64 = 60_000; // jeda bangun ulang saat mulai untuk indeks tua
const FS_RETRY_MS: u64 = 300_000; // gagal bangun: coba lagi 5 menit kemudian

/// Hasil kueri untuk UI (`struct FsResult`, :26). Dibuat thread pencari dengan alokasi nol,
/// lalu diserahkan ke UI lewat PostMessageW (lp). Setelah diterima, UI memilikinya.
#[repr(C)]
pub struct FsResult {
    pub r#gen: i32,
    pub n: i32,
    pub hit: [W12SearchHit; FS_MAXHITS as usize],
    pub flags: [u32; FS_MAXHITS as usize],
    pub name: [[u16; 128]; FS_MAXHITS as usize],
    pub sub: [[u16; 160]; FS_MAXHITS as usize],
    pub path: [[u16; FS_PATH as usize]; FS_MAXHITS as usize],
}

/// Satu pemantau folder (`struct FsWatch`, :34). Disimpan di Box agar alamat `ov` dan `buf`
/// stabil selama operasi overlapped menunggu.
struct FsWatch {
    dir: HANDLE,
    ov: OVERLAPPED,
    buf: [u32; 1024], // 4 KB; bila meluap, tetap dianggap berubah
    dummy: u32,
    subtree: bool,
    armed: bool, // ReadDirectoryChangesW sedang menunggu
}

impl FsWatch {
    fn zeroed() -> FsWatch {
        FsWatch {
            dir: fs_null(),
            // SAFETY: OVERLAPPED adalah struct POD; nilai nol valid untuk semua field-nya.
            ov: unsafe { std::mem::zeroed() },
            buf: [0; 1024],
            dummy: 0,
            subtree: false,
            armed: false,
        }
    }

    /// Setara ZeroMemory(&w) di C++.
    fn clear(&mut self) {
        *self = FsWatch::zeroed();
    }
}

const fn fs_null() -> HANDLE {
    HANDLE(std::ptr::null_mut())
}

// ---- Global (`static` di :43-64) ----
// Nilai Copy memakai Cell; String/Vec/COM memakai RefCell (rencana-sisa.md).
// FS_GEN, FS_RUNNING, FS_MAXHITS, FS_PATH, WM_W12_FXHITS, dan MSG sudah ada di state.rs: tidak dideklarasikan ulang.
pub static FS_RES: SyncCell<Cell<*mut FsResult>> = SyncCell::new(Cell::new(std::ptr::null_mut()));
/// Critical section (`g_fsCs`): melindungi FS_GEN, FS_Q, dan FS_QCAP.
static FS_CS: Mutex<()> = Mutex::new(());
static FS_Q: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
static FS_QCAP: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(8));
static FS_THREADS: SyncCell<Cell<[HANDLE; 2]>> = SyncCell::new(Cell::new([fs_null(), fs_null()]));
static FS_QUIT: SyncCell<Cell<HANDLE>> = SyncCell::new(Cell::new(fs_null()));
static FS_QUERY_EV: SyncCell<Cell<HANDLE>> = SyncCell::new(Cell::new(fs_null()));
static FS_CANCEL: AtomicI32 = AtomicI32::new(0);
static FS_SWAP: AtomicI32 = AtomicI32::new(0);
static FS_HAVE_VIEW: AtomicI32 = AtomicI32::new(0);
static FS_BUILDING: AtomicI32 = AtomicI32::new(0);
static FS_IDX_PATH: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
static FS_NEW_PATH: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
static FS_RECENT_DIR: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
static FS_PROFILE: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
/// Akar pemindaian (`g_fsRootBuf` + `g_fsRoots` + `g_fsNRoots`).
static FS_ROOTS: SyncCell<RefCell<Vec<String>>> = SyncCell::new(RefCell::new(Vec::new()));
static FS_WATCH: SyncCell<RefCell<Vec<Box<FsWatch>>>> = SyncCell::new(RefCell::new(Vec::new()));
/// Hanya thread pembangun (FsInitDue dipanggil sebelum thread berjalan).
static FS_DIRTY: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static FS_FIRST_CHG: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
static FS_DUE_AT: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
/// Slot ikon (`g_fsIcon`, `g_fsIconUsed`, `g_fsIconKey`, `g_fsIconPx`). Diisi saat FsStart.
static FS_ICON: SyncCell<RefCell<Vec<Icon>>> = SyncCell::new(RefCell::new(Vec::new()));
static FS_ICON_USED: SyncCell<RefCell<Vec<bool>>> = SyncCell::new(RefCell::new(Vec::new()));
static FS_ICON_KEY: SyncCell<RefCell<Vec<String>>> = SyncCell::new(RefCell::new(Vec::new()));
static FS_ICON_PX: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));

// ---- Pembantu global ----

/// Kunci critical section (`EnterCriticalSection` g_fsCs). Racun (poison) diabaikan agar tidak panik.
fn fs_cs() -> MutexGuard<'static, ()> {
    FS_CS.lock().unwrap_or_else(|e| e.into_inner())
}

fn fs_get(cell: &SyncCell<RefCell<String>>) -> String {
    cell.borrow().clone()
}

fn fs_set(cell: &SyncCell<RefCell<String>>, s: String) {
    *cell.borrow_mut() = s;
}

// ---- Waktu, path, lingkungan (:66-106) ----

/// Waktu monotonik dalam ms (`FsNowMs`, :67).
fn fs_now_ms() -> u64 {
    // SAFETY: GetTickCount64 tidak punya prasyarat dan tidak menulis memori pemanggil.
    unsafe { GetTickCount64() }
}

/// Waktu dinding dalam satuan FILETIME, UTC (`FsNowFt`, :68).
fn fs_now_ft() -> u64 {
    // SAFETY: GetSystemTimeAsFileTime tidak punya prasyarat; nilainya hanya dibaca.
    let ft = unsafe { GetSystemTimeAsFileTime() };
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

/// Panjang dalam unit UTF-16, yang dibandingkan dengan kapasitas FS_PATH.
fn fs_len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Gabung a + b (`FsCat`, :86). None bila hasilnya tidak muat di FS_PATH unit UTF-16 (C++: false).
/// `FsCopy` (:76) memakai fungsi ini dengan b kosong.
fn fs_cat(a: &str, b: &str) -> Option<String> {
    let s = format!("{a}{b}");
    (fs_len16(&s) < FS_PATH as usize).then_some(s)
}

/// Potong s ke paling banyak `max` unit UTF-16 tanpa memecah pasangan surrogate (`FsCopy` untuk kueri, :76).
fn fs_trunc16(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut n = 0usize;
    for c in s.chars() {
        let w = c.len_utf16();
        if n + w > max {
            break;
        }
        n += w;
        out.push(c);
    }
    out
}

/// Nilai variabel lingkungan (`FsEnv`, :102). Kosong atau tidak muat di FS_PATH dianggap gagal.
fn fs_env(name: &str) -> Option<String> {
    let v = std::env::var_os(name)?.into_string().ok()?;
    (!v.is_empty() && fs_len16(&v) < FS_PATH as usize).then_some(v)
}

// ---- Pemantau folder (:108-149) ----

/// Pasang ulang pemantau (`FsWatchArm`, :112). TRUE berarti permintaan DIANTREKAN, bukan selesai.
/// Jangan SetEvent di sini: lihat komentar C++ :108-111 (penumpukan IRP sampai RAM habis).
fn fs_watch_arm(w: &mut FsWatch) -> bool {
    if w.dir.0.is_null() {
        return false;
    }
    let filter = FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME;
    let buf_len = std::mem::size_of_val(&w.buf) as u32;
    let buf_ptr = w.buf.as_mut_ptr() as *mut c_void;
    let dummy_ptr: *mut u32 = &mut w.dummy;
    let ov_ptr: *mut OVERLAPPED = &mut w.ov;
    // SAFETY: dir adalah handel folder yang valid. buf, dummy, dan ov hidup selama operasi: FsWatch
    // ada di Box, dan operasi dibatalkan lalu ditunggu di fs_watch_close sebelum dibebaskan.
    let r = unsafe {
        let _ = ResetEvent(w.ov.hEvent);
        ReadDirectoryChangesW(
            w.dir,
            buf_ptr,
            buf_len,
            w.subtree,
            filter,
            Some(dummy_ptr),
            Some(ov_ptr),
            None,
        )
    };
    match r {
        Ok(()) => {
            w.armed = true;
            true
        }
        Err(e) if e.code() == HRESULT::from_win32(ERROR_IO_PENDING.0) => {
            w.armed = true;
            true
        }
        Err(_) => {
            w.armed = false;
            false
        }
    }
}

/// Tutup pemantau (`FsWatchClose`, :125). Urutan CancelIoEx, GetOverlappedResult(wait=TRUE),
/// CloseHandle(dir), lalu CloseHandle(ov.hEvent) meniru C++ persis.
fn fs_watch_close(w: &mut FsWatch) {
    if !w.dir.0.is_null() && w.armed {
        // Batalkan operasi yang menunggu, lalu tunggu sampai kernel benar-benar selesai menulis ke buf/ov.
        // Tanpa menunggu, kernel bisa masih menulis ke memori ini setelah handel ditutup.
        // SAFETY: dir dan ov adalah handel serta OVERLAPPED yang dipasang fs_watch_arm.
        unsafe {
            let _ = CancelIoEx(w.dir, Some(&w.ov as *const OVERLAPPED));
            let mut got: u32 = 0;
            let _ = GetOverlappedResult(w.dir, &w.ov, &mut got, true);
        }
    }
    if !w.dir.0.is_null() {
        // SAFETY: dir adalah handel yang dibuka CreateFileW dan belum ditutup.
        unsafe {
            let _ = CloseHandle(w.dir);
        }
    }
    if !w.ov.hEvent.0.is_null() {
        // SAFETY: hEvent adalah handel event yang dibuat CreateEventW dan belum ditutup.
        unsafe {
            let _ = CloseHandle(w.ov.hEvent);
        }
    }
    w.clear();
}

/// Buka folder untuk dipantau (`FsWatchOpen`, :138). Gagal bila folder tidak ada atau pemantau tidak bisa dipasang.
fn fs_watch_open(w: &mut FsWatch, path: &str, subtree: bool) -> bool {
    w.clear();
    w.subtree = subtree;
    // SAFETY: CreateEventW tanpa atribut keamanan dan tanpa nama; hasilnya diperiksa.
    w.ov.hEvent = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
        Ok(h) => h,
        Err(_) => return false,
    };
    let wpath = to_wide_z(path);
    // SAFETY: wpath berakhir NUL dan hidup selama panggilan.
    let dir = unsafe {
        CreateFileW(
            pcw(&wpath),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0),
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OVERLAPPED.0),
            None,
        )
    };
    match dir {
        Ok(h) => w.dir = h,
        Err(_) => {
            fs_watch_close(w);
            return false;
        }
    }
    if !fs_watch_arm(w) {
        fs_watch_close(w);
        return false;
    }
    true
}

// ---- Jatuh tempo indeks (:159-192) ----

/// Jatuh tempo awal (`FsInitDue`, :160). Indeks belum ada: bangun segera. Indeks lebih tua dari 10 menit:
/// bangun setelah FS_STARTBUILD_MS. Selain itu dianggap segar.
fn fs_init_due() {
    let now = fs_now_ms();
    let idx = fs_get(&FS_IDX_PATH);
    // Setara GetFileAttributesExW: waktu tulis terakhir berkas indeks.
    let mtime = std::fs::metadata(&idx).ok().and_then(|m| m.modified().ok());
    let Some(mtime) = mtime else {
        FS_DIRTY.set(true);
        FS_FIRST_CHG.set(now);
        FS_DUE_AT.set(now);
        return;
    };
    // Waktu tulis di masa depan dihitung usia 0, seperti `nowFt > ft ? ... : 0` di C++.
    let age_ms = std::time::SystemTime::now()
        .duration_since(mtime)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    if age_ms > FS_OLDIDX_MS {
        FS_DIRTY.set(true);
        FS_FIRST_CHG.set(now);
        FS_DUE_AT.set(now + FS_STARTBUILD_MS);
    } else {
        FS_DIRTY.set(false);
    }
}

/// Catat perubahan pada waktu `now` (`FsMarkDirty`, :183). Jatuh tempo = min(tenang 30 detik, perubahan pertama + 5 menit).
fn fs_mark_dirty(now: u64) {
    if !FS_DIRTY.get() {
        FS_DIRTY.set(true);
        FS_FIRST_CHG.set(now);
    }
    let quiet = now + FS_QUIET_MS;
    let cap = FS_FIRST_CHG.get() + FS_MAXDELAY_MS;
    FS_DUE_AT.set(quiet.min(cap));
}

/// Pengganti HasOverlappedIoCompleted (makro winbase.h): Internal != STATUS_PENDING (0x103).
fn fs_overlapped_done(ov: &OVERLAPPED) -> bool {
    ov.Internal != 0x103
}

/// Memeriksa pemantau yang sudah selesai (`FsPollWatchers`, :195). Tiap perubahan menandai indeks kotor,
/// lalu pemantau dipasang ulang.
fn fs_poll_watchers() {
    let mut watches = FS_WATCH.borrow_mut();
    for slot in watches.iter_mut() {
        let w: &mut FsWatch = slot;
        if !w.armed || w.dir.0.is_null() {
            continue;
        }
        let ev = w.ov.hEvent;
        // SAFETY: ev adalah event pemantau yang dibuat fs_watch_open.
        if unsafe { WaitForSingleObject(ev, 0) }.0 != WAIT_OBJECT_0.0 {
            continue;
        }
        if !fs_overlapped_done(&w.ov) {
            // Event tersinyal tetapi I/O belum selesai: jangan pasang ulang (IRP akan menumpuk).
            // SAFETY: ev adalah handel event yang valid.
            let _ = unsafe { ResetEvent(ev) };
            if !fs_overlapped_done(&w.ov) {
                continue;
            }
        }
        let mut got: u32 = 0;
        // SAFETY: dir dan ov adalah operasi overlapped yang dipasang fs_watch_arm; wait = FALSE.
        let code = unsafe { GetOverlappedResult(w.dir, &w.ov, &mut got, false) }
            .err()
            .map(|e| e.code());
        if code == Some(HRESULT::from_win32(ERROR_IO_INCOMPLETE.0)) {
            continue; // pertahanan tambahan: masih menunggu
        }
        w.armed = false;
        // Isi buffer diabaikan: selesai (atau meluap) berarti ada perubahan.
        fs_mark_dirty(fs_now_ms());
        let galat = matches!(code, Some(c) if c != HRESULT::from_win32(ERROR_NOTIFY_ENUM_DIR.0));
        if galat {
            fs_watch_close(w); // galat lain: lepas, jangan berputar
            continue;
        }
        if !fs_watch_arm(w) {
            fs_watch_close(w); // gagal pasang ulang: lepas pemantau ini saja
        }
    }
}

/// Membangun indeks ke files.idx.new (hanya thread pembangun, `FsBuildIndex`, :217).
/// Bila berhasil, pencari diminta menukar indeks.
fn fs_build_index() {
    fs_poll_watchers(); // pemantau dipasang ulang dulu: perubahan selama pembangunan tertangkap putaran berikutnya
    FS_DIRTY.set(false);
    FS_BUILDING.store(1, Ordering::SeqCst);
    let mut st = W12FxBuildStats::default();
    let roots = FS_ROOTS.borrow().clone();
    let refs: Vec<Option<&str>> = roots.iter().map(|s| Some(s.as_str())).collect();
    let recent = fs_get(&FS_RECENT_DIR);
    let recent_opt = (!recent.is_empty()).then_some(recent.as_str());
    let new_path = fs_get(&FS_NEW_PATH);
    let ok = w12_fx_build(&refs, recent_opt, &new_path, Some(&FS_CANCEL), Some(&mut st));
    FS_BUILDING.store(0, Ordering::SeqCst);
    if ok {
        FS_SWAP.store(1, Ordering::SeqCst);
        // SAFETY: FS_QUERY_EV dibuat fs_start; SetEvent hanya membangunkan thread pencari.
        let _ = unsafe { SetEvent(FS_QUERY_EV.get()) };
    } else if FS_CANCEL.load(Ordering::SeqCst) == 0 {
        // Gagal (bukan dibatalkan): coba lagi nanti.
        let now = fs_now_ms();
        FS_DIRTY.set(true);
        FS_FIRST_CHG.set(now);
        FS_DUE_AT.set(now + FS_RETRY_MS);
    }
}

/// Thread pembangun (`FsBuildThread`, :238). Menunggu jatuh tempo atau perubahan, lalu membangun indeks.
/// Keluar saat FS_QUIT diset.
unsafe extern "system" fn fs_build_thread(_param: *mut c_void) -> u32 {
    // SAFETY: GetCurrentThread mengembalikan pseudo-handle; SetThreadPriority hanya mengubah thread ini.
    let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) };
    loop {
        let quit = FS_QUIT.get();
        // SAFETY: quit adalah event manual-reset yang dibuat fs_start.
        if unsafe { WaitForSingleObject(quit, 0) }.0 == WAIT_OBJECT_0.0 {
            break;
        }
        let now = fs_now_ms();
        if FS_DIRTY.get() && now >= FS_DUE_AT.get() {
            fs_build_index();
            continue;
        }
        // Susun daftar handel tanpa memegang borrow FS_WATCH saat menunggu. Indeks 0 selalu quit.
        let mut hs: Vec<HANDLE> = vec![quit];
        {
            let watches = FS_WATCH.borrow();
            for slot in watches.iter() {
                if slot.armed && !slot.dir.0.is_null() {
                    hs.push(slot.ov.hEvent);
                }
            }
        }
        let wait = if FS_DIRTY.get() {
            FS_DUE_AT.get().wrapping_sub(now) as u32
        } else {
            INFINITE
        };
        // SAFETY: hs berisi handel event yang hidup selama thread berjalan.
        let r = unsafe { WaitForMultipleObjects(&hs, false, wait) }.0;
        if r == WAIT_OBJECT_0.0 || r == WAIT_FAILED.0 {
            break;
        }
        if r == WAIT_TIMEOUT.0 {
            continue;
        }
        fs_poll_watchers();
    }
    0
}

/// Membuang hasil terakhir milik UI (`FsClearHits`, :263). Hanya dipanggil dari thread UI.
pub fn fs_clear_hits() {
    let p = FS_RES.get();
    FS_RES.set(std::ptr::null_mut());
    if !p.is_null() {
        // SAFETY: p dibuat fs_alloc_result dengan alloc_zeroed(Layout::new::<FsResult>()) dan
        // kepemilikannya ada di sini; Box::from_raw membebaskannya dengan layout yang sama.
        drop(unsafe { Box::from_raw(p) });
    }
}

/// Mengirim kueri ke thread pencari (`FsQuery`, :270). Tiap pemanggilan menaikkan generasi;
/// kueri kosong membuang hasil lama.
pub fn fs_query(q: &str, cap: i32) {
    if !FS_RUNNING.get() {
        return;
    }
    let cap = cap.clamp(1, FS_MAXHITS);
    {
        let _cs = fs_cs();
        FS_GEN.set(FS_GEN.get() + 1);
        fs_set(&FS_Q, fs_trunc16(q, FS_MAXQ));
        FS_QCAP.set(cap);
    }
    if q.is_empty() {
        fs_clear_hits();
    }
    // SAFETY: FS_QUERY_EV dibuat fs_start; SetEvent hanya membangunkan thread pencari.
    let _ = unsafe { SetEvent(FS_QUERY_EV.get()) };
}

/// true bila indeks belum bisa dicari atau sedang dibangun (`FsIndexing`, :285).
pub fn fs_indexing() -> bool {
    FS_HAVE_VIEW.load(Ordering::SeqCst) == 0 || FS_BUILDING.load(Ordering::SeqCst) != 0
}

/// Menentukan files.idx dan files.idx.new di %LOCALAPPDATA%\Win12Shell (`FsInitPaths`, :291).
/// Folder dibuat bila belum ada.
fn fs_init_paths() -> bool {
    let Some(lad) = fs_env("LOCALAPPDATA") else {
        return false;
    };
    let Some(dir) = fs_cat(&lad, "\\Win12Shell") else {
        return false;
    };
    let _ = std::fs::create_dir(&dir); // gagal bila sudah ada: diabaikan
    match (fs_cat(&dir, "\\files.idx"), fs_cat(&dir, "\\files.idx.new")) {
        (Some(idx), Some(new)) => {
            fs_set(&FS_IDX_PATH, idx);
            fs_set(&FS_NEW_PATH, new);
            true
        }
        _ => false,
    }
}

/// Membaca PWSTR milik COM menjadi String, lalu membebaskannya dengan CoTaskMemFree.
fn fs_take_pwstr(p: PWSTR) -> String {
    // SAFETY: p adalah string wide berakhir NUL yang dikembalikan SHGetKnownFolderPath.
    let s = unsafe { p.to_string() }.unwrap_or_default();
    // SAFETY: p dialokasikan COM dan dibebaskan tepat sekali di sini.
    unsafe { CoTaskMemFree(Some(p.0 as *const c_void)) };
    s
}

/// Mengisi akar pemindaian, profil, dan folder Recent (`FsRoots`, :302). Dipanggil dari fs_start
/// sebelum thread berjalan.
fn fs_roots() {
    let folders = [
        &FOLDERID_Desktop,
        &FOLDERID_Documents,
        &FOLDERID_Downloads,
        &FOLDERID_Pictures,
        &FOLDERID_Music,
        &FOLDERID_Videos,
    ];
    let mut roots: Vec<String> = Vec::new();
    for rfid in folders {
        // SAFETY: rfid menunjuk GUID konstanta; dwflags 0; token None (token pengguna saat ini).
        if let Ok(p) = unsafe { SHGetKnownFolderPath(rfid, KNOWN_FOLDER_FLAG(0), None) } {
            let s = fs_take_pwstr(p);
            // FsCopy gagal bila terpotong: entri seperti itu tidak dimasukkan.
            if fs_len16(&s) < FS_PATH as usize {
                roots.push(s);
            }
        }
    }
    if roots.len() < FS_MAXROOTS {
        if let Some(od) = fs_env("OneDrive") {
            roots.push(od);
        }
    }
    let n = w12_fx_dedupe_roots(&mut roots);
    roots.truncate(n);
    *FS_ROOTS.borrow_mut() = roots;
    fs_set(&FS_PROFILE, fs_env("USERPROFILE").unwrap_or_default());
    let recent = fs_env("APPDATA")
        .and_then(|ad| fs_cat(&ad, "\\Microsoft\\Windows\\Recent"))
        .unwrap_or_default();
    fs_set(&FS_RECENT_DIR, recent);
}

/// Menyiapkan 33 slot ikon bila belum ada. C++ memakai global nol; di Rust slot dibuat saat start.
fn fs_icon_ensure() {
    {
        let mut v = FS_ICON.borrow_mut();
        if v.len() != FS_ICON_SLOTS {
            v.clear();
            v.resize_with(FS_ICON_SLOTS, Icon::default);
        }
    }
    {
        let mut u = FS_ICON_USED.borrow_mut();
        if u.len() != FS_ICON_SLOTS {
            u.clear();
            u.resize(FS_ICON_SLOTS, false);
        }
    }
    {
        let mut k = FS_ICON_KEY.borrow_mut();
        if k.len() != FS_ICON_SLOTS {
            k.clear();
            k.resize(FS_ICON_SLOTS, String::new());
        }
    }
}

/// Menyalakan indeks file (`FsStart`, :327): jalur, akar, pemantau, lalu dua thread. Idempoten.
/// Bila gagal membuat handel atau thread, fs_stop dipanggil.
pub fn fs_start() {
    if FS_RUNNING.get() {
        return;
    }
    if !fs_init_paths() {
        return;
    }
    fs_roots();
    FS_CANCEL.store(0, Ordering::SeqCst);
    FS_SWAP.store(0, Ordering::SeqCst);
    FS_HAVE_VIEW.store(0, Ordering::SeqCst);
    FS_BUILDING.store(0, Ordering::SeqCst);
    // Sebelum handel dibuat: fs_stop bisa membersihkan bila langkah berikutnya gagal.
    FS_RUNNING.set(true);
    // SAFETY: CreateEventW tanpa atribut keamanan dan tanpa nama; kegagalan dijadikan handel nol.
    let quit = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.unwrap_or(fs_null());
    let qev = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.unwrap_or(fs_null());
    FS_QUIT.set(quit);
    FS_QUERY_EV.set(qev);
    if quit.0.is_null() || qev.0.is_null() {
        fs_stop();
        return;
    }
    fs_icon_ensure();
    {
        let mut watches = FS_WATCH.borrow_mut();
        if watches.len() != FS_MAXWATCH {
            watches.clear();
            watches.resize_with(FS_MAXWATCH, || Box::new(FsWatch::zeroed()));
        }
        let roots = FS_ROOTS.borrow();
        for (i, root) in roots.iter().enumerate() {
            fs_watch_open(&mut watches[i], root, true);
        }
        let recent = fs_get(&FS_RECENT_DIR);
        if !recent.is_empty() {
            fs_watch_open(&mut watches[FS_RECENT_IDX], &recent, false);
        }
    }
    fs_init_due();
    // SAFETY: fungsi thread punya signature LPTHREAD_START_ROUTINE; lpparameter None.
    let th_build = unsafe {
        CreateThread(None, 0, Some(fs_build_thread), None, THREAD_CREATION_FLAGS(0), None)
    }
    .ok();
    let th_search = unsafe {
        CreateThread(None, 0, Some(fs_search_thread), None, THREAD_CREATION_FLAGS(0), None)
    }
    .ok();
    FS_THREADS.set([th_build.unwrap_or(fs_null()), th_search.unwrap_or(fs_null())]);
    if th_build.is_none() || th_search.is_none() {
        fs_stop();
    }
}

/// Menghentikan indeks (`FsStop`, :353): batalkan pembangunan, minta thread keluar, lalu lepas sumbernya.
/// Dipanggil dari thread UI. Bila thread belum keluar dalam 2 detik, sumber tidak dilepas.
pub fn fs_stop() {
    if !FS_RUNNING.get() {
        return;
    }
    FS_CANCEL.store(1, Ordering::SeqCst);
    let quit = FS_QUIT.get();
    if !quit.0.is_null() {
        // SAFETY: quit adalah event yang dibuat fs_start.
        let _ = unsafe { SetEvent(quit) };
    }
    {
        let _cs = fs_cs();
        FS_GEN.set(FS_GEN.get() + 1); // hasil yang masih terbang menjadi basi
    }
    let threads = FS_THREADS.get();
    let mut alive = false;
    for th in threads {
        if !th.0.is_null() {
            // SAFETY: th adalah handel thread yang dibuat fs_start.
            if unsafe { WaitForSingleObject(th, 2000) }.0 != WAIT_OBJECT_0.0 {
                alive = true;
            }
        }
    }
    if alive {
        return; // thread belum keluar: sumbernya masih dipakai, jangan dilepas sekarang
    }
    for th in threads {
        if !th.0.is_null() {
            // SAFETY: th adalah handel thread yang belum ditutup.
            let _ = unsafe { CloseHandle(th) };
        }
    }
    FS_THREADS.set([fs_null(); 2]);
    {
        let mut watches = FS_WATCH.borrow_mut();
        for slot in watches.iter_mut() {
            fs_watch_close(slot);
        }
    }
    let qev = FS_QUERY_EV.get();
    for h in [quit, qev] {
        if !h.0.is_null() {
            // SAFETY: h adalah handel event yang dibuat fs_start.
            let _ = unsafe { CloseHandle(h) };
        }
    }
    FS_QUIT.set(fs_null());
    FS_QUERY_EV.set(fs_null());
    // DeleteCriticalSection tidak diperlukan: FS_CS adalah Mutex statis.
    fs_icon_free_all();
    fs_clear_hits();
    FS_RUNNING.set(false);
}

// ---- Bagian 3: pencarian dan penerimaan hasil (w12filesearch.inc:76-83, 381-534) ----

/// FsCopy (:76-83): salin sumber ke dst sampai cap-1 unit lalu NUL. false bila sumber terpotong.
fn fs_copy(dst: &mut [u16], src: &[u16]) -> bool {
    if dst.is_empty() {
        return false;
    }
    let cap = dst.len() - 1;
    let end = src.iter().position(|&c| c == 0).unwrap_or(src.len());
    let n = end.min(cap);
    dst[..n].copy_from_slice(&src[..n]);
    dst[n] = 0;
    end <= cap
}

/// Alokasi FsResult bernilai nol (`malloc` + `memset` di C++). Null bila alokasi gagal.
fn fs_alloc_result() -> *mut FsResult {
    // SAFETY: FsResult hanya berisi bilangan bulat dan array bilangan bulat, sehingga pola bit nol valid.
    unsafe { std::alloc::alloc_zeroed(std::alloc::Layout::new::<FsResult>()) as *mut FsResult }
}

/// Membebaskan FsResult dari fs_alloc_result (`free`, :391, :395, :500). Null diabaikan.
fn fs_free_result(p: *mut FsResult) {
    if !p.is_null() {
        // SAFETY: p dialokasikan fs_alloc_result dengan layout FsResult dan belum dibebaskan.
        drop(unsafe { Box::from_raw(p) });
    }
}

/// Tampilan indeks kosong (`W12FxView v = {}` di C++, :507).
fn fs_view_empty() -> W12FxView {
    W12FxView {
        file: fs_null(),
        map: fs_null(),
        h: std::ptr::null(),
        e: std::ptr::null(),
        pool: std::ptr::null(),
    }
}

/// Entri ke-i pada tampilan (`v->e[i]`). i berasal dari hasil pencarian indeks yang sedang terbuka.
fn fs_entry(v: &W12FxView, i: u32) -> W12FxEntry {
    // SAFETY: v.e menunjuk tabel entri selama view terbuka, dan i selalu di dalam batas tabel.
    unsafe { *v.e.add(i as usize) }
}

/// Potongan pool sepanjang len unit mulai off (`v->pool + off`).
fn fs_pool(v: &W12FxView, off: u32, len: usize) -> &[u16] {
    // SAFETY: nama dan bentuk huruf kecilnya disimpan di pool (model.rs:349-353), sehingga off + len
    // berada di dalam pool yang dipetakan selama view terbuka.
    unsafe { std::slice::from_raw_parts(v.pool.add(off as usize), len) }
}

fn fs_flags_of(v: &W12FxView, idx: i32) -> u32 {
    u32::from(fs_entry(v, idx as u32).flags)
}

/// FsIsFile (:412): FILE biasa, bukan folder dan bukan entri Recent.
fn fs_is_file(flags: u32) -> bool {
    (flags & (W12FX_DIR | W12FX_RECENT)) == 0
}

/// FsSameName (:399): dua entri dengan nama huruf kecil yang sama.
fn fs_same_name(v: &W12FxView, a: u32, b: u32) -> bool {
    let ea = fs_entry(v, a);
    let eb = fs_entry(v, b);
    if ea.name_len != eb.name_len {
        return false;
    }
    let n = usize::from(ea.name_len);
    fs_pool(v, ea.lower, n) == fs_pool(v, eb.lower, n)
}

/// FsBuildHits (:418): menyusun baris hasil untuk UI (nama, subjudul, jalur, flags).
fn fs_build_hits(v: &W12FxView, h: &[W12SearchHit], res: &mut FsResult) {
    let profile = fs_get(&FS_PROFILE);
    for (row, hit) in h.iter().enumerate() {
        let idx = hit.reference as u32;
        let e = fs_entry(v, idx);
        if w12_fx_path(v, idx, &mut res.path[row]) == 0 {
            res.path[row][0] = 0;
        }
        let name = fs_pool(v, e.name, usize::from(e.name_len));
        fs_copy(&mut res.name[row], name);
        if (u32::from(e.flags) & W12FX_RECENT) != 0 {
            fs_copy(&mut res.sub[row], &to_wide_z("Baru dibuka"));
        } else if res.path[row][0] != 0 {
            let full = from_wide(&res.path[row]);
            w12_fx_short_parent(Some(full.as_str()), Some(profile.as_str()), &mut res.sub[row]);
        } else {
            res.sub[row][0] = 0;
        }
        res.flags[row] = u32::from(e.flags);
        res.hit[row] = *hit;
        // ref kini indeks baris di FsResult, bukan indeks entri
        res.hit[row].reference = row as i32;
    }
}

/// FsRankInto (:441): FILE yang namanya sama dengan entri Recent naik FS_RECENT_BONUS; entri Recent
/// yang pasangan FILE-nya ada di hasil dibuang. Hasil diurutkan score turun, lalu order naik.
/// Mengembalikan jumlah hasil yang tersisa.
fn fs_rank_into(v: &W12FxView, h: &mut [W12SearchHit]) -> usize {
    let n = h.len();
    for i in 0..n {
        let ai = h[i].reference as u32;
        if !fs_is_file(fs_flags_of(v, h[i].reference)) {
            continue;
        }
        for j in 0..n {
            let bj = h[j].reference as u32;
            if (fs_flags_of(v, h[j].reference) & W12FX_RECENT) != 0 && fs_same_name(v, ai, bj) {
                h[i].score += FS_RECENT_BONUS;
                break;
            }
        }
    }
    // FS_MAXHITS (20) adalah batas tmp di fs_search_once, sehingga n tidak pernah melebihi ukuran ini.
    let mut dropped = [false; FS_MAXHITS as usize];
    for i in 0..n {
        let ai = h[i].reference as u32;
        if (fs_flags_of(v, h[i].reference) & W12FX_RECENT) == 0 {
            continue;
        }
        for j in 0..n {
            if dropped[i] {
                break;
            }
            let bj = h[j].reference as u32;
            if fs_is_file(fs_flags_of(v, h[j].reference)) && fs_same_name(v, ai, bj) {
                dropped[i] = true;
            }
        }
    }
    let mut m = 0usize;
    for i in 0..n {
        if !dropped[i] {
            h[m] = h[i]; // m selalu <= i, jadi aman ditulis di tempat
            m += 1;
        }
    }
    // Insertion sort: score turun, lalu order naik (sama dengan C++ :466-474).
    for i in 1..m {
        let x = h[i];
        let mut j = i;
        while j > 0
            && (h[j - 1].score < x.score || (h[j - 1].score == x.score && h[j - 1].order > x.order))
        {
            h[j] = h[j - 1];
            j -= 1;
        }
        h[j] = x;
    }
    m
}

/// FsSearchOnce (:480): menjalankan satu kueri pada indeks terbuka dan mengirim hasil ke UI lewat
/// PostMessage WM_W12_FXHITS. Hasil basi (generasi sudah berganti) dibuang di sini.
fn fs_search_once(v: &W12FxView, q: &str, cap: i32, gen: i32) {
    let mut sq = W12SearchQuery::default();
    w12_search_make_query(q, &mut sq);
    let mut tmp = [W12SearchHit::default(); FS_MAXHITS as usize];
    let mut n = 0usize;
    if sq.length > 0 {
        let found = w12_fx_search(v, &sq, fs_now_ft(), &mut tmp);
        let found = (found.max(0) as usize).min(tmp.len());
        n = fs_rank_into(v, &mut tmp[..found]);
        n = n.min(cap.max(0) as usize);
    }
    let res = fs_alloc_result();
    if res.is_null() {
        return;
    }
    // SAFETY: res adalah FsResult nol yang baru dialokasikan dan hanya dipegang di sini
    // sampai PostMessageW berhasil (kepemilikan lalu berpindah ke UI lewat LPARAM).
    let r = unsafe { &mut *res };
    r.r#gen = gen;
    r.n = n as i32;
    fs_build_hits(v, &tmp[..n], r);
    let fresh = {
        let _cs = fs_cs();
        FS_GEN.get() == gen
    };
    let posted = if fresh {
        // SAFETY: MSG adalah jendela UI yang dibuat shell; LPARAM berisi pointer yang baru dibuat di atas.
        let sent = unsafe {
            PostMessageW(Some(MSG.get()), WM_W12_FXHITS, WPARAM(gen as usize), LPARAM(res as isize))
        };
        sent.is_ok()
    } else {
        false
    };
    if !posted {
        fs_free_result(res);
    }
}

/// Thread pencari (`FsSearchThread`, :505): membuka indeks, menukarnya bila pembangun sudah siap,
/// lalu menjawab kueri terbaru. Keluar saat FS_QUIT diset (atau WAIT gagal).
unsafe extern "system" fn fs_search_thread(_param: *mut c_void) -> u32 {
    let idx = fs_get(&FS_IDX_PATH);
    let mut v = fs_view_empty();
    let mut open = w12_fx_open(&idx, &mut v);
    FS_HAVE_VIEW.store(i32::from(open), Ordering::SeqCst);
    loop {
        let hs = [FS_QUIT.get(), FS_QUERY_EV.get()];
        // SAFETY: hs berisi dua event yang dibuat fs_start; fs_stop menutupnya hanya setelah thread ini keluar.
        let wait = unsafe { WaitForMultipleObjects(&hs, false, INFINITE) };
        if wait.0 != WAIT_OBJECT_0.0 + 1 {
            break;
        }
        if FS_SWAP.compare_exchange(1, 0, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            if open {
                w12_fx_close(&mut v);
            }
            FS_HAVE_VIEW.store(0, Ordering::SeqCst);
            let new_path = to_wide_z(&fs_get(&FS_NEW_PATH));
            let idx_wide = to_wide_z(&idx);
            // SAFETY: kedua buffer hidup sampai panggilan selesai. Kegagalan dibiarkan seperti C++ (:517):
            // files.idx.new ditinggal.
            let _ = unsafe {
                MoveFileExW(pcw(&new_path), pcw(&idx_wide), MOVEFILE_REPLACE_EXISTING)
            };
            open = w12_fx_open(&idx, &mut v);
            FS_HAVE_VIEW.store(i32::from(open), Ordering::SeqCst);
        }
        let (q, cap, gen) = {
            let _cs = fs_cs();
            (fs_get(&FS_Q), FS_QCAP.get(), FS_GEN.get())
        };
        // C++ memeriksa qbuf[0] (:527): kueri kosong atau berawal NUL tidak dicari.
        if open && !q.is_empty() && !q.starts_with('\0') {
            fs_search_once(&v, &q, cap, gen);
        }
    }
    if open {
        w12_fx_close(&mut v);
    }
    FS_HAVE_VIEW.store(0, Ordering::SeqCst);
    0
}

/// FsAcceptHits (:381): dipanggil UI pada WM_W12_FXHITS. true bila hasil milik kueri terbaru dan
/// menggantikan FS_RES; selain itu hasil dibebaskan.
///
/// # Safety
/// `res` harus berasal dari fs_search_once (lewat LPARAM WM_W12_FXHITS) dan belum dibebaskan.
pub unsafe fn fs_accept_hits(gen: i32, res: *mut FsResult) -> bool {
    if res.is_null() {
        return false;
    }
    let fresh = if FS_RUNNING.get() {
        let _cs = fs_cs();
        FS_GEN.get() == gen
    } else {
        false
    };
    if !fresh {
        fs_free_result(res);
        return false;
    }
    let old = FS_RES.get();
    FS_RES.set(res);
    fs_free_result(old);
    true
}

// ---- Bagian 4: kunci dan ikon hasil (w12filesearch.inc:534-634) ----

/// ASCII A-Z menjadi a-z. Pengganti `_wcsicmp` untuk ".lnk" (:593) dan kunci ekstensi (:547).
fn fs_ascii_lower(c: u16) -> u16 {
    if (u16::from(b'A')..=u16::from(b'Z')).contains(&c) {
        c + 32
    } else {
        c
    }
}

/// FsExtKey (:536): kunci ikon = ekstensi setelah titik terakhir (separator mereset), huruf A-Z jadi a-z,
/// maksimal 15 unit. Kosong bila tanpa ekstensi.
fn fs_ext_key(path: &[u16]) -> String {
    let mut dot: Option<usize> = None;
    for (i, &c) in path.iter().enumerate() {
        if c == u16::from(b'\\') || c == u16::from(b'/') {
            dot = None; // titik sebelum separator bukan ekstensi
        } else if c == u16::from(b'.') {
            dot = Some(i);
        }
    }
    let mut key: Vec<u16> = Vec::new();
    if let Some(d) = dot {
        for &c in path[d + 1..].iter().take(15) {
            if c == 0 {
                break; // C++ berhenti di NUL (:545)
            }
            key.push(fs_ascii_lower(c));
        }
    }
    String::from_utf16_lossy(&key)
}

/// FsKeyEq (:555): dua kunci sama bila isinya sama.
fn fs_key_eq(a: &str, b: &str) -> bool {
    a == b
}

/// FsIconSrc (:562): sumber ikon sistem untuk ekstensi (atau folder) pada ukuran px. Memakai atribut
/// saja, sehingga berkas tidak perlu ada. px <= 32 memakai daftar ikon 32 px, selain itu 48 px.
fn fs_icon_src(path: &str, folder: bool, px: i32) -> Option<IWICBitmapSource> {
    // SAFETY: SHFILEINFOW hanya berisi bilangan bulat dan array, sehingga pola bit nol valid (`= {}` di C++).
    let mut sfi: SHFILEINFOW = unsafe { std::mem::zeroed() };
    let wide = to_wide_z(path);
    let attr = if folder { FILE_ATTRIBUTE_DIRECTORY } else { FILE_ATTRIBUTE_NORMAL };
    // SAFETY: wide hidup sampai panggilan selesai; sfi adalah buffer keluaran yang valid.
    let got = unsafe {
        SHGetFileInfoW(
            pcw(&wide),
            attr,
            Some(&mut sfi as *mut SHFILEINFOW),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_SYSICONINDEX | SHGFI_USEFILEATTRIBUTES,
        )
    };
    if got == 0 {
        return None;
    }
    let list: i32 = if px <= 32 { SHIL_LARGE as i32 } else { SHIL_EXTRALARGE as i32 };
    // SAFETY: SHGetImageList mengembalikan IImageList yang sudah di-AddRef; Drop melepasnya (C++ :576 Release).
    let il: IImageList = unsafe { SHGetImageList::<IImageList>(list) }.ok()?;
    // SAFETY: il valid; sfi.iIcon diisi SHGetFileInfoW di atas.
    let hi = unsafe { il.GetIcon(sfi.iIcon, ILD_TRANSPARENT.0) }.ok()?;
    if hi.0.is_null() {
        return None;
    }
    let src = icon_from_hicon(hi, px);
    // SAFETY: hi dibuat GetIcon dan tidak dipakai lagi; IconFromHICON tidak menghancurkannya (C++ :573-574).
    let _ = unsafe { DestroyIcon(hi) };
    src
}

/// FsIcon (:583): ikon untuk satu baris hasil. Folder memakai FS_FOLDER_SLOT; file memakai slot per
/// ekstensi (32 slot). Entri Recent memakai ekstensi targetnya (".lnk" dibuang). None bila tak ada ikon
/// atau slot penuh; pemanggil lalu menggambar ikon generik. Hanya dipanggil dari thread UI.
/// Deviasi: mengembalikan clone IWICBitmapSource (AddRef), bukan pointer ke cache `Icon*`.
pub fn fs_icon(path: &str, flags: u32, px: i32) -> Option<IWICBitmapSource> {
    fs_icon_ensure();
    if px != FS_ICON_PX.get() {
        fs_icon_free_all();
        FS_ICON_PX.set(px);
    }
    // FsCopy(buf, FS_PATH, path) gagal bila sumber tidak muat (:590): cap-1 unit maksimum.
    if fs_len16(path) >= FS_PATH as usize {
        return None;
    }
    let mut units: Vec<u16> = path.encode_utf16().collect();
    if (flags & W12FX_RECENT) != 0 && units.len() >= 4 {
        let lnk: Vec<u16> = ".lnk".encode_utf16().collect();
        let start = units.len() - 4;
        let is_lnk = units[start..]
            .iter()
            .zip(lnk.iter())
            .all(|(&a, &b)| fs_ascii_lower(a) == b);
        if is_lnk {
            units.truncate(start);
        }
    }
    let full = String::from_utf16_lossy(&units);
    if (flags & W12FX_DIR) != 0 {
        let mut icon = FS_ICON.borrow_mut();
        let mut used = FS_ICON_USED.borrow_mut();
        let mut keys = FS_ICON_KEY.borrow_mut();
        if !used[FS_FOLDER_SLOT] {
            used[FS_FOLDER_SLOT] = true;
            keys[FS_FOLDER_SLOT].clear();
            icon[FS_FOLDER_SLOT] = Icon::default();
            icon[FS_FOLDER_SLOT].src = fs_icon_src(&full, true, px);
        }
        return icon[FS_FOLDER_SLOT].src.clone();
    }
    let key = fs_ext_key(&units);
    let mut icon = FS_ICON.borrow_mut();
    let mut used = FS_ICON_USED.borrow_mut();
    let mut keys = FS_ICON_KEY.borrow_mut();
    for i in 0..FS_FOLDER_SLOT {
        if used[i] && fs_key_eq(&keys[i], &key) {
            // kegagalan ikon ikut di-cache agar tidak dicoba ulang (:610)
            return icon[i].src.clone();
        }
    }
    for i in 0..FS_FOLDER_SLOT {
        if used[i] {
            continue;
        }
        used[i] = true;
        keys[i] = key.clone(); // FsCopy(g_fsIconKey[i], 16, key) (:616): key <= 15 unit
        icon[i] = Icon::default();
        icon[i].src = fs_icon_src(&full, false, px);
        return icon[i].src.clone();
    }
    None // slot ekstensi penuh: pakai ikon generik (:621)
}

/// FsIconFreeAll (:625): melepas semua ikon cache, saat fs_stop atau ukuran ikon berubah.
fn fs_icon_free_all() {
    let mut icon = FS_ICON.borrow_mut();
    let mut used = FS_ICON_USED.borrow_mut();
    let mut keys = FS_ICON_KEY.borrow_mut();
    let n = icon.len().min(used.len()).min(keys.len());
    for i in 0..n {
        if used[i] {
            icon_free(&mut icon[i]);
        }
        icon[i] = Icon::default();
        used[i] = false;
        keys[i].clear();
    }
    FS_ICON_PX.set(0);
}

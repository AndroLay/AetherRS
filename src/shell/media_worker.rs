//! Pekerja media GSMTC di thread MTA sendiri.
//!
//! Pengganti polling async lewat slot vtable di thread UI (core_c2 `media_tick` lama). Build 2026-11-15 10:52
//! terbukti tidak pernah mencatat `media: metadata berubah` walau Spotify diputar dan GSMTC OS melaporkannya
//! (probe PowerShell 15:10: sesi Spotify status 4, judul, artis, sampul, timeline ada).
//!
//! Desain:
//! - Thread `w12-media` (MTA) memakai API bertipe aman `IAsyncOperation::get()`.
//! - Event CurrentSessionChanged, SessionsChanged, PlaybackInfoChanged, MediaPropertiesChanged, dan
//!   TimelinePropertiesChanged hanya menyalakan bit di `DIRTY` lalu membangunkan thread (SetEvent).
//! - Hasil dikirim sebagai snapshot (`MediaSnap`) ke thread UI lewat WM_W12_MEDIAEV (digabung: satu pesan
//!   sampai UI mengambil snapshot). Sampul dikirim sebagai byte (didekode WIC di thread UI).
//! - Perintah (sebelumnya, putar/jeda, berikutnya, seek) diantre dari UI dan dijalankan di thread ini.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::Mutex;

use windows::core::{IUnknown, Interface, PCWSTR};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as Props,
};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, IStream, COINIT_MULTITHREADED};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::Win32::System::WinRT::CreateStreamOverRandomAccessStream;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use super::state::WM_W12_MEDIAEV;
use crate::log::log;

/// Keadaan media yang dikirim ke thread UI. Waktu dalam satuan 100 ns (TimeSpan/DateTime WinRT).
#[derive(Clone, Default, PartialEq)]
pub struct MediaSnap {
    pub has: bool,
    /// GSMTC PlaybackStatus: 0 Closed, 1 Opened, 2 Changing, 3 Stopped, 4 Playing, 5 Paused.
    pub status: i32,
    pub app: String,
    pub title: String,
    pub artist: String,
    /// Naik setiap sampul baru dibaca; 0 = tidak ada sampul.
    pub art_seq: u32,
    pub tl_start: i64,
    pub tl_end: i64,
    pub tl_pos: i64,
    /// LastUpdatedTime (FILETIME UTC, 100 ns sejak 1601).
    pub tl_upd: i64,
    pub can_prev: bool,
    pub can_next: bool,
    pub can_seek: bool,
}

/// Perintah dari UI.
#[derive(Clone, Copy)]
pub enum Cmd {
    Prev,
    Toggle,
    Next,
    /// Fraksi 0..1 dari panjang lagu.
    Seek(f32),
}

const D_SESSION: u32 = 1;
const D_PLAY: u32 = 2;
const D_PROPS: u32 = 4;
const D_TL: u32 = 8;
const D_CMD: u32 = 16;

/// Batas catatan per proses agar boot.log tidak membengkak (lagu berganti terus selama Spotify diputar).
const LOG_MAX: u32 = 400;
/// Sampul lebih besar dari ini diabaikan.
const ART_MAX: u64 = 8 << 20;

static RUNNING: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static DIRTY: AtomicU32 = AtomicU32::new(0);
static WAKE: AtomicIsize = AtomicIsize::new(0);
static HWND_MSG: AtomicIsize = AtomicIsize::new(0);
/// Snapshot baru belum diambil UI (pesan WM_W12_MEDIAEV sudah dikirim).
static NEW: AtomicBool = AtomicBool::new(false);
static LOGS: AtomicU32 = AtomicU32::new(0);

static SNAP: Mutex<Option<MediaSnap>> = Mutex::new(None);
static ART: Mutex<Option<(u32, Vec<u8>)>> = Mutex::new(None);
static CMDS: Mutex<VecDeque<Cmd>> = Mutex::new(VecDeque::new());

fn mlog(msg: &str) {
    let n = LOGS.fetch_add(1, Ordering::Relaxed);
    if n < LOG_MAX {
        log(msg);
    } else if n == LOG_MAX {
        log("media: batas catatan tercapai");
    }
}

fn wake(bits: u32) {
    DIRTY.fetch_or(bits, Ordering::SeqCst);
    let h = WAKE.load(Ordering::SeqCst);
    if h != 0 {
        // SAFETY: h adalah event dari CreateEventW yang tidak pernah ditutup selama proses hidup.
        let _ = unsafe { SetEvent(HANDLE(h as *mut c_void)) };
    }
}

/// Menyalakan pekerja bila belum berjalan. `true` bila pekerja berjalan setelah panggilan ini.
pub fn start(msg: HWND) -> bool {
    if msg.0.is_null() {
        return false;
    }
    HWND_MSG.store(msg.0 as isize, Ordering::SeqCst);
    if RUNNING.swap(true, Ordering::SeqCst) {
        return true;
    }
    STOP.store(false, Ordering::SeqCst);
    if WAKE.load(Ordering::SeqCst) == 0 {
        // SAFETY: event auto-reset tanpa nama; handle disimpan untuk seluruh umur proses.
        match unsafe { CreateEventW(None, false, false, PCWSTR::null()) } {
            Ok(h) => WAKE.store(h.0 as isize, Ordering::SeqCst),
            Err(e) => {
                mlog(&format!("media: CreateEventW gagal {:?}", e.code()));
                RUNNING.store(false, Ordering::SeqCst);
                return false;
            }
        }
    }
    let spawned = std::thread::Builder::new()
        .name("w12-media".into())
        .stack_size(512 * 1024)
        .spawn(worker_main);
    if spawned.is_err() {
        mlog("media: thread pekerja gagal dibuat");
        RUNNING.store(false, Ordering::SeqCst);
        return false;
    }
    true
}

/// Meminta pekerja berhenti (tidak menunggu). Event dilepas oleh pekerja sebelum keluar.
pub fn stop() {
    STOP.store(true, Ordering::SeqCst);
    wake(0);
}

/// Mengantre perintah media.
pub fn send(c: Cmd) {
    if let Ok(mut q) = CMDS.lock() {
        if q.len() < 16 {
            q.push_back(c);
        }
    }
    wake(D_CMD);
}

/// Mengambil snapshot terbaru bila ada yang belum diambil.
pub fn take() -> Option<MediaSnap> {
    if !NEW.swap(false, Ordering::SeqCst) {
        return None;
    }
    SNAP.lock().ok().and_then(|g| g.clone())
}

/// Byte sampul untuk `seq` (None bila sudah diganti sampul yang lebih baru).
pub fn take_art(seq: u32) -> Option<Vec<u8>> {
    let g = ART.lock().ok()?;
    match g.as_ref() {
        Some((s, b)) if *s == seq => Some(b.clone()),
        _ => None,
    }
}

fn post() {
    let h = HWND_MSG.load(Ordering::SeqCst);
    if h == 0 {
        return;
    }
    // SAFETY: PostMessageW hanya memakai handle jendela pesan milik thread UI; gagal bila jendela sudah hilang.
    let _ = unsafe { PostMessageW(Some(HWND(h as *mut c_void)), WM_W12_MEDIAEV, WPARAM(0), LPARAM(0)) };
}

fn now_ms() -> u64 {
    // SAFETY: tanpa argumen; hanya membaca penghitung waktu sistem.
    unsafe { GetTickCount64() }
}

fn worker_main() {
    // SAFETY: inisialisasi COM untuk thread ini saja; dipasangkan dengan CoUninitialize di bawah.
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    mlog(&format!("media: pekerja GSMTC mulai (MTA hr=0x{:08X})", hr.0 as u32));
    let mut w = Worker::default();
    w.run();
    w.unhook_all();
    mlog("media: pekerja GSMTC berhenti");
    // SAFETY: pasangan CoInitializeEx di awal fungsi ini.
    unsafe { CoUninitialize() };
    RUNNING.store(false, Ordering::SeqCst);
}

#[derive(Default)]
struct Worker {
    mgr: Option<Manager>,
    tok_cur: i64,
    tok_list: i64,
    sess: Option<Session>,
    sess_key: usize,
    tok_play: i64,
    tok_props: i64,
    tok_tl: i64,
    snap: MediaSnap,
    /// Judul|artis yang sampulnya sudah dicoba dibaca.
    art_key: String,
    art_seq: u32,
    last_props: u64,
    last_refresh: u64,
    mgr_fail_logged: bool,
}

/// Jeda minimum antar-penyegaran yang hanya dipicu event garis waktu saat notch tersembunyi (2026-11-19).
/// Peramban mengirim TimelinePropertiesChanged berkali-kali per detik; tiap penyegaran = beberapa panggilan RPC.
/// UI mengekstrapolasi posisi dari tl_pos + tl_upd, jadi pembaruan jarang tetap akurat.
const TL_IDLE_MS: u64 = 4000;
static TL_HOT: AtomicBool = AtomicBool::new(false);

/// Dipanggil UI saat notch tampil/tersembunyi: tampil = event garis waktu langsung diproses (lirik, bar progres).
pub fn set_tl_hot(hot: bool) {
    if TL_HOT.swap(hot, Ordering::SeqCst) != hot && hot {
        wake(D_TL);
    }
}

impl Worker {
    fn wait(&self, ms: u32) {
        let h = WAKE.load(Ordering::SeqCst);
        if h == 0 {
            std::thread::sleep(std::time::Duration::from_millis(ms as u64));
            return;
        }
        // SAFETY: h adalah event yang valid selama proses hidup.
        let _ = unsafe { WaitForSingleObject(HANDLE(h as *mut c_void), ms) };
    }

    fn run(&mut self) {
        loop {
            if STOP.load(Ordering::SeqCst) {
                break;
            }
            if self.mgr.is_none() {
                if !self.open_manager() {
                    self.wait(5000);
                    continue;
                }
                DIRTY.fetch_or(D_SESSION | D_PROPS, Ordering::SeqCst);
            }
            let bits = DIRTY.swap(0, Ordering::SeqCst);
            if bits == D_TL && !TL_HOT.load(Ordering::SeqCst) {
                let since = now_ms().wrapping_sub(self.last_refresh);
                if since < TL_IDLE_MS {
                    DIRTY.fetch_or(D_TL, Ordering::SeqCst); // tetap tertunda; event lain membangunkan lebih awal
                    self.wait((TL_IDLE_MS - since) as u32);
                    continue;
                }
            }
            self.last_refresh = now_ms();
            self.refresh(bits);
            if STOP.load(Ordering::SeqCst) {
                break;
            }
            // Event adalah pemicu utama; batas waktu hanya jaring pengaman bila event terlewat.
            let timeout = if self.snap.status == 4 { 4000 } else { 20000 };
            self.wait(timeout);
        }
    }

    fn open_manager(&mut self) -> bool {
        let m = match Manager::RequestAsync().and_then(|op| op.get()) {
            Ok(m) => m,
            Err(e) => {
                if !self.mgr_fail_logged {
                    mlog(&format!("media: RequestAsync gagal hr=0x{:08X}; coba lagi tiap 5 dtk", e.code().0 as u32));
                    self.mgr_fail_logged = true;
                }
                return false;
            }
        };
        self.tok_cur = m
            .CurrentSessionChanged(&TypedEventHandler::new(|_, _| {
                wake(D_SESSION);
                Ok(())
            }))
            .unwrap_or(0);
        self.tok_list = m
            .SessionsChanged(&TypedEventHandler::new(|_, _| {
                wake(D_SESSION);
                Ok(())
            }))
            .unwrap_or(0);
        mlog(&format!(
            "media: manager GSMTC siap (event sesi={} daftar={})",
            self.tok_cur != 0,
            self.tok_list != 0
        ));
        self.mgr = Some(m);
        true
    }

    fn unhook_sess(&mut self) {
        if let Some(s) = self.sess.take() {
            if self.tok_play != 0 {
                let _ = s.RemovePlaybackInfoChanged(self.tok_play);
            }
            if self.tok_props != 0 {
                let _ = s.RemoveMediaPropertiesChanged(self.tok_props);
            }
            if self.tok_tl != 0 {
                let _ = s.RemoveTimelinePropertiesChanged(self.tok_tl);
            }
        }
        self.tok_play = 0;
        self.tok_props = 0;
        self.tok_tl = 0;
        self.sess_key = 0;
    }

    fn unhook_all(&mut self) {
        self.unhook_sess();
        if let Some(m) = self.mgr.take() {
            if self.tok_cur != 0 {
                let _ = m.RemoveCurrentSessionChanged(self.tok_cur);
            }
            if self.tok_list != 0 {
                let _ = m.RemoveSessionsChanged(self.tok_list);
            }
        }
    }

    fn hook_sess(&mut self, s: Session, key: usize) {
        self.tok_play = s
            .PlaybackInfoChanged(&TypedEventHandler::new(|_, _| {
                wake(D_PLAY);
                Ok(())
            }))
            .unwrap_or(0);
        self.tok_props = s
            .MediaPropertiesChanged(&TypedEventHandler::new(|_, _| {
                wake(D_PROPS);
                Ok(())
            }))
            .unwrap_or(0);
        self.tok_tl = s
            .TimelinePropertiesChanged(&TypedEventHandler::new(|_, _| {
                wake(D_TL);
                Ok(())
            }))
            .unwrap_or(0);
        let app = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
        mlog(&format!(
            "media: sesi aktif app={} event play={} props={} timeline={}",
            app,
            self.tok_play != 0,
            self.tok_props != 0,
            self.tok_tl != 0
        ));
        self.sess = Some(s);
        self.sess_key = key;
    }

    fn publish(&mut self, next: MediaSnap) {
        if next == self.snap {
            return;
        }
        self.snap = next;
        if let Ok(mut g) = SNAP.lock() {
            *g = Some(self.snap.clone());
        }
        if !NEW.swap(true, Ordering::SeqCst) {
            post();
        }
    }

    fn refresh(&mut self, bits: u32) {
        let Some(mgr) = self.mgr.clone() else {
            return;
        };
        // 1. Sesi aktif (dicek setiap bangun: murah, dan menangkap event sesi yang terlewat).
        let cur = mgr.GetCurrentSession().ok();
        let key = cur
            .as_ref()
            .and_then(|s| s.cast::<IUnknown>().ok())
            .map(|u| u.as_raw() as usize)
            .unwrap_or(0);
        let mut props_needed = (bits & (D_SESSION | D_PROPS)) != 0;
        if key != self.sess_key {
            self.unhook_sess();
            if let Some(s) = cur {
                self.hook_sess(s, key);
            } else {
                mlog("media: tidak ada sesi media");
            }
            props_needed = true;
        }
        let Some(s) = self.sess.clone() else {
            if (bits & D_CMD) != 0 {
                if let Ok(mut q) = CMDS.lock() {
                    q.clear();
                }
            }
            self.art_key.clear();
            self.publish(MediaSnap::default());
            return;
        };

        // 2. Perintah dari UI (setelah sesi mutakhir).
        if (bits & D_CMD) != 0 {
            self.run_cmds(&s);
        }

        let mut next = self.snap.clone();
        next.has = true;
        next.app = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();

        // 3. Status dan kontrol.
        if let Ok(pi) = s.GetPlaybackInfo() {
            next.status = pi.PlaybackStatus().map(|v| v.0).unwrap_or(0);
            if let Ok(c) = pi.Controls() {
                next.can_prev = c.IsPreviousEnabled().unwrap_or(true);
                next.can_next = c.IsNextEnabled().unwrap_or(true);
                next.can_seek = c.IsPlaybackPositionEnabled().unwrap_or(false);
            }
        }

        // 4. Garis waktu.
        if let Ok(tl) = s.GetTimelineProperties() {
            next.tl_start = tl.StartTime().map(|v| v.Duration).unwrap_or(0);
            next.tl_end = tl.EndTime().map(|v| v.Duration).unwrap_or(0);
            next.tl_pos = tl.Position().map(|v| v.Duration).unwrap_or(0);
            next.tl_upd = tl.LastUpdatedTime().map(|v| v.UniversalTime).unwrap_or(0);
        }

        // 5. Judul dan artis: saat event properti/sesi, bila judul masih kosong, atau tiap 30 dtk sebagai jaring pengaman.
        let t = now_ms();
        if props_needed || next.title.is_empty() || t.wrapping_sub(self.last_props) > 30_000 {
            self.last_props = t;
            match s.TryGetMediaPropertiesAsync().and_then(|op| op.get()) {
                Ok(p) => {
                    next.title = p.Title().map(|h| h.to_string()).unwrap_or_default();
                    next.artist = p.Artist().map(|h| h.to_string()).unwrap_or_default();
                    let key = format!("{}\u{1}{}", next.title, next.artist);
                    // Sampul dibaca sekali per lagu; bila gagal (aplikasi sering mengirim sampul sesudah judul),
                    // event properti berikutnya mencoba lagi karena art_key dikosongkan.
                    if key != self.art_key && !next.title.is_empty() {
                        match read_art(&p) {
                            Some(bytes) => {
                                self.art_seq = self.art_seq.wrapping_add(1).max(1);
                                let n = bytes.len();
                                if let Ok(mut g) = ART.lock() {
                                    *g = Some((self.art_seq, bytes));
                                }
                                next.art_seq = self.art_seq;
                                self.art_key = key;
                                mlog(&format!(
                                    "media: metadata berubah app={} judul_len={} artis_len={} status={} sampul={} B",
                                    next.app,
                                    next.title.chars().count(),
                                    next.artist.chars().count(),
                                    next.status,
                                    n
                                ));
                            }
                            None => {
                                if next.title != self.snap.title || next.artist != self.snap.artist {
                                    next.art_seq = 0;
                                    mlog(&format!(
                                        "media: metadata berubah app={} judul_len={} artis_len={} status={} sampul=belum",
                                        next.app,
                                        next.title.chars().count(),
                                        next.artist.chars().count(),
                                        next.status
                                    ));
                                }
                                self.art_key.clear();
                            }
                        }
                    }
                }
                Err(e) => mlog(&format!("media: TryGetMediaPropertiesAsync gagal hr=0x{:08X}", e.code().0 as u32)),
            }
        }
        if next.status != self.snap.status {
            mlog(&format!("media: status {} -> {}", self.snap.status, next.status));
        }
        self.publish(next);
    }

    fn run_cmds(&mut self, s: &Session) {
        let cmds: Vec<Cmd> = match CMDS.lock() {
            Ok(mut q) => q.drain(..).collect(),
            Err(_) => return,
        };
        for c in cmds {
            let (name, r) = match c {
                Cmd::Prev => ("sebelumnya", s.TrySkipPreviousAsync().and_then(|o| o.get())),
                Cmd::Toggle => ("putar/jeda", s.TryTogglePlayPauseAsync().and_then(|o| o.get())),
                Cmd::Next => ("berikutnya", s.TrySkipNextAsync().and_then(|o| o.get())),
                Cmd::Seek(f) => {
                    let r = s.GetTimelineProperties().and_then(|tl| {
                        let s0 = tl.StartTime().map(|v| v.Duration).unwrap_or(0);
                        let e0 = tl.EndTime().map(|v| v.Duration).unwrap_or(0);
                        if e0 - s0 <= 10_000_000 {
                            return Ok(false);
                        }
                        let target = s0 + ((e0 - s0) as f64 * f.clamp(0.0, 1.0) as f64) as i64;
                        s.TryChangePlaybackPositionAsync(target).and_then(|o| o.get())
                    });
                    ("seek", r)
                }
            };
            match r {
                Ok(ok) => mlog(&format!("media: perintah {} diterima={}", name, ok)),
                Err(e) => mlog(&format!("media: perintah {} gagal hr=0x{:08X}", name, e.code().0 as u32)),
            }
        }
        // Status sesudah perintah dibaca di langkah berikutnya; event PlaybackInfoChanged juga akan datang.
    }
}

/// Membaca byte sampul (PNG/JPEG) dari properti media. None bila tidak ada atau gagal.
fn read_art(p: &Props) -> Option<Vec<u8>> {
    let r = p.Thumbnail().ok()?;
    let st = r.OpenReadAsync().ok()?.get().ok()?;
    let size = st.Size().ok()?;
    if size == 0 || size > ART_MAX {
        return None;
    }
    let unk: IUnknown = st.cast().ok()?;
    // SAFETY: unk adalah IRandomAccessStream yang valid; fungsi membungkusnya sebagai IStream.
    let is: IStream = unsafe { CreateStreamOverRandomAccessStream(&unk).ok()? };
    let mut buf = vec![0u8; size as usize];
    let mut off = 0usize;
    while off < buf.len() {
        let mut got = 0u32;
        let rest = (buf.len() - off) as u32;
        // SAFETY: pointer menunjuk ke sisa buffer `buf` sepanjang `rest` byte.
        let hr = unsafe { is.Read(buf[off..].as_mut_ptr() as *mut c_void, rest, Some(&mut got)) };
        if hr.is_err() || got == 0 {
            break;
        }
        off += got as usize;
    }
    if off == 0 {
        return None;
    }
    buf.truncate(off);
    Some(buf)
}

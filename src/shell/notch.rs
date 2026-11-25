//! Sumber: w12notch.inc (seluruh file). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H05.
//! Pulau notch: satu jendela berlapis dengan alfa per piksel; keadaan diam, intip, HUD, dan terbuka.
//! Pomodoro memakai satu timer sekali jalan; catatan cepat memakai notes.txt di folder shell (UTF-8).
use super::prelude::*;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, SetClipboardData,
};
// PORT: GlobalFree berada di Win32::Foundation (sudah dari prelude), bukan System::Memory.
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Power::{
    HPOWERNOTIFY, POWERBROADCAST_SETTING, RegisterPowerSettingNotification, UnregisterPowerSettingNotification,
};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTimeAsFileTime, GetTickCount64};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::UI::Controls::{WM_MOUSEHOVER, WM_MOUSELEAVE};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};

// ---------------------------------------------------------------- konstanta dan global
// ID kontrol notch (enum NI_* di sumber). Hanya dipakai di file ini.
const NI_PREV: i32 = 1;
const NI_PLAY: i32 = 2;
const NI_NEXT: i32 = 3;
const NI_MEDIA: i32 = 4;
const NI_TAB: i32 = 10;
const NI_RING: i32 = 20;
const NI_PM25: i32 = 21;
const NI_PM5: i32 = 22;
const NI_PMTOG: i32 = 23;
const NI_PRESET: i32 = 24; // 24..29
const NI_PMGO: i32 = 30;
const NI_PMSTOP: i32 = 31;
const NI_PMADJ_DN: i32 = 32; // timer kustom: kurangi 5 menit
const NI_PMADJ_UP: i32 = 33; // timer kustom: tambah 5 menit
const NI_PMCUSTOM: i32 = 34; // timer kustom: mulai dengan durasi NT_PM_CUSTOM
const NI_NOTE: i32 = 40;
const NI_NOTEFILE: i32 = 41;
const NI_SEEK: i32 = 42; // klik bar progres media: seek ke posisi
const NI_NOTEROW: i32 = 50; // 50..54
const NI_SYS: i32 = 60;
const NI_NET: i32 = 61;
const NI_BAT: i32 = 62;
const NI_MUTE: i32 = 63;
const NI_VOLTRK: i32 = 64;
const NI_SYS_MEDIA: i32 = 65; // chip "Media" di kartu Sistem: aktif/nonaktifkan pulau media
const NI_SYS_ALWAYS: i32 = 66; // chip "Auto-hide"/"Selalu tampil" di kartu Sistem: notchalways
const NI_SYS_LYR: i32 = 67; // chip "Lirik" di panel Pengaturan notch: notchlyrics
const NI_SYS_MEDTEMP: i32 = 68; // chip "Terus"/"Sementara" di panel Pengaturan notch: notchmedtemp
const NI_SET: i32 = 94; // tombol roda gigi: buka/tutup panel Pengaturan notch
const NI_SET_NOTIF: i32 = 95; // panel Pengaturan: buka pengaturan Notifikasi Windows
const NI_BODY: i32 = 70;
const NI_NOTIF: i32 = 71; // kartu notifikasi di HUD: buka aplikasinya
const NI_NOTIF_ROW: i32 = 80; // 80..91: baris riwayat notifikasi di notch terbuka
const NT_PRESETS: [i32; 6] = [5, 10, 15, 25, 45, 60];

#[derive(Clone, Copy, Default)]
struct NtGeo {
    w: f32,
    h: f32,
    rad: f32,
    e: f32,
    a: f32,
}
const NT_GEO_ZERO: NtGeo = NtGeo { w: 0.0, h: 0.0, rad: 0.0, e: 0.0, a: 0.0 };

// Keadaan notch yang dipakai file lain (hotkeys.rs). g_ntState dan g_ntHud (state.rs:466).
pub static NT_STATE: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(NT_IDLE));
pub static NT_HUD_KIND: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(HUD_VOL));

static NT_HOT_N: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static NT_POLL_MS: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
pub static NT_WND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static NT_PREV_FG: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static NT_SHOWN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static NT_HOVER_PEEK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
// intip sementara (lagu baru / ganti putar-jeda) memakai tampilan penuh, bukan ringkas
static NT_FULL: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
// status media sebelumnya (4 = diputar, 5 = dijeda) untuk mendeteksi ganti putar/jeda
static NT_PLAY_PREV: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static NT_TRACK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static NT_NO_HIT: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static NT_BRI_INIT: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static NT_PREV: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(NT_IDLE));
static NT_TAB: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(NTT_HOME));
// Panel Pengaturan notch (tombol roda gigi di baris tab); menggantikan isi tab saat terbuka.
static NT_SET: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static NT_FROM: SyncCell<Cell<NtGeo>> = SyncCell::new(Cell::new(NT_GEO_ZERO));
static NT_TO: SyncCell<Cell<NtGeo>> = SyncCell::new(Cell::new(NT_GEO_ZERO));
static NT_T0: SyncCell<Cell<i64>> = SyncCell::new(Cell::new(0));
static NT_T: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(1.0));
static NT_BRI: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
// Rekaman tombol notch (seperti PAN_HIT di panel.rs). NT_HIT_EMPTY meniru PAN_HIT_EMPTY yang privat di panel.rs.
const NT_HIT_EMPTY: PanHitRec = PanHitRec { r: D2D_RECT_F { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }, id: 0 };
static NT_HIT: SyncCell<RefCell<[PanHitRec; NT_MAXHIT as usize]>> =
    SyncCell::new(RefCell::new([NT_HIT_EMPTY; NT_MAXHIT as usize]));
static NT_N_HIT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static NT_HOV: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
static NT_DOWN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
static NT_OX: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
static NT_OY: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
static NT_NOTE: SyncCell<RefCell<[u16; 301]>> = SyncCell::new(RefCell::new([0; 301]));
static NT_NOTE_LEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static NT_COPIED: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
static NT_SAVED: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
static NT_COPIED_T: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
static NT_START: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
static NT_LAST_TITLE: SyncCell<RefCell<[u16; 200]>> = SyncCell::new(RefCell::new([0; 200]));
// Pendaftaran notifikasi kecerahan (RegisterPowerSettingNotification); 0 bila belum terdaftar.
static NT_BRI_REG: SyncCell<Cell<HPOWERNOTIFY>> = SyncCell::new(Cell::new(HPOWERNOTIFY(0)));
static NT_SEC: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
static NT_NOTES: SyncCell<RefCell<[[u16; 160]; 5]>> = SyncCell::new(RefCell::new([[0; 160]; 5]));
static NT_NOTE_T: SyncCell<RefCell<[[u16; 20]; 5]>> = SyncCell::new(RefCell::new([[0; 20]; 5]));
static NT_N_NOTES: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static NT_ROUND: SyncCell<RefCell<Option<ID2D1StrokeStyle>>> = SyncCell::new(RefCell::new(None));
static NT_GROW: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static NT_LAYER_AUTO: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
static NT_VOL_L: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
static NT_VOL_R: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
static NT_WHEEL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
// Status Caps Lock terakhir (0 mati, 1 aktif, -1 belum dibaca): pesan hanya saat berubah.
static NT_CAPS: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
// Pomodoro: 0 tidak ada, 1 berjalan, 2 dijeda.
static PM_STATE: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static PM_TOTAL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(25 * 60));
static PM_PAUSED: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static PM_END: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
// Media dijeda: pulau tetap tampil paling lama NT_PAUSE_MS sejak jeda dimulai (GetTickCount64).
static NT_PAUSE_T0: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
const NT_PAUSE_MS: u64 = 5 * 60 * 1000;
// Posisi X klik terakhir (koordinat jendela) untuk seek pada bar progres (NI_SEEK).
static NT_CLICK_X: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
// Garis bar progres yang terakhir digambar: kiri, kanan, dan y tengah (piksel jendela).
static NT_TL_BAR: SyncCell<Cell<(f32, f32, f32)>> = SyncCell::new(Cell::new((0.0, 0.0, 0.0)));
// Durasi timer kustom (menit), dibatasi 5..180 dan bisa diubah dengan tombol +/-.
static NT_PM_CUSTOM: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(25));

fn nt_tick64() -> u64 {
    // SAFETY: GetTickCount64 tanpa argumen dan tanpa prasyarat.
    unsafe { GetTickCount64() }
}

fn nt_local_time() -> SYSTEMTIME {
    // SAFETY: GetLocalTime tanpa argumen dan tanpa prasyarat.
    unsafe { GetLocalTime() }
}

// ---------------------------------------------------------------- Pomodoro
fn pm_left() -> i32 {
    let st = PM_STATE.get();
    if st == 1 {
        let d = PM_END.get() as i64 - nt_tick64() as i64;
        return if d > 0 { ((d + 999) / 1000) as i32 } else { 0 };
    }
    if st == 2 {
        PM_PAUSED.get()
    } else {
        PM_TOTAL.get()
    }
}

// Satu timer sekali jalan sampai sesi selesai; tidak ada detak saat notch tertutup.
fn pm_arm() {
    let wnd = NT_WND.get();
    if wnd.0.is_null() {
        return;
    }
    if PM_STATE.get() != 1 {
        // SAFETY: wnd bukan null (dicek di atas); KillTimer hanya memakai handle.
        unsafe {
            let _ = KillTimer(Some(wnd), 4);
        }
        return;
    }
    let d = PM_END.get() as i64 - nt_tick64() as i64;
    let ms = if d > 50 { d as u32 } else { 50 };
    // SAFETY: wnd bukan null; lptimerfunc None berarti WM_TIMER dikirim ke jendela.
    unsafe {
        SetTimer(Some(wnd), 4, ms, None);
    }
}

fn pm_start(min: i32) {
    PM_TOTAL.set(min * 60);
    PM_END.set(nt_tick64() + PM_TOTAL.get() as u64 * 1000);
    PM_STATE.set(1);
    pm_arm();
}

fn pm_toggle() {
    let st = PM_STATE.get();
    if st == 1 {
        PM_PAUSED.set(pm_left());
        PM_STATE.set(2);
    } else if st == 2 {
        PM_END.set(nt_tick64() + PM_PAUSED.get() as u64 * 1000);
        PM_STATE.set(1);
    } else {
        let m = PM_TOTAL.get() / 60;
        pm_start(if m > 0 { m } else { 25 });
        return;
    }
    pm_arm();
}

fn pm_stop() {
    PM_STATE.set(0);
    pm_arm();
}

// ---------------------------------------------------------------- catatan cepat (notes.txt di folder shell, UTF-8)
fn nt_note_path() -> PathBuf {
    PathBuf::from(DIR.borrow().as_str()).join("notes.txt")
}

// 5 baris terakhir, terbaru dulu; hanya ekor file (maks 16 KB) yang dibaca.
fn nt_notes_load() {
    NT_N_NOTES.set(0);
    let Ok(mut f) = std::fs::File::open(nt_note_path()) else {
        return;
    };
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    let want = size.min(16384);
    let truncated = size > want;
    if truncated && f.seek(SeekFrom::Start(size - want)).is_err() {
        return;
    }
    let mut buf = Vec::new();
    if (&mut f).take(want).read_to_end(&mut buf).is_err() {
        return;
    }
    let st = nt_local_time();
    let today: Vec<u16> = format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay)
        .encode_utf16()
        .collect();
    let mut end = buf.len();
    while end > 0 && NT_N_NOTES.get() < 5 {
        let mut e = end;
        while e > 0 && (buf[e - 1] == b'\n' || buf[e - 1] == b'\r') {
            e -= 1;
        }
        let mut s = e;
        while s > 0 && buf[s - 1] != b'\n' {
            s -= 1;
        }
        let mut a = s;
        if a + 3 <= e && buf[a] == 0xEF && buf[a + 1] == 0xBB && buf[a + 2] == 0xBF {
            a += 3; // BOM
        }
        // Baris pertama potongan ekor bisa terpotong: dilewati.
        if e > a && !(s == 0 && truncated) {
            let line: Vec<u16> = String::from_utf8_lossy(&buf[a..e])
                .encode_utf16()
                .take(399)
                .collect();
            let k = NT_N_NOTES.get() as usize;
            let mut tm = [0u16; 20];
            let mut txt_from = 0usize;
            if line.len() >= 18
                && line[4] == '-' as u16
                && line[10] == ' ' as u16
                && line[16] == ' ' as u16
                && line[17] == ' ' as u16
            {
                if line[..10] == today[..] {
                    // Hari ini: "18.30".
                    tm[..5].copy_from_slice(&line[11..16]);
                } else {
                    // Lainnya: "06/11".
                    tm[..5].copy_from_slice(&[line[8], line[9], '/' as u16, line[5], line[6]]);
                }
                txt_from = 18;
            }
            NT_NOTE_T.borrow_mut()[k] = tm;
            let txt = &line[txt_from..];
            if txt.first().copied().unwrap_or(0) != 0 {
                let m = txt.len().min(159);
                let mut row = [0u16; 160];
                row[..m].copy_from_slice(&txt[..m]);
                NT_NOTES.borrow_mut()[k] = row;
                NT_N_NOTES.set(k as i32 + 1);
            }
        }
        end = s;
        if s == 0 {
            break;
        }
    }
}

fn nt_note_save() -> bool {
    let note: [u16; 301] = *NT_NOTE.borrow();
    let mut b = 0usize;
    let mut e = NT_NOTE_LEN.get().clamp(0, 300) as usize;
    while b < e && note[b] == ' ' as u16 {
        b += 1;
    }
    while e > b && note[e - 1] == ' ' as u16 {
        e -= 1;
    }
    if e <= b {
        return false;
    }
    let st = nt_local_time();
    let text = String::from_utf16_lossy(&note[b..e]);
    let line = format!(
        "{:04}-{:02}-{:02} {:02}.{:02}  {}\r\n",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, text
    );
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(nt_note_path())
    else {
        return false;
    };
    let empty = f.metadata().map(|m| m.len() == 0).unwrap_or(false);
    if empty {
        // Berkas baru: BOM agar Notepad membaca UTF-8.
        let _ = f.write_all(&[0xEF, 0xBB, 0xBF]);
    }
    if f.write_all(line.as_bytes()).is_err() {
        return false;
    }
    NT_NOTE.borrow_mut()[0] = 0;
    NT_NOTE_LEN.set(0);
    NT_SAVED.set(nt_tick64());
    nt_notes_load();
    true
}

fn nt_copy(s: &str) {
    let wide = to_wide_z(s); // termasuk NUL
    let bytes = wide.len() * 2;
    let wnd = NT_WND.get();
    // SAFETY: clipboard dan memori global. Setelah SetClipboardData berhasil, memori dimiliki sistem.
    unsafe {
        if OpenClipboard(Some(wnd)).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, bytes) {
            let p = GlobalLock(g);
            let locked = !p.is_null();
            if locked {
                std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, p as *mut u8, bytes);
                let _ = GlobalUnlock(g);
            }
            if !locked || SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(g.0))).is_err() {
                let _ = GlobalFree(Some(g));
            }
        }
        let _ = CloseClipboard();
    }
}

// ---------------------------------------------------------------- clipboard masuk (tempel ke catatan)
fn nt_paste() {
    // SAFETY: clipboard dibuka atas jendela notch; memori data dikunci lalu dibuka lagi sebelum CloseClipboard.
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
            return;
        }
        if OpenClipboard(Some(NT_WND.get())).is_err() {
            return;
        }
        if let Ok(hd) = GetClipboardData(CF_UNICODETEXT.0 as u32) {
            let g = HGLOBAL(hd.0);
            let p = GlobalLock(g) as *const u16;
            if !p.is_null() {
                {
                    let mut note = NT_NOTE.borrow_mut();
                    let mut n = NT_NOTE_LEN.get().clamp(0, 300) as usize;
                    let mut i = 0usize;
                    while n < 300 {
                        let mut c = *p.add(i);
                        if c == 0 {
                            break;
                        }
                        i += 1;
                        if c == '\r' as u16 || c == '\n' as u16 || c == '\t' as u16 {
                            c = ' ' as u16;
                        }
                        if c < 32 {
                            continue;
                        }
                        note[n] = c;
                        n += 1;
                    }
                    note[n] = 0;
                    NT_NOTE_LEN.set(n as i32);
                }
                let _ = GlobalUnlock(g);
            }
        }
        let _ = CloseClipboard();
    }
}

// ---------------------------------------------------------------- media: posisi lagu (GSMTC timeline)
// Pengganti IMediaTimeline di sumber: GSMTC memberi TimeSpan (Duration) dan DateTime (UniversalTime), keduanya 100 ns.
fn nt_filetime_now() -> i64 {
    // SAFETY: GetSystemTimeAsFileTime tanpa argumen; hanya membaca jam sistem.
    let ft = unsafe { GetSystemTimeAsFileTime() };
    (((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64) as i64
}

// Posisi dan durasi dalam detik; posisi diekstrapolasi dari waktu pembaruan terakhir saat berputar.
fn nt_timeline(pos: &mut i32, dur: &mut i32) -> bool {
    *pos = 0;
    *dur = 0;
    // 2026-11-15: garis waktu dikirim pekerja GSMTC (media_worker.rs) ke MEDIA; tanpa panggilan WinRT di thread UI.
    let (has, status, s0, e0, p0, u0) = {
        let m = MEDIA.borrow();
        (m.has, m.status, m.tl_start, m.tl_end, m.tl_pos, m.tl_upd)
    };
    if !has || e0 - s0 <= 10_000_000 {
        return false;
    }
    let mut ps = (p0 - s0) as f64 / 1e7;
    if status == 4 && u0 > 0 {
        let el = (nt_filetime_now() - u0) as f64 / 1e7;
        if el > 0.0 && el < 6.0 * 3600.0 {
            ps += el;
        }
    }
    *dur = ((e0 - s0) / 10_000_000) as i32;
    *pos = if ps < 0.0 {
        0
    } else if ps > *dur as f64 {
        *dur
    } else {
        ps as i32
    };
    true
}

fn nt_has_media() -> bool {
    let m = MEDIA.borrow();
    m.has && (m.title[0] != 0 || m.artist[0] != 0)
}

// Posisi putar dalam ms, diekstrapolasi seperti nt_timeline (dipakai lirik).
fn nt_pos_ms() -> i64 {
    let (has, status, s0, p0, u0) = {
        let m = MEDIA.borrow();
        (m.has, m.status, m.tl_start, m.tl_pos, m.tl_upd)
    };
    if !has {
        return 0;
    }
    let mut ps = (p0 - s0) as f64 / 1e7;
    if status == 4 && u0 > 0 {
        let el = (nt_filetime_now() - u0) as f64 / 1e7;
        if el > 0.0 && el < 6.0 * 3600.0 {
            ps += el;
        }
    }
    (ps * 1000.0).max(0.0) as i64
}

// Lirik untuk posisi sekarang (lyrics.rs). None bila lirik dimatikan, tanpa media, atau belum tersedia.
fn nt_lyric_line() -> Option<String> {
    if !NOTCH_LYRICS.get() || !nt_has_media() {
        return None;
    }
    let k = super::lyrics::key(&nt_wstr(&MEDIA.borrow().artist), &nt_wstr(&MEDIA.borrow().title));
    super::lyrics::line(&k, nt_pos_ms()).filter(|l| !l.trim().is_empty())
}

// Minta lirik untuk lagu di MEDIA; non-blokir (pencarian di thread latar, lihat lyrics.rs).
fn nt_lyr_want() {
    let (a, t, d) = {
        let m = MEDIA.borrow();
        (nt_wstr(&m.artist), nt_wstr(&m.title), (m.tl_end - m.tl_start) / 10_000_000)
    };
    super::lyrics::want(&a, &t, d as i32);
}

/// Garis diam selalu terlihat (notchalways=1), kecuali di atas video/game layar penuh.
fn nt_always() -> bool {
    NOTCH_ALWAYS.get() && NT_FS_MODE.get() == 0
}

// Media sedang diputar (status 4), atau dijeda (status 5) selama belum melewati NT_PAUSE_MS.
// Layar penuh "hanya notifikasi": pulau media tidak muncul sama sekali.
fn nt_live_on() -> bool {
    if nt_fs_quiet() || !(NOTCH_HUD.get() && NOTCH_LIVE.get() && nt_has_media()) {
        return false;
    }
    match MEDIA.borrow().status {
        4 => true,
        5 => nt_tick64().wrapping_sub(NT_PAUSE_T0.get()) < NT_PAUSE_MS,
        _ => false,
    }
}

// Pulau ringkas menetap (hanya bila notchcompact=1): media diputar dan tidak sedang diperluas hover atau intip.
// Default: pulau media penuh (judul, artis, kontrol) selama ada media, lewat cabang non-ringkas di nt_target.
fn nt_compact() -> bool {
    NOTCH_COMPACT.get() && nt_live_on() && !NT_HOVER_PEEK.get() && !NT_FULL.get()
}

// Keadaan yang boleh menerima hover untuk memperluas pulau (diam, atau ringkas tanpa hover).
fn nt_can_hover() -> bool {
    let st = NT_STATE.get();
    st == NT_IDLE || (st == NT_PEEK && !NT_HOVER_PEEK.get())
}

// ---------------------------------------------------------------- geometri dan animasi
fn nt_msg_string() -> String {
    // PORT: NT_MSG kanonik berupa String (state.rs), dipakai juga oleh hotkeys.rs dan usb.rs.
    NT_MSG.borrow().clone()
}

fn nt_target(st: i32) -> NtGeo {
    let mut g = NtGeo { w: 0.0, h: 0.0, rad: 0.0, e: 0.0, a: 1.0 };
    match st {
        NT_IDLE => {
            // tersembunyi: garis tipis yang memudar; notchalways=1: garis tetap terlihat
            g.w = sf(160.0);
            g.h = sf(10.0);
            g.rad = sf(5.0);
            g.a = if nt_always() { 1.0 } else { 0.0 };
        }
        NT_PEEK => {
            if nt_compact() {
                // ringkas menetap: sampul kecil dan bar visualizer
                g.w = sf(120.0);
                g.h = sf(30.0);
                g.rad = sf(15.0);
            } else {
                if nt_has_media() {
                    g.w = sf(400.0);
                    g.h = sf(52.0);
                } else {
                    g.w = sf(320.0);
                    g.h = sf(44.0);
                }
                g.rad = sf(22.0);
            }
        }
        NT_HUD => {
            g.h = sf(44.0);
            g.rad = sf(22.0);
            g.w = sf(300.0);
            if NT_HUD_KIND.get() == HUD_NOTIF {
                g.w = sf(420.0);
                g.h = sf(66.0);
                g.rad = sf(26.0);
            } else if NT_HUD_KIND.get() == HUD_MSG {
                let txt = nt_msg_string();
                let tf = TF_S.borrow();
                let f: Option<&IDWriteTextFormat> = tf.as_ref();
                let w = sf(64.0) + measure_text(&txt, f);
                let w = if w < sf(240.0) {
                    sf(240.0)
                } else if w > sf(480.0) {
                    sf(480.0)
                } else {
                    w
                };
                g.w = w.ceil();
            }
        }
        NT_OPEN => {
            // lebar ikut layar sempit; tinggi memberi ruang baris volume di kartu media
            // SAFETY: GetSystemMetrics hanya membaca metrik layar.
            let cx = unsafe { GetSystemMetrics(SM_CXSCREEN) };
            let sw = cx as f32 - sf(32.0);
            g.w = if sw < sf(640.0) { sw.floor() } else { sf(640.0) };
            g.h = sf(296.0);
            g.rad = sf(22.0);
            g.e = 1.0;
        }
        _ => {}
    }
    g
}

// Menyusut: ease-out kuartik. Membesar: ease-out "back" dengan c = 0,9 (lewat ±3,7% di sekitar 2/3 waktu lalu
// kembali), memberi kesan pegas tanpa frame tambahan.
fn nt_ease(t: f32) -> f32 {
    let u = t - 1.0;
    if NT_GROW.get() {
        let c = 0.9f32;
        return 1.0 + (c + 1.0) * u * u * u + c * u * u;
    }
    1.0 - u * u * u * u
}

fn nt_cur() -> NtGeo {
    if NT_T.get() >= 1.0 {
        return NT_TO.get();
    }
    let k = nt_ease(NT_T.get());
    let kc = if k > 1.0 { 1.0 } else { k }; // opasitas dan bayangan tidak ikut melewati target
    let f = NT_FROM.get();
    let t = NT_TO.get();
    NtGeo {
        w: f.w + (t.w - f.w) * k,
        h: f.h + (t.h - f.h) * k,
        rad: f.rad + (t.rad - f.rad) * k,
        e: f.e + (t.e - f.e) * kc,
        a: f.a + (t.a - f.a) * kc,
    }
}

// Pindah keadaan (atau ukuran baru pada keadaan yang sama) dengan animasi.
fn nt_go(st: i32) {
    NT_FROM.set(nt_cur());
    NT_PREV.set(NT_STATE.get());
    NT_STATE.set(st);
    // bar progres dan lirik hanya terlihat di intip/terbuka: hanya saat itu event garis waktu diproses segera
    super::media_worker::set_tl_hot(st == NT_PEEK || st == NT_OPEN);
    let to = nt_target(st);
    NT_TO.set(to);
    let from = NT_FROM.get();
    NT_GROW.set(to.w * to.h > from.w * from.h + 1.0);
    let wnd = NT_WND.get();
    // SAFETY: GetCapture dan ReleaseCapture hanya membaca dan melepas capture mouse thread ini.
    unsafe {
        if !wnd.0.is_null() && GetCapture() == wnd {
            let _ = ReleaseCapture(); // seret volume terputus oleh perubahan keadaan
        }
    }
    NT_HOV.set(-1);
    NT_DOWN.set(-1);
    if wnd.0.is_null() || !NT_SHOWN.get() {
        NT_T.set(1.0);
        return;
    }
    NT_T.set(0.0);
    // SAFETY: wnd bukan null; penunjuk QPC menunjuk variabel lokal yang valid.
    unsafe {
        let mut t0: i64 = 0;
        let _ = QueryPerformanceCounter(&mut t0);
        NT_T0.set(t0);
        SetTimer(Some(wnd), 1, 16, None);
    }
    notch_render();
    // SAFETY: wnd bukan null. Keluar dari tersembunyi: frame pertama sudah terpasang (alfa ~0).
    unsafe {
        if st != NT_IDLE && !IsWindowVisible(wnd).as_bool() {
            let _ = ShowWindow(wnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                wnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

// Terapkan notchalways pada keadaan diam: tampil (animasi ke garis terlihat) atau sembunyi (memudar, lalu nt_tick menyembunyikan).
// Keadaan lain (intip, HUD, terbuka) sudah menampilkan jendela dan akan mengikuti aturan ini saat kembali diam.
fn nt_idle_apply() {
    let h = NT_WND.get();
    if h.0.is_null() || !NT_SHOWN.get() || NT_STATE.get() != NT_IDLE {
        return;
    }
    if nt_always() {
        // SAFETY: h adalah jendela notch yang valid.
        unsafe {
            if !IsWindowVisible(h).as_bool() {
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }
    nt_go(NT_IDLE);
}

fn nt_tick() {
    let mut now: i64 = 0;
    let mut fq: i64 = 0;
    // SAFETY: penunjuk ke variabel lokal yang valid.
    unsafe {
        let _ = QueryPerformanceCounter(&mut now);
        let _ = QueryPerformanceFrequency(&mut fq);
    }
    let dur = (if NT_GROW.get() { NT_GROW_MS } else { NT_ANIM_MS }) as f64;
    let t = ((now - NT_T0.get()) as f64 * 1000.0 / fq as f64 / dur) as f32;
    let done = t >= 1.0;
    NT_T.set(if done { 1.0 } else { t });
    let wnd = NT_WND.get();
    // SAFETY: KillTimer dan ShowWindow hanya memakai handle jendela notch.
    unsafe {
        if done {
            let _ = KillTimer(Some(wnd), 1);
        }
        if done && NT_STATE.get() == NT_IDLE && !nt_always() {
            // tersembunyi penuh: tidak ada jendela, tidak ada gambar
            let _ = ShowWindow(wnd, SW_HIDE);
        } else {
            notch_render();
        }
    }
    // Keluar dari keadaan terbuka: DIB bersama sempat sebesar panel; dibuat lagi seukuran kecil saat render berikutnya.
    if done && NT_PREV.get() == NT_OPEN && NT_STATE.get() != NT_OPEN && PANEL_OPEN.get() < 0 {
        release_mem();
        trim_soon();
    }
}

// Gambar ulang; bila ukuran isi berubah (mis. ekualiser muncul), pulau beranimasi ke ukuran baru.
pub fn notch_invalidate() {
    let wnd = NT_WND.get();
    if wnd.0.is_null() || !NT_SHOWN.get() {
        return;
    }
    if NT_T.get() >= 1.0 {
        let t = nt_target(NT_STATE.get());
        let to = NT_TO.get();
        if (t.w - to.w).abs() > 0.5 || (t.h - to.h).abs() > 0.5 {
            nt_go(NT_STATE.get());
            return;
        }
    }
    // wnd bukan null; w12_invalidate memposting WM_W12_RENDER untuk jendela berlapis.
    let _ = w12_invalidate(wnd, None, false);
}

// Intip/HUD sementara; tidak pernah menimpa keadaan terbuka (w12notch.inc:350-359).
pub fn nt_show_temp(st: i32, ms: u32) {
    let wnd = NT_WND.get();
    if wnd.0.is_null() || !NT_SHOWN.get() || NT_STATE.get() == NT_OPEN {
        return;
    }
    NT_HOVER_PEEK.set(false);
    NT_FULL.set(st == NT_PEEK); // intip lagu memakai tampilan penuh
    let t = nt_target(st);
    let to = NT_TO.get();
    if NT_STATE.get() != st || (t.w - to.w).abs() > 0.5 || (t.h - to.h).abs() > 0.5 {
        nt_go(st);
    } else {
        let _ = w12_invalidate(wnd, None, false);
    }
    unsafe { SetTimer(Some(wnd), 2, ms, None) };
}

// Buka panel (tab NTT_*), aktifkan jendela agar ketikan masuk, lalu mulai pembaruan per detik (w12notch.inc:361-385).
fn nt_open(tab: i32) {
    let wnd = NT_WND.get();
    if wnd.0.is_null() || !NT_SHOWN.get() {
        return;
    }
    if NT_STATE.get() != NT_OPEN {
        let fg = unsafe { GetForegroundWindow() };
        NT_PREV_FG.set(if fg != wnd { fg } else { HWND(std::ptr::null_mut()) });
    }
    NT_TAB.set(tab);
    if tab == NTT_NOTES {
        nt_notes_load();
    }
    let _ = unsafe { KillTimer(Some(wnd), 2) };
    let ex = unsafe { GetWindowLongPtrW(wnd, GWL_EXSTYLE) };
    unsafe { SetWindowLongPtrW(wnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize)) };
    if !SYS_TIMER.get() {
        sys_sample(); // widget Sistem tidak sedang mengambil sampel: mulai sendiri selama terbuka
    }
    if NT_STATE.get() != NT_OPEN {
        nt_go(NT_OPEN);
    } else {
        let _ = w12_invalidate(wnd, None, false);
    }
    let _ = unsafe {SetForegroundWindow(wnd) }; // aktif: ketikan masuk ke catatan; klik di luar menutup (WM_ACTIVATE)
    let fg_now = unsafe { GetForegroundWindow() };
    if fg_now != wnd {
        let fg = unsafe { GetForegroundWindow() };
        let ft = if fg.0.is_null() { 0 } else { unsafe { GetWindowThreadProcessId(fg, None) } };
        let me = unsafe { GetCurrentThreadId() };
        let attached = ft != 0 && ft != me && unsafe { AttachThreadInput(me, ft, true) }.as_bool();
        if attached {
            let _ = unsafe {SetForegroundWindow(wnd) };
            let _ = unsafe {AttachThreadInput(me, ft, false) };
        }
    }
    NT_SEC.set(0);
    unsafe { SetTimer(Some(wnd), 3, 1000, None) };
}

// Tutup panel terbuka; kembalikan fokus ke jendela sebelumnya bila diminta (w12notch.inc:387-401).
fn nt_close(restore_focus: bool) {
    let wnd = NT_WND.get();
    if wnd.0.is_null() {
        return;
    }
    if NT_STATE.get() == NT_OPEN {
        let _ = unsafe { KillTimer(Some(wnd), 3) };
        let ex = unsafe { GetWindowLongPtrW(wnd, GWL_EXSTYLE) };
        unsafe { SetWindowLongPtrW(wnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize) };
        if !SYS_TIMER.get() {
            sys_release(); // query PDH ditutup lagi bila widget Sistem juga tidak memakainya
        }
        let back = NT_PREV_FG.get();
        NT_PREV_FG.set(HWND(std::ptr::null_mut()));
        nt_go(if nt_live_on() { NT_PEEK } else { NT_IDLE });
        nt_viz_sync();
        let fg_now = unsafe { GetForegroundWindow() };
        let back_ok = !back.0.is_null() && unsafe { IsWindow(Some(back)) }.as_bool();
        if restore_focus && back_ok && fg_now == wnd {
            let _ = unsafe {SetForegroundWindow(back) };
        }
        return;
    }
    if NT_STATE.get() != NT_IDLE {
        nt_go(if nt_live_on() { NT_PEEK } else { NT_IDLE });
        nt_viz_sync();
    }
}

// ---------------------------------------------------------------- gambar: bantuan
// Mencatat area klik baru (w12notch.inc:404-410).
fn nt_hit(l: f32, t: f32, r: f32, b: f32, id: i32) {
    let n = NT_N_HIT.get();
    if NT_NO_HIT.get() || n >= NT_MAXHIT {
        return;
    }
    {
        let mut hits = NT_HIT.borrow_mut();
        hits[n as usize] = PanHitRec { r: rf(l, t, r, b), id };
    }
    NT_N_HIT.set(n + 1);
}

// Catat area klik dan gambar latar saat disorot atau ditekan (w12notch.inc:412-418).
fn nt_hover(l: f32, t: f32, r: f32, b: f32, rad: f32, id: i32) {
    nt_hit(l, t, r, b, id);
    if NT_NO_HIT.get() {
        return;
    }
    let p = *PALETTE.borrow();
    if NT_DOWN.get() == id {
        fill_rr(l, t, r, b, rad, p.press, 1.0);
    } else if NT_HOV.get() == id {
        fill_rr(l, t, r, b, rad, p.hover, 1.0);
    }
}

// Tombol bulat: kotak klik selalu seukuran r; saat ditekan bentuknya mengecil 12% (w12notch.inc:420-429).
fn nt_round_btn(cx: f32, cy: f32, r: f32, id: i32, fill: Option<D2D1_COLOR_F>) {
    nt_hit(cx - r, cy - r, cx + r, cy + r, id);
    let down = !NT_NO_HIT.get() && NT_DOWN.get() == id;
    let hov = !NT_NO_HIT.get() && NT_HOV.get() == id;
    let d = if down { r * 0.88 } else { r };
    let p = *PALETTE.borrow();
    if let Some(f) = fill {
        fill_rr(cx - d, cy - d, cx + d, cy + d, d, f, 1.0);
    }
    if down {
        fill_rr(cx - d, cy - d, cx + d, cy + d, d, p.press, 1.0);
    } else if hov {
        fill_rr(cx - d, cy - d, cx + d, cy + d, d, p.hover, 1.0);
    }
}

// Id area klik terakhir (paling atas) di bawah titik (x, y) dalam koordinat jendela; -1 bila tidak ada (w12notch.inc:431-439).
fn nt_hit_at(x: i32, y: i32) -> i32 {
    let lx = x as f32 - NT_OX.get();
    let ly = y as f32 - NT_OY.get();
    let hits = NT_HIT.borrow();
    for i in (0..NT_N_HIT.get()).rev() {
        let r = &hits[i as usize].r;
        if lx >= r.left && lx < r.right && ly >= r.top && ly < r.bottom {
            return hits[i as usize].id;
        }
    }
    -1
}

// Kartu latar notch (w12notch.inc:441).
fn nt_card(l: f32, t: f32, r: f32, b: f32) {
    let p = *PALETTE.borrow();
    bt_card(l, t, r, b, sf(16.0), &p.chip, &p.line, 1.0);
}

// Chip teks kecil; `on` memakai warna aksen (w12notch.inc:443-450).
fn nt_chip(l: f32, t: f32, r: f32, b: f32, s: &str, id: i32, on: bool) {
    let rad = (b - t) * 0.5;
    let p = *PALETTE.borrow();
    if on {
        bt_card(l, t, r, b, rad, &p.accent_soft, &p.accent_line, 1.0);
    } else {
        fill_rr(l, t, r, b, rad, p.chip_hover, 1.0);
    }
    nt_hover(l, t, r, b, rad, id);
    let c = if on { p.accent_ink } else { p.text };
    wf_text_c(s, TF_XS.borrow().as_ref(), (l + r) * 0.5, t, b, &c);
}

// Tombol pil berlabel (w12notch.inc:452-459).
fn nt_button(l: f32, t: f32, r: f32, b: f32, s: &str, id: i32, accent: bool) {
    let rad = (b - t) * 0.5;
    let p = *PALETTE.borrow();
    if accent {
        fill_rr(l, t, r, b, rad, p.accent, 1.0);
    } else {
        bt_card(l, t, r, b, rad, &p.chip, &p.line, 1.0);
    }
    nt_hover(l, t, r, b, rad, id);
    let c = if accent { rgba(255, 255, 255, 1.0) } else { p.text };
    wf_text_c(s, TF_SS.borrow().as_ref(), (l + r) * 0.5, t, b, &c);
}

// Bilah progres datar; frac < 0 berarti tanpa isi (w12notch.inc:461-467).
fn nt_bar(l: f32, cy: f32, r: f32, th: f32, frac: f32, c: D2D1_COLOR_F) {
    let p = *PALETTE.borrow();
    fill_rr(l, cy - th * 0.5, r, cy + th * 0.5, th * 0.5, p.track, 1.0);
    if frac < 0.0 {
        return;
    }
    let frac = frac.min(1.0);
    if frac > 0.005 {
        fill_rr(l, cy - th * 0.5, l + (r - l) * frac, cy + th * 0.5, th * 0.5, c, 1.0);
    }
}

// DrawEllipse dengan kuas BR dan tebal sw (dipakai cincin).
fn nt_draw_ellipse(e: &D2D1_ELLIPSE, sw: f32) {
    let rt_g = RT.borrow();
    let br_g = BR.borrow();
    if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
        let brush: &ID2D1Brush = br;
        unsafe { rt.DrawEllipse(e, brush, sw, None::<&ID2D1StrokeStyle>) };
    }
}

// Cincin progres: lintasan penuh lalu busur sebesar frac mulai dari jam 12 (w12notch.inc:469-498).
fn nt_ring(cx: f32, cy: f32, r: f32, sw: f32, frac: f32, c: D2D1_COLOR_F) {
    let e = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: r, radiusY: r };
    let track = PALETTE.borrow().track;
    set_color(track, 1.0);
    nt_draw_ellipse(&e, sw);
    if frac <= 0.002 {
        return;
    }
    set_color(c, 1.0);
    if frac >= 0.998 {
        nt_draw_ellipse(&e, sw);
        return;
    }
    let need_style = NT_ROUND.borrow().is_none();
    if need_style {
        let sp = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: D2D1_CAP_STYLE_ROUND,
            endCap: D2D1_CAP_STYLE_ROUND,
            dashCap: D2D1_CAP_STYLE_ROUND,
            lineJoin: D2D1_LINE_JOIN_ROUND,
            miterLimit: 10.0,
            dashStyle: D2D1_DASH_STYLE_SOLID,
            dashOffset: 0.0,
        };
        let made = match D2D.borrow().as_ref() {
            Some(d2d) => unsafe { d2d.CreateStrokeStyle(&sp, None::<&[f32]>) }.ok(),
            None => None,
        };
        *NT_ROUND.borrow_mut() = made;
    }
    let Some(d2d) = D2D.borrow().as_ref().cloned() else {
        return;
    };
    let Ok(pg) = (unsafe { d2d.CreatePathGeometry() }) else {
        return;
    };
    let Ok(sk) = (unsafe { pg.Open() }) else {
        return;
    };
    let a0 = -std::f32::consts::FRAC_PI_2;
    let a1 = a0 + frac * std::f32::consts::TAU;
    let p0 = Vector2 { X: cx + r * a0.cos(), Y: cy + r * a0.sin() };
    let p1 = Vector2 { X: cx + r * a1.cos(), Y: cy + r * a1.sin() };
    let arc = D2D1_ARC_SEGMENT {
        point: p1,
        size: D2D_SIZE_F { width: r, height: r },
        rotationAngle: 0.0,
        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
        arcSize: if frac > 0.5 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
    };
    unsafe {
        sk.BeginFigure(p0, D2D1_FIGURE_BEGIN_HOLLOW);
        sk.AddArc(&arc);
        sk.EndFigure(D2D1_FIGURE_END_OPEN);
    }
    let _ = unsafe { sk.Close() };
    let rt_g = RT.borrow();
    let br_g = BR.borrow();
    let round_g = NT_ROUND.borrow();
    if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
        let brush: &ID2D1Brush = br;
        let geo: &ID2D1Geometry = &pg;
        match round_g.as_ref() {
            Some(sty) => unsafe { rt.DrawGeometry(geo, brush, sw, sty) },
            None => unsafe { rt.DrawGeometry(geo, brush, sw, None::<&ID2D1StrokeStyle>) },
        }
    }
}

// ---------------------------------------------------------------- teks, jaringan, volume (w12notch.inc:499-575)

// Teks rata kanan di tepi r (BtTextR; bt_text_r di flyout.rs bersifat privat).
fn nt_text_r(s: &str, f: Option<&IDWriteTextFormat>, r: f32, t: f32, b: f32, c: D2D1_COLOR_F) {
    let w = measure_text(s, f) + sf(2.0);
    text(s, f, r - w, t, r, b, c, 1.0);
}

// Buffer UTF-16 berakhir null menjadi String.
fn nt_wstr(w: &[u16]) -> String {
    let n = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..n])
}

// NtMmss: "m:ss" dari detik (w12notch.inc:500).
fn nt_mmss(s: i32) -> String {
    let s = s.max(0);
    format!("{}:{:02}", s / 60, s % 60)
}

// Baris sumber media: "artis · sumber" bila keduanya ada, selain itu salah satunya (w12notch.inc:502-508).
fn nt_media_sub() -> String {
    // lirik tersinkron menggantikan baris artis selama ada baris untuk posisi sekarang
    if let Some(l) = nt_lyric_line() {
        return copy_wide(&l, 300);
    }
    let artist = nt_wstr(&MEDIA.borrow().artist);
    let src = media_source_name(64);
    let s = if !artist.is_empty() && !src.is_empty() {
        format!("{} \u{00B7} {}", artist, src)
    } else if !artist.is_empty() {
        artist
    } else {
        src
    };
    copy_wide(&s, 300)
}

thread_local! {
    // Cache potongan teks terakhir (teks, lebar, hasil) agar tidak mengukur ulang setiap gambar.
    static NT_FIT_CACHE: std::cell::RefCell<(String, i32, String)> =
        std::cell::RefCell::new((String::new(), -1, String::new()));
}

// Potong `s` agar muat di `maxw` dengan elipsis di akhir. Teks yang sudah muat dikembalikan apa adanya.
fn nt_fit(s: &str, f: Option<&IDWriteTextFormat>, maxw: f32) -> String {
    if s.is_empty() || measure_text(s, f) <= maxw {
        return s.to_string();
    }
    let key_w = maxw.floor() as i32;
    let hit = NT_FIT_CACHE.with(|c| {
        let c = c.borrow();
        if c.0 == s && c.1 == key_w { Some(c.2.clone()) } else { None }
    });
    if let Some(v) = hit {
        return v;
    }
    // Pencarian biner panjang awalan terpanjang yang muat bersama elipsis.
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let cand: String = chars[..mid].iter().collect::<String>() + "\u{2026}";
        if measure_text(&cand, f) <= maxw {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let out: String = chars[..lo].iter().collect::<String>() + "\u{2026}";
    NT_FIT_CACHE.with(|c| *c.borrow_mut() = (s.to_string(), key_w, out.clone()));
    out
}

// Bar progres tipis dan samar di tepi bawah pil, hanya saat lirik tampil. Tidak menggeser teks.
fn nt_lyr_progress(l: f32, r: f32, h: f32) {
    let mut pos = 0i32;
    let mut dur = 0i32;
    if r <= l || !nt_timeline(&mut pos, &mut dur) || dur <= 0 {
        return;
    }
    let p = *PALETTE.borrow();
    let th = sf(2.0);
    let cy = h - sf(3.0);
    let frac = (pos as f32 / dur as f32).clamp(0.0, 1.0);
    fill_rr(l, cy - th * 0.5, r, cy + th * 0.5, th * 0.5, p.track, 0.35);
    if frac > 0.005 {
        fill_rr(l, cy - th * 0.5, l + (r - l) * frac, cy + th * 0.5, th * 0.5, p.sub, 0.7);
    }
}

// Glyph jaringan; NET_BOTH ditampilkan sebagai kabel (w12notch.inc:510-511).
fn nt_net_glyph() -> u16 {
    if NET.get() == NET_WIFI {
        0xE701
    } else if NET.get() == NET_NONE {
        0xEB55
    } else {
        0xE839
    }
}

// Status jaringan dan baterai rata kanan di rx; mengembalikan x kiri gugus (w12notch.inc:513-530).
fn nt_status(rx: f32, cy: f32, tf: Option<&IDWriteTextFormat>, bg: D2D1_COLOR_F, hit: bool) -> f32 {
    let p = *PALETTE.borrow();
    let mut rx = rx;
    if HAS_BAT.get() && BAT.get() >= 0 {
        let b = format!("{}%", BAT.get());
        let tw = measure_text(&b, tf);
        let l = rx - tw - sf(30.0);
        let low = !CHARGING.get() && BAT.get() <= 15;
        if hit {
            nt_hover(l - sf(6.0), cy - sf(13.0), rx + sf(6.0), cy + sf(13.0), sf(13.0), NI_BAT);
        }
        let c = if low { rgba(232, 72, 85, 1.0) } else { p.sub };
        nt_text_r(&b, tf, rx, cy - sf(10.0), cy + sf(10.0), c);
        draw_battery(rx - tw - sf(15.0), cy, sf(17.0), p.glyph, bg);
        rx = l - sf(10.0);
    }
    if hit {
        nt_hover(rx - sf(26.0), cy - sf(13.0), rx, cy + sf(13.0), sf(13.0), NI_NET);
    }
    let gc = if NET.get() == NET_NONE { p.sub } else { p.glyph };
    glyph(nt_net_glyph(), TF_GLYPH_SM.borrow().as_ref(), rx - sf(13.0), cy, sf(8.0), gc, 1.0);
    rx - sf(26.0)
}

// Volume dari pulau, sama seperti Pengaturan Cepat; menaikkan dari 0 membatalkan bisu (w12notch.inc:532-541).
fn nt_vol_set(v: i32) {
    // Salinan COM dipegang agar borrow EP_VOL tidak melewati pemanggilan yang memicu callback.
    let ep = EP_VOL.borrow().clone();
    if let Some(ep) = ep {
        let v = v.clamp(0, 100);
        let _ = unsafe { ep.SetMasterVolumeLevelScalar(v as f32 / 100.0, std::ptr::null()) };
        VOL.set(v);
        if MUTE.get() && v > 0 {
            let _ = unsafe { ep.SetMute(false, std::ptr::null()) };
            MUTE.set(false);
        }
        let _ = w12_invalidate(NT_WND.get(), None, false);
    }
}

// Seret penggeser volume; x = koordinat klien jendela pulau (w12notch.inc:543-549).
fn nt_vol_drag(x: i32) {
    let (l, r) = (NT_VOL_L.get(), NT_VOL_R.get());
    if r <= l {
        return;
    }
    let f = ((x as f32) - NT_OX.get() - l) / (r - l);
    let f = f.clamp(0.0, 1.0);
    nt_vol_set((f * 100.0).round() as i32);
}

// Baris volume: tombol bisu, penggeser (jempol membesar saat disorot atau diseret), persentase (w12notch.inc:551-575).
fn nt_vol_row(l: f32, r: f32, cy: f32) {
    let p = *PALETTE.borrow();
    let br = sf(13.0);
    nt_hover(l, cy - br, l + 2.0 * br, cy + br, br, NI_MUTE);
    let mute = MUTE.get();
    let mc = if mute { p.sub } else { p.glyph };
    glyph(vol_glyph(), TF_GLYPH_SM.borrow().as_ref(), l + br, cy, sf(8.0), mc, 1.0);
    let tl = l + 2.0 * br + sf(10.0);
    let tr = r - sf(40.0);
    NT_VOL_L.set(tl);
    NT_VOL_R.set(tr);
    let vol = VOL.get();
    let live = EP_VOL.borrow().is_some() && vol >= 0;
    if live {
        nt_hit(tl - sf(8.0), cy - sf(12.0), tr + sf(8.0), cy + sf(12.0), NI_VOLTRK);
    }
    let frac = if live && !mute { vol as f32 / 100.0 } else { 0.0 };
    nt_bar(tl, cy, tr, sf(4.0), frac, p.accent);
    if live {
        let press = NT_DOWN.get() == NI_VOLTRK;
        let hot = press || NT_HOV.get() == NI_VOLTRK;
        let kx = (tl + frac * (tr - tl)).round();
        let ko = if hot { sf(8.0) } else { sf(6.0) };
        let ki = if press { sf(3.0) } else { sf(4.0) };
        fill_rr(kx - ko, cy - ko + sf(1.0), kx + ko, cy + ko + sf(1.0), ko, p.shadow, 0.6);
        let knob = if LIGHT.get() { rgba(255, 255, 255, 1.0) } else { rgba(66, 72, 110, 1.0) };
        fill_rr(kx - ko, cy - ko, kx + ko, cy + ko, ko, knob, 1.0);
        if hot {
            fill_rr(kx - ki, cy - ki, kx + ki, cy + ki, ki, p.accent, 1.0);
        }
    }
    let pct = if live { format!("{}%", if mute { 0 } else { vol }) } else { "--".to_string() };
    let pc = if NT_DOWN.get() == NI_VOLTRK { p.text } else { p.sub };
    nt_text_r(&pct, TF_XS.borrow().as_ref(), r, cy - sf(9.0), cy + sf(9.0), pc);
}

// ---------------------------------------------------------------- gambar: keadaan kecil dan HUD (w12notch.inc:577-627)
// Pulau ringkas saat media diputar: sampul kecil di kiri, empat bar visualizer di kanan.
// Animasi ini tiruan (bukan spektrum audio). Isi dipilih dari lebar yang sedang tampil agar transisi mulus.
fn nt_draw_live(w: f32, h: f32) {
    let p = *PALETTE.borrow();
    let cy = h * 0.5;
    let art = sf(20.0);
    draw_art(sf(5.0), cy - art * 0.5, art);
    let bw = sf(3.0);
    let gap = sf(2.5);
    let x_end = w - sf(10.0);
    let t = nt_tick64() as f32 / 1000.0;
    let playing = MEDIA.borrow().status == 4;
    for i in 0..4 {
        let k = i as f32;
        let amp = if playing { ((t * (4.0 + k * 1.3)) + k * 1.7).sin().abs() } else { 0.0 };
        let bh = sf(4.0) + amp * sf(11.0);
        let x = x_end - bw - (3.0 - k) * (bw + gap);
        fill_rr(x, cy - bh * 0.5, x + bw, cy + bh * 0.5, bw * 0.5, p.accent, 1.0);
    }
}

fn nt_draw_peek(w: f32, h: f32) {
    if nt_has_media() && w < sf(250.0) {
        nt_draw_live(w, h);
        return;
    }
    let p = *PALETTE.borrow();
    let cy = h * 0.5;
    if !nt_has_media() {
        // tanpa media: jam, tanggal, jaringan, baterai
        let mut x = sf(18.0);
        let tz = TIME_TZ.borrow().clone();
        let tw = measure_text(&tz, TF_BS.borrow().as_ref());
        text(&tz, TF_BS.borrow().as_ref(), x, 0.0, x + tw + sf(2.0), h, p.text, 1.0);
        x += tw + sf(12.0);
        let ds = DATE_SHORT.borrow().clone();
        text(&ds, TF_S.borrow().as_ref(), x, 0.0, w - sf(96.0), h, p.sub, 1.0);
        nt_status(w - sf(18.0), cy, TF_S.borrow().as_ref(), p.veil, false);
        return;
    }
    let art = sf(36.0);
    let ax = sf(8.0);
    nt_hit(ax, cy - art / 2.0, ax + art, cy + art / 2.0, NI_MEDIA);
    draw_art(ax, cy - art / 2.0, art);
    let bx = w - sf(8.0) - sf(30.0) * 3.0 - sf(4.0) * 2.0;
    let tx = ax + art + sf(10.0);
    let lyr_on = nt_lyric_line().is_some();
    let sub = nt_media_sub();
    let sub_show = {
        let fx = TF_XS.borrow();
        let fitted = nt_fit(&sub, fx.as_ref(), bx - sf(8.0) - tx);
        if fitted.is_empty() { " ".to_string() } else { fitted }
    };
    let title = nt_wstr(&MEDIA.borrow().title);
    text(&title, TF_SS.borrow().as_ref(), tx, cy - sf(17.0), bx - sf(8.0), cy, p.text, 1.0);
    text(&sub_show, TF_XS.borrow().as_ref(), tx, cy, bx - sf(8.0), cy + sf(16.0), p.sub, 1.0);
    if lyr_on {
        nt_lyr_progress(tx, bx - sf(8.0), h);
    }
    for i in 0..3i32 {
        let cx = bx + sf(15.0) + i as f32 * sf(34.0);
        let fill = if i == 1 { Some(p.chip_hover) } else { None };
        nt_round_btn(cx, cy, sf(15.0), NI_PREV + i, fill);
        let g: u16 = if i == 0 {
            0xE892
        } else if i == 2 {
            0xE893
        } else if MEDIA.borrow().status == 4 {
            0xE769
        } else {
            0xE768
        };
        glyph(g, TF_GLYPH_MD.borrow().as_ref(), cx, cy, sf(12.0), p.glyph, 1.0);
    }
}

// Nilai NT_MSG_GLYPH untuk pesan pengisi daya tersambung (bukan kode glyph sungguhan).
const NT_GLYPH_CHG: u16 = 0xFFFF;

fn nt_draw_hud(w: f32, h: f32) {
    let p = *PALETTE.borrow();
    let cy = h * 0.5;
    let hud = NT_HUD_KIND.get();
    if hud == HUD_NOTIF {
        nt_draw_notif(w, h);
        return;
    }
    if hud == HUD_MSG {
        let msg = nt_msg_string();
        if NT_MSG_WARN.get() {
            // peringatan baterai: ikon baterai digambar sendiri (glyph font tampil blok di Windows ini)
            let warn = rgba(232, 72, 85, 1.0);
            draw_battery(sf(26.0), cy, sf(17.0), warn, p.veil);
            text(&msg, TF_S.borrow().as_ref(), sf(46.0), 0.0, w - sf(16.0), h, warn, 1.0);
            return;
        }
        if NT_MSG_GLYPH.get() == NT_GLYPH_CHG {
            // pengisi daya tersambung: ikon baterai berwarna aksen (glyph font tampil blok di Windows ini)
            draw_battery(sf(26.0), cy, sf(17.0), p.accent, p.veil);
            text(&msg, TF_S.borrow().as_ref(), sf(46.0), 0.0, w - sf(16.0), h, p.text, 1.0);
            return;
        }
        let gl = NT_MSG_GLYPH.get();
        let g = if gl != 0 { gl } else { 0xE916 };
        let gc = if NT_ALERT_RED.get() { rgba(232, 72, 85, 1.0) } else { p.accent_ink };
        glyph(g, TF_GLYPH.borrow().as_ref(), sf(26.0), cy, sf(10.0), gc, 1.0);
        text(&msg, TF_S.borrow().as_ref(), sf(46.0), 0.0, w - sf(16.0), h, p.text, 1.0);
        return;
    }
    let vol = hud == HUD_VOL;
    let raw = if vol { if MUTE.get() { 0 } else { VOL.get() } } else { NT_BRI.get() };
    let v = raw.max(0);
    let g: u16 = if vol { vol_glyph() } else { 0xE706 };
    glyph(g, TF_GLYPH.borrow().as_ref(), sf(26.0), cy, sf(10.0), p.glyph, 1.0);
    let pct = format!("{}%", v);
    nt_text_r(&pct, TF_S.borrow().as_ref(), w - sf(18.0), 0.0, h, p.sub);
    let bc = if vol && MUTE.get() { p.sub } else { p.accent };
    nt_bar(sf(48.0), cy, w - sf(62.0), sf(4.0), v as f32 / 100.0, bc);
}

// ---------------------------------------------------------------- gambar: kartu terbuka (w12notch.inc:629-715)

// Kartu media: sampul, judul, garis waktu, dan tiga tombol (NtHomeMedia, w12notch.inc:630-666).
fn nt_home_media(l: f32, t: f32, r: f32, b: f32) {
    let p = *PALETTE.borrow();
    nt_card(l, t, r, b);
    let pad = sf(12.0);
    let cx = (l + r) * 0.5;
    nt_vol_row(l + pad - sf(4.0), r - pad, b - pad - sf(10.0)); // volume selalu ada, juga tanpa media
    let b = b - sf(30.0); // sisa kartu untuk media
    if !nt_has_media() {
        glyph(0xE8D6, TF_GLYPH_LG.borrow().as_ref(), cx, (t + b) * 0.5 - sf(16.0), sf(14.0), p.sub, 1.0);
        wf_text_c("Tidak ada media diputar", TF_S.borrow().as_ref(), cx, (t + b) * 0.5 + sf(4.0), (t + b) * 0.5 + sf(22.0), &p.sub);
        return;
    }
    let art = sf(64.0);
    nt_hit(l + pad, t + pad, r - pad, t + pad + art, NI_MEDIA); // sampul dan judul: buka aplikasi sumbernya
    draw_art(l + pad, t + pad, art);
    let tx = l + pad + art + sf(10.0);
    let sub = nt_media_sub();
    let sub_show = {
        let fx = TF_XS.borrow();
        let fitted = nt_fit(&sub, fx.as_ref(), r - pad - tx);
        if fitted.is_empty() { " ".to_string() } else { fitted }
    };
    let title = nt_wstr(&MEDIA.borrow().title);
    text(&title, TF_SS.borrow().as_ref(), tx, t + pad + sf(12.0), r - pad, t + pad + sf(32.0), p.text, 1.0);
    text(&sub_show, TF_XS.borrow().as_ref(), tx, t + pad + sf(32.0), r - pad, t + pad + sf(50.0), p.sub, 1.0);
    // garis waktu: posisi dan durasi (detik)
    let (mut pos, mut dur) = (0i32, 0i32);
    if nt_timeline(&mut pos, &mut dur) {
        let py = t + pad + art + sf(14.0);
        let lw = sf(34.0);
        let a = nt_mmss(pos);
        let z = nt_mmss(dur);
        text(&a, TF_XS.borrow().as_ref(), l + pad, py - sf(9.0), l + pad + lw, py + sf(9.0), p.sub, 1.0);
        nt_text_r(&z, TF_XS.borrow().as_ref(), r - pad, py - sf(9.0), py + sf(9.0), p.sub);
        let (bl, br) = (l + pad + lw + sf(2.0), r - pad - lw - sf(2.0));
        nt_bar(bl, py, br, sf(3.0), pos as f32 / dur as f32, p.text);
        // bar progres bisa diklik untuk seek; geometri dicatat untuk perhitungan fraksi di nt_act
        NT_TL_BAR.set((bl, br, py));
        nt_hit(bl, py - sf(8.0), br, py + sf(8.0), NI_SEEK);
    }
    let cy = b - pad - sf(20.0);
    for i in 0..3i32 {
        let x = cx + (i - 1) as f32 * sf(48.0);
        let rb = if i == 1 { sf(20.0) } else { sf(17.0) };
        let fill = if i == 1 { Some(p.text) } else { None };
        nt_round_btn(x, cy, rb, NI_PREV + i, fill);
        let g: u16 = if i == 0 {
            0xE892
        } else if i == 2 {
            0xE893
        } else if MEDIA.borrow().status == 4 {
            0xE769
        } else {
            0xE768
        };
        let gc = if i == 1 {
            if LIGHT.get() { rgba(255, 255, 255, 1.0) } else { rgba(18, 22, 58, 1.0) }
        } else {
            p.glyph
        };
        glyph(g, TF_GLYPH_MD.borrow().as_ref(), x, cy, sf(12.0), gc, 1.0);
    }
}

// Kartu pomodoro: cincin, sisa waktu, dan chip durasi (NtHomeTimer, w12notch.inc:668-690).
fn nt_home_timer(l: f32, t: f32, r: f32, b: f32) {
    let p = *PALETTE.borrow();
    nt_card(l, t, r, b);
    let pad = sf(12.0);
    let cx = (l + r) * 0.5;
    let cy = t + sf(76.0);
    text("Fokus (Pomodoro)", TF_SS.borrow().as_ref(), l + pad, t + pad, r - pad, t + pad + sf(16.0), p.sub, 1.0);
    let left = pm_left();
    let pm = PM_STATE.get();
    let frac = if pm != 0 { left as f32 / PM_TOTAL.get().max(1) as f32 } else { 0.0 };
    let ring_c = if pm == 2 { p.sub } else { p.accent };
    nt_ring(cx, cy, sf(42.0), sf(6.0), frac, ring_c);
    nt_hit(cx - sf(46.0), cy - sf(46.0), cx + sf(46.0), cy + sf(46.0), NI_RING);
    let tm = nt_mmss(left);
    wf_text_c(&tm, TF_BIG.borrow().as_ref(), cx, cy - sf(13.0), cy + sf(13.0), &p.text);
    // chip: 25 m, 5 m, lalu Jeda/Lanjut/Mulai
    let third = if pm == 1 {
        "Jeda"
    } else if pm == 2 {
        "Lanjut"
    } else {
        "Mulai"
    };
    let w1 = measure_text("25 m", TF_XS.borrow().as_ref()) + sf(20.0);
    let w2 = measure_text("5 m", TF_XS.borrow().as_ref()) + sf(20.0);
    let w3 = measure_text(third, TF_XS.borrow().as_ref()) + sf(20.0);
    let mut x = cx - (w1 + w2 + w3 + sf(12.0)) * 0.5;
    let c0 = b - pad - sf(22.0);
    let c1 = b - pad;
    nt_chip(x, c0, x + w1, c1, "25 m", NI_PM25, pm != 0 && PM_TOTAL.get() == 25 * 60);
    x += w1 + sf(6.0);
    nt_chip(x, c0, x + w2, c1, "5 m", NI_PM5, pm != 0 && PM_TOTAL.get() == 5 * 60);
    x += w2 + sf(6.0);
    nt_chip(x, c0, x + w3, c1, third, NI_PMTOG, false);
}

// Kartu sistem: CPU, RAM, GPU, dan baterai bila ada; jenis jaringan di bawah (NtHomeSys, w12notch.inc:692-715).
fn nt_home_sys(l: f32, t: f32, r: f32, b: f32) {
    let p = *PALETTE.borrow();
    nt_card(l, t, r, b);
    nt_hover(l, t, r, b, sf(16.0), NI_SYS); // klik: Pengelola Tugas
    let pad = sf(12.0);
    text("Sistem", TF_SS.borrow().as_ref(), l + pad, t + pad, r - pad, t + pad + sf(16.0), p.sub, 1.0);
    // baterai menjadi batang keempat (merah bila <= 15% tanpa adaptor)
    let bat = HAS_BAT.get() && BAT.get() >= 0;
    let low = bat && !CHARGING.get() && BAT.get() <= 15;
    let lb = ["CPU", "RAM", "GPU", "Bat"];
    let val = [CPU.get(), RAM.get(), GPU.get(), if bat { BAT.get() as f32 } else { -1.0 }];
    let n: usize = if bat { 4 } else { 3 };
    for i in 0..n {
        let y = t + sf(46.0) + i as f32 * sf(28.0);
        text(lb[i], TF_SS.borrow().as_ref(), l + pad, y - sf(9.0), l + pad + sf(34.0), y + sf(9.0), p.sub, 1.0);
        let v = if val[i] >= 0.0 { format!("{}%", (val[i] + 0.5) as i32) } else { "\u{2014}".to_string() };
        nt_text_r(&v, TF_XS.borrow().as_ref(), r - pad, y - sf(9.0), y + sf(9.0), p.text);
        let bc = if i == 3 && low { rgba(232, 72, 85, 1.0) } else { p.accent };
        nt_bar(l + pad + sf(36.0), y, r - pad - sf(38.0), sf(5.0), val[i] / 100.0, bc);
    }
    let nt = if NET.get() == NET_WIFI {
        "Wi-Fi"
    } else if NET.get() == NET_NONE {
        "Offline"
    } else {
        "Ethernet"
    };
    let foot = if bat && CHARGING.get() { format!("{} \u{00B7} adaptor tersambung", nt) } else { nt.to_string() };
    text(&foot, TF_XS.borrow().as_ref(), l + pad, b - pad - sf(18.0), r - pad, b - pad, p.sub, 1.0);
    // chip "Media": pulau media langsung (NOTCH_LIVE); didaftarkan setelah area kartu agar klik chip diutamakan
    nt_chip(r - pad - sf(64.0), t + sf(8.0), r - pad, t + sf(30.0), "Media", NI_SYS_MEDIA, NOTCH_LIVE.get());
    // chip mode notch: aktif = selalu tampil (garis tetap di tepi atas), nonaktif = auto-hide
    let al = if NOTCH_ALWAYS.get() { "Selalu tampil" } else { "Auto-hide" };
    nt_chip(r - pad - sf(170.0), t + sf(8.0), r - pad - sf(70.0), t + sf(30.0), al, NI_SYS_ALWAYS, NOTCH_ALWAYS.get());
}

// Tab pengatur waktu: cincin besar, sisa waktu, durasi, dan tombol Mulai/Hentikan (NtTimerTab, w12notch.inc:717-749).
fn nt_timer_tab(l: f32, t: f32, r: f32, b: f32) {
    let p = *PALETTE.borrow();
    nt_card(l, t, r, b);
    let pad = sf(14.0);
    let cx = l + sf(98.0);
    let cy = (t + b) * 0.5;
    let pm = PM_STATE.get();
    let left = pm_left();
    let frac = if pm != 0 { left as f32 / PM_TOTAL.get().max(1) as f32 } else { 0.0 };
    nt_ring(cx, cy, sf(62.0), sf(8.0), frac, if pm == 2 { p.sub } else { p.accent });
    nt_hit(cx - sf(66.0), cy - sf(66.0), cx + sf(66.0), cy + sf(66.0), NI_RING);
    let tm = nt_mmss(left);
    wf_text_c(&tm, TF_PCT.borrow().as_ref(), cx, cy - sf(24.0), cy + sf(14.0), &p.text);
    let mode = if pm == 2 {
        "Dijeda"
    } else if pm == 0 {
        "Siap"
    } else if PM_TOTAL.get() <= 5 * 60 {
        "Istirahat"
    } else {
        "Fokus"
    };
    wf_text_c(mode, TF_XS.borrow().as_ref(), cx, cy + sf(16.0), cy + sf(32.0), &p.sub);
    let x0 = l + sf(196.0);
    let x1 = r - pad;
    text("Durasi", TF_SS.borrow().as_ref(), x0, t + pad, x1, t + pad + sf(18.0), p.sub, 1.0);
    // durasi kustom di kanan baris "Durasi": [-] [N m] [+]; chip tengah memulai timer dengan durasi itu
    let up_l = x1 - sf(30.0);
    let mid_r = up_l - sf(6.0);
    let mid_l = mid_r - sf(56.0);
    let dn_r = mid_l - sf(6.0);
    let dn_l = dn_r - sf(30.0);
    let cm = NT_PM_CUSTOM.get();
    let cm_on = pm != 0 && PM_TOTAL.get() == cm * 60;
    nt_chip(dn_l, t + sf(8.0), dn_r, t + sf(38.0), "-", NI_PMADJ_DN, false);
    nt_chip(mid_l, t + sf(8.0), mid_r, t + sf(38.0), &format!("{} m", cm), NI_PMCUSTOM, cm_on);
    nt_chip(up_l, t + sf(8.0), x1, t + sf(38.0), "+", NI_PMADJ_UP, false);
    let cw = (x1 - x0 - 5.0 * sf(8.0)) / 6.0;
    for i in 0..6usize {
        let s = format!("{} m", NT_PRESETS[i]);
        let cl = x0 + i as f32 * (cw + sf(8.0));
        let on = pm != 0 && PM_TOTAL.get() == NT_PRESETS[i] * 60;
        nt_chip(cl, t + sf(42.0), cl + cw, t + sf(72.0), &s, NI_PRESET + i as i32, on);
    }
    let go = if pm == 1 {
        "Jeda"
    } else if pm == 2 {
        "Lanjut"
    } else {
        "Mulai"
    };
    nt_button(x0, t + sf(86.0), x0 + sf(120.0), t + sf(118.0), go, NI_PMGO, true);
    if pm != 0 {
        nt_button(x0 + sf(128.0), t + sf(86.0), x0 + sf(248.0), t + sf(118.0), "Hentikan", NI_PMSTOP, false);
    }
    let mut info = String::from("Pilih durasi untuk mulai");
    if pm == 1 {
        let st = nt_local_time();
        let end_min = (st.wHour as i32 * 60 + st.wMinute as i32 + (left + st.wSecond as i32 + 59) / 60) % (24 * 60);
        info = format!("Selesai pukul {:02}.{:02}", end_min / 60, end_min % 60);
    } else if pm == 2 {
        info = "Dijeda \u{00B7} tekan Lanjut untuk meneruskan".to_string();
    }
    text(&info, TF_S.borrow().as_ref(), x0, t + sf(128.0), x1, t + sf(148.0), p.sub, 1.0);
}

// Tab catatan: hingga lima catatan terakhir dan tombol buka notes.txt (NtNotesTab, w12notch.inc:751-771).
fn nt_notes_tab(l: f32, t: f32, r: f32, b: f32) {
    let p = *PALETTE.borrow();
    nt_card(l, t, r, b);
    let pad = sf(14.0);
    text("Catatan terakhir", TF_SS.borrow().as_ref(), l + pad, t + sf(10.0), r - sf(150.0), t + sf(30.0), p.sub, 1.0);
    let lw = measure_text("Buka notes.txt", TF_XS.borrow().as_ref());
    nt_hover(r - pad - lw - sf(16.0), t + sf(8.0), r - pad + sf(4.0), t + sf(32.0), sf(12.0), NI_NOTEFILE);
    nt_text_r("Buka notes.txt", TF_XS.borrow().as_ref(), r - pad - sf(4.0), t + sf(8.0), t + sf(32.0), p.accent_ink);
    let n = NT_N_NOTES.get();
    if n == 0 {
        wf_text_c("Belum ada catatan. Tulis di bawah lalu tekan Enter.", TF_S.borrow().as_ref(), (l + r) * 0.5, (t + b) * 0.5, (t + b) * 0.5 + sf(20.0), &p.sub);
        return;
    }
    // selisih waktu memakai wrapping_sub agar tidak panik bila penghitung tick berbalik
    let copied = NT_COPIED.get() >= 0 && nt_tick64().wrapping_sub(NT_COPIED_T.get()) < 1600;
    for i in 0..n {
        let y0 = t + sf(38.0) + i as f32 * sf(23.0);
        let y1 = y0 + sf(22.0);
        nt_hover(l + sf(6.0), y0, r - sf(6.0), y1, sf(8.0), NI_NOTEROW + i); // klik: salin ke clipboard
        let note = nt_wstr(&NT_NOTES.borrow()[i as usize]);
        text(&note, TF_S.borrow().as_ref(), l + pad, y0, r - sf(84.0), y1, p.text, 1.0);
        if copied && NT_COPIED.get() == i {
            nt_text_r("Disalin", TF_XS.borrow().as_ref(), r - pad, y0, y1, p.accent_ink);
        } else {
            let tm = nt_wstr(&NT_NOTE_T.borrow()[i as usize]);
            nt_text_r(&tm, TF_XS.borrow().as_ref(), r - pad, y0, y1, p.sub);
        }
    }
}

// Baris catatan cepat: teks yang sedang diketik, caret, dan petunjuk Enter (NtNoteRow, w12notch.inc:773-796).
fn nt_note_row(l: f32, t: f32, r: f32, b: f32) {
    let p = *PALETTE.borrow();
    bt_card(l, t, r, b, sf(12.0), &p.chip, &p.line, 1.0);
    nt_hit(l, t, r, b, NI_NOTE);
    let cy = (t + b) * 0.5;
    let x0 = l + sf(34.0);
    let x1 = r - sf(72.0);
    glyph(0xE70F, TF_GLYPH_SM.borrow().as_ref(), l + sf(18.0), cy, sf(8.0), p.sub, 1.0);
    let active = unsafe { GetForegroundWindow() } == NT_WND.get();
    let mut caret = x0;
    if NT_NOTE_LEN.get() != 0 {
        let note = nt_wstr(&NT_NOTE.borrow()[..]);
        let mw = measure_text(&note, TF_S.borrow().as_ref());
        let tx = if mw > x1 - x0 - sf(4.0) { x1 - sf(4.0) - mw } else { x0 }; // teks panjang: tampilkan ekornya
        let clip = rf(x0, t, x1, b);
        {
            let rt_g = RT.borrow();
            if let Some(rt) = rt_g.as_ref() {
                unsafe { rt.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
            }
        }
        text(&note, TF_S.borrow().as_ref(), tx, t, tx + mw + sf(4.0), b, p.text, 1.0);
        {
            let rt_g = RT.borrow();
            if let Some(rt) = rt_g.as_ref() {
                unsafe { rt.PopAxisAlignedClip() };
            }
        }
        caret = tx + mw + sf(1.0);
    } else {
        let saved = nt_tick64().wrapping_sub(NT_SAVED.get()) < 1600;
        let (msg, mc) = if saved {
            ("Tersimpan ke notes.txt", p.accent_ink)
        } else {
            ("Catatan cepat\u{2026}", p.sub2)
        };
        text(msg, TF_S.borrow().as_ref(), x0 + sf(3.0), t, x1, b, mc, 1.0);
    }
    if active {
        fill_rr(caret, cy - sf(8.0), caret + sf(1.5), cy + sf(8.0), 0.0, p.accent_ink, 1.0);
    }
    bt_card(r - sf(60.0), cy - sf(10.0), r - sf(12.0), cy + sf(10.0), sf(6.0), &p.chip, &p.line, 1.0);
    wf_text_c("Enter", TF_XS.borrow().as_ref(), r - sf(36.0), cy - sf(10.0), cy + sf(10.0), &p.sub);
}

// Kartu terbuka: bilah tab, jam/tanggal, gugus status, isi tab, lalu baris catatan (w12notch.inc:798-828)
// ---------------------------------------------------------------- Pengaturan notch (2026-11-19)
// Gaya Pengaturan Windows 11: navigasi bagian di kiri, baris (judul + keterangan) dengan sakelar atau kontrol segmen
// di kanan. Id lama (NI_SYS_*) tetap dipakai agar penanganan klik kartu Sistem tidak berubah.
const NI_ST_SEC: i32 = 100; // 100..103: navigasi bagian
const NI_ST_HUD: i32 = 104; // notchhud: volume, kecerahan, baterai, pesan status
const NI_ST_NOTIF: i32 = 105; // notchnotif: notifikasi aplikasi lewat notch
const NI_ST_ONLY: i32 = 106; // notchonly: sembunyikan banner Windows
const NI_ST_FSV: i32 = 110; // 110..112: notchfsvideo 0/1/2
const NI_ST_FSG: i32 = 113; // 113..115: notchfsgame 0/1/2
static NT_SET_SEC: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
// Salinan notchnotif / notchonly (-1 = belum dibaca) agar render tidak membaca settings.ini tiap frame.
static NT_CFG_NOTIF: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
static NT_CFG_ONLY: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));

fn nt_ini_flag(c: &SyncCell<Cell<i32>>, key: &str) -> bool {
    if c.get() < 0 {
        c.set(if ini_int("shell", key, 1) != 0 { 1 } else { 0 });
    }
    c.get() != 0
}

fn nt_ini_toggle(c: &SyncCell<Cell<i32>>, key: &str) {
    let on = !nt_ini_flag(c, key);
    c.set(on as i32);
    let _ = ini_write("shell", key, if on { "1" } else { "0" });
    super::notif_worker::apply_settings();
}

/// Sakelar Windows 11 (40x20): nyala = isi aksen + kenop putih di kanan; mati = garis tepi + kenop di kiri.
fn nt_switch(r: f32, cy: f32, on: bool, id: i32) {
    let p = *PALETTE.borrow();
    let (w, h) = (sf(40.0), sf(20.0));
    let (l, t) = (r - w, cy - h * 0.5);
    nt_hit(l - sf(6.0), t - sf(6.0), r + sf(4.0), t + h + sf(6.0), id);
    let hov = !NT_NO_HIT.get() && NT_HOV.get() == id;
    let down = !NT_NO_HIT.get() && NT_DOWN.get() == id;
    let kr = if down { sf(7.0) } else if hov { sf(6.0) } else { sf(5.0) };
    if on {
        fill_rr(l, t, r, t + h, h * 0.5, p.accent, if hov { 0.9 } else { 1.0 });
        fill_rr(r - h * 0.5 - kr, cy - kr, r - h * 0.5 + kr, cy + kr, kr, rgba(255, 255, 255, 1.0), 1.0);
    } else {
        let fill = if hov { p.hover } else { p.chip };
        bt_card(l, t, r, t + h, h * 0.5, &fill, &p.sub, 1.0);
        fill_rr(l + h * 0.5 - kr, cy - kr, l + h * 0.5 + kr, cy + kr, kr, p.sub, 1.0);
    }
}

/// Kontrol segmen (pilihan tunggal); `sel` = indeks terpilih, id = id0 + indeks.
fn nt_segment(l: f32, t: f32, r: f32, b: f32, labels: &[&str], sel: usize, id0: i32) {
    let p = *PALETTE.borrow();
    let rad = sf(6.0);
    bt_card(l, t, r, b, rad, &p.chip_hover, &p.line, 1.0);
    let n = labels.len().max(1) as f32;
    let sw = (r - l) / n;
    for (i, s) in labels.iter().enumerate() {
        let sl = l + sw * i as f32;
        let id = id0 + i as i32;
        if i == sel {
            bt_card(sl + sf(2.0), t + sf(2.0), sl + sw - sf(2.0), b - sf(2.0), rad - sf(1.0), &p.chip, &p.line, 1.0);
            // garis aksen kecil di bawah pilihan, seperti pemilih segmen Windows 11
            let cx = sl + sw * 0.5;
            fill_rr(cx - sf(8.0), b - sf(5.0), cx + sf(8.0), b - sf(3.0), sf(1.0), p.accent, 1.0);
            nt_hit(sl, t, sl + sw, b, id);
        } else {
            nt_hover(sl + sf(2.0), t + sf(2.0), sl + sw - sf(2.0), b - sf(2.0), rad - sf(1.0), id);
        }
        let c = if i == sel { p.text } else { p.sub };
        wf_text_c(s, TF_XS.borrow().as_ref(), sl + sw * 0.5, t, b - sf(2.0), &c);
    }
}

/// Satu baris pengaturan: judul + keterangan di kiri (sampai `tr`), kontrol digambar pemanggil di kanan.
fn nt_set_row(l: f32, t: f32, tr: f32, h: f32, title: &str, desc: &str) {
    let p = *PALETTE.borrow();
    if desc.is_empty() {
        text(title, TF_SS.borrow().as_ref(), l, t, tr, t + h, p.text, 1.0);
    } else {
        let m = (h - sf(34.0)).max(0.0) * 0.5;
        text(title, TF_SS.borrow().as_ref(), l, t + m, tr, t + m + sf(18.0), p.text, 1.0);
        text(desc, TF_XS.borrow().as_ref(), l, t + m + sf(17.0), tr, t + m + sf(34.0), p.sub, 1.0);
    }
}

fn nt_settings_tab(l: f32, t: f32, r: f32, b: f32) {
    let p = *PALETTE.borrow();
    nt_card(l, t, r, b);
    // navigasi bagian (kiri)
    const SL: [&str; 4] = ["Tampilan", "Media", "Notifikasi", "Layar penuh"];
    const SG: [u16; 4] = [0xE790, 0xE8D6, 0xEA8F, 0xE740];
    let sec = NT_SET_SEC.get().clamp(0, 3);
    let nl = l + sf(8.0);
    let nr = l + sf(148.0);
    let ih = sf(36.0);
    let mut y = t + sf(8.0);
    for i in 0..4usize {
        let on = sec == i as i32;
        let id = NI_ST_SEC + i as i32;
        if on {
            fill_rr(nl, y, nr, y + ih, sf(8.0), p.chip_hover, 1.0);
            // indikator aksen vertikal di kiri, seperti navigasi Pengaturan Windows 11
            fill_rr(nl, y + sf(10.0), nl + sf(3.0), y + ih - sf(10.0), sf(1.5), p.accent, 1.0);
            nt_hit(nl, y, nr, y + ih, id);
        } else {
            nt_hover(nl, y, nr, y + ih, sf(8.0), id);
        }
        let c = if on { p.text } else { p.sub };
        glyph(SG[i], TF_GLYPH_SM.borrow().as_ref(), nl + sf(20.0), y + ih * 0.5, sf(8.0), c, 1.0);
        let f = if on { TF_SS.borrow() } else { TF_S.borrow() };
        text(SL[i], f.as_ref(), nl + sf(36.0), y, nr - sf(4.0), y + ih, c, 1.0);
        y += ih + sf(4.0);
    }
    fill_rr(nr + sf(8.0), t + sf(12.0), nr + sf(9.0), b - sf(12.0), 0.0, p.line, 1.0);
    // isi bagian (kanan)
    let cl = nr + sf(22.0);
    let cr = r - sf(16.0);
    text(SL[sec as usize], TF_S.borrow().as_ref(), cl, t + sf(8.0), cr, t + sf(32.0), p.text, 1.0);
    let rh = sf(46.0);
    let mut y = t + sf(36.0);
    let sw_r = cr; // sakelar rata kanan
    let tr = cr - sf(56.0);
    let row_sep = |y: f32| fill_rr(cl, y, cr, y + sf(1.0), 0.0, p.line, 1.0);
    match sec {
        0 => {
            nt_set_row(cl, y, tr, rh, "Selalu tampilkan garis notch", "Mati: muncul saat kursor di tepi atas tengah");
            nt_switch(sw_r, y + rh * 0.5, NOTCH_ALWAYS.get(), NI_SYS_ALWAYS);
            y += rh;
            row_sep(y);
            nt_set_row(cl, y, tr, rh, "Status sistem di notch", "Volume, kecerahan, baterai, dan pengisi daya");
            nt_switch(sw_r, y + rh * 0.5, NOTCH_HUD.get(), NI_ST_HUD);
        }
        1 => {
            nt_set_row(cl, y, tr, rh, "Pulau media", "Sampul, judul, dan kontrol saat musik diputar");
            nt_switch(sw_r, y + rh * 0.5, NOTCH_LIVE.get(), NI_SYS_MEDIA);
            y += rh;
            row_sep(y);
            nt_set_row(cl, y, tr, rh, "Hanya muncul sebentar", "Intip 3 detik saat lagu berganti atau dijeda");
            nt_switch(sw_r, y + rh * 0.5, NOTCH_MEDIA_TEMP.get(), NI_SYS_MEDTEMP);
            y += rh;
            row_sep(y);
            nt_set_row(cl, y, tr, rh, "Lirik lagu", "Baris lirik tersinkron dari LRCLIB");
            nt_switch(sw_r, y + rh * 0.5, NOTCH_LYRICS.get(), NI_SYS_LYR);
        }
        2 => {
            let on = nt_ini_flag(&NT_CFG_NOTIF, "notchnotif");
            nt_set_row(cl, y, tr, rh, "Notifikasi lewat notch", "Kartu notifikasi aplikasi muncul di pulau");
            nt_switch(sw_r, y + rh * 0.5, on, NI_ST_NOTIF);
            y += rh;
            row_sep(y);
            let only = on && nt_ini_flag(&NT_CFG_ONLY, "notchonly");
            nt_set_row(cl, y, tr, rh, "Sembunyikan banner Windows", "Alarm dan panggilan tetap memakai banner");
            nt_switch(sw_r, y + rh * 0.5, only, NI_ST_ONLY);
            y += rh;
            row_sep(y);
            nt_set_row(cl, y, cr - sf(150.0), rh, "Notifikasi per aplikasi", "Atur di Pengaturan Windows");
            nt_button(cr - sf(140.0), y + rh * 0.5 - sf(13.0), cr, y + rh * 0.5 + sf(13.0), "Buka Pengaturan", NI_SET_NOTIF, false);
        }
        _ => {
            const OPT: [&str; 3] = ["Sembunyi", "Notifikasi", "+ Media"];
            let segl = cr - sf(228.0);
            let tr = segl - sf(10.0);
            let fs_now = super::state::FS_KIND.get();
            let dv = if fs_now == 1 { "Peramban, YouTube, pemutar \u{00B7} aktif" } else { "Peramban, YouTube, pemutar" };
            nt_set_row(cl, y, tr, rh, "Video layar penuh", dv);
            let sv = super::state::NOTCH_FS_VIDEO.get().min(2) as usize;
            nt_segment(segl, y + rh * 0.5 - sf(14.0), cr, y + rh * 0.5 + sf(14.0), &OPT, sv, NI_ST_FSV);
            y += rh;
            row_sep(y);
            let dg = if fs_now == 2 { "Game dan aplikasi lain \u{00B7} aktif" } else { "Game dan aplikasi lain" };
            nt_set_row(cl, y, tr, rh, "Game layar penuh", dg);
            let sg = super::state::NOTCH_FS_GAME.get().min(2) as usize;
            nt_segment(segl, y + rh * 0.5 - sf(14.0), cr, y + rh * 0.5 + sf(14.0), &OPT, sg, NI_ST_FSG);
            y += rh;
            let note = "+ Media: juga intip lagu dan buka notch dari tepi atas.";
            text(note, TF_XS.borrow().as_ref(), cl, y + sf(6.0), cr, b - sf(6.0), p.sub, 1.0);
        }
    }
}

fn nt_draw_open(w: f32, h: f32) {
    let p = *PALETTE.borrow();
    let pad = sf(12.0);
    let t0 = pad;
    let t1 = pad + sf(30.0);
    const TL: [&str; 4] = ["Beranda", "Timer", "Catatan", "Notifikasi"];
    const TG: [u16; 4] = [0xE80F, 0xE916, 0xE70B, 0xEA8F];
    let tab = NT_TAB.get();
    let mut x = pad;
    for i in 0..4usize {
        let on = tab == i as i32;
        let fb = if on { TF_SS.borrow() } else { TF_S.borrow() };
        let f = fb.as_ref();
        let tw = sf(36.0) + measure_text(TL[i], f) + sf(12.0);
        let c = if on { p.text } else { p.sub };
        if on {
            fill_rr(x, t0, x + tw, t1, sf(15.0), p.chip_hover, 1.0);
        }
        nt_hover(x, t0, x + tw, t1, sf(15.0), NI_TAB + i as i32);
        glyph(TG[i], TF_GLYPH_SM.borrow().as_ref(), x + sf(20.0), (t0 + t1) * 0.5, sf(8.0), c, 1.0);
        text(TL[i], f, x + sf(34.0), t0, x + tw, t1, c, 1.0);
        x += tw + sf(6.0);
    }
    // tombol roda gigi: membuka/menutup panel Pengaturan notch
    let gs = sf(30.0);
    let on_set = NT_SET.get();
    if on_set {
        fill_rr(x, t0, x + gs, t1, sf(15.0), p.chip_hover, 1.0);
    }
    nt_hover(x, t0, x + gs, t1, sf(15.0), NI_SET);
    glyph(0xE713, TF_GLYPH_SM.borrow().as_ref(), x + gs * 0.5, (t0 + t1) * 0.5, sf(8.0), if on_set { p.text } else { p.sub }, 1.0);
    x += gs + sf(6.0);
    let ds = DATE_SHORT.borrow().clone();
    let tz = TIME_TZ.borrow().clone();
    let dw = measure_text(&ds, TF_S.borrow().as_ref());
    let tw = measure_text(&tz, TF_SS.borrow().as_ref());
    nt_text_r(&ds, TF_S.borrow().as_ref(), w - pad - sf(4.0), t0, t1, p.sub);
    nt_text_r(&tz, TF_SS.borrow().as_ref(), w - pad - sf(4.0) - dw - sf(10.0), t0, t1, p.text);
    let sx = w - pad - sf(4.0) - dw - sf(10.0) - tw - sf(14.0);
    if sx - sf(110.0) > x {
        // layar sempit: gugus status dilewati
        nt_status(sx, (t0 + t1) * 0.5, TF_XS.borrow().as_ref(), p.panel, true);
    }
    let b0 = t1 + sf(10.0);
    let b1 = h - pad - sf(34.0) - sf(10.0);
    if NT_SET.get() {
        nt_settings_tab(pad, b0, w - pad, b1);
    } else if tab == NTT_HOME {
        let gap = sf(10.0);
        let avail = w - 2.0 * pad - 2.0 * gap;
        let w1 = avail * 1.35 / 3.35;
        let w2 = avail / 3.35;
        nt_home_media(pad, b0, pad + w1, b1);
        nt_home_timer(pad + w1 + gap, b0, pad + w1 + gap + w2, b1);
        nt_home_sys(pad + w1 + 2.0 * gap + w2, b0, w - pad, b1);
    } else if tab == NTT_TIMER {
        nt_timer_tab(pad, b0, w - pad, b1);
    } else if tab == NTT_NOTIF {
        nt_notif_tab(pad, b0, w - pad, b1);
    } else {
        nt_notes_tab(pad, b0, w - pad, b1);
    }
    nt_note_row(pad, h - pad - sf(34.0), w - pad, h - pad);
}

// Isi kartu sesuai keadaan; NT_IDLE tidak digambar (tersembunyi). (w12notch.inc:830-838)
fn nt_content(st: i32, w: f32, h: f32) {
    match st {
        NT_PEEK => nt_draw_peek(w, h),
        NT_HUD => nt_draw_hud(w, h),
        NT_OPEN => nt_draw_open(w, h),
        _ => {} // NT_IDLE: tersembunyi, tanpa isi
    }
}

// Gambar seluruh pulau: bayangan, permukaan, lalu isi (atau crossfade saat animasi). (w12notch.inc:841-916)
fn notch_render() {
    let h = NT_WND.get();
    if h.0.is_null() {
        return;
    }
    // SAFETY: h bukan null; RemovePropW hanya menghapus penanda properti milik jendela ini.
    let _ = unsafe { RemovePropW(h, w!("w12p")) };
    if !NT_SHOWN.get() {
        return;
    }
    if NT_STATE.get() == NT_IDLE && NT_T.get() >= 1.0 {
        return; // tersembunyi: jam/tema berubah tidak perlu digambar
    }
    let Some(rt) = RT.borrow().as_ref().cloned() else {
        return;
    };
    let p = *PALETTE.borrow();
    let g = nt_cur();
    let m = sf(18.0) * g.e; // ruang bayangan, hanya saat terbuka
    // SAFETY: GetSystemMetrics hanya membaca metrik layar.
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let shape_x = (sw as f32 - g.w) * 0.5;
    let shape_y = s(NT_TOP as f32) as f32;
    let x0 = (shape_x - m).floor() as i32;
    let y0 = (shape_y - m).floor() as i32;
    let sx = shape_x - x0 as f32;
    let sy = shape_y - y0 as f32;
    let ww = (sx + g.w + m).ceil() as i32 + 1;
    let wh = (sy + g.h + m).ceil() as i32 + 1;
    let wr = RECT { left: x0, top: y0, right: x0 + ww, bottom: y0 + wh };
    ensure_mem(ww, wh);
    let br = RECT { left: 0, top: 0, right: ww, bottom: wh };
    // SAFETY: MEMDC sudah dibuat oleh ensure_mem; br hidup selama pemanggilan BindDC.
    let bind = unsafe { rt.BindDC(MEMDC.get(), &br) };
    if bind.is_err() {
        return;
    }
    NT_N_HIT.set(0);
    NT_NO_HIT.set(false);
    NT_OX.set(sx);
    NT_OY.set(sy);
    let o = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: sx, M32: sy };
    let ident = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 };
    // SAFETY: rt valid dan terikat ke MEMDC; BeginDraw dan EndDraw dipasangkan di bawah.
    unsafe {
        rt.BeginDraw();
        rt.Clear(Some(&rgba(0, 0, 0, 0.0)));
        rt.SetTransform(&o);
    }
    mica_begin(h, &wr);
    if g.e > 0.01 {
        // bayangan: 8 lapis saat diam, 4 lapis (selang 2x, alfa 2x) selama animasi = separuh isi piksel
        let n: i32 = if NT_T.get() < 1.0 { 4 } else { 8 };
        for i in 0..n {
            let e = sf((i * 16 / n) as f32);
            fill_rr(-e, -e + sf(5.0), g.w + e, g.h + e + sf(5.0), g.rad + e, p.shadow, 0.56 / n as f32 * g.e);
        }
    }
    surface(0.0, 0.0, g.w, g.h, g.rad, mix_c(p.veil, p.panel, g.e));
    nt_hit(0.0, 0.0, g.w, g.h, NI_BODY);
    if NT_T.get() >= 1.0 {
        nt_content(NT_STATE.get(), g.w, g.h);
    } else {
        // Animasi: isi lama memudar di 35% awal, isi baru muncul sesudahnya; keduanya dipotong ke bentuk pulau.
        NT_NO_HIT.set(true);
        // Sejak Windows 8 PushLayer boleh tanpa objek layer: D2D memakai ulang memorinya antar-frame. Bila ditolak
        // (EndDraw gagal), NT_LAYER_AUTO dimatikan dan layer dibuat per frame seperti sebelumnya.
        let ly: Option<ID2D1Layer> = if NT_LAYER_AUTO.get() {
            None
        } else {
            let made = unsafe { rt.CreateLayer(None) };
            made.ok()
        };
        let use_layer = NT_LAYER_AUTO.get() || ly.is_some();
        let t = NT_T.get();
        for pass in 0..2 {
            let old = pass == 0;
            if old && NT_PREV.get() == NT_STATE.get() {
                continue;
            }
            let st = if old { NT_PREV.get() } else { NT_STATE.get() };
            let mut op = if old {
                1.0 - t / 0.35
            } else if NT_PREV.get() == NT_STATE.get() {
                1.0
            } else {
                (t - 0.35) / 0.65
            };
            if op <= 0.01 {
                continue;
            }
            if op > 1.0 {
                op = 1.0;
            }
            let sz = if old { nt_target(st) } else { NT_TO.get() };
            if use_layer {
                let lp = D2D1_LAYER_PARAMETERS {
                    contentBounds: rf(0.0, 0.0, g.w, g.h),
                    geometricMask: std::mem::ManuallyDrop::new(None),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: ident,
                    opacity: op,
                    opacityBrush: std::mem::ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS_NONE,
                };
                // SAFETY: lp hidup selama pemanggilan; ly None berarti D2D memakai layer internal.
                unsafe { rt.PushLayer(&lp, ly.as_ref()) };
            }
            let oc = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: sx + (g.w - sz.w) * 0.5, M32: sy };
            unsafe { rt.SetTransform(&oc) };
            nt_content(st, sz.w, sz.h);
            unsafe { rt.SetTransform(&o) };
            if use_layer {
                unsafe { rt.PopLayer() };
            }
        }
        NT_NO_HIT.set(false);
    }
    unsafe { rt.SetTransform(&ident) };
    let hr = unsafe { rt.EndDraw(None, None) };
    if let Err(e) = &hr {
        if e.code() == D2DERR_RECREATE_TARGET {
            create_target();
            *NT_ROUND.borrow_mut() = None;
            let _ = w12_invalidate(h, None, false);
            return;
        }
        if NT_T.get() < 1.0 && NT_LAYER_AUTO.get() {
            NT_LAYER_AUTO.set(false);
            let _ = w12_invalidate(h, None, false);
            return;
        }
    }
    let a = g.a.clamp(0.0, 1.0);
    layer_present(h, &wr, (255.0 * a + 0.5) as u8);
}

// Zona panas: tepi paling atas, tengah layar utama (w12notch.inc:918-923).
fn nt_hot_zone(pt: POINT) -> bool {
    // SAFETY: GetSystemMetrics hanya membaca metrik layar; layar utama selalu berawal di (0,0).
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    pt.y >= 0 && pt.y <= 1 && (pt.x - sw / 2).abs() <= s(110.0)
}

// Jadwal polling kursor (timer id 5), hanya bila periodenya berubah (w12notch.inc:925-931).
fn nt_poll_arm(ms: u32) {
    let h = NT_WND.get();
    if ms == NT_POLL_MS.get() || h.0.is_null() {
        return;
    }
    NT_POLL_MS.set(ms);
    // SAFETY: h bukan null; timer id 5 milik jendela notch.
    unsafe {
        if ms != 0 {
            SetTimer(Some(h), 5, ms, None);
        } else {
            let _ = KillTimer(Some(h), 5);
        }
    }
}

// Pemicu intip: kursor diam di zona panas sekitar 300 ms, tombol mouse tidak ditekan (w12notch.inc:933-948).
fn nt_poll() {
    let h = NT_WND.get();
    let mut pt = POINT::default();
    // SAFETY: GetCursorPos hanya menulis ke pt yang hidup selama pemanggilan.
    let got = unsafe { GetCursorPos(&mut pt) }.is_ok();
    if !NT_SHOWN.get() || !got {
        NT_HOT_N.set(0);
        return;
    }
    // Caps Lock: bit toggle GetKeyState. Pesan muncul hanya bila status berubah, bukan saat pertama dibaca.
    // SAFETY: GetKeyState hanya membaca status toggle tombol.
    let caps = (unsafe { GetKeyState(windows::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL.0 as i32) } & 1) as i32;
    let caps_prev = NT_CAPS.get();
    NT_CAPS.set(caps);
    if caps_prev >= 0 && caps_prev != caps {
        super::hotkeys::hk_toast(if caps == 1 { "Caps Lock aktif" } else { "Caps Lock nonaktif" }, 0xE765);
    }
    // SAFETY: GetSystemMetrics hanya membaca metrik layar.
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    // rapat hanya saat dekat tepi atas
    nt_poll_arm(if pt.y >= 0 && pt.y < s(80.0) && pt.x >= 0 && pt.x < sw { 100 } else { 400 });
    if NT_STATE.get() != NT_IDLE {
        NT_HOT_N.set(0);
        return;
    }
    // tombol mouse ditahan (menyeret jendela ke Snap Layouts di tepi atas): bukan pemicu
    // SAFETY: GetAsyncKeyState hanya membaca status tombol.
    let down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) | GetAsyncKeyState(VK_RBUTTON.0 as i32) };
    let drag = ((down as u16) & 0x8000) != 0;
    NT_HOT_N.set(if nt_hot_zone(pt) && !drag { NT_HOT_N.get() + 1 } else { 0 });
    if NT_HOT_N.get() < 3 {
        return; // kursor diam ±300 ms di zona: menyapu ke tab browser tidak memicunya
    }
    NT_HOT_N.set(0);
    NT_HOVER_PEEK.set(true);
    nt_go(NT_PEEK);
    // tampil beberapa detik; diperpanjang selama kursor di atasnya atau di zona
    // SAFETY: h adalah jendela notch yang sudah dibuat (NT_SHOWN bernilai true).
    unsafe { SetTimer(Some(h), 2, 3000, None) };
}

// Menyegarkan seluruh klien notch (setara InvalidateRect(g_nt, nullptr, FALSE)).
fn nt_inval() {
    let h = NT_WND.get();
    if h.0.is_null() {
        return;
    }
    // h bukan null; w12_invalidate memposting WM_W12_RENDER untuk jendela berlapis (notch_render).
    let _ = w12_invalidate(h, None, false);
}

// Klik pada elemen notch: buka, media, tab, timer, catatan, jaringan/baterai, dan lain-lain (w12notch.inc:951-993).
fn nt_act(id: i32) {
    let st = NT_STATE.get();
    if id == NI_NOTIF {
        let cur = NT_NOTIF_CUR.borrow().clone();
        NT_NOTIF_Q.borrow_mut().clear();
        if let Some(n) = cur {
            nt_open_notif_app(&n);
        }
        return;
    }
    if id == NI_NOTIF_CLEAR {
        NT_NOTIF_RECENT.borrow_mut().clear();
        nt_inval();
        return;
    }
    if id == NI_NOTIF_CENTER {
        nt_close(false);
        shell_open("ms-actioncenter:");
        return;
    }
    if (NI_NOTIF_ROW..NI_NOTIF_ROW + 12).contains(&id) {
        let n = NT_NOTIF_RECENT.borrow().get((id - NI_NOTIF_ROW) as usize).cloned();
        if let Some(n) = n {
            nt_open_notif_app(&n);
        }
        return;
    }
    if id == NI_BODY || (id == NI_MEDIA && st != NT_OPEN) {
        if st != NT_OPEN {
            nt_open(if st == NT_HUD && NT_HUD_KIND.get() == HUD_MSG { NTT_TIMER } else { NTT_HOME });
        }
        return;
    }
    if id == NI_MEDIA {
        nt_close(false);
        focus_media_source();
        return;
    }
    if (NI_PREV..=NI_NEXT).contains(&id) {
        media_cmd(id - NI_PREV, "notch");
        return;
    }
    if id == NI_SEEK {
        // fraksi dihitung dari posisi klik terhadap garis bar yang terakhir digambar (ruang gambar: NT_OX)
        let (bl, br, _) = NT_TL_BAR.get();
        if br > bl {
            media_seek((NT_CLICK_X.get() - NT_OX.get() - bl) / (br - bl));
        }
        nt_inval();
        return;
    }
    if id == NI_SYS_MEDIA {
        // chip "Media" di kartu Sistem: menyalakan atau mematikan pulau media, disimpan ke ini
        let on = !NOTCH_LIVE.get();
        NOTCH_LIVE.set(on);
        let _ = ini_write("shell", "notchlive", if on { "1" } else { "0" });
        nt_live_sync();
        nt_inval();
        return;
    }
    if id == NI_SYS_LYR {
        // chip "Lirik": disimpan ke [shell] notchlyrics; saat diaktifkan, lirik lagu aktif diminta
        let on = !NOTCH_LYRICS.get();
        NOTCH_LYRICS.set(on);
        let _ = ini_write("shell", "notchlyrics", if on { "1" } else { "0" });
        if on {
            nt_lyr_want();
        }
        nt_inval();
        return;
    }
    if id == NI_SYS_MEDTEMP {
        // chip "Terus"/"Sementara": disimpan ke [shell] notchmedtemp
        let on = !NOTCH_MEDIA_TEMP.get();
        NOTCH_MEDIA_TEMP.set(on);
        let _ = ini_write("shell", "notchmedtemp", if on { "1" } else { "0" });
        nt_live_sync();
        nt_inval();
        return;
    }
    if id == NI_SYS_ALWAYS {
        // chip "Auto-hide"/"Selalu tampil": disimpan ke [shell] notchalways
        let on = !NOTCH_ALWAYS.get();
        NOTCH_ALWAYS.set(on);
        let _ = ini_write("shell", "notchalways", if on { "1" } else { "0" });
        nt_idle_apply();
        nt_inval();
        return;
    }
    if id == NI_SET {
        NT_SET.set(!NT_SET.get());
        nt_inval();
        return;
    }
    if (NI_ST_SEC..NI_ST_SEC + 4).contains(&id) {
        NT_SET_SEC.set(id - NI_ST_SEC);
        nt_inval();
        return;
    }
    if id == NI_ST_HUD {
        let on = !NOTCH_HUD.get();
        NOTCH_HUD.set(on);
        let _ = ini_write("shell", "notchhud", if on { "1" } else { "0" });
        nt_live_sync();
        nt_inval();
        return;
    }
    if id == NI_ST_NOTIF {
        nt_ini_toggle(&NT_CFG_NOTIF, "notchnotif");
        nt_inval();
        return;
    }
    if id == NI_ST_ONLY {
        if nt_ini_flag(&NT_CFG_NOTIF, "notchnotif") {
            nt_ini_toggle(&NT_CFG_ONLY, "notchonly");
        }
        nt_inval();
        return;
    }
    if (NI_ST_FSV..NI_ST_FSG + 3).contains(&id) {
        let video = id < NI_ST_FSG;
        let v = (id - if video { NI_ST_FSV } else { NI_ST_FSG }) as u8;
        let (cell, key) = if video {
            (&super::state::NOTCH_FS_VIDEO, "notchfsvideo")
        } else {
            (&super::state::NOTCH_FS_GAME, "notchfsgame")
        };
        cell.set(v);
        let _ = ini_write("shell", key, &v.to_string());
        nt_inval();
        return;
    }
    if id == NI_SET_NOTIF {
        nt_close(false);
        shell_open("ms-settings:notifications");
        return;
    }
    if id == NI_NET || id == NI_BAT {
        nt_close(false);
        shell_open(if id == NI_NET { "ms-settings:network-status" } else { "ms-settings:batterysaver" });
        return;
    }
    if id == NI_MUTE {
        qs_toggle_mute(); // nilai baru juga datang lewat WM_VOLCHG
    } else if (NI_TAB..NI_TAB + 4).contains(&id) {
        NT_TAB.set(id - NI_TAB);
        NT_SET.set(false);
        if NT_TAB.get() == NTT_NOTES {
            nt_notes_load();
        }
    } else if id == NI_RING || id == NI_PMTOG || id == NI_PMGO {
        pm_toggle();
    } else if id == NI_PM25 {
        pm_start(25);
    } else if id == NI_PM5 {
        pm_start(5);
    } else if (NI_PRESET..NI_PRESET + 6).contains(&id) {
        pm_start(NT_PRESETS[(id - NI_PRESET) as usize]);
    } else if id == NI_PMSTOP {
        pm_stop();
    } else if id == NI_PMADJ_DN || id == NI_PMADJ_UP {
        // durasi kustom: langkah 5 menit, rentang 5..180 menit
        let d = if id == NI_PMADJ_UP { 5 } else { -5 };
        NT_PM_CUSTOM.set((NT_PM_CUSTOM.get() + d).clamp(5, 180));
    } else if id == NI_PMCUSTOM {
        pm_start(NT_PM_CUSTOM.get());
    } else if id == NI_NOTEFILE {
        let path = nt_note_path();
        if !path.exists() {
            // belum ada: buat kosong (UTF-8 BOM) agar bisa dibuka
            if let Ok(mut f) = std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                let _ = f.write_all(&[0xEF, 0xBB, 0xBF]);
            }
        }
        nt_close(false);
        shell_open(&path.to_string_lossy());
        return;
    } else if (NI_NOTEROW..NI_NOTEROW + NT_N_NOTES.get()).contains(&id) {
        let i = (id - NI_NOTEROW) as usize;
        let row = nt_wstr(&NT_NOTES.borrow()[i]);
        nt_copy(&row);
        NT_COPIED.set(i as i32);
        NT_COPIED_T.set(nt_tick64());
    } else if id == NI_SYS {
        nt_close(false);
        shell_open("taskmgr.exe");
        return;
    }
    nt_inval();
}

// Tombol saat notch terbuka: Esc, panah (volume), Enter (simpan), Backspace, Tab (tab) (w12notch.inc:995-1010).
fn nt_key(vk: VIRTUAL_KEY) {
    if NT_STATE.get() != NT_OPEN {
        return;
    }
    if vk == VK_ESCAPE {
        nt_close(true);
        return;
    }
    if vk == VK_UP || vk == VK_DOWN {
        if VOL.get() >= 0 {
            let step = if vk == VK_UP { 2 } else { -2 };
            nt_vol_set(VOL.get() + step);
        }
        return;
    }
    if vk == VK_RETURN {
        nt_note_save();
    } else if vk == VK_BACK && NT_NOTE_LEN.get() > 0 {
        let mut note = NT_NOTE.borrow_mut();
        let mut len = (NT_NOTE_LEN.get() - 1) as usize;
        // pasangan surrogate (UTF-16) dihapus sekaligus
        if len > 0 && (0xDC00..=0xDFFF).contains(&note[len]) && (0xD800..=0xDBFF).contains(&note[len - 1]) {
            len -= 1;
        }
        note[len] = 0;
        NT_NOTE_LEN.set(len as i32);
    } else if vk == VK_TAB {
        let raw = unsafe { GetKeyState(VK_SHIFT.0 as i32) };
        let shift = ((raw as u16) & 0x8000) != 0;
        let step = if shift { 3 } else { 1 };
        NT_TAB.set((NT_TAB.get() + step) % 4);
        if NT_TAB.get() == NTT_NOTES {
            nt_notes_load();
        }
    } else {
        return;
    }
    nt_inval();
}

// Karakter teks catatan; Ctrl+V menempel (w12notch.inc:1012-1019).
fn nt_char(c: u16) {
    if NT_STATE.get() != NT_OPEN {
        return;
    }
    if c == 0x16 {
        nt_paste(); // Ctrl+V
    } else if c >= 32 && c != 127 && NT_NOTE_LEN.get() < 300 {
        let len = NT_NOTE_LEN.get() as usize;
        {
            let mut note = NT_NOTE.borrow_mut();
            note[len] = c;
            note[len + 1] = 0;
        }
        NT_NOTE_LEN.set(len as i32 + 1);
    } else {
        return;
    }
    nt_inval();
}

// Pomodoro selesai: bunyi sistem lalu pesan HUD 5 detik (w12notch.inc:1021-1032).
fn nt_pm_done() {
    let brk = PM_TOTAL.get() <= 5 * 60;
    PM_STATE.set(0);
    // SAFETY: MessageBeep hanya memutar bunyi notifikasi sistem.
    let _ = unsafe { MessageBeep(MB_ICONASTERISK) };
    let text = if brk {
        "Istirahat selesai \u{00B7} siap fokus lagi"
    } else {
        "Sesi fokus selesai \u{00B7} waktunya istirahat"
    };
    nt_set_msg(text);
    NT_HUD_KIND.set(HUD_MSG);
    NT_MSG_WARN.set(false);
    NT_MSG_GLYPH.set(0);
    if NT_STATE.get() == NT_OPEN {
        nt_inval();
    } else {
        nt_show_temp(NT_HUD, 5000);
    }
}

// Penanda perubahan status untuk pemicu gambar ulang timer 3 (padanan s_sig di NtTimer).
static NT_SIG_S: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));

// Dispatch timer notch (w12notch.inc:1034-1080).
// 1 = animasi, 2 = intip/HUD berakhir, 3 = detik saat terbuka, 4 = pomodoro, 5 = polling kursor.
fn nt_timer(id: usize) {
    let h = NT_WND.get();
    match id {
        1 => nt_tick(),
        2 => {
            // SAFETY: timer 2 dipasang pada jendela notch ini (h).
            let _ = unsafe { KillTimer(Some(h), 2) };
            let st = NT_STATE.get();
            if st != NT_PEEK && st != NT_HUD {
                return;
            }
            let mut pt = POINT::default();
            // SAFETY: GetCursorPos menulis ke pt yang hidup selama pemanggilan.
            let got = unsafe { GetCursorPos(&mut pt) }.is_ok();
            let mut wr = RECT::default();
            // SAFETY: GetWindowRect menulis ke wr yang hidup selama pemanggilan.
            let got_wr = unsafe { GetWindowRect(h, &mut wr) }.is_ok();
            let in_win = got_wr && pt.x >= wr.left && pt.x < wr.right && pt.y >= wr.top && pt.y < wr.bottom;
            if got && (in_win || nt_hot_zone(pt)) {
                // SAFETY: h adalah jendela notch yang sudah dibuat.
                unsafe { SetTimer(Some(h), 2, 800, None) };
                return;
            }
            if st == NT_HUD && nt_notif_next() {
                return;
            }
            NT_HOVER_PEEK.set(false);
            NT_FULL.set(false);
            // lagu yang masih diputar kembali ke pulau ringkas; selain itu diam
            nt_go(if nt_live_on() && !NOTCH_MEDIA_TEMP.get() { NT_PEEK } else { NT_IDLE });
            nt_viz_sync();
        }
        5 => nt_poll(),
        6 => nt_viz_tick(),
        7 => {
            // Batas jeda media (NT_PAUSE_MS) tercapai: timer sekali jalan, lalu sinkronkan pulau.
            // SAFETY: KillTimer hanya memakai handle jendela notch dan id timer.
            let _ = unsafe { KillTimer(Some(h), 7) };
            nt_live_sync();
        }
        3 => {
            if NT_STATE.get() != NT_OPEN {
                // SAFETY: timer 3 dipasang pada jendela notch ini (h).
                let _ = unsafe { KillTimer(Some(h), 3) };
                return;
            }
            NT_SEC.set(NT_SEC.get().wrapping_add(1));
            // Jaring pengaman: aktivasi gagal (kunci fokus Windows) atau fokus pindah tanpa WM_ACTIVATE.
            // SAFETY: GetForegroundWindow tidak menerima pointer.
            let fg = unsafe { GetForegroundWindow() };
            if fg != h {
                nt_close(false);
                return;
            }
            if !SYS_TIMER.get() && (NT_SEC.get() & 1) == 0 {
                sys_sample();
            }
            // Gambar ulang hanya bila ada yang bergerak: Beranda, hitung mundur, atau penanda "Disalin"/"Tersimpan".
            let sig = (BAT.get() & 0xFF)
                | (if CHARGING.get() { 0x100 } else { 0 })
                | (if HAS_BAT.get() { 0x200 } else { 0 })
                | (NET.get() << 10);
            let now = nt_tick64();
            let mut need = sig != NT_SIG_S.get() || (PM_STATE.get() == 1 && NT_TAB.get() != NTT_NOTES);
            if NT_TAB.get() == NTT_HOME {
                need = need || (NT_SEC.get() & 1) == 0 || MEDIA.borrow().status == 4;
            } else if NT_TAB.get() == NTT_NOTES {
                need = need
                    || now.wrapping_sub(NT_COPIED_T.get()) < 2600
                    || now.wrapping_sub(NT_SAVED.get()) < 2600;
            }
            NT_SIG_S.set(sig);
            if need {
                nt_inval();
            }
        }
        4 => {
            // SAFETY: timer 4 dipasang pada jendela notch ini (h).
            let _ = unsafe { KillTimer(Some(h), 4) };
            if PM_STATE.get() == 1 && pm_left() <= 0 {
                nt_pm_done();
            } else {
                pm_arm();
            }
            if NT_STATE.get() == NT_IDLE {
                notch_invalidate(); // titik timer hilang: lebar pulau berubah
            }
        }
        _ => {}
    }
}

// Koordinat kursor dari lParam (GET_X_LPARAM / GET_Y_LPARAM).
fn nt_lp_x(lp: LPARAM) -> i32 {
    (lp.0 & 0xFFFF) as i16 as i32
}

fn nt_lp_y(lp: LPARAM) -> i32 {
    ((lp.0 >> 16) & 0xFFFF) as i16 as i32
}

// Prosedur jendela notch (w12notch.inc:1082-1149).
pub unsafe extern "system" fn notch_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_WINDOWPOSCHANGED {
        mica_moved(h, lp);
    }
    match m {
        WM_NCHITTEST => return LRESULT(HTCLIENT as isize),
        WM_MOUSEACTIVATE => {
            // diam/intip tidak mencuri fokus
            let ma = if NT_STATE.get() == NT_OPEN { MA_ACTIVATE } else { MA_NOACTIVATE };
            return LRESULT(ma as isize);
        }
        WM_NCACTIVATE => return DefWindowProcW(h, m, WPARAM(1), lp),
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(h, &mut ps);
            let _ = EndPaint(h, &ps);
            return LRESULT(0);
        }
        WM_W12_RENDER => {
            notch_render();
            return LRESULT(0);
        }
        WM_TIMER => {
            nt_timer(wp.0);
            return LRESULT(0);
        }
        WM_ACTIVATE => {
            // klik di luar = tutup
            if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE && NT_STATE.get() == NT_OPEN {
                nt_close(false);
            }
            return LRESULT(0);
        }
        WM_KEYDOWN => {
            nt_key(VIRTUAL_KEY(wp.0 as u16));
            return LRESULT(0);
        }
        WM_CHAR => {
            nt_char(wp.0 as u16);
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            // volume 2% per takik di atas pulau yang tampil; intip/HUD berubah menjadi HUD volume
            if NT_STATE.get() == NT_IDLE || EP_VOL.borrow().is_none() || VOL.get() < 0 {
                return LRESULT(0);
            }
            let delta = ((wp.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            NT_WHEEL.set(NT_WHEEL.get() + delta);
            let step = (WHEEL_DELTA / 2) as i32;
            let d = NT_WHEEL.get() / step;
            NT_WHEEL.set(NT_WHEEL.get() - d * step);
            if d == 0 {
                return LRESULT(0);
            }
            nt_vol_set(VOL.get() + d);
            if NT_STATE.get() != NT_OPEN {
                NT_HUD_KIND.set(HUD_VOL);
                nt_show_temp(NT_HUD, 1500);
            }
            return LRESULT(0);
        }
        WM_CAPTURECHANGED => {
            if NT_DOWN.get() == NI_VOLTRK {
                NT_DOWN.set(-1);
                nt_inval();
            }
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            // SAFETY: GetCapture hanya membaca jendela penangkap mouse saat ini.
            if NT_DOWN.get() == NI_VOLTRK && unsafe { GetCapture() } == h {
                nt_vol_drag(nt_lp_x(lp)); // menyeret volume
                return LRESULT(0);
            }
            // TME_HOVER dipasang ulang tiap gerak saat diam: WM_MOUSEHOVER datang setelah kursor diam 150 ms
            let can_hover = nt_can_hover();
            let hover = if can_hover { TME_HOVER.0 } else { 0 };
            let mut t = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TRACKMOUSEEVENT_FLAGS(TME_LEAVE.0 | hover),
                hwndTrack: h,
                dwHoverTime: 150,
            };
            if !NT_TRACK.get() || can_hover {
                // SAFETY: t hidup selama pemanggilan.
                let _ = unsafe { TrackMouseEvent(&mut t) };
                NT_TRACK.set(true);
            }
            if NT_STATE.get() == NT_PEEK || NT_STATE.get() == NT_HUD {
                // kursor di atas pulau: jangan disembunyikan
                let _ = unsafe { KillTimer(Some(h), 2) };
            }
            let hit = nt_hit_at(nt_lp_x(lp), nt_lp_y(lp));
            if hit != NT_HOV.get() {
                NT_HOV.set(hit);
                nt_inval();
            }
            return LRESULT(0);
        }
        WM_MOUSEHOVER => {
            if nt_can_hover() && NT_T.get() >= 1.0 {
                NT_HOVER_PEEK.set(true);
                nt_go(NT_PEEK);
                let _ = unsafe { KillTimer(Some(h), 2) };
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            NT_TRACK.set(false);
            if NT_HOV.get() != -1 {
                NT_HOV.set(-1);
                nt_inval();
            }
            if NT_STATE.get() == NT_PEEK || NT_STATE.get() == NT_HUD {
                // kursor pergi: hilang ±1 detik kemudian
                let _ = unsafe { SetTimer(Some(h), 2, 1200, None) };
            }
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            NT_DOWN.set(nt_hit_at(nt_lp_x(lp), nt_lp_y(lp)));
            if NT_DOWN.get() < 0 && NT_STATE.get() == NT_OPEN {
                nt_close(true); // bayangan di luar bentuk
                return LRESULT(0);
            }
            if NT_DOWN.get() == NI_VOLTRK {
                // klik di trek = lompat ke situ
                // SAFETY: SetCapture hanya memasang penangkap mouse ke jendela notch ini.
                let _ = unsafe { SetCapture(h) };
                nt_vol_drag(nt_lp_x(lp));
                return LRESULT(0);
            }
            nt_inval();
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let hit = nt_hit_at(nt_lp_x(lp), nt_lp_y(lp));
            let down = NT_DOWN.get();
            NT_DOWN.set(-1);
            if down == NI_VOLTRK {
                // SAFETY: GetCapture dan ReleaseCapture hanya mengatur penangkap mouse.
                if unsafe { GetCapture() } == h {
                    let _ = unsafe { ReleaseCapture() };
                }
                nt_inval();
                return LRESULT(0);
            }
            // posisi klik (koordinat jendela) dipakai NI_SEEK di nt_act untuk menghitung fraksi seek
            NT_CLICK_X.set(nt_lp_x(lp) as f32);
            if hit >= 0 && hit == down {
                nt_act(hit);
            }
            nt_inval();
            return LRESULT(0);
        }
        WM_SETCURSOR => {
            if (lp.0 & 0xFFFF) as u32 == HTCLIENT
                && NT_HOV.get() >= 0
                && (NT_HOV.get() != NI_BODY || NT_STATE.get() != NT_OPEN)
            {
                let cur = if NT_HOV.get() == NI_NOTE { IDC_IBEAM } else { IDC_HAND };
                // SAFETY: LoadCursorW dengan kursor sistem (hinstance None) tidak membaca memori pemanggil.
                if let Ok(c) = unsafe { LoadCursorW(None, cur) } {
                    // SAFETY: SetCursor menerima handle kursor yang baru dimuat.
                    let _ = unsafe { SetCursor(Some(c)) };
                }
                return LRESULT(1); // TRUE
            }
        }
        _ => {}
    }
    DefWindowProcW(h, m, wp, lp)
}

// GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS (kNtBri di w12notch.inc:32).
const NT_BRI_GUID: windows::core::GUID = windows::core::GUID::from_u128(0x8ffee2c6_2d01_46be_adb9_398addc5b4ff);

// Membandingkan dua judul UTF-16 sampai NUL pertama (padanan wcscmp).
fn nt_title_eq(a: &[u16], b: &[u16]) -> bool {
    let la = a.iter().position(|&c| c == 0).unwrap_or(a.len());
    let lb = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    a[..la] == b[..lb]
}

// Membuat jendela notch sekali, bila fitur aktif (w12notch.inc:1177-1196).
pub fn notch_create() {
    if !NOTCH_ON.get() || !NT_WND.get().0.is_null() {
        return;
    }
    // SAFETY: WNDCLASSEXW adalah struct POD; nol berarti None/null untuk setiap pointer dan handle.
    let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(notch_proc);
    wc.hInstance = HI.get();
    wc.lpszClassName = w!("Win12ShellNotch");
    // SAFETY: IDC_ARROW adalah kursor sistem.
    if let Ok(cur) = unsafe { LoadCursorW(None, IDC_ARROW) } {
        wc.hCursor = cur;
    }
    // Gagal bila kelas sudah terdaftar dari pemanggilan sebelumnya: diabaikan (sama seperti C).
    let _ = unsafe { RegisterClassExW(&wc) };
    let h = match unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            w!("Win12ShellNotch"),
            w!(""),
            WS_POPUP,
            0,
            0,
            10,
            10,
            None,
            None,
            Some(HI.get()),
            None,
        )
    } {
        Ok(h) => h,
        Err(_) => return,
    };
    NT_WND.set(h);
    // berlapis milik kita (prop "w12l"): w12_invalidate mengirim WM_W12_RENDER, bukan InvalidateRect mentah.
    // SAFETY: h adalah jendela yang baru dibuat; HANDLE(1) hanya penanda properti.
    let _ = unsafe { SetPropW(h, w!("w12l"), Some(HANDLE(1usize as *mut std::ffi::c_void))) };
    NT_START.set(nt_tick64());
    *NT_LAST_TITLE.borrow_mut() = MEDIA.borrow().title;
    // SAFETY: MSG adalah jendela pesan utama; id NT_HOTKEY hanya dipakai notch.
    let _ = unsafe {
        RegisterHotKey(
            Some(MSG.get()),
            NT_HOTKEY,
            HOT_KEY_MODIFIERS(MOD_WIN.0 | MOD_ALT.0 | MOD_NOREPEAT.0),
            'N' as u32,
        )
    };
    // SAFETY: NT_BRI_GUID hidup selama pemanggilan; jendela pesan utama sebagai penerima.
    let reg = unsafe {
        RegisterPowerSettingNotification(HANDLE(MSG.get().0), &NT_BRI_GUID, DEVICE_NOTIFY_WINDOW_HANDLE)
    };
    NT_BRI_REG.set(reg.unwrap_or(HPOWERNOTIFY(0)));
    notch_sync();
}

// Melepas jendela notch dan pendaftaran sistemnya (w12notch.inc:1198-1207).
pub fn notch_destroy() {
    let h = NT_WND.get();
    if h.0.is_null() {
        return;
    }
    // SAFETY: id hotkey dan jendela pesan sama dengan saat didaftarkan.
    let _ = unsafe { UnregisterHotKey(Some(MSG.get()), NT_HOTKEY) };
    let reg = NT_BRI_REG.get();
    if reg.0 != 0 {
        // SAFETY: handle berasal dari RegisterPowerSettingNotification.
        let _ = unsafe { UnregisterPowerSettingNotification(reg) };
        NT_BRI_REG.set(HPOWERNOTIFY(0));
    }
    NT_SHOWN.set(false);
    // SAFETY: h adalah jendela notch yang dibuat notch_create.
    let _ = unsafe { DestroyWindow(h) };
    NT_WND.set(HWND(std::ptr::null_mut()));
    *NT_ROUND.borrow_mut() = None;
}

/// Mode notch yang berlaku: 0 = normal, 1 = layar penuh (hanya notifikasi & status), 2 = layar penuh (juga media).
static NT_FS_MODE: SyncCell<Cell<u8>> = SyncCell::new(Cell::new(0));

/// Setelan pengguna untuk jenis layar penuh yang sedang di depan (0 sembunyi, 1 notifikasi, 2 notifikasi + media).
fn nt_fs_policy() -> u8 {
    match super::state::FS_KIND.get() {
        1 => super::state::NOTCH_FS_VIDEO.get(),
        _ => super::state::NOTCH_FS_GAME.get(),
    }
}

/// true = layar penuh dengan mode "hanya notifikasi": tanpa intip media, pulau media, atau zona kursor.
fn nt_fs_quiet() -> bool {
    NT_FS_MODE.get() == 1
}

// Tampil kecuali sedang keluar atau layar penuh dengan setelan "sembunyi" (w12notch.inc:1152-1175).
// Dipanggil dari ApplyPills saat keadaan fokus/layar penuh berubah.
pub fn notch_sync() {
    let h = NT_WND.get();
    if h.0.is_null() {
        return;
    }
    let fs = FULLSCREEN.get();
    let pol = if fs { nt_fs_policy() } else { 0 };
    let want = !QUITTING.get() && (!fs || pol > 0);
    let mode = if fs && want { pol } else { 0 };
    if want == NT_SHOWN.get() && mode == NT_FS_MODE.get() {
        return;
    }
    NT_FS_MODE.set(mode);
    if want && NT_SHOWN.get() {
        // tetap boleh tampil, hanya mode yang berganti (masuk/keluar layar penuh): HUD/notifikasi yang sedang
        // tampil dibiarkan selesai sendiri; keadaan lain kembali diam.
        if NT_STATE.get() != NT_HUD {
            if NT_STATE.get() == NT_OPEN {
                nt_close(false);
            }
            nt_go(NT_IDLE);
        }
        nt_idle_apply();
        nt_poll_arm(if mode == 1 { 0 } else { 400 });
        if mode != 1 {
            nt_live_sync();
        }
        return;
    }
    if !want {
        if NT_STATE.get() == NT_OPEN {
            // SAFETY: timer dan gaya jendela milik jendela notch ini.
            let _ = unsafe { KillTimer(Some(h), 3) };
            let ex = unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) };
            unsafe { SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize) };
        }
        // SAFETY: penghapusan timer dan penangkap mouse pada jendela notch ini.
        let _ = unsafe { KillTimer(Some(h), 1) };
        let _ = unsafe { KillTimer(Some(h), 2) };
        let _ = unsafe { KillTimer(Some(h), 6) };
        if unsafe { GetCapture() } == h {
            let _ = unsafe { ReleaseCapture() };
        }
    }
    // "boleh tampil"; dalam keadaan diam jendela tetap tersembunyi sampai zona panas dipicu
    NT_SHOWN.set(want);
    NT_STATE.set(NT_IDLE);
    super::media_worker::set_tl_hot(false);
    NT_PREV.set(NT_IDLE);
    NT_T.set(1.0);
    NT_HOV.set(-1);
    NT_DOWN.set(-1);
    NT_HOT_N.set(0);
    NT_TO.set(nt_target(NT_IDLE));
    // SAFETY: h adalah jendela notch yang valid.
    if want && NOTCH_ALWAYS.get() && mode == 0 {
        // notchalways=1: garis diam tetap terlihat di tepi atas tengah (tidak di atas video/game)
        unsafe {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        notch_render();
    } else {
        let _ = unsafe { ShowWindow(h, SW_HIDE) };
    }
    // sembunyi atau "hanya notifikasi": tidak ada polling zona kursor sama sekali
    nt_poll_arm(if want && mode != 1 { 400 } else { 0 });
    if want {
        nt_live_sync(); // kembali dari layar penuh: pulau ringkas media langsung tampil lagi bila diputar
        nt_notif_next(); // notifikasi yang antre sebelum disembunyikan
    }
}

// Pintasan notch: buka atau tutup pulau (w12notch.inc:1209-1214).
pub fn notch_hotkey() {
    if NT_WND.get().0.is_null() || !NT_SHOWN.get() {
        return;
    }
    if NT_STATE.get() == NT_OPEN {
        nt_close(true);
    } else {
        nt_open(NTT_HOME);
    }
}

// Media berubah (MediaChanged): lagu baru yang sedang diputar memicu intip 3 detik (w12notch.inc:1216-1228).
pub fn notch_media() {
    if NT_WND.get().0.is_null() {
        return;
    }
    let (new_track, title, status) = {
        let m = MEDIA.borrow();
        let same = nt_title_eq(&m.title, &NT_LAST_TITLE.borrow()[..]);
        (m.title[0] != 0 && !same, m.title, m.status)
    };
    *NT_LAST_TITLE.borrow_mut() = title;
    // ganti putar/jeda (4 <-> 5) juga memicu intip, seperti lagu baru
    if new_track && NOTCH_LYRICS.get() {
        nt_lyr_want(); // lagu baru: cari lirik di thread latar (lyrics.rs); hasilnya datang lewat WM_W12_LYRIC
    }
    let prev = NT_PLAY_PREV.get();
    NT_PLAY_PREV.set(status);
    let toggled = (prev == 4 && status == 5) || (prev == 5 && status == 4);
    if status == 5 && prev != 5 {
        NT_PAUSE_T0.set(nt_tick64());
    }
    if NOTCH_HUD.get()
        && !nt_fs_quiet()
        && ((new_track && status == 4) || toggled)
        && nt_tick64().wrapping_sub(NT_START.get()) > 8000
        && NT_STATE.get() != NT_OPEN
        && !(NT_STATE.get() == NT_PEEK && NT_HOVER_PEEK.get())
    {
        nt_show_temp(NT_PEEK, 3000);
    }
    nt_live_sync();
    notch_invalidate();
}

// Sinkron pulau ringkas menetap dengan status media. Diputar: tampil ringkas. Berhenti: kembali diam,
// kecuali bila intip, hover, atau keadaan lain sedang berjalan dan akan mengaturnya sendiri.
fn nt_live_sync() {
    if NT_WND.get().0.is_null() || !NT_SHOWN.get() {
        return;
    }
    let st = NT_STATE.get();
    // notchmedtemp=1: pulau tidak dipaksa menetap; hanya intip 3 detik (notch_media) yang memunculkannya
    // layar penuh (mode 2): media hanya mengintip, tidak menetap di atas video/game
    if nt_live_on() && !NOTCH_MEDIA_TEMP.get() && NT_FS_MODE.get() == 0 {
        if st == NT_IDLE {
            nt_go(NT_PEEK);
        }
    } else if st == NT_PEEK && !NT_HOVER_PEEK.get() && !NT_FULL.get() {
        nt_go(NT_IDLE);
    }
    nt_viz_sync();
}

// Timer 6 (120 ms) hanya aktif selama media diputar dan pulau ringkas tampil; menggerakkan bar visualizer.
// Timer 7 (sekali jalan) menutup pulau media jeda setelah NT_PAUSE_MS.
fn nt_viz_sync() {
    let h = NT_WND.get();
    if h.0.is_null() {
        return;
    }
    let playing = MEDIA.borrow().status == 4;
    // timer 6 juga dipakai untuk menggambar ulang baris lirik yang berganti (pulau penuh)
    if (nt_compact() || nt_lyric_line().is_some()) && NT_STATE.get() == NT_PEEK && playing {
        // SAFETY: h adalah jendela notch yang sudah dibuat; timer 6 hanya dipakai di sini.
        unsafe { SetTimer(Some(h), 6, 120, None) };
    } else {
        // SAFETY: KillTimer hanya memakai handle jendela notch dan id timer.
        let _ = unsafe { KillTimer(Some(h), 6) };
    }
    if !playing && nt_live_on() {
        let sisa = NT_PAUSE_MS.saturating_sub(nt_tick64().wrapping_sub(NT_PAUSE_T0.get()));
        // SAFETY: h adalah jendela notch yang sudah dibuat; timer 7 hanya dipakai di sini.
        unsafe { SetTimer(Some(h), 7, sisa.max(1) as u32, None) };
    } else {
        // SAFETY: KillTimer hanya memakai handle jendela notch dan id timer.
        let _ = unsafe { KillTimer(Some(h), 7) };
    }
}

fn nt_viz_tick() {
    let h = NT_WND.get();
    if !NT_SHOWN.get() || !nt_live_on() || NT_STATE.get() != NT_PEEK || MEDIA.borrow().status != 4 {
        // SAFETY: KillTimer hanya memakai handle jendela notch dan id timer.
        let _ = unsafe { KillTimer(Some(h), 6) };
        return;
    }
    if nt_compact() || nt_lyric_line().is_some() {
        nt_inval();
    }
}

// Pengisi daya dipasang/dicabut (dipanggil battery_update, core_c1.rs): pesan singkat di pulau.
pub fn notch_plug(charging: bool, pct: i32) {
    if !NOTCH_HUD.get() || NT_WND.get().0.is_null() || !NT_SHOWN.get() || pct < 0 {
        return;
    }
    let msg = if charging {
        format!("Mengisi daya \u{00B7} {}%", pct)
    } else {
        format!("Pengisi daya dilepas \u{00B7} {}%", pct)
    };
    nt_set_msg(&msg);
    NT_MSG_WARN.set(false);
    NT_MSG_GLYPH.set(if charging { NT_GLYPH_CHG } else { 0 });
    NT_HUD_KIND.set(HUD_MSG);
    if NT_STATE.get() == NT_OPEN {
        nt_inval();
    } else {
        nt_show_temp(NT_HUD, 3000);
    }
}

// Volume berubah (WM_VOLCHG). Tidak tampil saat Pengaturan Cepat terbuka (w12notch.inc:1230-1238).
pub fn notch_volume(prev_vol: i32, prev_mute: bool) {
    let st = NT_STATE.get();
    if NT_SHOWN.get() && (st == NT_OPEN || (st == NT_HUD && NT_HUD_KIND.get() == HUD_VOL)) {
        nt_inval();
    }
    if !NOTCH_HUD.get()
        || NT_WND.get().0.is_null()
        || prev_vol < 0
        || VOL.get() < 0
        || (prev_vol == VOL.get() && prev_mute == MUTE.get())
        || PANEL_OPEN.get() == PN_QS
    {
        return;
    }
    NT_HUD_KIND.set(HUD_VOL);
    nt_show_temp(NT_HUD, 1500);
}

// Kecerahan berubah (WM_POWERBROADCAST, PBT_POWERSETTINGCHANGE). Notifikasi pertama = nilai awal,
// tidak ditampilkan (w12notch.inc:1240-1252). lp adalah POWERBROADCAST_SETTING.
pub fn notch_power(lp: LPARAM) {
    if NT_WND.get().0.is_null() || lp.0 == 0 {
        return;
    }
    // SAFETY: lp menunjuk ke POWERBROADCAST_SETTING yang valid selama pesan WM_POWERBROADCAST;
    // Data dibaca sebagai u32 hanya bila DataLength >= 4.
    let (guid, len, raw) = unsafe {
        let s = lp.0 as *const POWERBROADCAST_SETTING;
        let len = (*s).DataLength;
        let guid = (*s).PowerSetting;
        let raw = if len >= 4 {
            std::ptr::read_unaligned(std::ptr::addr_of!((*s).Data) as *const u32)
        } else {
            0
        };
        (guid, len, raw)
    };
    if guid != NT_BRI_GUID || len < 4 {
        return;
    }
    let v = raw as i32;
    if !NT_BRI_INIT.get() {
        NT_BRI_INIT.set(true);
        NT_BRI.set(v);
        return;
    }
    if v == NT_BRI.get() {
        return;
    }
    NT_BRI.set(v);
    // 2026-11-15: HUD kecerahan juga di mode default (permintaan pengguna: semua peringatan lewat notch).
    if !NOTCH_HUD.get() || PANEL_OPEN.get() == PN_QS {
        return;
    }
    NT_HUD_KIND.set(HUD_BRI);
    nt_show_temp(NT_HUD, 1500);
}

// Peringatan baterai lemah: dipanggil BatteryUpdate. false = belum bisa tampil; pemanggil menunda
// peringatan (w12notch.inc:1254-1267).
pub fn notch_battery_alert(pct: i32) -> bool {
    if !NOTCH_HUD.get() || NT_WND.get().0.is_null() || !NT_SHOWN.get() {
        return false;
    }
    nt_set_msg(&format!("Baterai {}% \u{2014} sambungkan pengisi daya", pct));
    NT_MSG_WARN.set(pct <= 5);
    NT_MSG_GLYPH.set(0);
    NT_HUD_KIND.set(HUD_MSG);
    if NT_STATE.get() == NT_OPEN {
        nt_inval();
    } else {
        nt_show_temp(NT_HUD, 6000);
    }
    true
}

// Menyalin teks ke NT_MSG (String), dipotong 95 karakter (Copy(g_ntMsg, ..., 96) di C).
fn nt_set_msg(text: &str) {
    *NT_MSG.borrow_mut() = text.chars().take(95).collect();
    NT_ALERT_RED.set(false);
}

// ---------------------------------------------------------------- peringatan dan notifikasi lewat notch (2026-11-15)

/// Glyph pesan HUD_MSG berwarna merah (peringatan jaringan, dsb.). Direset tiap nt_set_msg.
static NT_ALERT_RED: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

/// Satu notifikasi Windows yang dialihkan ke notch (notif_worker.rs).
#[derive(Clone, Default)]
pub struct NtNotif {
    pub app: String,
    pub title: String,
    pub body: String,
    pub aumid: String,
    pub t: u64,
}

static NT_NOTIF_CUR: SyncCell<RefCell<Option<NtNotif>>> = SyncCell::new(RefCell::new(None));
static NT_NOTIF_Q: SyncCell<RefCell<std::collections::VecDeque<NtNotif>>> =
    SyncCell::new(RefCell::new(std::collections::VecDeque::new()));
/// Riwayat notifikasi terbaru (paling baru di depan), ditampilkan di kartu Beranda saat notch terbuka.
static NT_NOTIF_RECENT: SyncCell<RefCell<std::collections::VecDeque<NtNotif>>> =
    SyncCell::new(RefCell::new(std::collections::VecDeque::new()));
const NT_NOTIF_MS: u32 = 5500;

/// Pesan satu baris di pulau: `glyph` 0 = ikon info, `red` = glyph merah (peringatan).
/// Tidak menimpa notifikasi yang sedang tampil; tidak tampil saat layar penuh (NT_SHOWN false).
pub fn notch_alert(text: &str, glyph: u16, red: bool, ms: u32) {
    if !NOTCH_HUD.get() || NT_WND.get().0.is_null() || !NT_SHOWN.get() {
        return;
    }
    if NT_STATE.get() == NT_HUD && NT_HUD_KIND.get() == HUD_NOTIF {
        return;
    }
    nt_set_msg(text);
    NT_ALERT_RED.set(red);
    NT_MSG_WARN.set(false);
    NT_MSG_GLYPH.set(glyph);
    NT_HUD_KIND.set(HUD_MSG);
    if NT_STATE.get() == NT_OPEN {
        nt_inval();
    } else {
        nt_show_temp(NT_HUD, ms);
    }
}

fn nt_notif_show(n: NtNotif) {
    *NT_NOTIF_CUR.borrow_mut() = Some(n);
    NT_HUD_KIND.set(HUD_NOTIF);
    nt_show_temp(NT_HUD, NT_NOTIF_MS);
}

/// Notifikasi Windows baru. Masuk riwayat; tampil sebagai kartu di pulau (antre bila kartu lain masih tampil).
pub fn notch_notify(n: NtNotif) {
    {
        let mut r = NT_NOTIF_RECENT.borrow_mut();
        r.push_front(n.clone());
        r.truncate(12);
    }
    if NT_WND.get().0.is_null() || !NT_SHOWN.get() {
        return;
    }
    let st = NT_STATE.get();
    if st == NT_OPEN {
        nt_inval();
        return;
    }
    if st == NT_HUD && NT_HUD_KIND.get() == HUD_NOTIF {
        let mut q = NT_NOTIF_Q.borrow_mut();
        if q.len() < 6 {
            q.push_back(n);
        }
        return;
    }
    nt_notif_show(n);
}

/// Dipanggil saat HUD berakhir: tampilkan notifikasi berikutnya bila ada. true = ada yang ditampilkan.
fn nt_notif_next() -> bool {
    let next = NT_NOTIF_Q.borrow_mut().pop_front();
    match next {
        Some(n) => {
            nt_notif_show(n);
            true
        }
        None => false,
    }
}

/// Kartu notifikasi: lingkaran ikon, nama aplikasi, judul, dan isi (satu baris masing-masing).
fn nt_draw_notif(w: f32, h: f32) {
    let p = *PALETTE.borrow();
    let cur = NT_NOTIF_CUR.borrow().clone();
    let Some(n) = cur else {
        return;
    };
    nt_hit(0.0, 0.0, w, h, NI_NOTIF);
    let cx = sf(30.0);
    let cy = h * 0.5;
    fill_rr(cx - sf(16.0), cy - sf(16.0), cx + sf(16.0), cy + sf(16.0), sf(16.0), p.accent, 1.0);
    glyph(0xEA8F, TF_GLYPH.borrow().as_ref(), cx, cy, sf(10.0), rgba(255, 255, 255, 1.0), 1.0);
    let tx = sf(56.0);
    let r = w - sf(18.0);
    let app = if n.app.is_empty() { "Notifikasi".to_string() } else { n.app.clone() };
    let ago = {
        let s = nt_tick64().saturating_sub(n.t) / 1000;
        if s < 60 { "sekarang".to_string() } else { format!("{} mnt", s / 60) }
    };
    let aw = measure_text(&ago, TF_XS.borrow().as_ref());
    text(&app, TF_XS.borrow().as_ref(), tx, sf(8.0), r - aw - sf(8.0), sf(24.0), p.sub, 1.0);
    nt_text_r(&ago, TF_XS.borrow().as_ref(), r, sf(8.0), sf(24.0), p.sub);
    let title = if n.title.is_empty() { app.clone() } else { n.title.clone() };
    text(&title, TF_SS.borrow().as_ref(), tx, sf(23.0), r, sf(43.0), p.text, 1.0);
    if !n.body.is_empty() {
        text(&n.body, TF_XS.borrow().as_ref(), tx, sf(42.0), r, sf(60.0), p.sub, 1.0);
    }
}

/// Daftar notifikasi terbaru di kartu Beranda notch terbuka (maks `rows` baris). Mengembalikan true bila ada isi.
fn nt_recent_rows(l: f32, t: f32, r: f32, rows: usize) -> bool {
    let p = *PALETTE.borrow();
    let list: Vec<NtNotif> = NT_NOTIF_RECENT.borrow().iter().take(rows).cloned().collect();
    if list.is_empty() {
        return false;
    }
    let row_h = sf(22.0);
    for (i, n) in list.iter().enumerate() {
        let y = t + i as f32 * row_h;
        let app = if n.app.is_empty() { "Notifikasi" } else { n.app.as_str() };
        let aw = measure_text(app, TF_XS.borrow().as_ref()).min((r - l) * 0.35);
        text(app, TF_XS.borrow().as_ref(), l, y, l + aw + sf(2.0), y + row_h, p.sub, 1.0);
        let line = if n.body.is_empty() { n.title.clone() } else { format!("{} \u{2014} {}", n.title, n.body) };
        text(&line, TF_XS.borrow().as_ref(), l + aw + sf(10.0), y, r, y + row_h, p.text, 1.0);
        nt_hit(l, y, r, y + row_h, NI_NOTIF_ROW + i as i32);
    }
    true
}

/// Tab Notifikasi di notch terbuka.
const NTT_NOTIF: i32 = 3;
const NI_NOTIF_CLEAR: i32 = 92;
const NI_NOTIF_CENTER: i32 = 93;

fn nt_notif_tab(l: f32, t: f32, r: f32, b: f32) {
    let p = *PALETTE.borrow();
    nt_card(l, t, r, b);
    let pad = sf(12.0);
    text("Notifikasi terbaru", TF_SS.borrow().as_ref(), l + pad, t + pad, r - pad, t + pad + sf(18.0), p.sub, 1.0);
    nt_button(r - pad - sf(150.0), t + sf(8.0), r - pad - sf(78.0), t + sf(32.0), "Bersihkan", NI_NOTIF_CLEAR, false);
    nt_button(r - pad - sf(72.0), t + sf(8.0), r - pad, t + sf(32.0), "Pusat", NI_NOTIF_CENTER, false);
    let top = t + sf(42.0);
    let rows = (((b - pad) - top) / sf(22.0)).floor().max(0.0) as usize;
    if !nt_recent_rows(l + pad, top, r - pad, rows) {
        let cy = (top + b) * 0.5;
        glyph(0xEA8F, TF_GLYPH_LG.borrow().as_ref(), (l + r) * 0.5, cy - sf(14.0), sf(14.0), p.sub, 1.0);
        wf_text_c("Belum ada notifikasi", TF_S.borrow().as_ref(), (l + r) * 0.5, cy + sf(6.0), cy + sf(24.0), &p.sub);
    }
}

fn nt_open_notif_app(n: &NtNotif) {
    nt_close(false);
    if n.aumid.is_empty() {
        shell_open("ms-actioncenter:");
    } else {
        shell_open(&format!("shell:AppsFolder\\{}", n.aumid));
    }
}

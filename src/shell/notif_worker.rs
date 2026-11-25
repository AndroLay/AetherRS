//! Notifikasi Windows -> notch (2026-11-15).
//!
//! Thread MTA `w12-notif` membaca toast lewat `UserNotificationListener` (akses dicek: Allowed untuk proses tanpa
//! paket). Event NotificationChanged dicoba dulu; bila tidak tersedia, polling tiap 2 dtk. Notifikasi baru dikirim ke
//! thread UI lewat WM_W12_NOTIF lalu `notch_notify` menampilkannya sebagai kartu di pulau.
//!
//! "Notifikasi lewat notch saja" (`[shell] notchonly`, default 1): banner pop-up Windows dimatikan per aplikasi
//! (`ShowBanner=0` di HKCU\...\Notifications\Settings\<AUMID>). Notifikasi tetap masuk Pusat Notifikasi. Nilai asli
//! dicadangkan di `HKCU\Software\Win12Shell\BannerBackup` (2 = sebelumnya tidak ada) dan dipulihkan saat
//! `notchonly=0`. Alarm dan panggilan tidak disentuh agar tidak terlewat.

use std::collections::{HashSet, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;

use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::Management::{UserNotificationListener, UserNotificationListenerAccessStatus};
use windows::UI::Notifications::{KnownNotificationBindings, NotificationBinding, NotificationKinds, UserNotification};
use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
};
use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::Storage::FileSystem::{
    FindFirstChangeNotificationW, FindNextChangeNotification, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use super::notch::{notch_notify, NtNotif};
use super::state::WM_W12_NOTIF;
use crate::log::log;
use crate::reg;
use crate::wide::{pcw, to_wide_z};

const NOTIF_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings";
const BACKUP_KEY: &str = r"Software\Win12Shell\BannerBackup";

static RUNNING: AtomicBool = AtomicBool::new(false);
static WAKE: AtomicIsize = AtomicIsize::new(0);
static HWND_MSG: AtomicIsize = AtomicIsize::new(0);
static ONLY: AtomicBool = AtomicBool::new(true);
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Terapkan ulang setelan dari settings.ini (dipanggil Pengaturan notch).
pub fn apply_settings() {
    start(HWND(HWND_MSG.load(Ordering::SeqCst) as *mut c_void));
}
static QUEUE: Mutex<VecDeque<NtNotif>> = Mutex::new(VecDeque::new());

fn wake() {
    let h = WAKE.load(Ordering::SeqCst);
    if h != 0 {
        // SAFETY: event dari CreateEventW, hidup selama proses.
        let _ = unsafe { SetEvent(HANDLE(h as *mut c_void)) };
    }
}

/// Mulai pekerja notifikasi. Membaca `[shell] notchnotif` (default 1) dan `notchonly` (default 1).
/// Aman dipanggil ulang (Pengaturan notch, 2026-11-19): setelan dibaca lagi dan thread hanya dibuat sekali.
/// Banner Windows dipulihkan bila notifikasi notch mati atau `notchonly=0`, agar notifikasi tidak hilang ke mana pun.
pub fn start(msg: HWND) {
    if msg.0.is_null() {
        return;
    }
    HWND_MSG.store(msg.0 as isize, Ordering::SeqCst);
    let on = super::core_c1::ini_int("shell", "notchnotif", 1) != 0;
    let only = super::core_c1::ini_int("shell", "notchonly", 1) != 0;
    ENABLED.store(on, Ordering::SeqCst);
    ONLY.store(only && on, Ordering::SeqCst);
    if !on || !only {
        restore_banners();
    }
    if !on {
        log("notif: dinonaktifkan (notchnotif=0)");
        return;
    }
    if RUNNING.swap(true, Ordering::SeqCst) {
        if only {
            suppress_existing(); // dinyalakan lagi dari Pengaturan saat pekerja sudah berjalan
        }
        return;
    }
    if WAKE.load(Ordering::SeqCst) == 0 {
        // SAFETY: event auto-reset tanpa nama, disimpan selama proses hidup.
        if let Ok(h) = unsafe { CreateEventW(None, false, false, PCWSTR::null()) } {
            WAKE.store(h.0 as isize, Ordering::SeqCst);
        }
    }
    let _ = std::thread::Builder::new()
        .name("w12-notif".into())
        .stack_size(512 * 1024)
        .spawn(worker_main);
}

/// Thread UI (WM_W12_NOTIF): teruskan notifikasi yang antre ke notch.
pub fn deliver() {
    let items: Vec<NtNotif> = match QUEUE.lock() {
        Ok(mut q) => q.drain(..).collect(),
        Err(_) => return,
    };
    if !ENABLED.load(Ordering::SeqCst) {
        return; // dimatikan dari Pengaturan; pekerja tetap hidup tetapi kartu tidak ditampilkan
    }
    for n in items {
        notch_notify(n);
    }
}

fn post() {
    let h = HWND_MSG.load(Ordering::SeqCst);
    if h != 0 {
        // SAFETY: jendela pesan milik thread UI; gagal tanpa efek bila sudah hilang.
        let _ = unsafe { PostMessageW(Some(HWND(h as *mut c_void)), WM_W12_NOTIF, WPARAM(0), LPARAM(0)) };
    }
}

fn tick() -> u64 {
    // SAFETY: hanya membaca penghitung waktu.
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
}

fn worker_main() {
    // SAFETY: COM per thread; dipasangkan CoUninitialize.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    run();
    // SAFETY: pasangan CoInitializeEx.
    unsafe { CoUninitialize() };
    RUNNING.store(false, Ordering::SeqCst);
}

fn run() {
    let listener = match UserNotificationListener::Current() {
        Ok(l) => l,
        Err(e) => {
            log(&format!("notif: UserNotificationListener gagal hr=0x{:08X}", e.code().0 as u32));
            return;
        }
    };
    let mut access = listener.GetAccessStatus().unwrap_or(UserNotificationListenerAccessStatus::Unspecified);
    if access != UserNotificationListenerAccessStatus::Allowed {
        access = listener
            .RequestAccessAsync()
            .and_then(|op| op.get())
            .unwrap_or(UserNotificationListenerAccessStatus::Denied);
    }
    if access != UserNotificationListenerAccessStatus::Allowed {
        log(&format!("notif: akses ditolak ({})", access.0));
        return;
    }
    let tok = listener
        .NotificationChanged(&TypedEventHandler::new(|_, _| {
            wake();
            Ok(())
        }))
        .ok();
    let chg = if tok.is_some() { None } else { notif_db_watch() };
    let poll_ms: u32 = if tok.is_some() { 30_000 } else { 2_000 };
    log(&format!(
        "notif: pendengar siap (event={}, pantau-db={}, notchonly={})",
        tok.is_some(),
        chg.is_some(),
        ONLY.load(Ordering::SeqCst)
    ));
    if ONLY.load(Ordering::SeqCst) {
        suppress_existing();
    }
    let mut seen: HashSet<u32> = HashSet::new();
    let mut first = true;
    let mut logged = 0u32;
    let mut last_read: u64;
    let mut idle_wakes = 0u32;
    loop {
        let list = listener.GetNotificationsAsync(NotificationKinds::Toast).and_then(|op| op.get());
        last_read = tick();
        if let Ok(v) = list {
            let n = v.Size().unwrap_or(0);
            let mut now_ids: HashSet<u32> = HashSet::with_capacity(n as usize);
            let mut fresh: Vec<NtNotif> = Vec::new();
            for i in 0..n {
                let Ok(un) = v.GetAt(i) else { continue };
                let Ok(id) = un.Id() else { continue };
                now_ids.insert(id);
                if seen.contains(&id) {
                    continue;
                }
                if !first {
                    if let Some(item) = read(&un) {
                        fresh.push(item);
                    }
                }
            }
            seen = now_ids;
            first = false;
            idle_wakes = if fresh.is_empty() { idle_wakes.saturating_add(1) } else { 0 };
            if !fresh.is_empty() {
                for it in &fresh {
                    if ONLY.load(Ordering::SeqCst) && !it.aumid.is_empty() {
                        suppress_banner(&it.aumid);
                    }
                    if logged < 200 {
                        logged += 1;
                        log(&format!(
                            "notif: baru app={} judul_len={} isi_len={}",
                            it.aumid,
                            it.title.chars().count(),
                            it.body.chars().count()
                        ));
                    }
                }
                if let Ok(mut q) = QUEUE.lock() {
                    q.extend(fresh);
                    while q.len() > 20 {
                        q.pop_front();
                    }
                }
                post();
            }
        }
        let h = WAKE.load(Ordering::SeqCst);
        if let (Some(c), true) = (chg, h != 0) {
            // Tanpa event: tunggu perubahan database notifikasi (toast baru langsung menulis -wal), jaring pengaman 15 dtk.
            let hs = [HANDLE(h as *mut c_void), c];
            // SAFETY: kedua handle valid selama proses hidup (loop pekerja tidak pernah selesai).
            let r = unsafe { WaitForMultipleObjects(&hs, false, 15_000) };
            if r.0 == WAIT_OBJECT_0.0 + 1 {
                // SAFETY: c dari FindFirstChangeNotificationW.
                let _ = unsafe { FindNextChangeNotification(c) };
                // Satu toast = beberapa tulisan beruntun. Aplikasi lain bisa menulis database puluhan kali per detik
                // (toast progres; terukur 72/dtk pada 2026-11-19), jadi jeda minimum antar-pembacaan dibuat adaptif:
                // 1 dtk, naik ke 3 dtk bila 5 kali berturut-turut tidak ada notifikasi baru.
                let gap: u64 = if idle_wakes >= 5 { 3000 } else { 1000 };
                let since = tick().saturating_sub(last_read);
                let pause = if since < gap { gap - since } else { 250 };
                std::thread::sleep(std::time::Duration::from_millis(pause.max(250)));
                // SAFETY: sama seperti di atas; menyerap sinyal tulisan beruntun selama jeda.
                if unsafe { WaitForSingleObject(c, 0) } == WAIT_OBJECT_0 {
                    let _ = unsafe { FindNextChangeNotification(c) };
                }
            }
        } else if h != 0 {
            // SAFETY: event valid selama proses hidup.
            let _ = unsafe { WaitForSingleObject(HANDLE(h as *mut c_void), poll_ms) };
        } else {
            std::thread::sleep(std::time::Duration::from_millis(poll_ms as u64));
        }
    }
}

/// Handle perubahan folder database notifikasi Windows (%LOCALAPPDATA%\Microsoft\Windows\Notifications).
/// Dipakai saat event NotificationChanged tidak tersedia (proses tanpa paket), menggantikan polling 2 dtk (2026-11-19).
fn notif_db_watch() -> Option<HANDLE> {
    let base = std::env::var("LOCALAPPDATA").ok()?;
    let dir = to_wide_z(&format!(r"{}\Microsoft\Windows\Notifications", base));
    // SAFETY: dir string UTF-16 berakhiran nol yang hidup selama pemanggilan.
    let h = unsafe {
        FindFirstChangeNotificationW(pcw(&dir), false, FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_SIZE)
    }
    .ok()?;
    if h.is_invalid() {
        return None;
    }
    Some(h)
}

/// Tambahkan teks dari satu binding toast (tanpa duplikat, tanpa baris baru).
fn push_texts(b: &NotificationBinding, texts: &mut Vec<String>) {
    let Ok(els) = b.GetTextElements() else { return };
    for i in 0..els.Size().unwrap_or(0) {
        if let Ok(t) = els.GetAt(i).and_then(|e| e.Text()) {
            let s = t.to_string().replace(['\r', '\n'], " ").trim().to_string();
            if !s.is_empty() && !texts.contains(&s) {
                texts.push(s);
            }
        }
    }
}

fn read(un: &UserNotification) -> Option<NtNotif> {
    let (app, aumid) = match un.AppInfo() {
        Ok(ai) => (
            ai.DisplayInfo().and_then(|d| d.DisplayName()).map(|h| h.to_string()).unwrap_or_default(),
            ai.AppUserModelId().map(|h| h.to_string()).unwrap_or_default(),
        ),
        Err(_) => (String::new(), String::new()),
    };
    let mut texts: Vec<String> = Vec::new();
    if let Ok(vis) = un.Notification().and_then(|n| n.Visual()) {
        // Semua binding, bukan hanya ToastGeneric: sebagian aplikasi (mis. WhatsApp) menaruh isi pesan di binding lain.
        if let Ok(binds) = vis.Bindings() {
            for i in 0..binds.Size().unwrap_or(0) {
                if let Ok(b) = binds.GetAt(i) {
                    push_texts(&b, &mut texts);
                }
            }
        }
        if texts.is_empty() {
            if let Ok(b) = vis.GetBinding(&KnownNotificationBindings::ToastGeneric().unwrap_or_else(|_| HSTRING::from("ToastGeneric"))) {
                push_texts(&b, &mut texts);
            }
        }
    }
    if texts.is_empty() && app.is_empty() {
        return None;
    }
    let title = texts.first().cloned().unwrap_or_default();
    let body = if texts.len() > 1 { texts[1..].join(" \u{00B7} ") } else { String::new() };
    Some(NtNotif {
        app,
        title: title.chars().take(120).collect(),
        body: body.chars().take(200).collect(),
        aumid,
        t: tick(),
    })
}

/// Alarm dan panggilan tetap memakai banner Windows (tidak boleh terlewat saat layar penuh).
fn keep_banner(aumid: &str) -> bool {
    let l = aumid.to_ascii_lowercase();
    l.contains("alarms") || l.contains("calling") || l.contains("windows.systemtoast.call")
}

fn suppress_banner(aumid: &str) {
    if aumid.is_empty() || keep_banner(aumid) || aumid.contains('\\') {
        return;
    }
    let key = format!(r"{}\{}", NOTIF_KEY, aumid);
    let cur = reg::get_dword(&key, "ShowBanner");
    if cur == Some(0) {
        return;
    }
    if reg::get_dword(BACKUP_KEY, aumid).is_none() {
        let _ = reg::set_dword(BACKUP_KEY, aumid, cur.unwrap_or(2));
    }
    if reg::set_dword(&key, "ShowBanner", 0) {
        log(&format!("notif: banner Windows dialihkan ke notch untuk {}", aumid));
    }
}

/// Nama subkunci di bawah HKCU\`path`.
fn subkeys(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let w = to_wide_z(path);
    let mut k = HKEY::default();
    // SAFETY: k ditutup di akhir; buffer nama lokal.
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, pcw(&w), Some(0), KEY_READ, &mut k) != ERROR_SUCCESS {
            return out;
        }
        let mut i = 0u32;
        loop {
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            let rc = RegEnumKeyExW(k, i, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None);
            if rc != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
            i += 1;
        }
        let _ = RegCloseKey(k);
    }
    out
}

/// Nama nilai di bawah HKCU\`path`.
fn values(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let w = to_wide_z(path);
    let mut k = HKEY::default();
    // SAFETY: k ditutup di akhir; buffer nama lokal.
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, pcw(&w), Some(0), KEY_READ, &mut k) != ERROR_SUCCESS {
            return out;
        }
        let mut i = 0u32;
        loop {
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            let rc = RegEnumValueW(k, i, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None);
            if rc != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
            i += 1;
        }
        let _ = RegCloseKey(k);
    }
    out
}

/// Matikan banner untuk aplikasi yang sudah terdaftar di Pengaturan Notifikasi.
fn suppress_existing() {
    let keys = subkeys(NOTIF_KEY);
    let mut n = 0;
    for k in keys {
        let key = format!(r"{}\{}", NOTIF_KEY, k);
        if reg::get_dword(&key, "ShowBanner") != Some(0) && !keep_banner(&k) {
            suppress_banner(&k);
            n += 1;
        }
    }
    log(&format!("notif: {} aplikasi dialihkan ke notch", n));
}

/// Kembalikan nilai ShowBanner asli dari cadangan (dipanggil saat `notchonly=0`).
pub fn restore_banners() {
    let names = values(BACKUP_KEY);
    if names.is_empty() {
        return;
    }
    let mut n = 0;
    for a in names {
        let key = format!(r"{}\{}", NOTIF_KEY, a);
        match reg::get_dword(BACKUP_KEY, &a) {
            Some(2) => {
                let _ = reg::delete_value(&key, "ShowBanner");
            }
            Some(v) => {
                let _ = reg::set_dword(&key, "ShowBanner", v);
            }
            None => {}
        }
        let _ = reg::delete_value(BACKUP_KEY, &a);
        n += 1;
    }
    log(&format!("notif: banner Windows dipulihkan untuk {} aplikasi", n));
}

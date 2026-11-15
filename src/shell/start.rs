//! Sumber: w12start.inc:1-757 (global, indeks, semua aplikasi, pencarian, menu konteks, ikon hasil).
//! Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H06.
use std::ffi::c_void;
use std::collections::HashSet;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use windows::core::PCSTR;
use windows::Win32::Storage::FileSystem::{GetFileAttributesW, FILE_ATTRIBUTE_DIRECTORY, INVALID_FILE_ATTRIBUTES};
use windows::Win32::System::Com::{
    CoFreeUnusedLibraries, CoInitializeEx, CoTaskMemFree, CoUninitialize, IBindCtx,
    COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, CMF_EXTENDEDVERBS, CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO,
    CMINVOKECOMMANDINFOEX, FOLDERID_AppsFolder, IContextMenu, IContextMenu2, IContextMenu3,
    IEnumShellItems, IShellFolder, IShellItem, KF_FLAG_DEFAULT, SHBindToParent,
    SHGetKnownFolderItem, SHParseDisplayName, ShellExecuteW, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING,
};

use super::prelude::*;

// Ukuran panel dan batas daftar (w12start.inc:17-18).
pub const START_W: i32 = 780;
pub const START_H: i32 = 618;
pub const START_PINS: i32 = 18;
pub const START_MAXQ: i32 = 63;
pub const START_HITS: i32 = 24;
pub const ST_APP_INDEX_MAX: i32 = 4000;
pub const ST_ALLAPP_MAX: i32 = ST_APP_INDEX_MAX + MAXAPPS;

// Jenis baris hasil (w12start.inc:19). Disimpan di `StHit.kind`.
pub const ST_HIT_APP_INDEX: i32 = 0;
pub const ST_HIT_APP_LIST: i32 = 1;
pub const ST_HIT_SETTING: i32 = 2;
pub const ST_HIT_FILE: i32 = 3;
pub const ST_HIT_CALC: i32 = 4;
pub const ST_HIT_CMD: i32 = 5;

/// Baris hasil pencarian (w12start.inc:23).
pub type StHit = W12SearchHit;

/// Entri indeks shell:AppsFolder (`StIdx`, :21). `id` = shell:AppsFolder.
/// Ikon dibuat di thread UI saat indeks dipasang (`st_idx_install`), bukan di thread latar.
pub struct StIdx {
    pub name: String,
    pub id: String,
    pub fields: W12SearchFields,
    pub icon: Icon,
    pub icon_tried: bool,
}

/// Satu entri hasil thread latar (bagian dari `StIdxBuild`, :22). Hanya berisi data yang aman lintas thread.
pub struct StIdxEntry {
    pub name: String,
    pub id: String,
    pub fields: W12SearchFields,
}

/// Paket hasil thread latar (`StIdxBuild`, :22). Dikirim ke UI lewat LPARAM sebagai `Box`.
pub struct StIdxBuild {
    pub v: Vec<StIdxEntry>,
}

/// Katalog pengaturan (`StSetting`, :24). Nama dan keyword ditulis tetap; bidang pencarian dibangun saat dibutuhkan.
pub struct StSetting {
    pub name: &'static str,
    pub keywords: &'static str,
    pub uri: &'static str,
}

/// Referensi semua aplikasi (`StAllAppRef`, :25). `kind` 0 = indeks shell:AppsFolder, 1 = apps.txt khusus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StAllAppRef {
    pub kind: i32,
    pub reference: i32,
}

/// Bidang pencarian aplikasi yang sudah dibangun (`StAppSearchFields`, :124).
/// `snapshot` menyimpan nama saat bidang dibangun; bila berubah, bidang dibangun ulang.
#[derive(Default)]
pub struct StAppSearchFields {
    pub used: bool,
    pub snapshot: String,
    pub fields: Option<W12SearchFields>,
}

// Global Start dan pencarian (w12start.inc:26-64, :48-64, :72-116).
// Nilai Copy memakai Cell; daftar dan string memakai RefCell (lihat CONVENTIONS.md).
pub static ST_ALL_APPS: SyncCell<RefCell<Vec<StAllAppRef>>> = SyncCell::new(RefCell::new(Vec::new()));
pub static ST_ALL_IDX_GEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static ST_ALL_N_APPS: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static ST_ALL_SCROLL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static ST_ALL_SELECTED: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static ST_ALL_ROW_STEP: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static ST_ALL_VIEW_HEIGHT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static ST_QUERY_ALL: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static ST_IDX: SyncCell<RefCell<Vec<StIdx>>> = SyncCell::new(RefCell::new(Vec::new()));
pub static ST_IDX_BUSY: AtomicBool = AtomicBool::new(false);
pub static ST_IDX_TICK: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
pub static ST_IDX_GEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static ST_HITS: SyncCell<RefCell<Vec<StHit>>> = SyncCell::new(RefCell::new(Vec::new()));
pub static ST_N_HITS: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static ST_SEL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static ST_HITS_GEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static ST_HITS_FS_GEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static ST_CALC: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static ST_CMD_TEXT: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static ST_FS_SENT_Q: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static ST_FS_SENT_CAP: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static ST_FS_SENT_GEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static ST_NO_ICON: SyncCell<RefCell<Icon>> = SyncCell::new(RefCell::new(Icon::new()));
pub static ST_QUERY: SyncCell<RefCell<[u16; 64]>> = SyncCell::new(RefCell::new([0u16; 64]));
pub static ST_USER: SyncCell<RefCell<[u16; 128]>> = SyncCell::new(RefCell::new([0u16; 128]));
pub static ST_USER_OK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static ST_PIN: SyncCell<RefCell<Vec<i32>>> = SyncCell::new(RefCell::new(Vec::new()));
pub static ST_REC: SyncCell<RefCell<Vec<i32>>> = SyncCell::new(RefCell::new(Vec::new()));
pub static ST_LISTS_READY: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
/// Menu Daya: 0 tertutup, 1 daftar, 2 konfirmasi Mulai ulang, 3 konfirmasi Matikan (:60).
pub static ST_POWER: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
/// NectarLink: 0 belum terpasang, 1 terpasang tetapi tidak berjalan, 2 berjalan (:61).
pub static ST_NL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static ST_NL_EXE: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static ST_NL_ICON: SyncCell<RefCell<Icon>> = SyncCell::new(RefCell::new(Icon::new()));
pub static ST_SETTINGS_READY: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static ST_SETTING_FIELDS: SyncCell<RefCell<Vec<W12SearchFields>>> = SyncCell::new(RefCell::new(Vec::new()));
pub static ST_APP_SEARCH: SyncCell<RefCell<Vec<StAppSearchFields>>> = SyncCell::new(RefCell::new(Vec::new()));
/// Penangan menu konteks yang sedang aktif (dipakai saat WM_MENUCHAR/WM_INITMENUPOPUP lewat HandleMenuMsg).
pub static ST_CONTEXT_MENU2: SyncCell<RefCell<Option<IContextMenu2>>> = SyncCell::new(RefCell::new(None));
pub static ST_CONTEXT_MENU3: SyncCell<RefCell<Option<IContextMenu3>>> = SyncCell::new(RefCell::new(None));

// Katalog lokal ms-settings (:66-112). Pencarian memakai nama dan sinonim, lalu membuka URI resmi Settings.
const ST_SETTINGS: &[StSetting] = &[
    StSetting { name: "Semua pengaturan", keywords: "pengaturan settings preferences", uri: "ms-settings:" },
    StSetting { name: "Sistem", keywords: "sistem system", uri: "ms-settings:system" },
    StSetting { name: "Tampilan", keywords: "tampilan layar display monitor resolution", uri: "ms-settings:display" },
    StSetting { name: "Suara", keywords: "suara audio sound volume speaker microphone", uri: "ms-settings:sound" },
    StSetting { name: "Notifikasi", keywords: "notifikasi notifications", uri: "ms-settings:notifications" },
    StSetting { name: "Daya dan baterai", keywords: "daya baterai power battery sleep", uri: "ms-settings:powersleep" },
    StSetting { name: "Penyimpanan", keywords: "penyimpanan storage disk drive", uri: "ms-settings:storagesense" },
    StSetting { name: "Tentang PC", keywords: "tentang about device pc computer", uri: "ms-settings:about" },
    StSetting { name: "Bluetooth", keywords: "bluetooth perangkat devices", uri: "ms-settings:bluetooth" },
    StSetting { name: "Wi-Fi", keywords: "wifi wi-fi jaringan network internet wireless", uri: "ms-settings:network-wifi" },
    StSetting { name: "Jaringan dan internet", keywords: "jaringan internet network internet status", uri: "ms-settings:network-status" },
    StSetting { name: "Ethernet", keywords: "ethernet kabel wired network", uri: "ms-settings:network-ethernet" },
    StSetting { name: "VPN", keywords: "vpn jaringan private network", uri: "ms-settings:network-vpn" },
    StSetting { name: "Mode pesawat", keywords: "mode pesawat airplane flight network", uri: "ms-settings:network-airplanemode" },
    StSetting { name: "Hotspot seluler", keywords: "hotspot seluler mobile tethering", uri: "ms-settings:network-mobilehotspot" },
    StSetting { name: "Personalisasi", keywords: "personalisasi personalization customize", uri: "ms-settings:personalization" },
    StSetting { name: "Latar belakang", keywords: "latar belakang wallpaper background image", uri: "ms-settings:personalization-background" },
    StSetting { name: "Warna", keywords: "warna colors accent theme", uri: "ms-settings:personalization-colors" },
    StSetting { name: "Layar kunci", keywords: "layar kunci lock screen", uri: "ms-settings:lockscreen" },
    StSetting { name: "Tema", keywords: "tema themes", uri: "ms-settings:themes" },
    StSetting { name: "Start Windows", keywords: "start mulai menu personalization-start", uri: "ms-settings:personalization-start" },
    StSetting { name: "Taskbar Windows", keywords: "taskbar bilah tugas personalization-taskbar", uri: "ms-settings:taskbar" },
    StSetting { name: "Aplikasi terinstal", keywords: "aplikasi apps installed programs uninstall", uri: "ms-settings:appsfeatures" },
    StSetting { name: "Aplikasi default", keywords: "aplikasi bawaan default apps browser", uri: "ms-settings:defaultapps" },
    StSetting { name: "Aplikasi startup", keywords: "aplikasi startup autostart mulai otomatis", uri: "ms-settings:startupapps" },
    StSetting { name: "Akun Anda", keywords: "akun account user profile", uri: "ms-settings:yourinfo" },
    StSetting { name: "Opsi masuk", keywords: "opsi masuk sign in login pin password", uri: "ms-settings:signinoptions" },
    StSetting { name: "Akun email dan kerja", keywords: "akun email kerja school workplace", uri: "ms-settings:emailandaccounts" },
    StSetting { name: "Pembaruan Windows", keywords: "pembaruan windows update upgrade", uri: "ms-settings:windowsupdate" },
    StSetting { name: "Riwayat pembaruan", keywords: "riwayat update history windowsupdate-history", uri: "ms-settings:windowsupdate-history" },
    StSetting { name: "Keamanan Windows", keywords: "keamanan security defender virus firewall", uri: "ms-settings:windowsdefender" },
    StSetting { name: "Privasi", keywords: "privasi privacy permissions izin", uri: "ms-settings:privacy" },
    StSetting { name: "Privasi kamera", keywords: "kamera camera privacy webcam", uri: "ms-settings:privacy-webcam" },
    StSetting { name: "Privasi mikrofon", keywords: "mikrofon microphone privacy recording", uri: "ms-settings:privacy-microphone" },
    StSetting { name: "Privasi lokasi", keywords: "lokasi location privacy gps", uri: "ms-settings:privacy-location" },
    StSetting { name: "Aksesibilitas", keywords: "aksesibilitas accessibility magnifier narrator", uri: "ms-settings:easeofaccess" },
    StSetting { name: "Tanggal dan waktu", keywords: "tanggal waktu date time clock timezone", uri: "ms-settings:dateandtime" },
    StSetting { name: "Bahasa", keywords: "bahasa language region keyboard input", uri: "ms-settings:regionlanguage" },
    StSetting { name: "Pencetak dan pemindai", keywords: "pencetak printer scanner perangkat", uri: "ms-settings:printers" },
    StSetting { name: "Perangkat seluler", keywords: "ponsel perangkat seluler phone mobile", uri: "ms-settings:mobile-devices" },
    StSetting { name: "Game Bar", keywords: "game bar gaming xbox game recorder", uri: "ms-settings:gaming-gamebar" },
    StSetting { name: "Pencarian Windows", keywords: "pencarian search windows", uri: "ms-settings:search" },
    StSetting { name: "Clipboard", keywords: "clipboard papan klip riwayat salin", uri: "ms-settings:clipboard" },
    StSetting { name: "Pemecahan masalah", keywords: "pemecahan masalah troubleshoot diagnosis", uri: "ms-settings:troubleshoot" },
];

/// Membangun bidang pencarian katalog pengaturan sekali (`StSettingsEnsure`, :116).
fn st_settings_ensure() {
    if ST_SETTINGS_READY.get() {
        return;
    }
    let fields: Vec<W12SearchFields> = ST_SETTINGS
        .iter()
        .map(|st| {
            w12_search_build_fields(st.keywords).unwrap_or(W12SearchFields {
                lower: Vec::new(),
                initials: Vec::new(),
            })
        })
        .collect();
    *ST_SETTING_FIELDS.borrow_mut() = fields;
    ST_SETTINGS_READY.set(true);
}

/// Membangun ulang bidang pencarian aplikasi bila nama berubah (`StAppSearchEnsure`, :132).
/// Nama diambil sebagai salinan agar borrow `APPS` tidak dipegang saat membangun bidang.
pub fn st_app_search_ensure() {
    let count = apps_count().clamp(0, MAXAPPS) as usize;
    let names: Vec<String> = APPS.borrow().iter().take(count).map(|a| a.name.clone()).collect();
    let slots = MAXAPPS as usize;
    let mut sf = ST_APP_SEARCH.borrow_mut();
    if sf.len() < slots {
        sf.resize_with(slots, StAppSearchFields::default);
    }
    for (i, slot) in sf.iter_mut().enumerate().take(slots) {
        match names.get(i) {
            Some(name) => {
                if slot.used && slot.snapshot == *name && slot.fields.is_some() {
                    continue;
                }
                slot.used = true;
                slot.snapshot = name.clone();
                slot.fields = w12_search_build_fields(name);
            }
            None => {
                *slot = StAppSearchFields::default();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Indeks shell:AppsFolder (w12start.inc:154-249)
// ---------------------------------------------------------------------------

/// Status "indeks pernah dipasang" (`g_stIdx != NULL`, :221-240). Privat.
static ST_IDX_LOADED: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

/// Membaca string dari PWSTR milik COM lalu membebaskannya (:176-181).
fn st_take_pwstr(p: PWSTR) -> String {
    if p.0.is_null() {
        return String::new();
    }
    // SAFETY: p berasal dari IShellItem::GetDisplayName (memori CoTaskMem) dan tidak dipakai setelah dibebaskan di sini.
    unsafe {
        let text = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const c_void));
        text
    }
}

/// Mengenumerasi shell:AppsFolder (`StIdxThread` :154-205). Entri dengan nama/ID kosong, atau yang gagal
/// dibangun bidang pencariannya, dilewati tanpa membatalkan enumerasi. Semua objek COM dilepas di sini.
fn st_idx_enumerate() -> Vec<StIdxEntry> {
    let mut out: Vec<StIdxEntry> = Vec::new();
    // SAFETY: dipanggil di thread latar yang sudah CoInitializeEx; pointer COM dikelola wrapper windows-rs (Drop melepas referensi).
    unsafe {
        let Ok(apps) = SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) else {
            return out;
        };
        let Ok(en) = apps.BindToHandler::<_, IEnumShellItems>(None::<&IBindCtx>, &BHID_EnumItems) else {
            return out;
        };
        while out.len() < ST_APP_INDEX_MAX as usize {
            let mut slot: [Option<IShellItem>; 1] = [None];
            let mut fetched: u32 = 0;
            if en.Next(&mut slot, Some(&mut fetched as *mut u32)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = slot[0].take() else {
                break;
            };
            let name = match item.GetDisplayName(SIGDN_NORMALDISPLAY) {
                Ok(p) => st_take_pwstr(p),
                Err(_) => String::new(),
            };
            let id = match item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING) {
                Ok(p) => st_take_pwstr(p),
                Err(_) => String::new(),
            };
            if name.is_empty() || id.is_empty() {
                continue;
            }
            let Some(fields) = w12_search_build_fields(&name) else {
                continue;
            };
            out.push(StIdxEntry { name, id, fields });
        }
    }
    out
}

/// Thread latar indeks (`StIdxThread` :154-205): enumerasi, lalu kirim `StIdxBuild` ke jendela utama lewat WM_W12_APPIDX.
fn st_idx_thread(host: isize) {
    // SAFETY: CoInitializeEx dipasangkan dengan CoUninitialize hanya bila pemanggilannya berhasil (S_OK atau S_FALSE).
    let co = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let uninit = co.is_ok();
    let v = st_idx_enumerate();
    if uninit {
        // SAFETY: pasangan dari CoInitializeEx yang berhasil di atas.
        unsafe { CoUninitialize() };
    }
    let build = Box::into_raw(Box::new(StIdxBuild { v }));
    // SAFETY: host adalah HWND jendela utama; LPARAM membawa pointer StIdxBuild yang kepemilikannya pindah ke penerima.
    let posted = unsafe {
        PostMessageW(
            Some(HWND(host as *mut c_void)),
            WM_W12_APPIDX,
            WPARAM(0),
            LPARAM(build as isize),
        )
    };
    if posted.is_err() {
        // SAFETY: pointer dari Box::into_raw di atas belum diterima siapa pun; dikembalikan ke Box agar dibebaskan.
        drop(unsafe { Box::from_raw(build) });
        ST_IDX_BUSY.store(false, AtomicOrdering::SeqCst);
    }
}

/// Memulai pembaruan indeks di thread latar bila belum pernah dipasang atau sudah lebih dari 10 menit (`StIdxRefresh` :242-249).
pub fn st_idx_refresh() {
    // SAFETY: GetTickCount tidak punya prasyarat.
    let now = unsafe { GetTickCount() };
    if ST_IDX_LOADED.get() && now.wrapping_sub(ST_IDX_TICK.get()) < 600_000 {
        return;
    }
    if ST_IDX_BUSY
        .compare_exchange(false, true, AtomicOrdering::SeqCst, AtomicOrdering::SeqCst)
        .is_err()
    {
        return;
    }
    let host = MSG.get().0 as isize;
    let spawned = std::thread::Builder::new()
        .name("w12-appidx".into())
        .spawn(move || st_idx_thread(host));
    if spawned.is_err() {
        ST_IDX_BUSY.store(false, AtomicOrdering::SeqCst);
    }
}

/// Memasang hasil indeks dari thread latar (`StIdxInstall` :221-240). Dipanggil dari WM_W12_APPIDX (core_c4b).
pub fn st_idx_install(lp: LPARAM) {
    ST_IDX_BUSY.store(false, AtomicOrdering::SeqCst);
    if lp.0 == 0 {
        return;
    }
    // SAFETY: lp berisi pointer dari Box::into_raw di st_idx_thread; pesan ini hanya diterima sekali.
    let StIdxBuild { v } = *unsafe { Box::from_raw(lp.0 as *mut StIdxBuild) };
    if v.is_empty() && ST_IDX_LOADED.get() {
        return;
    }
    let fresh: Vec<StIdx> = v
        .into_iter()
        .map(|e| StIdx {
            name: e.name,
            id: e.id,
            fields: e.fields,
            icon: Icon::new(),
            icon_tried: false,
        })
        .collect();
    let mut old = std::mem::replace(&mut *ST_IDX.borrow_mut(), fresh);
    for e in old.iter_mut() {
        icon_free(&mut e.icon);
    }
    ST_IDX_LOADED.set(true);
    ST_IDX_GEN.set(ST_IDX_GEN.get() + 1);
    // SAFETY: GetTickCount tidak punya prasyarat.
    ST_IDX_TICK.set(unsafe { GetTickCount() });
    ST_N_HITS.set(0);
    st_search();
    if PANEL_OPEN.get() == PN_START && (ST_Q_LEN.get() != 0 || ST_ALL_APPS_MODE.get()) {
        let pan = PAN.get();
        if !pan.0.is_null() {
            // pan adalah HWND panel yang non-null; w12_invalidate memposting WM_W12_RENDER untuk panel berlapis.
            let _ = w12_invalidate(pan, None, false);
        }
    }
}

// ---------------------------------------------------------------------------
// Daftar semua aplikasi (w12start.inc:251-352)
// ---------------------------------------------------------------------------

/// Membatalkan cache daftar semua aplikasi (`StAllAppsInvalidate` :251-255).
pub fn st_all_apps_invalidate() {
    ST_ALL_IDX_GEN.set(-1);
    ST_ALL_N_APPS.set(-1);
}

/// Nama baris daftar semua aplikasi (`StAllAppName` :257-261). Di luar rentang → kosong.
pub fn st_all_app_name(r: StAllAppRef) -> String {
    if r.kind == 0 {
        ST_IDX
            .borrow()
            .get(r.reference as usize)
            .map(|e| e.name.clone())
            .unwrap_or_default()
    } else {
        APPS.borrow()
            .get(r.reference as usize)
            .map(|a| a.name.clone())
            .unwrap_or_default()
    }
}

/// Huruf pertama nama dalam huruf kecil, untuk urutan dan lompat huruf. Nama kosong → '\0'.
fn st_first_lower(name: &str) -> char {
    name.chars()
        .next()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .unwrap_or('\0')
}

/// Menyusun daftar semua aplikasi: indeks shell:AppsFolder, lalu apps.txt tanpa duplikat (tanpa membedakan
/// huruf besar-kecil), diurutkan menurut nama (`StAllAppsEnsure` :273-291). Dilewati bila cache masih berlaku.
pub fn st_all_apps_ensure() {
    let napps = apps_count();
    if ST_ALL_IDX_GEN.get() == ST_IDX_GEN.get() && ST_ALL_N_APPS.get() == napps {
        return;
    }
    // (nama huruf kecil, referensi). `seen` hanya dipakai untuk memeriksa duplikat apps.txt.
    let mut list: Vec<(String, StAllAppRef)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    {
        let idx = ST_IDX.borrow();
        for (i, e) in idx.iter().enumerate().take(ST_APP_INDEX_MAX as usize) {
            let key = e.name.to_lowercase();
            seen.insert(key.clone());
            list.push((key, StAllAppRef { kind: 0, reference: i as i32 }));
        }
    }
    {
        let apps = APPS.borrow();
        for (i, a) in apps.iter().enumerate().take(napps.clamp(0, MAXAPPS) as usize) {
            if list.len() >= ST_ALLAPP_MAX as usize {
                break;
            }
            if a.name.is_empty() {
                continue;
            }
            let key = a.name.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key.clone());
            list.push((key, StAllAppRef { kind: 1, reference: i as i32 }));
        }
    }
    list.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.kind.cmp(&b.1.kind))
            .then(a.1.reference.cmp(&b.1.reference))
    });
    let n = list.len();
    *ST_ALL_APPS.borrow_mut() = list.into_iter().map(|(_, r)| r).collect();
    ST_ALL_IDX_GEN.set(ST_IDX_GEN.get());
    ST_ALL_N_APPS.set(napps);
    if n == 0 {
        ST_ALL_SELECTED.set(-1);
    } else {
        let sel = ST_ALL_SELECTED.get();
        if sel < 0 || (sel as usize) >= n {
            ST_ALL_SELECTED.set(0);
        }
    }
}

/// Teks parsing untuk baris daftar semua aplikasi (`StAllAppParsingName` :293-309). None bila indeks di luar rentang.
pub fn st_all_app_parsing_name(index: i32) -> Option<String> {
    let r = ST_ALL_APPS.borrow().get(index as usize).copied()?;
    if r.kind == 0 {
        let idx = ST_IDX.borrow();
        let e = idx.get(r.reference as usize)?;
        return Some(format!("shell:AppsFolder\\{}", e.id));
    }
    let apps = APPS.borrow();
    let a = apps.get(r.reference as usize)?;
    if !a.aumid.is_empty() {
        return Some(format!("shell:AppsFolder\\{}", a.aumid));
    }
    Some(if a.launch.is_empty() {
        a.exe.clone()
    } else {
        a.launch.clone()
    })
}

/// Membuka baris daftar semua aplikasi (`StAllAppOpen` :311-318). Aplikasi khusus dibuka lewat `start_pick` (T07).
pub fn st_all_app_open(index: i32) {
    let Some(r) = ST_ALL_APPS.borrow().get(index as usize).copied() else {
        return;
    };
    if r.kind == 1 {
        start_pick(r.reference);
        return;
    }
    let Some(uri) = st_all_app_parsing_name(index) else {
        return;
    };
    panel_close();
    pan_uri(&uri);
}

/// Langkah tinggi baris daftar semua aplikasi (`StAllAppsMaxScroll` :320-326). Bila belum diukur, pakai S(40).
fn st_all_row_step() -> i32 {
    let v = ST_ALL_ROW_STEP.get();
    if v > 0 {
        v
    } else {
        s(40.0)
    }
}

/// Tinggi tampilan daftar semua aplikasi. Bila belum diukur, pakai S(400).
fn st_all_view_height() -> i32 {
    let v = ST_ALL_VIEW_HEIGHT.get();
    if v > 0 {
        v
    } else {
        s(400.0)
    }
}

/// Scroll maksimum daftar semua aplikasi (`StAllAppsMaxScroll` :320-326).
pub fn st_all_apps_max_scroll() -> i32 {
    let content = ST_ALL_APPS.borrow().len() as i32 * st_all_row_step();
    (content - st_all_view_height()).max(0)
}

/// Menjepit scroll daftar semua aplikasi ke [0, maks] (`StAllAppsClampScroll` :328-333).
pub fn st_all_apps_clamp_scroll() {
    let max = st_all_apps_max_scroll();
    ST_ALL_SCROLL.set(ST_ALL_SCROLL.get().clamp(0, max));
}

/// Melompat ke baris pertama dengan huruf awal `letter` (0 = a ... 25 = z) pada daftar semua aplikasi
/// (`StartAllAppsJumpToLetter` :335-352). Pencarian biner memakai urutan yang sudah disusun `st_all_apps_ensure`.
pub fn start_all_apps_jump_to_letter(letter: i32) {
    st_all_apps_ensure();
    if !(0..=25).contains(&letter) {
        return;
    }
    let names: Vec<char> = ST_ALL_APPS
        .borrow()
        .iter()
        .map(|r| st_first_lower(&st_all_app_name(*r)))
        .collect();
    let n = names.len();
    if n == 0 {
        return;
    }
    let target = char::from(b'a' + letter as u8);
    let lo = names.partition_point(|&c| c < target).min(n - 1);
    ST_ALL_SELECTED.set(lo as i32);
    ST_ALL_SCROLL.set(lo as i32 * st_all_row_step());
    st_all_apps_clamp_scroll();
}

// ---------------------------------------------------------------------------
// Hasil pencarian: penambahan, nama, dan papan klip (w12start.inc:355-392)
// ---------------------------------------------------------------------------

/// Hasil pencarian file terakhir (`g_fsRes`), atau None bila belum ada.
pub(crate) fn st_fs_result() -> Option<&'static FsResult> {
    let p = FS_RES.get();
    if p.is_null() {
        None
    } else {
        // SAFETY: pointer dikelola filesearch; hanya dibaca singkat di thread UI dan hasilnya disalin ke String.
        Some(unsafe { &*p })
    }
}

/// Menyalin string UTF-16 yang berakhir NUL (atau seluruh buffer) menjadi String.
fn st_wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Menambahkan hasil ke daftar teratas berkapasitas START_HITS (`StAddHit` :355-359).
fn st_add_hit(hit: StHit) {
    let mut hits = ST_HITS.borrow_mut();
    if hits.len() != START_HITS as usize {
        hits.resize(START_HITS as usize, StHit::default());
    }
    let mut n = ST_N_HITS.get();
    w12_search_top_add(&mut hits[..], Some(&mut n), Some(&hit));
    ST_N_HITS.set(n);
}

/// Nama tampilan sebuah hasil (`StHitName` :361-369). Di luar rentang → kosong.
pub(crate) fn st_hit_name(h: &StHit) -> String {
    match h.kind {
        ST_HIT_APP_INDEX => ST_IDX
            .borrow()
            .get(h.reference as usize)
            .map(|e| e.name.clone())
            .unwrap_or_default(),
        ST_HIT_SETTING => ST_SETTINGS
            .get(h.reference as usize)
            .map(|st| st.name.to_string())
            .unwrap_or_default(),
        ST_HIT_FILE => st_fs_result()
            .filter(|r| h.reference >= 0 && h.reference < r.n)
            .map(|r| st_wide_to_string(&r.name[h.reference as usize]))
            .unwrap_or_default(),
        ST_HIT_CALC => ST_CALC.borrow().clone(),
        ST_HIT_CMD => ST_CMD_TEXT.borrow().clone(),
        _ => APPS
            .borrow()
            .get(h.reference as usize)
            .map(|a| a.name.clone())
            .unwrap_or_default(),
    }
}

/// Menyalin teks ke papan klip sebagai CF_UNICODETEXT (`StCopyClip` :380-392).
fn st_copy_clip(text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: pemanggilan clipboard dan memori global; memori dibebaskan bila SetClipboardData gagal, dan clipboard selalu ditutup.
    unsafe {
        if OpenClipboard(Some(MSG.get())).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
            let dst = GlobalLock(g) as *mut u16;
            let mut ok = false;
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
                let _ = GlobalUnlock(g);
                ok = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(g.0))).is_ok();
            }
            if !ok {
                let _ = GlobalFree(Some(g));
            }
        }
        let _ = CloseClipboard();
    }
}

// ---------------------------------------------------------------------------
// Pencarian (w12start.inc:394-466)
// ---------------------------------------------------------------------------

/// Membentuk satu hasil dengan urutan bidang seperti `StAddHit` (:355-359).
fn st_make_hit(kind: i32, reference: i32, score: i32, match_pos: i32, match_len: i32, order: i32) -> StHit {
    StHit {
        kind,
        reference,
        score,
        match_pos,
        match_len,
        order,
    }
}

/// Teks kueri saat ini (`g_stQuery`) dari buffer UTF-16 dan panjang kueri.
fn st_query_string() -> String {
    let q = ST_QUERY.borrow();
    let n = (ST_Q_LEN.get().clamp(0, START_MAXQ) as usize).min(q.len());
    String::from_utf16_lossy(&q[..n])
}

/// Mengambil paling banyak `max_units` unit UTF-16 dari teks, sesuai batas buffer C++.
fn st_clip_units(text: &str, max_units: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().take(max_units).collect();
    String::from_utf16_lossy(&units)
}

/// True bila hasil teratas (k < nHits) sudah memakai nama ini tanpa membedakan huruf besar-kecil (`StSearch` :423).
fn st_has_hit_named(name: &str) -> bool {
    let n = ST_N_HITS.get().clamp(0, START_HITS) as usize;
    let want = name.to_lowercase();
    let hits = ST_HITS.borrow();
    (0..n).any(|k| {
        hits.get(k)
            .is_some_and(|h| st_hit_name(h).to_lowercase() == want)
    })
}

/// Menghitung ulang hasil untuk kueri saat ini (`StSearch` :394-466). Dipanggil saat kueri, indeks, atau berkas berubah.
pub fn st_search() {
    let query = st_query_string();
    if query.is_empty() {
        // Kueri kosong: hasil dan kunci kirim berkas ikut dikosongkan (dc).
        ST_N_HITS.set(0);
        ST_HITS_Q.borrow_mut().clear();
        ST_FS_SENT_Q.borrow_mut().clear();
        ST_FS_SENT_CAP.set(0);
        ST_FS_SENT_GEN.set(-1);
        return;
    }
    // Tidak berubah sejak pencarian terakhir: lewati (cm).
    if ST_HITS_GEN.get() == ST_IDX_GEN.get()
        && ST_HITS_FS_GEN.get() == FS_GEN.get()
        && *ST_HITS_Q.borrow() == query
    {
        return;
    }
    *ST_HITS_Q.borrow_mut() = st_clip_units(&query, START_MAXQ as usize);
    ST_HITS_GEN.set(ST_IDX_GEN.get());
    st_settings_ensure();

    let mut rest: &str = &query;
    let mode = w12_query_classify(&query, Some(&mut rest));
    let normal = mode == W12Q_NORMAL;
    ST_N_HITS.set(0);
    let napps = apps_count();
    let n_idx = ST_IDX.borrow().len() as i32;

    // Aplikasi dan pengaturan hanya di mode NORMAL (dg).
    if normal {
        let mut wq = W12SearchQuery::default();
        w12_search_make_query(&query, &mut wq);
        {
            let idx = ST_IDX.borrow();
            for (i, e) in idx.iter().enumerate() {
                let mut mp = -1i32;
                let mut ml = 0i32;
                let sc = w12_search_score(
                    &e.fields.lower,
                    &e.fields.initials,
                    Some(&wq),
                    Some(&mut mp),
                    Some(&mut ml),
                );
                if sc != 0 {
                    st_add_hit(st_make_hit(ST_HIT_APP_INDEX, i as i32, sc, mp, ml, i as i32));
                }
            }
        }
        // Aplikasi dok yang tidak ada di indeks (dup dicek terhadap hasil teratas saat ini).
        st_app_search_ensure();
        for i in 0..napps.clamp(0, MAXAPPS) as usize {
            let (sc, mp, ml) = {
                let sf = ST_APP_SEARCH.borrow();
                let Some(f) = sf.get(i).and_then(|x| x.fields.as_ref()) else {
                    continue;
                };
                let mut mp = -1i32;
                let mut ml = 0i32;
                let sc = w12_search_score(&f.lower, &f.initials, Some(&wq), Some(&mut mp), Some(&mut ml));
                (sc, mp, ml)
            };
            if sc == 0 {
                continue;
            }
            let name = APPS
                .borrow()
                .get(i)
                .map(|a| a.name.clone())
                .unwrap_or_default();
            if st_has_hit_named(&name) {
                continue;
            }
            st_add_hit(st_make_hit(ST_HIT_APP_LIST, i as i32, sc, mp, ml, n_idx + i as i32));
        }
        // Pengaturan: skor tanpa posisi kecocokan (mpos = -1, mlen = panjang kueri).
        {
            let sfs = ST_SETTING_FIELDS.borrow();
            for (i, f) in sfs.iter().enumerate() {
                let sc = w12_search_score(&f.lower, &f.initials, Some(&wq), None, None);
                if sc != 0 {
                    st_add_hit(st_make_hit(
                        ST_HIT_SETTING,
                        i as i32,
                        sc,
                        -1,
                        wq.length,
                        n_idx + napps + i as i32,
                    ));
                }
            }
        }
    }

    // Kalkulator: hanya bila hasilnya valid (cn).
    if mode == W12Q_CALC || (normal && w12_calc_looks_like(rest)) {
        let mut v = 0.0f64;
        if w12_calc_eval(rest, &mut v) {
            let mut buf = [0u16; 64];
            w12_calc_format(v, &mut buf);
            let num = st_wide_to_string(&buf);
            *ST_CALC.borrow_mut() = st_clip_units(&format!("= {num}"), 63);
            st_add_hit(st_make_hit(ST_HIT_CALC, 0, 1100, -1, 0, -1));
        }
    }

    // Perintah: hanya bila ada isinya (co).
    if mode == W12Q_CMD && !rest.is_empty() {
        *ST_CMD_TEXT.borrow_mut() = st_clip_units(&format!("Jalankan: {rest}"), 95);
        st_add_hit(st_make_hit(ST_HIT_CMD, 0, 1100, -1, 0, -1));
    }

    // Berkas: FsQuery hanya bila kunci kirim berubah (cl, dc).
    if mode == W12Q_NORMAL || mode == W12Q_FILES {
        let cap: i32 = if mode == W12Q_FILES { 20 } else { 8 };
        if rest.is_empty() {
            ST_FS_SENT_Q.borrow_mut().clear();
            ST_FS_SENT_CAP.set(0);
            ST_FS_SENT_GEN.set(-1);
        } else if !FS_RUNNING.get()
            || ST_FS_SENT_GEN.get() != FS_GEN.get()
            || ST_FS_SENT_CAP.get() != cap
            || ST_FS_SENT_Q.borrow().as_str() != rest
        {
            fs_query(rest, cap);
            if FS_RUNNING.get() {
                *ST_FS_SENT_Q.borrow_mut() = st_clip_units(rest, START_MAXQ as usize);
                ST_FS_SENT_CAP.set(cap);
                ST_FS_SENT_GEN.set(FS_GEN.get());
            }
        }
        // Hasil berkas hanya bila sesuai generasi kueri (dk).
        if !rest.is_empty() {
            if let Some(fs) = st_fs_result() {
                if fs.r#gen == FS_GEN.get() {
                    for i in 0..fs.n.clamp(0, 20) {
                        let h = fs.hit[i as usize];
                        st_add_hit(st_make_hit(
                            ST_HIT_FILE,
                            i,
                            h.score,
                            h.match_pos,
                            h.match_len,
                            100000 + i,
                        ));
                    }
                }
            }
        }
    }

    {
        let n = ST_N_HITS.get().clamp(0, START_HITS) as usize;
        if n > 0 {
            let mut hits = ST_HITS.borrow_mut();
            w12_search_top_sort(&mut hits[..n]);
        }
    }
    ST_HITS_FS_GEN.set(FS_GEN.get());
    if ST_SEL.get() > ST_N_HITS.get() {
        ST_SEL.set(ST_N_HITS.get());
    }
}

// ---------------------------------------------------------------------------
// Membuka hasil (w12start.inc:469-538)
// ---------------------------------------------------------------------------

/// Menjalankan perintah dari kueri `Jalankan:` (`StRunCommand` :469-487). Ctrl+Shift menjalankan sebagai admin.
fn st_run_command(rest: &str) {
    let units: Vec<u16> = rest.encode_utf16().collect();
    let max = START_MAXQ as usize;
    let quote = b'"' as u16;
    let space = b' ' as u16;
    let mut p = 0usize;
    let mut file: Vec<u16> = Vec::new();
    if units.first() == Some(&quote) {
        p = 1;
        while p < units.len() && units[p] != quote && file.len() < max {
            file.push(units[p]);
            p += 1;
        }
        if p < units.len() && units[p] == quote {
            p += 1;
        }
    } else {
        while p < units.len() && units[p] != space && file.len() < max {
            file.push(units[p]);
            p += 1;
        }
    }
    while p < units.len() && units[p] == space {
        p += 1;
    }
    let mut args: Vec<u16> = units[p..].iter().take(max).copied().collect();
    if file.is_empty() {
        return;
    }
    file.push(0);
    if !args.is_empty() {
        args.push(0);
    }
    // SAFETY: GetKeyState hanya membaca status tombol modifier.
    let admin = unsafe {
        (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0
            && (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0
    };
    // Seluruh string sudah disalin ke Vec sebelum panel ditutup (cw).
    panel_close();
    if !admin && SHELL_MODE.get() {
        // PORT: mode shell memakai jalur native (ActivateForProtocol/ActivateApplication) agar tidak butuh Explorer.
        // Bila jalur itu tidak berlaku, shell_open_with jatuh ke ShellExecute biasa. Admin (runas) tetap ShellExecute.
        let target = String::from_utf16_lossy(&file[..file.len() - 1]);
        let arg_s = (!args.is_empty()).then(|| String::from_utf16_lossy(&args[..args.len() - 1]));
        super::hotkeys::shell_open_with(&target, arg_s.as_deref());
        return;
    }
    let verb = to_wide_z(if admin { "runas" } else { "open" });
    let args_pw = if args.is_empty() { PCWSTR::null() } else { pcw(&args) };
    // SAFETY: ShellExecuteW menerima string NUL-terminated yang hidup selama pemanggilan.
    unsafe {
        let _ = ShellExecuteW(
            Some(MSG.get()),
            pcw(&verb),
            pcw(&file),
            args_pw,
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Membuka berkas atau folder hasil (`StOpenHit` :509-521). Folder di mode shell dibuka di jendela Explorer.
fn st_open_file_hit(idx: i32) {
    let Some(fs) = st_fs_result() else {
        return;
    };
    if idx < 0 || idx >= fs.n {
        return;
    }
    // Disalin dulu: pan_uri menutup panel (cw).
    let path = st_wide_to_string(&fs.path[idx as usize]);
    let wide = to_wide_z(&path);
    // SAFETY: GetFileAttributesW hanya membaca atribut dari string NUL-terminated.
    let attr = unsafe { GetFileAttributesW(pcw(&wide)) };
    if SHELL_MODE.get() && attr != INVALID_FILE_ATTRIBUTES && (attr & FILE_ATTRIBUTE_DIRECTORY.0) != 0 {
        panel_close();
        open_folder_window(&path);
        return;
    }
    pan_uri(&path);
}

/// Membuka hasil ke-`k` (`StOpenHit` :489-538). `k == n` adalah baris pencarian web.
pub fn st_open_hit(k: i32) {
    let n = ST_N_HITS.get();
    if k < 0 || k > n {
        return;
    }
    if k == n {
        // Baris web: kueri tanpa prefiks dikodekan menjadi URL pencarian (cp, cu, cv).
        let query = st_query_string();
        let mut rest: &str = &query;
        let _ = w12_query_classify(&query, Some(&mut rest));
        if rest.is_empty() {
            return;
        }
        let mut enc = [0u8; START_MAXQ as usize * 9 + 16];
        if w12_url_encode(rest, &mut enc) < 0 {
            return;
        }
        let enc_len = enc.iter().position(|&b| b == 0).unwrap_or(enc.len());
        let base = ini_string("search", "web", "https://www.google.com/search?q=", 511);
        let uri = st_clip_units(&format!("{base}{}", String::from_utf8_lossy(&enc[..enc_len])), 1099);
        pan_uri(&uri);
        return;
    }
    let h: StHit = match ST_HITS.borrow().get(k as usize) {
        Some(h) => *h,
        None => return,
    };
    match h.kind {
        ST_HIT_APP_LIST => start_pick(h.reference),
        ST_HIT_SETTING => {
            if let Some(s) = ST_SETTINGS.get(h.reference as usize) {
                pan_uri(s.uri);
            }
        }
        ST_HIT_FILE => st_open_file_hit(h.reference),
        ST_HIT_CALC => {
            // Hasil tanpa "= " disalin ke papan klip, lalu Start ditutup (cw).
            let calc = ST_CALC.borrow().clone();
            let value = st_clip_units(calc.strip_prefix("= ").unwrap_or(calc.as_str()), 63);
            st_copy_clip(&value);
            panel_close();
        }
        ST_HIT_CMD => {
            let query = st_query_string();
            let mut rest: &str = &query;
            let _ = w12_query_classify(&query, Some(&mut rest));
            st_run_command(rest);
        }
        _ => {
            let id = ST_IDX.borrow().get(h.reference as usize).map(|e| e.id.clone());
            if let Some(id) = id {
                pan_uri(&st_clip_units(&format!("shell:AppsFolder\\{id}"), 599));
            }
        }
    }
}

/// Pesan menu konteks (IContextMenu3, lalu IContextMenu2) yang diteruskan dari WndProc panel Start
/// (C++ `StartContextMenuMessage`, :544-562). Mengembalikan true bila pesan sudah ditangani.
pub fn start_context_menu_message(_h: HWND, m: u32, wp: WPARAM, lp: LPARAM, result: &mut LRESULT) -> bool {
    if !matches!(m, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
        return false;
    }
    let cm3 = ST_CONTEXT_MENU3.borrow().clone();
    if let Some(cm3) = cm3 {
        if unsafe { cm3.HandleMenuMsg2(m, wp, lp, Some(&mut *result as *mut LRESULT)) }.is_ok() {
            return true;
        }
    }
    let cm2 = ST_CONTEXT_MENU2.borrow().clone();
    if let Some(cm2) = cm2 {
        if unsafe { cm2.HandleMenuMsg(m, wp, lp) }.is_ok() {
            *result = LRESULT(0);
            return true;
        }
    }
    false
}

/// Ref konteks untuk id panel (C++ `W12StartContextFromHit` dengan basis PAN_ID_START_*).
fn start_context_ref(hit: i32) -> W12StartContextRef {
    w12_start_context_from_hit(
        hit,
        PAN_ID_START_PIN,
        PAN_ID_START_REC,
        PAN_ID_START_RES,
        apps_count(),
        ST_N_HITS.get(),
    )
}

/// Baris FILE di daftar Start (C++ `StartContextIsFile`, :564-570).
fn start_context_is_file(hit: i32) -> bool {
    let r = start_context_ref(hit);
    if r.kind != W12_CTX_SEARCH_RESULT || r.index < 0 || r.index >= ST_N_HITS.get() {
        return false;
    }
    let hits = ST_HITS.borrow();
    hits.get(r.index as usize).is_some_and(|h| h.kind == ST_HIT_FILE)
}

/// Nama parsing aplikasi: AUMID lalu jalur launch/exe (C++ `StartContextParsingName`, :581-606).
fn start_app_parsing_name(index: usize) -> Option<String> {
    let apps = APPS.borrow();
    let app = apps.get(index)?;
    if !app.aumid.is_empty() {
        return Some(format!("shell:AppsFolder\\{}", app.aumid));
    }
    let name = if app.launch.is_empty() { &app.exe } else { &app.launch };
    if name.is_empty() {
        None
    } else {
        Some(name.clone())
    }
}

/// Nama parsing untuk id panel (C++ `StartContextParsingName`, :572-607).
fn start_context_parsing_name(hit: i32) -> Option<String> {
    if hit >= PAN_ID_START_ALL_APP && hit - PAN_ID_START_ALL_APP < ST_ALL_N_APPS.get() {
        return st_all_app_parsing_name(hit - PAN_ID_START_ALL_APP);
    }
    let r = start_context_ref(hit);
    if r.kind == W12_CTX_APP {
        if r.index < 0 || r.index >= apps_count() {
            return None;
        }
        return start_app_parsing_name(r.index as usize);
    }
    if r.kind != W12_CTX_SEARCH_RESULT || r.index < 0 || r.index >= ST_N_HITS.get() {
        return None;
    }
    let sh: StHit = *ST_HITS.borrow().get(r.index as usize)?;
    match sh.kind {
        ST_HIT_FILE => {
            // Jalur penuh berkas dari pencarian berkas (cr).
            let fs = st_fs_result()?;
            if sh.reference < 0 || sh.reference >= fs.n {
                return None;
            }
            let path = st_wide_to_string(&fs.path[sh.reference as usize]);
            if path.is_empty() {
                None
            } else {
                Some(path)
            }
        }
        ST_HIT_APP_INDEX => {
            let ix = sh.reference;
            let id = {
                let list = ST_IDX.borrow();
                if ix < 0 || (ix as usize) >= list.len() {
                    return None;
                }
                list[ix as usize].id.clone()
            };
            if id.is_empty() {
                None
            } else {
                Some(format!("shell:AppsFolder\\{id}"))
            }
        }
        ST_HIT_APP_LIST => {
            if sh.reference < 0 || sh.reference >= apps_count() {
                return None;
            }
            start_app_parsing_name(sh.reference as usize)
        }
        _ => None,
    }
}

/// Titik tengah baris panel dalam koordinat layar (C++ `StartContextHitCenter`, :609-623).
fn start_context_hit_center(hit: i32) -> Option<POINT> {
    let pan = PAN.get();
    if pan.0.is_null() {
        return None;
    }
    let mut wr = RECT::default();
    if unsafe { GetWindowRect(pan, &mut wr) }.is_err() {
        return None;
    }
    let n = PAN_N_HIT.get().clamp(0, PAN_MAXHIT) as usize;
    let hits = PAN_HIT.borrow();
    for i in (0..n).rev() {
        let rec = hits[i];
        if rec.id != hit {
            continue;
        }
        let r = rec.r;
        let rise = (1.0 - start_pan_ease()) * sf(PAN_RISE as f32);
        let pm = s(PAN_MARGIN as f32);
        return Some(POINT {
            x: wr.left + pm + ((r.left + r.right) * 0.5) as i32,
            y: wr.top + pm + ((r.top + r.bottom) * 0.5 + rise) as i32,
        });
    }
    None
}

/// Salinan lokal `pan_ease` (panel.rs, privat): kurva easing panel saat muncul.
fn start_pan_ease() -> f32 {
    let u = 1.0 - PAN_T.get();
    1.0 - u * u * u * u
}

/// Id baris yang dipilih lewat keyboard (C++ `StartKeyboardContextHit`, :625-634).
fn start_keyboard_context_hit() -> Option<i32> {
    let sel = ST_ALL_SELECTED.get();
    if ST_ALL_APPS_MODE.get() && ST_Q_LEN.get() == 0 && sel >= 0 && sel < ST_ALL_N_APPS.get() {
        return Some(PAN_ID_START_ALL_APP + sel);
    }
    let sel = ST_SEL.get();
    if ST_Q_LEN.get() != 0 && sel >= 0 && sel < ST_N_HITS.get() {
        return Some(PAN_ID_START_RES + sel);
    }
    None
}

/// Menampilkan menu konteks shell untuk `parsing` lalu menjalankan perintah pilihan
/// (C++ `StartContextMenu` bagian pelacakan, :653-711).
fn start_context_track(owner: HWND, pt: POINT, parsing: &str, file_hit: bool) {
    let wide = to_wide_z(parsing);
    let mut pidl: *mut ITEMIDLIST = null_mut();
    let parsed = unsafe { SHParseDisplayName(pcw(&wide), None::<&IBindCtx>, &mut pidl, 0, None) }.is_ok();

    let mut parent: Option<IShellFolder> = None;
    let mut context_menu: Option<IContextMenu> = None;
    if parsed && !pidl.is_null() {
        let mut child: *mut ITEMIDLIST = null_mut();
        parent = unsafe {
            SHBindToParent::<IShellFolder>(pidl as *const ITEMIDLIST, Some(&mut child as *mut *mut ITEMIDLIST))
        }
        .ok();
        if let Some(folder) = parent.as_ref() {
            if !child.is_null() {
                let children = [child as *const ITEMIDLIST];
                context_menu = unsafe { folder.GetUIObjectOf::<IContextMenu>(owner, &children, None) }.ok();
            }
        }
    }

    let mut menu: Option<HMENU> = None;
    if let Some(cm) = context_menu.as_ref() {
        if let Ok(m) = unsafe { CreatePopupMenu() } {
            menu = Some(m);
            let qhr = unsafe { cm.QueryContextMenu(m, 0, 1, 0x7FFF, CMF_NORMAL | CMF_EXTENDEDVERBS) };
            if qhr.is_ok() {
                *ST_CONTEXT_MENU2.borrow_mut() = cm.cast::<IContextMenu2>().ok();
                *ST_CONTEXT_MENU3.borrow_mut() = cm.cast::<IContextMenu3>().ok();
                START_CONTEXT_MENU_OPEN.set(true);
                let _ = unsafe { SetForegroundWindow(owner) };
                let before = unsafe { GetForegroundWindow() };
                let flags = TPM_RETURNCMD.0 | TPM_RIGHTBUTTON.0 | TPM_LEFTALIGN.0;
                let command = unsafe { TrackPopupMenuEx(m, flags, pt.x, pt.y, owner, None) }.0 as u32;
                let _ = unsafe { PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0)) };
                START_CONTEXT_MENU_OPEN.set(false);
                *ST_CONTEXT_MENU2.borrow_mut() = None;
                *ST_CONTEXT_MENU3.borrow_mut() = None;

                if command != 0 {
                    let invoke = CMINVOKECOMMANDINFOEX {
                        cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
                        // CMIC_MASK_UNICODE (0x4000) tidak ada di crate windows 0.61; nilainya dipakai langsung.
                        fMask: CMIC_MASK_PTINVOKE | 0x0000_4000,
                        hwnd: owner,
                        lpVerb: PCSTR((command - 1) as usize as *const u8),
                        lpVerbW: PCWSTR((command - 1) as usize as *const u16),
                        nShow: SW_SHOWNORMAL.0,
                        ptInvoke: pt,
                        ..Default::default()
                    };
                    let _ = unsafe {
                        cm.InvokeCommand(&invoke as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO)
                    };
                    panel_close();
                } else if before == owner
                    && unsafe { GetForegroundWindow() } != owner
                    && PANEL_OPEN.get() == PN_START
                {
                    panel_close();
                }
            }
        }
    }

    if let Some(m) = menu {
        let _ = unsafe { DestroyMenu(m) };
    }
    drop(context_menu);
    drop(parent);
    if !pidl.is_null() {
        unsafe { CoTaskMemFree(Some(pidl as *const c_void)) };
    }
    if file_hit {
        // Menu berkas memuat pustaka shell; dilepas setelah menu tertutup (cr).
        unsafe { CoFreeUnusedLibraries() };
        trim_soon();
    }
}

/// Membuka menu konteks untuk baris Start di layar `(x, y)`; `(-1, -1)` berarti dari keyboard
/// (C++ `StartContextMenu`, :636-714).
pub fn start_context_menu(owner: HWND, x: i32, y: i32) -> bool {
    if PANEL_OPEN.get() != PN_START || START_CONTEXT_MENU_OPEN.get() {
        return false;
    }
    let (hit, pt) = if x == -1 && y == -1 {
        let Some(h) = start_keyboard_context_hit() else {
            return false;
        };
        let Some(c) = start_context_hit_center(h) else {
            return false;
        };
        (h, c)
    } else {
        let mut client = POINT { x, y };
        if !unsafe { ScreenToClient(owner, &mut client) }.as_bool() {
            return false;
        }
        (pan_hit_at(client.x, client.y), POINT { x, y })
    };
    let Some(parsing) = start_context_parsing_name(hit) else {
        return false;
    };
    // Dihitung sebelum menu tampil (cr).
    let file_hit = start_context_is_file(hit);

    let co = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let uninitialize = co.is_ok();
    if co.is_err() && co != RPC_E_CHANGED_MODE {
        return false;
    }
    start_context_track(owner, pt, &parsing, file_hit);
    if uninitialize {
        unsafe { CoUninitialize() };
    }
    true
}

/// Menjalankan `f` dengan ikon baris hasil `h` pada ukuran `px` (C++ `StHitIcon`, :716-733).
/// Ikon berkas dibuat sementara (tidak di-cache) karena `fs_icon` mengembalikan sumber WIC langsung.
/// Catatan: untuk baris aplikasi/indeks, `APPS`/`ST_IDX` dipegang selama `f` berjalan;
/// `f` tidak boleh menyentuh tabel tersebut.
pub fn st_hit_icon_with<R>(h: &StHit, px: i32, f: impl FnOnce(&mut Icon) -> R) -> R {
    match h.kind {
        ST_HIT_APP_LIST => {
            let mut apps = APPS.borrow_mut();
            if h.reference >= 0 {
                if let Some(app) = apps.get_mut(h.reference as usize) {
                    return f(&mut app.icon);
                }
            }
        }
        ST_HIT_FILE => {
            if let Some(fs) = st_fs_result() {
                if h.reference >= 0 && h.reference < fs.n {
                    let i = h.reference as usize;
                    let path = st_wide_to_string(&fs.path[i]);
                    if let Some(src) = fs_icon(&path, fs.flags[i], px) {
                        let mut tmp = Icon::new();
                        tmp.src = Some(src);
                        return f(&mut tmp);
                    }
                }
            }
        }
        ST_HIT_APP_INDEX => {
            let mut list = ST_IDX.borrow_mut();
            if h.reference >= 0 {
                if let Some(e) = list.get_mut(h.reference as usize) {
                    if !e.icon_tried {
                        e.icon_tried = true;
                        let name = format!("shell:AppsFolder\\{}", e.id);
                        e.icon.src = icon_from_parsing_name(&name, px);
                    }
                    return f(&mut e.icon);
                }
            }
        }
        _ => {}
    }
    let mut none = ST_NO_ICON.borrow_mut();
    f(&mut *none)
}

/// Deteksi NectarLink saat Start dibuka: ST_NL 0 = tidak terpasang, 1 = terpasang, 2 = berjalan
/// (C++ `StartNlCheck`, :736-749).
pub fn start_nl_check() {
    if ST_NL_EXE.borrow().is_empty() {
        let src = to_wide_z("%ProgramFiles%\\Nectarlink\\nectarlink-desktop.exe");
        let mut buf = [0u16; 260];
        unsafe { ExpandEnvironmentStringsW(pcw(&src), Some(&mut buf[..])) };
        *ST_NL_EXE.borrow_mut() = st_wide_to_string(&buf);
    }
    ST_NL.set(0);
    let exe = ST_NL_EXE.borrow().clone();
    let wide = to_wide_z(&exe);
    if unsafe { GetFileAttributesW(pcw(&wide)) } == INVALID_FILE_ATTRIBUTES {
        return;
    }
    ST_NL.set(1);
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return;
    };
    let mut pe: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    pe.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut ok = unsafe { Process32FirstW(snap, &mut pe) }.is_ok();
    while ok {
        if st_wide_to_string(&pe.szExeFile).to_lowercase() == "nectarlink-desktop.exe" {
            ST_NL.set(2);
            break;
        }
        ok = unsafe { Process32NextW(snap, &mut pe) }.is_ok();
    }
    let _ = unsafe { CloseHandle(snap) };
}

/// Memuat ikon NectarLink sekali bila terpasang (C++ `StartNlIconLoad`, :751-756).
pub fn start_nl_icon_load() {
    if ST_NL_ICON_TRIED.get() || ST_NL.get() == 0 {
        return;
    }
    ST_NL_ICON_TRIED.set(true);
    let exe = ST_NL_EXE.borrow().clone();
    ST_NL_ICON.borrow_mut().src = icon_from_parsing_name(&exe, s(28.0));
}

/// Menandai daftar Start perlu dibangun ulang. Pemanggil: core_c2.rs (relayout), start_draw.rs (43, 86).
pub fn start_lists_invalidate() {
    ST_LISTS_READY.set(false);
    ST_HITS_GEN.set(-1);
}

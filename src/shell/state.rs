//! Global dan konstanta shell bersama. Sumber: win12shell.cpp dan .inc (lihat docs/port/shell-split.md).
//! Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime.

use std::cell::{Cell, RefCell};
use std::sync::atomic::AtomicI32;

use windows::core::{GUID, IInspectable};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
};
use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, PROPERTYKEY, RECT};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Direct2D::{
    ID2D1DCRenderTarget, ID2D1Factory, ID2D1LinearGradientBrush, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{IDWriteFactory, IDWriteTextFormat};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::Imaging::IWICImagingFactory;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::IMMDeviceEnumerator;
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use crate::sync::SyncCell;

// ---- Pesan jendela (WM_APP + n) ----
pub const WM_W12_RENDER: u32 = WM_APP + 40;
pub const WM_APPBAR: u32 = WM_APP + 1;
pub const WM_VOLCHG: u32 = WM_APP + 2;
pub const WM_NETCHG: u32 = WM_APP + 3;
pub const WM_PEEK: u32 = WM_APP + 10;
pub const WM_SETMODE: u32 = WM_APP + 11;
pub const WM_WINKEY: u32 = WM_APP + 12;
pub const WM_W12_BRI: u32 = WM_APP + 13;
pub const WM_W12_APPIDX: u32 = WM_APP + 14;
pub const WM_W12_RADIO: u32 = WM_APP + 15;
pub const WM_W12_WLAN: u32 = WM_APP + 16;
pub const WM_W12_MEDIAEV: u32 = WM_APP + 17;
pub const WM_W12_NLRESULT: u32 = WM_APP + 18;
pub const WM_W12_SHELLKEY: u32 = WM_APP + 19;
pub const WM_W12_TASK: u32 = WM_APP + 20;
pub const WM_W12_WALL: u32 = WM_APP + 21;
pub const WM_W12_FXHITS: u32 = WM_APP + 23;
pub const WM_W12_WALLCHG: u32 = WM_APP + 24;
pub const WM_W12_DIAG: u32 = WM_APP + 25;
pub const WM_W12_NEEDEXP: u32 = WM_APP + 26;
/// Notifikasi Windows baru menunggu di antrean notif_worker.
pub const WM_W12_NOTIF: u32 = WM_APP + 27;
/// Tombol volume fisik/roda: wp = APPCOMMAND_VOLUME_UP/DOWN/MUTE. Dikirim dari kait keyboard (core_c4b.rs);
/// Windows tidak menampilkan OSD volume bawaan karena tombol ditelan di kait.
pub const WM_W12_VOLKEY: u32 = WM_APP + 28;
/// Lirik untuk lagu aktif sudah diambil (lyrics.rs); UI cukup menggambar ulang notch.
pub const WM_W12_LYRIC: u32 = WM_APP + 29;
/// Pemberitahuan shell dari folder Start Menu/AppsFolder (start.rs `st_idx_register_notify`, 2026-11-24).
pub const WM_W12_STARTNOTIFY: u32 = WM_APP + 30;

// ---- Timer ----
pub const T_CLOCK: i32 = 1;
pub const T_STATUS: i32 = 2;
pub const T_MEDIA: i32 = 3;
pub const T_MEDIA_FAST: i32 = 4;
pub const T_REFRESH: i32 = 5;
pub const T_ANIM: i32 = 6;
pub const T_TIP: i32 = 7;
pub const T_TRIM: i32 = 8;
pub const T_PEEK: i32 = 9;
pub const T_FGPOLL: i32 = 10;
pub const T_TRAYSYNC: i32 = 11;
pub const T_MINANIM: i32 = 12;
pub const T_QUIET: i32 = 13;
pub const T_SYSW: i32 = 14;
pub const T_NL: i32 = 15;
pub const T_NL_ICON: i32 = 16;
pub const T_WALLPAPER: i32 = 17;
pub const T_START_TRIM: i32 = 18;
pub const T_FSSTART: i32 = 19;
pub const T_DIAG: i32 = 20;
/// Cek status internet (net_watch.rs).
pub const T_NETCHK: i32 = 21;
/// Animasi scroll halus dan animasi masuk "Semua aplikasi" (start_draw.rs).
pub const T_ALLSCROLL: i32 = 22;
/// Debounce pemindaian indeks aplikasi setelah perubahan Start Menu (start.rs, 2026-11-24).
pub const T_APPIDX_DEBOUNCE: i32 = 23;
/// Pemindaian berkala indeks aplikasi, untuk aplikasi Store/paket yang tidak memberi pemberitahuan.
pub const T_APPIDX_POLL: i32 = 25;

// ---- Tata letak, mode, jaringan (C1) ----
pub const BAR_H: i32 = 48;
pub const BAR_H_SMALL: i32 = 38;
pub const ICON_PX: i32 = 26;
pub const ICON_PX_SM: i32 = 22;
pub const SLOT: i32 = 44;
pub const SLOT_SMALL: i32 = 36;
pub const TRAY_SLOT: i32 = 30;
pub const TRAY_SLOT_SMALL: i32 = 26;
pub const PILL_PAD: i32 = 4;
pub const GAP: i32 = 6;
pub const MARGIN_B: i32 = 8;
pub const RESERVE_GAP: i32 = 4;
pub const TOP_H: i32 = 40;
pub const TOP_M: i32 = 6;
pub const MODE_SEGMENTED: i32 = 0;
pub const MODE_DOCK: i32 = 1;
pub const MODE_CLASSIC: i32 = 2;
pub const MAXW: i32 = 24;
pub const MAXAPPS: i32 = 48;
pub const NET_NONE: i32 = 0;
pub const NET_WIFI: i32 = 1;
pub const NET_ETH: i32 = 2;
pub const NET_BOTH: i32 = 3;
pub const W12_KBSIG: usize = 0x5731_3253;

// ---- Pil dan item dok (C2) ----
pub const IT_NOTE: i32 = 0;
pub const IT_APP: i32 = 1;
pub const IT_SEARCH: i32 = 2;
pub const IT_START: i32 = 3;
pub const IT_TASKVIEW: i32 = 4;
pub const IT_NET: i32 = 5;
pub const IT_VOL: i32 = 6;
pub const IT_BAT: i32 = 7;
pub const IT_TCLOCK: i32 = 8;
pub const IT_CHEV: i32 = 9;
pub const IT_BELL: i32 = 10;
pub const IT_CLOCK: i32 = 11;
pub const IT_MEDIA_MAIN: i32 = 12;
pub const IT_MEDIA_PREV: i32 = 13;
pub const IT_MEDIA_PLAY: i32 = 14;
pub const IT_MEDIA_NEXT: i32 = 15;
pub const IT_USB: i32 = 16;
pub const MAXITEMS: i32 = 96;
pub const MAXPILLS: i32 = 10;
pub const NBOTTOM: i32 = 6;
pub const P_SYS: i32 = 6;
pub const P_CAL: i32 = 7;
pub const P_CLOCK: i32 = 8;
pub const P_MEDIA: i32 = 9;
pub const P_TOP: i32 = P_SYS;
pub const PN_START: i32 = 0;
pub const PN_QS: i32 = 1;
pub const PN_WIFI: i32 = 2;
pub const PN_BATT: i32 = 3;
pub const PN_CAL: i32 = 4;

// ---- Panel dan ID elemen panel ----
pub const PAN_MARGIN: i32 = 24;
pub const PAN_RADIUS: i32 = 24;
pub const PAN_RISE: i32 = 14;
pub const PAN_EDGE: i32 = 12;
pub const PAN_GAP: i32 = 8;
pub const PAN_MAXHIT: i32 = 160;
pub const PAN_ID_QS_TILE: i32 = 100;
pub const PAN_ID_VOL_TRACK: i32 = 200;
pub const PAN_ID_VOL_MUTE: i32 = 201;
pub const PAN_ID_QS_SET: i32 = 202;
pub const PAN_ID_BRI_TRACK: i32 = 203;
pub const PAN_ID_QS_AUDIO: i32 = 204;
pub const PAN_ID_BRI_ICON: i32 = 205;
pub const PAN_ID_WIFI_SET: i32 = 300;
pub const PAN_ID_WIFI_ROW: i32 = 301;
pub const PAN_ID_WIFI_TOG: i32 = 302;
pub const PAN_ID_WIFI_SCAN: i32 = 303;
pub const PAN_ID_WIFI_DISC: i32 = 304;
pub const PAN_ID_WIFI_PERM: i32 = 305;
pub const PAN_ID_WIFI_MORE: i32 = 306;
pub const PAN_ID_WIFI_LOGIN: i32 = 307;
pub const PAN_ID_ETH_ROW: i32 = 308;
pub const PAN_ID_WIFI_NET: i32 = 310;
pub const PAN_ID_BATT_SAVER: i32 = 400;
pub const PAN_ID_BATT_SET: i32 = 401;
pub const PAN_ID_BATT_MORE: i32 = 402;
pub const PAN_ID_BATT_MODE: i32 = 403;
pub const PAN_ID_CAL_PREV: i32 = 500;
pub const PAN_ID_CAL_NEXT: i32 = 501;
pub const PAN_ID_CAL_TODAY: i32 = 502;
pub const PAN_ID_START_Q: i32 = 900;
pub const PAN_ID_START_PHONE: i32 = 901;
pub const PAN_ID_START_GEAR: i32 = 902;
pub const PAN_ID_START_USER: i32 = 903;
pub const PAN_ID_START_NL: i32 = 904;
pub const PAN_ID_START_POWER: i32 = 905;
pub const PAN_ID_START_SLEEP: i32 = 906;
pub const PAN_ID_START_RESTART: i32 = 907;
pub const PAN_ID_START_OFF: i32 = 908;
pub const PAN_ID_START_YES: i32 = 909;
pub const PAN_ID_START_NO: i32 = 910;
pub const PAN_ID_START_DISMISS: i32 = 911;
pub const PAN_ID_START_PMENU: i32 = 912;
pub const PAN_ID_START_FILES: i32 = 913;
pub const PAN_ID_START_OPEN: i32 = 914;
pub const PAN_ID_START_ALL_VIEW: i32 = 915;
pub const PAN_ID_START_HOME: i32 = 916;
pub const PAN_ID_START_PIN: i32 = 1000;
pub const PAN_ID_START_REC: i32 = 2000;
pub const PAN_ID_START_RES: i32 = 3000;
pub const PAN_ID_START_ALL_APP: i32 = 5000;
pub const PAN_ID_START_ALPHA: i32 = 10000;

// ---- Notch, tugas, snap, file search, NL ----
pub const NT_TOP: i32 = 10;
pub const NT_ANIM_MS: i32 = 180;
pub const NT_GROW_MS: i32 = 260;
pub const NT_HOTKEY: i32 = 0x4E54;
pub const NT_MAXHIT: i32 = 48;
pub const TK_SW_STEP: i32 = 1;
pub const TK_SW_COMMIT: i32 = 2;
pub const TK_SW_CANCEL: i32 = 3;
pub const TK_TASKVIEW: i32 = 4;
pub const TK_DESK_GO: i32 = 5;
pub const TK_DESK_NEW: i32 = 6;
pub const TK_DESK_CLOSE: i32 = 7;
pub const TK_MOVE_WIN: i32 = 8;
pub const TK_GEST_UP: i32 = 9;
pub const TK_GEST_DOWN: i32 = 10;
pub const TK_COUNT: i32 = 11;
pub const TKT_SWSHOW: i32 = 1;
pub const TKT_FOCUS: i32 = 2;
pub const TKT_HUD: i32 = 3;
pub const TKT_TVREBUILD: i32 = 4;
pub const VD_MAX: i32 = 9;
pub const VD_WINS: i32 = 256;
pub const SNL_LEFT: i32 = 1;
pub const SNL_RIGHT: i32 = 2;
pub const SNL_TOP: i32 = 3;
pub const SNL_BOTTOM: i32 = 4;
pub const SNL_TL: i32 = 5;
pub const SNL_TR: i32 = 6;
pub const SNL_BL: i32 = 7;
pub const SNL_BR: i32 = 8;
pub const SNL_L3: i32 = 9;
pub const SNL_C3: i32 = 10;
pub const SNL_R3: i32 = 11;
pub const SNL_L23: i32 = 12;
pub const SNL_R23: i32 = 13;
pub const SNL_MAX: i32 = 14;
pub const FS_MAXHITS: i32 = 20;
pub const FS_PATH: i32 = 1024;
pub const NL_API_IDLE: i32 = 0;
pub const NL_API_LOADING: i32 = 1;
pub const NL_API_READY: i32 = 2;
pub const NL_API_NO_TOKEN: i32 = 3;
pub const NL_API_AUTH: i32 = 4;
pub const NL_API_ERROR: i32 = 5;
pub const NL_API_BAD_DATA: i32 = 6;

// ---- Nilai konstan non-enum ----
pub const NOTE_APP: &str = "shell:AppsFolder\\Microsoft.WindowsNotepad_8wekyb3d8bbwe!App";
pub const NOTE_T1: &str = "Create new note";
pub const NOTE_T2: &str = "Notepad";
pub const PK_AUMID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
    pid: 5,
};
// win12shell.cpp:545 PK_TARGET
pub const PK_TARGET: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xb9b4b3fc_2b51_4a42_b5d8_324146afcf25),
    pid: 2,
};
pub const QS_ON_INK: D2D1_COLOR_F = D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

// ---- Global C1: jendela, tampilan, pengaturan ----
// Global yang juga dibaca thread pekerja/hook memakai pembungkus atomik (bebas data race).
pub static HI: crate::sync::AtomicHandle<HINSTANCE> = crate::sync::AtomicHandle::null();
pub static MSG: crate::sync::AtomicHandle<HWND> = crate::sync::AtomicHandle::null();
pub static TIP: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
pub static WM_SHELLHOOK: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
pub static WM_TASKBARCREATED: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
pub static DPI: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(96));
pub static DIR: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static INI: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static APPS_TXT: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static MODE: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(MODE_SEGMENTED));
pub static SMALL: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static WIDGETS: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
pub static WIN_KEY: crate::sync::AtomicFlag = crate::sync::AtomicFlag::new(true);
pub static GLASS: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
pub static DESK_LAYER: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
pub static NOTCH_ON: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
pub static NOTCH_HUD: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
// [shell] notchlive: pulau ringkas menetap saat media diputar (default aktif)
pub static NOTCH_LIVE: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
// [shell] notchalways: 1 = notch selalu terlihat sebagai garis kecil di tepi atas tengah; 0 = auto-hide (default).
pub static NOTCH_ALWAYS: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
// [shell] notchlyrics: 1 = baris lirik tersinkron (LRCLIB) menggantikan artis di pulau media (default aktif).
pub static NOTCH_LYRICS: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
// [shell] notchmedtemp: 1 = pulau media hanya muncul sementara (intip 3 dtk saat lagu baru atau putar/jeda);
// 0 = pulau media menetap selama media diputar (default, perilaku sebelumnya).
pub static NOTCH_MEDIA_TEMP: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
// [shell] notchcompact: 1 = pulau media ringkas (sampul + bar, tanpa judul). Default 0: judul, artis, dan kontrol tampil selama media diputar.
pub static NOTCH_COMPACT: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
// Notch saat layar penuh (2026-11-19): [shell] notchfsvideo / notchfsgame.
// 0 = sembunyi total (jangan ganggu), 1 = notifikasi & status (volume, kecerahan, baterai), 2 = juga media dan zona kursor.
pub static NOTCH_FS_VIDEO: SyncCell<Cell<u8>> = SyncCell::new(Cell::new(1));
pub static NOTCH_FS_GAME: SyncCell<Cell<u8>> = SyncCell::new(Cell::new(1));
// Jenis jendela layar penuh di depan: 0 = tidak layar penuh, 1 = video (peramban/pemutar), 2 = game/lainnya.
pub static FS_KIND: SyncCell<Cell<u8>> = SyncCell::new(Cell::new(0));
pub static SHELL_MODE: crate::sync::AtomicFlag = crate::sync::AtomicFlag::new(false);
pub static FILES_OWN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
pub static THEME_PREF: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(1));
pub static ORIG_AUTO_HIDE: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static LIGHT: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static GLASS_OK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static MICA_FULLSCREEN_SNAPSHOT: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

// ---- Global C1: Direct2D, DirectWrite, WIC, GDI (COM: RefCell<Option<T>>) ----
pub static D2D: SyncCell<RefCell<Option<ID2D1Factory>>> = SyncCell::new(RefCell::new(None));
pub static DW: SyncCell<RefCell<Option<IDWriteFactory>>> = SyncCell::new(RefCell::new(None));
pub static WIC: SyncCell<RefCell<Option<IWICImagingFactory>>> = SyncCell::new(RefCell::new(None));
pub static RT: SyncCell<RefCell<Option<ID2D1DCRenderTarget>>> = SyncCell::new(RefCell::new(None));
pub static BR: SyncCell<RefCell<Option<ID2D1SolidColorBrush>>> = SyncCell::new(RefCell::new(None));
pub static START_BRUSH: SyncCell<RefCell<Option<ID2D1LinearGradientBrush>>> = SyncCell::new(RefCell::new(None));
pub static RT_GEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(1));
pub static TF_TITLE: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_SUB: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_TRAY: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_GLYPH: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_GLYPH_SM: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_GLYPH_MD: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_BIG: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_GLYPH_LG: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_HUGE: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_TZ: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_H: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_B: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_BS: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_S: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_SS: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_XS: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_PCT: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_SSC: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_SC: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_XSC: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_MEDC: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_T2: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
// Ditambah integrator (Tahap B): g_tfClock, g_tfDate, g_tfMed (win12shell.cpp:305-308) dan g_startStops (:302).
pub static TF_CLOCK: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_DATE: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static TF_MED: SyncCell<RefCell<Option<IDWriteTextFormat>>> = SyncCell::new(RefCell::new(None));
pub static START_STOPS: SyncCell<RefCell<Option<windows::Win32::Graphics::Direct2D::ID2D1GradientStopCollection>>> =
    SyncCell::new(RefCell::new(None));
pub static MEMDC: SyncCell<Cell<HDC>> = SyncCell::new(Cell::new(HDC(std::ptr::null_mut())));

// ---- Global C1: aplikasi, jaringan, baterai, waktu ----
pub static NWC: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static DEV_ENUM: SyncCell<RefCell<Option<IMMDeviceEnumerator>>> = SyncCell::new(RefCell::new(None));
pub static EP_VOL: SyncCell<RefCell<Option<IAudioEndpointVolume>>> = SyncCell::new(RefCell::new(None));
pub static VOL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static MUTE: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static NET: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(NET_NONE));
/// Diganti nama dari g_netEth: NET_ETH sudah dipakai konstanta (= 2).
pub static NET_ETH_OK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static SSID: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static ETH_NAME: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static ETH_DESC: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static ETH_IP: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static ETH_SPEED: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
pub static NET_NOTIFY: SyncCell<Cell<HANDLE>> = SyncCell::new(Cell::new(HANDLE(std::ptr::null_mut())));
pub static BAT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static CHARGING: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static HAS_BAT: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static TIME: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static TZ: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static TIME_TZ: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static DATE: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static DATE_SHORT: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));

// ---- Global C1: media (WinRT via crate windows::Media::Control) ----
pub static ART_PX: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static MGR: SyncCell<RefCell<Option<GlobalSystemMediaTransportControlsSessionManager>>> =
    SyncCell::new(RefCell::new(None));
pub static SESS: SyncCell<RefCell<Option<GlobalSystemMediaTransportControlsSession>>> =
    SyncCell::new(RefCell::new(None));
pub static OP_MGR: SyncCell<RefCell<Option<IInspectable>>> = SyncCell::new(RefCell::new(None));
pub static OP_PROPS: SyncCell<RefCell<Option<IInspectable>>> = SyncCell::new(RefCell::new(None));
pub static OP_THUMB: SyncCell<RefCell<Option<IInspectable>>> = SyncCell::new(RefCell::new(None));
pub static MEDIA_FAILED: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static MEDIA_TICKS: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
pub static MEDIA_EV: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static MEDIA_WANT: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static MGR_HOOKED: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static TOK_SESSION: SyncCell<Cell<i64>> = SyncCell::new(Cell::new(0));

// ---- Global C2: pil, drag, mica, glow ----
pub static NITEMS: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static HOT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static TRACKING: SyncCell<Cell<[bool; MAXPILLS as usize]>> =
    SyncCell::new(Cell::new([false; MAXPILLS as usize]));
pub static FULLSCREEN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
/// Mode game: jendela depan fullscreen. Ditulis thread UI (`update_foreground`), dibaca thread kait keyboard.
pub static GAME_FS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// `[shell] gamehook` (hanya dibaca): 1 = kait tetap aktif saat fullscreen dengan logika minimum, 0 = kait dilepas
/// saat fullscreen (hanya di luar mode shell; di mode shell kait dibutuhkan untuk Alt+Tab dan tombol Win).
pub static GAME_HOOK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
pub static FG_VALID: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static QUITTING: crate::sync::AtomicFlag = crate::sync::AtomicFlag::new(false);
pub static AB_REG: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static PEEK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static FLY_OPEN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static FULLSCREEN_START_PEEK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static POP_OPEN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static QUIET: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static PANEL_OPEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static DOCK_HX: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(-1.0));
pub static GLOW: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
pub static FG_RECT: SyncCell<Cell<RECT>> =
    SyncCell::new(Cell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }));
pub static WG_SCALE: SyncCell<Cell<[f32; 2]>> = SyncCell::new(Cell::new([1.0, 1.0]));
pub static WG_HOVER: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static DRAG_ON: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static DRAG_CAND: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static DRAG_ITEM: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static SETTLE_APP: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static SETTLE_K: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
pub static ANIM_ON: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static BOUNCE_KEY: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static BOUNCE_START: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
pub static USB_COUNT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static DESK_Z_DIRTY: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
pub static MICA_PATH: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static MICA_FILE_SIZE: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
pub static MICA_FILE_WRITE: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
pub static GLOW_DIRTY: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));

// ---- Global C3: taskbar, tray, flyout, animasi ----
pub static TRAY_WND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
pub static TRAY_PID: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
pub static NTI: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static TRAY_HOST: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
pub static FLY: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
// TRAY_LOST dipindah ke core_c3.rs (satu-satunya definisi; lihat core_c3::TRAY_LOST).
pub static DESK: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
pub static ANIM_WND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
pub static ANIM_FROM: SyncCell<Cell<RECT>> =
    SyncCell::new(Cell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }));
pub static ANIM_TO: SyncCell<Cell<RECT>> =
    SyncCell::new(Cell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }));
pub static ANIM_START: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));

// ---- Global non-C (shared antar owner) ----
pub static CPU: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(-1.0));
pub static SYS_TIMER: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static WG_MODE: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static NL_API_STATE: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(NL_API_IDLE));
pub static PAN: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
pub static START_CONTEXT_MENU_OPEN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static PAN_N_HIT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static PAN_HOV: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static PAN_DRAG: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static QS_SCROLL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static RAD_WIFI: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
/// Volatile LONG (qs): diakses lintas thread, jadi AtomicI32 dan bukan SyncCell.
pub static RAD_BUSY: AtomicI32 = AtomicI32::new(0);
pub static RAD_PEND_KIND: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static RAD_PEND_ON: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static NT: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
pub static NT_MSG: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static NT_MSG_WARN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static NT_MSG_GLYPH: SyncCell<Cell<u16>> = SyncCell::new(Cell::new(0));
pub static FS_GEN: crate::sync::AtomicI32Cell = crate::sync::AtomicI32Cell::new(0);
pub static FS_RUNNING: crate::sync::AtomicFlag = crate::sync::AtomicFlag::new(false);
pub static ST_ALL_APPS_MODE: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static ST_HITS_Q: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
pub static ST_Q_LEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static ST_NL_ICON_TRIED: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static TK_MSG: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
/// Volatile LONG (tasks_ui): AtomicI32 dan bukan SyncCell.
pub static WALL_GEN: AtomicI32 = AtomicI32::new(0);
pub static VD_N: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static VD_COUNT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(1));
pub static HUD: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
pub static EXPLORER_ON_EXIT: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static WIN_KEY_TICK: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));

// Enum dari sumber (diverifikasi: w12notch.inc:24-27, w12overlay.inc:5, win12shell.cpp:773, 1480).
pub const NT_IDLE: i32 = 0;
pub const NT_PEEK: i32 = 1;
pub const NT_HUD: i32 = 2;
pub const NT_OPEN: i32 = 3;
pub const HUD_VOL: i32 = 0;
pub const HUD_BRI: i32 = 1;
pub const HUD_MSG: i32 = 2;
/// Kartu notifikasi Windows (notif_worker.rs → notch_notify).
pub const HUD_NOTIF: i32 = 3;
pub const NTT_HOME: i32 = 0;
pub const NTT_TIMER: i32 = 1;
pub const NTT_NOTES: i32 = 2;
pub const OV_NONE: i32 = 0;
pub const OV_SWITCH: i32 = 1;
pub const OV_TV: i32 = 2;
pub const MAXWC: i32 = 128;
pub const G_NOTE: i32 = 0;
pub const G_LEFT: i32 = 1;
pub const G_CENTER: i32 = 2;
pub const G_RIGHT: i32 = 3;
pub const G_RUN: i32 = 4;
pub const G_TRAY: i32 = 5;
pub const G_COUNT: i32 = 6;

// Catatan 2026-11-14 (diperiksa dengan grep): daftar di bawah adalah riwayat. Item-itemnya sudah dideklarasikan di file pemilik,
// dengan nama HK_* menjadi HK_WEV_* dan g_thumb menjadi ANIM_THUMB (core_c3b.rs). PFN_CREATE_STREAM_OVER_RAS tidak dideklarasikan:
// crate windows dipakai langsung (core_c1.rs:111), bukan LoadLibrary seperti C++.
// Daftar asal:
// PALETTE (C1: Palette, sumber :179), WALLPAPER_TINT (C1: W12Rgb8, :180),
// PFN_GET_APP_USER_MODEL_ID (C1: typedef + pointer, :548), PFN_CREATE_STREAM_OVER_RAS (C1, :1285),
// APPS (C1: App[MAXAPPS], :589), NOTE_ICON (C1: Icon, :592), WC (C1: WinInfo[MAXWC], :774), MEDIA (C1: MediaState, :1187),
// ITEMS (C2: Item[MAXITEMS], :1485), PILLS (C2: Pill[MAXPILLS], :1487),
// PAN_HIT (panel: PanHitRec[PAN_MAXHIT], :27), FS_RES (filesearch: FsResult*, :43), VD (tasks_ui: VdWin[VD_WINS], :255),
// NL_DEVICES (nl_ui:7: NlJsonDeviceList dari crate nl_json, perlu Option karena bukan const fn),
// HK_FG, HK_MIN, HK_CLOAK, HK_LOC, HK_LIFE (C3: HWINEVENTHOOK, butuh fitur Win32_UI_Accessibility),
// THUMB (C3: HTHUMBNAIL, tipe belum ditemukan di windows 0.61.3),
// NT_STATE (g_ntState, int) dan NT_HUD_KIND (g_ntHud, int berisi HUD_VOL/BRI/MSG). Nama NT_HUD sudah dipakai konstanta state, jadi global diberi nama lain (usulan, diputuskan saat notch.rs dibuat).
// Tidak dipindah: CLSID_MMDevEnum, IID_MMDevEnum, IID_EPVol, IID_MediaStatics, IID_StreamRef (pakai IID/CLSID crate), g_mstat (dihapus).

//! Sumber: w12flyout.inc (seluruh file). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H08.
//! Flyout Wi-Fi (thread WLAN, daftar jaringan, sakelar radio) dan Baterai (riwayat, mode daya, ikon, grafik).
//! Pemanggil lintas owner: panel.rs (PanelContent/PanelMeasure/PanelActivate) dan handler pesan utama
//! (WM_W12_WLAN -> wifi_wlan_notify, perubahan radio -> wifi_radio_changed).

use std::cell::{Cell, RefCell};
use std::ffi::{c_void, CStr};
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use windows::Win32::Globalization::{
    MultiByteToWideChar, CP_ACP, CP_UTF8, MB_ERR_INVALID_CHARS, MULTI_BYTE_TO_WIDE_CHAR_FLAGS,
};
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    GET_ADAPTERS_ADDRESSES_FLAGS, IP_ADAPTER_ADDRESSES_LH,
};
use windows::core::PCSTR;
use windows::Win32::NetworkManagement::WiFi::{
    wlan_connection_mode_profile, wlan_intf_opcode_current_connection,
    wlan_interface_state_connected, wlan_notification_acm_connection_attempt_fail,
    wlan_notification_acm_connection_complete, wlan_notification_acm_disconnected,
    wlan_notification_acm_scan_complete, wlan_notification_acm_scan_fail, DOT11_BSS_TYPE, DOT11_SSID,
    L2_NOTIFICATION_DATA, WLAN_AVAILABLE_NETWORK, WLAN_AVAILABLE_NETWORK_CONNECTED,
    WLAN_AVAILABLE_NETWORK_HAS_PROFILE, WLAN_AVAILABLE_NETWORK_LIST, WLAN_BSS_ENTRY, WLAN_BSS_LIST,
    WLAN_CONNECTION_ATTRIBUTES, WLAN_CONNECTION_PARAMETERS, WLAN_INTERFACE_INFO,
    WLAN_INTERFACE_INFO_LIST, WLAN_INTF_OPCODE, WLAN_NOTIFICATION_CALLBACK,
    WLAN_NOTIFICATION_SOURCE_ACM, WLAN_OPCODE_VALUE_TYPE,
};
use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
use windows::Win32::System::Com::StringFromGUID2;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTimeAsFileTime};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, SetEvent, WaitForSingleObject};
use windows::Win32::UI::Shell::ShellExecuteW;

use super::prelude::*;
use crate::sync::SyncCell;
use crate::wide::{from_wide, pcw, to_wide_z};

// ---------------------------------------------------------------- konstanta (sumber baris 8, 14, 33-111)
const FLY_WIFI_W: i32 = 380;
const FLY_WIFI_H: i32 = 460;
const FLY_BATT_W: i32 = 400;
const FLY_BATT_H: i32 = 432;

const WL_MAXNET: usize = 24;
const WL_SCAN: i32 = 1;
const WL_CONNECT: i32 = 2;
const WL_DISC: i32 = 3;
const WL_ROW: i32 = 48;

// Konstanta WinHTTP (DLL dimuat saat dipakai; tanpa fitur crate WinHttp).
const WH_ACCESS_DEFAULT_PROXY: u32 = 0;
const WH_OPT_REDIRECT_POLICY: u32 = 88;
const WH_REDIRECT_POLICY_NEVER: u32 = 0;
const WH_QUERY_STATUS_CODE_NUMBER: u32 = 19 | 0x2000_0000; // WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER

// ---------------------------------------------------------------- pemuat DLL saat dipakai (lazy.rs, sumber baris 113-126)
static G_WLAN: SyncCell<Cell<HMODULE>> = SyncCell::new(Cell::new(HMODULE(std::ptr::null_mut())));
pub(crate) static G_WINHTTP: SyncCell<Cell<HMODULE>> = SyncCell::new(Cell::new(HMODULE(std::ptr::null_mut())));
static G_POWRPROF: SyncCell<Cell<HMODULE>> = SyncCell::new(Cell::new(HMODULE(std::ptr::null_mut())));

crate::w12_lazy!(G_WLAN, "wlanapi.dll", "WlanOpenHandle", fn wlan_open_handle(ver: u32, reserved: *const c_void, negotiated: *mut u32, handle: *mut HANDLE) -> u32, ERROR_NOT_SUPPORTED.0);
crate::w12_lazy!(G_WLAN, "wlanapi.dll", "WlanCloseHandle", fn wlan_close_handle(handle: HANDLE, reserved: *const c_void) -> u32, ERROR_NOT_SUPPORTED.0);
crate::w12_lazy!(G_WLAN, "wlanapi.dll", "WlanEnumInterfaces", fn wlan_enum_interfaces(handle: HANDLE, reserved: *const c_void, list: *mut *mut WLAN_INTERFACE_INFO_LIST) -> u32, ERROR_NOT_SUPPORTED.0);
crate::w12_lazy!(G_WLAN, "wlanapi.dll", "WlanQueryInterface", fn wlan_query_interface(handle: HANDLE, guid: *const GUID, op: WLAN_INTF_OPCODE, reserved: *const c_void, size: *mut u32, data: *mut *mut c_void, vt: *mut WLAN_OPCODE_VALUE_TYPE) -> u32, ERROR_NOT_SUPPORTED.0);
crate::w12_lazy!(G_WLAN, "wlanapi.dll", "WlanGetNetworkBssList", fn wlan_get_bss_list(handle: HANDLE, guid: *const GUID, ssid: *const DOT11_SSID, bss_type: DOT11_BSS_TYPE, security: BOOL, reserved: *const c_void, list: *mut *mut WLAN_BSS_LIST) -> u32, ERROR_NOT_SUPPORTED.0);
crate::w12_lazy!(G_WLAN, "wlanapi.dll", "WlanGetAvailableNetworkList", fn wlan_get_available(handle: HANDLE, guid: *const GUID, flags: u32, reserved: *const c_void, list: *mut *mut WLAN_AVAILABLE_NETWORK_LIST) -> u32, ERROR_NOT_SUPPORTED.0);
crate::w12_lazy!(G_WLAN, "wlanapi.dll", "WlanRegisterNotification", fn wlan_register_notification(handle: HANDLE, source: u32, ignore_dup: BOOL, cb: WLAN_NOTIFICATION_CALLBACK, ctx: *const c_void, reserved: *const c_void, prev: *mut u32) -> u32, ERROR_NOT_SUPPORTED.0);
crate::w12_lazy!(G_WLAN, "wlanapi.dll", "WlanConnect", fn wlan_connect(handle: HANDLE, guid: *const GUID, params: *const WLAN_CONNECTION_PARAMETERS, reserved: *const c_void) -> u32, ERROR_NOT_SUPPORTED.0);
crate::w12_lazy!(G_WLAN, "wlanapi.dll", "WlanDisconnect", fn wlan_disconnect(handle: HANDLE, guid: *const GUID, reserved: *const c_void) -> u32, ERROR_NOT_SUPPORTED.0);
crate::w12_lazy!(G_WLAN, "wlanapi.dll", "WlanScan", fn wlan_scan(handle: HANDLE, guid: *const GUID, ssid: *const DOT11_SSID, raw: *const c_void, reserved: *const c_void) -> u32, ERROR_NOT_SUPPORTED.0);
crate::w12_lazy_void!(G_WLAN, "wlanapi.dll", "WlanFreeMemory", fn wlan_free_memory(p: *const c_void));

crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpOpen", pub(crate) fn winhttp_open(agent: PCWSTR, access: u32, proxy: PCWSTR, bypass: PCWSTR, flags: u32) -> *mut c_void, std::ptr::null_mut());
crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpSetTimeouts", pub(crate) fn winhttp_set_timeouts(h: *mut c_void, resolve: i32, connect: i32, send: i32, recv: i32) -> i32, 0);
crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpConnect", pub(crate) fn winhttp_connect(h: *mut c_void, server: PCWSTR, port: u16, reserved: u32) -> *mut c_void, std::ptr::null_mut());
crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpOpenRequest", pub(crate) fn winhttp_open_request(h: *mut c_void, verb: PCWSTR, obj: PCWSTR, version: PCWSTR, referrer: PCWSTR, accept: *const PCWSTR, flags: u32) -> *mut c_void, std::ptr::null_mut());
crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpSetOption", pub(crate) fn winhttp_set_option(h: *mut c_void, option: u32, buf: *const c_void, len: u32) -> i32, 0);
crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpSendRequest", pub(crate) fn winhttp_send_request(h: *mut c_void, headers: PCWSTR, hlen: u32, opt: *const c_void, optlen: u32, total: u32, ctx: usize) -> i32, 0);
crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpReceiveResponse", pub(crate) fn winhttp_receive_response(h: *mut c_void, reserved: *mut c_void) -> i32, 0);
crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpQueryHeaders", pub(crate) fn winhttp_query_headers(h: *mut c_void, info: u32, name: PCWSTR, buf: *mut c_void, len: *mut u32, index: *mut u32) -> i32, 0);
crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpReadData", pub(crate) fn winhttp_read_data(h: *mut c_void, buf: *mut c_void, n: u32, read: *mut u32) -> i32, 0);
crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpCloseHandle", pub(crate) fn winhttp_close(h: *mut c_void) -> i32, 0);

// ---------------------------------------------------------------- status Wi-Fi (sumber baris 26-31)
static WL_RES: SyncCell<RefCell<Option<WlRes>>> = SyncCell::new(RefCell::new(None)); // g_wl: hasil terakhir (UI)
static WL_PEND: SyncCell<RefCell<Option<WlReq>>> = SyncCell::new(RefCell::new(None)); // g_wlPend: antre selagi pekerja sibuk
static WL_BUSY: AtomicI32 = AtomicI32::new(0); // g_wlBusy: 1 selagi pekerja berjalan
static WL_RUN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0)); // g_wlRun: perintah yang sedang berjalan
static WL_JOIN: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new())); // g_wlJoin
static WL_FAIL: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new())); // g_wlFail
static WL_SCROLL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0)); // g_wlScroll (px)
static WL_SCROLL_MAX: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0)); // g_wlScrollMax (px)

/// Satu jaringan di daftar (sumber baris 15). `ssid`/`prof` memakai String, bukan buffer wchar tetap.
struct WlNet {
    ssid: String,
    prof: String,
    sig: i32,
    bss: i32,
    sec: bool,
    saved: bool,
}

/// Hasil pindai (sumber baris 16-23). `net` menggantikan array tetap net[WL_MAXNET] + n.
struct WlRes {
    err: WIN32_ERROR, // ERROR_SUCCESS; ERROR_ACCESS_DENIED: izin lokasi ditolak; ERROR_NOT_FOUND: tanpa adaptor
    fin: bool,        // false: hasil cache, pindai baru masih berjalan
    con: bool,
    con_sec: bool,
    need_login: bool,
    con_sig: i32,
    con_mbps: i32,
    con_band: i32, // pita: 24 = 2,4 GHz, 5, 6; 0 tidak diketahui
    con_ssid: String,
    ip: String,
    net: Vec<WlNet>, // tanpa jaringan yang sedang terhubung; urut sinyal menurun
}

impl WlRes {
    fn new(err: WIN32_ERROR, fin: bool) -> Self {
        Self {
            err,
            fin,
            con: false,
            con_sec: false,
            need_login: false,
            con_sig: 0,
            con_mbps: 0,
            con_band: 0,
            con_ssid: String::new(),
            ip: String::new(),
            net: Vec::new(),
        }
    }
}

/// Perintah ke pekerja (sumber baris 24). `prof` kosong bila bukan CONNECT.
struct WlReq {
    cmd: i32,
    bss: i32,
    prof: String,
}

/// Konteks yang dibagi ke callback notifikasi (sumber baris 25). Dibaca lintas thread.
struct WlCtx {
    ev: HANDLE,
    want1: AtomicU32,
    want2: AtomicU32,
}

/// Callback WLAN (sumber baris 33-38): kode ACM yang ditunggu -> bangunkan pekerja.
unsafe extern "system" fn wl_notify(d: *mut L2_NOTIFICATION_DATA, ctx: *mut c_void) {
    if ctx.is_null() || d.is_null() {
        return;
    }
    let cx = unsafe { &*(ctx as *const WlCtx) };
    let (src, code) = unsafe { ((*d).NotificationSource.0, (*d).NotificationCode) };
    if src == WLAN_NOTIFICATION_SOURCE_ACM.0
        && (code == cx.want1.load(Ordering::SeqCst) || code == cx.want2.load(Ordering::SeqCst))
    {
        let _ = unsafe { SetEvent(cx.ev) };
    }
}

/// MultiByteToWideChar dua tahap (ukur, lalu isi). None bila gagal.
fn mb_to_wide(cp: u32, flags: u32, bytes: &[u8]) -> Option<Vec<u16>> {
    let fl = MULTI_BYTE_TO_WIDE_CHAR_FLAGS(flags);
    let need = unsafe { MultiByteToWideChar(cp, fl, bytes, None) };
    if need <= 0 {
        return None;
    }
    let mut out = vec![0u16; need as usize];
    let got = unsafe { MultiByteToWideChar(cp, fl, bytes, Some(&mut out[..])) };
    if got <= 0 {
        return None;
    }
    out.truncate(got as usize);
    Some(out)
}

/// SSID (sumber baris 40-46): UTF-8 ketat (MB_ERR_INVALID_CHARS) dulu, lalu ANSI. Kosong bila keduanya gagal.
fn wl_ssid(s: &DOT11_SSID) -> String {
    let n = (s.uSSIDLength as usize).min(32);
    if n == 0 {
        return String::new();
    }
    let bytes = &s.ucSSID[..n];
    match mb_to_wide(CP_UTF8, MB_ERR_INVALID_CHARS.0, bytes).or_else(|| mb_to_wide(CP_ACP, 0, bytes)) {
        Some(w) => from_wide(&w),
        None => String::new(),
    }
}

/// String C ASCII (PSTR) sama dengan `name` tanpa membedakan huruf (pengganti lstrcmpiA).
unsafe fn pstr_eq_ci(p: *const u8, name: &str) -> bool {
    if p.is_null() {
        return false;
    }
    let c = unsafe { CStr::from_ptr(p.cast()) };
    c.to_bytes().eq_ignore_ascii_case(name.as_bytes())
}

/// IPv4 dari adaptor (sumber baris 48-74). AdapterName = GUID antarmuka "{...}". Kosong bila tidak ada.
fn wl_ip(g: &GUID) -> String {
    let mut gw = [0u16; 40];
    let glen = unsafe { StringFromGUID2(g, &mut gw[..]) };
    if glen <= 0 {
        return String::new();
    }
    // StringFromGUID2 menghitung NUL penutup; nama adaptor berupa ASCII.
    let guid_name = from_wide(&gw[..(glen as usize).saturating_sub(1)]);
    let mut len: u32 = 16384;
    for _ in 0..3 {
        // Vec<u64> agar alignment memadai untuk IP_ADAPTER_ADDRESSES_LH.
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        let aa = buf.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let flags = GET_ADAPTERS_ADDRESSES_FLAGS(
            GAA_FLAG_SKIP_ANYCAST.0 | GAA_FLAG_SKIP_MULTICAST.0 | GAA_FLAG_SKIP_DNS_SERVER.0,
        );
        let r = unsafe { GetAdaptersAddresses(AF_INET.0 as u32, flags, None, Some(aa), &mut len) };
        if WIN32_ERROR(r) == ERROR_SUCCESS {
            let mut cur = aa;
            while !cur.is_null() {
                let a = unsafe { &*cur };
                if unsafe { pstr_eq_ci(a.AdapterName.0, &guid_name) } {
                    let mut u = a.FirstUnicastAddress;
                    while !u.is_null() {
                        let ua = unsafe { &*u };
                        let sa = ua.Address.lpSockaddr;
                        if !sa.is_null() && unsafe { (*sa).sa_family.0 } == AF_INET.0 {
                            let sin = sa.cast::<SOCKADDR_IN>();
                            let b = unsafe {
                                std::ptr::addr_of!((*sin).sin_addr).cast::<[u8; 4]>().read_unaligned()
                            };
                            return format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3]);
                        }
                        u = ua.Next;
                    }
                    return String::new();
                }
                cur = a.Next;
            }
            return String::new();
        }
        if WIN32_ERROR(r) != ERROR_BUFFER_OVERFLOW {
            return String::new();
        }
    }
    String::new()
}

/// Mencari `needle` di dalam `hay` (pengganti strstr untuk buffer byte).
fn bytes_has(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Teks dari buffer UTF-16 yang dipotong di NUL pertama (pengganti lstrcpynW).
fn wide_z(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    from_wide(&w[..end])
}

/// Salinan nilai DOT11_SSID (tidak diasumsikan Copy).
fn ssid_copy(s: &DOT11_SSID) -> DOT11_SSID {
    DOT11_SSID {
        uSSIDLength: s.uSSIDLength,
        ucSSID: s.ucSSID,
    }
}

/// Memeriksa portal tawanan lewat WinHTTP (sumber WlCheckCaptivePortal, baris 76-111).
fn wl_captive_portal() -> bool {
    let agent = to_wide_z("Win12Shell/1.0");
    let host = to_wide_z("www.msftconnecttest.com");
    let verb = to_wide_z("GET");
    let path = to_wide_z("/connecttest.txt");
    let mut need = false;
    unsafe {
        let session = winhttp_open(pcw(&agent), WH_ACCESS_DEFAULT_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
        if session.is_null() {
            return false;
        }
        winhttp_set_timeouts(session, 800, 800, 1000, 1000);
        let conn = winhttp_connect(session, pcw(&host), 80, 0);
        if !conn.is_null() {
            let req = winhttp_open_request(
                conn,
                pcw(&verb),
                pcw(&path),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                0,
            );
            if !req.is_null() {
                let opt: u32 = WH_REDIRECT_POLICY_NEVER;
                winhttp_set_option(req, WH_OPT_REDIRECT_POLICY, (&opt as *const u32).cast(), 4);
                if winhttp_send_request(req, PCWSTR::null(), 0, std::ptr::null(), 0, 0, 0) != 0
                    && winhttp_receive_response(req, std::ptr::null_mut()) != 0
                {
                    let mut status: u32 = 0;
                    let mut sz: u32 = 4;
                    if winhttp_query_headers(
                        req,
                        WH_QUERY_STATUS_CODE_NUMBER,
                        PCWSTR::null(),
                        (&mut status as *mut u32).cast(),
                        &mut sz,
                        std::ptr::null_mut(),
                    ) != 0
                    {
                        if matches!(status, 302 | 307 | 301 | 401 | 403) {
                            need = true;
                        } else if status == 200 {
                            let mut buf = [0u8; 64];
                            let mut rd: u32 = 0;
                            if winhttp_read_data(req, buf.as_mut_ptr().cast(), 63, &mut rd) != 0 && rd > 0 {
                                let n = (rd as usize).min(63);
                                // strstr: berhenti di NUL pertama
                                let end = buf[..n].iter().position(|&b| b == 0).unwrap_or(n);
                                if !bytes_has(&buf[..end], b"Microsoft Connect Test") {
                                    need = true;
                                }
                            }
                        }
                    }
                }
                winhttp_close(req);
            }
            winhttp_close(conn);
        }
        winhttp_close(session);
    }
    need
}

/// SSID Wi-Fi yang sedang tersambung (peringatan notch, net_watch.rs). None bila tidak tersambung atau WLAN tidak ada.
pub(crate) fn wifi_current_ssid() -> Option<String> {
    let mut neg = 0u32;
    let mut h = HANDLE(std::ptr::null_mut());
    // SAFETY: semua penunjuk menunjuk variabel lokal; handle ditutup di akhir fungsi.
    if unsafe { wlan_open_handle(2, std::ptr::null(), &mut neg, &mut h) } != 0 || h.0.is_null() {
        return None;
    }
    let mut out = None;
    let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    // SAFETY: list diisi WlanEnumInterfaces dan dibebaskan dengan WlanFreeMemory.
    unsafe {
        if wlan_enum_interfaces(h, std::ptr::null(), &mut list) == 0 && !list.is_null() {
            let n = (*list).dwNumberOfItems as usize;
            let base = (*list).InterfaceInfo.as_ptr();
            for i in 0..n {
                let info = &*base.add(i);
                let mut sz = 0u32;
                let mut vt = WLAN_OPCODE_VALUE_TYPE(0);
                let mut data: *mut c_void = std::ptr::null_mut();
                let rc = wlan_query_interface(
                    h,
                    &info.InterfaceGuid,
                    wlan_intf_opcode_current_connection,
                    std::ptr::null(),
                    &mut sz,
                    &mut data,
                    &mut vt,
                );
                if rc == 0 && !data.is_null() {
                    let ca = data.cast::<WLAN_CONNECTION_ATTRIBUTES>();
                    if (*ca).isState.0 == wlan_interface_state_connected.0 {
                        let cs = ssid_copy(&(*ca).wlanAssociationAttributes.dot11Ssid);
                        out = Some(wl_ssid(&cs));
                    }
                    wlan_free_memory(data);
                }
                if out.is_some() {
                    break;
                }
            }
            wlan_free_memory(list as *const c_void);
        }
        let _ = wlan_close_handle(h, std::ptr::null());
    }
    out.filter(|s: &String| !s.is_empty())
}

/// Membaca koneksi aktif dan daftar jaringan ke `r` (sumber WlRead, baris 128-194).
fn wl_read(h: HANDLE, g: &GUID, r: &mut WlRes) {
    let mut cs = DOT11_SSID {
        uSSIDLength: 0,
        ucSSID: [0u8; 32],
    };
    let mut sz: u32 = 0;
    let mut vt = WLAN_OPCODE_VALUE_TYPE(0);
    let mut data: *mut c_void = std::ptr::null_mut();
    let rc = unsafe {
        wlan_query_interface(
            h,
            g,
            wlan_intf_opcode_current_connection,
            std::ptr::null(),
            &mut sz,
            &mut data,
            &mut vt,
        )
    };
    if WIN32_ERROR(rc) == ERROR_SUCCESS && !data.is_null() {
        let ca = data.cast::<WLAN_CONNECTION_ATTRIBUTES>();
        unsafe {
            if (*ca).isState.0 == wlan_interface_state_connected.0 {
                let assoc = &(*ca).wlanAssociationAttributes;
                cs = ssid_copy(&assoc.dot11Ssid);
                r.con = true;
                r.con_ssid = wl_ssid(&cs);
                r.con_sig = assoc.wlanSignalQuality as i32;
                r.con_mbps = (assoc.ulRxRate / 1000) as i32; // kbps -> Mbps
                r.con_sec = (*ca).wlanSecurityAttributes.bSecurityEnabled.0 != 0;
                r.need_login = !r.con_sec || wl_captive_portal();

                // pita dari frekuensi titik akses yang tersambung
                let mut bl: *mut WLAN_BSS_LIST = std::ptr::null_mut();
                let e = wlan_get_bss_list(
                    h,
                    g,
                    &cs,
                    DOT11_BSS_TYPE(assoc.dot11BssType.0),
                    BOOL(i32::from(r.con_sec)),
                    std::ptr::null(),
                    &mut bl,
                );
                if WIN32_ERROR(e) == ERROR_SUCCESS && !bl.is_null() {
                    let n = (*bl).dwNumberOfItems as usize;
                    let entries = std::slice::from_raw_parts(
                        std::ptr::addr_of!((*bl).wlanBssEntries).cast::<WLAN_BSS_ENTRY>(),
                        n,
                    );
                    if let Some(en) = entries.iter().find(|en| en.dot11Bssid == assoc.dot11Bssid) {
                        let f = en.ulChCenterFrequency; // kHz
                        r.con_band = if f < 3_000_000 {
                            24
                        } else if f < 5_925_000 {
                            5
                        } else {
                            6
                        };
                    }
                    wlan_free_memory(bl as *const c_void);
                }
                r.ip = wl_ip(g);
            }
            wlan_free_memory(ca as *const c_void);
        }
    }

    let mut al: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
    let e = WIN32_ERROR(unsafe { wlan_get_available(h, g, 0, std::ptr::null(), &mut al) });
    if e != ERROR_SUCCESS || al.is_null() {
        r.err = if e != ERROR_SUCCESS { e } else { ERROR_GEN_FAILURE };
        return;
    }
    unsafe {
        let n = (*al).dwNumberOfItems as usize;
        let items = std::slice::from_raw_parts(
            std::ptr::addr_of!((*al).Network).cast::<WLAN_AVAILABLE_NETWORK>(),
            n,
        );
        for a in items {
            if a.dot11Ssid.uSSIDLength == 0 {
                continue; // jaringan tersembunyi
            }
            if (a.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED) != 0 {
                continue; // sudah tampil di kartu koneksi
            }
            let len = (cs.uSSIDLength as usize).min(32);
            if r.con
                && a.dot11Ssid.uSSIDLength == cs.uSSIDLength
                && a.dot11Ssid.ucSSID[..len] == cs.ucSSID[..len]
            {
                continue;
            }
            let s = wl_ssid(&a.dot11Ssid);
            // entri per profil dan per SSID digabung jadi satu baris
            let pos = r.net.iter().position(|x| x.ssid == s);
            let j = match pos {
                Some(j) => j,
                None => {
                    if r.net.len() >= WL_MAXNET {
                        continue;
                    }
                    r.net.push(WlNet {
                        ssid: s,
                        prof: String::new(),
                        sig: -1,
                        bss: a.dot11BssType.0,
                        sec: false,
                        saved: false,
                    });
                    r.net.len() - 1
                }
            };
            let x = &mut r.net[j];
            let q = a.wlanSignalQuality as i32;
            if q > x.sig {
                x.sig = q;
            }
            if a.bSecurityEnabled.0 != 0 {
                x.sec = true;
            }
            if (a.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE) != 0 && a.strProfileName[0] != 0 && !x.saved {
                x.saved = true;
                x.prof = wide_z(&a.strProfileName);
                x.bss = a.dot11BssType.0;
            }
        }
        wlan_free_memory(al as *const c_void);
    }
    // urut sinyal menurun (sort stabil)
    r.net.sort_by(|a, b| b.sig.cmp(&a.sig));
}

/// Mengosongkan sinyal ACM pada event (pengganti ResetEvent yang dijaga null).
fn reset_ev(ev: HANDLE) {
    if !ev.0.is_null() {
        let _ = unsafe { ResetEvent(ev) };
    }
}

/// Mengirim hasil pekerja ke UI lewat WM_W12_WLAN (sumber WlPost, baris 196-207).
/// Bila PostMessageW berhasil, kepemilikan `WlRes` pindah ke UI dan kunci busy dilepas di sana.
fn wl_post(h: HANDLE, g: Option<&GUID>, err: WIN32_ERROR, fin: bool) {
    let mut r = Box::new(WlRes::new(err, fin));
    if err == ERROR_SUCCESS {
        if let Some(g) = g {
            wl_read(h, g, &mut r);
        }
    }
    let p = Box::into_raw(r);
    let posted = unsafe { PostMessageW(Some(MSG.get()), WM_W12_WLAN, WPARAM(0), LPARAM(p as isize)) };
    if posted.is_ok() {
        return;
    }
    drop(unsafe { Box::from_raw(p) });
    if fin {
        WL_BUSY.store(0, Ordering::SeqCst); // UI tidak akan menerima hasil akhir: lepas kunci di sini
    }
}

/// Badan pekerja WLAN (sumber WlThread, baris 209-255). Dijalankan lewat std::thread::spawn.
fn wl_thread(q: Box<WlReq>) {
    let ev = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.unwrap_or(HANDLE(std::ptr::null_mut()));
    let cx = WlCtx {
        ev,
        want1: AtomicU32::new(0xFFFF),
        want2: AtomicU32::new(0xFFFF),
    };
    let mut h = HANDLE(std::ptr::null_mut());
    let mut ver: u32 = 0;
    let mut err = WIN32_ERROR(unsafe { wlan_open_handle(2, std::ptr::null(), &mut ver, &mut h) });
    let mut g = GUID::zeroed();
    let mut have = false;
    if err == ERROR_SUCCESS {
        let mut il: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        let e = unsafe { wlan_enum_interfaces(h, std::ptr::null(), &mut il) };
        if WIN32_ERROR(e) == ERROR_SUCCESS && !il.is_null() {
            unsafe {
                let n = (*il).dwNumberOfItems as usize;
                let items = std::slice::from_raw_parts(
                    std::ptr::addr_of!((*il).InterfaceInfo).cast::<WLAN_INTERFACE_INFO>(),
                    n,
                );
                // utamakan adaptor yang sedang terhubung
                for it in items {
                    if !have || it.isState.0 == wlan_interface_state_connected.0 {
                        g = it.InterfaceGuid;
                        have = true;
                    }
                }
                wlan_free_memory(il as *const c_void);
            }
        }
        if !have {
            err = ERROR_NOT_FOUND;
        }
    }
    if have && !cx.ev.0.is_null() {
        unsafe {
            wlan_register_notification(
                h,
                WLAN_NOTIFICATION_SOURCE_ACM.0,
                BOOL(1),
                Some(wl_notify),
                (&cx as *const WlCtx).cast(),
                std::ptr::null(),
                std::ptr::null_mut(),
            );
        }
    }

    let cmd = q.cmd;
    if have && cmd == WL_CONNECT {
        // profil tersimpan: Windows memakai sandi yang sudah ada
        let prof = to_wide_z(&q.prof);
        let cp = WLAN_CONNECTION_PARAMETERS {
            wlanConnectionMode: wlan_connection_mode_profile,
            strProfile: pcw(&prof),
            pDot11Ssid: std::ptr::null_mut(),
            pDesiredBssidList: std::ptr::null_mut(),
            dot11BssType: DOT11_BSS_TYPE(q.bss),
            dwFlags: 0,
        };
        cx.want1
            .store(wlan_notification_acm_connection_complete.0 as u32, Ordering::SeqCst);
        cx.want2
            .store(wlan_notification_acm_connection_attempt_fail.0 as u32, Ordering::SeqCst);
        reset_ev(cx.ev);
        let rc = unsafe { wlan_connect(h, &g, &cp, std::ptr::null()) };
        if WIN32_ERROR(rc) == ERROR_SUCCESS && !cx.ev.0.is_null() {
            let _ = unsafe { WaitForSingleObject(cx.ev, 12000) };
        }
    } else if have && cmd == WL_DISC {
        let code = wlan_notification_acm_disconnected.0 as u32;
        cx.want1.store(code, Ordering::SeqCst);
        cx.want2.store(code, Ordering::SeqCst);
        reset_ev(cx.ev);
        let rc = unsafe { wlan_disconnect(h, &g, std::ptr::null()) };
        if WIN32_ERROR(rc) == ERROR_SUCCESS && !cx.ev.0.is_null() {
            let _ = unsafe { WaitForSingleObject(cx.ev, 4000) };
        }
    } else if have {
        // pindai: tampilkan cache dulu, lalu hasil pindai baru (maks +-5 detik)
        wl_post(h, Some(&g), ERROR_SUCCESS, false);
        cx.want1
            .store(wlan_notification_acm_scan_complete.0 as u32, Ordering::SeqCst);
        cx.want2
            .store(wlan_notification_acm_scan_fail.0 as u32, Ordering::SeqCst);
        reset_ev(cx.ev);
        let rc = unsafe { wlan_scan(h, &g, std::ptr::null(), std::ptr::null(), std::ptr::null()) };
        if WIN32_ERROR(rc) == ERROR_SUCCESS && !cx.ev.0.is_null() {
            let _ = unsafe { WaitForSingleObject(cx.ev, 5000) };
        }
    }

    // menunggu callback yang sedang berjalan; sesudah WlanCloseHandle event aman ditutup
    wl_post(
        h,
        if have { Some(&g) } else { None },
        if have { ERROR_SUCCESS } else { err },
        true,
    );
    if !h.0.is_null() {
        unsafe { wlan_close_handle(h, std::ptr::null()) };
    }
    if !cx.ev.0.is_null() {
        let _ = unsafe { CloseHandle(cx.ev) };
    }
}

/// Menjalankan pekerja untuk `q` (sumber WlRun, baris 257-265). Kunci busy sudah dipegang pemanggil.
/// Deviasi: std::thread::spawn sebagai pengganti CreateThread.
fn wl_run(q: Box<WlReq>) {
    WL_RUN.set(q.cmd);
    let spawned = std::thread::Builder::new()
        .name("w12-wlan".into())
        .spawn(move || wl_thread(q));
    if spawned.is_ok() {
        return;
    }
    WL_RUN.set(0);
    WL_BUSY.store(0, Ordering::SeqCst);
}

/// Memulai perintah WLAN (sumber WlStart, baris 267-276). `n` = jaringan yang dipilih (bila ada).
fn wl_start(cmd: i32, n: Option<&WlNet>) {
    if WL_BUSY.load(Ordering::SeqCst) != 0 && cmd == WL_SCAN {
        return; // pekerja yang berjalan akan mengirim daftar terbaru juga
    }
    let mut q = WlReq {
        cmd,
        bss: 0,
        prof: String::new(),
    };
    if let Some(n) = n {
        q.prof = n.prof.clone();
        q.bss = n.bss;
    }
    if WL_BUSY
        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        *WL_PEND.borrow_mut() = Some(q); // antre; menggantikan antrean sebelumnya
        return;
    }
    wl_run(Box::new(q));
}

/// Menerima hasil pekerja dari WM_W12_WLAN (sumber WifiWlanNotify, baris 278-298).
/// `lp` = pointer `WlRes` dari Box::into_raw di wl_post; kepemilikannya diambil di sini.
pub fn wifi_wlan_notify(lp: isize) {
    if lp == 0 {
        return;
    }
    let r: WlRes = *unsafe { Box::from_raw(lp as *mut WlRes) };
    let fin = r.fin;
    let r_con = r.con;
    let r_ssid = r.con_ssid.clone();
    let _old = WL_RES.replace(Some(r));
    if fin {
        let was = WL_RUN.get();
        WL_RUN.set(0);
        let join = WL_JOIN.borrow().clone();
        if was == WL_CONNECT && !join.is_empty() && (!r_con || r_ssid != join) {
            *WL_FAIL.borrow_mut() = join.chars().take(32).collect::<String>(); // lstrcpynW(.., 33)
        }
        let pend_connect = WL_PEND
            .borrow()
            .as_ref()
            .is_some_and(|q| q.cmd == WL_CONNECT);
        if !pend_connect {
            WL_JOIN.borrow_mut().clear();
        }
        WL_BUSY.store(0, Ordering::SeqCst);
        let pend = WL_PEND.borrow_mut().take();
        if let Some(q) = pend {
            WL_BUSY.store(1, Ordering::SeqCst);
            wl_run(Box::new(q));
        }
    }
    if PANEL_OPEN.get() == PN_WIFI {
        let pan = PAN.get();
        if !pan.0.is_null() {
            w12_invalidate(pan, None, false);
        }
    }
}

/// Status radio Wi-Fi: 1 nyala, 0 mati, -1 belum terbaca. Perintah yang berjalan ditampilkan optimistis
/// (sumber WifiRadio, baris 300-303).
pub fn wifi_radio() -> i32 {
    if RAD_PEND_KIND.get() == 1 {
        RAD_PEND_ON.get()
    } else {
        RAD_WIFI.get()
    }
}

/// Dipanggil saat status radio berubah dari QsRadioNotify (sumber WifiRadioChanged, baris 305-309).
/// `was` = status sebelumnya.
pub fn wifi_radio_changed(was: i32) {
    if RAD_WIFI.get() == 0 {
        if let Some(r) = WL_RES.borrow_mut().as_mut() {
            r.con = false;
            r.net.clear(); // radio mati: daftar lama tidak berlaku
        }
    }
    if RAD_WIFI.get() == 1 && was == 0 && PANEL_OPEN.get() == PN_WIFI {
        wl_start(WL_SCAN, None);
    }
}

// ---------------------------------------------------------------- baterai: data (sumber C 529-639)
// Riwayat 24 jam: satu slot per jam UTC (indeks = jam sejak 1601 mod 24), 144 byte little-endian.
// Deviasi: file lewat std::fs (share mode default), bukan CreateFileW dengan share 0.

#[derive(Clone, Copy)]
struct BatHist {
    key: [u32; 24],
    pct: [u8; 24],
    chg: [u8; 24],
}

const BAT_HIST_EMPTY: BatHist = BatHist {
    key: [0; 24],
    pct: [0; 24],
    chg: [0; 24],
};
static BAT_HIST: SyncCell<RefCell<BatHist>> = SyncCell::new(RefCell::new(BAT_HIST_EMPTY));
static BAT_HIST_LOADED: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

fn bat_hist_path() -> std::path::PathBuf {
    std::path::PathBuf::from(DIR.borrow().as_str()).join("battery-history.dat")
}

fn bat_hist_decode(b: &[u8]) -> BatHist {
    let mut h = BAT_HIST_EMPTY;
    for (i, k) in h.key.iter_mut().enumerate() {
        *k = u32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
    }
    h.pct.copy_from_slice(&b[96..120]);
    h.chg.copy_from_slice(&b[120..144]);
    h
}

fn bat_hist_encode(h: &BatHist) -> [u8; 144] {
    let mut out = [0u8; 144];
    for (i, k) in h.key.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&k.to_le_bytes());
    }
    out[96..120].copy_from_slice(&h.pct);
    out[120..144].copy_from_slice(&h.chg);
    out
}

/// Jam UTC sejak 1601: berurutan dan tidak terpengaruh perubahan zona waktu (sumber BatHourKey).
fn bat_hour_key() -> u32 {
    let ft = unsafe { GetSystemTimeAsFileTime() };
    let t =((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    (t / 36_000_000_000u64) as u32
}

/// Mencatat status baterai ke riwayat per jam; file ditulis sekali per jam (sumber BatHistNote, baris 549-575).
pub fn bat_hist_note(pct: i32, charging: bool) {
    let path = bat_hist_path();
    if !BAT_HIST_LOADED.get() {
        BAT_HIST_LOADED.set(true);
        if let Ok(b) = std::fs::read(&path) {
            if b.len() == 144 {
                *BAT_HIST.borrow_mut() = bat_hist_decode(&b);
            }
        }
    }
    let k = bat_hour_key();
    let s = (k % 24) as usize;
    let snapshot = {
        let mut h = BAT_HIST.borrow_mut();
        let fresh = h.key[s] != k;
        if fresh {
            h.key[s] = k;
            h.chg[s] = 0;
        }
        h.pct[s] = pct.min(100) as u8;
        if charging {
            h.chg[s] = 1;
        }
        if fresh {
            Some(bat_hist_encode(&h))
        } else {
            None
        }
    };
    if let Some(bytes) = snapshot {
        let _ = std::fs::write(&path, bytes);
    }
}

// Fungsi powrprof dimuat dinamis (tanpa pustaka tambahan), seperti C.
type FnGetActive = unsafe extern "system" fn(*mut c_void, *mut *mut GUID) -> u32;
type FnReadDc = unsafe extern "system" fn(*mut c_void, *const GUID, *const GUID, *const GUID, *mut u32) -> u32;
type FnNtInfo = unsafe extern "system" fn(i32, *mut c_void, u32, *mut c_void, u32) -> i32;
type FnOvGet = unsafe extern "system" fn(*mut GUID) -> u32;
type FnOvSet = unsafe extern "system" fn(GUID) -> u32;

#[derive(Clone, Copy)]
struct BattPow {
    loaded: bool,
    get_active: Option<FnGetActive>,
    read_dc: Option<FnReadDc>,
    nt_info: Option<FnNtInfo>,
    ov_get: Option<FnOvGet>,
    ov_set: Option<FnOvSet>,
}

const BATT_POW_INIT: BattPow = BattPow {
    loaded: false,
    get_active: None,
    read_dc: None,
    nt_info: None,
    ov_get: None,
    ov_set: None,
};
static BATT_POW: SyncCell<Cell<BattPow>> = SyncCell::new(Cell::new(BATT_POW_INIT));

const PWR_OV_EFF: GUID = GUID {
    data1: 0x961cc777,
    data2: 0x2547,
    data3: 0x4f9d,
    data4: [0x81, 0x74, 0x7d, 0x86, 0x18, 0x1b, 0x8a, 0x7a],
};
const PWR_OV_PERF: GUID = GUID {
    data1: 0xded574b5,
    data2: 0x45a0,
    data3: 0x4f42,
    data4: [0x87, 0x37, 0x46, 0x34, 0x5c, 0x09, 0xc2, 0x38],
};
const PWR_OV_BAL: GUID = GUID {
    data1: 0,
    data2: 0,
    data3: 0,
    data4: [0; 8],
};
const PWR_SCH_BALANCED: GUID = GUID {
    data1: 0x381b4222,
    data2: 0xf694,
    data3: 0x41f0,
    data4: [0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e],
};
const PWR_SUB_ES: GUID = GUID {
    data1: 0xde830923,
    data2: 0xa562,
    data3: 0x41af,
    data4: [0xa0, 0x86, 0xe3, 0xa2, 0xc6, 0xba, 0xd2, 0xda],
};
const PWR_ES_THRESH: GUID = GUID {
    data1: 0xe69653ca,
    data2: 0xcf7f,
    data3: 0x4f05,
    data4: [0xaa, 0x73, 0xcb, 0x83, 0x3f, 0xa9, 0x0a, 0xd4],
};

/// Mode daya aktif: 0 Efisiensi, 1 Seimbang, 2 Performa, -1 tidak diketahui (sumber g_bmMode).
static BM_MODE: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
/// Paket daya bukan Seimbang bawaan: tombol mode hanya tampilan (sumber g_bmCustom).
static BM_CUSTOM: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
/// Ambang penghemat otomatis dalam persen, -1 tidak diketahui (sumber g_bsThresh).
static BS_THRESH: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));

fn batt_pow_load() {
    let mut bp = BATT_POW.get();
    if bp.loaded {
        return;
    }
    bp.loaded = true;
    BATT_POW.set(bp);
    let Ok(m) = (unsafe { LoadLibraryW(w!("powrprof.dll")) }) else {
        return;
    };
    G_POWRPROF.set(m);
    unsafe {
        bp.get_active = GetProcAddress(m, PCSTR::from_raw(b"PowerGetActiveScheme\0".as_ptr()))
            .map(|f| std::mem::transmute::<_, FnGetActive>(f));
        bp.read_dc = GetProcAddress(m, PCSTR::from_raw(b"PowerReadDCValueIndex\0".as_ptr()))
            .map(|f| std::mem::transmute::<_, FnReadDc>(f));
        bp.nt_info = GetProcAddress(m, PCSTR::from_raw(b"CallNtPowerInformation\0".as_ptr()))
            .map(|f| std::mem::transmute::<_, FnNtInfo>(f));
        bp.ov_get = GetProcAddress(m, PCSTR::from_raw(b"PowerGetEffectiveOverlayScheme\0".as_ptr()))
            .map(|f| std::mem::transmute::<_, FnOvGet>(f));
        bp.ov_set = GetProcAddress(m, PCSTR::from_raw(b"PowerSetActiveOverlayScheme\0".as_ptr()))
            .map(|f| std::mem::transmute::<_, FnOvSet>(f));
    }
    BATT_POW.set(bp);
}

/// Membaca paket daya aktif, mode overlay, dan ambang penghemat (sumber BattReadModes, baris 610-626).
/// Dipanggil saat flyout baterai dibuka dan setelah mode diganti.
fn batt_read_modes() {
    batt_pow_load();
    let bp = BATT_POW.get();
    BM_MODE.set(-1);
    BM_CUSTOM.set(true);
    BS_THRESH.set(-1);
    let mut sch: *mut GUID = std::ptr::null_mut();
    if let Some(get_active) = bp.get_active {
        let rc = unsafe { get_active(std::ptr::null_mut(), &mut sch) };
        if WIN32_ERROR(rc) == ERROR_SUCCESS && !sch.is_null() {
            let guid = unsafe { *sch };
            BM_CUSTOM.set(guid != PWR_SCH_BALANCED);
            if let Some(read_dc) = bp.read_dc {
                let mut v: u32 = 0;
                let rc2 = unsafe { read_dc(std::ptr::null_mut(), sch, &PWR_SUB_ES, &PWR_ES_THRESH, &mut v) };
                if WIN32_ERROR(rc2) == ERROR_SUCCESS {
                    BS_THRESH.set(v as i32);
                }
            }
            // Verifikasi saat cargo check: signature LocalFree (HLOCAL) pada windows 0.61.
            let _ = unsafe { LocalFree(Some(HLOCAL(sch.cast()))) };
        }
    }
    if let Some(ov_get) = bp.ov_get {
        let mut ov = GUID::zeroed();
        if WIN32_ERROR(unsafe { ov_get(&mut ov) }) == ERROR_SUCCESS {
            let mode = if ov == PWR_OV_EFF {
                0
            } else if ov == PWR_OV_PERF {
                2
            } else {
                1
            };
            BM_MODE.set(mode);
        }
    }
}

/// SYSTEM_BATTERY_STATE (layout C: 4 BOOLEAN + tag, lalu 6 DWORD).
#[allow(dead_code)]
#[repr(C)]
#[derive(Default)]
struct SysBatState {
    ac: u8,
    present: u8,
    charging: u8,
    discharging: u8,
    spare: [u8; 3],
    tag: u8,
    max_cap: u32,
    rem_cap: u32,
    rate: u32,
    est: u32,
    a1: u32,
    a2: u32,
}

/// Perkiraan dalam menit: waktu sampai penuh (to_full) atau sisa pemakaian. 0 = tidak diketahui
/// (sumber BattEstimate, baris 629-638).
fn batt_estimate(to_full: &mut bool) -> i32 {
    *to_full = false;
    let Some(nt) = BATT_POW.get().nt_info else {
        return 0;
    };
    let mut s = SysBatState::default();
    let rc = unsafe {
        nt(
            5, // SystemBatteryState
            std::ptr::null_mut(),
            0,
            (&mut s as *mut SysBatState).cast(),
            std::mem::size_of::<SysBatState>() as u32,
        )
    };
    if rc != 0 {
        return 0;
    }
    let rate = s.rate as i32;
    if s.charging != 0 && rate > 0 && s.max_cap > s.rem_cap {
        *to_full = true;
        return ((s.max_cap - s.rem_cap) as u64 * 60 / rate as u64) as i32;
    }
    if s.discharging != 0 && s.est != 0xFFFF_FFFF && s.est > 0 {
        return (s.est / 60) as i32;
    }
    0
}

// ---------------------------------------------------------------- baterai: ikon vektor (sumber C 640-713)
// Garis 1,5 dip. Titik dihitung dari pusat (cx, cy) dengan skala s (setengah ukuran kotak ikon).

/// Titik ikon: pusat + (x, y) * s (sumber BtP).
fn bt_p(cx: f32, cy: f32, s: f32, x: f32, y: f32) -> Vector2 {
    Vector2 {
        X: cx + x * s,
        Y: cy + y * s,
    }
}

fn bt_bez(k: &ID2D1GeometrySink, cx: f32, cy: f32, s: f32, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
    let b = D2D1_BEZIER_SEGMENT {
        point1: bt_p(cx, cy, s, x1, y1),
        point2: bt_p(cx, cy, s, x2, y2),
        point3: bt_p(cx, cy, s, x3, y3),
    };
    unsafe { k.AddBezier(&b) };
}

fn bt_line(k: &ID2D1GeometrySink, cx: f32, cy: f32, s: f32, x0: f32, y0: f32, x1: f32, y1: f32) {
    unsafe {
        k.BeginFigure(bt_p(cx, cy, s, x0, y0), D2D1_FIGURE_BEGIN_HOLLOW);
        k.AddLine(bt_p(cx, cy, s, x1, y1));
        k.EndFigure(D2D1_FIGURE_END_OPEN);
    }
}

/// Geometri jalur baru beserta sinknya (sumber BtPath, baris 641-648).
fn bt_path() -> Option<(ID2D1PathGeometry, ID2D1GeometrySink)> {
    let d = D2D.borrow();
    let f = d.as_ref()?;
    let g = unsafe { f.CreatePathGeometry() }.ok()?;
    let sk = unsafe { g.Open() }.ok()?;
    Some((g, sk))
}

/// Ikon vektor baterai (sumber BtIcon, baris 666-713): 0 daun, 1 timbangan, 2 roket.
fn bt_icon(kind: i32, cx: f32, cy: f32, s: f32, c: D2D1_COLOR_F) {
    let Some((g, k)) = bt_path() else {
        return;
    };
    if kind == 0 {
        bt_line(&k, cx, cy, s, -0.62, 0.62, -0.30, 0.30); // tangkai
        unsafe { k.BeginFigure(bt_p(cx, cy, s, -0.30, 0.30), D2D1_FIGURE_BEGIN_HOLLOW) }; // helai daun
        bt_bez(&k, cx, cy, s, -0.48, -0.20, -0.02, -0.62, 0.62, -0.62);
        bt_bez(&k, cx, cy, s, 0.62, 0.02, 0.20, 0.48, -0.30, 0.30);
        unsafe { k.EndFigure(D2D1_FIGURE_END_CLOSED) };
        bt_line(&k, cx, cy, s, -0.30, 0.30, 0.26, -0.26); // tulang daun
    } else if kind == 1 {
        bt_line(&k, cx, cy, s, 0.0, -0.62, 0.0, 0.60); // tiang
        bt_line(&k, cx, cy, s, -0.28, 0.60, 0.28, 0.60); // alas
        bt_line(&k, cx, cy, s, -0.48, -0.40, 0.48, -0.40); // lengan
        for i in 0..2 {
            // piringan: dua tali dan mangkuk
            let x: f32 = if i == 1 { 0.48 } else { -0.48 };
            unsafe {
                k.BeginFigure(bt_p(cx, cy, s, x - 0.20, 0.06), D2D1_FIGURE_BEGIN_HOLLOW);
                k.AddLine(bt_p(cx, cy, s, x, -0.40));
                k.AddLine(bt_p(cx, cy, s, x + 0.20, 0.06));
            }
            bt_bez(&k, cx, cy, s, x + 0.14, 0.26, x - 0.14, 0.26, x - 0.20, 0.06);
            unsafe { k.EndFigure(D2D1_FIGURE_END_CLOSED) };
        }
    } else {
        // badan roket miring
        unsafe { k.BeginFigure(bt_p(cx, cy, s, 0.62, -0.62), D2D1_FIGURE_BEGIN_HOLLOW) };
        bt_bez(&k, cx, cy, s, 0.20, -0.58, -0.16, -0.24, -0.26, 0.08);
        unsafe { k.AddLine(bt_p(cx, cy, s, -0.08, 0.26)) };
        bt_bez(&k, cx, cy, s, 0.24, 0.16, 0.58, -0.20, 0.62, -0.62);
        unsafe { k.EndFigure(D2D1_FIGURE_END_CLOSED) };
        unsafe {
            // sirip kiri
            k.BeginFigure(bt_p(cx, cy, s, -0.18, -0.06), D2D1_FIGURE_BEGIN_HOLLOW);
            k.AddLine(bt_p(cx, cy, s, -0.46, -0.04));
            k.AddLine(bt_p(cx, cy, s, -0.26, 0.08));
            k.EndFigure(D2D1_FIGURE_END_OPEN);
            // sirip kanan
            k.BeginFigure(bt_p(cx, cy, s, 0.06, 0.18), D2D1_FIGURE_BEGIN_HOLLOW);
            k.AddLine(bt_p(cx, cy, s, 0.04, 0.46));
            k.AddLine(bt_p(cx, cy, s, -0.08, 0.26));
            k.EndFigure(D2D1_FIGURE_END_OPEN);
        }
        bt_line(&k, cx, cy, s, -0.30, 0.30, -0.56, 0.56); // nyala
    }
    unsafe {
        let _ = k.Close();
    }
    set_color(c, 1.0);
    let rt_g = RT.borrow();
    let br_g = BR.borrow();
    if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
        let geo: &ID2D1Geometry = &g;
        let brush: &ID2D1Brush = br;
        unsafe {
            rt.DrawGeometry(geo, brush, sf(1.5), None::<&ID2D1StrokeStyle>);
            if kind == 2 {
                // jendela roket
                let e = D2D1_ELLIPSE {
                    point: bt_p(cx, cy, s, 0.22, -0.22),
                    radiusX: s * 0.11,
                    radiusY: s * 0.11,
                };
                rt.DrawEllipse(&e, brush, sf(1.3), None::<&ID2D1StrokeStyle>);
            }
        }
    }
}

// ---------------------------------------------------------------- baterai: ikon dan flyout (sumber C 717-904)

/// Petir isi (status mengisi); h = tinggi total (sumber BtBolt).
fn bt_bolt(cx: f32, cy: f32, h: f32, c: D2D1_COLOR_F) {
    let Some((g, k)) = bt_path() else {
        return;
    };
    let hs = h * 0.5;
    unsafe {
        k.BeginFigure(bt_p(cx, cy, hs, 0.18, -1.0), D2D1_FIGURE_BEGIN_FILLED);
        k.AddLine(bt_p(cx, cy, hs, -0.52, 0.12));
        k.AddLine(bt_p(cx, cy, hs, -0.04, 0.12));
        k.AddLine(bt_p(cx, cy, hs, -0.18, 1.0));
        k.AddLine(bt_p(cx, cy, hs, 0.52, -0.14));
        k.AddLine(bt_p(cx, cy, hs, 0.04, -0.14));
        k.EndFigure(D2D1_FIGURE_END_CLOSED);
        let _ = k.Close();
    }
    set_color(c, 1.0);
    let rt_g = RT.borrow();
    let br_g = BR.borrow();
    if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
        let geo: &ID2D1Geometry = &g;
        let brush: &ID2D1Brush = br;
        unsafe { rt.FillGeometry(geo, brush, None::<&ID2D1Brush>) };
    }
}

/// Garis tepi rounded rectangle dengan warna c (sumber SetColor + DrawRoundedRectangle).
fn bt_outline(rr: &D2D1_ROUNDED_RECT, c: D2D1_COLOR_F, lw: f32) {
    set_color(c, 1.0);
    let rt_g = RT.borrow();
    let br_g = BR.borrow();
    if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
        let brush: &ID2D1Brush = br;
        unsafe { rt.DrawRoundedRectangle(rr, brush, lw, None::<&ID2D1StrokeStyle>) };
    }
}

/// Ikon baterai besar: badan bertepi, isi aksen sebanding persen (merah <=15% tanpa adaptor), petir saat mengisi (sumber BtBig).
fn bt_big(l: f32, cy: f32, w: f32, h: f32, charging: bool) {
    let pal = *PALETTE.borrow();
    let t = cy - h * 0.5;
    let r = l + w - sf(6.0);
    let b = cy + h * 0.5;
    let rad = sf(9.0);
    let rim = if LIGHT.get() {
        rgba(40, 24, 96, 0.55)
    } else {
        rgba(232, 235, 250, 0.78)
    };
    let body = D2D1_ROUNDED_RECT {
        rect: rf(l + 1.0, t + 1.0, r - 1.0, b - 1.0),
        radiusX: rad,
        radiusY: rad,
    };
    bt_outline(&body, rim, sf(2.0));
    fill_rr(r + sf(1.5), cy - h * 0.18, r + sf(5.5), cy + h * 0.18, sf(2.0), rim, 1.0); // kutub
    let inset = sf(4.5);
    let fl = l + inset;
    let fr = r - inset;
    let bat = BAT.get();
    let frac = if bat < 0 {
        0.0
    } else if bat > 100 {
        1.0
    } else {
        bat as f32 / 100.0
    };
    let low = bat >= 0 && bat <= 15 && !CHARGING.get();
    if frac > 0.0 {
        let c = if low { rgba(232, 72, 85, 1.0) } else { pal.accent };
        fill_rr(fl, t + inset, fl + (fr - fl) * frac, b - inset, sf(5.0), c, 1.0);
    }
    if charging {
        bt_bolt((l + r) * 0.5, cy, h * 0.62, rgba(255, 255, 255, 1.0));
    }
}

/// Tiga titik menu (sumber BtDots).
fn bt_dots(cx: f32, cy: f32, c: D2D1_COLOR_F) {
    for i in -1..=1i32 {
        let fi = i as f32;
        fill_rr(
            cx + fi * sf(6.0) - sf(1.6),
            cy - sf(1.6),
            cx + fi * sf(6.0) + sf(1.6),
            cy + sf(1.6),
            sf(1.6),
            c,
            1.0,
        );
    }
}

/// Teks rata kanan di tepi r (sumber BtTextR).
fn bt_text_r(s: &str, f: Option<&IDWriteTextFormat>, r: f32, t: f32, b: f32, c: D2D1_COLOR_F) {
    let w = measure_text(s, f) + sf(2.0);
    text(s, f, r - w, t, r, b, c, 1.0);
}

/// Kartu membulat: isi lalu garis tepi (sumber BtCard). Dipakai juga oleh calendar.rs.
pub fn bt_card(l: f32, t: f32, r: f32, b: f32, rad: f32, fill: &D2D1_COLOR_F, line: &D2D1_COLOR_F, lw: f32) {
    fill_rr(l, t, r, b, rad, *fill, 1.0);
    let rr = D2D1_ROUNDED_RECT {
        rect: rf(l + 0.5, t + 0.5, r - 0.5, b - 0.5),
        radiusX: rad,
        radiusY: rad,
    };
    bt_outline(&rr, *line, lw);
}

/// Ukuran flyout Baterai (sumber BattMeasure). Dipanggil dari panel.rs (panel_measure).
pub fn batt_measure(cw: &mut i32, ch: &mut i32) {
    *cw = s(FLY_BATT_W as f32);
    *ch = s(FLY_BATT_H as f32);
    batt_read_modes();
}

/// Menggambar flyout Baterai (sumber BattDraw). Dipanggil dari panel.rs (panel_content).
pub fn batt_draw(w: f32, h: f32) {
    let _ = h;
    let mut sps = SYSTEM_POWER_STATUS::default();
    let _ = unsafe { GetSystemPowerStatus(&mut sps) };
    let saver = (sps.SystemStatusFlag & 1) != 0;
    let chg = (sps.BatteryFlag & 8) != 0;
    let ac = sps.ACLineStatus == 1;
    let pal = *PALETTE.borrow();
    let light = LIGHT.get();
    let bat = BAT.get();
    let has_bat = HAS_BAT.get();
    let lx = sf(18.0);
    let rx = w - sf(18.0);

    // kepala: ikon besar, persen, status, menu lainnya
    bt_big(lx, sf(44.0), sf(62.0), sf(34.0), chg);
    let pct = if has_bat && bat >= 0 {
        format!("{}%", bat)
    } else {
        "--".to_string()
    };
    text(&pct, TF_HUGE.borrow().as_ref(), lx + sf(80.0), sf(12.0), rx - sf(40.0), sf(56.0), pal.text, 1.0);
    let st = if !has_bat {
        "Tanpa baterai \u{00B7} adaptor tersambung"
    } else if chg {
        "Mengisi daya \u{00B7} adaptor tersambung"
    } else if ac {
        if bat >= 99 {
            "Terisi penuh \u{00B7} adaptor tersambung"
        } else {
            "Tidak mengisi \u{00B7} adaptor tersambung"
        }
    } else if saver {
        "Memakai baterai \u{00B7} penghemat aktif"
    } else {
        "Memakai baterai"
    };
    let mut sx = lx + sf(82.0);
    if chg {
        bt_bolt(sx + sf(4.0), sf(62.0), sf(12.0), pal.accent_ink);
        sx += sf(14.0);
    }
    text(st, TF_B.borrow().as_ref(), sx, sf(52.0), rx, sf(72.0), pal.sub, 1.0);
    pan_hover(rx - sf(30.0), sf(20.0), rx + sf(2.0), sf(52.0), sf(10.0), PAN_ID_BATT_MORE);
    bt_dots(rx - sf(14.0), sf(36.0), pal.glyph);

    // perkiraan waktu sisa atau penuh
    let mut to_full = false;
    let est = batt_estimate(&mut to_full);
    let ket = if to_full || chg {
        "Perkiraan penuh"
    } else {
        "Perkiraan sisa"
    };
    text(ket, TF_B.borrow().as_ref(), lx, sf(86.0), rx - sf(120.0), sf(108.0), pal.sub, 1.0);
    let ev = if est > 0 && est < 60 {
        format!("\u{00B1} {} menit", est)
    } else if est >= 60 {
        format!("\u{00B1} {} j {} m", est / 60, est % 60)
    } else if ac && !chg {
        "Penuh".to_string()
    } else {
        "\u{2014}".to_string()
    };
    bt_text_r(&ev, TF_BS.borrow().as_ref(), rx, sf(86.0), sf(108.0), pal.text);

    // kartu penghemat baterai: status saja; klik membuka pengaturan (tanpa API publik untuk menyalakannya)
    let c0 = sf(121.0);
    let c1 = sf(180.0);
    let cy = (c0 + c1) * 0.5;
    bt_card(lx, c0, rx, c1, sf(12.0), &pal.chip, &pal.line, 1.0);
    pan_hover(lx, c0, rx, c1, sf(12.0), PAN_ID_BATT_SAVER);
    bt_icon(0, lx + sf(24.0), cy, sf(8.0), pal.glyph);
    text(
        "Penghemat baterai",
        TF_BS.borrow().as_ref(),
        lx + sf(49.0),
        c0 + sf(10.0),
        rx - sf(70.0),
        c0 + sf(30.0),
        pal.text,
        1.0,
    );
    let thresh = BS_THRESH.get();
    let ss = if saver {
        "Aktif sekarang".to_string()
    } else if thresh > 0 && thresh < 100 {
        format!("Aktif otomatis di {}%", thresh)
    } else if thresh >= 100 {
        "Selalu aktif saat memakai baterai".to_string()
    } else {
        "Tidak aktif otomatis".to_string()
    };
    text(&ss, TF_S.borrow().as_ref(), lx + sf(49.0), c0 + sf(30.0), rx - sf(70.0), c0 + sf(48.0), pal.sub, 1.0);
    let tx = rx - sf(16.0) - sf(40.0);
    let ty = cy - sf(11.0);
    fill_rr(
        tx,
        ty,
        tx + sf(40.0),
        ty + sf(22.0),
        sf(11.0),
        if saver { pal.accent } else { pal.track },
        1.0,
    );
    let kx = if saver { tx + sf(29.0) } else { tx + sf(11.0) };
    let knob = if saver {
        QS_ON_INK
    } else if light {
        rgba(90, 84, 120, 1.0)
    } else {
        rgba(205, 208, 224, 1.0)
    };
    fill_rr(kx - sf(7.0), cy - sf(7.0), kx + sf(7.0), cy + sf(7.0), sf(7.0), knob, 1.0);

    // mode daya: tiga tombol paket bawaan
    text("Mode daya", TF_BS.borrow().as_ref(), lx, sf(194.0), rx - sf(150.0), sf(216.0), pal.text, 1.0);
    let custom = BM_CUSTOM.get();
    if custom {
        bt_text_r("Diatur paket daya kustom", TF_S.borrow().as_ref(), rx, sf(194.0), sf(216.0), pal.sub);
    }
    let ml = ["Efisiensi", "Seimbang", "Performa"];
    let bw = (rx - lx - 2.0 * sf(8.0)) / 3.0;
    let b0 = sf(221.0);
    let b1 = sf(285.0);
    let mode = BM_MODE.get();
    for i in 0..3usize {
        let x0 = lx + i as f32 * (bw + sf(8.0));
        let x1 = x0 + bw;
        let sel = !custom && mode == i as i32;
        let fill = if sel { pal.accent_soft } else { pal.chip };
        let line = if sel { pal.accent_line } else { pal.line };
        bt_card(x0, b0, x1, b1, sf(12.0), &fill, &line, if sel { sf(1.2) } else { 1.0 });
        pan_hover(x0, b0, x1, b1, sf(12.0), PAN_ID_BATT_MODE + i as i32);
        let mul = if custom { 0.55 } else { 1.0 };
        let mut ic = if sel { pal.accent_ink } else { pal.glyph };
        ic.a *= mul;
        bt_icon(i as i32, (x0 + x1) * 0.5, b0 + sf(21.0), sf(8.0), ic);
        let tf = if sel { TF_SSC.borrow() } else { TF_SC.borrow() };
        text(ml[i], tf.as_ref(), x0, b0 + sf(35.0), x1, b0 + sf(55.0), pal.text, mul);
    }

    // riwayat 24 jam: satu batang per jam; terang = sempat mengisi
    text("Riwayat baterai", TF_BS.borrow().as_ref(), lx, sf(299.0), rx - sf(120.0), sf(321.0), pal.text, 1.0);
    bt_text_r("24 jam terakhir", TF_S.borrow().as_ref(), rx, sf(299.0), sf(321.0), pal.sub);
    let gl = lx + sf(36.0);
    let gb = sf(395.0);
    let gh = sf(64.0);
    let pitch = (rx - gl) / 24.0;
    let bar_w = pitch - sf(3.5);
    for (i, &label) in ["100", "50", "0"].iter().enumerate() {
        let y = gb - gh * (1.0 - i as f32 * 0.5);
        bt_text_r(label, TF_XS.borrow().as_ref(), gl - sf(6.0), y - sf(8.0), y + sf(8.0), pal.sub);
    }
    let now = bat_hour_key();
    let hist_loaded = BAT_HIST_LOADED.get();
    let hist = *BAT_HIST.borrow();
    for i in 0..24u32 {
        let k = now.wrapping_sub(23).wrapping_add(i);
        let hs = (k % 24) as usize;
        let x0 = gl + i as f32 * pitch + sf(1.75);
        let x1 = x0 + bar_w;
        if hist_loaded && hist.key[hs] == k {
            let mut top = gb - gh * (f32::from(hist.pct[hs]) / 100.0);
            if top > gb - sf(2.0) {
                top = gb - sf(2.0);
            }
            let c = if hist.chg[hs] != 0 {
                if light {
                    pal.accent
                } else {
                    rgba(76, 168, 240, 1.0)
                }
            } else if light {
                rgba(0, 99, 184, 0.55)
            } else {
                rgba(70, 120, 180, 1.0)
            };
            fill_rr(x0, top, x1, gb, sf(2.0), c, 1.0);
        } else {
            fill_rr(x0, gb - sf(2.0), x1, gb, sf(1.0), pal.track, 1.0); // jam tanpa catatan
        }
    }
    let lt = unsafe { GetLocalTime() };
    for i in (0..=24usize).step_by(6) {
        let bi = if i < 24 { i } else { 23 };
        let hr = (lt.wHour as i32 - (23 - bi as i32) + 48) % 24;
        let hl = format!("{:02}.00", hr);
        let cx = gl + bi as f32 * pitch + pitch * 0.5;
        if i < 24 {
            text(&hl, TF_XSC.borrow().as_ref(), cx - sf(22.0), sf(400.0), cx + sf(22.0), sf(416.0), pal.sub, 1.0);
        } else {
            bt_text_r(&hl, TF_XS.borrow().as_ref(), rx, sf(400.0), sf(416.0), pal.sub);
        }
    }
}

/// Menu tombol "lainnya" baterai (sumber BattMenu).
fn batt_menu() {
    let Ok(mn) = (unsafe { CreatePopupMenu() }) else {
        return;
    };
    let t1 = to_wide_z("Pengaturan daya && baterai");
    let t2 = to_wide_z("Pemakaian baterai per aplikasi");
    let _ = unsafe { AppendMenuW(mn, MF_STRING, 1, pcw(&t1)) };
    let _ = unsafe { AppendMenuW(mn, MF_STRING, 2, pcw(&t2)) };
    let mut pt = POINT::default();
    let _ = unsafe { GetCursorPos(&mut pt) };
    let cmd = unsafe {
        TrackPopupMenu(
            mn,
            TPM_RETURNCMD | TPM_RIGHTALIGN | TPM_NONOTIFY,
            pt.x,
            pt.y,
            None,
            PAN.get(),
            None,
        )
    }
    .0;
    let _ = unsafe { DestroyMenu(mn) };
    if cmd == 1 {
        pan_uri("ms-settings:powersleep");
    } else if cmd == 2 {
        pan_uri("ms-settings:batterysaver-usagedetails");
    }
}

/// Menangani klik panel Baterai (sumber BattActivate). Dipanggil dari panel.rs (panel_activate).
pub fn batt_activate(id: i32) {
    if id == PAN_ID_BATT_SAVER {
        pan_uri("ms-settings:batterysaver");
    } else if id == PAN_ID_BATT_SET {
        pan_uri("ms-settings:powersleep");
    } else if id == PAN_ID_BATT_MORE {
        batt_menu();
    } else if id >= PAN_ID_BATT_MODE && id < PAN_ID_BATT_MODE + 3 {
        // paket kustom: tidak diubah dari sini
        let set = match (BM_CUSTOM.get(), BATT_POW.get().ov_set) {
            (false, Some(f)) => f,
            _ => {
                pan_uri("ms-settings:powersleep");
                return;
            }
        };
        let g = if id == PAN_ID_BATT_MODE {
            PWR_OV_EFF
        } else if id == PAN_ID_BATT_MODE + 1 {
            PWR_OV_BAL
        } else {
            PWR_OV_PERF
        };
        if WIN32_ERROR(unsafe { set(g) }) == ERROR_SUCCESS {
            batt_read_modes();
        }
        let pan = PAN.get();
        if !pan.0.is_null() {
            w12_invalidate(pan, None, false);
        }
    }
}

// ---------------------------------------------------------------- Wi-Fi: helper gambar, kotak klik, ukuran, gulir (sumber C 318-362)

/// Kotak klik panel (salinan pan_hit privat di panel.rs; sumber PanHit).
fn wf_hit(l: f32, t: f32, r: f32, b: f32, id: i32) {
    let n = PAN_N_HIT.get();
    if n >= PAN_MAXHIT {
        return;
    }
    PAN_HIT.borrow_mut()[n as usize] = PanHitRec { r: rf(l, t, r, b), id };
    PAN_N_HIT.set(n + 1);
}

/// Teks rata tengah horizontal di cx (sumber WfTextC). Dipakai juga oleh calendar.rs.
pub fn wf_text_c(s: &str, f: Option<&IDWriteTextFormat>, cx: f32, t: f32, b: f32, c: &D2D1_COLOR_F) {
    let hw = (measure_text(s, f) + sf(2.0)) * 0.5;
    text(s, f, cx - hw, t, cx + hw, b, *c, 1.0);
}

/// Ikon sinyal Wi-Fi: busur penuh redup dan busur sesuai kekuatan (sumber WfBars).
fn wf_bars(cx: f32, cy: f32, sig: i32, c: D2D1_COLOR_F) {
    let lv: [u16; 4] = [0xE872, 0xE873, 0xE874, 0xE701];
    let k: usize = if sig >= 75 {
        3
    } else if sig >= 50 {
        2
    } else if sig >= 25 {
        1
    } else {
        0
    };
    let gf = TF_GLYPH.borrow();
    if k < 3 {
        glyph(0xE701, gf.as_ref(), cx, cy, sf(10.0), c, 0.28);
    }
    glyph(lv[k], gf.as_ref(), cx, cy, sf(10.0), c, 1.0);
}

/// Sakelar 40x22 rata kanan di rx (sumber WfToggle).
fn wf_toggle(rx: f32, cy: f32, on: bool, id: i32) {
    let pal = *PALETTE.borrow();
    let tx = rx - sf(40.0);
    let ty = cy - sf(11.0);
    wf_hit(tx - sf(6.0), ty - sf(6.0), rx + sf(4.0), ty + sf(28.0), id);
    fill_rr(tx, ty, rx, ty + sf(22.0), sf(11.0), if on { pal.accent } else { pal.track }, 1.0);
    if PAN_DOWN.get() == id {
        fill_rr(tx, ty, rx, ty + sf(22.0), sf(11.0), pal.press, 1.0);
    } else if PAN_HOV.get() == id {
        fill_rr(tx, ty, rx, ty + sf(22.0), sf(11.0), pal.hover, 1.0);
    }
    let kx = if on { tx + sf(29.0) } else { tx + sf(11.0) };
    let knob = if on {
        QS_ON_INK
    } else if LIGHT.get() {
        rgba(90, 84, 120, 1.0)
    } else {
        rgba(205, 208, 224, 1.0)
    };
    fill_rr(kx - sf(7.0), cy - sf(7.0), kx + sf(7.0), cy + sf(7.0), sf(7.0), knob, 1.0);
}

/// Tombol kapsul: aksen atau kartu biasa; teks selalu TF_S (sumber WfButton).
fn wf_button(l: f32, t: f32, r: f32, b: f32, s: &str, id: i32, accent: bool) {
    let pal = *PALETTE.borrow();
    let rad = (b - t) * 0.5;
    if accent {
        fill_rr(l, t, r, b, rad, pal.accent, 1.0);
    } else {
        bt_card(l, t, r, b, rad, &pal.chip, &pal.line, 1.0);
    }
    pan_hover(l, t, r, b, rad, id);
    let ink = if accent { QS_ON_INK } else { pal.text };
    wf_text_c(s, TF_S.borrow().as_ref(), (l + r) * 0.5, t, b, &ink);
}

/// Ukuran flyout Wi-Fi; memulai pindai cache lalu pindai baru (sumber WifiMeasure). Dipanggil dari panel.rs (panel_measure).
pub fn wifi_measure(cw: &mut i32, ch: &mut i32) {
    *cw = s(FLY_WIFI_W as f32);
    let tall = if NET_ETH_OK.get() {
        let con = WL_RES.borrow().as_ref().is_some_and(|r| r.con);
        if con {
            540.0
        } else {
            490.0
        }
    } else {
        FLY_WIFI_H as f32
    };
    *ch = s(tall);
    WL_SCROLL.set(0);
    WL_FAIL.borrow_mut().clear();
    rad_start(0, false); // baca status radio untuk sakelar (cannot-find: rad_start dari w12qs.inc)
    wl_start(WL_SCAN, None);
}

/// Gulir daftar Wi-Fi oleh roda mouse (sumber WifiWheel). Dipanggil dari panel.rs.
pub fn wifi_wheel(delta: i32) {
    let mut v = WL_SCROLL.get() - delta * s(WL_ROW as f32) / 120;
    if v > WL_SCROLL_MAX.get() {
        v = WL_SCROLL_MAX.get();
    }
    if v < 0 {
        v = 0;
    }
    WL_SCROLL.set(v);
}

// ---------------------------------------------------------------- Wi-Fi: menggambar flyout (sumber C 364-504)

/// Menggambar flyout Wi-Fi (sumber WifiDraw). Dipanggil dari panel.rs (panel_content).
/// WL_RES dipegang (borrow) selama menggambar; fungsi gambar tidak mengirim pesan.
pub fn wifi_draw(w: f32, h: f32) {
    let pal = *PALETTE.borrow();
    let lx = sf(16.0);
    let rx = w - sf(16.0);
    let rs = wifi_radio();
    let off = rs == 0;
    let eth = NET_ETH_OK.get();
    let on = rs == 1 || (rs < 0 && NET.get() == NET_WIFI);
    let busy = WL_BUSY.load(Ordering::Relaxed) != 0;
    let run = WL_RUN.get();
    let scanning = busy && run == WL_SCAN;
    let wl_g = WL_RES.borrow();
    let r: Option<&WlRes> = wl_g.as_ref();
    let con = !off && r.is_some_and(|x| x.con);
    let need_login = con && r.is_some_and(|x| x.need_login || !x.con_sec);

    // kepala: judul, status, pindai ulang, sakelar
    let hy = sf(34.0);
    let title = if eth { "Ethernet & Wi-Fi" } else { "Wi-Fi" };
    text(title, TF_BS.borrow().as_ref(), lx, sf(14.0), rx - sf(110.0), sf(34.0), pal.text, 1.0);
    let st: &str = if eth && con {
        "Keduanya terhubung"
    } else if eth {
        if con {
            "LAN & Wi-Fi terhubung"
        } else if on {
            "LAN terhubung"
        } else {
            "LAN terhubung \u{00B7} Wi-Fi mati"
        }
    } else if RAD_PEND_KIND.get() == 1 {
        if RAD_PEND_ON.get() != 0 {
            "Menyalakan\u{2026}"
        } else {
            "Mematikan\u{2026}"
        }
    } else if off {
        "Mati"
    } else if con || NET.get() == NET_WIFI {
        "Terhubung"
    } else {
        "Tidak terhubung"
    };
    text(st, TF_S.borrow().as_ref(), lx, sf(34.0), rx - sf(110.0), sf(52.0), pal.sub, 1.0);
    wf_toggle(rx, hy, on, PAN_ID_WIFI_TOG);
    if !off {
        let bx = rx - sf(40.0) - sf(14.0) - sf(32.0);
        pan_hover(bx, hy - sf(16.0), bx + sf(32.0), hy + sf(16.0), sf(10.0), PAN_ID_WIFI_SCAN);
        let mul = if scanning { 0.4 } else { 1.0 };
        glyph(0xE72C, TF_GLYPH.borrow().as_ref(), bx + sf(16.0), hy, sf(10.0), pal.glyph, mul);
    }

    let mut y = sf(66.0);

    // kartu koneksi Ethernet / LAN (bila tersambung)
    if eth {
        let e0 = y;
        let e1 = y + sf(62.0);
        let ecy = (e0 + e1) * 0.5;
        bt_card(lx, e0, rx, e1, sf(12.0), &pal.chip, &pal.line, 1.0);
        pan_hover(lx, e0, rx, e1, sf(12.0), PAN_ID_ETH_ROW);
        glyph(0xE839, TF_GLYPH.borrow().as_ref(), lx + sf(24.0), ecy, sf(12.0), pal.accent_ink, 1.0);
        let name = ETH_NAME.borrow().clone();
        let name_s = if name.is_empty() { "Ethernet".to_string() } else { name };
        text(&name_s, TF_BS.borrow().as_ref(), lx + sf(48.0), e0 + sf(8.0), rx - sf(14.0), e0 + sf(28.0), pal.text, 1.0);
        text("Terhubung", TF_S.borrow().as_ref(), lx + sf(48.0), e0 + sf(27.0), rx - sf(14.0), e0 + sf(44.0), pal.accent_ink, 1.0);
        let mut parts: Vec<String> = Vec::new();
        let speed = ETH_SPEED.get();
        if speed > 0 {
            parts.push(format!("{} Mbps", speed));
        }
        let ip = ETH_IP.borrow().clone();
        if !ip.is_empty() {
            parts.push(ip);
        }
        let mut edet = parts.join(" \u{00B7} ");
        if edet.is_empty() {
            edet = ETH_DESC.borrow().clone();
        }
        if !edet.is_empty() {
            text(&edet, TF_SUB.borrow().as_ref(), lx + sf(48.0), e0 + sf(44.0), rx - sf(14.0), e0 + sf(59.0), pal.sub, 1.0);
        }
        y = e1 + sf(10.0);
    }

    // kartu koneksi Wi-Fi: SSID, keamanan, pita, kecepatan, IP, tombol putuskan
    if let (true, Some(rv)) = (con, r) {
        let c0 = y;
        let c1 = y + sf(72.0);
        let cy = (c0 + c1) * 0.5;
        bt_card(lx, c0, rx, c1, sf(12.0), &pal.chip, &pal.line, 1.0);
        pan_hover(lx, c0, rx, c1, sf(12.0), PAN_ID_WIFI_ROW);
        wf_bars(lx + sf(24.0), cy, rv.con_sig, pal.accent_ink);
        let br = rx - sf(12.0);
        let bl = br - measure_text("Putuskan", TF_S.borrow().as_ref()) - sf(28.0);
        let tr = bl - sf(8.0);
        let ssid: &str = if rv.con_ssid.is_empty() { "Wi-Fi" } else { rv.con_ssid.as_str() };
        text(ssid, TF_BS.borrow().as_ref(), lx + sf(48.0), c0 + sf(9.0), tr, c0 + sf(29.0), pal.text, 1.0);
        let disc = busy && run == WL_DISC;
        let stc: &str = if disc {
            "Memutuskan\u{2026}"
        } else if rv.con_sec {
            "Terhubung, aman"
        } else {
            "Terhubung, terbuka"
        };
        text(stc, TF_S.borrow().as_ref(), lx + sf(48.0), c0 + sf(29.0), tr, c0 + sf(46.0), pal.accent_ink, 1.0);
        let mut parts: Vec<String> = Vec::new();
        if rv.con_band == 24 {
            parts.push("2,4 GHz".to_string());
        } else if rv.con_band != 0 {
            parts.push(format!("{} GHz", rv.con_band));
        }
        if rv.con_mbps > 0 {
            parts.push(format!("{} Mbps", rv.con_mbps));
        }
        if !rv.ip.is_empty() {
            parts.push(rv.ip.clone());
        }
        let det = if parts.is_empty() {
            format!("Sinyal {}%", rv.con_sig)
        } else {
            parts.join(" \u{00B7} ")
        };
        text(&det, TF_SUB.borrow().as_ref(), lx + sf(48.0), c0 + sf(46.0), tr, c0 + sf(63.0), pal.sub, 1.0);
        wf_button(bl, cy - sf(14.0), br, cy + sf(14.0), "Putuskan", PAN_ID_WIFI_DISC, false);
        y = c1 + sf(10.0);
        if need_login {
            let l0 = y;
            let l1 = y + sf(38.0);
            let lcy = (l0 + l1) * 0.5;
            bt_card(lx, l0, rx, l1, sf(10.0), &pal.accent, &pal.accent, 1.0);
            pan_hover(lx, l0, rx, l1, sf(10.0), PAN_ID_WIFI_LOGIN);
            glyph(0xE774, TF_GLYPH.borrow().as_ref(), lx + sf(20.0), lcy, sf(9.0), QS_ON_INK, 1.0);
            text(
                "Tindakan diperlukan \u{00B7} Masuk ke jaringan",
                TF_S.borrow().as_ref(),
                lx + sf(36.0),
                l0,
                rx - sf(72.0),
                l1,
                QS_ON_INK,
                1.0,
            );
            wf_button(rx - sf(68.0), lcy - sf(13.0), rx - sf(8.0), lcy + sf(13.0), "Masuk", PAN_ID_WIFI_LOGIN, false);
            y = l1 + sf(10.0);
        }
    }

    // daftar jaringan terdeteksi
    let fy0 = h - sf(56.0);
    let fy1 = h - sf(16.0);
    if !off {
        text("Jaringan tersedia", TF_B.borrow().as_ref(), lx, y, rx - sf(100.0), y + sf(20.0), pal.sub, 1.0);
        if scanning {
            bt_text_r("Mencari\u{2026}", TF_S.borrow().as_ref(), rx, y, y + sf(20.0), pal.sub);
        }
    }
    let vt = y + sf(26.0);
    let vb = fy0 - sf(10.0);
    let vcx = w * 0.5;
    let vcy = (vt + vb) * 0.5;
    WL_SCROLL_MAX.set(0);
    if off {
        glyph(0xE701, TF_GLYPH.borrow().as_ref(), vcx, vcy - sf(34.0), sf(10.0), pal.glyph, 0.45);
        wf_text_c("Wi-Fi mati", TF_BS.borrow().as_ref(), vcx, vcy - sf(14.0), vcy + sf(6.0), &pal.text);
        wf_text_c(
            "Nyalakan untuk melihat jaringan di sekitar",
            TF_S.borrow().as_ref(),
            vcx,
            vcy + sf(8.0),
            vcy + sf(26.0),
            &pal.sub,
        );
    } else if r.is_some_and(|x| x.err == ERROR_ACCESS_DENIED) {
        wf_text_c("Izinkan akses lokasi", TF_BS.borrow().as_ref(), vcx, vcy - sf(56.0), vcy - sf(36.0), &pal.text);
        wf_text_c(
            "Windows hanya memberi daftar jaringan sekitar",
            TF_S.borrow().as_ref(),
            vcx,
            vcy - sf(30.0),
            vcy - sf(12.0),
            &pal.sub,
        );
        wf_text_c(
            "ke aplikasi yang diizinkan memakai lokasi.",
            TF_S.borrow().as_ref(),
            vcx,
            vcy - sf(12.0),
            vcy + sf(6.0),
            &pal.sub,
        );
        wf_button(lx + sf(8.0), vcy + sf(20.0), vcx - sf(5.0), vcy + sf(52.0), "Buka izin lokasi", PAN_ID_WIFI_PERM, true);
        wf_button(vcx + sf(5.0), vcy + sf(20.0), rx - sf(8.0), vcy + sf(52.0), "Daftar Windows", PAN_ID_WIFI_MORE, false);
    } else if let Some(rv) = r.filter(|x| x.err != ERROR_SUCCESS) {
        let no_ad = rv.err == ERROR_NOT_FOUND || rv.err == ERROR_SERVICE_NOT_ACTIVE;
        let (t1, t2) = if no_ad {
            ("Adaptor Wi-Fi tidak ditemukan", "Periksa driver atau layanan WLAN AutoConfig")
        } else {
            ("Daftar jaringan tidak tersedia", "Coba pindai ulang atau buka daftar Windows")
        };
        wf_text_c(t1, TF_BS.borrow().as_ref(), vcx, vcy - sf(34.0), vcy - sf(14.0), &pal.text);
        wf_text_c(t2, TF_S.borrow().as_ref(), vcx, vcy - sf(12.0), vcy + sf(6.0), &pal.sub);
        wf_button(vcx - sf(80.0), vcy + sf(20.0), vcx + sf(80.0), vcy + sf(52.0), "Daftar Windows", PAN_ID_WIFI_MORE, false);
    } else if r.map_or(true, |x| x.net.is_empty()) {
        let msg = if r.is_none() || busy {
            "Mencari jaringan\u{2026}"
        } else if con {
            "Tidak ada jaringan lain terdeteksi"
        } else {
            "Tidak ada jaringan terdeteksi"
        };
        wf_text_c(msg, TF_S.borrow().as_ref(), vcx, vcy - sf(10.0), vcy + sf(10.0), &pal.sub);
    } else if let Some(rv) = r {
        let row = sf(WL_ROW as f32);
        let total = row * rv.net.len() as f32;
        let view = vb - vt;
        let smax = if total > view { (total - view + 0.5) as i32 } else { 0 };
        WL_SCROLL_MAX.set(smax);
        if WL_SCROLL.get() > smax {
            WL_SCROLL.set(smax);
        }
        let scroll = WL_SCROLL.get();
        let clip = rf(0.0, vt, w, vb);
        if let Some(rt) = RT.borrow().as_ref() {
            unsafe { rt.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
        }
        for (i, n) in rv.net.iter().enumerate() {
            let t = vt + row * i as f32 - scroll as f32;
            let b = t + row;
            if b <= vt || t >= vb {
                continue;
            }
            let id = PAN_ID_WIFI_NET + i as i32;
            wf_hit(lx, t.max(vt), rx, b.min(vb), id); // kotak klik dipotong ke area daftar
            if PAN_DOWN.get() == id {
                fill_rr(lx, t + sf(2.0), rx, b - sf(2.0), sf(10.0), pal.press, 1.0);
            } else if PAN_HOV.get() == id {
                fill_rr(lx, t + sf(2.0), rx, b - sf(2.0), sf(10.0), pal.hover, 1.0);
            }
            let cy = (t + b) * 0.5;
            wf_bars(lx + sf(24.0), cy, n.sig, pal.glyph);
            let is_join = !WL_JOIN.borrow().is_empty() && *WL_JOIN.borrow() == n.ssid;
            let is_fail = !is_join && !WL_FAIL.borrow().is_empty() && *WL_FAIL.borrow() == n.ssid;
            let saved_s = if n.saved { " \u{00B7} tersimpan" } else { "" };
            let sub = if is_join {
                "Menghubungkan\u{2026}".to_string()
            } else if is_fail {
                "Tidak dapat tersambung".to_string()
            } else if !n.sec {
                format!("Terbuka \u{00B7} Perlu login web{}", saved_s)
            } else {
                // cabang ini hanya tercapai bila n.sec benar, jadi teks "Aman"
                format!("Aman{}", saved_s)
            };
            text(&n.ssid, TF_BS.borrow().as_ref(), lx + sf(48.0), t + sf(6.0), rx - sf(14.0), t + sf(26.0), pal.text, 1.0);
            let sc = if is_join { pal.accent_ink } else { pal.sub };
            text(&sub, TF_S.borrow().as_ref(), lx + sf(48.0), t + sf(25.0), rx - sf(14.0), t + sf(42.0), sc, 1.0);
        }
        if let Some(rt) = RT.borrow().as_ref() {
            unsafe { rt.PopAxisAlignedClip() };
        }
        if smax > 0 {
            // bilah gulir tipis
            let th = view * view / total;
            let ty = vt + (view - th) * scroll as f32 / smax as f32;
            fill_rr(w - sf(8.0), ty, w - sf(5.0), ty + th, sf(1.5), pal.sub, 0.5);
        }
    }

    // kaki: pengaturan jaringan
    fill_rr(lx, fy0, rx, fy1, sf(12.0), pal.chip, 1.0);
    pan_hover(lx, fy0, rx, fy1, sf(12.0), PAN_ID_WIFI_SET);
    glyph(0xE713, TF_GLYPH.borrow().as_ref(), lx + sf(22.0), (fy0 + fy1) * 0.5, sf(10.0), pal.glyph, 1.0);
    text("Pengaturan jaringan & internet", TF_S.borrow().as_ref(), lx + sf(44.0), fy0, rx - sf(12.0), fy1, pal.text, 1.0);
}

/// Menangani klik di flyout Wi-Fi (sumber WifiActivate, baris 506-527). Dipanggil dari panel.rs (PanelActivate).
pub fn wifi_activate(id: i32) {
    if id == PAN_ID_WIFI_SET {
        pan_uri("ms-settings:network");
    } else if id == PAN_ID_ETH_ROW {
        pan_uri("ms-settings:network-ethernet");
    } else if id == PAN_ID_WIFI_ROW {
        pan_uri("ms-settings:network-wifi");
    } else if id == PAN_ID_WIFI_PERM {
        pan_uri("ms-settings:privacy-location");
    } else if id == PAN_ID_WIFI_MORE {
        pan_uri("ms-availablenetworks:");
    } else if id == PAN_ID_WIFI_LOGIN {
        let op = to_wide_z("open");
        let url = to_wide_z("http://www.msftconnecttest.com/redirect");
        // SAFETY: buffer UTF-16 (ber-NUL) hidup sampai ShellExecuteW selesai; hwnd dan parameter kosong.
        let _ = unsafe {
            ShellExecuteW(None, pcw(&op), pcw(&url), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL)
        };
    } else if id == PAN_ID_WIFI_SCAN {
        WL_FAIL.borrow_mut().clear();
        wl_start(WL_SCAN, None);
    } else if id == PAN_ID_WIFI_DISC {
        wl_start(WL_DISC, None);
    } else if id == PAN_ID_WIFI_TOG {
        // radio belum terbaca dan tidak ada perintah radio berjalan: buka pengaturan Wi-Fi
        if RAD_WIFI.get() < 0 && RAD_BUSY.load(Ordering::Relaxed) == 0 {
            pan_uri("ms-settings:network-wifi");
            return;
        }
        let rs = wifi_radio();
        rad_start(1, !(rs == 1 || (rs < 0 && NET.get() == NET_WIFI)));
    } else {
        // baris jaringan (PAN_ID_WIFI_NET + indeks). Salinan dibuat agar borrow WL_RES lepas sebelum wl_start.
        let k = id - PAN_ID_WIFI_NET;
        let pick: Option<WlNet> = {
            let g = WL_RES.borrow();
            match &*g {
                Some(r) if k >= 0 && (k as usize) < r.net.len() => {
                    let n = &r.net[k as usize];
                    Some(WlNet {
                        ssid: n.ssid.clone(),
                        prof: n.prof.clone(),
                        sig: n.sig,
                        bss: n.bss,
                        sec: n.sec,
                        saved: n.saved,
                    })
                }
                _ => None,
            }
        };
        if let Some(n) = pick {
            if !n.saved {
                pan_uri("ms-availablenetworks:"); // jaringan baru: sandi diisi di daftar Windows
                return;
            }
            *WL_JOIN.borrow_mut() = n.ssid.chars().take(32).collect::<String>(); // lstrcpynW(.., 33)
            WL_FAIL.borrow_mut().clear();
            wl_start(WL_CONNECT, Some(&n));
        }
    }
}

//! Sumber: w12nl.inc (1–163). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime.
//! NectarLink: hanya membaca /v1/devices lewat loopback; token tidak disimpan atau dicatat.
//! Makro `w12_lazy!` TIDAK didefinisikan di sini: sudah ada di lazy.rs (`#[macro_export]`).
use super::prelude::*;
use std::ffi::c_void;
use std::io::Read;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};

/// Port API lokal NectarLink (C: `out->port == 53320` dan `WinHttpConnect`).
const NL_API_PORT: u16 = 53320;
/// Kapasitas buffer respons (C: `char body[32769]`); data dibaca sampai 32768 byte.
const NL_API_BODY_CAP: usize = 32769;

// Konstanta WinHTTP. Nilai sama dengan winhttp.h (dicek di windows-0.61.3); fitur Win32_Networking_WinHttp kini aktif di Cargo.toml (konstanta tetap ditulis lokal).
const NL_WH_ACCESS_NO_PROXY: u32 = 1;
const NL_WH_OPTION_REDIRECT_POLICY: u32 = 88;
const NL_WH_REDIRECT_POLICY_NEVER: u32 = 0;
/// WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER
const NL_WH_QUERY_STATUS_NUMBER: u32 = 19 | 0x2000_0000;

// Wrapper yang belum ada di flyout.rs. Memakai slot `G_WINHTTP` yang sama dengan flyout (seperti `g_hWinHttp` di C).
crate::w12_lazy!(G_WINHTTP, "winhttp.dll", "WinHttpQueryDataAvailable", fn winhttp_query_data_available(h: *mut c_void, avail: *mut u32) -> i32, 0);

/// Hasil perangkat NectarLink (C: `g_nlDevices`). `None` = dikosongkan (C: SecureZeroMemory).
/// Dibuat `Option` karena `NlJsonDeviceList::default()` bukan const fn (lihat catatan PENDING di state.rs).
pub static NL_DEVICES: SyncCell<RefCell<Option<NlJsonDeviceList>>> = SyncCell::new(RefCell::new(None));

// Penanda yang dibaca thread pembaca dan thread UI, jadi memakai atomik (C: volatile LONG).
static NL_API_OPEN: AtomicI32 = AtomicI32::new(0);
static NL_API_BUSY: AtomicI32 = AtomicI32::new(0);
static NL_API_GENERATION: AtomicU32 = AtomicU32::new(0);

/// Hasil pembacaan (C: `NlApiResult`). Dibuat di thread pembaca; kepemilikan pindah ke UI lewat LPARAM.
struct NlApiResult {
    state: i32,
    devices: NlJsonDeviceList,
}

/// Pembungkus handle WinHTTP. Drop memanggil `winhttp_close` (urutan sama dengan label `done:` di C).
struct NlWinHttpHandle(*mut c_void);

impl Drop for NlWinHttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let _ = unsafe { winhttp_close(self.0) };
        }
    }
}

/// Pengganti `SecureZeroMemory`: tulis nol dengan `write_volatile` agar tidak dihapus kompilator.
fn nl_api_wipe<T: Default + Copy>(buf: &mut [T]) {
    for item in buf.iter_mut() {
        unsafe { std::ptr::write_volatile(item, T::default()) };
    }
    std::sync::atomic::compiler_fence(Ordering::SeqCst);
}

/// Kosongkan hasil perangkat lama (C: `SecureZeroMemory(&g_nlDevices)`).
fn nl_api_clear_devices() {
    let old = NL_DEVICES.borrow_mut().take();
    if let Some(mut devices) = old {
        nl_api_wipe(std::slice::from_mut(&mut devices));
    }
}

/// `NlApiReadToken`: baca `%LOCALAPPDATA%\Nectarlink\api-token.json` (1..=4096 byte), uraikan, lalu pastikan port 53320.
fn nl_api_read_token(out: &mut NlJsonApiToken) -> bool {
    nl_api_wipe(std::slice::from_mut(&mut *out));
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return false;
    };
    let path = std::path::PathBuf::from(local)
        .join("Nectarlink")
        .join("api-token.json");
    let Ok(mut file) = std::fs::File::open(&path) else {
        return false;
    };
    let size = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => return false,
    };
    if size == 0 || size > 4096 {
        return false;
    }
    let mut json = vec![0u8; size as usize];
    let read_ok = file.read_exact(&mut json).is_ok();
    let ok = read_ok && nl_json_parse_api_token(&json, &mut *out) && out.port == NL_API_PORT;
    nl_api_wipe(&mut json);
    if !ok {
        nl_api_wipe(std::slice::from_mut(&mut *out));
    }
    ok
}

/// Loop `WinHttpQueryDataAvailable` / `WinHttpReadData` dari `NlApiFetch`.
/// `Err` berisi kode hasil: NL_API_ERROR bila panggilan WinHTTP gagal, NL_API_BAD_DATA bila melebihi buffer.
fn nl_api_read_body(request: *mut c_void, body: &mut [u8]) -> Result<usize, i32> {
    let mut total: usize = 0;
    loop {
        let mut available: u32 = 0;
        if unsafe { winhttp_query_data_available(request, &mut available) } == 0 {
            return Err(NL_API_ERROR);
        }
        if available == 0 {
            break;
        }
        if available as usize > body.len() - 1 - total {
            return Err(NL_API_BAD_DATA);
        }
        let mut read: u32 = 0;
        let read_ok = unsafe {
            winhttp_read_data(request, body[total..].as_mut_ptr() as *mut c_void, available, &mut read)
        };
        if read_ok == 0 {
            return Err(NL_API_ERROR);
        }
        if read == 0 {
            break;
        }
        total += read as usize;
    }
    Ok(total)
}

/// Bagian HTTP dari `NlApiFetch`: GET /v1/devices ke 127.0.0.1 dengan header Bearer, redirect dimatikan.
fn nl_api_http_get(token: &NlJsonApiToken, devices: &mut NlJsonDeviceList) -> i32 {
    let agent = to_wide_z("AetherRS/0.1");
    let session = NlWinHttpHandle(unsafe {
        winhttp_open(pcw(&agent), NL_WH_ACCESS_NO_PROXY, PCWSTR::null(), PCWSTR::null(), 0)
    });
    if session.0.is_null() {
        return NL_API_ERROR;
    }
    unsafe { winhttp_set_timeouts(session.0, 400, 400, 700, 700) };
    let host = to_wide_z("127.0.0.1");
    let connect = NlWinHttpHandle(unsafe { winhttp_connect(session.0, pcw(&host), NL_API_PORT, 0) });
    if connect.0.is_null() {
        return NL_API_ERROR;
    }
    let verb = to_wide_z("GET");
    let object = to_wide_z("/v1/devices");
    let request = NlWinHttpHandle(unsafe {
        winhttp_open_request(
            connect.0,
            pcw(&verb),
            pcw(&object),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            0,
        )
    });
    if request.0.is_null() {
        return NL_API_ERROR;
    }
    let redirect: u32 = NL_WH_REDIRECT_POLICY_NEVER;
    let set_ok = unsafe {
        winhttp_set_option(
            request.0,
            NL_WH_OPTION_REDIRECT_POLICY,
            &redirect as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    if set_ok == 0 {
        return NL_API_ERROR;
    }

    // Header seperti C: "Authorization: Bearer " + 64 karakter token + penutup, diakhiri NUL.
    let mut headers: Vec<u16> = "Authorization: Bearer ".encode_utf16().collect();
    headers.extend(token.token[..64].iter().map(|&b| b as u16));
    headers.extend("\r\nAccept: application/json\r\n".encode_utf16());
    headers.push(0);
    let sent = unsafe {
        winhttp_send_request(request.0, pcw(&headers), u32::MAX, std::ptr::null(), 0, 0, 0)
    };
    nl_api_wipe(&mut headers);
    if sent == 0 || unsafe { winhttp_receive_response(request.0, std::ptr::null_mut()) } == 0 {
        return NL_API_ERROR;
    }

    let mut status: u32 = 0;
    let mut status_size: u32 = std::mem::size_of::<u32>() as u32;
    let query_ok = unsafe {
        winhttp_query_headers(
            request.0,
            NL_WH_QUERY_STATUS_NUMBER,
            PCWSTR::null(),
            &mut status as *mut u32 as *mut c_void,
            &mut status_size,
            std::ptr::null_mut(),
        )
    };
    if query_ok == 0 {
        return NL_API_ERROR;
    }
    if status == 401 || status == 403 {
        return NL_API_AUTH;
    }
    if status != 200 {
        return NL_API_ERROR;
    }

    let mut body = vec![0u8; NL_API_BODY_CAP];
    let result = match nl_api_read_body(request.0, &mut body) {
        Ok(total) => {
            if total > 0 && nl_json_parse_devices(&body[..total], devices) {
                NL_API_READY
            } else {
                NL_API_BAD_DATA
            }
        }
        Err(code) => code,
    };
    nl_api_wipe(&mut body);
    result
}

/// `NlApiFetch`: baca token, lakukan GET, dan kembalikan kode status. Token dihapus dari memori setelah selesai.
fn nl_api_fetch(devices: &mut NlJsonDeviceList) -> i32 {
    let mut token = NlJsonApiToken::default();
    if !nl_api_read_token(&mut token) {
        return NL_API_NO_TOKEN;
    }
    let result = nl_api_http_get(&token, devices);
    nl_api_wipe(std::slice::from_mut(&mut token));
    result
}

/// `PostMessageW` ke jendela shell dengan hasil; `raw` berpindah kepemilikan ke UI bila berhasil.
fn nl_api_post(hwnd: isize, generation: u32, raw: *mut NlApiResult) -> bool {
    unsafe {
        PostMessageW(
            Some(HWND(hwnd as *mut c_void)),
            WM_W12_NLRESULT,
            WPARAM(generation as usize),
            LPARAM(raw as isize),
        )
    }
    .is_ok()
}

/// Membebaskan `NlApiResult` yang tidak sempat diterima UI (C: SecureZeroMemory + free).
fn nl_api_free_result(raw: *mut NlApiResult) {
    let mut result = unsafe { Box::from_raw(raw) };
    nl_api_wipe(std::slice::from_mut(&mut result.devices));
}

/// `NlApiThread`: hitung hasil di thread latar, lalu kirim ke jendela shell (MSG) lewat WM_W12_NLRESULT.
/// `hwnd` diambil di thread UI saat spawn (C membaca `g_msg` di thread; nilainya tidak berubah setelah start).
fn nl_api_thread(generation: u32, hwnd: isize) {
    let mut devices = NlJsonDeviceList::default();
    let state = nl_api_fetch(&mut devices);
    let raw = Box::into_raw(Box::new(NlApiResult { state, devices }));
    let posted = hwnd != 0 && nl_api_post(hwnd, generation, raw);
    if !posted {
        nl_api_free_result(raw);
        NL_API_BUSY.store(0, Ordering::SeqCst);
    }
}

/// `NlApiRefresh`: mulai satu pembacaan latar bila panel terbuka dan belum ada yang berjalan.
pub fn nl_api_refresh() {
    if NL_API_OPEN.load(Ordering::SeqCst) == 0 {
        return;
    }
    if NL_API_BUSY
        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let generation = NL_API_GENERATION.load(Ordering::SeqCst);
    let hwnd = MSG.get().0 as isize;
    // Handle thread dilepas (sama dengan CloseHandle di C); thread berjalan terpisah.
    let spawned = std::thread::Builder::new().spawn(move || nl_api_thread(generation, hwnd));
    if spawned.is_err() {
        NL_API_BUSY.store(0, Ordering::SeqCst);
    }
}

/// `NlApiOpen`: panel dibuka. Status jadi LOADING lalu pembacaan dimulai (tidak menunggu jaringan).
pub fn nl_api_open() {
    NL_API_OPEN.store(1, Ordering::SeqCst);
    NL_API_GENERATION.fetch_add(1, Ordering::SeqCst);
    nl_api_clear_devices();
    NL_API_STATE.set(NL_API_LOADING);
    nl_api_refresh();
}

/// `NlApiClose`: panel ditutup. Hasil yang masih di thread akan diabaikan karena generasi berubah.
pub fn nl_api_close() {
    NL_API_OPEN.store(0, Ordering::SeqCst);
    NL_API_GENERATION.fetch_add(1, Ordering::SeqCst);
    NL_API_STATE.set(NL_API_IDLE);
    nl_api_clear_devices();
}

/// `NlApiAcceptResult`: dipanggil dari WndProc (thread UI) saat WM_W12_NLRESULT.
/// `value` adalah `*mut NlApiResult` dari `nl_api_thread`. Safety: hanya boleh diterima sekali dan berasal dari sana.
pub unsafe fn nl_api_accept_result(generation: WPARAM, value: LPARAM) {
    NL_API_BUSY.store(0, Ordering::SeqCst);
    if value.0 == 0 {
        return;
    }
    let mut result = unsafe { Box::from_raw(value.0 as *mut NlApiResult) };
    let current = NL_API_GENERATION.load(Ordering::SeqCst);
    if NL_API_OPEN.load(Ordering::SeqCst) != 0 && current == generation.0 as u32 {
        NL_API_STATE.set(result.state);
        nl_api_clear_devices(); // wipe hasil lama sebelum diganti (C: SecureZeroMemory)
        *NL_DEVICES.borrow_mut() = Some(result.devices);
    }
    nl_api_wipe(std::slice::from_mut(&mut result.devices));
}

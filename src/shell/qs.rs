//! Pengaturan Cepat (QS) dan kecerahan WMI. Sumber: w12qs.inc:1-891 (seluruh file).
//! Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H09.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;

use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Devices::Radios::{Radio, RadioState};
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, STGM_READ,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_BINARY, RRF_RT_REG_BINARY,
};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::Win32::System::Variant::{
    VariantClear, VariantInit, VARIANT, VT_BSTR, VT_I2, VT_I4, VT_LPWSTR, VT_UI1,
};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
use windows::core::{Interface, BSTR};
use windows::Win32::System::Wmi::{
    IEnumWbemClassObject, IWbemClassObject, IWbemContext, IWbemLocator, IWbemServices, WbemLocator,
    WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE, WBEM_S_NO_ERROR,
};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

use super::prelude::*;

// Ukuran dalam dip (sebelum skala DPI), sama dengan enum di w12qs.inc:11-12.
const QS_W: i32 = 384;
const QS_PAD: i32 = 16;
const QS_HEADH: i32 = 32;
const QS_ROWH: i32 = 36;
const QS_VGAP: i32 = 12;
const QS_VPH: i32 = 268;
const QS_AUDH: i32 = 52;
const QS_TILEH: i32 = 72;
const QS_TGAP: i32 = 8;
const QS_TROWS: i32 = 6;
const QS_TCOLS: i32 = 3;
const QS_NTILE: usize = 18;

// Jenis aksi ubin (QK_*, w12qs.inc:13).
const QK_URI: i32 = 0;
const QK_KEYS: i32 = 1;
const QK_DARK: i32 = 2;
const QK_RADIO: i32 = 3;
const QK_AIRPLANE: i32 = 4;
const QK_HOTSPOT: i32 = 5;
const QK_NIGHT: i32 = 6;
const QK_DND: i32 = 7;
const QK_NEARBY: i32 = 8;
const QK_SAVER: i32 = 9;
const QK_MIC: i32 = 10;
const QK_LOC: i32 = 11;

// Sumber status on/off ubin (QST_*, w12qs.inc:14).
const QST_NONE: i32 = 0;
const QST_WIFI: i32 = 1;
const QST_DARK: i32 = 2;
const QST_SAVER: i32 = 3;
const QST_BT: i32 = 4;
const QST_AIRPLANE: i32 = 5;
const QST_HOTSPOT: i32 = 6;
const QST_NIGHT: i32 = 7;
const QST_DND: i32 = 8;
const QST_NEARBY: i32 = 9;
const QST_MIC: i32 = 10;
const QST_LOC: i32 = 11;
const QST_CAST: i32 = 12;

// Kode virtual-key untuk pintasan ubin (VK_LWIN, VK_CONTROL di C++).
const QS_VK_LWIN: u16 = 0x5B;
const QS_VK_CONTROL: u16 = 0x11;

// Pop-up konfirmasi: SetTimer(PAN, 88, 1900) dan kedaluwarsa 1800 ms (w12qs.inc:329-338).
const QS_TOAST_TIMER: usize = 88;
const QS_TOAST_TIMER_MS: u32 = 1900;
const QS_TOAST_LIFE_MS: u64 = 1800;

/// Definisi satu ubin (setara `QsTileDef` di w12qs.inc:17).
struct QsTileDef {
    label: &'static str,
    glyph: u16,
    kind: i32,
    st: i32,
    uri: Option<&'static str>,
    k1: u16,
    k2: u16,
    k3: u16,
}

// Urutan ubin mengikuti w12qs.inc:19-38 (18 entri berurutan).
static G_QS_TILES: [QsTileDef; QS_NTILE] = [
    QsTileDef { label: "Wi-Fi", glyph: 0xE701, kind: QK_RADIO, st: QST_WIFI, uri: Some("ms-settings:network-wifi"), k1: 1, k2: 0, k3: 0 },
    QsTileDef { label: "Bluetooth", glyph: 0xE702, kind: QK_RADIO, st: QST_BT, uri: Some("ms-settings:bluetooth"), k1: 3, k2: 0, k3: 0 },
    QsTileDef { label: "Mode pesawat", glyph: 0xE709, kind: QK_AIRPLANE, st: QST_AIRPLANE, uri: Some("ms-settings:network-airplanemode"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Hotspot seluler", glyph: 0xE704, kind: QK_HOTSPOT, st: QST_HOTSPOT, uri: Some("ms-settings:network-mobilehotspot"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "VPN", glyph: 0xE705, kind: QK_URI, st: QST_NONE, uri: Some("ms-settings:network-vpn"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Lampu malam", glyph: 0xE708, kind: QK_NIGHT, st: QST_NIGHT, uri: Some("ms-settings:nightlight"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Mode gelap", glyph: 0xE793, kind: QK_DARK, st: QST_DARK, uri: Some("ms-settings:colors"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Tampilan", glyph: 0xE7F4, kind: QK_URI, st: QST_NONE, uri: Some("ms-settings:display"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Transmisikan", glyph: 0xE7F3, kind: QK_KEYS, st: QST_CAST, uri: None, k1: QS_VK_LWIN, k2: b'K' as u16, k3: 0 },
    QsTileDef { label: "Proyeksikan", glyph: 0xE7F8, kind: QK_KEYS, st: QST_NONE, uri: None, k1: QS_VK_LWIN, k2: b'P' as u16, k3: 0 },
    QsTileDef { label: "Jangan ganggu", glyph: 0xE7ED, kind: QK_DND, st: QST_DND, uri: Some("ms-settings:quiethours"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Aksesibilitas", glyph: 0xE776, kind: QK_URI, st: QST_NONE, uri: Some("ms-settings:easeofaccess"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Berbagi sekitar", glyph: 0xE72D, kind: QK_NEARBY, st: QST_NEARBY, uri: Some("ms-settings:crossdevice"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Perangkat", glyph: 0xE770, kind: QK_URI, st: QST_NONE, uri: Some("ms-settings:devices"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Hemat energi", glyph: 0, kind: QK_SAVER, st: QST_SAVER, uri: Some("ms-settings:batterysaver"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Mikrofon", glyph: 0xE720, kind: QK_MIC, st: QST_MIC, uri: Some("ms-settings:privacy-microphone"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Lokasi", glyph: 0xE81D, kind: QK_LOC, st: QST_LOC, uri: Some("ms-settings:privacy-location"), k1: 0, k2: 0, k3: 0 },
    QsTileDef { label: "Teks langsung", glyph: 0xE7F0, kind: QK_KEYS, st: QST_NONE, uri: None, k1: QS_VK_LWIN, k2: QS_VK_CONTROL, k3: b'L' as u16 },
];

// Status Pengaturan Cepat (w12qs.inc:40-52). Hanya diakses dari thread UI, kecuali BriJob.
static QS_DARK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static QS_SAVER: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static QS_AIRPLANE: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static QS_HOTSPOT: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static QS_DND: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static QS_TOAST_MSG: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
static QS_TOAST_EXPIRY: SyncCell<Cell<u64>> = SyncCell::new(Cell::new(0));
static QS_TRK_L: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
static QS_TRK_R: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
static QS_OUT: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));
static QS_VOL_T: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
static QS_VOL_B: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
static QS_BRI_T: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
static QS_BRI_B: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
/// Kecerahan 0..100 yang ditampilkan; -1 = tidak didukung (g_bri di C++).
static BRI_VAL: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
/// Pekerja kecerahan aktif; hanya selama Pengaturan Cepat terbuka (g_briJob di C++).
static BRI_JOB: SyncCell<RefCell<Option<Arc<BriJob>>>> = SyncCell::new(RefCell::new(None));
/// Generasi pekerja; pesan dari pekerja lama diabaikan (g_briGen di C++).
static BRI_GEN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
/// Status Bluetooth dari pekerja radio (RAD_BT di C++); -1 = belum dibaca.
static RAD_BT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));

/// Skala int dip ke px (S(v) di C++).
fn sd(v: i32) -> i32 {
    s(v as f32)
}

/// Skala float dip ke px (SF(v) di C++).
fn sdf(v: i32) -> f32 {
    sf(v as f32)
}

fn qs_bri_h() -> i32 {
    if BRI_VAL.get() >= 0 {
        sd(4) + sd(QS_ROWH)
    } else {
        0
    }
}

fn qs_vp_top() -> i32 {
    sd(QS_PAD) + sd(QS_HEADH) + sd(QS_VGAP)
}

fn qs_vp_bot() -> i32 {
    qs_vp_top() + sd(QS_VPH)
}

fn qs_content_h() -> i32 {
    QS_TROWS * sd(QS_TILEH) + (QS_TROWS - 1) * sd(QS_TGAP)
}

fn qs_max_scroll() -> i32 {
    (qs_content_h() - sd(QS_VPH)).max(0)
}

fn qs_clamp_scroll() {
    let m = qs_max_scroll();
    QS_SCROLL.set(QS_SCROLL.get().clamp(0, m));
}

/// Pekerja kecerahan (BriJob di C++). Dipakai bersama UI dan thread pekerja lewat Arc; handle wake
/// ditutup saat Arc terakhir lepas.
struct BriJob {
    wake: HANDLE,
    notify: HWND,
    gen: i32,
    target: AtomicI32,
    stop: AtomicBool,
    drag: AtomicBool,
}

// SAFETY: HANDLE dan HWND hanya dipakai lewat API Win32; seluruh status lain atomik.
unsafe impl Send for BriJob {}
unsafe impl Sync for BriJob {}

impl Drop for BriJob {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.wake) };
    }
}

/// Parameter WmiSetBrightness yang dipakai ulang pekerja (BriSetter di C++).
struct BriSetter {
    path: Option<BSTR>,
    input: Option<IWbemClassObject>,
}

/// Koneksi WMI ke ROOT\WMI (BriConnect di C++).
fn bri_connect() -> Option<IWbemServices> {
    let loc: IWbemLocator =
        unsafe { CoCreateInstance(&WbemLocator, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }.ok()?;
    let svc = unsafe {
        loc.ConnectServer(
            &BSTR::from("ROOT\\WMI"),
            &BSTR::new(),
            &BSTR::new(),
            &BSTR::new(),
            0,
            &BSTR::new(),
            None::<&IWbemContext>,
        )
    }
    .ok()?;
    // RPC_C_AUTHN_WINNT (10) dan RPC_C_AUTHZ_NONE (0) ditulis sebagai literal; fitur Win32_System_Rpc belum aktif.
    let _ = unsafe {
        CoSetProxyBlanket(
            &svc,
            10,
            0,
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )
    };
    Some(svc)
}

/// Objek pertama hasil query WQL (BriFirst di C++); None bila kosong atau gagal.
fn bri_first(svc: &IWbemServices, wql: &str) -> Option<IWbemClassObject> {
    let lang = BSTR::from("WQL");
    let q = BSTR::from(wql);
    let en: IEnumWbemClassObject = unsafe {
        svc.ExecQuery(
            &lang,
            &q,
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None::<&IWbemContext>,
        )
    }
    .ok()?;
    let mut arr: [Option<IWbemClassObject>; 1] = [None];
    let mut got: u32 = 0;
    let hr = unsafe { en.Next(800, &mut arr, &mut got as *mut u32) };
    if hr.0 == WBEM_S_NO_ERROR.0 && got == 1 {
        arr[0].take()
    } else {
        None
    }
}

/// Kecerahan sebenarnya 0..100; -1 bila tidak didukung (BriGet di C++).
fn bri_get(svc: &IWbemServices) -> i32 {
    let Some(o) = bri_first(svc, "SELECT CurrentBrightness FROM WmiMonitorBrightness WHERE Active=TRUE") else {
        return -1;
    };
    let mut v = unsafe { VariantInit() };
    let mut r = -1;
    if unsafe { o.Get(w!("CurrentBrightness"), 0, &mut v, None, None) }.is_ok() {
        let vt = unsafe { v.Anonymous.Anonymous.vt };
        r = if vt == VT_UI1 {
            unsafe { v.Anonymous.Anonymous.Anonymous.bVal as i32 }
        } else if vt == VT_I4 {
            unsafe { v.Anonymous.Anonymous.Anonymous.lVal }
        } else if vt == VT_I2 {
            unsafe { v.Anonymous.Anonymous.Anonymous.iVal as i32 }
        } else {
            -1
        };
    }
    let _ = unsafe { VariantClear(&mut v) };
    r.min(100)
}

/// Jalur instance WmiMonitorBrightnessMethods (bagian path di BriSet C++).
fn bri_path(svc: &IWbemServices) -> Option<BSTR> {
    let o = bri_first(svc, "SELECT * FROM WmiMonitorBrightnessMethods WHERE Active=TRUE")?;
    let mut p = unsafe { VariantInit() };
    let mut out = None;
    if unsafe { o.Get(w!("__PATH"), 0, &mut p, None, None) }.is_ok()
        && unsafe { p.Anonymous.Anonymous.vt } == VT_BSTR
    {
        let b = unsafe { (*p.Anonymous.Anonymous.Anonymous.bstrVal).clone() };
        if b.len() > 0 {
            out = Some(b);
        }
    }
    let _ = unsafe { VariantClear(&mut p) };
    out
}

/// Objek parameter WmiSetBrightness hasil SpawnInstance (bagian input di BriSet C++).
fn bri_input(svc: &IWbemServices) -> Option<IWbemClassObject> {
    let cn = BSTR::from("WmiMonitorBrightnessMethods");
    let mut cls: Option<IWbemClassObject> = None;
    unsafe {
        svc.GetObject(
            &cn,
            WBEM_GENERIC_FLAG_TYPE(0),
            None::<&IWbemContext>,
            Some(&mut cls as *mut Option<IWbemClassObject>),
            None,
        )
    }
    .ok()?;
    let cls = cls?;
    let mut def: Option<IWbemClassObject> = None;
    unsafe {
        cls.GetMethod(
            w!("WmiSetBrightness"),
            0,
            &mut def as *mut Option<IWbemClassObject>,
            std::ptr::null_mut(),
        )
    }
    .ok()?;
    let def = def?;
    unsafe { def.SpawnInstance(0).ok() }
}

/// Menulis kecerahan 0..100 lewat WMI (BriSet di C++).
fn bri_set(svc: &IWbemServices, st: &mut BriSetter, v: i32) -> bool {
    let v = v.clamp(0, 100);
    if st.path.is_none() {
        st.path = bri_path(svc);
    }
    if st.input.is_none() {
        st.input = bri_input(svc);
    }
    let (Some(path), Some(inp)) = (st.path.as_ref(), st.input.as_ref()) else {
        return false;
    };
    let mut a = unsafe { VariantInit() };
    unsafe {
        // PORT: field union ManuallyDrop tidak di-DerefMut otomatis; deref eksplisit.
        (*a.Anonymous.Anonymous).vt = VT_I4;
        (*a.Anonymous.Anonymous).Anonymous.lVal = 0; // Timeout 0: berlaku langsung
    }
    let mut b = unsafe { VariantInit() };
    unsafe {
        (*b.Anonymous.Anonymous).vt = VT_UI1;
        (*b.Anonymous.Anonymous).Anonymous.bVal = v as u8;
    }
    let _ = unsafe { inp.Put(w!("Timeout"), 0, &a, 0) };
    let _ = unsafe { inp.Put(w!("Brightness"), 0, &b, 0) };
    let mn = BSTR::from("WmiSetBrightness");
    unsafe {
        svc.ExecMethod(
            path,
            &mn,
            WBEM_GENERIC_FLAG_TYPE(0),
            None::<&IWbemContext>,
            inp,
            None,
            None,
        )
        .is_ok()
    }
}

/// Lingkaran pekerja (BriThread di C++): mengejar target dengan ease-out, lalu membaca nilai sebenarnya tiap detik.
fn bri_run(svc: &IWbemServices, j: &BriJob) {
    // Dideklarasi setelah svc: saat fungsi selesai, st lepas lebih dulu (urutan sama dengan C++).
    let mut st = BriSetter { path: None, input: None };
    let mut cur = bri_get(svc);
    let mut last_set = unsafe { GetTickCount64() };
    while cur >= 0 {
        let t = j.target.load(Ordering::SeqCst);
        if j.stop.load(Ordering::SeqCst) {
            // Nilai akhir selalu diterapkan sebelum berhenti.
            if t >= 0 && t != cur {
                bri_set(svc, &mut st, t);
            }
            break;
        }
        if t >= 0 && t != cur {
            // Ease-out: separuh sisa jarak tiap langkah (minimal 2 poin).
            let d = t - cur;
            let a = d.abs();
            let m = if a <= 2 { a } else { (a / 2).max(2) };
            let nv = if d < 0 { cur - m } else { cur + m };
            // Gagal: jangan mengulang terus; pembacaan berikut meluruskan.
            cur = if bri_set(svc, &mut st, nv) { nv } else { t };
            last_set = unsafe { GetTickCount64() };
            let _ = unsafe { WaitForSingleObject(j.wake, 12) };
            continue;
        }
        let _ = unsafe { WaitForSingleObject(j.wake, 1000) };
        let now = unsafe { GetTickCount64() };
        if j.stop.load(Ordering::SeqCst)
            || j.drag.load(Ordering::SeqCst)
            || now.saturating_sub(last_set) < 600
        {
            continue;
        }
        let t0 = j.target.load(Ordering::SeqCst);
        if t0 >= 0 && t0 != cur {
            continue; // permintaan baru masuk: kerjakan dulu
        }
        let r = bri_get(svc);
        if r >= 0
            && r != cur
            && j.target
                .compare_exchange(t0, -1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            cur = r;
            if !j.stop.load(Ordering::SeqCst) {
                let _ = unsafe {
                    PostMessageW(Some(j.notify), WM_W12_BRI, WPARAM(r as usize), LPARAM(j.gen as isize))
                };
            }
        }
    }
}

/// Titik masuk thread pekerja (BriThread di C++); Arc dilepas di akhir.
fn bri_thread(job: Arc<BriJob>) {
    if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok() {
        if let Some(svc) = bri_connect() {
            bri_run(&svc, &job);
        }
        unsafe { CoUninitialize() };
    }
}

/// Menyalakan pekerja bila belum ada dan kecerahan didukung (QsBriStart di C++).
fn qs_bri_start() {
    if BRI_JOB.borrow().is_some() || BRI_VAL.get() < 0 || PAN.get().0.is_null() {
        return;
    }
    let wake = match unsafe { CreateEventW(None, false, false, PCWSTR::null()) } {
        Ok(h) => h,
        Err(_) => return,
    };
    let gen = BRI_GEN.get() + 1;
    BRI_GEN.set(gen);
    let job = Arc::new(BriJob {
        wake,
        notify: PAN.get(),
        gen,
        target: AtomicI32::new(-1),
        stop: AtomicBool::new(false),
        drag: AtomicBool::new(false),
    });
    let worker = Arc::clone(&job);
    // Gagal membuat thread: closure dan Arc terlepas, BriJob menutup handle wake.
    if std::thread::Builder::new()
        .name("w12-bri".into())
        .spawn(move || bri_thread(worker))
        .is_err()
    {
        return;
    }
    *BRI_JOB.borrow_mut() = Some(job);
}

/// Membaca kecerahan awal lalu menyiapkan pekerja (QsBriRead di C++).
fn qs_bri_read() {
    BRI_VAL.set(-1);
    let Some(svc) = bri_connect() else {
        return;
    };
    BRI_VAL.set(bri_get(&svc));
    qs_bri_start();
}

/// Permintaan dari UI: tampilan langsung berubah, pekerja menyusul di belakang (QsBriSet di C++).
fn qs_bri_set(v: i32) {
    if BRI_VAL.get() < 0 {
        return;
    }
    let nv = v.clamp(0, 100);
    BRI_VAL.set(nv);
    if BRI_JOB.borrow().is_none() {
        qs_bri_start();
    }
    let job = BRI_JOB.borrow().as_ref().cloned();
    let Some(job) = job else {
        return;
    };
    job.target.store(nv, Ordering::SeqCst);
    let _ = unsafe { SetEvent(job.wake) };
}

/// Nilai sebenarnya dari pekerja (WM_W12_BRI); diabaikan saat diseret (QsBriNotify di C++).
pub fn qs_bri_notify(wp: WPARAM, lp: LPARAM) {
    if PANEL_OPEN.get() != PN_QS || PAN_DRAG.get() {
        return;
    }
    let gen_ok = match BRI_JOB.borrow().as_ref() {
        Some(j) => j.gen == lp.0 as i32,
        None => false,
    };
    if !gen_ok {
        return;
    }
    BRI_VAL.set(wp.0 as i32);
    w12_invalidate(PAN.get(), None, false);
}

/// Menghentikan pekerja saat Pengaturan Cepat ditutup; UI tidak menunggu (QsRelease di C++).
pub fn qs_release() {
    let job = BRI_JOB.borrow_mut().take();
    if let Some(job) = job {
        job.stop.store(true, Ordering::SeqCst);
        let _ = unsafe { SetEvent(job.wake) };
    }
    BRI_VAL.set(-1);
}

// ---------------------------------------------------------------- lampu malam, mikrofon, lokasi, berbagi sekitar, toggle

const QS_NIGHT_SUB: &str = r"Software\Microsoft\Windows\CurrentVersion\CloudStore\Store\DefaultAccount\Current\default$windows.data.bluelightreduction.bluelightreductionstate\windows.data.bluelightreduction.bluelightreductionstate";
const QS_MIC_SUB: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
const QS_LOC_SUB: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location";
const QS_CDP_SUB: &str = r"Software\Microsoft\Windows\CurrentVersion\CDP";
const QS_THEME_SUB: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// Data biner lampu malam (nilai `Data`); `None` bila kunci atau nilai tidak ada (bagian awal QsIsNightLightOn di C++).
fn qs_night_data() -> Option<Vec<u8>> {
    let sub = to_wide_z(QS_NIGHT_SUB);
    let nm = to_wide_z("Data");
    let mut buf = vec![0u8; 256];
    let mut cb: u32 = buf.len() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            pcw(&sub),
            pcw(&nm),
            RRF_RT_REG_BINARY,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut cb),
        )
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    buf.truncate(cb as usize);
    Some(buf)
}

/// Lampu malam menyala bila byte ke-18 bernilai 0x13 (QsIsNightLightOn di C++).
fn qs_is_night_light_on() -> bool {
    qs_night_data().is_some_and(|d| d.len() > 18 && d[18] == 0x13)
}

/// Menukar byte ke-18 antara 0x13 (nyala) dan 0x15 (mati). Nilai harus sudah ada (QsToggleNightLight di C++).
fn qs_toggle_night_light() {
    let Some(mut data) = qs_night_data() else {
        return;
    };
    if data.len() <= 18 {
        return;
    }
    data[18] = if data[18] == 0x13 { 0x15 } else { 0x13 };
    let sub = to_wide_z(QS_NIGHT_SUB);
    let nm = to_wide_z("Data");
    let _ = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            pcw(&sub),
            pcw(&nm),
            REG_BINARY.0,
            Some(data.as_ptr() as *const c_void),
            data.len() as u32,
        )
    };
}

/// Izin akses perangkat: `Deny` berarti diblokir; kunci atau nilai yang hilang berarti diizinkan (QsIsMicOn/QsIsLocOn di C++).
fn qs_consent_allowed(sub: &str) -> bool {
    match reg::get_string(sub, "Value") {
        Some(v) => !v.trim_end_matches('\0').eq_ignore_ascii_case("Deny"),
        None => true,
    }
}

/// Mengganti izin: bila diizinkan tulis `Deny`, bila diblokir tulis `Allow` (QsToggleMic/QsToggleLoc di C++).
fn qs_toggle_consent(sub: &str) {
    let val = if qs_consent_allowed(sub) { "Deny" } else { "Allow" };
    reg::set_string(sub, "Value", val);
}

/// Berbagi sekitar aktif bila DWORD `NearShareChannelUserAuthzPolicy` tidak nol (QsIsNearbyOn di C++).
fn qs_is_nearby_on() -> bool {
    reg::get_dword(QS_CDP_SUB, "NearShareChannelUserAuthzPolicy").is_some_and(|v| v != 0)
}

fn qs_toggle_nearby() {
    let nv: u32 = if qs_is_nearby_on() { 0 } else { 1 };
    reg::set_dword(QS_CDP_SUB, "NearShareChannelUserAuthzPolicy", nv);
}

/// Layar sedang diproyeksikan (teknologi 15/17) atau sesi jarak jauh (QsIsCastingActive di C++).
fn qs_is_casting_active() -> bool {
    if unsafe { GetSystemMetrics(SM_REMOTESESSION) } != 0 {
        return true;
    }
    let mut path_count: u32 = 0;
    let mut mode_count: u32 = 0;
    let rc = unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count) };
    if rc != ERROR_SUCCESS || path_count == 0 {
        return false;
    }
    // Buffer dinolkan; ukurannya dari GetDisplayConfigBufferSizes (pengganti malloc di C++).
    let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = (0..path_count).map(|_| unsafe { std::mem::zeroed() }).collect();
    let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = (0..mode_count).map(|_| unsafe { std::mem::zeroed() }).collect();
    let rc = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if rc != ERROR_SUCCESS {
        return false;
    }
    paths
        .iter()
        .take(path_count as usize)
        .any(|p| matches!(p.targetInfo.outputTechnology.0, 15 | 17))
}

fn qs_toggle_airplane() {
    let on = !QS_AIRPLANE.get();
    QS_AIRPLANE.set(on);
    if on {
        rad_start(1, false);
        rad_start(3, false);
        qs_show_toast("Mode pesawat: Aktif");
    } else {
        rad_start(1, true);
        qs_show_toast("Mode pesawat: Nonaktif");
    }
}

fn qs_toggle_hotspot() {
    let on = !QS_HOTSPOT.get();
    QS_HOTSPOT.set(on);
    qs_show_toast(if on { "Hotspot seluler: Aktif" } else { "Hotspot seluler: Nonaktif" });
}

fn qs_toggle_dnd() {
    let on = !QS_DND.get();
    QS_DND.set(on);
    qs_show_toast(if on { "Jangan ganggu: Aktif" } else { "Jangan ganggu: Nonaktif" });
}

fn qs_toggle_saver() {
    let on = !QS_SAVER.get();
    QS_SAVER.set(on);
    qs_show_toast(if on { "Hemat energi: Aktif" } else { "Hemat energi: Nonaktif" });
}

/// Mode gelap/terang lewat HKCU Themes\Personalize, lalu siarkan ImmersiveColorSet (QsToggleDark di C++).
fn qs_toggle_dark() {
    let dark = !QS_DARK.get();
    let v: u32 = if dark { 0 } else { 1 };
    if !reg::set_dword(QS_THEME_SUB, "AppsUseLightTheme", v) {
        return;
    }
    reg::set_dword(QS_THEME_SUB, "SystemUsesLightTheme", v);
    QS_DARK.set(dark);
    let name = to_wide_z("ImmersiveColorSet");
    let mut res: usize = 0;
    let _ = unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(name.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            200,
            Some(&mut res),
        )
    };
}

/// Bisukan master: membaca status lalu membalik (QsToggleMute di C++).
pub(crate) fn qs_toggle_mute() {
    let ep = EP_VOL.borrow().clone();
    let Some(ep) = ep else {
        return;
    };
    let m = unsafe { ep.GetMute() }.map(|b| b.as_bool()).unwrap_or(false);
    let _ = unsafe { ep.SetMute(!m, std::ptr::null()) };
    MUTE.set(!m);
}

/// Seret slider: `lx` dalam koordinat isi (panel sudah mengurangi margin); PAN_DOWN menentukan slider (QsDrag di C++).
pub fn qs_drag(lx: f32) {
    let (l, r) = (QS_TRK_L.get(), QS_TRK_R.get());
    if r <= l {
        return;
    }
    let f = ((lx - l) / (r - l)).clamp(0.0, 1.0);
    if PAN_DOWN.get() == PAN_ID_BRI_TRACK {
        if BRI_VAL.get() < 0 {
            return;
        }
        if let Some(job) = BRI_JOB.borrow().as_ref() {
            // Pekerja tidak membaca balik selama diseret.
            job.drag.store(true, Ordering::SeqCst);
        }
        qs_bri_set((f * 100.0).round() as i32);
    } else {
        let ep = EP_VOL.borrow().clone();
        let Some(ep) = ep else {
            return;
        };
        let _ = unsafe { ep.SetMasterVolumeLevelScalar(f, std::ptr::null()) };
        let v = (f * 100.0).round() as i32;
        VOL.set(v);
        if MUTE.get() && v > 0 {
            let _ = unsafe { ep.SetMute(false, std::ptr::null()) };
            MUTE.set(false);
        }
    }
    w12_invalidate(PAN.get(), None, false);
}

/// Akhir seret slider kecerahan: pekerja membaca nilai akhir setelah diam (QsDragEnd di C++).
pub fn qs_drag_end() {
    if PAN_DOWN.get() != PAN_ID_BRI_TRACK {
        return;
    }
    if let Some(job) = BRI_JOB.borrow().as_ref() {
        job.drag.store(false, Ordering::SeqCst);
        let _ = unsafe { SetEvent(job.wake) };
    }
}

// ---------------------------------------------------------------- radio Wi-Fi dan Bluetooth (Windows.Devices.Radios)
// Semua panggilan WinRT di thread pekerja MTA; async ditunggu dengan polling singkat. Hasil dikirim sebagai WM_W12_RADIO.

/// Menunggu operasi async selesai, maks ±6 detik; `true` bila berhasil (RadWait di C++).
fn rad_wait<T: Interface>(op: &T) -> bool {
    for _ in 0..300 {
        let st = async_status(op);
        if st == 1 {
            return true;
        }
        if st >= 2 {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

/// Isi pekerja radio: minta akses bila perlu, ubah radio jenis `kind`, lalu baca status (RadThread di C++).
fn rad_work(kind: i32, on: bool, wifi: &mut i32, bt: &mut i32) {
    if kind != 0 {
        if let Ok(op) = Radio::RequestAccessAsync() {
            rad_wait(&op);
        }
    }
    let Ok(op) = Radio::GetRadiosAsync() else {
        return;
    };
    if !rad_wait(&op) {
        return;
    }
    let Ok(radios) = op.GetResults() else {
        return;
    };
    let n = radios.Size().unwrap_or(0);
    for i in 0..n {
        let Ok(r) = radios.GetAt(i) else {
            continue;
        };
        let k = r.Kind().map(|k| k.0).unwrap_or(0);
        if k == kind {
            if let Ok(so) = r.SetStateAsync(RadioState(if on { 1 } else { 2 })) {
                rad_wait(&so);
            }
        }
        let rs = r.State().map(|v| v.0).unwrap_or(0);
        if k == 1 {
            *wifi = if *wifi == 1 || rs == 1 { 1 } else { 0 };
        } else if k == 3 {
            *bt = if *bt == 1 || rs == 1 { 1 } else { 0 };
        }
    }
}

/// Pekerja radio: hasil (Wi-Fi, Bluetooth) dikirim ke jendela utama; +1 agar -1 (tidak diketahui) tetap muat di WPARAM.
fn rad_thread(kind: i32, on: bool) {
    let mut wifi = -1;
    let mut bt = -1;
    if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok() {
        rad_work(kind, on, &mut wifi, &mut bt);
        unsafe { RoUninitialize() };
    }
    let posted = unsafe {
        PostMessageW(
            Some(MSG.get()),
            WM_W12_RADIO,
            WPARAM((wifi + 1) as usize),
            LPARAM((bt + 1) as isize),
        )
    };
    if posted.is_err() {
        RAD_BUSY.store(0, Ordering::SeqCst);
    }
}

/// Memulai pekerja radio: `kind` 0 hanya baca status, 1 Wi-Fi, 3 Bluetooth. `false` bila pekerja masih sibuk (RadStart di C++; dipakai flyout.rs).
pub fn rad_start(kind: i32, on: bool) -> bool {
    if RAD_BUSY.compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return false;
    }
    let spawned = std::thread::Builder::new()
        .name("w12-radio".into())
        .spawn(move || rad_thread(kind, on))
        .is_ok();
    if !spawned {
        RAD_BUSY.store(0, Ordering::SeqCst);
        return false;
    }
    if kind != 0 {
        RAD_PEND_KIND.set(kind);
        RAD_PEND_ON.set(on as i32);
    }
    true
}

/// Pesan notch saat radio berubah. Hanya bila status lama dan baru sudah diketahui (0 = mati, 1 = aktif, -1 = tidak diketahui).
/// Deviasi: C++ tidak punya pesan ini. Pembacaan gagal (qs.rs rad_work) terbaca sebagai mati, jadi bisa muncul pesan palsu.
fn radio_toast(was: i32, now: i32, nama: &str, glyph: u16) {
    if (was == 0 || was == 1) && (now == 0 || now == 1) && was != now {
        let teks = format!("{nama} {}", if now == 1 { "aktif" } else { "nonaktif" });
        super::hotkeys::hk_toast(&teks, glyph);
    }
}

/// Hasil pekerja radio (WM_W12_RADIO), dipanggil dari WndProc (QsRadioNotify di C++).
pub fn qs_radio_notify(wp: WPARAM, lp: LPARAM) {
    let was = RAD_WIFI.get();
    let bt_was = RAD_BT.get();
    RAD_WIFI.set(wp.0 as i32 - 1);
    RAD_BT.set(lp.0 as i32 - 1);
    RAD_PEND_KIND.set(0);
    RAD_BUSY.store(0, Ordering::SeqCst);
    wifi_radio_changed(was);
    radio_toast(was, RAD_WIFI.get(), "Wi-Fi", 0xE701);
    radio_toast(bt_was, RAD_BT.get(), "Bluetooth", 0xE702);
    let open = PANEL_OPEN.get();
    if (open == PN_QS || open == PN_WIFI) && !PAN.get().0.is_null() {
        w12_invalidate(PAN.get(), None, false);
    }
}

// ---------------------------------------------------------------- aksi ubin dan elemen

/// Aksi ketuk ubin (QsTileAct di C++).
fn qs_tile_act(d: &QsTileDef) {
    match d.kind {
        QK_DARK => {
            qs_toggle_dark();
            qs_show_toast(if QS_DARK.get() { "Mode gelap: Aktif" } else { "Mode gelap: Nonaktif" });
        }
        QK_AIRPLANE => qs_toggle_airplane(),
        QK_HOTSPOT => qs_toggle_hotspot(),
        QK_NIGHT => {
            qs_toggle_night_light();
            qs_show_toast(if qs_is_night_light_on() { "Lampu malam: Aktif" } else { "Lampu malam: Nonaktif" });
        }
        QK_DND => qs_toggle_dnd(),
        QK_NEARBY => {
            qs_toggle_nearby();
            qs_show_toast(if qs_is_nearby_on() { "Berbagi sekitar: Aktif" } else { "Berbagi sekitar: Nonaktif" });
        }
        QK_SAVER => qs_toggle_saver(),
        QK_MIC => {
            qs_toggle_consent(QS_MIC_SUB);
            qs_show_toast(if qs_consent_allowed(QS_MIC_SUB) { "Akses mikrofon: Diizinkan" } else { "Akses mikrofon: Diblokir" });
        }
        QK_LOC => {
            qs_toggle_consent(QS_LOC_SUB);
            qs_show_toast(if qs_consent_allowed(QS_LOC_SUB) { "Akses lokasi: Diizinkan" } else { "Akses lokasi: Diblokir" });
        }
        QK_KEYS => pan_keys(d.k1, d.k2, d.k3),
        QK_RADIO => {
            let known = if d.k1 == 1 { RAD_WIFI.get() } else { RAD_BT.get() };
            // Radio tidak ditemukan: buka pengaturannya.
            if known < 0 && RAD_BUSY.load(Ordering::SeqCst) == 0 {
                if let Some(u) = d.uri {
                    pan_uri(u);
                }
                return;
            }
            let will_be_on = !qs_on(d);
            // PORT: k1 bertipe u16 di QsTileDef, rad_start menerima i32.
            if rad_start(d.k1 as i32, will_be_on) {
                if d.k1 == 1 {
                    qs_show_toast(if will_be_on { "Wi-Fi: Menyala" } else { "Wi-Fi: Dimatikan" });
                } else if d.k1 == 3 {
                    qs_show_toast(if will_be_on { "Bluetooth: Menyala" } else { "Bluetooth: Dimatikan" });
                }
                w12_invalidate(PAN.get(), None, false);
            }
        }
        _ => {
            if let Some(u) = d.uri {
                pan_uri(u);
            }
        }
    }
}

/// `id` termasuk rentang ubin Pengaturan Cepat (QsIsTile di C++).
pub fn qs_is_tile(id: i32) -> bool {
    id >= PAN_ID_QS_TILE && (id - PAN_ID_QS_TILE) < QS_NTILE as i32
}

/// Tahan ubin: buka halaman pengaturannya; ubin tanpa halaman menjalankan aksinya (QsTileHold di C++).
pub fn qs_tile_hold(id: i32) {
    if !qs_is_tile(id) {
        return;
    }
    let d = &G_QS_TILES[(id - PAN_ID_QS_TILE) as usize];
    match d.uri {
        Some(u) => pan_uri(u),
        None => qs_tile_act(d),
    }
}

/// Klik pada elemen Pengaturan Cepat (QsActivate di C++).
pub fn qs_activate(id: i32) {
    if qs_is_tile(id) {
        qs_tile_act(&G_QS_TILES[(id - PAN_ID_QS_TILE) as usize]);
        return;
    }
    match id {
        PAN_ID_VOL_MUTE => qs_toggle_mute(),
        PAN_ID_QS_SET => pan_uri("ms-settings:"),
        PAN_ID_QS_AUDIO => pan_uri("ms-settings:sound"),
        PAN_ID_BRI_ICON => pan_uri("ms-settings:display"), // kecerahan, lampu malam, dan HDR
        _ => {} // PAN_ID_VOL_TRACK dan PAN_ID_BRI_TRACK: klik pada trek sudah ditangani qs_drag
    }
}

// ---------------------------------------------------------------- audio, ukuran, roda, dan pop-up

/// Nama perangkat keluaran audio bawaan (QsAudioName di C++). Mengganti perangkat tidak punya API publik,
/// jadi baris ini hanya menampilkan nama lalu membuka Pengaturan suara. Maks 127 unit UTF-16.
fn qs_audio_name() {
    QS_OUT.borrow_mut().clear();
    let Some(en) = DEV_ENUM.borrow().clone() else {
        return;
    };
    let Ok(dev) = (unsafe { en.GetDefaultAudioEndpoint(eRender, eConsole) }) else {
        return;
    };
    let Ok(ps) = (unsafe { dev.OpenPropertyStore(STGM_READ) }) else {
        return;
    };
    if let Ok(mut v) = unsafe { ps.GetValue(&PKEY_Device_FriendlyName) } {
        unsafe {
            let p = v.Anonymous.Anonymous.Anonymous.pwszVal.0;
            if v.Anonymous.Anonymous.vt.0 == VT_LPWSTR.0 && !p.is_null() {
                let mut len = 0usize;
                while len < 127 && *p.add(len) != 0 {
                    len += 1;
                }
                *QS_OUT.borrow_mut() = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
            }
            let _ = PropVariantClear(&mut v);
        }
    }
}

/// Ukuran konten Pengaturan Cepat (QsMeasure di C++): tema, hemat daya, kecerahan, dan nama audio dibaca dulu.
pub fn qs_measure(cw: &mut i32, ch: &mut i32) {
    *cw = sd(QS_W);
    QS_DARK.set(!windows_uses_light());
    let mut sps = SYSTEM_POWER_STATUS::default();
    let ok = unsafe { GetSystemPowerStatus(&mut sps) }.is_ok();
    QS_SAVER.set(ok && (sps.SystemStatusFlag & 1) != 0);
    qs_bri_read();
    qs_audio_name();
    rad_start(0, false); // status radio dibaca di latar; ubin diperbarui saat hasilnya tiba
    *ch = qs_vp_bot()
        + sd(QS_VGAP)
        + 1
        + sd(QS_VGAP)
        + sd(QS_ROWH)
        + qs_bri_h()
        + sd(QS_VGAP)
        + sd(QS_AUDH)
        + sd(QS_PAD);
    qs_clamp_scroll();
}

/// Akumulator roda tetikus untuk volume dan kecerahan (`static int acc` di QsWheel C++).
static QS_WHEEL_ACC: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));

/// Roda tetikus (QsWheel di C++). Di atas baris Volume/Kecerahan mengubah nilai 2% per takik (touchpad presisi
/// diakumulasi), di tempat lain menggulir ubin. `ly` = y dalam koordinat isi.
pub fn qs_wheel(delta: i32, ly: i32) {
    let y = ly as f32;
    let on_vol = y >= QS_VOL_T.get() && y < QS_VOL_B.get();
    let on_bri = BRI_VAL.get() >= 0 && y >= QS_BRI_T.get() && y < QS_BRI_B.get();
    if on_vol || on_bri {
        let unit = WHEEL_DELTA as i32 / 2;
        let acc = QS_WHEEL_ACC.get() + delta;
        let d = acc / unit;
        QS_WHEEL_ACC.set(acc - d * unit);
        if d == 0 {
            return;
        }
        if on_bri {
            qs_bri_set(BRI_VAL.get() + d);
            return;
        }
        let Some(ep) = EP_VOL.borrow().clone() else {
            return;
        };
        let v = (VOL.get() + d).clamp(0, 100);
        let _ = unsafe { ep.SetMasterVolumeLevelScalar(v as f32 / 100.0, std::ptr::null()) };
        VOL.set(v);
        if MUTE.get() && v > 0 {
            let _ = unsafe { ep.SetMute(false, std::ptr::null()) };
            MUTE.set(false);
        }
        return;
    }
    // Satu takik = 40 dip; MulDiv(delta, 40, WHEEL_DELTA) dibulatkan ke terdekat (setengah menjauh dari nol).
    let steps = (delta as f64 * sd(40) as f64 / WHEEL_DELTA as f64).round() as i32;
    QS_SCROLL.set(QS_SCROLL.get() - steps);
    qs_clamp_scroll();
    PAN_HOV.set(-1); // kotak klik lama tidak lagi sesuai posisi gulir
}

/// Pop-up konfirmasi (QsShowToast di C++): teks maks 63 karakter, kedaluwarsa 1800 ms, timer 1900 ms.
fn qs_show_toast(msg: &str) {
    *QS_TOAST_MSG.borrow_mut() = msg.chars().take(63).collect();
    QS_TOAST_EXPIRY.set(unsafe { GetTickCount64() } + QS_TOAST_LIFE_MS);
    if !PAN.get().0.is_null() {
        unsafe { SetTimer(Some(PAN.get()), QS_TOAST_TIMER, QS_TOAST_TIMER_MS, None) };
        w12_invalidate(PAN.get(), None, false);
    }
}

// ---------------------------------------------------------------- gambar

/// Status aktif sebuah ubin (QsOn di C++). Ubin radio memakai hasil pekerja bila sedang berjalan.
fn qs_on(d: &QsTileDef) -> bool {
    match d.st {
        QST_WIFI => {
            if RAD_PEND_KIND.get() == 1 {
                return RAD_PEND_ON.get() != 0;
            }
            let w = RAD_WIFI.get();
            if w >= 0 {
                w == 1
            } else {
                NET.get() == NET_WIFI
            }
        }
        QST_BT => {
            if RAD_PEND_KIND.get() == 3 {
                return RAD_PEND_ON.get() != 0;
            }
            RAD_BT.get() == 1
        }
        QST_DARK => QS_DARK.get(),
        QST_SAVER => QS_SAVER.get(),
        QST_AIRPLANE => QS_AIRPLANE.get(),
        QST_HOTSPOT => QS_HOTSPOT.get(),
        QST_NIGHT => qs_is_night_light_on(),
        QST_DND => QS_DND.get(),
        QST_NEARBY => qs_is_nearby_on(),
        QST_MIC => qs_consent_allowed(QS_MIC_SUB),
        QST_LOC => qs_consent_allowed(QS_LOC_SUB),
        QST_CAST => qs_is_casting_active(),
        _ => false,
    }
}

/// Satu baris slider (QsSlider di C++): ikon (bisa diklik bila `icon_id` >= 0), label, trek, dan persen.
#[allow(clippy::too_many_arguments)]
fn qs_slider(
    cw: i32,
    t: i32,
    b: i32,
    glyph_ch: u16,
    icon_id: i32,
    label: &str,
    val: i32,
    dim: bool,
    track_id: i32,
) {
    let c = *PALETTE.borrow();
    let pad = sd(QS_PAD);
    let cy = (t + b) as f32 * 0.5;
    let (tf, bf) = (t as f32, b as f32);
    if icon_id >= 0 {
        pan_hover((pad - sd(4)) as f32, tf, (pad + sd(26)) as f32, bf, sf(10.0), icon_id);
    }
    glyph(
        glyph_ch,
        TF_GLYPH_MD.borrow().as_ref(),
        (pad + sd(11)) as f32,
        cy,
        sd(12) as f32,
        if dim { c.sub2 } else { c.glyph },
        1.0,
    );
    text(
        label,
        TF_S.borrow().as_ref(),
        (pad + sd(34)) as f32,
        tf,
        (pad + sd(112)) as f32,
        bf,
        c.text,
        1.0,
    );
    let trk_l = (pad + sd(118)) as f32;
    let trk_r = (cw - pad - sd(50)) as f32;
    QS_TRK_L.set(trk_l);
    QS_TRK_R.set(trk_r);
    pan_hit(trk_l - sf(10.0), tf, trk_r + sf(10.0), bf, track_id);
    let press = PAN_DOWN.get() == track_id;
    let hot = press || PAN_HOV.get() == track_id;
    let bh = sf(4.0);
    let frac = if val < 0 {
        0.0
    } else if val > 100 {
        1.0
    } else {
        val as f32 / 100.0
    };
    let kx = (trk_l + frac * (trk_r - trk_l)).round();
    let fill = if dim { c.sub2 } else { c.accent };
    fill_rr(trk_l, cy - bh * 0.5, trk_r, cy + bh * 0.5, bh * 0.5, c.track, 1.0);
    if kx - trk_l > bh {
        fill_rr(trk_l, cy - bh * 0.5, kx, cy + bh * 0.5, bh * 0.5, fill, 1.0);
    }
    // Jempol gaya Windows 11: cakram netral padat bertepi tipis dan bayangan lembut, titik isi membesar saat disorot.
    let ko = sf(10.0);
    let ki = if press {
        sf(4.5)
    } else if hot {
        sf(6.5)
    } else {
        sf(5.5)
    };
    let disc = if LIGHT.get() {
        rgba(255, 255, 255, 1.0)
    } else {
        rgba(66, 72, 110, 1.0)
    };
    fill_rr(kx - ko, cy - ko + sf(1.0), kx + ko, cy + ko + sf(1.0), ko, c.shadow, 0.6);
    fill_rr(kx - ko, cy - ko, kx + ko, cy + ko, ko, disc, 1.0);
    let ring = D2D1_ELLIPSE {
        point: Vector2 { X: kx, Y: cy },
        radiusX: ko - 0.5,
        radiusY: ko - 0.5,
    };
    set_color(c.line, 1.0);
    {
        let rt_g = RT.borrow();
        let br_g = BR.borrow();
        if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
            let brush: &ID2D1Brush = br;
            unsafe { rt.DrawEllipse(&ring, brush, 1.0, None::<&ID2D1StrokeStyle>) };
        }
    }
    fill_rr(kx - ki, cy - ki, kx + ki, cy + ki, ki, fill, 1.0);
    let pct = if val < 0 {
        "--".to_string()
    } else {
        format!("{}%", val)
    };
    text(
        &pct,
        TF_SSC.borrow().as_ref(),
        (cw - pad - sd(38)) as f32,
        tf,
        (cw - pad) as f32,
        bf,
        if press { c.text } else { c.sub },
        1.0,
    );
}

/// Baris keluaran audio (QsAudioRow di C++): klik membuka Pengaturan suara.
fn qs_audio_row(cw: i32, t: i32, b: i32) {
    let c = *PALETTE.borrow();
    let l = sd(QS_PAD) as f32;
    let r = (cw - sd(QS_PAD)) as f32;
    let cy = (t + b) as f32 * 0.5;
    let (tf, bf) = (t as f32, b as f32);
    fill_rr(l, tf, r, bf, sf(12.0), c.chip, 1.0);
    pan_hover(l, tf, r, bf, sf(12.0), PAN_ID_QS_AUDIO);
    glyph(0xE7F5, TF_GLYPH_MD.borrow().as_ref(), l + sf(22.0), cy, sf(10.0), c.glyph, 1.0);
    text(
        "Keluaran audio",
        TF_SUB.borrow().as_ref(),
        l + sf(44.0),
        tf + sf(6.0),
        r - sf(34.0),
        cy,
        c.sub,
        1.0,
    );
    let out = QS_OUT.borrow().clone();
    let shown: &str = if out.is_empty() {
        "Tidak ada perangkat keluaran"
    } else {
        &out
    };
    text(
        shown,
        TF_S.borrow().as_ref(),
        l + sf(44.0),
        cy,
        r - sf(34.0),
        bf - sf(6.0),
        c.text,
        1.0,
    );
    glyph(0xE76C, TF_GLYPH_SM.borrow().as_ref(), r - sf(18.0), cy, sf(8.0), c.sub, 1.0);
}

/// Satu ubin (QsTile di C++): ikon di tengah atas, label di bawah. Kotak klik hanya bagian yang terlihat
/// di viewport `[vt, vb]`. Aksen hanya untuk ubin yang menyala.
fn qs_tile(i: usize, x0: f32, y0: f32, x1: f32, y1: f32, vt: f32, vb: f32) {
    let d = &G_QS_TILES[i];
    let c = *PALETTE.borrow();
    let id = PAN_ID_QS_TILE + i as i32;
    let on = qs_on(d);
    let rad = sf(14.0);
    let ct = if y0 < vt { vt } else { y0 };
    let cb = if y1 > vb { vb } else { y1 };
    if cb > ct {
        pan_hit(x0, ct, x1, cb, id);
    }
    fill_rr(x0, y0, x1, y1, rad, if on { c.accent } else { c.chip }, 1.0);
    if PAN_DOWN.get() == id {
        fill_rr(x0, y0, x1, y1, rad, c.press, 1.0);
    } else if PAN_HOV.get() == id {
        fill_rr(x0, y0, x1, y1, rad, c.hover, 1.0);
    }
    let ink = if on { QS_ON_INK } else { c.text };
    let mid = (x0 + x1) * 0.5;
    if d.glyph != 0 {
        let gc = if on { QS_ON_INK } else { c.glyph };
        glyph(d.glyph, TF_GLYPH_MD.borrow().as_ref(), mid, y0 + sf(25.0), sf(10.0), gc, 1.0);
    } else {
        // Hemat energi: ikon baterai dengan latar sesuai tema.
        let bg = if on {
            c.accent
        } else if LIGHT.get() {
            rgba(251, 250, 254, 1.0)
        } else {
            rgba(30, 35, 66, 1.0)
        };
        let gc = if on { QS_ON_INK } else { c.glyph };
        draw_battery(mid, y0 + sf(25.0), sf(20.0), gc, bg);
    }
    text(
        d.label,
        TF_SC.borrow().as_ref(),
        x0 + sf(6.0),
        y1 - sf(31.0),
        x1 - sf(6.0),
        y1 - sf(11.0),
        ink,
        1.0,
    );
}

/// Kisi ubin 3 kolom (QsTiles di C++), digeser sesuai gulir dan dipotong viewport.
fn qs_tiles(cw: i32, vp_t: i32, vp_b: i32) {
    let pad = sd(QS_PAD);
    let gap = sd(QS_TGAP);
    let th = sd(QS_TILEH);
    let tw = (cw - 2 * pad - (QS_TCOLS - 1) * gap) / QS_TCOLS; // lebar sama untuk semua ubin
    let scroll = QS_SCROLL.get();
    for i in 0..QS_NTILE {
        let r = i as i32 / QS_TCOLS;
        let c = i as i32 % QS_TCOLS;
        let x0 = (pad + c * (tw + gap)) as f32;
        let y0 = (vp_t + r * (th + gap) - scroll) as f32;
        qs_tile(i, x0, y0, x0 + tw as f32, y0 + th as f32, vp_t as f32, vp_b as f32);
    }
}

/// Indikator gulir (QsScrollBar di C++): hanya tampil bila isi lebih tinggi dari viewport.
fn qs_scroll_bar(w: f32, vp_t: i32, vp_b: i32) {
    let m = qs_max_scroll();
    if m <= 0 {
        return;
    }
    let c = *PALETTE.borrow();
    let vh = (vp_b - vp_t) as f32;
    let total = qs_content_h() as f32;
    let mut th = vh * vh / total;
    if th < sf(28.0) {
        th = sf(28.0);
    }
    let ty = vp_t as f32 + (vh - th) * QS_SCROLL.get() as f32 / m as f32;
    let x0 = w - sf(9.0);
    let x1 = w - sf(6.0);
    fill_rr(x0, ty, x1, ty + th, (x1 - x0) * 0.5, c.sub2, 0.8);
}

/// Judul "Pengaturan Cepat" dan ikon pengaturan (QsHeader di C++).
fn qs_header(cw: i32, t: i32, b: i32) {
    let c = *PALETTE.borrow();
    let l = sd(QS_PAD) as f32;
    let r = (cw - sd(QS_PAD)) as f32;
    text(
        "Pengaturan Cepat",
        TF_T2.borrow().as_ref(),
        l,
        t as f32,
        r - sf(44.0),
        b as f32,
        c.text,
        1.0,
    );
    pan_hover(r - sf(32.0), t as f32, r, b as f32, sf(10.0), PAN_ID_QS_SET);
    glyph(
        0xE713,
        TF_GLYPH_MD.borrow().as_ref(),
        r - sf(16.0),
        (t + b) as f32 * 0.5,
        sf(10.0),
        c.glyph,
        1.0,
    );
}

/// Memotong gambar ke persegi `clip` lewat clip sumbu-sejajar render target (PushAxisAlignedClip di C++).
fn qs_clip_push(clip: D2D_RECT_F) {
    let rt_g = RT.borrow();
    if let Some(rt) = rt_g.as_ref() {
        unsafe { rt.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
    }
}

fn qs_clip_pop() {
    let rt_g = RT.borrow();
    if let Some(rt) = rt_g.as_ref() {
        unsafe { rt.PopAxisAlignedClip() };
    }
}

/// Gambar seluruh panel Pengaturan Cepat (QsDraw di C++). Dipanggil dari `panel_content` (panel.rs).
pub fn qs_draw(w: f32, _h: f32) {
    let cw = (w + 0.5) as i32;
    let c = *PALETTE.borrow();
    let pad = sd(QS_PAD);
    let vp_t = qs_vp_top();
    let vp_b = qs_vp_bot();
    qs_header(cw, pad, pad + sd(QS_HEADH));
    let toast_on = unsafe { GetTickCount64() } < QS_TOAST_EXPIRY.get();
    let msg = QS_TOAST_MSG.borrow().clone();
    if toast_on && !msg.is_empty() {
        let tw = measure_text(&msg, TF_S.borrow().as_ref()) + sf(32.0);
        let tx0 = (w - tw) * 0.5;
        let tx1 = tx0 + tw;
        let ty0 = pad as f32;
        let ty1 = (pad + sd(QS_HEADH)) as f32;
        fill_rr(tx0 - sf(2.0), ty0 - sf(1.0), tx1 + sf(2.0), ty1 + sf(1.0), sf(14.0), c.shadow, 0.35);
        fill_rr(tx0, ty0, tx1, ty1, sf(13.0), c.accent, 1.0);
        text(&msg, TF_S.borrow().as_ref(), tx0 + sf(12.0), ty0, tx1 - sf(12.0), ty1, QS_ON_INK, 1.0);
    }
    let clip = rf(0.0, vp_t as f32, w, vp_b as f32);
    qs_clip_push(clip);
    qs_tiles(cw, vp_t, vp_b);
    qs_clip_pop();
    qs_scroll_bar(w, vp_t, vp_b);
    let div_y = vp_b + sd(QS_VGAP);
    fill_rr(
        pad as f32,
        div_y as f32,
        (cw - pad) as f32,
        div_y as f32 + 1.0,
        0.0,
        c.sep,
        1.0,
    );
    let mut y = div_y + 1 + sd(QS_VGAP);
    QS_VOL_T.set(y as f32);
    QS_VOL_B.set((y + sd(QS_ROWH)) as f32);
    qs_slider(
        cw,
        y,
        y + sd(QS_ROWH),
        vol_glyph(),
        PAN_ID_VOL_MUTE,
        "Volume",
        VOL.get(),
        MUTE.get(),
        PAN_ID_VOL_TRACK,
    );
    y += sd(QS_ROWH);
    QS_BRI_T.set(0.0);
    QS_BRI_B.set(0.0);
    if BRI_VAL.get() >= 0 {
        y += sd(4);
        QS_BRI_T.set(y as f32);
        QS_BRI_B.set((y + sd(QS_ROWH)) as f32);
        // Ikon matahari membuka Pengaturan Tampilan (kecerahan, lampu malam), seperti pintasan di Windows 11.
        qs_slider(
            cw,
            y,
            y + sd(QS_ROWH),
            0xE706,
            PAN_ID_BRI_ICON,
            "Kecerahan",
            BRI_VAL.get(),
            false,
            PAN_ID_BRI_TRACK,
        );
        y += sd(QS_ROWH);
    }
    y += sd(QS_VGAP);
    qs_audio_row(cw, y, y + sd(QS_AUDH));
}

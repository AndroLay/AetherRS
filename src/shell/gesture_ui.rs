//! Sumber: w12gesture.inc (1–286). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H15.
//! Thread gestur: jendela pesan tersembunyi (HWND_MESSAGE) + Raw Input usage page 0x0D, usage 0x05.
//! Keputusan gesture_logic (w12_gest_feed, w12_gest_lift, w12_ptp_add) dikirim ke TK_MSG sebagai WM_W12_TASK.
use super::prelude::*;
use std::ffi::c_void;
use windows::Win32::Devices::HumanInterfaceDevice::{
    HidP_GetButtonCaps, HidP_GetCaps, HidP_GetUsageValue, HidP_GetUsages, HidP_GetValueCaps, HidP_Input,
    HIDP_BUTTON_CAPS, HIDP_CAPS, HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, PHIDP_PREPARSED_DATA,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CreateEventW, CreateThread, SetEvent, THREAD_CREATION_FLAGS, WaitForSingleObject,
};
use windows::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList, HRAWINPUT, RAWINPUT,
    RAWINPUTDEVICE, RAWINPUTDEVICELIST, RAWINPUTHEADER, RID_DEVICE_INFO, RID_INPUT, RIDEV_DEVNOTIFY,
    RIDEV_INPUTSINK, RIDEV_REMOVE, RIDI_DEVICEINFO, RIDI_PREPARSEDDATA, RIM_TYPEHID,
    RegisterRawInputDevices,
};

const GEST_TMR_LIFT: usize = 1; // timer sekali jalan: angkat jari bila tak ada bingkai selama liftMs
const GEST_MAXDEV: usize = 4;
const GEST_MAXLINK: usize = 10;
const GEST_BUF_QWORDS: usize = 512; // 4096 byte; RAWINPUT dibaca lewat pointer, wajib rata 8

// Keadaan thread gestur (C: g_gdev, g_gest, g_gBuf). Hanya dipakai dari thread gestur, seperti di C.
static GEST_DEVS: SyncCell<RefCell<Vec<GestDev>>> = SyncCell::new(RefCell::new(Vec::new()));
static GEST_BUF: SyncCell<RefCell<Vec<u64>>> = SyncCell::new(RefCell::new(Vec::new()));
static GEST_G: SyncCell<RefCell<W12Gest>> = SyncCell::new(RefCell::new(W12Gest {
    fingers: 0,
    mode: 0,
    x0: 0.0,
    y0: 0.0,
    last_step_x: 0.0,
}));
// Handle jendela, thread, dan id thread (C: g_gestHwnd, g_gestThr, g_gestTid).
static GEST_HWND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static GEST_THR: SyncCell<Cell<HANDLE>> = SyncCell::new(Cell::new(HANDLE(std::ptr::null_mut())));
static GEST_TID: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));

/// C: `GestDev`. `dev` = HANDLE perangkat sebagai isize (0 = slot kosong); `pp` = data preparsed (kosong = ditolak).
struct GestDev {
    dev: isize,
    pp: Vec<u64>,
    cc_link: u16,
    has_cc: bool,
    has_conf: bool,
    link: [u16; GEST_MAXLINK],
    min_x: [i32; GEST_MAXLINK],
    max_x: [i32; GEST_MAXLINK],
    min_y: [i32; GEST_MAXLINK],
    max_y: [i32; GEST_MAXLINK],
    n_link: usize,
    fr: W12PtpFrame,
}

impl GestDev {
    fn empty() -> Self {
        GestDev {
            dev: 0,
            pp: Vec::new(),
            cc_link: 0,
            has_cc: false,
            has_conf: false,
            link: [0; GEST_MAXLINK],
            min_x: [0; GEST_MAXLINK],
            max_x: [0; GEST_MAXLINK],
            min_y: [0; GEST_MAXLINK],
            max_y: [0; GEST_MAXLINK],
            n_link: 0,
            fr: W12PtpFrame::default(),
        }
    }
}

fn pp_of(buf: &[u64]) -> PHIDP_PREPARSED_DATA {
    PHIDP_PREPARSED_DATA(buf.as_ptr() as isize)
}

/// C: `GestLinkSlot`. Slot untuk link collection jari; None bila penuh.
fn gest_link_slot(d: &mut GestDev, link: u16) -> Option<usize> {
    for k in 0..d.n_link {
        if d.link[k] == link {
            return Some(k);
        }
    }
    if d.n_link >= GEST_MAXLINK {
        return None;
    }
    let k = d.n_link;
    d.link[k] = link;
    d.min_x[k] = 0;
    d.max_x[k] = 0;
    d.min_y[k] = 0;
    d.max_y[k] = 0;
    d.n_link += 1;
    Some(k)
}

/// C: `GestBuild`. Membaca data preparsed dan caps. None bila bukan touch pad (usage 0x0D/0x05) atau tanpa Contact Count.
fn gest_build(hd: HANDLE) -> Option<GestDev> {
    let mut cb: u32 = 0;
    let need = unsafe { GetRawInputDeviceInfoW(Some(hd), RIDI_PREPARSEDDATA, None, &mut cb) };
    if need == u32::MAX || cb == 0 {
        return None;
    }
    let mut buf: Vec<u64> = vec![0; (cb as usize + 7) / 8];
    let got = unsafe {
        GetRawInputDeviceInfoW(
            Some(hd),
            RIDI_PREPARSEDDATA,
            Some(buf.as_mut_ptr() as *mut c_void),
            &mut cb,
        )
    };
    if got == u32::MAX {
        return None;
    }
    let pp = pp_of(&buf);
    let mut caps = HIDP_CAPS::default();
    let st = unsafe { HidP_GetCaps(pp, &mut caps) };
    if st != HIDP_STATUS_SUCCESS || caps.UsagePage != 0x0D || caps.Usage != 0x05 {
        return None;
    }

    let mut b = GestDev::empty();
    b.dev = hd.0 as isize;
    let mut nv: u16 = caps.NumberInputValueCaps;
    let mut vc: Vec<HIDP_VALUE_CAPS> = (0..nv.max(1)).map(|_| HIDP_VALUE_CAPS::default()).collect();
    let st = unsafe { HidP_GetValueCaps(HidP_Input, vc.as_mut_ptr(), &mut nv, pp) };
    if st == HIDP_STATUS_SUCCESS {
        for v in vc.iter().take(nv as usize) {
            if v.IsRange {
                continue;
            }
            let u = unsafe { v.Anonymous.NotRange.Usage };
            if v.UsagePage == 0x0D && u == 0x54 {
                // Contact Count
                b.has_cc = true;
                b.cc_link = v.LinkCollection;
            } else if v.UsagePage == 0x01 && (u == 0x30 || u == 0x31) {
                // X / Y milik link collection jari
                let k = match gest_link_slot(&mut b, v.LinkCollection) {
                    Some(k) => k,
                    None => continue,
                };
                if u == 0x30 {
                    b.min_x[k] = v.LogicalMin;
                    b.max_x[k] = v.LogicalMax;
                } else {
                    b.min_y[k] = v.LogicalMin;
                    b.max_y[k] = v.LogicalMax;
                }
            }
        }
    }
    if !b.has_cc || b.n_link == 0 {
        return None;
    }

    // Confidence (0x0D/0x47) ada atau tidak: tanpanya setiap kontak dianggap confident.
    let mut nb: u16 = caps.NumberInputButtonCaps;
    let mut bc: Vec<HIDP_BUTTON_CAPS> = (0..nb.max(1)).map(|_| HIDP_BUTTON_CAPS::default()).collect();
    let st = unsafe { HidP_GetButtonCaps(HidP_Input, bc.as_mut_ptr(), &mut nb, pp) };
    if st == HIDP_STATUS_SUCCESS {
        for x in bc.iter().take(nb as usize) {
            if x.UsagePage != 0x0D {
                continue;
            }
            let hit = unsafe {
                if x.IsRange {
                    x.Anonymous.Range.UsageMin <= 0x47 && x.Anonymous.Range.UsageMax >= 0x47
                } else {
                    x.Anonymous.NotRange.Usage == 0x47
                }
            };
            if hit {
                b.has_conf = true;
            }
        }
    }
    b.pp = buf;
    Some(b)
}

/// C: `GestDevFor`. Indeks entri untuk perangkat ini; None bila ditolak. Entri baru dibangun pada laporan pertamanya.
fn gest_dev_for(devs: &mut Vec<GestDev>, hd: HANDLE) -> Option<usize> {
    if devs.is_empty() {
        for _ in 0..GEST_MAXDEV {
            devs.push(GestDev::empty());
        }
    }
    let key = hd.0 as isize;
    if let Some(i) = devs.iter().position(|d| d.dev == key) {
        return if devs[i].pp.is_empty() { None } else { Some(i) };
    }
    let free = devs.iter().position(|d| d.dev == 0)?;
    match gest_build(hd) {
        Some(b) => {
            devs[free] = b;
            Some(free)
        }
        None => {
            devs[free].dev = key; // cache penolakan: laporan berikutnya tidak membangun ulang caps
            None
        }
    }
}

/// C: `GestForget`. Perangkat dicabut (GIDC_REMOVAL): bebaskan entrinya.
fn gest_forget(hd: HANDLE) {
    let key = hd.0 as isize;
    let mut devs = GEST_DEVS.borrow_mut();
    for d in devs.iter_mut() {
        if d.dev == key {
            *d = GestDev::empty();
        }
    }
}

/// C: `GestCollect`. Kontak di laporan ini: slot yang X, Y, dan tombol (Tip Switch, Confidence)-nya terbaca.
fn gest_collect(d: &GestDev, rep: &mut [u8], out: &mut [W12PtpContact]) -> usize {
    let pp = pp_of(&d.pp);
    let mut n = 0usize;
    for k in 0..d.n_link {
        let link = d.link[k];
        let mut x: u32 = 0;
        let mut y: u32 = 0;
        let mut usages = [0u16; 8];
        let mut nu: u32 = 8;
        let st = unsafe { HidP_GetUsageValue(HidP_Input, 0x01, Some(link), 0x30, &mut x, pp, &*rep) };
        if st != HIDP_STATUS_SUCCESS {
            continue;
        }
        let st = unsafe { HidP_GetUsageValue(HidP_Input, 0x01, Some(link), 0x31, &mut y, pp, &*rep) };
        if st != HIDP_STATUS_SUCCESS {
            continue;
        }
        let st = unsafe {
            HidP_GetUsages(HidP_Input, 0x0D, Some(link), usages.as_mut_ptr(), &mut nu, pp, rep)
        };
        if st != HIDP_STATUS_SUCCESS {
            continue;
        }
        let mut tip = false;
        let mut conf = !d.has_conf;
        for &u in usages.iter().take(nu as usize) {
            if u == 0x42 {
                tip = true; // Tip Switch
            } else if u == 0x47 {
                conf = true; // Confidence
            }
        }
        if n < out.len() {
            out[n] = W12PtpContact {
                x: x as i32,
                y: y as i32,
                tip,
                confident: conf,
            };
            n += 1;
        }
    }
    n
}

/// C: `GestRange`. Rentang koordinat seluruh pad dari semua link collection jari: (x0, x1, y0, y1).
fn gest_range(d: &GestDev) -> (f32, f32, f32, f32) {
    let mut ax = d.min_x[0];
    let mut bx = d.max_x[0];
    let mut ay = d.min_y[0];
    let mut by = d.max_y[0];
    for k in 1..d.n_link {
        ax = ax.min(d.min_x[k]);
        bx = bx.max(d.max_x[k]);
        ay = ay.min(d.min_y[k]);
        by = by.max(d.max_y[k]);
    }
    (ax as f32, bx as f32, ay as f32, by as f32)
}

/// C: `GestPost`. Menerjemahkan keputusan gestur ke pesan UI (TK_* di state.rs). Dipanggil dari thread gestur.
/// Deviasi: `tk_post` di overlay.rs bersifat privat, sehingga helper posting ini ditulis ulang di sini.
fn gest_post(ev: i32) {
    let hw = TK_MSG.get();
    if hw.0.is_null() {
        return;
    }
    let (kind, lp): (i32, isize) = match ev {
        W12G_SW_STEP_FWD => (TK_SW_STEP, 2),
        W12G_SW_STEP_BACK => (TK_SW_STEP, -2),
        W12G_SW_COMMIT => (TK_SW_COMMIT, 0),
        W12G_DESK_NEXT => (TK_DESK_GO, 1),
        W12G_DESK_PREV => (TK_DESK_GO, -1),
        W12G_UP => (TK_GEST_UP, 0),
        W12G_DOWN => (TK_GEST_DOWN, 0),
        _ => return,
    };
    let _ = unsafe { PostMessageW(Some(hw), WM_W12_TASK, WPARAM(kind as usize), LPARAM(lp)) };
}

/// C: `GestFrame`. Bingkai lengkap: pusat jari dinormalisasi ke 0..1 lalu diumpankan ke keputusan gestur.
fn gest_frame(d: &mut GestDev) {
    let fingers = d.fr.fingers;
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    if fingers > 0 {
        let (x0, x1, y0, y1) = gest_range(d);
        let ax = d.fr.sx as f32 / fingers as f32;
        let ay = d.fr.sy as f32 / fingers as f32;
        cx = if x1 > x0 { (ax - x0) / (x1 - x0) } else { 0.5 };
        cy = if y1 > y0 { (ay - y0) / (y1 - y0) } else { 0.5 };
        unsafe {
            let _ = SetTimer(Some(GEST_HWND.get()), GEST_TMR_LIFT, W12_GEST_DEFAULT.lift_ms, None);
        }
    } else {
        unsafe {
            let _ = KillTimer(Some(GEST_HWND.get()), GEST_TMR_LIFT);
        }
    }
    let ev = w12_gest_feed(&mut *GEST_G.borrow_mut(), &W12_GEST_DEFAULT, fingers, cx, cy);
    gest_post(ev);
}

/// C: `GestReport`. Satu laporan HID: hitung kontaknya lalu gabungkan ke bingkai.
fn gest_report(d: &mut GestDev, mut rep: Vec<u8>) {
    let pp = pp_of(&d.pp);
    let mut cc: u32 = 0;
    let cc_ok = d.has_cc
        && unsafe { HidP_GetUsageValue(HidP_Input, 0x0D, Some(d.cc_link), 0x54, &mut cc, pp, &rep) }
            == HIDP_STATUS_SUCCESS;
    if !cc_ok {
        cc = 0;
    }
    let mut contacts = [W12PtpContact::default(); GEST_MAXLINK];
    let n = gest_collect(d, &mut rep, &mut contacts);
    if w12_ptp_add(&mut d.fr, cc as i32, &contacts[..n], n as i32) == 1 {
        gest_frame(d);
    }
}

/// C: `GestInput`. WM_INPUT: hanya laporan HID dari touch pad yang diproses.
fn gest_input(hr: HRAWINPUT) {
    // header + dwSizeHid + dwCount; bRawData mulai di offset ini
    let hdr = (std::mem::size_of::<RAWINPUTHEADER>() + 2 * std::mem::size_of::<u32>()) as u32;
    let mut buf = GEST_BUF.borrow_mut();
    if buf.len() != GEST_BUF_QWORDS {
        buf.resize(GEST_BUF_QWORDS, 0);
    }
    let mut cb = (buf.len() * 8) as u32;
    let got = unsafe {
        GetRawInputData(
            hr,
            RID_INPUT,
            Some(buf.as_mut_ptr() as *mut c_void),
            &mut cb,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if got == u32::MAX || got < hdr {
        return;
    }
    let base = buf.as_ptr() as *const u8;
    let (dw_type, sz, cnt, dev) = unsafe {
        let ri = buf.as_ptr() as *const RAWINPUT;
        (
            (*ri).header.dwType,
            (*ri).data.hid.dwSizeHid,
            (*ri).data.hid.dwCount,
            (*ri).header.hDevice,
        )
    };
    if dw_type != RIM_TYPEHID.0 {
        return;
    }
    if sz == 0 || cnt == 0 || (sz as u64) * (cnt as u64) > (got - hdr) as u64 {
        return;
    }
    let mut devs = GEST_DEVS.borrow_mut();
    let idx = match gest_dev_for(&mut devs, dev) {
        Some(i) => i,
        None => return,
    };
    for r in 0..cnt {
        let off = hdr as usize + (r as usize) * (sz as usize);
        // Laporan disalin agar &rep (HidP_GetUsageValue) dan &mut rep (HidP_GetUsages) tidak berbenturan.
        let rep: Vec<u8> = unsafe { std::slice::from_raw_parts(base.add(off), sz as usize) }.to_vec();
        gest_report(&mut devs[idx], rep);
    }
}

/// C: `GestProc`. Jendela pesan tersembunyi: WM_INPUT (laporan), timer angkat jari, dan perangkat yang dicabut.
/// Seperti C, pesan yang tidak ditangani diteruskan ke DefWindowProcW (WM_INPUT wajib untuk pembersihan sistem).
unsafe extern "system" fn gest_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_INPUT => gest_input(HRAWINPUT(l.0 as *mut c_void)),
        WM_TIMER if w.0 == GEST_TMR_LIFT => {
            unsafe {
                let _ = KillTimer(Some(h), GEST_TMR_LIFT);
            }
            gest_post(w12_gest_lift(&mut *GEST_G.borrow_mut()));
            return LRESULT(0);
        }
        WM_INPUT_DEVICE_CHANGE => {
            if w.0 == GIDC_REMOVAL as usize {
                gest_forget(HANDLE(l.0 as *mut c_void));
            }
        }
        _ => {}
    }
    unsafe { DefWindowProcW(h, m, w, l) }
}

/// C: `GestThread`. Membuat jendela pesan, mendaftarkan Raw Input (usage 0x0D/0x05), lalu memompa pesan sampai WM_QUIT.
/// `param` = handle event `ready`; di-signal setelah percobaan membuat jendela (berhasil atau gagal), seperti C.
unsafe extern "system" fn gest_thread(param: *mut c_void) -> u32 {
    let ready = HANDLE(param);
    // SAFETY: struct C POD; nol adalah nilai valid.
    let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
    let hinst = match unsafe { GetModuleHandleW(PCWSTR::null()) } {
        Ok(m) => HINSTANCE(m.0),
        Err(_) => HINSTANCE(std::ptr::null_mut()),
    };
    let class_name = to_wide_z("Win12Gest");
    let title = to_wide_z("");
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(gest_proc);
    wc.hInstance = hinst;
    wc.lpszClassName = pcw(&class_name);
    // C: hasil RegisterClassExW diabaikan; keberhasilan ditentukan oleh CreateWindowExW dan RegisterRawInputDevices.
    let _ = unsafe { RegisterClassExW(&wc) };
    let hw = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            pcw(&class_name),
            pcw(&title),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinst),
            None,
        )
    }
    .unwrap_or(HWND(std::ptr::null_mut()));
    GEST_HWND.set(hw);

    let size = std::mem::size_of::<RAWINPUTDEVICE>() as u32;
    let rid = [RAWINPUTDEVICE {
        usUsagePage: 0x0D,
        usUsage: 0x05,
        dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
        hwndTarget: hw,
    }];
    let reg_ok = !hw.0.is_null() && unsafe { RegisterRawInputDevices(&rid, size) }.is_ok();
    if !ready.0.is_null() {
        let _ = unsafe { SetEvent(ready) };
    }
    if !reg_ok {
        if !hw.0.is_null() {
            let _ = unsafe { DestroyWindow(hw) };
        }
        GEST_HWND.set(HWND(std::ptr::null_mut()));
        return 0;
    }

    // SAFETY: struct C POD; nol adalah nilai valid.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }

    // Pembersihan setelah WM_QUIT (C: GestThread).
    let rm = [RAWINPUTDEVICE {
        usUsagePage: 0x0D,
        usUsage: 0x05,
        dwFlags: RIDEV_REMOVE,
        hwndTarget: HWND(std::ptr::null_mut()),
    }];
    let _ = unsafe { RegisterRawInputDevices(&rm, size) };
    if !hw.0.is_null() {
        let _ = unsafe { DestroyWindow(hw) };
    }
    GEST_HWND.set(HWND(std::ptr::null_mut()));
    GEST_DEVS.borrow_mut().clear();
    *GEST_G.borrow_mut() = W12Gest {
        fingers: 0,
        mode: 0,
        x0: 0.0,
        y0: 0.0,
        last_step_x: 0.0,
    };
    0
}

/// C: `GestHasPtp`. Ada perangkat HID dengan usage page 0x0D dan usage 0x05 (precision touch pad)?
fn gest_has_ptp() -> bool {
    let sz = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
    let mut n: u32 = 0;
    if unsafe { GetRawInputDeviceList(None, &mut n, sz) } == u32::MAX || n == 0 {
        return false;
    }
    // SAFETY: struct C POD; nol adalah nilai valid.
    let mut list: Vec<RAWINPUTDEVICELIST> = (0..n).map(|_| unsafe { std::mem::zeroed() }).collect();
    let got = unsafe { GetRawInputDeviceList(Some(list.as_mut_ptr()), &mut n, sz) };
    if got == u32::MAX {
        return false;
    }
    for dv in list.iter().take(got as usize) {
        if dv.dwType.0 != RIM_TYPEHID.0 {
            continue;
        }
        // SAFETY: struct C POD; nol adalah nilai valid.
        let mut di: RID_DEVICE_INFO = unsafe { std::mem::zeroed() };
        di.cbSize = std::mem::size_of::<RID_DEVICE_INFO>() as u32;
        let mut cbi = di.cbSize;
        let r = unsafe {
            GetRawInputDeviceInfoW(
                Some(dv.hDevice),
                RIDI_DEVICEINFO,
                Some(&mut di as *mut RID_DEVICE_INFO as *mut c_void),
                &mut cbi,
            )
        };
        if r == u32::MAX {
            continue;
        }
        let (up, us) = unsafe { (di.Anonymous.hid.usUsagePage, di.Anonymous.hid.usUsage) };
        if up == 0x0D && us == 0x05 {
            return true;
        }
    }
    false
}

/// C: `GestStart`. Menyalakan thread gestur bila `[gestures] enabled` tidak 0 dan ada touch pad PTP.
pub fn gest_start() {
    if !GEST_THR.get().0.is_null() {
        return;
    }
    if ini_int("gestures", "enabled", 1) == 0 || !gest_has_ptp() {
        return;
    }
    let ready = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
        Ok(h) => h,
        Err(_) => return,
    };
    let mut tid: u32 = 0;
    let thr = match unsafe {
        CreateThread(
            None,
            0,
            Some(gest_thread),
            Some(ready.0 as *const c_void),
            THREAD_CREATION_FLAGS(0),
            Some(&mut tid as *mut u32),
        )
    } {
        Ok(h) => h,
        Err(_) => {
            let _ = unsafe { CloseHandle(ready) };
            return;
        }
    };
    GEST_THR.set(thr);
    GEST_TID.set(tid);
    // Tunggu thread membuat jendela (maks 1 detik). Bila tenggat lewat, handle ready dibiarkan terbuka (sama seperti C).
    if unsafe { WaitForSingleObject(ready, 1000) } == WAIT_OBJECT_0 {
        let _ = unsafe { CloseHandle(ready) };
    }
}

/// C: `GestStop`. Mengirim WM_QUIT ke thread gestur lalu menunggu sampai thread selesai.
pub fn gest_stop() {
    let thr = GEST_THR.get();
    if thr.0.is_null() {
        return;
    }
    unsafe {
        let _ = PostThreadMessageW(GEST_TID.get(), WM_QUIT, WPARAM(0), LPARAM(0));
        let _ = WaitForSingleObject(thr, 1000);
        let _ = CloseHandle(thr);
    }
    GEST_THR.set(HANDLE(std::ptr::null_mut()));
    GEST_TID.set(0);
}

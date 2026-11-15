//! Sumber: w12start.inc:758-akhir (StartMeasure s.d. StartDraw).
//! Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H07.
use super::prelude::*;
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ACCESS_MASK, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Shutdown::{
    ExitWindowsEx, EWX_LOGOFF, EWX_POWEROFF, EWX_REBOOT, EWX_SHUTDOWN, EXIT_WINDOWS_FLAGS,
    SHTDN_REASON_FLAG_PLANNED, SHTDN_REASON_MAJOR_OTHER, SHTDN_REASON_MINOR_OTHER, SHUTDOWN_REASON,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::System::WindowsProgramming::{GetUserNameW, MulDiv};

/// `lastN` (static lokal di StartBuildLists, w12start.inc:799).
static START_LAST_N: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));

// Konvensi: panjang Vec `ST_PIN` dan `ST_REC` adalah `g_stNPin` dan `g_stNRec`.

/// Sumber field `spin` dan `pin` untuk `w12_start_build_pin_indices` (start_lists.rs).
impl AppLike for App {
    fn spin(&self) -> i32 {
        self.spin
    }
    fn pin(&self) -> i32 {
        self.pin
    }
}

/// Port `StartMeasure` (w12start.inc:758). Dipakai panel saat mengukur PN_START.
pub fn start_measure(cw: &mut i32, ch: &mut i32) {
    *cw = s(START_W as f32);
    *ch = s(START_H as f32);
    if !ST_USER_OK.get() {
        let mut buf = ST_USER.borrow_mut();
        let mut n: u32 = buf.len() as u32;
        let ok = unsafe { GetUserNameW(Some(PWSTR(buf.as_mut_ptr())), &mut n) }.is_ok();
        ST_USER_OK.set(ok);
    }
    start_nl_check();
    st_all_apps_invalidate();
    start_lists_invalidate();
    nl_api_open(); // API dibaca di thread; pembukaan panel tidak menunggu jaringan.
    st_idx_refresh();
}

/// Port `StartReset` (w12start.inc:773). Mengosongkan kueri, menu daya, seleksi, dan mode Semua aplikasi.
pub fn start_reset() {
    ST_QUERY.borrow_mut()[0] = 0;
    ST_Q_LEN.set(0);
    ST_QUERY_ALL.set(false);
    ST_POWER.set(0);
    ST_SEL.set(0);
    ST_ALL_APPS_MODE.set(false);
    ST_ALL_SCROLL.set(0);
    ST_ALL_SELECTED.set(-1);
    // Ikon hasil cari hanya dipakai selama mencari: lepas agar RAM tetap kecil.
    for e in ST_IDX.borrow_mut().iter_mut() {
        if e.icon_tried {
            icon_free(&mut e.icon);
            e.icon_tried = false;
        }
    }
}

/// Port `StartChar` (w12start.inc:787). Enter, Backspace, dan Esc ditangani `start_key`.
pub fn start_char(ch: u16) {
    if ch < 32 || ch == 127 {
        return;
    }
    let len = {
        let mut q = ST_QUERY.borrow_mut();
        w12_start_query_insert_char(&mut q[..], ST_Q_LEN.get(), ST_QUERY_ALL.get(), ch)
    };
    ST_Q_LEN.set(len);
    ST_QUERY_ALL.set(false);
    ST_SEL.set(0);
}

/// Port `StartBuildLists` (w12start.inc:797). Isi daftar Disematkan, Terbaru, dan hasil cari.
fn start_build_lists() {
    let napps = apps_count();
    if START_LAST_N.get() != napps {
        START_LAST_N.set(napps);
        start_lists_invalidate();
    }
    if ST_LISTS_READY.get() {
        st_search();
        return;
    }
    {
        let apps = APPS.borrow();
        let mut pin = vec![0i32; START_PINS.max(0) as usize];
        let n = w12_start_build_pin_indices(&apps[..], napps, START_PINS, &mut pin[..]);
        pin.truncate(n.max(0) as usize);
        let mut rec: Vec<i32> = Vec::new();
        let lim = (napps.max(0) as usize).min(apps.len());
        for pass in 0..2 {
            for i in 0..lim {
                if !apps[i].wins.is_empty() && apps[i].fg == (pass == 0) {
                    rec.push(i as i32);
                }
            }
        }
        *ST_PIN.borrow_mut() = pin;
        *ST_REC.borrow_mut() = rec;
    }
    ST_LISTS_READY.set(true);
    st_search();
}

/// Port `StartPick` (w12start.inc:814). Menutup panel lalu meluncurkan aplikasi `ai`.
pub fn start_pick(ai: i32) {
    if ai < 0 || ai >= apps_count() {
        return;
    }
    panel_close();
    click_app(ai, false);
}

/// Port `StartPowerDo` (w12start.inc:822).
/// 1 = Tidur, 2 = Mulai ulang, 3 = Matikan, 4 = Keluar (EWX_LOGOFF; menu Win+X).
pub fn start_power_do(what: i32) {
    panel_close();
    if what == 1 {
        // PORT: BOOLEAN tidak ada di windows 0.61; memakai u8 (UCHAR, ABI sama).
        type PfnSuspend = unsafe extern "system" fn(u8, u8, u8) -> u8;
        if let Ok(pp) = unsafe { LoadLibraryW(w!("powrprof.dll")) } {
            let proc = unsafe { GetProcAddress(pp, windows::core::PCSTR(b"SetSuspendState\0".as_ptr())) };
            if let Some(p) = proc {
                let f: PfnSuspend = unsafe { std::mem::transmute(p) };
                unsafe { f(0, 0, 0) };
            }
        }
        return;
    }
    let reason = SHUTDOWN_REASON(
        SHTDN_REASON_MAJOR_OTHER.0 | SHTDN_REASON_MINOR_OTHER.0 | SHTDN_REASON_FLAG_PLANNED.0,
    );
    if what == 4 {
        // Keluar: EWX_LOGOFF tidak butuh hak istimewa; cabang eksplisit agar tidak jatuh ke Matikan.
        let _ = unsafe { ExitWindowsEx(EWX_LOGOFF, reason) };
        return;
    }
    let mut tok = HANDLE(std::ptr::null_mut());
    let access = TOKEN_ACCESS_MASK(TOKEN_ADJUST_PRIVILEGES.0 | TOKEN_QUERY.0);
    if unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut tok) }.is_ok() {
        let mut tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: LUID { LowPart: 0, HighPart: 0 },
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        if unsafe { LookupPrivilegeValueW(PCWSTR::null(), w!("SeShutdownPrivilege"), &mut tp.Privileges[0].Luid) }
            .is_ok()
        {
            let _ = unsafe {
                AdjustTokenPrivileges(tok, false, Some(&tp as *const TOKEN_PRIVILEGES), 0, None, None)
            };
        }
        let _ = unsafe { CloseHandle(tok) };
    }
    let flags = if what == 2 {
        EWX_REBOOT
    } else {
        EXIT_WINDOWS_FLAGS(EWX_SHUTDOWN.0 | EWX_POWEROFF.0)
    };
    let _ = unsafe { ExitWindowsEx(flags, reason) };
}

/// Port `StartKey` (w12start.inc:850). Mengembalikan true bila tampilan perlu digambar ulang.
/// Enter menjalankan hasil pertama lalu menutup panel.
pub fn start_key(vk: i32) -> bool {
    let is = |k: VIRTUAL_KEY| vk == k.0 as i32;
    let ctrl = || (unsafe { GetKeyState(VK_CONTROL.0 as i32) } as u16 & 0x8000) != 0;

    if is(VK_ESCAPE) && ST_POWER.get() != 0 {
        ST_POWER.set(0); // Esc menutup menu Daya dulu
        return true;
    }
    if vk == b'A' as i32 && ST_Q_LEN.get() != 0 && ST_POWER.get() == 0 && ctrl() {
        let all = w12_start_query_select_all(ST_Q_LEN.get());
        ST_QUERY_ALL.set(all);
        return all;
    }
    if is(VK_BACK) || is(VK_DELETE) {
        if ST_POWER.get() != 0 || ST_Q_LEN.get() == 0 {
            return false;
        }
        if ST_QUERY_ALL.get() {
            let len = {
                let mut q = ST_QUERY.borrow_mut();
                w12_start_query_clear_selection(&mut q[..], ST_Q_LEN.get(), true)
            };
            ST_Q_LEN.set(len);
            ST_QUERY_ALL.set(false);
            ST_SEL.set(0);
            return true;
        }
        if is(VK_DELETE) {
            return false;
        }
        let c = ctrl();
        let len = {
            let mut q = ST_QUERY.borrow_mut();
            w12_search_backspace(&mut q[..], ST_Q_LEN.get() as usize, c) as i32
        };
        ST_Q_LEN.set(len);
        ST_SEL.set(0);
        return true;
    }
    if is(VK_ESCAPE) {
        if ST_Q_LEN.get() == 0 {
            if !ST_ALL_APPS_MODE.get() {
                return false; // halaman Beranda: Esc menutup panel
            }
            ST_ALL_APPS_MODE.set(false);
            ST_ALL_SCROLL.set(0);
            ST_ALL_SELECTED.set(-1);
            return true;
        }
        let return_to_all_apps = ST_ALL_APPS_MODE.get();
        start_reset();
        ST_ALL_APPS_MODE.set(return_to_all_apps);
        return true;
    }
    if ST_ALL_APPS_MODE.get() && ST_Q_LEN.get() == 0 && ST_POWER.get() == 0 {
        st_all_apps_ensure();
        if is(VK_RETURN) && ST_ALL_SELECTED.get() >= 0 {
            st_all_app_open(ST_ALL_SELECTED.get());
            return true;
        }
        if is(VK_UP) || is(VK_DOWN) || is(VK_HOME) || is(VK_END) || is(VK_PRIOR) || is(VK_NEXT) {
            let n_all = ST_ALL_N_APPS.get();
            if n_all == 0 {
                return false;
            }
            let row = ST_ALL_ROW_STEP.get();
            let step = if row > 0 { row } else { s(40.0) };
            let vh = ST_ALL_VIEW_HEIGHT.get();
            let view = if vh > 0 { vh } else { s(400.0) };
            let mut sel = ST_ALL_SELECTED.get();
            if is(VK_HOME) {
                sel = 0;
            } else if is(VK_END) {
                sel = n_all - 1;
            } else if is(VK_PRIOR) || is(VK_NEXT) {
                let page = if view / step > 0 { view / step } else { 1 };
                if sel < 0 {
                    sel = 0;
                } else {
                    sel += if is(VK_NEXT) { page } else { -page };
                }
                if sel < 0 {
                    sel = 0;
                }
                if sel >= n_all {
                    sel = n_all - 1;
                }
            } else if sel < 0 {
                sel = 0;
            } else {
                let delta = if is(VK_DOWN) { 1 } else { n_all - 1 };
                sel = (sel + delta) % n_all;
            }
            ST_ALL_SELECTED.set(sel);
            let row_top = sel * step;
            let mut scroll = ST_ALL_SCROLL.get();
            if row_top < scroll {
                scroll = row_top;
            } else if row_top + step > scroll + view {
                scroll = row_top + step - view;
            }
            ST_ALL_SCROLL.set(scroll);
            st_all_apps_clamp_scroll();
            return true;
        }
    }
    if ST_Q_LEN.get() == 0 || ST_POWER.get() != 0 {
        return false;
    }
    start_build_lists();
    if is(VK_DOWN) || is(VK_UP) {
        // baris terakhir (indeks g_stNHits) = "Cari di web"
        ST_QUERY_ALL.set(false);
        let n = ST_N_HITS.get() + 1;
        let delta = if is(VK_DOWN) { 1 } else { n - 1 };
        ST_SEL.set((ST_SEL.get() + delta) % n);
        return true;
    }
    if is(VK_RETURN) {
        st_open_hit(ST_SEL.get());
    }
    false
}

/// Port `StartWheel` (w12start.inc:922). Menggulir daftar Semua aplikasi.
pub fn start_wheel(delta: i32) {
    if !ST_ALL_APPS_MODE.get() || ST_Q_LEN.get() != 0 {
        return;
    }
    st_all_apps_ensure();
    let row = ST_ALL_ROW_STEP.get();
    let step = if row > 0 { row } else { s(40.0) };
    let moved = unsafe { MulDiv(delta, step, WHEEL_DELTA as i32) };
    ST_ALL_SCROLL.set(ST_ALL_SCROLL.get() - moved);
    ST_ALL_SELECTED.set(-1);
    st_all_apps_clamp_scroll();
}

/// Port `StartActivate` (w12start.inc:931). Dipanggil panel saat item PN_START diklik.
pub fn start_activate(id: i32) {
    if ST_POWER.get() != 0 {
        // menu Daya terbuka: hanya isinya yang bereaksi; klik di luar menu menutupnya
        if id == PAN_ID_START_SLEEP {
            start_power_do(1);
        } else if id == PAN_ID_START_RESTART {
            ST_POWER.set(2);
        } else if id == PAN_ID_START_OFF {
            ST_POWER.set(3);
        } else if id == PAN_ID_START_YES {
            start_power_do(ST_POWER.get());
        } else if id != PAN_ID_START_PMENU {
            ST_POWER.set(0); // Batal, tombol Daya lagi, atau di luar menu
        }
        return;
    }
    if id == PAN_ID_START_ALL_VIEW {
        ST_ALL_APPS_MODE.set(true);
        st_all_apps_ensure();
        ST_ALL_SCROLL.set(0);
        ST_ALL_SELECTED.set(if ST_ALL_N_APPS.get() != 0 { 0 } else { -1 });
        return;
    }
    if id == PAN_ID_START_HOME {
        ST_ALL_APPS_MODE.set(false);
        ST_ALL_SCROLL.set(0);
        ST_ALL_SELECTED.set(-1);
        return;
    }
    if id >= PAN_ID_START_ALPHA && id < PAN_ID_START_ALPHA + 26 {
        start_all_apps_jump_to_letter(id - PAN_ID_START_ALPHA);
        return;
    }
    if id >= PAN_ID_START_ALL_APP && id - PAN_ID_START_ALL_APP < ST_ALL_N_APPS.get() {
        st_all_app_open(id - PAN_ID_START_ALL_APP);
        return;
    }
    if id == PAN_ID_START_POWER {
        ST_POWER.set(1);
        return;
    }
    if id == PAN_ID_START_NL {
        let exe = ST_NL_EXE.borrow().clone();
        pan_uri(&exe);
        return;
    }
    if id == PAN_ID_START_PHONE {
        pan_uri("ms-settings:mobile-devices");
        return;
    }
    if id == PAN_ID_START_GEAR {
        pan_uri("ms-settings:");
        return;
    }
    if id == PAN_ID_START_USER {
        pan_uri("ms-settings:yourinfo");
        return;
    }
    if id >= PAN_ID_START_PIN && id < PAN_ID_START_PIN + MAXAPPS {
        start_pick(id - PAN_ID_START_PIN);
    } else if id >= PAN_ID_START_REC && id < PAN_ID_START_REC + MAXAPPS {
        start_pick(id - PAN_ID_START_REC);
    } else if id == PAN_ID_START_FILES {
        st_open_hit(ST_N_HITS.get());
    } else if id == PAN_ID_START_OPEN {
        st_open_hit(ST_SEL.get());
    } else if id >= PAN_ID_START_RES && id < PAN_ID_START_RES + START_HITS {
        st_open_hit(id - PAN_ID_START_RES);
    }
}

/// Titik 2D untuk `DrawTextLayout`.
fn pt2(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

/// Byte C sampai NUL (buffer `char[]` dari C++).
fn c_bytes(buf: &[u8]) -> &[u8] {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    &buf[..end]
}

/// Padanan `StTextWrap` / `StTextWrapC` (w12start.inc:975 dan :989).
/// Teks dibungkus mulai dari atas kotak; `center` memilih rata tengah, `clip` memotong sisa baris.
fn st_layout(text_s: &str, f: Option<&IDWriteTextFormat>, l: f32, t: f32, r: f32, b: f32, c: D2D1_COLOR_F, center: bool, clip: bool) {
    let Some(f) = f else { return };
    let Some(dw) = DW.borrow().as_ref().cloned() else { return };
    let Some(rt) = RT.borrow().as_ref().cloned() else { return };
    let wide = to_wide(text_s);
    let Ok(lay) = (unsafe { dw.CreateTextLayout(&wide, f, r - l, b - t) }) else { return };
    unsafe {
        let _ = lay.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
        let _ = lay.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
        let align = if center { DWRITE_TEXT_ALIGNMENT_CENTER } else { DWRITE_TEXT_ALIGNMENT_LEADING };
        let _ = lay.SetTextAlignment(align);
    }
    set_color(c, 1.0);
    let opts = if clip { D2D1_DRAW_TEXT_OPTIONS_CLIP } else { D2D1_DRAW_TEXT_OPTIONS_NONE };
    if let Some(br) = BR.borrow().as_ref() {
        unsafe { rt.DrawTextLayout(pt2(l, t), &lay, br, opts) };
    }
}

/// Port `StTextWrap` (w12start.inc:975): teks terbungkus, rata kiri, tanpa pemotongan.
fn st_text_wrap(text_s: &str, f: &RefCell<Option<IDWriteTextFormat>>, l: f32, t: f32, r: f32, b: f32, c: D2D1_COLOR_F) {
    st_layout(text_s, f.borrow().as_ref(), l, t, r, b, c, false, false);
}

/// Port `StTextWrapC` (w12start.inc:989): teks terbungkus, rata tengah, dipotong di kotak.
fn st_text_wrap_c(text_s: &str, f: &RefCell<Option<IDWriteTextFormat>>, l: f32, t: f32, r: f32, b: f32, c: D2D1_COLOR_F) {
    st_layout(text_s, f.borrow().as_ref(), l, t, r, b, c, true, true);
}

/// Port `StartPinCell` (w12start.inc:1003): sel Disematkan, ikon 32 dan label satu baris.
fn start_pin_cell(ai: i32, x0: f32, y0: f32, x1: f32, y1: f32) {
    let p = PALETTE.borrow();
    pan_hover(x0, y0, x1, y1, sf(12.0), PAN_ID_START_PIN + ai);
    let t = y0 + (y1 - y0 - sf(58.0)) * 0.5;
    let mut apps = APPS.borrow_mut();
    let app = &mut apps[ai as usize];
    draw_icon_at(&mut app.icon, (x0 + x1) * 0.5 - sf(16.0), t, sf(32.0), 1.0);
    text(&app.name, TF_XSC.borrow().as_ref(), x0 + sf(2.0), t + sf(40.0), x1 - sf(2.0), t + sf(58.0), p.text, 1.0);
}

/// Port `StartRow` (w12start.inc:1011): baris aplikasi dengan ikon, nama, dan keterangan.
fn start_row(ai: i32, id: i32, x0: f32, y0: f32, x1: f32, y1: f32) {
    let p = PALETTE.borrow();
    pan_hover(x0, y0, x1, y1, sf(12.0), id);
    let mut apps = APPS.borrow_mut();
    let app = &mut apps[ai as usize];
    draw_icon_at(&mut app.icon, x0 + sf(12.0), y0 + sf(12.0), sf(28.0), 1.0);
    let nw = app.wins.len() as i32;
    let sub = if nw > 1 {
        format!("{} jendela", nw)
    } else if nw == 1 {
        "Berjalan".to_string()
    } else if app.pin >= 0 || app.spin != 0 {
        "Disematkan".to_string()
    } else {
        String::new()
    };
    text(&app.name, TF_S.borrow().as_ref(), x0 + sf(50.0), y0 + sf(8.0), x1 - sf(10.0), y0 + sf(28.0), p.text, 1.0);
    text(&sub, TF_SUB.borrow().as_ref(), x0 + sf(50.0), y0 + sf(28.0), x1 - sf(10.0), y0 + sf(46.0), p.sub, 1.0);
}

/// Data ponsel utama dari NectarLink (`g_nlDevices.primary`), disalin agar tidak memegang borrow.
struct PhoneInfo {
    has_android: bool,
    online: bool,
    /// `None` bila nama bukan UTF-8 valid (C++: MultiByteToWideChar gagal).
    name: Option<String>,
    link: String,
    has_battery: bool,
    battery: i32,
    charging: bool,
}

fn nl_phone_info() -> PhoneInfo {
    let g = NL_DEVICES.borrow();
    match g.as_ref() {
        Some(d) => PhoneInfo {
            has_android: d.has_android,
            online: d.primary.online,
            name: std::str::from_utf8(c_bytes(&d.primary.name)).ok().map(|x| x.to_string()),
            link: String::from_utf8_lossy(c_bytes(&d.primary.link)).into_owned(),
            has_battery: d.primary.has_battery,
            battery: d.primary.battery_level,
            charging: d.primary.charging,
        },
        None => PhoneInfo {
            has_android: false,
            online: false,
            name: None,
            link: String::new(),
            has_battery: false,
            battery: 0,
            charging: false,
        },
    }
}

/// Port `StartPhoneCard` (w12start.inc:1024): kartu Ponsel dari API lokal NectarLink.
fn start_phone_card(l: f32, t: f32, r: f32, b: f32) {
    let p = PALETTE.borrow();
    let inn = sf(18.0);
    fill_rr(l, t, r, b, sf(18.0), p.chip, 1.0);
    let ix = l + sf(18.0);
    let tx = l + sf(56.0);
    {
        let mut ic = ST_NL_ICON.borrow_mut();
        if ic.src.is_some() {
            draw_icon_at(&mut ic, ix, t + sf(18.0), sf(28.0), 1.0);
        } else {
            glyph(0xE8EA, TF_GLYPH_MD.borrow().as_ref(), ix + sf(14.0), t + sf(32.0), sf(12.0), p.glyph, 1.0);
        }
    }
    text("Ponsel", TF_T2.borrow().as_ref(), tx, t + sf(14.0), r - inn, t + sf(34.0), p.text, 1.0);
    let nd = nl_phone_info();
    let nl = ST_NL.get();
    let has_phone = NL_API_STATE.get() == NL_API_READY && nd.has_android;
    let phone_online = has_phone && nd.online;
    let st_text = if phone_online {
        "Ponsel terhubung"
    } else if nl == 2 {
        "NectarLink berjalan"
    } else if nl == 1 {
        "NectarLink tidak berjalan"
    } else {
        "NectarLink belum terpasang"
    };
    let dot = if phone_online { p.accent } else { p.sub2 };
    fill_rr(tx, t + sf(42.0), tx + sf(6.0), t + sf(48.0), sf(3.0), dot, 1.0);
    text(st_text, TF_SUB.borrow().as_ref(), tx + sf(12.0), t + sf(36.0), r - inn, t + sf(54.0), p.sub, 1.0);
    fill_rr(l + inn, t + sf(70.0), r - inn, t + sf(70.0) + 1.0, 0.0, p.sep, 1.0);
    if has_phone {
        let phone_name = nd.name.clone().unwrap_or_else(|| "Ponsel Android".to_string());
        text(&phone_name, TF_SSC.borrow().as_ref(), l + inn, t + sf(82.0), r - inn, t + sf(103.0), p.text, 1.0);
        let route = if !nd.online {
            "Tidak tersambung"
        } else if nd.link == "relay" {
            "Online · Relay internet"
        } else if nd.link == "lan" {
            "Online · Wi-Fi lokal"
        } else if nd.link == "tailscale" {
            "Online · Tailscale"
        } else {
            "Online · Terhubung"
        };
        text(route, TF_SC.borrow().as_ref(), l + inn, t + sf(104.0), r - inn, t + sf(124.0), p.sub, 1.0);
        let battery = if nd.has_battery {
            let suffix = if nd.charging { " · Mengisi daya" } else { "" };
            format!("Baterai {}%{}", nd.battery, suffix)
        } else {
            "Baterai tidak tersedia".to_string()
        };
        text(&battery, TF_SC.borrow().as_ref(), l + inn, t + sf(126.0), r - inn, t + sf(146.0), p.sub, 1.0);
    } else {
        let message = if nl == 0 {
            "NectarLink belum terpasang di PC ini."
        } else {
            match NL_API_STATE.get() {
                NL_API_LOADING => "Memuat status ponsel…",
                NL_API_NO_TOKEN => "Local API NectarLink mati. Buka NectarLink untuk mengaktifkannya.",
                NL_API_AUTH => "Token API tidak valid. Buka NectarLink untuk memperbarui akses.",
                NL_API_BAD_DATA => "Data ponsel tidak dapat dibaca.",
                NL_API_READY => "Belum ada ponsel Android yang terhubung.",
                _ => "API lokal NectarLink tidak merespons. Pastikan aplikasinya berjalan.",
            }
        };
        st_text_wrap(message, &TF_S, l + inn, t + sf(84.0), r - inn, t + sf(150.0), p.sub);
    }
    let b2b = b - sf(16.0);
    let b2t = b2b - sf(36.0);
    let b1b = b2t - sf(6.0);
    let b1t = b1b - sf(40.0);
    glyph(0xE8EA, TF_GLYPH_LG.borrow().as_ref(), (l + r) * 0.5, (t + sf(150.0) + b1t) * 0.5, sf(24.0), p.sub2, 0.6); // keadaan kosong
    if nl != 0 {
        fill_rr(l + inn, b1t, r - inn, b1b, sf(20.0), p.chip_hover, 1.0);
        pan_hover(l + inn, b1t, r - inn, b1b, sf(20.0), PAN_ID_START_NL);
        text("Buka NectarLink", TF_SSC.borrow().as_ref(), l + inn, b1t, r - inn, b1b, p.text, 1.0);
    }
    pan_hover(l + inn, b2t, r - inn, b2b, sf(18.0), PAN_ID_START_PHONE);
    text("Pengaturan perangkat seluler", TF_SC.borrow().as_ref(), l + inn, b2t, r - inn, b2b, p.sub, 1.0);
}

/// Port `StartFooter` (w12start.inc:1077): pengguna (avatar, nama) dan tombol Pengaturan serta Daya.
fn start_footer(w: f32, m: f32, foot_t: f32) {
    let p = PALETTE.borrow();
    let fy = foot_t + sf(30.0);
    fill_rr(m, foot_t, w - m, foot_t + sf(1.0), 0.0, p.sep, 1.0);
    pan_hover(m - sf(6.0), fy - sf(22.0), m + sf(260.0), fy + sf(22.0), sf(10.0), PAN_ID_START_USER);
    // avatar netral: aksen hanya untuk status hidup
    fill_rr(m, fy - sf(16.0), m + sf(32.0), fy + sf(16.0), sf(16.0), p.chip_hover, 1.0);
    let user_ok = ST_USER_OK.get();
    let user_buf: [u16; 128] = *ST_USER.borrow();
    let ini: String = if user_ok && user_buf[0] != 0 {
        let mut c0 = user_buf[0];
        if (b'a' as u16..=b'z' as u16).contains(&c0) {
            c0 -= 32;
        }
        String::from_utf16_lossy(&[c0])
    } else {
        "?".to_string()
    };
    text(&ini, TF_SSC.borrow().as_ref(), m, fy - sf(16.0), m + sf(32.0), fy + sf(16.0), p.text, 1.0);
    let name = if user_ok { from_wide(&user_buf) } else { "Akun".to_string() };
    text(&name, TF_B.borrow().as_ref(), m + sf(42.0), fy - sf(12.0), m + sf(260.0), fy + sf(12.0), p.text, 1.0);
    pan_hover(w - m - sf(88.0), fy - sf(20.0), w - m - sf(48.0), fy + sf(20.0), sf(10.0), PAN_ID_START_GEAR);
    glyph(0xE713, TF_GLYPH_MD.borrow().as_ref(), w - m - sf(68.0), fy, sf(10.0), p.glyph, 1.0);
    if ST_POWER.get() != 0 {
        fill_rr(w - m - sf(40.0), fy - sf(20.0), w - m, fy + sf(20.0), sf(10.0), p.active, 1.0);
    }
    pan_hover(w - m - sf(40.0), fy - sf(20.0), w - m, fy + sf(20.0), sf(10.0), PAN_ID_START_POWER);
    glyph(0xE7E8, TF_GLYPH_MD.borrow().as_ref(), w - m - sf(20.0), fy, sf(10.0), p.glyph, 1.0);
}

/// Port `StartPowerMenu` (w12start.inc:1099): menu Daya di atas tombolnya, digambar paling akhir.
fn start_power_menu(w: f32, h: f32, m: f32, foot_t: f32) {
    pan_hit(0.0, 0.0, w, h, PAN_ID_START_DISMISS);
    let p = PALETTE.borrow();
    let power = ST_POWER.get();
    let ask = power >= 2;
    let rs = power == 2;
    let r = w - m;
    let l = r - sf(220.0);
    let b = foot_t - sf(6.0);
    let t = b - if ask { sf(112.0) } else { sf(140.0) };
    for i in 0..4 {
        let e = sf((i * 2) as f32);
        fill_rr(l - e, t - e + sf(3.0), r + e, b + e + sf(3.0), sf(14.0) + e, p.shadow, 0.08);
    }
    surface(l, t, r, b, sf(14.0), p.panel);
    pan_hit(l, t, r, b, PAN_ID_START_PMENU);
    if !ask {
        let ids = [PAN_ID_START_SLEEP, PAN_ID_START_RESTART, PAN_ID_START_OFF];
        let gl: [u16; 3] = [0xE708, 0xE72C, 0xE7E8];
        let lb = ["Tidur", "Mulai ulang", "Matikan"];
        for i in 0..3 {
            let y0 = t + sf(8.0) + i as f32 * sf(42.0);
            let y1 = y0 + sf(40.0);
            pan_hover(l + sf(6.0), y0, r - sf(6.0), y1, sf(10.0), ids[i]);
            glyph(gl[i], TF_GLYPH_MD.borrow().as_ref(), l + sf(28.0), (y0 + y1) * 0.5, sf(10.0), p.glyph, 1.0);
            text(lb[i], TF_B.borrow().as_ref(), l + sf(48.0), y0, r - sf(12.0), y1, p.text, 1.0);
        }
        return;
    }
    let judul = if rs { "Mulai ulang sekarang?" } else { "Matikan sekarang?" };
    text(judul, TF_B.borrow().as_ref(), l + sf(16.0), t + sf(12.0), r - sf(16.0), t + sf(36.0), p.text, 1.0);
    text("Simpan pekerjaan Anda dulu.", TF_SUB.borrow().as_ref(), l + sf(16.0), t + sf(36.0), r - sf(16.0), t + sf(56.0), p.sub, 1.0);
    let by0 = b - sf(48.0);
    let by1 = b - sf(12.0);
    let mid = (l + r) * 0.5;
    fill_rr(l + sf(12.0), by0, mid - sf(4.0), by1, sf(18.0), p.chip, 1.0);
    pan_hover(l + sf(12.0), by0, mid - sf(4.0), by1, sf(18.0), PAN_ID_START_NO);
    text("Batal", TF_SSC.borrow().as_ref(), l + sf(12.0), by0, mid - sf(4.0), by1, p.text, 1.0);
    fill_rr(mid + sf(4.0), by0, r - sf(12.0), by1, sf(18.0), p.chip_hover, 1.0);
    pan_hover(mid + sf(4.0), by0, r - sf(12.0), by1, sf(18.0), PAN_ID_START_YES);
    let aksi = if rs { "Mulai ulang" } else { "Matikan" };
    text(aksi, TF_SSC.borrow().as_ref(), mid + sf(4.0), by0, r - sf(12.0), by1, p.text, 1.0);
}

/// Port `StTextHL` (w12start.inc:1134): teks satu baris dengan rentang `mpos..mpos+mlen` dicetak tebal.
/// `mpos` dan `mlen` dalam satuan UTF-16, sama seperti C++. `mpos < 0` berarti tanpa sorotan.
fn st_text_hl(s: &str, f: Option<&IDWriteTextFormat>, l: f32, t: f32, r: f32, b: f32, c: D2D1_COLOR_F, mpos: i32, mlen: i32) {
    let wide = to_wide(s);
    let n = wide.len() as i32;
    if mpos < 0 || mlen <= 0 || mpos >= n {
        text(s, f, l, t, r, b, c, 1.0);
        return;
    }
    let Some(f) = f else { return };
    let Some(dw) = DW.borrow().as_ref().cloned() else { return };
    let Some(rt) = RT.borrow().as_ref().cloned() else { return };
    let Ok(lay) = (unsafe { dw.CreateTextLayout(&wide, f, r - l, b - t) }) else {
        text(s, Some(f), l, t, r, b, c, 1.0);
        return;
    };
    let rg = DWRITE_TEXT_RANGE { startPosition: mpos as u32, length: mlen as u32 };
    unsafe {
        let _ = lay.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, rg);
    }
    set_color(c, 1.0);
    if let Some(br) = BR.borrow().as_ref() {
        unsafe { rt.DrawTextLayout(pt2(l, t), &lay, br, D2D1_DRAW_TEXT_OPTIONS_CLIP) };
    }
}

/// Port `g_stIconBudget` dan `g_stIconMore` (w12start.inc:1149-1150): jatah muat ikon per gambar.
static ST_ICON_BUDGET: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static ST_ICON_MORE: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

/// Port `StHitIconDraw` (w12start.inc:1152): ikon satu hasil pencarian, atau glyph bila tidak ada ikon.
fn st_hit_icon_draw(hh: &StHit, x: f32, y: f32, sz: f32) {
    let p = PALETTE.borrow();
    if hh.kind == ST_HIT_SETTING || hh.kind == ST_HIT_CALC || hh.kind == ST_HIT_CMD {
        let g: u16 = if hh.kind == ST_HIT_SETTING {
            0xE713
        } else if hh.kind == ST_HIT_CALC {
            0xE8EF
        } else {
            0xE756
        };
        glyph(g, TF_GLYPH_MD.borrow().as_ref(), x + sz * 0.5, y + sz * 0.5, sz * 0.56, p.glyph, 1.0);
        return;
    }
    let belum_dimuat = ST_IDX
        .borrow()
        .get(hh.reference as usize)
        .is_some_and(|ix| !ix.icon_tried);
    if hh.kind == ST_HIT_APP_INDEX && belum_dimuat {
        if ST_ICON_BUDGET.get() <= 0 {
            // belum dimuat: kotak bayangan, gambar berikutnya menyusul
            ST_ICON_MORE.set(true);
            fill_rr(x, y, x + sz, y + sz, sz * 0.25, p.chip, 1.0);
            return;
        }
        ST_ICON_BUDGET.set(ST_ICON_BUDGET.get() - 1);
    }
    // cukup tajam untuk pratinjau 56 dip
    let ada = st_hit_icon_with(hh, s(56.0), |ic| {
        if ic.src.is_some() {
            draw_icon_at(ic, x, y, sz, 1.0);
            true
        } else {
            false
        }
    });
    if !ada {
        // ikon aplikasi umum
        glyph(0xE71D, TF_GLYPH_MD.borrow().as_ref(), x + sz * 0.5, y + sz * 0.5, sz * 0.5, p.sub, 1.0);
    }
}

/// Port `StAppendParent` (w12start.inc:1173): menambah " · <nama folder induk>" dari jalur penuh berkas.
/// Jalur tanpa '\' tidak menambah apa pun. Batas 259 unit UTF-16 untuk nama folder seperti C++.
fn st_append_parent(dst: &mut String, path: &str) {
    let Some(end) = path.rfind('\\') else { return };
    let start = path[..end].rfind('\\').map_or(0, |i| i + 1);
    if end <= start {
        return;
    }
    let seg: Vec<u16> = path[start..end].encode_utf16().take(259).collect();
    dst.push_str(" \u{00B7} ");
    dst.push_str(&String::from_utf16_lossy(&seg));
}

/// Port `StWebIndexing` (w12start.inc:1190): kueri mencari berkas dan indeks belum siap, tanpa hit berkas.
fn st_web_indexing() -> bool {
    let q = from_wide(&ST_QUERY.borrow()[..]);
    let mut rest: &str = "";
    let mode = w12_query_classify(&q, Some(&mut rest));
    if (mode != W12Q_NORMAL && mode != W12Q_FILES) || rest.is_empty() {
        return false;
    }
    if !fs_indexing() {
        return false;
    }
    let n = ST_N_HITS.get().max(0) as usize;
    let hits = ST_HITS.borrow();
    if hits.iter().take(n).any(|h| h.kind == ST_HIT_FILE) {
        return false;
    }
    true
}

/// Port `StHitKind` (w12start.inc:1202): keterangan kecil di bawah nama hasil pencarian.
fn st_hit_kind(hh: &StHit) -> String {
    if hh.kind == ST_HIT_SETTING {
        return "Pengaturan Windows".to_string();
    }
    if hh.kind == ST_HIT_CALC {
        return "Kalkulator \u{00B7} Enter menyalin".to_string();
    }
    if hh.kind == ST_HIT_CMD {
        return "Perintah \u{00B7} Ctrl+Shift+Enter sebagai admin".to_string();
    }
    if hh.kind == ST_HIT_FILE {
        if let Some(fs) = st_fs_result() {
            if hh.reference >= 0 && hh.reference < fs.n {
                let idx = hh.reference as usize;
                let f = fs.flags[idx];
                let mut out = String::from(if f & crate::files::model::W12FX_DIR != 0 {
                    "Folder"
                } else if f & crate::files::model::W12FX_RECENT != 0 {
                    "Terbaru"
                } else {
                    "File"
                });
                let p = &fs.path[idx];
                let end = p.iter().position(|&c| c == 0).unwrap_or(p.len());
                let path = String::from_utf16_lossy(&p[..end]);
                st_append_parent(&mut out, &path);
                return out;
            }
        }
    }
    if hh.kind == ST_HIT_APP_LIST {
        let apps = APPS.borrow();
        if let Some(a) = apps.get(hh.reference as usize) {
            if !a.wins.is_empty() {
                return "Aplikasi \u{2022} Berjalan".to_string();
            }
            if a.pin >= 0 || a.spin != 0 {
                return "Aplikasi \u{2022} Disematkan".to_string();
            }
        }
    }
    "Aplikasi".to_string()
}

/// Port `StResRow` (w12start.inc:1223): satu baris hasil cari. `k == ST_N_HITS` adalah baris "Cari di web".
fn st_res_row(k: i32, x0: f32, y0: f32, x1: f32, y1: f32, big: bool) {
    let p = PALETTE.borrow();
    let sel = k == ST_SEL.get();
    if sel {
        fill_rr(x0, y0, x1, y1, sf(10.0), p.chip_hover, 1.0);
        let cy = (y0 + y1) * 0.5;
        let tinggi = sf(if big { 10.0 } else { 8.0 });
        fill_rr(x0, cy - tinggi, x0 + sf(3.0), cy + tinggi, sf(1.5), p.accent, 1.0);
    }
    let n_hits = ST_N_HITS.get();
    let id = if k == n_hits { PAN_ID_START_FILES } else { PAN_ID_START_RES + k };
    pan_hover(x0, y0, x1, y1, sf(10.0), id);
    if k == n_hits {
        // label memakai teks setelah prefiks; sub saat indeks belum siap
        let q = from_wide(&ST_QUERY.borrow()[..]);
        let mut rest: &str = "";
        w12_query_classify(&q, Some(&mut rest));
        let label_penuh = format!("Cari di web: \u{201C}{}\u{201D}", rest);
        // C++ memakai buffer START_MAXQ + 40 wchar termasuk NUL
        let wide: Vec<u16> = label_penuh.encode_utf16().take(START_MAXQ as usize + 39).collect();
        let label = String::from_utf16_lossy(&wide);
        glyph(0xE774, TF_GLYPH_MD.borrow().as_ref(), x0 + sf(26.0), (y0 + y1) * 0.5, sf(10.0), p.glyph, 1.0);
        if st_web_indexing() {
            text(&label, TF_S.borrow().as_ref(), x0 + sf(48.0), y0 + sf(3.0), x1 - sf(30.0), y0 + sf(21.0), p.text, 1.0);
            text("Mengindeks file\u{2026}", TF_SUB.borrow().as_ref(), x0 + sf(48.0), y0 + sf(21.0), x1 - sf(30.0), y1 - sf(1.0), p.sub, 1.0);
        } else {
            text(&label, TF_S.borrow().as_ref(), x0 + sf(48.0), y0, x1 - sf(10.0), y1, p.text, 1.0);
        }
        glyph(0xE76C, TF_GLYPH_SM.borrow().as_ref(), x1 - sf(18.0), (y0 + y1) * 0.5, sf(8.0), p.sub, 1.0);
        return;
    }
    // salin hit agar borrow ST_HITS selesai sebelum menggambar
    let hh: StHit = {
        let hits = ST_HITS.borrow();
        match hits.get(k as usize) {
            Some(h) => *h,
            None => return,
        }
    };
    if big {
        st_hit_icon_draw(&hh, x0 + sf(14.0), y0 + (y1 - y0 - sf(36.0)) * 0.5, sf(36.0));
        let nama = st_hit_name(&hh);
        st_text_hl(&nama, TF_B.borrow().as_ref(), x0 + sf(62.0), y0 + sf(12.0), x1 - sf(12.0), y0 + sf(34.0), p.text, hh.match_pos, hh.match_len);
        let ket = st_hit_kind(&hh);
        text(&ket, TF_SUB.borrow().as_ref(), x0 + sf(62.0), y0 + sf(34.0), x1 - sf(12.0), y0 + sf(52.0), p.sub, 1.0);
    } else {
        st_hit_icon_draw(&hh, x0 + sf(14.0), y0 + (y1 - y0 - sf(24.0)) * 0.5, sf(24.0));
        let nama = st_hit_name(&hh);
        st_text_hl(&nama, TF_S.borrow().as_ref(), x0 + sf(48.0), y0, x1 - sf(10.0), y1, p.text, hh.match_pos, hh.match_len);
    }
}

/// Susun teks "“<rest>”" dengan batas `START_MAXQ + 39` unit UTF-16 seperti buffer C++.
fn st_query_quoted_label(rest: &str) -> String {
    let penuh = format!("\u{201C}{}\u{201D}", rest);
    let wide: Vec<u16> = penuh.encode_utf16().take(START_MAXQ as usize + 39).collect();
    String::from_utf16_lossy(&wide)
}

/// Port `StPreview` (w12start.inc:1263): pratinjau kolom kanan untuk baris hasil terpilih.
fn st_preview(l: f32, t: f32, r: f32, b: f32) {
    let p = PALETTE.borrow();
    fill_rr(l, t, r, b, sf(18.0), p.chip, 1.0);
    let cx = (l + r) * 0.5;
    let inn = sf(18.0);
    let sel = ST_SEL.get();
    let files = sel >= ST_N_HITS.get();
    let hit: Option<StHit> = if files { None } else { ST_HITS.borrow().get(sel as usize).copied() };
    let mut y = t + sf(36.0);
    if files {
        glyph(0xE774, TF_GLYPH_LG.borrow().as_ref(), cx, y + sf(28.0), sf(28.0), p.glyph, 1.0);
    } else if let Some(hh) = hit.as_ref() {
        st_hit_icon_draw(hh, cx - sf(28.0), y, sf(56.0));
    }
    y += sf(72.0);
    let judul = if files {
        let q = from_wide(&ST_QUERY.borrow()[..]);
        let mut rest: &str = "";
        w12_query_classify(&q, Some(&mut rest));
        st_query_quoted_label(rest)
    } else {
        hit.as_ref().map(st_hit_name).unwrap_or_default()
    };
    st_text_wrap_c(&judul, &TF_MEDC, l + inn, y, r - inn, y + sf(44.0), p.text);
    y += sf(46.0);
    let ket = if files {
        "Buka di browser".to_string()
    } else {
        hit.as_ref().map(st_hit_kind).unwrap_or_default()
    };
    text(&ket, TF_SC.borrow().as_ref(), l + inn, y, r - inn, y + sf(18.0), p.sub, 1.0);
    y += sf(34.0);
    fill_rr(l + inn, y, r - inn, y + 1.0, 0.0, p.sep, 1.0);
    y += sf(14.0);
    let bh = sf(40.0);
    fill_rr(l + inn, y, r - inn, y + bh, sf(20.0), p.chip_hover, 1.0);
    pan_hover(l + inn, y, r - inn, y + bh, sf(20.0), PAN_ID_START_OPEN);
    let ikon: u16 = if files { 0xE721 } else { 0xE8A7 };
    glyph(ikon, TF_GLYPH_SM.borrow().as_ref(), l + inn + sf(22.0), y + bh * 0.5, sf(8.0), p.glyph, 1.0);
    let aksi = if files { "Cari" } else { "Buka" };
    text(aksi, TF_SSC.borrow().as_ref(), l + inn, y, r - inn, y + bh, p.text, 1.0);
    text("\u{2191} \u{2193} pilih  \u{2022}  Enter buka", TF_XSC.borrow().as_ref(), l + inn, b - sf(34.0), r - inn, b - sf(14.0), p.sub2, 1.0);
}

/// Port `StartQueryDraw` (w12start.inc:1295): kotak kueri dengan kursor, atau placeholder bila kosong.
fn start_query_draw(m: f32, top: f32, lx1: f32) {
    let p = PALETTE.borrow();
    let q_len = ST_Q_LEN.get();
    if q_len == 0 {
        text(
            "Ketik untuk mencari aplikasi, pengaturan, atau file",
            TF_B.borrow().as_ref(),
            m + sf(44.0),
            top,
            lx1 - sf(14.0),
            top + sf(44.0),
            p.sub,
            1.0,
        );
        return;
    }
    let x = m + sf(44.0);
    let right = lx1 - sf(14.0);
    let q_buf: Vec<u16> = {
        let qb = ST_QUERY.borrow();
        let n = (q_len.max(0) as usize).min(qb.len());
        qb[..n].to_vec()
    };
    // lebar teks, None bila layout gagal dibuat (C++: `measured` = false)
    let mut lebar: Option<f32> = None;
    if let (Some(dw), Some(f)) = (DW.borrow().as_ref().cloned(), TF_B.borrow().as_ref()) {
        if let Ok(lay) = unsafe { dw.CreateTextLayout(&q_buf, f, sf(2000.0), sf(44.0)) } {
            // PORT: GetMetrics di binding windows 0.61 memakai out-parameter, bukan nilai kembalian.
            let mut tm = DWRITE_TEXT_METRICS::default();
            if unsafe { lay.GetMetrics(&mut tm) }.is_ok() {
                lebar = Some(tm.widthIncludingTrailingWhitespace);
            }
        }
    }
    let semua = ST_QUERY_ALL.get();
    if semua {
        if let Some(wd) = lebar {
            let width = wd + sf(4.0);
            let clipped = if width < right - x { width } else { right - x };
            if clipped > 0.0 {
                fill_rr(x - sf(2.0), top + sf(8.0), x + clipped, top + sf(36.0), sf(4.0), p.accent, 1.0);
            }
        }
    }
    let teks = from_wide(&q_buf);
    let warna = if semua { p.accent_ink } else { p.text };
    text(&teks, TF_B.borrow().as_ref(), x, top, right, top + sf(44.0), warna, 1.0);
    if !semua {
        if let Some(wd) = lebar {
            let cx = x + wd + sf(2.0);
            if cx < right {
                fill_rr(cx, top + sf(13.0), cx + sf(1.5), top + sf(31.0), 0.0, p.accent, 1.0);
            }
        }
    }
}

/// Port `StartAllAppsDraw` (w12start.inc:1320): daftar "Semua aplikasi", huruf pintas di tepi kanan,
/// lalu kartu Ponsel, footer, dan menu Daya.
fn start_all_apps_draw(w: f32, h: f32, m: f32, lx1: f32, px0: f32, px1: f32, top: f32, sec_y: f32, body_bot: f32, foot_t: f32) {
    st_all_apps_ensure();
    let p = PALETTE.borrow();
    text("Semua aplikasi", TF_SS.borrow().as_ref(), m, sec_y, lx1, sec_y + sf(18.0), p.sub, 1.0);
    let bx0 = lx1 - sf(82.0);
    let by0 = sec_y - sf(4.0);
    let by1 = by0 + sf(26.0);
    fill_rr(bx0, by0, lx1, by1, sf(13.0), p.chip_hover, 1.0);
    pan_hover(bx0, by0, lx1, by1, sf(13.0), PAN_ID_START_HOME);
    text("Beranda", TF_SSC.borrow().as_ref(), bx0, by0, lx1, by1, p.text, 1.0);

    let list_top = sec_y + sf(26.0);
    let list_bottom = body_bot - sf(2.0);
    let row_h = sf(36.0);
    let row_step = sf(40.0);
    let strip_w = sf(20.0);
    let list_right = lx1 - strip_w - sf(4.0);
    ST_ALL_ROW_STEP.set(row_step.round() as i32);
    ST_ALL_VIEW_HEIGHT.set((list_bottom - list_top).round() as i32);
    st_all_apps_clamp_scroll();
    let n_apps = ST_ALL_N_APPS.get();
    if n_apps == 0 {
        let busy = ST_IDX_BUSY.load(std::sync::atomic::Ordering::SeqCst) && ST_IDX.borrow().is_empty();
        let pesan = if busy { "Memuat daftar aplikasi\u{2026}" } else { "Aplikasi belum ditemukan" };
        text(pesan, TF_S.borrow().as_ref(), m, list_top, list_right, list_top + sf(22.0), p.sub, 1.0);
    } else {
        let scroll = ST_ALL_SCROLL.get();
        let step = ST_ALL_ROW_STEP.get();
        let range = w12_start_visible_items(n_apps, scroll, step, ST_ALL_VIEW_HEIGHT.get());
        let selected = ST_ALL_SELECTED.get();
        for i in range.first..range.last {
            let y = list_top + (i * step - scroll) as f32;
            if y + row_h <= list_top || y >= list_bottom {
                continue;
            }
            let x0 = m;
            let x1 = list_right;
            let id = PAN_ID_START_ALL_APP + i;
            if selected == i {
                fill_rr(x0, y, x1, y + row_h, sf(9.0), p.chip_hover, 1.0);
                fill_rr(x0, y + sf(9.0), x0 + sf(2.0), y + sf(27.0), sf(1.0), p.accent, 1.0);
            }
            pan_hover(x0, y, x1, y + row_h, sf(9.0), id);
            let rf: StAllAppRef = match ST_ALL_APPS.borrow().get(i as usize) {
                Some(r) => *r,
                None => continue,
            };
            if rf.kind == 0 {
                let hit = StHit {
                    kind: ST_HIT_APP_INDEX,
                    reference: rf.reference,
                    score: 0,
                    match_pos: -1,
                    match_len: 0,
                    order: 0,
                };
                st_hit_icon_draw(&hit, x0 + sf(9.0), y + sf(7.0), sf(22.0));
            } else {
                let drawn = {
                    let mut apps = APPS.borrow_mut();
                    match apps.get_mut(rf.reference as usize) {
                        Some(app) if app.icon.src.is_some() => {
                            draw_icon_at(&mut app.icon, x0 + sf(9.0), y + sf(7.0), sf(22.0), 1.0);
                            true
                        }
                        _ => false,
                    }
                };
                if !drawn {
                    glyph(0xE71D, TF_GLYPH_MD.borrow().as_ref(), x0 + sf(20.0), y + sf(18.0), sf(11.0), p.sub, 1.0);
                }
            }
            let nama = st_all_app_name(rf);
            text(&nama, TF_S.borrow().as_ref(), x0 + sf(42.0), y, x1 - sf(8.0), y + row_h, p.text, 1.0);
        }
    }

    let letter_h = (list_bottom - list_top) / 26.0;
    let letter_x = lx1 - strip_w;
    for i in 0..26 {
        let y = list_top + i as f32 * letter_h;
        pan_hover(letter_x, y, lx1, y + letter_h, sf(4.0), PAN_ID_START_ALPHA + i);
        let huruf = ((b'A' + i as u8) as char).to_string();
        text(&huruf, TF_XSC.borrow().as_ref(), letter_x, y, lx1, y + letter_h, p.sub, 1.0);
    }
    start_phone_card(px0, top, px1, body_bot);
    start_footer(w, m, foot_t);
    if ST_POWER.get() != 0 {
        start_power_menu(w, h, m, foot_t);
    }
    if ST_ICON_MORE.get() && !PAN.get().0.is_null() {
        let _ = w12_invalidate(PAN.get(), None, false);
    }
}

/// Port `StartDraw` (w12start.inc:1378): seluruh isi panel Start. Dipanggil dari panel.rs (`start_draw`).
pub fn start_draw(w: f32, h: f32) {
    start_build_lists();
    ST_ICON_BUDGET.set(3);
    ST_ICON_MORE.set(false);
    let m = sf(22.0);
    let cg = sf(8.0);
    let top = sf(20.0);
    let col_gap = sf(22.0);
    let phone_w = sf(272.0);
    let lx1 = w - m - phone_w - col_gap; // sisi kanan kolom kiri
    let px0 = w - m - phone_w; // kartu Ponsel
    let px1 = w - m;
    let body_bot = h - sf(72.0);
    let foot_t = h - sf(60.0);
    let sec_y = top + sf(66.0);
    let list_y = sec_y + sf(22.0);
    let row_h = sf(52.0);
    let row_gap = sf(6.0);
    // Terbaru 2x2, bawah = bodyBot
    let rec_lbl = body_bot - sf(24.0) - 2.0 * row_h - row_gap;
    let rec_y = rec_lbl + sf(24.0);
    let rw = (lx1 - m - cg) / 2.0; // lebar satu kolom baris
    let p = PALETTE.borrow();

    // kotak cari
    fill_rr(m, top, lx1, top + sf(44.0), sf(22.0), p.chip, 1.0);
    pan_hover(m, top, lx1, top + sf(44.0), sf(22.0), PAN_ID_START_Q);
    glyph(0xE721, TF_GLYPH.borrow().as_ref(), m + sf(22.0), top + sf(22.0), sf(10.0), p.sub, 1.0);
    start_query_draw(m, top, lx1);

    if ST_Q_LEN.get() != 0 {
        let mut y = sec_y;
        let list_bot = body_bot;
        let n_hits = ST_N_HITS.get();
        if n_hits != 0 {
            text("Kecocokan terbaik", TF_SS.borrow().as_ref(), m, y, lx1, y + sf(18.0), p.sub, 1.0);
            y += sf(24.0);
            st_res_row(0, m, y, lx1, y + sf(64.0), true);
            y += sf(64.0) + sf(12.0);
            if n_hits > 1 {
                text("Aplikasi", TF_SS.borrow().as_ref(), m, y, lx1, y + sf(18.0), p.sub, 1.0);
                y += sf(24.0);
            }
            let rh = sf(40.0);
            // jaga baris terpilih tetap tampak: geser jendela daftar bila pilihan di bawah batas
            let fit = ((list_bot - sf(52.0) - y) / (rh + sf(2.0))) as i32;
            let sel = ST_SEL.get();
            let mut first = 1;
            if fit > 0 && sel < n_hits && sel >= first + fit {
                first = sel - fit + 1;
            }
            for k in first..n_hits.min(first + fit) {
                st_res_row(k, m, y, lx1, y + rh, false);
                y += rh + sf(2.0);
            }
        } else {
            let busy = ST_IDX_BUSY.load(std::sync::atomic::Ordering::SeqCst) && ST_IDX.borrow().is_empty();
            let pesan = if busy { "Memuat daftar aplikasi\u{2026}" } else { "Tidak ada aplikasi yang cocok" };
            text(pesan, TF_S.borrow().as_ref(), m, y, lx1, y + sf(20.0), p.sub, 1.0);
            y += sf(30.0);
        }
        y += sf(6.0);
        fill_rr(m + sf(10.0), y, lx1 - sf(10.0), y + 1.0, 0.0, p.sep, 1.0);
        y += sf(8.0);
        st_res_row(n_hits, m, y, lx1, y + sf(40.0), false);
        st_preview(px0, top, px1, body_bot);
        start_footer(w, m, foot_t);
        if ST_POWER.get() != 0 {
            start_power_menu(w, h, m, foot_t);
        }
        // sisa ikon dimuat di gambar berikutnya
        if ST_ICON_MORE.get() && !PAN.get().0.is_null() {
            let _ = w12_invalidate(PAN.get(), None, false);
        }
        return;
    }

    if ST_ALL_APPS_MODE.get() {
        start_all_apps_draw(w, h, m, lx1, px0, px1, top, sec_y, body_bot, foot_t);
        return;
    }

    text("Disematkan", TF_SS.borrow().as_ref(), m, sec_y, lx1, sec_y + sf(18.0), p.sub, 1.0);
    let all_x0 = lx1 - sf(108.0);
    let all_y0 = sec_y - sf(4.0);
    fill_rr(all_x0, all_y0, lx1, all_y0 + sf(26.0), sf(13.0), p.chip_hover, 1.0);
    pan_hover(all_x0, all_y0, lx1, all_y0 + sf(26.0), sf(13.0), PAN_ID_START_ALL_VIEW);
    text("Semua aplikasi", TF_SSC.borrow().as_ref(), all_x0, all_y0, lx1, all_y0 + sf(26.0), p.text, 1.0);
    let pins: Vec<i32> = ST_PIN.borrow().clone();
    if pins.is_empty() {
        text("Belum ada aplikasi yang disematkan", TF_S.borrow().as_ref(), m, list_y, lx1, list_y + sf(20.0), p.sub, 1.0);
    }
    let cell_w = (lx1 - m - 5.0 * cg) / 6.0;
    let cell_h = (rec_lbl - sf(24.0) - list_y - 2.0 * cg) / 3.0;
    for (i, &ai) in pins.iter().enumerate() {
        let x0 = m + (i % 6) as f32 * (cell_w + cg);
        let y0 = list_y + (i / 6) as f32 * (cell_h + cg);
        start_pin_cell(ai, x0, y0, x0 + cell_w, y0 + cell_h);
    }
    text("Terbaru", TF_SS.borrow().as_ref(), m, rec_lbl, lx1, rec_lbl + sf(18.0), p.sub, 1.0);
    let recs: Vec<i32> = ST_REC.borrow().clone();
    if recs.is_empty() {
        text("Belum ada aplikasi yang berjalan", TF_S.borrow().as_ref(), m, rec_y, lx1, rec_y + sf(20.0), p.sub, 1.0);
    }
    for (i, &ai) in recs.iter().take(4).enumerate() {
        let y0 = rec_y + (i / 2) as f32 * (row_h + row_gap);
        let x0 = m + (i % 2) as f32 * (rw + cg);
        start_row(ai, PAN_ID_START_REC + ai, x0, y0, x0 + rw, y0 + row_h);
    }
    start_phone_card(px0, top, px1, body_bot);
    start_footer(w, m, foot_t);
    if ST_POWER.get() != 0 {
        start_power_menu(w, h, m, foot_t);
    }
}

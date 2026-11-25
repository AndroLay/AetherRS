//! Sumber: win12shell.cpp:3961-4297 dan 4316-4530 (DockIconRect s.d. TrimWorkingSet; baris 4298-4315 = include .inc).
//! Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H03.
use super::prelude::*;
use std::time::Instant;
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
    DWMWA_CLOAKED, DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
    DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount};
use windows::Win32::System::SystemServices::{MK_CONTROL, MK_LBUTTON};
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;

// ---------------------------------------------------------------- hit-test ikon dok (win12shell.cpp:3961)

/// Sumber: `DockIconRect` (:3961). Mengisi `r` dengan kotak layar ikon dok milik jendela `w`.
pub fn dock_icon_rect(w: HWND, r: &mut RECT) -> bool {
    let n = NITEMS.get().max(0) as usize;
    let items = ITEMS.borrow();
    let pills = PILLS.borrow();
    let apps = APPS.borrow();
    for it in items.iter().take(n) {
        if it.ty != IT_APP || it.pill < 0 || it.pill >= NBOTTOM {
            continue;
        }
        let Some(p) = pills.get(it.pill as usize).copied() else {
            continue;
        };
        if !p.shown {
            continue;
        }
        let Some(a) = apps.get(it.app as usize) else {
            continue;
        };
        if a.wins.contains(&w) {
            r.left = p.x + it.x as i32;
            r.top = p.y;
            r.right = p.x + (it.x + it.w) as i32;
            r.bottom = p.y + p.ht;
            return true;
        }
    }
    false
}

/// Sumber: `AnimWndProc` (:3977). Jendela animasi transparan dan tidak pernah aktif.
pub unsafe extern "system" fn anim_wnd_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_NCHITTEST {
        return LRESULT(HTTRANSPARENT as isize);
    }
    if m == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

/// Sumber: `MinAnimStop` (:3984).
pub fn min_anim_stop() {
    let _ = unsafe { KillTimer(Some(MSG.get()), T_MINANIM as usize) };
    let thumb = ANIM_THUMB.get();
    if thumb != 0 {
        let _ = unsafe { DwmUnregisterThumbnail(thumb) };
        ANIM_THUMB.set(0);
    }
    let aw = ANIM_WND.get();
    if !aw.0.is_null() {
        let _ = unsafe { ShowWindow(aw, SW_HIDE) };
    }
}

/// Sumber: `MinAnimStep` (:3991). Satu langkah animasi: thumbnail diskala dari jendela ke ikon dok.
pub fn min_anim_step() {
    if ANIM_THUMB.get() == 0 {
        min_anim_stop();
        return;
    }
    // Selisih DWORD bersifat wrap-around, sama seperti pengurangan unsigned di C.
    let elapsed = unsafe { GetTickCount() }.wrapping_sub(ANIM_START.get()) as f32;
    let t = elapsed / 300.0;
    if t >= 1.0 {
        min_anim_stop();
        return;
    }
    // Percepatan seperti efek "scale" macOS.
    let e = t * t * (1.6 - 0.6 * t);
    let from = ANIM_FROM.get();
    let to = ANIM_TO.get();
    let mix = |a: i32, b: i32| a + ((b - a) as f32 * e) as i32;
    let rect = RECT {
        left: mix(from.left, to.left),
        top: mix(from.top, to.top),
        right: mix(from.right, to.right),
        bottom: mix(from.bottom, to.bottom),
    };
    let props = DWM_THUMBNAIL_PROPERTIES {
        dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_OPACITY | DWM_TNP_VISIBLE | DWM_TNP_SOURCECLIENTAREAONLY,
        rcDestination: rect,
        opacity: (255.0 * if t < 0.75 { 1.0 } else { (1.0 - t) / 0.25 }) as u8,
        fVisible: BOOL(1),
        fSourceClientAreaOnly: BOOL(0),
        ..unsafe { std::mem::zeroed() }
    };
    let _ = unsafe { DwmUpdateThumbnailProperties(ANIM_THUMB.get(), &props) };
}

/// Sumber: `RaisePills` (:4014). Pil dok dan pil atas tetap di atas thumbnail animasi.
/// Handle disalin lebih dulu agar `PILLS` tidak dipinjam saat `SetWindowPos` mengirim pesan sinkron.
fn raise_pills() {
    let targets: Vec<HWND> = {
        let pills = PILLS.borrow();
        pills
            .iter()
            .enumerate()
            .take(MAXPILLS as usize)
            .filter(|(i, p)| !p.h.0.is_null() && !desk_layer(*i as i32))
            .map(|(_, p)| p.h)
            .collect()
    };
    for h in targets {
        let _ = unsafe {
            SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
        };
    }
}

// ---------------------------------------------------------------- alas adaptif (win12shell.cpp:4020-4118)

/// Sumber: `g_base`, `g_baseWant`, `g_baseA`, `g_baseFrom`, `g_baseT0` (:4025-4028).
/// `g_baseT0` (LARGE_INTEGER dari QueryPerformanceCounter) diganti `Instant`; fungsinya sama.
pub static BASE_WND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
pub static BASE_WANT: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static BASE_A: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
pub static BASE_FROM: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
pub static BASE_T0: SyncCell<Cell<Option<Instant>>> = SyncCell::new(Cell::new(None));
/// Sumber: `enum { BASE_MS = 170 }` (:4029).
pub const BASE_MS: f64 = 170.0;

/// Sumber: `ZoomedOnPrimary` (:4031). Jendela maksimal yang terlihat, tidak tercloak, di monitor utama.
pub(crate) fn zoomed_on_primary(w: HWND) -> bool {
    if w.0.is_null() {
        return false;
    }
    unsafe {
        if !IsWindowVisible(w).as_bool() || IsIconic(w).as_bool() || !IsZoomed(w).as_bool() {
            return false;
        }
        let mut cloaked: u32 = 0;
        if DwmGetWindowAttribute(
            w,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return false; // desktop virtual lain
        }
        let origin = POINT { x: 0, y: 0 };
        MonitorFromWindow(w, MONITOR_DEFAULTTONULL) == MonitorFromPoint(origin, MONITOR_DEFAULTTOPRIMARY)
    }
}

/// Sumber: `BaseRender` (:4040). Menggambar alas kaca, lalu menampilkannya dengan alfa smoothstep dari `BASE_A`.
fn base_render() {
    let base = BASE_WND.get();
    if base.0.is_null() {
        return;
    }
    let Some(rt) = RT.borrow().as_ref().cloned() else {
        return;
    };
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let rh = reserve_h();
    let wr = RECT { left: 0, top: sh - rh, right: sw, bottom: sh };
    ensure_mem(sw, rh);
    let br = RECT { left: 0, top: 0, right: sw, bottom: rh };
    if unsafe { rt.BindDC(MEMDC.get(), &br) }.is_err() {
        return;
    }
    let pal = *PALETTE.borrow();
    let (swf, rhf) = (sw as f32, rh as f32);
    unsafe {
        rt.BeginDraw();
        rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
    }
    mica_begin(base, &wr);
    mica_fill(0.0, 0.0, swf, rhf, 0.0);
    fill_rr(0.0, 0.0, swf, rhf, 0.0, pal.veil, 1.0);
    // Garis rambut di atas alas: warna terang untuk mode terang, putih transparan untuk mode gelap.
    let rule = if LIGHT.get() { pal.line } else { rgba(255, 255, 255, 0.12) };
    fill_rr(0.0, 0.0, swf, 1.0, 0.0, rule, 1.0);
    if let Err(e) = unsafe { rt.EndDraw(None, None) } {
        if e.code() == D2DERR_RECREATE_TARGET {
            create_target();
            return;
        }
    }
    let a = BASE_A.get().clamp(0.0, 1.0);
    let e = a * a * (3.0 - 2.0 * a); // smoothstep
    layer_present(base, &wr, (255.0 * e) as u8);
}

/// Sumber: `BaseProc` (:4060).
pub unsafe extern "system" fn base_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER => {
            let elapsed_ms = BASE_T0.get().map_or(0.0, |t0| t0.elapsed().as_secs_f64() * 1000.0);
            let mut u = (elapsed_ms / BASE_MS) as f32;
            if u > 1.0 {
                u = 1.0;
            }
            let target = if BASE_WANT.get() { 1.0 } else { 0.0 };
            let from = BASE_FROM.get();
            BASE_A.set(from + (target - from) * u);
            if u >= 1.0 {
                let _ = unsafe { KillTimer(Some(h), 1) };
                if !BASE_WANT.get() {
                    let _ = unsafe { ShowWindow(h, SW_HIDE) };
                    return LRESULT(0);
                }
            }
            base_render();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

/// Sumber: `BaseUpdate` (:4084). `force`: gambar ulang walau keadaan sama (wallpaper, tema, mode, DPI berubah).
/// Dipanggil oleh WinEventProc, timer, dan panel.rs (lihat PORT-MAP / shell-split.md:64).
pub fn base_update(force: bool) {
    let mut want = MODE.get() != MODE_CLASSIC
        && (!FULLSCREEN.get() || FULLSCREEN_START_PEEK.get())
        && !QUITTING.get();
    if want {
        want = zoomed_on_primary(unsafe { GetForegroundWindow() });
        if !want {
            // Salin dulu daftar jendela agar peminjaman APPS tidak aktif saat memanggil Win32.
            let cands: Vec<HWND> = {
                let apps = APPS.borrow();
                apps.iter()
                    .flat_map(|a| a.wins.iter().copied())
                    .collect()
            };
            want = cands.into_iter().any(zoomed_on_primary);
        }
    }
    if want == BASE_WANT.get() && !force {
        return;
    }
    if want && BASE_WND.get().0.is_null() {
        let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(base_proc);
        wc.hInstance = HI.get();
        wc.lpszClassName = w!("Win12ShellBase");
        let _ = unsafe { RegisterClassExW(&wc) };
        let created = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                w!("Win12ShellBase"),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(HI.get()),
                None,
            )
        };
        if let Ok(h) = created {
            BASE_WND.set(h);
        }
    }
    let base = BASE_WND.get();
    if base.0.is_null() {
        return;
    }
    if want == BASE_WANT.get() {
        // force tanpa perubahan keadaan
        if want && unsafe { IsWindowVisible(base) }.as_bool() {
            base_render();
        }
        return;
    }
    BASE_WANT.set(want);
    BASE_FROM.set(BASE_A.get());
    BASE_T0.set(Some(Instant::now()));
    if want && !unsafe { IsWindowVisible(base) }.as_bool() {
        BASE_A.set(0.0);
        BASE_FROM.set(0.0);
        base_render(); // isi dan ukuran dipasang dulu pada alfa 0
        let _ = unsafe {
            SetWindowPos(
                base,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        };
        raise_pills(); // pil tetap di atas alas
    }
    let _ = unsafe { SetTimer(Some(base), 1, 16, None) };
}

// ---------------------------------------------------------------- animasi minimize (win12shell.cpp:4120-4157)

/// Stempel `GetTickCount` terakhir untuk `min_anim_start` (C++: `static DWORD lastTick` lokal).
static MIN_ANIM_LAST_TICK: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));

/// Sumber: `MinAnimStart` (:4120). Memulai animasi jendela yang diminimalkan menuju ikon dok.
pub fn min_anim_start(w: HWND) {
    let aw = ANIM_WND.get();
    if aw.0.is_null() || FULLSCREEN.get() {
        return;
    }
    // Win+D / Win+M meminimalkan banyak jendela sekaligus: animasi per jendela saling menimpa dan tampak aneh.
    // Dilewati saat ada rentetan minimize (400 ms) atau tombol Win + D/M sedang ditekan.
    let now = unsafe { GetTickCount() };
    let burst = now.wrapping_sub(MIN_ANIM_LAST_TICK.get()) < 400;
    MIN_ANIM_LAST_TICK.set(now);
    // GetAsyncKeyState: bit tinggi (tekan) = nilai negatif pada i16.
    let key_down = |vk: i32| unsafe { GetAsyncKeyState(vk) } < 0;
    let win_held = key_down(VK_LWIN.0 as i32) || key_down(VK_RWIN.0 as i32);
    let dm_held = key_down(b'D' as i32) || key_down(b'M' as i32);
    if burst || (win_held && dm_held) {
        return;
    }
    let mut icon = RECT::default();
    if !dock_icon_rect(w, &mut icon) {
        return;
    }
    let mut wpl: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    wpl.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    if unsafe { GetWindowPlacement(w, &mut wpl) }.is_err() {
        return;
    }
    let mut from = wpl.rcNormalPosition;
    if (wpl.flags.0 & WPF_RESTORETOMAXIMIZED.0) != 0 {
        let mut mi: MONITORINFO = unsafe { std::mem::zeroed() };
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let mon = unsafe { MonitorFromWindow(w, MONITOR_DEFAULTTOPRIMARY) };
        if unsafe { GetMonitorInfoW(mon, &mut mi) }.as_bool() {
            from = mi.rcWork;
        }
    }
    let fw = from.right - from.left;
    let fh = from.bottom - from.top;
    if fw <= 0 || fh <= 0 {
        return;
    }
    min_anim_stop();
    let thumb = match unsafe { DwmRegisterThumbnail(aw, w) } {
        Ok(t) => t,
        Err(_) => return, // g_thumb tetap nol (sudah di-reset oleh min_anim_stop)
    };
    ANIM_THUMB.set(thumb);
    let tw = icon_size() as f32 * 1.4; // akhir: seukuran ikon, rasio jendela tetap
    let thh = tw * fh as f32 / fw as f32;
    let cx = (icon.left + icon.right) as f32 / 2.0;
    let cy = (icon.top + icon.bottom) as f32 / 2.0;
    ANIM_FROM.set(from);
    ANIM_TO.set(RECT {
        left: (cx - tw / 2.0) as i32,
        right: (cx + tw / 2.0) as i32,
        top: (cy - thh / 2.0) as i32,
        bottom: (cy + thh / 2.0) as i32,
    });
    ANIM_START.set(unsafe { GetTickCount() });
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let _ = unsafe {
        SetWindowPos(aw, Some(HWND_TOPMOST), 0, 0, sw, sh, SWP_NOACTIVATE | SWP_SHOWWINDOW)
    };
    raise_pills(); // dok tetap di atas thumbnail: naikkan ulang pil setelah g_animWnd
    min_anim_step();
    let _ = unsafe { SetTimer(Some(MSG.get()), T_MINANIM as usize, 15, None) };
}

// ---------------------------------------------------------------- jendela app (win12shell.cpp:4159-4243)

/// Pengganti `EVENT_OBJECT_UNCLOAKED` (0x8018). Konstanta ini tidak diekspor crate windows 0.61.3.
const EV_OBJECT_UNCLOAKED: u32 = 0x8018;

/// `IsKnownAppWindow` (C++ 4159): `h` sudah tercatat sebagai jendela salah satu aplikasi di `APPS`.
pub fn is_known_app_window(h: HWND) -> bool {
    let apps = APPS.borrow();
    apps.iter().any(|a| a.wins.iter().any(|&w| w == h))
}

/// Nama kelas jendela (`GetClassNameW`). Kosong bila gagal.
fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// `IsNewAppWindow` (C++ 4170): jendela tingkat atas yang tampil dan layak dianggap aplikasi baru.
/// Judul dan status cloak sengaja tidak diperiksa (lihat komentar C++ 4167-4169).
pub fn is_new_app_window(h: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(h).as_bool() || is_our_window(h) {
            return false;
        }
        if GetAncestor(h, GA_ROOT) != h {
            return false;
        }
        let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        if (ex & WS_EX_APPWINDOW.0) == 0 {
            if (ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0)) != 0 {
                return false;
            }
            if let Ok(owner) = GetWindow(h, GW_OWNER) {
                if !owner.0.is_null() {
                    return false;
                }
            }
        }
    }
    let cls = class_of(h);
    if is_system_flyout(&cls) {
        return false;
    }
    !matches!(cls.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

/// `PillsHideFor` (C++ 4188): sembunyikan pil atas saat jendela app baru muncul, lalu masa tenang 2,5 dtk.
pub fn pills_hide_for() {
    if DESK_LAYER.get() {
        return; // lapisan desktop: jendela app baru muncul di atas widget
    }
    pills_ensure();
    // Kumpulkan handle dulu agar borrow PILLS tidak dipegang saat ShowWindow.
    let mut hide: Vec<HWND> = Vec::new();
    {
        let mut pills = PILLS.borrow_mut();
        for i in (P_TOP as usize)..(MAXPILLS as usize) {
            let p = &mut pills[i];
            if p.h.0.is_null() || !p.shown {
                continue;
            }
            hide.push(p.h);
            p.shown = false;
        }
    }
    for h in hide {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
    quiet_start(2500);
}

static LAST_CENTERED_DLG: SyncCell<Cell<(HWND, u32)>> =
    SyncCell::new(Cell::new((HWND(std::ptr::null_mut()), 0)));

/// Read lowercase executable basename for a top-level window (resolving UWP CoreWindow child if hosted).
unsafe fn window_exe_basename(h: HWND) -> Option<String> {
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let mut pid: u32 = 0;
    let cls = class_of(h);
    if cls == "ApplicationFrameWindow" {
        let core = to_wide_z("Windows.UI.Core.CoreWindow");
        if let Ok(c) = FindWindowExW(Some(h), None, pcw(&core), PCWSTR::null()) {
            if !c.0.is_null() {
                GetWindowThreadProcessId(c, Some(&mut pid as *mut u32));
            }
        }
    }
    if pid == 0 {
        GetWindowThreadProcessId(h, Some(&mut pid as *mut u32));
    }
    if pid == 0 {
        return None;
    }
    let hp = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; MAX_PATH as usize];
    let mut n = MAX_PATH;
    let ok = QueryFullProcessImageNameW(hp, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
    let _ = CloseHandle(hp);
    if !ok || n == 0 {
        return None;
    }
    let full = from_wide(&buf[..n as usize]);
    let base = full.rsplit(['\\', '/']).next().unwrap_or(&full);
    Some(base.to_ascii_lowercase())
}

/// Center Windows system picker dialogs (e.g. OpenWith "Just once / Always / Set default")
/// that fall back to the top-left corner when Explorer's taskbar is hidden or absent.
pub(crate) unsafe fn center_stuck_system_dialog(h: HWND) {
    if h.0.is_null() {
        return;
    }
    let root = GetAncestor(h, GA_ROOT);
    let h = if !root.0.is_null() { root } else { h };
    if is_our_window(h) {
        return;
    }
    let now = GetTickCount();
    let (last_h, first_tick) = LAST_CENTERED_DLG.get();
    // Allow recentering during the first 1.5s in case XAML layout resets position after show/uncloak,
    // then leave the window alone so manual dragging always works.
    if h == last_h && now.wrapping_sub(first_tick) >= 1500 {
        return;
    }
    if !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() || IsZoomed(h).as_bool() {
        return;
    }
    let mut cloaked: u32 = 0;
    let _ = DwmGetWindowAttribute(
        h,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut std::ffi::c_void,
        std::mem::size_of::<u32>() as u32,
    );
    if cloaked != 0 {
        return;
    }
    let mut rc = RECT::default();
    if GetWindowRect(h, &mut rc).is_err() {
        return;
    }
    let w = rc.right - rc.left;
    let ht = rc.bottom - rc.top;
    if w < 180 || ht < 120 {
        return;
    }
    let mon = MonitorFromWindow(h, MONITOR_DEFAULTTOPRIMARY);
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(mon, &mut mi).as_bool() {
        return;
    }
    let work = mi.rcWork;
    let ww = work.right - work.left;
    let wh = work.bottom - work.top;
    // Skip fullscreen/near-maximized windows and windows not stuck near the top-left corner.
    if w >= ww - 40 || ht >= wh - 40 {
        return;
    }
    if rc.left < work.left - 24 || rc.left > work.left + 80 || rc.top < work.top - 24 || rc.top > work.top + 80 {
        return;
    }
    let Some(exe) = window_exe_basename(h) else {
        return;
    };
    if !matches!(exe.as_str(), "openwith.exe" | "pickerhost.exe" | "credentialuibroker.exe") {
        return;
    }
    let nx = work.left + (ww - w) / 2;
    let ny = work.top + (wh - ht) / 2;
    let _ = SetWindowPos(
        h,
        None,
        nx,
        ny,
        0,
        0,
        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
    );
    if h != last_h {
        LAST_CENTERED_DLG.set((h, now));
        crate::log::log(&format!("recenter: {exe} ({},{} {}x{}) -> ({nx},{ny})", rc.left, rc.top, w, ht));
    }
}

/// `WinEventProc` (C++ 4200): hook WinEvent untuk jendela app, taskbar bawaan, dan foreground.
pub unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    ev: u32,
    h: HWND,
    obj: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if QUITTING.get() {
        return;
    }
    let has_h = !h.0.is_null();
    let is_self = (child as u32) == CHILDID_SELF;
    let is_win = obj == OBJID_WINDOW.0;
    if ev == EVENT_SYSTEM_MINIMIZESTART && is_win && has_h {
        min_anim_start(h);
    }
    if ev == EVENT_OBJECT_SHOW {
        // Explorer menampilkan taskbar bawaan lagi: sembunyikan.
        if has_h && is_win {
            let cls = class_of(h);
            if h == TRAY_WND.get()
                || h == tray_wnd()
                || cls == "Shell_TrayWnd"
                || cls == "Shell_SecondaryTrayWnd"
            {
                let _ = EnableWindow(h, false);
                let _ = ShowWindowAsync(h, SW_HIDE);
                return;
            }
        }
        if has_h && is_win && is_self {
            center_stuck_system_dialog(h);
            if is_new_app_window(h) {
                pills_hide_for();
            }
        }
        return;
    }
    if ev == EVENT_OBJECT_DESTROY {
        if has_h && is_win && h == LAST_CENTERED_DLG.get().0 {
            LAST_CENTERED_DLG.set((HWND(std::ptr::null_mut()), 0));
        }
        // Jendela app ditutup: fokus sementara ke desktop tidak menampilkan pil atas.
        if has_h && is_win && is_self && is_known_app_window(h) {
            quiet_start(1000);
            schedule_refresh();
        }
        return;
    }
    if ev == EVENT_OBJECT_LOCATIONCHANGE {
        if is_win && is_self && has_h && h == GetForegroundWindow() {
            update_foreground(false);
        }
        return;
    }
    if ev == EVENT_SYSTEM_MOVESIZEEND {
        update_foreground(false);
        return;
    }
    if ev == EVENT_SYSTEM_MINIMIZEEND && is_win && has_h {
        // Dipulihkan lewat jalur apa pun: catatan maksimal dari dok tidak berlaku lagi.
        take_dock_zoomed(h);
    }
    if ev == EVENT_SYSTEM_MINIMIZEEND && is_win && has_h && is_new_app_window(h) {
        pills_hide_for(); // app dipulihkan dari dok
    }
    if ev == EVENT_SYSTEM_FOREGROUND && is_win && has_h {
        center_stuck_system_dialog(h);
    }
    if ev == EVENT_SYSTEM_FOREGROUND || ev == EVENT_SYSTEM_MINIMIZESTART || ev == EVENT_SYSTEM_MINIMIZEEND {
        update_foreground(false);
        schedule_refresh();
        return;
    }
    if !is_win || !is_self || !has_h {
        return;
    }
    if ev == EV_OBJECT_UNCLOAKED {
        center_stuck_system_dialog(h);
        // Uncloaked: hanya jendela taskbar.
        if is_task_window(h) {
            schedule_refresh();
        }
        return;
    }
    // Cloaked: hanya jendela yang dikenal.
    if is_known_app_window(h) {
        schedule_refresh();
    }
}

/// `OnDpi` (C++ 4244): DPI berubah, bangun ulang format, ikon, tata letak, dan posisi app bar.
pub fn on_dpi(dpi: i32) {
    if dpi <= 0 || dpi == DPI.get() {
        return;
    }
    DPI.set(dpi);
    create_formats();
    reload_icons();
    menu_icons();
    relayout();
    app_bar_set_pos();
}

/// `HitItem` (C++ 4255): indeks item di pil `pi` yang memuat koordinat x klien, atau -1.
pub fn hit_item(pi: i32, x: i32) -> i32 {
    let n = NITEMS.get().max(0) as usize;
    let items = ITEMS.borrow();
    let xf = x as f32;
    for (i, it) in items.iter().enumerate().take(n) {
        if it.pill == pi && xf >= it.x && xf < it.x + it.w {
            return i as i32;
        }
    }
    -1
}

/// `MicaReloadWallpaperIfChanged` (C++ 4317): muat ulang mica bila berkas wallpaper berubah.
pub fn mica_reload_wallpaper_if_changed(force: bool, repaint: bool) -> bool {
    if MICA_FULLSCREEN_SNAPSHOT.get() {
        return false;
    }
    let path = mica_resolve_wallpaper_path();
    let (size, write) = mica_wallpaper_stamp(&path);
    let same_path = path.as_str() == MICA_PATH.borrow().as_str();
    if !force && same_path && size == MICA_FILE_SIZE.get() && write == MICA_FILE_WRITE.get() {
        return false;
    }
    GLASS_OK.set(mica_load());
    apply_wallpaper_surface_tint();
    if repaint {
        base_update(true);
        invalidate_pills(true, true);
        notch_invalidate();
        let pan = PAN.get();
        if PANEL_OPEN.get() >= 0 && !pan.0.is_null() {
            let _ = w12_invalidate(pan, None, false);
        }
    }
    true
}

// ---------------------------------------------------------------- seret ikon (win12shell.cpp:4265-4296)

/// `FinishDrag` (C++ 4265): akhiri seret ikon app. Bila urutan berubah: pindahkan di `APPS`, simpan, susun ulang.
pub fn finish_drag() {
    let from = DRAG_FROM.get();
    let to = DRAG_TO.get();
    let nslots = NSLOTS.get();
    let slots = SLOTS.get();
    // Indeks app di slot `s` (dari `SLOTS` -> `ITEMS`). -1 bila di luar jangkauan.
    let slot_app = |s: i32| -> i32 {
        if s < 0 || s >= nslots {
            return -1;
        }
        ITEMS.borrow().get(slots[s as usize] as usize).map_or(-1, |it| it.app)
    };
    if from < 0 || to < 0 || from == to || from >= nslots || to >= nslots {
        // Kembali ke tempat: ukuran ikon tetap meluruh halus.
        if from >= 0 && from < nslots {
            SETTLE_APP.set(slot_app(from));
            SETTLE_K.set(1.0);
        }
        start_anim();
        return;
    }
    let a = slot_app(from);
    let b = slot_app(to);
    let napps_len = APPS.borrow().len() as i32;
    if a < 0 || b < 0 || a >= napps_len || b >= napps_len || a >= MAXAPPS || b >= MAXAPPS {
        // Indeks app tak valid: jangan memindahkan apa pun (C++ tidak memeriksa; di sini dicegah panic).
        SETTLE_APP.set(a);
        SETTLE_K.set(1.0);
        start_anim();
        return;
    }
    // Posisi yang sedang tergambar sebelum urutan berubah (ikon yang dipegang ada di kursor).
    let mut drawn = vec![0.0f32; MAXAPPS as usize];
    let mut seen = vec![false; MAXAPPS as usize];
    {
        let items = ITEMS.borrow();
        let n = NITEMS.get().max(0) as usize;
        for it in items.iter().take(n) {
            if it.ty == IT_APP && it.app >= 0 && it.app < MAXAPPS {
                drawn[it.app as usize] = it.dx;
                seen[it.app as usize] = true;
            }
        }
    }
    // C++ `memmove` pada `g_apps`: lepas elemen `a`, sisipkan di `b`.
    {
        let mut apps = APPS.borrow_mut();
        let tmp = apps.remove(a as usize);
        apps.insert(b as usize, tmp);
    }
    save_apps();
    relayout(); // semua item langsung di slot baru; posisi tergambar dikembalikan di bawah agar meluncur halus
    {
        let mut items = ITEMS.borrow_mut();
        let n = NITEMS.get().max(0) as usize;
        for it in items.iter_mut().take(n) {
            if it.ty != IT_APP {
                continue;
            }
            let nn = it.app; // indeks baru aplikasi ini
            let mut o = nn; // indeks lama (kebalikan dari pemindahan di atas)
            if nn == b {
                o = a;
            } else if a < b && nn >= a && nn < b {
                o = nn + 1;
            } else if a > b && nn > b && nn <= a {
                o = nn - 1;
            }
            if o >= 0 && o < MAXAPPS && seen[o as usize] {
                it.dx = drawn[o as usize];
            }
        }
    }
    SETTLE_APP.set(b); // ikon yang dilepas mulai dari ukuran saat digeser
    SETTLE_K.set(1.0);
    start_anim();
}

// ---------------------------------------------------------------- pil (win12shell.cpp:4337-4494)

// GET_X_LPARAM / GET_Y_LPARAM: crate tidak menyediakan macro-nya (lihat CONVENTIONS).
fn lp_x(lp: LPARAM) -> i32 {
    (lp.0 & 0xFFFF) as u16 as i16 as i32
}

fn lp_y(lp: LPARAM) -> i32 {
    ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32
}

// GET_WHEEL_DELTA_WPARAM: delta roda bertanda dari HIWORD(wp).
fn wp_hi_signed(wp: WPARAM) -> i32 {
    ((wp.0 >> 16) & 0xFFFF) as u16 as i16 as i32
}

// Pengganti PtInRect (kiri/atas inklusif, kanan/bawah eksklusif).
fn point_in_rect(pt: POINT, rc: &RECT) -> bool {
    pt.x >= rc.left && pt.x < rc.right && pt.y >= rc.top && pt.y < rc.bottom
}

/// `PillProc` (C++ 4337): wndproc jendela pil (kapsul dok, pil atas, pil widget). `GWLP_USERDATA` = indeks pil.
pub unsafe extern "system" fn pill_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_WINDOWPOSCHANGED {
        mica_moved(h, lp); // widget digeser: potongan wallpaper kaca ikut posisi baru
    }
    let mut pi = GetWindowLongPtrW(h, GWLP_USERDATA) as i32;
    if pi < 0 || pi >= MAXPILLS {
        pi = 0;
    }
    match m {
        WM_NCCALCSIZE => {
            if wp.0 != 0 {
                return LRESULT(0);
            }
        }
        WM_NCHITTEST => return LRESULT(HTCLIENT as isize),
        WM_NCACTIVATE => return DefWindowProcW(h, m, WPARAM(1), lp), // material acrylic tetap "aktif"
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_GETMINMAXINFO => {
            let mm = lp.0 as *mut MINMAXINFO;
            (*mm).ptMinTrackSize.x = 1;
            (*mm).ptMinTrackSize.y = 1;
            return LRESULT(0);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            // jendela berlapis: isi digambar lewat WM_W12_RENDER
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            BeginPaint(h, &mut ps);
            let _ = EndPaint(h, &ps);
            return LRESULT(0);
        }
        WM_W12_RENDER => {
            render_pill(h, pi);
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            if pi == 0 && MODE.get() == MODE_DOCK {
                // kilau bibir dok mengikuti kursor (render hanya saat bergerak > 2 px)
                let nx = lp_x(lp) as f32;
                let hx = DOCK_HX.get();
                if hx < 0.0 || (nx - hx).abs() > sf(2.0) {
                    DOCK_HX.set(nx);
                    let _ = w12_invalidate(h, None, false);
                }
            }
            let mut tr = TRACKING.get();
            if !tr[pi as usize] {
                let mut t: TRACKMOUSEEVENT = std::mem::zeroed();
                t.cbSize = std::mem::size_of::<TRACKMOUSEEVENT>() as u32;
                t.dwFlags = TME_LEAVE;
                t.hwndTrack = h;
                let _ = TrackMouseEvent(&mut t);
                tr[pi as usize] = true;
                TRACKING.set(tr);
            }
            if wg_idx(pi) >= 0 {
                // widget: sorot (pegangan ukuran tampil), geser, atau ubah ukuran
                if WG_HOVER.get() != pi {
                    WG_HOVER.set(pi);
                    let _ = w12_invalidate(h, None, false);
                }
                if wp.0 & (MK_LBUTTON.0 as usize) != 0 {
                    wg_drag(pi);
                }
                return LRESULT(0);
            }
            if DRAG_CAND.get() && (wp.0 & (MK_LBUTTON.0 as usize)) != 0 && DRAG_PILL.get() == pi {
                // geser aplikasi
                let mx = lp_x(lp);
                if !DRAG_ON.get() && (mx - DRAG_DOWN_X.get()).abs() > s(6.0) {
                    DRAG_ON.set(true);
                    HOT.set(-1);
                    tip_hide();
                }
                if DRAG_ON.get() {
                    DRAG_X.set(mx);
                    let nslots = NSLOTS.get().clamp(0, MAXITEMS);
                    let slots = SLOTS.get();
                    let mut best = 1e9f32;
                    {
                        let items = ITEMS.borrow();
                        for si in 0..nslots {
                            if let Some(it) = items.get(slots[si as usize] as usize) {
                                let d = (it.x + it.w / 2.0 - mx as f32).abs();
                                if d < best {
                                    best = d;
                                    DRAG_TO.set(si);
                                }
                            }
                        }
                    }
                    // ikon yang dipegang langsung ikut kursor; nilai dihitung dulu agar tidak memegang borrow
                    let di = DRAG_ITEM.get();
                    let tx = drag_target_x(di);
                    if di >= 0 {
                        if let Some(it) = ITEMS.borrow_mut().get_mut(di as usize) {
                            it.dx = tx;
                        }
                    }
                    let _ = w12_invalidate(h, None, false);
                    let _ = UpdateWindow(h);
                    start_anim();
                    return LRESULT(0);
                }
            }
            let hit = hit_item(pi, lp_x(lp));
            if hit != HOT.get() {
                HOT.set(hit);
                start_anim();
                tip_hover(hit);
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            let mut tr = TRACKING.get();
            tr[pi as usize] = false;
            TRACKING.set(tr);
            if WG_HOVER.get() == pi {
                WG_HOVER.set(-1);
                let _ = w12_invalidate(h, None, false);
            }
            if pi == 0 && DOCK_HX.get() >= 0.0 {
                DOCK_HX.set(-1.0);
                let _ = w12_invalidate(h, None, false);
            }
            let hot = HOT.get();
            if hot >= 0 && hot < NITEMS.get() && ITEMS.borrow().get(hot as usize).map_or(false, |it| it.pill == pi) {
                HOT.set(-1);
                start_anim();
                tip_hide();
            }
            return LRESULT(0);
        }
        WM_LBUTTONDOWN | WM_MBUTTONDOWN => {
            if wg_idx(pi) >= 0 {
                // widget: calon geser/ubah ukuran; klik biasa diputuskan saat tombol dilepas
                if m == WM_LBUTTONDOWN {
                    tip_hide();
                    wg_down(h, pi, lp_x(lp), lp_y(lp));
                }
                return LRESULT(0);
            }
            let pressed = hit_item(pi, lp_x(lp));
            PRESSED.set(pressed);
            DRAG_CAND.set(false);
            DRAG_ON.set(false);
            let pressed_is_app = pressed >= 0 && ITEMS.borrow().get(pressed as usize).map_or(false, |it| it.ty == IT_APP);
            if m == WM_LBUTTONDOWN && pi < NBOTTOM && pressed_is_app {
                // calon geser ikon app: kumpulkan slot app di pil ini
                DRAG_CAND.set(true);
                DRAG_ITEM.set(pressed);
                DRAG_PILL.set(pi);
                DRAG_DOWN_X.set(lp_x(lp));
                DRAG_X.set(lp_x(lp));
                let mut slots = [0i32; MAXITEMS as usize];
                let mut n = 0i32;
                let mut from = -1i32;
                {
                    let items = ITEMS.borrow();
                    let nitems = NITEMS.get().clamp(0, MAXITEMS) as usize;
                    for (i, it) in items.iter().enumerate().take(nitems) {
                        if it.pill == pi && it.ty == IT_APP {
                            if i as i32 == pressed {
                                from = n;
                            }
                            slots[n as usize] = i as i32;
                            n += 1;
                        }
                    }
                }
                NSLOTS.set(n);
                SLOTS.set(slots);
                if from >= 0 {
                    DRAG_FROM.set(from);
                }
                DRAG_TO.set(DRAG_FROM.get());
            }
            SetCapture(h);
            tip_hide();
            let _ = w12_invalidate(h, None, false);
            return LRESULT(0);
        }
        WM_LBUTTONUP | WM_MBUTTONUP => {
            if wg_idx(pi) >= 0 {
                if m != WM_LBUTTONUP || WG_PILL.get() != pi {
                    return LRESULT(0);
                }
                let moved = wg_up();
                let _ = w12_invalidate(h, None, false);
                let pt = POINT { x: lp_x(lp), y: lp_y(lp) };
                let mut rc: RECT = std::mem::zeroed();
                let _ = GetClientRect(h, &mut rc);
                let sc = wg_scale(pi); // kotak klik widget dalam koordinat dasar (sebelum skala)
                if !moved && point_in_rect(pt, &rc) {
                    widget_click(pi, (pt.x as f32 / sc) as i32, (pt.y as f32 / sc) as i32);
                }
                return LRESULT(0);
            }
            if DRAG_ON.get() && m == WM_LBUTTONUP {
                // lepas setelah menggeser: simpan urutan, tanpa klik
                DRAG_ON.set(false);
                DRAG_CAND.set(false);
                PRESSED.set(-1);
                let _ = ReleaseCapture();
                finish_drag();
                return LRESULT(0);
            }
            DRAG_CAND.set(false);
            let pt = POINT { x: lp_x(lp), y: lp_y(lp) };
            let hit = hit_item(pi, pt.x);
            let pr = PRESSED.get();
            PRESSED.set(-1);
            let _ = ReleaseCapture();
            let _ = w12_invalidate(h, None, false);
            let mut rc: RECT = std::mem::zeroed();
            let _ = GetClientRect(h, &mut rc);
            if hit >= 0 && hit == pr && point_in_rect(pt, &rc) {
                activate_item(hit, m == WM_MBUTTONUP);
            }
            return LRESULT(0);
        }
        WM_RBUTTONUP => {
            let hit = hit_item(pi, lp_x(lp));
            let mut pt = POINT { x: 0, y: 0 };
            let _ = GetCursorPos(&mut pt);
            tip_hide();
            if wg_idx(pi) >= 0 {
                wg_menu(h, pi, pt); // widget: ukuran, posisi, lalu menu shell
                return LRESULT(0);
            }
            let (ty, app) = if hit >= 0 {
                ITEMS.borrow().get(hit as usize).map_or((-1, -1), |it| (it.ty, it.app))
            } else {
                (-1, -1)
            };
            if ty == IT_APP {
                app_menu(h, pt, app);
            } else if ty == IT_START && SHELL_MODE.get() {
                winx_menu(Some(h)); // mode shell: menu Win+X di atas Start
            } else {
                main_menu(h, pt, pi >= P_TOP);
            }
            return LRESULT(0);
        }
        WM_SETCURSOR => {
            // widget: kursor ubah ukuran di pegangan, kursor geser saat menggeser
            if wg_idx(pi) >= 0 && (lp.0 & 0xFFFF) as u32 == HTCLIENT {
                let mut c = POINT { x: 0, y: 0 };
                let _ = GetCursorPos(&mut c);
                let _ = ScreenToClient(h, &mut c);
                let grip = if WG_PILL.get() == pi { WG_MODE.get() >= 3 } else { wg_in_grip(h, c.x, c.y) };
                let cur = if grip {
                    Some(IDC_SIZENWSE)
                } else if WG_PILL.get() == pi && WG_MODE.get() == 2 {
                    Some(IDC_SIZEALL)
                } else {
                    None
                };
                if let Some(cur) = cur {
                    if let Ok(hc) = LoadCursorW(None, cur) {
                        SetCursor(Some(hc));
                    }
                    return LRESULT(1); // TRUE
                }
            }
        }
        WM_CAPTURECHANGED => {
            // tangkapan hilang di tengah geser (mis. Alt+Tab): akhiri dan simpan posisi terakhir
            if WG_PILL.get() == pi && WG_MODE.get() != 0 {
                wg_up();
            }
        }
        WM_MOUSEWHEEL => {
            if wg_idx(pi) >= 0 {
                // Ctrl + roda: perbesar/perkecil widget
                if (wp.0 & 0xFFFF) & (MK_CONTROL.0 as usize) != 0 {
                    wg_step(pi, if wp_hi_signed(wp) > 0 { 0.1 } else { -0.1 });
                }
                return LRESULT(0);
            }
            let mut pt = POINT { x: lp_x(lp), y: lp_y(lp) };
            let _ = ScreenToClient(h, &mut pt);
            let hit = hit_item(pi, pt.x);
            if hit >= 0 && ITEMS.borrow().get(hit as usize).map_or(false, |it| it.ty == IT_VOL) {
                let d = wp_hi_signed(wp);
                let n = d.unsigned_abs() / WHEEL_DELTA;
                for _ in 0..n.max(1) {
                    // Lewat shell (bukan send_keys) agar OSD volume bawaan Windows tidak muncul
                    super::core_c4b::vol_wheel_post(d > 0);
                }
            }
            return LRESULT(0);
        }
        WM_DPICHANGED => {
            on_dpi(((wp.0 >> 16) & 0xFFFF) as i32); // HIWORD(wp) = DPI baru
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(h, m, wp, lp)
}

/// Mendaftarkan kelas jendela pil "Win12ShellPill" (C++ WinMain 5181–5184, bagian pil). Idempoten: pemanggilan ulang gagal diam-diam.
fn pill_class_register() {
    let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(pill_proc);
    wc.hInstance = HI.get();
    wc.lpszClassName = w!("Win12ShellPill");
    if let Ok(c) = unsafe { LoadCursorW(None, IDC_ARROW) } {
        wc.hCursor = c;
    }
    let _ = unsafe { RegisterClassExW(&wc) };
}

/// `CreatePill` (C++ 4496): satu jendela pil berlapis per-piksel. `idx` = indeks pil (0..MAXPILLS).
pub fn create_pill(idx: i32) -> HWND {
    // Pil atas di lapisan desktop tidak topmost: jendela app dan menu selalu di atasnya (lihat PillsToDesk di C++).
    let top = if desk_layer(idx) { WINDOW_EX_STYLE(0) } else { WS_EX_TOPMOST };
    let ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | top | WS_EX_NOACTIVATE;
    pill_class_register();
    let Ok(h) = (unsafe {
        CreateWindowExW(
            ex,
            w!("Win12ShellPill"),
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
    }) else {
        return HWND(std::ptr::null_mut());
    };
    unsafe {
        SetWindowLongPtrW(h, GWLP_USERDATA, idx as isize);
        let _ = SetPropW(h, w!("w12l"), Some(HANDLE(1usize as *mut std::ffi::c_void)));
    }
    h
}

/// `ArmClock` (C++ 4509): jadwalkan T_CLOCK tepat di pergantian menit berikutnya (+30 ms).
pub fn arm_clock() {
    let st = unsafe { GetLocalTime() };
    let ms = st.wSecond as i32 * 1000 + st.wMilliseconds as i32;
    set_timer(T_CLOCK, (60000 - ms + 30) as u32);
}

/// `TrimWorkingSet` (C++ 4520). Di C++ `kTrimWorkingSet = false`, sehingga cabang
/// SetProcessWorkingSetSize tidak pernah jalan.
/// DEVIASI (2026-11-14, permintaan penghematan RAM): port memangkas working set sungguhan.
/// Dipanggil hanya sesekali (TrimSoon 3 dtk setelah panel/notch tutup, dan saat dok disembunyikan
/// di fullscreen), tidak periodik. Konsekuensi: halaman yang dibuang dimuat ulang saat panel dibuka lagi.
/// `DeactivateNativeStartProcesses()` (C++ 4528) dipanggil tanpa syarat, seperti C++.
/// Fungsinya no-op di port (kFreezeNativeStart = false), lihat core_c4b.rs.
pub fn trim_working_set(_force: bool) {
    use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
    crate::shell::core_c4b::deactivate_native_start_processes();
    let _ = unsafe { SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX) };
}

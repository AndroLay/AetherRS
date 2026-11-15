//! Sumber: win12shell.cpp:2636-3337 (AnimStep s.d. AppBarRemove). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime.
use super::prelude::*;
use crate::wide::to_wide_z;
use windows::Win32::Globalization::{GetDateFormatEx, DATE_LONGDATE};
use windows::Win32::Storage::FileSystem::{GetFileAttributesW, FILE_ATTRIBUTE_DIRECTORY, INVALID_FILE_ATTRIBUTES};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount};
use windows::Win32::UI::Controls::{
    TTM_GETBUBBLESIZE, TTM_TRACKACTIVATE, TTM_TRACKPOSITION, TTM_UPDATETIPTEXTW, TTTOOLINFOW,
    WM_MOUSELEAVE,
};
// EVENT_OBJECT_SHOW dan WINEVENT_OUTOFCONTEXT ada di UI::WindowsAndMessaging (sudah lewat prelude)
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABE_BOTTOM, ABM_GETSTATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
    ABM_SETSTATE, ABS_ALWAYSONTOP, ABS_AUTOHIDE, APPBARDATA,
    NIF_GUID, NIF_ICON, NIF_MESSAGE, NIF_STATE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NIM_SETVERSION, NIN_SELECT, NIS_HIDDEN,
};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use std::ffi::c_void;

// W12_KBSIG (penanda input sintetis) hanya didefinisikan di state.rs; dipakai lewat prelude.

/// Item tooltip yang sedang ditunjuk (`g_tipItem`).
pub static TIP_ITEM: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));

/// Mendekati `target` sebesar `step` tanpa melewatinya (animasi hover dan aktif).
fn approach(v: f32, target: f32, step: f32) -> f32 {
    if v < target {
        if v + step > target { target } else { v + step }
    } else if v - step < target {
        target
    } else {
        v - step
    }
}

/// Tandai pil `pill` perlu digambar ulang. Indeks negatif atau di luar batas diabaikan.
fn mark(dirty: &mut [bool], pill: i32) {
    if pill >= 0 {
        if let Some(d) = dirty.get_mut(pill as usize) {
            *d = true;
        }
    }
}

/// `MAKELPARAM(x, y)` untuk pesan yang menerima koordinat bertanda.
fn make_lparam(x: i32, y: i32) -> LPARAM {
    LPARAM((((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF)) as isize)
}

/// Menampilkan menu popup dan mengembalikan id perintah yang dipilih (0 bila dibatalkan).
/// Sumber: `TrackMenu` (win12shell.cpp:3466). `from_top` memilih penjajaran atas (true) atau bawah (false).
pub fn track_menu(m: HMENU, owner: HWND, pt: POINT, from_top: bool) -> i32 {
    // SAFETY: `m` adalah menu valid dari CreatePopupMenu; `owner` adalah HWND milik shell.
    unsafe {
        let _ = SetForegroundWindow(owner);
        let align = if from_top { TPM_TOPALIGN.0 } else { TPM_BOTTOMALIGN.0 };
        let fl = TPM_RETURNCMD.0 | TPM_RIGHTBUTTON.0 | TPM_NONOTIFY.0 | TPM_LEFTALIGN.0 | align;
        let cmd = TrackPopupMenuEx(m, fl, pt.x, pt.y, owner, None).0;
        let _ = PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0));
        // SetForegroundWindow di atas bisa menaikkan widget lapisan desktop ke atas jendela app: kembalikan ke desktop.
        if is_top_pill_wnd(owner) {
            pills_to_desk(None);
        }
        cmd
    }
}

/// Teks string wide sampai NUL pertama (`MediaState.title/artist`).
fn nul_str(w: &[u16]) -> String {
    let n = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..n])
}

/// Judul jendela (`GetWindowTextW`).
fn window_text(h: HWND) -> String {
    let mut buf = [0u16; 400];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Sumber: `AnimStep` (:2636). Dipanggil timer `T_ANIM`; mematikan timer bila animasi sudah selesai.
pub fn anim_step() {
    let step: f32 = 15.0 / 110.0;
    let mut busy = false;
    let mut dirty = [false; MAXPILLS as usize];
    let n = NITEMS.get().max(0) as usize;
    let hot = HOT.get();
    let drag_on = DRAG_ON.get();
    let drag_item = DRAG_ITEM.get();
    let drag_pill = DRAG_PILL.get();

    // Target geser dihitung dulu: `drag_target_x` membaca ITEMS, jadi tidak boleh berada di dalam borrow_mut.
    let targets: Vec<f32> = {
        let items = ITEMS.borrow();
        (0..n)
            .map(|i| {
                let pill = items.get(i).map_or(-1, |it| it.pill);
                let x = items.get(i).map_or(0.0, |it| it.x);
                if drag_on && pill == drag_pill { drag_target_x(i as i32) } else { x }
            })
            .collect()
    };

    {
        let mut items = ITEMS.borrow_mut();
        let apps = APPS.borrow();
        let n = n.min(items.len());

        // hover: ikon yang ditunjuk membesar
        for (i, it) in items.iter_mut().enumerate().take(n) {
            let target = if i as i32 == hot { 1.0 } else { 0.0 };
            if it.hov != target {
                it.hov = approach(it.hov, target, step);
                if it.hov != target {
                    busy = true;
                }
                mark(&mut dirty, it.pill);
            }
        }

        // indikator aktif: melebar dan berubah warna saat aplikasi difokuskan
        for it in items.iter_mut().take(n) {
            let focused = it.ty == IT_APP && apps.get(it.app as usize).map_or(false, |a| a.fg);
            let target = if focused { 1.0 } else { 0.0 };
            if it.act != target {
                it.act = approach(it.act, target, step);
                if it.act != target {
                    busy = true;
                }
                mark(&mut dirty, it.pill);
            }
        }

        // geser: item bergeser halus ke slot barunya
        for (i, it) in items.iter_mut().enumerate().take(n) {
            let target = targets[i];
            if drag_on && i as i32 == drag_item {
                if it.dx != target {
                    it.dx = target;
                    mark(&mut dirty, it.pill);
                }
                continue;
            }
            if (it.dx - target).abs() > 0.3 {
                it.dx += (target - it.dx) * 0.35;
                busy = true;
                mark(&mut dirty, it.pill);
            } else if it.dx != target {
                it.dx = target;
                mark(&mut dirty, it.pill);
            }
        }
    }

    // ikon yang baru dilepas: skala kembali normal dengan meluruh cepat, lalu berhenti
    let mut k = SETTLE_K.get();
    if k > 0.0 {
        k *= 0.7;
        if k < 0.01 {
            k = 0.0;
        } else {
            busy = true;
        }
        SETTLE_K.set(k);
        mark(&mut dirty, drag_pill);
    }

    // pantulan ikon saat aplikasi dibuka
    if !BOUNCE_KEY.borrow().is_empty() {
        let elapsed = unsafe { GetTickCount() }.wrapping_sub(BOUNCE_START.get());
        if elapsed < 1500 {
            busy = true;
        } else {
            BOUNCE_KEY.borrow_mut().clear();
        }
        for d in dirty.iter_mut().take(NBOTTOM as usize) {
            *d = true;
        }
    }

    // salinan dulu: UpdateWindow mengirim WM_PAINT secara sinkron dan handler-nya bisa membaca PILLS
    let pills: Vec<Pill> = PILLS.borrow().to_vec();
    for (p, pl) in pills.iter().enumerate().take(MAXPILLS as usize) {
        if dirty[p] && !pl.h.0.is_null() && pl.shown {
            w12_invalidate(pl.h, None, false);
            unsafe {
                let _ = UpdateWindow(pl.h);
            }
        }
    }

    // geser masuk flyout (FlySlideStep, C++ 3098; didefinisikan di core_c3 ini, T08)
    if fly_slide_step() {
        busy = true;
    }
    if !busy {
        kill_timer(T_ANIM);
        ANIM_ON.set(false);
    }
}

/// Sumber: `TipHide` (:2679).
pub fn tip_hide() {
    kill_timer(T_TIP);
    let tip = TIP.get();
    if tip.0.is_null() {
        return;
    }
    let ti = TTTOOLINFOW {
        cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
        hwnd: MSG.get(),
        uId: 1,
        ..TTTOOLINFOW::default()
    };
    unsafe {
        SendMessageW(
            tip,
            TTM_TRACKACTIVATE,
            Some(WPARAM(0)),
            Some(LPARAM(&ti as *const TTTOOLINFOW as isize)),
        );
    }
}

/// Sumber: `TipHover` (:2687). `item < 0` hanya menyembunyikan tooltip.
pub fn tip_hover(item: i32) {
    tip_hide();
    TIP_ITEM.set(item);
    if item >= 0 {
        set_timer(T_TIP, 450);
    }
}

/// Tanggal panjang sesuai locale pengguna (`GetDateFormatEx`, `DATE_LONGDATE`).
fn long_date_text() -> String {
    unsafe {
        let st = GetLocalTime();
        let mut buf = [0u16; 256];
        // LOCALE_NAME_USER_DEFAULT di winnls.h didefinisikan sebagai NULL.
        let n = GetDateFormatEx(
            PCWSTR::null(),
            DATE_LONGDATE,
            Some(&st),
            PCWSTR::null(),
            Some(&mut buf),
            PCWSTR::null(),
        );
        String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize])
    }
}

/// Sumber: `ItemTooltip` (:2694). Teks tooltip untuk satu item dok; kosong bila tidak ada.
fn item_tooltip(it: &Item) -> String {
    match it.ty {
        IT_APP => {
            let apps = APPS.borrow();
            let Some(a) = apps.get(it.app as usize) else {
                return String::new();
            };
            let nw = a.wins.len();
            if nw == 1 {
                let t = window_text(a.wins[0]);
                if t.is_empty() { a.name.clone() } else { t }
            } else if nw > 1 {
                format!("{} - {} windows", a.name, nw)
            } else {
                a.name.clone()
            }
        }
        IT_NOTE => "Create a new note in Notepad".to_string(),
        IT_SEARCH => "Search".to_string(),
        IT_START => "Start".to_string(),
        IT_TASKVIEW => "Task view".to_string(),
        IT_NET => {
            let net = NET.get();
            let eth = ETH_NAME.borrow().clone();
            let eth_desc = ETH_DESC.borrow().clone();
            let ssid = SSID.borrow().clone();
            let or_default = |s: &str, def: &str| {
                if s.is_empty() { def.to_string() } else { s.to_string() }
            };
            if net == NET_BOTH {
                format!(
                    "Ethernet: Terhubung ({}), utama\nWi-Fi: Terhubung ({}), cadangan",
                    or_default(&eth, "LAN"),
                    or_default(&ssid, "Wi-Fi")
                )
            } else if net == NET_ETH {
                let desc = if eth_desc.is_empty() { or_default(&eth, "Kabel LAN") } else { eth_desc };
                format!("Ethernet: Terhubung\n{}", desc)
            } else if net == NET_WIFI {
                format!("Wi-Fi: Terhubung\n{}", or_default(&ssid, "Wi-Fi"))
            } else {
                "Tidak ada koneksi internet".to_string()
            }
        }
        IT_VOL => {
            let vol = VOL.get();
            if vol < 0 {
                "No audio device".to_string()
            } else if MUTE.get() {
                "Speakers: muted".to_string()
            } else {
                format!("Speakers: {}%", vol)
            }
        }
        IT_BAT => {
            let bat = BAT.get().max(0);
            if CHARGING.get() {
                format!("Battery: {}% (charging)", bat)
            } else {
                format!("Battery: {}% remaining", bat)
            }
        }
        IT_CHEV => "Show hidden icons".to_string(),
        IT_BELL => "Notifications".to_string(),
        IT_USB => "Safely remove USB drive".to_string(),
        IT_TCLOCK | IT_CLOCK => long_date_text(),
        IT_MEDIA_MAIN => {
            let m = MEDIA.borrow();
            format!("{}\n{}", nul_str(&m.title), nul_str(&m.artist))
        }
        IT_MEDIA_PREV => "Previous".to_string(),
        IT_MEDIA_PLAY => {
            if MEDIA.borrow().status == 4 { "Pause".to_string() } else { "Play".to_string() }
        }
        IT_MEDIA_NEXT => "Next".to_string(),
        _ => String::new(),
    }
}

/// Sumber: `TipShow` (:2745). Menampilkan tooltip di atas item (atau di bawah bila pil jam).
pub fn tip_show() {
    kill_timer(T_TIP);
    let tip = TIP.get();
    let idx = TIP_ITEM.get();
    if tip.0.is_null() || idx < 0 || idx >= NITEMS.get() || idx != HOT.get() {
        return;
    }
    let it = match ITEMS.borrow().get(idx as usize) {
        Some(it) => *it,
        None => return,
    };
    if it.pill < 0 {
        return;
    }
    let text = item_tooltip(&it);
    if text.is_empty() {
        return;
    }
    let mut wtext = to_wide_z(&text);
    let mut ti = TTTOOLINFOW::default();
    ti.cbSize = std::mem::size_of::<TTTOOLINFOW>() as u32;
    ti.hwnd = MSG.get();
    ti.uId = 1;
    ti.lpszText = PWSTR(wtext.as_mut_ptr());
    let ti_lp = LPARAM(&ti as *const TTTOOLINFOW as isize);

    // ukuran gelembung tooltip dari kontrol tooltip
    let (tw, th) = unsafe {
        SendMessageW(tip, TTM_UPDATETIPTEXTW, Some(WPARAM(0)), Some(ti_lp));
        let sz = SendMessageW(tip, TTM_GETBUBBLESIZE, Some(WPARAM(0)), Some(ti_lp)).0 as usize;
        ((sz & 0xFFFF) as i32, ((sz >> 16) & 0xFFFF) as i32)
    };

    let Some(p) = PILLS.borrow().get(it.pill as usize).copied() else {
        return;
    };
    let mut x = p.x + (it.x + it.w / 2.0) as i32 - tw / 2;
    let y = if it.pill >= P_CLOCK { p.y + p.ht + s(8.0) } else { p.y - th - s(8.0) };
    if x < s(4.0) {
        x = s(4.0);
    }
    unsafe {
        SendMessageW(tip, TTM_TRACKPOSITION, Some(WPARAM(0)), Some(make_lparam(x, y)));
        SendMessageW(tip, TTM_TRACKACTIVATE, Some(WPARAM(1)), Some(ti_lp));
    }
}

/// Sumber: `SendKeys` (:2771). `k1` ditahan lalu `k2` ditekan; dengan `k3`: k1+k2 ditahan dan `k3` ditekan
/// (mis. Win+Ctrl+L). Input sintetis diberi penanda `W12_KBSIG` agar kait tombol Windows mengabaikannya.
/// Nilai 0 berarti "tidak dipakai".
pub fn send_keys(k1: u16, k2: u16, k3: u16) {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(6);
    let mut key = |vk: u16, up: bool| {
        let ext = vk == VK_LWIN.0 || (vk >= VK_VOLUME_MUTE.0 && vk <= VK_MEDIA_PLAY_PAUSE.0);
        let mut flags: u32 = 0;
        if up {
            flags |= KEYEVENTF_KEYUP.0;
        }
        if ext {
            flags |= KEYEVENTF_EXTENDEDKEY.0;
        }
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(flags),
                    time: 0,
                    dwExtraInfo: W12_KBSIG,
                },
            },
        });
    };
    key(k1, false);
    if k2 != 0 && k3 != 0 {
        key(k2, false);
        key(k3, false);
        key(k3, true);
        key(k2, true);
    } else if k2 != 0 {
        key(k2, false);
        key(k2, true);
    }
    key(k1, true);
    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Sumber: `ActivateWindow` (:2791). Memulihkan jendela yang diminimalkan, lalu memaksa fokus.
pub fn activate_window(h: HWND) {
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindowAsync(h, SW_RESTORE);
        }
        if !SetForegroundWindow(h).as_bool() || GetForegroundWindow() != h {
            SwitchToThisWindow(h, true);
        }
    }
}

/// Sumber: `LaunchApp` (:2800). Di mode shell, pintasan yang menunjuk folder dibuka lewat
/// `open_folder_window` (T09, core_c3b); selain itu lewat `shell_open` (T10, hotkeys.rs).
pub fn launch_app(launch: &str) {
    if launch.is_empty() {
        return;
    }
    let attr = unsafe { GetFileAttributesW(pcw(&to_wide_z(launch))) };
    if SHELL_MODE.get() && attr != INVALID_FILE_ATTRIBUTES && (attr & FILE_ATTRIBUTE_DIRECTORY.0) != 0 {
        open_folder_window(launch);
        return;
    }
    shell_open(launch);
}

/// Sumber: `QuietStart` (:2810). Masa tenang sesudah aksi dok: pil yang sedang tersembunyi tidak ditampilkan.
pub fn quiet_start(ms: u32) {
    QUIET.set(true);
    set_timer(T_QUIET, ms);
}

/// Sumber: `ClickApp` (:2816). Klik ikon aplikasi: luncurkan bila `new_instance` atau tanpa jendela;
/// dengan satu jendela: minimalkan bila sudah aktif, selain itu fokuskan; dengan banyak jendela: klik berulang berganti jendela.
pub fn click_app(ai: i32, new_instance: bool) {
    quiet_start(1000);
    // Salin data dulu: borrow APPS tidak boleh aktif saat memanggil Win32 yang bisa memicu pesan.
    let (launch, wins) = {
        let apps = APPS.borrow();
        let Some(a) = apps.get(ai as usize) else {
            return;
        };
        (a.launch.clone(), a.wins.clone())
    };
    if new_instance || wins.is_empty() {
        launch_app(&launch);
        let key: Vec<u16> = launch.encode_utf16().take(599).collect();
        *BOUNCE_KEY.borrow_mut() = String::from_utf16_lossy(&key);
        BOUNCE_START.set(unsafe { GetTickCount() });
        start_anim();
        return;
    }
    let fg = unsafe { GetForegroundWindow() };
    let cur = wins.iter().rposition(|&w| w == fg);
    if wins.len() == 1 {
        if cur == Some(0) {
            unsafe {
                let _ = ShowWindowAsync(fg, SW_MINIMIZE);
            }
        } else {
            activate_window(wins[0]);
        }
    } else {
        // klik berulang = berganti jendela
        activate_window(if cur.is_some() { wins[wins.len() - 1] } else { wins[0] });
    }
}

/// Sumber: `FocusMediaSource` (:2832). Fokuskan aplikasi sumber media (cocok AUMID atau nama exe);
/// bila tidak ada, kirim perintah media "dok".
pub fn focus_media_source() {
    let src = nul_str(&MEDIA.borrow().src);
    let target = {
        let apps = APPS.borrow();
        apps.iter()
            .find(|a| {
                if a.wins.is_empty() || src.is_empty() {
                    return false;
                }
                let mut m = !a.aumid.is_empty() && a.aumid.eq_ignore_ascii_case(&src);
                if !m && !a.exe.is_empty() {
                    // setara PathFindFileNameW: bagian setelah pemisah terakhir (`\`, `/`, `:`)
                    let file = a.exe.rsplit(|c: char| c == '\\' || c == '/' || c == ':').next().unwrap_or("");
                    m = file.eq_ignore_ascii_case(&src);
                }
                m
            })
            .map(|a| a.wins[0])
    };
    match target {
        Some(h) => activate_window(h),
        None => media_cmd(1, "dok"),
    }
}

/// Sumber: `ActivateItem` (:2845). Aksi pada item dok; `middle` = klik tengah (aplikasi baru).
pub fn activate_item(idx: i32, middle: bool) {
    if idx < 0 || idx >= NITEMS.get() {
        return;
    }
    let it = match ITEMS.borrow().get(idx as usize) {
        Some(it) => *it,
        None => return,
    };
    let p = match PILLS.borrow().get(it.pill as usize) {
        Some(p) => *p,
        None => return,
    };
    // jangkar pil: g_pills[pill].x + (item.x + item.w/2), y pil
    let ax = p.x + (it.x + it.w / 2.0) as i32;
    let ay = p.y;
    match it.ty {
        IT_APP => click_app(it.app, middle),
        IT_NOTE => {
            quiet_start(1000);
            shell_open(NOTE_APP); // Notepad Store: tanpa Explorer lewat DesktopAppxActivator
        }
        IT_SEARCH | IT_START => panel_toggle(PN_START, ax, ay),
        IT_TASKVIEW => {
            if SHELL_MODE.get() {
                tv_toggle();
            } else {
                quiet_start(1000);
                send_keys(VK_LWIN.0, VK_TAB.0, 0);
            }
        }
        IT_NET => panel_toggle(PN_WIFI, ax, ay),
        IT_VOL => panel_toggle(PN_QS, ax, ay),
        IT_BAT => panel_toggle(PN_BATT, ax, ay),
        IT_TCLOCK | IT_CLOCK | IT_BELL => {
            if SHELL_MODE.get() {
                panel_toggle(PN_CAL, ax, ay);
            } else {
                quiet_start(1000);
                send_keys(VK_LWIN.0, b'N' as u16, 0);
            }
        }
        IT_CHEV => toggle_tray_flyout(ax, ay),
        IT_USB => usb_menu(ax, ay),
        IT_MEDIA_MAIN => focus_media_source(),
        IT_MEDIA_PREV => media_cmd(0, "dok"),
        IT_MEDIA_PLAY => media_cmd(1, "dok"),
        IT_MEDIA_NEXT => media_cmd(2, "dok"),
        _ => {}
    }
}

// ---------------------------------------------------------------- taskbar asli & appbar (win12shell.cpp:2871-2944)
// Taskbar bawaan: auto-hide (tidak memesan ruang) + jendelanya disembunyikan total di semua mode.
// Dipulihkan saat Win12 Shell keluar normal; bila crash, CrashFilter dan win12guard menampilkannya lagi.

/// Hook WinEvent `g_hkShow`: memantau kemunculan taskbar bawaan (EVENT_OBJECT_SHOW).
pub static TRAY_HK_SHOW: SyncCell<Cell<HWINEVENTHOOK>> =
    SyncCell::new(Cell::new(HWINEVENTHOOK(std::ptr::null_mut())));

/// Semua jendela top-level berkelas `class`, berurutan seperti `FindWindowExW(NULL, w, class, NULL)` berulang.
fn find_top_windows(class: &str) -> Vec<HWND> {
    let cls = to_wide_z(class);
    let mut out = Vec::new();
    let mut after: Option<HWND> = None;
    while let Ok(h) = unsafe { FindWindowExW(None, after, pcw(&cls), PCWSTR::null()) } {
        if h.0.is_null() {
            break;
        }
        out.push(h);
        after = Some(h);
    }
    out
}

/// Sumber: `TrayWnd` (:2879). Taskbar milik Explorer (bukan jendela milik kita), di-cache di `TRAY_WND`.
pub fn tray_wnd() -> HWND {
    let cur = TRAY_WND.get();
    let alive = !cur.0.is_null() && unsafe { IsWindow(Some(cur)) }.as_bool();
    if !alive || is_our_window(cur) {
        TRAY_WND.set(HWND(std::ptr::null_mut()));
        if let Some(w) = find_top_windows("Shell_TrayWnd").into_iter().find(|&w| !is_our_window(w)) {
            TRAY_WND.set(w);
        }
    }
    TRAY_WND.get()
}

fn new_appbar(tray: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: tray,
        ..Default::default()
    }
}

/// Sumber: `ShowTrayWindows` (:2889).
fn show_tray_windows(show: bool) {
    let tray = tray_wnd();
    let cmd = if show { SW_SHOWNA } else { SW_HIDE };
    if !tray.0.is_null() {
        let _ = unsafe { ShowWindowAsync(tray, cmd) };
    }
    for s in find_top_windows("Shell_SecondaryTrayWnd") {
        let _ = unsafe { ShowWindowAsync(s, cmd) };
    }
}

/// Sumber: `SaveOrigAutoHide` (:2897).
fn save_orig_auto_hide(v: bool) {
    ORIG_AUTO_HIDE.set(if v { 1 } else { 0 });
    ini_write("state", "OrigAutoHide", if v { "1" } else { "0" });
}

/// Sumber: `HideNativeTaskbar` (:2903). Taskbar bawaan dibuat auto-hide, dinonaktifkan, lalu disembunyikan.
pub fn hide_native_taskbar() {
    let tray = tray_wnd();
    if tray.0.is_null() {
        return;
    }
    let mut ab = new_appbar(tray);
    let st = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut ab) } as u32;
    if ORIG_AUTO_HIDE.get() < 0 {
        save_orig_auto_hide((st & ABS_AUTOHIDE) != 0);
    }
    if (st & ABS_AUTOHIDE) == 0 {
        ab.lParam = LPARAM(ABS_AUTOHIDE as isize);
        unsafe {
            SHAppBarMessage(ABM_SETSTATE, &mut ab);
        }
    }
    let _ = unsafe { EnableWindow(tray, false) };
    show_tray_windows(false);
    for s in find_top_windows("Shell_SecondaryTrayWnd") {
        let _ = unsafe { EnableWindow(s, false) };
    }
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(tray, Some(&mut pid as *mut u32)) };
    if pid != TRAY_PID.get() {
        // segera sembunyikan lagi bila Explorer menampilkan taskbar
        let old = TRAY_HK_SHOW.get();
        if !old.0.is_null() {
            let _ = unsafe { UnhookWinEvent(old) };
        }
        let hook = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_SHOW,
                None,
                Some(win_event_proc),
                pid,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        TRAY_HK_SHOW.set(hook);
        TRAY_PID.set(pid);
    }
}

/// Sumber: `RestoreNativeTaskbar` (:2926). Mengembalikan taskbar bawaan saat Win12 Shell keluar normal.
pub fn restore_native_taskbar() {
    let hook = TRAY_HK_SHOW.get();
    if !hook.0.is_null() {
        let _ = unsafe { UnhookWinEvent(hook) };
        TRAY_HK_SHOW.set(HWINEVENTHOOK(std::ptr::null_mut()));
        TRAY_PID.set(0);
    }
    let tray = tray_wnd();
    if !tray.0.is_null() {
        let _ = unsafe { EnableWindow(tray, true) };
    }
    for s in find_top_windows("Shell_SecondaryTrayWnd") {
        let _ = unsafe { EnableWindow(s, true) };
    }
    show_tray_windows(true);
    let orig = ORIG_AUTO_HIDE.get();
    if orig < 0 {
        return;
    }
    if !tray.0.is_null() {
        let mut ab = new_appbar(tray);
        let flag: u32 = if orig != 0 { ABS_AUTOHIDE } else { ABS_ALWAYSONTOP };
        ab.lParam = LPARAM(flag as isize);
        unsafe {
            SHAppBarMessage(ABM_SETSTATE, &mut ab);
        }
    }
    ORIG_AUTO_HIDE.set(-1);
    let _ = ini_delete("state", "OrigAutoHide");
}

// ---------------------------------------------------------------- ikon tray (notification area) milik Win12 Shell
// Sumber: win12shell.cpp:2946-3095 (NID32, TrayRemove, TrayChanged, TrayApply, TrayHostProc, TrayHostCreate,
// TrayClick, FlyoutLayout). Aplikasi mendaftar ikon lewat Shell_NotifyIcon -> WM_COPYDATA ke "Shell_TrayWnd" milik kita.

/// Sumber: `NID32` (win12shell.cpp:2951). Format tetap di dalam WM_COPYDATA, juga untuk aplikasi 64-bit.
/// Offset sama dengan C++: tip 24, state 280, info 288, guid 936.
#[repr(C)]
pub struct Nid32 {
    pub cb_size: u32,
    pub hwnd: u32,
    pub uid: u32,
    pub flags: u32,
    pub callback_msg: u32,
    pub hicon: u32,
    pub tip: [u16; 128],
    pub state: u32,
    pub state_mask: u32,
    pub info: [u16; 256],
    pub version: u32,
    pub info_title: [u16; 64],
    pub info_flags: u32,
    pub guid: GUID,
    pub balloon_icon: u32,
}

/// Sumber: `TrayIcon` (win12shell.cpp:2965). C++ memakai array tetap; di sini `Vec` dengan batas `MAX_TRAY`.
pub struct TrayIcon {
    pub hwnd: HWND,
    pub uid: u32,
    pub cb_msg: u32,
    pub version: u32,
    pub guid: GUID,
    pub has_guid: bool,
    pub hidden: bool,
    pub hicon: HICON,
    pub icon: Icon,
    pub tip: String,
    /// PID proses pemilik ikon (dari hwnd pengirim), dipakai untuk mencegah ikon ganda.
    pub pid: u32,
}

/// Sumber: `MAXTRAY` (win12shell.cpp:2966).
pub const MAX_TRAY: usize = 64;

pub static TRAY_ICONS: SyncCell<RefCell<Vec<TrayIcon>>> = SyncCell::new(RefCell::new(Vec::new()));
pub static TRAY_LOST: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static FLY_RECT: SyncCell<Cell<RECT>> =
    SyncCell::new(Cell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }));
pub static FLY_SLIDE: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(0.0));
pub static FLY_HOT: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static FLY_N: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static FLY_COLS: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(1));
pub static FLY_ANCHOR_X: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static FLY_BOTTOM: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static FLY_MAP: SyncCell<RefCell<Vec<usize>>> = SyncCell::new(RefCell::new(Vec::new()));
pub static FLY_TRACKING: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

/// Sumber: `TrayRemove` (win12shell.cpp:2973).
pub(crate) fn tray_remove(v: &mut Vec<TrayIcon>, i: usize) {
    let hic = v[i].hicon;
    if !hic.0.is_null() {
        let _ = unsafe { DestroyIcon(hic) };
    }
    icon_free(&mut v[i].icon);
    v.remove(i);
}

/// Sumber: `TrayChanged` (win12shell.cpp:2982).
fn tray_changed() {
    if FLY_OPEN.get() {
        fly_layout();
        let _ = w12_invalidate(FLY.get(), None, false);
    }
}

/// Sumber: `TrayApply` (win12shell.cpp:2987). Memperbarui daftar ikon dari satu pesan Shell_NotifyIcon.
fn tray_apply(msg: u32, n: &Nid32, cb: usize) {
    let use_guid = (n.flags & NIF_GUID.0) != 0
        && cb >= std::mem::offset_of!(Nid32, guid) + std::mem::size_of::<GUID>();
    let hw = HWND(n.hwnd as usize as *mut c_void);
    let mut owner_pid: u32 = 0;
    // SAFETY: hw hanya dipakai untuk membaca PID pemiliknya; hwnd tidak valid menghasilkan 0.
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hw, Some(&mut owner_pid as *mut u32)) };
    let new_tip: String = if n.flags & NIF_TIP.0 != 0 {
        let cap: usize = if cb >= std::mem::offset_of!(Nid32, state) { 128 } else { 64 };
        nul_str(&n.tip[..cap])
    } else {
        String::new()
    };
    let changed = {
        let mut v = TRAY_ICONS.borrow_mut();
        let mut idx = v.iter().position(|t| {
            if use_guid {
                t.has_guid && t.guid == n.guid
            } else {
                t.hwnd.0 == hw.0 && t.uid == n.uid
            }
        });
        // Ikon dari proses yang sama dengan tooltip yang sama dianggap pendaftaran ulang (NIM_ADD berulang
        // dengan hwnd/uid/GUID baru), bukan ikon baru. Tanpa ini ikon menumpuk selama proses itu berjalan.
        if idx.is_none() && msg == NIM_ADD.0 && !new_tip.is_empty() {
            if let Some(k) = v.iter().position(|t| t.pid == owner_pid && t.tip == new_tip) {
                let t = &mut v[k];
                t.hwnd = hw;
                t.uid = n.uid;
                t.has_guid = use_guid;
                t.guid = if use_guid { n.guid } else { unsafe { std::mem::zeroed() } };
                idx = Some(k);
            }
        }
        if msg == NIM_DELETE.0 {
            match idx {
                Some(k) => {
                    tray_remove(&mut v, k);
                    true
                }
                None => false,
            }
        } else if msg == NIM_SETVERSION.0 {
            if let Some(k) = idx {
                v[k].version = n.version;
            }
            false
        } else if msg != NIM_ADD.0 && msg != NIM_MODIFY.0 {
            false
        } else {
            let k = match idx {
                Some(k) => k,
                None => {
                    if msg != NIM_ADD.0 || v.len() >= MAX_TRAY {
                        return;
                    }
                    v.push(TrayIcon {
                        hwnd: hw,
                        uid: n.uid,
                        cb_msg: 0,
                        version: 0,
                        guid: if use_guid { n.guid } else { unsafe { std::mem::zeroed() } },
                        has_guid: use_guid,
                        hidden: false,
                        hicon: HICON(std::ptr::null_mut()),
                        icon: Icon::new(),
                        tip: String::new(),
                        pid: owner_pid,
                    });
                    crate::log::log(&format!(
                        "tray: ikon baru pid={} uid={} guid={} tip={:?} (total {})",
                        owner_pid,
                        n.uid,
                        use_guid,
                        new_tip,
                        v.len() + 1
                    ));
                    v.len() - 1
                }
            };
            let t = &mut v[k];
            if use_guid && t.hwnd.0.is_null() {
                t.hwnd = hw;
            }
            if n.flags & NIF_MESSAGE.0 != 0 {
                t.cb_msg = n.callback_msg;
            }
            if n.flags & NIF_ICON.0 != 0 {
                if !t.hicon.0.is_null() {
                    let _ = unsafe { DestroyIcon(t.hicon) };
                }
                t.hicon = if n.hicon != 0 {
                    unsafe { CopyIcon(HICON(n.hicon as usize as *mut c_void)) }
                        .unwrap_or(HICON(std::ptr::null_mut()))
                } else {
                    HICON(std::ptr::null_mut())
                };
                icon_free(&mut t.icon);
                t.icon.src = if t.hicon.0.is_null() {
                    None
                } else {
                    icon_from_hicon(t.hicon, s(16.0))
                };
            }
            if n.flags & NIF_TIP.0 != 0 {
                let cap: usize = if cb >= std::mem::offset_of!(Nid32, state) { 128 } else { 64 };
                t.tip = nul_str(&n.tip[..cap]);
            }
            if n.flags & NIF_STATE.0 != 0
                && cb >= std::mem::offset_of!(Nid32, info)
                && (n.state_mask & NIS_HIDDEN.0) != 0
            {
                t.hidden = (n.state & NIS_HIDDEN.0) != 0;
            }
            true
        }
    };
    if changed {
        tray_changed();
    }
}

/// Sumber: `TrayHostProc` (win12shell.cpp:3020). Menangkap WM_COPYDATA Shell_NotifyIcon dan meneruskannya ke Explorer.
pub unsafe extern "system" fn tray_host_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_COPYDATA && lp.0 != 0 {
        let cds = &*(lp.0 as usize as *const COPYDATASTRUCT);
        let mut r: usize = 0;
        let ex = tray_wnd();
        let fwd = !ex.0.is_null()
            && SendMessageTimeoutW(ex, WM_COPYDATA, wp, lp, SMTO_ABORTIFHUNG, 4000, Some(&mut r as *mut usize)).0 != 0;
        if cds.dwData == 1 && cds.cbData >= 8 && !cds.lpData.is_null() {
            let p = cds.lpData as *const u8;
            if std::ptr::read_unaligned(p as *const u32) == 0x3475_3423 {
                let msg = std::ptr::read_unaligned(p.add(4) as *const u32);
                let cb = (cds.cbData - 8) as usize;
                let mut n: Nid32 = std::mem::zeroed();
                let copy = cb.min(std::mem::size_of::<Nid32>());
                std::ptr::copy_nonoverlapping(p.add(8), &mut n as *mut Nid32 as *mut u8, copy);
                tray_apply(msg, &n, cb);
                return LRESULT(1);
            }
        }
        return LRESULT(if fwd { r as isize } else { 0 });
    }
    DefWindowProcW(h, m, wp, lp)
}

/// Sumber: `TrayHostCreate` (win12shell.cpp:3043). Jendela penerima Shell_NotifyIcon milik Win12.
pub fn tray_host_create() {
    let ex = tray_wnd();
    let mut r = RECT { left: 0, top: 0, right: 1, bottom: 1 };
    if !ex.0.is_null() {
        let _ = unsafe { GetWindowRect(ex, &mut r) };
    }
    let hi = HI.get();
    let host = match unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("Shell_TrayWnd"),
            w!(""),
            WS_POPUP,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            None,
            None,
            Some(hi),
            None,
        )
    } {
        Ok(h) => h,
        Err(_) => return,
    };
    TRAY_HOST.set(host);
    let _ = unsafe { ChangeWindowMessageFilterEx(host, WM_COPYDATA, MSGFLT_ALLOW, None) };
    let _ = unsafe {
        SetWindowPos(host, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
    };
    // lalu minta semua aplikasi mendaftarkan ulang ikon tray-nya
    set_timer(T_TRAYSYNC, 400);
}

/// Sumber: `TrayClick` (win12shell.cpp:3057). Kirim klik ke aplikasi pemilik ikon sesuai versi NOTIFYICON-nya.
fn tray_click(i: usize, mouse_msg: u32, pt: POINT) {
    let (hwnd, cb_msg, uid, version) = {
        let v = TRAY_ICONS.borrow();
        let Some(t) = v.get(i) else { return };
        (t.hwnd, t.cb_msg, t.uid, t.version)
    };
    if cb_msg == 0 || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return;
    }
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));
    }
    // agar jendela/menu aplikasi boleh tampil di depan
    let _ = unsafe { AllowSetForegroundWindow(pid) };
    let down: u32 = if mouse_msg == WM_RBUTTONUP {
        WM_RBUTTONDOWN
    } else if mouse_msg == WM_LBUTTONUP {
        WM_LBUTTONDOWN
    } else {
        0
    };
    let post = |wp: WPARAM, lp: LPARAM| {
        let _ = unsafe { PostMessageW(Some(hwnd), cb_msg, wp, lp) };
    };
    if version >= 4 {
        let w = WPARAM(((pt.x as u16) as usize) | (((pt.y as u16) as usize) << 16));
        if down != 0 {
            post(w, make_lparam(down as i32, uid as i32));
        }
        post(w, make_lparam(mouse_msg as i32, uid as i32));
        if mouse_msg == WM_LBUTTONUP {
            post(w, make_lparam(NIN_SELECT as i32, uid as i32));
        }
        if mouse_msg == WM_RBUTTONUP {
            post(w, make_lparam(WM_CONTEXTMENU as i32, uid as i32));
        }
    } else {
        let w = WPARAM(uid as usize);
        if down != 0 {
            post(w, LPARAM(down as isize));
        }
        post(w, LPARAM(mouse_msg as isize));
        if version == 3 && mouse_msg == WM_LBUTTONUP {
            post(w, LPARAM(NIN_SELECT as isize));
        }
        if version == 3 && mouse_msg == WM_RBUTTONUP {
            post(w, LPARAM(WM_CONTEXTMENU as isize));
        }
    }
}

/// Sumber: `FlyoutLayout` (win12shell.cpp:3081). Menyusun ikon yang tampil di flyout beserta ukuran dan posisinya.
pub fn fly_layout() {
    // Bar tidak menggambar ikon tray (tidak ada IT_TRAY), jadi flyout ini satu-satunya tempat ikon tray tampil.
    // Ikon NIS_HIDDEN tetap ditampilkan, tetapi setelah ikon biasa (urutan overflow Windows). Deviasi dari C++
    // yang hanya menampilkan ikon tidak tersembunyi: tanpa ini ikon tersembunyi tidak tampil di mana pun.
    let map: Vec<usize> = {
        let v = TRAY_ICONS.borrow();
        let mut idx: Vec<usize> = v
            .iter()
            .enumerate()
            .filter(|(_, t)| t.icon.src.is_some() && unsafe { IsWindow(Some(t.hwnd)) }.as_bool())
            .map(|(i, _)| i)
            .collect();
        // sort_by_key stabil: ikon biasa (false) lebih dulu, urutan asal dipertahankan di dalam tiap kelompok.
        idx.sort_by_key(|&i| v[i].hidden);
        idx
    };
    let n = map.len() as i32;
    let cols: i32 = if n >= 5 {
        5
    } else if n > 0 {
        n
    } else {
        1
    };
    let rows = if n > 0 { (n + cols - 1) / cols } else { 1 };
    let cell = s(40.0);
    let pad = s(8.0);
    let w = if n > 0 { cols * cell + 2 * pad } else { s(160.0) };
    let h = rows * cell + 2 * pad;
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let mut x = FLY_ANCHOR_X.get() - w / 2;
    if x + w > sw - s(6.0) {
        x = sw - s(6.0) - w;
    }
    if x < s(6.0) {
        x = s(6.0);
    }
    let bottom = FLY_BOTTOM.get();
    let top = bottom - h;
    FLY_RECT.set(RECT { left: x, top, right: x + w, bottom });
    FLY_N.set(n);
    FLY_COLS.set(cols);
    *FLY_MAP.borrow_mut() = map;
    let _ = unsafe {
        SetWindowPos(
            FLY.get(),
            Some(HWND_TOPMOST),
            x,
            top + FLY_SLIDE.get() as i32,
            w,
            h,
            SWP_NOACTIVATE,
        )
    };
}

/// Geser flyout saat muncul (dipanggil dari anim_step). Sumber: `FlySlideStep` (win12shell.cpp:3098).
fn fly_slide_step() -> bool {
    if FLY_SLIDE.get() <= 0.0 {
        return false;
    }
    if !FLY_OPEN.get() || FLY.get().0.is_null() {
        FLY_SLIDE.set(0.0);
        return false;
    }
    let mut slide = FLY_SLIDE.get() * 0.6;
    if slide < 0.5 {
        slide = 0.0;
    }
    FLY_SLIDE.set(slide);
    let r = FLY_RECT.get();
    let _ = unsafe {
        SetWindowPos(
            FLY.get(),
            Some(HWND_TOPMOST),
            r.left,
            r.top + slide as i32,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE,
        )
    };
    slide > 0.0
}

/// Sumber: `FlyoutClose` (win12shell.cpp:3108).
pub fn flyout_close() {
    if !FLY_OPEN.get() {
        return;
    }
    FLY_OPEN.set(false);
    FLY_HOT.set(-1);
    let _ = unsafe { ShowWindow(FLY.get(), SW_HIDE) };
    tip_hide();
    invalidate_pills(true, false);
}

/// Sumber: `ToggleTrayFlyout` (win12shell.cpp:3118).
pub fn toggle_tray_flyout(ax: i32, bar_top: i32) {
    let fly = FLY.get();
    if fly.0.is_null() {
        return;
    }
    if FLY_OPEN.get() {
        flyout_close();
        return;
    }
    if TRAY_LOST.get() {
        TRAY_LOST.set(false);
        let _ = unsafe {
            SendNotifyMessageW(HWND_BROADCAST, WM_TASKBARCREATED.get(), WPARAM(0), LPARAM(0))
        };
    }
    FLY_ANCHOR_X.set(ax);
    FLY_BOTTOM.set(bar_top - s(8.0));
    FLY_HOT.set(-1);
    FLY_OPEN.set(true);
    FLY_SLIDE.set(s(10.0) as f32);
    fly_layout();
    let _ = unsafe { ShowWindow(fly, SW_SHOW) };
    // Fokus flyout: tanpa fokus, klik di luar tidak menutupnya (WM_ACTIVATE). Bila SetForegroundWindow
    // gagal, pakai SwitchToThisWindow seperti ActivateWindow (baris ~431). C++ 3130 tidak mengecek hasilnya.
    unsafe {
        if !SetForegroundWindow(fly).as_bool() || GetForegroundWindow() != fly {
            SwitchToThisWindow(fly, true);
        }
    }
    let _ = w12_invalidate(fly, None, false);
    invalidate_pills(true, false);
    apply_pills(false);
    start_anim();
}

/// Sumber: `FlyHit` (win12shell.cpp:3137). Mengembalikan indeks sel (-1 bila di luar).
fn fly_hit(x: i32, y: i32) -> i32 {
    let cell = s(40.0);
    let pad = s(8.0);
    if x < pad || y < pad {
        return -1;
    }
    let cols = FLY_COLS.get().max(1);
    let col = (x - pad) / cell;
    let row = (y - pad) / cell;
    if col >= cols {
        return -1;
    }
    let k = row * cols + col;
    if k < FLY_N.get() {
        k
    } else {
        -1
    }
}

/// Sumber: `TipText` (win12shell.cpp:3147). Tooltip flyout di atas sel.
fn tip_text(msg: &str, cx: i32, top: i32) {
    let tip = TIP.get();
    if tip.0.is_null() || msg.is_empty() {
        tip_hide();
        return;
    }
    let mut wtext = to_wide_z(msg);
    let mut ti = TTTOOLINFOW::default();
    ti.cbSize = std::mem::size_of::<TTTOOLINFOW>() as u32;
    ti.hwnd = MSG.get();
    ti.uId = 1;
    ti.lpszText = PWSTR(wtext.as_mut_ptr());
    let ti_lp = LPARAM(&ti as *const TTTOOLINFOW as isize);
    unsafe {
        SendMessageW(tip, TTM_UPDATETIPTEXTW, Some(WPARAM(0)), Some(ti_lp));
        let sz = SendMessageW(tip, TTM_GETBUBBLESIZE, Some(WPARAM(0)), Some(ti_lp)).0 as usize;
        let tw = (sz & 0xFFFF) as i32;
        let th = ((sz >> 16) & 0xFFFF) as i32;
        SendMessageW(
            tip,
            TTM_TRACKPOSITION,
            Some(WPARAM(0)),
            Some(make_lparam(cx - tw / 2, top - th - s(6.0))),
        );
        SendMessageW(tip, TTM_TRACKACTIVATE, Some(WPARAM(1)), Some(ti_lp));
    }
}

/// Sumber: `RenderFlyout` (win12shell.cpp:3160). Dipanggil lewat WM_W12_RENDER.
fn render_flyout(h: HWND) {
    let _ = unsafe { RemovePropW(h, w!("w12p")) };
    let mut wr = RECT::default();
    let _ = unsafe { GetWindowRect(h, &mut wr) };
    let w = wr.right - wr.left;
    let ht = wr.bottom - wr.top;
    let Some(rt) = RT.borrow().as_ref().cloned() else {
        return;
    };
    if w <= 0 || ht <= 0 || !unsafe { IsWindowVisible(h) }.as_bool() {
        return;
    }
    ensure_mem(w, ht);
    let br = RECT { left: 0, top: 0, right: w, bottom: ht };
    if unsafe { rt.BindDC(MEMDC.get(), &br) }.is_err() {
        return;
    }
    let pal = *PALETTE.borrow();
    let wf = w as f32;
    let hf = ht as f32;
    unsafe {
        rt.BeginDraw();
        rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
    }
    mica_begin(h, &wr);
    surface(0.0, 0.0, wf, hf, sf(16.0), pal.panel);
    let cell = sf(40.0);
    let pad = sf(8.0);
    let ic = sf(16.0);
    let n = FLY_N.get();
    if n == 0 {
        text("Tidak ada ikon tersembunyi", TF_SC.borrow().as_ref(), 0.0, 0.0, wf, hf, pal.sub, 1.0);
    }
    let cols = FLY_COLS.get().max(1);
    let hot = FLY_HOT.get();
    for k in 0..n {
        let x = pad + (k % cols) as f32 * cell;
        let y = pad + (k / cols) as f32 * cell;
        if k == hot {
            fill_rr(
                x + sf(2.0),
                y + sf(2.0),
                x + cell - sf(2.0),
                y + cell - sf(2.0),
                sf(10.0),
                pal.hover,
                1.0,
            );
        }
        let idx = FLY_MAP.borrow().get(k as usize).copied();
        if let Some(idx) = idx {
            let mut tray = TRAY_ICONS.borrow_mut();
            if let Some(t) = tray.get_mut(idx) {
                draw_icon_tile(&mut t.icon, x + cell / 2.0, y + cell / 2.0, ic.round() as i32);
            }
        }
    }
    let hr = unsafe { rt.EndDraw(None, None) };
    if let Err(e) = hr {
        if e.code() == D2DERR_RECREATE_TARGET {
            create_target();
            let _ = w12_invalidate(h, None, false);
            return;
        }
    }
    layer_present(h, &wr, 255);
}

fn fly_lp_x(lp: LPARAM) -> i32 {
    (lp.0 & 0xFFFF) as u16 as i16 as i32
}

fn fly_lp_y(lp: LPARAM) -> i32 {
    ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32
}

/// Indeks `TRAY_ICONS` untuk sel flyout `k` (lewat `FLY_MAP`).
fn fly_tray_index(k: i32) -> Option<usize> {
    FLY_MAP.borrow().get(k as usize).copied()
}

fn fly_tip_of(k: i32) -> String {
    let Some(idx) = fly_tray_index(k) else {
        return String::new();
    };
    let v = TRAY_ICONS.borrow();
    let tip = v.get(idx).map(|t| t.tip.clone()).unwrap_or_default();
    tip
}

/// Sumber: `FlyoutProc` (win12shell.cpp:3187).
pub unsafe extern "system" fn flyout_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_WINDOWPOSCHANGED {
        mica_moved(h, lp);
    }
    match m {
        WM_NCCALCSIZE => {
            if wp.0 != 0 {
                return LRESULT(0);
            }
        }
        WM_NCHITTEST => return LRESULT(HTCLIENT as isize),
        WM_NCACTIVATE => return DefWindowProcW(h, m, WPARAM(1), lp),
        WM_GETMINMAXINFO => {
            let mm = lp.0 as *mut MINMAXINFO;
            (*mm).ptMinTrackSize = POINT { x: 1, y: 1 };
            return LRESULT(0);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(h, &mut ps);
            let _ = EndPaint(h, &ps);
            return LRESULT(0);
        }
        WM_W12_RENDER => {
            render_flyout(h);
            return LRESULT(0);
        }
        WM_ACTIVATE => {
            // klik di luar = tutup (light dismiss)
            if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE {
                flyout_close();
            }
            return LRESULT(0);
        }
        WM_KEYDOWN => {
            if wp.0 == VK_ESCAPE.0 as usize {
                flyout_close();
            }
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            if !FLY_TRACKING.get() {
                let mut t = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: h,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut t);
                FLY_TRACKING.set(true);
            }
            let hit = fly_hit(fly_lp_x(lp), fly_lp_y(lp));
            if hit != FLY_HOT.get() {
                FLY_HOT.set(hit);
                let _ = w12_invalidate(h, None, false);
                if hit >= 0 {
                    let mut wr = RECT::default();
                    let _ = GetWindowRect(h, &mut wr);
                    let cell = s(40.0);
                    let pad = s(8.0);
                    let cols = FLY_COLS.get().max(1);
                    let cx = wr.left + pad + (hit % cols) * cell + cell / 2;
                    tip_text(&fly_tip_of(hit), cx, wr.top);
                } else {
                    tip_hide();
                }
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            FLY_TRACKING.set(false);
            FLY_HOT.set(-1);
            tip_hide();
            let _ = w12_invalidate(h, None, false);
            return LRESULT(0);
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_LBUTTONDBLCLK => {
            let hit = fly_hit(fly_lp_x(lp), fly_lp_y(lp));
            if hit < 0 {
                return LRESULT(0);
            }
            tip_hide();
            let mut wr = RECT::default();
            let _ = GetWindowRect(h, &mut wr);
            let cell = s(40.0);
            let pad = s(8.0);
            let cols = FLY_COLS.get().max(1);
            let pt = POINT {
                x: wr.left + pad + (hit % cols) * cell + cell / 2,
                y: wr.top + pad + (hit / cols) * cell + cell / 2,
            };
            if let Some(idx) = fly_tray_index(hit) {
                tray_click(idx, m, pt);
            }
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(h, m, wp, lp)
}

/// Sumber: `CreateFlyout` (win12shell.cpp:3255). Kelas "Win12ShellFlyout" didaftarkan di core_c4 (T15).
pub fn create_flyout() -> HWND {
    let hi = HI.get();
    let created = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("Win12ShellFlyout"),
            w!(""),
            WS_POPUP,
            0,
            0,
            10,
            10,
            None,
            None,
            Some(hi),
            None,
        )
    };
    let Ok(h) = created else {
        return HWND(std::ptr::null_mut());
    };
    let _ = unsafe {
        SetPropW(h, w!("w12l"), Some(HANDLE(1usize as *mut std::ffi::c_void)))
    };
    h
}

fn hide_tray_bar(hw: HWND) {
    if unsafe { IsWindowEnabled(hw) }.as_bool() {
        let _ = unsafe { EnableWindow(hw, false) };
    }
    if unsafe { IsWindowVisible(hw) }.as_bool() {
        let _ = unsafe { ShowWindowAsync(hw, SW_HIDE) };
    }
}

/// Sumber: `TrayCheck` (win12shell.cpp:3265). Tiap 500 ms: taskbar bawaan tetap tersembunyi, jendela tray kita
/// tetap di atas milik Explorer, ikon mati dibuang.
pub fn tray_check() {
    let tray = tray_wnd();
    if !tray.0.is_null() {
        hide_tray_bar(tray);
    }
    let mut sec: Option<HWND> = None;
    loop {
        let next = unsafe {
            FindWindowExW(None, sec, w!("Shell_SecondaryTrayWnd"), PCWSTR::null())
        }
        .ok();
        match next {
            Some(sw) => {
                hide_tray_bar(sw);
                sec = Some(sw);
            }
            None => break,
        }
    }
    let host = TRAY_HOST.get();
    if !host.0.is_null() {
        let found = unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.ok();
        if found != Some(host) {
            let _ = unsafe {
                SetWindowPos(
                    host,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
            TRAY_LOST.set(true);
        }
    }
    let mut changed = false;
    {
        let mut v = TRAY_ICONS.borrow_mut();
        let mut i = 0usize;
        while i < v.len() {
            if !unsafe { IsWindow(Some(v[i].hwnd)) }.as_bool() {
                tray_remove(&mut v, i);
                changed = true;
            } else {
                i += 1;
            }
        }
    }
    if changed {
        tray_changed();
    }
}

/// Tinggi area kerja yang dicadangkan di bawah layar (px). Juga tinggi alas adaptif.
/// Sumber: `ReserveH` (win12shell.cpp:3286).
pub fn reserve_h() -> i32 {
    if MODE.get() == MODE_CLASSIC {
        bar_h()
    } else {
        bar_h() + s(MARGIN_B as f32) + s(RESERVE_GAP as f32)
    }
}

/// Jendela maksimal di monitor utama beserta rect layarnya, dicatat sebelum area kerja berubah.
struct ZoomRefit {
    h: HWND,
    rc: RECT,
}

/// Callback `EnumWindows`: kumpulkan jendela maksimal di monitor utama.
/// Jendela milik shell sendiri dilewati: rumus bingkai hanya berlaku untuk jendela aplikasi.
unsafe extern "system" fn zoom_refit_enum(h: HWND, lp: LPARAM) -> BOOL {
    let list = &mut *(lp.0 as *mut Vec<ZoomRefit>);
    if !is_our_window(h) && zoomed_on_primary(h) {
        let mut rc = RECT::default();
        if GetWindowRect(h, &mut rc).is_ok() {
            list.push(ZoomRefit { h, rc });
        }
    }
    BOOL(1)
}

/// Menata ulang jendela maksimal setelah area kerja berubah dari `old` ke `new`.
/// Windows tidak selalu menyesuaikan jendela maksimal saat area berubah, jadi tepi dihitung ulang:
/// rect target = area baru + selisih bingkai (rect jendela - area lama). Dipasang sekali, tanpa animasi.
/// Jendela yang sudah tidak maksimal di monitor utama (diminimize/dipulihkan pengguna) dilewati.
fn zoom_refit_apply(old: RECT, new: RECT, list: &[ZoomRefit]) {
    for z in list {
        if !zoomed_on_primary(z.h) {
            continue;
        }
        let left = new.left + (z.rc.left - old.left);
        let top = new.top + (z.rc.top - old.top);
        let right = new.right + (z.rc.right - old.right);
        let bottom = new.bottom + (z.rc.bottom - old.bottom);
        let _ = unsafe {
            SetWindowPos(
                z.h,
                None,
                left,
                top,
                right - left,
                bottom - top,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
            )
        };
    }
}

/// Mode shell: tidak ada pengelola appbar Explorer, jadi area kerja layar utama diatur langsung.
/// Tidak menyiarkan bila tidak berubah: WM_SETTINGCHANGE memicu Relayout.
/// Sumber: `ShellWorkArea` (win12shell.cpp:3290).
pub(crate) fn shell_work_area(reserve: bool) {
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let mut r = RECT { left: 0, top: 0, right: sw, bottom: sh };
    if reserve {
        r.bottom -= reserve_h();
    }
    let mut cur = RECT::default();
    let got = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut cur as *mut RECT as *mut std::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok();
    if got && unsafe { EqualRect(&cur, &r) }.as_bool() {
        return;
    }
    // Catat jendela maksimal SEBELUM area berubah: Windows kadang sudah menggesernya saat area mengecil,
    // jadi rect lama harus diambil dari sini, bukan dari GetWindowRect sesudahnya.
    let mut zoomed: Vec<ZoomRefit> = Vec::new();
    if got {
        let _ = unsafe {
            EnumWindows(Some(zoom_refit_enum), LPARAM(&mut zoomed as *mut Vec<ZoomRefit> as isize))
        };
    }
    let set_ok = unsafe {
        SystemParametersInfoW(
            SPI_SETWORKAREA,
            0,
            Some(&mut r as *mut RECT as *mut std::ffi::c_void),
            SPIF_SENDCHANGE,
        )
    }
    .is_ok();
    if got && set_ok && !zoomed.is_empty() {
        zoom_refit_apply(cur, r, &zoomed);
    }
}

/// Sumber: `AppBarSetPos` (win12shell.cpp:3298).
pub(crate) fn app_bar_set_pos() {
    if SHELL_MODE.get() {
        shell_work_area(true);
        return;
    }
    if !AB_REG.get() {
        return;
    }
    let mut ab = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: MSG.get(),
        uEdge: ABE_BOTTOM,
        ..Default::default()
    };
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let reserve = reserve_h();
    ab.rc = RECT { left: 0, top: sh - reserve, right: sw, bottom: sh };
    unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut ab) };
    ab.rc.top = ab.rc.bottom - reserve;
    unsafe { SHAppBarMessage(ABM_SETPOS, &mut ab) };
}

/// Sumber: `AppBarRegister` (win12shell.cpp:3314).
pub fn app_bar_register() {
    if SHELL_MODE.get() {
        shell_work_area(true);
        return;
    }
    if !AB_REG.get() {
        let mut ab = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: MSG.get(),
            uCallbackMessage: WM_APPBAR,
            ..Default::default()
        };
        let ok = unsafe { SHAppBarMessage(ABM_NEW, &mut ab) } != 0;
        AB_REG.set(ok);
    }
    app_bar_set_pos();
}

/// Sumber: `AppBarRemove` (win12shell.cpp:3327).
pub fn app_bar_remove() {
    if SHELL_MODE.get() {
        shell_work_area(false);
        return;
    }
    if !AB_REG.get() {
        return;
    }
    let mut ab = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: MSG.get(),
        ..Default::default()
    };
    unsafe { SHAppBarMessage(ABM_REMOVE, &mut ab) };
    AB_REG.set(false);
}


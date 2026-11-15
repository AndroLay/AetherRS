//! Sumber: win12shell.cpp:4531-5284 (OnTimer s.d. wWinMain). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H04.
use super::prelude::*;
use std::ffi::c_void;
use std::time::{Duration, Instant};
use windows::Win32::System::SystemServices::{
    APPCOMMAND_MEDIA_NEXTTRACK, APPCOMMAND_MEDIA_PLAY_PAUSE, APPCOMMAND_MEDIA_PREVIOUSTRACK,
    APPCOMMAND_VOLUME_DOWN, APPCOMMAND_VOLUME_MUTE, APPCOMMAND_VOLUME_UP,
};
use windows::Win32::System::Memory::{GetProcessHeap, HeapCompact, HEAP_FLAGS};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, CreateThread, RegisterWaitForSingleObject, Sleep, UnregisterWait,
    WaitForSingleObject, INFINITE, THREAD_CREATION_FLAGS, WT_EXECUTEONLYONCE,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_NOTIFY,
    REG_NOTIFY_CHANGE_LAST_SET,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::NetworkManagement::IpHelper::CancelMibChangeNotify2;
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount, GetWindowsDirectoryW};
// PORT: RegisterShellHookWindow/DeregisterShellHookWindow ada di UI::WindowsAndMessaging (prelude).
use windows::Win32::UI::Shell::{ShellExecuteW, ABN_POSCHANGED};
// Impor tambahan untuk crash_filter dan w_win_main (port wWinMain, C++ 5108-5284).
use windows::Win32::System::Diagnostics::Debug::{
    SetUnhandledExceptionFilter, EXCEPTION_EXECUTE_HANDLER, EXCEPTION_POINTERS,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_SINGLETHREADED};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, IWICImagingFactory};
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, TOOLTIPS_CLASSW, TTF_ABSOLUTE,
    TTF_TRACK, TTM_ADDTOOLW, TTM_SETMAXTIPWIDTH, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::core::PCSTR;

// ---------------------------------------------------------------- OnTimer (C++ 4531-4579)

/// Dispatch WM_TIMER (C++ OnTimer). `id` = nilai T_* (i32).
pub fn on_timer(id: i32) {
    match id {
        T_CLOCK => {
            clock_update();
            arm_clock();
            relayout();
            notch_invalidate();
        }
        T_STATUS => {
            let had = HAS_BAT.get();
            let mu = MUTE.get();
            let v = VOL.get();
            let was_net = NET.get();
            let mut ch = net_update();
            ch = battery_update() || ch;
            volume_attach();
            ch = ch || v != VOL.get() || mu != MUTE.get();
            if had != HAS_BAT.get() || (was_net == NET_BOTH) != (NET.get() == NET_BOTH) {
                relayout();
            } else if ch {
                invalidate_pills(true, false);
            }
            if PANEL_OPEN.get() >= 0 && !PAN.get().0.is_null() && (ch || PANEL_OPEN.get() == PN_BATT) {
                let _ = w12_invalidate(PAN.get(), None, false);
            }
            // Fase 4 Task 7: stopHosts= hanya mode shell; ikut tick 10 dtk ini (tanpa timer baru).
            // Tidak ada pemangkasan berkala: pemangkasan hanya setelah panel/notch ditutup (TrimSoon) dan saat layar penuh.
            stop_hosts_tick();
        }
        T_TRIM => {
            kill_timer(T_TRIM);
            // Fase 4 Task 4: heap dipadatkan sebelum working set dipangkas.
            // PORT: C++ _heapmin() tidak diport (tidak ada padanan CRT Rust); dicatat sebagai deviasi.
            unsafe {
                if let Ok(heap) = GetProcessHeap() {
                    HeapCompact(heap, HEAP_FLAGS(0));
                }
            }
            trim_working_set(true);
        }
        T_START_TRIM => {
            kill_timer(T_START_TRIM);
            deactivate_native_start_processes();
        }
        T_PEEK => {
            kill_timer(T_PEEK);
            PEEK.set(false);
            apply_pills(false);
        }
        T_FGPOLL => {
            update_foreground(false);
            tray_check();
        }
        T_WALLPAPER => {
            mica_reload_wallpaper_if_changed(false, true);
        }
        T_FSSTART => {
            kill_timer(T_FSSTART); // indeks berkas dimulai 60 s setelah jendela tampil
            fs_start();
        }
        T_DIAG => diag_tick(),
        T_MINANIM => min_anim_step(),
        T_QUIET => {
            kill_timer(T_QUIET);
            QUIET.set(false); // akhir masa tenang: baca ulang fokus nyata
            update_foreground(true);
        }
        T_TRAYSYNC => {
            kill_timer(T_TRAYSYNC);
            TRAY_LOST.set(false);
            let _ = unsafe {
                SendNotifyMessageW(HWND_BROADCAST, WM_TASKBARCREATED.get(), WPARAM(0), LPARAM(0))
            };
        }
        T_MEDIA => {
            // Mode event: sekali jalan (baca ulang judul 1,5 detik setelah event); mode polling: tiap 3 detik.
            if MEDIA_EV.get() {
                kill_timer(T_MEDIA);
                MEDIA_WANT.set(true);
            }
            media_tick();
        }
        T_MEDIA_FAST => {
            kill_timer(T_MEDIA_FAST);
            media_tick();
        }
        T_REFRESH => {
            kill_timer(T_REFRESH);
            refresh_windows();
        }
        T_ANIM => anim_step(),
        T_TIP => tip_show(),
        T_SYSW => {
            sys_sample();
            let (ph, shown) = {
                let p = PILLS.borrow();
                (p[P_SYS as usize].h, p[P_SYS as usize].shown)
            };
            if shown {
                let _ = w12_invalidate(ph, None, false);
            }
        }
        T_NL => {
            if PANEL_OPEN.get() == PN_START {
                nl_api_refresh();
            } else {
                kill_timer(T_NL);
            }
        }
        T_NL_ICON => {
            kill_timer(T_NL_ICON);
            if PANEL_OPEN.get() == PN_START && !ST_NL_ICON_TRIED.get() {
                start_nl_icon_load(); // C++ StartNlIconLoad; port di start.rs
                if !PAN.get().0.is_null() {
                    let _ = w12_invalidate(PAN.get(), None, false);
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------- tombol Windows (C++ 4581-4703)
// Keputusan pengguna 2026-11-06. Kait keyboard tingkat rendah di thread sendiri, agar input tidak tertahan
// saat thread UI sibuk. Win tunggal dan Ctrl+Esc membuka/menutup Start Win12; Win+S membuka Start untuk pencarian.
// Kombinasi Win lainnya dan Ctrl/Shift/Alt+Win tetap ke Windows. Start bawaan ditahan pada pintasan yang dialihkan.

static KB_HOOK: SyncCell<Cell<HHOOK>> = SyncCell::new(Cell::new(HHOOK(std::ptr::null_mut())));
static KB_THREAD: SyncCell<Cell<HANDLE>> = SyncCell::new(Cell::new(HANDLE(std::ptr::null_mut())));
static KB_TID: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
// Status Win; hanya dipakai thread kait (C++ g_winDown, g_winCombo, g_winSearch, g_winSSwallowed, g_ctrlEscPending).
static KB_WIN_DOWN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static KB_WIN_COMBO: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static KB_WIN_SEARCH: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static KB_WIN_SS_SWALLOWED: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static KB_CTRL_ESC_PENDING: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

/// Pembungkus GetAsyncKeyState (C++ memakai `GetAsyncKeyState(vk) < 0` langsung).
#[inline]
fn ak(vk: u32) -> i16 {
    unsafe { GetAsyncKeyState(vk as i32) }
}

/// C++ KbMaskStartKey (4594-4602). Start OS terbuka saat Win dilepas bila tidak ada tombol lain tercatat;
/// VK 0xE8 (tak terpakai) dikirim sebagai pengganti. Input sintetis ini dilewatkan kait (dwExtraInfo = W12_KBSIG).
fn kb_mask_start_key() {
    let mut inp: [INPUT; 2] = unsafe { std::mem::zeroed() };
    for i in inp.iter_mut() {
        i.r#type = INPUT_KEYBOARD;
        i.Anonymous.ki.dwExtraInfo = W12_KBSIG;
    }
    inp[0].Anonymous.ki.wVk = VIRTUAL_KEY(0xE8);
    inp[1].Anonymous.ki.wVk = VIRTUAL_KEY(0xE8);
    inp[1].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
    unsafe { SendInput(&inp, std::mem::size_of::<INPUT>() as i32) };
}

/// C++ KbHookProc (4604-4694). Berjalan di thread kait (kb_thread).
unsafe extern "system" fn kb_hook_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k: &KBDLLHOOKSTRUCT = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        if k.dwExtraInfo != W12_KBSIG {
            // input sintetis milik shell sendiri dilewatkan (blok di atas)
            let vk = k.vkCode;
            let win = vk == VK_LWIN.0 as u32 || vk == VK_RWIN.0 as u32;
            let msg = wp.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;

            if SHELL_MODE.get() {
                let r = tasks_key(k, wp.0, KB_WIN_DOWN.get());
                if r == 2 {
                    KB_WIN_COMBO.set(true);
                    KB_WIN_SEARCH.set(false);
                }
                if r != 0 {
                    if KB_WIN_DOWN.get() {
                        kb_mask_start_key();
                    }
                    return LRESULT(1);
                }
            }

            if !win && vk == 'S' as u32 && KB_WIN_SS_SWALLOWED.get() {
                if !down {
                    KB_WIN_SS_SWALLOWED.set(false);
                    return LRESULT(1);
                }
                if KB_WIN_DOWN.get() || ak('S' as u32) < 0 {
                    return LRESULT(1);
                }
                KB_WIN_SS_SWALLOWED.set(false); // pulihkan bila Windows tidak mengirim pelepasan S sebelumnya
            }

            if !win && vk == VK_ESCAPE.0 as u32 {
                if down {
                    let ctrl_esc = !KB_WIN_DOWN.get()
                        && ak(VK_CONTROL.0 as u32) < 0
                        && ak(VK_SHIFT.0 as u32) >= 0
                        && ak(VK_MENU.0 as u32) >= 0;
                    let action = crate::ui_logic::w12_start_shortcut_action(
                        false,
                        false,
                        ctrl_esc,
                        WIN_KEY.get(),
                        QUITTING.get(),
                        !MSG.get().0.is_null(),
                    );
                    if action == W12_STARTKEY_TOGGLE {
                        KB_CTRL_ESC_PENDING.set(true);
                        return LRESULT(1);
                    }
                    KB_CTRL_ESC_PENDING.set(false);
                } else if KB_CTRL_ESC_PENDING.get() {
                    KB_CTRL_ESC_PENDING.set(false);
                    let action = crate::ui_logic::w12_start_shortcut_action(
                        false,
                        false,
                        true,
                        WIN_KEY.get(),
                        QUITTING.get(),
                        !MSG.get().0.is_null(),
                    );
                    if action != W12_STARTKEY_NONE {
                        let _ = PostMessageW(Some(MSG.get()), WM_WINKEY, WPARAM(action as usize), LPARAM(0));
                        return LRESULT(1);
                    }
                }
            }

            if win && down {
                // tekanan baru (bukan ulangan): status direset, termasuk bila pelepasan sebelumnya hilang (mis. Win+L)
                if !KB_WIN_DOWN.get() || ak(vk) >= 0 {
                    KB_WIN_DOWN.set(true);
                    KB_WIN_COMBO.set(
                        ak(VK_CONTROL.0 as u32) < 0 || ak(VK_SHIFT.0 as u32) < 0 || ak(VK_MENU.0 as u32) < 0,
                    );
                    KB_WIN_SEARCH.set(false);
                    if ak('S' as u32) >= 0 {
                        KB_WIN_SS_SWALLOWED.set(false);
                    }
                }
            } else if win {
                let action = crate::ui_logic::w12_start_shortcut_action(
                    KB_WIN_DOWN.get() && !KB_WIN_COMBO.get() && !KB_WIN_SEARCH.get(),
                    KB_WIN_SEARCH.get() && !KB_WIN_COMBO.get(),
                    false,
                    WIN_KEY.get(),
                    QUITTING.get(),
                    !MSG.get().0.is_null(),
                );
                KB_WIN_DOWN.set(false);
                KB_WIN_COMBO.set(false);
                KB_WIN_SEARCH.set(false);
                if action != W12_STARTKEY_NONE {
                    // Pelepasan Win asli ditahan; penggantinya (0xE8 down/up + VK Win up) dikirim di sini.
                    let mut inp: [INPUT; 3] = std::mem::zeroed();
                    for i in inp.iter_mut() {
                        i.r#type = INPUT_KEYBOARD;
                        i.Anonymous.ki.dwExtraInfo = W12_KBSIG;
                    }
                    inp[0].Anonymous.ki.wVk = VIRTUAL_KEY(0xE8);
                    inp[1].Anonymous.ki.wVk = VIRTUAL_KEY(0xE8);
                    inp[1].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
                    inp[2].Anonymous.ki.wVk = VIRTUAL_KEY(k.vkCode as u16);
                    inp[2].Anonymous.ki.dwFlags = KEYBD_EVENT_FLAGS(KEYEVENTF_KEYUP.0 | KEYEVENTF_EXTENDEDKEY.0);
                    SendInput(&inp, std::mem::size_of::<INPUT>() as i32);
                    let _ = PostMessageW(Some(MSG.get()), WM_WINKEY, WPARAM(action as usize), LPARAM(0));
                    return LRESULT(1);
                }
            } else if KB_WIN_DOWN.get() && down {
                if vk == 'S' as u32
                    && !KB_WIN_COMBO.get()
                    && ak(VK_CONTROL.0 as u32) >= 0
                    && ak(VK_SHIFT.0 as u32) >= 0
                    && ak(VK_MENU.0 as u32) >= 0
                {
                    KB_WIN_SEARCH.set(true);
                    KB_WIN_SS_SWALLOWED.set(true);
                    return LRESULT(1);
                }
                // Fase 5 Task 5 (ruling 5m): Win+Shift+S di mode shell. Kait hanya memposting pesan; ShellKey menjalankan ms-screenclip.
                if SHELL_MODE.get()
                    && vk == 'S' as u32
                    && ak(VK_SHIFT.0 as u32) < 0
                    && ak(VK_CONTROL.0 as u32) >= 0
                    && ak(VK_MENU.0 as u32) >= 0
                {
                    KB_WIN_COMBO.set(true); // pelepasan Win sesudahnya tidak membuka Start
                    KB_WIN_SEARCH.set(false);
                    KB_WIN_SS_SWALLOWED.set(true); // pelepasan S berikutnya ditelan blok S di atas
                    kb_mask_start_key();
                    let _ = PostMessageW(Some(MSG.get()), WM_W12_SHELLKEY, WPARAM('S' as usize), LPARAM(0));
                    return LRESULT(1);
                }
                // Mode shell: pintasan yang biasanya ditangani Explorer. Hanya Win+E/R/I/D/A/X, angka 1-9, atau
                // panah (Fase 5 Task 3) tanpa Ctrl/Alt/Shift.
                let shell_vk = matches!(vk, 0x45 | 0x52 | 0x49 | 0x44 | 0x41 | 0x58) // E R I D A X
                    || ('1' as u32..='9' as u32).contains(&vk)
                    || (VK_LEFT.0 as u32..=VK_DOWN.0 as u32).contains(&vk);
                if SHELL_MODE.get()
                    && shell_vk
                    && ak(VK_CONTROL.0 as u32) >= 0
                    && ak(VK_SHIFT.0 as u32) >= 0
                    && ak(VK_MENU.0 as u32) >= 0
                {
                    KB_WIN_COMBO.set(true); // pelepasan Win sesudahnya tidak membuka Start
                    KB_WIN_SEARCH.set(false);
                    kb_mask_start_key();
                    let _ = PostMessageW(Some(MSG.get()), WM_W12_SHELLKEY, WPARAM(vk as usize), LPARAM(0));
                    return LRESULT(1);
                }
                KB_WIN_COMBO.set(true);
                KB_WIN_SEARCH.set(false);
            }
        }
    }
    CallNextHookEx(Some(KB_HOOK.get()), code, wp, lp)
}

/// C++ KbThread (4696-4703). WM_QUIT dari kb_hook_stop menghentikan loop pesan.
unsafe extern "system" fn kb_thread(_param: *mut c_void) -> u32 {
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_hook_proc), Some(HI.get()), 0).ok();
    KB_HOOK.set(hook.unwrap_or(HHOOK(std::ptr::null_mut())));
    let mut m: MSG = std::mem::zeroed();
    while GetMessageW(&mut m, None, 0, 0).0 > 0 {}
    if !KB_HOOK.get().0.is_null() {
        let _ = UnhookWindowsHookEx(KB_HOOK.get());
        KB_HOOK.set(HHOOK(std::ptr::null_mut()));
    }
    0
}

/// C++ KbHookStart (4705-4708): thread kait dinyalakan bila tombol Win dipakai atau mode shell aktif.
pub fn kb_hook_start() {
    if (WIN_KEY.get() || SHELL_MODE.get()) && KB_THREAD.get().0.is_null() {
        let mut tid: u32 = 0;
        let th = unsafe {
            CreateThread(
                None,
                0,
                Some(kb_thread),
                None,
                THREAD_CREATION_FLAGS(0),
                Some(&mut tid as *mut u32),
            )
        };
        if let Ok(h) = th {
            KB_THREAD.set(h);
            KB_TID.set(tid);
        }
    }
}

/// C++ KbHookStop (4710-4717).
pub fn kb_hook_stop() {
    let th = KB_THREAD.get();
    if th.0.is_null() {
        return;
    }
    unsafe {
        let _ = PostThreadMessageW(KB_TID.get(), WM_QUIT, WPARAM(0), LPARAM(0));
        WaitForSingleObject(th, 1000);
        let _ = CloseHandle(th);
    }
    KB_THREAD.set(HANDLE(std::ptr::null_mut()));
}

// ---------------------------------------------------------------- Start native (C++ 4719-4786)

/// C++ DeactivateNativeStartProcesses (4727-4768). Di C, kFreezeNativeStart = false sehingga fungsi keluar
/// pada baris pertama. Port: no-op. Tidak ada NtSuspendProcess / SetProcessWorkingSetSize (deviasi terdokumentasi).
pub fn deactivate_native_start_processes() {}

/// C++ ResumeNativeStartProcesses (4770-4786). g_suspendedNativeCount selalu 0 karena tidak pernah dibekukan: no-op.
pub fn resume_native_start_processes() {}

// ---------------------------------------------------------------- StartFromKey (C++ 4788-4832)

/// Pembungkus InvalidateRect(g_pan, nullptr, FALSE) bila panel ada (lewat w12_invalidate: panel berlapis).
fn invalidate_pan() {
    let p = PAN.get();
    if !p.0.is_null() {
        let _ = w12_invalidate(p, None, false);
    }
}

/// C++ StartFromKey. Buka/tutup Start Win12 di atas ikon Start dok; fallback: panel di tengah bawah layar.
/// `shortcut` = W12_STARTKEY_TOGGLE atau W12_STARTKEY_SEARCH (ui_logic.rs).
pub fn start_from_key(shortcut: i32) {
    let search = shortcut == W12_STARTKEY_SEARCH;
    if PANEL_OPEN.get() == PN_START {
        if !search {
            panel_close();
            return;
        }
        start_reset();
        invalidate_pan();
        return;
    }
    if FULLSCREEN.get() {
        if MICA_FULLSCREEN_SNAPSHOT.get() {
            mica_restore_wallpaper();
        }
        let _ = mica_reload_wallpaper_if_changed(false, false); // jika capture game gagal, wallpaper terbaru tetap dipakai
        MICA_FULLSCREEN_SNAPSHOT.set(mica_capture_fullscreen()); // satu sampel sebelum Win12 muncul di atas game
        FULLSCREEN_START_PEEK.set(true);
        base_update(true);
        apply_pills(true);
    }
    let start_idx = ITEMS.borrow().iter().position(|it| it.ty == IT_START);
    if let Some(i) = start_idx {
        activate_item(i as i32, false);
        if search && PANEL_OPEN.get() == PN_START {
            start_reset();
            invalidate_pan();
        }
        if FULLSCREEN_START_PEEK.get() && PANEL_OPEN.get() != PN_START {
            FULLSCREEN_START_PEEK.set(false);
            mica_restore_wallpaper();
            base_update(false);
            apply_pills(false);
        }
        return;
    }
    let (left, right, bottom) = {
        let mut work: RECT = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work as *mut RECT as *mut c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .is_ok();
        if ok {
            (work.left, work.right, work.bottom)
        } else {
            let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
            (0, w, h)
        }
    };
    panel_toggle(PN_START, (left + right) / 2, bottom);
    if search && PANEL_OPEN.get() == PN_START {
        start_reset();
        invalidate_pan();
    }
}

// ---------------------------------------------------------------- Pemantau wallpaper (C++ 4834-4856)

static WALL_KEY: SyncCell<Cell<HKEY>> = SyncCell::new(Cell::new(HKEY(std::ptr::null_mut())));
static WALL_EV: SyncCell<Cell<HANDLE>> = SyncCell::new(Cell::new(HANDLE(std::ptr::null_mut())));
static WALL_WAIT: SyncCell<Cell<HANDLE>> = SyncCell::new(Cell::new(HANDLE(std::ptr::null_mut())));

/// C++ WallWatchCb: kunci registri berubah, kirim WM_W12_WALLCHG ke jendela utama.
unsafe extern "system" fn wall_watch_cb(_param: *mut c_void, _fired: bool) {
    let m = MSG.get();
    if !m.0.is_null() {
        let _ = PostMessageW(Some(m), WM_W12_WALLCHG, WPARAM(0), LPARAM(0));
    }
}

/// C++ WallWatchStop (4842-4847).
fn wall_watch_stop() {
    let w = WALL_WAIT.get();
    if !w.0.is_null() {
        let _ = unsafe { UnregisterWait(w) };
        WALL_WAIT.set(HANDLE(std::ptr::null_mut()));
    }
    let e = WALL_EV.get();
    if !e.0.is_null() {
        let _ = unsafe { CloseHandle(e) };
        WALL_EV.set(HANDLE(std::ptr::null_mut()));
    }
    let k = WALL_KEY.get();
    if !k.0.is_null() {
        let _ = unsafe { RegCloseKey(k) };
        WALL_KEY.set(HKEY(std::ptr::null_mut()));
    }
}

/// C++ WallWatchArm (4848-4856): pasang notifikasi sekali jalan untuk HKCU\Control Panel\Desktop.
pub fn wall_watch_arm() {
    let w = WALL_WAIT.get();
    if !w.0.is_null() {
        let _ = unsafe { UnregisterWait(w) };
        WALL_WAIT.set(HANDLE(std::ptr::null_mut()));
    }
    if WALL_KEY.get().0.is_null() {
        let mut k = HKEY(std::ptr::null_mut());
        let r = unsafe {
            RegOpenKeyExW(HKEY_CURRENT_USER, w!("Control Panel\\Desktop"), None, KEY_NOTIFY, &mut k)
        };
        WALL_KEY.set(if r == ERROR_SUCCESS { k } else { HKEY(std::ptr::null_mut()) });
    }
    if WALL_EV.get().0.is_null() {
        let ev = unsafe { CreateEventW(None, false, false, PCWSTR::null()) };
        WALL_EV.set(ev.unwrap_or(HANDLE(std::ptr::null_mut())));
    }
    let k = WALL_KEY.get();
    let e = WALL_EV.get();
    if k.0.is_null() || e.0.is_null() {
        return;
    }
    let r = unsafe { RegNotifyChangeKeyValue(k, false, REG_NOTIFY_CHANGE_LAST_SET, Some(e), true) };
    if r != ERROR_SUCCESS {
        return;
    }
    let mut w_new = HANDLE(std::ptr::null_mut());
    let reg = unsafe {
        RegisterWaitForSingleObject(
            &mut w_new,
            e,
            Some(wall_watch_cb),
            None,
            INFINITE,
            WT_EXECUTEONLYONCE,
        )
    };
    if reg.is_ok() {
        WALL_WAIT.set(w_new);
    }
}

// ---------------------------------------------------------------- Shutdown (C++ 4858-4891)

// ---------------------------------------------------------------- AppCmd (C++ 4893-4929)

/// `FAPPCOMMAND_MASK` (winuser.h), tidak ada di crate windows 0.61.3.
const FAPPCOMMAND_MASK_W12: u32 = 0xF000;

/// Pengganti makro `GET_APPCOMMAND_LPARAM` (`(short)HIWORD(lp) & ~FAPPCOMMAND_MASK`).
pub fn get_appcommand_lparam(lp: LPARAM) -> i32 {
    let hi = ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    hi & !(FAPPCOMMAND_MASK_W12 as i32)
}

/// Pengganti makro `GET_DEVICE_LPARAM` (`HIWORD(lp) & FAPPCOMMAND_MASK`).
pub fn get_device_lparam(lp: LPARAM) -> u32 {
    (((lp.0 >> 16) & 0xFFFF) as u32) & FAPPCOMMAND_MASK_W12
}

/// Riwayat lompat lagu (C++ `s_at`, `s_n`) dan batas blokir (C++ `s_blockUntil`).
static APPCMD_AT: SyncCell<RefCell<Vec<Instant>>> = SyncCell::new(RefCell::new(Vec::new()));
static APPCMD_BLOCK: SyncCell<Cell<Option<Instant>>> = SyncCell::new(Cell::new(None));

/// Sumber: AppCmdSkipAllowed (C++ 4897-4915). Lebih dari 4 lompat dalam 10 dtk diblokir 60 dtk.
fn app_cmd_skip_allowed() -> bool {
    let now = Instant::now();
    if let Some(until) = APPCMD_BLOCK.get() {
        if now < until {
            return false;
        }
        APPCMD_BLOCK.set(None);
    }
    let mut at = APPCMD_AT.borrow_mut();
    at.retain(|t| now.duration_since(*t) < Duration::from_millis(10_000));
    if at.len() >= 4 {
        APPCMD_BLOCK.set(Some(now + Duration::from_secs(60)));
        at.clear();
        crate::log::log("media: lompat lagu dari APPCOMMAND > 4 kali dalam 10 dtk, diblokir 60 dtk");
        return false;
    }
    at.push(now);
    true
}

/// Sumber: ShellAppCommand (C++ 4917-4929). `true` = perintah ditelan (tidak diteruskan ke app lain).
pub fn shell_app_command(cmd: i32, dev: u32) -> bool {
    let src = if dev == FAPPCOMMAND_MOUSE {
        "appcommand-mouse"
    } else if dev == FAPPCOMMAND_OEM {
        "appcommand-oem"
    } else {
        "appcommand-tombol"
    };
    let c = cmd as u32;
    if c == APPCOMMAND_VOLUME_UP.0 {
        let ep = EP_VOL.borrow().as_ref().cloned();
        if let Some(ep) = ep {
            let _ = unsafe { ep.VolumeStepUp(std::ptr::null()) };
        }
        true
    } else if c == APPCOMMAND_VOLUME_DOWN.0 {
        let ep = EP_VOL.borrow().as_ref().cloned();
        if let Some(ep) = ep {
            let _ = unsafe { ep.VolumeStepDown(std::ptr::null()) };
        }
        true
    } else if c == APPCOMMAND_VOLUME_MUTE.0 {
        super::qs::qs_toggle_mute();
        true
    } else if c == APPCOMMAND_MEDIA_PREVIOUSTRACK.0 {
        if app_cmd_skip_allowed() {
            media_cmd(0, src);
        } else {
            super::core_c2::media_log(0, "appcommand (diblokir)");
        }
        true
    } else if c == APPCOMMAND_MEDIA_PLAY_PAUSE.0 {
        media_cmd(1, src);
        true
    } else if c == APPCOMMAND_MEDIA_NEXTTRACK.0 {
        if app_cmd_skip_allowed() {
            media_cmd(2, src);
        } else {
            super::core_c2::media_log(2, "appcommand (diblokir)");
        }
        true
    } else {
        false
    }
}

/// Hancurkan jendela bila ada lalu kosongkan handle-nya (pola C: `if (g_x) { DestroyWindow(g_x); g_x = nullptr; }`).
fn destroy_win(cell: &SyncCell<Cell<HWND>>) {
    let h = cell.get();
    if !h.0.is_null() {
        let _ = unsafe { DestroyWindow(h) };
        cell.set(HWND(std::ptr::null_mut()));
    }
}

/// C++ Shutdown. Urutan langkah sama dengan C. Dipanggil dari WM_DESTROY / akhir wWinMain.
pub fn shutdown_app() {
    QUITTING.set(true);
    kill_timer(T_START_TRIM);
    kb_hook_stop();
    kill_timer(T_CLOCK);
    kill_timer(T_STATUS);
    kill_timer(T_MEDIA);
    kill_timer(T_FGPOLL);
    kill_timer(T_QUIET);
    kill_timer(T_WALLPAPER);
    wall_watch_stop();
    kill_timer(T_FSSTART);
    fs_stop(); // hentikan thread indeks berkas sebelum jendela dilepas (w12filesearch.rs)
    kill_timer(T_SYSW);
    kill_timer(T_NL);
    kill_timer(T_NL_ICON);
    nl_api_close();
    sys_release();
    for cell in [&HK_WEV_FG, &HK_WEV_MIN, &HK_WEV_CLOAK, &HK_WEV_LOC, &HK_WEV_LIFE] {
        let h = cell.get();
        if !h.0.is_null() {
            let _ = unsafe { UnhookWinEvent(h) };
            cell.set(HWINEVENTHOOK(std::ptr::null_mut()));
        }
    }
    let msg = MSG.get();
    if !msg.0.is_null() {
        let _ = unsafe { DeregisterShellHookWindow(msg) };
    }
    app_bar_remove();
    if SHELL_MODE.get() {
        shell_mode_stop(); // win12boot menjalankan explorer.exe bila Win12 ditutup pengguna
    } else {
        restore_native_taskbar();
    }
    resume_native_start_processes();
    notch_destroy();
    for p in PILLS.borrow_mut().iter_mut() {
        if !p.h.0.is_null() {
            let _ = unsafe { DestroyWindow(p.h) };
            p.h = HWND(std::ptr::null_mut());
        }
    }
    if !PAN.get().0.is_null() {
        PANEL_OPEN.set(-1); // -1 dulu: WM_ACTIVATE tidak memanggil panel_close
    }
    destroy_win(&PAN);
    destroy_win(&GLOW);
    destroy_win(&TRAY_HOST); // aplikasi kembali ke tray Explorer
    destroy_win(&FLY);
    min_anim_stop();
    destroy_win(&ANIM_WND);
    loop {
        let n = TRAY_ICONS.borrow().len();
        if n == 0 {
            break;
        }
        tray_remove(&mut TRAY_ICONS.borrow_mut(), n - 1);
    }
    volume_detach();
    *DEV_ENUM.borrow_mut() = None;
    let nn = NET_NOTIFY.get();
    if !nn.0.is_null() {
        let _ = unsafe { CancelMibChangeNotify2(nn) };
        NET_NOTIFY.set(HANDLE(std::ptr::null_mut()));
    }
    media_clear(); // juga melepas event sesi per-sesi (media_unhook)
    if MGR_HOOKED.get() {
        if let Some(m) = MGR.borrow().as_ref() {
            let _ = m.RemoveCurrentSessionChanged(TOK_SESSION.get());
        }
        MGR_HOOKED.set(false);
    }
    *OP_MGR.borrow_mut() = None;
    *MGR.borrow_mut() = None;
}

// ---------------------------------------------------------------- MsgProc (C++ 4931-5080)

/// HSHELL_FLASH (0x8006) dan HSHELL_RUDEAPPACTIVATED (0x8004): tidak ada di crate windows 0.61.3,
/// jadi C++-nya didefinisikan sendiri (`#define`/literal). Di sini juga lokal.
const HSHELL_FLASH_W12: usize = 0x8006;
const HSHELL_RUDEAPPACTIVATED_W12: usize = 0x8004;

/// Struktur SHELLHOOKINFO milik C++ (wp == HSHELL_GETMINRECT): `hwnd` masukan, `rc` keluaran.
#[repr(C)]
pub struct ShellHookInfo {
    pub hwnd: HWND,
    pub rc: RECT,
}

/// Rect ikon dock untuk jendela `hwnd` (C++ HSHELL_GETMINRECT, 4938-4949).
/// Hanya item aplikasi di pill yang tampil. `NITEMS` membatasi loop seperti `g_nitems`.
fn min_rect_for(hwnd: HWND) -> Option<RECT> {
    let n = NITEMS.get().max(0) as usize;
    let items = ITEMS.borrow();
    let pills = PILLS.borrow();
    let apps = APPS.borrow();
    for it in items.iter().take(n) {
        if it.ty != IT_APP || it.pill < 0 || it.pill >= NBOTTOM {
            continue;
        }
        let Some(p) = pills.get(it.pill as usize) else {
            continue;
        };
        if !p.shown {
            continue;
        }
        let Some(app) = apps.get(it.app as usize) else {
            continue;
        };
        if !app.wins.contains(&hwnd) {
            continue;
        }
        return Some(RECT {
            left: p.x + it.x as i32,
            top: p.y,
            right: p.x + (it.x + it.w) as i32,
            bottom: p.y + p.ht,
        });
    }
    None
}

/// String wide NUL-terminated dari LPARAM (WM_SETTINGCHANGE). Kosong bila pointer null.
fn lparam_wstr(lp: LPARAM) -> String {
    let p = lp.0 as usize as *const u16;
    if p.is_null() {
        return String::new();
    }
    // SAFETY: lp untuk WM_SETTINGCHANGE adalah string wide NUL-terminated dari sistem.
    unsafe {
        let mut n = 0usize;
        while *p.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    }
}

/// WM_SHELLHOOK selain AppCommand (C++ 4935-4961). Selalu mengembalikan LRESULT.
fn shell_hook_msg(wp: WPARAM, lp: LPARAM) -> LRESULT {
    if wp.0 == 5 /* HSHELL_GETMINRECT */ && lp.0 != 0 {
        let info = lp.0 as usize as *mut ShellHookInfo;
        // SAFETY: lp adalah SHELLHOOKINFO* dari sistem untuk HSHELL_GETMINRECT.
        let hwnd = unsafe { (*info).hwnd };
        return match min_rect_for(hwnd) {
            Some(rc) => {
                unsafe { (*info).rc = rc };
                LRESULT(1)
            }
            None => LRESULT(0),
        };
    }
    if wp.0 == HSHELL_FLASH_W12 {
        let target = lp.0;
        let mut flashed = false;
        for app in APPS.borrow_mut().iter_mut() {
            if !app.fg && app.wins.iter().any(|w| w.0 as isize == target) {
                app.flash = true;
                flashed = true;
            }
        }
        // C++ memanggil InvalidatePills per kecocokan; hasilnya sama dengan sekali di sini.
        // Dipanggil setelah borrow_mut APPS dilepas agar tidak terjadi BorrowMutError.
        if flashed {
            invalidate_pills(true, false);
        }
    } else if [
        HSHELL_WINDOWCREATED as usize,
        HSHELL_WINDOWDESTROYED as usize,
        HSHELL_WINDOWACTIVATED as usize,
        HSHELL_RUDEAPPACTIVATED_W12,
        HSHELL_WINDOWREPLACED as usize, // 13 (C++ literal)
        14,                             // HSHELL_WINDOWREPLACING (C++ literal)
        16,                             // HSHELL_MONITORCHANGED (C++ literal)
    ]
    .contains(&wp.0)
    {
        schedule_refresh();
    }
    LRESULT(0)
}

/// Window procedure utama shell (C++ MsgProc 4931-5080). Pesan terdaftar (WM_SHELLHOOK,
/// WM_TASKBARCREATED) dicek sebelum `match`, seperti di C, karena nilainya dinamis.
pub unsafe extern "system" fn msg_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let shell_msg = WM_SHELLHOOK.get();
    if shell_msg != 0 && m == shell_msg {
        if SHELL_MODE.get()
            && wp.0 == HSHELL_APPCOMMAND as usize
            && shell_app_command(get_appcommand_lparam(lp), get_device_lparam(lp))
        {
            return LRESULT(1);
        }
        return shell_hook_msg(wp, lp);
    }

    let tbc_msg = WM_TASKBARCREATED.get();
    if tbc_msg != 0 && m == tbc_msg {
        // explorer dimulai ulang (C++ 4962-4974)
        let old = TRAY_WND.get();
        TRAY_WND.set(HWND(std::ptr::null_mut()));
        if !old.0.is_null() && tray_wnd() == old {
            // siaran kita sendiri (sinkron ikon tray): Explorer tidak berganti
            return LRESULT(0);
        }
        AB_REG.set(false);
        TRAY_PID.set(0);
        let host = TRAY_HOST.get();
        if !host.0.is_null() {
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
        }
        hide_native_taskbar();
        app_bar_register();
        let _ = unsafe { RegisterShellHookWindow(MSG.get()) };
        DESK_Z_DIRTY.set(true); // desktop baru: widget disisipkan ulang tepat di atasnya
        relayout();
        return LRESULT(0);
    }

    match m {
        WM_APPBAR => {
            if wp.0 == ABN_POSCHANGED as usize {
                app_bar_set_pos();
            }
            return LRESULT(0);
        }
        WM_W12_APPIDX => {
            // indeks aplikasi Start selesai dibangun (start.rs)
            st_idx_install(lp);
            return LRESULT(0);
        }
        WM_W12_FXHITS => {
            // hasil pencarian berkas; wp = generasi, lp = FsResult* (filesearch.rs)
            if unsafe { fs_accept_hits(wp.0 as i32, lp.0 as usize as *mut FsResult) } {
                ST_HITS_Q.borrow_mut().clear(); // paksa st_search menghitung ulang
                st_search();
                if PANEL_OPEN.get() == PN_START
                    && (ST_Q_LEN.get() > 0 || ST_ALL_APPS_MODE.get())
                {
                    invalidate_pan();
                }
            }
            return LRESULT(0);
        }
        WM_W12_NLRESULT => {
            unsafe { nl_api_accept_result(wp, lp) };
            if PANEL_OPEN.get() == PN_START {
                invalidate_pan();
            }
            return LRESULT(0);
        }
        WM_W12_RADIO => {
            // status radio Wi-Fi/Bluetooth (qs.rs)
            qs_radio_notify(wp, lp);
            return LRESULT(0);
        }
        WM_W12_WLAN => {
            // daftar jaringan Wi-Fi (flyout.rs)
            wifi_wlan_notify(lp.0);
            return LRESULT(0);
        }
        WM_W12_MEDIAEV => {
            // sesi/status/judul media berubah
            media_event();
            return LRESULT(0);
        }
        WM_TIMER => {
            on_timer(wp.0 as i32);
            return LRESULT(0);
        }
        WM_VOLCHG => {
            let (pv, pm) = (VOL.get(), MUTE.get());
            MUTE.set(wp.0 != 0);
            VOL.set(lp.0 as i32);
            invalidate_pills(true, false);
            if PANEL_OPEN.get() >= 0 {
                invalidate_pan();
            }
            notch_volume(pv, pm); // HUD volume di notch (tambahan OSD Windows)
            return LRESULT(0);
        }
        super::cliphist::WM_CLIPBOARDUPDATE => {
            // Riwayat clipboard (Win+V): simpan teks yang baru disalin (cliphist.rs)
            super::cliphist::ch_on_update();
            return LRESULT(0);
        }
        WM_HOTKEY => {
            // Win+Alt+N: notch; selain itu pintasan mode shell (hotkeys.rs)
            if wp.0 == NT_HOTKEY as usize {
                notch_hotkey();
            } else {
                hotkey_run(wp.0 as i32);
            }
            return LRESULT(0);
        }
        WM_SYSCOMMAND => {
            // SC_TASKLIST: tombol Windows yang tidak terlihat kait (jendela admin fokus)
            if SHELL_MODE.get() && ((wp.0 as u32) & 0xFFF0) == SC_TASKLIST {
                hk_task_list();
                return LRESULT(0);
            }
        }
        WM_W12_NEEDEXP => {
            // DEVIASI: C++ mengirim wchar_t*; Rust mengirim Box<String> mentah (hotkeys.rs:164-169).
            // Penerima WAJIB mengambil alih kepemilikan dengan Box::from_raw.
            if lp.0 != 0 {
                let s: String = *unsafe { Box::from_raw(lp.0 as usize as *mut String) };
                hk_need_explorer(s);
            }
            return LRESULT(0);
        }
        WM_NETCHG => {
            let was = NET.get();
            if net_update() {
                if (was == NET_BOTH) != (NET.get() == NET_BOTH) {
                    relayout();
                } else {
                    invalidate_pills(true, false);
                }
                if PANEL_OPEN.get() >= 0 {
                    invalidate_pan();
                }
            }
            return LRESULT(0);
        }
        WM_POWERBROADCAST => {
            let ev = wp.0 as u32;
            if ev == PBT_APMRESUMEAUTOMATIC || ev == PBT_APMRESUMESUSPEND {
                hide_native_taskbar();
                deactivate_native_start_processes();
                relayout();
                app_bar_set_pos();
                update_foreground(true);
            } else if ev == PBT_APMPOWERSTATUSCHANGE {
                let had = HAS_BAT.get();
                if battery_update() {
                    if had != HAS_BAT.get() {
                        relayout();
                    } else {
                        invalidate_pills(true, false);
                    }
                }
            } else if ev == PBT_POWERSETTINGCHANGE {
                // kecerahan layar berubah: HUD di notch
                notch_power(lp);
            }
            return LRESULT(1);
        }
        WM_DEVICECHANGE => {
            // volume USB tiba/dilepas (broadcast ke jendela top-level)
            let ev = wp.0 as u32;
            if ev != DBT_DEVICEARRIVAL && ev != DBT_DEVICEREMOVECOMPLETE {
                return LRESULT(1);
            }
            if lp.0 == 0 {
                return LRESULT(1);
            }
            // SAFETY: lp untuk DBT_DEVICEARRIVAL/REMOVECOMPLETE adalah DEV_BROADCAST_HDR* dari sistem.
            let devtype = unsafe { (*(lp.0 as usize as *const DEV_BROADCAST_HDR)).dbch_devicetype };
            if devtype.0 != DBT_DEVTYP_VOLUME.0 {
                return LRESULT(1);
            }
            let had = USB_COUNT.get() > 0;
            usb_refresh(ev == DBT_DEVICEARRIVAL);
            if had != (USB_COUNT.get() > 0) {
                relayout();
            } else {
                invalidate_pills(true, false);
            }
            return LRESULT(1);
        }
        WM_DISPLAYCHANGE => {
            relayout();
            app_bar_set_pos();
            if MICA_FULLSCREEN_SNAPSHOT.get() {
                mica_restore_wallpaper();
            } else {
                mica_reload_wallpaper_if_changed(true, false);
            }
            update_foreground(true);
            base_update(true);
            notch_invalidate();
            return LRESULT(0);
        }
        WM_SETTINGCHANGE => {
            // Personalization tidak selalu mengirim SPI_SETDESKWALLPAPER
            let s = lparam_wstr(lp);
            if wp.0 == SPI_SETDESKWALLPAPER.0 as usize || s == "Control Panel\\Desktop" {
                mica_reload_wallpaper_if_changed(true, true);
            }
            if s == "ImmersiveColorSet" {
                if THEME_PREF.get() == 0 {
                    load_theme(); // tema gelap/terang ikut Windows
                } else {
                    load_accent_colors(); // aksen tetap mengikuti Windows walau tema dikunci
                }
                GLOW_DIRTY.set(true);
                invalidate_pills(true, true);
                base_update(true);
                notch_invalidate();
            }
            return LRESULT(0);
        }
        WM_DWMCOLORIZATIONCOLORCHANGED => {
            load_accent_colors();
            GLOW_DIRTY.set(true);
            invalidate_pills(true, true);
            if PANEL_OPEN.get() >= 0 {
                invalidate_pan();
            }
            base_update(true);
            notch_invalidate();
            return LRESULT(0);
        }
        WM_PEEK => {
            PEEK.set(true);
            apply_pills(true);
            set_timer(T_PEEK, 5000);
            return LRESULT(0);
        }
        WM_WINKEY => {
            WIN_KEY_TICK.set(unsafe { GetTickCount() });
            start_from_key(wp.0 as i32);
            return LRESULT(0);
        }
        WM_W12_SHELLKEY => {
            shell_key(wp.0 as i32);
            return LRESULT(0);
        }
        WM_W12_DIAG => {
            diag_start(wp.0 as i32, lp.0 as i32);
            return LRESULT(0);
        }
        WM_W12_WALLCHG => {
            // pasang ulang dulu, lalu muat ulang
            wall_watch_arm();
            mica_reload_wallpaper_if_changed(false, true);
            return LRESULT(0);
        }
        WM_SETMODE => {
            let mode = wp.0 as i32;
            if (MODE_SEGMENTED..=MODE_CLASSIC).contains(&mode) && mode != MODE.get() {
                set_mode(mode);
            }
            return LRESULT(0);
        }
        WM_QUERYENDSESSION => return LRESULT(1),
        WM_ENDSESSION => {
            if wp.0 != 0 {
                shutdown_app();
            }
            return LRESULT(0);
        }
        WM_CLOSE => {
            let _ = unsafe { DestroyWindow(h) };
            return LRESULT(0);
        }
        WM_DESTROY => {
            shutdown_app();
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

// ---------------------------------------------------------------------------
// Crash filter dan helper kelas jendela (port CrashFilter, C++ 5083-5106, dan
// pendaftaran kelas di wWinMain, C++ 5179-5202).
// ---------------------------------------------------------------------------

/// Port `CrashFilter` (C++ 5083-5106): tulis crash.log (append, UTF-8), hentikan proses start
/// native, lalu tampilkan lagi taskbar native agar shell tidak hilang saat crash.
pub unsafe extern "system" fn crash_filter(ep: *const EXCEPTION_POINTERS) -> i32 {
    use std::io::Write as _;
    // Kode dan alamat exception (C++ 5089-5092).
    let (has_rec, code, addr) = if !ep.is_null() && !(*ep).ExceptionRecord.is_null() {
        let rec = (*ep).ExceptionRecord;
        (true, (*rec).ExceptionCode.0 as u32, (*rec).ExceptionAddress as usize)
    } else {
        (false, 0u32, 0usize)
    };
    let base = GetModuleHandleW(PCWSTR::null()).map(|h| h.0 as usize).unwrap_or(0);
    let rva = if has_rec { addr.wrapping_sub(base) } else { 0 };
    // PORT: napps/media memakai -1 bila RefCell sedang dipinjam (C++ membaca g_napps/g_media langsung).
    let napps: i64 = APPS.try_borrow().map(|a| a.len() as i64).unwrap_or(-1);
    let media: i64 = MEDIA.try_borrow().map(|m| if m.has { 1 } else { 0 }).unwrap_or(-1);
    let nitems = NITEMS.get();
    let dir = DIR.try_borrow().map(|d| String::clone(&d)).unwrap_or_default();
    let st = GetLocalTime();
    let line = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} shell crash code=0x{:08X} rva=0x{:X} napps={} nitems={} media={}\r\n",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond,
        code, rva, napps, nitems, media
    );
    // Sama dengan C++: path = g_dir + "\\crash.log", OPEN_ALWAYS + FILE_APPEND_DATA.
    let path = format!("{}\\crash.log", dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
    crash_recover();
    EXCEPTION_EXECUTE_HANDLER
}

/// Pemulihan bersama untuk CrashFilter dan panic hook: hentikan proses start, kembalikan tray dan jendela Explorer.
unsafe fn crash_recover() {
    resume_native_start_processes();
    let th = TRAY_HOST.get();
    if !th.0.is_null() {
        let _ = DestroyWindow(th);
    }
    let tray = tray_wnd();
    if !tray.0.is_null() {
        let _ = EnableWindow(tray, true);
        let _ = ShowWindowAsync(tray, SW_SHOWNA);
    }
    let mut after: Option<HWND> = None;
    while let Ok(s) = FindWindowExW(None, after, w!("Shell_SecondaryTrayWnd"), PCWSTR::null()) {
        if s.0.is_null() {
            break;
        }
        let _ = EnableWindow(s, true);
        let _ = ShowWindowAsync(s, SW_SHOWNA);
        after = Some(s);
    }
}

/// Prosedur jendela default untuk kelas glow (C++ 5198-5199).
pub unsafe extern "system" fn glow_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(h, m, wp, lp)
}

/// Daftarkan kelas jendela (pola `pill_class_register`). Gagal diam-diam seperti C++ (`RegisterClassExW` tanpa cek).
pub fn reg_class(name: PCWSTR, proc_: WNDPROC, style: WNDCLASS_STYLES, cursor: bool) {
    let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.style = style;
    wc.lpfnWndProc = proc_;
    wc.hInstance = HI.get();
    wc.lpszClassName = name;
    if cursor {
        if let Ok(c) = unsafe { LoadCursorW(None, IDC_ARROW) } {
            wc.hCursor = c;
        }
    }
    let _ = unsafe { RegisterClassExW(&wc) };
}

/// Jendela pesan Win12Shell yang sudah berjalan (C++ `FindWindowW(L"Win12ShellMsg", nullptr)`); null bila tidak ada.
pub fn find_msg_window() -> HWND {
    unsafe { FindWindowW(w!("Win12ShellMsg"), PCWSTR::null()) }.unwrap_or(HWND(std::ptr::null_mut()))
}

/// HWND dari hasil pembuatan jendela; null bila gagal (C++ hanya memeriksa nilai null).
fn hwnd_or_null(r: windows::core::Result<HWND>) -> HWND {
    r.unwrap_or(HWND(std::ptr::null_mut()))
}

// ---------------------------------------------------------------------------
// wWinMain (port, C++ 5108-5284).
// ---------------------------------------------------------------------------

/// Titik masuk shell (port `wWinMain`). Kembalian = kode keluar proses:
/// 0 = selesai / perintah diteruskan ke instans lain; 1 = gagal inisialisasi / shell tidak berjalan (diag);
/// 2 = argumen --diag-cycle tidak valid.
pub fn w_win_main() -> i32 {
    // C++ 5110-5111: g_hi dan filter crash.
    let hi = unsafe { GetModuleHandleW(PCWSTR::null()) }
        .map(|h| HINSTANCE(h.0))
        .unwrap_or(HINSTANCE(std::ptr::null_mut()));
    HI.set(hi);
    let _ = unsafe { SetUnhandledExceptionFilter(Some(crash_filter)) };
    // Panic Rust tidak lewat UnhandledExceptionFilter (mis. panic di thread utama). Catat baris "shell panic"
    // ke crash.log dengan format yang sama, lalu pulihkan shell seperti CrashFilter. Hook bawaan tetap dipanggil.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write as _;
        let dir = DIR.try_borrow().map(|d| String::clone(&d)).unwrap_or_default();
        // SAFETY: GetLocalTime tidak punya prasyarat selain berjalan di thread proses ini.
        let st = unsafe { GetLocalTime() };
        let msg = format!("{info}").replace('\r', "").replace('\n', " | ");
        let line = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} shell panic {}\r\n",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond, msg
        );
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(format!("{}\\crash.log", dir)) {
            let _ = f.write_all(line.as_bytes());
        }
        // SAFETY: pemulihan memakai handle jendela milik proses ini; dipanggil dari hook panic.
        unsafe { crash_recover() };
        prev_hook(info);
    }));

    // PORT: C++ memakai lpCmdLine. Di sini argumen digabung spasi; tanda kutip asli tidak dipertahankan.
    let cmd_str: String = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<String>>()
        .join(" ");
    let cmd_w = to_wide_z(&cmd_str);
    let has = |flag: &str| cmd_str.contains(flag);

    // C++ 5112-5117: --mode segmented|dock|classic mengganti mode pada instans yang berjalan.
    let mut cmd_mode: i32 = -1;
    if has("--mode") {
        cmd_mode = if has("dock") {
            MODE_DOCK
        } else if has("classic") {
            MODE_CLASSIC
        } else {
            MODE_SEGMENTED
        };
        let w = find_msg_window();
        if !w.0.is_null() {
            let _ = unsafe { PostMessageW(Some(w), WM_SETMODE, WPARAM(cmd_mode as usize), LPARAM(0)) };
            return 0;
        }
    }
    // C++ 5118-5122: --peek.
    if has("--peek") {
        let w = find_msg_window();
        if !w.0.is_null() {
            let _ = unsafe { PostMessageW(Some(w), WM_PEEK, WPARAM(0), LPARAM(0)) };
        }
        return 0;
    }
    // C++ 5123-5130: --exit menunggu hingga jendela pesan tertutup (maks. 50 x 100 ms).
    if has("--exit") {
        let w = find_msg_window();
        if !w.0.is_null() {
            let _ = unsafe { PostMessageW(Some(w), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            for _ in 0..50 {
                if !unsafe { IsWindow(Some(w)) }.as_bool() {
                    break;
                }
                unsafe { Sleep(100) };
            }
        }
        return 0;
    }
    // C++ 5133-5143: --diag-cycle. Kode keluar: 0 terkirim; 1 shell tidak berjalan; 2 argumen tidak valid.
    {
        let (mut dc, mut dn) = (0i32, 0i32);
        let parsed = w12_diag_parse_cmd(Some(&cmd_w[..]), &mut dc, &mut dn);
        if parsed < 0 {
            return 2;
        }
        if parsed > 0 {
            let w = find_msg_window();
            if w.0.is_null() {
                return 1;
            }
            let _ = unsafe {
                PostMessageW(Some(w), WM_W12_DIAG, WPARAM(dc as isize as usize), LPARAM(dn as isize))
            };
            return 0;
        }
    }
    // C++ 5144-5145: instans tunggal.
    let mtx = unsafe { CreateMutexW(None, false, w!("Win12ShellSingleInstance")) }.ok();
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return 0;
    }
    // C++ 5147: --shell hanya bila Progman (Explorer) tidak berjalan.
    let progman_absent =
        unsafe { FindWindowW(w!("Progman"), PCWSTR::null()) }.map_or(true, |h| h.0.is_null());
    let shell_mode = has("--shell") && progman_absent;
    SHELL_MODE.set(shell_mode);
    if has("--shell") && !progman_absent {
        // PORT: log tambahan (2026-11-14) agar boot.log menunjukkan sebab mode shell tidak aktif.
        log("--shell diabaikan: Progman (Explorer) masih berjalan; berjalan tanpa mode shell");
    }

    // C++ 5149-5154: DPI dan folder program.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let dpi = unsafe { GetDpiForSystem() } as i32;
    DPI.set(dpi);
    // PORT: C++ memakai GetModuleFileNameW + PathRemoveFileSpecW; di sini current_exe().parent().
    let dir: String = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
        .unwrap_or_default();
    *INI.borrow_mut() = format!("{}\\settings.ini", dir);
    *APPS_TXT.borrow_mut() = format!("{}\\apps.txt", dir);
    *DIR.borrow_mut() = dir;

    // C++ 5156-5159: WinRT, AUMID (GetApplicationUserModelId dari kernel32).
    let _ = unsafe { RoInitialize(RO_INIT_SINGLETHREADED) };
    if let Ok(k32) = unsafe { GetModuleHandleW(w!("kernel32.dll")) } {
        let f = unsafe {
            GetProcAddress(k32, PCSTR(b"GetApplicationUserModelId\0".as_ptr()))
        };
        let pfn = unsafe { std::mem::transmute::<FARPROC, Option<PfnGetAppUserModelId>>(f) };
        PFN_GET_APP_USER_MODEL_ID.set(pfn);
    }
    // PORT: C++ memuat CreateStreamOverRandomAccessStream lewat LoadLibrary (shcore.dll). Rust mengimpornya
    // langsung (core_c1.rs:109), jadi tidak ada pointer fungsi yang perlu diisi di sini.

    // C++ 5160-5164: kontrol umum dan faktori D2D/DirectWrite/WIC.
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_WIN95_CLASSES,
    };
    let _ = unsafe { InitCommonControlsEx(&icc) };
    *D2D.borrow_mut() =
        unsafe { D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }.ok();
    *DW.borrow_mut() = unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) }.ok();
    *WIC.borrow_mut() = unsafe {
        CoCreateInstance::<_, IWICImagingFactory>(&CLSID_WICImagingFactory, None::<&IUnknown>, CLSCTX_INPROC_SERVER)
    }
    .ok();
    // C++ 5165: tanpa faktori atau target, shell berhenti dengan kode 1.
    let have = D2D.borrow().is_some() && DW.borrow().is_some() && WIC.borrow().is_some();
    if !have || !create_target() {
        return 1;
    }

    // C++ 5166-5171: pengaturan, tema, dan format.
    load_settings();
    if shell_mode {
        NOTCH_HUD.set(true);
    }
    if cmd_mode >= 0 {
        MODE.set(cmd_mode);
        save_settings();
    }
    widgets_load();
    load_theme();
    create_formats();
    WM_SHELLHOOK.set(unsafe { RegisterWindowMessageW(w!("SHELLHOOK")) });
    WM_TASKBARCREATED.set(unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) });

    // C++ 5175-5204: kelas jendela. Kursor mengikuti C++: kelas Msg tanpa kursor (wc = {}), kelas lain dengan panah.
    // PORT: kelas "Win12ShellPill" didaftar di create_pill (idempoten), bukan di sini.
    reg_class(w!("Win12ShellMsg"), Some(msg_proc), WNDCLASS_STYLES(0), false);
    reg_class(w!("Win12ShellFlyout"), Some(flyout_proc), CS_DBLCLKS, true);
    reg_class(w!("Win12ShellAnim"), Some(anim_wnd_proc), WNDCLASS_STYLES(0), true);
    reg_class(w!("Shell_TrayWnd"), Some(tray_host_proc), WNDCLASS_STYLES(0), true);
    reg_class(w!("Win12ShellGlow"), Some(glow_proc), WNDCLASS_STYLES(0), true);
    reg_class(w!("Win12ShellPanel"), Some(panel_proc), WNDCLASS_STYLES(0), true);

    // C++ 5205-5209: jendela pesan dan filter pesan (agar perintah dari proses biasa tetap diterima).
    let msg_hwnd = match unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("Win12ShellMsg"),
            w!("Win12 Shell"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hi),
            None,
        )
    } {
        Ok(h) => h,
        Err(_) => return 1,
    };
    MSG.set(msg_hwnd);
    let _ = unsafe { ChangeWindowMessageFilterEx(msg_hwnd, WM_CLOSE, MSGFLT_ALLOW, None) };
    let _ = unsafe { ChangeWindowMessageFilterEx(msg_hwnd, WM_SETMODE, MSGFLT_ALLOW, None) };
    let _ = unsafe { ChangeWindowMessageFilterEx(msg_hwnd, WM_PEEK, MSGFLT_ALLOW, None) };

    // C++ 5211-5218: tooltip pelacak (TTF_TRACK) dan lebar maksimum 360 px.
    let tip = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            TOOLTIPS_CLASSW,
            PCWSTR::null(),
            WINDOW_STYLE(WS_POPUP.0 | TTS_NOPREFIX | TTS_ALWAYSTIP),
            0,
            0,
            0,
            0,
            Some(msg_hwnd),
            None,
            Some(hi),
            None,
        )
    }
    .ok();
    if let Some(tip_hwnd) = tip {
        TIP.set(tip_hwnd);
        let mut empty = [0u16; 1];
        let ti = TTTOOLINFOW {
            cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_TRACK | TTF_ABSOLUTE,
            hwnd: msg_hwnd,
            uId: 1,
            lpszText: PWSTR(empty.as_mut_ptr()),
            ..Default::default()
        };
        unsafe {
            SendMessageW(tip_hwnd, TTM_ADDTOOLW, None, Some(LPARAM(&ti as *const TTTOOLINFOW as isize)));
            SendMessageW(tip_hwnd, TTM_SETMAXTIPWIDTH, None, Some(LPARAM(s(360.0) as isize)));
        }
    }

    // C++ 5219-5225: pil. Penugasan PILLS[i].h dilakukan di sini (bukan di create_pill), seperti C++.
    pills_ensure();
    let set_pill = |idx: i32| {
        let h = create_pill(idx);
        PILLS.borrow_mut()[idx as usize].h = h;
    };
    for i in 0..NBOTTOM {
        set_pill(i);
    }
    if !NOTCH_ON.get() {
        set_pill(P_CLOCK);
        set_pill(P_MEDIA);
    }
    set_pill(P_SYS);
    set_pill(P_CAL);

    // C++ 5226-5227: jendela glow (cahaya di belakang dok).
    GLOW.set(hwnd_or_null(unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            w!("Win12ShellGlow"),
            w!(""),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(hi),
            None,
        )
    }));

    // C++ 5229-5237: ikon catatan, aplikasi, jam, jaringan, baterai, USB, volume, menu.
    NOTE_ICON.borrow_mut().src = icon_from_parsing_name(NOTE_APP, icon_size());
    load_apps();
    clock_update();
    net_init();
    net_update();
    battery_update();
    usb_refresh(false);
    volume_attach();
    menu_icons();

    // C++ 5239-5245: shell hook dan hook event jendela (WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS = 0 | 2).
    let _ = unsafe { RegisterShellHookWindow(msg_hwnd) };
    let fl: u32 = 0x0002;
    let hook = |lo: u32, hi_ev: u32| unsafe {
        SetWinEventHook(lo, hi_ev, None, Some(win_event_proc), 0, 0, fl)
    };
    HK_WEV_FG.set(hook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND));
    HK_WEV_MIN.set(hook(22, 23)); // EVENT_SYSTEM_MINIMIZESTART..MINIMIZEEND
    HK_WEV_CLOAK.set(hook(0x8017, 0x8018)); // EVENT_OBJECT_CLOAKED..UNCLOAKED
    HK_WEV_LOC.set(hook(EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZEEND));
    HK_WEV_LIFE.set(hook(0x8001, 0x8002)); // EVENT_OBJECT_DESTROY..SHOW
    kb_hook_start();
    set_timer(T_START_TRIM, 5000);

    // C++ 5247-5259: mode shell atau taskbar native, dok, flyout, jendela animasi, dan tampilan awal.
    if shell_mode {
        shell_mode_start();
    } else {
        hide_native_taskbar();
    }
    app_bar_register();
    FLY.set(create_flyout());
    ANIM_WND.set(hwnd_or_null(unsafe {
        CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            w!("Win12ShellAnim"),
            w!(""),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(hi),
            None,
        )
    }));
    tray_host_create();
    refresh_windows();
    relayout();
    update_foreground(true);
    notch_create();
    arm_clock();

    // C++ 5260-5267: timer.
    set_timer(T_STATUS, 10000);
    set_timer(T_MEDIA, 3000);
    media_tick();
    set_timer(T_TRIM, 4000);
    set_timer(T_FGPOLL, 500);
    set_timer(T_WALLPAPER, 15000);
    wall_watch_arm();
    set_timer(T_FSSTART, 60000);

    // C++ 5269-5273: loop pesan.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // C++ 5274-5282: lepas mutex; bila "Beralih ke Windows biasa", jalankan explorer.exe.
    if let Some(m) = mtx {
        let _ = unsafe { CloseHandle(m) };
    }
    if EXPLORER_ON_EXIT.get() && unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) } == 0 {
        let mut buf = [0u16; 260];
        let n = unsafe { GetWindowsDirectoryW(Some(&mut buf[..])) } as usize;
        let mut path = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
        path.push_str("\\explorer.exe");
        let _ = unsafe {
            ShellExecuteW(None, w!("open"), pcw(&to_wide_z(&path)), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL)
        };
    }
    0
}

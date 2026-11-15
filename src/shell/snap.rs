//! Sumber: w12snap.inc:1-148 (Win+panah snap dan menu tata letak Win+Z, hanya mode shell).
//! Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Aturan keadaan dan rect ada di tasks_logic.rs (w12_snap_next, w12_snap_rect).

use super::prelude::*;
use std::ffi::c_void;
use std::mem::size_of;

// Dwm tidak diekspor prelude; harus di-use eksplisit.
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};

/// Jumlah catatan jendela maksimum (`W12SNAP_MAX_SLOTS`).
const SNAP_MAX_SLOTS: usize = 32;

/// Status per jendela (`W12SnapSlot`): keadaan terakhir yang diterapkan, dan rect pulih yang dicatat
/// saat jendela keluar dari RESTORED.
#[derive(Clone, Copy)]
struct SnapSlot {
    hwnd: HWND,
    state: i32,
    has_restore: bool,
    restore: RECT,
}

static SNAP_SLOTS: SyncCell<RefCell<Vec<SnapSlot>>> = SyncCell::new(RefCell::new(Vec::new()));

/// Item menu tata letak (`kItems` di SnapLayoutMenu). Id 0 adalah pemisah.
const SNAP_MENU: &[(i32, &str)] = &[
    (SNL_LEFT, "&1  Setengah kiri"),
    (SNL_RIGHT, "&2  Setengah kanan"),
    (SNL_TOP, "&3  Setengah atas"),
    (SNL_BOTTOM, "&4  Setengah bawah"),
    (0, ""),
    (SNL_TL, "&5  Kiri atas"),
    (SNL_TR, "&6  Kanan atas"),
    (SNL_BL, "&7  Kiri bawah"),
    (SNL_BR, "&8  Kanan bawah"),
    (0, ""),
    (SNL_L3, "&Q  Sepertiga kiri"),
    (SNL_C3, "&W  Sepertiga tengah"),
    (SNL_R3, "&E  Sepertiga kanan"),
    (SNL_L23, "&A  Dua pertiga kiri"),
    (SNL_R23, "&S  Dua pertiga kanan"),
    (0, ""),
    (SNL_MAX, "&M  Maksimalkan"),
];

/// Indeks catatan `h`. Bila belum ada, buat catatan baru (RESTORED). Bila penuh, buang yang paling lama.
fn slot_index(slots: &mut Vec<SnapSlot>, h: HWND) -> usize {
    if let Some(i) = slots.iter().position(|sl| sl.hwnd == h) {
        return i;
    }
    if slots.len() == SNAP_MAX_SLOTS {
        slots.remove(0);
    }
    slots.push(SnapSlot {
        hwnd: h,
        state: W12SN_RESTORED,
        has_restore: false,
        restore: RECT::default(),
    });
    slots.len() - 1
}

/// Baca atau ubah catatan `h`. Borrow hanya dipegang di dalam fungsi ini, tidak melewati panggilan Win32.
fn snap_with<R>(h: HWND, f: impl FnOnce(&mut SnapSlot) -> R) -> R {
    let mut slots = SNAP_SLOTS.borrow_mut();
    let i = slot_index(&mut slots, h);
    f(&mut slots[i])
}

/// Buang catatan jendela yang sudah tidak ada, supaya catatan lama tidak dipakai HWND baru yang sama (`SnapPrune`).
fn snap_prune() {
    SNAP_SLOTS
        .borrow_mut()
        .retain(|sl| unsafe { IsWindow(Some(sl.hwnd)) }.as_bool());
}

fn is_iconic(w: HWND) -> bool {
    unsafe { IsIconic(w) }.as_bool()
}

fn is_zoomed(w: HWND) -> bool {
    unsafe { IsZoomed(w) }.as_bool()
}

/// Area kerja monitor `w`, `GetWindowRect`, dan bingkai tak terlihat (`DWMWA_EXTENDED_FRAME_BOUNDS`).
/// Mengembalikan `None` bila info monitor tidak terbaca. Bila atribut DWM gagal, bingkai = `GetWindowRect`.
fn monitor_and_frame(w: HWND) -> Option<(RECT, RECT, RECT)> {
    let mut mi = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let hm = unsafe { MonitorFromWindow(w, MONITOR_DEFAULTTONEAREST) };
    if !unsafe { GetMonitorInfoW(hm, &mut mi) }.as_bool() {
        return None;
    }
    let mut wr = RECT::default();
    let mut fr = RECT::default();
    unsafe {
        let _ = GetWindowRect(w, &mut wr);
        if DwmGetWindowAttribute(
            w,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut fr as *mut RECT as *mut c_void,
            size_of::<RECT>() as u32,
        )
        .is_err()
        {
            fr = wr;
        }
    }
    Some((mi.rcWork, wr, fr))
}

/// LEFT/RIGHT: area kerja monitor jendela, dikoreksi bingkai tak terlihat (`SnapPlace`).
fn snap_place(w: HWND, state: i32) {
    let (work, wr, fr) = match monitor_and_frame(w) {
        Some(v) => v,
        None => return,
    };
    let wk = W12RectI {
        l: work.left,
        t: work.top,
        r: work.right,
        b: work.bottom,
    };
    let inset = W12RectI {
        l: fr.left - wr.left,
        t: fr.top - wr.top,
        r: wr.right - fr.right,
        b: wr.bottom - fr.bottom,
    };
    let rc = w12_snap_rect(wk, state, inset);
    unsafe {
        let _ = SetWindowPos(
            w,
            None,
            rc.l,
            rc.t,
            rc.r - rc.l,
            rc.b - rc.t,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Win+panah (mode shell): ← → ↑ ↓ mengubah keadaan jendela depan menurut tabel `w12_snap_next` (`SnapKey`).
pub fn snap_key(vk: i32) {
    let w = unsafe { GetForegroundWindow() };
    if w.0.is_null() || !is_task_window(w) {
        return;
    }
    let key = if vk == VK_LEFT.0 as i32 {
        W12SK_LEFT
    } else if vk == VK_RIGHT.0 as i32 {
        W12SK_RIGHT
    } else if vk == VK_UP.0 as i32 {
        W12SK_UP
    } else {
        W12SK_DOWN
    };
    snap_prune();
    let saved_state = snap_with(w, |sl| sl.state);
    // Keadaan nyata didahulukan: maksimal/minimal yang berasal dari luar (judul, taskbar) ikut terbaca.
    let mut cur = saved_state;
    if is_iconic(w) {
        cur = W12SN_MIN;
    } else if is_zoomed(w) {
        cur = W12SN_MAX;
    } else if cur == W12SN_MAX || cur == W12SN_MIN {
        cur = W12SN_RESTORED;
    }
    let next = w12_snap_next(cur, key);
    if cur == W12SN_RESTORED && next != W12SN_RESTORED {
        // catat rect pulih sebelum meninggalkan keadaan biasa
        let mut rc = RECT::default();
        unsafe {
            let _ = GetWindowRect(w, &mut rc);
        }
        snap_with(w, |sl| {
            sl.restore = rc;
            sl.has_restore = true;
        });
    }
    if next == W12SN_MAX {
        unsafe {
            let _ = ShowWindow(w, SW_MAXIMIZE);
        }
    } else if next == W12SN_MIN {
        unsafe {
            let _ = ShowWindow(w, SW_MINIMIZE);
        }
    } else {
        if is_iconic(w) || is_zoomed(w) {
            unsafe {
                let _ = ShowWindow(w, SW_RESTORE);
            }
        }
        if next == W12SN_RESTORED {
            let (had_restore, r) = snap_with(w, |sl| (sl.has_restore, sl.restore));
            if had_restore {
                unsafe {
                    let _ = SetWindowPos(
                        w,
                        None,
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                snap_with(w, |sl| sl.has_restore = false);
            }
        } else {
            snap_place(w, next);
        }
    }
    snap_with(w, |sl| sl.state = next);
}

/// Tata letak snap (`SnapLayout`). Fraksi per enam area kerja, urutan sama dengan SNL_LEFT..SNL_R23.
pub fn snap_layout(w: HWND, layout: i32) {
    const K: [[i32; 4]; 13] = [
        [0, 0, 3, 6],
        [3, 0, 6, 6],
        [0, 0, 6, 3],
        [0, 3, 6, 6],
        [0, 0, 3, 3],
        [3, 0, 6, 3],
        [0, 3, 3, 6],
        [3, 3, 6, 6],
        [0, 0, 2, 6],
        [2, 0, 4, 6],
        [4, 0, 6, 6],
        [0, 0, 4, 6],
        [2, 0, 6, 6],
    ];
    if w.0.is_null() || !is_task_window(w) || !(SNL_LEFT..=SNL_MAX).contains(&layout) {
        return;
    }
    snap_prune();
    if layout == SNL_MAX {
        unsafe {
            let _ = ShowWindow(w, SW_MAXIMIZE);
        }
        snap_with(w, |sl| sl.state = W12SN_MAX);
        return;
    }
    if is_iconic(w) || is_zoomed(w) {
        unsafe {
            let _ = ShowWindow(w, SW_RESTORE);
        }
    }
    let (state, has_restore) = snap_with(w, |sl| (sl.state, sl.has_restore));
    if state == W12SN_RESTORED || !has_restore {
        // rect pulih untuk Win+↓
        let mut rc = RECT::default();
        unsafe {
            let _ = GetWindowRect(w, &mut rc);
        }
        snap_with(w, |sl| {
            sl.restore = rc;
            sl.has_restore = true;
        });
    }
    let (work, wr, fr) = match monitor_and_frame(w) {
        Some(v) => v,
        None => return,
    };
    let f = K[(layout - SNL_LEFT) as usize];
    let ww = work.right - work.left;
    let hh = work.bottom - work.top;
    let mut rc = RECT {
        left: work.left + ww * f[0] / 6,
        top: work.top + hh * f[1] / 6,
        right: work.left + ww * f[2] / 6,
        bottom: work.top + hh * f[3] / 6,
    };
    rc.left -= fr.left - wr.left;
    rc.top -= fr.top - wr.top;
    rc.right += wr.right - fr.right;
    rc.bottom += wr.bottom - fr.bottom;
    unsafe {
        let _ = SetWindowPos(
            w,
            None,
            rc.left,
            rc.top,
            rc.right - rc.left,
            rc.bottom - rc.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    // bukan RESTORED: Win+↓ memulihkan rect semula
    let next = if f[0] > 0 && f[2] == 6 { W12SN_RIGHT } else { W12SN_LEFT };
    snap_with(w, |sl| sl.state = next);
}

/// Win+Z: menu tata letak di tepi atas jendela depan (`SnapLayoutMenu`).
pub fn snap_layout_menu() {
    let w = unsafe { GetForegroundWindow() };
    if w.0.is_null() || !is_task_window(w) {
        return;
    }
    let m = match unsafe { CreatePopupMenu() } {
        Ok(m) => m,
        Err(_) => {
            // Menu tidak bisa dibuat: C tetap lanjut dan hanya mengembalikan fokus.
            activate_window(w);
            return;
        }
    };
    for &(id, text) in SNAP_MENU {
        if id == 0 {
            unsafe {
                let _ = AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null());
            }
        } else {
            let t = to_wide_z(text);
            unsafe {
                let _ = AppendMenuW(m, MF_STRING, id as usize, pcw(&t));
            }
        }
    }
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(w, &mut r);
    }
    let mut pt = POINT {
        x: (r.left + r.right) / 2 - s(90.0),
        y: r.top + s(10.0),
    };
    if pt.x < r.left {
        pt.x = r.left;
    }
    let c = track_menu(m, winx_start_owner(), pt, true);
    unsafe {
        let _ = DestroyMenu(m);
    }
    if c != 0 {
        snap_layout(w, c);
    }
    activate_window(w); // fokus kembali ke jendela, juga bila menu dibatalkan
}

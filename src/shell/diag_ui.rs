//! Sumber: w12diag.inc (1–167). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime.
//! Perintah diagnosis `--diag-cycle <komponen> <N>`: membuka lalu menutup satu komponen N kali lewat timer T_DIAG
//! (300 ms per langkah), lalu mencatat handle, GDI, USER, dan memori privat sebelum dan sesudah.
//! Deviasi: `DiagLog` = `crate::log::log`, sehingga baris diberi timestamp dan dipotong 1000 karakter (C menulis mentah).
use super::prelude::*;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetGuiResources, GetProcessHandleCount, GR_GDIOBJECTS, GR_USEROBJECTS,
};

// Status siklus (C: g_diagRun, g_diagComp, g_diagN, g_diagStep, g_diagMiss, g_diagA).
static DIAG_RUN: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static DIAG_COMP: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static DIAG_N: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static DIAG_STEP: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static DIAG_MISS: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static DIAG_A: SyncCell<Cell<W12DiagSample>> = SyncCell::new(Cell::new(W12DiagSample {
    handles: 0,
    gdi: 0,
    user: 0,
    priv_: 0,
}));

/// C: `DiagMeasure`. Mengukur handle, GDI, USER, dan memori privat proses ini (nilai dikembalikan, bukan out-param).
fn diag_measure() -> W12DiagSample {
    let hp = unsafe { GetCurrentProcess() };
    let mut h: u32 = 0;
    let handles_ok = unsafe { GetProcessHandleCount(hp, &mut h) }.is_ok();
    let handles = if handles_ok { h } else { 0 };
    let gdi = unsafe { GetGuiResources(hp, GR_GDIOBJECTS) };
    let user = unsafe { GetGuiResources(hp, GR_USEROBJECTS) };
    let mut pmc = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    let cb = pmc.cb;
    let mem_ok = unsafe {
        GetProcessMemoryInfo(
            hp,
            &mut pmc as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            cb,
        )
    }
    .is_ok();
    let priv_ = if mem_ok { pmc.PrivateUsage as u64 } else { 0 };
    W12DiagSample {
        handles,
        gdi,
        user,
        priv_,
    }
}

/// C: `DiagNote`. Menulis baris "diag <komponen> <teks>" lewat `log` (C: `DiagLog`).
fn diag_note(comp: i32, text: &str) {
    let mut row = [0u16; 160];
    let n = w12_diag_format_note(&mut row, comp, Some(text));
    if n > 0 {
        log(&from_wide(&row[..n as usize]));
    }
}

/// C: `DiagIndexOf`. Indeks pertama item dengan tipe `ty`, atau -1.
fn diag_index_of(ty: i32) -> i32 {
    let n = NITEMS.get().max(0) as usize;
    let items = ITEMS.borrow();
    for (i, item) in items.iter().take(n).enumerate() {
        if item.ty == ty {
            return i as i32;
        }
    }
    -1
}

/// C: `DiagItemShown`. Item ada, pilnya valid, dan pil itu sedang tampil.
fn diag_item_shown(ty: i32) -> bool {
    let i = diag_index_of(ty);
    if i < 0 {
        return false;
    }
    let p = {
        let items = ITEMS.borrow();
        match items.get(i as usize) {
            Some(item) => item.pill,
            None => return false,
        }
    };
    if p < 0 || p >= MAXPILLS {
        return false;
    }
    let pills = PILLS.borrow();
    pills.get(p as usize).is_some_and(|pill| pill.shown)
}

/// C: `DiagAvailable`.
fn diag_available(comp: i32) -> bool {
    match comp {
        W12DIAG_START => true,
        W12DIAG_PANEL => diag_item_shown(IT_NET),
        // shell_open_qs tetap membuka bila IT_VOL tidak tampil (sudut kanan bawah).
        W12DIAG_QS => true,
        W12DIAG_FLYOUT => !FLY.get().0.is_null() && diag_item_shown(IT_CHEV),
        W12DIAG_TASKVIEW => SHELL_MODE.get() && !TK_MSG.get().0.is_null(),
        _ => false,
    }
}

/// C: `DiagIsOpen`.
fn diag_is_open(comp: i32) -> bool {
    match comp {
        W12DIAG_START => PANEL_OPEN.get() == PN_START,
        W12DIAG_PANEL => PANEL_OPEN.get() == PN_WIFI,
        W12DIAG_QS => PANEL_OPEN.get() == PN_QS,
        W12DIAG_FLYOUT => FLY_OPEN.get(),
        W12DIAG_TASKVIEW => ov_mode() == OV_TV,
        _ => false,
    }
}

/// C: `DiagAct`. Buka atau tutup lewat jalur yang sama dengan pengguna.
/// Flyout dan Task View bersifat toggle; pemanggil hanya memanggil bila status aktual berbeda.
fn diag_act(comp: i32, open: bool) {
    match comp {
        W12DIAG_START => {
            if open {
                start_from_key(W12_STARTKEY_TOGGLE);
            } else {
                panel_close();
            }
        }
        W12DIAG_PANEL => {
            let idx = diag_index_of(IT_NET);
            if !open {
                panel_close();
            } else if idx >= 0 {
                activate_item(idx, false);
            }
        }
        W12DIAG_QS => {
            if open {
                shell_open_qs();
            } else {
                panel_close();
            }
        }
        W12DIAG_FLYOUT => {
            // ToggleTrayFlyout: buka atau tutup.
            let idx = diag_index_of(IT_CHEV);
            if idx >= 0 {
                activate_item(idx, false);
            }
        }
        W12DIAG_TASKVIEW => tv_toggle(),
        _ => {}
    }
}

/// C: `DiagStart` (dipanggil dari WM_W12_DIAG: wp = komponen, lp = N). Hanya menyiapkan; langkah dijalankan `diag_tick`.
pub fn diag_start(comp: i32, n: i32) {
    if comp < 0 || (comp as usize) >= W12DIAG_COUNT || !(1..=100).contains(&n) {
        return;
    }
    if DIAG_RUN.get() {
        diag_note(comp, "dilewati: diag sedang berjalan");
        return;
    }
    if !diag_available(comp) {
        diag_note(comp, "tidak tersedia");
        return;
    }
    if diag_is_open(comp) {
        diag_note(comp, "dilewati: sudah terbuka");
        return;
    }
    if comp != W12DIAG_FLYOUT && comp != W12DIAG_TASKVIEW && PANEL_OPEN.get() >= 0 {
        diag_note(comp, "dilewati: panel lain terbuka");
        return;
    }
    DIAG_COMP.set(comp);
    DIAG_N.set(n);
    DIAG_STEP.set(0);
    DIAG_MISS.set(0);
    DIAG_RUN.set(true);
    DIAG_A.set(diag_measure());
    unsafe { SetTimer(Some(MSG.get()), T_DIAG as usize, 300, None) };
}

/// C: `DiagTick` (dipanggil dari WM_TIMER dengan T_DIAG).
/// Langkah s = 0..2N: genap = buka, ganjil = tutup, langkah 2N = tutup terakhir. Setiap langkah juga memeriksa
/// apakah langkah sebelumnya berpengaruh; status yang tidak sesuai dihitung di DIAG_MISS.
/// Langkah 2N+1 mengukur dan mencatat.
pub fn diag_tick() {
    if !DIAG_RUN.get() {
        unsafe {
            let _ = KillTimer(Some(MSG.get()), T_DIAG as usize);
        }
        return;
    }
    let comp = DIAG_COMP.get();
    let n = DIAG_N.get();
    let last = 2 * n;
    let s = DIAG_STEP.get();
    if s > 0 {
        let prev_want = (s - 1) < last && (s - 1) % 2 == 0;
        if diag_is_open(comp) != prev_want {
            DIAG_MISS.set(DIAG_MISS.get() + 1);
        }
    }
    if s <= last {
        let want = s < last && s % 2 == 0;
        if diag_is_open(comp) != want {
            diag_act(comp, want);
        }
        DIAG_STEP.set(s + 1);
        return;
    }
    let after = diag_measure();
    let mut row = [0u16; 160];
    let units = w12_diag_format_row(&mut row, comp, n, &DIAG_A.get(), &after);
    if units > 0 {
        log(&from_wide(&row[..units as usize]));
    }
    let miss = DIAG_MISS.get();
    if miss > 0 {
        diag_note(comp, &format!("{} dari {} pengecekan status tidak sesuai", miss, last + 1));
    }
    DIAG_RUN.set(false);
    unsafe {
        let _ = KillTimer(Some(MSG.get()), T_DIAG as usize);
    }
    trim_soon();
}

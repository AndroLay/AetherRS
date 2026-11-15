//! Sumber: w12stophosts.inc (seluruh file). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H12.
//! stopHosts= di [shell] settings.ini: hanya mode shell; dipanggil dari handler T_STATUS tiap 10 dtk.
use super::prelude::*;

use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, TerminateProcess, PROCESS_TERMINATE,
};

/// Keadaan awal `W12StopState` (setara `W12StopReset`); konstanta agar bisa dipakai di static.
const STOP_EMPTY: W12StopState = W12StopState {
    at: [[0; W12STOP_LIMIT]; W12STOP_COUNT],
    used: [0; W12STOP_COUNT],
    off: [false; W12STOP_COUNT],
};

// g_stopIdx, g_stopN, g_stopSt, dan `loaded` (static lokal di StopHostsTick).
static STOP_IDX: SyncCell<Cell<[i32; W12STOP_COUNT]>> = SyncCell::new(Cell::new([0; W12STOP_COUNT]));
static STOP_N: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static STOP_ST: SyncCell<Cell<W12StopState>> = SyncCell::new(Cell::new(STOP_EMPTY));
static STOP_LOADED: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

/// Jumlah nama yang diterima dari stopHosts= (dibatasi W12STOP_COUNT).
fn stop_count() -> usize {
    (STOP_N.get().max(0) as usize).min(W12STOP_COUNT)
}

/// `StopListed(idx)`: apakah indeks whitelist `idx` termasuk nama yang diterima.
pub fn stop_listed(idx: i32) -> bool {
    let n = stop_count();
    let arr = STOP_IDX.get();
    arr[..n].contains(&idx)
}

/// Memindai proses di sesi sendiri; `None` bila sesi atau snapshot tidak bisa diperoleh (C: return dini).
fn stop_scan() -> Option<([[u32; 16]; W12STOP_COUNT], [usize; W12STOP_COUNT])> {
    let mut pids = [[0u32; 16]; W12STOP_COUNT];
    let mut hit_n = [0usize; W12STOP_COUNT];
    unsafe {
        let mut my_sess = 0u32;
        ProcessIdToSessionId(GetCurrentProcessId(), &mut my_sess).ok()?;
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) if h != INVALID_HANDLE_VALUE => h,
            _ => return None,
        };
        let mut pe: PROCESSENTRY32W = std::mem::zeroed();
        pe.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut pe).is_ok() {
            loop {
                let idx = w12_stop_index(&pe.szExeFile);
                let ix = idx as usize;
                if idx >= 0 && ix < W12STOP_COUNT && stop_listed(idx) && hit_n[ix] < 16 {
                    let mut s = 0u32;
                    if ProcessIdToSessionId(pe.th32ProcessID, &mut s).is_ok() && s == my_sess {
                        pids[ix][hit_n[ix]] = pe.th32ProcessID;
                        hit_n[ix] += 1;
                    }
                }
                if Process32NextW(snap, &mut pe).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        Some((pids, hit_n))
    }
}

/// `OpenProcess(PROCESS_TERMINATE)` lalu `TerminateProcess(h, 1)`; `false` bila handle tidak bisa dibuka atau gagal diakhiri.
fn stop_kill(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_TERMINATE, false, pid) {
            Ok(h) => {
                let ok = TerminateProcess(h, 1).is_ok();
                let _ = CloseHandle(h);
                ok
            }
            Err(_) => false,
        }
    }
}

/// `StopHostsTick()`: dipanggil dari handler T_STATUS tiap 10 dtk; tanpa efek di luar mode shell.
pub fn stop_hosts_tick() {
    if !SHELL_MODE.get() {
        return;
    }
    if !STOP_LOADED.get() {
        STOP_LOADED.set(true);
        let mut st = STOP_ST.get();
        w12_stop_reset(&mut st);
        STOP_ST.set(st);
        let raw = to_wide_z(&ini_string("shell", "stopHosts", "", 256));
        let mut idx = [0i32; W12STOP_COUNT];
        let mut rej = 0i32;
        let n = w12_stop_parse(&raw, &mut idx, Some(&mut rej));
        STOP_IDX.set(idx);
        STOP_N.set(n);
        if rej > 0 {
            log("stopHosts: nama di luar daftar yang diizinkan diabaikan");
        }
        if n > 0 {
            log(&format!("stopHosts aktif (mode shell): {} nama", n));
        }
    }
    let n = stop_count();
    if n == 0 {
        return;
    }
    let idxs = STOP_IDX.get();
    let mut st = STOP_ST.get();
    if !idxs[..n].iter().any(|&ix| !st.off[ix as usize]) {
        return; // semua nama sudah nonaktif: tidak perlu memindai lagi
    }
    let Some((pids, hit_n)) = stop_scan() else {
        return;
    };
    let now = unsafe { GetTickCount64() };
    for &idx in &idxs[..n] {
        let ix = idx as usize;
        if hit_n[ix] == 0 {
            continue;
        }
        let act = w12_stop_decide(&mut st, idx, now);
        let name = w12_stop_name(idx).unwrap_or("");
        if act == W12STOP_TERMINATE {
            for &pid in &pids[ix][..hit_n[ix]] {
                let ok = stop_kill(pid);
                log(&format!(
                    "stopHosts: {} pid {} {}",
                    name,
                    pid,
                    if ok { "dihentikan" } else { "tidak bisa dihentikan" }
                ));
            }
        } else if act == W12STOP_DISABLE {
            log(&format!(
                "stopHosts: {} muncul {} kali dalam 10 menit; nama ini dinonaktifkan sampai login berikutnya",
                name, W12STOP_LIMIT
            ));
        }
    }
    STOP_ST.set(st);
}

//! Port `win12guard.c` (baris 1–64). Status: lengkap.
//! Keluar kode 0 berhenti; crash dicatat ke `crash.log`, taskbar bawaan ditampilkan, shell dijalankan ulang (maks 3 kali per 2 menit).
//! Perbedaan kecil: `crash.log` lewat `std::fs` (mode berbagi lebih longgar daripada `FILE_SHARE_READ` di C).

#![windows_subsystem = "windows"]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::Storage::FileSystem::SYNCHRONIZE;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{
    CreateMutexW, CreateProcessW, GetCurrentProcess, GetExitCodeProcess, OpenProcess,
    SetProcessWorkingSetSize, WaitForSingleObject, INFINITE, PROCESS_ACCESS_RIGHTS,
    PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, ShowWindowAsync, SW_SHOWNA};

use w12::log::{exe_dir, timestamp};
use w12::wide::{pcw, to_wide_z};

/// Nama mutex tunggal pengawas (sama dengan sumber C).
const MUTEX_NAME: &str = "Win12GuardSingleInstance";
/// Kelas jendela taskbar bawaan yang ditampilkan saat shell mati.
const TRAY_CLASS: &str = "Shell_TrayWnd";
/// Panjang jendela hitung restart: 120000 ms (2 menit).
const WINDOW_MS: u32 = 120_000;
/// Batas restart dalam jendela itu (`if (++restarts > 3) break;`).
const MAX_RESTARTS: u32 = 3;
/// Jeda sebelum shell dijalankan ulang (`Sleep(1500)`).
const RESTART_DELAY_MS: u64 = 1500;

/// Pid dari baris perintah: digit awal (setelah spasi), seperti `wcstoul(cmd, NULL, 10)`. Gagal = 0.
fn parse_pid() -> u32 {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let line = args.join(" ");
    let digits: String = line
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u32>().unwrap_or(0)
}

/// Jalankan `"<exe>" --guarded` dengan folder kerja `dir` (setara `CreateProcessW` di C).
/// Handle thread langsung ditutup seperti sumber. `None` bila gagal.
fn launch(exe: &str, dir: &str) -> Option<HANDLE> {
    let app = to_wide_z(exe);
    let mut cl = to_wide_z(&format!("\"{}\" --guarded", exe));
    let cwd = to_wide_z(dir);
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        CreateProcessW(
            pcw(&app),
            Some(PWSTR(cl.as_mut_ptr())),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(0),
            None,
            pcw(&cwd),
            &si,
            &mut pi,
        )
    };
    if ok.is_err() {
        return None;
    }
    unsafe {
        let _ = CloseHandle(pi.hThread);
    }
    Some(pi.hProcess)
}

/// Tambah satu baris ke `crash.log` di folder exe (UTF-8, CRLF). Setara `CreateFileW` FILE_APPEND_DATA + OPEN_ALWAYS di C.
fn append_crash(dir: &Path, code: u32, restart_no: u32) {
    let line = format!(
        "{} guard: shell keluar kode=0x{:08X}, restart ke-{}\r\n",
        timestamp(),
        code,
        restart_no
    );
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("crash.log"))
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Tampilkan lagi taskbar bawaan bila jendelanya ada (`FindWindowW` lalu `ShowWindowAsync(SW_SHOWNA)`).
fn show_tray(cls: &[u16]) {
    if let Ok(tray) = unsafe { FindWindowW(pcw(cls), PCWSTR::null()) } {
        if !tray.0.is_null() {
            unsafe {
                let _ = ShowWindowAsync(tray, SW_SHOWNA);
            }
        }
    }
}

fn main() {
    // Mutex tunggal: bila sudah ada pengawas lain, keluar tanpa tindakan.
    let mname = to_wide_z(MUTEX_NAME);
    let mtx = unsafe { CreateMutexW(None, false, pcw(&mname)) };
    let last = unsafe { GetLastError() };
    if last == ERROR_ALREADY_EXISTS {
        return;
    }

    let dir = exe_dir();
    let exe_s = dir.join("win12shell.exe").to_string_lossy().into_owned();
    let dir_s = dir.to_string_lossy().into_owned();
    let tray_cls = to_wide_z(TRAY_CLASS);

    // Proses yang diawasi (pid dari argumen). Gagal dibuka berarti tidak ada yang diawasi.
    let pid = parse_pid();
    let mut p: Option<HANDLE> = None;
    if pid != 0 {
        let access = PROCESS_ACCESS_RIGHTS(SYNCHRONIZE.0 | PROCESS_QUERY_LIMITED_INFORMATION.0);
        p = unsafe { OpenProcess(access, false, pid) }.ok();
    }

    // Sekali saat mulai: kecilkan working set pengawas sendiri (hanya prosesnya sendiri).
    unsafe {
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }

    let mut restarts: u32 = 0;
    let mut window_start: u32 = unsafe { GetTickCount() };
    while let Some(ph) = p {
        let mut code: u32 = 0;
        unsafe {
            let _ = WaitForSingleObject(ph, INFINITE);
            let _ = GetExitCodeProcess(ph, &mut code);
            let _ = CloseHandle(ph);
        }
        if code == 0 {
            break; // keluar normal (mis. menu "Exit Win12 Shell"): pengawas ikut berhenti
        }

        append_crash(&dir, code, restarts + 1);
        show_tray(&tray_cls);

        let now = unsafe { GetTickCount() };
        if now.wrapping_sub(window_start) > WINDOW_MS {
            restarts = 0;
            window_start = unsafe { GetTickCount() };
        }
        restarts += 1;
        if restarts > MAX_RESTARTS {
            break; // menyerah; taskbar bawaan tetap tampil
        }

        std::thread::sleep(Duration::from_millis(RESTART_DELAY_MS));
        match launch(&exe_s, &dir_s) {
            Some(h) => p = Some(h),
            None => break,
        }
    }

    if let Ok(h) = mtx {
        unsafe {
            let _ = CloseHandle(h);
        }
    }
}

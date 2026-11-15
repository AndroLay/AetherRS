//! Port `win12boot.c` (baris 1–415): peluncur login untuk mode shell penuh (rencana-shell-penuh.md).
//! Status: lengkap. Penulisan `HKCU\...\Winlogon\Shell` hanya di `start_explorer` dan uji sekali di `main`,
//! ditandai `// PORT: menulis Winlogon\Shell, JANGAN dijalankan`; fungsi itu tidak boleh dipanggil dari tes. Semua penulisan itu dimatikan oleh `WINLOGON_WRITE_ENABLED = false`.

#![windows_subsystem = "windows"]

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, HANDLE,
    HWND, LPARAM, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM, SYNCHRONIZE,
};
use windows::Win32::System::Com::{
    CoInitializeEx, COINIT, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegEnumValueW, RegGetValueW, RegOpenKeyExW, HKEY,
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_32KEY,
    KEY_WOW64_64KEY, REG_EXPAND_SZ, REG_SAM_FLAGS, REG_SZ, RRF_RT_REG_BINARY,
};
use windows::Win32::System::SystemInformation::{GetTickCount, GetWindowsDirectoryW};
use windows::Win32::System::Threading::{
    CreateMutexW, CreateProcessW, GetCurrentProcess, GetExitCodeProcess, OpenProcess,
    QueryFullProcessImageNameW, SetProcessWorkingSetSize, Sleep, WaitForSingleObject,
    INFINITE, PROCESS_ACCESS_RIGHTS, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION,
    PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::{
    SHGetFolderPathW, ShellExecuteW, CSIDL_COMMON_STARTUP, CSIDL_STARTUP, SHGFP_TYPE_CURRENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetShellWindow, GetSystemMetrics, GetWindowThreadProcessId, IsWindow,
    IsWindowVisible, PostMessageW, ShowWindowAsync, SM_SHUTTINGDOWN, SW_SHOWNA, SW_SHOWNORMAL,
    WM_CLOSE, WM_USER,
};

use w12::log::{boot_log_path, exe_dir, log};
use w12::reg::{self, OUR_KEY};
use w12::vdfile::w12_vd_parse_next;
use w12::wide::{from_wide, pcw, to_wide_z};

/// Setara `kWinlogon` (baris 31). Hanya dipakai untuk membaca dan menulis `Winlogon\Shell` di HKCU.
const WINLOGON: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Winlogon";
/// PORT: penulisan `Winlogon\Shell` dimatikan (false). Jangan diubah ke true tanpa persetujuan pengguna.
const WINLOGON_WRITE_ENABLED: bool = false;
/// Setara `kCv` (baris 32).
const CV: &str = r"Software\Microsoft\Windows\CurrentVersion";
/// Nama mutex tunggal (baris 348 dan seterusnya).
const MUTEX_NAME: &str = "Win12BootSingleInstance";
/// Kelas jendela shell Win12 (literal sama dengan sumber).
const SHELL_CLASS: &str = "Win12ShellMsg";
/// Kelas jendela taskbar Explorer (literal sama dengan sumber).
const TRAY_CLASS: &str = "Shell_TrayWnd";
/// Pesan `WM_USER + 436` untuk beralih ke Win12 penuh (literal sama dengan sumber).
const MSG_SWITCH: u32 = WM_USER + 436;

/// Setara `DelValue` (baris 61–67). Semua pemanggil di sumber memakai HKCU.
fn del_value(sub: &str, name: &str) {
    let _ = reg::delete_value(sub, name);
}

/// Setara `Approved` (baris 69–81): `StartupApproved\<sub>`, byte pertama ganjil berarti nonaktif.
/// Diperiksa di HKCU lalu HKLM; salah satu menonaktifkan berarti `false`.
fn approved(sub: &str, name: &str) -> bool {
    let path = to_wide_z(&format!(r"{}\Explorer\StartupApproved\{}", CV, sub));
    let nm = to_wide_z(name);
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let mut b = [0u8; 16];
        let mut cb: u32 = b.len() as u32;
        let rc = unsafe {
            RegGetValueW(
                root,
                pcw(&path),
                pcw(&nm),
                RRF_RT_REG_BINARY,
                None,
                Some(b.as_mut_ptr() as *mut c_void),
                Some(&mut cb),
            )
        };
        if rc == ERROR_SUCCESS && cb > 0 && (b[0] & 1) != 0 {
            return false;
        }
    }
    true
}

/// Setara `IsOurs` (baris 83–89): substring tanpa membedakan huruf besar-kecil.
fn is_ours(s: &str) -> bool {
    let low = s.to_lowercase();
    low.contains("win12shell.exe") || low.contains("win12boot.exe") || low.contains("win12guard.exe")
}

/// Pengganti sederhana `ExpandEnvironmentStringsW` yang ditulis sendiri (tanpa memanggil API; fitur `Win32_System_Environment` sudah aktif di Cargo.toml, tetapi helper ini dipertahankan).
/// Mengganti `%NAMA%` dengan nilai variabel lingkungan; nama yang tidak dikenal dibiarkan apa adanya.
fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    loop {
        let Some(a) = rest.find('%') else { break };
        let after = &rest[a + 1..];
        let Some(b) = after.find('%') else { break };
        out.push_str(&rest[..a]);
        let name = &after[..b];
        match std::env::var(name) {
            Ok(v) if !name.is_empty() => out.push_str(&v),
            _ => {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
        }
        rest = &after[b + 1..];
    }
    out.push_str(rest);
    out
}

/// Pembungkus `CreateProcessW` (flag 0). Mengembalikan `hProcess` bila berhasil; `hThread` langsung ditutup.
/// Bila gagal, `Err` berisi kode error Win32 (`GetLastError`) untuk dicatat ke log.
/// `app` = lpApplicationName (`None` = NULL), `cwd` = lpCurrentDirectory (`None` = mewarisi direktori aktif).
fn create_process(app: Option<&str>, cmd: &str, cwd: Option<&Path>) -> Result<HANDLE, u32> {
    let app_w = app.map(to_wide_z);
    let cwd_w = cwd.map(|p| to_wide_z(&p.to_string_lossy()));
    let mut cmd_w = to_wide_z(cmd);
    let app_p = app_w.as_ref().map_or(PCWSTR::null(), |v| pcw(v));
    let cwd_p = cwd_w.as_ref().map_or(PCWSTR::null(), |v| pcw(v));
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let res = unsafe {
        CreateProcessW(
            app_p,
            Some(PWSTR(cmd_w.as_mut_ptr())),
            None,
            None,
            false.into(),
            PROCESS_CREATION_FLAGS(0),
            None,
            cwd_p,
            &si,
            &mut pi,
        )
    };
    match res {
        Ok(()) => {
            unsafe {
                let _ = CloseHandle(pi.hThread);
            }
            Ok(pi.hProcess)
        }
        Err(_) => Err(unsafe { GetLastError() }.0),
    }
}

/// Setara `Launch` (baris 91–105): jalankan satu entri Run/RunOnce, lalu jeda 150 ms.
fn launch(what: &str, cmdline: &str) {
    let ex = expand_env(cmdline);
    match create_process(None, &ex, None) {
        Ok(h) => {
            unsafe {
                let _ = CloseHandle(h);
            }
            log(&format!("startup: {}", what));
        }
        Err(code) => log(&format!("startup GAGAL ({}): {} = {}", code, what, ex)),
    }
    unsafe { Sleep(150) };
}

/// Setara `RunKey` (baris 107–134): entri `Run` atau `RunOnce` pada satu hive.
/// `RunOnce` dihapus dulu, kecuali nama berawalan `!` (dihapus sesudah dijalankan); nama `*` dilewati.
/// `Run` hanya dijalankan bila disetujui di `StartupApproved`.
fn run_key(root: HKEY, wow: REG_SAM_FLAGS, approved_sub: &str, once: bool) {
    let path = to_wide_z(&format!(r"{}\{}", CV, if once { "RunOnce" } else { "Run" }));
    let access = REG_SAM_FLAGS(KEY_READ.0 | wow.0 | if once { KEY_SET_VALUE.0 } else { 0 });
    let mut k = HKEY::default();
    if unsafe { RegOpenKeyExW(root, pcw(&path), Some(0), access, &mut k) } != ERROR_SUCCESS {
        return;
    }
    // Setara buffer statis names[MAXV][260] dan data[MAXV][1024] di sumber C.
    const MAXV: usize = 64;
    let mut entries: Vec<(Vec<u16>, Vec<u16>)> = Vec::new();
    let mut i: u32 = 0;
    while entries.len() < MAXV {
        let mut name = vec![0u16; 260];
        let mut data = vec![0u16; 1024];
        let mut cn: u32 = name.len() as u32;
        let mut cd: u32 = (data.len() * 2 - 2) as u32;
        let mut ty: u32 = 0;
        let rc = unsafe {
            RegEnumValueW(
                k,
                i,
                Some(PWSTR(name.as_mut_ptr())),
                &mut cn,
                None,
                Some(&mut ty as *mut u32),
                Some(data.as_mut_ptr() as *mut u8),
                Some(&mut cd as *mut u32),
            )
        };
        i += 1;
        if rc == ERROR_NO_MORE_ITEMS {
            break;
        }
        if rc != ERROR_SUCCESS || (ty != REG_SZ.0 && ty != REG_EXPAND_SZ.0) || name[0] == 0 {
            continue;
        }
        let end = ((cd / 2) as usize).min(data.len() - 1);
        data[end] = 0;
        entries.push((name, data));
    }
    for (name_buf, data_buf) in &entries {
        let name = from_wide(name_buf);
        let cmd = from_wide(data_buf);
        if is_ours(&cmd) || cmd.is_empty() {
            continue;
        }
        if once {
            if name.starts_with('*') {
                continue; // khusus Safe Mode
            }
            let deferred = name.starts_with('!');
            if !deferred {
                let _ = unsafe { RegDeleteValueW(k, pcw(name_buf)) };
            }
            launch(&name, &cmd);
            if deferred {
                let _ = unsafe { RegDeleteValueW(k, pcw(name_buf)) };
            }
        } else if approved(approved_sub, &name) {
            launch(&name, &cmd);
        }
    }
    unsafe {
        let _ = RegCloseKey(k);
    }
}

/// Setara `RunFolder` (baris 136–155): berkas di folder Startup yang disetujui (`StartupFolder`).
/// Direktori, berkas tersembunyi, dan berkas sistem dilewati; `desktop.ini` dan `Win12 Shell.lnk` juga.
fn run_folder(csidl: u32) {
    let mut buf = [0u16; 260];
    let hr = unsafe { SHGetFolderPathW(None, csidl as i32, None, SHGFP_TYPE_CURRENT.0 as u32, &mut buf) };
    if hr.is_err() {
        return;
    }
    let dir = PathBuf::from(from_wide(&buf));
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    let skip = FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_HIDDEN.0 | FILE_ATTRIBUTE_SYSTEM.0;
    let dir_w = to_wide_z(&dir.to_string_lossy());
    for ent in rd.flatten() {
        let attrs = ent
            .metadata()
            .map(|m| std::os::windows::fs::MetadataExt::file_attributes(&m))
            .unwrap_or(0);
        if attrs & skip != 0 {
            continue;
        }
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("Win12 Shell.lnk") {
            continue;
        }
        if !approved("StartupFolder", &name) {
            continue;
        }
        let full = dir.join(&name).to_string_lossy().into_owned();
        let full_w = to_wide_z(&full);
        let r = unsafe {
            ShellExecuteW(
                None,
                PCWSTR::null(),
                pcw(&full_w),
                PCWSTR::null(),
                pcw(&dir_w),
                SW_SHOWNORMAL,
            )
        };
        let code = r.0 as isize;
        if code > 32 {
            log(&format!("startup: {}", name));
        } else {
            log(&format!("startup GAGAL ({}): {}", code as i32, full));
        }
        unsafe { Sleep(150) };
    }
}

/// Setara `RunStartupItems` (baris 157–165): urutan pemanggilan sama dengan sumber C.
fn run_startup_items() {
    run_key(HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY, "Run", false);
    run_key(HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, "Run32", false);
    run_key(HKEY_CURRENT_USER, REG_SAM_FLAGS(0), "Run", false);
    run_key(HKEY_CURRENT_USER, REG_SAM_FLAGS(0), "Run", true);
    run_folder(CSIDL_COMMON_STARTUP);
    run_folder(CSIDL_STARTUP);
}

/// Pembungkus `FindWindowW(kelas, NULL)`: `Some` bila jendela dengan kelas itu ditemukan.
fn find_window(class: &str) -> Option<HWND> {
    let cls = to_wide_z(class);
    match unsafe { FindWindowW(pcw(&cls), PCWSTR::null()) } {
        Ok(h) if !h.0.is_null() => Some(h),
        _ => None,
    }
}

/// Setara `ExplorerIsShell` (baris 167–184): apakah jendela desktop (shell) dimiliki `explorer.exe`.
fn explorer_is_shell() -> bool {
    let sw = unsafe { GetShellWindow() };
    if sw.0.is_null() {
        return false;
    }
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(sw, Some(&mut pid as *mut u32)) };
    let h = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(_) => return false,
    };
    let mut buf = [0u16; 260];
    let mut n: u32 = buf.len() as u32;
    let mut r = false;
    let ok = unsafe {
        QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut n)
    };
    if ok.is_ok() {
        let p = from_wide(&buf);
        let base = p.rsplit('\\').next().unwrap_or("");
        r = base.eq_ignore_ascii_case("explorer.exe");
    }
    let _ = unsafe { CloseHandle(h) };
    r
}

/// Setara `StartShell` (baris 186–200): menjalankan `"<win12shell.exe>" --shell` dengan folder program sebagai cwd.
/// Mengembalikan handle proses bila berhasil; gagal dicatat ke log.
fn start_shell() -> Option<HANDLE> {
    let dir = exe_dir();
    let exe = dir.join("win12shell.exe");
    let exe_s = exe.to_string_lossy().into_owned();
    let cl = format!("\"{}\" --shell", exe_s);
    match create_process(Some(&exe_s), &cl, Some(&dir)) {
        Ok(h) => Some(h),
        Err(code) => {
            log(&format!("win12shell.exe tidak bisa dijalankan ({})", code));
            None
        }
    }
}

/// Setara `AdoptRunningShell` (baris 204–212): shell yang sudah berjalan diawasi, tidak dijalankan ulang.
/// Tanpa ini instans kedua langsung keluar lewat mutex dengan kode 0 dan terbaca sebagai "ditutup pengguna".
fn adopt_running_shell() -> Option<HANDLE> {
    let w = find_window(SHELL_CLASS)?;
    let mut pid: u32 = 0;
    let tid = unsafe { GetWindowThreadProcessId(w, Some(&mut pid as *mut u32)) };
    if tid == 0 || pid == 0 {
        return None;
    }
    let access = PROCESS_ACCESS_RIGHTS(SYNCHRONIZE.0 | PROCESS_QUERY_LIMITED_INFORMATION.0);
    match unsafe { OpenProcess(access, false, pid) } {
        Ok(p) => {
            log(&format!("mengawasi shell yang sudah berjalan (pid {})", pid));
            Some(p)
        }
        Err(_) => None,
    }
}

/// Pulihkan jendela tersembunyi dari `vdesk-hidden.txt` (`RestoreHiddenWindows`, win12boot.c 217–244).
/// Baris rusak dilewati (sama dengan VdRecover); berkas dihapus setelah dibaca.
fn restore_hidden_windows() {
    use std::io::Read;
    let path = exe_dir().join("vdesk-hidden.txt");
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let mut data: Vec<u8> = Vec::new();
    // Batas 16 KB seperti ReadFile di sumber C; bila baca gagal, anggap 0 byte.
    if file.take(16 * 1024).read_to_end(&mut data).is_err() {
        data.clear();
    }
    let mut cur: &[u8] = &data;
    loop {
        let mut hv: u64 = 0;
        let mut pid: u32 = 0;
        if w12_vd_parse_next(&mut cur, &mut hv, &mut pid) {
            let h = HWND(hv as usize as *mut c_void);
            let alive = unsafe { IsWindow(Some(h)) }.as_bool();
            if alive {
                let mut wp: u32 = 0;
                let tid = unsafe { GetWindowThreadProcessId(h, Some(&mut wp as *mut u32)) };
                let visible = unsafe { IsWindowVisible(h) }.as_bool();
                if tid != 0 && wp == pid && !visible {
                    unsafe {
                        let _ = ShowWindowAsync(h, SW_SHOWNA);
                    }
                }
            }
        } else {
            // Baris rusak: lewati sampai setelah '\n' berikutnya; berhenti bila tidak ada lagi.
            match cur.iter().position(|&b| b == b'\n') {
                Some(k) => cur = &cur[k + 1..],
                None => break,
            }
        }
    }
    let _ = std::fs::remove_file(&path);
}

/// Direktori Windows (`GetWindowsDirectoryW`); kosong bila gagal.
fn windows_dir() -> String {
    let mut buf = [0u16; 260];
    let n = unsafe { GetWindowsDirectoryW(Some(&mut buf[..])) } as usize;
    if n == 0 {
        return String::new();
    }
    from_wide(&buf[..n.min(buf.len())])
}

/// Handle SYNCHRONIZE untuk proses `pid` (setara `OpenProcess(SYNCHRONIZE, FALSE, pid)`); `None` bila pid 0 atau gagal.
fn open_sync(pid: u32) -> Option<HANDLE> {
    if pid == 0 {
        return None;
    }
    let access = PROCESS_ACCESS_RIGHTS(SYNCHRONIZE.0);
    let r = unsafe { OpenProcess(access, false, pid) };
    r.ok()
}

/// Jalankan Explorer sebagai shell cadangan (`StartExplorer`, win12boot.c 246–286).
/// Menulis `HKCU\...\Winlogon\Shell` (dihapus lalu dipulihkan): JANGAN dijalankan.
fn start_explorer(why: &str) {
    let shutting_down = unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) };
    if shutting_down != 0 {
        return;
    }
    restore_hidden_windows();
    let ex = format!("{}\\explorer.exe", windows_dir());
    // Simpan nilai Shell asli ke ShellPending sebelum dihapus, agar bisa dipulihkan (lihat win12-startup.ps1).
    let saved = reg::get_string(WINLOGON, "Shell");
    let mut had = false;
    // PORT: tanpa WINLOGON_WRITE_ENABLED, blok ini dilewati: Winlogon\Shell dan ShellPending tidak disentuh.
    if let Some(value) = saved.as_deref().filter(|_| WINLOGON_WRITE_ENABLED) {
        had = reg::set_string(OUR_KEY, "ShellPending", value);
        if had {
            // PORT: menulis Winlogon\Shell, JANGAN dijalankan
            del_value(WINLOGON, "Shell");
        }
    }
    let cmd = format!("\"{}\"", ex);
    let launched = match create_process(Some(ex.as_str()), &cmd, None) {
        Ok(h) => {
            unsafe {
                let _ = CloseHandle(h);
            }
            true
        }
        Err(_) => false,
    };
    log(&format!(
        "explorer.exe dijalankan sebagai shell cadangan: {}{}",
        why,
        if launched { "" } else { " (GAGAL)" }
    ));
    if !had {
        return;
    }
    let mut i: u32 = 0;
    while launched && i < 100 && find_window(TRAY_CLASS).is_none() {
        unsafe { Sleep(200) };
        i += 1;
    }
    if reg::get_string(OUR_KEY, "ShellPending").is_none() {
        log("nilai Shell tidak dipulihkan: mode shell dimatikan selama menunggu");
        return;
    }
    del_value(OUR_KEY, "ShellPending");
    let value = saved.unwrap_or_default();
    // PORT: menulis Winlogon\Shell, JANGAN dijalankan
    if WINLOGON_WRITE_ENABLED && reg::set_string(WINLOGON, "Shell", &value) {
        let tray = if find_window(TRAY_CLASS).is_some() {
            "ada"
        } else {
            "belum ada"
        };
        log(&format!(
            "nilai Shell dipulihkan setelah {} ms (taskbar Explorer {})",
            i * 200,
            tray
        ));
    }
}

/// Tunggu selagi deploy berjalan (`WaitWhileUpdating`, win12boot.c 288–291).
fn wait_while_updating() {
    let mut i: u32 = 0;
    while i < 400 && reg::our_dword("Updating") != 0 {
        unsafe { Sleep(300) };
        i += 1;
    }
}

/// Buka target `OpenAfterSwitch` setelah beralih ke Explorer (`OpenAfterSwitch`, win12boot.c 295–307).
fn open_after_switch() {
    let Some(target) = reg::get_string(OUR_KEY, "OpenAfterSwitch") else {
        return;
    };
    del_value(OUR_KEY, "OpenAfterSwitch");
    // Hanya awalan ms-settings: dan shell:AppsFolder\ yang diterima (tanpa membedakan huruf besar-kecil).
    let starts_with_ci =
        |p: &str| target.get(..p.len()).is_some_and(|s| s.eq_ignore_ascii_case(p));
    if !starts_with_ci("ms-settings:") && !starts_with_ci("shell:AppsFolder\\") {
        log(&format!("OpenAfterSwitch diabaikan: {}", target));
        return;
    }
    let mut i: u32 = 0;
    while i < 100 && find_window(TRAY_CLASS).is_none() {
        unsafe { Sleep(200) };
        i += 1;
    }
    unsafe { Sleep(2500) };
    let verb = to_wide_z("open");
    let file = to_wide_z(&target);
    let r = unsafe {
        ShellExecuteW(
            None,
            pcw(&verb),
            pcw(&file),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = r.0 as isize as i32;
    log(&format!(
        "dibuka setelah beralih ke Explorer: {} (hasil {})",
        target, code
    ));
}

/// Pindah dari Explorer ke Win12 penuh (`SwitchFromExplorer`, win12boot.c 311–346).
/// Mengembalikan `true` bila Explorer keluar; bila gagal, shell Win12 sebelumnya dijalankan lagi.
fn switch_from_explorer() -> bool {
    let msg = find_window(SHELL_CLASS);
    let had_win12 = msg.is_some();
    if let Some(m) = msg {
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(m, Some(&mut pid as *mut u32)) };
        let p = open_sync(pid);
        unsafe {
            let _ = PostMessageW(Some(m), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if let Some(h) = p {
            unsafe {
                let _ = WaitForSingleObject(h, 8000);
                let _ = CloseHandle(h);
            }
        }
    }
    let mut epid: u32 = 0;
    let sw = unsafe { GetShellWindow() };
    if !sw.0.is_null() {
        unsafe { GetWindowThreadProcessId(sw, Some(&mut epid as *mut u32)) };
    }
    let ep = open_sync(epid);
    if let Some(tray) = find_window(TRAY_CLASS) {
        unsafe {
            let _ = PostMessageW(Some(tray), MSG_SWITCH, WPARAM(0), LPARAM(0));
        }
    }
    let mut gone = false;
    if let Some(h) = ep {
        let wait = unsafe { WaitForSingleObject(h, 15000) };
        gone = wait == WAIT_OBJECT_0;
        unsafe {
            let _ = CloseHandle(h);
        }
    }
    let mut i: u32 = 0;
    while i < 50 && !gone {
        if explorer_is_shell() {
            unsafe { Sleep(200) };
        } else {
            gone = true;
        }
        i += 1;
    }
    if gone {
        log("beralih ke Win12 penuh: Explorer keluar");
        return true;
    }
    log("beralih ke Win12 penuh GAGAL: Explorer tidak keluar dalam 25 dtk");
    if had_win12 {
        let dir = exe_dir();
        let shell_s = dir.join("win12shell.exe").to_string_lossy().into_owned();
        let cmd = format!("\"{}\"", shell_s);
        if let Ok(h) = create_process(Some(shell_s.as_str()), &cmd, Some(dir.as_path())) {
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
    false
}

/// Titik masuk (`wWinMain`, win12boot.c 348–415): peluncur login untuk mode shell penuh.
fn main() {
    let name = to_wide_z(MUTEX_NAME);
    let mtx = unsafe { CreateMutexW(None, false, pcw(&name)) };
    let last = unsafe { GetLastError() };
    if last == ERROR_ALREADY_EXISTS {
        return;
    }
    // boot.log dibatasi 256 KB; berkas yang lebih besar dihapus sebelum dipakai (sama dengan sumber C).
    let boot_log = boot_log_path();
    if let Ok(md) = std::fs::metadata(&boot_log) {
        if md.len() > 256 * 1024 {
            let _ = std::fs::remove_file(&boot_log);
        }
    }
    let com_flags = COINIT(COINIT_APARTMENTTHREADED.0 | COINIT_DISABLE_OLE1DDE.0);
    let _ = unsafe { CoInitializeEx(None, com_flags) };
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let _ = std::env::set_current_dir(profile);
    }
    if WINLOGON_WRITE_ENABLED && reg::our_dword("ShellTestOnce") != 0 {
        // PORT: menulis Winlogon\Shell, JANGAN dijalankan
        del_value(WINLOGON, "Shell");
        del_value(OUR_KEY, "ShellTestOnce");
        log("uji sekali: nilai Shell per pengguna dihapus; login berikutnya kembali ke Explorer");
    }
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let cmd = args.join(" ");
    let sw = cmd.contains("--switch");
    if sw && explorer_is_shell() && !switch_from_explorer() {
        return;
    }
    if explorer_is_shell() {
        log("Explorer sudah menjadi shell; win12boot berhenti");
        return;
    }
    let mut startup_done = sw || cmd.contains("--no-startup");
    log(&format!(
        "mulai mode shell{}",
        if startup_done {
            " (tanpa aplikasi Startup: pindah di tengah sesi)"
        } else {
            ""
        }
    ));
    let mut restarts: u32 = 0;
    let mut window_start = unsafe { GetTickCount() };
    loop {
        wait_while_updating();
        let mut shell_proc = adopt_running_shell();
        if shell_proc.is_none() {
            // PORT: deviasi dari C (2026-11-14). Win12 Shell masuk mode shell hanya bila Progman tidak ada
            // (core_c4b.rs, `progman_absent`). Restart setelah deploy bisa jatuh ke Explorer yang masih hidup;
            // tanpa langkah ini shell berjalan tanpa mode shell dan stopHosts tidak pernah jalan.
            if explorer_is_shell() {
                log("Explorer masih memegang desktop saat shell dimulai ulang; menutup Explorer dulu");
                if !switch_from_explorer() {
                    log("Explorer belum keluar; shell dimulai, mode shell kemungkinan tidak aktif");
                }
            }
            shell_proc = start_shell();
        }
        let Some(p) = shell_proc else {
            start_explorer("shell gagal dijalankan");
            break;
        };
        if !startup_done {
            startup_done = true;
            let mut i: u32 = 0;
            while i < 75 && find_window(SHELL_CLASS).is_none() {
                unsafe { Sleep(200) };
                i += 1;
            }
            unsafe { Sleep(1000) };
            run_startup_items();
            let _ = unsafe { SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX) };
        }
        let _ = unsafe { WaitForSingleObject(p, INFINITE) };
        let mut code: u32 = 0;
        let _ = unsafe { GetExitCodeProcess(p, &mut code) };
        let _ = unsafe { CloseHandle(p) };
        unsafe { Sleep(500) };
        let shutting_down = unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) };
        if shutting_down != 0 {
            break;
        }
        if code == 0 {
            if reg::our_dword("Updating") != 0 {
                log("deploy: menunggu shell baru");
                continue;
            }
            if find_window(SHELL_CLASS).is_some() {
                continue;
            }
            start_explorer("Win12 Shell ditutup pengguna");
            open_after_switch();
            break;
        }
        log(&format!("shell keluar kode=0x{:08X}", code));
        let now = unsafe { GetTickCount() };
        if now.wrapping_sub(window_start) > 120_000 {
            restarts = 0;
            window_start = unsafe { GetTickCount() };
        }
        restarts += 1;
        if restarts > 3 {
            start_explorer("shell crash lebih dari 3 kali dalam 2 menit");
            break;
        }
        unsafe { Sleep(1500) };
    }
    if let Ok(h) = mtx {
        unsafe {
            let _ = CloseHandle(h);
        }
    }
}

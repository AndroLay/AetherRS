//! Penjaga sumber daya Win12 (padanan "systemd" di Linux): mengatur EcoQoS, prioritas memori,
//! dan prioritas GPU untuk proses latar saat banyak aplikasi terbuka. Tanpa admin: hanya proses
//! milik akun ini yang diubah; proses sistem ditolak Windows dan dilewati.
//! Port Rust dari `Pictures\win12-shell\tools\ram-governor.ps1` (angka dan aturan sama).
//! Log: Documents\optimasi-backup\governor-log.txt
//!
//! Pemakaian: win12gov.exe [status|scan|test|apply|restore|run|install|uninstall]
//!   run --tersembunyi   dipakai tugas saat logon (menyembunyikan jendela konsol)

#![allow(non_snake_case, dead_code)]

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::mem::{size_of, zeroed};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM};
use windows::Win32::System::Console::GetConsoleWindow;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{
    GetPerformanceInfo, GetProcessMemoryInfo, PERFORMANCE_INFORMATION, PROCESS_MEMORY_COUNTERS,
    PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::{
    GetProcessInformation, GetProcessTimes, OpenProcess, SetProcessInformation,
    MEMORY_PRIORITY, MEMORY_PRIORITY_INFORMATION, PROCESS_ACCESS_RIGHTS,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_STATE, ProcessMemoryPriority,
    ProcessPowerThrottling,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowThreadProcessId, IsWindowVisible,
    ShowWindow, GW_OWNER, SW_HIDE,
};

use w12::log::timestamp;

// D3DKMT berada di gdi32.dll; modul Wdk tidak diaktifkan di Cargo.toml, jadi dideklarasikan langsung.
#[link(name = "gdi32")]
extern "system" {
    fn D3DKMTSetProcessSchedulingPriorityClass(h: *mut c_void, kelas: i32) -> i32;
    fn D3DKMTGetProcessSchedulingPriorityClass(h: *mut c_void, kelas: *mut i32) -> i32;
}

const ACCESS: u32 = 0x1600; // SET_INFORMATION | QUERY_INFORMATION | QUERY_LIMITED_INFORMATION
const GB: u64 = 1024 * 1024 * 1024;
const AMBANG_TEKANAN: u64 = GB * 5 / 2; // RAM tersedia (termasuk standby) di bawah ini = tekanan
const AMBANG_DARURAT: u64 = GB * 12 / 10; // di bawah ini = darurat (hanya dicatat)
const AMBANG_JENDELA: usize = 12; // jumlah jendela terbuka yang dianggap "banyak aplikasi"
const TURUN_SETELAH: u32 = 3; // putaran tenang berturut-turut sebelum tingkat diturunkan
const DETIK_PUTARAN: u64 = 5;
const NAMA_TINGKAT: [&str; 3] = ["normal", "tekanan", "darurat"];
const NAMA_TUGAS: &str = "Andro-Penjaga-Sumber-Daya";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// Tidak pernah dibatasi (nama proses huruf kecil tanpa .exe)
const KEBAL: &[&str] = &[
    "explorer", "shellexperiencehost", "startmenuexperiencehost", "searchhost", "textinputhost",
    "applicationframehost", "sihost", "ctfmon", "audiodg", "win12shell", "suhuaman",
    "robloxplayerbeta", "minecraft.windows", "powershell", "pwsh", "conhost", "windowsterminal",
    // Game, launcher, dan anti-cheat (nama exe diperiksa di mesin ini 2026-11-18; vgc untuk Riot Vanguard bila dipasang).
    // Proses ini tidak boleh kena EcoQoS/prioritas rendah walau sedang di latar (mis. setelah Alt+Tab keluar dari game).
    "gamelaunchhelper", "robloxcrashhandler", "hyp", "hyphelper", "steam", "steamwebhelper", "prismlauncher",
    "javaw", "terraria", "wallpaper32", "wallpaper64", "vgc",
];

const TEMPLATE_TUGAS: &str = r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Penjaga sumber daya Win12 (padanan systemd): EcoQoS, prioritas memori dan GPU untuk proses latar saat banyak aplikasi terbuka. Tanpa admin.</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>__USER__</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>__USER__</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>3</Count>
    </RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>__EXE__</Command>
      <Arguments>run --tersembunyi</Arguments>
    </Exec>
  </Actions>
</Task>
"#;

// ---------- utilitas ----------

fn dir_kerja() -> PathBuf {
    let dir = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Documents")
        .join("optimasi-backup");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn catat(teks: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir_kerja().join("governor-log.txt"))
    {
        let _ = writeln!(f, "{} | {}", timestamp(), teks);
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn tugas_ada() -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", NAMA_TUGAS])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------- proses ----------

struct Proses {
    pid: u32,
    nama: String, // huruf kecil tanpa .exe, seperti ProcessName di PowerShell
}

fn daftar_proses() -> Vec<Proses> {
    let mut hasil = Vec::new();
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return hasil,
        };
        let mut pe: PROCESSENTRY32W = zeroed();
        pe.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut pe).is_ok() {
            loop {
                let panjang = pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(pe.szExeFile.len());
                let nama = String::from_utf16_lossy(&pe.szExeFile[..panjang]).to_lowercase();
                let nama = nama.strip_suffix(".exe").unwrap_or(&nama).to_string();
                hasil.push(Proses { pid: pe.th32ProcessID, nama });
                if Process32NextW(snap, &mut pe).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    hasil.retain(|p| p.pid > 4);
    hasil
}

fn nama_fokus(daftar: &[Proses]) -> String {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid as *mut u32));
    }
    daftar
        .iter()
        .find(|p| p.pid == pid)
        .map(|p| p.nama.clone())
        .unwrap_or_default()
}

fn buka(pid: u32) -> Option<HANDLE> {
    unsafe { OpenProcess(PROCESS_ACCESS_RIGHTS(ACCESS), false, pid).ok() }
}

fn tutup(h: HANDLE) {
    unsafe {
        let _ = CloseHandle(h);
    }
}

// Waktu mulai (FILETIME) dipakai sebagai bagian kunci cache, agar PID yang dipakai ulang tidak salah cocok
fn waktu_mulai(h: HANDLE) -> Option<u64> {
    let mut buat: FILETIME = unsafe { zeroed() };
    let mut sisa: FILETIME = unsafe { zeroed() };
    let mut kernel: FILETIME = unsafe { zeroed() };
    let mut user: FILETIME = unsafe { zeroed() };
    unsafe {
        GetProcessTimes(h, &mut buat, &mut sisa, &mut kernel, &mut user).ok()?;
    }
    Some(((buat.dwHighDateTime as u64) << 32) | buat.dwLowDateTime as u64)
}

// peran: "latar" = dibatasi | "aktif" = fokus/kebal, tidak dibatasi | lainnya = pulih ke default Windows
fn set_keadaan(h: HANDLE, peran: &str, tingkat: usize) -> bool {
    let (cm, sm, mem, gpu): (u32, u32, u32, i32) = match peran {
        "latar" => (1, 1, if tingkat >= 1 { 1 } else { 3 }, 1),
        "aktif" => (1, 0, 5, 2),
        _ => (0, 0, 5, 2),
    };
    let ppt = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: cm,
        StateMask: sm,
    };
    let mi = MEMORY_PRIORITY_INFORMATION { MemoryPriority: MEMORY_PRIORITY(mem) };
    unsafe {
        if SetProcessInformation(
            h,
            ProcessPowerThrottling,
            &ppt as *const _ as *const c_void,
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
        .is_err()
        {
            return false;
        }
        let _ = SetProcessInformation(
            h,
            ProcessMemoryPriority,
            &mi as *const _ as *const c_void,
            size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        );
        let _ = D3DKMTSetProcessSchedulingPriorityClass(h.0, gpu);
    }
    true
}

// Kembalikan (EcoQoS, memori, GPU) untuk tabel uji
fn baca_keadaan(h: HANDLE) -> (String, u32, String) {
    let mut ppt = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION, // wajib diisi saat membaca
        ControlMask: 0,
        StateMask: 0,
    };
    let mut mi = MEMORY_PRIORITY_INFORMATION { MemoryPriority: MEMORY_PRIORITY(0) };
    let mut g: i32 = -1;
    let (okp, rc) = unsafe {
        let okp = GetProcessInformation(
            h,
            ProcessPowerThrottling,
            &mut ppt as *mut _ as *mut c_void,
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
        .is_ok();
        let _ = GetProcessInformation(
            h,
            ProcessMemoryPriority,
            &mut mi as *mut _ as *mut c_void,
            size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        );
        (okp, D3DKMTGetProcessSchedulingPriorityClass(h.0, &mut g as *mut i32))
    };
    let eco = if okp {
        format!("{}/{}", ppt.ControlMask, ppt.StateMask)
    } else {
        "gagal".to_string()
    };
    let gpu = if rc == 0 { g.to_string() } else { format!("rc=0x{:08X}", rc as u32) };
    (eco, mi.MemoryPriority.0, gpu)
}

fn pemakaian_privat(h: HANDLE) -> u64 {
    let mut pmc: PROCESS_MEMORY_COUNTERS_EX = unsafe { zeroed() };
    let cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    let ok = unsafe {
        GetProcessMemoryInfo(h, &mut pmc as *mut _ as *mut PROCESS_MEMORY_COUNTERS, cb).is_ok()
    };
    if ok {
        pmc.PrivateUsage as u64
    } else {
        0
    }
}

// ---------- RAM dan jendela ----------

// RAM tersedia = Standby + Free + Zero (sama dengan yang dihitung Task Manager sebagai "Tersedia")
fn ram_tersedia() -> Option<u64> {
    let mut pi: PERFORMANCE_INFORMATION = unsafe { zeroed() };
    let cb = size_of::<PERFORMANCE_INFORMATION>() as u32;
    pi.cb = cb;
    unsafe {
        GetPerformanceInfo(&mut pi as *mut _, cb).ok()?;
    }
    Some(pi.PhysicalAvailable as u64 * pi.PageSize as u64)
}

unsafe extern "system" fn cb_jendela(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let tanpa_pemilik = GetWindow(hwnd, GW_OWNER).map(|o| o.0.is_null()).unwrap_or(true);
    if IsWindowVisible(hwnd).as_bool() && tanpa_pemilik {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));
        (*(lparam.0 as *mut HashSet<u32>)).insert(pid);
    }
    BOOL(1)
}

// PID (di atas 4) yang punya jendela tingkat atas yang terlihat. Pendekatan untuk MainWindowHandle.
fn jendela_terbuka() -> HashSet<u32> {
    let mut set: HashSet<u32> = HashSet::new();
    unsafe {
        let _ = EnumWindows(Some(cb_jendela), LPARAM(&mut set as *mut HashSet<u32> as isize));
    }
    set.retain(|&pid| pid > 4);
    set
}

fn tingkat_dari(tersedia: u64, jendela: usize) -> usize {
    if tersedia < AMBANG_DARURAT {
        2
    } else if tersedia < AMBANG_TEKANAN || jendela >= AMBANG_JENDELA {
        1
    } else {
        0
    }
}

// ---------- putaran ----------

struct Hasil {
    diatur: u32,
    ditolak: u32,
}

fn putaran(cache: &mut HashMap<String, String>, tingkat: usize) -> Hasil {
    let daftar = daftar_proses();
    let fokus = nama_fokus(&daftar);
    let saya = std::process::id();
    let mut baru: HashMap<String, String> = HashMap::new();
    let mut hasil = Hasil { diatur: 0, ditolak: 0 };
    for p in daftar.iter().filter(|p| p.pid != saya) {
        // Semua proses dengan nama sama dengan jendela fokus dianggap satu aplikasi (mis. proses konten Zen)
        let peran = if p.nama == fokus || KEBAL.contains(&p.nama.as_str()) { "aktif" } else { "latar" };
        let Some(h) = buka(p.pid) else {
            hasil.ditolak += 1;
            continue;
        };
        let Some(mulai) = waktu_mulai(h) else {
            tutup(h);
            continue;
        };
        let kunci = format!("{}:{}", p.pid, mulai);
        let cek = format!("{peran}|{tingkat}");
        let sama = cache.get(&kunci) == Some(&cek);
        baru.insert(kunci, cek);
        if !sama {
            if set_keadaan(h, peran, tingkat) {
                hasil.diatur += 1;
            } else {
                hasil.ditolak += 1;
            }
        }
        tutup(h);
    }
    *cache = baru;
    hasil
}

fn pulihkan_semua() -> u32 {
    let saya = std::process::id();
    let mut n = 0;
    for p in daftar_proses().iter().filter(|p| p.pid != saya) {
        if let Some(h) = buka(p.pid) {
            if set_keadaan(h, "pulih", 0) {
                n += 1;
            }
            tutup(h);
        }
    }
    n
}

// ---------- perintah ----------

fn cmd_status() {
    let Some(tersedia) = ram_tersedia() else {
        println!("RAM: GetPerformanceInfo gagal");
        return;
    };
    let jendela = jendela_terbuka().len();
    let t = tingkat_dari(tersedia, jendela);
    println!("RAM tersedia (incl. standby): {:.2} GB", tersedia as f64 / GB as f64);
    println!("Jendela terbuka            : {jendela} (ambang {AMBANG_JENDELA})");
    println!("Tingkat sekarang           : {}", NAMA_TINGKAT[t]);
    let status_tugas = if tugas_ada() { "terpasang" } else { "belum dipasang" };
    println!("Tugas saat logon           : {status_tugas}");
    if let Ok(isi) = fs::read_to_string(dir_kerja().join("governor-log.txt")) {
        let baris: Vec<&str> = isi.lines().collect();
        if !baris.is_empty() {
            println!("Log terakhir:");
            for b in &baris[baris.len().saturating_sub(5)..] {
                println!("{b}");
            }
        }
    }
}

fn cmd_scan() {
    let daftar = daftar_proses();
    let fokus = nama_fokus(&daftar);
    let jendela = jendela_terbuka();
    let mut baris: Vec<(u64, u32, String)> = Vec::new();
    for p in &daftar {
        let bytes = match buka(p.pid) {
            Some(h) => {
                let m = pemakaian_privat(h);
                tutup(h);
                m
            }
            None => 0,
        };
        baris.push((bytes, p.pid, p.nama.clone()));
    }
    baris.sort_by(|a, b| b.0.cmp(&a.0));
    println!("{:<28} {:>7} {:>7}  {:<7} {}", "Nama", "PID", "MB", "Jendela", "Peran");
    for (bytes, pid, nama) in baris.iter().take(25) {
        let peran = if *nama == fokus {
            "fokus"
        } else if KEBAL.contains(&nama.as_str()) {
            "kebal"
        } else {
            "latar"
        };
        let jw = if jendela.contains(pid) { "ya" } else { "-" };
        println!("{:<28} {:>7} {:>7}  {:<7} {}", nama, pid, bytes / (1024 * 1024), jw, peran);
    }
}

fn uji_pada(h: HANDLE) -> String {
    let mut out = String::from("Tahap   Set    EcoQoS    Memori  GPU\n");
    let awal = baca_keadaan(h);
    out += &format!("{:<7} {:<6} {:<9} {:<7} {}\n", "awal", "-", awal.0, awal.1, awal.2);
    for (tahap, peran) in [("latar", "latar"), ("aktif", "aktif"), ("pulih", "pulih")] {
        let set = set_keadaan(h, peran, 1);
        let k = baca_keadaan(h);
        out += &format!("{:<7} {:<6} {:<9} {:<7} {}\n", tahap, set, k.0, k.1, k.2);
    }
    out += "Harapan: latar 1/1, 1, 1 | aktif 1/0, 5, 2 | pulih 0/0, 5, 2 (EcoQoS, Memori, GPU)\n";
    out
}

fn cmd_test() {
    // Proses dummy milik sendiri (ping), dihentikan setelah uji
    let mut anak = match Command::new("ping")
        .args(["-n", "120", "127.0.0.1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(a) => a,
        Err(e) => {
            println!("Gagal menjalankan proses uji: {e}");
            return;
        }
    };
    sleep(Duration::from_millis(1500));
    match buka(anak.id()) {
        Some(h) => {
            print!("{}", uji_pada(h));
            tutup(h);
        }
        None => println!("OpenProcess ditolak untuk proses uji"),
    }
    let _ = anak.kill();
    let _ = anak.wait();
}

fn cmd_apply() {
    let Some(tersedia) = ram_tersedia() else {
        println!("GetPerformanceInfo gagal");
        return;
    };
    let t = tingkat_dari(tersedia, jendela_terbuka().len());
    let r = putaran(&mut HashMap::new(), t);
    println!("Tingkat: {} | diatur: {} | ditolak/dilewati: {}", NAMA_TINGKAT[t], r.diatur, r.ditolak);
}

fn cmd_restore() {
    let n = pulihkan_semua();
    catat("Restore manual");
    println!("Dikembalikan: {n} proses");
}

fn sembunyikan_konsol() {
    unsafe {
        let _ = ShowWindow(GetConsoleWindow(), SW_HIDE);
    }
}

fn cmd_run(tersembunyi: bool) {
    if tersembunyi {
        sembunyikan_konsol();
    }
    catat(&format!("Run mulai (PID {})", std::process::id()));
    let mut tingkat = 0usize;
    let mut tenang = 0u32;
    let mut galat_akhir = String::new();
    let mut cache: HashMap<String, String> = HashMap::new();
    loop {
        match ram_tersedia() {
            Some(tersedia) => {
                let jendela = jendela_terbuka().len();
                let mentah = tingkat_dari(tersedia, jendela);
                let gb = tersedia as f64 / GB as f64;
                if mentah > tingkat {
                    tingkat = mentah;
                    tenang = 0;
                    catat(&format!("tingkat naik: {} (tersedia {gb:.2} GB, jendela {jendela})", NAMA_TINGKAT[mentah]));
                } else if mentah < tingkat {
                    tenang += 1;
                    if tenang >= TURUN_SETELAH {
                        tingkat = mentah;
                        tenang = 0;
                        catat(&format!("tingkat turun: {} (tersedia {gb:.2} GB, jendela {jendela})", NAMA_TINGKAT[mentah]));
                    }
                } else {
                    tenang = 0;
                }
                putaran(&mut cache, tingkat);
            }
            None => {
                let pesan = "GetPerformanceInfo gagal";
                if galat_akhir != pesan {
                    catat(&format!("galat: {pesan}"));
                    galat_akhir = pesan.to_string();
                }
            }
        }
        sleep(Duration::from_secs(DETIK_PUTARAN));
    }
}

fn cmd_install() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let user = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default()
    );
    let xml = TEMPLATE_TUGAS
        .replace("__USER__", &xml_escape(&user))
        .replace("__EXE__", &xml_escape(&exe.to_string_lossy()));
    // schtasks /XML menerima UTF-16 LE dengan BOM
    let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let berkas = dir_kerja().join("governor-task.xml");
    fs::write(&berkas, &bytes).map_err(|e| e.to_string())?;
    let out = Command::new("schtasks")
        .args(["/Create", "/TN", NAMA_TUGAS, "/XML"])
        .arg(&berkas)
        .arg("/F")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "schtasks kode {:?}: {}{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    catat(&format!("Install: tugas {NAMA_TUGAS} didaftarkan (akun {})", std::env::var("USERNAME").unwrap_or_default()));
    println!("Tugas {NAMA_TUGAS} didaftarkan. Berjalan otomatis saat logon berikutnya.");
    println!("Untuk sekarang: schtasks /Run /TN {NAMA_TUGAS}");
    Ok(())
}

fn cmd_uninstall() {
    if tugas_ada() {
        let _ = Command::new("schtasks")
            .args(["/Delete", "/TN", NAMA_TUGAS, "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    let saya = std::process::id();
    for p in daftar_proses().iter().filter(|p| p.nama == "win12gov" && p.pid != saya) {
        let pid_teks = p.pid.to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", pid_teks.as_str(), "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    let n = pulihkan_semua();
    catat(&format!("Uninstall: tugas dihapus, loop dihentikan, {n} proses dikembalikan"));
    println!("Tugas dihapus, loop dihentikan, {n} proses dikembalikan ke default.");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let perintah = args.first().map(|s| s.as_str()).unwrap_or("status");
    let tersembunyi = args.iter().any(|a| a == "--tersembunyi");
    match perintah {
        "status" => cmd_status(),
        "scan" => cmd_scan(),
        "test" => cmd_test(),
        "apply" => cmd_apply(),
        "restore" => cmd_restore(),
        "run" => cmd_run(tersembunyi),
        "install" => {
            if let Err(e) = cmd_install() {
                println!("Gagal memasang tugas: {e}");
            }
        }
        "uninstall" => cmd_uninstall(),
        _ => println!("Pemakaian: win12gov [status|scan|test|apply|restore|run|install|uninstall]"),
    }
}

//! Menu konteks aplikasi Start (2026-11-15): perintah Win12 di atas, lalu "Opsi lainnya" berisi menu shell asli.
//!
//! Perintah: Buka, Jalankan sebagai administrator, Sematkan/Lepas dari Start, Sematkan/Lepas dari taskbar,
//! Buka lokasi file, Salin jalur, Pengaturan aplikasi, Copot pemasangan. Verba shell yang sama (runas,
//! opencontainer, uninstall) dipakai bila tersedia. Verba pin milik Explorer dibuang dari submenu karena
//! menyematkan ke taskbar/Start Explorer, bukan Win12.

use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
    KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_SAM_FLAGS, RRF_RT_REG_SZ,
};
use windows::Win32::UI::Shell::{IContextMenu, ShellExecuteW, GCS_VERBW};

use super::prelude::*;

pub const ID_OPEN: u32 = 0x9001;
pub const ID_ADMIN: u32 = 0x9002;
pub const ID_PIN_START: u32 = 0x9003;
pub const ID_PIN_TASK: u32 = 0x9004;
pub const ID_LOC: u32 = 0x9005;
pub const ID_COPY: u32 = 0x9006;
pub const ID_SETTINGS: u32 = 0x9007;
pub const ID_UNINST: u32 = 0x9008;

/// Id perintah shell (1-based, seperti dikembalikan TrackPopupMenuEx) untuk verba yang dipakai ulang. 0 = tidak ada.
#[derive(Default, Clone, Copy)]
pub struct ShellVerbs {
    pub runas: u32,
    pub location: u32,
    pub uninstall: u32,
}

/// Info aplikasi untuk menu.
pub struct CtxApp {
    pub ident: String,
    pub name: String,
    /// Jalur exe/target (kosong untuk app Store).
    pub exe: String,
    /// App Store/UWP (AUMID berisi '!').
    pub packaged: bool,
    pub is_file: bool,
}

pub fn ctx_app(parsing: &str, file_hit: bool) -> CtxApp {
    let ident = ident_from_parsing(parsing);
    let packaged = ident.contains('!') && !ident.contains('\\');
    let mut name = String::new();
    let mut exe = String::new();
    if file_hit {
        exe = parsing.to_string();
        name = parsing.rsplit('\\').next().unwrap_or(parsing).to_string();
    } else if let Some(i) = app_index_by_ident(&ident) {
        let apps = APPS.borrow();
        name = apps[i].name.clone();
        exe = apps[i].exe.clone();
    } else if let Some(mut a) = resolve_entry(&ident) {
        name = a.name.clone();
        exe = a.exe.clone();
        icon_free(&mut a.icon);
    }
    if exe.is_empty() && (ident.contains('\\') || ident.contains('/')) {
        exe = ident.clone();
    }
    CtxApp { ident, name, exe, packaged, is_file: file_hit }
}

fn verb_of(cm: &IContextMenu, id: u32) -> String {
    let mut buf = [0u16; 128];
    // SAFETY: GCS_VERBW menulis string UTF-16 ke buf (pszname bertipe PSTR untuk kedua varian).
    let ok = unsafe {
        cm.GetCommandString(
            (id - 1) as usize,
            GCS_VERBW,
            None,
            windows::core::PSTR(buf.as_mut_ptr() as *mut u8),
            buf.len() as u32,
        )
    }
    .is_ok();
    if !ok {
        return String::new();
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n]).to_ascii_lowercase()
}

/// Catat verba yang dipakai ulang lalu buang verba pin Explorer dari `sub`.
pub fn scan_shell_menu(sub: HMENU, cm: &IContextMenu) -> ShellVerbs {
    let mut sv = ShellVerbs::default();
    // SAFETY: sub adalah menu valid yang diisi QueryContextMenu.
    let n = unsafe { GetMenuItemCount(Some(sub)) };
    let mut i = n - 1;
    while i >= 0 {
        // SAFETY: indeks dalam rentang.
        let id = unsafe { GetMenuItemID(sub, i) };
        if id != u32::MAX && id >= 1 && id <= 0x7FFF {
            let v = verb_of(cm, id);
            match v.as_str() {
                "runas" => sv.runas = id,
                "opencontainer" | "openfilelocation" => sv.location = id,
                "uninstall" => sv.uninstall = id,
                "pintohome" | "taskbarpin" | "taskbarunpin" | "pintostartscreen" | "unpinfromstartscreen" | "pintostart"
                | "unpinfromstart" => {
                    // SAFETY: hapus berdasarkan posisi.
                    let _ = unsafe { DeleteMenu(sub, i as u32, MF_BYPOSITION) };
                }
                _ => {}
            }
        }
        i -= 1;
    }
    sv
}

fn add(m: HMENU, id: u32, label: &str, enabled: bool) {
    let w = to_wide_z(label);
    let f = if enabled { MF_STRING } else { MF_STRING | MF_GRAYED };
    // SAFETY: w NUL-terminated selama pemanggilan.
    let _ = unsafe { AppendMenuW(m, f, id as usize, pcw(&w)) };
}

fn sep(m: HMENU) {
    // SAFETY: separator tanpa teks.
    let _ = unsafe { AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null()) };
}

/// Isi menu utama. `sub` (bila ada) dipasang sebagai "Opsi lainnya".
pub fn build_menu(m: HMENU, app: &CtxApp, sv: &ShellVerbs, sub: Option<HMENU>) {
    add(m, ID_OPEN, "Buka", true);
    if app.is_file {
        add(m, ID_LOC, "Buka lokasi file", true);
        add(m, ID_COPY, "Salin jalur", true);
    } else {
        let can_admin = sv.runas != 0 || (!app.packaged && !app.exe.is_empty());
        add(m, ID_ADMIN, "Jalankan sebagai administrator", can_admin);
        sep(m);
        let (in_start, in_task) = ident_pin_state(&app.ident);
        add(m, ID_PIN_START, if in_start { "Lepas dari Start" } else { "Sematkan ke Start" }, true);
        add(m, ID_PIN_TASK, if in_task { "Lepas dari taskbar" } else { "Sematkan ke taskbar" }, true);
        sep(m);
        let can_loc = sv.location != 0 || !app.exe.is_empty();
        add(m, ID_LOC, "Buka lokasi file", can_loc);
        add(m, ID_COPY, "Salin jalur", !app.exe.is_empty() || app.packaged);
        add(m, ID_SETTINGS, "Pengaturan aplikasi", true);
        add(m, ID_UNINST, "Copot pemasangan", true);
    }
    if let Some(s) = sub {
        // SAFETY: s menjadi milik m setelah AppendMenuW (dihancurkan bersama m).
        if unsafe { GetMenuItemCount(Some(s)) } > 0 {
            sep(m);
            let _ = unsafe { AppendMenuW(m, MF_POPUP, s.0 as usize, w!("Opsi lainnya")) };
        }
    }
}

fn exec(verb: &str, file: &str, params: &str) -> bool {
    let wv = to_wide_z(verb);
    let wf = to_wide_z(file);
    let wp = to_wide_z(params);
    // SAFETY: semua buffer hidup sampai ShellExecuteW kembali.
    let h = unsafe {
        ShellExecuteW(
            None,
            pcw(&wv),
            pcw(&wf),
            if params.is_empty() { PCWSTR::null() } else { pcw(&wp) },
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    h.0 as isize > 32
}

/// Jalankan perintah Win12. Mengembalikan false bila `cmd` bukan milik menu ini.
/// `invoke_shell(id)` menjalankan perintah shell (id 1-based) lewat IContextMenu milik pemanggil.
pub fn run(cmd: u32, app: &CtxApp, parsing: &str, sv: &ShellVerbs, invoke_shell: &mut dyn FnMut(u32)) -> bool {
    match cmd {
        ID_OPEN => {
            panel_close();
            pan_uri(parsing);
        }
        ID_ADMIN => {
            panel_close();
            if sv.runas != 0 {
                invoke_shell(sv.runas);
            } else if !app.exe.is_empty() {
                exec("runas", &app.exe, "");
            }
        }
        ID_PIN_START | ID_PIN_TASK => {
            if cmd == ID_PIN_START {
                start_pin_toggle_ident(&app.ident);
            } else {
                taskbar_pin_toggle_ident(&app.ident);
            }
            if !PAN.get().0.is_null() {
                let _ = w12_invalidate(PAN.get(), None, false);
            }
        }
        ID_LOC => {
            panel_close();
            if sv.location != 0 && !app.is_file {
                invoke_shell(sv.location);
            } else if !app.exe.is_empty() {
                let dir = app.exe.rsplit_once('\\').map(|(d, _)| d.to_string()).unwrap_or_default();
                if !dir.is_empty() {
                    open_folder_window(&dir);
                }
            }
        }
        ID_COPY => {
            let text = if app.exe.is_empty() { app.ident.clone() } else { app.exe.clone() };
            copy_text_clip(&text);
        }
        ID_SETTINGS => {
            panel_close();
            pan_uri("ms-settings:appsfeatures");
        }
        ID_UNINST => {
            panel_close();
            uninstall(app, sv, invoke_shell);
        }
        _ => return false,
    }
    true
}

fn uninstall(app: &CtxApp, sv: &ShellVerbs, invoke_shell: &mut dyn FnMut(u32)) {
    if sv.uninstall != 0 {
        invoke_shell(sv.uninstall);
        return;
    }
    let found = if app.packaged || app.name.is_empty() { None } else { find_uninstall(&app.name, &app.exe) };
    let Some(cmdline) = found else {
        // App Store atau tidak ada entri pencopot: halaman Aplikasi terpasang di Pengaturan.
        pan_uri("ms-settings:appsfeatures");
        return;
    };
    let msg = to_wide_z(&format!(
        "Copot pemasangan \"{}\"?\n\nPencopot milik aplikasi akan dijalankan.",
        app.name
    ));
    // SAFETY: msg hidup selama pemanggilan.
    let r = unsafe { MessageBoxW(None, pcw(&msg), w!("Win12"), MB_YESNO | MB_ICONQUESTION | MB_TOPMOST) };
    if r != IDYES {
        return;
    }
    let (file, params) = split_cmdline(&cmdline);
    log(&format!("start: copot pemasangan {}", app.name));
    if !exec("open", &file, &params) {
        pan_uri("ms-settings:appsfeatures");
    }
}

/// Pisahkan baris perintah UninstallString menjadi berkas dan argumen.
fn split_cmdline(s: &str) -> (String, String) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return (rest[..end].to_string(), rest[end + 1..].trim().to_string());
        }
    }
    let low = s.to_ascii_lowercase();
    if let Some(p) = low.find(".exe") {
        let e = p + 4;
        return (s[..e].to_string(), s[e..].trim().to_string());
    }
    match s.split_once(' ') {
        Some((a, b)) => (a.to_string(), b.trim().to_string()),
        None => (s.to_string(), String::new()),
    }
}

fn reg_str(k: HKEY, sub: &str, name: &str) -> Option<String> {
    let ws = to_wide_z(sub);
    let wn = to_wide_z(name);
    let mut buf = vec![0u16; 1024];
    let mut cb = (buf.len() * 2) as u32;
    // SAFETY: buffer lokal dengan ukuran byte yang dikirim.
    let rc = unsafe {
        RegGetValueW(k, pcw(&ws), pcw(&wn), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut cb))
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let n = (cb as usize / 2).min(buf.len());
    let end = buf[..n].iter().position(|&c| c == 0).unwrap_or(n);
    let v = String::from_utf16_lossy(&buf[..end]);
    if v.trim().is_empty() {
        None
    } else {
        Some(v)
    }
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Cari UninstallString di kunci Uninstall (HKLM 64/32-bit dan HKCU): DisplayName sama dengan nama aplikasi,
/// atau InstallLocation berisi folder exe.
fn find_uninstall(name: &str, exe: &str) -> Option<String> {
    let want = norm(name);
    let exe_dir = exe.rsplit_once('\\').map(|(d, _)| d.to_ascii_lowercase()).unwrap_or_default();
    let roots: [(HKEY, REG_SAM_FLAGS); 3] = [
        (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
        (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
        (HKEY_CURRENT_USER, REG_SAM_FLAGS(0)),
    ];
    let base = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    let wb = to_wide_z(base);
    let mut prefix_hit: Option<String> = None;
    for (root, wow) in roots {
        let mut k = HKEY::default();
        // SAFETY: k ditutup di akhir iterasi.
        if unsafe { RegOpenKeyExW(root, pcw(&wb), Some(0), KEY_READ | wow, &mut k) } != ERROR_SUCCESS {
            continue;
        }
        let mut i = 0u32;
        loop {
            let mut nb = [0u16; 260];
            let mut nl = nb.len() as u32;
            // SAFETY: buffer nama lokal.
            let rc = unsafe { RegEnumKeyExW(k, i, Some(PWSTR(nb.as_mut_ptr())), &mut nl, None, None, None, None) };
            if rc != ERROR_SUCCESS {
                break;
            }
            i += 1;
            let sub = String::from_utf16_lossy(&nb[..nl as usize]);
            let Some(dn) = reg_str(k, &sub, "DisplayName") else { continue };
            let Some(un) = reg_str(k, &sub, "UninstallString") else { continue };
            let d = norm(&dn);
            if d == want {
                // SAFETY: k dibuka di atas.
                unsafe {
                    let _ = RegCloseKey(k);
                }
                return Some(un);
            }
            if prefix_hit.is_none() && want.len() >= 4 && d.starts_with(&want) {
                prefix_hit = Some(un.clone());
            }
            if prefix_hit.is_none() && !exe_dir.is_empty() {
                if let Some(loc) = reg_str(k, &sub, "InstallLocation") {
                    let l = loc.trim_matches('"').trim_end_matches('\\').to_ascii_lowercase();
                    if l.len() > 3 && exe_dir.starts_with(&l) {
                        prefix_hit = Some(un.clone());
                    }
                }
            }
        }
        // SAFETY: k dibuka di atas.
        unsafe {
            let _ = RegCloseKey(k);
        }
    }
    prefix_hit
}

/// Salin teks ke papan klip (CF_UNICODETEXT).
fn copy_text_clip(text: &str) {
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    let w = to_wide_z(text);
    // SAFETY: memori global diisi lalu diserahkan ke papan klip; bila gagal, papan klip tetap ditutup.
    unsafe {
        if OpenClipboard(None).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2) {
            let p = GlobalLock(h) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
                let _ = GlobalUnlock(h);
                let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(h.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

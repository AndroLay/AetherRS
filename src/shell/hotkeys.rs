//! Sumber: w12hotkeys.inc baris 1–694. Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H10.
//! Deviasi: ShellOpen(target,args) menjadi shell_open(&str) satu argumen (varian dengan args: shell_open_with); GetProcAddress appmodel diganti pemanggilan langsung Appx.
use super::prelude::*;
use std::ffi::c_void;
// PORT: dibutuhkan oleh expansion #[windows_core::interface] di IW12DesktopAppXActivator.
use windows::core::IUnknown_Vtbl;
use windows::Win32::Storage::Packaging::Appx::{FindPackagesByPackageFamily, GetPackagePathByFullName};
use windows::Win32::Storage::FileSystem::{DeleteFileW, GetFileAttributesW, INVALID_FILE_ATTRIBUTES};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IBindCtx, IStream, CLSCTX, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Com::StructuredStorage::IPropertyBag2;
use windows::Win32::System::Threading::{CreateThread, GetProcessId, OpenMutexW, SYNCHRONIZATION_ACCESS_RIGHTS, THREAD_CREATION_FLAGS};
use windows::Win32::UI::Shell::{AssocQueryStringW, ApplicationActivationManager, IApplicationActivationManager, IShellItem, IShellItemArray, SHCreateItemFromParsingName, SHCreateShellItemArrayFromShellItem, SHGetKnownFolderPath, ShellExecuteW, AO_NONE, ASSOCF_IS_PROTOCOL, ASSOCSTR_APPID, FOLDERID_Screenshots, KF_FLAG_CREATE, SIGDN_NORMALDISPLAY};
use windows::Win32::Graphics::Dwm::{DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties, DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_RECTSOURCE, DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount};
use windows::Win32::Media::Audio::{eCapture, eCommunications, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Graphics::Imaging::{GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA, IWICBitmapFrameEncode, IWICImagingFactory, WICBitmapEncoderNoCache};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_DIB;

/// Pesan singkat di notch (HkToast). glyph: kode ikon Segoe MDL2 (mis. 0xE946).
pub fn hk_toast(msg: &str, glyph: u16) {
    if super::notch::NT_WND.get().0.is_null() {
        return;
    }
    // Buffer C 96 wchar_t: maksimal 95 karakter.
    *NT_MSG.borrow_mut() = msg.chars().take(95).collect();
    NT_MSG_WARN.set(false);
    NT_MSG_GLYPH.set(glyph);
    NT_HUD_KIND.set(HUD_MSG);
    if NT_STATE.get() == NT_OPEN {
        // Jendela notch berlapis: repaint lewat w12_invalidate (InvalidateRect mentah tidak merender).
        let _ = super::core_c1::w12_invalidate(super::notch::NT_WND.get(), None, false);
    } else {
        nt_show_temp(NT_HUD, 2600);
    }
}

// ---------------------------------------------------------------- app paket tanpa Explorer

/// CLSID DesktopAppXActivator (in-proc twinui.appcore.dll). Sama dengan kClsidDaxa di C.
const CLSID_DAXA: GUID = GUID::from_u128(0x168eb462_775f_42ae_9111_d714b2306c2e);

/// Antarmuka DesktopAppXActivator. IID f158268a-d5a5-45ce-99cf-00d6c3f3fc0a sama dengan kIidDaxa di C.
#[windows_core::interface("f158268a-d5a5-45ce-99cf-00d6c3f3fc0a")]
unsafe trait IW12DesktopAppXActivator: IUnknown {
    unsafe fn Activate(&self, aumid: PCWSTR, package_relative_exe: PCWSTR, args: PCWSTR, proc_handle: *mut HANDLE) -> HRESULT;
}

/// Mencari needle (non-kosong) di dalam haystack byte.
fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Nilai atribut `name="..."` (atau '...') dalam tag. Nama harus diawali spasi/tab/CR/LF,
/// sehingga "Id" tidak cocok dengan "uap10:Id".
fn hk_attr(tag: &[u8], name: &str) -> Option<String> {
    let nb = name.as_bytes();
    let n = nb.len();
    let mut i = 1;
    while i + n + 2 < tag.len() {
        let ws = matches!(tag[i - 1], b' ' | b'\t' | b'\r' | b'\n');
        if ws && &tag[i..i + n] == nb && tag[i + n] == b'=' && matches!(tag[i + n + 1], b'"' | b'\'') {
            let q = tag[i + n + 1];
            let start = i + n + 2;
            let end = tag[start..].iter().position(|&c| c == q).map_or(tag.len(), |k| start + k);
            return Some(String::from_utf8_lossy(&tag[start..end]).into_owned());
        }
        i += 1;
    }
    None
}

/// Tag mengandung needle (mis. `TrustLevel="mediumIL"`).
fn hk_has(tag: &[u8], needle: &str) -> bool {
    tag.windows(needle.len()).any(|w| w == needle.as_bytes())
}

/// String wide sampai NUL pertama.
fn wide_until_nul(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

/// Executable (relatif paket) dari `<Application Id="app_id">` di AppxManifest.xml (HkManifestApp).
/// Hanya untuk app full trust; None bila bukan full trust atau manifest tidak terbaca.
fn hk_manifest_entry(dir: &str, app_id: &str) -> Option<(bool, String)> {
    let text = std::fs::read(format!("{dir}\\AppxManifest.xml")).ok()?;
    if text.is_empty() || text.len() >= 4 * 1024 * 1024 {
        return None;
    }
    let mut pos = 0;
    while let Some(off) = find_bytes(&text[pos..], b"<Application") {
        let p = pos + off;
        pos = p + 1;
        // Lewati <Applications>: karakter setelah nama harus spasi.
        if !matches!(text.get(p + 12).copied(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            continue;
        }
        let Some(end_off) = text[p..].iter().position(|&c| c == b'>') else {
            break;
        };
        let tag = &text[p..p + end_off];
        let id = hk_attr(tag, "Id").unwrap_or_default();
        if !id.eq_ignore_ascii_case(app_id) {
            continue;
        }
        let ex = hk_attr(tag, "Executable").unwrap_or_default();
        let ep = hk_attr(tag, "EntryPoint").unwrap_or_default();
        let full_trust = ep == "Windows.FullTrustApplication"
            || hk_has(tag, "TrustLevel=\"mediumIL\"")
            || hk_has(tag, "RuntimeBehavior=\"packagedClassicApp\"")
            || hk_has(tag, "RuntimeBehavior=\"win32App\"");
        return Some((full_trust, ex));
    }
    None
}

/// Executable app full trust dari manifest (None = bukan full trust atau tidak ditemukan).
fn hk_manifest_app(dir: &str, app_id: &str) -> Option<String> {
    match hk_manifest_entry(dir, app_id) {
        Some((true, ex)) if !ex.is_empty() => Some(ex),
        _ => None,
    }
}

/// App paket UWP murni (CoreWindow, bukan full trust). App seperti ini tidak bisa tampil saat Explorer bukan
/// shell (uji 2026-11-18: Kamera, Kalkulator, Pengaturan -> 0x80040900 setelah 45 dtk).
fn hk_is_uwp(aumid: &str) -> bool {
    let Some(bang) = aumid.find('!') else {
        return false;
    };
    let Some(dir) = hk_package_dir(&aumid[..bang]) else {
        return false;
    };
    matches!(hk_manifest_entry(&dir, &aumid[bang + 1..]), Some((false, _)))
}

/// Paket utama keluarga pfn -> folder instalasi -> Executable app (HkPackagedExe).
fn hk_packaged_exe(pfn: &str, app_id: &str) -> Option<String> {
    hk_manifest_app(&hk_package_dir(pfn)?, app_id)
}

/// Folder instalasi paket utama keluarga `pfn`.
fn hk_package_dir(pfn: &str) -> Option<String> {
    // PACKAGE_FILTER_HEAD | PACKAGE_FILTER_DIRECT
    const FILTER: u32 = 0x10 | 0x20;
    let pw = to_wide_z(pfn);
    let (mut count, mut len) = (0u32, 0u32);
    // SAFETY: panggilan pertama hanya menghitung ukuran; pointer keluaran NULL.
    let r = unsafe { FindPackagesByPackageFamily(pcw(&pw), FILTER, &mut count, None, &mut len, None, None) };
    if r != ERROR_INSUFFICIENT_BUFFER || count == 0 || len == 0 {
        return None;
    }
    let mut names: Vec<PWSTR> = vec![PWSTR(std::ptr::null_mut()); count as usize];
    let mut buf: Vec<u16> = vec![0; len as usize];
    let mut props: Vec<u32> = vec![0; count as usize];
    // SAFETY: names, buf, dan props berukuran sesuai count/len dari panggilan pertama.
    let r = unsafe {
        FindPackagesByPackageFamily(
            pcw(&pw),
            FILTER,
            &mut count,
            Some(names.as_mut_ptr()),
            &mut len,
            Some(PWSTR(buf.as_mut_ptr())),
            Some(props.as_mut_ptr()),
        )
    };
    if r != ERROR_SUCCESS || count == 0 {
        return None;
    }
    let mut dir = vec![0u16; MAX_PATH as usize];
    let mut n = MAX_PATH;
    // SAFETY: names[0] menunjuk ke buf yang masih hidup; dir berukuran MAX_PATH.
    let r = unsafe { GetPackagePathByFullName(PCWSTR(names[0].0 as *const u16), &mut n, Some(PWSTR(dir.as_mut_ptr()))) };
    if r != ERROR_SUCCESS {
        return None;
    }
    Some(wide_until_nul(&dir))
}

/// Job peluncuran app paket: aumid "pfn!appid", args, dan target asli (untuk fallback UI).
/// proto true = ActivateForProtocol dengan URI di `args` (ms-settings:<page>); false = ActivateApplication.
struct HkLaunchJob {
    aumid: String,
    args: String,
    orig: String,
    proto: bool,
}

/// Kirim WM_W12_NEEDEXP ke jendela shell. LPARAM = Box<String> mentah; penerima wajib memanggil
/// `hk_need_explorer(*Box::from_raw(ptr))`. Kontrak ini perubahan terhadap C (C: wchar_t* dari _wcsdup).
fn hk_post_needexp(target: &str) {
    hk_post_needexp_kind(target, 0);
}

/// `kind` 0 = dialog "beralih ke Windows biasa", 1 = Explorer sementara tanpa dialog (hk_temp_explorer).
fn hk_post_needexp_kind(target: &str, kind: usize) {
    let raw = Box::into_raw(Box::new(target.to_string()));
    // SAFETY: PostMessageW hanya menyalin nilai LPARAM; kepemilikan pindah ke penerima pesan.
    let posted = unsafe { PostMessageW(Some(MSG.get()), WM_W12_NEEDEXP, WPARAM(kind), LPARAM(raw as isize)) };
    if posted.is_err() {
        // SAFETY: pesan gagal dikirim, jadi raw tidak pernah sampai ke penerima.
        drop(unsafe { Box::from_raw(raw) });
    }
}

/// Aktivasi app paket. true bila berhasil.
fn hk_activate(job: &HkLaunchJob) -> bool {
    if job.proto {
        return hk_activate_mgr(&job.aumid, &job.args, true);
    }
    let Some(bang) = job.aumid.find('!') else {
        return false;
    };
    // 1) Paket full trust: DesktopAppXActivator (jalur lama).
    if let Some(exe) = hk_packaged_exe(&job.aumid[..bang], &job.aumid[bang + 1..]) {
        if let Some(ok) = hk_activate_daxa(job, &exe) {
            if ok {
                return true;
            }
            log(&format!("aktivasi: DesktopAppxActivator gagal untuk {}, coba ApplicationActivationManager", job.aumid));
        }
    }
    // 2) UWP murni (tanpa exe full trust) atau DesktopAppXActivator tidak tersedia: ApplicationActivationManager.
    hk_activate_mgr(&job.aumid, &job.args, false)
}

/// CLSID_DesktopAppxActivator untuk paket full trust. None = objek COM tidak tersedia (belum ada yang diluncurkan).
fn hk_activate_daxa(job: &HkLaunchJob, exe: &str) -> Option<bool> {
    let act: windows::core::Result<IW12DesktopAppXActivator> =
        // SAFETY: CLSID konstan; objek COM dilepas otomatis saat keluar dari scope.
        unsafe { CoCreateInstance(&CLSID_DAXA, None::<&IUnknown>, CLSCTX_INPROC_SERVER) };
    let act = act.ok()?;
    let aumid_w = to_wide_z(&job.aumid);
    let exe_w = to_wide_z(exe);
    let args_w = to_wide_z(&job.args);
    let mut h = HANDLE(std::ptr::null_mut());
    // SAFETY: string wide hidup selama pemanggilan; h adalah parameter keluaran.
    let hr = unsafe { act.Activate(pcw(&aumid_w), pcw(&exe_w), pcw(&args_w), &mut h) };
    if !h.0.is_null() {
        // SAFETY: h adalah handle proses yang dikembalikan Activate; ditutup setelah dipakai.
        unsafe {
            let _ = AllowSetForegroundWindow(GetProcessId(h));
            let _ = CloseHandle(h);
        }
    }
    Some(hr.is_ok())
}

/// ApplicationActivationManager (CLSID di twinui.appcore.dll, in-proc, bukan Explorer). Dipakai untuk app UWP murni.
// PORT: jalur baru (2026-11-14), belum diuji runtime; uji dengan Explorer tidak berjalan.
// proto true = ActivateForProtocol dengan URI sebagai item shell (ms-settings:<page>); false = ActivateApplication dengan args.
fn hk_activate_mgr(aumid: &str, arg: &str, proto: bool) -> bool {
    // 2026-11-18: Kamera (UWP murni) gagal dibuka di mode shell. Contoh resmi memakai CLSCTX_LOCAL_SERVER;
    // jalur in-proc lama dicoba dulu (paritas), lalu LOCAL_SERVER. Setiap kegagalan dicatat dengan HRESULT.
    for (ctx, name) in [(CLSCTX_INPROC_SERVER, "inproc"), (CLSCTX_LOCAL_SERVER, "local")] {
        match hk_activate_mgr_ctx(aumid, arg, proto, ctx) {
            Ok(()) => return true,
            Err(e) => log(&format!("aktivasi {name} gagal untuk {aumid}: {e}")),
        }
    }
    false
}

fn hk_activate_mgr_ctx(aumid: &str, arg: &str, proto: bool, ctx: CLSCTX) -> Result<(), String> {
    let mgr: windows::core::Result<IApplicationActivationManager> =
        // SAFETY: CLSID konstan; objek COM dilepas otomatis saat keluar dari scope.
        unsafe { CoCreateInstance(&ApplicationActivationManager, None::<&IUnknown>, ctx) };
    let mgr = mgr.map_err(|e| format!("CoCreateInstance 0x{:08X}", e.code().0 as u32))?;
    let aumid_w = to_wide_z(aumid);
    let pid: windows::core::Result<u32> = if proto {
        let uri_w = to_wide_z(arg);
        let item: windows::core::Result<IShellItem> =
            // SAFETY: uri_w berakhir NUL dan hidup selama pemanggilan; bind ctx NULL.
            unsafe { SHCreateItemFromParsingName(pcw(&uri_w), None::<&IBindCtx>) };
        let item = item.map_err(|e| format!("SHCreateItemFromParsingName 0x{:08X}", e.code().0 as u32))?;
        let arr: windows::core::Result<IShellItemArray> =
            // SAFETY: item hidup selama pemanggilan.
            unsafe { SHCreateShellItemArrayFromShellItem(&item) };
        let arr = arr.map_err(|e| format!("SHCreateShellItemArrayFromShellItem 0x{:08X}", e.code().0 as u32))?;
        // SAFETY: aumid_w berakhir NUL dan hidup selama pemanggilan; arr hidup selama pemanggilan.
        unsafe { mgr.ActivateForProtocol(pcw(&aumid_w), &arr) }
    } else {
        let args_w = to_wide_z(arg);
        // SAFETY: aumid_w dan args_w berakhir NUL dan hidup selama pemanggilan.
        unsafe { mgr.ActivateApplication(pcw(&aumid_w), pcw(&args_w), AO_NONE) }
    };
    match pid {
        Ok(pid) => {
            if pid != 0 {
                // SAFETY: AllowSetForegroundWindow hanya mengubah izin fokus untuk pid tersebut.
                let _ = unsafe { AllowSetForegroundWindow(pid) };
            }
            Ok(())
        }
        Err(e) => Err(format!("{} 0x{:08X}", if proto { "ActivateForProtocol" } else { "ActivateApplication" }, e.code().0 as u32)),
    }
}

/// Isi job di thread peluncur: CoInitializeEx MTA, aktivasi, lalu CoUninitialize.
fn hk_launch_job(job: &HkLaunchJob) -> bool {
    // SAFETY: CoInitializeEx MTA dipasangkan dengan CoUninitialize di bawah, di thread ini.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let ok = hk_activate(job);
    // SAFETY: pasangan dari CoInitializeEx di atas.
    unsafe { CoUninitialize() };
    ok
}

/// Thread peluncur (HkLaunchThread). Pemilik tunggal job; gagal -> UI menawarkan beralih ke Windows biasa.
unsafe extern "system" fn hk_launch_thread(p: *mut c_void) -> u32 {
    // SAFETY: p berasal dari Box::into_raw di hk_launch_packaged; thread ini pemilik tunggalnya.
    let job = unsafe { Box::from_raw(p as *mut HkLaunchJob) };
    // Mode shell + app UWP (termasuk Pengaturan lewat protokol): langsung minta Explorer sementara. Aktivasi
    // pasti gagal setelah ~45 dtk tanpa Explorer sebagai shell, jadi tidak dicoba.
    if SHELL_MODE.get() && (job.proto || hk_is_uwp(&job.aumid)) {
        log(&format!("app UWP {} di mode shell: minta Explorer sementara", job.aumid));
        hk_post_needexp_kind(&job.orig, 1);
        return 0;
    }
    if !hk_launch_job(&job) {
        hk_post_needexp(&job.orig);
    }
    0
}

/// Jalankan app paket di thread terpisah (HkLaunchPackaged). Gagal membuat thread -> job dibebaskan.
fn hk_launch_packaged(aumid: &str, args: &str, orig: &str) {
    hk_spawn_job(HkLaunchJob {
        aumid: aumid.to_string(),
        args: args.to_string(),
        orig: orig.to_string(),
        proto: false,
    });
}

/// Buka `ms-settings:<page>` lewat ActivateForProtocol ke AUMID Pengaturan (tanpa padanan Win32).
/// Gagal -> hk_launch_thread memanggil hk_post_needexp(uri) seperti biasa.
fn hk_launch_settings_uri(uri: &str) {
    hk_spawn_job(HkLaunchJob {
        aumid: HK_SETTINGS_AUMID.to_string(),
        args: uri.to_string(),
        orig: uri.to_string(),
        proto: true,
    });
}

/// Serahkan job ke thread peluncur baru. Gagal membuat thread -> job dibebaskan.
fn hk_spawn_job(job: HkLaunchJob) {
    let raw = Box::into_raw(Box::new(job));
    // SAFETY: parameter thread adalah pointer job; kepemilikan pindah ke thread, atau dibebaskan bila gagal.
    match unsafe {
        CreateThread(
            None,
            0,
            Some(hk_launch_thread),
            Some(raw as *const c_void),
            THREAD_CREATION_FLAGS(0),
            None,
        )
    } {
        Ok(h) => {
            // SAFETY: h adalah handle thread yang baru dibuat; tidak perlu ditunggu.
            let _ = unsafe { CloseHandle(h) };
        }
        Err(_) => {
            // SAFETY: thread tidak jadi dibuat, jadi raw tidak dipakai siapa pun.
            drop(unsafe { Box::from_raw(raw) });
        }
    }
}

// ---------------------------------------------------------------- halaman Pengaturan pengganti

/// Padanan Win32 halaman Pengaturan (HkSettingsAlt): (page, prefix, berkas, argumen).
/// prefix true = page cukup diawali nama ini; false = harus sama persis. Pencocokan tanpa membedakan huruf.
const HK_SETTINGS_ALT: [(&str, bool, &str, Option<&str>); 9] = [
    ("network", true, "ncpa.cpl", None),
    ("powersleep", false, "powercfg.cpl", None),
    ("batterysaver", true, "powercfg.cpl", None),
    ("about", false, "msinfo32.exe", None),
    ("appsfeatures", false, "appwiz.cpl", None),
    ("sound", false, "mmsys.cpl", None),
    ("dateandtime", false, "timedate.cpl", None),
    ("regionlanguage", false, "intl.cpl", None),
    ("startupapps", false, "taskmgr.exe", Some("/0 /startup")),
];

/// AUMID Keamanan Windows (app paket SecHealthUI). Diverifikasi dengan Get-StartApps di mesin ini (2026-11-14).
/// Tidak ada padanan Win32 untuk `ms-settings:windowsdefender`, jadi mode shell mengaktifkan app paketnya.
const HK_SECHEALTH_AUMID: &str = "Microsoft.SecHealthUI_8wekyb3d8bbwe!SecHealthUI";

/// AUMID aplikasi Pengaturan (UWP murni). Diverifikasi dengan Get-StartApps di mesin ini (2026-11-14).
const HK_SETTINGS_AUMID: &str = "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel";

/// Padanan Win32 untuk `ms-settings:<page>`: (berkas, argumen). None bila tidak ada padanan.
fn hk_settings_alt(page: &str) -> Option<(&'static str, Option<&'static str>)> {
    HK_SETTINGS_ALT.iter().find_map(|&(name, prefix, file, args)| {
        let hit = if prefix {
            page.get(..name.len()).map_or(false, |p| p.eq_ignore_ascii_case(name))
        } else {
            page.eq_ignore_ascii_case(name)
        };
        hit.then_some((file, args))
    })
}

// ---------------------------------------------------------------- ShellExecute

/// Ubah Option<Vec<u16>> menjadi PCWSTR (NULL bila None).
fn opt_pcw(o: &Option<Vec<u16>>) -> PCWSTR {
    match o {
        Some(w) => pcw(w),
        None => PCWSTR::null(),
    }
}

/// ShellExecuteW dengan argumen opsional. Hasil di atas 32 berarti berhasil (INT_PTR).
fn shell_exec(verb: Option<&str>, file: &str, params: Option<&str>, dir: Option<&str>) -> isize {
    let v = verb.map(to_wide_z);
    let f = to_wide_z(file);
    let p = params.map(to_wide_z);
    let d = dir.map(to_wide_z);
    // SAFETY: semua buffer wide berakhir NUL dan hidup sampai pemanggilan selesai.
    unsafe { ShellExecuteW(None, opt_pcw(&v), pcw(&f), opt_pcw(&p), opt_pcw(&d), SW_SHOWNORMAL).0 as isize }
}

/// Mode shell (ShellModeOpen): true bila target sudah ditangani (diluncurkan atau ditawari beralih).
/// false = ShellExecute biasa.
fn shell_mode_open(target: &str, args: Option<&str>) -> bool {
    const APPS: &str = "shell:AppsFolder\\";
    // explorer.exe dan control.exe yang dipanggil tanpa argumen khusus akan menyalakan shell Explorer di mode shell.
    // Keduanya dialihkan ke win12files (IExplorerBrowser), yang juga bisa menampung Panel Kontrol. Argumen lain
    // (mis. `explorer /select,...`, `control ncpa.cpl`) tidak diubah.
    let base = target.rsplit(['\\', '/']).next().unwrap_or(target);
    let arg = args.map(str::trim).unwrap_or("");
    if target.eq_ignore_ascii_case("control") || base.eq_ignore_ascii_case("control.exe") {
        if arg.is_empty() {
            crate::shell::core_c3b::open_folder_window(CONTROL_PANEL_PATH);
            return true;
        }
    } else if target.eq_ignore_ascii_case("explorer") || base.eq_ignore_ascii_case("explorer.exe") {
        if arg.is_empty() || (!arg.starts_with('/') && !arg.starts_with('-')) {
            crate::shell::core_c3b::open_folder_window(arg.trim_matches('"'));
            return true;
        }
    }
    if target.get(..APPS.len()).map_or(false, |p| p.eq_ignore_ascii_case(APPS)) {
        let aumid = &target[APPS.len()..];
        // App Win32 di AppsFolder (tanpa '!') bekerja lewat ShellExecute tanpa Explorer.
        if !aumid.contains('!') {
            return false;
        }
        hk_launch_packaged(aumid, args.unwrap_or(""), target);
        return true;
    }
    if target.get(..12).map_or(false, |p| p.eq_ignore_ascii_case("ms-settings:")) {
        if target[12..].eq_ignore_ascii_case("windowsdefender") {
            // Gagal aktivasi -> hk_launch_job jatuh ke hk_post_needexp(orig) (dialog beralih), sama seperti app paket lain.
            hk_launch_packaged(HK_SECHEALTH_AUMID, args.unwrap_or(""), target);
            return true;
        }
        match hk_settings_alt(&target[12..]) {
            Some((file, alt_args)) => {
                shell_exec(None, file, alt_args, None);
            }
            // Tanpa padanan Win32: coba ActivateForProtocol ke Pengaturan; gagal -> dialog beralih.
            None => hk_launch_settings_uri(target),
        }
        return true;
    }
    // URI lain ("skema:..."): bila penanganannya app paket, jalankan app itu dengan URI sebagai argumen.
    let Some(colon) = target.find(':') else {
        return false;
    };
    if !(2..=40).contains(&colon) || args.is_some() {
        return false; // "C:\..." dan berkas biasa
    }
    if !target[..colon].chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '.' | '+')) {
        return false;
    }
    let scheme = to_wide_z(&target[..colon]);
    let mut n: u32 = 300;
    let mut buf = vec![0u16; 300];
    // SAFETY: scheme berakhir NUL; buf berukuran 300 sesuai kapasitas n.
    let hr = unsafe {
        AssocQueryStringW(
            ASSOCF_IS_PROTOCOL,
            ASSOCSTR_APPID,
            pcw(&scheme),
            PCWSTR::null(),
            Some(PWSTR(buf.as_mut_ptr())),
            &mut n,
        )
    };
    if hr.is_err() {
        return false;
    }
    let app_id = wide_until_nul(&buf);
    if !app_id.contains('!') {
        return false;
    }
    hk_launch_packaged(&app_id, target, target);
    true
}

/// `ShellOpen(target)` tanpa argumen (dipakai pemanggil Rust). Target kosong diabaikan.
pub fn shell_open(target: &str) {
    shell_open_with(target, None);
}

/// `ShellOpen(target, args)`: mode shell lewat shell_mode_open, lalu ShellExecute biasa.
pub fn shell_open_with(target: &str, args: Option<&str>) {
    if target.is_empty() {
        return;
    }
    // Entri "File Explorer" dari AppsFolder (Start, pencarian, daftar semua aplikasi) selalu membuka
    // Win12 Explorer, sama seperti pin dok dan Win+E. open_folder_window jatuh ke explorer.exe hanya di luar
    // mode shell bila win12files.exe tidak ada.
    if args.is_none() && target.eq_ignore_ascii_case("shell:AppsFolder\\Microsoft.Windows.Explorer") {
        crate::shell::core_c3b::open_folder_window("");
        return;
    }
    if SHELL_MODE.get() && shell_mode_open(target, args) {
        return;
    }
    shell_exec(None, target, args, None);
}

// ---------------------------------------------------------------- pesan "butuh Explorer" dan peralihan mode

/// Panel Kontrol (tampilan kategori), dibuka di win12files pada mode shell.
const CONTROL_PANEL_PATH: &str = "shell:::{26EE0668-A00A-44D7-9371-BEB064C98683}";

/// Kunci HKCU untuk pesan peralihan (Software\Win12Shell).
const WIN12_KEY: &str = "Software\\Win12Shell";

/// Nama mutex instans win12boot (dibuka dengan SYNCHRONIZE).
const BOOT_MUTEX: &str = "Win12BootSingleInstance";

/// Path win12boot.exe di folder aplikasi (BootExePath). None bila berkas tidak ada.
pub fn boot_exe_path() -> Option<String> {
    let p = format!("{}\\win12boot.exe", DIR.borrow());
    let w = to_wide_z(&p);
    // SAFETY: w berakhir NUL dan hidup selama pemanggilan.
    let attr = unsafe { GetFileAttributesW(pcw(&w)) };
    (attr != INVALID_FILE_ATTRIBUTES).then_some(p)
}

/// Nama tampilan item shell untuk pesan (bagian SHCreateItemFromParsingName di HkNeedExplorer). None bila gagal.
fn shell_display_name(target: &str) -> Option<String> {
    let w = to_wide_z(target);
    // SAFETY: w berakhir NUL dan hidup selama pemanggilan.
    let si: IShellItem = unsafe { SHCreateItemFromParsingName(pcw(&w), None::<&IBindCtx>) }.ok()?;
    // SAFETY: si valid; string hasil GetDisplayName wajib dibebaskan dengan CoTaskMemFree di bawah.
    let p = unsafe { si.GetDisplayName(SIGDN_NORMALDISPLAY) }.ok()?;
    // SAFETY: p.0 menunjuk string NUL-terminated dari COM.
    let s = unsafe { from_pcwstr(p.0 as *const u16) };
    // SAFETY: p berasal dari GetDisplayName dan belum dibebaskan.
    unsafe { CoTaskMemFree(Some(p.0 as *const c_void)) };
    (!s.is_empty()).then_some(s)
}

/// Pesan "app UWP hanya bisa dibuka saat Explorer berjalan" (HkNeedExplorer).
/// Penerima WM_W12_NEEDEXP memanggil ini dengan String yang dibongkar dari Box (kontrak LPARAM).
/// Ya -> shell_switch_to_windows(target); tidak -> tidak terjadi apa-apa.
pub fn hk_need_explorer(target: String) {
    let name = if target.get(..12).map_or(false, |p| p.eq_ignore_ascii_case("ms-settings:")) {
        if target.get(12..).map_or(false, |p| p.eq_ignore_ascii_case("windowsdefender")) {
            "Keamanan Windows".to_string()
        } else {
            "Pengaturan Windows".to_string()
        }
    } else {
        shell_display_name(&target).unwrap_or_else(|| target.clone())
    };
    let text = to_wide_z(&format!(
        "{name} adalah app Windows modern (UWP) yang hanya bisa dibuka saat Explorer berjalan.\n\n\
         Beralih ke Windows biasa sekarang? Jendela app yang terbuka tetap ada, dan {name} dibuka setelah Explorer siap.\n\n\
         Kembali ke Win12 penuh nanti: Start > Win12 penuh, atau klik kanan dok > Beralih ke Win12 penuh."
    ));
    let caption = to_wide_z("Win12 Shell");
    // SAFETY: text dan caption berakhir NUL dan hidup selama pemanggilan.
    let r = unsafe {
        MessageBoxW(
            None,
            pcw(&text),
            pcw(&caption),
            MB_YESNO | MB_ICONINFORMATION | MB_SETFOREGROUND | MB_TOPMOST,
        )
    };
    if r == IDYES {
        shell_switch_to_windows(Some(&target));
    }
}

/// Explorer sementara (2026-11-18, permintaan pengguna): mode shell diserahkan ke Explorer agar app UWP bisa tampil.
/// win12boot melihat tanda `TempExplorer`, menjalankan Explorer + Win12 biasa, membuka `target`, lalu kembali ke
/// Win12 penuh setelah tidak ada jendela app UWP lagi.
pub fn hk_temp_explorer(target: String) {
    if !SHELL_MODE.get() {
        shell_open(&target);
        return;
    }
    let _ = crate::reg::set_our_dword("TempExplorer", 1);
    shell_switch_to_windows(Some(&target));
}

/// ShellSwitchToWindows: mode shell -> Windows biasa. win12boot melihat kode keluar 0 dan menjalankan Explorer.
/// `pending` (boleh None) dibuka win12boot setelah taskbar Explorer muncul (HKCU\Software\Win12Shell OpenAfterSwitch).
/// Tidak berbuat apa-apa bila bukan mode shell.
pub fn shell_switch_to_windows(pending: Option<&str>) {
    if !SHELL_MODE.get() {
        return;
    }
    match pending.filter(|p| !p.is_empty()) {
        Some(p) => {
            crate::reg::set_string(WIN12_KEY, "OpenAfterSwitch", p);
        }
        None => {
            crate::reg::delete_value(WIN12_KEY, "OpenAfterSwitch");
        }
    }
    let mname = to_wide_z(BOOT_MUTEX);
    // SAFETY: mname berakhir NUL dan hidup selama pemanggilan; handle yang berhasil dibuka langsung ditutup.
    match unsafe { OpenMutexW(SYNCHRONIZATION_ACCESS_RIGHTS(0x0010_0000), false, pcw(&mname)) } {
        Ok(h) => {
            // SAFETY: h adalah handle mutex yang baru dibuka.
            let _ = unsafe { CloseHandle(h) };
        }
        Err(_) => EXPLORER_ON_EXIT.set(true),
    }
    log("beralih ke Windows biasa (permintaan pengguna)");
    // SAFETY: WM_CLOSE tanpa parameter pointer ke jendela shell.
    let _ = unsafe { PostMessageW(Some(MSG.get()), WM_CLOSE, WPARAM(0), LPARAM(0)) };
}

/// NormalSwitchToShell: Windows biasa -> Win12 penuh. Minta konfirmasi, lalu jalankan win12boot --switch.
pub fn normal_switch_to_shell() {
    if SHELL_MODE.get() {
        return;
    }
    let Some(exe) = boot_exe_path() else {
        return;
    };
    let text = to_wide_z(
        "Explorer akan ditutup dan Win12 menjadi shell penuh sampai Anda beralih kembali.\n\n\
         Jendela File Explorer yang terbuka ikut tertutup; jendela app lain tetap terbuka.\n\
         Kembali ke Windows biasa: Win+X > Beralih ke Windows biasa.\n\nLanjutkan?",
    );
    let caption = to_wide_z("Win12 Shell");
    // SAFETY: text dan caption berakhir NUL dan hidup selama pemanggilan.
    let r = unsafe {
        MessageBoxW(
            None,
            pcw(&text),
            pcw(&caption),
            MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND | MB_TOPMOST,
        )
    };
    if r != IDYES {
        return;
    }
    let dir = DIR.borrow().clone();
    shell_exec(Some("open"), &exe, Some("--switch"), Some(dir.as_str()));
}

/// Ada jendela app yang tampil dan tidak diminimalkan (memakai filter `is_task_window` yang sama).
fn desk_ada_app_tampil() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetTopWindow, GetWindow, IsIconic, GW_HWNDNEXT};
    let mut w = unsafe { GetTopWindow(None) }.unwrap_or_default();
    while !w.0.is_null() {
        if is_task_window(w) && !unsafe { IsIconic(w) }.as_bool() {
            return true;
        }
        w = unsafe { GetWindow(w, GW_HWNDNEXT) }.unwrap_or_default();
    }
    false
}

/// ShowDesktopToggle (win12shell.cpp:3648), diubah: bila masih ada app yang tampil (termasuk yang dibuka setelah
/// Win+D sebelumnya, mis. Task Manager), minimalkan semuanya dan GABUNGKAN ke daftar pemulihan. Pulihkan hanya
/// bila tidak ada lagi app yang tampil. Sebelumnya, satu jendela lama yang masih diminimalkan membuat tekanan
/// kedua selalu memulihkan dan jendela baru tidak ikut diminimalkan.
pub fn show_desktop_toggle() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;
    use super::core_c3b::SD_WINS;
    if !desk_ada_app_tampil() && desk_restore_all() {
        return;
    }
    let sebelum: Vec<HWND> = SD_WINS.borrow().to_vec();
    desk_minimize_all(None);
    let mut gabung = SD_WINS.borrow_mut();
    for h in sebelum {
        if !gabung.contains(&h) && unsafe { IsWindow(Some(h)) }.as_bool() {
            gabung.push(h);
        }
    }
}

// ---------------------------------------------------------------- dok, siklus app, item, kalender

/// Indeks item dok ke-n (1-based) yang bertipe IT_APP (HkDockItem). None bila tidak ada.
fn hk_dock_item(n: i32) -> Option<usize> {
    let count = NITEMS.get().max(0) as usize;
    let items = ITEMS.borrow();
    let mut seen = 0;
    for (i, it) in items.iter().take(count).enumerate() {
        if it.ty == IT_APP {
            seen += 1;
            if seen == n {
                return Some(i);
            }
        }
    }
    None
}

/// Win+Shift+n (how 0): instans baru. Win+Ctrl+n (how 1): jendela terakhir, berganti bila app sudah aktif.
/// Win+Alt+n (how 2): menu app (jump list) dari pil dok.
fn hk_dock(n: i32, how: i32) {
    let Some(i) = hk_dock_item(n) else {
        return;
    };
    let (ai, pill_i, item_x, item_w) = {
        let items = ITEMS.borrow();
        let Some(it) = items.get(i) else {
            return;
        };
        (it.app, it.pill, it.x, it.w)
    };
    if ai < 0 || ai >= apps_count() {
        return;
    }
    if how == 0 {
        click_app(ai, true);
        return;
    }
    if how == 1 {
        let wins: Vec<HWND> = match APPS.borrow().get(ai as usize) {
            Some(a) => a.wins.clone(),
            None => return,
        };
        if wins.is_empty() {
            click_app(ai, false);
            return;
        }
        // SAFETY: GetForegroundWindow tidak membaca memori milik kita.
        let fg = unsafe { GetForegroundWindow() };
        let cur = wins.iter().rposition(|&w| w == fg).map_or(-1i64, |c| c as i64);
        let next = if cur < 0 { 0 } else { (cur as usize + 1) % wins.len() };
        quiet_start(1000);
        activate_window(wins[next]);
        return;
    }
    if !(0..NBOTTOM).contains(&pill_i) {
        return;
    }
    let pill = {
        let pills = PILLS.borrow();
        match pills.get(pill_i as usize) {
            Some(p) if !p.h.0.is_null() && p.shown => *p,
            _ => return,
        }
    };
    let pt = POINT {
        x: pill.x + (item_x + item_w / 2.0) as i32,
        y: pill.y,
    };
    app_menu(pill.h, pt, ai);
}

/// Win+T / Win+Shift+T (dir +1 / -1): berpindah antar app dok yang punya jendela, urut dok (HkCycleDock).
fn hk_cycle_dock(dir: i32) {
    // SAFETY: GetForegroundWindow tidak membaca memori milik kita.
    let fg = unsafe { GetForegroundWindow() };
    let count = NITEMS.get().max(0) as usize;
    // Daftar jendela tiap app yang punya jendela, urut dok; cur = posisi app yang aktif.
    let mut list: Vec<Vec<HWND>> = Vec::new();
    let mut cur: Option<usize> = None;
    {
        let items = ITEMS.borrow();
        let apps = APPS.borrow();
        for it in items.iter().take(count) {
            if it.ty != IT_APP || it.app < 0 || it.app >= apps_count() {
                continue;
            }
            let Some(a) = apps.get(it.app as usize) else {
                continue;
            };
            if a.wins.is_empty() {
                continue;
            }
            if a.wins.contains(&fg) {
                cur = Some(list.len());
            }
            list.push(a.wins.clone());
        }
    }
    let n = list.len();
    if n == 0 {
        return;
    }
    let next = match cur {
        None => {
            if dir > 0 {
                0
            } else {
                n - 1
            }
        }
        Some(c) => ((c as i64 + dir as i64 + n as i64) % n as i64) as usize,
    };
    quiet_start(1000);
    activate_window(list[next][0]);
}

/// Aktifkan item dok pertama bertipe `ty` yang pilnya tampil (HkItem). true bila ditemukan dan diaktifkan.
fn hk_item(ty: i32) -> bool {
    let count = NITEMS.get().max(0) as usize;
    let found = {
        let items = ITEMS.borrow();
        let pills = PILLS.borrow();
        items.iter().take(count).enumerate().find_map(|(i, it)| {
            let shown = it.pill >= 0 && it.pill < NBOTTOM && pills.get(it.pill as usize).map_or(false, |p| p.shown);
            (it.ty == ty && shown).then_some(i)
        })
    };
    match found {
        Some(i) => {
            activate_item(i as i32, false);
            true
        }
        None => false,
    }
}

/// Win+N / Win+Alt+D: kalender + notifikasi (PN_CAL), dari jam di dok bila tampil (HkCalendar).
fn hk_calendar() {
    if hk_item(IT_BELL) || hk_item(IT_CLOCK) || hk_item(IT_TCLOCK) {
        return;
    }
    let mut work = RECT::default();
    // SAFETY: work adalah RECT yang hidup selama pemanggilan; SPI_GETWORKAREA mengisi RECT.
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work as *mut RECT as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    panel_toggle(PN_CAL, work.right - s(170.0), work.bottom);
}

// ---------------------------------------------------------------- Win+, : intip desktop

// Jendela topmost berisi thumbnail DWM dari desktop Win12 (DESK). Sumber: g_hkPeekWnd, g_hkPeekTh, g_hkPeekT0.
static HK_PEEK_WND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static HK_PEEK_TH: SyncCell<Cell<isize>> = SyncCell::new(Cell::new(0));
static HK_PEEK_T0: SyncCell<Cell<u32>> = SyncCell::new(Cell::new(0));
static HK_PEEK_REG: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

/// HkPeekEnd: lepas thumbnail DWM lalu hancurkan jendela peek.
fn hk_peek_end() {
    let th = HK_PEEK_TH.get();
    if th != 0 {
        // SAFETY: th adalah id thumbnail DWM yang dibuat hk_peek.
        let _ = unsafe { DwmUnregisterThumbnail(th) };
        HK_PEEK_TH.set(0);
    }
    let wnd = HK_PEEK_WND.get();
    if !wnd.0.is_null() {
        // SAFETY: wnd adalah jendela peek yang dibuat hk_peek.
        let _ = unsafe { DestroyWindow(wnd) };
        HK_PEEK_WND.set(HWND(std::ptr::null_mut()));
    }
}

/// HkPeekTick (TIMERPROC): tutup peek setelah Win dilepas, atau paksa tutup setelah 20 detik.
unsafe extern "system" fn hk_peek_tick(h: HWND, _msg: u32, id: usize, _time: u32) {
    // SAFETY: GetAsyncKeyState dan GetTickCount tidak punya prasyarat.
    let held = unsafe { GetAsyncKeyState(VK_LWIN.0 as i32) } < 0 || unsafe { GetAsyncKeyState(VK_RWIN.0 as i32) } < 0;
    if held && unsafe { GetTickCount() }.wrapping_sub(HK_PEEK_T0.get()) < 20000 {
        return;
    }
    // SAFETY: h dan id berasal dari SetTimer di hk_peek.
    let _ = unsafe { KillTimer(Some(h), id) };
    hk_peek_end();
}

/// Prosedur jendela peek: teruskan ke DefWindowProcW (WNDPROC butuh ABI extern "system").
unsafe extern "system" fn hk_peek_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: argumen diteruskan apa adanya dari sistem.
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

/// HkPeek (Win+,): jendela topmost di monitor kursor berisi thumbnail desktop (DESK).
fn hk_peek() {
    if !HK_PEEK_WND.get().0.is_null() {
        return;
    }
    let desk = DESK.get();
    if desk.0.is_null() {
        return;
    }
    let hi = HI.get();
    if !HK_PEEK_REG.get() {
        let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(hk_peek_proc);
        wc.hInstance = hi;
        // SAFETY: IDC_ARROW adalah kursor sistem.
        if let Ok(cur) = unsafe { LoadCursorW(None, IDC_ARROW) } {
            wc.hCursor = cur;
        }
        // SAFETY: BLACK_BRUSH adalah objek GDI stok (tidak perlu dihapus).
        wc.hbrBackground = HBRUSH(unsafe { GetStockObject(BLACK_BRUSH) }.0);
        wc.lpszClassName = w!("Win12Peek");
        // SAFETY: wc hidup selama pemanggilan.
        HK_PEEK_REG.set(unsafe { RegisterClassExW(&wc) } != 0);
    }
    let mut pt = POINT::default();
    // SAFETY: pt adalah POINT yang hidup selama pemanggilan.
    let _ = unsafe { GetCursorPos(&mut pt) };
    let mut mi: MONITORINFO = unsafe { std::mem::zeroed() };
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    // SAFETY: mi berukuran cbSize dan hidup selama pemanggilan.
    let hm = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY) };
    if !unsafe { GetMonitorInfoW(hm, &mut mi) }.as_bool() {
        return;
    }
    let r = mi.rcWork;
    let (cw, ch) = (r.right - r.left, r.bottom - r.top);
    // SAFETY: kelas Win12Peek sudah didaftarkan; jendela top-level tanpa parent.
    let hwnd = match unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("Win12Peek"),
            w!(""),
            WS_POPUP,
            r.left,
            r.top,
            cw,
            ch,
            None,
            None,
            Some(hi),
            None,
        )
    } {
        Ok(h) => h,
        Err(_) => return,
    };
    HK_PEEK_WND.set(hwnd);
    // SAFETY: hwnd dan desk adalah jendela valid.
    match unsafe { DwmRegisterThumbnail(hwnd, desk) } {
        Ok(th) => HK_PEEK_TH.set(th),
        Err(_) => {
            hk_peek_end();
            return;
        }
    }
    let mut dr = RECT::default();
    // SAFETY: dr hidup selama pemanggilan.
    let _ = unsafe { GetWindowRect(desk, &mut dr) };
    let tp = DWM_THUMBNAIL_PROPERTIES {
        dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_RECTSOURCE | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
        rcDestination: RECT { left: 0, top: 0, right: cw, bottom: ch },
        rcSource: RECT {
            left: r.left - dr.left,
            top: r.top - dr.top,
            right: r.right - dr.left,
            bottom: r.bottom - dr.top,
        },
        opacity: 255,
        fVisible: BOOL(1),
        fSourceClientAreaOnly: BOOL(0),
    };
    // SAFETY: tp hidup selama pemanggilan; id thumbnail sudah valid.
    let _ = unsafe { DwmUpdateThumbnailProperties(HK_PEEK_TH.get(), &tp) };
    // SAFETY: hwnd adalah jendela yang baru dibuat.
    let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
    // SAFETY: GetTickCount tidak punya prasyarat.
    HK_PEEK_T0.set(unsafe { GetTickCount() });
    // SAFETY: hwnd valid; hk_peek_tick kompatibel dengan TIMERPROC.
    let _ = unsafe { SetTimer(Some(hwnd), 1, 50, Some(hk_peek_tick)) };
}

/// Win+Alt+K: bisukan/aktifkan mikrofon default komunikasi (sumber HkMicMute, 432–448).
fn hk_mic_mute() {
    let cached = DEV_ENUM.borrow().as_ref().cloned();
    let enumerator = match cached {
        Some(e) => e,
        None => {
            // SAFETY: CLSID MMDeviceEnumerator dengan INPROC_SERVER dan tanpa outer unknown.
            let created: windows::core::Result<IMMDeviceEnumerator> =
                unsafe { CoCreateInstance(&MMDeviceEnumerator, None::<&IUnknown>, CLSCTX_INPROC_SERVER) };
            match created {
                Ok(e) => {
                    *DEV_ENUM.borrow_mut() = Some(e.clone());
                    e
                }
                Err(_) => return,
            }
        }
    };
    // SAFETY: enumerator hidup selama pemanggilan; interface lepas borrow sebelum hk_toast.
    let dev = match unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications) } {
        Ok(d) => d,
        Err(_) => {
            hk_toast("Mikrofon tidak ditemukan", 0xE720);
            return;
        }
    };
    // SAFETY: dev hidup selama pemanggilan; Activate mengembalikan interface yang dikelola windows-rs.
    let vol: Option<IAudioEndpointVolume> =
        unsafe { dev.Activate::<IAudioEndpointVolume>(CLSCTX_INPROC_SERVER, None) }.ok();
    if let Some(v) = vol {
        // SAFETY: v adalah IAudioEndpointVolume yang valid dari Activate di atas.
        if let Ok(muted) = unsafe { v.GetMute() } {
            let m = muted.as_bool();
            // SAFETY: pguideventcontext NULL, sama dengan sumber.
            if unsafe { v.SetMute(!m, std::ptr::null()) }.is_ok() {
                if m {
                    hk_toast("Mikrofon aktif", 0xE720);
                } else {
                    hk_toast("Mikrofon dibisukan", 0xEC54);
                }
            }
        }
    }
}

/// Win+PrtScn: tulis PNG 32bpp BGRA ke `path`; gagal → file dihapus (sumber HkSavePng, 451–470).
fn hk_save_png(path: &str, bits: &[u8], w: i32, h: i32) -> bool {
    let pw = to_wide_z(path);
    // Faktori WIC milik shell dulu; bila tidak ada atau gagal, faktori baru (2026-11-19: tangkapan hanya masuk
    // clipboard tanpa jejak penyebab, jadi tiap langkah kini dicatat di boot.log).
    let shared = WIC.borrow().as_ref().cloned();
    let mut err = String::from("faktori WIC shell tidak ada");
    if let Some(wic) = shared {
        // SAFETY: pw dan bits hidup sampai pemanggilan selesai; wic dipakai dari thread UI.
        match unsafe { hk_save_png_com(&wic, &pw, bits, w, h) } {
            Ok(()) => return true,
            Err(e) => err = e,
        }
        // SAFETY: pw berakhir NUL dan hidup sampai pemanggilan selesai.
        let _ = unsafe { DeleteFileW(pcw(&pw)) };
    }
    // SAFETY: CoCreateInstance di thread yang sudah memakai COM; hasil dikelola windows-rs.
    let fresh: windows::core::Result<IWICImagingFactory> = unsafe {
        windows::Win32::System::Com::CoCreateInstance(
            &windows::Win32::Graphics::Imaging::CLSID_WICImagingFactory,
            None,
            windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
        )
    };
    let res = match fresh {
        // SAFETY: sama seperti di atas.
        Ok(wic) => unsafe { hk_save_png_com(&wic, &pw, bits, w, h) },
        Err(e) => Err(format!("CoCreateInstance WIC {:08X}", e.code().0)),
    };
    match res {
        Ok(()) => {
            crate::log::log(&format!("tangkapan: tersimpan lewat faktori baru ({err})"));
            true
        }
        Err(e2) => {
            // SAFETY: pw berakhir NUL dan hidup sampai pemanggilan selesai.
            let _ = unsafe { DeleteFileW(pcw(&pw)) };
            crate::log::log(&format!("tangkapan: simpan PNG gagal ke {path}: {err}; ulang: {e2}"));
            false
        }
    }
}

/// Bagian COM dari `hk_save_png`. Err berisi langkah WIC yang gagal beserta HRESULT-nya.
unsafe fn hk_save_png_com(wic: &IWICImagingFactory, path: &[u16], bits: &[u8], w: i32, h: i32) -> Result<(), String> {
    let hr = |step: &str, e: windows::core::Error| format!("{step} {:08X}", e.code().0);
    let st = wic.CreateStream().map_err(|e| hr("CreateStream", e))?;
    st.InitializeFromFilename(pcw(path), GENERIC_WRITE.0).map_err(|e| hr("buka berkas", e))?;
    let enc = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null()).map_err(|e| hr("CreateEncoder", e))?;
    let stream = st.cast::<IStream>().map_err(|e| hr("IStream", e))?;
    enc.Initialize(&stream, WICBitmapEncoderNoCache).map_err(|e| hr("enc.Initialize", e))?;
    let mut fr: Option<IWICBitmapFrameEncode> = None;
    enc.CreateNewFrame(&mut fr, std::ptr::null_mut()).map_err(|e| hr("CreateNewFrame", e))?;
    let fr = fr.ok_or_else(|| "CreateNewFrame kosong".to_string())?;
    fr.Initialize(None::<&IPropertyBag2>).map_err(|e| hr("frame.Initialize", e))?;
    fr.SetSize(w as u32, h as u32).map_err(|e| hr("SetSize", e))?;
    let mut pf = GUID_WICPixelFormat32bppBGRA;
    fr.SetPixelFormat(&mut pf).map_err(|e| hr("SetPixelFormat", e))?;
    if pf != GUID_WICPixelFormat32bppBGRA {
        return Err("format piksel ditolak".into());
    }
    fr.WritePixels(h as u32, (w * 4) as u32, bits).map_err(|e| hr("WritePixels", e))?;
    fr.Commit().map_err(|e| hr("frame.Commit", e))?;
    enc.Commit().map_err(|e| hr("enc.Commit", e))
}

/// Win+PrtScn: seluruh layar ke PNG di Gambar\Screenshots dan ke clipboard (CF_DIB) (sumber HkScreenshot, 472–535).
fn hk_screenshot() {
    // SAFETY: GetSystemMetrics hanya membaca metrik sistem.
    let (x, y, w, h) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    hk_shot_rect(x, y, w, h);
}

/// Alt+PrtScn: jendela aktif ke PNG di Gambar\Screenshots dan ke clipboard. Batas memakai
/// DWMWA_EXTENDED_FRAME_BOUNDS (tanpa bayangan jendela); bila DWM gagal, pakai GetWindowRect.
fn hk_screenshot_window() {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    // SAFETY: GetForegroundWindow tidak membaca memori milik kita.
    let fg = unsafe { GetForegroundWindow() };
    if fg.0.is_null() {
        hk_toast("Tidak ada jendela aktif", 0xE783);
        return;
    }
    let mut rc = RECT::default();
    // SAFETY: rc adalah RECT yang hidup selama pemanggilan; DWM menulis paling banyak size_of::<RECT>() byte.
    let dwm_ok = unsafe {
        DwmGetWindowAttribute(
            fg,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rc as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok()
    };
    if !dwm_ok {
        // SAFETY: rc adalah RECT yang hidup selama pemanggilan; GetWindowRect hanya menulisnya.
        if unsafe { GetWindowRect(fg, &mut rc) }.is_err() {
            hk_toast("Tangkapan jendela gagal", 0xE722);
            return;
        }
    }
    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
    if w <= 0 || h <= 0 {
        hk_toast("Tangkapan jendela gagal", 0xE722);
        return;
    }
    hk_shot_rect(rc.left, rc.top, w, h);
}

/// Tangkapan satu persegi (koordinat layar virtual) ke clipboard dan PNG. Dipakai `hk_screenshot` dan `hk_screenshot_window`.
fn hk_shot_rect(x: i32, y: i32, w: i32, h: i32) {
    if w <= 0 || h <= 0 {
        return;
    }
    // SAFETY: BITMAPINFO adalah struct POD; nol adalah nilai awal yang valid.
    let mut bi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = -h; // atas-ke-bawah
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB.0;

    let mut bits: *mut c_void = std::ptr::null_mut();
    let mut shot = false;
    // SAFETY: sdc dan mdc dilepas sebelum blok selesai; bits menunjuk memori DIB milik bmp
    // sampai DeleteObject di bawah.
    let bmp: Option<HBITMAP> = unsafe {
        let sdc = GetDC(None);
        let mdc = CreateCompatibleDC(Some(sdc));
        let bmp = CreateDIBSection(Some(sdc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok();
        if let Some(b) = bmp {
            if !bits.is_null() && !mdc.0.is_null() {
                let old = SelectObject(mdc, HGDIOBJ(b.0));
                shot = BitBlt(mdc, 0, 0, w, h, Some(sdc), x, y, ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0)).is_ok();
                let _ = GdiFlush();
                let _ = SelectObject(mdc, old);
            }
        }
        if !mdc.0.is_null() {
            let _ = DeleteDC(mdc);
        }
        let _ = ReleaseDC(None, sdc);
        bmp
    };
    let bmp = match bmp {
        Some(b) if shot => b,
        other => {
            if let Some(b) = other {
                // SAFETY: b adalah HBITMAP dari CreateDIBSection dan belum dipakai lagi.
                let _ = unsafe { DeleteObject(HGDIOBJ(b.0)) };
            }
            hk_toast("Tangkapan layar gagal", 0xE722);
            return;
        }
    };
    // SAFETY: bits menunjuk buffer DIB 32bpp sebesar w*h*4 byte yang hidup sampai DeleteObject di bawah.
    let px: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, (w as usize) * 4 * (h as usize)) };
    for pix in px.chunks_exact_mut(4) {
        pix[3] = 255; // BitBlt tidak mengisi alfa
    }
    hk_publish_shot(px, w, h);
    // SAFETY: bmp adalah HBITMAP dari CreateDIBSection dan tidak dipakai lagi setelah ini.
    let _ = unsafe { DeleteObject(HGDIOBJ(bmp.0)) };
}

/// Bagian bersama tangkapan layar: salin BGRA atas-ke-bawah (alfa 255) ke clipboard, simpan PNG di
/// folder Screenshots, lalu tampilkan toast. Dipakai PrtScn (`hk_screenshot`) dan tangkapan area
/// Win+Shift+S (`shot_area.rs`). `px` harus berukuran w*h*4 byte.
pub fn hk_publish_shot(px: &[u8], w: i32, h: i32) {
    let hu = h as usize;
    let row = (w as usize) * 4;

    let mut clip = false;
    // SAFETY: OpenClipboard dipasangkan dengan CloseClipboard di bawah.
    if unsafe { OpenClipboard(Some(MSG.get())) }.is_ok() {
        let hdr = std::mem::size_of::<BITMAPINFOHEADER>();
        // SAFETY: g hasil GlobalAlloc; dst dipakai hanya sampai GlobalUnlock.
        if let Ok(g) = unsafe { GlobalAlloc(GMEM_MOVEABLE, hdr + row * hu) } {
            let dst = unsafe { GlobalLock(g) } as *mut u8;
            if !dst.is_null() {
                // SAFETY: BITMAPINFOHEADER adalah struct POD; nol adalah nilai awal yang valid.
                let mut bh: BITMAPINFOHEADER = unsafe { std::mem::zeroed() };
                bh.biSize = hdr as u32;
                bh.biWidth = w;
                bh.biHeight = h;
                bh.biPlanes = 1;
                bh.biBitCount = 32;
                bh.biCompression = BI_RGB.0;
                // SAFETY: dst menunjuk blok g berukuran hdr + row*h; sumber dan tujuan tidak tumpang tindih.
                unsafe {
                    std::ptr::copy_nonoverlapping(&bh as *const BITMAPINFOHEADER as *const u8, dst, hdr);
                    for r in 0..hu {
                        std::ptr::copy_nonoverlapping(
                            px.as_ptr().add(row * r),
                            dst.add(hdr + row * (hu - 1 - r)),
                            row,
                        );
                    }
                    let _ = GlobalUnlock(g);
                }
                // SAFETY: clipboard sudah dibuka; g diserahkan ke clipboard bila berhasil.
                let _ = unsafe { EmptyClipboard() };
                clip = unsafe { SetClipboardData(CF_DIB.0 as u32, Some(HANDLE(g.0))) }.is_ok();
            }
            if !clip {
                // SAFETY: g belum diserahkan ke clipboard sehingga masih milik kita.
                let _ = unsafe { GlobalFree(Some(g)) };
            }
        }
        // SAFETY: clipboard dibuka oleh OpenClipboard di atas.
        let _ = unsafe { CloseClipboard() };
    }

    let mut dir = String::new();
    // SAFETY: SHGetKnownFolderPath mengalokasikan PWSTR yang dibebaskan dengan CoTaskMemFree.
    if let Ok(kf) = unsafe { SHGetKnownFolderPath(&FOLDERID_Screenshots, KF_FLAG_CREATE, None) } {
        dir = unsafe { from_pcwstr(kf.0) };
        unsafe { CoTaskMemFree(Some(kf.0 as *const c_void)) };
    }
    if dir.is_empty() {
        // 2026-11-19: di mode shell SHGetKnownFolderPath(Screenshots/Pictures) gagal, jadi PNG tak pernah tersimpan.
        // Cadangan: baca lokasi folder langsung dari registri pengguna (RegGetValue mengembangkan %VAR%).
        let err = unsafe { SHGetKnownFolderPath(&FOLDERID_Screenshots, KF_FLAG_CREATE, None) }
            .err()
            .map(|e| format!("{:08X}", e.code().0))
            .unwrap_or_default();
        const USF: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders";
        const SF: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders";
        const SHOTS: &str = "{B7BEDE81-DF94-4682-A7D8-57A52620B86F}";
        dir = crate::reg::get_string(USF, SHOTS)
            .or_else(|| crate::reg::get_string(SF, SHOTS))
            .or_else(|| crate::reg::get_string(USF, "My Pictures").map(|p| format!("{p}\\Screenshot")))
            .or_else(|| std::env::var("USERPROFILE").ok().map(|p| format!("{p}\\Pictures\\Screenshots")))
            .unwrap_or_default();
        if !dir.is_empty() {
            let _ = std::fs::create_dir_all(&dir);
        }
        crate::log::log(&format!("tangkapan: FOLDERID_Screenshots gagal ({err}), pakai {dir}"));
    }
    let mut saved = false;
    if !dir.is_empty() {
        // SAFETY: GetLocalTime tidak punya prasyarat.
        let t = unsafe { GetLocalTime() };
        let stamp = format!(
            "{:04}-{:02}-{:02} {:02}{:02}{:02}",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
        );
        for k in 1..50 {
            let path = if k == 1 {
                format!("{}\\Screenshot {}.png", dir, stamp)
            } else {
                format!("{}\\Screenshot {} ({}).png", dir, stamp, k)
            };
            let pw = to_wide_z(&path);
            // SAFETY: pw berakhir NUL dan hidup sampai pemanggilan selesai.
            if unsafe { GetFileAttributesW(pcw(&pw)) } == INVALID_FILE_ATTRIBUTES {
                saved = hk_save_png(&path, px, w, h);
                break;
            }
        }
    }
    if saved {
        hk_toast("Tangkapan layar disimpan di Gambar\\Screenshot", 0xE722);
    } else if clip {
        hk_toast("Tangkapan layar disalin ke clipboard", 0xE722);
    } else {
        hk_toast("Tangkapan layar gagal", 0xE783);
    }
}

/// Win+Ctrl+C: filter warna, hanya bila pintasannya diizinkan di Pengaturan (sumber HkColorFilter, 538–544).
fn hk_color_filter() {
    // Nilai yang tak terbaca dianggap 0, sama dengan `DWORD on = 0` di sumber.
    let on = crate::reg::get_dword("Software\\Microsoft\\ColorFiltering", "HotkeyEnabled").unwrap_or(0);
    if on == 0 {
        hk_toast("Pintasan filter warna nonaktif di Pengaturan Aksesibilitas", 0xE790);
        return;
    }
    let _ = shell_exec(None, "atbroker.exe", Some("/colorfiltershortcut"), None);
}

/// Tombol Windows tunggal yang tidak sampai ke kait (sumber HkTaskList, 546–554).
/// Diabaikan bila kait baru saja menanganinya (kurang dari 600 ms).
pub fn hk_task_list() {
    // SAFETY: GetTickCount tidak punya prasyarat.
    let now = unsafe { GetTickCount() };
    if now.wrapping_sub(WIN_KEY_TICK.get()) < 600 {
        return;
    }
    let a = w12_start_shortcut_action(
        true,
        false,
        false,
        WIN_KEY.get(),
        QUITTING.get(),
        !MSG.get().0.is_null(),
    );
    if a != W12_STARTKEY_NONE {
        // PORT: start_from_key menerima i32 (core_c4b.rs:431).
        start_from_key(a as i32);
    }
}

// ---------------------------------------------------------------- tabel pintasan (sumber w12hotkeys.inc 556–694)

const HK_SHELLKEY: i32 = 1;
const HK_START: i32 = 2;
const HK_TASK: i32 = 3;
const HK_MINALL: i32 = 4;
const HK_RESTOREALL: i32 = 5;
const HK_MINOTHERS: i32 = 6;
const HK_PEEK: i32 = 7;
const HK_CYCLE: i32 = 8;
const HK_TRAY: i32 = 9;
const HK_CAL: i32 = 10;
const HK_SNAPMENU: i32 = 11;
const HK_SNAPLAYOUT: i32 = 12;
const HK_OPEN: i32 = 13;
const HK_SHOT: i32 = 14;
const HK_MIC: i32 = 15;
const HK_DOCK: i32 = 16;
const HK_COLOR: i32 = 17;
const HK_INFO: i32 = 18;
const HK_SHOT_WIN: i32 = 19; // Alt+PrtScn: tangkapan jendela aktif
const HK_CLIP: i32 = 20; // Win+V: panel riwayat clipboard (cliphist_ui)
const HK_EMOJI: i32 = 21; // Win+. dan Win+;: panel emoji (emoji_ui)

const HK_ID0: i32 = 0x5700;
const HK_MAXN: usize = 96;
const HK_M_WIN: u32 = MOD_WIN.0;
const HK_M_SHIFT: u32 = MOD_SHIFT.0;
const HK_M_CTRL: u32 = MOD_CONTROL.0;
const HK_M_ALT: u32 = MOD_ALT.0;
const HK_M_NOREPEAT: u32 = MOD_NOREPEAT.0;

/// Satu pintasan (sumber struct HkDef). `text` dipakai HK_OPEN dan HK_INFO; kosong bila tidak ada.
#[derive(Clone, Copy)]
struct HkDef {
    mods: u32,
    vk: u32,
    act: i32,
    arg: i32,
    arg2: i32,
    text: &'static str,
}

/// Pintasan beserta status terdaftar (sumber g_hk dan g_hkOn).
struct HkSlot {
    def: HkDef,
    on: bool,
}

/// Tabel pintasan mode shell. Diisi `hotkeys_start`, dikosongkan `hotkeys_stop`.
static HK_TABLE: SyncCell<RefCell<Vec<HkSlot>>> = SyncCell::new(RefCell::new(Vec::new()));

fn hk_add(t: &mut Vec<HkSlot>, mods: u32, vk: u32, act: i32, arg: i32, arg2: i32, text: &'static str) {
    if t.len() < HK_MAXN {
        t.push(HkSlot {
            def: HkDef {
                mods,
                vk,
                act,
                arg,
                arg2,
                text,
            },
            on: false,
        });
    }
}

/// Isi tabel sesuai urutan sumber; indeks menentukan id WM_HOTKEY (HK_ID0 + indeks). Sumber HkBuild, 572–624.
fn hk_build() -> Vec<HkSlot> {
    let mut t: Vec<HkSlot> = Vec::with_capacity(HK_MAXN);
    let wm = HK_M_WIN;
    let sm = HK_M_SHIFT;
    let cm = HK_M_CTRL;
    let am = HK_M_ALT;
    // Pintasan yang sudah ditangani kait: didaftarkan juga agar tetap bekerja saat jendela admin fokus.
    let shell_keys = [
        b'E' as u32,
        b'R' as u32,
        b'I' as u32,
        b'D' as u32,
        b'A' as u32,
        b'X' as u32,
        VK_LEFT.0 as u32,
        VK_RIGHT.0 as u32,
        VK_UP.0 as u32,
        VK_DOWN.0 as u32,
    ];
    for vk in shell_keys {
        hk_add(&mut t, wm, vk, HK_SHELLKEY, vk as i32, 0, "");
    }
    for n in 1..=9u32 {
        let k = b'0' as u32 + n;
        hk_add(&mut t, wm, k, HK_SHELLKEY, k as i32, 0, "");
        hk_add(&mut t, wm | sm, k, HK_DOCK, n as i32, 0, ""); // instans baru
        hk_add(&mut t, wm | cm, k, HK_DOCK, n as i32, 1, ""); // jendela terakhir
        hk_add(&mut t, wm | am, k, HK_DOCK, n as i32, 2, ""); // menu app (jump list)
    }
    hk_add(&mut t, wm | sm, b'S' as u32, HK_SHELLKEY, b'S' as i32, 0, "");
    hk_add(&mut t, wm, b'S' as u32, HK_START, W12_STARTKEY_SEARCH, 0, "");
    hk_add(&mut t, wm, b'Q' as u32, HK_START, W12_STARTKEY_SEARCH, 0, "");
    hk_add(&mut t, wm, VK_TAB.0 as u32, HK_TASK, TK_TASKVIEW, 0, "");
    hk_add(&mut t, wm | cm, b'D' as u32, HK_TASK, TK_DESK_NEW, 0, "");
    hk_add(&mut t, wm | cm, VK_F4.0 as u32, HK_TASK, TK_DESK_CLOSE, 0, "");
    hk_add(&mut t, wm | cm, VK_LEFT.0 as u32, HK_TASK, TK_DESK_GO, -1, "");
    hk_add(&mut t, wm | cm, VK_RIGHT.0 as u32, HK_TASK, TK_DESK_GO, 1, "");
    hk_add(&mut t, wm | cm | sm, VK_LEFT.0 as u32, HK_TASK, TK_MOVE_WIN, -1, "");
    hk_add(&mut t, wm | cm | sm, VK_RIGHT.0 as u32, HK_TASK, TK_MOVE_WIN, 1, "");
    // Pintasan baru (dulu tidak berbuat apa-apa tanpa Explorer).
    hk_add(&mut t, wm, b'M' as u32, HK_MINALL, 0, 0, "");
    hk_add(&mut t, wm | sm, b'M' as u32, HK_RESTOREALL, 0, 0, "");
    hk_add(&mut t, wm, VK_HOME.0 as u32, HK_MINOTHERS, 0, 0, "");
    hk_add(&mut t, wm, VK_OEM_COMMA.0 as u32, HK_PEEK, 0, 0, "");
    hk_add(&mut t, wm, b'W' as u32, HK_SHELLKEY, b'D' as i32, 0, ""); // widget Win12 ada di desktop: tampilkan desktop
    hk_add(&mut t, wm, b'T' as u32, HK_CYCLE, 1, 0, "");
    hk_add(&mut t, wm | sm, b'T' as u32, HK_CYCLE, -1, 0, "");
    hk_add(&mut t, wm, b'B' as u32, HK_TRAY, 0, 0, "");
    hk_add(&mut t, wm, b'N' as u32, HK_CAL, 0, 0, "");
    hk_add(&mut t, wm | am, b'D' as u32, HK_CAL, 0, 0, "");
    hk_add(&mut t, wm, b'Z' as u32, HK_SNAPMENU, 0, 0, "");
    hk_add(&mut t, wm | am, VK_UP.0 as u32, HK_SNAPLAYOUT, SNL_TOP, 0, "");
    hk_add(&mut t, wm | am, VK_DOWN.0 as u32, HK_SNAPLAYOUT, SNL_BOTTOM, 0, "");
    hk_add(&mut t, wm, VK_PAUSE.0 as u32, HK_OPEN, 0, 0, "ms-settings:about");
    hk_add(&mut t, wm, VK_SNAPSHOT.0 as u32, HK_SHOT, 0, 0, "");
    hk_add(&mut t, wm | am, b'K' as u32, HK_MIC, 0, 0, "");
    hk_add(&mut t, wm | cm, b'O' as u32, HK_OPEN, 0, 0, "osk.exe");
    hk_add(&mut t, wm | cm, b'N' as u32, HK_OPEN, 0, 0, "ms-settings:easeofaccess-narrator");
    hk_add(&mut t, wm | cm, b'M' as u32, HK_OPEN, 0, 0, "ms-settings:easeofaccess-magnifier");
    hk_add(&mut t, wm | cm, b'C' as u32, HK_COLOR, 0, 0, "");
    hk_add(&mut t, wm, b'F' as u32, HK_OPEN, 0, 0, "feedback-hub:");
    hk_add(&mut t, wm | cm, b'Q' as u32, HK_OPEN, 0, 0, "ms-quick-assist:");
    // Bagian dari Explorer/TextInputHost yang belum ada penggantinya: beri tahu, jangan diam saja.
    hk_add(&mut t, wm, b'V' as u32, HK_CLIP, 0, 0, "");
    hk_add(&mut t, wm, VK_OEM_PERIOD.0 as u32, HK_EMOJI, 0, 0, "");
    hk_add(&mut t, wm, VK_OEM_1.0 as u32, HK_EMOJI, 0, 0, "");
    hk_add(&mut t, wm, b'H' as u32, HK_INFO, 0, 0, "Dikte suara butuh Windows biasa");
    hk_add(&mut t, wm, b'G' as u32, HK_INFO, 0, 0, "Game Bar tidak terpasang");
    // PrtScn polos: layar penuh seperti Win+PrtScn. Alt+PrtScn: jendela aktif.
    // Ditaruh di akhir agar indeks (id WM_HOTKEY) pintasan lama tidak bergeser.
    hk_add(&mut t, 0, VK_SNAPSHOT.0 as u32, HK_SHOT, 0, 0, "");
    hk_add(&mut t, am, VK_SNAPSHOT.0 as u32, HK_SHOT_WIN, 0, 0, "");
    t
}

/// Nama pintasan untuk pesan gagal, mis. "Win+Ctrl+D" (sumber HkName, 626–633).
fn hk_name(d: &HkDef) -> String {
    let key = if d.vk >= b'0' as u32 && d.vk <= b'Z' as u32 {
        (d.vk as u8 as char).to_string()
    } else {
        format!("vk{:02X}", d.vk)
    };
    format!(
        "Win{}{}{}+{}",
        if d.mods & HK_M_CTRL != 0 { "+Ctrl" } else { "" },
        if d.mods & HK_M_SHIFT != 0 { "+Shift" } else { "" },
        if d.mods & HK_M_ALT != 0 { "+Alt" } else { "" },
        key
    )
}

/// Daftarkan pintasan mode shell ke jendela pesan MSG dan catat yang gagal (sumber HotkeysStart, 635–651).
pub fn hotkeys_start() {
    let msg = MSG.get();
    if msg.0.is_null() {
        return;
    }
    super::cliphist::ch_start(msg);
    let mut t = hk_build();
    let mut ok = 0usize;
    let mut taken = String::new();
    for (i, slot) in t.iter_mut().enumerate() {
        let id = HK_ID0 + i as i32;
        // SAFETY: msg adalah jendela pesan milik shell; id unik dalam rentang HK_ID0.
        slot.on = unsafe {
            RegisterHotKey(Some(msg), id, HOT_KEY_MODIFIERS(slot.def.mods | HK_M_NOREPEAT), slot.def.vk)
        }
        .is_ok();
        if slot.on {
            ok += 1;
            continue;
        }
        let nm = hk_name(&slot.def);
        if taken.len() + nm.len() + 2 < 400 {
            if !taken.is_empty() {
                taken.push(' ');
            }
            taken.push_str(&nm);
        }
    }
    let total = t.len();
    let suffix = if taken.is_empty() {
        String::new()
    } else {
        format!(": {}", taken)
    };
    log(&format!(
        "pintasan mode shell: {} terdaftar, {} dipegang proses lain{}",
        ok,
        total - ok,
        suffix
    ));
    *HK_TABLE.borrow_mut() = t;
}

/// Lepas semua pintasan mode shell dan tutup jendela peek (sumber HotkeysStop, 653–660).
pub fn hotkeys_stop() {
    let msg = MSG.get();
    if msg.0.is_null() {
        return;
    }
    super::cliphist::ch_stop(msg);
    {
        let mut t = HK_TABLE.borrow_mut();
        for (i, slot) in t.iter_mut().enumerate() {
            if slot.on {
                // SAFETY: id sama dengan yang didaftarkan hotkeys_start.
                let _ = unsafe { UnregisterHotKey(Some(msg), HK_ID0 + i as i32) };
                slot.on = false;
            }
        }
        t.clear();
    }
    hk_peek_end();
}

/// Jalankan aksi pintasan dengan id WM_HOTKEY `id`. false bila id bukan milik tabel ini (sumber HotkeyRun, 663–694).
pub fn hotkey_run(id: i32) -> bool {
    let i = id - HK_ID0;
    if i < 0 || !SHELL_MODE.get() {
        return false;
    }
    // Salin definisinya agar borrow tabel lepas sebelum aksi apa pun dijalankan.
    let def = {
        let t = HK_TABLE.borrow();
        match t.get(i as usize) {
            Some(slot) => slot.def,
            None => return false,
        }
    };
    match def.act {
        HK_SHELLKEY => {
            shell_key(def.arg);
        }
        HK_START => {
            // SAFETY: GetTickCount tidak punya prasyarat.
            WIN_KEY_TICK.set(unsafe { GetTickCount() });
            // PORT: start_from_key menerima i32 (core_c4b.rs:431).
            start_from_key(def.arg as i32);
        }
        HK_TASK => {
            let tk = TK_MSG.get();
            if !tk.0.is_null() {
                // SAFETY: tk adalah jendela TaskView milik shell; WPARAM/LPARAM hanya berisi bilangan.
                let _ = unsafe {
                    PostMessageW(Some(tk), WM_W12_TASK, WPARAM(def.arg as usize), LPARAM(def.arg2 as isize))
                };
            }
        }
        HK_MINALL => {
            desk_minimize_all(None);
        }
        HK_RESTOREALL => {
            desk_restore_all();
        }
        HK_MINOTHERS => {
            // Win+Home kedua kali: kembalikan semua jendela.
            if !desk_restore_all() {
                // SAFETY: GetForegroundWindow tidak punya prasyarat.
                let fg = unsafe { GetForegroundWindow() };
                if !fg.0.is_null() && is_task_window(fg) {
                    desk_minimize_all(Some(fg));
                }
            }
        }
        HK_PEEK => {
            hk_peek();
        }
        HK_CYCLE => {
            hk_cycle_dock(def.arg);
        }
        HK_TRAY => {
            if !hk_item(IT_CHEV) {
                hk_toast("Tidak ada ikon tray tersembunyi", 0xE946);
            }
        }
        HK_CAL => {
            hk_calendar();
        }
        HK_SNAPMENU => {
            snap_layout_menu();
        }
        HK_SNAPLAYOUT => {
            // SAFETY: GetForegroundWindow tidak punya prasyarat.
            let fg = unsafe { GetForegroundWindow() };
            snap_layout(fg, def.arg);
        }
        HK_OPEN => {
            shell_open(def.text);
        }
        HK_SHOT => {
            hk_screenshot();
        }
        HK_SHOT_WIN => {
            hk_screenshot_window();
        }
        HK_MIC => {
            hk_mic_mute();
        }
        HK_DOCK => {
            hk_dock(def.arg, def.arg2);
        }
        HK_COLOR => {
            hk_color_filter();
        }
        HK_CLIP => {
            super::cliphist_ui::clp_toggle();
        }
        HK_EMOJI => {
            super::emoji_ui::emo_toggle();
        }
        HK_INFO => {
            hk_toast(def.text, 0xE946);
        }
        _ => {}
    }
    true
}

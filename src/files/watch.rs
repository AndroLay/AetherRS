//! Pantau folder aktif tiap jendela dan perbarui tampilan saat isinya berubah (2026-11-19).
//!
//! Di mode shell tanpa explorer.exe, DefView tidak menerima pemberitahuan perubahan (SHChangeNotify), jadi berkas
//! yang dihapus/dibuat baru terlihat setelah F5. Tiap jendela memegang satu handle FindFirstChangeNotificationW
//! (bukan ReadDirectoryChangesW, lihat kebocoran IRP 2026-11-13) untuk folder tab aktif; loop pesan menunggunya
//! bersama pesan. Sinyal -> timer debounce -> isi folder dibandingkan dengan potret terakhir (nama + ukuran) ->
//! hanya item yang berubah dimasukkan/dikeluarkan lewat IShellFolderView::AddObject/RemoveObject.
//! Tidak ada muat ulang penuh: gambar mini yang sudah jadi, gulir, dan pilihan tetap; memori hanya potret nama.
//!
//! Versi pertama memakai IShellView::Refresh dan juga memantau atribut; OneDrive terus mengubah atribut berkas
//! sehingga tampilan dimuat ulang berulang dan gambar mini tidak pernah selesai.

use std::cell::RefCell;
use std::collections::HashMap;
use std::os::windows::fs::MetadataExt;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification, FILE_NOTIFY_CHANGE,
    FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_SIZE,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
use windows::Win32::UI::Shell::{
    IFolderView, IShellFolder, IShellFolderView, SHGetPathFromIDListW, StrRetToStrW, SHGDNF, SHGDN_FORPARSING,
    SHGDN_INFOLDER, SVGIO_ALLVIEW,
};
use windows::core::Interface;
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, IsChild, KillTimer, SetTimer};

use crate::files::state::{FmWin, WINS};
use crate::sync::SyncCell;

/// Timer jendela: perbarui tampilan sesaat setelah perubahan terakhir.
pub(crate) const FM_TIMER_WATCH: usize = 9;
const DEBOUNCE_MS: u32 = 300;
/// Di atas ini potret tidak disimpan (hemat memori); perubahan memakai Refresh penuh.
const MAX_SNAP: usize = 20_000;
/// Perubahan sebanyak ini sekaligus (salin/hapus massal) lebih cepat dengan Refresh penuh.
const MAX_INCR: usize = 300;

/// Nama huruf kecil -> (nama asli, ukuran).
type Snap = HashMap<String, (String, u64)>;

struct Watch {
    h: HWND,
    /// PIDL tab aktif saat handle dibuat (penanda murah: berubah tiap navigasi/ganti tab).
    pidl: *const ITEMIDLIST,
    ev: HANDLE,
    dir: String,
    snap: Option<Snap>,
}

static WATCHES: SyncCell<RefCell<Vec<Watch>>> = SyncCell::new(RefCell::new(Vec::new()));

fn close(ev: HANDLE) {
    if !ev.is_invalid() && !ev.0.is_null() {
        // SAFETY: handle dari FindFirstChangeNotificationW milik modul ini.
        let _ = unsafe { FindCloseChangeNotification(ev) };
    }
}

/// Isi folder tanpa berkas tersembunyi/sistem (yang juga disembunyikan tampilan bawaan). None bila terlalu besar.
fn list(dir: &str) -> Option<Snap> {
    const HIDDEN_SYSTEM: u32 = 0x2 | 0x4;
    let mut m = Snap::new();
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let Ok(md) = e.metadata() else { continue };
        if md.file_attributes() & HIDDEN_SYSTEM != 0 {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        m.insert(name.to_lowercase(), (name, if md.is_dir() { 0 } else { md.len() }));
        if m.len() > MAX_SNAP {
            return None;
        }
    }
    Some(m)
}

fn open(pidl: *const ITEMIDLIST) -> (HANDLE, String) {
    if pidl.is_null() {
        return (HANDLE::default(), String::new());
    }
    let mut buf = [0u16; 260];
    // SAFETY: buffer MAX_PATH; pidl valid selama tab hidup.
    if !unsafe { SHGetPathFromIDListW(pidl, &mut buf) }.as_bool() || buf[0] == 0 {
        return (HANDLE::default(), String::new()); // folder virtual (This PC, Beranda, dst.): biarkan DefView
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let dir = String::from_utf16_lossy(&buf[..n]);
    // Nama dan ukuran saja; atribut sengaja tidak (OneDrive terus mengubahnya).
    let filter = FILE_NOTIFY_CHANGE(FILE_NOTIFY_CHANGE_FILE_NAME.0 | FILE_NOTIFY_CHANGE_DIR_NAME.0 | FILE_NOTIFY_CHANGE_SIZE.0);
    // SAFETY: buf berakhir NUL.
    match unsafe { FindFirstChangeNotificationW(PCWSTR(buf.as_ptr()), false, filter) } {
        Ok(h) if !h.is_invalid() => (h, dir),
        _ => (HANDLE::default(), String::new()),
    }
}

/// Selaraskan daftar pantauan dengan folder tab aktif tiap jendela. Murah bila tidak ada yang berubah.
/// Mengembalikan handle yang perlu ditunggu loop pesan.
pub(crate) fn sync() -> Vec<HANDLE> {
    let mut want: Vec<(HWND, *const ITEMIDLIST)> = Vec::new();
    for &p in WINS.borrow().iter() {
        if p.is_null() {
            continue;
        }
        // SAFETY: penunjuk FmWin hidup selama terdaftar di WINS (dilepas di WM_NCDESTROY).
        let w: &FmWin = unsafe { &*p };
        let a = w.tabs.active;
        if a >= 0 && (a as usize) < w.tab.len() {
            want.push((w.h, w.tab[a as usize].cur as *const ITEMIDLIST));
        }
    }
    let mut ws = WATCHES.borrow_mut();
    ws.retain(|x| {
        let keep = want.iter().any(|(h, p)| *h == x.h && *p == x.pidl);
        if !keep {
            close(x.ev);
        }
        keep
    });
    for (h, p) in want {
        if !ws.iter().any(|x| x.h == h) {
            let (ev, dir) = open(p);
            // Potret diambil saat folder dibuka: perubahan sesudahnya dibandingkan dengan ini.
            let snap = if ev.0.is_null() { None } else { list(&dir) };
            ws.push(Watch { h, pidl: p, ev, dir, snap });
        }
    }
    ws.iter().filter(|x| !x.ev.0.is_null() && !x.ev.is_invalid()).map(|x| x.ev).collect()
}

/// Handle `ev` berbunyi: pasang lagi dan jadwalkan pembaruan jendela pemiliknya.
pub(crate) fn fired(ev: HANDLE) {
    let ws = WATCHES.borrow();
    if let Some(x) = ws.iter().find(|x| x.ev == ev) {
        // SAFETY: handle valid milik modul ini; timer pada jendela frame thread ini.
        unsafe {
            let _ = FindNextChangeNotification(ev);
            let _ = SetTimer(Some(x.h), FM_TIMER_WATCH, DEBOUNCE_MS, None);
        }
    }
}

/// Nama parsing item (relatif terhadap folder) dalam huruf kecil.
unsafe fn child_name(sf: &IShellFolder, pidl: *const ITEMIDLIST) -> Option<String> {
    let mut sr = STRRET::default();
    sf.GetDisplayNameOf(pidl, SHGDNF(SHGDN_INFOLDER.0 | SHGDN_FORPARSING.0), &mut sr).ok()?;
    let mut p = PWSTR::null();
    StrRetToStrW(&mut sr, Some(pidl), &mut p).ok()?;
    let s = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as *const _));
    s.map(|s| s.to_lowercase())
}

/// Timer berbunyi: masukkan/keluarkan item yang berubah, kecuali pengguna sedang mengganti nama.
pub(crate) fn on_timer(w: &mut FmWin) {
    // SAFETY: timer milik jendela ini.
    let _ = unsafe { KillTimer(Some(w.h), FM_TIMER_WATCH) };
    // Dengan explorer.exe berjalan DefView sudah memperbarui sendiri; jangan dobel.
    if crate::files::frame::explorer_process_running() {
        return;
    }
    let h = w.h;
    let Some(t) = crate::files::app::fm_active(w) else { return };
    let Some(sv) = crate::files::app::fm_current_view(t) else { return };
    // SAFETY: GetWindow/GetFocus hanya membaca keadaan jendela.
    unsafe {
        if let Ok(hv) = sv.GetWindow() {
            let f = GetFocus();
            if !f.0.is_null() && IsChild(hv, f).as_bool() {
                let mut cls = [0u16; 16];
                let n = GetClassNameW(f, &mut cls) as usize;
                if String::from_utf16_lossy(&cls[..n]).eq_ignore_ascii_case("Edit") {
                    let _ = SetTimer(Some(h), FM_TIMER_WATCH, 1000, None);
                    return;
                }
            }
        }
    }
    let mut ws = WATCHES.borrow_mut();
    let Some(x) = ws.iter_mut().find(|x| x.h == h) else { return };
    let now = list(&x.dir);
    let (Some(old), Some(new)) = (x.snap.as_ref(), now.as_ref()) else {
        x.snap = now;
        let _ = unsafe { sv.Refresh() };
        return;
    };
    // Ukuran berubah = keluarkan lalu masukkan lagi (gambar mini dan ukuran diperbarui).
    let removed: Vec<&String> = old
        .iter()
        .filter(|(k, (_, sz))| new.get(*k).is_none_or(|(_, s2)| s2 != sz))
        .map(|(k, _)| k)
        .collect();
    let added: Vec<&String> = new
        .iter()
        .filter(|(k, (_, sz))| old.get(*k).is_none_or(|(_, s0)| s0 != sz))
        .map(|(_, (orig, _))| orig)
        .collect();
    if removed.is_empty() && added.is_empty() {
        return;
    }
    let ok = removed.len() + added.len() <= MAX_INCR && unsafe { apply_incr(&sv, &removed, &added) };
    if !ok {
        let _ = unsafe { sv.Refresh() };
    }
    x.snap = now;
    drop(ws);
    crate::files::viewstate::fill_soon(h);
}

/// Keluarkan `removed` (nama huruf kecil) dan masukkan `added` (nama asli) ke tampilan. False bila antarmuka tidak ada.
unsafe fn apply_incr(sv: &windows::Win32::UI::Shell::IShellView, removed: &[&String], added: &[&String]) -> bool {
    let (Ok(sfv), Ok(fv)) = (sv.cast::<IShellFolderView>(), sv.cast::<IFolderView>()) else { return false };
    let Ok(sf) = fv.GetFolder::<IShellFolder>() else { return false };
    if !removed.is_empty() {
        // Kumpulkan dulu PIDL yang cocok (indeks bergeser setelah RemoveObject), lalu keluarkan.
        let n = fv.ItemCount(SVGIO_ALLVIEW).unwrap_or(0);
        let mut kill: Vec<*mut ITEMIDLIST> = Vec::new();
        for i in 0..n {
            if let Ok(p) = fv.Item(i) {
                if child_name(&sf, p).is_some_and(|s| removed.iter().any(|r| **r == s)) {
                    kill.push(p);
                } else {
                    CoTaskMemFree(Some(p as *const _));
                }
            }
        }
        for p in kill {
            let _ = sfv.RemoveObject(Some(p));
            CoTaskMemFree(Some(p as *const _));
        }
    }
    for name in added {
        let wn = crate::wide::to_wide_z(name);
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        if sf.ParseDisplayName(HWND::default(), None, PCWSTR(wn.as_ptr()), None, &mut pidl, std::ptr::null_mut()).is_ok()
            && !pidl.is_null()
        {
            let _ = sfv.AddObject(pidl);
            // DefView menyalin PIDL (ILClone, seperti CDefView ReactOS); salinan kita dibebaskan.
            CoTaskMemFree(Some(pidl as *const _));
        }
    }
    true
}

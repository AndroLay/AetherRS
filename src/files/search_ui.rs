//! Sumber: `w12fm_search.inc` 1-191 (dipakai `win12files.cpp`). Status: lengkap, belum diuji runtime.
//! Penyimpangan: field `n` ditiadakan (jumlah = `pidl.len()`); PIDL dibebaskan lewat `free_result`, bukan Drop; InitializeCriticalSection diganti `FX_CS` (Mutex statis).
//! Lintas-file: `files::app` (fm_active, fm_pos, fm_status, fm_focus_view), `files::model` (w12_fx_*), `fm_logic::w12_fm_under_folder`.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH, WPARAM};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, IBindCtx, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows::Win32::System::Threading::{CreateThread, THREAD_CREATION_FLAGS};
use windows::Win32::UI::Controls::EM_SETCUEBANNER;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    SHCreateShellItemArrayFromIDLists, SHGetPathFromIDListW, SHParseDisplayName, EBF_NONE,
    SBSP_ABSOLUTE,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SendMessageW, SetWindowTextW};

use crate::files::app::{fm_active, fm_focus_view, fm_pos, fm_status};
use crate::files::model::{w12_fx_close, w12_fx_open, w12_fx_path, w12_fx_search};
use crate::files::state::{
    FmSearchJob, FmSearchResult, FmWin, FM_SEARCH_MAX, FM_SEARCH_PATH, FM_SEARCH_QUERY,
    FM_SEARCH_SCAN, FX_CS, FX_OPEN, FX_PATH, FX_VIEW, MSG_SEARCH_DONE, PCIDLIST_ABSOLUTE,
};
use crate::fm_logic::w12_fm_under_folder;
use crate::search::{w12_search_make_query, W12SearchHit, W12SearchQuery};
use crate::wide::{from_wide, pcw, to_wide, to_wide_z};

/// Potong di NUL pertama (perilaku string C).
fn until_nul(s: &[u16]) -> &[u16] {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    &s[..end]
}

/// Unit UTF-16 dari `s` sampai NUL pertama, dibatasi `max` unit (sama dengan lstrcpynW).
fn clip_utf16(s: &str, max: usize) -> Vec<u16> {
    s.encode_utf16().take_while(|&c| c != 0).take(max).collect()
}

/// FmSearchInit: jalur indeks = %LOCALAPPDATA%\Win12Shell\files.idx. Bila variabel tidak ada
/// atau terlalu panjang, jalur tetap kosong dan indeks tidak akan dibuka.
pub fn fm_search_init() {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let local: Vec<u16> = local.encode_wide().collect();
    if local.is_empty() || local.len() >= MAX_PATH as usize - 32 {
        return;
    }
    let mut path = local;
    path.extend(to_wide("\\Win12Shell\\files.idx"));
    let mut arr = [0u16; MAX_PATH as usize];
    let n = path.len().min(arr.len() - 1);
    arr[..n].copy_from_slice(&path[..n]);
    *FX_PATH.borrow_mut() = arr;
}

/// FmSearchShutdown: tutup indeks bila sedang terbuka.
pub fn fm_search_shutdown() {
    let _guard = FX_CS.lock().unwrap_or_else(|e| e.into_inner());
    if FX_OPEN.get() {
        w12_fx_close(&mut FX_VIEW.borrow_mut());
        FX_OPEN.set(false);
    }
}

/// FmNowFt: waktu sekarang dalam satuan FILETIME (100 ns sejak 1601).
fn fm_now_ft() -> u64 {
    // SAFETY: GetSystemTimeAsFileTime tidak punya prasyarat; nilainya hanya dibaca.
    let ft = unsafe { GetSystemTimeAsFileTime() };
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

/// FmSearchFreeResult: bebaskan setiap PIDL. Wadah `Vec` dibebaskan oleh pemanggil (Box::from_raw).
fn free_result(res: &FmSearchResult) {
    for &pidl in &res.pidl {
        // SAFETY: setiap PIDL dibuat SHParseDisplayName (CoTaskMemAlloc) dan hanya dibebaskan sekali di sini.
        unsafe { CoTaskMemFree(Some(pidl as *const c_void)) };
    }
}

/// Pasang teks ke jendela (SetWindowTextW). Kegagalan diabaikan, sama dengan C.
fn set_text(h: HWND, s: &str) {
    let wide = to_wide_z(s);
    // SAFETY: wide berakhir NUL dan hidup selama pemanggilan.
    let _ = unsafe { SetWindowTextW(h, pcw(&wide)) };
}

/// SHParseDisplayName untuk satu jalur. Mengembalikan PIDL (CoTaskMemAlloc) atau None.
fn parse_display_name(path: &str) -> Option<PCIDLIST_ABSOLUTE> {
    let wide = to_wide_z(path);
    let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
    // SAFETY: wide berakhir NUL dan hidup selama pemanggilan; pidl ditulis oleh fungsi.
    let res = unsafe { SHParseDisplayName(pcw(&wide), None::<&IBindCtx>, &mut pidl, 0, None) };
    if res.is_ok() && !pidl.is_null() {
        Some(pidl as PCIDLIST_ABSOLUTE)
    } else {
        None
    }
}

/// FmSearchCollect: kumpulkan PIDL hasil dari indeks untuk kueri dan folder. Dipanggil di bawah FX_CS.
/// Folder kosong = tanpa filter (sesuai `W12FmUnderFolder`).
fn fm_search_collect(query: &[u16], folder: &[u16]) -> Vec<PCIDLIST_ABSOLUTE> {
    let mut out: Vec<PCIDLIST_ABSOLUTE> = Vec::new();
    let text = from_wide(until_nul(query));
    let mut sq = W12SearchQuery::default();
    w12_search_make_query(&text, &mut sq);
    if sq.length <= 0 {
        return out;
    }
    let folder_text = from_wide(until_nul(folder));
    let view = FX_VIEW.borrow();
    let mut hits = vec![W12SearchHit::default(); FM_SEARCH_SCAN];
    let n = w12_fx_search(&view, &sq, fm_now_ft(), &mut hits);
    if n <= 0 {
        return out;
    }
    let mut path = [0u16; FM_SEARCH_PATH];
    for hit in hits.iter().take(n as usize) {
        if out.len() >= FM_SEARCH_MAX {
            break;
        }
        let units = w12_fx_path(&view, hit.reference as u32, &mut path);
        if units <= 0 {
            continue;
        }
        let units = (units as usize).min(path.len());
        let full = from_wide(until_nul(&path[..units]));
        if !w12_fm_under_folder(&full, Some(folder_text.as_str())) {
            continue;
        }
        if let Some(pidl) = parse_display_name(&full) {
            out.push(pidl);
        }
    }
    out
}

/// FmSearchThread: pekerja pencarian di thread terpisah. Membuka indeks bila belum terbuka,
/// mengumpulkan PIDL, lalu mengirim hasil ke jendela lewat MSG_SEARCH_DONE.
/// `param` adalah `Box<FmSearchJob>` dari `fm_search_start`; thread ini yang memilikinya.
unsafe extern "system" fn fm_search_thread(param: *mut c_void) -> u32 {
    if param.is_null() {
        return 0;
    }
    // SAFETY: param dibuat Box::into_raw di fm_search_start dan hanya dipakai thread ini.
    let job: Box<FmSearchJob> = unsafe { Box::from_raw(param as *mut FmSearchJob) };
    // SAFETY: CoInitializeEx dipanggil sekali per thread; hr menentukan CoUninitialize di akhir.
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let mut res = FmSearchResult {
        gen: job.gen,
        tab_id: job.tab_id,
        indexed: false,
        pidl: Vec::new(),
    };
    {
        let _guard = FX_CS.lock().unwrap_or_else(|e| e.into_inner());
        if !FX_OPEN.get() {
            let path = from_wide(until_nul(&FX_PATH.borrow()[..]));
            let ok = w12_fx_open(&path, &mut FX_VIEW.borrow_mut());
            FX_OPEN.set(ok);
        }
        res.indexed = FX_OPEN.get();
        if res.indexed {
            res.pidl = fm_search_collect(&job.query, &job.folder);
        }
    }
    let raw = Box::into_raw(Box::new(res));
    // SAFETY: job.hwnd adalah jendela file; bila pengiriman gagal, kepemilikan kembali ke thread ini.
    let posted = unsafe {
        PostMessageW(Some(job.hwnd), MSG_SEARCH_DONE, WPARAM(0), LPARAM(raw as isize))
    };
    if posted.is_err() {
        // SAFETY: raw berasal dari Box::into_raw di atas dan belum diserahkan ke jendela.
        let res = unsafe { Box::from_raw(raw) };
        free_result(&res);
    }
    drop(job);
    if hr.is_ok() {
        // SAFETY: pasangan dari CoInitializeEx yang berhasil (S_OK atau S_FALSE).
        unsafe { CoUninitialize() };
    }
    0
}

/// FmSearchStart: jalankan pencarian indeks untuk tab aktif di thread terpisah. Hasil datang lewat MSG_SEARCH_DONE.
pub fn fm_search_start(w: &mut FmWin, text: &str) {
    let (has_eb, cur) = match fm_active(w) {
        Some(t) => (t.eb.is_some(), t.cur),
        None => return,
    };
    if !has_eb {
        return;
    }
    let mut job = Box::new(FmSearchJob {
        hwnd: w.h,
        gen: 0,
        tab_id: 0,
        query: [0; FM_SEARCH_QUERY],
        folder: [0; MAX_PATH as usize],
    });
    let query = clip_utf16(text, FM_SEARCH_QUERY - 1);
    job.query[..query.len()].copy_from_slice(&query);
    if !cur.is_null() {
        let mut p = [0u16; MAX_PATH as usize];
        // SAFETY: cur adalah PIDL folder tab aktif yang masih hidup; p berukuran MAX_PATH.
        if unsafe { SHGetPathFromIDListW(cur, &mut p) }.as_bool() {
            let n = until_nul(&p).len().min(job.folder.len() - 1);
            job.folder[..n].copy_from_slice(&p[..n]);
        }
    }
    job.tab_id = w.tabs.id[w.tabs.active as usize];
    w.search_gen = w.search_gen.wrapping_add(1);
    job.gen = w.search_gen;
    let raw = Box::into_raw(job);
    // SAFETY: raw dimiliki thread baru bila CreateThread berhasil; bila gagal, dibebaskan di bawah.
    let created = unsafe {
        CreateThread(
            None,
            0,
            Some(fm_search_thread),
            Some(raw as *const c_void),
            THREAD_CREATION_FLAGS(0),
            None,
        )
    };
    match created {
        Ok(th) => {
            // SAFETY: th adalah handle thread yang baru dibuat dan tidak dipakai lagi.
            let _ = unsafe { CloseHandle(th) };
            fm_status(w, "Mencari...", false);
        }
        Err(_) => {
            // SAFETY: thread gagal dibuat, jadi raw masih milik thread UI.
            drop(unsafe { Box::from_raw(raw) });
            fm_status(w, "Pencarian tidak bisa dimulai.", true);
        }
    }
}

/// FmSearchCancel (Esc): buang pencarian yang berjalan, kosongkan kotak, lalu kembali ke folder tab aktif.
pub fn fm_search_cancel(w: &mut FmWin) {
    w.search_gen = w.search_gen.wrapping_add(1);
    if !w.search.0.is_null() {
        set_text(w.search, "");
    }
    // Salin eb (AddRef) dan cur, lalu lepas pinjaman sebelum memanggil COM.
    let target = match fm_active(w) {
        Some(t) if t.searching => match &t.eb {
            Some(eb) if !t.cur.is_null() => Some((eb.clone(), t.cur)),
            _ => None,
        },
        _ => None,
    };
    if let Some((eb, cur)) = target {
        if let Some(t) = fm_active(w) {
            t.searching = false;
            // Folder ini sudah tercatat di riwayat: navigasi kembali ini tidak mencatat lagi.
            t.hist_pending = true;
        }
        // SAFETY: cur adalah PIDL folder tab yang masih dipegang tab.
        let ok = unsafe { eb.BrowseToIDList(cur, SBSP_ABSOLUTE) }.is_ok();
        if !ok {
            if let Some(t) = fm_active(w) {
                t.hist_pending = false;
            }
        }
    }
    fm_status(w, "", false);
    if let Some(t) = fm_active(w) {
        fm_focus_view(t);
    }
}

/// FmSearchDone: hasil pencarian tiba di thread UI. Dipasang sebagai isi tab bila generasinya masih yang terbaru.
/// # Safety
/// `lparam` harus berisi pointer `Box<FmSearchResult>` yang dikirim `fm_search_thread` lewat MSG_SEARCH_DONE.
pub unsafe fn fm_search_done(w: &mut FmWin, lparam: LPARAM) {
    if lparam.0 == 0 {
        return;
    }
    // SAFETY: pointer dari Box::into_raw di fm_search_thread; kepemilikan pindah ke sini.
    let res: Box<FmSearchResult> = unsafe { Box::from_raw(lparam.0 as *mut FmSearchResult) };
    let pos = fm_pos(w, res.tab_id);
    let usable = pos >= 0
        && res.gen == w.search_gen
        && w.tab.get(pos as usize).is_some_and(|t| t.eb.is_some());
    if !usable {
        free_result(&res);
        return;
    }
    let idx = pos as usize;
    if !res.indexed {
        fm_status(w, "Indeks pencarian belum tersedia.", true);
        free_result(&res);
        return;
    }
    let eb = w.tab[idx].eb.clone();
    let mut ok = false;
    if let Some(eb) = eb {
        // SAFETY: res.pidl berisi PIDL hidup dari SHParseDisplayName dan masih dipegang `res`.
        if let Ok(psia) = unsafe { SHCreateShellItemArrayFromIDLists(&res.pidl) } {
            // SAFETY: eb dan psia adalah objek COM hidup di thread UI.
            ok = unsafe { eb.FillFromObject(&psia, EBF_NONE) }.is_ok();
            // psia dilepas sebelum PIDL dibebaskan, sama dengan Release di C.
            drop(psia);
        }
    }
    if ok {
        w.tab[idx].searching = true;
    }
    let n = res.pidl.len();
    let text = if n == 0 {
        "Tidak ada hasil di folder ini.".to_string()
    } else {
        format!("{n} hasil pencarian.")
    };
    fm_status(w, &text, false);
    free_result(&res);
}

/// FmSearchDone tanpa jendela (C: `FmSearchDone(nullptr, res)` → FmSearchFreeResult, w12fm_search.inc:156-160).
/// Kepemilikan Box diambil dan dibebaskan di sini agar hasil tidak bocor saat jendela sudah hilang.
/// # Safety
/// Sama dengan `fm_search_done`: `lparam` harus berisi pointer `Box<FmSearchResult>` dari MSG_SEARCH_DONE.
pub unsafe fn fm_search_discard(lparam: LPARAM) {
    if lparam.0 == 0 {
        return;
    }
    // SAFETY: pointer dari Box::into_raw di fm_search_thread; kepemilikan pindah ke sini.
    let res: Box<FmSearchResult> = unsafe { Box::from_raw(lparam.0 as *mut FmSearchResult) };
    free_result(&res);
}

/// FmUpdateCue: teks petunjuk kotak cari, "Cari di <judul tab>" mengikuti tab aktif.
pub fn fm_update_cue(w: &mut FmWin) {
    if w.search.0.is_null() {
        return;
    }
    let cue = match fm_active(w) {
        Some(t) if t.title[0] != 0 => format!("Cari di {}", from_wide(until_nul(&t.title[..]))),
        _ => "Cari".to_string(),
    };
    let wide = to_wide_z(&cue);
    // SAFETY: wide berakhir NUL dan hidup selama SendMessageW (pemanggilan sinkron).
    unsafe {
        SendMessageW(
            w.search,
            EM_SETCUEBANNER,
            Some(WPARAM(1)),
            Some(LPARAM(wide.as_ptr() as isize)),
        )
    };
}

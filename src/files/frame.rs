//! Port win12files.cpp:385-421 (FmOpen), 424-515 (FmCommand, FmTranslateView), 621-749 (FmOpenFromCommandLine, FmHandleKey, FmRunLoop, wWinMain).
//! Status: belum diuji runtime (jendela, COM, pintasan, instance kedua); hanya cargo check. GetCommandLineW lewat crate windows (Win32_System_Environment aktif).

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use std::slice::from_raw_parts;

use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, COLORREF, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HINSTANCE, HWND, LPARAM, S_OK, WPARAM,
};
use windows::Win32::Graphics::Gdi::{COLOR_BTNFACE, HBRUSH};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Controls::{InitCommonControlsEx, EM_SETSEL, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_RETURN, VK_SHIFT};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{IInputObject, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, FindWindowExW, GetAncestor, GetMessageW, GetWindowTextW, LoadCursorW,
    RegisterClassExW, SendMessageW, ShowWindow, TranslateMessage, CS_DBLCLKS, CW_USEDEFAULT, GA_ROOT, HWND_MESSAGE,
    IDC_ARROW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW, MSG, SW_SHOW, SW_SHOWNORMAL,
    WINDOW_EX_STYLE, WM_COPYDATA, WM_KEYDOWN, WM_SYSKEYDOWN, WNDCLASSEXW, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
    WINDOW_STYLE,
};

use crate::fm_logic::{
    w12_fm_accel, w12_fm_explorer_args, w12_fm_parse_args, w12_tabs_add, w12_tabs_cycle, W12FmPalette, W12Tabs,
    W12FA_BACK, W12FA_CLOSE_TAB, W12FA_FOCUS_ADDRESS, W12FA_FOCUS_SEARCH, W12FA_FORWARD, W12FA_NEW_FOLDER,
    W12FA_NEW_TAB, W12FA_NEW_WINDOW, W12FA_NEXT_TAB, W12FA_NONE, W12FA_PREV_TAB, W12FA_REFRESH, W12FA_TAB_N, W12FA_UP,
};
use crate::files::app::{
    fm_active, fm_browse, fm_current_view, fm_focus, fm_focus_view, fm_navigate_to, fm_refresh_chrome, fm_tab_empty,
    fm_tab_init, fm_win_of,
};
use crate::files::bar::{
    fm_activate_tab, fm_close_tab, fm_hist_go, fm_nav_refresh, fm_nav_up, fm_navigate_home, fm_new_tab, fm_run_verb,
};
use crate::files::search_ui::{fm_search_cancel, fm_search_init, fm_search_shutdown, fm_search_start};
use crate::files::state::{CLASS_FRAME, CLASS_MSG, COPY_TAG, FmHit, FmTab, FmWin, FM_MAX_WINDOWS, MUTEX_NAME, WINS};
use crate::files::wndproc::{fm_msg_proc, fm_wnd_proc};
use crate::wide::{from_wide, pcw, to_wide, to_wide_z};

// win12files.cpp:621-639. GetCommandLineW dipanggil lewat crate windows (fitur Win32_System_Environment aktif di Cargo.toml).

/// Nilai awal FmWin (pengganti ZeroMemory + inisialisasi di C++). Dikemas Box lalu dipasang lewat into_raw.
fn fm_win_new() -> Box<FmWin> {
    Box::new(FmWin {
        h: HWND(null_mut()),
        addr: HWND(null_mut()),
        search: HWND(null_mut()),
        tabs: W12Tabs::default(),
        tab: std::array::from_fn(|_| fm_tab_empty()),
        status: [0u16; 256],
        status_error: false,
        tip: HWND(null_mut()),
        pal: W12FmPalette::default(),
        dark: false,
        accent: COLORREF(0),
        edit_brush: HBRUSH(null_mut()),
        hot: FmHit::default(),
        press: FmHit::default(),
        dragging: false,
        drag_tab: 0,
        tracking: false,
        has_sel: false,
        search_gen: 0,
    })
}

/// Jumlah jendela hidup (slot non-null di WINS).
pub(crate) fn fm_count() -> usize {
    WINS.borrow().iter().filter(|p| !p.is_null()).count()
}

/// Ambil kepemilikan FmWin yang HWND-nya cocok (dipakai WM_NCDESTROY). Slot dikosongkan lebih dulu.
pub(crate) fn fm_take_by_hwnd(h: HWND) -> Option<Box<FmWin>> {
    let found: Option<(usize, *mut FmWin)> = {
        let ws = WINS.borrow();
        let mut hit = None;
        for (i, &p) in ws.iter().enumerate() {
            if p.is_null() {
                continue;
            }
            // SAFETY: p berasal dari Box::into_raw dan hidup selama slotnya terisi.
            if unsafe { (*p).h } == h {
                hit = Some((i, p));
                break;
            }
        }
        hit
    };
    let (i, p) = found?;
    WINS.borrow_mut()[i] = null_mut();
    // SAFETY: slot sudah dikosongkan, jadi kepemilikan p hanya ada di sini.
    Some(unsafe { Box::from_raw(p) })
}

/// Kosongkan slot bila masih berisi p, lalu lepas Box-nya. Dipakai jalur gagal CreateWindowExW.
/// Pointer hanya dibandingkan, tidak di-dereference, sehingga aman bila WM_NCDESTROY sudah melepasnya.
fn fm_release_slot(slot: usize, p: *mut FmWin) {
    let owned = {
        let mut ws = WINS.borrow_mut();
        if ws[slot] == p {
            ws[slot] = null_mut();
            true
        } else {
            false
        }
    };
    if owned {
        // SAFETY: p berasal dari Box::into_raw dan slotnya sudah dikosongkan.
        drop(unsafe { Box::from_raw(p) });
    }
}

/// Instance modul untuk CreateWindowExW (setara GetModuleHandleW(nullptr)).
pub(crate) fn fm_hinstance() -> HINSTANCE {
    // SAFETY: nama null berarti modul utama proses.
    match unsafe { GetModuleHandleW(PCWSTR::null()) } {
        Ok(m) => HINSTANCE(m.0),
        Err(_) => HINSTANCE(null_mut()),
    }
}

/// FmOpen (win12files.cpp:385-421). Jendela penuh: pakai jendela pertama. Mengembalikan HWND, atau None bila gagal.
pub(crate) fn fm_open(path: Option<&str>, select: bool) -> Option<HWND> {
    let empty = path.map_or(true, |s| s.is_empty());
    let free = WINS.borrow().iter().position(|p| p.is_null());
    let slot = match free {
        Some(s) => s,
        None => {
            let w0 = WINS.borrow()[0];
            if w0.is_null() {
                return None;
            }
            // SAFETY: slot 0 terisi (jendela penuh) dan pointernya berasal dari Box::into_raw.
            let w = unsafe { &mut *w0 };
            if !empty {
                if let Some(s) = path {
                    fm_navigate_to(w, s, select);
                }
            }
            fm_focus(w);
            return Some(w.h);
        }
    };
    let p: *mut FmWin = Box::into_raw(fm_win_new());
    WINS.borrow_mut()[slot] = p;
    let cls = to_wide_z(CLASS_FRAME);
    let title = to_wide_z("Win12 Files");
    let inst = fm_hinstance();
    // SAFETY: buffer UTF-16 lokal hidup selama panggilan; lpCreateParams = p (dibaca WM_NCCREATE).
    let created = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            pcw(&cls),
            pcw(&title),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1100,
            720,
            None,
            None,
            Some(inst),
            Some(p as *const c_void),
        )
    };
    let h = match created {
        Ok(h) if !h.0.is_null() => h,
        _ => {
            fm_release_slot(slot, p);
            return None;
        }
    };
    // SAFETY: h valid dan WM_NCDESTROY belum terjadi, jadi p masih hidup.
    let w = unsafe { &mut *p };
    let id = w12_tabs_add(&mut w.tabs, false);
    if id < 0 || !fm_tab_init(w, 0) {
        // DestroyWindow memicu WM_NCDESTROY yang melepas p. Setelah ini w tidak boleh dipakai.
        let _ = unsafe { DestroyWindow(h) };
        return None;
    }
    if empty {
        fm_navigate_home(w);
    } else if let Some(s) = path {
        fm_navigate_to(w, s, select);
    }
    let _ = unsafe { ShowWindow(h, SW_SHOW) };
    fm_focus(w);
    if let Some(t) = fm_active(w) {
        fm_focus_view(t);
    }
    Some(h)
}

/// FmNewWindow (win12files.cpp:485-489): buka jendela baru, lalu arahkan tab pertamanya ke folder tab aktif.
fn fm_new_window(w: &mut FmWin) {
    let cur = match fm_active(w) {
        Some(t) => t.cur,
        None => return,
    };
    if cur.is_null() || fm_count() >= FM_MAX_WINDOWS {
        return;
    }
    let h = match fm_open(None, false) {
        Some(h) => h,
        None => return,
    };
    if let Some(nw) = fm_win_of(h) {
        if let Some(nt) = fm_active(nw) {
            let _ = fm_browse(nt, cur as *const ITEMIDLIST);
        }
    }
}

/// FmCommand (win12files.cpp:424-490): jalankan satu aksi akselerator pada jendela w.
pub(crate) fn fm_command(w: &mut FmWin, cmd: i32, tab_n: i32) {
    match cmd {
        W12FA_BACK => fm_hist_go(w, -1),
        W12FA_FORWARD => fm_hist_go(w, 1),
        W12FA_NEW_TAB => fm_new_tab(w),
        W12FA_CLOSE_TAB => {
            let active = w.tabs.active;
            fm_close_tab(w, active);
        }
        W12FA_NEXT_TAB | W12FA_PREV_TAB => {
            let dir = if cmd == W12FA_NEXT_TAB { 1 } else { -1 };
            w12_tabs_cycle(&mut w.tabs, dir);
            fm_activate_tab(w);
        }
        W12FA_TAB_N => {
            let n = w.tabs.n;
            let target = if tab_n < 0 { n - 1 } else { tab_n };
            if target >= 0 && target < n && target != w.tabs.active {
                w.tabs.active = target;
                fm_activate_tab(w);
            }
        }
        W12FA_NEW_FOLDER => fm_run_verb(w, true, &to_wide("NewFolder")),
        W12FA_UP => fm_nav_up(w),
        W12FA_REFRESH => fm_nav_refresh(w),
        W12FA_FOCUS_ADDRESS => {
            // SAFETY: w.addr adalah edit yang dibuat WM_CREATE; dipanggil dari thread UI.
            unsafe {
                let _ = SetFocus(Some(w.addr));
                let _ = SendMessageW(w.addr, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
            }
        }
        W12FA_FOCUS_SEARCH => {
            // SAFETY: w.search adalah edit yang dibuat WM_CREATE; dipanggil dari thread UI.
            unsafe {
                let _ = SetFocus(Some(w.search));
            }
        }
        W12FA_NEW_WINDOW => fm_new_window(w),
        _ => {}
    }
}

/// FmTranslateView (win12files.cpp:503-515): teruskan pesan ke IInputObject tampilan aktif.
/// Mengembalikan true hanya bila TranslateAcceleratorIO menjawab S_OK (S_FALSE dianggap belum ditangani).
fn fm_translate_view(t: &FmTab, msg: &MSG) -> bool {
    let sv = match fm_current_view(t) {
        Some(sv) => sv,
        None => return false,
    };
    let io: IInputObject = match sv.cast::<IInputObject>() {
        Ok(io) => io,
        Err(_) => return false,
    };
    // SAFETY: io hidup selama pemanggilan; TranslateAcceleratorIO hanya membaca msg.
    let hr = unsafe { (io.vtable().TranslateAcceleratorIO)(io.as_raw(), msg as *const MSG) };
    hr == S_OK
}

/// Tombol vk sedang ditekan (bit tinggi GetKeyState).
fn key_down(vk: u16) -> bool {
    // SAFETY: GetKeyState aman dipanggil dari thread UI.
    unsafe { GetKeyState(vk as i32) < 0 }
}

/// Teks sebuah jendela (edit), dipotong di NUL pertama.
fn fm_window_text(h: HWND) -> String {
    let mut buf = [0u16; 1024];
    // SAFETY: buf hidup selama pemanggilan; GetWindowTextW menulis paling banyak buf.len() unit.
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    let n = (n.max(0) as usize).min(buf.len());
    from_wide(&buf[..n])
}

/// FmHandleKey (win12files.cpp:656-692): pintasan dan kotak alamat dicegat sebelum DispatchMessage.
/// Mengembalikan true bila pesan sudah ditangani sehingga tidak perlu diterjemahkan atau didispatch.
fn fm_handle_key(msg: &MSG) -> bool {
    if msg.message != WM_KEYDOWN && msg.message != WM_SYSKEYDOWN {
        return false;
    }
    // SAFETY: GetAncestor hanya membaca HWND dari pesan yang sedang diproses.
    let root = unsafe { GetAncestor(msg.hwnd, GA_ROOT) };
    let w = match fm_win_of(root) {
        Some(w) => w,
        None => return false,
    };
    let vk = msg.wParam.0 as u32;
    let in_addr = msg.hwnd == w.addr;
    let in_search = msg.hwnd == w.search;
    let in_edit = in_addr || in_search;

    if in_addr && vk == VK_RETURN.0 as u32 {
        let text = fm_window_text(w.addr);
        if fm_navigate_to(w, &text, false) {
            if let Some(t) = fm_active(w) {
                fm_focus_view(t);
            }
        }
        return true;
    }
    if in_addr && vk == VK_ESCAPE.0 as u32 {
        fm_refresh_chrome(w);
        return true;
    }
    if in_search && vk == VK_RETURN.0 as u32 {
        let text = fm_window_text(w.search);
        if text.is_empty() {
            fm_search_cancel(w);
        } else {
            fm_search_start(w, &text);
        }
        return true;
    }
    if in_search && vk == VK_ESCAPE.0 as u32 {
        fm_search_cancel(w);
        return true;
    }
    let mut tab_n: i32 = -1;
    let cmd = w12_fm_accel(
        vk,
        key_down(VK_CONTROL.0),
        key_down(VK_SHIFT.0),
        key_down(VK_MENU.0),
        in_edit,
        Some(&mut tab_n),
    );
    if cmd != W12FA_NONE {
        fm_command(w, cmd, tab_n);
        return true;
    }
    if in_edit {
        return false;
    }
    match fm_active(w) {
        Some(t) => fm_translate_view(t, msg),
        None => false,
    }
}

/// FmRunLoop (win12files.cpp:694-702): pompa pesan utama. Berhenti bila GetMessageW mengembalikan nilai <= 0.
fn fm_run_loop() {
    // SAFETY: MSG adalah struct POD; dinolkan sebelum dipakai GetMessageW.
    let mut msg: MSG = unsafe { zeroed() };
    loop {
        // SAFETY: msg adalah buffer lokal yang hidup selama pemanggilan.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        if fm_handle_key(&msg) {
            continue;
        }
        // SAFETY: msg berisi pesan yang baru diambil GetMessageW.
        unsafe {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
}

/// FmOpenFromCommandLine (win12files.cpp:621-627): parse "[/select,]<jalur>" lalu buka jendela. Tanpa jalur = Beranda.
pub(crate) fn fm_open_from_command_line(cmd: &str) -> bool {
    let mut buf = [0u16; 1024];
    let mut sel = false;
    let n = w12_fm_parse_args(Some(cmd), &mut buf, Some(&mut sel));
    let path = if n > 0 { Some(from_wide(&buf)) } else { None };
    fm_open(path.as_deref(), sel).is_some()
}

/// Apakah proses `explorer.exe` sedang berjalan (snapshot proses).
/// PORT: Win12 mendaftarkan kelas `Shell_TrayWnd` sendiri (core_c3.rs, core_c4b.rs), jadi jendela itu tidak bisa dipakai sebagai penanda Explorer.
fn explorer_process_running() -> bool {
    // SAFETY: snapshot proses dibuat, dibaca, lalu ditutup dalam fungsi ini.
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return false;
    };
    let mut pe: PROCESSENTRY32W = unsafe { zeroed() };
    pe.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut found = false;
    let mut ok = unsafe { Process32FirstW(snap, &mut pe) }.is_ok();
    while ok {
        if from_wide(&pe.szExeFile).eq_ignore_ascii_case("explorer.exe") {
            found = true;
            break;
        }
        ok = unsafe { Process32NextW(snap, &mut pe) }.is_ok();
    }
    let _ = unsafe { CloseHandle(snap) };
    found
}

/// FmFallbackExplorer (win12files.cpp:631-639): browser gagal diinisialisasi, jalur dibuka lewat explorer.exe.
/// PORT: hanya memanggil explorer.exe bila Explorer sudah berjalan; jika tidak, tidak menyalakan Explorer (shell Win12 tanpa Explorer).
fn fm_fallback_explorer(cmd: &str) {
    if !explorer_process_running() {
        // Shell Win12 tanpa Explorer: cadangan tidak bisa dipakai. Beri tahu pengguna, jangan gagal diam-diam.
        let text = to_wide_z(
            "Win12 Files tidak dapat membuka lokasi itu. Explorer sedang tidak berjalan, jadi cadangan Explorer tidak dipakai.",
        );
        let caption = to_wide_z("Win12 Files");
        // SAFETY: text dan caption berakhir NUL dan hidup selama pemanggilan.
        let _ = unsafe {
            MessageBoxW(None, pcw(&text), pcw(&caption), MB_OK | MB_ICONWARNING | MB_SETFOREGROUND | MB_TOPMOST)
        };
        return;
    }
    let mut buf = [0u16; 1024];
    let mut sel = false;
    let _ = w12_fm_parse_args(Some(cmd), &mut buf, Some(&mut sel));
    let path_str = from_wide(&buf);
    let mut out = [0u16; 1100];
    let ok = w12_fm_explorer_args(Some(path_str.as_str()), sel, &mut out);
    let args = if ok { from_wide(&out) } else { String::new() };
    let verb = to_wide_z("open");
    let file = to_wide_z("explorer.exe");
    let args_z = to_wide_z(&args);
    let args_ptr = if args.is_empty() { PCWSTR::null() } else { pcw(&args_z) };
    // SAFETY: semua buffer UTF-16 (ber-NUL) hidup sampai ShellExecuteW selesai.
    unsafe {
        let _ = ShellExecuteW(None, pcw(&verb), pcw(&file), args_ptr, PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// Unit UTF-16 baris perintah proses ini (GetCommandLineW), termasuk NUL terakhir.
fn command_line_units() -> &'static [u16] {
    // SAFETY: GetCommandLineW mengembalikan buffer proses yang hidup selama proses berjalan.
    let p = unsafe { windows::Win32::System::Environment::GetCommandLineW() }.0;
    if p.is_null() {
        return &[0u16];
    }
    let mut n = 0usize;
    loop {
        // SAFETY: string baris perintah diakhiri NUL; pembacaan berhenti di NUL pertama.
        let c = unsafe { *p.add(n) };
        if c == 0 {
            break;
        }
        n += 1;
    }
    // SAFETY: n+1 unit (termasuk NUL) berada di dalam buffer yang sama.
    unsafe { from_raw_parts(p, n + 1) }
}

/// wWinMain (win12files.cpp:704-749): titik masuk file manager. Mengembalikan kode keluar proses.
pub fn fm_main() -> i32 {
    // SAFETY: dipanggil sekali di awal proses, sebelum jendela apa pun dibuat.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let icc = INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_WIN95_CLASSES,
    };
    // SAFETY: icc hidup selama pemanggilan.
    unsafe {
        let _ = InitCommonControlsEx(&icc);
    }

    let frame_cls = to_wide_z(CLASS_FRAME);
    let msg_cls = to_wide_z(CLASS_MSG);
    let mutex_name = to_wide_z(MUTEX_NAME);
    let empty = to_wide_z("");

    // SAFETY: mutex_name hidup sampai akhir fungsi.
    let created = unsafe { CreateMutexW(None, false, pcw(&mutex_name)) };
    // SAFETY: GetLastError dibaca segera setelah CreateMutexW, tanpa panggilan API lain di antaranya.
    let already = created.is_ok() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let mutex: Option<HANDLE> = created.ok();

    if already {
        // Instance kedua: kirim baris perintah (termasuk NUL) ke instance pertama lalu keluar.
        let units = command_line_units();
        // SAFETY: msg_cls hidup selama pemanggilan.
        let other = unsafe { FindWindowExW(Some(HWND_MESSAGE), None, pcw(&msg_cls), PCWSTR::null()) }
            .ok()
            .filter(|h| !h.0.is_null());
        if let Some(target) = other {
            let cds = COPYDATASTRUCT {
                dwData: COPY_TAG,
                cbData: (units.len() * size_of::<u16>()) as u32,
                lpData: units.as_ptr() as *mut c_void,
            };
            // SAFETY: cds dan units hidup selama SendMessageW (sinkron) berjalan.
            unsafe {
                let _ = SendMessageW(
                    target,
                    WM_COPYDATA,
                    Some(WPARAM(0)),
                    Some(LPARAM(&cds as *const COPYDATASTRUCT as isize)),
                );
            }
        }
        if let Some(h) = mutex {
            // SAFETY: h adalah handle mutex yang dibuat di atas.
            unsafe {
                let _ = CloseHandle(h);
            }
        }
        return 0;
    }

    // SAFETY: OleInitialize dipasangkan dengan OleUninitialize di akhir fungsi.
    unsafe {
        let _ = OleInitialize(None);
    }
    let inst = fm_hinstance();

    // SAFETY: WNDCLASSEXW adalah struct POD; nol berarti None/null untuk setiap pointer dan handle.
    let mut fc: WNDCLASSEXW = unsafe { zeroed() };
    fc.cbSize = size_of::<WNDCLASSEXW>() as u32;
    fc.style = CS_DBLCLKS;
    fc.lpfnWndProc = Some(fm_wnd_proc);
    fc.hInstance = inst;
    // SAFETY: IDC_ARROW adalah kursor sistem; LoadCursorW tidak memakai buffer milik kita.
    if let Ok(cur) = unsafe { LoadCursorW(None, IDC_ARROW) } {
        fc.hCursor = cur;
    }
    fc.hbrBackground = HBRUSH((COLOR_BTNFACE.0 + 1) as usize as *mut c_void);
    fc.lpszClassName = pcw(&frame_cls);

    // SAFETY: WNDCLASSEXW adalah struct POD; nol berarti None/null untuk setiap pointer dan handle.
    let mut mc: WNDCLASSEXW = unsafe { zeroed() };
    mc.cbSize = size_of::<WNDCLASSEXW>() as u32;
    mc.lpfnWndProc = Some(fm_msg_proc);
    mc.hInstance = inst;
    mc.lpszClassName = pcw(&msg_cls);

    // SAFETY: kedua struct dan buffer nama kelas hidup selama pemanggilan.
    unsafe {
        let _ = RegisterClassExW(&fc);
        let _ = RegisterClassExW(&mc);
    }
    // SAFETY: nama kelas pesan sudah didaftarkan; buffer hidup selama pemanggilan.
    let msg_wnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            pcw(&msg_cls),
            pcw(&empty),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(inst),
            None,
        )
    }
    .ok()
    .filter(|h| !h.0.is_null());

    fm_search_init();
    let cmd = from_wide(command_line_units());
    if fm_open_from_command_line(&cmd) {
        fm_run_loop();
    } else {
        fm_fallback_explorer(&cmd);
    }
    fm_search_shutdown();

    if let Some(h) = msg_wnd {
        // SAFETY: h adalah jendela pesan yang dibuat di atas.
        unsafe {
            let _ = DestroyWindow(h);
        }
    }
    // SAFETY: pasangan dari OleInitialize di atas.
    unsafe {
        OleUninitialize();
    }
    if let Some(h) = mutex {
        // SAFETY: h adalah handle mutex yang dibuat di atas.
        unsafe {
            let _ = CloseHandle(h);
        }
    }
    0
}

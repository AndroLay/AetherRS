//! Panel Win+V: daftar riwayat clipboard (dari `cliphist`) di pojok kanan bawah monitor kursor.
//! Enter atau klik baris: teks disalin ke clipboard, fokus kembali ke jendela sebelumnya, lalu Ctrl+V dikirim.
//! Esc, klik di luar panel, atau Win+V lagi menutup panel. Riwayat tidak pernah ditulis ke disk.
use super::cliphist::{ch_items, ch_remove};
use super::prelude::*;
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Lebar panel (piksel).
const CLP_W: i32 = 360;
/// Tinggi area judul.
const CLP_HEAD: i32 = 40;
/// Tinggi satu baris riwayat.
const CLP_ROW: i32 = 44;
/// Jumlah baris yang tampil sekaligus; sisanya digulir dengan panah.
const CLP_VIS: usize = 8;
/// Jeda sebelum Ctrl+V dikirim, memberi waktu jendela tujuan kembali aktif.
const CLP_PASTE_MS: u32 = 120;
/// Id timer (tanpa jendela) untuk pengiriman Ctrl+V.
const CLP_PASTE_ID: usize = 0x5C50;
/// Warna dalam COLORREF (0x00BBGGRR): latar #202020, sorot #0078D4, teks putih, teks redup abu-abu.
const CLP_BG: u32 = 0x0020_2020;
const CLP_SEL: u32 = 0x00D4_7800;
const CLP_FG: u32 = 0x00FF_FFFF;
const CLP_DIM: u32 = 0x00A0_A0A0;

static CLP_WND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static CLP_PREV: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static CLP_REG: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static CLP_SEL_IDX: SyncCell<Cell<usize>> = SyncCell::new(Cell::new(0));
static CLP_TOP: SyncCell<Cell<usize>> = SyncCell::new(Cell::new(0));
static CLP_ITEMS: SyncCell<RefCell<Vec<String>>> = SyncCell::new(RefCell::new(Vec::new()));

/// Win+V: buka panel bila tertutup, tutup bila sudah terbuka.
pub fn clp_toggle() {
    if !CLP_WND.get().0.is_null() {
        clp_close();
        return;
    }
    clp_open();
}

/// Buka panel di pojok kanan bawah work area monitor kursor, lalu minta fokus keyboard.
fn clp_open() {
    let items = ch_items();
    let hi = HI.get();
    // SAFETY: GetForegroundWindow tidak punya prasyarat; jendela ini dipakai sebagai tujuan Ctrl+V.
    CLP_PREV.set(unsafe { GetForegroundWindow() });
    CLP_SEL_IDX.set(0);
    CLP_TOP.set(0);
    *CLP_ITEMS.borrow_mut() = items;
    if !CLP_REG.get() {
        let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(clp_proc);
        wc.hInstance = hi;
        // SAFETY: IDC_ARROW adalah kursor sistem.
        if let Ok(cur) = unsafe { LoadCursorW(None, IDC_ARROW) } {
            wc.hCursor = cur;
        }
        wc.lpszClassName = w!("Win12ClipPanel");
        // SAFETY: wc hidup selama pemanggilan.
        CLP_REG.set(unsafe { RegisterClassExW(&wc) } != 0);
    }
    let rows = CLP_ITEMS.borrow().len().clamp(1, CLP_VIS) as i32;
    let tinggi = CLP_HEAD + rows * CLP_ROW + 8;
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
    let (x, y) = (r.right - CLP_W - 12, r.bottom - tinggi - 12);
    // SAFETY: kelas Win12ClipPanel sudah didaftarkan; jendela top-level tanpa parent.
    let Ok(hwnd) = (unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("Win12ClipPanel"),
            w!(""),
            WS_POPUP,
            x,
            y,
            CLP_W,
            tinggi,
            None,
            None,
            Some(hi),
            None,
        )
    }) else {
        return;
    };
    CLP_WND.set(hwnd);
    // SAFETY: hwnd baru dibuat di atas dan belum dipakai pihak lain.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Tutup panel. Pointer diosongkan dulu agar WM_ACTIVATE yang muncul saat DestroyWindow tidak mengulang.
fn clp_close() {
    let wnd = CLP_WND.get();
    if wnd.0.is_null() {
        return;
    }
    CLP_WND.set(HWND(std::ptr::null_mut()));
    // SAFETY: wnd adalah jendela panel yang dibuat clp_open.
    let _ = unsafe { DestroyWindow(wnd) };
}

/// Pindah pilihan sebesar `delta` dan geser daftar bila pilihan keluar dari area tampil.
fn clp_move(delta: isize) {
    let n = CLP_ITEMS.borrow().len();
    if n == 0 {
        return;
    }
    let cur = CLP_SEL_IDX.get() as isize;
    let next = (cur + delta).clamp(0, n as isize - 1) as usize;
    CLP_SEL_IDX.set(next);
    let top = CLP_TOP.get();
    if next < top {
        CLP_TOP.set(next);
    } else if next >= top + CLP_VIS {
        CLP_TOP.set(next + 1 - CLP_VIS);
    }
    clp_repaint();
}

/// Hapus entri ke-`idx` dari riwayat dan daftar panel. Pilihan dijaga tetap valid; panel tutup bila kosong.
fn clp_delete(idx: usize) {
    let text = {
        let mut items = CLP_ITEMS.borrow_mut();
        if idx >= items.len() {
            return;
        }
        items.remove(idx)
    };
    ch_remove(&text);
    let n = CLP_ITEMS.borrow().len();
    if n == 0 {
        clp_close();
        return;
    }
    let sel = idx.min(n - 1);
    let mut top = CLP_TOP.get();
    if sel < top {
        top = sel;
    }
    if top + CLP_VIS > n {
        top = n.saturating_sub(CLP_VIS);
    }
    CLP_SEL_IDX.set(sel);
    CLP_TOP.set(top);
    clp_repaint();
}

/// Minta WM_PAINT untuk panel bila masih terbuka.
fn clp_repaint() {
    let wnd = CLP_WND.get();
    if !wnd.0.is_null() {
        // SAFETY: wnd adalah jendela panel yang masih hidup.
        let _ = unsafe { InvalidateRect(Some(wnd), None, true) };
    }
}

/// Salin riwayat ke-`idx` lalu tempel: fokus ke jendela sebelumnya selagi panel masih foreground,
/// tutup panel, dan kirim Ctrl+V setelah jeda singkat.
fn clp_pick(idx: usize) {
    let text = CLP_ITEMS.borrow().get(idx).cloned();
    if let Some(text) = text {
        if clp_set_clipboard(&text) {
            let prev = CLP_PREV.get();
            if !prev.0.is_null() {
                // SAFETY: panel masih foreground sehingga shell berhak memindah fokus ke jendela tujuan.
                let _ = unsafe { SetForegroundWindow(prev) };
            }
            clp_close();
            // SAFETY: timer tanpa jendela; callback dipanggil dari antrean pesan thread shell.
            unsafe { SetTimer(None, CLP_PASTE_ID, CLP_PASTE_MS, Some(clp_paste_timer)) };
            return;
        }
    }
    clp_close();
}

/// Callback timer sekali jalan: hentikan timer lalu kirim Ctrl+V.
unsafe extern "system" fn clp_paste_timer(_h: HWND, _m: u32, id: usize, _t: u32) {
    // SAFETY: id adalah timer yang dipasang clp_pick.
    let _ = unsafe { KillTimer(None, id) };
    clp_send_ctrl_v();
}

/// Kirim Ctrl+V sintetis. dwExtraInfo = W12_KBSIG agar kait keyboard tidak memproses ulang input ini.
pub(super) fn clp_send_ctrl_v() {
    let keys: [(u16, KEYBD_EVENT_FLAGS); 4] = [
        (VK_CONTROL.0, KEYBD_EVENT_FLAGS(0)),
        (b'V' as u16, KEYBD_EVENT_FLAGS(0)),
        (b'V' as u16, KEYEVENTF_KEYUP),
        (VK_CONTROL.0, KEYEVENTF_KEYUP),
    ];
    // SAFETY: INPUT nol-kan lalu seluruh field yang dipakai diisi di bawah.
    let mut inp: [INPUT; 4] = unsafe { std::mem::zeroed() };
    for (slot, &(vk, flags)) in inp.iter_mut().zip(keys.iter()) {
        slot.r#type = INPUT_KEYBOARD;
        slot.Anonymous.ki.wVk = VIRTUAL_KEY(vk);
        slot.Anonymous.ki.dwFlags = flags;
        slot.Anonymous.ki.dwExtraInfo = W12_KBSIG;
    }
    // SAFETY: inp berisi empat INPUT keyboard yang sudah lengkap.
    unsafe { SendInput(&inp, std::mem::size_of::<INPUT>() as i32) };
}

/// Tulis teks ke clipboard sebagai CF_UNICODETEXT. Memori dibebaskan sendiri bila gagal sebelum diserahkan.
pub(super) fn clp_set_clipboard(text: &str) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: memori dialokasi, diisi, dibuka-kunci, lalu diserahkan ke clipboard; bila gagal dibebaskan lagi.
    unsafe {
        let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) else {
            return false;
        };
        let p = GlobalLock(hg) as *mut u16;
        if p.is_null() {
            let _ = GlobalFree(Some(hg));
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        let _ = GlobalUnlock(hg);
        if OpenClipboard(None).is_err() {
            let _ = GlobalFree(Some(hg));
            return false;
        }
        let _ = EmptyClipboard();
        let ok = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(hg.0))).is_ok();
        let _ = CloseClipboard();
        if !ok {
            let _ = GlobalFree(Some(hg));
        }
        ok
    }
}

/// Prosedur jendela panel: gambar, tombol panah/Enter/Esc, klik baris, dan tutup saat kehilangan fokus.
unsafe extern "system" fn clp_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_PAINT => {
            clp_paint(h);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wp.0 as u16;
            if vk == VK_ESCAPE.0 {
                clp_close();
            } else if vk == VK_RETURN.0 {
                clp_pick(CLP_SEL_IDX.get());
            } else if vk == VK_UP.0 {
                clp_move(-1);
            } else if vk == VK_DOWN.0 {
                clp_move(1);
            } else if vk == VK_DELETE.0 {
                clp_delete(CLP_SEL_IDX.get());
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let y = ((lp.0 >> 16) & 0xFFFF) as i32;
            if y >= CLP_HEAD {
                let idx = CLP_TOP.get() + ((y - CLP_HEAD) / CLP_ROW) as usize;
                if idx < CLP_ITEMS.borrow().len() {
                    clp_pick(idx);
                }
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            // WA_INACTIVE: panel kehilangan fokus (klik di luar, atau fokus sudah pindah ke jendela tujuan).
            if wp.0 & 0xFFFF == WA_INACTIVE as usize {
                clp_close();
            }
            LRESULT(0)
        }
        // SAFETY: argumen diteruskan apa adanya dari sistem.
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

/// Gambar latar, judul, dan baris riwayat (atau pesan kosong). Semua objek GDI dihapus sebelum EndPaint.
fn clp_paint(h: HWND) {
    // SAFETY: PAINTSTRUCT dan RECT nol-kan lalu diisi sistem; BeginPaint dan EndPaint dipasangkan di bawah.
    let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    let mut rc: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        let dc = BeginPaint(h, &mut ps);
        let _ = GetClientRect(h, &mut rc);
        let bg = CreateSolidBrush(COLORREF(CLP_BG));
        let sel = CreateSolidBrush(COLORREF(CLP_SEL));
        let old = SelectObject(dc, GetStockObject(DEFAULT_GUI_FONT));
        let _ = SetBkMode(dc, TRANSPARENT);
        let _ = FillRect(dc, &rc, bg);

        let items = CLP_ITEMS.borrow();
        let top = CLP_TOP.get();
        let cur = CLP_SEL_IDX.get();
        clp_text(dc, "Riwayat clipboard", RECT { left: 12, top: 0, right: rc.right - 12, bottom: CLP_HEAD }, CLP_DIM);
        if items.is_empty() {
            let r = RECT { left: 12, top: CLP_HEAD, right: rc.right - 12, bottom: CLP_HEAD + CLP_ROW };
            clp_text(dc, "Belum ada riwayat. Salin teks dulu.", r, CLP_DIM);
        }
        for (i, text) in items.iter().enumerate().skip(top).take(CLP_VIS) {
            let y0 = CLP_HEAD + ((i - top) as i32) * CLP_ROW;
            if i == cur {
                let row = RECT { left: 4, top: y0 + 2, right: rc.right - 4, bottom: y0 + CLP_ROW - 2 };
                let _ = FillRect(dc, &row, sel);
            }
            let r = RECT { left: 12, top: y0, right: rc.right - 12, bottom: y0 + CLP_ROW };
            clp_text(dc, text, r, CLP_FG);
        }
        drop(items);

        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(bg.0));
        let _ = DeleteObject(HGDIOBJ(sel.0));
        let _ = EndPaint(h, &ps);
    }
}

/// Gambar satu baris teks (satu baris, terpotong dengan elipsis). Kontrol karakter diganti spasi.
fn clp_text(dc: HDC, s: &str, mut rc: RECT, col: u32) {
    let mut buf: Vec<u16> = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .encode_utf16()
        .collect();
    let fmt = DRAW_TEXT_FORMAT(DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0 | DT_NOPREFIX.0);
    // SAFETY: dc adalah HDC sah dari BeginPaint; buf dan rc hidup selama pemanggilan.
    unsafe {
        let _ = SetTextColor(dc, COLORREF(col));
        let _ = DrawTextW(dc, &mut buf, &mut rc, fmt);
    }
}

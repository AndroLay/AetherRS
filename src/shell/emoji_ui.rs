//! Panel emoji (Win+. dan Win+;): kisi emoji di pojok kanan bawah monitor kursor.
//! Enter atau klik sel: emoji disalin ke clipboard, fokus kembali ke jendela sebelumnya, lalu Ctrl+V dikirim.
//! Esc, klik di luar panel, atau pintasan yang sama lagi menutup panel. Glyph digambar GDI dengan
//! Segoe UI Emoji sehingga tampil monokrom (bukan emoji berwarna).
use super::cliphist_ui::{clp_send_ctrl_v, clp_set_clipboard};
use super::prelude::*;

/// Jumlah kolom dan baris kisi; daftar di bawah harus berisi tepat COLS * ROWS entri.
const EMO_COLS: usize = 8;
const EMO_ROWS: usize = 8;
/// Ukuran satu sel dan margin panel (piksel).
const EMO_CELL: i32 = 44;
const EMO_PAD: i32 = 8;
/// Tinggi area judul.
const EMO_HEAD: i32 = 32;
const EMO_W: i32 = EMO_PAD * 2 + EMO_COLS as i32 * EMO_CELL;
const EMO_H: i32 = EMO_HEAD + EMO_ROWS as i32 * EMO_CELL + EMO_PAD;
/// Jeda sebelum Ctrl+V dikirim, sama seperti panel riwayat.
const EMO_PASTE_MS: u32 = 120;
/// Id timer (tanpa jendela) untuk pengiriman Ctrl+V.
const EMO_PASTE_ID: usize = 0x5C51;
/// Warna COLORREF (0x00BBGGRR), sama dengan panel riwayat.
const EMO_BG: u32 = 0x0020_2020;
const EMO_SEL: u32 = 0x00D4_7800;
const EMO_FG: u32 = 0x00FF_FFFF;
const EMO_DIM: u32 = 0x00A0_A0A0;
/// Tinggi em font glyph (negatif = tinggi karakter).
const EMO_FONT_H: i32 = -24;

/// Daftar emoji yang tampil, urut baris demi baris.
const EMO_LIST: [&str; EMO_COLS * EMO_ROWS] = [
    "😀", "😂", "🤣", "😊", "😍", "😘", "😎", "🤔",
    "😢", "😭", "😡", "😴", "🙄", "😅", "🥺", "🤗",
    "🤩", "😉", "👍", "👎", "👏", "🙏", "👋", "💪",
    "🙌", "🤝", "👀", "❤️", "🔥", "✨", "🎉", "💯",
    "✅", "❌", "⚠️", "💡", "📝", "📌", "📅", "🚀",
    "🎯", "⭐", "☕", "🍕", "🎵", "💻", "🔒", "🔑",
    "😇", "🥰", "😋", "🙂", "🤷", "😬", "🥳", "💔",
    "🌟", "🌈", "🌍", "🐶", "🐱", "🍎", "🎁", "💰",
];

static EMO_WND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static EMO_PREV: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static EMO_REG: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
static EMO_SEL_IDX: SyncCell<Cell<usize>> = SyncCell::new(Cell::new(0));

/// Win+. / Win+;: buka panel bila tertutup, tutup bila sudah terbuka.
pub fn emo_toggle() {
    if !EMO_WND.get().0.is_null() {
        emo_close();
        return;
    }
    emo_open();
}

/// Buka panel di pojok kanan bawah work area monitor kursor, lalu minta fokus keyboard.
fn emo_open() {
    let hi = HI.get();
    // SAFETY: GetForegroundWindow tidak punya prasyarat; jendela ini dipakai sebagai tujuan Ctrl+V.
    EMO_PREV.set(unsafe { GetForegroundWindow() });
    EMO_SEL_IDX.set(0);
    if !EMO_REG.get() {
        let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(emo_proc);
        wc.hInstance = hi;
        // SAFETY: IDC_ARROW adalah kursor sistem.
        if let Ok(cur) = unsafe { LoadCursorW(None, IDC_ARROW) } {
            wc.hCursor = cur;
        }
        wc.lpszClassName = w!("Win12EmojiPanel");
        // SAFETY: wc hidup selama pemanggilan.
        EMO_REG.set(unsafe { RegisterClassExW(&wc) } != 0);
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
    let (x, y) = (r.right - EMO_W - 12, r.bottom - EMO_H - 12);
    // SAFETY: kelas Win12EmojiPanel sudah didaftarkan; jendela top-level tanpa parent.
    let Ok(hwnd) = (unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("Win12EmojiPanel"),
            w!(""),
            WS_POPUP,
            x,
            y,
            EMO_W,
            EMO_H,
            None,
            None,
            Some(hi),
            None,
        )
    }) else {
        return;
    };
    EMO_WND.set(hwnd);
    // SAFETY: hwnd baru dibuat di atas dan belum dipakai pihak lain.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Tutup panel. Pointer dikosongkan dulu agar WM_ACTIVATE yang muncul saat DestroyWindow tidak mengulang.
fn emo_close() {
    let wnd = EMO_WND.get();
    if wnd.0.is_null() {
        return;
    }
    EMO_WND.set(HWND(std::ptr::null_mut()));
    // SAFETY: wnd adalah jendela panel yang dibuat emo_open.
    let _ = unsafe { DestroyWindow(wnd) };
}

/// Minta WM_PAINT untuk panel bila masih terbuka.
fn emo_repaint() {
    let wnd = EMO_WND.get();
    if !wnd.0.is_null() {
        // SAFETY: wnd adalah jendela panel yang masih hidup.
        let _ = unsafe { InvalidateRect(Some(wnd), None, true) };
    }
}

/// Geser pilihan satu sel ke kiri/kanan (dx) atau atas/bawah (dy), dibatasi tepi kisi.
fn emo_move(dx: isize, dy: isize) {
    let cur = EMO_SEL_IDX.get();
    let col = ((cur % EMO_COLS) as isize + dx).clamp(0, EMO_COLS as isize - 1) as usize;
    let row = ((cur / EMO_COLS) as isize + dy).clamp(0, EMO_ROWS as isize - 1) as usize;
    EMO_SEL_IDX.set(row * EMO_COLS + col);
    emo_repaint();
}

/// Indeks sel di bawah titik klien (x, y), bila titik itu berada di dalam kisi.
fn emo_cell_at(x: i32, y: i32) -> Option<usize> {
    if x < EMO_PAD || y < EMO_HEAD {
        return None;
    }
    let col = ((x - EMO_PAD) / EMO_CELL) as usize;
    let row = ((y - EMO_HEAD) / EMO_CELL) as usize;
    if col >= EMO_COLS || row >= EMO_ROWS {
        return None;
    }
    Some(row * EMO_COLS + col)
}

/// Salin emoji ke-`idx` lalu tempel: fokus ke jendela sebelumnya selagi panel masih foreground,
/// tutup panel, dan kirim Ctrl+V setelah jeda singkat.
fn emo_pick(idx: usize) {
    let Some(text) = EMO_LIST.get(idx).copied() else {
        emo_close();
        return;
    };
    if clp_set_clipboard(text) {
        let prev = EMO_PREV.get();
        if !prev.0.is_null() {
            // SAFETY: panel masih foreground sehingga shell berhak memindah fokus ke jendela tujuan.
            let _ = unsafe { SetForegroundWindow(prev) };
        }
        emo_close();
        // SAFETY: timer tanpa jendela; callback dipanggil dari antrean pesan thread shell.
        unsafe { SetTimer(None, EMO_PASTE_ID, EMO_PASTE_MS, Some(emo_paste_timer)) };
        return;
    }
    emo_close();
}

/// Callback timer sekali jalan: hentikan timer lalu kirim Ctrl+V.
unsafe extern "system" fn emo_paste_timer(_h: HWND, _m: u32, id: usize, _t: u32) {
    // SAFETY: id adalah timer yang dipasang emo_pick.
    let _ = unsafe { KillTimer(None, id) };
    clp_send_ctrl_v();
}

/// Prosedur jendela panel: gambar, tombol panah/Enter/Esc, klik sel, dan tutup saat kehilangan fokus.
unsafe extern "system" fn emo_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_PAINT => {
            emo_paint(h);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wp.0 as u16;
            if vk == VK_ESCAPE.0 {
                emo_close();
            } else if vk == VK_RETURN.0 {
                emo_pick(EMO_SEL_IDX.get());
            } else if vk == VK_LEFT.0 {
                emo_move(-1, 0);
            } else if vk == VK_RIGHT.0 {
                emo_move(1, 0);
            } else if vk == VK_UP.0 {
                emo_move(0, -1);
            } else if vk == VK_DOWN.0 {
                emo_move(0, 1);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let x = (lp.0 & 0xFFFF) as i32;
            let y = ((lp.0 >> 16) & 0xFFFF) as i32;
            if let Some(idx) = emo_cell_at(x, y) {
                emo_pick(idx);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            // WA_INACTIVE: panel kehilangan fokus (klik di luar, atau fokus sudah pindah ke jendela tujuan).
            if wp.0 & 0xFFFF == WA_INACTIVE as usize {
                emo_close();
            }
            LRESULT(0)
        }
        // SAFETY: argumen diteruskan apa adanya dari sistem.
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

/// Gambar latar, judul, dan kisi emoji dengan sorot pada sel terpilih. Semua objek GDI dihapus sebelum EndPaint.
fn emo_paint(h: HWND) {
    // SAFETY: PAINTSTRUCT dan RECT nol-kan lalu diisi sistem; BeginPaint dan EndPaint dipasangkan di bawah.
    let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    let mut rc: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        let dc = BeginPaint(h, &mut ps);
        let _ = GetClientRect(h, &mut rc);
        let bg = CreateSolidBrush(COLORREF(EMO_BG));
        let sel = CreateSolidBrush(COLORREF(EMO_SEL));
        let _ = FillRect(dc, &rc, bg);
        let _ = SetBkMode(dc, TRANSPARENT);
        let old = SelectObject(dc, GetStockObject(DEFAULT_GUI_FONT));
        let judul = RECT { left: EMO_PAD + 4, top: 0, right: rc.right - EMO_PAD, bottom: EMO_HEAD };
        emo_draw(dc, "Emoji", judul, EMO_DIM, DT_LEFT.0);

        let font = CreateFontW(
            EMO_FONT_H,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            w!("Segoe UI Emoji"),
        );
        SelectObject(dc, HGDIOBJ(font.0));

        let cur = EMO_SEL_IDX.get();
        for (i, s) in EMO_LIST.iter().enumerate() {
            let x0 = EMO_PAD + (i % EMO_COLS) as i32 * EMO_CELL;
            let y0 = EMO_HEAD + (i / EMO_COLS) as i32 * EMO_CELL;
            if i == cur {
                let r = RECT { left: x0 + 2, top: y0 + 2, right: x0 + EMO_CELL - 2, bottom: y0 + EMO_CELL - 2 };
                let _ = FillRect(dc, &r, sel);
            }
            let cell = RECT { left: x0, top: y0, right: x0 + EMO_CELL, bottom: y0 + EMO_CELL };
            emo_draw(dc, s, cell, EMO_FG, DT_CENTER.0);
        }

        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteObject(HGDIOBJ(bg.0));
        let _ = DeleteObject(HGDIOBJ(sel.0));
        let _ = EndPaint(h, &ps);
    }
}

/// Gambar satu teks satu baris di dalam `rc`. `align` berisi DT_LEFT atau DT_CENTER.
fn emo_draw(dc: HDC, s: &str, mut rc: RECT, col: u32, align: u32) {
    let mut buf: Vec<u16> = s.encode_utf16().collect();
    let fmt = DRAW_TEXT_FORMAT(align | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_NOPREFIX.0);
    // SAFETY: dc adalah HDC sah dari BeginPaint; buf dan rc hidup selama pemanggilan.
    unsafe {
        let _ = SetTextColor(dc, COLORREF(col));
        let _ = DrawTextW(dc, &mut buf, &mut rc, fmt);
    }
}

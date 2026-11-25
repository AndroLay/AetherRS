//! Panel emoji (Win+. dan Win+;): kisi emoji di pojok kanan bawah monitor kursor.
//! Di atas kisi ada tab kategori. Mengetik langsung mencari emoji lintas kategori (kata kunci Indonesia dan Inggris).
//! Tab (atau klik tab) pindah kategori; Backspace menghapus huruf pencarian; Esc menghapus pencarian, lalu menutup.
//! Enter atau klik sel: emoji disalin ke clipboard, fokus kembali ke jendela sebelumnya, lalu Ctrl+V dikirim.
//! Klik di luar panel atau pintasan yang sama lagi menutup panel. Glyph digambar D2D dengan
//! Segoe UI Emoji (berwarna); bila target D2D gagal, jatuh ke GDI (monokrom).
use super::cliphist_ui::{clp_send_ctrl_v, clp_set_clipboard};
use super::emoji_data::{EmoItem, EMO_CATS, EMO_ITEMS};
use super::prelude::*;
use std::cell::RefCell;

/// Jumlah kolom dan baris kisi; jumlah emoji yang tampil sekaligus dibatasi COLS * ROWS.
const EMO_COLS: usize = 8;
const EMO_ROWS: usize = 8;
const EMO_MAX_SHOWN: usize = EMO_COLS * EMO_ROWS;
/// Panjang maksimum kueri pencarian (karakter).
const EMO_QUERY_MAX: usize = 32;
/// Ukuran satu sel dan margin panel (piksel).
const EMO_CELL: i32 = 44;
const EMO_PAD: i32 = 8;
/// Tinggi area judul dan baris tab kategori.
const EMO_HEAD: i32 = 32;
const EMO_TAB_H: i32 = 40;
/// Posisi atas kisi emoji (di bawah judul dan tab).
const EMO_GRID_Y: i32 = EMO_HEAD + EMO_TAB_H;
const EMO_W: i32 = EMO_PAD * 2 + EMO_COLS as i32 * EMO_CELL;
const EMO_H: i32 = EMO_GRID_Y + EMO_ROWS as i32 * EMO_CELL + EMO_PAD;
/// Jeda sebelum Ctrl+V dikirim, sama seperti panel riwayat.
const EMO_PASTE_MS: u32 = 120;
/// Id timer (tanpa jendela) untuk pengiriman Ctrl+V.
const EMO_PASTE_ID: usize = 0x5C51;
/// Warna COLORREF (0x00BBGGRR), sama dengan panel riwayat.
const EMO_BG: u32 = 0x0020_2020;
const EMO_SEL: u32 = 0x00D4_7800;
const EMO_FG: u32 = 0x00FF_FFFF;
const EMO_DIM: u32 = 0x00A0_A0A0;
/// Latar tab kategori yang sedang aktif.
const EMO_TAB_ON: u32 = 0x0040_4040;
/// Tinggi em font glyph (negatif = tinggi karakter).
const EMO_FONT_H: i32 = -24;
/// Ukuran font emoji berwarna (D2D, DIP pada DPI 96 = piksel); sama dengan `EMO_FONT_H`.
const EMO_GLYPH_PX: f32 = 24.0;

static EMO_WND: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static EMO_PREV: SyncCell<Cell<HWND>> = SyncCell::new(Cell::new(HWND(std::ptr::null_mut())));
static EMO_REG: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
/// Indeks sel terpilih dalam daftar yang tampil sekarang.
static EMO_SEL_IDX: SyncCell<Cell<usize>> = SyncCell::new(Cell::new(0));
/// Kategori aktif (indeks ke EMO_CATS); hanya dipakai bila kueri kosong.
static EMO_CAT: SyncCell<Cell<usize>> = SyncCell::new(Cell::new(0));
/// Kueri pencarian; kosong berarti tampilkan kategori aktif.
static EMO_QUERY: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));

/// Akses ke kueri pencarian (RefCell di dalam SyncCell, dipakai dari WndProc).
fn emo_qcell() -> &'static RefCell<String> {
    &EMO_QUERY
}

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
    EMO_CAT.set(0);
    emo_qcell().borrow_mut().clear();
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

/// Emoji yang tampil sekarang: kategori aktif bila kueri kosong, atau hasil pencarian lintas kategori.
fn emo_visible() -> Vec<&'static EmoItem> {
    let cat = EMO_CAT.get();
    let q: String = emo_qcell().borrow().clone();
    EMO_ITEMS
        .iter()
        .filter(|it| {
            if q.is_empty() {
                it.cat == cat
            } else {
                it.ch.contains(q.as_str()) || it.kw.contains(q.as_str())
            }
        })
        .take(EMO_MAX_SHOWN)
        .collect()
}

/// Tambah satu karakter ke kueri (huruf kecil, maksimal EMO_QUERY_MAX). Karakter kontrol
/// (Backspace, Tab, Enter, Esc) dan surrogate diabaikan karena ditangani di WM_KEYDOWN atau tidak valid.
fn emo_query_add(code: u32) {
    let Some(ch) = char::from_u32(code) else {
        return;
    };
    if ch.is_control() {
        return;
    }
    {
        let mut q = emo_qcell().borrow_mut();
        if q.chars().count() >= EMO_QUERY_MAX {
            return;
        }
        q.extend(ch.to_lowercase());
    }
    EMO_SEL_IDX.set(0);
    emo_repaint();
}

/// Hapus satu karakter terakhir dari kueri.
fn emo_query_pop() {
    let removed = emo_qcell().borrow_mut().pop().is_some();
    if removed {
        EMO_SEL_IDX.set(0);
        emo_repaint();
    }
}

/// Kosongkan kueri. Mengembalikan false bila kueri memang sudah kosong (Esc lalu menutup panel).
fn emo_query_clear() -> bool {
    let cleared = {
        let mut q = emo_qcell().borrow_mut();
        let had = !q.is_empty();
        q.clear();
        had
    };
    if cleared {
        EMO_SEL_IDX.set(0);
    }
    cleared
}

/// Pindah ke kategori `cat` (dibungkus ke jumlah tab) dan hapus kueri pencarian.
fn emo_set_cat(cat: usize) {
    emo_qcell().borrow_mut().clear();
    EMO_CAT.set(cat % EMO_CATS.len());
    EMO_SEL_IDX.set(0);
    emo_repaint();
}

/// Geser pilihan satu sel ke kiri/kanan (dx) atau atas/bawah (dy), dibatasi tepi kisi dan jumlah emoji yang tampil.
fn emo_move(dx: isize, dy: isize) {
    let n = emo_visible().len();
    if n == 0 {
        return;
    }
    let cur = EMO_SEL_IDX.get().min(n - 1);
    let col = ((cur % EMO_COLS) as isize + dx).clamp(0, EMO_COLS as isize - 1) as usize;
    let row = ((cur / EMO_COLS) as isize + dy).clamp(0, EMO_ROWS as isize - 1) as usize;
    EMO_SEL_IDX.set((row * EMO_COLS + col).min(n - 1));
    emo_repaint();
}

/// Indeks tab kategori di bawah titik klien (x, y), bila titik itu berada di baris tab.
fn emo_tab_at(x: i32, y: i32) -> Option<usize> {
    if x < EMO_PAD || y < EMO_HEAD || y >= EMO_GRID_Y {
        return None;
    }
    let col = ((x - EMO_PAD) / EMO_CELL) as usize;
    (col < EMO_CATS.len()).then_some(col)
}

/// Posisi sel (dalam daftar yang tampil) di bawah titik klien (x, y), bila titik itu berada di dalam kisi.
fn emo_cell_at(x: i32, y: i32) -> Option<usize> {
    if x < EMO_PAD || y < EMO_GRID_Y {
        return None;
    }
    let col = ((x - EMO_PAD) / EMO_CELL) as usize;
    let row = ((y - EMO_GRID_Y) / EMO_CELL) as usize;
    if col >= EMO_COLS || row >= EMO_ROWS {
        return None;
    }
    Some(row * EMO_COLS + col)
}

/// Salin emoji ke-`pos` dari daftar yang tampil lalu tempel: fokus ke jendela sebelumnya selagi panel
/// masih foreground, tutup panel, dan kirim Ctrl+V setelah jeda singkat. Bila posisi tidak ada, panel tetap terbuka.
fn emo_pick(pos: usize) {
    let items = emo_visible();
    let Some(item) = items.get(pos) else {
        return;
    };
    if clp_set_clipboard(item.ch) {
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
                // Esc: hapus kueri dulu; bila kueri sudah kosong, tutup panel.
                if emo_query_clear() {
                    emo_repaint();
                } else {
                    emo_close();
                }
            } else if vk == VK_RETURN.0 {
                emo_pick(EMO_SEL_IDX.get());
            } else if vk == VK_TAB.0 {
                emo_set_cat(EMO_CAT.get() + 1);
            } else if vk == VK_BACK.0 {
                emo_query_pop();
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
            if let Some(cat) = emo_tab_at(x, y) {
                emo_set_cat(cat);
            } else if let Some(pos) = emo_cell_at(x, y) {
                emo_pick(pos);
            }
            LRESULT(0)
        }
        WM_CHAR => {
            emo_query_add(wp.0 as u32);
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

/// Gambar latar, judul (nama kategori atau hasil pencarian), baris tab, dan kisi emoji dengan sorot pada sel terpilih.
/// Semua objek GDI dihapus sebelum EndPaint.
fn emo_paint(h: HWND) {
    // SAFETY: PAINTSTRUCT dan RECT nol-kan lalu diisi sistem; BeginPaint dan EndPaint dipasangkan di bawah.
    let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    let mut rc: RECT = unsafe { std::mem::zeroed() };
    let items = emo_visible();
    let cat = EMO_CAT.get().min(EMO_CATS.len() - 1);
    let query = emo_qcell().borrow().clone();
    unsafe {
        let dc = BeginPaint(h, &mut ps);
        let _ = GetClientRect(h, &mut rc);
        let bg = CreateSolidBrush(COLORREF(EMO_BG));
        let sel = CreateSolidBrush(COLORREF(EMO_SEL));
        let tab_on = CreateSolidBrush(COLORREF(EMO_TAB_ON));
        let _ = FillRect(dc, &rc, bg);
        let _ = SetBkMode(dc, TRANSPARENT);
        let old = SelectObject(dc, GetStockObject(DEFAULT_GUI_FONT));
        let judul_teks = if query.is_empty() {
            EMO_CATS[cat].name.to_string()
        } else {
            format!("Cari: {} ({} hasil)", query, items.len())
        };
        let judul = RECT { left: EMO_PAD + 4, top: 0, right: rc.right - EMO_PAD, bottom: EMO_HEAD };
        emo_draw(dc, &judul_teks, judul, EMO_DIM, DT_LEFT.0);

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

        // Baris tab kategori; saat pencarian aktif tidak ada tab yang disorot.
        // Glyph emoji (ikon tab dan sel) dikumpulkan dulu, digambar setelah semua sorot GDI:
        // lewat D2D berwarna; bila gagal, jatuh ke GDI monokrom.
        let mut glyphs: Vec<(&str, RECT)> = Vec::with_capacity(EMO_CATS.len() + EMO_MAX_SHOWN);
        for (i, c) in EMO_CATS.iter().enumerate() {
            let x0 = EMO_PAD + i as i32 * EMO_CELL;
            let tab = RECT { left: x0, top: EMO_HEAD, right: x0 + EMO_CELL, bottom: EMO_GRID_Y };
            if i == cat && query.is_empty() {
                let _ = FillRect(dc, &tab, tab_on);
            }
            glyphs.push((c.icon, tab));
        }

        if items.is_empty() {
            let r = RECT { left: EMO_PAD, top: EMO_GRID_Y, right: rc.right - EMO_PAD, bottom: EMO_GRID_Y + EMO_CELL };
            emo_draw(dc, "Tidak ada hasil", r, EMO_DIM, DT_CENTER.0);
        }

        let cur = EMO_SEL_IDX.get();
        for (i, it) in items.iter().enumerate() {
            let x0 = EMO_PAD + (i % EMO_COLS) as i32 * EMO_CELL;
            let y0 = EMO_GRID_Y + (i / EMO_COLS) as i32 * EMO_CELL;
            if i == cur {
                let r = RECT { left: x0 + 2, top: y0 + 2, right: x0 + EMO_CELL - 2, bottom: y0 + EMO_CELL - 2 };
                let _ = FillRect(dc, &r, sel);
            }
            let cell = RECT { left: x0, top: y0, right: x0 + EMO_CELL, bottom: y0 + EMO_CELL };
            glyphs.push((it.ch, cell));
        }

        if !emo_draw_color(dc, rc, &glyphs) {
            for (s, r) in &glyphs {
                emo_draw(dc, s, *r, EMO_FG, DT_CENTER.0);
            }
        }

        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteObject(HGDIOBJ(bg.0));
        let _ = DeleteObject(HGDIOBJ(sel.0));
        let _ = DeleteObject(HGDIOBJ(tab_on.0));
        let _ = EndPaint(h, &ps);
    }
}

/// Gambar glyph emoji berwarna lewat Direct2D (`D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT`) ke `dc`.
/// `client` adalah area klien jendela (sub-rect `BindDC`). Mengembalikan false bila target DC,
/// kuas, format, atau EndDraw gagal; pemanggil lalu menggambar ulang dengan GDI.
/// Pola mengikuti `create_target` (core_c1) dan `draw_text_units`.
fn emo_draw_color(dc: HDC, client: RECT, glyphs: &[(&str, RECT)]) -> bool {
    use windows::Win32::Graphics::Direct2D::Common::*;
    use windows::Win32::Graphics::Direct2D::*;
    use windows::Win32::Graphics::DirectWrite::{DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL};
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
    let Some(d2d) = D2D.borrow().as_ref().cloned() else { return false };
    let Some(fmt) = make_format("Segoe UI Emoji", EMO_GLYPH_PX, DWRITE_FONT_WEIGHT_NORMAL, true) else {
        return false;
    };
    let p = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE, // sama dengan create_target: tanpa GPU
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
        ..Default::default()
    };
    let Ok(rt) = (unsafe { d2d.CreateDCRenderTarget(&p) }) else { return false };
    let white = D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    let Ok(brush) = (unsafe { rt.CreateSolidColorBrush(&white, None) }) else { return false };
    if unsafe { rt.BindDC(dc, &client) }.is_err() {
        return false;
    }
    let b: &ID2D1Brush = &brush;
    unsafe {
        rt.BeginDraw();
        for (s, r) in glyphs {
            let units: Vec<u16> = s.encode_utf16().collect();
            let lay = D2D_RECT_F {
                left: r.left as f32,
                top: r.top as f32,
                right: r.right as f32,
                bottom: r.bottom as f32,
            };
            rt.DrawText(
                &units,
                &fmt,
                &lay as *const D2D_RECT_F,
                b,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
        rt.EndDraw(None, None).is_ok()
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

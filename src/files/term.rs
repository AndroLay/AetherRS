//! Panel terminal F4 win12files (2026-11-19, permintaan pengguna: "seperti Dolphin"). Bukan fitur File Explorer.
//! Shell asli (pwsh / powershell / cmd, atau `[files] terminal=` di settings.ini) dijalankan lewat ConPTY; keluarannya
//! diurai emulator VT ringan di sini lalu digambar dengan GDI (Cascadia Mono, warna mengikuti tema terang/gelap).
//! Folder panel ikut folder aktif (cd otomatis saat shell sedang menganggur). Satu sesi per jendela; F4 menyembunyikan
//! tanpa menutup sesi; `exit` di shell menutup panel.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, COLORREF, HANDLE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush, DeleteDC,
    DeleteObject, EndPaint, ExtTextOutW, FillRect, FrameRect, GetDC, GetTextMetricsW, InvalidateRect, ReleaseDC,
    SelectObject, SetBkColor, SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, ETO_CLIPPED,
    ETO_OPAQUE, FF_MODERN, FIXED_PITCH, FW_NORMAL, HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SRCCOPY,
    TEXTMETRICW,
};
use windows::Win32::Storage::FileSystem::{ReadFile, SearchPathW, WriteFile};
use windows::Win32::System::Console::{ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, UpdateProcThreadAttribute,
    EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, STARTUPINFOEXW,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetWindowLongPtrW, LoadCursorW, PostMessageW, RegisterClassW,
    SetWindowLongPtrW, CS_DBLCLKS, GWLP_USERDATA, IDC_IBEAM, WINDOW_EX_STYLE, WM_APP, WM_CHAR, WM_DESTROY,
    WM_ERASEBKGND, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCDESTROY,
    WM_PAINT, WM_RBUTTONUP, WM_SETFOCUS, WM_SIZE, WM_SYSCHAR, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS,
};

use crate::wide::{pcw, to_wide_z};

const CLASS: &str = "Win12FilesTerm";
const WM_TERM_DATA: u32 = WM_APP + 0x40;
const WM_TERM_EXIT: u32 = WM_APP + 0x41;
/// Diposting ke jendela frame saat shell keluar (`exit`): frame menyembunyikan panel.
pub(crate) const WM_FM_TERM_CLOSED: u32 = WM_APP + 0x42;
/// Folder shell berubah (cd, z, Set-Location): LPARAM = Box<String> mentah, penerima wajib Box::from_raw.
pub(crate) const WM_FM_TERM_CWD: u32 = WM_APP + 0x43;
const SCROLLBACK: usize = 3000;

// ---- warna ----

const C_DEF: u32 = 0x0200_0000;
const C_IDX: u32 = 0x0100_0000;

#[derive(Clone, Copy)]
struct Theme {
    fg: u32,
    bg: u32,
    cursor: u32,
    sel: u32,
    pal: [u32; 16],
}

fn theme(dark: bool) -> Theme {
    if dark {
        // Campbell (Windows Terminal) di atas latar isi Explorer gelap.
        Theme {
            fg: 0xCCCCCC,
            bg: 0x191919,
            cursor: 0xFFFFFF,
            sel: 0x264F78,
            pal: [
                0x0C0C0C, 0xC50F1F, 0x13A10E, 0xC19C00, 0x0037DA, 0x881798, 0x3A96DD, 0xCCCCCC, 0x767676, 0xE74856,
                0x16C60C, 0xF9F1A5, 0x3B78FF, 0xB4009E, 0x61D6D6, 0xF2F2F2,
            ],
        }
    } else {
        // One Half Light (Windows Terminal).
        Theme {
            fg: 0x383A42,
            bg: 0xFFFFFF,
            cursor: 0x383A42,
            sel: 0xBFDBFE,
            pal: [
                0x383A42, 0xE45649, 0x50A14F, 0xC18301, 0x0184BC, 0xA626A4, 0x0997B3, 0xFAFAFA, 0x4F525D, 0xDF6C75,
                0x98C379, 0xE4C07A, 0x61AFEF, 0xC577DD, 0x56B5C1, 0xFFFFFF,
            ],
        }
    }
}

fn rgb(c: u32) -> COLORREF {
    COLORREF(((c >> 16) & 0xFF) | (c & 0xFF00) | ((c & 0xFF) << 16))
}

fn idx256(i: u32, th: &Theme) -> u32 {
    let i = i & 0xFF;
    if i < 16 {
        return th.pal[i as usize];
    }
    if i < 232 {
        let v = i - 16;
        let lvl = |x: u32| if x == 0 { 0 } else { 55 + x * 40 };
        return (lvl(v / 36) << 16) | (lvl((v / 6) % 6) << 8) | lvl(v % 6);
    }
    let g = 8 + (i - 232) * 10;
    (g << 16) | (g << 8) | g
}

// ---- layar ----

const F_BOLD: u8 = 1;
const F_INV: u8 = 2;
const F_UL: u8 = 4;

#[derive(Clone, Copy, PartialEq)]
struct Cell {
    ch: u32,
    fg: u32,
    bg: u32,
    fl: u8,
}

const BLANK: Cell = Cell { ch: b' ' as u32, fg: C_DEF, bg: C_DEF, fl: 0 };

#[derive(Clone, Copy)]
struct Attr {
    fg: u32,
    bg: u32,
    fl: u8,
}

const ATTR0: Attr = Attr { fg: C_DEF, bg: C_DEF, fl: 0 };

#[derive(PartialEq, Clone, Copy)]
enum St {
    Ground,
    Esc,
    Csi,
    Osc,
    OscEsc,
    Skip1,
}

struct Saved {
    cells: Vec<Cell>,
    x: usize,
    y: usize,
}

struct Term {
    hwnd: HWND,
    frame: HWND,
    hpc: HPCON,
    input: HANDLE,
    proc: HANDLE,
    pid: u32,
    cmd_kind: u8, // 0 PowerShell, 1 cmd, 2 posix
    cols: usize,
    rows: usize,
    cells: Vec<Cell>,
    x: usize,
    y: usize,
    wrap: bool,
    attr: Attr,
    save: (usize, usize, Attr),
    top: usize,
    bot: usize,
    cursor_on: bool,
    app_cursor: bool,
    paste_br: bool,
    alt: Option<Saved>,
    sb: VecDeque<Vec<Cell>>,
    view: usize,
    st: St,
    params: Vec<i64>,
    cur: Option<i64>,
    private: u8,
    osc: Vec<u8>,
    utf8: Vec<u8>,
    font: HFONT,
    cw: i32,
    ch: i32,
    dark: bool,
    focused: bool,
    sel_a: Option<(usize, usize)>,
    sel_b: Option<(usize, usize)>,
    selecting: bool,
    skip_char: bool,
    hi_sur: u16,
    last_cd: String,
    /// Folder terakhir yang dilaporkan shell lewat OSC 9;9 / OSC 7 (prompt).
    cwd_rep: String,
}

fn with_term<R>(h: HWND, f: impl FnOnce(&mut Term) -> R) -> Option<R> {
    let p = unsafe { GetWindowLongPtrW(h, GWLP_USERDATA) } as *const std::cell::RefCell<Term>;
    if p.is_null() {
        return None;
    }
    // SAFETY: pointer Box<RefCell<Term>> dipasang term_open dan dilepas di WM_NCDESTROY (sebelum with_term).
    // try_borrow_mut mencegah aliasing &mut bila pesan Win32 masuk ulang (re-entrancy).
    let cell = unsafe { &*p };
    let mut guard = cell.try_borrow_mut().ok()?;
    Some(f(&mut guard))
}

impl Term {
    fn at(&mut self, x: usize, y: usize) -> &mut Cell {
        let i = y * self.cols + x;
        &mut self.cells[i]
    }

    fn blank(&self) -> Cell {
        Cell { ch: b' ' as u32, fg: C_DEF, bg: self.attr.bg, fl: 0 }
    }

    fn write(&self, data: &[u8]) {
        if self.input.is_invalid() || data.is_empty() {
            return;
        }
        let mut n = 0u32;
        let _ = unsafe { WriteFile(self.input, Some(data), Some(&mut n), None) };
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(2);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        let mut cells = vec![BLANK; cols * rows];
        // Pertahankan baris bawah (dekat kursor) bila tinggi berkurang.
        let shift = if self.y >= rows { self.y + 1 - rows } else { 0 };
        for y in 0..rows.min(self.rows.saturating_sub(shift)) {
            for x in 0..cols.min(self.cols) {
                cells[y * cols + x] = self.cells[(y + shift) * self.cols + x];
            }
        }
        self.cells = cells;
        self.cols = cols;
        self.rows = rows;
        self.y = self.y.saturating_sub(shift).min(rows - 1);
        self.x = self.x.min(cols - 1);
        self.top = 0;
        self.bot = rows - 1;
        self.wrap = false;
        self.alt = None.or(self.alt.take().map(|mut s| {
            s.cells = vec![BLANK; cols * rows];
            s.x = s.x.min(cols - 1);
            s.y = s.y.min(rows - 1);
            s
        }));
        if self.hpc.0 != 0 {
            let _ = unsafe { ResizePseudoConsole(self.hpc, COORD { X: cols as i16, Y: rows as i16 }) };
        }
    }

    // ---- operasi layar ----

    fn scroll_up(&mut self, n: usize) {
        let (t, b) = (self.top, self.bot);
        for _ in 0..n.max(1) {
            if t == 0 && self.alt.is_none() {
                let row = self.cells[0..self.cols].to_vec();
                self.sb.push_back(row);
                if self.sb.len() > SCROLLBACK {
                    self.sb.pop_front();
                    self.sel_a = None;
                    self.sel_b = None;
                }
                if self.view > 0 {
                    self.view = (self.view + 1).min(self.sb.len());
                }
            }
            let c = self.cols;
            self.cells.copy_within((t + 1) * c..(b + 1) * c, t * c);
            let bl = self.blank();
            for x in 0..c {
                self.cells[b * c + x] = bl;
            }
        }
    }

    fn scroll_down(&mut self, n: usize) {
        let (t, b) = (self.top, self.bot);
        for _ in 0..n.max(1) {
            let c = self.cols;
            self.cells.copy_within(t * c..b * c, (t + 1) * c);
            let bl = self.blank();
            for x in 0..c {
                self.cells[t * c + x] = bl;
            }
        }
    }

    fn linefeed(&mut self) {
        if self.y == self.bot {
            self.scroll_up(1);
        } else if self.y + 1 < self.rows {
            self.y += 1;
        }
    }

    fn put(&mut self, ch: u32) {
        if self.wrap {
            self.x = 0;
            self.linefeed();
            self.wrap = false;
        }
        let a = self.attr;
        let (x, y) = (self.x, self.y);
        *self.at(x, y) = Cell { ch, fg: a.fg, bg: a.bg, fl: a.fl };
        if self.x + 1 >= self.cols {
            self.wrap = true;
        } else {
            self.x += 1;
        }
    }

    fn erase(&mut self, from: usize, to: usize) {
        let bl = self.blank();
        let end = to.min(self.cells.len());
        for i in from.min(end)..end {
            self.cells[i] = bl;
        }
    }

    fn p(&self, i: usize, d: i64) -> i64 {
        match self.params.get(i) {
            Some(&v) if v > 0 => v,
            _ => d,
        }
    }

    fn sgr(&mut self) {
        if self.params.is_empty() {
            self.attr = ATTR0;
            return;
        }
        let ps = self.params.clone();
        let mut i = 0;
        while i < ps.len() {
            let v = ps[i];
            match v {
                0 => self.attr = ATTR0,
                1 => self.attr.fl |= F_BOLD,
                4 => self.attr.fl |= F_UL,
                7 => self.attr.fl |= F_INV,
                22 => self.attr.fl &= !F_BOLD,
                24 => self.attr.fl &= !F_UL,
                27 => self.attr.fl &= !F_INV,
                30..=37 => self.attr.fg = C_IDX | (v - 30) as u32,
                39 => self.attr.fg = C_DEF,
                40..=47 => self.attr.bg = C_IDX | (v - 40) as u32,
                49 => self.attr.bg = C_DEF,
                90..=97 => self.attr.fg = C_IDX | (v - 90 + 8) as u32,
                100..=107 => self.attr.bg = C_IDX | (v - 100 + 8) as u32,
                38 | 48 => {
                    let mode = ps.get(i + 1).copied().unwrap_or(0);
                    let col = if mode == 5 {
                        let c = C_IDX | (ps.get(i + 2).copied().unwrap_or(0) as u32 & 0xFF);
                        i += 2;
                        Some(c)
                    } else if mode == 2 {
                        let r = ps.get(i + 2).copied().unwrap_or(0) as u32 & 0xFF;
                        let g = ps.get(i + 3).copied().unwrap_or(0) as u32 & 0xFF;
                        let b = ps.get(i + 4).copied().unwrap_or(0) as u32 & 0xFF;
                        i += 4;
                        Some((r << 16) | (g << 8) | b)
                    } else {
                        None
                    };
                    if let Some(c) = col {
                        if v == 38 {
                            self.attr.fg = c;
                        } else {
                            self.attr.bg = c;
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn csi(&mut self, f: u8) {
        let (cols, rows) = (self.cols, self.rows);
        let n = self.p(0, 1) as usize;
        if self.private == b'?' {
            let on = f == b'h';
            if f == b'h' || f == b'l' {
                for &m in &self.params.clone() {
                    match m {
                        1 => self.app_cursor = on,
                        25 => self.cursor_on = on,
                        2004 => self.paste_br = on,
                        47 | 1047 | 1049 => self.set_alt(on),
                        _ => {}
                    }
                }
            }
            return;
        }
        match f {
            b'A' => self.y = self.y.saturating_sub(n),
            b'B' | b'e' => self.y = (self.y + n).min(rows - 1),
            b'C' | b'a' => self.x = (self.x + n).min(cols - 1),
            b'D' => self.x = self.x.saturating_sub(n),
            b'E' => {
                self.y = (self.y + n).min(rows - 1);
                self.x = 0;
            }
            b'F' => {
                self.y = self.y.saturating_sub(n);
                self.x = 0;
            }
            b'G' | b'`' => self.x = (n - 1).min(cols - 1),
            b'd' => self.y = (n - 1).min(rows - 1),
            b'H' | b'f' => {
                self.y = (self.p(0, 1) as usize - 1).min(rows - 1);
                self.x = (self.p(1, 1) as usize - 1).min(cols - 1);
            }
            b'J' => {
                let cur = self.y * cols + self.x;
                match self.params.first().copied().unwrap_or(0) {
                    0 => self.erase(cur, cols * rows),
                    1 => self.erase(0, cur + 1),
                    2 => self.erase(0, cols * rows),
                    3 => {
                        self.erase(0, cols * rows);
                        self.sb.clear();
                        self.view = 0;
                    }
                    _ => {}
                }
            }
            b'K' => {
                let row = self.y * cols;
                match self.params.first().copied().unwrap_or(0) {
                    0 => self.erase(row + self.x, row + cols),
                    1 => self.erase(row, row + self.x + 1),
                    2 => self.erase(row, row + cols),
                    _ => {}
                }
            }
            b'L' | b'M' => {
                if self.y >= self.top && self.y <= self.bot {
                    let t = self.top;
                    self.top = self.y;
                    if f == b'L' {
                        self.scroll_down(n.min(self.bot - self.y + 1));
                    } else {
                        let sbk = std::mem::take(&mut self.sb);
                        let alt = self.alt.take();
                        // Hapus baris di tengah tidak masuk riwayat gulir.
                        self.alt = Some(Saved { cells: Vec::new(), x: 0, y: 0 });
                        self.scroll_up(n.min(self.bot - self.y + 1));
                        self.alt = alt;
                        self.sb = sbk;
                    }
                    self.top = t;
                    self.x = 0;
                }
            }
            b'@' => {
                let row = self.y * cols;
                let n = n.min(cols - self.x);
                self.cells.copy_within(row + self.x..row + cols - n, row + self.x + n);
                self.erase(row + self.x, row + self.x + n);
            }
            b'P' => {
                let row = self.y * cols;
                let n = n.min(cols - self.x);
                self.cells.copy_within(row + self.x + n..row + cols, row + self.x);
                self.erase(row + cols - n, row + cols);
            }
            b'X' => {
                let row = self.y * cols;
                self.erase(row + self.x, row + (self.x + n).min(cols));
            }
            b'S' => self.scroll_up(n),
            b'T' => self.scroll_down(n),
            b'm' => self.sgr(),
            b'r' => {
                let t = (self.p(0, 1) as usize).saturating_sub(1);
                let b = (self.p(1, rows as i64) as usize).saturating_sub(1).min(rows - 1);
                if t < b {
                    self.top = t;
                    self.bot = b;
                }
                self.x = 0;
                self.y = 0;
            }
            b's' => self.save = (self.x, self.y, self.attr),
            b'u' => {
                self.x = self.save.0.min(cols - 1);
                self.y = self.save.1.min(rows - 1);
                self.attr = self.save.2;
            }
            b'n' => {
                if self.params.first() == Some(&6) {
                    let s = format!("\x1b[{};{}R", self.y + 1, self.x + 1);
                    self.write(s.as_bytes());
                } else if self.params.first() == Some(&5) {
                    self.write(b"\x1b[0n");
                }
            }
            b'c' => self.write(b"\x1b[?1;0c"),
            _ => {}
        }
        self.wrap = false;
    }

    fn set_alt(&mut self, on: bool) {
        if on && self.alt.is_none() {
            let main = std::mem::replace(&mut self.cells, vec![BLANK; self.cols * self.rows]);
            self.alt = Some(Saved { cells: main, x: self.x, y: self.y });
        } else if !on {
            if let Some(s) = self.alt.take() {
                if s.cells.len() == self.cols * self.rows {
                    self.cells = s.cells;
                } else {
                    self.cells = vec![BLANK; self.cols * self.rows];
                }
                self.x = s.x.min(self.cols - 1);
                self.y = s.y.min(self.rows - 1);
            }
        }
        self.top = 0;
        self.bot = self.rows - 1;
    }

    fn control(&mut self, c: u32) {
        match c {
            0x08 => {
                self.x = self.x.saturating_sub(1);
                self.wrap = false;
            }
            0x09 => self.x = (((self.x / 8) + 1) * 8).min(self.cols - 1),
            0x0A..=0x0C => self.linefeed(),
            0x0D => {
                self.x = 0;
                self.wrap = false;
            }
            _ => {}
        }
    }

    fn feed_char(&mut self, c: u32) {
        match self.st {
            St::Ground => {
                if c == 0x1B {
                    self.st = St::Esc;
                } else if c < 0x20 {
                    self.control(c);
                } else if c != 0x7F {
                    self.put(c);
                }
            }
            St::Esc => {
                self.st = St::Ground;
                match c {
                    0x5B => {
                        self.st = St::Csi;
                        self.params.clear();
                        self.cur = None;
                        self.private = 0;
                    }
                    0x5D => {
                        self.st = St::Osc;
                        self.osc.clear();
                    }
                    0x37 => self.save = (self.x, self.y, self.attr),
                    0x38 => {
                        self.x = self.save.0.min(self.cols - 1);
                        self.y = self.save.1.min(self.rows - 1);
                        self.attr = self.save.2;
                    }
                    0x4D => {
                        if self.y == self.top {
                            self.scroll_down(1);
                        } else {
                            self.y = self.y.saturating_sub(1);
                        }
                    }
                    0x44 => self.linefeed(),
                    0x45 => {
                        self.x = 0;
                        self.linefeed();
                    }
                    0x63 => {
                        self.attr = ATTR0;
                        let n = self.cells.len();
                        self.erase(0, n);
                        self.x = 0;
                        self.y = 0;
                    }
                    0x28..=0x2B => self.st = St::Skip1,
                    _ => {}
                }
            }
            St::Skip1 => self.st = St::Ground,
            St::Csi => match c {
                0x30..=0x39 => self.cur = Some(self.cur.unwrap_or(0).saturating_mul(10) + (c - 0x30) as i64),
                0x3A | 0x3B => {
                    self.params.push(self.cur.take().unwrap_or(0));
                }
                0x3C..=0x3F => {
                    if self.params.is_empty() && self.cur.is_none() {
                        self.private = c as u8;
                    }
                }
                0x20..=0x2F => {}
                0x40..=0x7E => {
                    if let Some(v) = self.cur.take() {
                        self.params.push(v);
                    }
                    self.st = St::Ground;
                    self.csi(c as u8);
                }
                0x1B => self.st = St::Esc,
                _ if c < 0x20 => self.control(c),
                _ => self.st = St::Ground,
            },
            St::Osc => match c {
                0x07 => {
                    self.st = St::Ground;
                    self.osc_done();
                }
                0x1B => self.st = St::OscEsc,
                _ => {
                    if self.osc.len() < 1024 {
                        // simpan UTF-8 utuh (jalur bisa berisi huruf non-ASCII)
                        let mut b = [0u8; 4];
                        let s = char::from_u32(c).unwrap_or('?').encode_utf8(&mut b);
                        self.osc.extend_from_slice(s.as_bytes());
                    }
                }
            },
            St::OscEsc => {
                self.st = St::Ground;
                if c == u32::from(b'\\') {
                    self.osc_done();
                }
            }
        }
    }

    /// OSC selesai. Folder kerja shell: `9;9;"C:\path"` (Windows Terminal) atau `7;file://host/C:/path`.
    /// Folder baru dikirim ke jendela File Explorer agar ikut pindah (2026-11-19, permintaan pengguna).
    fn osc_done(&mut self) {
        let s = String::from_utf8_lossy(&self.osc).into_owned();
        self.osc.clear();
        let path = if let Some(p) = s.strip_prefix("9;9;") {
            p.trim().trim_matches('"').to_string()
        } else if let Some(u) = s.strip_prefix("7;file://") {
            // buang host, dekode %XX, ubah / ke \
            let rest = u.find('/').map(|i| &u[i..]).unwrap_or("");
            let dec = pct_decode(rest);
            let dec = dec.strip_prefix('/').filter(|d| d.as_bytes().get(1) == Some(&b':')).unwrap_or(&dec);
            dec.replace('/', "\\")
        } else {
            return;
        };
        if path.is_empty() || path.starts_with("Microsoft.PowerShell") || path.eq_ignore_ascii_case(&self.cwd_rep) {
            return;
        }
        self.cwd_rep = path.clone();
        // folder ini sekarang sudah "diketahui" terminal: File Explorer yang pindah ke sini tidak mengetik cd balik
        self.last_cd = path.clone();
        let raw = Box::into_raw(Box::new(path));
        // SAFETY: kepemilikan pindah ke penerima; bila gagal dikirim, dibebaskan di sini.
        unsafe {
            if PostMessageW(Some(self.frame), WM_FM_TERM_CWD, WPARAM(0), LPARAM(raw as isize)).is_err() {
                drop(Box::from_raw(raw));
            }
        }
    }

    fn feed(&mut self, data: &[u8]) {
        let mut buf = std::mem::take(&mut self.utf8);
        buf.extend_from_slice(data);
        let mut i = 0;
        while i < buf.len() {
            let b = buf[i];
            let (len, init) = if b < 0x80 {
                (1, b as u32)
            } else if b >= 0xF0 {
                (4, (b & 0x07) as u32)
            } else if b >= 0xE0 {
                (3, (b & 0x0F) as u32)
            } else if b >= 0xC0 {
                (2, (b & 0x1F) as u32)
            } else {
                i += 1;
                continue;
            };
            if i + len > buf.len() {
                self.utf8 = buf[i..].to_vec();
                return;
            }
            let mut c = init;
            for k in 1..len {
                c = (c << 6) | (buf[i + k] & 0x3F) as u32;
            }
            self.feed_char(c);
            i += len;
        }
    }

    // ---- teks terlihat dan pilihan ----

    fn line_abs(&self, abs: usize) -> Option<&[Cell]> {
        if abs < self.sb.len() {
            Some(&self.sb[abs][..])
        } else {
            let r = abs - self.sb.len();
            if r < self.rows {
                Some(&self.cells[r * self.cols..(r + 1) * self.cols])
            } else {
                None
            }
        }
    }

    fn first_visible(&self) -> usize {
        self.sb.len().saturating_sub(self.view)
    }

    fn sel_range(&self) -> Option<((usize, usize), (usize, usize))> {
        let (a, b) = (self.sel_a?, self.sel_b?);
        if a == b {
            return None;
        }
        Some(if a <= b { (a, b) } else { (b, a) })
    }

    fn selected(&self, abs: usize, x: usize) -> bool {
        match self.sel_range() {
            Some((s, e)) => (abs, x) >= s && (abs, x) < e,
            None => false,
        }
    }

    fn sel_text(&self) -> String {
        let Some((s, e)) = self.sel_range() else {
            return String::new();
        };
        let mut out = String::new();
        for abs in s.0..=e.0 {
            let Some(line) = self.line_abs(abs) else { break };
            let x0 = if abs == s.0 { s.1 } else { 0 };
            let x1 = if abs == e.0 { e.1.min(line.len()) } else { line.len() };
            let mut row: String = line[x0.min(x1)..x1].iter().filter_map(|c| char::from_u32(c.ch)).collect();
            if abs != e.0 {
                row = row.trim_end().to_string();
                row.push_str("\r\n");
            }
            out.push_str(&row);
        }
        out
    }

    fn cell_at(&self, px: i32, py: i32) -> (usize, usize) {
        let pad = self.pad();
        let col = ((px - pad).max(0) / self.cw.max(1)) as usize;
        let row = ((py - pad).max(0) / self.ch.max(1)) as usize;
        (self.first_visible() + row.min(self.rows - 1), col.min(self.cols))
    }

    fn pad(&self) -> i32 {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) } as i32;
        8 * dpi.max(96) / 96
    }
}

// ---- jendela ----

fn ensure_class() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let cls = to_wide_z(CLASS);
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(term_proc),
            hInstance: crate::files::frame::fm_hinstance(),
            hCursor: unsafe { LoadCursorW(None, IDC_IBEAM) }.unwrap_or_default(),
            lpszClassName: pcw(&cls),
            ..Default::default()
        };
        // Nama kelas harus hidup selama proses: dibocorkan sekali.
        std::mem::forget(cls);
        unsafe {
            RegisterClassW(&wc);
        }
    });
}

fn make_font(dpi: i32) -> HFONT {
    let mk = |face: &str| {
        let name = to_wide_z(face);
        unsafe {
            CreateFontW(
                -((10 * dpi + 36) / 72),
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
                (FIXED_PITCH.0 as u32) | (FF_MODERN.0 as u32),
                pcw(&name),
            )
        }
    };
    mk("Cascadia Mono")
}

/// Perintah shell: `[files] terminal=` di settings.ini, lalu pwsh, lalu Windows PowerShell.
fn shell_command() -> String {
    let ini = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("settings.ini")))
        .unwrap_or_default();
    if let Ok(text) = std::fs::read_to_string(&ini) {
        let mut sect = String::new();
        for line in text.lines() {
            let l = line.trim();
            if l.starts_with('[') {
                sect = l.to_ascii_lowercase();
            } else if sect == "[files]" {
                if let Some(v) = l.strip_prefix("terminal=") {
                    if !v.trim().is_empty() {
                        return v.trim().to_string();
                    }
                }
            }
        }
    }
    let mut buf = [0u16; 512];
    let name = to_wide_z("pwsh.exe");
    let n = unsafe { SearchPathW(PCWSTR::null(), pcw(&name), PCWSTR::null(), Some(&mut buf), None) } as usize;
    if n > 0 && n < buf.len() {
        return format!("\"{}\" -NoLogo", String::from_utf16_lossy(&buf[..n]));
    }
    "powershell.exe -NoLogo".to_string()
}

/// Dekode %XX dalam URL file:// (UTF-8).
fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && s.is_char_boundary(i + 3) {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Base64 standar (untuk -EncodedCommand PowerShell).
fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for ch in data.chunks(3) {
        let n = (u32::from(ch[0]) << 16)
            | (u32::from(*ch.get(1).unwrap_or(&0)) << 8)
            | u32::from(*ch.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if ch.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if ch.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Kait prompt PowerShell (dijalankan setelah profil, jadi prompt dari profil/zoxide/oh-my-posh tetap dipakai):
/// setiap prompt diawali OSC 9;9 berisi folder sistem berkas aktif, seperti saran Windows Terminal.
const PS_HOOK: &str = r#"$global:__w12Prompt = $function:prompt
function global:prompt {
  $o = ''
  $l = $executionContext.SessionState.Path.CurrentLocation
  if ($l.Provider.Name -eq 'FileSystem') { $o = "$([char]27)]9;9;`"$($l.ProviderPath)`"$([char]27)\" }
  $p = if ($global:__w12Prompt) { & $global:__w12Prompt } else { "PS $l> " }
  "$o$p"
}"#;

/// Tambahkan pelapor folder ke perintah shell bila aman (tidak menimpa -Command/-File atau /k milik pengguna).
fn with_cwd_report(cmd: String) -> String {
    let l = cmd.to_ascii_lowercase();
    match kind_of(&cmd) {
        0 if !l.contains(" -command") && !l.contains(" -c ") && !l.contains(" -file") && !l.contains("-encodedcommand") => {
            let utf16: Vec<u8> = PS_HOOK.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
            format!("{} -NoExit -EncodedCommand {}", cmd, b64(&utf16))
        }
        1 if !l.contains("/k") && !l.contains("/c") => format!("{} /k prompt $e]9;9;$P$e\\$P$G", cmd),
        _ => cmd,
    }
}

fn kind_of(cmd: &str) -> u8 {
    let l = cmd.to_ascii_lowercase();
    if l.contains("pwsh") || l.contains("powershell") {
        0
    } else if l.contains("cmd") {
        1
    } else {
        2
    }
}

/// Buat panel terminal (anak `frame`) dan jalankan shell di `cwd`. None bila ConPTY/proses gagal.
pub(crate) fn term_open(frame: HWND, cwd: &str, dark: bool) -> Option<HWND> {
    ensure_class();
    let cls = to_wide_z(CLASS);
    let h = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            pcw(&cls),
            PCWSTR::null(),
            WS_CHILD | WS_CLIPSIBLINGS,
            0,
            0,
            100,
            100,
            Some(frame),
            None,
            Some(crate::files::frame::fm_hinstance()),
            None,
        )
    }
    .ok()?;
    let dpi = unsafe { GetDpiForWindow(h) } as i32;
    let font = make_font(dpi.max(96));
    let (cw, ch) = unsafe {
        let dc = GetDC(Some(h));
        let old = SelectObject(dc, HGDIOBJ(font.0));
        let mut tm: TEXTMETRICW = zeroed();
        let _ = GetTextMetricsW(dc, &mut tm);
        SelectObject(dc, old);
        ReleaseDC(Some(h), dc);
        (tm.tmAveCharWidth.max(1), tm.tmHeight.max(1))
    };
    let cols = 120usize;
    let rows = 24usize;
    let cmd = shell_command();
    let kind = kind_of(&cmd);
    let cmd = with_cwd_report(cmd);
    let mut t = Box::new(Term {
        hwnd: h,
        frame,
        hpc: HPCON(0),
        input: HANDLE::default(),
        proc: HANDLE::default(),
        pid: 0,
        cmd_kind: kind,
        cols,
        rows,
        cells: vec![BLANK; cols * rows],
        x: 0,
        y: 0,
        wrap: false,
        attr: ATTR0,
        save: (0, 0, ATTR0),
        top: 0,
        bot: rows - 1,
        cursor_on: true,
        app_cursor: false,
        paste_br: false,
        alt: None,
        sb: VecDeque::new(),
        view: 0,
        st: St::Ground,
        params: Vec::new(),
        cur: None,
        private: 0,
        osc: Vec::new(),
        utf8: Vec::new(),
        font,
        cw,
        ch,
        dark,
        focused: false,
        sel_a: None,
        sel_b: None,
        selecting: false,
        skip_char: false,
        hi_sur: 0,
        last_cd: cwd.to_string(),
        cwd_rep: String::new(),
    });
    if !spawn(&mut t, &cmd, cwd) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(font.0));
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(h);
        }
        return None;
    }
    let cell = Box::new(std::cell::RefCell::new(*t));
    unsafe {
        SetWindowLongPtrW(h, GWLP_USERDATA, Box::into_raw(cell) as isize);
    }
    Some(h)
}

fn spawn(t: &mut Term, cmd: &str, cwd: &str) -> bool {
    unsafe {
        let (mut in_r, mut in_w, mut out_r, mut out_w) =
            (HANDLE::default(), HANDLE::default(), HANDLE::default(), HANDLE::default());
        if CreatePipe(&mut in_r, &mut in_w, None, 0).is_err() {
            return false;
        }
        if CreatePipe(&mut out_r, &mut out_w, None, 0).is_err() {
            let _ = CloseHandle(in_r);
            let _ = CloseHandle(in_w);
            return false;
        }
        let hpc = CreatePseudoConsole(COORD { X: t.cols as i16, Y: t.rows as i16 }, in_r, out_w, 0);
        let _ = CloseHandle(in_r);
        let _ = CloseHandle(out_w);
        let Ok(hpc) = hpc else {
            let _ = CloseHandle(in_w);
            let _ = CloseHandle(out_r);
            return false;
        };
        let mut size = 0usize;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
        let mut attr_buf = vec![0u8; size.max(64)];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut c_void);
        let mut ok = InitializeProcThreadAttributeList(Some(list), 1, None, &mut size).is_ok();
        if ok {
            ok = UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                Some(hpc.0 as *const c_void),
                size_of::<HPCON>(),
                None,
                None,
            )
            .is_ok();
        }
        let mut pi = PROCESS_INFORMATION::default();
        if ok {
            let mut si: STARTUPINFOEXW = zeroed();
            si.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
            si.lpAttributeList = list;
            let mut cmdline = to_wide_z(cmd);
            let dir = to_wide_z(cwd);
            let dir_p = if cwd.is_empty() { PCWSTR::null() } else { pcw(&dir) };
            ok = CreateProcessW(
                PCWSTR::null(),
                Some(PWSTR(cmdline.as_mut_ptr())),
                None,
                None,
                false,
                EXTENDED_STARTUPINFO_PRESENT,
                None,
                dir_p,
                &si.StartupInfo,
                &mut pi,
            )
            .is_ok();
            DeleteProcThreadAttributeList(list);
        }
        if !ok {
            ClosePseudoConsole(hpc);
            let _ = CloseHandle(in_w);
            let _ = CloseHandle(out_r);
            crate::log::log(&format!("win12files terminal: gagal menjalankan {}", cmd));
            return false;
        }
        let _ = CloseHandle(pi.hThread);
        t.hpc = hpc;
        t.input = in_w;
        t.pid = pi.dwProcessId;
        let hwnd = t.hwnd.0 as isize;
        let out = out_r.0 as isize;
        std::thread::spawn(move || reader(hwnd, out));
        // ConPTY tidak menutup pipa keluaran saat shell keluar: tunggu prosesnya (thread ini pemilik handle proses).
        let ph = pi.hProcess.0 as isize;
        std::thread::spawn(move || {
            let h = HANDLE(ph as *mut c_void);
            // (closure ini mewarisi blok unsafe fungsi spawn)
            let _ = windows::Win32::System::Threading::WaitForSingleObject(h, windows::Win32::System::Threading::INFINITE);
            let _ = CloseHandle(h);
            let _ = PostMessageW(Some(HWND(hwnd as *mut c_void)), WM_TERM_EXIT, WPARAM(0), LPARAM(0));
        });
        true
    }
}

/// Thread baca keluaran ConPTY: potongan dikirim ke jendela panel. Pemilik handle baca.
fn reader(hwnd: isize, out: isize) {
    let h = HANDLE(out as *mut c_void);
    let hw = HWND(hwnd as *mut c_void);
    let mut buf = vec![0u8; 16384];
    loop {
        let mut n = 0u32;
        let ok = unsafe { ReadFile(h, Some(&mut buf), Some(&mut n), None) }.is_ok();
        if !ok || n == 0 {
            break;
        }
        let data = Box::new(buf[..n as usize].to_vec());
        let p = Box::into_raw(data);
        if unsafe { PostMessageW(Some(hw), WM_TERM_DATA, WPARAM(0), LPARAM(p as isize)) }.is_err() {
            // SAFETY: pesan gagal diposting, kepemilikan kembali ke sini.
            drop(unsafe { Box::from_raw(p) });
            break;
        }
    }
    unsafe {
        let _ = CloseHandle(h);
        let _ = PostMessageW(Some(hw), WM_TERM_EXIT, WPARAM(0), LPARAM(0));
    }
}

/// Fokus ke panel.
pub(crate) fn term_focus(h: HWND) {
    unsafe {
        let _ = SetFocus(Some(h));
    }
}

/// Shell sedang menganggur (tidak punya proses anak), jadi aman mengetikkan cd.
fn shell_idle(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let mut pe: PROCESSENTRY32W = zeroed();
        pe.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut idle = true;
        if Process32FirstW(snap, &mut pe).is_ok() {
            loop {
                if pe.th32ParentProcessID == pid {
                    idle = false;
                    break;
                }
                if Process32NextW(snap, &mut pe).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        idle
    }
}

/// Ikuti folder aktif: ketik perintah cd bila folder berubah dan shell menganggur (seperti Dolphin).
pub(crate) fn term_sync(h: HWND, path: &str) {
    let _ = with_term(h, |t| {
        if path.is_empty() || path.eq_ignore_ascii_case(&t.last_cd) || !shell_idle(t.pid) {
            return;
        }
        t.last_cd = path.to_string();
        let line = match t.cmd_kind {
            0 => format!("\x1bSet-Location -LiteralPath '{}'\r", path.replace('\'', "''")),
            1 => format!("\x1bcd /d \"{}\"\r", path),
            _ => format!("cd '{}'\r", path.replace('\'', "'\\''")),
        };
        t.view = 0;
        t.write(line.as_bytes());
    });
}

/// Ganti tema (terang/gelap) panel.
pub(crate) fn term_set_dark(h: HWND, dark: bool) {
    let _ = with_term(h, |t| {
        t.dark = dark;
        unsafe {
            let _ = InvalidateRect(Some(h), None, false);
        }
    });
}

// ---- gambar ----

fn resolve(c: u32, fg: bool, bold: bool, th: &Theme) -> u32 {
    if c & C_DEF != 0 {
        return if fg { th.fg } else { th.bg };
    }
    if c & C_IDX != 0 {
        let mut i = c & 0xFF;
        if fg && bold && i < 8 {
            i += 8;
        }
        return idx256(i, th);
    }
    c & 0xFF_FFFF
}

fn paint(t: &mut Term, dc: HDC) {
    let mut rc = RECT::default();
    let _ = unsafe { GetClientRect(t.hwnd, &mut rc) };
    let (w, hgt) = (rc.right.max(1), rc.bottom.max(1));
    let th = theme(t.dark);
    unsafe {
        let mem = CreateCompatibleDC(Some(dc));
        let bmp = CreateCompatibleBitmap(dc, w, hgt);
        let ob = SelectObject(mem, HGDIOBJ(bmp.0));
        let of = SelectObject(mem, HGDIOBJ(t.font.0));
        let bgb = CreateSolidBrush(rgb(th.bg));
        FillRect(mem, &rc, bgb);
        let _ = DeleteObject(HGDIOBJ(bgb.0));
        let pad = t.pad();
        let first = t.first_visible();
        let mut text: Vec<u16> = Vec::with_capacity(t.cols * 2);
        let mut dx: Vec<i32> = Vec::with_capacity(t.cols * 2);
        for r in 0..t.rows {
            let abs = first + r;
            let Some(line) = t.line_abs(abs) else { break };
            let line: Vec<Cell> = line.to_vec();
            let y = pad + r as i32 * t.ch;
            let mut x = 0usize;
            while x < line.len().min(t.cols) {
                let c0 = line[x];
                let s0 = t.selected(abs, x);
                let (mut fg, mut bg) = (resolve(c0.fg, true, c0.fl & F_BOLD != 0, &th), resolve(c0.bg, false, false, &th));
                if c0.fl & F_INV != 0 {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if s0 {
                    bg = th.sel;
                }
                let start = x;
                text.clear();
                dx.clear();
                while x < line.len().min(t.cols) {
                    let c = line[x];
                    if c.fg != c0.fg || c.bg != c0.bg || c.fl != c0.fl || t.selected(abs, x) != s0 {
                        break;
                    }
                    let ch = char::from_u32(c.ch).unwrap_or(' ');
                    let mut u = [0u16; 2];
                    let enc = ch.encode_utf16(&mut u);
                    for (k, unit) in enc.iter().enumerate() {
                        text.push(*unit);
                        dx.push(if k == 0 { t.cw } else { 0 });
                    }
                    x += 1;
                }
                let cell_r = RECT {
                    left: pad + start as i32 * t.cw,
                    top: y,
                    right: pad + x as i32 * t.cw,
                    bottom: y + t.ch,
                };
                SetTextColor(mem, rgb(fg));
                SetBkColor(mem, rgb(bg));
                let _ = ExtTextOutW(
                    mem,
                    cell_r.left,
                    y,
                    ETO_OPAQUE | ETO_CLIPPED,
                    Some(&cell_r),
                    PCWSTR(text.as_ptr()),
                    text.len() as u32,
                    Some(dx.as_ptr()),
                );
                if c0.fl & F_UL != 0 {
                    let ul = RECT { left: cell_r.left, top: y + t.ch - 2, right: cell_r.right, bottom: y + t.ch - 1 };
                    let b = CreateSolidBrush(rgb(fg));
                    FillRect(mem, &ul, b);
                    let _ = DeleteObject(HGDIOBJ(b.0));
                }
            }
        }
        if t.view == 0 && t.cursor_on {
            let cx = pad + t.x as i32 * t.cw;
            let cy = pad + t.y as i32 * t.ch;
            let b = CreateSolidBrush(rgb(th.cursor));
            if t.focused {
                let bw = (t.cw / 6).max(2);
                let r = RECT { left: cx, top: cy, right: cx + bw, bottom: cy + t.ch };
                FillRect(mem, &r, b);
            } else {
                let r = RECT { left: cx, top: cy, right: cx + t.cw, bottom: cy + t.ch };
                FrameRect(mem, &r, b);
            }
            let _ = DeleteObject(HGDIOBJ(b.0));
        }
        let _ = BitBlt(dc, 0, 0, w, hgt, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, of);
        SelectObject(mem, ob);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
    }
}

// ---- input ----

fn key_down(vk: u16) -> bool {
    unsafe { GetKeyState(vk as i32) < 0 }
}

fn copy_sel(t: &mut Term) -> bool {
    let s = t.sel_text();
    t.sel_a = None;
    t.sel_b = None;
    if s.is_empty() {
        return false;
    }
    let mut w: Vec<u16> = s.encode_utf16().collect();
    w.push(0);
    unsafe {
        if OpenClipboard(Some(t.hwnd)).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        if let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2) {
            let p = GlobalLock(hg) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
                let _ = GlobalUnlock(hg);
                let _ = SetClipboardData(13, Some(HANDLE(hg.0)));
            }
        }
        let _ = CloseClipboard();
        let _ = InvalidateRect(Some(t.hwnd), None, false);
    }
    true
}

fn paste(t: &mut Term) {
    let mut text = String::new();
    unsafe {
        if OpenClipboard(Some(t.hwnd)).is_err() {
            return;
        }
        if let Ok(h) = GetClipboardData(13) {
            let hg = windows::Win32::Foundation::HGLOBAL(h.0);
            let p = GlobalLock(hg) as *const u16;
            if !p.is_null() {
                let mut n = 0usize;
                while *p.add(n) != 0 && n < 1 << 20 {
                    n += 1;
                }
                text = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
                let _ = GlobalUnlock(hg);
            }
        }
        let _ = CloseClipboard();
    }
    if text.is_empty() {
        return;
    }
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    t.view = 0;
    if t.paste_br {
        t.write(b"\x1b[200~");
        t.write(text.as_bytes());
        t.write(b"\x1b[201~");
    } else {
        t.write(text.as_bytes());
    }
}

fn key_seq(t: &Term, vk: u32) -> Option<String> {
    let shift = key_down(VK_SHIFT.0);
    let alt = key_down(VK_MENU.0);
    let ctrl = key_down(VK_CONTROL.0);
    let m = 1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8;
    let arrow = |c: char| {
        if m > 1 {
            format!("\x1b[1;{}{}", m, c)
        } else if t.app_cursor {
            format!("\x1bO{}", c)
        } else {
            format!("\x1b[{}", c)
        }
    };
    let tilde = |n: u32| if m > 1 { format!("\x1b[{};{}~", n, m) } else { format!("\x1b[{}~", n) };
    Some(match vk {
        0x26 => arrow('A'),
        0x28 => arrow('B'),
        0x27 => arrow('C'),
        0x25 => arrow('D'),
        0x24 => arrow('H'),
        0x23 => arrow('F'),
        0x2D => tilde(2),
        0x2E => tilde(3),
        0x21 => tilde(5),
        0x22 => tilde(6),
        0x70 => "\x1bOP".into(),
        0x71 => "\x1bOQ".into(),
        0x72 => "\x1bOR".into(),
        0x74 => tilde(15),
        0x75 => tilde(17),
        0x76 => tilde(18),
        0x77 => tilde(19),
        0x78 => tilde(20),
        0x79 => tilde(21),
        0x7A => tilde(23),
        0x7B => tilde(24),
        _ => return None,
    })
}

fn invalidate(h: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(h), None, false);
    }
}

unsafe extern "system" fn term_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Tangani WM_NCDESTROY sebelum meminjam RefCell agar Box::from_raw tidak membebaskan data saat borrow hidup.
    if m == WM_NCDESTROY {
        let p = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut std::cell::RefCell<Term>;
        SetWindowLongPtrW(h, GWLP_USERDATA, 0);
        if !p.is_null() {
            let b = Box::from_raw(p).into_inner();
            let _ = DeleteObject(HGDIOBJ(b.font.0));
        }
        return DefWindowProcW(h, m, wp, lp);
    }
    with_term(h, |t| match m {
        WM_TERM_DATA => {
            if lp.0 != 0 {
                let data = Box::from_raw(lp.0 as *mut Vec<u8>);
                t.feed(&data);
                invalidate(h);
            }
            LRESULT(0)
        }
        WM_TERM_EXIT => {
            let _ = PostMessageW(Some(t.frame), WM_FM_TERM_CLOSED, WPARAM(0), LPARAM(h.0 as isize));
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(h, &mut ps);
            paint(t, dc);
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_SIZE => {
            let w = (lp.0 & 0xFFFF) as i32;
            let hh = ((lp.0 >> 16) & 0xFFFF) as i32;
            let pad = t.pad();
            let cols = ((w - pad * 2) / t.cw).max(2) as usize;
            let rows = ((hh - pad * 2) / t.ch).max(1) as usize;
            t.resize(cols, rows);
            invalidate(h);
            LRESULT(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            t.focused = m == WM_SETFOCUS;
            invalidate(h);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wp.0 as u32;
            let ctrl = key_down(VK_CONTROL.0);
            let shift = key_down(VK_SHIFT.0);
            if ctrl && vk == 'C' as u32 && (shift || t.sel_range().is_some()) {
                copy_sel(t);
                t.skip_char = true;
                return LRESULT(0);
            }
            if ctrl && vk == 'V' as u32 {
                paste(t);
                t.skip_char = true;
                return LRESULT(0);
            }
            if shift && (vk == 0x21 || vk == 0x22) && t.alt.is_none() {
                let step = t.rows.saturating_sub(1).max(1);
                t.view = if vk == 0x21 { (t.view + step).min(t.sb.len()) } else { t.view.saturating_sub(step) };
                invalidate(h);
                return LRESULT(0);
            }
            if let Some(s) = key_seq(t, vk) {
                t.view = 0;
                t.sel_a = None;
                t.write(s.as_bytes());
                return LRESULT(0);
            }
            DefWindowProcW(h, m, wp, lp)
        }
        WM_CHAR | WM_SYSCHAR => {
            if t.skip_char {
                t.skip_char = false;
                return LRESULT(0);
            }
            let u = wp.0 as u16;
            if (0xD800..0xDC00).contains(&u) {
                t.hi_sur = u;
                return LRESULT(0);
            }
            let s = if (0xDC00..0xE000).contains(&u) && t.hi_sur != 0 {
                let r = String::from_utf16_lossy(&[t.hi_sur, u]);
                t.hi_sur = 0;
                r
            } else if u == 0x08 {
                "\x7f".to_string()
            } else {
                String::from_utf16_lossy(&[u])
            };
            t.view = 0;
            if t.sel_a.is_some() {
                t.sel_a = None;
                t.sel_b = None;
                invalidate(h);
            }
            if m == WM_SYSCHAR {
                t.write(b"\x1b");
            }
            t.write(s.as_bytes());
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            if t.alt.is_none() {
                let lines = (delta / 120 * 3).unsigned_abs() as usize;
                t.view = if delta > 0 { (t.view + lines).min(t.sb.len()) } else { t.view.saturating_sub(lines) };
                invalidate(h);
            } else {
                let seq = if delta > 0 { "\x1b[A\x1b[A\x1b[A" } else { "\x1b[B\x1b[B\x1b[B" };
                t.write(seq.as_bytes());
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let _ = SetFocus(Some(h));
            let (x, y) = ((lp.0 & 0xFFFF) as u16 as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32);
            let c = t.cell_at(x, y);
            t.sel_a = Some(c);
            t.sel_b = Some(c);
            t.selecting = true;
            SetCapture(h);
            invalidate(h);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if t.selecting {
                let (x, y) = ((lp.0 & 0xFFFF) as u16 as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32);
                t.sel_b = Some(t.cell_at(x, y));
                invalidate(h);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if t.selecting {
                t.selecting = false;
                let _ = ReleaseCapture();
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            // Seperti Windows Terminal: ada pilihan = salin, tanpa pilihan = tempel.
            if !copy_sel(t) {
                paste(t);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if t.hpc.0 != 0 {
                ClosePseudoConsole(t.hpc);
                t.hpc = HPCON(0);
            }
            if !t.input.is_invalid() {
                let _ = CloseHandle(t.input);
                t.input = HANDLE::default();
            }
            if !t.proc.is_invalid() {
                let _ = CloseHandle(t.proc);
                t.proc = HANDLE::default();
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    })
    .unwrap_or_else(|| DefWindowProcW(h, m, wp, lp))
}

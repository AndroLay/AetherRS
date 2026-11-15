//! Riwayat clipboard (Win+V): pantau perubahan clipboard dan simpan teks terakhir di memori.
//! Riwayat hanya hidup selama shell berjalan dan tidak pernah ditulis ke disk.
use super::prelude::*;
use windows::Win32::Foundation::{HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RemoveClipboardFormatListener,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Sama dengan WM_CLIPBOARDUPDATE (797) dari windows::Win32::UI::WindowsAndMessaging.
pub const WM_CLIPBOARDUPDATE: u32 = 0x031D;
/// Jumlah maksimal entri riwayat.
pub const CH_MAX: usize = 25;
/// Batas panjang satu entri (karakter UTF-16) agar memori tidak membengkak.
const CH_MAX_CHARS: usize = 4096;

static CH_HIST: SyncCell<RefCell<Vec<String>>> = SyncCell::new(RefCell::new(Vec::new()));

/// Pasang pemantau clipboard ke jendela pesan shell.
pub fn ch_start(hwnd: HWND) {
    // SAFETY: hwnd adalah jendela pesan milik shell; pemantau dilepas di ch_stop.
    let _ = unsafe { AddClipboardFormatListener(hwnd) };
}

/// Lepas pemantau dan kosongkan riwayat.
pub fn ch_stop(hwnd: HWND) {
    // SAFETY: pasangan AddClipboardFormatListener dari ch_start.
    let _ = unsafe { RemoveClipboardFormatListener(hwnd) };
    CH_HIST.borrow_mut().clear();
}

/// Salinan riwayat (terbaru di depan) untuk ditampilkan.
pub fn ch_items() -> Vec<String> {
    CH_HIST.borrow().clone()
}

/// Dipanggil saat WM_CLIPBOARDUPDATE: simpan teks baru di depan riwayat.
pub fn ch_on_update() {
    let Some(text) = ch_read_text() else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    let mut h = CH_HIST.borrow_mut();
    h.retain(|s| *s != text);
    h.insert(0, text);
    h.truncate(CH_MAX);
}

/// Hapus satu entri riwayat berdasarkan teksnya (bukan indeks, karena riwayat bisa bergeser saat panel terbuka).
pub fn ch_remove(text: &str) {
    CH_HIST.borrow_mut().retain(|s| s != text);
}

/// Baca CF_UNICODETEXT. None bila tidak ada teks atau clipboard sedang dipakai proses lain.
fn ch_read_text() -> Option<String> {
    // SAFETY: clipboard dibuka tanpa jendela pemilik dan selalu ditutup; memori dikunci lalu dibuka lagi
    // sebelum CloseClipboard, dan panjang dibatasi GlobalSize.
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
            return None;
        }
        if OpenClipboard(None).is_err() {
            return None;
        }
        let mut out = None;
        if let Ok(hd) = GetClipboardData(CF_UNICODETEXT.0 as u32) {
            let g = HGLOBAL(hd.0);
            let p = GlobalLock(g) as *const u16;
            if !p.is_null() {
                let cap = (GlobalSize(g) / 2).min(CH_MAX_CHARS);
                let len = (0..cap).take_while(|&i| *p.add(i) != 0).count();
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)));
                let _ = GlobalUnlock(g);
            }
        }
        let _ = CloseClipboard();
        out
    }
}

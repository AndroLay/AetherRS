//! Sumber: w12calendar.inc:1-112 (CalFirstDow, CalToday, CalStep, CalMeasure, CalDraw, CalActivate). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime.
//! Logika kisi dari crate::cal_logic (Tahap A). Pemanggil lintas owner: panel.rs (PanelMeasure, PanelContent, PanelActivate).

use std::cell::Cell;

use windows::core::PCWSTR;
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::Globalization::{
    GetDateFormatEx, GetLocaleInfoEx, ENUM_DATE_FORMATS_FLAGS, LOCALE_IFIRSTDAYOFWEEK,
};
use windows::Win32::System::SystemInformation::GetLocalTime;

use super::core_c1::{fill_rr, glyph, s, sf, text, w12_invalidate, PALETTE};
use super::prelude::*; // bt_card dan wf_text_c dari flyout.rs ikut lewat prelude
use super::panel::pan_hover;
use super::state::{
    PAN, PAN_ID_CAL_NEXT, PAN_ID_CAL_PREV, PAN_ID_CAL_TODAY, TF_B, TF_BS, TF_GLYPH, TF_S, TF_XSC,
};
use crate::cal_logic::{w12_month_grid, W12Day};
use crate::sync::SyncCell;
use crate::wide::{from_wide, pcw, to_wide_z};

/// FLY_CAL_W / FLY_CAL_H (sumber baris 5). Hanya dipakai berkas ini.
const FLY_CAL_W: i32 = 340;
const FLY_CAL_H: i32 = 420;

/// Bulan yang sedang tampil (g_calY, g_calM). Hanya dipakai berkas ini.
static CAL_Y: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
static CAL_M: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));

/// LOCALE_IFIRSTDAYOFWEEK: 0 = Senin ... 6 = Minggu (sama dengan w12_month_grid).
fn cal_first_dow() -> i32 {
    let mut b = [0u16; 4];
    let n = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_IFIRSTDAYOFWEEK, Some(&mut b[..])) };
    if n > 0 && b[0] >= b'0' as u16 && b[0] <= b'6' as u16 {
        return (b[0] - b'0' as u16) as i32;
    }
    0
}

fn cal_today() {
    let lt = unsafe { GetLocalTime() };
    CAL_Y.set(lt.wYear as i32);
    CAL_M.set(lt.wMonth as i32);
}

fn cal_step(delta: i32) {
    let mut y = CAL_Y.get();
    let mut m = CAL_M.get() + delta;
    while m < 1 {
        m += 12;
        y -= 1;
    }
    while m > 12 {
        m -= 12;
        y += 1;
    }
    CAL_Y.set(y);
    CAL_M.set(m);
}

/// Format tanggal dengan locale pengguna (GetDateFormatEx), hasil dipotong di NUL pertama.
fn date_text(st: &SYSTEMTIME, fmt: &str, cap: usize) -> String {
    let fmt_w = to_wide_z(fmt);
    let mut out = vec![0u16; cap];
    unsafe {
        GetDateFormatEx(
            PCWSTR::null(),
            ENUM_DATE_FORMATS_FLAGS(0),
            Some(st as *const SYSTEMTIME),
            pcw(&fmt_w),
            Some(&mut out[..]),
            PCWSTR::null(),
        );
    }
    let end = out.iter().position(|&c| c == 0).unwrap_or(out.len());
    from_wide(&out[..end])
}

/// Ukuran flyout Kalender (mode shell). Dipanggil dari panel.rs (PanelMeasure, w12panel.inc:126) untuk PN_CAL.
/// CalMeasure juga menyetel bulan ke bulan ini (CalToday).
pub fn cal_measure(cw: &mut i32, ch: &mut i32) {
    *cw = s(FLY_CAL_W as f32);
    *ch = s(FLY_CAL_H as f32);
    cal_today();
}

/// Menggambar flyout Kalender dan notifikasi (PN_CAL). Dipanggil dari panel.rs (PanelContent, w12panel.inc:114).
pub fn cal_draw(w: f32, h: f32) {
    let _ = h;
    let lt = unsafe { GetLocalTime() };
    // Salin warna palet dulu agar borrow PALETTE tidak dipegang saat menggambar.
    let (c_text, c_sub, c_sub2, c_glyph, c_line, c_chip, c_accent, c_accent_ink) = {
        let p = PALETTE.borrow();
        (p.text, p.sub, p.sub2, p.glyph, p.line, p.chip, p.accent, p.accent_ink)
    };
    let lx = sf(18.0);
    let rx = w - sf(18.0);

    // notifikasi: notifikasi aplikasi lain tidak bisa dibaca tanpa identitas paket, jadi hanya status ini
    text("Notifikasi", TF_BS.borrow().as_ref(), lx, sf(14.0), rx - sf(40.0), sf(34.0), c_text, 1.0);
    bt_card(lx, sf(40.0), rx, sf(84.0), sf(12.0), &c_chip, &c_line, 1.0);
    wf_text_c(
        "Tidak ada notifikasi baru",
        TF_S.borrow().as_ref(),
        (lx + rx) * 0.5,
        sf(40.0),
        sf(84.0),
        &c_sub,
    );

    // judul: tanggal hari ini memakai format locale pengguna, mis. "Kamis, 12 November"
    let title = date_text(&lt, "dddd, d MMMM", 64);
    text(&title, TF_B.borrow().as_ref(), lx, sf(104.0), rx, sf(126.0), c_text, 1.0);

    // bulan dan tahun yang tampil, lalu tombol ‹ ›
    let ms = SYSTEMTIME {
        wYear: CAL_Y.get() as u16,
        wMonth: CAL_M.get() as u16,
        wDay: 1,
        ..Default::default()
    };
    let my = date_text(&ms, "MMMM yyyy", 48);
    text(&my, TF_BS.borrow().as_ref(), lx, sf(136.0), rx - sf(70.0), sf(164.0), c_text, 1.0);
    let nt = sf(136.0);
    let nb = sf(164.0);
    pan_hover(rx - sf(64.0), nt, rx - sf(36.0), nb, sf(14.0), PAN_ID_CAL_PREV);
    glyph(0xE76B, TF_GLYPH.borrow().as_ref(), rx - sf(50.0), (nt + nb) * 0.5, sf(10.0), c_glyph, 1.0);
    pan_hover(rx - sf(28.0), nt, rx, nb, sf(14.0), PAN_ID_CAL_NEXT);
    glyph(0xE76C, TF_GLYPH.borrow().as_ref(), rx - sf(14.0), (nt + nb) * 0.5, sf(10.0), c_glyph, 1.0);

    // header hari: urutan kolom mengikuti hari pertama minggu. 1..7 Nov 2026 = Minggu..Sabtu.
    let first_dow = cal_first_dow();
    let start_sun = (first_dow + 1) % 7; // basis Minggu = 0
    let cw7 = (rx - lx) / 7.0;
    for c in 0..7 {
        let dn = SYSTEMTIME {
            wYear: 2026,
            wMonth: 11,
            wDay: (1 + (start_sun + c) % 7) as u16,
            ..Default::default()
        };
        let dl = date_text(&dn, "ddd", 16);
        wf_text_c(
            &dl,
            TF_XSC.borrow().as_ref(),
            lx + c as f32 * cw7 + cw7 * 0.5,
            sf(170.0),
            sf(186.0),
            &c_sub,
        );
    }

    // kisi 6x7: hari ini = lingkaran aksen; di luar bulan = C.sub2
    let mut g = [W12Day::default(); 42];
    w12_month_grid(CAL_Y.get(), CAL_M.get(), first_dow, &mut g);
    let top = sf(192.0);
    let cell = sf(30.0);
    for (i, day) in g.iter().enumerate() {
        let r = (i / 7) as f32;
        let c = (i % 7) as f32;
        let cx = lx + c * cw7 + cw7 * 0.5;
        let cy = top + r * cell + cell * 0.5;
        let today = day.in_month
            && day.y == lt.wYear as i32
            && day.m == lt.wMonth as i32
            && day.d == lt.wDay as i32;
        if today {
            fill_rr(cx - sf(14.0), cy - sf(14.0), cx + sf(14.0), cy + sf(14.0), sf(14.0), c_accent, 1.0);
        }
        let num = format!("{}", day.d);
        let ink = if today {
            c_accent_ink
        } else if day.in_month {
            c_text
        } else {
            c_sub2
        };
        wf_text_c(&num, TF_S.borrow().as_ref(), cx, cy - sf(11.0), cy + sf(11.0), &ink);
    }

    // tombol "Hari ini": kembali ke bulan sekarang
    let by0 = sf(386.0);
    let by1 = sf(414.0);
    let bl = w * 0.5 - sf(56.0);
    let br = w * 0.5 + sf(56.0);
    bt_card(bl, by0, br, by1, sf(14.0), &c_chip, &c_line, 1.0);
    pan_hover(bl, by0, br, by1, sf(14.0), PAN_ID_CAL_TODAY);
    wf_text_c("Hari ini", TF_S.borrow().as_ref(), w * 0.5, by0, by1, &c_text);
}

/// Menangani klik tombol panel Kalender (PAN_ID_CAL_*). Dipanggil dari panel.rs (PanelActivate, w12panel.inc:137).
pub fn cal_activate(id: i32) {
    if id == PAN_ID_CAL_PREV {
        cal_step(-1);
    } else if id == PAN_ID_CAL_NEXT {
        cal_step(1);
    } else if id == PAN_ID_CAL_TODAY {
        cal_today();
    } else {
        return;
    }
    let pan = PAN.get();
    if !pan.0.is_null() {
        w12_invalidate(pan, None, false);
    }
}

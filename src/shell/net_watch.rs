//! Peringatan jaringan lewat notch (2026-11-15): Wi-Fi/Ethernet tersambung atau terputus, jaringan hilang,
//! dan "tersambung tetapi tidak ada internet" / "internet kembali".
//!
//! Sumber status:
//! - jenis koneksi: `net_update` (core_c1, NotifyIpInterfaceChange -> WM_NETCHG);
//! - internet: INetworkListManager::GetConnectivity (NCSI, sama dengan ikon jaringan Windows), dicek 5 dtk
//!   setelah perubahan jaringan lalu tiap 60 dtk (15 dtk selama tidak ada internet).

use std::cell::Cell;

use windows::Win32::Networking::NetworkListManager::{
    INetworkListManager, NetworkListManager, NLM_CONNECTIVITY_IPV4_INTERNET, NLM_CONNECTIVITY_IPV6_INTERNET,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

use super::core_c1::set_timer;
use super::flyout::wifi_current_ssid;
use super::notch::notch_alert;
use super::state::{NET, NET_BOTH, NET_ETH, NET_NONE, NET_WIFI, T_NETCHK};
use crate::log::log;
use crate::sync::SyncCell;

const G_WIFI: u16 = 0xE701;
const G_ETH: u16 = 0xE839;
const G_GLOBE: u16 = 0xE774;
const G_WARN: u16 = 0xE7BA;

/// -1 belum diketahui, 0 tanpa internet, 1 ada internet.
static INET: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));

fn has_eth(k: i32) -> bool {
    k == NET_ETH || k == NET_BOTH
}

fn has_wifi(k: i32) -> bool {
    k == NET_WIFI || k == NET_BOTH
}

/// Mulai pengecekan internet berkala (dipanggil sekali saat start; pengecekan pertama tidak diumumkan).
pub fn start() {
    set_timer(T_NETCHK, 4000);
}

/// Dipanggil WM_NETCHG setelah `net_update()` mengembalikan perubahan. `was` = NET sebelum pembaruan.
pub fn on_change(was: i32) {
    let now = NET.get();
    if now == was {
        return;
    }
    let (we, ww, ne, nw) = (has_eth(was), has_wifi(was), has_eth(now), has_wifi(now));
    if now == NET_NONE {
        notch_alert("Jaringan terputus", G_WARN, true, 3500);
        // -1: saat tersambung lagi, pesan "Wi-Fi tersambung" sudah cukup; hanya "tidak ada internet" yang diumumkan.
        INET.set(-1);
    } else if nw && !ww {
        let name = wifi_current_ssid().unwrap_or_else(|| "Wi-Fi".to_string());
        notch_alert(&format!("Wi-Fi tersambung \u{00B7} {}", name), G_WIFI, false, 3000);
    } else if ne && !we {
        notch_alert("Ethernet tersambung", G_ETH, false, 3000);
    } else if !nw && ww {
        notch_alert("Wi-Fi terputus \u{00B7} memakai Ethernet", G_WIFI, false, 3000);
    } else if !ne && we {
        notch_alert("Ethernet dilepas \u{00B7} memakai Wi-Fi", G_ETH, false, 3000);
    }
    log(&format!("jaringan: {} -> {}", was, now));
    // NCSI butuh beberapa detik setelah antarmuka berubah.
    set_timer(T_NETCHK, 5000);
}

fn connectivity() -> Option<bool> {
    // SAFETY: COM sudah diinisialisasi di thread UI; objek dilepas saat keluar cakupan.
    let nlm: INetworkListManager = unsafe { CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL).ok()? };
    let c = unsafe { nlm.GetConnectivity().ok()? };
    Some((c.0 & (NLM_CONNECTIVITY_IPV4_INTERNET.0 | NLM_CONNECTIVITY_IPV6_INTERNET.0)) != 0)
}

/// Timer T_NETCHK: baca status internet, umumkan perubahan, lalu jadwalkan pengecekan berikutnya.
pub fn on_timer() {
    let prev = INET.get();
    let ok = connectivity();
    let next_ms = match ok {
        Some(true) => {
            if prev == 0 && NET.get() != NET_NONE {
                notch_alert("Internet tersambung", G_GLOBE, false, 2500);
                log("jaringan: internet kembali");
            }
            INET.set(1);
            60_000
        }
        Some(false) => {
            if prev != 0 && prev != -1 && NET.get() != NET_NONE {
                notch_alert("Tersambung, tetapi tidak ada internet", G_WARN, true, 4500);
                log("jaringan: tersambung tanpa internet");
            } else if prev == -1 && NET.get() != NET_NONE {
                // saat start: hanya beri tahu bila jaringan ada tetapi internet tidak
                notch_alert("Tidak ada akses internet", G_WARN, true, 4000);
            }
            INET.set(0);
            15_000
        }
        None => 60_000,
    };
    set_timer(T_NETCHK, next_ms);
}

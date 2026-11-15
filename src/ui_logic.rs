//! Port `w12ui_logic.h` (baris 1–101): helper logika UI murni (warna, kunci Win, pil, baterai, teks USB). Status: lengkap.
//! Tes di `tests/ui_logic.rs`. Fungsi C `W12*` menjadi `w12_*`; buffer `wchar_t` keluaran menjadi `&mut [u16]`.

/// Warna 8-bit per kanal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Warna aksen Windows dari registry (DWORD dalam urutan bayte AABBGGRR).
pub fn w12_decode_windows_accent(packed: u32) -> W12Rgb8 {
    W12Rgb8 {
        r: (packed & 0xFF) as u8,
        g: ((packed >> 8) & 0xFF) as u8,
        b: ((packed >> 16) & 0xFF) as u8,
    }
}

/// Campur `surface` dengan `wallpaper` sebesar `amount`/255 (pembulatan ke terdekat).
pub fn w12_tint_surface(surface: W12Rgb8, wallpaper: W12Rgb8, amount: u8) -> W12Rgb8 {
    let amt = u32::from(amount);
    let remain = 255u32 - amt;
    let mix = |s: u8, w: u8| ((u32::from(s) * remain + u32::from(w) * amt + 127) / 255) as u8;
    W12Rgb8 {
        r: mix(surface.r, wallpaper.r),
        g: mix(surface.g, wallpaper.g),
        b: mix(surface.b, wallpaper.b),
    }
}

/// Rata-rata tiga kanal dari jumlah dan `count` sampel (dibulatkan), dijepit ke 255.
/// `count` = 0 menghasilkan hitam.
pub fn w12_average_rgb(red_sum: u64, green_sum: u64, blue_sum: u64, count: u32) -> W12Rgb8 {
    if count == 0 {
        return W12Rgb8 { r: 0, g: 0, b: 0 };
    }
    let cnt = u64::from(count);
    let half = cnt / 2;
    let avg = |sum: u64| (sum.wrapping_add(half) / cnt).min(255) as u8;
    W12Rgb8 {
        r: avg(red_sum),
        g: avg(green_sum),
        b: avg(blue_sum),
    }
}

/// Tombol Win tunggal milik Win12 baik di mode jendela maupun layar penuh (`fullscreen` tidak dipakai).
pub fn w12_can_capture_win_key(
    solo: bool,
    enabled: bool,
    fullscreen: bool,
    quitting: bool,
    main_window_ready: bool,
) -> bool {
    let _ = fullscreen;
    solo && enabled && !quitting && main_window_ready
}

pub const W12_STARTKEY_NONE: i32 = 0;
pub const W12_STARTKEY_TOGGLE: i32 = 1;
pub const W12_STARTKEY_SEARCH: i32 = 2;

/// Aksi pintasan Start: `W12_STARTKEY_TOGGLE`, `W12_STARTKEY_SEARCH`, atau `W12_STARTKEY_NONE`.
pub fn w12_start_shortcut_action(
    solo_win: bool,
    win_search: bool,
    ctrl_escape: bool,
    enabled: bool,
    quitting: bool,
    main_window_ready: bool,
) -> i32 {
    if !enabled || quitting || !main_window_ready {
        return W12_STARTKEY_NONE;
    }
    if ctrl_escape {
        return W12_STARTKEY_TOGGLE;
    }
    if win_search {
        return W12_STARTKEY_SEARCH;
    }
    if solo_win {
        W12_STARTKEY_TOGGLE
    } else {
        W12_STARTKEY_NONE
    }
}

pub fn w12_can_trim_native_start_working_set(
    win_key_enabled: bool,
    hook_ready: bool,
    quitting: bool,
    host_found: bool,
) -> bool {
    win_key_enabled && hook_ready && !quitting && host_found
}

pub fn w12_should_show_pill(wants: bool, fullscreen: bool, fullscreen_peek: bool, taskbar_pill: bool) -> bool {
    wants && (!fullscreen || (fullscreen_peek && taskbar_pill))
}

/// Ambang peringatan baterai lemah, menaik: yang paling mendesak lebih dulu.
const BATTERY_LEVELS: [i32; 3] = [5, 10, 20];

/// Peringatan baterai lemah (Fase 7 Task 3). Mengembalikan ambang 5/10/20 yang baru terlewati saat tidak mengisi daya
/// dan belum diperingatkan, selain itu 0. `*armed` = ambang terendah yang sudah diperingatkan (awal 100). Mengisi daya
/// atau naik sampai `*armed + 5` mengatur ulang ke 100 lebih dulu. `pct` < 0 (baterai tidak terbaca) atau `armed` = `None`
/// tidak mengubah apa pun.
pub fn w12_battery_alert(pct: i32, charging: bool, armed: Option<&mut i32>) -> i32 {
    let armed = match armed {
        Some(a) if pct >= 0 => a,
        _ => return 0,
    };
    if charging || pct >= *armed + 5 {
        *armed = 100;
    }
    for t in BATTERY_LEVELS {
        if !charging && pct <= t && t < *armed {
            *armed = t;
            return t;
        }
    }
    0
}

/// Teks gagal mengeluarkan perangkat USB (Fase 7 Task 2). `vetoed` = CR_REMOVE_VETOED; `name` = pengambil veto
/// (boleh `None` atau kosong). Menulis teks ber-NUL ke `out`; bila tidak muat, dipotong dan mengembalikan -1
/// (seperti `swprintf` di C). Bila berhasil mengembalikan jumlah unit UTF-16 tanpa NUL.
pub fn w12_usb_eject_fail_text(vetoed: bool, cr: u32, name: Option<&str>, out: &mut [u16]) -> i32 {
    let text = if vetoed {
        match name.filter(|n| !n.is_empty()) {
            Some(n) => format!("Perangkat sedang dipakai oleh {n}"),
            None => "Perangkat sedang dipakai".to_string(),
        }
    } else {
        format!("Gagal mengeluarkan (CM {cr})")
    };
    let w: Vec<u16> = text.encode_utf16().collect();
    if out.is_empty() {
        return -1;
    }
    let fit = w.len().min(out.len() - 1);
    out[..fit].copy_from_slice(&w[..fit]);
    out[fit] = 0;
    if fit < w.len() {
        -1
    } else {
        w.len() as i32
    }
}

pub const W12_CTX_NONE: i32 = 0;
pub const W12_CTX_APP: i32 = 1;
pub const W12_CTX_SEARCH_RESULT: i32 = 2;

/// Konteks hasil pencarian Start: `kind` = `W12_CTX_*`, `index` = indeks di daftar (-1 bila tidak ada).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct W12StartContextRef {
    pub kind: i32,
    pub index: i32,
}

/// Memetakan indeks baris yang di-klik ke daftar aplikasi (disematkan lalu terbaru) atau hasil pencarian.
pub fn w12_start_context_from_hit(
    hit: i32,
    pin_base: i32,
    recent_base: i32,
    result_base: i32,
    app_count: i32,
    result_count: i32,
) -> W12StartContextRef {
    if app_count > 0 && hit >= pin_base && hit - pin_base < app_count {
        return W12StartContextRef { kind: W12_CTX_APP, index: hit - pin_base };
    }
    if app_count > 0 && hit >= recent_base && hit - recent_base < app_count {
        return W12StartContextRef { kind: W12_CTX_APP, index: hit - recent_base };
    }
    if result_count > 0 && hit >= result_base && hit - result_base < result_count {
        return W12StartContextRef { kind: W12_CTX_SEARCH_RESULT, index: hit - result_base };
    }
    W12StartContextRef { kind: W12_CTX_NONE, index: -1 }
}

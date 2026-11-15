//! Inti shell Win12 (C1): INI, tema, palet, aksen, DPI, timer, pembaruan jendela berlapis, helper gambar D2D/DWrite, ikon, dan model aplikasi.
//! Sumber: win12shell.cpp 1–1329. Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime.

use super::prelude::*;

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::io::Write;

use windows::core::{IUnknown, Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, LPARAM, PROPERTYKEY, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    ID2D1Bitmap, ID2D1Brush, ID2D1DCRenderTarget, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
    D2D1_DRAW_TEXT_OPTIONS, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_DRAW_TEXT_OPTIONS_NONE,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    IDWriteTextFormat, DWRITE_MEASURING_MODE_NATURAL, DWRITE_TEXT_METRICS,
};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, InvalidateRect, ReleaseDC, SelectObject,
    UpdateWindow, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::Graphics::Imaging::{
    IWICBitmapSource, IWICPalette, GUID_WICPixelFormat32bppPBGRA, WICBitmapInterpolationModeFant,
};
use windows::Win32::Storage::FileSystem::{
    GetFileAttributesW, GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    INVALID_FILE_ATTRIBUTES,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoTaskMemFree, IBindCtx, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ,
};
use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;
use windows::Win32::UI::Shell::{
    IShellItem, IShellItem2, IShellItemImageFactory, IShellLinkW, SHCreateItemFromParsingName,
    ShellLink, SIGDN_NORMALDISPLAY, SIIGBF_ICONONLY,
};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetClassLongPtrW, GetClassNameW, GetPropW, GetWindow, GetWindowLongW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, KillTimer,
    PostMessageW, SendMessageTimeoutW, SendMessageW, SetPropW, SetTimer, GCLP_HICON, GW_OWNER,
    GWL_EXSTYLE, HICON, ICON_BIG, ICON_SMALL2, SMTO_ABORTIFHUNG, WM_GETICON, WS_EX_APPWINDOW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

use super::flyout::bat_hist_note;
use super::state::{
    BR, DESK_LAYER, DPI, DW, FILES_OWN, GLASS, GLASS_OK, ICON_PX, ICON_PX_SM, INI, LIGHT,
    MEMDC, MICA_FULLSCREEN_SNAPSHOT, MODE, MODE_DOCK, MODE_SEGMENTED, MSG, NOTCH_HUD, NOTCH_ON,
    ORIG_AUTO_HIDE, RT, RT_GEN, SMALL, THEME_PREF, T_TRIM, WIC, WIDGETS, WIN_KEY, WM_W12_RENDER,
    APPS_TXT, MAXAPPS, MAXWC, NOTE_APP, PK_AUMID, PK_TARGET,
};
use crate::reg;
use windows::core::{implement, BOOL, HRESULT};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDIO_VOLUME_NOTIFICATION_DATA,
};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use super::state::{DEV_ENUM, EP_VOL, MUTE, VOL, WM_VOLCHG};
use super::state::{
    ETH_DESC, ETH_IP, ETH_NAME, ETH_SPEED, NET, NET_BOTH, NET_ETH, NET_ETH_OK, NET_NONE,
    NET_NOTIFY, NET_WIFI, SSID, WM_NETCHG,
};
use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
// IpHelper: hanya konstanta yang tidak butuh fitur WinSock/Ndis. Fungsi dan struct lainnya ada di `iph` (deviasi FFI).
use windows::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
    GAA_FLAG_SKIP_MULTICAST, GET_ADAPTERS_ADDRESSES_FLAGS, IF_TYPE_ETHERNET_CSMACD,
    IF_TYPE_IEEE80211,
};
use iph::{
    AF_INET, AF_UNSPEC, GetAdaptersAddresses, IfOperStatusUp, IP_ADAPTER_ADDRESSES_LH,
    NotifyIpInterfaceChange, SOCKADDR_IN,
};
use crate::sync::SyncCell;
use crate::ui_logic::{w12_decode_windows_accent, w12_tint_surface, W12Rgb8};
use crate::wide::{from_pcwstr, from_wide, pcw, to_wide, to_wide_z};
// Baterai, jam, dan media (C 1066–1329).
use crate::ui_logic::w12_battery_alert;
use std::sync::atomic::{AtomicI32, Ordering};
use windows::core::{w, HSTRING};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, MediaPropertiesChangedEventArgs,
    PlaybackInfoChangedEventArgs,
};
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::Globalization::{GetDateFormatEx, DATE_LONGDATE, ENUM_DATE_FORMATS_FLAGS};
use windows::Win32::Graphics::Imaging::{
    IWICBitmap, IWICBitmapClipper, IWICBitmapDecoder, IWICBitmapFrameDecode, IWICBitmapScaler,
    IWICFormatConverter, WICBitmapCacheOnLoad, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand, WICRect,
};
use windows::Win32::System::Com::IStream;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::WinRT::CreateStreamOverRandomAccessStream;
use super::state::{
    BAT, CHARGING, DATE, DATE_SHORT, HAS_BAT, MEDIA_EV, MEDIA_WANT, TIME, TIME_TZ, TZ, T_MEDIA,
    T_MEDIA_FAST, WM_W12_MEDIAEV,
};

// ---------------------------------------------------------------- FFI INI (deviasi, lihat files/bar.rs)

// Fitur `Win32_System_WindowsProgramming` sudah aktif di Cargo.toml; fungsi INI tetap dideklarasikan langsung (FFI).
// Signature sama dengan windows 0.61.3 (kernel32).
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetPrivateProfileIntW(lp_app_name: *const u16, lp_key_name: *const u16, n_default: i32, lp_file_name: *const u16) -> u32;
    fn GetPrivateProfileStringW(lp_app_name: *const u16, lp_key_name: *const u16, lp_default: *const u16, lp_returned_string: *mut u16, n_size: u32, lp_file_name: *const u16) -> u32;
    fn WritePrivateProfileStringW(lp_app_name: *const u16, lp_key_name: *const u16, lp_string: *const u16, lp_file_name: *const u16) -> i32;
}

/// `GetPrivateProfileIntW(section, key, def, g_ini)` (`win12shell.cpp:141`).
pub fn ini_int(section: &str, key: &str, default: i32) -> i32 {
    let file = to_wide_z(&INI.borrow());
    let sec = to_wide_z(section);
    let k = to_wide_z(key);
    unsafe { GetPrivateProfileIntW(sec.as_ptr(), k.as_ptr(), default, file.as_ptr()) as i32 }
}

/// `GetPrivateProfileStringW(section, key, def, buf, cap, g_ini)`; hasil dipotong sepanjang nilai yang dikembalikan API.
pub fn ini_string(section: &str, key: &str, default: &str, cap: usize) -> String {
    let file = to_wide_z(&INI.borrow());
    let sec = to_wide_z(section);
    let k = to_wide_z(key);
    let d = to_wide_z(default);
    let mut buf = vec![0u16; cap];
    let n = unsafe {
        GetPrivateProfileStringW(
            sec.as_ptr(),
            k.as_ptr(),
            d.as_ptr(),
            buf.as_mut_ptr(),
            cap as u32,
            file.as_ptr(),
        )
    };
    from_wide(&buf[..(n as usize).min(cap)])
}

/// `WritePrivateProfileStringW(section, key, value, g_ini)`.
pub fn ini_write(section: &str, key: &str, value: &str) -> bool {
    let file = to_wide_z(&INI.borrow());
    let sec = to_wide_z(section);
    let k = to_wide_z(key);
    let v = to_wide_z(value);
    unsafe { WritePrivateProfileStringW(sec.as_ptr(), k.as_ptr(), v.as_ptr(), file.as_ptr()) != 0 }
}

/// `WritePrivateProfileStringW(section, key, NULL, g_ini)`: menghapus kunci (`RestoreNativeTaskbar`, win12shell.cpp:2943).
pub fn ini_delete(section: &str, key: &str) -> bool {
    let file = to_wide_z(&INI.borrow());
    let sec = to_wide_z(section);
    let k = to_wide_z(key);
    unsafe { WritePrivateProfileStringW(sec.as_ptr(), k.as_ptr(), std::ptr::null(), file.as_ptr()) != 0 }
}

// ---------------------------------------------------------------- global milik C1 (tidak ada di state.rs)

/// `g_search` (`win12shell.cpp:119`).
pub static SEARCH: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
/// `g_trayClock` (`win12shell.cpp:119`).
pub static TRAY_CLOCK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(true));
/// `g_wallpaperTint` (`win12shell.cpp:180`).
pub static WALLPAPER_TINT: SyncCell<Cell<W12Rgb8>> =
    SyncCell::new(Cell::new(W12Rgb8 { r: 0, g: 120, b: 212 }));
/// `g_themeVeilBase` dan `g_themePanelBase` (`win12shell.cpp:181`).
pub static THEME_VEIL_BASE: SyncCell<Cell<D2D1_COLOR_F>> = SyncCell::new(Cell::new(ZERO_COLOR));
pub static THEME_PANEL_BASE: SyncCell<Cell<D2D1_COLOR_F>> = SyncCell::new(Cell::new(ZERO_COLOR));

const ZERO_COLOR: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

/// Palet desain (`struct Palette`, `win12shell.cpp:175`; global `C` di baris 179): 22 warna.
/// Jangan memegang borrow `PALETTE` melewati pemanggilan Win32 atau fungsi lain yang juga menyentuhnya.
#[derive(Clone, Copy)]
pub struct Palette {
    pub text: D2D1_COLOR_F,
    pub sub: D2D1_COLOR_F,
    pub sub2: D2D1_COLOR_F,
    pub glyph: D2D1_COLOR_F,
    pub veil: D2D1_COLOR_F,
    pub panel: D2D1_COLOR_F,
    pub hover: D2D1_COLOR_F,
    pub press: D2D1_COLOR_F,
    pub active: D2D1_COLOR_F,
    pub sep: D2D1_COLOR_F,
    pub line: D2D1_COLOR_F,
    pub dot: D2D1_COLOR_F,
    pub accent: D2D1_COLOR_F,
    pub flash: D2D1_COLOR_F,
    pub chip: D2D1_COLOR_F,
    pub chip_hover: D2D1_COLOR_F,
    pub track: D2D1_COLOR_F,
    pub accent_ink: D2D1_COLOR_F,
    pub accent_soft: D2D1_COLOR_F,
    pub accent_line: D2D1_COLOR_F,
    pub glow: D2D1_COLOR_F,
    pub shadow: D2D1_COLOR_F,
}

impl Palette {
    pub const ZERO: Palette = Palette {
        text: ZERO_COLOR,
        sub: ZERO_COLOR,
        sub2: ZERO_COLOR,
        glyph: ZERO_COLOR,
        veil: ZERO_COLOR,
        panel: ZERO_COLOR,
        hover: ZERO_COLOR,
        press: ZERO_COLOR,
        active: ZERO_COLOR,
        sep: ZERO_COLOR,
        line: ZERO_COLOR,
        dot: ZERO_COLOR,
        accent: ZERO_COLOR,
        flash: ZERO_COLOR,
        chip: ZERO_COLOR,
        chip_hover: ZERO_COLOR,
        track: ZERO_COLOR,
        accent_ink: ZERO_COLOR,
        accent_soft: ZERO_COLOR,
        accent_line: ZERO_COLOR,
        glow: ZERO_COLOR,
        shadow: ZERO_COLOR,
    };
}

/// Global `C` (`win12shell.cpp:179`).
pub static PALETTE: SyncCell<RefCell<Palette>> = SyncCell::new(RefCell::new(Palette::ZERO));

// ---------------------------------------------------------------- ukuran DPI dan timer

/// `S(v)` (`win12shell.cpp:113`): ukuran 96 DPI diskalakan ke DPI layar lalu dibulatkan.
pub fn s(v: f32) -> i32 {
    (v * DPI.get() as f32 / 96.0).round() as i32
}

/// `SF(v)` (`win12shell.cpp:114`): versi float tanpa pembulatan.
pub fn sf(v: f32) -> f32 {
    v * DPI.get() as f32 / 96.0
}

/// `SetTimer(g_msg, id, ms, nullptr)`.
pub fn set_timer(id: i32, ms: u32) {
    unsafe { SetTimer(Some(MSG.get()), id as usize, ms, None) };
}

/// `KillTimer(g_msg, id)`.
pub fn kill_timer(id: i32) {
    let _ = unsafe { KillTimer(Some(MSG.get()), id as usize) };
}

/// `TrimSoon()` (`win12shell.cpp:110`): pangkas working set sekali, 3 detik setelah panel/notch ditutup.
pub fn trim_soon() {
    set_timer(T_TRIM, 3000);
}

// ---------------------------------------------------------------- jendela berlapis (C1: 64–84)

/// `IsLayeredOurs(h)` (`win12shell.cpp:69`): jendela berlapis milik kita (prop "w12l").
pub fn is_layered_ours(h: HWND) -> bool {
    if h.0.is_null() {
        return false;
    }
    let name = to_wide_z("w12l");
    let v = unsafe { GetPropW(h, pcw(&name)) };
    !v.0.is_null()
}

/// `W12Invalidate(h, r, erase)` (`win12shell.cpp:70`). Dipakai sebagai pengganti `InvalidateRect` di seluruh shell.
pub fn w12_invalidate(h: HWND, rect: Option<&RECT>, erase: bool) -> bool {
    if is_layered_ours(h) {
        let flag = to_wide_z("w12p");
        let v = unsafe { GetPropW(h, pcw(&flag)) };
        if v.0.is_null() {
            let _ = unsafe { SetPropW(h, pcw(&flag), Some(HANDLE(1 as *mut c_void))) };
            let _ = unsafe { PostMessageW(Some(h), WM_W12_RENDER, WPARAM(0), LPARAM(0)) };
        }
        return true;
    }
    unsafe { InvalidateRect(Some(h), rect.map(|r| r as *const RECT), erase) }.as_bool()
}

/// `W12Update(h)` (`win12shell.cpp:78`). Dipakai sebagai pengganti `UpdateWindow` di seluruh shell.
pub fn w12_update(h: HWND) -> bool {
    if is_layered_ours(h) {
        let flag = to_wide_z("w12p");
        let v = unsafe { GetPropW(h, pcw(&flag)) };
        if !v.0.is_null() {
            let _ = unsafe { SendMessageW(h, WM_W12_RENDER, None, None) };
        }
        return true;
    }
    unsafe { UpdateWindow(h) }.as_bool()
}

// ---------------------------------------------------------------- konfigurasi (C1: 116–170)

/// `LoadSettings()` (`win12shell.cpp:139`).
pub fn load_settings() {
    let mode = ini_int("shell", "mode", MODE_DOCK);
    MODE.set(if (0..=2).contains(&mode) { mode } else { MODE_SEGMENTED });
    SMALL.set(ini_int("shell", "small", 0) != 0);
    SEARCH.set(ini_int("shell", "search", 1) != 0);
    TRAY_CLOCK.set(ini_int("shell", "trayclock", 1) != 0);
    WIDGETS.set(ini_int("shell", "widgets", 1) != 0);
    WIN_KEY.set(ini_int("shell", "winkey", 1) != 0);
    FILES_OWN.set(ini_int("files", "own", 1) != 0);
    // Hanya dibaca (tidak ditulis SaveSettings): tanpa kunci di settings.ini keduanya aktif.
    GLASS.set(ini_int("shell", "glass", 1) != 0);
    DESK_LAYER.set(ini_int("shell", "desklayer", 1) != 0);
    NOTCH_ON.set(ini_int("shell", "notch", 1) != 0);
    // Default 1: HUD notch (media, volume, baterai, pesan) juga tampil saat Explorer masih berjalan.
    // Dulu 0 dan hanya aktif lewat `--shell` (core_c4b.rs:1391), sehingga tidak muncul di mode ini. notchhud=0 untuk mematikan.
    NOTCH_HUD.set(ini_int("shell", "notchhud", 1) != 0);
    NOTCH_LIVE.set(ini_int("shell", "notchlive", 1) != 0);
    let theme = ini_string("shell", "theme", "dark", 16);
    THEME_PREF.set(if theme.eq_ignore_ascii_case("system") {
        0
    } else if theme.eq_ignore_ascii_case("light") {
        2
    } else {
        1
    });
    ORIG_AUTO_HIDE.set(ini_int("state", "OrigAutoHide", -1));
}

fn bit_str(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

/// `SaveSettings()` (`win12shell.cpp:159`).
pub fn save_settings() {
    ini_write("shell", "mode", &MODE.get().to_string());
    ini_write("shell", "small", bit_str(SMALL.get()));
    ini_write("shell", "search", bit_str(SEARCH.get()));
    ini_write("shell", "trayclock", bit_str(TRAY_CLOCK.get()));
    ini_write("shell", "widgets", bit_str(WIDGETS.get()));
    ini_write("shell", "winkey", bit_str(WIN_KEY.get()));
    let theme = match THEME_PREF.get() {
        0 => "system",
        2 => "light",
        _ => "dark",
    };
    ini_write("shell", "theme", theme);
}

// ---------------------------------------------------------------- LoadTheme (C1: 269–294)

/// `LoadTheme()` (`win12shell.cpp:269`): kaca Mica (bila belum tampilan layar penuh), palet, lalu aksen.
pub fn load_theme() {
    if !MICA_FULLSCREEN_SNAPSHOT.get() {
        // win12shell.cpp:271 `g_glassOk = MicaLoad();`
        GLASS_OK.set(super::core_c2::mica_load());
    }
    let light = THEME_PREF.get() == 2 || (THEME_PREF.get() == 0 && windows_uses_light());
    LIGHT.set(light);
    {
        let mut p = PALETTE.borrow_mut();
        if light {
            let text = rgba(23, 24, 42, 1.0);
            p.text = text;
            p.glyph = text;
            p.sub = rgba(23, 24, 42, 0.70);
            p.sub2 = rgba(23, 24, 42, 0.54);
            p.veil = rgba(251, 250, 254, 0.88);
            p.panel = rgba(252, 251, 255, 0.95);
            p.hover = rgba(40, 24, 96, 0.08);
            p.press = rgba(40, 24, 96, 0.05);
            p.active = rgba(40, 24, 96, 0.06);
            p.sep = rgba(40, 24, 96, 0.12);
            p.line = rgba(40, 24, 96, 0.10);
            p.chip = rgba(40, 24, 96, 0.05);
            p.chip_hover = rgba(40, 24, 96, 0.09);
            p.track = rgba(40, 24, 96, 0.13);
            p.shadow = rgba(72, 40, 140, 0.16);
        } else {
            let text = rgba(242, 243, 250, 1.0);
            p.text = text;
            p.glyph = text;
            p.sub = rgba(232, 235, 250, 0.72);
            p.sub2 = rgba(232, 235, 250, 0.54);
            p.veil = rgba(18, 22, 50, 0.86);
            p.panel = rgba(21, 26, 58, 0.95);
            p.hover = rgba(255, 255, 255, 0.11);
            p.press = rgba(255, 255, 255, 0.07);
            p.active = rgba(255, 255, 255, 0.08);
            p.sep = rgba(255, 255, 255, 0.12);
            p.line = rgba(255, 255, 255, 0.09);
            p.chip = rgba(255, 255, 255, 0.06);
            p.chip_hover = rgba(255, 255, 255, 0.11);
            p.track = rgba(255, 255, 255, 0.14);
            p.shadow = rgba(2, 4, 20, 0.46);
        }
        let veil = p.veil;
        let panel = p.panel;
        THEME_VEIL_BASE.set(veil);
        THEME_PANEL_BASE.set(panel);
    }
    apply_wallpaper_surface_tint();
    load_accent_colors();
}

// ---------------------------------------------------------------- warna dan tema (C1: win12shell.cpp 184–294)

/// `RGBA(r, g, b, a)` (`win12shell.cpp:184`): komponen 0–255 dibagi 255.
pub fn rgba(r: i32, g: i32, b: i32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a,
    }
}

/// `WindowsUsesLight()` (`win12shell.cpp:186`). Default 1 bila registri tidak terbaca.
pub fn windows_uses_light() -> bool {
    reg::get_dword(
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "SystemUsesLightTheme",
    )
    .unwrap_or(1)
        != 0
}

/// `WindowsAccentColor()` (`win12shell.cpp:194`): AccentColorMenu, lalu DWM\AccentColor, lalu 0xFFD47800.
pub fn windows_accent_color() -> D2D1_COLOR_F {
    let packed = reg::get_dword(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent",
        "AccentColorMenu",
    )
    .or_else(|| reg::get_dword(r"Software\Microsoft\Windows\DWM", "AccentColor"))
    .unwrap_or(0xFFD4_7800);
    let c = w12_decode_windows_accent(packed);
    rgba(c.r as i32, c.g as i32, c.b as i32, 1.0)
}

/// `AccentTint(base, target, amount)` (`win12shell.cpp:208`): interpolasi linear, alfa 1.0.
pub fn accent_tint(base: D2D1_COLOR_F, target: D2D1_COLOR_F, amount: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: base.r + (target.r - base.r) * amount,
        g: base.g + (target.g - base.g) * amount,
        b: base.b + (target.b - base.b) * amount,
        a: 1.0,
    }
}

/// `LoadAccentColors()` (`win12shell.cpp:216`).
pub fn load_accent_colors() {
    let light = LIGHT.get();
    let accent = windows_accent_color();
    let luminance = 0.2126 * accent.r + 0.7152 * accent.g + 0.0722 * accent.b;
    let shade = if light {
        if luminance > 0.52 { 0.38 } else { 0.12 }
    } else if luminance < 0.45 {
        0.36
    } else {
        0.12
    };
    let target = if light {
        rgba(0, 0, 0, 1.0)
    } else {
        rgba(255, 255, 255, 1.0)
    };
    let accent_ink = accent_tint(accent, target, shade);
    let mut p = PALETTE.borrow_mut();
    p.accent = accent;
    p.accent_ink = accent_ink;
    p.accent_soft = D2D1_COLOR_F { a: if light { 0.10 } else { 0.20 }, ..accent };
    p.accent_line = D2D1_COLOR_F { a: if light { 0.45 } else { 0.55 }, ..accent_ink };
    p.glow = D2D1_COLOR_F { a: if light { 0.16 } else { 0.30 }, ..accent };
    p.flash = D2D1_COLOR_F { a: 0.55, ..accent };
    p.dot = accent_ink;
}

/// `ColorChannel8(float)` (`win12shell.cpp:230`): dibulatkan lalu dijepit ke 0–255.
pub fn color_channel8(value: f32) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

/// `WallpaperTintedSurface(base, amount)` (`win12shell.cpp:238`): alfa `base` dipertahankan.
pub fn wallpaper_tinted_surface(base: D2D1_COLOR_F, amount: u8) -> D2D1_COLOR_F {
    let surface = W12Rgb8 {
        r: color_channel8(base.r),
        g: color_channel8(base.g),
        b: color_channel8(base.b),
    };
    let rgb = w12_tint_surface(surface, WALLPAPER_TINT.get(), amount);
    D2D1_COLOR_F {
        r: rgb.r as f32 / 255.0,
        g: rgb.g as f32 / 255.0,
        b: rgb.b as f32 / 255.0,
        a: base.a,
    }
}

/// `ApplyWallpaperSurfaceTint()` (`win12shell.cpp:246`).
pub fn apply_wallpaper_surface_tint() {
    let light = LIGHT.get();
    let veil = wallpaper_tinted_surface(THEME_VEIL_BASE.get(), if light { 28 } else { 72 });
    let panel = wallpaper_tinted_surface(THEME_PANEL_BASE.get(), if light { 22 } else { 56 });
    let glass = GLASS_OK.get();
    let mut p = PALETTE.borrow_mut();
    p.veil = veil;
    p.panel = panel;
    if glass {
        p.veil.a = if light { 0.70 } else { 0.66 };
        p.panel.a = if light { 0.82 } else { 0.80 };
    }
}

// ---------------------------------------------------------------- DIB cadangan, primitif gambar, ikon (C++ 400-599)

/// `g_membmp` (sekitar :400): DIB cadangan untuk `MEMDC`. Dikosongkan oleh `release_mem`.
pub static MEMBMP: SyncCell<Cell<HBITMAP>> = SyncCell::new(Cell::new(HBITMAP(std::ptr::null_mut())));
/// `g_memold`: objek GDI asli yang dipilih ke `MEMDC` sebelum DIB dipasang.
pub static MEMOLD: SyncCell<Cell<HGDIOBJ>> = SyncCell::new(Cell::new(HGDIOBJ(std::ptr::null_mut())));
/// `g_memw` / `g_memh`: ukuran DIB cadangan.
pub static MEMW: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static MEMH: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));

/// `ReleaseMem` (:403-410). Lepas DIB bersama; `EnsureMem` membuatnya lagi saat dibutuhkan.
pub fn release_mem() {
    let memdc = MEMDC.get();
    let membmp = MEMBMP.get();
    if memdc.0.is_null() || membmp.0.is_null() {
        return;
    }
    unsafe {
        SelectObject(memdc, MEMOLD.get());
        let _ = DeleteObject(HGDIOBJ(membmp.0));
    }
    MEMBMP.set(HBITMAP(std::ptr::null_mut()));
    MEMW.set(0);
    MEMH.set(0);
}

// ---- Ditambah integrator (Tahap B): MakeFormat, CreateFormats, CreateTarget, EnsureMem (C++ 313-400) ----

/// `MakeFormat` (:313-328): format teks satu baris, rata tengah vertikal, terpotong dengan elipsis.
pub fn make_format(family: &str, size: f32, weight: windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT, center: bool) -> Option<IDWriteTextFormat> {
    use windows::Win32::Graphics::DirectWrite::*;
    let dw = DW.borrow().as_ref().cloned()?;
    let fam = to_wide_z(family);
    let loc = to_wide_z("en-us");
    let f = unsafe {
        dw.CreateTextFormat(pcw(&fam), None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, pcw(&loc))
    }
    .ok()?;
    unsafe {
        let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        let _ = f.SetTextAlignment(if center { DWRITE_TEXT_ALIGNMENT_CENTER } else { DWRITE_TEXT_ALIGNMENT_LEADING });
        let tr = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
        let ell = dw.CreateEllipsisTrimmingSign(&f).ok();
        let _ = f.SetTrimming(&tr, ell.as_ref());
    }
    Some(f)
}

/// `CreateFormats` (:330-365): buat ulang semua format teks sesuai DPI saat ini.
pub fn create_formats() {
    use windows::Win32::Graphics::DirectWrite::{DWRITE_FONT_WEIGHT_NORMAL as N, DWRITE_FONT_WEIGHT_SEMI_BOLD as SB};
    use super::state::{
        TF_B, TF_BIG, TF_BS, TF_CLOCK, TF_DATE, TF_GLYPH, TF_GLYPH_LG, TF_GLYPH_MD, TF_GLYPH_SM, TF_H, TF_HUGE,
        TF_MED, TF_MEDC, TF_PCT, TF_S, TF_SC, TF_SS, TF_SSC, TF_SUB, TF_T2, TF_TITLE, TF_TRAY, TF_TZ, TF_XS,
        TF_XSC,
    };
    const T: &str = "Segoe UI Variable Text";
    const D: &str = "Segoe UI Variable Display";
    const SM: &str = "Segoe UI Variable Small";
    const FI: &str = "Segoe Fluent Icons";
    TF_TITLE.replace(make_format(T, sf(12.5), SB, false));
    TF_SUB.replace(make_format(SM, sf(11.0), N, false));
    TF_CLOCK.replace(make_format(T, sf(13.0), SB, false));
    TF_BIG.replace(make_format(D, sf(19.0), SB, false));
    TF_DATE.replace(make_format(T, sf(12.5), N, false));
    TF_GLYPH_LG.replace(make_format(FI, sf(20.0), N, true));
    TF_TRAY.replace(make_format(T, sf(12.0), N, true));
    TF_GLYPH.replace(make_format(FI, sf(16.0), N, true));
    TF_GLYPH_MD.replace(make_format(FI, sf(14.0), N, true));
    TF_GLYPH_SM.replace(make_format(FI, sf(11.0), N, true));
    TF_HUGE.replace(make_format(D, sf(36.0), SB, false));
    TF_TZ.replace(make_format(T, sf(14.0), SB, false));
    TF_H.replace(make_format(T, sf(14.0), SB, false));
    TF_T2.replace(make_format(T, sf(14.5), SB, false));
    TF_B.replace(make_format(T, sf(13.0), N, false));
    TF_BS.replace(make_format(T, sf(13.0), SB, false));
    TF_S.replace(make_format(T, sf(12.0), N, false));
    TF_SS.replace(make_format(T, sf(12.0), SB, false));
    TF_XS.replace(make_format(T, sf(11.5), N, false));
    TF_PCT.replace(make_format(D, sf(30.0), SB, false));
    TF_MED.replace(make_format(T, sf(15.0), SB, false));
    TF_SSC.replace(make_format(T, sf(12.0), SB, true));
    TF_SC.replace(make_format(T, sf(12.0), N, true));
    TF_XSC.replace(make_format(T, sf(11.0), N, true));
    TF_MEDC.replace(make_format(T, sf(15.0), SB, true));
}

/// `CreateTarget` (:367-385): render target DC perangkat lunak + kuas warna dan gradien logo Start.
pub fn create_target() -> bool {
    use windows::Win32::Graphics::Direct2D::Common::*;
    use windows::Win32::Graphics::Direct2D::*;
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
    use super::state::{D2D, START_BRUSH, START_STOPS};
    START_BRUSH.replace(None);
    START_STOPS.replace(None);
    BR.replace(None);
    RT.replace(None);
    let Some(d2d) = D2D.borrow().as_ref().cloned() else { return false };
    let p = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE, // tanpa driver GPU: RAM kecil
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
        ..Default::default()
    };
    let Ok(rt) = (unsafe { d2d.CreateDCRenderTarget(&p) }) else { return false };
    unsafe {
        rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        let c = D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
        BR.replace(rt.CreateSolidColorBrush(&c, None).ok());
        let st = [
            D2D1_GRADIENT_STOP { position: 0.0, color: rgba(64, 196, 255, 1.0) },
            D2D1_GRADIENT_STOP { position: 1.0, color: rgba(0, 92, 204, 1.0) },
        ];
        if let Ok(stops) = rt.CreateGradientStopCollection(&st, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) {
            let lp = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: windows_numerics::Vector2 { X: 0.0, Y: 0.0 },
                endPoint: windows_numerics::Vector2 { X: 1.0, Y: 1.0 },
            };
            START_BRUSH.replace(rt.CreateLinearGradientBrush(&lp, None, &stops).ok());
            START_STOPS.replace(Some(stops));
        }
    }
    RT.replace(Some(rt));
    RT_GEN.set(RT_GEN.get() + 1);
    true
}

/// `EnsureMem` (:387-400): DIB 32-bit bersama minimal `w` x `h`, dipilih ke `MEMDC`.
pub fn ensure_mem(w: i32, h: i32) {
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, CreateDIBSection};
    let mut memdc = MEMDC.get();
    if !memdc.0.is_null() && w <= MEMW.get() && h <= MEMH.get() {
        return;
    }
    unsafe {
        if memdc.0.is_null() {
            memdc = CreateCompatibleDC(None);
            MEMDC.set(memdc);
        }
        let old_bmp = MEMBMP.get();
        if !old_bmp.0.is_null() {
            SelectObject(memdc, MEMOLD.get());
            let _ = DeleteObject(HGDIOBJ(old_bmp.0));
            MEMBMP.set(HBITMAP(std::ptr::null_mut()));
        }
        MEMW.set(w.max(MEMW.get()));
        MEMH.set(h.max(MEMH.get()));
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = MEMW.get();
        bi.bmiHeader.biHeight = -MEMH.get();
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(None, &bi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
        MEMBMP.set(bmp);
        MEMOLD.set(SelectObject(memdc, HGDIOBJ(bmp.0)));
    }
}

/// `RF` (:412): kotak D2D dari sisi kiri, atas, kanan, bawah.
pub fn rf(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}

/// `SetColor` (:413): salin warna, kalikan alfa dengan `mul`, lalu pasang ke kuas `BR`.
pub fn set_color(c: D2D1_COLOR_F, mul: f32) {
    let mut t = c;
    t.a *= mul;
    if let Some(br) = BR.borrow().as_ref() {
        unsafe { br.SetColor(&t) };
    }
}

/// Memanggil `DrawText` ke `RT` dengan kuas `BR`. Tanpa target atau kuas, tidak melakukan apa-apa.
fn draw_text_units(units: &[u16], f: &IDWriteTextFormat, rc: &D2D_RECT_F, options: D2D1_DRAW_TEXT_OPTIONS) {
    let rt_g = RT.borrow();
    let br_g = BR.borrow();
    if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
        let brush: &ID2D1Brush = br;
        unsafe {
            rt.DrawText(units, f, rc as *const D2D_RECT_F, brush, options, DWRITE_MEASURING_MODE_NATURAL)
        };
    }
}

/// `FillRR` (:414-419): persegi panjang membulat terisi dengan warna `c` (dikali `mul`).
pub fn fill_rr(l: f32, t: f32, r: f32, b: f32, rad: f32, c: D2D1_COLOR_F, mul: f32) {
    let rr = D2D1_ROUNDED_RECT { rect: rf(l, t, r, b), radiusX: rad, radiusY: rad };
    set_color(c, mul);
    let rt_g = RT.borrow();
    let br_g = BR.borrow();
    if let (Some(rt), Some(br)) = (rt_g.as_ref(), br_g.as_ref()) {
        let brush: &ID2D1Brush = br;
        unsafe { rt.FillRoundedRectangle(&rr, brush) };
    }
}

/// `Text` (:420-426): teks dengan `DrawText` (CLIP, NATURAL). Lewati bila string atau format kosong.
#[allow(clippy::too_many_arguments)]
pub fn text(s: &str, f: Option<&IDWriteTextFormat>, l: f32, t: f32, r: f32, b: f32, c: D2D1_COLOR_F, mul: f32) {
    let Some(f) = f else { return };
    if s.is_empty() {
        return;
    }
    let units = to_wide(s);
    let rc = rf(l, t, r, b);
    set_color(c, mul);
    draw_text_units(&units, f, &rc, D2D1_DRAW_TEXT_OPTIONS_CLIP);
}

/// `Glyph` (:427-433): satu glyph (mis. 0xE72C) di kotak berpusat (cx, cy) dengan setengah sisi `bx`.
#[allow(clippy::too_many_arguments)]
pub fn glyph(g: u16, f: Option<&IDWriteTextFormat>, cx: f32, cy: f32, bx: f32, c: D2D1_COLOR_F, mul: f32) {
    let Some(f) = f else { return };
    let units = [g];
    let rc = rf(cx - bx, cy - bx, cx + bx, cy + bx);
    set_color(c, mul);
    draw_text_units(&units, f, &rc, D2D1_DRAW_TEXT_OPTIONS_NONE);
}

/// `MeasureText` (:434-445): lebar teks termasuk spasi akhir, dibulatkan ke atas. 0 bila gagal.
pub fn measure_text(s: &str, f: Option<&IDWriteTextFormat>) -> f32 {
    let Some(f) = f else { return 0.0 };
    if s.is_empty() {
        return 0.0;
    }
    let units = to_wide(s);
    let dw_g = DW.borrow();
    let Some(dw) = dw_g.as_ref() else { return 0.0 };
    let Ok(layout) = (unsafe { dw.CreateTextLayout(&units, f, 4096.0, 512.0) }) else {
        return 0.0;
    };
    let mut m = DWRITE_TEXT_METRICS::default();
    let ok = unsafe { layout.GetMetrics(&mut m) }.is_ok();
    let w = if ok { m.widthIncludingTrailingWhitespace } else { 0.0 };
    w.ceil()
}

/// Kotak gambar ikon (`struct Icon`, :448). `src` = sumber WIC; `bmp` = bitmap D2D, dibuat ulang bila generasi `RT_GEN` berubah.
/// Field `gen`, `tgen`, `tsz` diganti `bmp_gen`, `tile_gen`, `tile_sz` (lihat deviasi di laporan).
#[derive(Default)]
pub struct Icon {
    pub src: Option<IWICBitmapSource>,
    pub bmp: Option<ID2D1Bitmap>,
    pub bmp_gen: i32,
    pub tile: Option<ID2D1Bitmap>,
    pub tile_gen: i32,
    pub tile_sz: i32,
}

impl Icon {
    /// Ikon kosong. Bentuk `const` agar bisa dipakai di `static`.
    pub const fn new() -> Self {
        Self { src: None, bmp: None, bmp_gen: 0, tile: None, tile_gen: 0, tile_sz: 0 }
    }
}

/// `WicFromHBITMAP` (:450-483): salin piksel HBITMAP ke WIC 32bpp PBGRA (premultiplied). None bila gagal.
pub fn wic_from_hbitmap(hb: HBITMAP) -> Option<IWICBitmapSource> {
    let mut bm = BITMAP::default();
    let got = unsafe {
        GetObjectW(
            HGDIOBJ(hb.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut c_void),
        )
    };
    if got == 0 || bm.bmWidth <= 0 {
        return None;
    }
    let w = bm.bmWidth;
    let h = bm.bmHeight.abs();
    let mut bi = BITMAPINFO::default();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = -h; // top-down
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut px = vec![0u8; (w * h * 4) as usize];
    let dc = unsafe { GetDC(None) };
    let lines = unsafe {
        GetDIBits(dc, hb, 0, h as u32, Some(px.as_mut_ptr() as *mut c_void), &mut bi, DIB_RGB_COLORS)
    };
    unsafe { ReleaseDC(None, dc) };
    if lines == 0 {
        return None;
    }
    // Deteksi: ada alfa > 0? ada warna yang melebihi alfa (berarti belum premultiplied)?
    let mut straight = false;
    let mut any_alpha = false;
    for p in px.chunks_exact(4) {
        let a = p[3];
        if a != 0 {
            any_alpha = true;
        }
        if p[0] > a || p[1] > a || p[2] > a {
            straight = true;
        }
    }
    for p in px.chunks_exact_mut(4) {
        if !any_alpha {
            p[3] = 255; // bitmap tanpa alpha: anggap opak
            continue;
        }
        if straight {
            let a = p[3] as u32;
            p[0] = (p[0] as u32 * a / 255) as u8;
            p[1] = (p[1] as u32 * a / 255) as u8;
            p[2] = (p[2] as u32 * a / 255) as u8;
        }
    }
    let wic_g = WIC.borrow();
    let wic = wic_g.as_ref()?;
    let wb = unsafe {
        wic.CreateBitmapFromMemory(
            w as u32,
            h as u32,
            &GUID_WICPixelFormat32bppPBGRA,
            (w * 4) as u32,
            &px,
        )
    }
    .ok()?;
    wb.cast::<IWICBitmapSource>().ok()
}

/// `IconFromShellItem` (:485-496): ikon ukuran `size` px dari `IShellItem` (IShellItemImageFactory, ICONONLY).
pub fn icon_from_shell_item(si: &IShellItem, size: i32) -> Option<IWICBitmapSource> {
    let f: IShellItemImageFactory = si.cast().ok()?;
    let hb = unsafe { f.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY) }.ok()?;
    if hb.0.is_null() {
        return None;
    }
    let out = wic_from_hbitmap(hb);
    unsafe {
        let _ = DeleteObject(HGDIOBJ(hb.0));
    }
    out
}

/// `IconFromParsingName` (:498-504): ikon dari nama penguraian shell (mis. `shell:AppsFolder\...`).
pub fn icon_from_parsing_name(name: &str, size: i32) -> Option<IWICBitmapSource> {
    let w = to_wide_z(name);
    let si: IShellItem = unsafe { SHCreateItemFromParsingName(pcw(&w), None::<&IBindCtx>) }.ok()?;
    icon_from_shell_item(&si, size)
}

/// `IconFromHICON` (:506-517): HICON -> WIC. Bila lebar tidak sama dengan `size`, diskalakan (Fant).
/// Deviasi kecil: bila `Initialize` skalar gagal, C++ tetap mengembalikan skalar kosong; di sini None.
pub fn icon_from_hicon(hi: HICON, size: i32) -> Option<IWICBitmapSource> {
    if hi.0.is_null() {
        return None;
    }
    let wic_g = WIC.borrow();
    let wic = wic_g.as_ref()?;
    let wb = unsafe { wic.CreateBitmapFromHICON(hi) }.ok()?;
    let (mut w, mut h) = (0u32, 0u32);
    let _ = unsafe { wb.GetSize(&mut w, &mut h) };
    if w as i32 == size {
        return wb.cast::<IWICBitmapSource>().ok();
    }
    let Ok(sc) = (unsafe { wic.CreateBitmapScaler() }) else {
        return wb.cast::<IWICBitmapSource>().ok();
    };
    let src: IWICBitmapSource = wb.cast().ok()?;
    unsafe { sc.Initialize(&src, size as u32, size as u32, WICBitmapInterpolationModeFant) }.ok()?;
    sc.cast::<IWICBitmapSource>().ok()
}

// ---------------------------------------------------------------- panggilan vtable D2D mentah (PORT)
// `windows` 0.61 hanya menyediakan `ID2D1RenderTarget::CreateBitmap` dan `CreateBitmapFromWicBitmap` bila fitur
// `Win32_Graphics_Dxgi_Common` kini aktif di Cargo.toml, tetapi kedua slot vtable tetap dipanggil langsung
// (tidak diganti tipe crate). Urutan slot: ID2D1Resource = IUnknown (0-2) + GetFactory (3);
// ID2D1RenderTarget: CreateBitmap (4), CreateBitmapFromWicBitmap (5). Struct di bawah mengikuti layout C (repr C)
// dari D2D1_PIXEL_FORMAT, D2D1_BITMAP_PROPERTIES, dan D2D_SIZE_U.

/// `DXGI_FORMAT_B8G8R8A8_UNORM` (87 di header DXGI).
const DXGI_FORMAT_B8G8R8A8_UNORM_VAL: i32 = 87;
/// `D2D1_ALPHA_MODE_PREMULTIPLIED` (1 di header D2D).
const D2D1_ALPHA_MODE_PREMULTIPLIED_VAL: i32 = 1;

/// `D2D1_PIXEL_FORMAT`.
#[repr(C)]
#[derive(Clone, Copy)]
struct D2dPixelFormat {
    format: i32,
    alpha_mode: i32,
}

/// `D2D1_BITMAP_PROPERTIES` (16 byte).
#[repr(C)]
#[derive(Clone, Copy)]
struct D2dBitmapProps {
    pixel_format: D2dPixelFormat,
    dpi_x: f32,
    dpi_y: f32,
}

/// `D2D_SIZE_U` (8 byte; ABI x64 Windows meneruskan struct 8 byte lewat register, sama seperti C).
#[repr(C)]
#[derive(Clone, Copy)]
struct D2dSizeU {
    width: u32,
    height: u32,
}

/// Signature `ID2D1RenderTarget::CreateBitmap(this, size, srcData, pitch, props, out)`.
type FnRtCreateBitmap = unsafe extern "system" fn(
    *mut c_void,
    D2dSizeU,
    *const c_void,
    u32,
    *const D2dBitmapProps,
    *mut *mut c_void,
) -> HRESULT;

/// Signature `ID2D1RenderTarget::CreateBitmapFromWicBitmap(this, wic, props, out)`.
type FnRtCreateBitmapFromWic = unsafe extern "system" fn(
    *mut c_void,
    *mut c_void,
    *const D2dBitmapProps,
    *mut *mut c_void,
) -> HRESULT;

/// Pointer fungsi di slot `index` dari vtable objek COM `this` (pointer pertama objek COM = pointer vtable).
unsafe fn com_slot(this: *mut c_void, index: usize) -> *const c_void {
    let vtbl = *(this as *const *const *const c_void);
    *vtbl.add(index)
}

/// `g_rt->CreateBitmap({w, h}, px, pitch, &bp, &out)` dengan `bp` = B8G8R8A8 PREMULTIPLIED, 96 dpi (C :1998-2003).
pub fn d2d_bitmap_bgra(rt: &ID2D1DCRenderTarget, w: u32, h: u32, px: &[u8], pitch: u32) -> Option<ID2D1Bitmap> {
    let bp = D2dBitmapProps {
        pixel_format: D2dPixelFormat {
            format: DXGI_FORMAT_B8G8R8A8_UNORM_VAL,
            alpha_mode: D2D1_ALPHA_MODE_PREMULTIPLIED_VAL,
        },
        dpi_x: 96.0,
        dpi_y: 96.0,
    };
    let this = rt.as_raw();
    let mut out: *mut c_void = std::ptr::null_mut();
    unsafe {
        let f: FnRtCreateBitmap = std::mem::transmute(com_slot(this, 4));
        let hr = f(
            this,
            D2dSizeU { width: w, height: h },
            px.as_ptr() as *const c_void,
            pitch,
            &bp,
            &mut out,
        );
        if hr.is_ok() && !out.is_null() {
            Some(ID2D1Bitmap::from_raw(out))
        } else {
            None
        }
    }
}

/// `g_rt->CreateBitmapFromWicBitmap(cv, nullptr, &bmp)` (C :527); `props` = nullptr.
pub fn d2d_bitmap_from_wic(rt: &ID2D1DCRenderTarget, wic: &IWICBitmapSource) -> Option<ID2D1Bitmap> {
    let this = rt.as_raw();
    let mut out: *mut c_void = std::ptr::null_mut();
    unsafe {
        let f: FnRtCreateBitmapFromWic = std::mem::transmute(com_slot(this, 5));
        let hr = f(this, wic.as_raw(), std::ptr::null(), &mut out);
        if hr.is_ok() && !out.is_null() {
            Some(ID2D1Bitmap::from_raw(out))
        } else {
            None
        }
    }
}

/// `IconBitmap` (win12shell.cpp:519-532): bitmap D2D dari sumber WIC (PBGRA, DitherNone, palette Custom),
/// di-cache per generasi `RT_GEN`. Cache dipakai hanya bila `bmp` ada (C :522). Generasi tetap disimpan walau
/// pembuatan gagal (C :530), sehingga pembuatan dicoba lagi pada panggilan berikutnya.
/// Deviasi: C++ tidak memeriksa `g_rt`; di sini bila `RT` belum ada hasilnya None (tanpa crash).
pub fn icon_bitmap(ic: &mut Icon) -> Option<ID2D1Bitmap> {
    let src = ic.src.as_ref()?;
    if ic.bmp.is_some() && ic.bmp_gen == RT_GEN.get() {
        return ic.bmp.clone();
    }
    ic.bmp = None;
    let wic_g = WIC.borrow();
    if let Some(wic) = wic_g.as_ref() {
        if let Ok(cv) = unsafe { wic.CreateFormatConverter() } {
            let init = unsafe {
                cv.Initialize(
                    src,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapDitherTypeNone,
                    None::<&IWICPalette>,
                    0.0,
                    WICBitmapPaletteTypeCustom,
                )
            };
            if init.is_ok() {
                let rt_g = RT.borrow();
                if let Some(rt) = rt_g.as_ref() {
                    let base: &IWICBitmapSource = &cv;
                    ic.bmp = d2d_bitmap_from_wic(rt, base);
                }
            }
        }
    }
    ic.bmp_gen = RT_GEN.get();
    ic.bmp.clone()
}

/// `IconFree` (:533): lepas bitmap, sumber, dan ubin; reset generasi ubin.
pub fn icon_free(ic: &mut Icon) {
    ic.bmp = None;
    ic.src = None;
    ic.tile = None;
    ic.tile_gen = 0;
}

/// `DrawIconAt` (:535-541): gambar ikon di kotak (x, y)-(x+size, y+size) dengan opasitas `opacity`.
pub fn draw_icon_at(ic: &mut Icon, x: f32, y: f32, size: f32, opacity: f32) {
    let Some(b) = icon_bitmap(ic) else { return };
    let rc = rf(x, y, x + size, y + size);
    let rt_g = RT.borrow();
    if let Some(rt) = rt_g.as_ref() {
        unsafe {
            rt.DrawBitmap(
                &b,
                Some(&rc as *const D2D_RECT_F),
                opacity,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                None,
            )
        };
    }
}

// ---------------------------------------------------------------- model aplikasi (C++ 544-598)

/// `PFN_GetAppUserModelId` (:547): fungsi dimuat dinamis; diisi saat init (ditulis di blok berikutnya).
pub type PfnGetAppUserModelId = unsafe extern "system" fn(HANDLE, *mut u32, PWSTR) -> i32;
/// `pGetApplicationUserModelId` (:548).
pub static PFN_GET_APP_USER_MODEL_ID: SyncCell<Cell<Option<PfnGetAppUserModelId>>> =
    SyncCell::new(Cell::new(None));

/// `Lower` (:550): huruf kecil per karakter (pengganti `towlower`, satu karakter per karakter).
pub fn lower(s: &str) -> String {
    s.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect()
}

/// `Copy` (:551): salin paling banyak `cch - 1` unit UTF-16 (sisakan NUL), setara `wcsncpy` + NUL.
pub fn copy_wide(s: &str, cch: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(cch.saturating_sub(1)).collect();
    String::from_utf16_lossy(&units)
}

/// `FileDescription` (:553-572): FileDescription dari resource versi berkas exe. None bila tidak ada.
pub fn file_description(exe: &str, cch: usize) -> Option<String> {
    let path = to_wide_z(exe);
    let sz = unsafe { GetFileVersionInfoSizeW(pcw(&path), None) };
    if sz == 0 {
        return None;
    }
    // Vec<u32> menjamin keselarasan 4 byte untuk data versi.
    let mut buf = vec![0u32; (sz as usize + 3) / 4];
    unsafe { GetFileVersionInfoW(pcw(&path), None, sz, buf.as_mut_ptr() as *mut c_void) }.ok()?;
    let block = buf.as_ptr() as *const c_void;
    // Translation: pasangan (lang, cp) 16-bit pertama.
    let mut tr: *mut c_void = std::ptr::null_mut();
    let mut cb: u32 = 0;
    let tq = to_wide_z("\\VarFileInfo\\Translation");
    let ok_tr = unsafe { VerQueryValueW(block, pcw(&tq), &mut tr, &mut cb) }.as_bool();
    if !ok_tr || cb < 4 || tr.is_null() {
        return None;
    }
    let (lang, cp) = unsafe {
        let p = tr as *const u16;
        (*p, *p.add(1))
    };
    let q = to_wide_z(&format!("\\StringFileInfo\\{:04x}{:04x}\\FileDescription", lang, cp));
    let mut v: *mut c_void = std::ptr::null_mut();
    let mut vl: u32 = 0;
    let ok_v = unsafe { VerQueryValueW(block, pcw(&q), &mut v, &mut vl) }.as_bool();
    if !ok_v || vl <= 1 || v.is_null() {
        return None;
    }
    let units = unsafe { std::slice::from_raw_parts(v as *const u16, vl as usize) };
    let n = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    Some(copy_wide(&String::from_utf16_lossy(&units[..n]), cch))
}

/// Satu aplikasi di Start/taskbar (`struct App`, :576-588). `g_apps[]` + `g_napps` menjadi `APPS` (Vec).
/// `pin`: -1 tidak disematkan, 0 kiri, 1 kanan. `spin`: urutan Disematkan Start (0 = tidak).
#[derive(Default)]
pub struct App {
    /// AUMID (`aumid[256]`); kosong bila tidak ada.
    pub aumid: String,
    /// Path exe huruf kecil (`exe[MAX_PATH]`).
    pub exe: String,
    /// Perintah peluncuran (`launch[600]`).
    pub launch: String,
    /// Nama ikon di berkas (`iconName[600]`); kosong = ikon jendela.
    pub icon_name: String,
    /// Nama tampilan (`name[128]`).
    pub name: String,
    pub icon: Icon,
    pub pin: i32,
    pub spin: i32,
    /// Jendela milik aplikasi, batas `MAXW`. Menggantikan `wins[MAXW]` + `nw`.
    pub wins: Vec<HWND>,
    pub fg: bool,
    pub flash: bool,
}

/// `g_apps[MAXAPPS]` + `g_napps` (:589-590). Panjang Vec = jumlah slot terpakai; batas `MAXAPPS` (state.rs).
pub static APPS: SyncCell<RefCell<Vec<App>>> = SyncCell::new(RefCell::new(Vec::new()));
/// `g_noteIcon` (:592): ikon Notepad untuk item "Create new note".
pub static NOTE_ICON: SyncCell<RefCell<Icon>> = SyncCell::new(RefCell::new(Icon::new()));

/// `IconSize()` (:596): ukuran ikon (px, DPI-scaled): kecil, dock (32), atau normal.
pub fn icon_size() -> i32 {
    let base = if SMALL.get() {
        ICON_PX_SM
    } else if MODE.get() == MODE_DOCK {
        32
    } else {
        ICON_PX
    };
    s(base as f32)
}

/// `TileMargin()` (:597): margin ubin (DPI-scaled).
pub fn tile_margin() -> i32 {
    s(3.0)
}

/// `SrcPx()` (:598): ukuran sumber WIC = ceil(IconSize() * 1.28).
pub fn src_px() -> i32 {
    (icon_size() as f32 * 1.28).ceil() as i32
}

// ---------------------------------------------------------------- apps.txt (C++ 600–769)

/// `MAX_PATH` C (260 WCHAR termasuk NUL). Konstanta lokal: `MAX_PATH` crate windows bertipe u32 dan belum diimpor di sini.
const MAX_PATH: usize = 260;

/// `CoTaskMemFree`: membebaskan memori shell. Pointer null diabaikan.
fn co_free(p: *mut u16) {
    if !p.is_null() {
        unsafe { CoTaskMemFree(Some(p as *const c_void)) };
    }
}

/// `GetWindowsDirectoryW` (C++ 628). String kosong bila gagal atau tidak muat `MAX_PATH`.
fn windows_directory() -> String {
    let mut buf = [0u16; MAX_PATH];
    let n = unsafe { GetWindowsDirectoryW(Some(&mut buf[..])) } as usize;
    if n == 0 || n >= MAX_PATH {
        return String::new();
    }
    from_wide(&buf[..n])
}

/// `ExpandEnvironmentStringsW(entry, path, MAX_PATH)` (C++ 635). Deviasi: hasil 0 atau > `MAX_PATH` dianggap gagal (None).
fn expand_env(teks: &str) -> Option<String> {
    let sumber = to_wide_z(teks);
    let mut buf = [0u16; MAX_PATH];
    let n = unsafe { windows::Win32::System::Environment::ExpandEnvironmentStringsW(crate::wide::pcw(&sumber), Some(&mut buf[..])) } as usize;
    if n == 0 || n > MAX_PATH {
        return None;
    }
    Some(from_wide(&buf[..]))
}

/// Target `.lnk` lewat `IShellLinkW` + `IPersistFile` (C++ 640–645). None bila salah satu langkah gagal.
fn shell_link_target(path: &str) -> Option<String> {
    let wp = to_wide_z(path);
    let sl: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }.ok()?;
    let pf: IPersistFile = sl.cast::<IPersistFile>().ok()?;
    unsafe { pf.Load(pcw(&wp), STGM_READ) }.ok()?;
    let mut t = [0u16; MAX_PATH];
    unsafe { sl.GetPath(&mut t[..], std::ptr::null_mut(), 0) }.ok()?;
    Some(from_wide(&t[..]))
}

/// `PathFindFileNameW` (pure-Rust, sederhana): bagian setelah separator terakhir `\`, `/`, atau `:`.
fn path_find_file_name(p: &str) -> &str {
    match p.rfind(|c: char| c == '\\' || c == '/' || c == ':') {
        Some(i) => &p[i + 1..],
        None => p,
    }
}

/// `PathFindExtensionW` (pure-Rust, sederhana): titik terakhir setelah separator terakhir, termasuk titik. "" bila tidak ada.
fn path_extension(p: &str) -> &str {
    let mut titik: Option<usize> = None;
    for (i, c) in p.char_indices() {
        match c {
            '\\' | '/' | ':' => titik = None,
            '.' => titik = Some(i),
            _ => {}
        }
    }
    match titik {
        Some(i) => &p[i..],
        None => "",
    }
}

/// `PathRemoveExtensionW`: membuang ekstensi (termasuk titik) dari akhir string.
fn path_remove_extension(s_nama: &str) -> String {
    let ext = path_extension(s_nama);
    s_nama[..s_nama.len() - ext.len()].to_string()
}

/// Identitas entri di apps.txt: AUMID bila ada, selain itu `launch` (C++ 755, 764).
fn app_identity(a: &App) -> &str {
    if a.aumid.is_empty() { &a.launch } else { &a.aumid }
}

/// `AppFromShellItem` (C++ 600–611): isi `name` dan `exe` (dari PKEY target), lalu ikon dari `IShellItem`.
fn app_from_shell_item(si: &IShellItem, a: &mut App) {
    if a.name.is_empty() {
        if let Ok(p) = unsafe { si.GetDisplayName(SIGDN_NORMALDISPLAY) } {
            let nama = unsafe { from_pcwstr(p.0) };
            a.name = copy_wide(&nama, 128);
            co_free(p.0);
        }
    }
    if a.exe.is_empty() {
        if let Ok(si2) = si.cast::<IShellItem2>() {
            if let Ok(p) = unsafe { si2.GetString(&PK_TARGET) } {
                let target = unsafe { from_pcwstr(p.0) };
                let b = target.as_bytes();
                // C++ 606: iswalpha(s[0]) && s[1]==':' && s[2]=='\\'. Deviasi: iswalpha dipersempit ke ASCII.
                if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
                    a.exe = copy_wide(&target, MAX_PATH);
                }
                co_free(p.0);
            }
        }
    }
    a.icon.src = icon_from_shell_item(si, src_px());
}

/// `ResolveEntry` (C++ 614–659): satu baris apps.txt menjadi `App` dengan pin = -1. None bila gagal (C: false).
fn resolve_entry(entry: &str) -> Option<App> {
    let mut a = App { pin: -1, ..App::default() };
    let is_path = entry.contains('\\') || entry.contains('/');
    if !is_path {
        a.aumid = copy_wide(entry, 256);
        a.launch = copy_wide(&format!("shell:AppsFolder\\{}", entry), 600);
        a.icon_name = copy_wide(&a.launch, 600);
        let wl = to_wide_z(&a.launch);
        let si: IShellItem = unsafe { SHCreateItemFromParsingName(pcw(&wl), None::<&IBindCtx>) }.ok()?;
        app_from_shell_item(&si, &mut a);
        if entry.eq_ignore_ascii_case("Microsoft.Windows.Explorer") {
            // PORT: pin Explorer dialihkan ke win12files.exe (native) bila ada. explorer.exe tidak dipakai di sini
            // karena akan menyalakan shell Explorer (taskbar/desktop) saat Win12 menjadi shell.
            let wf = format!("{}\\win12files.exe", DIR.borrow());
            let wwf = to_wide_z(&wf);
            // SAFETY: wwf berakhir NUL dan hidup selama pemanggilan.
            if FILES_OWN.get() && unsafe { GetFileAttributesW(pcw(&wwf)) } != INVALID_FILE_ATTRIBUTES {
                a.launch = copy_wide(&wf, 600);
                a.icon_name = copy_wide(&wf, 600);
                a.exe = copy_wide(&wf, MAX_PATH);
            } else if a.exe.is_empty() {
                a.exe = copy_wide(&format!("{}\\explorer.exe", windows_directory()), MAX_PATH);
            }
        }
        a.exe = lower(&a.exe);
        return Some(a);
    }
    let path = expand_env(entry)?;
    let wp = to_wide_z(&path);
    if unsafe { GetFileAttributesW(pcw(&wp)) } == INVALID_FILE_ATTRIBUTES {
        return None;
    }
    a.launch = copy_wide(&path, 600);
    a.icon_name = copy_wide(&path, 600);
    if path_extension(&path).eq_ignore_ascii_case(".lnk") {
        if let Some(target) = shell_link_target(&path) {
            a.exe = copy_wide(&target, MAX_PATH);
        }
        a.name = path_remove_extension(&copy_wide(path_find_file_name(&path), 128));
    } else {
        a.exe = copy_wide(&path, MAX_PATH);
        a.name = match file_description(&path, 128) {
            Some(d) => d,
            None => path_remove_extension(&copy_wide(path_find_file_name(&path), 128)),
        };
    }
    a.icon.src = icon_from_parsing_name(&path, src_px());
    a.exe = lower(&a.exe);
    Some(a)
}

// ---------------------------------------------------------------- apps.txt: teks default dan baca/tulis (C++ 661–704)

/// Teks awal apps.txt (C++ 661–667). Semuanya ASCII, jadi bytenya sama dengan UTF-8 tanpa BOM.
const DEFAULT_APPS: &str = concat!(
    "# Win12 Shell - aplikasi tersemat di taskbar.\r\n",
    "# Satu entri per baris: AUMID aplikasi (lihat Get-StartApps), path .exe, atau path .lnk.\r\n",
    "# [left] = grup sebelah kiri Start, [right] = grup sebelah kanan. Simpan, lalu jalankan ulang Win12 Shell.\r\n",
    "[left]\r\nMSEdge\r\nMicrosoft.Office.POWERPNT.EXE.15\r\nMicrosoft.Office.EXCEL.EXE.15\r\nMSTeams_8wekyb3d8bbwe!MSTeams\r\n",
    "[right]\r\nwindows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel\r\nMicrosoft.Windows.Explorer\r\n",
    "Microsoft.Windows.Photos_8wekyb3d8bbwe!App\r\nMicrosoft.WindowsCalculator_8wekyb3d8bbwe!App\r\n",
);

/// Dua baris header yang ditulis SaveApps (C++ 749–750). Sama dengan dua baris pertama DEFAULT_APPS.
const APPS_HEADER: &str = concat!(
    "# Win12 Shell - aplikasi tersemat di taskbar.\r\n",
    "# Satu entri per baris: AUMID aplikasi (lihat Get-StartApps), path .exe, atau path .lnk.\r\n",
);

/// `ReadTextFile` (C++ 669–693). None bila ≥ 1 MiB atau gagal dibuka/dibaca (C: nullptr).
/// BOM FF FE = UTF-16LE; selain itu BOM UTF-8 dilewati lalu didekode UTF-8 (byte rusak jadi U+FFFD).
/// NUL di dalam teks tidak dipotong di sini; pemotongan per baris dilakukan di `load_apps`, seperti C.
fn read_text_file(path: &str) -> Option<String> {
    use std::io::Read;
    use std::os::windows::fs::OpenOptionsExt;
    // share_mode(1) = FILE_SHARE_READ (C++ 671)
    let mut f = std::fs::OpenOptions::new().read(true).share_mode(1).open(path).ok()?;
    let size = f.metadata().ok()?.len();
    if size >= (1u64 << 20) {
        return None;
    }
    let mut b: Vec<u8> = Vec::with_capacity(size as usize);
    f.read_to_end(&mut b).ok()?;
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        let units: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return Some(String::from_utf16_lossy(&units));
    }
    let skip = if b.len() >= 3 && b[0] == 0xEF && b[1] == 0xBB && b[2] == 0xBF { 3 } else { 0 };
    Some(String::from_utf8_lossy(&b[skip..]).into_owned())
}

/// `WriteUtf8File` (C++ 695–704): tulis teks sebagai UTF-8 tanpa NUL. CREATE_ALWAYS dan share 0 seperti C.
fn write_utf8_file(path: &str, text: &str) {
    use std::io::Write;
    use std::os::windows::fs::OpenOptionsExt;
    // share_mode(0) = tanpa berbagi (C++ 701)
    match std::fs::OpenOptions::new().write(true).create(true).truncate(true).share_mode(0).open(path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(text.as_bytes()) {
                crate::log::log(&format!("write_utf8_file: tulis gagal: {e}"));
            }
        }
        Err(e) => crate::log::log(&format!("write_utf8_file: buka gagal: {e}")),
    }
}

/// `FindAppByIdentity` (C++ 3732–3739) atas daftar `list` (= g_apps[0..g_napps)). Dengan AUMID: cocok bila
/// AUMID sama (`_wcsicmp`, di sini `eq_ignore_ascii_case`). Tanpa AUMID: cocok bila entri juga tanpa AUMID
/// dan exe serta launch sama persis.
fn find_app_by_identity(s: &App, list: &[App]) -> Option<usize> {
    list.iter().position(|g| {
        if !s.aumid.is_empty() {
            g.aumid.eq_ignore_ascii_case(&s.aumid)
        } else {
            g.aumid.is_empty() && g.exe == s.exe && g.launch == s.launch
        }
    })
}

/// `LoadApps` (C++ 710–744). Membuat apps.txt dari DEFAULT_APPS bila belum ada, lalu memuat `APPS`.
/// Beda dari C: `APPS_TXT` kosong ditolak (diisi di `w_win_main`, core_c4b.rs); daftar disusun lokal lalu diganti sekaligus.
pub fn load_apps() {
    let path: String = APPS_TXT.borrow().clone();
    if path.is_empty() {
        crate::log::log("load_apps: APPS_TXT kosong, apps.txt tidak dimuat");
        return;
    }
    let wp = to_wide_z(&path);
    if unsafe { GetFileAttributesW(pcw(&wp)) } == INVALID_FILE_ATTRIBUTES {
        // C++ 713–714: CREATE_NEW lalu tulis DEFAULT_APPS; kegagalan diabaikan seperti C
        if let Ok(mut f) = std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            if let Err(e) = f.write_all(DEFAULT_APPS.as_bytes()) {
                crate::log::log(&format!("load_apps: tulis apps.txt awal gagal: {e}"));
            }
        }
    }
    let text = match read_text_file(&path) {
        Some(t) => t,
        None => return,
    };
    let mut list: Vec<App> = Vec::new(); // C: g_napps mulai dari 0
    let mut group: i32 = 1; // C: grup awal = 1 (kanan)
    let mut nstart: i32 = 0;
    let mut pos: usize = 0;
    loop {
        let rest = &text[pos..];
        // C: `for (line = t; line && *line;)`
        if rest.is_empty() || rest.starts_with('\0') {
            break;
        }
        // C: wcschr mencari '\n' dan berhenti di NUL. NUL lebih dulu berarti baris ini yang terakhir.
        let (seg, next) = match rest.find(|c: char| c == '\n' || c == '\0') {
            Some(i) if rest.as_bytes()[i] == b'\n' => (&rest[..i], Some(pos + i + 1)),
            Some(i) => (&rest[..i], None),
            None => (rest, None),
        };
        // C: spasi/tab di depan, lalu spasi/tab/CR di belakang
        let s = seg.trim_start_matches([' ', '\t']).trim_end_matches([' ', '\t', '\r']);
        if !s.is_empty() && !s.starts_with('#') {
            if s.eq_ignore_ascii_case("[left]") {
                group = 0;
            } else if s.eq_ignore_ascii_case("[right]") {
                group = 1;
            } else if s.eq_ignore_ascii_case("[start]") {
                group = 2;
            } else if list.len() < MAXAPPS as usize {
                if let Some(mut a) = resolve_entry(s) {
                    let dup = find_app_by_identity(&a, &list);
                    if group == 2 {
                        match dup {
                            Some(d) => {
                                if list[d].spin == 0 {
                                    nstart += 1;
                                    list[d].spin = nstart;
                                }
                                icon_free(&mut a.icon);
                            }
                            None => {
                                nstart += 1;
                                a.spin = nstart;
                                list.push(a);
                            }
                        }
                    } else {
                        match dup {
                            Some(d) if list[d].pin < 0 => {
                                list[d].pin = group;
                                icon_free(&mut a.icon);
                            }
                            _ => {
                                a.pin = group;
                                list.push(a);
                            }
                        }
                    }
                }
            }
        }
        pos = match next {
            Some(p) => p,
            None => break,
        };
    }
    *APPS.borrow_mut() = list;
}

/// `SaveApps` (C++ 746–769). Menulis `APPS` ke apps.txt. Isi dibatasi 23000 unit UTF-16 seperti C (buffer 24000).
pub fn save_apps() {
    let path: String = APPS_TXT.borrow().clone();
    if path.is_empty() {
        crate::log::log("save_apps: APPS_TXT kosong, apps.txt tidak ditulis");
        return;
    }
    let list = APPS.borrow();
    let mut buf = String::from(APPS_HEADER);
    let mut n: usize = APPS_HEADER.encode_utf16().count();
    for g in 0..2 {
        let head = if g == 1 { "[right]\r\n" } else { "[left]\r\n" };
        buf.push_str(head);
        n += head.encode_utf16().count();
        for a in list.iter() {
            if n >= 23000 {
                break;
            }
            if a.pin == g {
                let line = format!("{}\r\n", app_identity(a));
                n += line.encode_utf16().count();
                buf.push_str(&line);
            }
        }
    }
    // C: ns = spin tertinggi; entri [start] ditulis menurut urutan spin 1..=ns (pemilik spin pertama)
    let ns: i32 = list.iter().map(|a| a.spin).fold(0, i32::max);
    if ns > 0 {
        let head = "[start]\r\n";
        buf.push_str(head);
        n += head.encode_utf16().count();
        for k in 1..=ns {
            if n >= 23000 {
                break;
            }
            if let Some(a) = list.iter().find(|a| a.spin == k) {
                let line = format!("{}\r\n", app_identity(a));
                n += line.encode_utf16().count();
                buf.push_str(&line);
            }
        }
    }
    write_utf8_file(&path, &buf);
}

// ---------------------------------------------------------------- pelacakan jendela (C 770–919)

/// Identitas satu jendela (`WinInfo`): AUMID dan nama exe (huruf kecil) hasil `resolve_window`.
#[derive(Clone, Debug)]
pub struct WinInfo {
    pub h: HWND,
    pub aumid: String,
    pub exe: String,
    pub seen: bool,
    pub retry: bool,
}

/// Cache identitas jendela (`g_wc`), dibatasi `MAXWC` entri.
pub static WC: SyncCell<RefCell<Vec<WinInfo>>> = SyncCell::new(RefCell::new(Vec::new()));

/// Jendela milik proses ini sendiri (`IsOurWindow`).
pub fn is_our_window(h: HWND) -> bool {
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid as *mut u32)) };
    pid == unsafe { GetCurrentProcessId() }
}

/// Jendela yang layak tampil di bilah tugas (`IsTaskWindow`).
pub fn is_task_window(h: HWND) -> bool {
    if !unsafe { IsWindowVisible(h) }.as_bool() {
        return false;
    }
    let ex = unsafe { GetWindowLongW(h, GWL_EXSTYLE) } as u32;
    if (ex & WS_EX_APPWINDOW.0) == 0 {
        if (ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0)) != 0 {
            return false;
        }
        if matches!(unsafe { GetWindow(h, GW_OWNER) }, Ok(o) if !o.0.is_null()) {
            return false;
        }
    }
    let mut cloaked: u32 = 0;
    // Kegagalan DWM dibiarkan: `cloaked` tetap 0, sama seperti C++.
    let _ = unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    if cloaked != 0 {
        return false;
    }
    if unsafe { GetWindowTextLengthW(h) } == 0 {
        return false;
    }
    if is_our_window(h) {
        return false;
    }
    !matches!(
        window_class(h).as_str(),
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    )
}

/// Nama kelas jendela (`GetClassNameW`, maksimal 63 karakter).
pub fn window_class(h: HWND) -> String {
    let mut cls = [0u16; 64];
    unsafe { GetClassNameW(h, &mut cls) };
    from_wide(&cls)
}

/// Layout vtbl `IPropertyStore`: slot 0–2 `IUnknown`, 3 `GetCount`, 4 `GetAt`, 5 `GetValue`
/// (sama dengan `IPropertyStore_Vtbl` crate windows 0.61.3). `GetValue` dipanggil lewat slot ini karena
/// `IPropertyStore::GetValue` di crate `windows` memerlukan fitur `Win32_System_Com_StructuredStorage`
/// yang tidak diaktifkan di `Cargo.toml` (deviasi, dilaporkan).
#[repr(C)]
struct PropertyStoreVtbl {
    _query_interface: usize,
    _add_ref: usize,
    _release: usize,
    _get_count: usize,
    _get_at: usize,
    get_value: unsafe extern "system" fn(*mut c_void, *const PROPERTYKEY, *mut c_void) -> windows::core::HRESULT,
}

/// Awal `PROPVARIANT` (x86/x64): `vt`, tiga word cadangan, lalu union dengan pointer pada offset 8.
/// `_rest` menyediakan ruang untuk tipe lain yang mungkin ditulis `GetValue`.
#[repr(C)]
struct PropVariantRaw {
    vt: u16,
    _reserved: [u16; 3],
    pwsz_val: *mut u16,
    _rest: [usize; 2],
}

/// AUMID jendela dari `IPropertyStore` (`PK_AUMID`), dengan pembebasan seperti `PropVariantClear` di C.
/// Hanya `VT_LPWSTR` yang dibaca lalu dibebaskan dengan `CoTaskMemFree`; AUMID selalu bertipe itu.
pub(crate) fn property_store_aumid(ps: &IPropertyStore) -> String {
    let mut pv = PropVariantRaw {
        vt: 0,
        _reserved: [0; 3],
        pwsz_val: std::ptr::null_mut(),
        _rest: [0; 2],
    };
    let this = ps.as_raw();
    // `this` menunjuk objek COM hidup milik `ps`; slot 5 vtbl adalah `GetValue`.
    let hr = unsafe {
        let vtbl = *(this as *const *const PropertyStoreVtbl);
        ((*vtbl).get_value)(this, &PK_AUMID, (&mut pv as *mut PropVariantRaw).cast())
    };
    let mut out = String::new();
    if hr.is_ok() && pv.vt == VT_LPWSTR.0 && !pv.pwsz_val.is_null() {
        out = copy_wide(&unsafe { from_pcwstr(pv.pwsz_val as *const u16) }, 256);
    }
    if pv.vt == VT_LPWSTR.0 && !pv.pwsz_val.is_null() {
        unsafe { CoTaskMemFree(Some(pv.pwsz_val as *const c_void)) };
    }
    out
}

/// Isi AUMID dan exe jendela `w` (`ResolveWindow`). `retry` = jendela UWP yang CoreWindow-nya belum ada.
fn resolve_window(w: &mut WinInfo) {
    w.aumid.clear();
    w.exe.clear();
    w.retry = false;
    let mut aumid = String::new();
    if let Ok(ps) = unsafe { SHGetPropertyStoreForWindow::<IPropertyStore>(w.h) } {
        aumid = property_store_aumid(&ps);
    }
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(w.h, Some(&mut pid as *mut u32)) };
    if window_class(w.h) == "ApplicationFrameWindow" {
        // UWP: proses sebenarnya ada pada jendela anak CoreWindow.
        let core = to_wide_z("Windows.UI.Core.CoreWindow");
        match unsafe { FindWindowExW(Some(w.h), None, pcw(&core), PCWSTR::null()) } {
            Ok(c) if !c.0.is_null() => {
                unsafe { GetWindowThreadProcessId(c, Some(&mut pid as *mut u32)) };
            }
            _ => w.retry = true,
        }
    }
    let mut exe = String::new();
    if let Ok(hproc) = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        let mut buf = [0u16; MAX_PATH];
        let mut n = MAX_PATH as u32;
        let ok = unsafe {
            QueryFullProcessImageNameW(hproc, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n)
        };
        if ok.is_ok() {
            exe = from_wide(&buf);
        }
        if aumid.is_empty() {
            if let Some(get_aumid) = PFN_GET_APP_USER_MODEL_ID.get() {
                let mut len: u32 = 256;
                let mut ab = [0u16; 256];
                if unsafe { get_aumid(hproc, &mut len, PWSTR(ab.as_mut_ptr())) } == 0 {
                    aumid = from_wide(&ab);
                }
            }
        }
        let _ = unsafe { CloseHandle(hproc) };
    }
    w.aumid = aumid;
    w.exe = lower(&exe);
    if !w.aumid.is_empty() {
        w.retry = false;
    }
}

/// Identitas jendela `h` dari cache `WC`; entri baru dibuat bila belum ada (`WindowIdentity`).
/// Mengembalikan salinan, sehingga tidak ada borrow `WC` yang ditahan saat memanggil Win32.
pub fn window_identity(h: HWND) -> Option<WinInfo> {
    let found = WC.borrow().iter().position(|w| w.h == h);
    if let Some(i) = found {
        let mut info = {
            let mut list = WC.borrow_mut();
            list[i].seen = true;
            list[i].clone()
        };
        if info.retry {
            resolve_window(&mut info);
            if let Some(slot) = WC.borrow_mut().get_mut(i) {
                *slot = info.clone();
            }
        }
        return Some(info);
    }
    if WC.borrow().len() >= MAXWC as usize {
        return None;
    }
    let mut info = WinInfo {
        h,
        aumid: String::new(),
        exe: String::new(),
        seen: true,
        retry: false,
    };
    resolve_window(&mut info);
    WC.borrow_mut().push(info.clone());
    Some(info)
}

// ---------------------------------------------------------------- pencocokan & aplikasi berjalan (C 852–918)

/// Indeks aplikasi di `APPS` yang cocok dengan jendela `w` (`MatchApp`), atau `None`.
/// Urutan: AUMID persis, lalu AUMID berawalan "X." (mis. "MSEdge" vs "MSEdge.Profile"), lalu nama exe.
pub fn match_app(w: &WinInfo) -> Option<usize> {
    let apps = APPS.borrow();
    if !w.aumid.is_empty() {
        if let Some(i) = apps
            .iter()
            .position(|a| !a.aumid.is_empty() && a.aumid.eq_ignore_ascii_case(&w.aumid))
        {
            return Some(i);
        }
        if let Some(i) = apps.iter().position(|a| {
            let n = a.aumid.len();
            n > 0
                && w.aumid.get(..n).is_some_and(|p| p.eq_ignore_ascii_case(&a.aumid))
                && w.aumid.as_bytes().get(n) == Some(&b'.')
        }) {
            return Some(i);
        }
    }
    if !w.exe.is_empty() {
        if let Some(i) = apps.iter().position(|a| {
            !a.exe.is_empty() && a.exe == w.exe && (w.aumid.is_empty() || a.aumid.is_empty())
        }) {
            return Some(i);
        }
    }
    None
}

/// Tambah aplikasi baru untuk jendela `h` dari identitas `w` (`AddRunningApp`).
/// Mengembalikan indeks baru, atau `None` bila `APPS` penuh.
pub fn add_running_app(w: &WinInfo, h: HWND) -> Option<usize> {
    if APPS.borrow().len() >= MAXAPPS as usize {
        return None;
    }
    // Entri dibangun lokal: tidak ada borrow APPS yang ditahan saat memanggil Win32/COM.
    let mut a = App {
        pin: -1,
        ..App::default()
    };
    a.aumid = copy_wide(&w.aumid, 256);
    a.exe = copy_wide(&w.exe, MAX_PATH);
    if !a.aumid.is_empty() {
        let pn = copy_wide(&format!("shell:AppsFolder\\{}", a.aumid), 600);
        let item: Option<IShellItem> =
            unsafe { SHCreateItemFromParsingName(pcw(&to_wide_z(&pn)), None::<&IBindCtx>) }.ok();
        if let Some(si) = item {
            app_from_shell_item(&si, &mut a);
            if a.icon.src.is_some() {
                a.launch = pn.clone();
                a.icon_name = pn;
            }
        }
    }
    let afh = a.exe.contains("applicationframehost.exe");
    if a.icon.src.is_none() && !a.exe.is_empty() && !afh {
        a.icon.src = icon_from_parsing_name(&a.exe, src_px());
        if a.icon.src.is_some() {
            a.icon_name = a.exe.clone();
        }
    }
    if a.name.is_empty() && !a.exe.is_empty() && !afh {
        match file_description(&a.exe, 128) {
            Some(d) => a.name = d,
            None => {
                let base = copy_wide(path_find_file_name(&a.exe), 128);
                a.name = path_remove_extension(&base);
            }
        }
    }
    if a.icon.src.is_none() {
        let hic = send_geticon(h, ICON_BIG)
            .or_else(|| class_hicon(h))
            .or_else(|| send_geticon(h, ICON_SMALL2));
        a.icon.src = hic.and_then(|hi| icon_from_hicon(hi, src_px()));
    }
    if a.name.is_empty() {
        let mut buf = [0u16; 128];
        // SAFETY: buf hidup selama pemanggilan; GetWindowTextW menulis paling banyak 128 unit.
        unsafe { GetWindowTextW(h, &mut buf) };
        a.name = from_wide(&buf);
    }
    if a.launch.is_empty() && !afh {
        a.launch = copy_wide(&a.exe, 600);
    }
    let mut apps = APPS.borrow_mut();
    if apps.len() >= MAXAPPS as usize {
        return None;
    }
    apps.push(a);
    Some(apps.len() - 1)
}

/// Ikon jendela lewat `WM_GETICON` dengan batas 100 ms; `None` bila gagal atau bernilai 0.
fn send_geticon(h: HWND, which: u32) -> Option<HICON> {
    let mut r: usize = 0;
    // SAFETY: r hidup selama pemanggilan; SMTO_ABORTIFHUNG membatasi tunggu pada jendela yang hang.
    let ok = unsafe {
        SendMessageTimeoutW(
            h,
            WM_GETICON,
            WPARAM(which as usize),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            100,
            Some(&mut r as *mut usize),
        )
    };
    if ok.0 != 0 && r != 0 {
        Some(HICON(r as *mut c_void))
    } else {
        None
    }
}

/// Ikon kelas jendela (`GetClassLongPtrW(GCLP_HICON)`); `None` bila 0.
fn class_hicon(h: HWND) -> Option<HICON> {
    let v = unsafe { GetClassLongPtrW(h, GCLP_HICON) };
    if v == 0 {
        None
    } else {
        Some(HICON(v as *mut c_void))
    }
}

/// Ikon jendela `h` untuk Alt+Tab (`px` piksel): ICON_BIG, lalu ikon kelas, lalu ICON_SMALL2. HICON dari
/// `WM_GETICON`/kelas tidak dimiliki kita, jadi tidak di-DestroyIcon. None bila semuanya gagal.
pub fn window_icon_src(h: HWND, px: i32) -> Option<IWICBitmapSource> {
    let hi = send_geticon(h, ICON_BIG)
        .or_else(|| class_hicon(h))
        .or_else(|| send_geticon(h, ICON_SMALL2))?;
    icon_from_hicon(hi, px)
}

/// Muat ulang ikon aplikasi yang punya `icon_name` dan ikon catatan (`ReloadIcons`).
pub fn reload_icons() {
    // Ambil daftar nama dulu agar tidak ada borrow APPS saat membuat ikon.
    let names: Vec<(usize, String)> = APPS
        .borrow()
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.icon_name.is_empty())
        .map(|(i, a)| (i, a.icon_name.clone()))
        .collect();
    for (i, name) in names {
        let src = icon_from_parsing_name(&name, src_px());
        let mut apps = APPS.borrow_mut();
        if let Some(a) = apps.get_mut(i) {
            icon_free(&mut a.icon);
            a.icon.src = src;
        }
    }
    let note = icon_from_parsing_name(NOTE_APP, icon_size());
    let mut ic = NOTE_ICON.borrow_mut();
    icon_free(&mut ic);
    ic.src = note;
}

// ---------------------------------------------------------------- volume (C 920–978)

/// Callback perubahan volume (C `VolCb`, 926–940). Refcount dikelola `#[implement]`;
/// C memakai nilai palsu (AddRef 2, Release 1).
#[implement(IAudioEndpointVolumeCallback)]
struct VolCb;

impl IAudioEndpointVolumeCallback_Impl for VolCb_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if pnotify.is_null() {
            return Ok(());
        }
        // SAFETY: pointer dari audio service; sudah dicek null, dibaca sekali.
        let (muted, vol) = unsafe { ((*pnotify).bMuted.0 != 0, (*pnotify).fMasterVolume) };
        let msg = MSG.get();
        if !msg.0.is_null() {
            // C 937: PostMessageW(g_msg, WM_VOLCHG, muted ? 1 : 0, lroundf(fMasterVolume * 100.0f))
            let _ = unsafe {
                PostMessageW(
                    Some(msg),
                    WM_VOLCHG,
                    WPARAM(usize::from(muted)),
                    LPARAM((vol * 100.0).round() as isize),
                )
            };
        }
        Ok(())
    }
}

/// `g_devId` (C 944): id endpoint render default yang terpasang. Kosong = belum terpasang.
pub static DEV_ID: SyncCell<RefCell<String>> = SyncCell::new(RefCell::new(String::new()));

/// `g_volCb` (C 941): objek callback. Objek yang sama dipakai untuk Register dan Unregister.
pub static VOL_CB: SyncCell<RefCell<Option<IAudioEndpointVolumeCallback>>> =
    SyncCell::new(RefCell::new(None));

/// Ambil callback volume; dibuat sekali pada pemanggilan pertama (pengganti `static VolCb g_volCb`).
fn volume_callback() -> IAudioEndpointVolumeCallback {
    let mut slot = VOL_CB.borrow_mut();
    slot.get_or_insert_with(|| VolCb.into()).clone()
}

/// Salin string wide ber-NUL dari memori COM ke `String` (NUL tidak ikut). Pointer null → kosong.
///
/// # Safety
/// `p` harus null atau menunjuk string wide ber-NUL yang valid.
unsafe fn com_wide_string(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut units: Vec<u16> = Vec::new();
    let mut i = 0usize;
    while *p.add(i) != 0 {
        units.push(*p.add(i));
        i += 1;
    }
    String::from_utf16_lossy(&units)
}


/// `VolumeDetach` (C 948–952): lepas callback dari endpoint lama, lalu lepas endpoint dan kosongkan id.
/// `VOL` dan `MUTE` tidak diubah, sama seperti C.
pub fn volume_detach() {
    let ep = EP_VOL.borrow_mut().take();
    if let Some(ep) = ep {
        let cb = VOL_CB.borrow().clone();
        if let Some(cb) = cb {
            // SAFETY: `ep` dan `cb` objek COM yang hidup; pemanggilan sinkron.
            let _ = unsafe { ep.UnregisterControlChangeNotify(&cb) };
        }
        // Saat `ep` keluar dari scope = SafeRelease (C 950).
    }
    DEV_ID.borrow_mut().clear();
}

/// `VolumeAttach` (C 954–978): pasang endpoint render default, daftarkan callback, baca volume dan mute.
pub fn volume_attach() {
    let existing = DEV_ENUM.borrow().clone();
    let enumerator = match existing {
        Some(e) => e,
        None => {
            // C 956–959: gagal membuat enumerator → keluar tanpa mengubah state lain.
            let created: windows::core::Result<IMMDeviceEnumerator> = unsafe {
                CoCreateInstance(&MMDeviceEnumerator, None::<&IUnknown>, CLSCTX_INPROC_SERVER)
            };
            match created {
                Ok(e) => {
                    *DEV_ENUM.borrow_mut() = Some(e.clone());
                    e
                }
                Err(_) => return,
            }
        }
    };

    // C 961: GetDefaultAudioEndpoint(eRender, eConsole) gagal → lepas dan VOL = -1.
    let dev: IMMDevice = match unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) } {
        Ok(d) => d,
        Err(_) => {
            volume_detach();
            VOL.set(-1);
            return;
        }
    };

    // C 962–964: GetId; id sama dan endpoint masih terpasang → tidak ada yang diubah.
    // Id null (GetId gagal) diperlakukan sebagai None, bukan string kosong.
    let id: Option<String> = match unsafe { dev.GetId() } {
        Ok(p) if !p.0.is_null() => {
            // SAFETY: GetId mengembalikan string wide ber-NUL dari CoTaskMemAlloc.
            let s = unsafe { com_wide_string(p.0) };
            co_free(p.0);
            Some(s)
        }
        _ => None,
    };
    if let Some(idv) = id.as_deref() {
        if EP_VOL.borrow().is_some() && DEV_ID.borrow().as_str() == idv {
            return;
        }
    }

    // C 965–967: VolumeDetach; Copy(g_devId, id, 256).
    volume_detach();
    *DEV_ID.borrow_mut() = copy_wide(id.as_deref().unwrap_or(""), 256);

    // C 968–976: Activate; bila sukses, daftarkan callback lalu baca volume dan mute.
    // SAFETY: `dev` adalah IMMDevice yang valid selama fungsi ini berjalan.
    if let Ok(ep) = unsafe { dev.Activate::<IAudioEndpointVolume>(CLSCTX_INPROC_SERVER, None) } {
        *EP_VOL.borrow_mut() = Some(ep.clone());
        let cb = volume_callback();
        // C 969: hasil Register diabaikan, seperti C.
        let _ = unsafe { ep.RegisterControlChangeNotify(&cb) };
        // C 970–975: nilai default 0 dan FALSE bila pembacaan gagal, seperti C.
        let f = unsafe { ep.GetMasterVolumeLevelScalar() }.unwrap_or(0.0);
        let m = unsafe { ep.GetMute() }.unwrap_or(BOOL(0));
        VOL.set((f * 100.0).round() as i32);
        MUTE.set(m.0 != 0);
    }
    // C 977: dev->Release() terjadi saat `dev` keluar dari scope.
}

// ---------------------------------------------------------------------------
// Jaringan (C 979–1064): status LAN dan Wi-Fi lewat NotifyIpInterfaceChange.
// Deviasi: HeapAlloc diganti Vec<u64> (alignment 8) + try_reserve_exact; sin_addr dibaca byte demi byte seperti C.
// ---------------------------------------------------------------------------

/// FFI iphlpapi lokal (deviasi): fitur Cargo `Win32_Networking_WinSock` dan `Win32_NetworkManagement_Ndis`
/// kini aktif, tetapi deklarasi lokal dipertahankan. Layout meniru SDK, sumber windows-0.61.3: IpHelper/mod.rs:1798 dan :2177,
/// WinSock/mod.rs:5222, :5253, :5450, Ndis/mod.rs:281. Hanya field yang dibaca `net_update` yang dideklarasikan.
mod iph {
    #![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code)]
    use std::ffi::c_void;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::IpHelper::GET_ADAPTERS_ADDRESSES_FLAGS;

    pub const AF_UNSPEC: u16 = 0;
    pub const AF_INET: u16 = 2;
    /// IfOperStatusUp = 1.
    pub const IfOperStatusUp: i32 = 1;

    /// Setara PWSTR (dibaca lewat `.0`).
    #[repr(transparent)]
    #[derive(Clone, Copy)]
    pub struct PWSTR(pub *mut u16);

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SOCKADDR {
        pub sa_family: u16,
        pub sa_data: [u8; 14],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SOCKADDR_IN {
        pub sin_family: u16,
        pub sin_port: u16,
        pub sin_addr: [u8; 4],
        pub sin_zero: [u8; 8],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SOCKET_ADDRESS {
        pub lpSockaddr: *mut SOCKADDR,
        pub iSockaddrLength: i32,
    }

    /// Awal IP_ADAPTER_UNICAST_ADDRESS_LH (offset: Next 8, Address 16).
    #[repr(C)]
    pub struct IP_ADAPTER_UNICAST_ADDRESS_LH {
        pub Length: u32,
        pub Flags: u32,
        pub Next: *mut IP_ADAPTER_UNICAST_ADDRESS_LH,
        pub Address: SOCKET_ADDRESS,
    }

    /// Awal IP_ADAPTER_ADDRESSES_LH sampai FirstGatewayAddress (offset 208). Urutan field sama dengan SDK.
    #[repr(C)]
    pub struct IP_ADAPTER_ADDRESSES_LH {
        pub Length: u32,
        pub IfIndex: u32,
        pub Next: *mut IP_ADAPTER_ADDRESSES_LH,
        pub AdapterName: *mut u8,
        pub FirstUnicastAddress: *mut IP_ADAPTER_UNICAST_ADDRESS_LH,
        pub FirstAnycastAddress: *mut c_void,
        pub FirstMulticastAddress: *mut c_void,
        pub FirstDnsServerAddress: *mut c_void,
        pub DnsSuffix: PWSTR,
        pub Description: PWSTR,
        pub FriendlyName: PWSTR,
        pub PhysicalAddress: [u8; 8],
        pub PhysicalAddressLength: u32,
        pub Flags: u32,
        pub Mtu: u32,
        pub IfType: u32,
        pub OperStatus: i32,
        pub Ipv6IfIndex: u32,
        pub ZoneIndices: [u32; 16],
        pub FirstPrefix: *mut c_void,
        pub TransmitLinkSpeed: u64,
        pub ReceiveLinkSpeed: u64,
        pub FirstWinsServerAddress: *mut c_void,
        pub FirstGatewayAddress: *mut c_void,
    }

    #[link(name = "iphlpapi")]
    unsafe extern "system" {
        /// Mengembalikan 0 (NO_ERROR) atau 111 (ERROR_BUFFER_OVERFLOW). Sama dengan GetAdaptersAddresses.
        pub fn GetAdaptersAddresses(
            family: u32,
            flags: GET_ADAPTERS_ADDRESSES_FLAGS,
            reserved: *mut c_void,
            addresses: *mut IP_ADAPTER_ADDRESSES_LH,
            size: *mut u32,
        ) -> u32;
        /// Sama dengan NotifyIpInterfaceChange (callback: PIPINTERFACE_CHANGE_CALLBACK).
        pub fn NotifyIpInterfaceChange(
            family: u16,
            callback: Option<unsafe extern "system" fn(*const c_void, *const c_void, i32)>,
            context: *const c_void,
            initial: u8,
            handle: *mut HANDLE,
        ) -> u32;
    }
}

/// Antrean Wi-Fi: 100 bila ada adaptor Wi-Fi, 0 bila tidak (C: g_wifiQ, 981). Dibaca C2.
pub static WIFI_Q: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
/// Adaptor Wi-Fi aktif dengan gateway (C: g_netWifi, 982). Tidak ada padanan di state.rs; dimiliki C1.
pub static NET_WIFI_OK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));

/// Callback NotifyIpInterfaceChange: kirim WM_NETCHG ke jendela utama (C: NetChanged, 991–994).
extern "system" fn net_changed(
    _ctx: *const c_void,
    _row: *const c_void,
    _kind: i32,
) {
    let h = MSG.get();
    if !h.0.is_null() {
        let _ = unsafe { PostMessageW(Some(h), WM_NETCHG, WPARAM(0), LPARAM(0)) };
    }
}

/// Daftarkan notifikasi perubahan IP (C: NetInit, 996–999). Hasil tidak diperiksa, seperti C.
pub fn net_init() {
    // Handle ditulis ke variabel lokal lalu disimpan, karena NotifyIpInterfaceChange butuh *mut HANDLE.
    let mut h = HANDLE(std::ptr::null_mut());
    // PORT: deklarasi lokal iph memakai context *const c_void (bukan Option) dan initial u8 (bukan bool).
    let _ = unsafe {
        NotifyIpInterfaceChange(AF_UNSPEC, Some(net_changed), std::ptr::null(), 0, &mut h)
    };
    NET_NOTIFY.set(h);
}

/// Baca adaptor jaringan dan perbarui status (C: NetUpdate, 1001–1064). Mengembalikan true bila ada perubahan.
/// ethDesc tidak ikut dibandingkan, sama seperti C.
pub fn net_update() -> bool {
    let mut has_eth = false;
    let mut has_wifi = false;
    let mut wifi_name = String::new();
    let mut eth_name = String::new();
    let mut eth_desc = String::new();
    let mut eth_ip = String::new();
    let mut eth_speed: u32 = 0;
    let mut sz: u32 = 16384;
    for _ in 0..2 {
        // Buffer adaptor: Vec<u64> menjamin alignment 8. Gagal alokasi = break, seperti `if (!buf) break;`.
        let words = (sz as usize + 7) / 8;
        let mut buf: Vec<u64> = Vec::new();
        if buf.try_reserve_exact(words).is_err() {
            break;
        }
        buf.resize(words, 0);
        let head = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        let flags = GET_ADAPTERS_ADDRESSES_FLAGS(
            GAA_FLAG_INCLUDE_GATEWAYS.0
                | GAA_FLAG_SKIP_ANYCAST.0
                | GAA_FLAG_SKIP_MULTICAST.0
                | GAA_FLAG_SKIP_DNS_SERVER.0,
        );
        let r = unsafe { GetAdaptersAddresses(AF_UNSPEC as u32, flags, std::ptr::null_mut(), head, &mut sz) };
        if r == NO_ERROR.0 {
            let mut a = head;
            while !a.is_null() {
                let ad = unsafe { &*a };
                a = ad.Next;
                if ad.OperStatus != IfOperStatusUp || ad.FirstGatewayAddress.is_null() {
                    continue;
                }
                if ad.IfType == IF_TYPE_ETHERNET_CSMACD {
                    has_eth = true;
                    if eth_name.is_empty() {
                        eth_name = copy_wide(&unsafe { from_pcwstr(ad.FriendlyName.0) }, 64);
                    }
                    if eth_desc.is_empty() {
                        eth_desc = copy_wide(&unsafe { from_pcwstr(ad.Description.0) }, 128);
                    }
                    if eth_speed == 0 && ad.TransmitLinkSpeed > 0 {
                        eth_speed = (ad.TransmitLinkSpeed / 1_000_000) as u32;
                    }
                    if eth_ip.is_empty() {
                        let mut u = ad.FirstUnicastAddress;
                        while !u.is_null() {
                            let ua = unsafe { &*u };
                            u = ua.Next;
                            let sa = ua.Address.lpSockaddr;
                            if !sa.is_null() && unsafe { (*sa).sa_family } == AF_INET {
                                let sin = sa as *const SOCKADDR_IN;
                                let base = unsafe { std::ptr::addr_of!((*sin).sin_addr) }.cast::<u8>();
                                let b = unsafe { std::slice::from_raw_parts(base, 4) };
                                eth_ip = copy_wide(
                                    &format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3]),
                                    48,
                                );
                                break;
                            }
                        }
                    }
                } else if ad.IfType == IF_TYPE_IEEE80211 {
                    has_wifi = true;
                    if wifi_name.is_empty() {
                        wifi_name = copy_wide(&unsafe { from_pcwstr(ad.FriendlyName.0) }, 64);
                    }
                }
            }
        }
        if r != ERROR_BUFFER_OVERFLOW.0 {
            break;
        }
    }

    let net = if has_eth && has_wifi {
        NET_BOTH
    } else if has_eth {
        NET_ETH
    } else if has_wifi {
        NET_WIFI
    } else {
        NET_NONE
    };
    let ch = net != NET.get()
        || has_eth != NET_ETH_OK.get()
        || has_wifi != NET_WIFI_OK.get()
        || wifi_name != *SSID.borrow()
        || eth_name != *ETH_NAME.borrow()
        || eth_ip != *ETH_IP.borrow()
        || eth_speed != ETH_SPEED.get();

    NET.set(net);
    NET_ETH_OK.set(has_eth);
    NET_WIFI_OK.set(has_wifi);
    WIFI_Q.set(if has_wifi { 100 } else { 0 });
    *SSID.borrow_mut() = wifi_name;
    *ETH_NAME.borrow_mut() = eth_name;
    *ETH_DESC.borrow_mut() = eth_desc;
    *ETH_IP.borrow_mut() = eth_ip;
    ETH_SPEED.set(eth_speed);
    ch
}

// ---------------------------------------------------------------- Baterai dan jam (C 1066–1110)

/// Ambang peringatan baterai terakhir (`g_batArmed`, awal 100). Diisi `w12_battery_alert`.
pub static BAT_ARMED: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(100));

// BatHistNote: sekarang di flyout.rs (diimpor di bawah); stub U3 dihapus.

/// Membaca status daya (C `BatteryUpdate`). Mengembalikan true bila ada perubahan.
pub fn battery_update() -> bool {
    let mut ps = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut ps) }.is_err() {
        return false;
    }
    let has = (ps.BatteryFlag & 128) == 0 && ps.BatteryFlag != 255;
    let lvl: i32 = if ps.BatteryLifePercent == 255 { -1 } else { ps.BatteryLifePercent as i32 };
    let ch = ps.ACLineStatus == 1;
    if has && lvl >= 0 {
        bat_hist_note(lvl, (ps.BatteryFlag & 8) != 0);
    }
    // Ambang hanya dianggap terpakai bila notch benar-benar tampil; bila tidak, kembalikan supaya tertunda.
    let armed_before = BAT_ARMED.get();
    let mut armed = armed_before;
    let fired = w12_battery_alert(if has { lvl } else { -1 }, ch, Some(&mut armed)) != 0;
    if fired && !super::notch::notch_battery_alert(lvl) {
        armed = armed_before;
    }
    BAT_ARMED.set(armed);
    let changed = has != HAS_BAT.get() || lvl != BAT.get() || ch != CHARGING.get();
    // adaptor dipasang/dicabut (bukan pembacaan pertama): pulau menampilkan pesan singkat
    let plug = has && HAS_BAT.get() && ch != CHARGING.get();
    HAS_BAT.set(has);
    BAT.set(lvl);
    CHARGING.set(ch);
    if plug {
        super::notch::notch_plug(ch, lvl);
    }
    changed
}

/// Struktur TIME_ZONE_INFORMATION. Fitur `Win32_System_Time` sudah aktif di Cargo.toml, tetapi struktur ini tetap dideklarasikan sendiri (deviasi FFI).
#[repr(C)]
#[allow(dead_code)] // field nama dan tanggal ada hanya agar tata letaknya sama dengan Win32
#[derive(Default)]
struct TzInfo {
    bias: i32,
    standard_name: [u16; 32],
    standard_date: SYSTEMTIME,
    standard_bias: i32,
    daylight_name: [u16; 32],
    daylight_date: SYSTEMTIME,
    daylight_bias: i32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetTimeZoneInformation(lp_time_zone_information: *mut TzInfo) -> u32;
}

const TIME_ZONE_ID_DAYLIGHT: u32 = 2;

/// Teks tanggal dari GetDateFormatEx untuk waktu `st`. None bila gagal (C `!GetDateFormatEx`).
fn date_text(locale: PCWSTR, flags: ENUM_DATE_FORMATS_FLAGS, st: &SYSTEMTIME, fmt: PCWSTR, cch: usize) -> Option<String> {
    let mut buf = vec![0u16; cch];
    let n = unsafe {
        GetDateFormatEx(locale, flags, Some(st as *const SYSTEMTIME), fmt, Some(&mut buf[..]), PCWSTR::null())
    };
    if n <= 0 {
        return None;
    }
    let len = (n as usize).saturating_sub(1).min(cch);
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Jam, zona, dan tanggal (C `ClockUpdate`). Hasil ditulis ke TIME, TZ, TIME_TZ, DATE, dan DATE_SHORT.
pub fn clock_update() {
    let st = unsafe { GetLocalTime() };
    let time = format!("{:02}.{:02}", st.wHour, st.wMinute);
    let mut tzi = TzInfo::default();
    let r = unsafe { GetTimeZoneInformation(&mut tzi) };
    let bias = tzi.bias + if r == TIME_ZONE_ID_DAYLIGHT { tzi.daylight_bias } else { tzi.standard_bias };
    let off = -bias; // menit di depan UTC
    let tz = match off {
        420 => "WIB".to_string(),
        480 => "WITA".to_string(),
        540 => "WIT".to_string(),
        _ if off % 60 == 0 => format!("GMT{:+}", off / 60),
        _ => format!("GMT{:+}:{:02}", off / 60, (off % 60).abs()),
    };
    let time_tz = format!("{} {}", time, tz);
    if let Some(d) = date_text(w!("id-ID"), ENUM_DATE_FORMATS_FLAGS(0), &st, w!("dddd, d MMMM yyyy"), 64)
        .or_else(|| date_text(PCWSTR::null(), DATE_LONGDATE, &st, PCWSTR::null(), 64))
    {
        *DATE.borrow_mut() = d;
    }
    if let Some(d) = date_text(w!("id-ID"), ENUM_DATE_FORMATS_FLAGS(0), &st, w!("ddd, d MMM"), 48)
        .or_else(|| date_text(PCWSTR::null(), ENUM_DATE_FORMATS_FLAGS(0), &st, w!("ddd, d MMM"), 48))
    {
        *DATE_SHORT.borrow_mut() = d;
    }
    *TIME.borrow_mut() = time;
    *TZ.borrow_mut() = tz;
    *TIME_TZ.borrow_mut() = time_tz;
}

// ---------------------------------------------------------------- Media (C 1218–1282)

/// Jenis kejadian media (C `MEV_*`), dipakai sebagai bit di `MEDIA_EV_MASK`.
pub const MEV_SESSION: i32 = 1;
pub const MEV_PLAY: i32 = 2;
pub const MEV_PROPS: i32 = 4;

/// Status media yang terakhir dibaca dari GSMTC (C `struct MediaState`).
pub struct MediaState {
    pub has: bool,
    pub status: i32,
    pub title: [u16; 200],
    pub artist: [u16; 200],
    pub src: [u16; 256],
    pub art: Icon,
}

impl MediaState {
    pub const fn new() -> Self {
        Self {
            has: false,
            status: 0,
            title: [0; 200],
            artist: [0; 200],
            src: [0; 256],
            art: Icon::new(),
        }
    }
}

/// Status media bersama (C `g_media`), dibaca dan diisi oleh C2.
pub static MEDIA: SyncCell<RefCell<MediaState>> = SyncCell::new(RefCell::new(MediaState::new()));

/// Bit kejadian yang menunggu diproses (C `g_mediaEvMask`), diisi dari thread event GSMTC.
static MEDIA_EV_MASK: AtomicI32 = AtomicI32::new(0);
/// Sesi manager sudah berganti (C `g_mediaSessDirty`).
static MEDIA_SESS_DIRTY: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
/// Sesi yang event-nya sedang dipasang (C `g_sessEv`).
static SESS_EV: SyncCell<RefCell<Option<GlobalSystemMediaTransportControlsSession>>> =
    SyncCell::new(RefCell::new(None));
/// Token PlaybackInfoChanged dan MediaPropertiesChanged (C `g_tokPlay`, `g_tokProps`).
static TOK_PLAY: SyncCell<Cell<i64>> = SyncCell::new(Cell::new(0));
static TOK_PROPS: SyncCell<Cell<i64>> = SyncCell::new(Cell::new(0));

/// Menandai kejadian dari thread event (C `MediaEvHandler::Invoke`). Pesan hanya dikirim bila mask sebelumnya kosong.
fn media_ev_signal(kind: i32) {
    let old = MEDIA_EV_MASK.fetch_or(kind, Ordering::SeqCst);
    if old == 0 {
        let _ = unsafe { PostMessageW(Some(MSG.get()), WM_W12_MEDIAEV, WPARAM(0), LPARAM(0)) };
    }
}

/// Handler PlaybackInfoChanged (C `MediaEvHandler`, kind MEV_PLAY).
fn play_handler() -> TypedEventHandler<GlobalSystemMediaTransportControlsSession, PlaybackInfoChangedEventArgs> {
    TypedEventHandler::new(|_, _| {
        media_ev_signal(MEV_PLAY);
        Ok(())
    })
}

/// Handler MediaPropertiesChanged (C `MediaEvHandler`, kind MEV_PROPS).
fn props_handler() -> TypedEventHandler<GlobalSystemMediaTransportControlsSession, MediaPropertiesChangedEventArgs> {
    TypedEventHandler::new(|_, _| {
        media_ev_signal(MEV_PROPS);
        Ok(())
    })
}

/// Membandingkan dua objek COM (C `SameObject`) lewat pointer IUnknown.
fn same_object(a: &GlobalSystemMediaTransportControlsSession, b: &GlobalSystemMediaTransportControlsSession) -> bool {
    let (Ok(ua), Ok(ub)) = (a.cast::<IUnknown>(), b.cast::<IUnknown>()) else {
        return false;
    };
    ua.as_raw() == ub.as_raw()
}

/// Melepas event sesi yang sedang dipasang (C `MediaUnhook`).
pub fn media_unhook() {
    let old = SESS_EV.borrow_mut().take();
    if let Some(s) = old {
        let _ = s.RemovePlaybackInfoChanged(TOK_PLAY.get());
        let _ = s.RemoveMediaPropertiesChanged(TOK_PROPS.get());
    }
}

/// Jatuh ke polling 3 detik bila event tidak bisa dipasang (C `MediaPollFallback`).
fn media_poll_fallback() {
    MEDIA_EV.set(false);
    set_timer(T_MEDIA, 3000);
}

/// Memasang event pada sesi `s` (C `MediaHook`). Sesi yang sama tidak dipasang ulang kecuali ada tanda sesi berganti.
pub fn media_hook(s: &GlobalSystemMediaTransportControlsSession) {
    if !MEDIA_EV.get() {
        return;
    }
    let has_cur = SESS_EV.borrow().is_some();
    let same = match SESS_EV.borrow().as_ref() {
        Some(c) => same_object(c, s),
        None => false,
    };
    if has_cur && (!MEDIA_SESS_DIRTY.get() || same) {
        MEDIA_SESS_DIRTY.set(false);
        return;
    }
    MEDIA_SESS_DIRTY.set(false);
    media_unhook();
    let tok_play = match s.PlaybackInfoChanged(&play_handler()) {
        Ok(t) => t,
        Err(_) => {
            media_poll_fallback();
            return;
        }
    };
    let tok_props = match s.MediaPropertiesChanged(&props_handler()) {
        Ok(t) => t,
        Err(_) => {
            let _ = s.RemovePlaybackInfoChanged(tok_play);
            media_poll_fallback();
            return;
        }
    };
    TOK_PLAY.set(tok_play);
    TOK_PROPS.set(tok_props);
    *SESS_EV.borrow_mut() = Some(s.clone());
}

/// Memproses kejadian media tertunda di thread UI (C `MediaEvent`, dipanggil dari WM_W12_MEDIAEV).
pub fn media_event() {
    let k = MEDIA_EV_MASK.swap(0, Ordering::SeqCst);
    if (k & MEV_SESSION) != 0 {
        MEDIA_SESS_DIRTY.set(true);
    }
    if (k & (MEV_SESSION | MEV_PROPS)) != 0 {
        MEDIA_WANT.set(true);
        if MEDIA_EV.get() {
            set_timer(T_MEDIA, 1500);
        }
    }
    set_timer(T_MEDIA_FAST, 60);
}

// ---------------------------------------------------------------- Async, HSTRING, thumbnail, dan gambar (C 1284–1328)

/// Pointer fungsi vtable WinRT dengan satu argumen keluaran (`get_Thumbnail`, `OpenReadAsync`, `GetResults`).
type RawOutFn = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;

/// Mengambil pointer fungsi slot ke-`idx` dari vtable objek COM `this` (slot 0–5 = IUnknown + IInspectable).
unsafe fn vtbl_slot(this: *mut c_void, idx: usize) -> *const c_void {
    let vtbl = *(this as *const *const *const c_void);
    *vtbl.add(idx)
}

/// Status operasi async (C `AsyncStatus`): 0 berjalan, 1 selesai, 2 batal, 3 gagal (default 3).
/// Tipe `IAsyncInfo` tidak diekspor crate `windows` 0.61 (ada di windows-future), jadi memakai QueryInterface mentah
/// (IID dari windows-future bindings.rs:1479) lalu slot 7 `get_Status` (urutan slot: IInspectable 0–5, Id 6, Status 7).
pub fn async_status<T: Interface>(op: &T) -> i32 {
    type QueryFn = unsafe extern "system" fn(*mut c_void, *const windows::core::GUID, *mut *mut c_void) -> HRESULT;
    type StatusFn = unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT;
    const IID_IASYNCINFO: windows::core::GUID = windows::core::GUID::from_u128(0x00000036_0000_0000_c000_000000000046);
    let mut info: *mut c_void = std::ptr::null_mut();
    unsafe {
        let query: QueryFn = std::mem::transmute(vtbl_slot(op.as_raw(), 0));
        if query(op.as_raw(), &IID_IASYNCINFO, &mut info).is_err() || info.is_null() {
            return 3;
        }
        let info = IUnknown::from_raw(info);
        let get_status: StatusFn = std::mem::transmute(vtbl_slot(info.as_raw(), 7));
        let mut status: i32 = 3;
        if get_status(info.as_raw(), &mut status).is_err() {
            return 3;
        }
        status
    }
}

/// Mengambil hasil operasi async yang sudah selesai (C `GetResults`, slot 8 IAsyncOperation). None bila gagal.
pub fn async_results(op: &windows::core::IInspectable) -> Option<IUnknown> {
    let mut out: *mut c_void = std::ptr::null_mut();
    unsafe {
        let get_results: RawOutFn = std::mem::transmute(vtbl_slot(op.as_raw(), 8));
        let hr = get_results(op.as_raw(), &mut out);
        if hr.is_err() || out.is_null() {
            return None;
        }
        Some(IUnknown::from_raw(out))
    }
}

/// Membuka aliran thumbnail sebuah properti media (C: `get_Thumbnail` slot 15 IMediaProps, lalu `OpenReadAsync` slot 6 IStreamRef).
/// Mengembalikan operasi async yang masih berjalan; hasilnya diambil `async_results` setelah `async_status` == 1.
pub fn thumb_begin<T: Interface>(props: &T) -> Option<windows::core::IInspectable> {
    let mut sref: *mut c_void = std::ptr::null_mut();
    let mut op: *mut c_void = std::ptr::null_mut();
    unsafe {
        let get_thumb: RawOutFn = std::mem::transmute(vtbl_slot(props.as_raw(), 15));
        let hr = get_thumb(props.as_raw(), &mut sref);
        if hr.is_err() || sref.is_null() {
            return None;
        }
        let sref = windows::core::IInspectable::from_raw(sref);
        let open: RawOutFn = std::mem::transmute(vtbl_slot(sref.as_raw(), 6));
        let hr = open(sref.as_raw(), &mut op);
        if hr.is_err() || op.is_null() {
            return None;
        }
        Some(windows::core::IInspectable::from_raw(op))
    }
}

/// Menyalin `src` ke `out` dengan terminator NUL (C `HStr`: dipotong bila terlalu panjang).
fn copy_u16_nul(src: &[u16], out: &mut [u16]) {
    if out.is_empty() {
        return;
    }
    let n = src.len().min(out.len() - 1);
    out[..n].copy_from_slice(&src[..n]);
    out[n] = 0;
}

/// Menyalin HSTRING ke buffer UTF-16 ber-NUL (C `HStr`).
pub fn h_str(h: &HSTRING, out: &mut [u16]) {
    let src: Vec<u16> = h.to_string().encode_utf16().collect();
    copy_u16_nul(&src, out);
}

/// Membandingkan dua string UTF-16 sampai NUL pertama (C `wcscmp` == 0).
pub fn wstr_eq(a: &[u16], b: &[u16]) -> bool {
    let la = a.iter().position(|&c| c == 0).unwrap_or(a.len());
    let lb = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    a[..la] == b[..lb]
}

/// Mendekode thumbnail dari aliran `ras` menjadi bitmap persegi `size` x `size` (C `DecodeArt`). None bila gagal atau sisi nol.
pub fn decode_art(ras: &IUnknown, size: i32) -> Option<IWICBitmapSource> {
    let wic_ref = WIC.borrow();
    let factory = wic_ref.as_ref()?;
    unsafe {
        let stream: IStream = CreateStreamOverRandomAccessStream(ras).ok()?;
        let decoder: IWICBitmapDecoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .ok()?;
        let frame: IWICBitmapFrameDecode = decoder.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        let side = w.min(h);
        if side == 0 {
            return None;
        }
        let src: IWICBitmapSource = frame.cast().ok()?;
        let rect = WICRect {
            X: ((w - side) / 2) as i32,
            Y: ((h - side) / 2) as i32,
            Width: side as i32,
            Height: side as i32,
        };
        let clipper: IWICBitmapClipper = factory.CreateBitmapClipper().ok()?;
        clipper.Initialize(&src, &rect).ok()?;
        let clip_src: IWICBitmapSource = clipper.cast().ok()?;
        let scaler: IWICBitmapScaler = factory.CreateBitmapScaler().ok()?;
        scaler
            .Initialize(&clip_src, size as u32, size as u32, WICBitmapInterpolationModeFant)
            .ok()?;
        let scaled: IWICBitmapSource = scaler.cast().ok()?;
        let conv: IWICFormatConverter = factory.CreateFormatConverter().ok()?;
        conv.Initialize(
            &scaled,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
        .ok()?;
        let conv_src: IWICBitmapSource = conv.cast().ok()?;
        let bmp: IWICBitmap = factory.CreateBitmapFromSource(&conv_src, WICBitmapCacheOnLoad).ok()?;
        bmp.cast().ok()
    }
}


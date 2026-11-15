//! Helper registry HKCU untuk kunci milik Win12 (`HKCU\Software\Win12Shell`).
//!
//! Setara dengan `OurDword()` dan `DelValue()` di win12boot.c, serta penulisan kunci
//! di w12hotkeys.inc. Semua fungsi memakai HKEY_CURRENT_USER; jangan pernah menulis
//! `Winlogon\Shell` dari modul ini (lihat aturan di PORT-MAP.md).

use std::ffi::c_void;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW, HKEY,
    HKEY_CURRENT_USER, KEY_SET_VALUE, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};

use crate::wide::{pcw, to_wide_z};

/// Kunci milik Win12 di HKCU (setara `kOurKey` di win12boot.c).
pub const OUR_KEY: &str = r"Software\Win12Shell";

/// Baca DWORD dari HKCU. `None` bila nilai tidak ada atau bukan DWORD.
pub fn get_dword(subkey: &str, name: &str) -> Option<u32> {
    let sub = to_wide_z(subkey);
    let nm = to_wide_z(name);
    let mut v: u32 = 0;
    let mut cb: u32 = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            pcw(&sub),
            pcw(&nm),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut c_void),
            Some(&mut cb),
        )
    };
    if rc == ERROR_SUCCESS {
        Some(v)
    } else {
        None
    }
}

/// Tulis DWORD (REG_DWORD) ke HKCU; kunci dibuat bila belum ada.
pub fn set_dword(subkey: &str, name: &str, value: u32) -> bool {
    let sub = to_wide_z(subkey);
    let nm = to_wide_z(name);
    let rc = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            pcw(&sub),
            pcw(&nm),
            REG_DWORD.0,
            Some(&value as *const u32 as *const c_void),
            std::mem::size_of::<u32>() as u32,
        )
    };
    rc == ERROR_SUCCESS
}

/// Baca string (REG_SZ) dari HKCU. `None` bila nilai tidak ada atau bukan string.
pub fn get_string(subkey: &str, name: &str) -> Option<String> {
    let sub = to_wide_z(subkey);
    let nm = to_wide_z(name);
    // Buffer 1024 karakter; cukup untuk nilai konfigurasi Win12.
    let mut buf = vec![0u16; 1024];
    let mut cb: u32 = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            pcw(&sub),
            pcw(&nm),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut cb),
        )
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let chars = (cb as usize / 2).min(buf.len());
    Some(crate::wide::from_wide(&buf[..chars]))
}

/// Tulis string (REG_SZ) ke HKCU; kunci dibuat bila belum ada.
pub fn set_string(subkey: &str, name: &str, value: &str) -> bool {
    let sub = to_wide_z(subkey);
    let nm = to_wide_z(name);
    let data = to_wide_z(value);
    let rc = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            pcw(&sub),
            pcw(&nm),
            REG_SZ.0,
            Some(data.as_ptr() as *const c_void),
            (data.len() * 2) as u32,
        )
    };
    rc == ERROR_SUCCESS
}

/// Hapus satu nilai dari HKCU. Mengembalikan `true` bila nilai terhapus
/// atau memang tidak ada (sama dengan DelValue di win12boot.c).
pub fn delete_value(subkey: &str, name: &str) -> bool {
    let sub = to_wide_z(subkey);
    let nm = to_wide_z(name);
    let mut key = HKEY::default();
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, pcw(&sub), Some(0), KEY_SET_VALUE, &mut key) };
    if rc != ERROR_SUCCESS {
        return true;
    }
    let rc2 = unsafe { RegDeleteValueW(key, pcw(&nm)) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    rc2 == ERROR_SUCCESS || rc2.0 == 2 // 2 = ERROR_FILE_NOT_FOUND
}

/// DWORD dari `HKCU\Software\Win12Shell`; 0 bila tidak ada (setara `OurDword`).
pub fn our_dword(name: &str) -> u32 {
    get_dword(OUR_KEY, name).unwrap_or(0)
}

/// Tulis DWORD ke `HKCU\Software\Win12Shell`.
pub fn set_our_dword(name: &str, value: u32) -> bool {
    set_dword(OUR_KEY, name, value)
}

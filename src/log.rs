//! Catatan ke `boot.log` di folder exe.
//!
//! Pengganti `Log()` di win12boot.c (baris 34) dan `DiagLog()` di w12diag.inc (baris 26).
//! Format sama dengan C++: "YYYY-MM-DD HH:MM:SS pesan\r\n" dalam UTF-8, mode append.
//! Kegagalan tulis diabaikan (sama seperti versi C++).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use windows::Win32::System::SystemInformation::GetLocalTime;

/// Folder exe yang sedang berjalan (setara `g_dir` di C++).
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Path lengkap `boot.log` di folder exe.
pub fn boot_log_path() -> PathBuf {
    exe_dir().join("boot.log")
}

/// Stempel waktu lokal "YYYY-MM-DD HH:MM:SS".
pub fn timestamp() -> String {
    let st = unsafe { GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
    )
}

/// Tambahkan satu baris ke boot.log.
///
/// Pesan dipotong kasar pada 1000 karakter (C++ memakai buffer 1200 wchar).
pub fn log(msg: &str) {
    let text: String = msg.chars().take(1000).collect();
    let line = format!("{} {}\r\n", timestamp(), text);
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(boot_log_path())
    {
        let _ = f.write_all(line.as_bytes());
    }
}

//! Lirik tersinkron dari LRCLIB (https://lrclib.net) untuk pulau media notch.
//!
//! - Permintaan dijalankan lewat `curl.exe` bawaan Windows 10/11 di thread latar (tanpa jendela).
//!   Tidak ada crate HTTP baru. Hanya judul, artis, dan durasi yang dikirim; tanpa kredensial.
//! - Hasil disimpan di `CUR` (satu lagu saja) lalu UI diberi tahu lewat WM_W12_LYRIC ke jendela pesan.
//! - Hanya lirik tersinkron (`syncedLyrics`, format LRC) yang dipakai. Lagu tanpa artis (mis. video)
//!   tidak dicari.
//! - Parser JSON dibuat manual: hanya perlu satu nilai string per kunci.

use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::Mutex;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use super::state::{MSG, WM_W12_LYRIC};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const LRC_HOST: &str = "https://lrclib.net";

struct Lyr {
    key: String,
    /// (waktu dalam ms, teks), terurut menurut waktu.
    lines: Vec<(i64, String)>,
    done: bool,
}

static CUR: Mutex<Option<Lyr>> = Mutex::new(None);

/// Kunci lagu: artis dan judul dipisah \u{1} agar tidak bertabrakan.
pub fn key(artist: &str, title: &str) -> String {
    format!("{}\u{1}{}", artist, title)
}

/// Minta lirik untuk lagu ini bila belum ada. Non-blokir: pencarian berjalan di thread latar.
/// Dipanggil saat judul berubah atau ketika pengguna mengaktifkan lirik.
pub fn want(artist: &str, title: &str, dur_s: i32) {
    let k = key(artist, title);
    if let Ok(mut c) = CUR.lock() {
        if c.as_ref().map(|l| l.key == k).unwrap_or(false) {
            return;
        }
        if artist.trim().is_empty() || title.trim().is_empty() {
            // tanpa artis (mis. video): tidak ada yang dicari
            *c = Some(Lyr { key: k, lines: Vec::new(), done: true });
            return;
        }
        *c = Some(Lyr { key: k.clone(), lines: Vec::new(), done: false });
    } else {
        return;
    }
    let hwnd = MSG.get().0 as usize;
    let (artist, title) = (artist.to_string(), title.to_string());
    let _ = std::thread::Builder::new().name("w12-lyrics".into()).spawn(move || {
        let lines = fetch(&artist, &title, dur_s);
        if let Ok(mut c) = CUR.lock() {
            if let Some(l) = c.as_mut() {
                if l.key == k {
                    l.lines = lines;
                    l.done = true;
                }
            }
        }
        // SAFETY: jendela pesan utama; PostMessageW aman dari thread lain dan gagal tanpa efek bila jendela hilang.
        let _ = unsafe { PostMessageW(Some(HWND(hwnd as *mut c_void)), WM_W12_LYRIC, WPARAM(0), LPARAM(0)) };
    });
}

/// Baris lirik untuk lagu `k` pada posisi `pos_ms`. None bila lirik belum ada atau bukan untuk lagu ini.
/// Mengembalikan Some("") bila posisi sebelum baris pertama atau baris saat ini kosong (jeda instrumental).
pub fn line(k: &str, pos_ms: i64) -> Option<String> {
    let c = CUR.lock().ok()?;
    let l = c.as_ref()?;
    if l.key != k || !l.done || l.lines.is_empty() {
        return None;
    }
    let idx = l.lines.partition_point(|x| x.0 <= pos_ms);
    if idx == 0 {
        return Some(String::new());
    }
    Some(l.lines[idx - 1].1.clone())
}

// ---------------------------------------------------------------- pengambilan (thread latar)

fn curl(args: &[String]) -> Option<String> {
    let out = Command::new("curl.exe")
        .args(["-s", "-m", "10", "-G", "-H", "User-Agent: Win12Shell/1.0"])
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

fn fetch(artist: &str, title: &str, dur_s: i32) -> Vec<(i64, String)> {
    let mut get = vec![
        "--data-urlencode".to_string(),
        format!("track_name={}", title),
        "--data-urlencode".to_string(),
        format!("artist_name={}", artist),
    ];
    if dur_s > 0 {
        get.push("--data-urlencode".to_string());
        get.push(format!("duration={}", dur_s));
    }
    get.push(format!("{}/api/get", LRC_HOST));
    let mut synced = curl(&get).and_then(|b| json_str(&b, "syncedLyrics"));
    if synced.is_none() {
        // cocokkan persis gagal: cari dengan judul dan artis
        let search = vec![
            "--data-urlencode".to_string(),
            format!("track_name={}", title),
            "--data-urlencode".to_string(),
            format!("artist_name={}", artist),
            format!("{}/api/search", LRC_HOST),
        ];
        synced = curl(&search).and_then(|b| json_str(&b, "syncedLyrics"));
    }
    synced.map(|s| parse_lrc(&s)).unwrap_or_default()
}

// ---------------------------------------------------------------- parser

/// Nilai string pertama untuk `"key":"..."` (null dilewati). Mengembalikan teks yang sudah di-unescape.
fn json_str(body: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let bytes = body.as_bytes();
    let mut from = 0usize;
    while let Some(i) = body[from..].find(&pat) {
        let mut p = from + i + pat.len();
        while p < bytes.len() && matches!(bytes[p], b' ' | b'\t' | b'\r' | b'\n' | b':') {
            p += 1;
        }
        if p < bytes.len() && bytes[p] == b'"' {
            return Some(json_unescape(&body[p + 1..]));
        }
        from = p.max(from + i + pat.len());
    }
    None
}

/// Mengurai string JSON sampai tanda kutip penutup. Surrogate pair ditangani lewat UTF-16.
fn json_unescape(s: &str) -> String {
    let mut units: Vec<u16> = Vec::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        match c {
            '"' => break,
            '\\' => match it.next() {
                Some('n') => units.push(10),
                Some('r') => units.push(13),
                Some('t') => units.push(9),
                Some('b') => units.push(8),
                Some('f') => units.push(12),
                Some('u') => {
                    let hex: String = it.by_ref().take(4).collect();
                    if let Ok(v) = u16::from_str_radix(&hex, 16) {
                        units.push(v);
                    }
                }
                Some(o) => {
                    let mut buf = [0u16; 2];
                    units.extend(o.encode_utf16(&mut buf).iter());
                }
                None => break,
            },
            _ => {
                let mut buf = [0u16; 2];
                units.extend(c.encode_utf16(&mut buf).iter());
            }
        }
    }
    String::from_utf16_lossy(&units)
}

/// Mengurai LRC: `[mm:ss.xx] teks`, baris bisa punya beberapa cap waktu. Tag metadata (`[ar:...]`) dilewati.
fn parse_lrc(s: &str) -> Vec<(i64, String)> {
    let mut out: Vec<(i64, String)> = Vec::new();
    for line in s.lines() {
        let mut rest = line.trim_start();
        let mut times: Vec<i64> = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            if let Some(ms) = lrc_time(&rest[1..end]) {
                times.push(ms);
            }
            rest = &rest[end + 1..];
        }
        if times.is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        for t in times {
            out.push((t, text.clone()));
        }
    }
    out.sort_by_key(|x| x.0);
    out
}

/// "mm:ss.xx" atau "mm:ss" menjadi milidetik.
fn lrc_time(s: &str) -> Option<i64> {
    let (m, rest) = s.split_once(':')?;
    let (sec, frac) = rest.split_once('.').unwrap_or((rest, "0"));
    let m: i64 = m.parse().ok()?;
    let sec: i64 = sec.parse().ok()?;
    let f: String = frac.chars().take(3).collect();
    if f.is_empty() || !f.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let fr: i64 = format!("{:0<3}", f).parse().ok()?;
    Some(m * 60_000 + sec * 1000 + fr)
}

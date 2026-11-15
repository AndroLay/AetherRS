//! Port `w12dock_logic.h` (baris 1–14): pilihan aksi Win+1..9 untuk dok (Fase 5 Task 6). Status: lengkap.
//! Logika murni tanpa Win32; tes di `tests/dock_logic.rs`.

/// Hasil `w12_dock_pick`: tanpa jendela, buka aplikasi.
pub const W12DK_LAUNCH: i32 = -1;
/// Hasil `w12_dock_pick`: satu-satunya jendela sudah aktif, minimalkan.
pub const W12DK_MINIMIZE: i32 = -2;

/// `nw`: jumlah jendela aplikasi (urutan dari array jendela app); `cur`: indeks jendela
/// yang sedang aktif, -1 bila bukan salah satunya.
/// Hasil: indeks jendela yang diaktifkan; `W12DK_LAUNCH` bila tanpa jendela;
/// `W12DK_MINIMIZE` bila satu-satunya jendela sudah aktif.
pub fn w12_dock_pick(nw: i32, cur: i32) -> i32 {
    if nw <= 0 {
        return W12DK_LAUNCH;
    }
    if cur < 0 || cur >= nw {
        return 0;
    }
    if nw == 1 {
        return W12DK_MINIMIZE;
    }
    (cur + 1) % nw
}

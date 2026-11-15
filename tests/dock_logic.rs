//! Tes dari `w12dock_logic_test.cpp` (baris 1–26) untuk `w12_dock_pick`. Status: ditulis; hasil uji ada di laporan.
//! Dijalankan dengan `cargo test --test dock_logic`.
use w12::dock_logic::{w12_dock_pick, W12DK_LAUNCH, W12DK_MINIMIZE};

// Fase 5 Task 6: Win+1..9 memilih aksi untuk aplikasi dok (aktifkan, jendela berikutnya, minimize, atau jalankan).
#[test]
fn pick_sesuai_sumber() {
    assert_eq!(w12_dock_pick(0, -1), W12DK_LAUNCH, "tanpa jendela: jalankan aplikasi");
    assert_eq!(w12_dock_pick(0, 0), W12DK_LAUNCH, "indeks tidak relevan bila tanpa jendela");
    assert_eq!(w12_dock_pick(1, -1), 0, "satu jendela, tidak aktif: aktifkan");
    assert_eq!(w12_dock_pick(1, 0), W12DK_MINIMIZE, "satu jendela sudah aktif: minimize");
    assert_eq!(w12_dock_pick(3, -1), 0, "tidak ada yang aktif: jendela pertama");
    assert_eq!(w12_dock_pick(3, 0), 1, "jendela berikutnya milik aplikasi itu");
    assert_eq!(w12_dock_pick(3, 1), 2);
    assert_eq!(w12_dock_pick(3, 2), 0, "membungkus ke jendela pertama");
    assert_eq!(w12_dock_pick(2, 5), 0, "indeks di luar daftar: dianggap tidak aktif");
}

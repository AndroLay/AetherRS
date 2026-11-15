//! Tes dari `w12cal_logic_test.cpp` (baris 1–30) untuk `w12_month_grid` dan `w12_days_in_month`. Status: ditulis; hasil uji ada di laporan.
//! Dijalankan dengan `cargo test --test cal_logic`.
use w12::cal_logic::{w12_days_in_month, w12_month_grid, W12Day};

// Tes logika kalender flyout (Fase 7 Task 1 Step 1).
#[test]
fn month_grid_sesuai_sumber() {
    let mut g = [W12Day::default(); 42];
    // Senin sebagai hari pertama: 1 Nov 2026 adalah Minggu (indeks 6); 12 Nov 2026 adalah Kamis.
    w12_month_grid(2026, 11, 0, &mut g);
    assert!(g[0].y == 2026 && g[0].m == 10 && g[0].d == 26 && !g[0].in_month);
    assert!(g[6].m == 11 && g[6].d == 1 && g[6].in_month && g[17].d == 12);
    // Minggu sebagai hari pertama: 1 Nov 2026 berada di indeks 0.
    w12_month_grid(2026, 11, 6, &mut g);
    assert!(g[0].m == 11 && g[0].d == 1);
    // Februari 2026 (1 Feb = Minggu, 28 hari) berakhir di 28 Feb lalu lanjut ke Maret.
    w12_month_grid(2026, 2, 0, &mut g);
    assert!(g[6].d == 1 && g[33].d == 28 && g[34].m == 3 && g[41].d == 8);
    assert!(w12_days_in_month(2028, 2) == 29);
    assert!(w12_days_in_month(2100, 2) == 28);
    assert!(w12_days_in_month(2000, 2) == 29);
    // Lintas tahun: Januari 2026 dimulai dari 29 Des 2025.
    w12_month_grid(2026, 1, 0, &mut g);
    assert!(g[0].y == 2025 && g[0].m == 12 && g[0].d == 29);
}

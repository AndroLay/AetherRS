//! Port `w12cal_logic.h` (baris 1–40): logika murni kalender flyout (Fase 7 Task 1). Status: lengkap.
//! Tanpa WinAPI; tes di `tests/cal_logic.rs`. Nama struct `W12Day` dipertahankan, fungsi `W12*` menjadi `w12_*`.

/// Satu sel kisi kalender. `in_month` false bila sel berisi hari dari bulan sebelum/sesudahnya.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12Day {
    pub y: i32,
    pub m: i32,
    pub d: i32,
    pub in_month: bool,
}

const DAYS_IN_MONTH: [i32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
/// Offset bulan untuk algoritma Sakamoto (indeks 0 = Januari).
const SAKAMOTO: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];

/// Tahun kabisat Gregorian.
pub fn w12_is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Jumlah hari dalam bulan (1..12). Bulan di luar rentang -> 0.
pub fn w12_days_in_month(year: i32, month: i32) -> i32 {
    if !(1..=12).contains(&month) {
        return 0;
    }
    if month == 2 && w12_is_leap(year) {
        29
    } else {
        DAYS_IN_MONTH[(month - 1) as usize]
    }
}

/// Hari dalam minggu untuk tanggal Gregorian (algoritma Sakamoto): 0 = Minggu … 6 = Sabtu.
/// `m` di luar 1..12 tidak terdefinisi di C (akses di luar batas); port memakai offset 0 agar tidak panik.
pub fn w12_day_of_week(y: i32, m: i32, d: i32) -> i32 {
    let y = if m < 3 { y - 1 } else { y };
    let t = usize::try_from(m - 1)
        .ok()
        .and_then(|i| SAKAMOTO.get(i))
        .copied()
        .unwrap_or(0);
    (y + y / 4 - y / 100 + y / 400 + t + d) % 7
}

/// Isi kisi 6x7 (42 sel) untuk bulan (`year`, `month`). `first_dow` seperti LOCALE_IFIRSTDAYOFWEEK:
/// 0 = Senin … 6 = Minggu. Sel di luar bulan diisi hari dari bulan sebelum/sesudahnya dengan `in_month` = false.
pub fn w12_month_grid(year: i32, month: i32, first_dow: i32, out: &mut [W12Day; 42]) {
    let first = w12_day_of_week(year, month, 1); // 0 = Minggu
    let start_sun = (first_dow + 1) % 7; // ubah ke basis Minggu = 0
    let lead = (first - start_sun + 7) % 7; // jumlah sel sebelum tanggal 1
    let dim = w12_days_in_month(year, month);

    let (mut py, mut pm) = (year, month - 1);
    if pm < 1 {
        pm = 12;
        py -= 1;
    }
    let prev_dim = w12_days_in_month(py, pm);

    let (mut ny, mut nm) = (year, month + 1);
    if nm > 12 {
        nm = 1;
        ny += 1;
    }

    for (i, cell) in out.iter_mut().enumerate() {
        let day = i as i32 - lead + 1;
        *cell = if day < 1 {
            W12Day { y: py, m: pm, d: prev_dim + day, in_month: false }
        } else if day <= dim {
            W12Day { y: year, m: month, d: day, in_month: true }
        } else {
            W12Day { y: ny, m: nm, d: day - dim, in_month: false }
        };
    }
}

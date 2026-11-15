//! Port dari `w12start_lists.h` baris 4–66 (`#pragma once` dan `#include` tidak diport).
//! Status: lengkap, fungsi murni tanpa Win32. `capacity` pada insert diganti panjang slice; template `AppLike` menjadi trait.

/// Port `W12StartQuerySelectAll`: kueri terpilih semua bila panjangnya > 0.
pub fn w12_start_query_select_all(length: i32) -> bool {
    length > 0
}

/// Port `W12StartQueryInsertChar`. Kapasitas buffer = panjang `query`.
/// Selalu menyisakan satu elemen untuk NUL. Mengembalikan panjang baru.
pub fn w12_start_query_insert_char(query: &mut [u16], length: i32, selected_all: bool, ch: u16) -> i32 {
    let capacity = query.len().min(i32::MAX as usize) as i32;
    if capacity <= 0 {
        return 0;
    }
    let mut length = length.max(0);
    if length >= capacity {
        length = capacity - 1;
    }
    if selected_all {
        length = 0;
    }
    if length < capacity - 1 {
        query[length as usize] = ch;
        length += 1;
    }
    query[length as usize] = 0;
    length
}

/// Port `W12StartQueryClearSelection`: bila semua terpilih, kosongkan kueri.
/// Buffer kosong tidak bisa ditulis, jadi `length` dikembalikan apa adanya.
pub fn w12_start_query_clear_selection(query: &mut [u16], length: i32, selected_all: bool) -> i32 {
    if !selected_all {
        return length;
    }
    match query.first_mut() {
        Some(first) => {
            *first = 0;
            0
        }
        None => length,
    }
}

/// Rentang baris `[first, last)` yang menyentuh viewport (`W12StartVisibleRange`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct W12StartVisibleRange {
    pub first: i32,
    pub last: i32,
}

/// Port `W12StartVisibleItems`.
pub fn w12_start_visible_items(
    count: i32,
    scroll: i32,
    row_height: i32,
    viewport_height: i32,
) -> W12StartVisibleRange {
    if count <= 0 || row_height <= 0 || viewport_height <= 0 {
        return W12StartVisibleRange { first: 0, last: 0 };
    }
    let scroll = scroll.max(0);
    let mut first = scroll / row_height;
    if first > count {
        first = count;
    }
    let bottom = if viewport_height > 0x7fffffff - scroll {
        0x7fffffff
    } else {
        scroll + viewport_height
    };
    let mut last = bottom / row_height + i32::from(bottom % row_height != 0);
    if last > count {
        last = count;
    }
    if last < first {
        last = first;
    }
    W12StartVisibleRange { first, last }
}

/// Syarat aplikasi untuk `w12_start_build_pin_indices` (template `AppLike` di C++).
/// Sumber field: `spin` dan `pin` pada struct app `win12shell.cpp` baris 583–584.
pub trait AppLike {
    /// Urutan di Disematkan Start; 0 = tidak disematkan di Start.
    fn spin(&self) -> i32;
    /// -1 tidak disematkan, 0 grup kiri, 1 grup kanan.
    fn pin(&self) -> i32;
}

/// Port `W12StartBuildPinIndices`. Mengisi `out` dengan indeks `apps` dan mengembalikan jumlahnya.
/// `count` dan `limit` dibatasi panjang `apps` dan `out`, karena C++ hanya menerima pointer.
pub fn w12_start_build_pin_indices<A: AppLike>(
    apps: &[A],
    count: i32,
    limit: i32,
    out: &mut [i32],
) -> i32 {
    let limit = limit.min(out.len().min(i32::MAX as usize) as i32);
    let count = count.min(apps.len().min(i32::MAX as usize) as i32);
    if count <= 0 || limit <= 0 {
        return 0;
    }

    let mut selected: i32 = 0;
    let mut has_start_order = false;
    for i in 0..count {
        let spin = apps[i as usize].spin();
        if spin <= 0 {
            continue;
        }
        has_start_order = true;

        let mut pos: i32 = 0;
        while pos < selected && apps[out[pos as usize] as usize].spin() <= spin {
            pos += 1;
        }
        if pos >= limit {
            continue;
        }

        let new_count = if selected < limit { selected + 1 } else { limit };
        for j in ((pos + 1)..new_count).rev() {
            out[j as usize] = out[(j - 1) as usize];
        }
        out[pos as usize] = i;
        selected = new_count;
    }
    if has_start_order {
        return selected;
    }

    for side in 0..2 {
        for i in 0..count {
            if selected >= limit {
                break;
            }
            if apps[i as usize].pin() == side {
                out[selected as usize] = i;
                selected += 1;
            }
        }
    }
    selected
}

# Aturan agen port (Tahap A dan B)

Berlaku untuk semua agen port. Baca bersama `CONVENTIONS.md` dan `PORT-MAP.md`. Untuk `Winlogon\Shell`, aturan 2 di sini menggantikan `CONVENTIONS.md`.

## Lokasi
- Sumber C/C++ (BACA SAJA): `C:\Users\andro\Pictures\win12-shell`
- Tujuan (satu-satunya tempat tulis): `C:\Users\andro\Pictures\win12-shell-rust`
- Sumber crate windows 0.61.3: `C:\Users\andro\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\windows-0.61.3\src\`

## Aturan keras
1. Tulis hanya file di tugasmu. Jangan ubah `Cargo.toml`, `src/lib.rs`, `src/files/mod.rs`, `PORT-MAP.md`, `CONVENTIONS.md`, `ATURAN-AGEN.md`. Pengecualian: SH0 boleh menulis `src/shell/mod.rs` dan `docs/port/*.md`.
2. Dilarang: menjalankan `.exe`, `.ps1`, atau `cargo run`; menjalankan kode yang menulis `HKCU\...\Winlogon\Shell`; membuat service atau scheduled task; commit/push; menghapus file; menyentuh `mockup\`, `.impeccable\`, atau cadangan `win12shell-before-*`, `win12guard-before-*`, `win12boot-before-*`; mengubah file sumber; mengubah `tests\w12query_test.cpp` baris 15–39.
   Kode port yang menulis `Winlogon\Shell` (mengikuti sumber, misalnya `win12boot.c`) boleh ditulis dengan komentar `// PORT: menulis Winlogon\Shell, JANGAN dijalankan`. Fungsi itu tidak boleh dipanggil dari tes, dan wajib disebut di laporan.
3. Tes tidak boleh menyentuh sistem nyata (registry, file hosts, proses, file di luar folder sementara). Jika tes C++ melakukannya, port ke versi in-memory dan catat di laporan.
4. Literal string (nama kelas jendela, mutex, kunci dan nilai registry, nama file, nilai WM_USER, nama flag baris perintah) harus identik dengan sumber.
5. Hemat konteks: Grep dulu (`output_mode` content, `head_limit` ≤ 60). Read dengan offset/limit, maksimal ±200 baris. Maksimal satu Read atau perintah berkeluaran besar per giliran. Jangan pakai `allow_large`. Satu Write maksimal ±300 baris. File lebih besar: Write kerangka yang diakhiri baris `// @@LANJUT@@`, lalu Edit penanda itu dengan bagian berikutnya dan penanda baru. Sebelum selesai, `grep -c "@@LANJUT@@"` pada file-filemu harus 0. Tulis paling banyak 2 file per giliran.
6. Verifikasi (Bash):
   `export PATH="$HOME/.cargo/bin:/c/msys64/ucrt64/bin:$PATH"; cd /c/Users/andro/Pictures/win12-shell-rust && cargo check --lib --message-format=short 2>&1 | grep -F "NAMA_FILE.rs" | head -n 40`
   Ganti `NAMA_FILE` dengan nama file-filemu (misalnya `query.rs`). Keluaran kosong berarti 0 error di file-filemu. Error dan warning di file lain bukan tanggunganmu; jangan memperbaikinya. Bila lib hanya gagal karena error parse di file lain, laporkan "belum terverifikasi" dan selesai; jangan menunggu lama.
   Untuk binary (`src\bin\...`): `cargo check --bin <nama>`, lalu filter dengan cara yang sama. Di binary, akses modul lib lewat `w12::...`, bukan `crate::...`.
7. Fitur crate `windows` yang belum aktif, atau fungsi lintas-file yang belum ada: tulis pemanggilan dengan nama sesuai konvensi, lalu laporkan. Jangan ubah `Cargo.toml`.
8. Tes: `cargo test --test <nama>` hanya bila lib tidak mengandung error sama sekali. Bila tidak, tulis tesnya dan laporkan "belum dijalankan".
9. Laporan akhir ≤ 25 baris, Bahasa Indonesia: (a) file yang ditulis; (b) perkiraan baris C++ yang diport; (c) bagian yang dilewati atau `todo!()` beserta alasan; (d) jumlah error `cargo check` di file-filemu (0 bila bersih); (e) permintaan: fitur crate, nama fungsi lintas-file, perubahan kontrak; (f) kode `Winlogon\Shell` yang ditulis (ya/tidak, lokasi).

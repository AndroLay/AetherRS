# Pemulihan Win12 Shell (kembali ke Explorer)

Ditulis 2026-11-18. Melengkapi `Documents\Win12-pemulihan` (`catatan.txt`, `pulihkan-explorer.reg`), yang tetap berlaku.
Urutan: coba langkah paling ringan dulu. Setiap langkah perlu dicoba sekali oleh pengguna sebelum Fase 5 (capaian 4.1).

## Pemulih otomatis yang sudah ada di kode

- `win12boot` (mode shell): bila Win12 ditutup normal, Explorer dijalankan. Bila Win12 crash lebih dari 3 kali dalam 2 menit, Explorer dijalankan (`src\bin\win12boot.rs:716–738`).
- `win12guard` (mode biasa, belum dipasang): menjalankan ulang shell yang crash (maks 3 kali per 2 menit), mencatat `crash.log`, dan menampilkan lagi taskbar bawaan (`src\bin\win12guard.rs:1–2`).

## A. Win12 macet, Explorer masih hidup (keadaan sekarang)

1. Ctrl+Shift+Esc -> Task Manager -> akhiri `win12shell.exe`.
2. Bila taskbar Windows tidak muncul (Win12 menyembunyikannya): Task Manager -> cari "Windows Explorer" -> Mulai ulang.
3. Bila taskbar tetap tersembunyi otomatis: Pengaturan -> Personalisasi -> Taskbar -> Perilaku -> matikan "Sembunyikan taskbar otomatis". (Nilai asli tercatat di `install\settings.ini` `[state] OrigAutoHide`; `win12shell.exe --exit` memulihkannya bila shell masih merespons.)
4. Mencegah Win12 jalan saat sign-in berikutnya: Task Manager -> Aplikasi startup -> nonaktifkan `Win12Shell`.

## B. Mode shell (Explorer mati), Win12 macet

1. Ctrl+Shift+Esc -> File -> Jalankan tugas baru -> `explorer.exe` -> OK.
2. Akhiri `win12shell.exe` dan `win12boot.exe` di Task Manager bila masih ada.
3. Bila Ctrl+Shift+Esc tidak merespons: Ctrl+Alt+Del -> Task Manager.

## C. Setelah `Winlogon\Shell` diganti: sign-in menampilkan layar kosong

1. Ctrl+Alt+Del -> Task Manager -> File -> Jalankan tugas baru -> centang "Buat tugas ini dengan hak administrator" -> `regedit` atau `powershell`.
2. PowerShell: `Set-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon' -Name Shell -Value 'explorer.exe'`
   Atau klik ganda `Documents\Win12-pemulihan\pulihkan-explorer.reg` -> Ya.
3. Sign out lalu sign in.

## D. Tidak bisa sign-in sama sekali (Lingkungan Pemulihan Windows)

1. Di layar masuk: tahan Shift lalu klik Daya -> Mulai ulang. Pilih Pemecahan masalah -> Opsi lanjutan -> Command Prompt.
2. Cari huruf drive Windows (di WinRE sering bukan C:): `dir C:\Windows`, lalu `dir D:\Windows` bila tidak ada.
3. Ganti `C:` di bawah dengan huruf yang benar:
   ```
   reg load HKLM\OFFSOFT C:\Windows\System32\config\SOFTWARE
   reg add "HKLM\OFFSOFT\Microsoft\Windows NT\CurrentVersion\Winlogon" /v Shell /t REG_SZ /d explorer.exe /f
   reg unload HKLM\OFFSOFT
   ```
4. Tutup Command Prompt -> Lanjutkan ke Windows.

## E. Melepas fitur tambahan Win12

| Fitur | Cara melepas |
|---|---|
| Asosiasi folder ke win12files | `win12-files-assoc.ps1 -Action Disable` (repo Rust). Cadangan nilai lama dipulihkan |
| Banner notifikasi | `install\settings.ini` `[shell] notchonly=0` (sudah begitu sekarang) lalu mulai ulang Win12 |
| Mode game: kait dilepas saat fullscreen | `[shell] gamehook=0`; hapus barisnya untuk kembali ke default |
| Governor `win12gov` | Task Scheduler -> `Andro-Penjaga-Sumber-Daya` -> Nonaktifkan |
| Win12 di startup | Task Manager -> Aplikasi startup -> `Win12Shell` -> Nonaktifkan |
| Mode shell otomatis saat sign-in (sejak 2026-11-18) | Kembalikan entri Run ke nilai cadangan: `Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' Win12Shell (Get-ItemProperty 'HKCU:\Software\Win12Shell').RunBackup`. Win12 lalu berjalan berdampingan dengan Explorer lagi |

## F. Uji langkah (diisi pengguna)

| Langkah | Tanggal | Hasil |
|---|---|---|
| A.1–A.2 | | |
| B.1 | | |
| C.2 | | |
| D (cukup dibaca, tidak perlu dicoba bila C berhasil) | | |

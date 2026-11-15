# Rencana sisa port Win12 Shell (untuk dieksekusi Haiku)

Ditulis Opus, 2026-11-14. Kerjakan dari atas ke bawah. Jangan merencanakan ulang. Centang `[x]` setiap langkah yang selesai di file ini (hanya bagian 8).

## 0. Keadaan awal (terverifikasi)

- Status terbaru 2026-11-14 (14:58): `cargo check --lib --bins -j 2` rc=0, 0 error, 0 warning (`%TEMP%\w12_check_s8.log`); `cargo build --release -j 2` rc=0, 0 error, 0 warning (`w12_build_release_s8.log`). Catatan pagi di bawah ini usang: `cargo check --lib` pagi: rc=101, tepat 10 error, semuanya di `calendar.rs` dan `snap.rs` (impor fungsi yang belum diport + 6× `&D2D1_COLOR_F`; diperbaiki di T10/T12). `core_c1`/`core_c2` tidak menghasilkan error. Catatan: rustc berhenti sebelum borrow-check, jadi error borrow di core_c1/c2 baru akan muncul setelah 10 error ini hilang.
- Selesai: `core_c1.rs` (ditambah `make_format`, `create_formats`, `create_target`, `ensure_mem`) dan `core_c2.rs` lengkap (C++ 1330–2634). `state.rs` ditambah `TF_CLOCK`, `TF_DATE`, `TF_MED`, `START_STOPS`.
- Kerangka: `src/shell/mod.rs` mendeklarasikan SEMUA modul. `src/shell/prelude.rs` me-re-ekspor semuanya. Setiap file tujuan sudah ada (berisi header + `use super::prelude::*;`). Jadi `cargo check` langsung memeriksa file yang sedang dikerjakan.
- `Cargo.toml` (disetujui pengguna): fitur windows untuk Accessibility, StructuredStorage, Storage_Streams, DeviceAndDriverInstallation, HumanInterfaceDevice, UI_Input, Ioctl, IO, Wmi, TaskScheduler, ProcessStatus, Environment, WindowsProgramming, Time, Diagnostics_Debug, Shutdown, RemoteDesktop, FunctionDiscovery, Packaging_Appx, Devices_Radios, WinSock, Ndis, Dxgi_Common, Kernel, StationsAndDesktops, Magnification, Console, Devices_Properties, serta dependensi `windows-numerics = "0.2"`. Fitur yang kurang boleh ditambah sendiri (cek dulu namanya ada di `windows-0.61.3\Cargo.toml`).

## 1. Aturan cepat (supaya tidak lambat)

1. Baris pertama kode setiap file: `use super::prelude::*;`. Dengan itu, semua global, konstanta, tipe, dan fungsi `pub` dari semua file shell, ditambah tipe Windows umum (Foundation, Gdi, Direct2D, DirectWrite, WindowsAndMessaging, KeyboardAndMouse, `w!`, `GUID`, `BOOL`, `PCWSTR`, `Vector2`, `Matrix3x2`), langsung bisa dipakai TANPA import. Modul Windows lain di-import dengan glob, misalnya `use windows::Win32::UI::Shell::*;`. Jangan glob `windows::core::*` karena merusak `Result`.
2. Nama: `FooBar()` → `foo_bar()`, `g_fooBar` → `FOO_BAR`, `struct Foo` → `Foo`, field `camelCase` → `snake_case`. Urutan parameter sama.
3. SEBELUM mendeklarasikan global: Grep `static NAMA\b` di `src/shell`. Bila sudah ada, pakai saja. Deklarasi ganda TIDAK error, tetapi menghasilkan dua variabel berbeda (bug diam).
4. Jangan menulis ulang fungsi milik file lain; panggil saja. Bila fungsinya belum ada, tetap tulis panggilannya. Error `cannot find function` untuk fungsi file lain BUKAN tugasmu; lanjutkan.
5. Tipe: `int`→`i32`, `UINT/DWORD`→`u32`, `BOOL/bool`→`bool`, `float`→`f32`. `const wchar_t*` masukan → `&str`. Buffer wchar keluaran (`wchar_t *out, int cch`) → kembalikan `String`. `int &x` → `&mut i32`. Pointer boleh NULL → `Option<&T>`. Parameter default C++ → pemanggil menulis nilainya. WNDPROC → `pub unsafe extern "system" fn nama(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT`.
6. Global: `pub static X: SyncCell<Cell<T>>` (nilai Copy) atau `SyncCell<RefCell<T>>` (String/Vec/COM `Option<I...>`). COM: ambil salinan dulu (`let Some(rt) = RT.borrow().as_ref().cloned() else { return };`), lalu pakai `rt`. Jangan memegang `borrow()` saat memanggil SendMessage, MessageBox, TrackPopupMenu, atau fungsi yang mungkin mengubah global yang sama.
7. Pola gambar (contoh benar ada di `core_c2.rs`, fungsi `surface`, `draw_item`, `render_pill`, `glow_update`):
   - `let pal = *PALETTE.borrow();` lalu `pal.text`, `pal.sub`, `pal.accent`, dll.
   - `text(&s, TF_B.borrow().as_ref(), l, t, r, b, pal.text, 1.0)`; `glyph(0xE721, TF_GLYPH.borrow().as_ref(), cx, cy, sf(16.0), pal.glyph, 1.0)`; `fill_rr(l, t, r, b, rad, warna, 1.0)`; `surface(l, t, r, b, rad, warna)`.
   - `s(v: f32) -> i32` = `S()`, `sf(v: f32) -> f32` = `SF()`, `rgba(r, g, b, a)`, `rf(l, t, r, b)`, `mix_c(a, b, t)`.
   - Gradien: `rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)`. Kuas: `rt.CreateLinearGradientBrush(&props, None, &sc)`. Garis: `rt.DrawRoundedRectangle(&rr, &brush, 1.0, None::<&ID2D1StrokeStyle>)`. Titik: `Vector2 { X, Y }`. Transform: `rt.GetTransform(&mut m)` / `rt.SetTransform(&m)` dengan `Matrix3x2`. Akhir: `rt.EndDraw(None, None)`; `D2DERR_RECREATE_TARGET` → `create_target()`.
   - Render jendela berlapis: `ensure_mem(w, h)`, `rt.BindDC(MEMDC.get(), &rect)`, `rt.BeginDraw()`, `rt.Clear(Some(&warna))`, lalu `layer_present(h, &wr, 255)`.
   - Timer: `set_timer(ID, ms)` / `kill_timer(ID)`. Invalidate: `w12_invalidate(h, None, false)`.
8. Jangan `unwrap()` pada Win32. Gunakan `let _ =`, `if let Ok`, atau `match`.
9. Literal string (nama kelas jendela, kunci registry, nama file, pesan) harus sama persis dengan C++.

## 2. Resep per tugas (ulangi untuk setiap baris di bagian 3)

1. Daftar fungsi sumber: `Grep` pola `^(static )?\w[\w\s\*&:<>]*\(` pada file sumber (`output_mode` content).
2. Read sumber 200 baris, lalu tulis Rust untuk potongan itu (tambah ke akhir file tujuan dengan Edit). Ulangi sampai rentang selesai. Global file-lokal (`static` di .inc) ditulis sebagai `static` privat di file tujuan, kecuali dipakai file lain (maka `pub`).
3. Setelah file selesai, cek sekali (PowerShell, timeout 600000):
   `cargo check --lib -j 2 --message-format short --manifest-path "C:/Users/andro/Pictures/win12-shell-rust/Cargo.toml" *> "$env:TEMP\w12-TUGAS.log"; Select-String -Path "$env:TEMP\w12-TUGAS.log" -Pattern 'shell\\NAMAFILE\.rs' | Select-Object -First 40 | ForEach-Object { $_.Line }`
4. Perbaiki error di file sendiri. Cek ulang maksimal 3 kali per tugas. Sisa error yang hanya `cannot find` untuk fungsi milik file yang belum dikerjakan → catat di bagian 8, lalu lanjut ke tugas berikutnya.
5. Ganti header `//!` file menjadi: sumber + "Status: lengkap, lolos cargo check, belum diuji runtime".

## 3. Daftar tugas (urutan dipilih supaya file yang banyak dipanggil selesai duluan)

| ID | File Rust | Sumber (baca saja) | Isi utama |
|---|---|---|---|
| T01 | `panel.rs` | `w12panel.inc` (401) | PanHit/PanHover/PanHitAt, PanelContent/Measure/Activate/Place/Render/Tick, PanelClose/Open/Toggle, PanUri, PanKeys, PanelDrag, PanelProc; tipe `PanHitRec`, global `PAN_HIT` (lihat keputusan-integrator.md §2) |
| T02 | `widgets.rs` | `w12widgets.inc` (399) | SysSample/SysRelease, WidgetsVisibility, WidgetsLoad/Save, WidgetsPlace, WidgetDraw (stub sudah ada: tanda tangan `widget_draw(pi: i32, w: f32, h: f32)` WAJIB tetap), WidgetClick, Wg* |
| T03 | `flyout.rs` | `w12flyout.inc` (906) | Wl* (thread WLAN), Wifi*, BatHistNote, Batt*, Bt* (dipakai calendar.rs: `bt_card`, `wf_text_c`) |
| T04 | `qs.rs` | `w12qs.inc` (891) | Bri* (WMI), Qs*, Rad* (radio: pakai `windows::Devices::Radios` bila raw vtable sulit; catat deviasi) |
| T05 | `notch.rs` | `w12notch.inc` (1269) | Pm*, Nt*, NotchRender/Proc/Sync/Create/Destroy/Hotkey/Media/Volume/Power/BatteryAlert; global `NT_STATE`, `NT_HUD_KIND` |
| T06 | `start.rs` | `w12start.inc` baris 1–757 | global & tipe start (StHit, StIdx, dll.), StartListsInvalidate, St* indeks/semua-aplikasi/pencarian, StartContextMenu*, StHitIcon, StartNlCheck, StartNlIconLoad. Global/tipe yang dipakai T07 dibuat `pub` |
| T07 | `start_draw.rs` | `w12start.inc` baris 758–akhir | StartMeasure, StartReset, StartChar, StartBuildLists, StartPick, StartPowerDo, StartKey, StartWheel, StartActivate, StTextWrap*, Start*Draw, StartDraw |
| T08 | `core_c3.rs` | `win12shell.cpp` 2636–3337 | AnimStep, Tip*, SendKeys, ActivateWindow, LaunchApp, QuietStart, ClickApp, FocusMediaSource, ActivateItem, TrayWnd, Hide/RestoreNativeTaskbar, tray host (`NID32`, `TrayIcon`, TrayHostProc), Flyout* (FlySlideStep, FlyoutProc, CreateFlyout), TrayCheck, ShellWorkArea, AppBar*. Tipe `HTHUMBNAIL(pub isize)` di sini |
| T09 | `core_c3b.rs` | `win12shell.cpp` 3338–3960 | SetMode, Autostart* (TaskScheduler), ShellModeConfigured (HANYA BACA Winlogon\Shell), GlyphBitmap, MenuIcons, TrackMenu, MainMenu, DeskProc, ShellModeStart/Stop, Desk*, ShellOpenQs, RunDlgThread, DockNumber, OpenFolderWindow, ShellKey, FindAppByIdentity, TogglePin, JumpList*, AppMenu, IsSystemFlyout, UpdateForeground; global `HK_*` (HWINEVENTHOOK) |
| T10 | `hotkeys.rs`, `winx.rs`, perbaiki `snap.rs` | `w12hotkeys.inc` (694), `w12winx.inc` (115) | Hk*, ShellOpen, BootExePath, ShellSwitchToWindows, NormalSwitchToShell, Hotkeys*, WinX*. `snap.rs` sudah ditulis: ganti import lamanya dengan `use super::prelude::*;` |
| T11 | `overlay.rs` | `w12overlay.inc` (611) | Ov*, Sw*, Tv*, Tk*, TasksStart/Stop/Key |
| T12 | `tasks_ui.rs`, `stophosts_ui.rs`, perbaiki `calendar.rs` | `w12tasks.inc` (477), `w12stophosts.inc` (82) | DeskWall*, Vd* (global `VD`, tipe `VdWin`), StopListed, StopHostsTick. `calendar.rs`: ganti import dengan prelude, perbaiki 6 error `&D2D1_COLOR_F` → `*warna` |
| T13 | `filesearch.rs` | `w12filesearch.inc` (634) | Fs* (thread indeks + pencarian, ReadDirectoryChangesW: IRP bocor pernah bikin freeze — tiru C++ persis, termasuk CancelIoEx/close), tipe `FsResult`, global `FS_RES` |
| T14 | `gesture_ui.rs`, `usb.rs`, `nl_ui.rs`, `diag_ui.rs` | `w12gesture.inc`, `w12usb.inc`, `w12nl.inc`, `w12diag.inc` | Gest* (RawInput/HID), Usb* (cfgmgr32), NlApi* + makro `w12_lazy!` (satu-satunya, di nl_ui), Diag* (`DiagLog` = `crate::log::log`) |
| T15 | `core_c4.rs` | `win12shell.cpp` 3961–4297 dan 4316–4530 | DockIconRect, AnimWndProc, MinAnim*, RaisePills, ZoomedOnPrimary, BaseRender/Proc/Update, IsKnown/NewAppWindow, PillsHideFor, WinEventProc, OnDpi, HitItem, FinishDrag, MicaReloadWallpaperIfChanged, PillProc, CreatePill, ArmClock, TrimWorkingSet. Baris 4298–4315 hanya `#include`, lewati |
| T16 | `core_c4b.rs` | `win12shell.cpp` 4531–5284 | OnTimer, KbMaskStartKey, KbHookProc, KbThread, KbHookStart/Stop, Deactivate/ResumeNativeStartProcesses, StartFromKey, WallWatch*, Shutdown, AppCmdSkipAllowed, ShellAppCommand, MsgProc, CrashFilter, `wWinMain` → `pub fn w_win_main() -> i32` (baca argumen dengan `std::env::args_os`, hInstance dari `GetModuleHandleW(None)`) |

Opsi lebih cepat (bila pengguna mengizinkan subagen): jalankan T01–T16 sebagai subagen Haiku paralel, satu tugas per agen, masing-masing hanya menulis file di barisnya. Prompt agen: "Kerjakan tugas TXX di `docs/port/rencana-sisa.md` bagian 1–3. Jangan ubah file lain. Laporkan ≤10 baris: file, error tersisa, fungsi yang belum ada."

## 4. Integrasi (setelah T01–T16)

1. Hapus stub privat yang sekarang punya pemilik:
   - `core_c2.rs`: `fn widgets_visibility() {}`, `fn notch_sync() {}`, `fn notch_media() {}`, `fn start_lists_invalidate() {}`, `fn widgets_place(...) {}` (cari dengan Grep `^fn (widgets_visibility|notch_sync|notch_media|start_lists_invalidate|widgets_place)`).
   - `core_c1.rs`: `fn bat_hist_note(...)` dan `fn notch_battery_alert(...)`. Lalu tambahkan `use super::prelude::*;` di `core_c1.rs`.
2. `core_c1.rs` ~baris 1729: PENDING `IMMDevice::Activate`. Fitur sudah aktif, jadi ganti jalur sementaranya dengan `dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)`.
3. `NAPPS` di `state.rs` tidak pernah diisi. Jumlah aplikasi = `APPS.borrow().len()`. Grep `NAPPS`, lalu ganti pemakaiannya.
4. `APPS_TXT` harus diisi saat start (C++ di wWinMain/LoadSettings). Pastikan `w_win_main` mengisi `DIR`, `INI`, `APPS_TXT` sama seperti C++.
5. Ulangi `cargo check --lib -j 2` sampai 0 error. Lalu `cargo check --bins -j 2`.
6. Buat `src/bin/win12shell/main.rs`:
   ```rust
   #![windows_subsystem = "windows"]
   fn main() { std::process::exit(w12::shell::core_c4b::w_win_main()); }
   ```
7. Buat `build.rs` di akar crate (manifest DPI PerMonitorV2 + Common Controls 6; toolchain = x86_64-pc-windows-gnu):
   ```rust
   use std::{env, path::PathBuf, process::Command};
   fn main() {
       println!("cargo:rerun-if-changed=build.rs");
       let src = PathBuf::from(r"C:\Users\andro\Pictures\win12-shell");
       let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("win12shell-res.o");
       let ok = Command::new(r"C:\msys64\ucrt64\bin\windres.exe")
           .current_dir(&src)
           .args(["win12shell.rc", "-O", "coff", "-o"])
           .arg(&out)
           .status()
           .map(|s| s.success())
           .unwrap_or(false);
       if ok {
           println!("cargo:rustc-link-arg-bin=win12shell={}", out.display());
       } else {
           println!("cargo:warning=windres gagal: win12shell.exe tanpa manifest");
       }
   }
   ```
8. `cargo build --release -j 2 --manifest-path "C:/Users/andro/Pictures/win12-shell-rust/Cargo.toml"`. Hasilnya `target\release\win12shell.exe`, `win12files.exe`, `win12boot.exe`, `win12guard.exe`. Jangan menjalankan exe-nya.

## 5. Pasang (DIJALANKAN PENGGUNA; mungkin butuh admin)

`deploy-win12.ps1 -SkipBuild` mengambil exe dari `%TEMP%\win12build` (`win12shell.exe`, `win12boot.exe`, `win12files.exe`), menutup shell lama dengan `--exit`, menyimpan cadangan `win12shell-before-deploy-<waktu>.exe`, lalu menyalin ke `C:\Users\andro\Pictures\win12-shell`. Skrip ini MENOLAK bila file sumber C++ lebih baru daripada build (baris 81–87). Karena itu, untuk build Rust:

1. Pengguna menyalin build Rust ke folder build:
   `! New-Item -ItemType Directory -Force "$env:TEMP\win12build"; Copy-Item "C:\Users\andro\Pictures\win12-shell-rust\target\release\win12shell.exe","C:\Users\andro\Pictures\win12-shell-rust\target\release\win12boot.exe","C:\Users\andro\Pictures\win12-shell-rust\target\release\win12files.exe" "$env:TEMP\win12build\" -Force`
2. Pengguna menjalankan: `! powershell -NoProfile -ExecutionPolicy Bypass -File "C:\Users\andro\Pictures\win12-shell\deploy-win12.ps1" -SkipBuild`
3. Bila skrip menolak karena cap waktu sumber, berhenti dan tanyakan pengguna. Jangan mengubah `deploy-win12.ps1` atau cap waktu file.
4. Kembali ke C++: jalankan deploy tanpa `-SkipBuild` (membangun ulang C++), atau salin cadangan `win12shell-before-deploy-*.exe`.

## 6. Larangan (tetap berlaku)

- Jangan jalankan `.exe`, `.ps1`, `cargo run`, `cargo test` (kecuali diminta), boot, atau mode shell. Jangan buat service atau scheduled task. Jangan commit/push.
- Jangan menulis `HKCU\...\Winlogon\Shell`. Di `win12boot.rs` sudah dijaga `WINLOGON_WRITE_ENABLED = false`.
- Jangan ubah `win12shell.cpp`, file `.inc`, `mockup\`, `.impeccable\`, atau cadangan `*-before-*`. Jangan sentuh SuhuAman, Defender, proses sistem, atau `settings.ini`.
- `cargo check` selalu memakai `-j 2` (batas panas laptop), satu proses cargo pada satu waktu.

## 7. Memori (akhir pekerjaan)

Perbarui `C:\Users\andro\.claude\projects\C--Windows-System32\memory\win12-rust-port.md` dan baris "Port Rust Win12" di `MEMORY.md`: status per tugas, hasil cargo check, dan status pemasangan.

## 8. Checklist

- [x] T01 panel  - [x] T02 widgets  - [x] T03 flyout  - [x] T04 qs  - [x] T05 notch
- [x] T06 start  - [x] T07 start_draw  - [x] T08 core_c3  - [x] T09 core_c3b
- [x] T10 hotkeys/winx/snap  - [x] T11 overlay  - [x] T12 tasks_ui/stophosts/calendar
- [x] T13 filesearch  - [x] T14 gesture/usb/nl/diag  - [x] T15 core_c4  - [x] T16 core_c4b
- [x] Integrasi 1–5 (lib 0 error)  - [x] bin + build.rs  - [x] build release (2026-11-14, rc=0, 0 error, 0 warning)  - [x] pasang (pengguna; deploy cadangan 23:38:53, shell PID 20196 mulai 23:39:18; pengguna melapor "sudah berhasil")
- [x] B1 deploy: parameter `-Shell` (parse OK)  - [x] B2 Del hapus item riwayat clipboard; emoji 8×8 (cargo check 0/0; build release 0 warning)  - [ ] pasang build B1/B2 + mode shell: butuh PowerShell admin pengguna (sesi asisten tidak elevated; UAC dibatalkan 2026-11-15)  - [ ] Phone Link admin (PhoneSvc Disabled, 2 paket provisioned)  - [ ] uji runtime B2 dan mode shell setelah pasang

Catatan error tersisa per tugas:

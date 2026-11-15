# Keputusan integrator untuk Tahap B (src/shell)

Dibaca semua agen C1–C4 dan U1–U8 sebelum menulis. Dokumen ini menjawab usulan SH0 di `shell-split.md` bagian 6 dan menggantikan `shell-funcs.md`, yang belum pernah dibuat. Ditulis integrator; agen tidak mengubahnya.

## 1. Keputusan atas usulan SH0

- Keputusan 1, 2, 3, 4, 5, 6, 8, 10, 11, 13, 14, dan 15: disetujui apa adanya.
- Keputusan 7 (`HTHUMBNAIL`): tipe ini tidak ada di windows 0.61.3 (pencarian di crate kosong). Definisikan di `core_c3.rs`:
  `#[derive(Clone, Copy, PartialEq, Eq, Default)] pub struct HTHUMBNAIL(pub isize);`. Nilai kosong `HTHUMBNAIL(0)`.
- Keputusan 9 (diubah): `g_fsRes` menjadi `RefCell<Option<Box<FsResult>>>`, tanpa pointer mentah. Owner: `filesearch.rs`.
- Keputusan 12: disetujui. `g_mstat` (`win12shell.cpp:1189, 1347–1360, 4890`) hanya pabrik `IMediaManagerStatics` dan tidak dipakai di luar media. Di Rust, panggil `GlobalSystemMediaTransportControlsSessionManager::RequestAsync()` langsung. Percobaan ulang lewat `T_MEDIA` tetap seperti C (baris 1360).

## 2. Global PENDING dideklarasikan di file owner

`state.rs` tidak memuat global yang tipenya milik owner. Setiap owner mendeklarasikan globalnya sendiri sebagai `pub static NAMA: SyncCell<...>` di file miliknya, dengan bentuk sesuai CONVENTIONS (`Cell<T>` untuk nilai Copy, `RefCell<T>` untuk lainnya). Pemakai lain mengimpor dengan `use super::<owner>::NAMA;`.

| Nama Rust | Sumber C | Owner | Bentuk |
|---|---|---|---|
| `PALETTE` | `C` (win12shell.cpp:179) | core_c1.rs | `RefCell<Palette>` |
| `WALLPAPER_TINT` | `g_wallpaperTint` (:180) | core_c1.rs | `Cell<W12Rgb8>` bila Copy, selain itu `RefCell` |
| `PFN_GET_APP_USER_MODEL_ID` | `pGetApplicationUserModelId` (:548) | core_c1.rs | `Cell<Option<PfnGetApplicationUserModelId>>` |
| `PFN_CREATE_STREAM_OVER_RAS` | `pCreateStreamOverRandomAccessStream` (:1285) | core_c1.rs | `Cell<Option<PfnCreateStreamOverRas>>` |
| `APPS` | `g_apps` (:589), `App[MAXAPPS]` | core_c1.rs | `RefCell<Vec<App>>` |
| `NOTE_ICON` | `g_noteIcon` (:592) | core_c1.rs | `RefCell<Icon>` (sesuaikan bila Icon tidak Default) |
| `WC` | `g_wc` (:774), `WinInfo[MAXWC]` | core_c1.rs | `RefCell<Vec<WinInfo>>` |
| `MEDIA` | `g_media` (:1187), `MediaState` | core_c1.rs | `RefCell<MediaState>` |
| `ITEMS` | `g_items` (C2:1485), `Item[MAXITEMS]` | core_c2.rs | `RefCell<Vec<Item>>` |
| `PILLS` | `g_pills` (C2:1487), `Pill[MAXPILLS]` | core_c2.rs | `RefCell<Vec<Pill>>` |
| `HK_FG`, `HK_MIN`, `HK_CLOAK`, `HK_LOC`, `HK_LIFE` | `HWINEVENTHOOK` (C3); nama C ditentukan agen C3 dengan Grep | core_c3.rs | `Cell<HWINEVENTHOOK>` |
| `THUMB` | `g_thumb` (C3:3957) | core_c3.rs | `Cell<HTHUMBNAIL>` |
| `NT_STATE` | `g_ntState` (notch:39) | notch.rs | `Cell<i32>`, awal `NT_IDLE` |
| `NT_HUD_KIND` | `g_ntHud` (notch:43) | notch.rs | `Cell<i32>` |
| `PAN_HIT` | `g_panHit` (panel:27), `PanHitRec[PAN_MAXHIT]` | panel.rs | `RefCell<Vec<PanHitRec>>` |
| `FS_RES` | `g_fsRes` (filesearch:43) | filesearch.rs | `RefCell<Option<Box<FsResult>>>` |
| `VD` | `g_vd` (tasks_ui:255), `VdWin[VD_WINS]` | tasks_ui.rs | `RefCell<Vec<VdWin>>` |
| `NL_DEVICES` | `g_nlDevices` (nl_ui:7), tipe dari `crate::nl_json` | nl_ui.rs | `RefCell<Option<...>>` (bukan const fn) |

Global non-PENDING yang sudah ada di `state.rs` tetap di sana dan dipakai langsung dengan `use super::state::NAMA;`.

## 3. Tipe owner

- `core_c1.rs`: `Palette`, `W12Rgb8`, `Icon`, `App`, `WinInfo`, `MediaState`, dan typedef PFN (`PfnXxx`).
- `core_c2.rs`: `Item`, `Pill`.
- `core_c3.rs`: `HTHUMBNAIL` (newtype, lihat bagian 1).
- `panel.rs`: `PanHitRec`.
- `filesearch.rs`: `FsResult`.
- `tasks_ui.rs`: `VdWin`.
- Tipe owner lain yang belum tercantum: definisikan di owner sesuai shell-split.md bagian 4, lalu laporkan.
- Tipe Windows yang tidak ada di crate: newtype dengan field `.0`, didefinisikan di file pemakai pertama, lalu dilaporkan.

## 4. Fungsi lintas-owner

- `shell-funcs.md` tidak dibuat. Tanda tangan diambil langsung dari definisi C di sumber (Grep `NamaFungsi(`) dan dipetakan dengan aturan CONVENTIONS.
- Fungsi yang dipakai owner lain ditulis `pub fn` dengan nama snake_case. Pemanggil memakai `use super::<owner>::nama;`, misalnya `use super::core_c1::trim_soon;`.
- Makro `InvalidateRect` (C baris 83) menjadi `w12_invalidate(hwnd: HWND, rect: Option<&RECT>, erase: bool) -> bool`. Makro `UpdateWindow` (baris 84) menjadi `w12_update(hwnd: HWND) -> bool`. Keduanya di `core_c1.rs`.
- `DiagLog` menjadi `crate::log::log`. Jangan membuat fungsi baru untuk itu.
- `W12_LAZY` hanya satu, di `nl_ui.rs`: `macro_rules! w12_lazy { ... }` diikuti `pub(crate) use w12_lazy;`. Flyout memakainya dengan `use super::nl_ui::w12_lazy;`.

## 5. Winlogon\Shell

- Penulisan `HKCU\...\Winlogon\Shell` hanya ada di `win12boot.rs` (owner B). Tahap B tidak menulis kunci itu.
- `win12shell.cpp:3405` hanya membaca. Di C3, port pembacaan dengan `crate::reg` dan periksa hive serta nama kunci yang dipakai C.

## 6. Fitur crate dan berkas bersama

- Integrator menambahkan fitur ke `Cargo.toml` setelah diverifikasi: `Win32_UI_Accessibility` (HWINEVENTHOOK), `Storage_Streams` (IStreamRef), `Media_Control` (`windows::Media::Control`), dan `Win32_Graphics_Dwm` bila thumbnail dipakai. `Win32_Storage_EnhancedStorage` tidak diaktifkan; `PK_AUMID` ditulis dengan `GUID::from_u128`.
- Agen tidak mengubah `Cargo.toml`, `mod.rs`, `state.rs`, `lib.rs`, atau berkas owner lain. Kebutuhan fitur atau konstanta yang kurang dilaporkan di laporan akhir.
- `pub mod` untuk berkas baru ditambahkan integrator setelah berkas owner selesai.

## 7. Verifikasi

- `cargo check` hanya memeriksa modul yang sudah dideklarasikan di `mod.rs`. Bila grep untuk berkasmu kosong karena modulmu belum dideklarasikan, tulis "belum terverifikasi" di laporan dan selesai.
- Setelah modul dideklarasikan, error di berkasmu diperbaiki dan diverifikasi ulang.

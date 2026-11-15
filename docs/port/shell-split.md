# Pembagian port shell: win12shell.cpp ke src/shell

Dokumen ini dibuat SH0 (arsitek shell). Isinya: pembagian sumber C/C++ ke berkas Rust, peta owner, daftar fungsi per owner (bahan `shell-funcs.md`), keputusan port, dan bagian yang belum selesai. Saat ini baru deklarasi (`state.rs`) dan `mod.rs` yang ada. Logika fungsi belum diport.

> **Status (2026-11-14): snapshot SH0, sudah usang.** Tabel bagian 1 dan baris 164–166 di bawah masih menulis "belum dibuat/belum ditulis". Per pemeriksaan Glob 2026-11-14, `src/shell/` sudah berisi `core_c1.rs` s.d. `core_c4.rs`, `core_c3b.rs`, `core_c4b.rs`, dan file owner (mis. `hotkeys.rs`, `usb.rs`, `start.rs`, `notch.rs`, `flyout.rs`, `snap.rs`). Yang belum diperbarui di bawah ini adalah tabel dan daftar fungsi per owner. Status paritas terkini ada di `paritas-explorer.md`. Dokumen ini tidak menyatakan paritas runtime.

## 1. Keadaan berkas

| Berkas | Keadaan |
|---|---|
| `src/shell/mod.rs` | Ada. Isinya hanya `pub mod state;` |
| `src/shell/state.rs` | Ada. Deklarasi saja: konstanta, global `SyncCell`, atomik. Tanpa logika. |
| `src/shell/core_c1.rs` s.d. `core_c4.rs` | Belum dibuat |
| `src/shell/<owner>.rs` (U1 s.d. U8, lihat bagian 3) | Belum dibuat |
| `docs/port/shell-funcs.md` | Belum ditulis (opsional) |

## 2. Pembagian C1 s.d. C4 (win12shell.cpp, 5284 baris, CR dibuang)

| Berkas Rust | Baris C++ | Mulai dari | Catatan |
|---|---|---|---|
| `core_c1.rs` | 1–1329 | awal file | typedef PFN; makro `InvalidateRect` (baris 83) dan `UpdateWindow` (baris 84) |
| `core_c2.rs` | 1330–2635 | `MediaClear` | enum item dan pil di baris 1478–1482 (sudah dicocokkan, lihat bagian 8) |
| `core_c3.rs` | 2636–3960 | `AnimStep` | pembacaan `Winlogon\Shell` di baris 3405 (hanya baca) |
| `core_c4.rs` | 3961–5284 | `DockIconRect` | include `w12*.inc` di baris 4298–4315 |

Konstanta yang dicocokkan dengan sumber: `PK_AUMID` (baris 544) dan `W12_KBSIG` = 0x57313253 (baris 2768).

## 3. Peta owner ke berkas

Panjang berkas sumber dalam baris (dari PORT-MAP yang dibaca SH0).

| Owner | Berkas Rust | Sumber |
|---|---|---|
| C1 | `core_c1.rs` | win12shell.cpp 1–1329 |
| C2 | `core_c2.rs` | win12shell.cpp 1330–2635 |
| C3 | `core_c3.rs` | win12shell.cpp 2636–3960 |
| C4 | `core_c4.rs` | win12shell.cpp 3961–5284 |
| U1 notch | `notch.rs` | `w12notch.inc` (1269) |
| U2 start | `start.rs` | `w12start.inc` (1460) |
| U3 flyout | `flyout.rs` | `w12flyout.inc` (906) |
| U4 qs | `qs.rs` | `w12qs.inc` (891) |
| U5 hotkeys | `hotkeys.rs` | `w12hotkeys.inc` (694) |
| U5 snap | `snap.rs` | `w12snap.inc` (148) |
| U5 winx | `winx.rs` | `w12winx.inc` (115) |
| U6 overlay | `overlay.rs` | `w12overlay.inc` (611) |
| U6 tasks | `tasks_ui.rs` | `w12tasks.inc` (477) |
| U7 panel | `panel.rs` | `w12panel.inc` (401) |
| U7 widgets | `widgets.rs` | `w12widgets.inc` (399) |
| U7 calendar | `calendar.rs` | `w12calendar.inc` (112) |
| U7 gesture | `gesture_ui.rs` | `w12gesture.inc` (286) |
| U8 filesearch | `filesearch.rs` | `w12filesearch.inc` (634) |
| U8 usb | `usb.rs` | `w12usb.inc` (176) |
| U8 nl | `nl_ui.rs` | `w12nl.inc` (163) |
| U8 diag | `diag_ui.rs` | `w12diag.inc` (167) |
| U8 stophosts | `stophosts_ui.rs` | `w12stophosts.inc` (82) |

Belum ada owner resmi (perlu ditetapkan integrator): `w12fm_bar.inc`, `w12fm_search.inc`, `win12boot.c`. Catatan 2026-11-14: `src\files\` sudah berisi `bar.rs` dan `search_ui.rs` (Glob); isi keduanya belum dibandingkan dengan `.inc`, jadi pemetaannya belum terbukti.

## 4. Fungsi per owner (bahan shell-funcs.md)

Format `nama/N`: N = jumlah pemakai dari analisis. Tanda `?` = perlu dicek apakah false positive (lihat bagian 7). Tanpa `/N` = pemakai tunggal atau belum dihitung.

- C1 (50): SafeRelease/8, TrimSoon/7, S?/15, SF?/13, LoadSettings/2, SaveSettings/3, RGBA?/10, WindowsUsesLight/2, LoadAccentColors/2, ApplyWallpaperSurfaceTint/3, LoadTheme/2, CreateFormats/2, CreateTarget/6, EnsureMem/8, ReleaseMem/5, RF?/9, SetColor/9, FillRR/13, Text?/11, Glyph?/10, MeasureText/5, IconFromParsingName/3, IconFromHICON/3, IconBitmap/2, IconFree/5, DrawIconAt/4, Copy?/9, IconSize/3, TileMargin/2, SrcPx/2, LoadApps/2, SaveApps/3, IsOurWindow/4, IsTaskWindow/8, WindowIdentity/2, MatchApp/2, AddRunningApp/2, ReloadIcons/3, VolumeDetach/2, VolumeAttach/2, NetInit/2, NetUpdate/2, BatteryUpdate/2, ClockUpdate/2, MediaUnhook/2, MediaHook/2, MediaEvent/2, AsyncStatus/3, HStr/2, DecodeArt/2.
- C2 (34): MediaClear, MediaTick, MediaLog, MediaCmd, WgIdx, WgScale, BarH, MixC, DeskLayer, IsTopPillWnd, PillsToDesk, ApplyPills, InvalidatePills, Relayout, MediaChanged, RefreshWindows, VolGlyph, DrawBattery, DrawArt, DrawIconTile, MicaResolveWallpaperPath, MicaGetWallpaperStamp, MicaLoad, MicaCaptureFullscreen, MicaRestoreWallpaper, MicaBegin, MicaMoved, MicaFill, Surface?, MediaSourceName, LayerPresent, RenderPill, StartAnim, DragTargetX.
- C3 (42): AnimStep, TipHide, TipHover, TipShow, SendKeys, ActivateWindow/5, QuietStart, ClickApp, FocusMediaSource, ActivateItem, TrayWnd, HideNativeTaskbar, RestoreNativeTaskbar, TrayRemove, TrayHostProc, TrayHostCreate, FlySlideStep, FlyoutClose, FlyoutProc, CreateFlyout, TrayCheck, ReserveH, AppBarSetPos, AppBarRegister, AppBarRemove, SetMode, MenuIcons, TrackMenu/5, MainMenu/3, ShellModeStart, ShellModeStop, DeskRestoreAll, DeskMinimizeAll, ShowDesktopToggle, ShellOpenQs, OpenFolderWindow/3, ShellKey/4, FindAppByIdentity, AppMenu/3, ScheduleRefresh, IsSystemFlyout, UpdateForeground.
- C4 (7): BaseUpdate/4, WinEventProc, MicaReloadWallpaperIfChanged, TrimWorkingSet/3, DeactivateNativeStartProcesses, ResumeNativeStartProcesses, StartFromKey/3.
- U1 notch (10): NotchInvalidate, NtShowTemp, NotchSync, NotchCreate, NotchDestroy, NotchHotkey, NotchMedia, NotchVolume, NotchPower, NotchBatteryAlert.
- U2 start (14): StartListsInvalidate, StIdxInstall, StSearch, StartContextMenuMessage, StartContextMenu, StartNlIconLoad, StartMeasure, StartReset, StartChar, StartPowerDo, StartKey, StartWheel, StartActivate, StartDraw.
- U3 flyout (13, plus makro W12_LAZY): WifiWlanNotify, WifiRadioChanged, WfTextC, WifiMeasure, WifiWheel, WifiDraw, WifiActivate, BatHistNote, BtTextR, BtCard, BattMeasure, BattDraw, BattActivate.
- U4 qs (13): QsBriNotify, QsRelease, QsMeasure, QsWheel, QsToggleMute, QsDrag, QsDragEnd, RadStart, QsRadioNotify, QsIsTile, QsTileHold, QsActivate, QsDraw.
- U5 hotkeys (9): ShellOpen, HkNeedExplorer, BootExePath, ShellSwitchToWindows, NormalSwitchToShell, HkTaskList, HotkeysStart, HotkeysStop, HotkeyRun. snap (3): SnapKey, SnapLayout, SnapLayoutMenu. winx (2): WinXStartOwner, WinXMenu.
- U6 overlay (5): TvIsOpen, TvToggle, TasksStart, TasksStop, TasksKey. tasks_ui (13): DeskWallLoad, DeskWallApply, DeskWallMenuAdd, DeskWallCmd, VdSave, VdRecover, VdScan, VdFocusNow, VdGo, VdNew, VdClose, VdMove, VdHud.
- U7 panel (10): PanM, PanEase, PanHit, PanHover, PanHitAt, PanelClose, PanelToggle, PanUri, PanKeys, PanelProc. widgets (14): FtU, SysSample, SysRelease, WidgetsVisibility, WidgetsLoad, WidgetsPlace, WidgetDraw, WidgetClick, WgInGrip, WgDown, WgDrag, WgUp, WgStep, WgMenu. calendar (3): CalMeasure, CalDraw, CalActivate. gesture_ui (2): GestStart, GestStop.
- U8 filesearch (3): FsStart, FsStop, FsAcceptHits. usb (2): UsbRefresh, UsbMenu. nl_ui (4, plus makro W12_LAZY): NlApiRefresh, NlApiOpen, NlApiClose, NlApiAcceptResult. diag_ui (3): DiagLog (diganti `crate::log::log`), DiagStart, DiagTick. stophosts_ui (1): StopHostsTick.
- Makro/alias, bukan fungsi: `InvalidateRect`, `UpdateWindow`.

Total fungsi shared f = 278: 257 nama, ditambah 21 kejadian makro `W12_LAZY` (nl_ui 11, flyout 10).

## 12. Verifikasi keberadaan nama per owner (2026-11-14, hanya grep)

Hanya memeriksa keberadaan nama `fn` di `src/` (nama snake_case dari bagian 4). Ini bukan bukti paritas perilaku.

| Owner | Nama (bagian 4) | Ditemukan | Catatan |
|---|---|---|---|
| C1 `core_c1.rs` | 42 nama pasti | 41 | `SafeRelease`: tidak ada fungsi. Padanannya RAII (Drop saat keluar scope), komentar `core_c1.rs:1935` |
| C2 `core_c2.rs` | 33 nama pasti | 33 | `MicaGetWallpaperStamp` = `mica_wallpaper_stamp` (`core_c2.rs:1652`) |
| C3 | 42 | 42 | Sebagian di `core_c1.rs:1371`, `hotkeys.rs:511`, `core_c3.rs`, `core_c3b.rs` |
| C4 | 7 | 7 | `core_c4.rs`, `core_c4b.rs` |
| U1 notch | 10 | 10 | `notch.rs` |
| U2 start | 14 | 14 | `start.rs`, `start_draw.rs` |
| U3 flyout | 13 | 13 | `flyout.rs` |
| U4 qs | 13 | 13 | `qs.rs` |
| U5 hotkeys + snap + winx | 9 + 3 + 2 = 14 | 14 | `hotkeys.rs`, `snap.rs`, `winx.rs` |
| U6 overlay + tasks_ui | 5 + 13 = 18 | 18 | `overlay.rs`, `tasks_ui.rs` |
| U7 panel + widgets + calendar + gesture_ui | 10 + 14 + 3 + 2 = 29 | 29 | `panel.rs`, `widgets.rs`, `calendar.rs`, `gesture_ui.rs` |
| U8 filesearch + usb + nl_ui + diag_ui + stophosts_ui | 3 + 2 + 4 + 3 + 1 = 13 | 13 | `StopHostsTick` = `stop_hosts_tick` (`stophosts_ui.rs:89`). `DiagLog` diganti `crate::log::log` (`log.rs:38`) |

Nama bertanda `?` dan makro/alias sudah dicek keberadaannya, lihat bagian 13 (2026-11-14).

## 5. Tipe shared (t = 14)

- `Icon` (C1:448), dipakai banyak owner.
- `PWSTR`: dari `windows::core`, bukan tipe lokal.
- `App` (C1:576), `WinInfo` (C1:772).
- `IAsyncOpRaw` (C1:1113): diganti `windows::core::IInspectable`.
- `IMediaSession` (C1:1125, 1152), `IMediaPlaybackInfo` (C1:1136), `IMediaProps` (C1:1140): diganti tipe crate `windows::Media::Control`.
- `IStreamRef` (C1:1179): butuh fitur `Storage_Streams`.
- `Item` (C2:1483), `Pill` (C2:1484).
- `PanHitRec` (panel:24), `FsResult` (filesearch:26, dipakai C4 dan filesearch).

Catatan usang (2026-11-14): sebagian besar tipe ini sudah ada di Rust, lihat bagian 13. Konstanta dan global yang memakainya masih PENDING (bagian 7).

## 6. Keputusan SH0 (usulan, menunggu persetujuan integrator)

1. Tipe didefinisikan di file owner. `state.rs` mengimpor `super::<owner>::Tipe` setelah owner ada. Sebelum itu item bertipe owner = PENDING.
2. File core: `core_c1.rs` s.d. `core_c4.rs`.
3. Global dan fungsi lintas-owner: global ke `state.rs`, fungsi ke `shell-funcs.md`.
4. Konstanta bersama: `pub const` di `state.rs`, tanpa `SyncCell`. Tipe `int` di C menjadi `i32`, termasuk konstanta MAX*.
5. Buffer wchar menjadi `String` (dalam `RefCell`). Array struct menjadi `Vec` (dalam `RefCell`). Array Copy kecil menjadi `Cell<[T; N]>`. wchar tunggal menjadi `Cell<u16>`.
6. Palette (`C`) menjadi `PALETTE: RefCell<Palette>`, PENDING.
7. COM non-Copy menjadi `RefCell<Option<T>>`. HWND, HDC, HANDLE, HINSTANCE, HTHUMBNAIL, HWINEVENTHOOK menjadi `Cell<T>` dengan `T(std::ptr::null_mut())`.
8. Pointer fungsi dinamis menjadi `Cell<Option<PfnXxx>>`. Typedef PFN ditulis di `core_c1.rs`.
9. `g_fsRes` menjadi `Cell<*mut FsResult>`, PENDING.
10. `g_radBusy` dan `g_wallGen` menjadi `RAD_BUSY` dan `WALL_GEN` (AtomicI32, sudah di `state.rs`).
11. GUID, IID, dan CLSID tidak dipindah (pakai crate). `PK_AUMID` ditulis lokal dengan `GUID::from_u128` karena `Win32_Storage_EnhancedStorage` belum aktif.
12. Media WinRT lewat `windows::Media::Control`. `g_mstat` dihapus. `IAsyncOpRaw` menjadi `IInspectable`. `IStreamRef` butuh `Storage_Streams`.
13. Makro `W12_LAZY` ada duplikat di nl_ui dan flyout. Implementasinya hanya satu, di `nl_ui.rs` (U8).
14. `InvalidateRect` dan `UpdateWindow` menjadi fungsi `w12_invalidate` dan `w12_update` di `core_c1.rs`.
15. Konflik nama: `g_netEth` menjadi `NET_ETH_OK`. `g_panNHit` menjadi `PAN_N_HIT`. `g_ntHud` menjadi `NT_HUD_KIND` (usulan baru, karena `NT_HUD` sudah dipakai konstanta state notch).

## 7. Isi state.rs dan item PENDING

Isi `state.rs` (deklarasi saja):
- Konstanta: `WM_*`, `T_*` (timer), layout C1, `MODE_*`, `MAXW`, `MAXAPPS`, `NET_*`, `W12_KBSIG`, `IT_*`, `MAXITEMS`, `MAXPILLS`, `NBOTTOM`, `P_*`, `PN_*`, `PAN_*` dan `PAN_ID_*`, `NT_*`, `HUD_*`, `NTT_*`, `OV_*`, `MAXWC`, `G_*`, `TK_*`, `TKT_*`, `VD_MAX`, `VD_WINS`, `SNL_*`, `FS_MAXHITS`, `FS_PATH`, `NL_API_*`, `NOTE_APP`, `NOTE_T1`, `NOTE_T2`, `PK_AUMID`, `QS_ON_INK`.
- Global `SyncCell` untuk grup C1, C2, C3, dan non-C. Daftar lengkap ada di `state.rs`.
- Atomik: `RAD_BUSY: AtomicI32`, `WALL_GEN: AtomicI32`.
- Tidak dipindah: CLSID dan IID dari crate, `g_mstat` (dihapus).

Item PENDING (belum dideklarasikan, sumber baris dari win12shell.cpp atau berkas owner):
- `PALETTE`: `C` (win12shell.cpp:179), tipe `Palette`, C1.
- `WALLPAPER_TINT`: `g_wallpaperTint` (:180), tipe `W12Rgb8`, C1.
- `PFN_GET_APP_USER_MODEL_ID`: `pGetApplicationUserModelId` (:548), C1.
- `PFN_CREATE_STREAM_OVER_RAS`: `pCreateStreamOverRandomAccessStream` (:1285), C1.
- `APPS`: `g_apps` (:589), `App[MAXAPPS]`, C1.
- `NOTE_ICON`: `g_noteIcon` (:592), tipe `Icon`, C1.
- `WC`: `g_wc` (:774), `WinInfo[MAXWC]`, C1.
- `MEDIA`: `g_media` (:1187), `MediaState`, C1.
- `ITEMS`: `g_items` (C2:1485), `Item[MAXITEMS]`.
- `PILLS`: `g_pills` (C2:1487), `Pill[MAXPILLS]`.
- `PAN_HIT`: `g_panHit` (panel:27), `PanHitRec[PAN_MAXHIT]`, owner panel.
- `FS_RES`: `g_fsRes` (filesearch:43), `FsResult*`, owner filesearch.
- `VD`: `g_vd` (tasks_ui:255), `VdWin[VD_WINS]`, owner tasks_ui.
- `NL_DEVICES`: `g_nlDevices` (nl_ui:7), tipe dari crate `nl_json`. Perlu `Option` karena bukan const fn.
- `HK_FG`, `HK_MIN`, `HK_CLOAK`, `HK_LOC`, `HK_LIFE`: `HWINEVENTHOOK` (C3). Butuh fitur `Win32_UI_Accessibility`, belum aktif.
- `THUMB`: `g_thumb` (C3:3957), `HTHUMBNAIL`. Tipe ini tidak ditemukan di windows 0.61.3.
- `NT_STATE`: `g_ntState` (notch:39), `int`, nilai awal `NT_IDLE`.
- `NT_HUD_KIND`: `g_ntHud` (notch:43), `int` berisi `HUD_VOL`/`HUD_BRI`/`HUD_MSG`. Nama `NT_HUD` dihindari karena sudah dipakai konstanta state (nilai 2).

## 8. Konstanta

Sudah dicocokkan dengan sumber, dan sudah ditulis di `state.rs`:
- `IT_*` (win12shell.cpp:1478–1479): IT_NOTE=0 s.d. IT_USB=16, berurutan.
- `NT_IDLE`, `NT_PEEK`, `NT_HUD`, `NT_OPEN` = 0, 1, 2, 3 (w12notch.inc:24).
- `HUD_VOL`, `HUD_BRI`, `HUD_MSG` = 0, 1, 2 (w12notch.inc:26).
- `NTT_HOME`, `NTT_TIMER`, `NTT_NOTES` = 0, 1, 2 (w12notch.inc:27).
- `OV_NONE`, `OV_SWITCH`, `OV_TV` = 0, 1, 2 (w12overlay.inc:5).
- `MAXWC` = 128 (win12shell.cpp:773).
- `G_NOTE`, `G_LEFT`, `G_CENTER`, `G_RIGHT`, `G_RUN`, `G_TRAY`, `G_COUNT` = 0 s.d. 6 (win12shell.cpp:1480).

Belum dicocokkan satu per satu: sebagian dari sekitar 167 konstanta kind `c` di analisis (`enum` dan `#define` di sumber). Hanya yang tercantum di atas yang sudah diverifikasi.

## 9. Kode Winlogon\Shell

SH0 tidak menulis kode yang mengubah `HKCU\...\Winlogon\Shell`. Penulisnya ada di `win12boot.c`: `DelValue` di baris 270 dan 368, `RegSetKeyValueW` di baris 284. Pembacaan di `win12shell.cpp:3405` hanya membaca nilai (`RegGetValueW`).

Aturan untuk port-nya (ATURAN-AGEN.md, aturan 2): kode yang menulis wajib diberi komentar `// PORT: menulis Winlogon\Shell, JANGAN dijalankan`, tidak boleh dipanggil dari tes, dan wajib disebut di laporan. Owner `win12boot.c` belum ada di tabel bagian 3.

## 10. Verifikasi

- `cargo check --lib --message-format=short`, difilter `shell.(state|mod)[.]rs`, dijalankan setelah konstanta bagian 8 ditambahkan: 0 baris (0 error, 0 warning) untuk `state.rs` dan `mod.rs`.
- Catatan usang: entri lama bahwa lib gagal di `src\files\` (8 error) sudah tidak berlaku. Status terbaru 2026-11-14: `cargo check --lib --bins -j 2` rc=0, 0 error, 0 warning (log `%TEMP%\w12_check_s8.log`, 14:58).
- pencarian penanda sisa pengerjaan (kata LANJUT dengan @ ganda di kedua sisi): `state.rs` = 0, `mod.rs` = 0, dokumen ini = 0.
- Bagian 8 diverifikasi dengan `Grep` dan `Read` terhadap sumber (baris disebut di atas). Tidak ada tes yang dijalankan.

## 11. Belum selesai dan permintaan

- Catatan usang: entri lama bahwa logika belum diport dan file owner belum dibuat tidak berlaku. File `core_c1.rs` s.d. `core_c4.rs` dan U1 s.d. U8 sudah ada di `src\shell\`. Keberadaan nama sudah dicek (bagian 12). Perilaku per fungsi belum dibandingkan dengan C++, dan tidak ada klaim paritas runtime.
- `shell-funcs.md` belum ditulis.
- Catatan usang: sebagian besar tipe owner di bagian 5 sudah ada (lihat bagian 13). Item PENDING di bagian 7 belum diperbarui.
- Fitur crate yang diminta: `Win32_UI_Accessibility` (HWINEVENTHOOK), `Storage_Streams` (IStreamRef), `Win32_Storage_EnhancedStorage` (opsional, PK_AUMID). `HTHUMBNAIL` tidak ditemukan di windows 0.61.3.
- Keputusan 1 s.d. 15 (bagian 6) menunggu persetujuan integrator. Keputusan 15 (`NT_HUD_KIND`) adalah usulan baru.
- Berkas tanpa owner: `w12fm_bar.inc`, `w12fm_search.inc`, `win12boot.c`.
- Bagian 4: tanda `?` sudah dicek keberadaan padanannya (bagian 13). Perilaku belum dibandingkan.

## 13. Pemeriksaan ulang 2026-11-14 (grep read-only)

Hanya bukti keberadaan nama dan baris. Ini bukan bukti paritas perilaku.

Padanan nama bertanda `?` (bagian 4) di Rust:
- `S` → `s` (`core_c1.rs:249`); `SF` → `sf` (`core_c1.rs:254`)
- `RGBA` → `rgba` (`core_c1.rs:419`); `RF` → `rf` (`core_c1.rs:687`)
- `Text` → `text` (`core_c1.rs:726`); `Glyph` → `glyph` (`core_c1.rs:739`)
- `Copy` → `copy_wide` (`core_c1.rs:1078`)
- `HStr` → `h_str` (`core_c1.rs:2567`). Koreksi: sebelumnya tertulis `HString` dengan 4 pemakaian. Grep `HString` di `core_c1.rs` = 0. Yang ada ialah `HSTRING` dari crate (`core_c1.rs:95`).
- `Surface` → `surface` (`core_c2.rs:2146`)
- `MicaGetWallpaperStamp` → `mica_wallpaper_stamp` (`core_c2.rs:1652`)
- `StopHostsTick` → `stop_hosts_tick` (`stophosts_ui.rs:89`)

Makro/alias:
- `InvalidateRect` → `w12_invalidate` (`core_c1.rs:286`)
- `UpdateWindow` → `w12_update` (`core_c1.rs:300`)

Tipe bagian 5 yang sudah ada: `Icon` (`core_c1.rs:768`), `App` (`core_c1.rs:1121`), `WinInfo` (`core_c1.rs:1517`), `MediaState` (`core_c1.rs:2354`), `Item` (`core_c2.rs:115`), `Pill` (`core_c2.rs:129`), `PanHitRec` (`panel.rs:10`), `FsResult` (`filesearch.rs:64`), `VdWin` (`tasks_ui.rs:542`). `Palette` juga ada (`core_c1.rs:191`). `W12Rgb8` ada di `src\ui_logic.rs:6`, bukan di `src\shell\`.

Belum diperiksa: apakah global PENDING di bagian 7 (`PALETTE`, `VD`, dan lainnya) sudah dideklarasikan, serta perilaku semua nama di atas.

Item multi-monitor dan wallpaper (bukti pencarian API saja, belum uji runtime):
- Wallpaper dari Settings: C++ `win12shell.cpp:5046–5048` dan `:3571` (WM_SETTINGCHANGE). Rust `core_c3b.rs:470` dan `core_c4b.rs:1059–1062`.
- Pemilihan monitor: C++ `win12shell.cpp:3925` dan `:4141` (`MONITOR_DEFAULTTOPRIMARY`/`MonitorFromWindow`). Rust `core_c4.rs:353` dan `core_c3b.rs:1150`. Enumerasi monitor untuk wallpaper: `tasks_ui.rs:149` (EnumDisplayMonitors).
- Belum diuji pada layar banyak.

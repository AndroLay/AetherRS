//! Sumber: w12panel.inc. Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime.
//! Satu jendela berlapis (PAN) untuk Start, Pengaturan Cepat, Wi-Fi, Baterai, dan Kalender.
//! Deviasi: PAN_T0 memakai Instant sebagai pengganti QueryPerformanceCounter. Progres tetap berbasis waktu.
use super::prelude::*;
use std::time::Instant;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;

#[derive(Clone, Copy)]
pub struct PanHitRec {
    pub r: D2D_RECT_F,
    pub id: i32,
}

const PAN_HIT_EMPTY: PanHitRec = PanHitRec {
    r: D2D_RECT_F { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 },
    id: 0,
};

pub static PAN_HIT: SyncCell<RefCell<[PanHitRec; PAN_MAXHIT as usize]>> =
    SyncCell::new(RefCell::new([PAN_HIT_EMPTY; PAN_MAXHIT as usize]));
pub static PAN_DOWN: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(-1));
pub static PAN_TRACK: SyncCell<Cell<bool>> = SyncCell::new(Cell::new(false));
pub static PAN_T: SyncCell<Cell<f32>> = SyncCell::new(Cell::new(1.0));
pub static PAN_T0: SyncCell<Cell<Option<Instant>>> = SyncCell::new(Cell::new(None));
pub static PAN_ANCHOR: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
pub static PAN_BAR: SyncCell<Cell<i32>> = SyncCell::new(Cell::new(0));
const PAN_ANIM_MS: f32 = 190.0;

fn pan_m() -> i32 {
    s(PAN_MARGIN as f32)
}

// ease-out kuartik (deselerasi)
fn pan_ease() -> f32 {
    let u = 1.0 - PAN_T.get();
    1.0 - u * u * u * u
}

// Kotak klik dalam koordinat lokal isi.
pub fn pan_hit(l: f32, t: f32, r: f32, b: f32, id: i32) {
    let n = PAN_N_HIT.get();
    if n >= PAN_MAXHIT {
        return;
    }
    PAN_HIT.borrow_mut()[n as usize] = PanHitRec { r: rf(l, t, r, b), id };
    PAN_N_HIT.set(n + 1);
}

// Daftarkan kotak klik sekaligus menggambar lapisan hover/tekan.
// Dipakai juga oleh calendar.rs.
pub fn pan_hover(l: f32, t: f32, r: f32, b: f32, rad: f32, id: i32) {
    pan_hit(l, t, r, b, id);
    let pal = *PALETTE.borrow();
    if PAN_DOWN.get() == id {
        fill_rr(l, t, r, b, rad, pal.press, 1.0);
    } else if PAN_HOV.get() == id {
        fill_rr(l, t, r, b, rad, pal.hover, 1.0);
    }
}

pub fn pan_hit_at(x: i32, y: i32) -> i32 {
    let m = pan_m();
    let lx = (x - m) as f32;
    let ly = (y - m) as f32;
    let n = PAN_N_HIT.get() as usize;
    let hits = PAN_HIT.borrow();
    for i in (0..n).rev() {
        let r = hits[i].r;
        if lx >= r.left && lx < r.right && ly >= r.top && ly < r.bottom {
            return hits[i].id;
        }
    }
    -1
}

// Bayangan lembut tanpa blur GPU: lapisan berlapis yang makin tipis ke luar.
fn pan_shadow(w: f32, h: f32, rad: f32) {
    let pal = *PALETTE.borrow();
    for i in 0..8 {
        let e = sf((i * 2) as f32);
        fill_rr(-e, -e + sf(5.0), w + e, h + e + sf(5.0), rad + e, pal.shadow, 0.07);
    }
}

fn panel_content(w: f32, h: f32) {
    match PANEL_OPEN.get() {
        PN_START => start_draw(w, h),
        PN_QS => qs_draw(w, h),
        PN_WIFI => wifi_draw(w, h),
        PN_BATT => batt_draw(w, h),
        PN_CAL => cal_draw(w, h),
        _ => {}
    }
}

pub fn panel_measure(id: i32, cw: &mut i32, ch: &mut i32) {
    *cw = 0;
    *ch = 0;
    match id {
        PN_START => start_measure(cw, ch),
        PN_QS => qs_measure(cw, ch),
        PN_WIFI => wifi_measure(cw, ch),
        PN_BATT => batt_measure(cw, ch),
        PN_CAL => cal_measure(cw, ch),
        _ => {}
    }
}

pub fn panel_activate(id: i32) {
    match PANEL_OPEN.get() {
        PN_START => start_activate(id),
        PN_QS => qs_activate(id),
        PN_WIFI => wifi_activate(id),
        PN_BATT => batt_activate(id),
        PN_CAL => cal_activate(id),
        _ => {}
    }
}

// Start terpusat di area kerja; panel lain di atas ikon pemicu.
// Tepi bawah isi = puncak dok - PAN_GAP. Mengembalikan (x, y, lebar, tinggi) jendela.
fn panel_place(cw: i32, ch: i32) -> (i32, i32, i32, i32) {
    let anchor = PAN_ANCHOR.get();
    let bar = PAN_BAR.get();
    let pt = POINT { x: anchor, y: bar - 1 };
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let _ = GetMonitorInfoW(mon, &mut mi);
    }
    let wa = mi.rcWork;
    let m = pan_m();
    let edge = s(PAN_EDGE as f32);
    let bottom = bar - s(PAN_GAP as f32);
    let mut x = if PANEL_OPEN.get() == PN_START {
        (wa.left + wa.right - cw) / 2
    } else {
        anchor - cw / 2
    };
    if x < wa.left + edge {
        x = wa.left + edge;
    }
    if x + cw > wa.right - edge {
        x = wa.right - edge - cw;
    }
    let mut y = bottom - ch;
    if y < wa.top + edge {
        y = wa.top + edge;
    }
    (x - m, y - m, cw + 2 * m, ch + 2 * m)
}

pub fn panel_render(h: HWND) {
    unsafe {
        let _ = RemovePropW(h, w!("w12p"));
    }
    if h.0.is_null() || PANEL_OPEN.get() < 0 {
        return;
    }
    let Some(rt) = RT.borrow().as_ref().cloned() else { return };
    let mut wr = RECT::default();
    unsafe {
        let _ = GetWindowRect(h, &mut wr);
    }
    let ww = wr.right - wr.left;
    let wh = wr.bottom - wr.top;
    let m = pan_m();
    if ww <= 0 || wh <= 0 {
        return;
    }
    let cw = (ww - 2 * m) as f32;
    let ch = (wh - 2 * m) as f32;
    ensure_mem(ww, wh);
    let br = RECT { left: 0, top: 0, right: ww, bottom: wh };
    if unsafe { rt.BindDC(MEMDC.get(), &br) }.is_err() {
        return;
    }
    let ease = pan_ease();
    let rise = (1.0 - ease) * sf(PAN_RISE as f32);
    let fm = m as f32;
    let origin = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: fm, M32: fm + rise };
    let ident = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 };
    PAN_N_HIT.set(0);
    let rad = sf(PAN_RADIUS as f32);
    unsafe {
        rt.BeginDraw();
        rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
        rt.SetTransform(&origin);
    }
    // Kaca Mica: potongan wallpaper tetap di posisi layarnya sementara panel naik.
    mica_begin(h, &wr);
    pan_shadow(cw, ch, rad);
    let pal = *PALETTE.borrow();
    surface(0.0, 0.0, cw, ch, rad, pal.panel);
    panel_content(cw, ch);
    unsafe {
        rt.SetTransform(&ident);
    }
    let hr = unsafe { rt.EndDraw(None, None) };
    if let Err(e) = hr {
        if e.code() == D2DERR_RECREATE_TARGET {
            create_target();
            let _ = w12_invalidate(h, None, false);
            return;
        }
    }
    layer_present(h, &wr, (255.0 * ease) as u8);
}

fn panel_tick() {
    let now = Instant::now();
    let t0 = PAN_T0.get().unwrap_or(now);
    let mut t = now.duration_since(t0).as_secs_f32() * 1000.0 / PAN_ANIM_MS;
    if t >= 1.0 {
        t = 1.0;
        unsafe {
            let _ = KillTimer(Some(PAN.get()), 1);
        }
    }
    PAN_T.set(t);
    panel_render(PAN.get());
}

pub fn panel_close() {
    let was = PANEL_OPEN.get();
    if was < 0 {
        return;
    }
    let close_fullscreen_peek = was == PN_START && FULLSCREEN_START_PEEK.get();
    let restore_fullscreen_mica = close_fullscreen_peek && MICA_FULLSCREEN_SNAPSHOT.get();
    // dulu: ShowWindow(SW_HIDE) memicu WM_ACTIVATE yang memanggil panel_close lagi
    PANEL_OPEN.set(-1);
    PAN_HOV.set(-1);
    PAN_DOWN.set(-1);
    PAN_DRAG.set(false);
    let pan = PAN.get();
    if !pan.0.is_null() {
        unsafe {
            if GetCapture() == pan {
                let _ = ReleaseCapture();
            }
            let _ = KillTimer(Some(pan), 1);
            let _ = ShowWindow(pan, SW_HIDE);
        }
    }
    if was == PN_START {
        FULLSCREEN_START_PEEK.set(false);
        kill_timer(T_NL);
        kill_timer(T_NL_ICON);
        nl_api_close();
        start_reset();
    }
    // lepas koneksi WMI kecerahan: tidak ada sisa pekerjaan saat panel tertutup
    if was == PN_QS {
        qs_release();
    }
    if restore_fullscreen_mica {
        mica_restore_wallpaper();
    }
    // DIB bersama sempat membesar seukuran panel; dibuat lagi seukuran dok saat render berikutnya
    release_mem();
    // halaman milik panel dibuang dari RAM 3 detik kemudian
    trim_soon();
    if close_fullscreen_peek {
        base_update(false);
    }
    // pil atas kembali bila tidak ada lagi yang menutupinya
    apply_pills(false);
}

pub fn panel_open(id: i32, anchor_x: i32, bar_top: i32) {
    let mut pan = PAN.get();
    if pan.0.is_null() {
        let created = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("Win12ShellPanel"),
                w!(""),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                Some(HI.get()),
                None,
            )
        };
        let Ok(hwnd) = created else { return };
        pan = hwnd;
        PAN.set(pan);
        // berlapis milik kita (prop "w12l"): w12_invalidate mengirim WM_W12_RENDER, bukan InvalidateRect mentah.
        unsafe {
            let _ = SetPropW(pan, w!("w12l"), Some(HANDLE(1 as *mut std::ffi::c_void)));
        }
    }
    // harus sebelum apply_pills: TopBlocked membaca PANEL_OPEN
    PANEL_OPEN.set(id);
    PAN_ANCHOR.set(anchor_x);
    PAN_BAR.set(bar_top);
    PAN_HOV.set(-1);
    PAN_DOWN.set(-1);
    PAN_DRAG.set(false);
    PAN_TRACK.set(false);
    QS_SCROLL.set(0);
    PAN_T.set(0.0);
    PAN_T0.set(Some(Instant::now()));
    let mut cw = 0;
    let mut ch = 0;
    panel_measure(id, &mut cw, &mut ch);
    if id == PN_START {
        set_timer(T_NL, 10000);
        set_timer(T_NL_ICON, 300);
    }
    let (wx, wy, ww, wh) = panel_place(cw, ch);
    unsafe {
        let _ = SetWindowPos(pan, Some(HWND_TOPMOST), wx, wy, ww, wh, SWP_NOACTIVATE);
    }
    panel_render(pan);
    unsafe {
        let _ = ShowWindow(pan, SW_SHOWNOACTIVATE);
        // aktif: klik di luar panel menutupnya (WM_ACTIVATE); Esc juga
        let _ = SetForegroundWindow(pan);
    }
    // dibuka dari tombol Windows: fokus masih dipegang aplikasi lain
    unsafe {
        if GetForegroundWindow() != pan {
            let fg = GetForegroundWindow();
            let ft = if fg.0.is_null() { 0 } else { GetWindowThreadProcessId(fg, None) };
            let me = GetCurrentThreadId();
            // PORT: AttachThreadInput menerima fattach: bool di binding windows 0.61.
            if ft != 0 && ft != me && AttachThreadInput(me, ft, true).as_bool() {
                let _ = SetForegroundWindow(pan);
                let _ = AttachThreadInput(me, ft, false);
            }
        }
        SetTimer(Some(pan), 1, 16, None);
    }
    // panel terbuka: pil atas disembunyikan (TopBlocked)
    apply_pills(false);
}

pub fn panel_toggle(id: i32, anchor_x: i32, bar_top: i32) {
    if PANEL_OPEN.get() == id {
        panel_close();
        return;
    }
    // flyout tray lama tidak tampil bersama panel
    if FLY_OPEN.get() {
        flyout_close();
    }
    // panel lain yang terbuka ditutup dulu: pil tidak mengaktifkan jendelanya,
    // jadi WM_ACTIVATE tidak terpicu
    panel_close();
    panel_open(id, anchor_x, bar_top);
}

// Aksi yang menutup panel dulu, lalu membuka sesuatu di luar (Pengaturan, aplikasi, pintasan).
pub fn pan_uri(uri: &str) {
    panel_close();
    // mode shell: app Store dan Pengaturan tanpa Explorer bila bisa
    shell_open(uri);
}

pub fn pan_keys(k1: u16, k2: u16, k3: u16) {
    panel_close();
    send_keys(k1, k2, k3);
}

pub fn panel_drag(x: i32) {
    if PANEL_OPEN.get() == PN_QS {
        qs_drag((x - pan_m()) as f32);
    }
}

// Parameter lParam/wParam dibongkar manual (LOWORD/HIWORD bertanda), sama dengan GET_X/Y_LPARAM.
fn lp_x(lp: LPARAM) -> i32 {
    (lp.0 & 0xFFFF) as u16 as i16 as i32
}

fn lp_y(lp: LPARAM) -> i32 {
    ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32
}

fn wheel_delta(wp: WPARAM) -> i32 {
    ((wp.0 >> 16) & 0xFFFF) as u16 as i16 as i32
}

pub unsafe extern "system" fn panel_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_WINDOWPOSCHANGED {
        // dipindah: potongan wallpaper kaca ikut posisi baru
        mica_moved(h, lp);
    }
    if START_CONTEXT_MENU_OPEN.get() {
        let mut context_result = LRESULT(0);
        if start_context_menu_message(h, m, wp, lp, &mut context_result) {
            return context_result;
        }
    }
    match m {
        WM_NCHITTEST => return LRESULT(HTCLIENT as isize),
        WM_NCACTIVATE => return DefWindowProcW(h, m, WPARAM(1), lp),
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(h, &mut ps);
            let _ = EndPaint(h, &ps);
            return LRESULT(0);
        }
        WM_W12_RENDER => {
            panel_render(h);
            return LRESULT(0);
        }
        WM_TIMER => {
            match wp.0 {
                1 => panel_tick(),
                // ubin Pengaturan Cepat ditahan 0,5 detik: buka pengaturannya, lepas tombol tidak mengaktifkan
                2 => {
                    let _ = KillTimer(Some(h), 2);
                    if PANEL_OPEN.get() == PN_QS && qs_is_tile(PAN_DOWN.get()) {
                        let id = PAN_DOWN.get();
                        PAN_DOWN.set(-1);
                        qs_tile_hold(id); // menutup panel
                    }
                }
                88 => {
                    let _ = KillTimer(Some(h), 88);
                    w12_invalidate(h, None, false);
                }
                _ => {}
            }
            return LRESULT(0);
        }
        WM_ACTIVATE => {
            // klik di luar = tutup (light dismiss)
            if (wp.0 & 0xFFFF) == WA_INACTIVE as usize && !START_CONTEXT_MENU_OPEN.get() {
                panel_close();
            }
            return LRESULT(0);
        }
        WM_CONTEXTMENU => {
            if PANEL_OPEN.get() == PN_START {
                start_context_menu(h, lp_x(lp), lp_y(lp));
                return LRESULT(0);
            }
        }
        WM_KEYDOWN => {
            let key = wp.0 as u16;
            let shift = (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
            if PANEL_OPEN.get() == PN_START && (key == VK_APPS.0 || (key == VK_F10.0 && shift)) {
                start_context_menu(h, -1, -1);
                return LRESULT(0);
            }
            if PANEL_OPEN.get() == PN_START && start_key(wp.0 as i32) {
                w12_invalidate(h, None, false);
            } else if key == VK_ESCAPE.0 {
                panel_close();
            }
            return LRESULT(0);
        }
        WM_CHAR => {
            if PANEL_OPEN.get() == PN_START {
                start_char(wp.0 as u16);
                w12_invalidate(h, None, false);
            }
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            if !PAN_TRACK.get() {
                let mut t = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: h,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut t);
                PAN_TRACK.set(true);
            }
            let x = lp_x(lp);
            let y = lp_y(lp);
            if PAN_DRAG.get() {
                panel_drag(x);
            }
            let hit = pan_hit_at(x, y);
            if hit != PAN_HOV.get() {
                PAN_HOV.set(hit);
                w12_invalidate(h, None, false);
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            PAN_TRACK.set(false);
            if PAN_HOV.get() != -1 {
                PAN_HOV.set(-1);
                w12_invalidate(h, None, false);
            }
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            let x = lp_x(lp);
            let y = lp_y(lp);
            PAN_DOWN.set(pan_hit_at(x, y));
            let down = PAN_DOWN.get();
            if down == PAN_ID_VOL_TRACK || down == PAN_ID_BRI_TRACK {
                PAN_DRAG.set(true);
                SetCapture(h);
                panel_drag(x);
            }
            if PANEL_OPEN.get() == PN_QS && qs_is_tile(down) {
                SetTimer(Some(h), 2, 500, None);
            }
            w12_invalidate(h, None, false);
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let x = lp_x(lp);
            let y = lp_y(lp);
            // dilepas sebelum 0,5 detik: ketuk biasa
            let _ = KillTimer(Some(h), 2);
            if PAN_DRAG.get() {
                PAN_DRAG.set(false);
                let _ = ReleaseCapture();
                if PANEL_OPEN.get() == PN_QS {
                    qs_drag_end();
                }
            }
            let hit = pan_hit_at(x, y);
            let down = PAN_DOWN.get();
            PAN_DOWN.set(-1);
            if hit >= 0 && hit == down {
                panel_activate(hit); // bisa menutup panel
            }
            if PANEL_OPEN.get() >= 0 {
                w12_invalidate(h, None, false);
            }
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            let delta = wheel_delta(wp);
            match PANEL_OPEN.get() {
                PN_QS => {
                    // koordinat layar diubah ke klien
                    let mut pt = POINT { x: lp_x(lp), y: lp_y(lp) };
                    let _ = ScreenToClient(h, &mut pt);
                    // di atas slider: ubah nilai; selain itu: gulir ubin
                    qs_wheel(delta, pt.y - pan_m());
                    w12_invalidate(h, None, false);
                }
                PN_WIFI => {
                    // gulir daftar jaringan
                    wifi_wheel(delta);
                    w12_invalidate(h, None, false);
                }
                PN_START => {
                    start_wheel(delta);
                    w12_invalidate(h, None, false);
                }
                _ => {}
            }
            return LRESULT(0);
        }
        WM_W12_BRI => {
            qs_bri_notify(wp, lp);
            return LRESULT(0);
        }
        WM_SETCURSOR => {
            if (lp.0 & 0xFFFF) == HTCLIENT as isize && PAN_HOV.get() >= 0 {
                if let Ok(cur) = LoadCursorW(None, IDC_HAND) {
                    let _ = SetCursor(Some(cur));
                }
                return LRESULT(1);
            }
        }
        _ => {}
    }
    DefWindowProcW(h, m, wp, lp)
}

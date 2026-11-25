//! Sumber: w12usb.inc (seluruh file). Status: kode lengkap; cargo check lib+bins lolos 2026-11-14; belum diuji runtime. Pemilik: H15.
//! Dipanggil integrator: `usb_refresh(arrival)` dari WM_DEVICECHANGE, `usb_menu(x, y)` dari klik ikon USB di dok.
//! Deviasi: label String (buffer 64 wchar seperti C); daftar `USB_VOLS` berupa Vec maks 8; notifikasi lewat `NT_MSG` (String, state.rs) seperti hotkeys.rs.
use super::prelude::*;
use std::ffi::c_void;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Device_IDW, CM_Get_Parent, CM_Request_Device_EjectW, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
    CR_REMOVE_VETOED, CR_SUCCESS, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, MAX_DEVICE_ID_LEN,
    PNP_VETO_TYPE, PNP_VetoTypeUnknown, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    SP_DEVINFO_DATA,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, FILE_FLAGS_AND_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    GUID_DEVINTERFACE_DISK, IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER,
};
use windows::Win32::System::WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOVABLE};

const USB_MAX: usize = 8;

/// C: `UsbVol`. Satu drive USB: huruf, label volume, dan simpul induk USB (DEVINST = u32).
#[derive(Clone)]
pub struct UsbVol {
    pub letter: char,
    pub label: String,
    pub inst: u32,
}

/// C: `g_usb` + `g_usbCount`. Jumlah = `USB_COUNT`.
pub static USB_VOLS: SyncCell<RefCell<Vec<UsbVol>>> = SyncCell::new(RefCell::new(Vec::new()));

/// Nomor disk fisik dari handle volume/disk lewat IOCTL. None bila IOCTL gagal.
fn usb_storage_number(h: HANDLE) -> Option<u32> {
    let mut sdn = STORAGE_DEVICE_NUMBER::default();
    let mut got: u32 = 0;
    let r = unsafe {
        DeviceIoControl(
            h,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            None,
            0,
            Some(&mut sdn as *mut STORAGE_DEVICE_NUMBER as *mut c_void),
            std::mem::size_of::<STORAGE_DEVICE_NUMBER>() as u32,
            Some(&mut got as *mut u32),
            None,
        )
    };
    if r.is_err() {
        return None;
    }
    Some(sdn.DeviceNumber)
}

/// C: `UsbDiskNumber`. Nomor disk fisik dari huruf drive. Akses 0: tanpa hak administrator.
fn usb_disk_number(letter: char) -> Option<u32> {
    let path = to_wide_z(&format!(r"\\.\{letter}:"));
    let h = unsafe {
        CreateFileW(
            pcw(&path),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .ok()?;
    let num = usb_storage_number(h);
    let _ = unsafe { CloseHandle(h) };
    num
}

/// C: `UsbParentOf`. Naik dari simpul disk sampai induk pertama berawalan "USB\"; 0 bila tidak ada (SD bawaan, SSD internal).
fn usb_parent_of(mut dn: u32) -> u32 {
    for _ in 0..4 {
        let mut p: u32 = 0;
        let cr = unsafe { CM_Get_Parent(&mut p, dn, 0) };
        if cr.0 != CR_SUCCESS.0 {
            return 0;
        }
        let mut id = buf(MAX_DEVICE_ID_LEN as usize);
        let cr = unsafe { CM_Get_Device_IDW(p, &mut id[..], 0) };
        if cr.0 != CR_SUCCESS.0 {
            return 0;
        }
        if from_wide(&id).starts_with("USB\\") {
            return p;
        }
        dn = p;
    }
    0
}

/// C: `UsbNodeOfDisk`. Simpul USB induk dari disk bernomor `num`; 0 bila disk itu bukan USB.
fn usb_node_of_disk(num: u32) -> u32 {
    let set = unsafe {
        SetupDiGetClassDevsW(
            Some(&GUID_DEVINTERFACE_DISK as *const _),
            PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    let Ok(set) = set else {
        return 0;
    };
    let mut usb: u32 = 0;
    // 1 KB; u32 agar sejajar untuk SP_DEVICE_INTERFACE_DETAIL_DATA_W.
    let mut det_buf = vec![0u32; 256];
    let cap = (det_buf.len() * std::mem::size_of::<u32>()) as u32;
    for i in 0u32.. {
        let mut ifd = SP_DEVICE_INTERFACE_DATA {
            cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        let enumerated = unsafe {
            SetupDiEnumDeviceInterfaces(set, None, &GUID_DEVINTERFACE_DISK, i, &mut ifd)
        };
        if enumerated.is_err() {
            break;
        }
        let mut dd = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        let det = det_buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        // SAFETY: det_buf berukuran 1 KB dan sejajar DWORD; cbSize diisi sesuai ukuran struct.
        unsafe {
            (*det).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        }
        let detailed = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                set,
                &ifd,
                Some(det),
                cap,
                None,
                Some(&mut dd as *mut SP_DEVINFO_DATA),
            )
        };
        if detailed.is_err() {
            continue;
        }
        // DevicePath mulai di byte 4 (setelah cbSize), string ber-NUL di dalam buffer.
        let path = unsafe { (det_buf.as_ptr() as *const u16).add(2) };
        let h = unsafe {
            CreateFileW(
                PCWSTR(path),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        };
        let Ok(h) = h else {
            continue;
        };
        let num_here = usb_storage_number(h);
        let _ = unsafe { CloseHandle(h) };
        if num_here == Some(num) {
            usb = usb_parent_of(dd.DevInst);
            break;
        }
    }
    let _ = unsafe { SetupDiDestroyDeviceInfoList(set) };
    usb
}

/// C: `UsbList`. Drive USB yang terpasang (flashdisk, HDD eksternal). Drive internal dan kartu SD bawaan tidak tercakup.
fn usb_list() -> Vec<UsbVol> {
    let mut out: Vec<UsbVol> = Vec::new();
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if out.len() >= USB_MAX {
            break;
        }
        if (mask & (1u32 << i)) == 0 {
            continue;
        }
        let letter = char::from(b'A' + i as u8);
        let root = to_wide_z(&format!("{letter}:\\"));
        let dtype = unsafe { GetDriveTypeW(pcw(&root)) };
        if dtype != DRIVE_REMOVABLE && dtype != DRIVE_FIXED {
            continue;
        }
        let Some(num) = usb_disk_number(letter) else {
            continue;
        };
        let inst = usb_node_of_disk(num);
        if inst == 0 {
            continue;
        }
        let mut lbuf = buf(64);
        let _ = unsafe {
            GetVolumeInformationW(pcw(&root), Some(&mut lbuf[..]), None, None, None, None)
        };
        let mut label = from_wide(&lbuf);
        if label.is_empty() {
            label = format!("Drive {letter}:");
        }
        out.push(UsbVol { letter, label, inst });
    }
    out
}

/// C: `UsbEject`. Keluarkan drive (sama dengan "Safely Remove Hardware"). Err berisi teks alasan gagal.
fn usb_eject(v: &UsbVol) -> Result<(), String> {
    let mut veto = PNP_VetoTypeUnknown;
    let mut name = buf(260); // MAX_PATH
    let cr = unsafe {
        CM_Request_Device_EjectW(
            v.inst,
            Some(&mut veto as *mut PNP_VETO_TYPE),
            Some(&mut name[..]),
            0,
        )
    };
    if cr.0 == CR_SUCCESS.0 {
        return Ok(());
    }
    let name_s = from_wide(&name);
    let mut why = buf(160);
    let _ = w12_usb_eject_fail_text(
        cr.0 == CR_REMOVE_VETOED.0,
        cr.0,
        Some(name_s.as_str()),
        &mut why[..],
    );
    Err(from_wide(&why))
}

/// C: `UsbNotice`. Notifikasi HUD notch. Bila notch terbuka: repaint via w12_invalidate; selain itu NtShowTemp 6000 ms.
fn usb_notice(text: &str, warn: bool) {
    if super::notch::NT_WND.get().0.is_null() {
        return;
    }
    // Buffer C 96 wchar_t: maksimal 95 karakter.
    *NT_MSG.borrow_mut() = text.chars().take(95).collect();
    NT_MSG_WARN.set(warn);
    // glyph pengisi daya (NT_GLYPH_CHG) tidak boleh terbawa ke pesan USB
    NT_MSG_GLYPH.set(0);
    NT_HUD_KIND.set(HUD_MSG);
    if NT_STATE.get() == NT_OPEN {
        let _ = super::core_c1::w12_invalidate(super::notch::NT_WND.get(), None, false);
    } else {
        nt_show_temp(NT_HUD, 6000);
    }
}

/// C: `UsbRefresh`. Membaca ulang daftar. arrival: drive yang baru muncul diberi tahu "<label> terhubung" (mode shell saja).
pub fn usb_refresh(arrival: bool) {
    let old: Vec<char> = USB_VOLS.borrow().iter().map(|v| v.letter).collect();
    let list = usb_list();
    USB_COUNT.set(list.len() as i32);
    let fresh = list
        .iter()
        .find(|v| !old.contains(&v.letter))
        .map(|v| format!("{} terhubung", v.label));
    *USB_VOLS.borrow_mut() = list;
    if !arrival || !SHELL_MODE.get() {
        return;
    }
    if let Some(t) = fresh {
        usb_notice(&t, false);
    }
}

/// C: `UsbMenu`. Menu ikon USB di dok: per drive "Keluarkan" (id 100+i) dan "Buka" (id 200+i).
/// x, y = posisi ikon di layar (menu terbuka ke atas).
pub fn usb_menu(x: i32, y: i32) {
    usb_refresh(false);
    // Salinan: menu bersifat modal, daftar global bisa berubah selama menu terbuka.
    let snap: Vec<UsbVol> = USB_VOLS.borrow().to_vec();
    if snap.is_empty() {
        return;
    }
    let Ok(m) = (unsafe { CreatePopupMenu() }) else {
        return;
    };
    for (i, v) in snap.iter().enumerate() {
        let t_eject = to_wide_z(&format!("Keluarkan {} ({}:)", v.label, v.letter));
        let t_open = to_wide_z(&format!("Buka {}:", v.letter));
        let _ = unsafe { AppendMenuW(m, MF_STRING, 100 + i, pcw(&t_eject)) };
        let _ = unsafe { AppendMenuW(m, MF_STRING, 200 + i, pcw(&t_open)) };
    }
    let _ = unsafe { SetForegroundWindow(MSG.get()) };
    let picked = unsafe {
        TrackPopupMenu(
            m,
            TPM_RETURNCMD | TPM_BOTTOMALIGN | TPM_CENTERALIGN,
            x,
            y,
            None,
            MSG.get(),
            None,
        )
    };
    let _ = unsafe { DestroyMenu(m) };
    let cmd: i32 = picked.0;
    let idx = if cmd >= 200 { cmd - 200 } else { cmd - 100 };
    if cmd <= 0 || idx < 0 || idx as usize >= snap.len() {
        return;
    }
    let v = &snap[idx as usize];
    if cmd >= 200 {
        // "E:\." : tanpa tanda kutip tertutup setelah backslash (CommandLineToArgvW membaca \" sebagai kutip).
        open_folder_window(&format!("{}:\\.", v.letter));
        return;
    }
    match usb_eject(v) {
        Ok(()) => usb_notice(&format!("Aman untuk mencabut {}", v.label), false),
        Err(why) => usb_notice(&why, true),
    }
}

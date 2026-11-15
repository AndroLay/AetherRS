//! Build script: menautkan manifest Win12Shell (DPI PerMonitorV2, Common Controls v6, supportedOS)
//! ke bin `win12shell` lewat windres. Sumber: `win12shell.rc` di proyek C++ hanya memuat manifest
//! (RT_MANIFEST, ID 1) dan tidak punya ikon.
//! Link arg dipasang per-bin (`rustc-link-arg-bin`), sehingga bin lain tidak terpengaruh.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=win12shell.rc");
    println!("cargo:rerun-if-changed=win12shell.manifest");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR tidak ada"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR tidak ada"));
    let obj = out_dir.join("win12shell_res.o");

    // Bisa diganti lewat env WINDRES bila windres tidak ada di PATH.
    let windres = env::var("WINDRES").unwrap_or_else(|_| "windres".to_string());

    // Dijalankan dari folder proyek agar `1 24 "win12shell.manifest"` ter-resolve.
    let status = Command::new(&windres)
        .current_dir(&manifest_dir)
        .args(["-O", "coff", "-i", "win12shell.rc", "-o"])
        .arg(&obj)
        .status()
        .unwrap_or_else(|e| panic!("gagal menjalankan {windres}: {e} (pasang MinGW/msys64 atau set env WINDRES)"));
    if !status.success() {
        panic!("windres gagal memproses win12shell.rc (status: {status})");
    }

    println!("cargo:rustc-link-arg-bin=win12shell={}", obj.display());
}

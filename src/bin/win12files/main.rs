//! Bin win12files: titik masuk file manager (wWinMain win12files.cpp:704-749) yang memanggil w12::files::frame::fm_main.
//! Status: belum diuji runtime; hanya cargo check.
#![windows_subsystem = "windows"]

fn main() {
    std::process::exit(w12::files::frame::fm_main());
}

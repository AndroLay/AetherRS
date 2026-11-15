//! Bin win12shell: titik masuk shell (wWinMain win12shell.cpp:5108-5284) yang memanggil w12::shell::core_c4b::w_win_main.

#![windows_subsystem = "windows"]

fn main() {
    std::process::exit(w12::shell::core_c4b::w_win_main());
}

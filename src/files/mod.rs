//! Aplikasi berkas win12files. Sumber: win12files.cpp, w12files.cpp/.h, w12fm_bar.inc, w12fm_search.inc.
//! Submodul: state (global), model (logika berkas), app (tata letak, tab, navigasi), bar (w12fm_bar.inc), search_ui (w12fm_search.inc), frame (siklus jendela dan pompa pesan), wndproc (FmWndProc, FmMsgProc).

pub mod app;
pub mod bar;
pub mod frame;
pub mod model;
pub mod search_ui;
pub mod state;
pub mod wndproc;

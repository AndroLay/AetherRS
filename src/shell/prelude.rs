//! Prelude shell (integrator, Tahap B): `use super::prelude::*;` memberi akses ke semua item `pub` dari semua
//! modul shell, meniru satu unit kompilasi win12shell.cpp + w12*.inc, ditambah tipe Windows yang umum.
//! Nama sama di dua modul baru error ("ambiguous") saat dipakai; selesaikan dengan path penuh atau `use` eksplisit.

pub use std::cell::{Cell, RefCell};

pub use crate::log::log;
pub use crate::reg;
pub use crate::sync::SyncCell;
pub use crate::wide::*;

// Modul logika murni (Tahap A). Nama C `W12Xxx` menjadi `w12_xxx` / tipe `W12Xxx`.
pub use crate::cal_logic::*;
pub use crate::diag::*;
pub use crate::dock_logic::*;
pub use crate::gesture_logic::*;
pub use crate::lazy::*;
pub use crate::nl_json::*;
pub use crate::query::*;
pub use crate::search::*;
pub use crate::start_lists::*;
pub use crate::stophosts::*;
pub use crate::tasks_logic::*;
pub use crate::ui_logic::*;
pub use crate::vdfile::*;

// Semua modul shell.
pub use super::state::*;
pub use super::core_c1::*;
pub use super::core_c2::*;
pub use super::core_c3::*;
pub use super::core_c3b::*;
pub use super::core_c4::*;
pub use super::core_c4b::*;
pub use super::notch::*;
pub use super::start::*;
pub use super::start_draw::*;
pub use super::flyout::*;
pub use super::qs::*;
pub use super::hotkeys::*;
pub use super::snap::*;
pub use super::winx::*;
pub use super::overlay::*;
pub use super::tasks_ui::*;
pub use super::panel::*;
pub use super::widgets::*;
pub use super::calendar::*;
pub use super::gesture_ui::*;
pub use super::filesearch::*;
pub use super::usb::*;
pub use super::nl_ui::*;
pub use super::diag_ui::*;
pub use super::stophosts_ui::*;

// Tipe Windows yang umum. Modul lain tetap boleh `use` item Windows secara eksplisit.
pub use windows_numerics::{Matrix3x2, Vector2};
pub use windows::core::{h, w, IInspectable, IUnknown, Interface, BOOL, GUID, HRESULT, HSTRING, PCWSTR, PWSTR};
pub use windows::Win32::Foundation::*;
pub use windows::Win32::Graphics::Direct2D::Common::*;
pub use windows::Win32::Graphics::Direct2D::*;
pub use windows::Win32::Graphics::DirectWrite::*;
pub use windows::Win32::Graphics::Gdi::*;
pub use windows::Win32::UI::Input::KeyboardAndMouse::*;
pub use windows::Win32::UI::WindowsAndMessaging::*;

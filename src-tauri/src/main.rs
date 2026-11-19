// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    ami_calculator_lib::display::prefer_x11_where_native_wayland_is_slow();
    ami_calculator_lib::run()
}

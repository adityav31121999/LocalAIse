// Keeps the extra console window out of release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    localaise_lib::run();
}

// Keep the console window from appearing alongside the app on Windows release
// builds. Harmless on macOS and Linux.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    beacon_split_lib::run()
}

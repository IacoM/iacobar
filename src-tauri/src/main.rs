// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--restore`: used by the uninstaller (and usable by hand) to give
    // Windows its taskbar back and remove the autostart entry.
    if std::env::args().any(|a| a == "--restore") {
        iacobar_lib::restore_windows();
        return;
    }
    iacobar_lib::run()
}

//! Native Windows entry point for the Clarity Disk desktop application.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    clarity_disk_lib::run();
}

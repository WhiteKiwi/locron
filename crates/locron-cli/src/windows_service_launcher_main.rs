//! Internal Windows GUI entry; automatic activation is composed separately from these probes.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows_launcher;

#[cfg(windows)]
fn main() {
    let entered = std::time::Instant::now();
    let arguments = std::env::args_os().skip(1).collect();
    std::process::exit(windows_launcher::run(entered, arguments));
}

#[cfg(not(windows))]
fn main() {
    // The optional companion has no supported non-Windows entry or metadata claim.
    std::process::exit(70);
}

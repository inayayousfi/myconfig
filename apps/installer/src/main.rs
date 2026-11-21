//! The InayOS installer, run as root from the live USB system. Its screens fill the answers
//! and the installer core installs InayOS from them; `install --answers` skips the screens.
//! `rollback` puts an installed system back to one of its snapshots.
#[cfg(target_os = "linux")]
mod app;
#[cfg(target_os = "linux")]
mod console;
#[cfg(target_os = "linux")]
mod installer;
#[cfg(target_os = "linux")]
mod live;
#[cfg(target_os = "linux")]
mod log_file;
#[cfg(target_os = "linux")]
mod qr;
#[cfg(target_os = "linux")]
mod questions;
#[cfg(target_os = "linux")]
mod rollback;
#[cfg(target_os = "linux")]
mod running;
#[cfg(target_os = "linux")]
mod screens;
#[cfg(target_os = "linux")]
mod theme;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    installer::main()
}

/// The installer runs on the Linux live USB system only. Other targets build it so the
/// workspace check covers them.
#[cfg(not(target_os = "linux"))]
fn main() -> std::process::ExitCode {
    eprintln!("inayos-installer runs only on Linux");
    std::process::ExitCode::FAILURE
}

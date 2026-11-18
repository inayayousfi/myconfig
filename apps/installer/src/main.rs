//! The InayOS installer, run as root from the live USB system. It reads an answers file and
//! installs InayOS with the installer core.
#[cfg(unix)]
mod console;
#[cfg(unix)]
mod installer;

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    installer::main()
}

/// The installer runs on the Linux live USB system only. Other targets build it so the
/// workspace check covers them.
#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!("inayos-installer runs only on Linux");
    std::process::ExitCode::FAILURE
}

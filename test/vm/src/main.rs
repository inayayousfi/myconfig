//! The entry point of `cargo xtask`.

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    inayos_xtask::main()
}

#[cfg(not(target_os = "linux"))]
fn main() -> std::process::ExitCode {
    eprintln!("error: cargo xtask runs only on a Linux host");
    std::process::ExitCode::FAILURE
}

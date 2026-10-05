//! `install.sh` against a fake local release, in a temporary home.
//!
//! `install.sh` stays a shell script because `curl | bash` runs it before any binary
//! exists, so these tests run it with bash.
#![cfg(unix)]
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output, Stdio},
};

use repository_tests::{Scratch, repository, run};

/// Publishes a fake binary per platform that prints its platform and arguments.
fn release(scratch: &Scratch) -> std::path::PathBuf {
    let release = scratch.0.join("release");
    fs::create_dir_all(&release).unwrap();
    for platform in ["cachyos", "arch-wsl", "ubuntu-server"] {
        let asset = format!("myconfig-{platform}-x86_64-unknown-linux-musl");
        fs::write(
            release.join(&asset),
            format!("#!/bin/sh\nprintf '{platform} %s\\n' \"$*\"\n"),
        )
        .unwrap();
        run(Command::new("sh")
            .arg("-c")
            .arg(format!("sha256sum {asset} > {asset}.sha256"))
            .current_dir(&release));
    }
    release
}

/// Runs install.sh detached from any terminal, like an unattended run.
fn install(scratch: &Scratch, release: &Path, arguments: &[&str]) -> Output {
    let home = scratch.0.join("home");
    fs::create_dir_all(&home).unwrap();
    Command::new("setsid")
        .arg("bash")
        .arg(repository().join("install.sh"))
        .args(arguments)
        .env("HOME", &home)
        .env(
            "MYCONFIG_RELEASE_URL",
            format!("file://{}", release.display()),
        )
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn kept(scratch: &Scratch, arguments: &[&str]) -> String {
    let output = run(Command::new(scratch.0.join("home/.local/bin/myconfig")).args(arguments));
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[test]
fn a_named_platform_installs_its_binary_and_passes_the_arguments() {
    let scratch = Scratch::new("install-named");
    let release = release(&scratch);
    let output = install(&scratch, &release, &["ubuntu", "install", "zsh"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "ubuntu-server install zsh"
    );
    let binary = scratch.0.join("home/.local/bin/myconfig");
    assert!(fs::metadata(&binary).unwrap().permissions().mode() & 0o111 != 0);
    assert_eq!(kept(&scratch, &["verify"]), "ubuntu-server verify");
}

#[test]
fn an_unattended_run_without_a_command_stops_before_downloading() {
    let scratch = Scratch::new("install-unattended");
    let release = release(&scratch);
    let output = install(&scratch, &release, &["cachyos"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("pass a command such as: install"));
    assert!(!scratch.0.join("home/.local/bin/myconfig").exists());
}

#[test]
fn a_binary_that_does_not_match_its_checksum_is_refused() {
    let scratch = Scratch::new("install-tampered");
    let release = release(&scratch);
    assert!(
        install(&scratch, &release, &["ubuntu-server", "list"])
            .status
            .success()
    );
    let asset = release.join("myconfig-arch-wsl-x86_64-unknown-linux-musl");
    let mut tampered = fs::read(&asset).unwrap();
    tampered.extend_from_slice(b"tampered\n");
    fs::write(&asset, tampered).unwrap();
    let output = install(&scratch, &release, &["arch-wsl", "list"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not match its published SHA-256")
    );
    assert_eq!(
        kept(&scratch, &["list"]),
        "ubuntu-server list",
        "the kept binary was replaced"
    );
}

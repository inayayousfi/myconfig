//! Checks that the host has every program, file and kernel feature the xtask uses for one
//! guest system. It only reports what is missing and how to install it; it never installs
//! anything.

use std::env;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::binary;
use crate::firmware::Firmware;
use crate::os::{Boot, Os};

const KVM: &str = "/dev/kvm";
const NESTED: [&str; 2] = [
    "/sys/module/kvm_amd/parameters/nested",
    "/sys/module/kvm_intel/parameters/nested",
];

/// How the user can provide a missing item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fix {
    /// An Arch Linux package.
    Pacman(&'static str),
    /// A program installed with `cargo install`.
    Cargo(&'static str),
    /// A Rust target installed with `rustup target add`.
    RustTarget(&'static str),
    /// KVM access, which needs firmware virtualization support and membership in the `kvm` group.
    Kvm,
    /// Nested virtualization, a parameter of the kvm_amd or kvm_intel kernel module.
    Nested,
}

/// One thing the host lacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Missing {
    pub item: String,
    pub fix: Fix,
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("{}", MissingReport(.0))]
    Missing(Vec<Missing>),
}

/// Returns `Ok` when the host has everything that `os` needs, or the full list of missing items.
pub fn check(os: Os) -> Result<(), HostError> {
    let missing = find_missing(os);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(HostError::Missing(missing))
    }
}

fn find_missing(os: Os) -> Vec<Missing> {
    let mut missing = Vec::new();
    let mut commands = vec![
        ("qemu-system-x86_64", Fix::Pacman("qemu-desktop")),
        ("qemu-img", Fix::Pacman("qemu-desktop")),
        ("remote-viewer", Fix::Pacman("virt-viewer")),
        ("ssh", Fix::Pacman("openssh")),
        ("scp", Fix::Pacman("openssh")),
        ("ssh-keygen", Fix::Pacman("openssh")),
        ("curl", Fix::Pacman("curl")),
        ("xorriso", Fix::Pacman("libisoburn")),
    ];
    if os.boot() == Boot::SecureUefiWithTpm {
        commands.push(("swtpm", Fix::Pacman("swtpm")));
    }
    if binary::needs_readelf(os) {
        commands.push(("readelf", Fix::Pacman("binutils")));
    }
    if binary::needs_xwin(os) {
        // cargo-xwin compiles C code with clang-cl and links with lld-link and llvm-lib.
        commands.extend([
            ("cargo-xwin", Fix::Cargo("cargo-xwin")),
            ("clang-cl", Fix::Pacman("clang")),
            ("lld-link", Fix::Pacman("lld")),
            ("llvm-lib", Fix::Pacman("llvm")),
        ]);
    }
    for (command, fix) in commands {
        if !on_path(command) {
            missing.push(Missing {
                item: format!("the {command} command"),
                fix,
            });
        }
    }

    let firmware = Firmware::for_boot(os.boot());
    for file in [firmware.code, firmware.vars_template] {
        if !file.is_file() {
            missing.push(Missing {
                item: format!("the UEFI firmware file {}", file.display()),
                fix: Fix::Pacman("edk2-ovmf"),
            });
        }
    }

    for target in binary::targets(os) {
        if !rust_target_installed(target) {
            missing.push(Missing {
                item: format!("the Rust standard library for {target}"),
                fix: Fix::RustTarget(target),
            });
        }
    }

    if OpenOptions::new().read(true).write(true).open(KVM).is_err() {
        missing.push(Missing {
            item: format!("read and write access to {KVM}"),
            fix: Fix::Kvm,
        });
    }

    // Only WSL inside the Windows guest runs a virtual machine of its own.
    let nested = NESTED
        .iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .any(|value| matches!(value.trim(), "1" | "Y" | "y"));
    if os == Os::Windows && !nested {
        missing.push(Missing {
            item: "nested virtualization in the kvm_amd or kvm_intel module".to_owned(),
            fix: Fix::Nested,
        });
    }
    missing
}

fn on_path(command: &str) -> bool {
    env::var_os("PATH")
        .is_some_and(|path| env::split_paths(&path).any(|dir| is_executable(&dir.join(command))))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn rust_target_installed(target: &str) -> bool {
    let Ok(output) = Command::new("rustc")
        .args(["--print", "target-libdir", "--target", target])
        .output()
    else {
        return false;
    };
    // rustc prints the folder even when the target is absent, so the folder itself is checked.
    output.status.success()
        && PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()).is_dir()
}

/// The missing items followed by the commands that provide them.
struct MissingReport<'a>(&'a [Missing]);

impl fmt::Display for MissingReport<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "the host is missing:")?;
        for missing in self.0 {
            writeln!(formatter, "  - {}", missing.item)?;
        }
        writeln!(formatter, "install them with:")?;

        let mut packages = Vec::new();
        let mut crates = Vec::new();
        let mut targets = Vec::new();
        let mut kvm = false;
        let mut nested = false;
        for missing in self.0 {
            match missing.fix {
                Fix::Pacman(package) if !packages.contains(&package) => packages.push(package),
                Fix::Cargo(name) if !crates.contains(&name) => crates.push(name),
                Fix::RustTarget(target) if !targets.contains(&target) => targets.push(target),
                Fix::Kvm => kvm = true,
                Fix::Nested => nested = true,
                Fix::Pacman(_) | Fix::Cargo(_) | Fix::RustTarget(_) => {}
            }
        }
        if !packages.is_empty() {
            writeln!(
                formatter,
                "  sudo pacman -S --needed {}",
                packages.join(" ")
            )?;
        }
        if !crates.is_empty() {
            writeln!(formatter, "  cargo install --locked {}", crates.join(" "))?;
        }
        if !targets.is_empty() {
            writeln!(formatter, "  rustup target add {}", targets.join(" "))?;
        }
        if kvm {
            writeln!(
                formatter,
                "  turn on virtualization in the firmware settings, then: sudo usermod -aG kvm \"$USER\" and log in again"
            )?;
        }
        if nested {
            writeln!(
                formatter,
                "  echo 'options kvm_amd nested=1' | sudo tee /etc/modprobe.d/kvm-nested.conf (kvm_intel on Intel), then reload the module or restart"
            )?;
        }
        Ok(())
    }
}

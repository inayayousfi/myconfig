//! The UEFI firmware files from the host's edk2-ovmf package.

use std::env;
use std::path::PathBuf;

use crate::os::Boot;

/// Overrides the folder that holds the edk2-ovmf files.
pub const FIRMWARE_VARIABLE: &str = "MYCONFIG_VM_FIRMWARE_DIR";
const DEFAULT_DIR: &str = "/usr/share/edk2/x64";

/// The read-only firmware code and the empty variable store that each new image copies.
#[derive(Clone, Debug)]
pub struct Firmware {
    pub code: PathBuf,
    pub vars_template: PathBuf,
}

impl Firmware {
    pub fn for_boot(boot: Boot) -> Firmware {
        let dir = env::var_os(FIRMWARE_VARIABLE)
            .filter(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from(DEFAULT_DIR), PathBuf::from);
        let code = match boot {
            Boot::Uefi => "OVMF_CODE.4m.fd",
            Boot::SecureUefiWithTpm => "OVMF_CODE.secboot.4m.fd",
        };
        // The variable store has no enrolled Secure Boot keys. Windows 11 Setup asks for
        // firmware that supports Secure Boot, not for Secure Boot turned on.
        Firmware {
            code: dir.join(code),
            vars_template: dir.join("OVMF_VARS.4m.fd"),
        }
    }
}

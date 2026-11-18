//! The guest systems the xtask can test and the fixed facts about each one.

use std::fmt;

/// A guest system. Each one has its own clean image and its own installer profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Os {
    Windows,
    Ubuntu,
}

/// The account that every base image creates. The image builders use these values,
/// and the runner uses them to log in and to answer `sudo`.
pub const GUEST_USER: &str = "tester";
pub const GUEST_PASSWORD: &str = "inayos";

/// The Windows guest folder for the xtask's own scripts and logs. It sits outside the user
/// profile, so the state lists never see it.
pub const WINDOWS_TOOL_DIR: &str = r"C:\inayos-vm";

/// How a guest starts its firmware.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boot {
    /// UEFI without Secure Boot and without a TPM.
    Uefi,
    /// UEFI with the Secure Boot firmware build and a TPM 2.0 from swtpm, which Windows 11 requires.
    SecureUefiWithTpm,
}

impl Os {
    pub fn name(self) -> &'static str {
        match self {
            Os::Windows => "windows",
            Os::Ubuntu => "ubuntu",
        }
    }

    pub fn memory_mib(self) -> u32 {
        match self {
            Os::Windows => 8192,
            Os::Ubuntu => 4096,
        }
    }

    pub fn boot(self) -> Boot {
        match self {
            Os::Windows => Boot::SecureUefiWithTpm,
            Os::Ubuntu => Boot::Uefi,
        }
    }

    /// Whether the guest can show a second display. Only a desktop guest is tested with two,
    /// and no current guest is one.
    pub fn supports_second_display(self) -> bool {
        false
    }

    /// How long the first SSH login may take after the guest starts.
    /// Windows needs longer because its first logon prepares the desktop before sshd answers.
    pub fn ssh_timeout_secs(self) -> u64 {
        match self {
            Os::Windows => 20 * 60,
            Os::Ubuntu => 10 * 60,
        }
    }
}

impl fmt::Display for Os {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

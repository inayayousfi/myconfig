//! The guest systems the xtask can test and the fixed facts about each one.

use std::fmt;

/// A guest system. Each one has its own clean image and its own installer profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Os {
    Windows,
    Ubuntu,
    /// Arch Linux. Its clean image hosts the build of the InayOS USB stick, and no installer
    /// cycle runs on it: the scenarios test the InayOS installer from the stick.
    Arch,
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
            Os::Arch => "arch",
        }
    }

    pub fn memory_mib(self) -> u32 {
        match self {
            // mkarchiso and the InayOS install both need more than 4 GiB.
            Os::Windows | Os::Arch => 8192,
            Os::Ubuntu => 4096,
        }
    }

    pub fn boot(self) -> Boot {
        match self {
            Os::Windows => Boot::SecureUefiWithTpm,
            Os::Ubuntu | Os::Arch => Boot::Uefi,
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
            Os::Ubuntu | Os::Arch => 10 * 60,
        }
    }

    /// Whether `cycle` and `run` test an installer on this system's clean image.
    pub fn tests_installer(self) -> bool {
        self != Os::Arch
    }
}

impl fmt::Display for Os {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

//! The answers file that a scenario gives the InayOS installer: TOML in the format that
//! `crates/install-core` reads.

use std::fmt::Write;

use crate::os::{GUEST_PASSWORD, GUEST_USER};

/// The exact questions that the modules of the InayOS desktop profile ask during an install,
/// with the answers of the cycle's table in `src/answers.rs`: the SSH server on, and no login.
pub const INSTALL_QUESTIONS: &[(&str, bool)] = &[
    // ssh.rs
    ("The OpenSSH server is off. Turn it on now?", true),
    // login.rs: a test machine has no accounts to log in to.
    ("GitHub CLI is not authenticated. Log in now?", false),
    ("Tailscale is not authenticated. Log in now?", false),
];

/// The disk choice of one install and the partitions it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiskChoice {
    WipeDisk {
        device: &'static str,
        system_size_gib: u32,
    },
    ReinstallKeepHome {
        device: &'static str,
        efi_partition: &'static str,
        system_partition: &'static str,
        home_partition: &'static str,
    },
    ShrinkExt4KeepHome {
        device: &'static str,
        system_size_gib: u32,
        efi_partition: &'static str,
        home_partition: &'static str,
    },
    /// Shrinks the Windows partition of `device` and installs beside it. The installer picks
    /// the partitions and the sizes itself.
    BesideWindows { device: &'static str },
}

impl DiskChoice {
    /// The whole disk the choice installs on.
    pub fn device(&self) -> &'static str {
        match self {
            DiskChoice::WipeDisk { device, .. }
            | DiskChoice::ReinstallKeepHome { device, .. }
            | DiskChoice::ShrinkExt4KeepHome { device, .. }
            | DiskChoice::BesideWindows { device } => device,
        }
    }
}

/// The answers of the security screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Security {
    pub secure_boot: bool,
    pub encryption: bool,
}

/// The answers file for an install with `disk`, the `security` answers, the test account, and
/// `authorized_key` as the only key that the new user accepts.
pub fn answers_file(disk: &DiskChoice, authorized_key: &str, security: Security) -> String {
    let mut text = String::new();
    push(&mut text, "keyboard", string("us"));
    push(&mut text, "language", string("en_US.UTF-8"));
    push(&mut text, "timezone", string("UTC"));
    push(&mut text, "hostname", string("inayos"));

    text.push_str("\n[disk]\n");
    match disk {
        DiskChoice::WipeDisk {
            device,
            system_size_gib,
        } => {
            push(&mut text, "choice", string("wipe-disk"));
            push(&mut text, "device", string(device));
            push(&mut text, "system_size_gib", system_size_gib.to_string());
        }
        DiskChoice::ReinstallKeepHome {
            device,
            efi_partition,
            system_partition,
            home_partition,
        } => {
            push(&mut text, "choice", string("reinstall-keep-home"));
            push(&mut text, "device", string(device));
            push(&mut text, "efi_partition", string(efi_partition));
            push(&mut text, "system_partition", string(system_partition));
            push(&mut text, "home_partition", string(home_partition));
        }
        DiskChoice::ShrinkExt4KeepHome {
            device,
            system_size_gib,
            efi_partition,
            home_partition,
        } => {
            push(&mut text, "choice", string("shrink-ext4-keep-home"));
            push(&mut text, "device", string(device));
            push(&mut text, "system_size_gib", system_size_gib.to_string());
            push(&mut text, "efi_partition", string(efi_partition));
            push(&mut text, "home_partition", string(home_partition));
        }
        DiskChoice::BesideWindows { device } => {
            push(&mut text, "choice", string("beside-windows"));
            push(&mut text, "device", string(device));
        }
    }

    text.push_str("\n[security]\n");
    push(&mut text, "secure_boot", security.secure_boot.to_string());
    push(&mut text, "encryption", security.encryption.to_string());

    text.push_str("\n[user]\n");
    push(&mut text, "full_name", string("Tester"));
    push(&mut text, "username", string(GUEST_USER));
    push(&mut text, "password", string(GUEST_PASSWORD));
    push(
        &mut text,
        "authorized_keys",
        format!("[{}]", string(authorized_key)),
    );

    text.push_str("\n[modules]\nskip = []\n");

    text.push_str("\n[questions]\n");
    for (question, yes) in INSTALL_QUESTIONS {
        push(&mut text, &string(question), yes.to_string());
    }
    text
}

/// Adds the line `key = value`, where `value` is already in TOML form.
fn push(text: &mut String, key: &str, value: String) {
    writeln!(text, "{key} = {value}").expect("writing to a string succeeds");
}

/// A TOML basic string. The escapes of a JSON string are all valid in a TOML basic string.
fn string(value: &str) -> String {
    serde_json::Value::from(value).to_string()
}

#[cfg(test)]
mod tests {
    use inayos_interface::QUESTION_END;

    use super::*;
    use crate::answers::{Answers, Response};

    const KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample inayos-vm";

    #[test]
    fn a_wipe_disk_install_names_the_disk_the_size_the_account_and_the_questions() {
        let text = answers_file(
            &DiskChoice::WipeDisk {
                device: "/dev/vda",
                system_size_gib: 40,
            },
            KEY,
            Security::default(),
        );
        assert_eq!(
            text,
            format!(
                "keyboard = \"us\"\n\
                 language = \"en_US.UTF-8\"\n\
                 timezone = \"UTC\"\n\
                 hostname = \"inayos\"\n\
                 \n[disk]\n\
                 choice = \"wipe-disk\"\n\
                 device = \"/dev/vda\"\n\
                 system_size_gib = 40\n\
                 \n[security]\n\
                 secure_boot = false\n\
                 encryption = false\n\
                 \n[user]\n\
                 full_name = \"Tester\"\n\
                 username = \"tester\"\n\
                 password = \"inayos\"\n\
                 authorized_keys = [\"{KEY}\"]\n\
                 \n[modules]\n\
                 skip = []\n\
                 \n[questions]\n\
                 \"The OpenSSH server is off. Turn it on now?\" = true\n\
                 \"GitHub CLI is not authenticated. Log in now?\" = false\n\
                 \"Tailscale is not authenticated. Log in now?\" = false\n"
            )
        );
    }

    #[test]
    fn the_keep_home_choices_name_the_partitions_they_reuse() {
        let reinstall = answers_file(
            &DiskChoice::ReinstallKeepHome {
                device: "/dev/vda",
                efi_partition: "/dev/vda1",
                system_partition: "/dev/vda2",
                home_partition: "/dev/vda3",
            },
            KEY,
            Security::default(),
        );
        assert!(reinstall.contains(
            "[disk]\nchoice = \"reinstall-keep-home\"\ndevice = \"/dev/vda\"\n\
             efi_partition = \"/dev/vda1\"\nsystem_partition = \"/dev/vda2\"\n\
             home_partition = \"/dev/vda3\"\n"
        ));
        let shrink = answers_file(
            &DiskChoice::ShrinkExt4KeepHome {
                device: "/dev/vda",
                system_size_gib: 30,
                efi_partition: "/dev/vda1",
                home_partition: "/dev/vda2",
            },
            KEY,
            Security::default(),
        );
        assert!(shrink.contains(
            "[disk]\nchoice = \"shrink-ext4-keep-home\"\ndevice = \"/dev/vda\"\n\
             system_size_gib = 30\nefi_partition = \"/dev/vda1\"\nhome_partition = \"/dev/vda2\"\n"
        ));
    }

    #[test]
    fn the_beside_windows_choice_names_only_the_disk() {
        let text = answers_file(
            &DiskChoice::BesideWindows { device: "/dev/sda" },
            KEY,
            Security::default(),
        );
        assert!(
            text.contains(
                "\n[disk]\nchoice = \"beside-windows\"\ndevice = \"/dev/sda\"\n\n[security]\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn the_security_answers_turn_secure_boot_and_encryption_on_separately() {
        let disk = DiskChoice::WipeDisk {
            device: "/dev/vda",
            system_size_gib: 40,
        };
        let secure_boot = answers_file(
            &disk,
            KEY,
            Security {
                secure_boot: true,
                encryption: false,
            },
        );
        assert!(secure_boot.contains("\n[security]\nsecure_boot = true\nencryption = false\n"));
        let encrypted = answers_file(
            &disk,
            KEY,
            Security {
                secure_boot: true,
                encryption: true,
            },
        );
        assert!(encrypted.contains("\n[security]\nsecure_boot = true\nencryption = true\n"));
    }

    #[test]
    fn the_installer_gets_the_answers_that_the_cycle_gives_to_the_same_questions() {
        let table = Answers::new();
        for (question, yes) in INSTALL_QUESTIONS {
            assert_eq!(
                table.respond(&format!("{question}{QUESTION_END}")),
                Response::Answer {
                    question: (*question).to_owned(),
                    yes: *yes,
                },
                "{question}"
            );
        }
    }
}

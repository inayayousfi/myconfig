//! The answers file: every choice the installer needs, read from TOML and checked before
//! anything on the machine changes.
use std::{collections::BTreeMap, fmt, path::Path};

use serde::Deserialize;

use crate::{disk_names, sizes};

/// The checked answers. Every field holds a value the install can use as it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answers {
    /// The console keymap, written as `KEYMAP` in `/etc/vconsole.conf`.
    pub keyboard: String,
    /// The locale, such as `en_US.UTF-8`.
    pub language: String,
    /// A time zone name under `/usr/share/zoneinfo`.
    pub timezone: String,
    pub hostname: String,
    /// The Wi-Fi network the live system connects to before the install, when there is one.
    pub wifi: Option<Wifi>,
    pub disk: Disk,
    pub security: Security,
    pub user: User,
    /// Profile module names not to install.
    pub skip: Vec<String>,
    /// Answers to the modules' questions, keyed by the exact question text.
    pub questions: BTreeMap<String, bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wifi {
    pub ssid: String,
    /// None for an open network.
    pub password: Option<Secret>,
}

/// What happens to the disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disk {
    /// Erases `device` and creates the EFI, system and home partitions.
    WipeDisk {
        device: String,
        system_size_gib: u64,
    },
    /// Formats the system partition and keeps the EFI and home partitions.
    ReinstallKeepHome {
        device: String,
        efi_partition: String,
        system_partition: String,
        home_partition: String,
    },
    /// Shrinks the ext4 partition that holds an older system and its home, keeps it as home,
    /// and puts the system in the freed space after it.
    ShrinkExt4KeepHome {
        device: String,
        system_size_gib: u64,
        efi_partition: String,
        home_partition: String,
    },
    /// Shrinks Windows and installs beside it.
    BesideWindows { device: String },
}

impl Disk {
    /// The whole disk the install works on.
    pub fn device(&self) -> &str {
        match self {
            Disk::WipeDisk { device, .. }
            | Disk::ReinstallKeepHome { device, .. }
            | Disk::ShrinkExt4KeepHome { device, .. }
            | Disk::BesideWindows { device } => device,
        }
    }

    /// Whether the install keeps an existing home partition.
    pub fn keeps_home(&self) -> bool {
        matches!(
            self,
            Disk::ReinstallKeepHome { .. } | Disk::ShrinkExt4KeepHome { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Security {
    pub secure_boot: bool,
    pub encryption: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub full_name: String,
    pub username: String,
    pub password: Secret,
    /// Public keys written to `~/.ssh/authorized_keys`, one per line.
    pub authorized_keys: Vec<String>,
}

/// A password. Its `Debug` form hides it, so it never reaches a log by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<hidden>")
    }
}

/// Why an answers file cannot be used. `field` names the TOML key, with its table.
#[derive(Debug, thiserror::Error)]
pub enum AnswersError {
    #[error("could not read the answers file {path}: {error}")]
    Read {
        path: String,
        #[source]
        error: std::io::Error,
    },
    #[error("the answers file is not valid: {0}")]
    Syntax(#[from] toml::de::Error),
    #[error("{field} is invalid: {reason}")]
    Invalid { field: &'static str, reason: String },
    #[error("disk choice {choice} needs {field}")]
    Missing {
        choice: &'static str,
        field: &'static str,
    },
    #[error("disk choice {choice} does not use {field}; remove it")]
    Unused {
        choice: &'static str,
        field: &'static str,
    },
}

impl Answers {
    /// Reads and checks the answers file at `path`.
    pub fn read(path: &Path) -> Result<Self, AnswersError> {
        let text = std::fs::read_to_string(path).map_err(|error| AnswersError::Read {
            path: path.display().to_string(),
            error,
        })?;
        Self::parse(&text)
    }

    /// Parses and checks the TOML text of an answers file.
    pub fn parse(text: &str) -> Result<Self, AnswersError> {
        let raw: RawAnswers = toml::from_str(text)?;
        raw.check()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAnswers {
    keyboard: String,
    language: String,
    #[serde(default = "utc")]
    timezone: String,
    hostname: String,
    #[serde(default)]
    network: RawNetwork,
    disk: RawDisk,
    #[serde(default)]
    security: RawSecurity,
    user: RawUser,
    #[serde(default)]
    modules: RawModules,
    #[serde(default)]
    questions: BTreeMap<String, bool>,
}

fn utc() -> String {
    "UTC".to_owned()
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNetwork {
    wifi_ssid: Option<String>,
    wifi_password: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Choice {
    WipeDisk,
    ReinstallKeepHome,
    ShrinkExt4KeepHome,
    BesideWindows,
}

impl Choice {
    fn name(self) -> &'static str {
        match self {
            Choice::WipeDisk => "wipe-disk",
            Choice::ReinstallKeepHome => "reinstall-keep-home",
            Choice::ShrinkExt4KeepHome => "shrink-ext4-keep-home",
            Choice::BesideWindows => "beside-windows",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDisk {
    choice: Choice,
    device: String,
    system_size_gib: Option<u64>,
    efi_partition: Option<String>,
    system_partition: Option<String>,
    home_partition: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSecurity {
    #[serde(default)]
    secure_boot: bool,
    #[serde(default)]
    encryption: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUser {
    full_name: String,
    username: String,
    password: String,
    #[serde(default)]
    authorized_keys: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModules {
    #[serde(default)]
    skip: Vec<String>,
}

fn invalid(field: &'static str, reason: impl Into<String>) -> AnswersError {
    AnswersError::Invalid {
        field,
        reason: reason.into(),
    }
}

/// A plain word: not empty, with no whitespace and no character a configuration line would
/// misread.
fn check_word(field: &'static str, value: &str) -> Result<(), AnswersError> {
    if value.is_empty() {
        return Err(invalid(field, "it is empty"));
    }
    if value
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '=' || c == '"' || c == '\'')
    {
        return Err(invalid(
            field,
            "it contains a space, a quote, `=` or a control character",
        ));
    }
    Ok(())
}

fn check_username(name: &str) -> Result<(), AnswersError> {
    let field = "user.username";
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(invalid(field, "it is empty"));
    };
    if name.len() > 32 {
        return Err(invalid(field, "it is longer than 32 characters"));
    }
    if !(first.is_ascii_lowercase() || first == '_') {
        return Err(invalid(
            field,
            "it must start with a lowercase letter or `_`",
        ));
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Err(invalid(
            field,
            "it may only contain lowercase letters, digits, `_` and `-`",
        ));
    }
    if name == "root" {
        return Err(invalid(field, "root already exists"));
    }
    Ok(())
}

fn check_hostname(name: &str) -> Result<(), AnswersError> {
    let field = "hostname";
    if name.is_empty() || name.len() > 63 {
        return Err(invalid(field, "it must have 1 to 63 characters"));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(invalid(
            field,
            "it may only contain letters, digits and `-`",
        ));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err(invalid(field, "it cannot start or end with `-`"));
    }
    Ok(())
}

fn check_line(field: &'static str, value: &str) -> Result<(), AnswersError> {
    if value.chars().any(|c| c == '\n' || c == '\r' || c == '\0') {
        return Err(invalid(field, "it contains a line break"));
    }
    Ok(())
}

fn check_size(gib: u64) -> Result<u64, AnswersError> {
    if gib < sizes::MIN_SYSTEM_GIB {
        return Err(invalid(
            "disk.system_size_gib",
            format!("the system needs at least {} GiB", sizes::MIN_SYSTEM_GIB),
        ));
    }
    if gib > sizes::MAX_SYSTEM_GIB {
        return Err(invalid(
            "disk.system_size_gib",
            format!("it is larger than {} GiB", sizes::MAX_SYSTEM_GIB),
        ));
    }
    Ok(gib)
}

impl RawDisk {
    fn check(self) -> Result<Disk, AnswersError> {
        let choice = self.choice;
        let name = choice.name();
        if !disk_names::is_disk_path(&self.device) {
            return Err(invalid("disk.device", "it must be a path under /dev"));
        }
        let device = self.device;
        let partition = |field: &'static str, value: Option<String>| match value {
            None => Err(AnswersError::Missing {
                choice: name,
                field,
            }),
            Some(path) if disk_names::partition_number(&device, &path).is_none() => Err(invalid(
                field,
                format!("{path} is not a partition of {device}"),
            )),
            Some(path) => Ok(path),
        };
        let unused = |field: &'static str, present: bool| {
            if present {
                Err(AnswersError::Unused {
                    choice: name,
                    field,
                })
            } else {
                Ok(())
            }
        };
        let size = || match self.system_size_gib {
            None => Err(AnswersError::Missing {
                choice: name,
                field: "disk.system_size_gib",
            }),
            Some(gib) => check_size(gib),
        };
        match choice {
            Choice::WipeDisk | Choice::BesideWindows => {
                unused("disk.efi_partition", self.efi_partition.is_some())?;
                unused("disk.system_partition", self.system_partition.is_some())?;
                unused("disk.home_partition", self.home_partition.is_some())?;
                if let Choice::WipeDisk = choice {
                    let system_size_gib = size()?;
                    Ok(Disk::WipeDisk {
                        device,
                        system_size_gib,
                    })
                } else {
                    unused("disk.system_size_gib", self.system_size_gib.is_some())?;
                    Ok(Disk::BesideWindows { device })
                }
            }
            Choice::ReinstallKeepHome => {
                unused("disk.system_size_gib", self.system_size_gib.is_some())?;
                let efi_partition = partition("disk.efi_partition", self.efi_partition.clone())?;
                let system_partition =
                    partition("disk.system_partition", self.system_partition.clone())?;
                let home_partition = partition("disk.home_partition", self.home_partition.clone())?;
                if system_partition == efi_partition || system_partition == home_partition {
                    return Err(invalid(
                        "disk.system_partition",
                        "it must differ from the EFI and home partitions",
                    ));
                }
                if efi_partition == home_partition {
                    return Err(invalid(
                        "disk.home_partition",
                        "it must differ from the EFI partition",
                    ));
                }
                Ok(Disk::ReinstallKeepHome {
                    device,
                    efi_partition,
                    system_partition,
                    home_partition,
                })
            }
            Choice::ShrinkExt4KeepHome => {
                unused("disk.system_partition", self.system_partition.is_some())?;
                let system_size_gib = size()?;
                let efi_partition = partition("disk.efi_partition", self.efi_partition.clone())?;
                let home_partition = partition("disk.home_partition", self.home_partition.clone())?;
                if efi_partition == home_partition {
                    return Err(invalid(
                        "disk.home_partition",
                        "it must differ from the EFI partition",
                    ));
                }
                Ok(Disk::ShrinkExt4KeepHome {
                    device,
                    system_size_gib,
                    efi_partition,
                    home_partition,
                })
            }
        }
    }
}

impl RawAnswers {
    fn check(self) -> Result<Answers, AnswersError> {
        check_word("keyboard", &self.keyboard)?;
        check_word("language", &self.language)?;
        check_word("timezone", &self.timezone)?;
        if self.timezone.starts_with('/') || self.timezone.split('/').any(|part| part == "..") {
            return Err(invalid(
                "timezone",
                "it must be a name such as Europe/Paris",
            ));
        }
        check_hostname(&self.hostname)?;

        let wifi = match (self.network.wifi_ssid, self.network.wifi_password) {
            (None, None) => None,
            (None, Some(_)) => {
                return Err(invalid(
                    "network.wifi_password",
                    "it needs network.wifi_ssid",
                ));
            }
            (Some(ssid), password) => {
                if ssid.is_empty() {
                    return Err(invalid("network.wifi_ssid", "it is empty"));
                }
                check_line("network.wifi_ssid", &ssid)?;
                if let Some(password) = &password {
                    check_line("network.wifi_password", password)?;
                }
                Some(Wifi {
                    ssid,
                    password: password.map(Secret),
                })
            }
        };

        let disk = self.disk.check()?;

        let user = self.user;
        check_username(&user.username)?;
        check_line("user.full_name", &user.full_name)?;
        if user.full_name.contains(':') {
            return Err(invalid("user.full_name", "it contains `:`"));
        }
        if user.password.is_empty() {
            return Err(invalid("user.password", "it is empty"));
        }
        check_line("user.password", &user.password)?;
        for key in &user.authorized_keys {
            if key.trim().is_empty() {
                return Err(invalid("user.authorized_keys", "a key is empty"));
            }
            check_line("user.authorized_keys", key)?;
        }

        for module in &self.modules.skip {
            check_word("modules.skip", module)?;
        }

        Ok(Answers {
            keyboard: self.keyboard,
            language: self.language,
            timezone: self.timezone,
            hostname: self.hostname,
            wifi,
            disk,
            security: Security {
                secure_boot: self.security.secure_boot,
                encryption: self.security.encryption,
            },
            user: User {
                full_name: user.full_name,
                username: user.username,
                password: Secret(user.password),
                authorized_keys: user.authorized_keys,
            },
            skip: self.modules.skip,
            questions: self.questions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
keyboard = "us"
language = "en_US.UTF-8"
timezone = "Europe/Paris"
hostname = "inayos"

[network]
wifi_ssid = "Home"
wifi_password = "secret"

[disk]
choice = "wipe-disk"
device = "/dev/vda"
system_size_gib = 150

[security]
secure_boot = false
encryption = false

[user]
full_name = "Tester"
username = "tester"
password = "inayos"
authorized_keys = ["ssh-ed25519 AAAA test"]

[modules]
skip = ["login"]

[questions]
"The OpenSSH server is off. Turn it on now?" = true
"#;

    fn with_disk(disk: &str) -> String {
        format!(
            "keyboard = \"us\"\nlanguage = \"en_US.UTF-8\"\nhostname = \"inayos\"\n\
             [disk]\n{disk}\n[user]\nfull_name = \"T\"\nusername = \"tester\"\npassword = \"p\"\n"
        )
    }

    #[test]
    fn a_full_file_gives_every_answer() {
        let answers = Answers::parse(FULL).unwrap();
        assert_eq!(answers.timezone, "Europe/Paris");
        assert_eq!(
            answers.wifi,
            Some(Wifi {
                ssid: "Home".into(),
                password: Some(Secret("secret".into()))
            })
        );
        assert_eq!(
            answers.disk,
            Disk::WipeDisk {
                device: "/dev/vda".into(),
                system_size_gib: 150
            }
        );
        assert_eq!(answers.user.authorized_keys, ["ssh-ed25519 AAAA test"]);
        assert_eq!(answers.skip, ["login"]);
        assert_eq!(
            answers
                .questions
                .get("The OpenSSH server is off. Turn it on now?"),
            Some(&true)
        );
    }

    #[test]
    fn optional_tables_take_their_defaults() {
        let answers = Answers::parse(&with_disk(
            "choice = \"wipe-disk\"\ndevice = \"/dev/nvme0n1\"\nsystem_size_gib = 40",
        ))
        .unwrap();
        assert_eq!(answers.timezone, "UTC");
        assert_eq!(answers.wifi, None);
        assert_eq!(answers.security, Security::default());
        assert!(answers.skip.is_empty() && answers.questions.is_empty());
        assert!(answers.user.authorized_keys.is_empty());
    }

    #[test]
    fn the_password_never_shows_in_debug_output() {
        let answers = Answers::parse(FULL).unwrap();
        let shown = format!("{answers:?}");
        assert!(shown.contains("password: <hidden>"));
        assert!(!shown.contains("\"secret\""));
    }

    #[test]
    fn an_unknown_field_is_refused() {
        let text = FULL.replace(
            "hostname = \"inayos\"",
            "hostname = \"inayos\"\nhost = \"x\"",
        );
        assert!(matches!(
            Answers::parse(&text),
            Err(AnswersError::Syntax(_))
        ));
        let text = FULL.replace("system_size_gib = 150", "system_size_gib = 150\nsize = 3");
        assert!(matches!(
            Answers::parse(&text),
            Err(AnswersError::Syntax(_))
        ));
    }

    #[test]
    fn bad_usernames_are_refused() {
        for name in ["Tester", "1tester", "", "root", "te ster", "a:b"] {
            let text = FULL.replace("username = \"tester\"", &format!("username = \"{name}\""));
            assert!(
                matches!(
                    Answers::parse(&text),
                    Err(AnswersError::Invalid {
                        field: "user.username",
                        ..
                    })
                ),
                "{name:?} was accepted"
            );
        }
        let text = FULL.replace("username = \"tester\"", "username = \"_te-st3r\"");
        assert!(Answers::parse(&text).is_ok());
    }

    #[test]
    fn the_system_size_must_be_usable() {
        for size in ["0", "10"] {
            let text = with_disk(&format!(
                "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = {size}"
            ));
            assert!(matches!(
                Answers::parse(&text),
                Err(AnswersError::Invalid {
                    field: "disk.system_size_gib",
                    ..
                })
            ));
        }
        let text = with_disk("choice = \"wipe-disk\"\ndevice = \"/dev/vda\"");
        assert!(matches!(
            Answers::parse(&text),
            Err(AnswersError::Missing {
                field: "disk.system_size_gib",
                ..
            })
        ));
    }

    #[test]
    fn partitions_must_belong_to_the_device() {
        let text = with_disk(
            "choice = \"reinstall-keep-home\"\ndevice = \"/dev/vda\"\n\
             efi_partition = \"/dev/vda1\"\nsystem_partition = \"/dev/vdb2\"\nhome_partition = \"/dev/vda3\"",
        );
        assert!(matches!(
            Answers::parse(&text),
            Err(AnswersError::Invalid {
                field: "disk.system_partition",
                ..
            })
        ));
        let text = with_disk(
            "choice = \"shrink-ext4-keep-home\"\ndevice = \"/dev/nvme0n1\"\nsystem_size_gib = 40\n\
             efi_partition = \"/dev/nvme0n1p1\"\nhome_partition = \"/dev/nvme0n12\"",
        );
        assert!(matches!(
            Answers::parse(&text),
            Err(AnswersError::Invalid {
                field: "disk.home_partition",
                ..
            })
        ));
    }

    #[test]
    fn each_choice_takes_exactly_its_fields() {
        let reinstall = with_disk(
            "choice = \"reinstall-keep-home\"\ndevice = \"/dev/vda\"\n\
             efi_partition = \"/dev/vda1\"\nsystem_partition = \"/dev/vda2\"\nhome_partition = \"/dev/vda3\"",
        );
        assert_eq!(
            Answers::parse(&reinstall).unwrap().disk,
            Disk::ReinstallKeepHome {
                device: "/dev/vda".into(),
                efi_partition: "/dev/vda1".into(),
                system_partition: "/dev/vda2".into(),
                home_partition: "/dev/vda3".into(),
            }
        );
        let missing = reinstall.replace("home_partition = \"/dev/vda3\"", "");
        assert!(matches!(
            Answers::parse(&missing),
            Err(AnswersError::Missing {
                field: "disk.home_partition",
                ..
            })
        ));
        let same = reinstall.replace(
            "system_partition = \"/dev/vda2\"",
            "system_partition = \"/dev/vda3\"",
        );
        assert!(matches!(
            Answers::parse(&same),
            Err(AnswersError::Invalid {
                field: "disk.system_partition",
                ..
            })
        ));
        let wipe_with_home = with_disk(
            "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40\nhome_partition = \"/dev/vda3\"",
        );
        assert!(matches!(
            Answers::parse(&wipe_with_home),
            Err(AnswersError::Unused {
                field: "disk.home_partition",
                ..
            })
        ));
        let shrink = with_disk(
            "choice = \"shrink-ext4-keep-home\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40\n\
             efi_partition = \"/dev/vda1\"\nhome_partition = \"/dev/vda2\"",
        );
        assert!(Answers::parse(&shrink).unwrap().disk.keeps_home());
    }

    #[test]
    fn a_wifi_password_needs_a_network_name() {
        let text = FULL.replace("wifi_ssid = \"Home\"\n", "");
        assert!(matches!(
            Answers::parse(&text),
            Err(AnswersError::Invalid {
                field: "network.wifi_password",
                ..
            })
        ));
    }
}

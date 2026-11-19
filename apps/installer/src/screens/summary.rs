//! The summary screen: every answer, the exact disk actions in plain words, and a field where
//! the user types the confirmation word that enables Install.
use iced::{
    Element, Length, Task,
    widget::{Column, column, scrollable, text, text_input},
};
use inayos_install_core::{Answers, Choice, Disk, inventory::DiskInfo, sizes};

use super::disk::{describe_disk, gib, shrink_line};
use crate::theme;

/// The word that enables Install.
pub const CONFIRMATION: &str = "erase";
pub const CONFIRM: &str = "confirm";

#[derive(Default)]
pub struct State {
    typed: String,
}

#[derive(Clone, Debug)]
pub enum Message {
    Typed(String),
}

impl State {
    /// Whether the user typed the confirmation word.
    pub fn confirmed(&self) -> bool {
        self.typed.trim().eq_ignore_ascii_case(CONFIRMATION)
    }

    /// Clears the confirmation and gives the text focus to its field, for when the screen
    /// opens: a changed answer needs a new confirmation.
    pub fn open(&mut self) -> Task<Message> {
        self.typed.clear();
        iced::widget::operation::focus(CONFIRM)
    }

    pub fn update(&mut self, message: Message) {
        let Message::Typed(value) = message;
        self.typed = value;
    }

    /// Draws the summary of `answers`, which are checked, or the reason they are not.
    pub fn view<'a>(
        &'a self,
        answers: &Result<Answers, String>,
        disk: Option<&DiskInfo>,
    ) -> Element<'a, Message> {
        let mut lines = Column::new().spacing(8);
        match answers {
            Err(reason) => {
                lines = lines.push(
                    text(format!("These answers cannot be installed: {reason}"))
                        .color(theme::DANGER),
                );
            }
            Ok(answers) => {
                lines = lines.push(text("The disk").size(theme::TEXT + 4).color(theme::PINK));
                for action in disk_actions(&answers.disk, disk) {
                    lines = lines.push(text(action));
                }
                lines = lines.push(text("The system").size(theme::TEXT + 4).color(theme::PINK));
                for line in overview(answers) {
                    lines = lines.push(text(line));
                }
            }
        }
        column![
            scrollable(lines).height(Length::Fill),
            text(format!(
                "Type {CONFIRMATION} to confirm that the disk changes above may happen."
            ))
            .color(theme::PINK),
            text_input(&format!("Type {CONFIRMATION} here"), &self.typed)
                .id(CONFIRM)
                .on_input(Message::Typed)
                .padding(theme::PAD)
                .size(theme::TEXT)
                .style(theme::input),
        ]
        .spacing(12)
        .into()
    }
}

/// The disk actions of `disk` in plain words, in the order the install does them. The first
/// line names what gets erased. `info` adds the model and size of the disk.
pub fn disk_actions(disk: &Disk, info: Option<&DiskInfo>) -> Vec<String> {
    let named = info.map_or_else(|| disk.device().to_owned(), describe_disk);
    match disk {
        Disk::WipeDisk {
            system_size_gib, ..
        } => {
            let mut layout = format!(
                "Creates a {} GiB EFI partition, a {system_size_gib} GiB system partition \
                 (Btrfs) and a home partition (ext4) with the rest of the disk",
                sizes::EFI_SIZE_GIB
            );
            if let Some(info) = info {
                let used = (sizes::EFI_SIZE_GIB + system_size_gib) * sizes::GIB;
                layout.push_str(&format!(
                    ", about {}",
                    gib(info.size_bytes.saturating_sub(used))
                ));
            }
            layout.push('.');
            vec![
                format!(
                    "Erases the whole disk {named}: every partition and every file on it is lost."
                ),
                layout,
            ]
        }
        Disk::ReinstallKeepHome {
            efi_partition,
            system_partition,
            home_partition,
            ..
        } => vec![
            format!(
                "Erases the system partition {system_partition} on {named} and formats it as \
                 the new system (Btrfs)."
            ),
            format!("Keeps the home partition {home_partition} and every file on it."),
            format!("Reuses the EFI partition {efi_partition} without formatting it."),
        ],
        Disk::ShrinkExt4KeepHome {
            system_size_gib,
            efi_partition,
            home_partition,
            ..
        } => vec![
            format!(
                "Shrinks the ext4 partition {home_partition} on {named} by {system_size_gib} \
                 GiB and creates the system partition (Btrfs) in the freed space after it."
            ),
            format!(
                "Inside {home_partition}, moves the home folder to the top and deletes \
                 everything else of the old system. The files in home stay."
            ),
            format!(
                "Reuses the EFI partition {efi_partition} and deletes the old system's kernels \
                 from it."
            ),
        ],
        Disk::BesideWindows {
            system_size_gib, ..
        } => {
            let offer = info.and_then(|info| {
                info.offers
                    .iter()
                    .find(|offer| offer.choice == Choice::BesideWindows)
            });
            let shrink = offer
                .and_then(|offer| offer.windows.as_ref())
                .zip(*system_size_gib)
                .and_then(|(shrink, gib)| shrink_line(shrink, gib));
            let system = system_size_gib.map_or_else(
                || "a system partition (Btrfs)".to_owned(),
                |gib| format!("a {gib} GiB system partition (Btrfs)"),
            );
            let efi = offer
                .and_then(|offer| offer.available.as_ref().ok())
                .and_then(|partitions| partitions.efi.as_deref())
                .map_or_else(String::new, |efi| format!(" {efi}"));
            vec![
                format!("Installs InayOS beside Windows on {named}. No Windows file is erased."),
                shrink.unwrap_or_else(|| {
                    "Shrinks the Windows partition, keeping its files and 20 GiB free.".to_owned()
                }),
                format!(
                    "Creates {system} and a home partition (ext4) in the free space after the \
                     Windows partition."
                ),
                format!(
                    "Reuses the EFI partition{efi} without formatting it. Windows keeps starting \
                     from its own firmware boot entry."
                ),
            ]
        }
    }
}

/// Every answer besides the disk, one line each.
pub fn overview(answers: &Answers) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Keyboard: {} (console keymap {}). Language: {}. Time zone: {}.",
            answers.xkb_layout.as_deref().unwrap_or("default"),
            answers.keyboard,
            answers.language,
            answers.timezone
        ),
        format!(
            "User: {} ({}). Computer name: {}.",
            answers.user.full_name, answers.user.username, answers.hostname
        ),
    ];
    if let Some(wifi) = &answers.wifi {
        lines.push(format!("Wi-Fi: {}.", wifi.ssid));
    }
    lines.push(if answers.security.secure_boot {
        "Secure Boot: on, with this computer's own keys enrolled in the firmware.".to_owned()
    } else {
        "Secure Boot: off.".to_owned()
    });
    lines.push(if answers.security.encryption {
        "Encryption: on. The TPM opens the disk at start, and the end of the install shows the \
         recovery key."
            .to_owned()
    } else {
        "Encryption: off.".to_owned()
    });
    lines.push(if answers.skip.is_empty() {
        "Modules: all of them.".to_owned()
    } else {
        format!("Modules left out: {}.", answers.skip.join(", "))
    });
    lines
}

#[cfg(test)]
mod tests {
    use inayos_install_core::Disk;

    use super::{State, disk_actions};
    use crate::screens::disk::tests::{disk, windows_shrink};

    #[test]
    fn a_wiped_disk_is_named_with_its_model_and_size_and_its_layout_adds_up() {
        let actions = disk_actions(
            &Disk::WipeDisk {
                device: "/dev/vda".into(),
                system_size_gib: 100,
            },
            Some(&disk()),
        );
        assert_eq!(
            actions,
            [
                "Erases the whole disk /dev/vda, QEMU HARDDISK, 128.0 GiB: every partition and \
                 every file on it is lost.",
                "Creates a 2 GiB EFI partition, a 100 GiB system partition (Btrfs) and a home \
                 partition (ext4) with the rest of the disk, about 26.0 GiB.",
            ]
        );
    }

    #[test]
    fn a_reinstall_erases_only_the_system_partition() {
        let actions = disk_actions(
            &Disk::ReinstallKeepHome {
                device: "/dev/vda".into(),
                efi_partition: "/dev/vda1".into(),
                system_partition: "/dev/vda2".into(),
                home_partition: "/dev/vda3".into(),
            },
            Some(&disk()),
        );
        assert_eq!(
            actions[0],
            "Erases the system partition /dev/vda2 on /dev/vda, QEMU HARDDISK, 128.0 GiB and \
             formats it as the new system (Btrfs)."
        );
        assert_eq!(
            actions[1],
            "Keeps the home partition /dev/vda3 and every file on it."
        );
    }

    #[test]
    fn a_shrink_names_the_partition_it_shrinks_and_what_it_deletes() {
        let actions = disk_actions(
            &Disk::ShrinkExt4KeepHome {
                device: "/dev/vda".into(),
                system_size_gib: 50,
                efi_partition: "/dev/vda1".into(),
                home_partition: "/dev/vda3".into(),
            },
            None,
        );
        assert_eq!(
            actions[0],
            "Shrinks the ext4 partition /dev/vda3 on /dev/vda by 50 GiB and creates the system \
             partition (Btrfs) in the freed space after it."
        );
    }

    #[test]
    fn beside_windows_names_how_much_windows_shrinks_and_the_reused_efi_partition() {
        let mut info = disk();
        info.offers[3] = inayos_install_core::inventory::Offer {
            choice: inayos_install_core::Choice::BesideWindows,
            available: Ok(inayos_install_core::inventory::Partitions {
                efi: Some("/dev/vda1".into()),
                system: None,
                home: None,
            }),
            system_size: None,
            windows: Some(windows_shrink()),
        };
        let actions = disk_actions(
            &Disk::BesideWindows {
                device: "/dev/vda".into(),
                system_size_gib: Some(30),
            },
            Some(&info),
        );
        assert_eq!(
            actions,
            [
                "Installs InayOS beside Windows on /dev/vda, QEMU HARDDISK, 128.0 GiB. No \
                 Windows file is erased.",
                "The Windows partition /dev/vda3 shrinks by 60.0 GiB, from 100.0 GiB to 40.0 \
                 GiB. Its files stay.",
                "Creates a 30 GiB system partition (Btrfs) and a home partition (ext4) in the \
                 free space after the Windows partition.",
                "Reuses the EFI partition /dev/vda1 without formatting it. Windows keeps \
                 starting from its own firmware boot entry.",
            ]
        );
    }

    #[test]
    fn install_needs_the_confirmation_word() {
        let mut state = State::default();
        assert!(!state.confirmed());
        state.update(super::Message::Typed("eras".into()));
        assert!(!state.confirmed());
        state.update(super::Message::Typed("erase".into()));
        assert!(state.confirmed());
    }
}

//! The Secure Boot and encryption screen. Both options are separate; encryption shows only
//! on a machine with a TPM 2.0. Secure Boot can be turned on while the firmware is in Setup
//! Mode, which lets the install enroll the machine's own keys. Encryption is not built yet,
//! so it shows turned off and disabled.
use iced::{
    Element,
    widget::{column, text, toggler},
};
use inayos_install_core::answers::Security;

use super::{Key, Reply};
use crate::theme;

const LATER: &str = "comes in a later version";

pub struct State {
    /// Whether the machine has a TPM 2.0, which encryption needs.
    tpm2: bool,
    /// Whether the firmware is in Setup Mode, or why that could not be read.
    setup_mode: Result<bool, String>,
    secure_boot: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    SecureBoot(bool),
}

impl State {
    /// Secure Boot starts off.
    pub fn new(tpm2: bool, setup_mode: Result<bool, String>) -> Self {
        State {
            tpm2,
            setup_mode,
            secure_boot: false,
        }
    }

    /// Whether the user can turn Secure Boot on.
    fn secure_boot_available(&self) -> bool {
        self.setup_mode == Ok(true)
    }

    pub fn update(&mut self, message: Message) {
        let Message::SecureBoot(on) = message;
        self.secure_boot = on && self.secure_boot_available();
    }

    /// Space turns Secure Boot on or off.
    pub fn key(&mut self, key: Key) -> Reply<Message> {
        if key == Key::Space {
            self.update(Message::SecureBoot(!self.secure_boot));
        }
        Reply::none()
    }

    pub fn answer(&self) -> Security {
        Security {
            secure_boot: self.secure_boot,
            encryption: false,
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let explanation = match &self.setup_mode {
            Ok(true) => "The firmware starts only boot files signed with this computer's own \
                         keys. The install enrolls them with Microsoft's keys, so Windows and \
                         add-in cards keep starting. Space turns it on or off."
                .to_owned(),
            Ok(false) => "The firmware is not in Setup Mode, so it cannot take this computer's \
                          own keys. To use Secure Boot, turn on Setup Mode in the firmware \
                          settings and start the installer again."
                .to_owned(),
            Err(reason) => {
                format!("Whether the firmware is in Setup Mode could not be read: {reason}.")
            }
        };
        let secure_boot = column![
            toggler(self.secure_boot)
                .label("Secure Boot")
                .size(32)
                .text_size(theme::TEXT + 2)
                .style(theme::toggler)
                .on_toggle_maybe(self.secure_boot_available().then_some(Message::SecureBoot)),
            text(explanation).color(theme::DIM),
        ]
        .spacing(8);
        let mut page = column![secure_boot].spacing(28);
        if self.tpm2 {
            page = page.push(
                column![
                    toggler(false)
                        .label("Encryption")
                        .size(32)
                        .text_size(theme::TEXT + 2)
                        .style(theme::toggler),
                    text(format!(
                        "The disk is encrypted and the TPM unlocks it at start. This {LATER}."
                    ))
                    .color(theme::DIM),
                ]
                .spacing(8),
            );
        }
        page.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_boot_turns_on_only_while_the_firmware_is_in_setup_mode() {
        let mut ready = State::new(false, Ok(true));
        ready.key(Key::Space);
        assert!(ready.answer().secure_boot);
        ready.update(Message::SecureBoot(false));
        assert!(!ready.answer().secure_boot);

        for setup_mode in [Ok(false), Err("efivarfs is missing".to_owned())] {
            let mut locked = State::new(false, setup_mode);
            locked.key(Key::Space);
            locked.update(Message::SecureBoot(true));
            assert!(!locked.answer().secure_boot);
        }
    }
}

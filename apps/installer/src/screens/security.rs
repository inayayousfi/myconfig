//! The Secure Boot and encryption screen. Both options are separate; encryption shows only
//! on a machine with a TPM 2.0. Secure Boot can be turned on while the firmware is in Setup
//! Mode, which lets the install enroll the machine's own keys. Encryption can be turned on
//! whenever it shows; the TPM then opens the disk at start, without a typed password.
use iced::{
    Border, Element,
    widget::{column, container, text, toggler},
};
use inayos_install_core::answers::Security;

use super::{Key, Reply};
use crate::theme;

pub struct State {
    /// Whether the machine has a TPM 2.0, which encryption needs.
    tpm2: bool,
    /// Whether the firmware is in Setup Mode, or why that could not be read.
    setup_mode: Result<bool, String>,
    secure_boot: bool,
    encryption: bool,
    /// The option that Space turns on or off.
    outlined: Switch,
}

/// One option of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Switch {
    SecureBoot,
    Encryption,
}

#[derive(Clone, Debug)]
pub enum Message {
    SecureBoot(bool),
    Encryption(bool),
}

impl State {
    /// Both options start off, with Secure Boot outlined.
    pub fn new(tpm2: bool, setup_mode: Result<bool, String>) -> Self {
        State {
            tpm2,
            setup_mode,
            secure_boot: false,
            encryption: false,
            outlined: Switch::SecureBoot,
        }
    }

    /// Whether the user can turn Secure Boot on.
    fn secure_boot_available(&self) -> bool {
        self.setup_mode == Ok(true)
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::SecureBoot(on) => {
                self.outlined = Switch::SecureBoot;
                self.secure_boot = on && self.secure_boot_available();
            }
            Message::Encryption(on) => {
                self.outlined = Switch::Encryption;
                self.encryption = on && self.tpm2;
            }
        }
    }

    /// Up and Down outline the other option, and Space turns the outlined one on or off.
    pub fn key(&mut self, key: Key) -> Reply<Message> {
        match key {
            Key::Up => self.outlined = Switch::SecureBoot,
            Key::Down if self.tpm2 => self.outlined = Switch::Encryption,
            Key::Space => match self.outlined {
                Switch::SecureBoot => self.update(Message::SecureBoot(!self.secure_boot)),
                Switch::Encryption => self.update(Message::Encryption(!self.encryption)),
            },
            _ => {}
        }
        Reply::none()
    }

    pub fn answer(&self) -> Security {
        Security {
            secure_boot: self.secure_boot,
            encryption: self.encryption,
        }
    }

    /// One option: its switch and the text under it, outlined when Space acts on it.
    fn option<'a>(
        &self,
        option: Switch,
        switch: toggler::Toggler<'a, Message>,
        explanation: String,
    ) -> Element<'a, Message> {
        let outlined = self.outlined == option;
        container(
            column![
                switch
                    .size(32)
                    .text_size(theme::TEXT + 2)
                    .style(theme::toggler),
                text(explanation).color(theme::DIM),
            ]
            .spacing(8),
        )
        .padding(theme::PAD)
        .style(move |_| container::Style {
            border: Border {
                color: if outlined { theme::PINK } else { theme::BLACK },
                width: 2.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let explanation = match &self.setup_mode {
            Ok(true) => "The firmware starts only boot files signed with this computer's own \
                         keys. The install enrolls them with Microsoft's keys, so Windows and \
                         add-in cards keep starting."
                .to_owned(),
            Ok(false) => "The firmware is not in Setup Mode, so it cannot take this computer's \
                          own keys. To use Secure Boot, turn on Setup Mode in the firmware \
                          settings and start the installer again."
                .to_owned(),
            Err(reason) => {
                format!("Whether the firmware is in Setup Mode could not be read: {reason}.")
            }
        };
        let secure_boot = self.option(
            Switch::SecureBoot,
            toggler(self.secure_boot)
                .label("Secure Boot")
                .on_toggle_maybe(self.secure_boot_available().then_some(Message::SecureBoot)),
            explanation,
        );
        let mut page = column![secure_boot].spacing(20);
        if self.tpm2 {
            page = page.push(
                self.option(
                    Switch::Encryption,
                    toggler(self.encryption)
                        .label("Encryption")
                        .on_toggle(Message::Encryption),
                    "The whole disk is encrypted, and the TPM opens it when the computer starts, \
                 without a password. The end of the install shows a recovery key that opens \
                 it when the TPM does not; keep it in a password manager. Encryption erases \
                 the whole disk."
                        .to_owned(),
                ),
            );
        }
        page.push(
            text("Up and Down choose an option, and Space turns it on or off.").color(theme::DIM),
        )
        .into()
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

    #[test]
    fn encryption_turns_on_only_with_a_tpm_and_apart_from_secure_boot() {
        let mut with_tpm = State::new(true, Ok(false));
        with_tpm.key(Key::Down);
        with_tpm.key(Key::Space);
        assert_eq!(
            with_tpm.answer(),
            Security {
                secure_boot: false,
                encryption: true
            }
        );
        with_tpm.key(Key::Space);
        assert!(!with_tpm.answer().encryption);

        let mut without_tpm = State::new(false, Ok(true));
        without_tpm.key(Key::Down);
        without_tpm.key(Key::Space);
        without_tpm.update(Message::Encryption(true));
        assert_eq!(
            without_tpm.answer(),
            Security {
                secure_boot: true,
                encryption: false
            }
        );
    }
}

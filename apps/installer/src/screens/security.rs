//! The Secure Boot and encryption screen. Both options are separate; encryption shows only
//! on a machine with a TPM 2.0. Neither is built yet, so both show turned off and disabled.
use iced::{
    Element,
    widget::{column, text, toggler},
};
use inayos_install_core::answers::Security;

use crate::theme;

const LATER: &str = "comes in a later version";

pub struct State {
    /// Whether the machine has a TPM 2.0, which encryption needs.
    tpm2: bool,
}

/// The screen has no message yet: its options cannot change.
#[derive(Clone, Debug)]
pub enum Message {}

impl State {
    pub fn new(tpm2: bool) -> Self {
        State { tpm2 }
    }

    pub fn answer(&self) -> Security {
        Security::default()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let option = |label: &'static str, explanation: &'static str| {
            column![
                toggler(false)
                    .label(label)
                    .size(32)
                    .text_size(theme::TEXT + 2)
                    .style(theme::toggler),
                text(format!("{explanation} This {LATER}.")).color(theme::DIM),
            ]
            .spacing(8)
        };
        let mut page = column![option(
            "Secure Boot",
            "The firmware starts only signed boot images."
        )]
        .spacing(28);
        if self.tpm2 {
            page = page.push(option(
                "Encryption",
                "The disk is encrypted and the TPM unlocks it at start.",
            ));
        }
        page.into()
    }
}

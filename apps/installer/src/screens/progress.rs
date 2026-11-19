//! The progress screen and its two ends: the live log while the install runs, then done with a
//! Reboot button, or failed with the error and the log path. Once an encrypted install
//! created the recovery key, done or failed shows it as text and as a QR code.
use std::path::PathBuf;

use iced::{
    Element, Font, Length, Task,
    font::Weight,
    widget::{Column, Id, canvas, column, container, operation, row, scrollable, text},
};
use inayos_install_core::{Installed, Line, RecoveryKey};

use crate::{qr::QrCode, theme};

const LOG: &str = "log";
/// The newest lines the screen draws. The log file keeps every line.
const SHOWN: usize = 400;

#[derive(Default)]
pub struct State {
    lines: Vec<Line>,
    /// The error of a failed install.
    failure: Option<String>,
    /// The recovery key of an encrypted install, once the install created it.
    recovery: Option<Recovery>,
}

/// The recovery key, its QR code, and where the new system keeps a copy.
struct Recovery {
    key: RecoveryKey,
    code: Option<QrCode>,
    file: Option<PathBuf>,
}

/// The side of the drawn QR code, in pixels.
const QR_SIDE: f32 = 300.0;

impl State {
    /// Adds the lines that arrived and scrolls the log to its end when there were any.
    pub fn receive<M: Send + 'static>(&mut self, lines: Vec<Line>) -> Task<M> {
        if lines.is_empty() {
            return Task::none();
        }
        self.lines.extend(lines);
        operation::snap_to_end(Id::new(LOG))
    }

    /// Keeps what a finished install handed over for the done screen.
    pub fn succeed(&mut self, installed: Installed) {
        self.keep_recovery(installed);
    }

    /// Keeps the error, and the recovery key when the install created it before it failed.
    pub fn fail(&mut self, error: String, installed: Installed) {
        self.failure = Some(error);
        self.keep_recovery(installed);
    }

    fn keep_recovery(&mut self, installed: Installed) {
        self.recovery = installed.recovery_key.map(|key| Recovery {
            code: QrCode::new(key.text()),
            key,
            file: installed.recovery_key_file,
        });
    }

    fn log<M: 'static>(&self) -> Element<'_, M> {
        // Details go to the log file only, as on the command line.
        let shown: Vec<&Line> = self
            .lines
            .iter()
            .filter(|line| !matches!(line, Line::Detail(_)))
            .collect();
        let start = shown.len().saturating_sub(SHOWN);
        let mut lines = Column::new().spacing(2).padding(12);
        for line in &shown[start..] {
            lines = lines.push(match line {
                Line::Step(step) => text(format!("==> {step}")).color(theme::PINK).font(Font {
                    weight: Weight::Bold,
                    ..theme::FONT
                }),
                Line::Command(command) => text(format!("$ {command}"))
                    .color(theme::DIM)
                    .font(Font::MONOSPACE),
                Line::Output(output) | Line::Detail(output) => {
                    text(output.as_str()).font(Font::MONOSPACE).size(16)
                }
            });
        }
        container(scrollable(lines).id(Id::new(LOG)).height(Length::Fill))
            .style(theme::panel)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    /// The last step the install reported.
    fn current_step(&self) -> Option<&str> {
        self.lines.iter().rev().find_map(|line| match line {
            Line::Step(step) => Some(step.as_str()),
            _ => None,
        })
    }

    pub fn running_view<M: 'static>(&self) -> Element<'_, M> {
        column![
            text(self.current_step().unwrap_or("Starting")).size(theme::TEXT + 4),
            text("This takes a while. Keep the computer on and connected.").color(theme::DIM),
            self.log(),
        ]
        .spacing(12)
        .into()
    }

    pub fn done_view<M: 'static>(&self) -> Element<'_, M> {
        let mut page =
            column![text("Remove the USB stick, then reboot into InayOS.").size(theme::TEXT + 4)]
                .spacing(12);
        if let Some(recovery) = &self.recovery {
            page = page.push(recovery_view(recovery));
        }
        page.push(self.log()).into()
    }

    pub fn failed_view<'a, M: 'static>(&'a self, log_path: &str) -> Element<'a, M> {
        let mut page = column![
            text(self.failure.clone().unwrap_or_default())
                .color(theme::DANGER)
                .size(theme::TEXT + 2),
            text(format!(
                "The whole log is in {log_path}. The repair workspace (workspace 2) has a \
                 terminal to look at it."
            )),
        ]
        .spacing(12);
        // The disk may already open with nothing but this key.
        if let Some(recovery) = &self.recovery {
            page = page.push(recovery_view(recovery));
        }
        page.push(self.log()).into()
    }
}

/// The recovery key as text and as a QR code, with what to do with it.
fn recovery_view<M: 'static>(recovery: &Recovery) -> Element<'_, M> {
    let mut words = column![
        text("Recovery key")
            .size(theme::TEXT + 4)
            .color(theme::PINK),
        text(recovery.key.text())
            .font(Font::MONOSPACE)
            .size(theme::TEXT + 6),
        text(
            "It opens the disk when the TPM does not, such as after a firmware update. Store \
             it in a password manager now: scan the code with a phone's camera, or copy the \
             text."
        ),
    ]
    .spacing(10)
    .width(Length::Fill);
    if let Some(file) = &recovery.file {
        words = words.push(
            text(format!(
                "{} in the new system is a copy, to delete once the key is in the password \
                 manager.",
                file.display()
            ))
            .color(theme::DIM),
        );
    }
    let mut parts = row![words].spacing(24);
    if let Some(code) = &recovery.code {
        parts = parts.push(
            canvas(code)
                .width(Length::Fixed(QR_SIDE))
                .height(Length::Fixed(QR_SIDE)),
        );
    }
    container(parts)
        .padding(theme::PAD)
        .style(theme::panel)
        .width(Length::Fill)
        .into()
}

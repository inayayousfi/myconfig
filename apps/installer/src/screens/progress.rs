//! The progress screen and its two ends: the live log while the install runs, then done with a
//! Reboot button, or failed with the error and the log path.
use iced::{
    Element, Font, Length, Task,
    font::Weight,
    widget::{Column, Id, column, container, operation, scrollable, text},
};
use inayos_install_core::Line;

use crate::theme;

const LOG: &str = "log";
/// The newest lines the screen draws. The log file keeps every line.
const SHOWN: usize = 400;

#[derive(Default)]
pub struct State {
    lines: Vec<Line>,
    /// The error of a failed install.
    failure: Option<String>,
}

impl State {
    /// Adds the lines that arrived and scrolls the log to its end when there were any.
    pub fn receive<M: Send + 'static>(&mut self, lines: Vec<Line>) -> Task<M> {
        if lines.is_empty() {
            return Task::none();
        }
        self.lines.extend(lines);
        operation::snap_to_end(Id::new(LOG))
    }

    pub fn fail(&mut self, error: String) {
        self.failure = Some(error);
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
        column![
            text("Remove the USB stick, then reboot into InayOS.").size(theme::TEXT + 4),
            self.log(),
        ]
        .spacing(12)
        .into()
    }

    pub fn failed_view<'a, M: 'static>(&'a self, log_path: &str) -> Element<'a, M> {
        column![
            text(self.failure.clone().unwrap_or_default())
                .color(theme::DANGER)
                .size(theme::TEXT + 2),
            text(format!(
                "The whole log is in {log_path}. The repair workspace (workspace 2) has a \
                 terminal to look at it."
            )),
            self.log(),
        ]
        .spacing(12)
        .into()
    }
}

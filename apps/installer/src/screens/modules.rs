//! The modules screen: a checkbox per module of the InayOS profile. The locked modules stay
//! ticked. Unticking a module unticks the modules that need it; ticking one ticks the modules
//! it needs.
use iced::{
    Element, Length,
    widget::{Column, Id, button, checkbox, column, scrollable, text},
};
use inayos_install_core::modules::{self, ModuleEntry};

use super::{Key, Reply, list};
use crate::theme;

const MODULES: &str = "modules";

pub struct State {
    entries: Result<Vec<ModuleEntry>, String>,
    ticked: Vec<bool>,
    /// The row Up, Down and Space act on.
    highlight: Option<usize>,
    /// What the last tick or untick changed besides the module itself.
    note: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Toggle(usize),
}

impl State {
    /// Every module starts ticked.
    pub fn new(entries: Result<Vec<ModuleEntry>, String>) -> Self {
        let count = entries.as_ref().map_or(0, Vec::len);
        State {
            entries,
            ticked: vec![true; count],
            highlight: (count > 0).then_some(0),
            note: None,
        }
    }

    pub fn ready(&self) -> bool {
        self.entries.is_ok()
    }

    /// The profile modules not to install, in profile order.
    pub fn skip(&self) -> Vec<String> {
        let Ok(entries) = &self.entries else {
            return Vec::new();
        };
        entries
            .iter()
            .zip(&self.ticked)
            .filter(|(_, ticked)| !**ticked)
            .map(|(entry, _)| entry.name.clone())
            .collect()
    }

    fn toggle(&mut self, index: usize) {
        let Ok(entries) = &self.entries else {
            return;
        };
        let Some(entry) = entries.get(index) else {
            return;
        };
        if entry.locked {
            return;
        }
        let tick = !self.ticked[index];
        let linked = if tick {
            modules::requirements(entries, &entry.name)
        } else {
            modules::dependents(entries, &entry.name)
        };
        let mut changed = Vec::new();
        self.ticked[index] = tick;
        for name in linked {
            if let Some(at) = entries.iter().position(|other| other.name == name)
                && self.ticked[at] != tick
                && !entries[at].locked
            {
                self.ticked[at] = tick;
                changed.push(name);
            }
        }
        self.note = (!changed.is_empty()).then(|| {
            if tick {
                format!(
                    "{} needs {}, so they are ticked too.",
                    entry.name,
                    changed.join(", ")
                )
            } else {
                format!(
                    "{} needs {}, so they are unticked too.",
                    changed.join(", "),
                    entry.name
                )
            }
        });
    }

    pub fn update(&mut self, message: Message) -> Reply<Message> {
        let Message::Toggle(index) = message;
        self.highlight = Some(index);
        self.toggle(index);
        Reply::none()
    }

    pub fn key(&mut self, key: Key) -> Reply<Message> {
        let count = self.ticked.len();
        match key {
            Key::Up | Key::Down => {
                let by = if key == Key::Up { -1 } else { 1 };
                match list::moved(self.highlight, count, by) {
                    Some(index) => {
                        self.highlight = Some(index);
                        Reply::task(list::reveal(MODULES, index))
                    }
                    None => Reply::none(),
                }
            }
            Key::Space => {
                if let Some(index) = self.highlight {
                    self.toggle(index);
                }
                Reply::none()
            }
            Key::Next | Key::Previous => Reply::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let entries = match &self.entries {
            Ok(entries) => entries,
            Err(reason) => {
                return text(format!("The modules cannot be read: {reason}"))
                    .color(theme::DANGER)
                    .into();
            }
        };
        let mut rows = Column::new().spacing(4).padding([0, 12]);
        for (index, entry) in entries.iter().enumerate() {
            let label = if entry.locked {
                format!("{} (always installed)", entry.name)
            } else {
                entry.name.clone()
            };
            let mut tick = checkbox(self.ticked[index])
                .label(label)
                .size(26)
                .text_size(theme::TEXT)
                .width(Length::Fill);
            if !entry.locked {
                tick = tick.on_toggle(move |_| Message::Toggle(index));
            }
            let highlighted = self.highlight == Some(index);
            let mut row = button(tick)
                .width(Length::Fill)
                .height(Length::Fixed(theme::ROW - 4.0))
                .padding([12, theme::PAD])
                .style(move |theme, status| {
                    let mut style = crate::theme::row(false, false)(theme, status);
                    if highlighted {
                        style.border.color = crate::theme::PINK;
                        style.border.width = 3.0;
                    }
                    style
                });
            if !entry.locked {
                row = row.on_press(Message::Toggle(index));
            }
            rows = rows.push(row);
        }
        let mut page = column![
            text("Untick what you do not want. Space ticks or unticks the outlined row.")
                .color(theme::DIM),
        ]
        .spacing(12);
        if let Some(note) = &self.note {
            page = page.push(text(note).color(theme::PINK));
        }
        page.push(scrollable(rows).id(Id::new(MODULES)).height(Length::Fill))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use inayos_install_core::modules::ModuleEntry;

    use super::{Key, Message, State};

    fn entry(name: &str, needs: &[&str], locked: bool) -> ModuleEntry {
        ModuleEntry {
            name: name.into(),
            hard_needs: needs.iter().map(|need| (*need).into()).collect(),
            locked,
        }
    }

    fn state() -> State {
        State::new(Ok(vec![
            entry("base", &[], true),
            entry("runtimes", &["base"], false),
            entry("agents-packages", &["base", "runtimes"], false),
            entry("agent-config", &["agents-packages"], false),
            entry("media", &["base"], false),
        ]))
    }

    #[test]
    fn unticking_a_module_unticks_every_module_that_needs_it() {
        let mut state = state();
        state.update(Message::Toggle(1));
        assert_eq!(
            state.skip(),
            ["runtimes", "agents-packages", "agent-config"]
        );
    }

    #[test]
    fn ticking_a_module_again_ticks_the_modules_it_needs() {
        let mut state = state();
        state.update(Message::Toggle(1));
        state.update(Message::Toggle(3));
        assert!(state.skip().is_empty());
    }

    #[test]
    fn a_locked_module_stays_ticked_by_click_and_by_space() {
        let mut state = state();
        state.update(Message::Toggle(0));
        state.key(Key::Space);
        assert!(state.skip().is_empty());
        state.key(Key::Down);
        state.key(Key::Down);
        state.key(Key::Down);
        state.key(Key::Down);
        state.key(Key::Space);
        assert_eq!(state.skip(), ["media"]);
    }
}

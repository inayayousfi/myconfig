//! The installer's screens. Each screen owns the state of its answers, turns its messages and
//! keys into new state, and draws itself. A screen never acts on the machine: it returns an
//! `Effect` that the app carries out.
pub mod disk;
pub mod keyboard;
pub mod list;
pub mod modules;
pub mod progress;
pub mod security;
pub mod summary;
pub mod user;
pub mod wifi;

use iced::{
    Task,
    widget::{Id, operation},
};

/// The screens, in the order the installer shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Keyboard,
    Wifi,
    Disk,
    Security,
    User,
    Modules,
    Summary,
    Progress,
    Done,
    Failed,
}

/// The screens that fill the answers, in order. The summary starts the install.
const FORM: [Screen; 7] = [
    Screen::Keyboard,
    Screen::Wifi,
    Screen::Disk,
    Screen::Security,
    Screen::User,
    Screen::Modules,
    Screen::Summary,
];

impl Screen {
    /// The name written to the screen file for the VM tests.
    pub fn name(self) -> &'static str {
        match self {
            Screen::Keyboard => "keyboard",
            Screen::Wifi => "wifi",
            Screen::Disk => "disk",
            Screen::Security => "security",
            Screen::User => "user",
            Screen::Modules => "modules",
            Screen::Summary => "summary",
            Screen::Progress => "progress",
            Screen::Done => "done",
            Screen::Failed => "failed",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Screen::Keyboard => "Keyboard and language",
            Screen::Wifi => "Wi-Fi",
            Screen::Disk => "Disk",
            Screen::Security => "Secure Boot and encryption",
            Screen::User => "You and this computer",
            Screen::Modules => "What to install",
            Screen::Summary => "Check and install",
            Screen::Progress => "Installing",
            Screen::Done => "InayOS is installed",
            Screen::Failed => "The install failed",
        }
    }

    /// The screen that Next opens. `with_wifi` is false when the live system already has a
    /// connection, which skips the Wi-Fi screen. None after the summary, whose next step is
    /// the install itself.
    pub fn next(self, with_wifi: bool) -> Option<Screen> {
        let shown = Self::shown(with_wifi);
        let at = shown.iter().position(|screen| *screen == self)?;
        shown.get(at + 1).copied()
    }

    /// The screen that Back opens. None on the first screen and once the install started.
    pub fn previous(self, with_wifi: bool) -> Option<Screen> {
        let shown = Self::shown(with_wifi);
        let at = shown.iter().position(|screen| *screen == self)?;
        at.checked_sub(1).map(|at| shown[at])
    }

    fn shown(with_wifi: bool) -> Vec<Screen> {
        FORM.into_iter()
            .filter(|screen| with_wifi || *screen != Screen::Wifi)
            .collect()
    }
}

/// A key that acts inside a screen. Enter and Escape act on the whole app instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Space,
    /// Tab: the next field.
    Next,
    /// Shift+Tab: the previous field.
    Previous,
}

/// A change to the live machine that a screen asks for. The app carries it out through the
/// core's `Host` and sends the result back to the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Applies an XKB layout to the live session's keyboards.
    ApplyLayout(String),
    /// Reads the Wi-Fi networks in range and whether the live system is connected.
    ScanWifi,
    /// Connects the live system to a Wi-Fi network.
    ConnectWifi {
        ssid: String,
        password: Option<String>,
    },
}

/// What a screen returns from a message or a key: widget operations, such as moving the text
/// focus or scrolling a list, and at most one change to the live machine.
pub struct Reply<M> {
    pub task: Task<M>,
    pub effect: Option<Effect>,
}

impl<M> Reply<M> {
    pub fn none() -> Self {
        Reply {
            task: Task::none(),
            effect: None,
        }
    }

    pub fn task(task: Task<M>) -> Self {
        Reply { task, effect: None }
    }

    pub fn effect(effect: Effect) -> Self {
        Reply {
            task: Task::none(),
            effect: Some(effect),
        }
    }
}

/// A field that Tab reaches. A list takes Up, Down and Space; a text input takes typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    List(&'static str),
    Input(&'static str),
}

/// The keyboard focus of a screen: which of its fields Tab last reached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Focus(pub usize);

impl Focus {
    /// Moves to the next or previous field, wrapping, and gives the text focus to that field
    /// when it is a text input. Removes the text focus from every input otherwise, so typed
    /// spaces toggle instead of typing.
    pub fn step<M: Send + 'static>(&mut self, fields: &[Field], forward: bool) -> Task<M> {
        if fields.is_empty() {
            return Task::none();
        }
        let count = fields.len();
        self.0 = if forward {
            (self.0 + 1) % count
        } else {
            (self.0 + count - 1) % count
        };
        self.apply(fields)
    }

    /// The field that has the focus.
    pub fn field(self, fields: &[Field]) -> Option<Field> {
        fields.get(self.0).copied()
    }

    /// Moves the focus to `field`, as a click or typing in it does.
    pub fn set(&mut self, fields: &[Field], field: Field) {
        if let Some(at) = fields.iter().position(|candidate| *candidate == field) {
            self.0 = at;
        }
    }

    /// Gives the text focus to the focused field when it is a text input, or to none.
    pub fn apply<M: Send + 'static>(self, fields: &[Field]) -> Task<M> {
        match self.field(fields) {
            Some(Field::Input(id)) => operation::focus(Id::new(id)),
            _ => operation::focus(Id::new("no-input")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Screen;

    #[test]
    fn next_and_back_follow_the_screen_order_and_skip_wifi_with_a_connection() {
        let walk = |with_wifi| {
            let mut screens = vec![Screen::Keyboard];
            while let Some(next) = screens.last().unwrap().next(with_wifi) {
                screens.push(next);
            }
            screens
        };
        assert_eq!(
            walk(true),
            [
                Screen::Keyboard,
                Screen::Wifi,
                Screen::Disk,
                Screen::Security,
                Screen::User,
                Screen::Modules,
                Screen::Summary
            ]
        );
        assert_eq!(
            walk(false),
            [
                Screen::Keyboard,
                Screen::Disk,
                Screen::Security,
                Screen::User,
                Screen::Modules,
                Screen::Summary
            ]
        );
        assert_eq!(Screen::Disk.previous(false), Some(Screen::Keyboard));
        assert_eq!(Screen::Disk.previous(true), Some(Screen::Wifi));
        assert_eq!(Screen::Keyboard.previous(true), None);
    }

    #[test]
    fn the_install_screens_have_no_next_or_back() {
        for screen in [Screen::Progress, Screen::Done, Screen::Failed] {
            assert_eq!(screen.next(true), None);
            assert_eq!(screen.previous(true), None);
        }
    }
}

//! The Wi-Fi screen: the networks in range, a password field and a connect button. The app
//! skips it when the live system already has a connection.
use iced::{
    Element, Length,
    widget::{button, column, row, text, text_input},
};
use inayos_install_core::answers::{Secret, Wifi};

use super::{Effect, Field, Focus, Key, Reply, list};
use crate::{live::Network, theme};

const NETWORKS: &str = "networks";
pub const PASSWORD: &str = "wifi-password";
const FIELDS: [Field; 2] = [Field::List(NETWORKS), Field::Input(PASSWORD)];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Status {
    Scanning,
    Ready,
    Connecting(String),
    Connected(String),
    Failed(String),
}

pub struct State {
    networks: Vec<Network>,
    selected: Option<usize>,
    password: String,
    status: Status,
    /// The network the screen connected to, which the answers carry so the install connects
    /// to it again.
    joined: Option<Wifi>,
    /// Whether the last scan found the live system connected, by cable or otherwise.
    online: bool,
    focus: Focus,
}

#[derive(Clone, Debug)]
pub enum Message {
    Select(usize),
    Password(String),
    Connect,
    Rescan,
    /// The networks in range, or why they could not be read, and whether the live system
    /// is connected.
    Scanned {
        networks: Result<Vec<Network>, String>,
        online: bool,
    },
    /// The end of a connection attempt.
    Joined(Result<(), String>),
}

impl State {
    /// A screen that scans as soon as it opens.
    pub fn new() -> Self {
        State {
            networks: Vec::new(),
            selected: None,
            password: String::new(),
            status: Status::Scanning,
            joined: None,
            online: false,
            focus: Focus::default(),
        }
    }

    /// What the screen asks for when it opens: a scan.
    pub fn open(&mut self) -> Effect {
        self.status = Status::Scanning;
        Effect::ScanWifi
    }

    pub fn answer(&self) -> Option<Wifi> {
        self.joined.clone()
    }

    fn network(&self) -> Option<&Network> {
        self.networks.get(self.selected?)
    }

    /// Whether Enter connects instead of going on: a network is chosen, the live system is
    /// not connected, and the screen has not tried that network yet. After a failed attempt
    /// Enter goes on, and choosing a network or typing the password again lets it connect.
    pub fn enter_connects(&self) -> bool {
        !self.online
            && !matches!(self.status, Status::Failed(_))
            && self.network().is_some_and(|network| {
                !matches!(&self.status, Status::Connected(ssid) | Status::Connecting(ssid) if *ssid == network.ssid)
            })
    }

    /// A new choice makes a failed attempt worth trying again.
    fn forget_failure(&mut self) {
        if matches!(self.status, Status::Failed(_)) {
            self.status = Status::Ready;
        }
    }

    fn connect(&mut self) -> Reply<Message> {
        let Some(network) = self.network() else {
            return Reply::none();
        };
        let ssid = network.ssid.clone();
        let password =
            (network.secured && !self.password.is_empty()).then(|| self.password.clone());
        self.status = Status::Connecting(ssid.clone());
        Reply::effect(Effect::ConnectWifi { ssid, password })
    }

    pub fn update(&mut self, message: Message) -> Reply<Message> {
        match message {
            Message::Select(index) => {
                self.focus.set(&FIELDS, Field::List(NETWORKS));
                self.selected = Some(index);
                self.forget_failure();
            }
            Message::Password(value) => {
                self.focus.set(&FIELDS, Field::Input(PASSWORD));
                self.password = value;
                self.forget_failure();
            }
            Message::Connect => return self.connect(),
            Message::Rescan => return Reply::effect(self.open()),
            Message::Scanned {
                networks: Ok(networks),
                online,
            } => {
                self.online = online;
                let chosen = self.network().map(|network| network.ssid.clone());
                self.networks = networks;
                self.selected = chosen
                    .and_then(|ssid| self.networks.iter().position(|n| n.ssid == ssid))
                    .or_else(|| (!self.networks.is_empty()).then_some(0));
                if self.status == Status::Scanning {
                    self.status = Status::Ready;
                }
            }
            Message::Scanned {
                networks: Err(reason),
                online,
            } => {
                self.online = online;
                self.status = Status::Failed(format!("The networks cannot be read: {reason}"));
            }
            Message::Joined(result) => {
                let Status::Connecting(ssid) = &self.status else {
                    return Reply::none();
                };
                let ssid = ssid.clone();
                match result {
                    Ok(()) => {
                        let password = self
                            .networks
                            .iter()
                            .find(|network| network.ssid == ssid)
                            .filter(|network| network.secured && !self.password.is_empty())
                            .map(|_| Secret(self.password.clone()));
                        self.joined = Some(Wifi {
                            ssid: ssid.clone(),
                            password,
                        });
                        self.status = Status::Connected(ssid);
                    }
                    Err(reason) => {
                        self.status =
                            Status::Failed(format!("Connecting to {ssid} failed: {reason}"));
                    }
                }
            }
        }
        Reply::none()
    }

    pub fn key(&mut self, key: Key) -> Reply<Message> {
        match key {
            Key::Next | Key::Previous => Reply::task(self.focus.step(&FIELDS, key == Key::Next)),
            Key::Up | Key::Down if self.focus.field(&FIELDS) == Some(Field::List(NETWORKS)) => {
                let by = if key == Key::Up { -1 } else { 1 };
                match list::moved(self.selected, self.networks.len(), by) {
                    Some(index) => {
                        self.selected = Some(index);
                        self.forget_failure();
                        Reply::task(list::reveal(NETWORKS, index))
                    }
                    None => Reply::none(),
                }
            }
            Key::Space => self.connect(),
            _ => Reply::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let focused = self.focus.field(&FIELDS);
        let rows = self
            .networks
            .iter()
            .map(|network| {
                let lock = if network.secured {
                    ", password"
                } else {
                    ", open"
                };
                list::Row::new(format!("{}  ({}%{lock})", network.ssid, network.signal))
            })
            .collect();
        let (status, color) = match &self.status {
            Status::Scanning => ("Looking for networks…".to_owned(), theme::DIM),
            Status::Ready if self.online => (
                "This computer is connected. Press Enter to go on, or choose a Wi-Fi network."
                    .to_owned(),
                theme::PINK,
            ),
            Status::Ready if self.networks.is_empty() => (
                "No network in range. The install needs the internet: plug in a cable, then look again (F5)."
                    .to_owned(),
                theme::DIM,
            ),
            Status::Ready => (
                "Choose a network, type its password, then connect (Enter or Space). F5 looks again."
                    .to_owned(),
                theme::DIM,
            ),
            Status::Connecting(ssid) => (format!("Connecting to {ssid}…"), theme::DIM),
            Status::Connected(ssid) => (format!("Connected to {ssid}."), theme::PINK),
            Status::Failed(reason) => (
                format!("{reason}. Enter goes on; choose a network or retype the password to try again."),
                theme::DANGER,
            ),
        };
        let needs_password = self.network().is_some_and(|network| network.secured);
        let connect = button(text("Connect").size(theme::TEXT))
            .padding(theme::PAD)
            .style(theme::primary)
            .on_press_maybe(self.network().is_some().then_some(Message::Connect));
        let rescan = button(text("Look again").size(theme::TEXT))
            .padding(theme::PAD)
            .style(theme::secondary)
            .on_press(Message::Rescan);
        let mut password = text_input("Wi-Fi password", &self.password)
            .id(PASSWORD)
            .secure(true)
            .padding(theme::PAD)
            .size(theme::TEXT)
            .style(theme::input);
        if needs_password {
            password = password.on_input(Message::Password);
        }
        column![
            text(status).color(color),
            list::view(
                NETWORKS,
                rows,
                self.selected,
                focused == Some(Field::List(NETWORKS)),
                Length::Fill,
                Message::Select,
            ),
            password,
            row![connect, rescan].spacing(12),
        ]
        .spacing(12)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use inayos_install_core::answers::{Secret, Wifi};

    use super::{Effect, Message, State};
    use crate::live::Network;

    #[test]
    fn a_successful_connection_goes_into_the_answers_with_its_password() {
        let mut state = State::new();
        state.update(Message::Scanned {
            networks: Ok(vec![
                Network {
                    ssid: "Cafe".into(),
                    signal: 80,
                    secured: false,
                },
                Network {
                    ssid: "Home".into(),
                    signal: 60,
                    secured: true,
                },
            ]),
            online: false,
        });
        state.update(Message::Select(1));
        state.update(Message::Password("hunter22".into()));
        assert!(state.enter_connects());
        let reply = state.update(Message::Connect);
        assert_eq!(
            reply.effect,
            Some(Effect::ConnectWifi {
                ssid: "Home".into(),
                password: Some("hunter22".into()),
            })
        );
        assert_eq!(
            state.answer(),
            None,
            "no answer before the connection works"
        );
        state.update(Message::Joined(Ok(())));
        assert_eq!(
            state.answer(),
            Some(Wifi {
                ssid: "Home".into(),
                password: Some(Secret("hunter22".into())),
            })
        );
        assert!(!state.enter_connects(), "Enter goes on once connected");
    }

    fn scanned(online: bool) -> Message {
        Message::Scanned {
            networks: Ok(vec![Network {
                ssid: "Home".into(),
                signal: 60,
                secured: true,
            }]),
            online,
        }
    }

    #[test]
    fn after_a_failed_attempt_enter_goes_on_until_the_password_is_typed_again() {
        let mut state = State::new();
        state.update(scanned(false));
        state.update(Message::Password("wrong".into()));
        state.update(Message::Connect);
        state.update(Message::Joined(Err("Secrets were required".into())));
        assert!(!state.enter_connects(), "Enter goes on after a failure");
        state.update(Message::Password("right".into()));
        assert!(
            state.enter_connects(),
            "a new password lets Enter try again"
        );
    }

    #[test]
    fn a_connected_system_lets_enter_go_on_and_space_still_connects() {
        let mut state = State::new();
        state.update(scanned(true));
        assert!(!state.enter_connects());
        let reply = state.key(super::Key::Space);
        assert!(matches!(reply.effect, Some(Effect::ConnectWifi { .. })));
    }
}

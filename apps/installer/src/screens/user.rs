//! The user screen: full name, a username suggested from it, the password twice, the computer
//! name, and the time zone from a searchable list that starts at the keyboard's country.
use iced::{
    Element, Length,
    widget::{column, row, scrollable, text, text_input},
};
use inayos_install_core::{
    answers::{self, Secret, User},
    catalog::{self, TimeZone},
};

use super::{Field, Focus, Key, Reply, list};
use crate::theme;

/// The computer name until the user types another.
pub const DEFAULT_HOSTNAME: &str = "inayos";
/// The time zone when the keyboard names no country.
const FALLBACK_ZONE: &str = "UTC";

pub const FULL_NAME: &str = "full-name";
const USERNAME: &str = "username";
const PASSWORD: &str = "password";
const PASSWORD_AGAIN: &str = "password-again";
const HOSTNAME: &str = "hostname";
const SEARCH: &str = "zone-search";
const ZONES: &str = "zones";
const FIELDS: [Field; 7] = [
    Field::Input(FULL_NAME),
    Field::Input(USERNAME),
    Field::Input(PASSWORD),
    Field::Input(PASSWORD_AGAIN),
    Field::Input(HOSTNAME),
    Field::Input(SEARCH),
    Field::List(ZONES),
];

pub struct State {
    full_name: String,
    username: String,
    /// Whether the user typed the username. Until then it follows the full name.
    username_typed: bool,
    password: String,
    password_again: String,
    hostname: String,
    /// Every time zone, or why the list could not be read.
    all_zones: Result<Vec<TimeZone>, String>,
    /// The zones in the order the list shows them: the keyboard's country first.
    zones: Vec<TimeZone>,
    search: String,
    zone: Option<String>,
    /// Whether the user chose the zone. Until then it follows the keyboard's country.
    zone_chosen: bool,
    /// The keyboard layout the live session types with, when it is not the US one.
    other_layout: Option<String>,
    focus: Focus,
}

#[derive(Clone, Debug)]
pub enum Message {
    FullName(String),
    Username(String),
    Password(String),
    PasswordAgain(String),
    Hostname(String),
    Search(String),
    Zone(usize),
}

impl State {
    pub fn new(zones: Result<Vec<TimeZone>, String>) -> Self {
        let mut state = State {
            full_name: String::new(),
            username: String::new(),
            username_typed: false,
            password: String::new(),
            password_again: String::new(),
            hostname: DEFAULT_HOSTNAME.to_owned(),
            all_zones: zones,
            zones: Vec::new(),
            search: String::new(),
            zone: None,
            zone_chosen: false,
            other_layout: None,
            focus: Focus::default(),
        };
        state.set_country(None);
        state
    }

    /// Notes the layout the password is typed with. The installed desktop starts with the
    /// US layout, so the screen warns when the live one differs.
    pub fn set_layout(&mut self, layout: Option<&str>) {
        self.other_layout = layout.filter(|layout| *layout != "us").map(str::to_owned);
    }

    /// Orders the zones with `country` first and, until the user chose a zone, selects that
    /// country's main zone.
    pub fn set_country(&mut self, country: Option<&str>) {
        let Ok(all) = &self.all_zones else {
            return;
        };
        self.zones = catalog::time_zones_for(all, country);
        if self.zone_chosen {
            return;
        }
        let main = self.zones.first().filter(|zone| {
            country.is_some() && zone.countries.first().map(String::as_str) == country
        });
        self.zone = Some(main.map_or(FALLBACK_ZONE.to_owned(), |zone| zone.name.clone()));
    }

    /// The zones that match the search, in list order.
    fn shown(&self) -> Vec<&TimeZone> {
        let words: Vec<String> = self
            .search
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        self.zones
            .iter()
            .filter(|zone| {
                let name = zone.name.to_lowercase().replace('_', " ");
                words.iter().all(|word| name.contains(word.as_str()))
            })
            .collect()
    }

    pub fn timezone(&self) -> Option<&str> {
        self.zone.as_deref()
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn answer(&self) -> User {
        User {
            full_name: self.full_name.trim().to_owned(),
            username: self.username.clone(),
            password: Secret(self.password.clone()),
            authorized_keys: Vec::new(),
        }
    }

    /// What still keeps Next disabled, in plain words, or None. The core's rules decide
    /// which usernames and computer names it accepts.
    pub fn missing(&self) -> Option<String> {
        let say = |words: &str| Some(words.to_owned());
        if self.full_name.trim().is_empty() {
            say("Type your name.")
        } else if self.username.is_empty() {
            say("Type a username.")
        } else if let Err(error) = answers::check_username(&self.username) {
            Some(format!("The username cannot be used: {error}."))
        } else if self.password.is_empty() {
            say("Type a password.")
        } else if self.password != self.password_again {
            say("The two passwords differ.")
        } else if let Err(error) = answers::check_hostname(&self.hostname) {
            Some(format!("The computer name cannot be used: {error}."))
        } else if self.zone.is_none() {
            say("Choose a time zone.")
        } else {
            None
        }
    }

    /// Gives the text focus to the full name and scrolls the zone list to the chosen zone,
    /// for when the screen opens.
    pub fn open(&mut self) -> iced::Task<Message> {
        self.focus = Focus::default();
        let at = self
            .shown()
            .iter()
            .position(|zone| Some(zone.name.as_str()) == self.zone.as_deref());
        iced::Task::batch([
            self.focus.apply(&FIELDS),
            list::reveal(ZONES, at.unwrap_or(0)),
        ])
    }

    pub fn update(&mut self, message: Message) -> Reply<Message> {
        let field = match &message {
            Message::FullName(_) => Field::Input(FULL_NAME),
            Message::Username(_) => Field::Input(USERNAME),
            Message::Password(_) => Field::Input(PASSWORD),
            Message::PasswordAgain(_) => Field::Input(PASSWORD_AGAIN),
            Message::Hostname(_) => Field::Input(HOSTNAME),
            Message::Search(_) => Field::Input(SEARCH),
            Message::Zone(_) => Field::List(ZONES),
        };
        self.focus.set(&FIELDS, field);
        match message {
            Message::FullName(value) => {
                if !self.username_typed {
                    self.username = suggest_username(&value);
                }
                self.full_name = value;
            }
            Message::Username(value) => {
                self.username_typed = !value.is_empty();
                self.username = value;
            }
            Message::Password(value) => self.password = value,
            Message::PasswordAgain(value) => self.password_again = value,
            Message::Hostname(value) => self.hostname = value,
            Message::Search(value) => {
                self.search = value;
                // The search picks the first match, so typing and Enter is enough.
                if let Some(first) = self.shown().first().map(|zone| zone.name.clone()) {
                    self.zone = Some(first);
                    self.zone_chosen = true;
                }
            }
            Message::Zone(index) => {
                if let Some(zone) = self.shown().get(index).map(|zone| zone.name.clone()) {
                    self.zone = Some(zone);
                    self.zone_chosen = true;
                }
            }
        }
        Reply::none()
    }

    pub fn key(&mut self, key: Key) -> Reply<Message> {
        let by = match key {
            Key::Up => -1,
            Key::Down => 1,
            Key::Space => return Reply::none(),
            Key::Next | Key::Previous => {
                return Reply::task(self.focus.step(&FIELDS, key == Key::Next));
            }
        };
        let on_zones = matches!(
            self.focus.field(&FIELDS),
            Some(Field::Input(SEARCH) | Field::List(ZONES))
        );
        if !on_zones {
            return Reply::none();
        }
        let shown = self.shown();
        let at = shown
            .iter()
            .position(|zone| Some(zone.name.as_str()) == self.zone.as_deref());
        let Some(index) = list::moved(at, shown.len(), by) else {
            return Reply::none();
        };
        self.zone = Some(shown[index].name.clone());
        self.zone_chosen = true;
        Reply::task(list::reveal(ZONES, index))
    }

    pub fn view(&self) -> Element<'_, Message> {
        let focused = self.focus.field(&FIELDS);
        let input = |placeholder: &str, value: &str, id: &'static str, secure: bool| {
            text_input(placeholder, value)
                .id(id)
                .secure(secure)
                .padding(theme::PAD)
                .size(theme::TEXT)
                .style(theme::input)
        };
        let label = |words: &'static str| text(words).color(theme::PINK);
        let layout_warning = self.other_layout.as_ref().map(|layout| {
            text(format!(
                "You type here with the {layout} layout, but the InayOS desktop starts with \
                 the US English layout. Choose a password that you can type on both."
            ))
            .color(theme::DANGER)
        });
        let form = column![
            label("Your name"),
            input("Full name", &self.full_name, FULL_NAME, false).on_input(Message::FullName),
            label("Username"),
            input("username", &self.username, USERNAME, false).on_input(Message::Username),
            label("Password"),
            layout_warning,
            input("Password", &self.password, PASSWORD, true).on_input(Message::Password),
            input(
                "The same password again",
                &self.password_again,
                PASSWORD_AGAIN,
                true
            )
            .on_input(Message::PasswordAgain),
            label("Computer name"),
            input("Computer name", &self.hostname, HOSTNAME, false).on_input(Message::Hostname),
        ]
        .spacing(10)
        .width(Length::FillPortion(1));
        let shown = self.shown();
        let selected = shown
            .iter()
            .position(|zone| Some(zone.name.as_str()) == self.zone.as_deref());
        let zones: Element<'_, Message> = match &self.all_zones {
            Ok(_) => list::view(
                ZONES,
                shown
                    .iter()
                    .map(|zone| list::Row::new(zone.name.replace('_', " ")))
                    .collect(),
                selected,
                focused == Some(Field::List(ZONES)),
                Length::Fill,
                Message::Zone,
            ),
            Err(reason) => text(format!(
                "The time zones cannot be read: {reason}. The clock uses UTC."
            ))
            .color(theme::DANGER)
            .into(),
        };
        let zone_column = column![
            label("Time zone"),
            text(format!(
                "Chosen: {}",
                self.zone.as_deref().unwrap_or("none")
            )),
            input("Search, such as Paris", &self.search, SEARCH, false).on_input(Message::Search),
            zones,
        ]
        .spacing(10)
        .width(Length::FillPortion(1));
        row![
            scrollable(form)
                .height(Length::Fill)
                .width(Length::FillPortion(1)),
            zone_column
        ]
        .spacing(24)
        .height(Length::Fill)
        .into()
    }
}

/// The username suggested for `full_name`: the first word that gives a valid start, in
/// lowercase ASCII, with accents removed and other characters dropped. Empty when no word
/// gives one.
pub fn suggest_username(full_name: &str) -> String {
    full_name
        .split_whitespace()
        .map(|word| {
            let plain: String = word
                .chars()
                .flat_map(fold)
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            let start = plain
                .find(|c: char| c.is_ascii_lowercase() || c == '_')
                .unwrap_or(plain.len());
            plain[start..].chars().take(32).collect::<String>()
        })
        .find(|name| !name.is_empty())
        .unwrap_or_default()
}

/// A character in lowercase ASCII: accented Latin letters lose their accent, and the letters
/// that have no single ASCII form become two.
fn fold(c: char) -> Vec<char> {
    let lower: Vec<char> = c.to_lowercase().collect();
    lower
        .into_iter()
        .flat_map(|c| -> Vec<char> {
            match c {
                'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => vec!['a'],
                'ç' | 'ć' | 'č' => vec!['c'],
                'ď' | 'đ' => vec!['d'],
                'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => vec!['e'],
                'ğ' => vec!['g'],
                'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => vec!['i'],
                'ł' | 'ľ' | 'ĺ' => vec!['l'],
                'ñ' | 'ń' | 'ň' => vec!['n'],
                'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => vec!['o'],
                'ř' | 'ŕ' => vec!['r'],
                'ś' | 'š' | 'ş' | 'ș' => vec!['s'],
                'ť' | 'ţ' | 'ț' => vec!['t'],
                'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => vec!['u'],
                'ý' | 'ÿ' => vec!['y'],
                'ź' | 'ż' | 'ž' => vec!['z'],
                'æ' => vec!['a', 'e'],
                'œ' => vec!['o', 'e'],
                'ß' => vec!['s', 's'],
                c => vec![c],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use inayos_install_core::catalog::TimeZone;

    use super::{Key, Message, State, suggest_username};

    #[test]
    fn the_username_comes_from_the_first_usable_word_of_the_name_in_plain_lowercase() {
        assert_eq!(suggest_username("Inaya Yousfi"), "inaya");
        assert_eq!(suggest_username("Élodie Martin"), "elodie");
        assert_eq!(suggest_username("Søren O'Brien"), "soren");
        assert_eq!(suggest_username("2Pac Shakur"), "pac");
        assert_eq!(suggest_username("42 Zoë"), "zoe");
        assert_eq!(suggest_username("  "), "");
    }

    #[test]
    fn the_username_follows_the_name_until_the_user_types_one() {
        let mut state = State::new(Ok(Vec::new()));
        state.update(Message::FullName("Inaya".into()));
        assert_eq!(state.answer().username, "inaya");
        state.update(Message::Username("iy".into()));
        state.update(Message::FullName("Inaya Yousfi".into()));
        assert_eq!(state.answer().username, "iy");
    }

    fn zone(name: &str, country: &str) -> TimeZone {
        TimeZone {
            name: name.into(),
            countries: vec![country.into()],
        }
    }

    #[test]
    fn the_time_zone_starts_at_the_keyboard_country_and_the_search_picks_its_first_match() {
        let mut state = State::new(Ok(vec![
            zone("America/New_York", "US"),
            zone("Europe/Paris", "FR"),
            TimeZone {
                name: "UTC".into(),
                countries: Vec::new(),
            },
        ]));
        assert_eq!(state.timezone(), Some("UTC"));
        state.set_country(Some("FR"));
        assert_eq!(state.timezone(), Some("Europe/Paris"));
        state.update(Message::Search("new york".into()));
        assert_eq!(state.timezone(), Some("America/New_York"));
        state.set_country(Some("FR"));
        assert_eq!(
            state.timezone(),
            Some("America/New_York"),
            "a chosen zone stays when the keyboard changes"
        );
        state.key(Key::Down);
        assert_eq!(
            state.timezone(),
            Some("America/New_York"),
            "the search shows one zone"
        );
    }
}

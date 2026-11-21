//! The keyboard and language screen: a layout list, a language list and a field to test the
//! layout. Choosing a layout applies it to the live session at once.
use iced::{
    Element, Length,
    widget::{column, row, text, text_input},
};
use inayos_install_core::catalog::Keyboard;

use super::{Effect, Field, Focus, Key, Reply, list};
use crate::theme;

/// The layout and language chosen until the user picks others.
pub const DEFAULT_LAYOUT: &str = "us";
pub const DEFAULT_LANGUAGE: &str = "en_US.UTF-8";

/// The language of the layouts whose code is not the language code of their country.
const LANGUAGE_OF_LAYOUT: [(&str, &str); 22] = [
    ("gb", "en_GB.UTF-8"),
    ("ie", "en_IE.UTF-8"),
    ("au", "en_AU.UTF-8"),
    ("ca", "en_CA.UTF-8"),
    ("ch", "de_CH.UTF-8"),
    ("at", "de_AT.UTF-8"),
    ("be", "fr_BE.UTF-8"),
    ("br", "pt_BR.UTF-8"),
    ("dk", "da_DK.UTF-8"),
    ("se", "sv_SE.UTF-8"),
    ("cz", "cs_CZ.UTF-8"),
    ("ee", "et_EE.UTF-8"),
    ("si", "sl_SI.UTF-8"),
    ("gr", "el_GR.UTF-8"),
    ("ua", "uk_UA.UTF-8"),
    ("by", "be_BY.UTF-8"),
    ("il", "he_IL.UTF-8"),
    ("jp", "ja_JP.UTF-8"),
    ("kr", "ko_KR.UTF-8"),
    ("cn", "zh_CN.UTF-8"),
    ("tw", "zh_TW.UTF-8"),
    ("vn", "vi_VN.UTF-8"),
];

const LAYOUTS: &str = "layouts";
const LANGUAGES: &str = "languages";
pub const TEST: &str = "keyboard-test";
const FIELDS: [Field; 3] = [
    Field::List(LAYOUTS),
    Field::List(LANGUAGES),
    Field::Input(TEST),
];

pub struct State {
    keyboards: Result<Vec<Keyboard>, String>,
    layout: Option<usize>,
    locales: Result<Vec<String>, String>,
    language: Option<usize>,
    /// Whether the user chose the language. Until then, the language follows the layout's
    /// country.
    language_chosen: bool,
    test: String,
    focus: Focus,
}

#[derive(Clone, Debug)]
pub enum Message {
    Layout(usize),
    Language(usize),
    Test(String),
}

impl State {
    pub fn new(
        keyboards: Result<Vec<Keyboard>, String>,
        locales: Result<Vec<String>, String>,
    ) -> Self {
        let layout = keyboards
            .as_ref()
            .ok()
            .and_then(|all| all.iter().position(|k| k.layout == DEFAULT_LAYOUT));
        let language = locales
            .as_ref()
            .ok()
            .and_then(|all| all.iter().position(|l| l == DEFAULT_LANGUAGE));
        State {
            keyboards,
            layout,
            locales,
            language,
            language_chosen: false,
            test: String::new(),
            focus: Focus::default(),
        }
    }

    /// The chosen layout, None when the layout list could not be read.
    pub fn layout(&self) -> Option<&Keyboard> {
        self.keyboards.as_ref().ok()?.get(self.layout?)
    }

    pub fn language(&self) -> Option<&str> {
        self.locales
            .as_ref()
            .ok()?
            .get(self.language?)
            .map(String::as_str)
    }

    /// The ISO 3166 code of the chosen layout's country, which the time zone list starts at.
    pub fn country(&self) -> Option<&str> {
        self.layout()?.country.as_deref()
    }

    pub fn ready(&self) -> bool {
        self.language().is_some()
    }

    fn choose_layout(&mut self, index: usize) -> Option<Effect> {
        self.layout = Some(index);
        if !self.language_chosen
            && let Some(at) = self.language_of_country()
        {
            self.language = Some(at);
        }
        Some(Effect::ApplyLayout(self.layout()?.layout.clone()))
    }

    /// The language that goes with the layout: the one written like the layout and its
    /// country, such as `fr_FR` for `fr`, or the one `LANGUAGE_OF_LAYOUT` names. None keeps
    /// the language as it is, because a country can have several languages and the first
    /// one listed is often not the main one, such as `cy_GB` for the UK.
    fn language_of_country(&self) -> Option<usize> {
        let keyboard = self.layout()?;
        let locales = self.locales.as_ref().ok()?;
        let exact = keyboard
            .country
            .as_deref()
            .map(|country| format!("{}_{country}.UTF-8", keyboard.layout));
        let exact = exact.and_then(|exact| locales.iter().position(|locale| *locale == exact));
        exact.or_else(|| {
            let (_, language) = LANGUAGE_OF_LAYOUT
                .iter()
                .find(|(layout, _)| *layout == keyboard.layout)?;
            locales.iter().position(|locale| locale == language)
        })
    }

    pub fn update(&mut self, message: Message) -> Reply<Message> {
        match message {
            Message::Layout(index) => {
                self.focus.set(&FIELDS, Field::List(LAYOUTS));
                match self.choose_layout(index) {
                    Some(effect) => Reply::effect(effect),
                    None => Reply::none(),
                }
            }
            Message::Language(index) => {
                self.focus.set(&FIELDS, Field::List(LANGUAGES));
                self.language = Some(index);
                self.language_chosen = true;
                Reply::none()
            }
            Message::Test(value) => {
                self.focus.set(&FIELDS, Field::Input(TEST));
                self.test = value;
                Reply::none()
            }
        }
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
        match self.focus.field(&FIELDS) {
            Some(Field::List(LAYOUTS)) => {
                let count = self.keyboards.as_ref().map_or(0, Vec::len);
                if let Some(index) = list::moved(self.layout, count, by) {
                    let effect = self.choose_layout(index);
                    return Reply {
                        task: list::reveal(LAYOUTS, index),
                        effect,
                    };
                }
            }
            Some(Field::List(LANGUAGES)) => {
                let count = self.locales.as_ref().map_or(0, Vec::len);
                if let Some(index) = list::moved(self.language, count, by) {
                    self.language = Some(index);
                    self.language_chosen = true;
                    return Reply::task(list::reveal(LANGUAGES, index));
                }
            }
            _ => {}
        }
        Reply::none()
    }

    /// Scrolls both lists to their selection, for when the screen opens.
    pub fn reveal(&self) -> iced::Task<Message> {
        iced::Task::batch([
            list::reveal(LAYOUTS, self.layout.unwrap_or(0)),
            list::reveal(LANGUAGES, self.language.unwrap_or(0)),
        ])
    }

    pub fn view(&self) -> Element<'_, Message> {
        let focused = self.focus.field(&FIELDS);
        let layouts: Element<'_, Message> = match &self.keyboards {
            Ok(keyboards) => list::view(
                LAYOUTS,
                keyboards
                    .iter()
                    .map(|k| list::Row::new(format!("{} ({})", k.name, k.layout)))
                    .collect(),
                self.layout,
                focused == Some(Field::List(LAYOUTS)),
                Length::Fill,
                Message::Layout,
            ),
            Err(reason) => text(format!(
                "The layouts cannot be read: {reason}. The keyboard stays US English."
            ))
            .color(theme::DANGER)
            .into(),
        };
        let languages: Element<'_, Message> = match &self.locales {
            Ok(locales) => list::view(
                LANGUAGES,
                locales.iter().map(list::Row::new).collect(),
                self.language,
                focused == Some(Field::List(LANGUAGES)),
                Length::Fill,
                Message::Language,
            ),
            Err(reason) => text(format!("The languages cannot be read: {reason}"))
                .color(theme::DANGER)
                .into(),
        };
        let heading = |label| text(label).size(theme::TEXT + 4).color(theme::PINK);
        column![
            row![
                column![heading("Keyboard layout"), layouts]
                    .spacing(12)
                    .width(Length::FillPortion(3)),
                column![heading("Language"), languages]
                    .spacing(12)
                    .width(Length::FillPortion(2)),
            ]
            .spacing(24)
            .height(Length::Fill),
            heading("Test the keyboard here"),
            text_input("Type a few letters", &self.test)
                .id(TEST)
                .on_input(Message::Test)
                .padding(theme::PAD)
                .size(theme::TEXT)
                .style(theme::input),
        ]
        .spacing(12)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use inayos_install_core::catalog::Keyboard;

    use super::{Effect, Message, State};

    fn keyboard(layout: &str, name: &str) -> Keyboard {
        Keyboard {
            layout: layout.into(),
            name: name.into(),
            country: (layout.len() == 2).then(|| layout.to_uppercase()),
        }
    }

    fn state() -> State {
        State::new(
            Ok(vec![
                keyboard("us", "English (US)"),
                keyboard("fr", "French"),
            ]),
            Ok(vec![
                "en_US.UTF-8".into(),
                "br_FR.UTF-8".into(),
                "fr_FR.UTF-8".into(),
            ]),
        )
    }

    #[test]
    fn a_chosen_layout_applies_at_once_and_brings_the_language_of_its_country() {
        let mut state = state();
        assert_eq!(state.language(), Some("en_US.UTF-8"));
        let reply = state.update(Message::Layout(1));
        assert_eq!(reply.effect, Some(Effect::ApplyLayout("fr".into())));
        assert_eq!(state.language(), Some("fr_FR.UTF-8"));
        assert_eq!(state.country(), Some("FR"));
    }

    #[test]
    fn a_layout_brings_the_main_language_of_its_country_not_the_first_one_listed() {
        // In the order of glibc's SUPPORTED file, which lists Welsh before English.
        let mut state = State::new(
            Ok(vec![
                keyboard("us", "English (US)"),
                keyboard("gb", "English (UK)"),
                keyboard("es", "Spanish"),
            ]),
            Ok(vec![
                "an_ES.UTF-8".into(),
                "cy_GB.UTF-8".into(),
                "en_GB.UTF-8".into(),
                "en_US.UTF-8".into(),
                "gd_GB.UTF-8".into(),
            ]),
        );
        state.update(Message::Layout(1));
        assert_eq!(state.language(), Some("en_GB.UTF-8"));
        state.update(Message::Layout(2));
        assert_eq!(
            state.language(),
            Some("en_GB.UTF-8"),
            "without es_ES the language stays as it was"
        );
    }

    #[test]
    fn a_language_the_user_chose_stays_when_the_layout_changes() {
        let mut state = state();
        state.update(Message::Language(0));
        state.update(Message::Layout(1));
        assert_eq!(state.language(), Some("en_US.UTF-8"));
    }
}

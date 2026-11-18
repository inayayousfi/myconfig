//! The installer's state machine: which screen shows, what Next, Back, Enter and the other
//! keys do, the answers the screens produce, and the changes to the live system that the
//! screens ask for, carried out through the core's `Host` on threads.
use std::{path::PathBuf, sync::Arc, thread, time::Duration};

use iced::{
    Element, Length, Subscription, Task,
    event::{self, Event, Status},
    futures::channel::oneshot,
    keyboard::{self, key::Named},
    widget::{button, column, container, row, text},
    window,
};
use inayos_install_core::{
    Answers, Host, Settings, catalog, inventory,
    modules::{self, ModuleEntry},
};

use crate::{
    live, questions,
    running::{Ended, Running},
    screens::{
        Effect, Key, Reply, Screen, disk, keyboard as keyboard_screen, modules as modules_screen,
        progress, security, summary, user, wifi,
    },
    theme,
};

/// What the app is started with.
#[derive(Clone, Debug)]
pub struct Options {
    /// Where to write the answers instead of installing, when set.
    pub save_answers: Option<PathBuf>,
    pub system_binary: PathBuf,
    pub log: PathBuf,
    /// The root of the files the catalogs read: `/` on the live system.
    pub root: PathBuf,
    /// Whether the app runs in a Sway session, where a chosen layout applies at once.
    pub in_sway: bool,
}

/// Everything the screens show that comes from the machine, read once at start.
pub struct Loaded {
    pub keyboards: Result<Vec<catalog::Keyboard>, String>,
    pub locales: Result<Vec<String>, String>,
    pub zones: Result<Vec<catalog::TimeZone>, String>,
    pub disks: Result<Vec<inventory::DiskInfo>, String>,
    pub modules: Result<Vec<ModuleEntry>, String>,
    pub tpm2: bool,
    /// Whether the live system already has a connection, which skips the Wi-Fi screen.
    pub connected: bool,
}

impl Loaded {
    pub fn read(host: &dyn Host, options: &Options) -> Self {
        let text = |error: inayos_install_core::Error| error.to_string();
        Loaded {
            keyboards: catalog::keyboards(&options.root).map_err(text),
            locales: catalog::locales(&options.root).map_err(text),
            zones: catalog::time_zones(&options.root).map_err(text),
            disks: inventory::disks(host).map_err(text),
            modules: modules::list(host, &options.system_binary).map_err(text),
            tpm2: inventory::has_tpm2(host),
            connected: live::connected(host),
        }
    }
}

pub struct App {
    options: Options,
    host: Arc<dyn Host + Send + Sync>,
    screen: Screen,
    with_wifi: bool,
    keyboard: keyboard_screen::State,
    wifi: wifi::State,
    disk: disk::State,
    security: security::State,
    user: user::State,
    modules: modules_screen::State,
    summary: summary::State,
    progress: progress::State,
    /// The checked answers, made when the summary opens.
    answers: Result<Answers, String>,
    /// Why the last action did not work, shown under the buttons until the next action.
    problem: Option<String>,
    running: Option<Running>,
    /// The screen name last written to the screen file.
    written: Option<Screen>,
    /// The layout the keyboard screen asks for, and the one `swaymsg` applies now. One
    /// `swaymsg` runs at a time, and when it ends the app applies the layout asked for
    /// since, so the session always ends on the chosen layout.
    layout_wanted: Option<String>,
    layout_applying: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Keyboard(keyboard_screen::Message),
    Wifi(wifi::Message),
    Disk(disk::Message),
    Security(security::Message),
    User(user::Message),
    Modules(modules_screen::Message),
    Summary(summary::Message),
    Next,
    Back,
    /// Enter: the screen's main action.
    Enter,
    Key(Key),
    /// A change to the live system ended; `Err` says why it failed.
    Applied(Result<(), String>),
    /// `swaymsg` ended for this layout.
    LayoutApplied(String, Result<(), String>),
    /// F5: looks for Wi-Fi networks again.
    Rescan,
    Tick,
    Reboot,
    /// A thread ended without an answer.
    Nothing,
}

impl App {
    pub fn new(options: Options, host: Arc<dyn Host + Send + Sync>, loaded: Loaded) -> Self {
        let mut app = App {
            with_wifi: !loaded.connected,
            keyboard: keyboard_screen::State::new(loaded.keyboards, loaded.locales),
            wifi: wifi::State::new(),
            disk: disk::State::new(loaded.disks),
            security: security::State::new(loaded.tpm2),
            user: user::State::new(loaded.zones),
            modules: modules_screen::State::new(loaded.modules),
            summary: summary::State::default(),
            progress: progress::State::default(),
            answers: Err("the summary is not open yet".to_owned()),
            problem: None,
            running: None,
            written: None,
            layout_wanted: None,
            layout_applying: None,
            screen: Screen::Keyboard,
            options,
            host,
        };
        app.write_screen();
        app
    }

    /// Reads the machine and opens the first screen.
    pub fn boot(options: Options, host: Arc<dyn Host + Send + Sync>) -> (Self, Task<Message>) {
        let loaded = Loaded::read(host.as_ref(), &options);
        let app = App::new(options, host, loaded);
        let task = app.keyboard.reveal().map(Message::Keyboard);
        (app, task)
    }

    /// The answers the screens hold now, unchecked. The console keymap comes from the
    /// layout, `us` without one.
    pub fn collect(&self) -> Option<Answers> {
        let layout = self
            .keyboard
            .layout()
            .map(|keyboard| keyboard.layout.clone());
        let keymap = layout.as_deref().map_or_else(
            || "us".to_owned(),
            |layout| catalog::console_keymap(&self.options.root, layout),
        );
        Some(Answers {
            keyboard: keymap,
            xkb_layout: layout,
            language: self.keyboard.language()?.to_owned(),
            timezone: self.user.timezone().unwrap_or("UTC").to_owned(),
            hostname: self.user.hostname().to_owned(),
            wifi: self.wifi.answer(),
            disk: self.disk.answer()?,
            security: self.security.answer(),
            user: self.user.answer(),
            skip: self.modules.skip(),
            questions: questions::defaults(),
        })
    }

    /// The answers, checked by the core as an answers file would be.
    fn checked(&self) -> Result<Answers, String> {
        let answers = self.collect().ok_or("an answer is missing")?;
        answers.check().map_err(|error| error.to_string())?;
        Ok(answers)
    }

    /// Why Next is disabled on the current screen, or None.
    fn blocked(&self) -> Option<String> {
        match self.screen {
            Screen::Keyboard if !self.keyboard.ready() => Some("Choose a language.".into()),
            Screen::Disk if self.disk.answer().is_none() => {
                Some("Choose a disk and something to do with it.".into())
            }
            Screen::User => self.user.missing(),
            Screen::Modules if !self.modules.ready() => {
                Some("The module list is needed to go on.".into())
            }
            Screen::Summary if self.answers.is_err() => Some("Go back and fix the answers.".into()),
            Screen::Summary if !self.summary.confirmed() => {
                Some(format!("Type {} to enable Install.", summary::CONFIRMATION))
            }
            _ => None,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.write_screen();
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        if !matches!(
            message,
            Message::Tick | Message::Nothing | Message::Applied(_) | Message::LayoutApplied(..)
        ) {
            self.problem = None;
        }
        match message {
            Message::Keyboard(message) => {
                let reply = self.keyboard.update(message);
                self.reply(reply, Message::Keyboard)
            }
            Message::Wifi(message) => {
                let reply = self.wifi.update(message);
                self.reply(reply, Message::Wifi)
            }
            Message::Disk(message) => {
                let reply = self.disk.update(message);
                self.reply(reply, Message::Disk)
            }
            Message::Security(never) => match never {},
            Message::User(message) => {
                let reply = self.user.update(message);
                self.reply(reply, Message::User)
            }
            Message::Modules(message) => {
                let reply = self.modules.update(message);
                self.reply(reply, Message::Modules)
            }
            Message::Summary(message) => {
                self.summary.update(message);
                Task::none()
            }
            Message::Next => self.next(),
            Message::Back => self.back(),
            Message::Enter => self.enter(),
            Message::Key(key) => self.key(key),
            Message::Applied(Ok(())) | Message::Nothing => Task::none(),
            Message::LayoutApplied(layout, result) => {
                if let Err(reason) = result {
                    self.problem = Some(reason);
                }
                self.layout_applying = None;
                if self.layout_wanted.as_deref() == Some(layout.as_str()) {
                    Task::none()
                } else {
                    self.apply_layout()
                }
            }
            Message::Rescan if self.screen == Screen::Wifi => {
                let reply = self.wifi.update(wifi::Message::Rescan);
                self.reply(reply, Message::Wifi)
            }
            Message::Rescan => Task::none(),
            Message::Applied(Err(reason)) => {
                self.problem = Some(reason);
                Task::none()
            }
            Message::Tick => self.tick(),
            Message::Reboot => {
                let host = self.host.clone();
                blocking(move || live::reboot(host.as_ref()), Message::Applied)
            }
        }
    }

    /// Maps a screen's reply and carries out the change it asks for.
    fn reply<M: Send + 'static>(
        &mut self,
        reply: Reply<M>,
        wrap: fn(M) -> Message,
    ) -> Task<Message> {
        let task = reply.task.map(wrap);
        match reply.effect {
            Some(effect) => Task::batch([task, self.carry_out(effect)]),
            None => task,
        }
    }

    fn carry_out(&mut self, effect: Effect) -> Task<Message> {
        let host = self.host.clone();
        match effect {
            Effect::ApplyLayout(layout) => {
                self.layout_wanted = Some(layout);
                self.apply_layout()
            }
            Effect::ScanWifi => blocking(
                move || {
                    let networks = live::networks(host.as_ref());
                    let online = live::connected(host.as_ref());
                    (networks, online)
                },
                |(networks, online)| Message::Wifi(wifi::Message::Scanned { networks, online }),
            ),
            Effect::ConnectWifi { ssid, password } => blocking(
                move || live::connect(host.as_ref(), &ssid, password.as_deref()),
                |result| Message::Wifi(wifi::Message::Joined(result)),
            ),
        }
    }

    /// Starts `swaymsg` for the wanted layout, unless one runs: its end starts the next.
    fn apply_layout(&mut self) -> Task<Message> {
        if self.layout_applying.is_some() {
            return Task::none();
        }
        let Some(layout) = self.layout_wanted.clone() else {
            return Task::none();
        };
        self.layout_applying = Some(layout.clone());
        let host = self.host.clone();
        let in_sway = self.options.in_sway;
        blocking(
            move || {
                let result = live::apply_layout(host.as_ref(), &layout, in_sway);
                (layout, result)
            },
            |(layout, result)| Message::LayoutApplied(layout, result),
        )
    }

    fn next(&mut self) -> Task<Message> {
        if let Some(reason) = self.blocked() {
            self.problem = Some(reason);
            return Task::none();
        }
        match self.screen {
            Screen::Keyboard => {
                self.user.set_country(self.keyboard.country());
                self.user
                    .set_layout(self.keyboard.layout().map(|k| k.layout.as_str()));
            }
            Screen::User => {
                // The core checks every answer; a refused username or computer name shows
                // here, next to its field.
                if let Err(reason) = self.checked() {
                    self.problem = Some(reason);
                    return Task::none();
                }
            }
            Screen::Summary => return self.install(),
            _ => {}
        }
        match self.screen.next(self.with_wifi) {
            Some(next) => self.open(next),
            None => Task::none(),
        }
    }

    fn back(&mut self) -> Task<Message> {
        match self.screen.previous(self.with_wifi) {
            Some(previous) => self.open(previous),
            None => Task::none(),
        }
    }

    /// Shows `screen` and runs what it needs when it opens.
    fn open(&mut self, screen: Screen) -> Task<Message> {
        self.screen = screen;
        let unfocus = iced::widget::operation::focus(iced::widget::Id::new("no-input"));
        match screen {
            Screen::Keyboard => self.keyboard.reveal().map(Message::Keyboard),
            Screen::Wifi => {
                let effect = self.wifi.open();
                Task::batch([unfocus, self.carry_out(effect)])
            }
            Screen::User => self.user.open().map(Message::User),
            Screen::Summary => {
                self.answers = self.checked();
                self.summary.open().map(Message::Summary)
            }
            _ => unfocus,
        }
    }

    fn enter(&mut self) -> Task<Message> {
        match self.screen {
            Screen::Wifi if self.wifi.enter_connects() => {
                let reply = self.wifi.update(wifi::Message::Connect);
                self.reply(reply, Message::Wifi)
            }
            Screen::Progress | Screen::Failed => Task::none(),
            Screen::Done => self.handle(Message::Reboot),
            _ => self.next(),
        }
    }

    fn key(&mut self, key: Key) -> Task<Message> {
        match self.screen {
            Screen::Keyboard => {
                let reply = self.keyboard.key(key);
                self.reply(reply, Message::Keyboard)
            }
            Screen::Wifi => {
                let reply = self.wifi.key(key);
                self.reply(reply, Message::Wifi)
            }
            Screen::Disk => {
                let reply = self.disk.key(key);
                self.reply(reply, Message::Disk)
            }
            Screen::User => {
                let reply = self.user.key(key);
                self.reply(reply, Message::User)
            }
            Screen::Modules => {
                let reply = self.modules.key(key);
                self.reply(reply, Message::Modules)
            }
            Screen::Summary if matches!(key, Key::Next | Key::Previous) => {
                iced::widget::operation::focus(summary::CONFIRM)
            }
            _ => Task::none(),
        }
    }

    /// Starts the install, or writes the answers and quits when the app only saves them.
    fn install(&mut self) -> Task<Message> {
        let answers = match &self.answers {
            Ok(answers) => answers.clone(),
            Err(reason) => {
                self.problem = Some(reason.clone());
                return Task::none();
            }
        };
        if let Some(path) = &self.options.save_answers {
            return match answers.write(path) {
                Ok(()) => iced::exit(),
                Err(error) => {
                    self.problem = Some(error.to_string());
                    Task::none()
                }
            };
        }
        let settings = Settings {
            system_binary: self.options.system_binary.clone(),
            log_file: Some(self.options.log.clone()),
        };
        match Running::start(answers, settings, self.host.clone(), &self.options.log) {
            Ok(running) => {
                self.running = Some(running);
                self.screen = Screen::Progress;
            }
            Err(reason) => self.problem = Some(reason),
        }
        Task::none()
    }

    fn tick(&mut self) -> Task<Message> {
        let Some(running) = &mut self.running else {
            return Task::none();
        };
        let (lines, ended) = running.poll();
        let task = self.progress.receive(lines);
        match ended {
            None => {}
            Some(Ended::Succeeded) => {
                self.running = None;
                self.screen = Screen::Done;
            }
            Some(Ended::Failed(message)) => {
                self.running = None;
                self.progress.fail(message);
                self.screen = Screen::Failed;
            }
        }
        task
    }

    /// Writes the screen name for the VM tests when the screen changed. Outside the live
    /// system the folder may not be writable; the screens work the same without the file.
    fn write_screen(&mut self) {
        if self.written == Some(self.screen) {
            return;
        }
        self.written = Some(self.screen);
        if let Err(reason) = live::write_screen(self.host.as_ref(), self.screen.name()) {
            eprintln!("{reason}");
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let keys = event::listen_with(key_message);
        if self.running.is_some() {
            Subscription::batch([
                keys,
                iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick),
            ])
        } else {
            keys
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match self.screen {
            Screen::Keyboard => self.keyboard.view().map(Message::Keyboard),
            Screen::Wifi => self.wifi.view().map(Message::Wifi),
            Screen::Disk => self.disk.view().map(Message::Disk),
            Screen::Security => self.security.view().map(Message::Security),
            Screen::User => self.user.view().map(Message::User),
            Screen::Modules => self.modules.view().map(Message::Modules),
            Screen::Summary => self
                .summary
                .view(&self.answers, self.disk.disk())
                .map(Message::Summary),
            Screen::Progress => self.progress.running_view(),
            Screen::Done => self.progress.done_view(),
            Screen::Failed => self
                .progress
                .failed_view(&self.options.log.display().to_string()),
        };
        let mut page = column![self.header()].spacing(16);
        if let Some(problem) = &self.problem {
            page = page.push(text(problem).color(theme::DANGER).size(theme::TEXT));
        }
        page = page.push(container(body).height(Length::Fill));
        container(page)
            .padding(24)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// The title and the Back and Next buttons. They sit at the top, so the on-screen
    /// keyboard, a layer-shell overlay at the bottom of the screen, never covers them.
    fn header(&self) -> Element<'_, Message> {
        let big = |label: String| text(label).size(theme::TEXT + 2);
        let mut bar = row![
            text(self.screen.title())
                .size(theme::TITLE)
                .color(theme::PINK)
                .width(Length::Fill)
        ]
        .spacing(16)
        .align_y(iced::Alignment::Center);
        if self.screen.previous(self.with_wifi).is_some() {
            bar = bar.push(
                button(big("Back".into()))
                    .padding(theme::PAD)
                    .style(theme::secondary)
                    .on_press(Message::Back),
            );
        }
        let main = match self.screen {
            Screen::Summary if self.options.save_answers.is_some() => Some("Save the answers"),
            Screen::Summary => Some("Install"),
            Screen::Progress | Screen::Failed => None,
            Screen::Done => Some("Reboot"),
            _ => Some("Next"),
        };
        if let Some(label) = main {
            let action = if self.screen == Screen::Done {
                Message::Reboot
            } else {
                Message::Next
            };
            let enabled = self.screen == Screen::Done || self.blocked().is_none();
            bar = bar.push(
                button(big(label.into()))
                    .padding(theme::PAD)
                    .style(theme::primary)
                    .on_press_maybe(enabled.then_some(action)),
            );
        }
        let mut header = column![bar].spacing(6);
        if let Some(reason) = self.blocked()
            && main.is_some()
            && self.problem.is_none()
        {
            header = header.push(text(reason).color(theme::DIM));
        }
        header.into()
    }
}

/// The app's keys: Escape goes back and Tab moves between fields wherever the focus is.
/// Enter, the arrows and Space act only when no text field took them.
fn key_message(event: Event, status: Status, _: window::Id) -> Option<Message> {
    let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };
    let keyboard::Key::Named(named) = key else {
        return None;
    };
    match named {
        Named::Escape => Some(Message::Back),
        Named::F5 => Some(Message::Rescan),
        Named::Tab if modifiers.shift() => Some(Message::Key(Key::Previous)),
        Named::Tab => Some(Message::Key(Key::Next)),
        _ if status == Status::Captured => None,
        Named::Enter => Some(Message::Enter),
        Named::ArrowUp => Some(Message::Key(Key::Up)),
        Named::ArrowDown => Some(Message::Key(Key::Down)),
        Named::Space => Some(Message::Key(Key::Space)),
        _ => None,
    }
}

/// Runs `work` on its own thread, so the window keeps drawing, and sends `done` with its
/// result.
fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) -> Message + Send + 'static,
) -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    Task::perform(receiver, move |result| match result {
        Ok(value) => done(value),
        Err(_) => Message::Nothing,
    })
}

/// Runs the screens. With `options.save_answers`, the summary writes the answers file and
/// the app quits instead of installing.
pub fn run(options: Options, host: Arc<dyn Host + Send + Sync>) -> iced::Result {
    let window = window::Settings {
        platform_specific: window::settings::PlatformSpecific {
            // The Sway session of the live system puts this app id on the installer
            // workspace.
            application_id: "org.inayos.installer".to_owned(),
            ..Default::default()
        },
        ..window::Settings::default()
    };
    iced::application(
        move || App::boot(options.clone(), host.clone()),
        App::update,
        App::view,
    )
    .title("InayOS installer")
    .theme(|_: &App| theme::theme())
    .subscription(App::subscription)
    .window(window)
    .default_font(theme::FONT)
    .settings(iced::Settings {
        default_text_size: theme::TEXT.into(),
        antialiasing: true,
        ..iced::Settings::default()
    })
    .run()
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    use inayos_install_core::{
        Answers, Disk, Host, Sink,
        catalog::{Keyboard, TimeZone},
        host::{Command, Output},
        modules::ModuleEntry,
    };

    use super::{App, Loaded, Message, Options};
    use crate::{
        questions,
        screens::{Screen, disk::tests::disk, keyboard, modules, summary, user},
    };

    /// A machine whose only effect is the screen file, whose names it records.
    #[derive(Default)]
    struct ScreenFile {
        names: Mutex<Vec<String>>,
    }

    impl Host for ScreenFile {
        fn run(&self, _: &Command, _: &dyn Sink) -> io::Result<Output> {
            Err(io::Error::other("no command runs in these tests"))
        }
        fn effective_uid(&self) -> io::Result<u32> {
            Ok(0)
        }
        fn exists(&self, _: &Path) -> bool {
            false
        }
        fn is_dir(&self, _: &Path) -> bool {
            false
        }
        fn read(&self, _: &Path) -> io::Result<String> {
            Err(io::ErrorKind::NotFound.into())
        }
        fn write(&self, _: &Path, contents: &str, _: u32) -> io::Result<()> {
            self.names.lock().unwrap().push(contents.trim().to_owned());
            Ok(())
        }
        fn copy(&self, _: &Path, _: &Path, _: u32) -> io::Result<()> {
            Ok(())
        }
        fn create_dir(&self, _: &Path, _: u32) -> io::Result<()> {
            Ok(())
        }
        fn list(&self, _: &Path) -> io::Result<Vec<String>> {
            Ok(Vec::new())
        }
        fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
            Ok(())
        }
        fn remove_file(&self, _: &Path) -> io::Result<()> {
            Ok(())
        }
        fn remove_all(&self, _: &Path) -> io::Result<()> {
            Ok(())
        }
        fn owner(&self, _: &Path) -> io::Result<(u32, u32)> {
            Ok((0, 0))
        }
        fn chown(&self, _: &Path, _: u32, _: u32) -> io::Result<()> {
            Ok(())
        }
    }

    fn loaded() -> Loaded {
        Loaded {
            keyboards: Ok(vec![
                Keyboard {
                    layout: "us".into(),
                    name: "English (US)".into(),
                    country: Some("US".into()),
                },
                Keyboard {
                    layout: "fr".into(),
                    name: "French".into(),
                    country: Some("FR".into()),
                },
            ]),
            locales: Ok(vec!["en_US.UTF-8".into(), "fr_FR.UTF-8".into()]),
            zones: Ok(vec![
                TimeZone {
                    name: "America/New_York".into(),
                    countries: vec!["US".into()],
                },
                TimeZone {
                    name: "Europe/Paris".into(),
                    countries: vec!["FR".into(), "MC".into()],
                },
                TimeZone {
                    name: "UTC".into(),
                    countries: Vec::new(),
                },
            ]),
            disks: Ok(vec![disk()]),
            modules: Ok(vec![
                ModuleEntry {
                    name: "base".into(),
                    hard_needs: Vec::new(),
                    locked: true,
                },
                ModuleEntry {
                    name: "media".into(),
                    hard_needs: vec!["base".into()],
                    locked: false,
                },
            ]),
            tpm2: false,
            connected: true,
        }
    }

    #[test]
    fn enter_walks_every_screen_and_the_summary_saves_the_answers_it_shows() {
        let folder =
            std::env::temp_dir().join(format!("inayos-installer-app-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let saved = folder.join("answers.toml");
        let host = Arc::new(ScreenFile::default());
        let options = Options {
            save_answers: Some(saved.clone()),
            system_binary: PathBuf::from("/usr/local/lib/inayos/inayos"),
            log: folder.join("install.log"),
            root: folder.clone(),
            in_sway: false,
        };
        let mut app = App::new(options, host.clone(), loaded());

        let _ = app.update(Message::Keyboard(keyboard::Message::Layout(1)));
        let _ = app.update(Message::Enter);
        assert_eq!(app.screen, Screen::Disk, "a connected machine skips Wi-Fi");
        let _ = app.update(Message::Enter);
        let _ = app.update(Message::Enter);
        assert_eq!(app.screen, Screen::User);
        let _ = app.update(Message::Enter);
        assert_eq!(
            app.screen,
            Screen::User,
            "Next waits for the user's answers"
        );
        for message in [
            user::Message::FullName("Inaya Yousfi".into()),
            user::Message::Password("secret pass".into()),
            user::Message::PasswordAgain("secret pass".into()),
        ] {
            let _ = app.update(Message::User(message));
        }
        let _ = app.update(Message::Enter);
        let _ = app.update(Message::Modules(modules::Message::Toggle(1)));
        let _ = app.update(Message::Enter);
        assert_eq!(app.screen, Screen::Summary);
        let _ = app.update(Message::Enter);
        assert!(!saved.exists(), "Install waits for the confirmation word");
        let _ = app.update(Message::Summary(summary::Message::Typed("erase".into())));
        let _ = app.update(Message::Enter);

        let expected = app.collect().unwrap();
        assert_eq!(Answers::read(&saved).unwrap(), expected);
        assert_eq!(expected.xkb_layout.as_deref(), Some("fr"));
        assert_eq!(expected.language, "fr_FR.UTF-8");
        assert_eq!(expected.timezone, "Europe/Paris");
        assert_eq!(expected.user.username, "inaya");
        assert_eq!(expected.hostname, "inayos");
        assert_eq!(
            expected.disk,
            Disk::ReinstallKeepHome {
                device: "/dev/vda".into(),
                efi_partition: "/dev/vda1".into(),
                system_partition: "/dev/vda2".into(),
                home_partition: "/dev/vda3".into(),
            },
            "Enter keeps the home partition of the disk's earlier InayOS"
        );
        assert_eq!(expected.skip, ["media"]);
        assert_eq!(expected.questions, questions::defaults());
        assert_eq!(
            *host.names.lock().unwrap(),
            ["keyboard", "disk", "security", "user", "modules", "summary"]
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_name_the_core_refuses_keeps_the_user_screen_with_the_reason() {
        let host = Arc::new(ScreenFile::default());
        let options = Options {
            save_answers: None,
            system_binary: PathBuf::from("/usr/local/lib/inayos/inayos"),
            log: PathBuf::from("/nonexistent/install.log"),
            root: PathBuf::from("/nonexistent"),
            in_sway: false,
        };
        let mut app = App::new(options, host, loaded());
        for _ in 0..3 {
            let _ = app.update(Message::Next);
        }
        for message in [
            user::Message::FullName("Root".into()),
            user::Message::Password("pw".into()),
            user::Message::PasswordAgain("pw".into()),
        ] {
            let _ = app.update(Message::User(message));
        }
        let _ = app.update(Message::Next);
        assert_eq!(app.screen, Screen::User);
        assert!(
            app.problem.as_deref().is_some_and(|p| p.contains("root")),
            "{:?}",
            app.problem
        );
    }
}

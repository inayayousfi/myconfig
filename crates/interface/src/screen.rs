//! The interactive screen: the module list, a log panel, and pop-ups.
//!
//! The runner works on a separate thread. It sends output, events and questions
//! here through a channel, and waits for answers on another channel.
use std::{
    collections::VecDeque,
    io,
    process::{Command, ExitStatus},
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use crossterm::{
    event::{self, Event as TerminalEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use myconfig_modules::{Event, Interaction, Module, ModuleResult, Step, Unanswered, runner};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{Profile, sudo::Session, with_context};

const LOG_LIMIT: usize = 5000;
const PINK: Color = Color::Rgb(255, 78, 173);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Unknown,
    Running,
    Installed,
    Broken,
    Missing,
}

impl Status {
    fn label(self) -> Span<'static> {
        match self {
            Self::Unknown => Span::raw(""),
            Self::Running => Span::styled("running", Style::new().fg(Color::Yellow)),
            Self::Installed => Span::styled("installed", Style::new().fg(Color::Green)),
            Self::Broken => Span::styled("broken", Style::new().fg(Color::Red)),
            Self::Missing => Span::styled("missing", Style::new().fg(Color::DarkGray)),
        }
    }
}

#[derive(Clone, Copy)]
enum Action {
    Install,
    Verify,
    Remove,
}

/// Messages from the runner thread to the screen.
enum Message {
    Output(String),
    Note(String),
    Event(Event),
    Status(String, Status),
    Confirm(String, Sender<bool>),
    Suspend(Sender<()>),
    Resume,
    Done(Result<String, String>),
}

/// The runner thread's side of the screen.
struct Remote {
    messages: Sender<Message>,
}

impl Interaction for Remote {
    fn output(&self, line: &str) {
        let _ = self.messages.send(Message::Output(line.to_owned()));
    }

    fn note(&self, message: &str) {
        let _ = self.messages.send(Message::Note(message.to_owned()));
    }

    fn confirm(&self, question: &str) -> Result<bool, Unanswered> {
        let (answer, received) = mpsc::channel();
        let unanswered = || Unanswered {
            question: question.to_owned(),
        };
        self.messages
            .send(Message::Confirm(question.to_owned(), answer))
            .map_err(|_| unanswered())?;
        received.recv().map_err(|_| unanswered())
    }

    fn has_terminal(&self) -> bool {
        true
    }

    fn with_terminal(&self, command: &mut Command) -> io::Result<ExitStatus> {
        let (ready, received) = mpsc::channel();
        self.messages
            .send(Message::Suspend(ready))
            .map_err(io::Error::other)?;
        received.recv().map_err(io::Error::other)?;
        let status = command.status();
        let _ = self.messages.send(Message::Resume);
        status
    }

    fn event(&self, event: Event) {
        let _ = self.messages.send(Message::Event(event));
    }
}

enum Popup {
    None,
    Password {
        input: String,
        error: Option<String>,
        action: Action,
    },
    Confirm {
        question: String,
        answer: Sender<bool>,
    },
}

struct App<'a> {
    profile: &'a Profile<'a>,
    checked: Vec<bool>,
    status: Vec<Status>,
    list: ListState,
    log: VecDeque<Line<'static>>,
    popup: Popup,
    running: bool,
    session: Option<Session>,
    quit: bool,
}

impl<'a> App<'a> {
    fn new(profile: &'a Profile<'a>) -> Self {
        let count = profile.modules.len();
        Self {
            profile,
            checked: vec![true; count],
            status: vec![Status::Unknown; count],
            list: ListState::default().with_selected(Some(0)),
            log: VecDeque::new(),
            popup: Popup::None,
            running: false,
            session: None,
            quit: false,
        }
    }

    fn log(&mut self, line: Line<'static>) {
        if self.log.len() == LOG_LIMIT {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    fn index(&self, module: &str) -> Option<usize> {
        self.profile
            .modules
            .iter()
            .position(|candidate| candidate.name() == module)
    }

    fn selected_modules(&self) -> Vec<&'a dyn Module> {
        self.profile
            .modules
            .iter()
            .zip(&self.checked)
            .filter(|(_, checked)| **checked)
            .map(|(module, _)| *module)
            .collect()
    }
}

pub fn run(profile: &Profile<'_>) -> ModuleResult<bool> {
    let mut terminal = ratatui::init();
    let result = std::thread::scope(|scope| event_loop(&mut terminal, profile, scope));
    ratatui::restore();
    result.map(|()| false)
}

fn event_loop<'scope>(
    terminal: &mut DefaultTerminal,
    profile: &'scope Profile<'scope>,
    scope: &'scope std::thread::Scope<'scope, '_>,
) -> ModuleResult {
    let mut app = App::new(profile);
    let (sender, messages) = mpsc::channel::<Message>();
    while !app.quit {
        terminal.draw(|frame| draw(frame, &mut app))?;
        drain(terminal, &mut app, &messages)?;
        if event::poll(Duration::from_millis(50))?
            && let TerminalEvent::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && let Some(action) = handle_key(&mut app, key)
        {
            start(&mut app, action, &sender, scope);
        }
    }
    Ok(())
}

/// Handles everything the runner thread sent since the last frame.
fn drain(
    terminal: &mut DefaultTerminal,
    app: &mut App<'_>,
    messages: &Receiver<Message>,
) -> ModuleResult {
    while let Ok(message) = messages.try_recv() {
        match message {
            Message::Output(line) => app.log(Line::raw(line)),
            Message::Note(note) => {
                app.log(Line::styled(format!("note: {note}"), Style::new().fg(PINK)))
            }
            Message::Event(Event::Started { module, step }) => {
                app.log(Line::styled(
                    format!("==> {step:?} {module}"),
                    Style::new().bold(),
                ));
                if let Some(index) = app.index(&module) {
                    app.status[index] = Status::Running;
                }
            }
            Message::Event(Event::Finished {
                module,
                step,
                error,
            }) => {
                let Some(index) = app.index(&module) else {
                    continue;
                };
                if let Some(error) = &error {
                    app.log(Line::styled(
                        format!("==> {step:?} {module} failed: {error}"),
                        Style::new().fg(Color::Red),
                    ));
                }
                app.status[index] = match (step, error.is_none()) {
                    (Step::Verify, true) => Status::Installed,
                    (Step::Verify, false) | (Step::Install, false) => Status::Broken,
                    (Step::Remove, true) => Status::Missing,
                    _ => app.status[index],
                };
            }
            Message::Status(module, status) => {
                if let Some(index) = app.index(&module) {
                    app.status[index] = status;
                }
            }
            Message::Confirm(question, answer) => app.popup = Popup::Confirm { question, answer },
            Message::Suspend(ready) => {
                disable_raw_mode()?;
                execute!(io::stdout(), LeaveAlternateScreen)?;
                let _ = ready.send(());
                // The program owns the terminal until the runner says it is done.
                while let Ok(message) = messages.recv() {
                    if matches!(message, Message::Resume) {
                        break;
                    }
                }
                enable_raw_mode()?;
                execute!(io::stdout(), EnterAlternateScreen)?;
                terminal.clear()?;
            }
            Message::Resume => {}
            Message::Done(result) => {
                app.running = false;
                match result {
                    Ok(summary) => app.log(Line::styled(summary, Style::new().fg(Color::Green))),
                    Err(error) => app.log(Line::styled(
                        format!("error: {error}"),
                        Style::new().fg(Color::Red),
                    )),
                }
            }
        }
    }
    Ok(())
}

fn handle_key(app: &mut App<'_>, key: KeyEvent) -> Option<Action> {
    match &mut app.popup {
        Popup::Confirm { answer, .. } => {
            let reply = match key.code {
                KeyCode::Char('y' | 'Y') => true,
                KeyCode::Char('n' | 'N') | KeyCode::Esc => false,
                _ => return None,
            };
            let _ = answer.send(reply);
            app.popup = Popup::None;
            None
        }
        Popup::Password {
            input,
            error,
            action,
        } => {
            match key.code {
                KeyCode::Esc => app.popup = Popup::None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Enter => {
                    let action = *action;
                    match Session::start_with_password(app.profile.package_system, input) {
                        Ok(session) => {
                            app.session = Some(session);
                            app.popup = Popup::None;
                            return Some(action);
                        }
                        Err(rejected) => {
                            *error = Some(rejected.to_string());
                            input.clear();
                        }
                    }
                }
                KeyCode::Char(character) => input.push(character),
                _ => {}
            }
            None
        }
        Popup::None => {
            let count = app.profile.modules.len();
            let selected = app.list.selected().unwrap_or(0);
            match key.code {
                KeyCode::Char('c')
                    if key.modifiers.contains(KeyModifiers::CONTROL) && !app.running =>
                {
                    app.quit = true;
                }
                KeyCode::Char('q') if !app.running => app.quit = true,
                KeyCode::Down | KeyCode::Char('j') => {
                    app.list.select(Some((selected + 1).min(count - 1)))
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    app.list.select(Some(selected.saturating_sub(1)))
                }
                KeyCode::Char(' ') if !app.running => {
                    app.checked[selected] = !app.checked[selected]
                }
                KeyCode::Char('i') if !app.running => return permission(app, Action::Install),
                KeyCode::Char('v') if !app.running => return permission(app, Action::Verify),
                KeyCode::Char('r') if !app.running => return permission(app, Action::Remove),
                _ => {}
            }
            None
        }
    }
}

/// Asks for the sudo password first when sudo has no valid permission yet.
fn permission(app: &mut App<'_>, action: Action) -> Option<Action> {
    if app.selected_modules().is_empty() {
        app.log(Line::styled("No module is checked", Style::new().fg(PINK)));
        return None;
    }
    if app.session.is_none() {
        if !Session::cached(app.profile.package_system) {
            app.popup = Popup::Password {
                input: String::new(),
                error: None,
                action,
            };
            return None;
        }
        app.session = Some(Session::start_cached(app.profile.package_system));
    }
    Some(action)
}

fn start<'scope>(
    app: &mut App<'scope>,
    action: Action,
    sender: &Sender<Message>,
    scope: &'scope std::thread::Scope<'scope, '_>,
) {
    app.running = true;
    let modules = app.selected_modules();
    let profile = app.profile;
    let messages = sender.clone();
    scope.spawn(move || {
        let remote = Remote {
            messages: messages.clone(),
        };
        let result = with_context(profile.package_system, &remote, |ctx| match action {
            Action::Install => {
                runner::install(ctx, &modules)?;
                Ok(format!("{} modules installed and verified", modules.len()))
            }
            Action::Verify => {
                let results = runner::verify(ctx, &modules);
                let failed = results.iter().filter(|(_, result)| result.is_err()).count();
                for (module, result) in &results {
                    if result.is_err()
                        && !modules.iter().any(|candidate| {
                            candidate.name() == *module && runner::installed(ctx, *candidate)
                        })
                    {
                        let _ =
                            messages.send(Message::Status((*module).to_owned(), Status::Missing));
                    }
                }
                Ok(format!("{} verified, {failed} failed", results.len()))
            }
            Action::Remove => {
                let report = runner::remove(ctx, profile.modules, &modules, false)?;
                Ok(if report.unanswered {
                    "Removed, but some questions had no answer and their items stayed".to_owned()
                } else {
                    format!("{} modules removed", modules.len())
                })
            }
        });
        let _ = messages.send(Message::Done(result.map_err(|error| error.to_string())));
    });
}

fn draw(frame: &mut Frame, app: &mut App<'_>) {
    let [modules_area, log_area, footer_area] = Layout::vertical([
        Constraint::Max(app.profile.modules.len() as u16 + 2),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let width = app
        .profile
        .modules
        .iter()
        .map(|module| module.name().len())
        .max()
        .unwrap_or(0);
    let items: Vec<ListItem> = app
        .profile
        .modules
        .iter()
        .enumerate()
        .map(|(index, module)| {
            let mark = if app.checked[index] { "[x]" } else { "[ ]" };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{mark} {:width$}  ", module.name())),
                app.status[index].label(),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::bordered()
                .title(format!(" myconfig: {} ", app.profile.title))
                .border_style(Style::new().fg(PINK)),
        )
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, modules_area, &mut app.list);

    let visible = log_area.height.saturating_sub(2) as usize;
    let start = app.log.len().saturating_sub(visible);
    let lines: Vec<Line> = app.log.iter().skip(start).cloned().collect();
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" log ")),
        log_area,
    );

    let footer = if app.running {
        " running…".to_owned()
    } else {
        " space toggle  i install  v verify  r remove  q quit".to_owned()
    };
    frame.render_widget(Paragraph::new(footer).fg(Color::Gray), footer_area);

    match &app.popup {
        Popup::None => {}
        Popup::Confirm { question, .. } => {
            popup(
                frame,
                " question ",
                vec![
                    Line::raw(question.clone()),
                    Line::raw(""),
                    Line::raw("y yes   n no"),
                ],
            );
        }
        Popup::Password { input, error, .. } => {
            let mut lines = vec![
                Line::raw("sudo password:"),
                Line::raw("*".repeat(input.chars().count())),
                Line::raw(""),
                Line::raw("Enter confirm   Esc cancel"),
            ];
            if let Some(error) = error {
                lines.insert(0, Line::styled(error.clone(), Style::new().fg(Color::Red)));
            }
            popup(frame, " permission ", lines);
        }
    }
}

fn popup(frame: &mut Frame, title: &str, lines: Vec<Line>) {
    let [area] = Layout::horizontal([Constraint::Percentage(60)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::vertical([Constraint::Length(lines.len() as u16 + 4)])
        .flex(Flex::Center)
        .areas::<1>(area);
    let area: Rect = area;
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title(title.to_owned())
                .border_style(Style::new().fg(PINK)),
        ),
        area,
    );
}

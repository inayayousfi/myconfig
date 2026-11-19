//! The install on its own thread: every line goes to the log file and over a channel to the
//! progress screen, which collects what arrived on each tick.
use std::{
    path::Path,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};

use inayos_install_core::{Answers, ChannelSink, Host, Installed, Line, Settings, Sink, install};

use crate::log_file::LogFile;

/// A running install.
pub struct Running {
    lines: Receiver<Line>,
    thread: Option<JoinHandle<Result<Installed, (String, Installed)>>>,
}

/// How the install ended, once it ended. A failure carries what the install made before
/// it, such as the recovery key.
pub enum Ended {
    Succeeded(Installed),
    Failed(String, Installed),
}

impl Running {
    /// Opens the log and starts the install. Fails when the log cannot be opened, before
    /// anything changes.
    pub fn start(
        answers: Answers,
        settings: Settings,
        host: Arc<dyn Host + Send + Sync>,
        log: &Path,
    ) -> Result<Self, String> {
        let log = LogFile::open(log)
            .map_err(|error| format!("could not open the log {}: {error}", log.display()))?;
        let (sender, lines) = mpsc::channel();
        let thread = thread::spawn(move || {
            let sink = Recorder {
                log,
                channel: ChannelSink::new(sender),
            };
            install(&answers, &settings, host.as_ref(), &sink).map_err(|failure| {
                let message = failure.to_string();
                sink.log.line(&format!("error: {message}"));
                (message, failure.installed)
            })
        });
        Ok(Running {
            lines,
            thread: Some(thread),
        })
    }

    /// The lines that arrived since the last call, and the end of the install when it ended.
    pub fn poll(&mut self) -> (Vec<Line>, Option<Ended>) {
        let lines: Vec<Line> = self.lines.try_iter().collect();
        if !self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            return (lines, None);
        }
        // The thread ended, so every line it sent is in the channel.
        let mut lines = lines;
        lines.extend(self.lines.try_iter());
        let ended = match self.thread.take().map(JoinHandle::join) {
            Some(Ok(Ok(installed))) => Ended::Succeeded(installed),
            Some(Ok(Err((message, installed)))) => Ended::Failed(message, installed),
            Some(Err(_)) => Ended::Failed(
                "the install stopped on an internal error".to_owned(),
                Installed::default(),
            ),
            None => return (lines, None),
        };
        (lines, Some(ended))
    }
}

/// Writes every line to the log file, as the command line does, and sends it to the screen.
struct Recorder {
    log: LogFile,
    channel: ChannelSink,
}

impl Sink for Recorder {
    fn step(&self, text: &str) {
        self.log.line(&format!("==> {text}"));
        self.channel.step(text);
    }

    fn command(&self, command: &str) {
        self.log.line(&format!("$ {command}"));
        self.channel.command(command);
    }

    fn output(&self, line: &str) {
        self.log.line(line);
        self.channel.output(line);
    }

    fn detail(&self, text: &str) {
        self.log.line(text);
        self.channel.detail(text);
    }
}

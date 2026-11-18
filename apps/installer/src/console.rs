//! The command line's sink: progress, commands and their output on stdout, and everything,
//! details included, in the log file.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::Mutex,
};

use inayos_install_core::Sink;

pub struct Console {
    log: Mutex<File>,
}

impl Console {
    /// Opens the log at `path` for appending, so earlier attempts stay in it.
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        let log = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Console {
            log: Mutex::new(log),
        })
    }

    /// Writes one line to the log. A log that cannot be written loses the line; the
    /// terminal still shows it.
    fn log(&self, line: &str) {
        if let Ok(mut log) = self.log.lock() {
            let _ = writeln!(log, "{line}");
        }
    }

    fn show(&self, line: &str) {
        let mut stdout = io::stdout().lock();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
        self.log(line);
    }

    pub fn error(&self, message: &str) {
        eprintln!("error: {message}");
        self.log(&format!("error: {message}"));
    }
}

impl Sink for Console {
    fn step(&self, text: &str) {
        self.show(&format!("==> {text}"));
    }

    fn command(&self, command: &str) {
        self.show(&format!("$ {command}"));
    }

    fn output(&self, line: &str) {
        self.show(line);
    }

    fn detail(&self, text: &str) {
        self.log(text);
    }
}

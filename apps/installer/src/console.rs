//! The command line's sink: progress, commands and their output on stdout, and everything,
//! details included, in the log file.
use std::{
    io::{self, Write},
    path::Path,
};

use inayos_install_core::Sink;

use crate::log_file::LogFile;

pub struct Console {
    log: LogFile,
}

impl Console {
    /// Opens the log at `path` for appending, so earlier attempts stay in it.
    pub fn open(path: &Path) -> io::Result<Self> {
        Ok(Console {
            log: LogFile::open(path)?,
        })
    }

    fn show(&self, line: &str) {
        let mut stdout = io::stdout().lock();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
        self.log.line(line);
    }

    pub fn error(&self, message: &str) {
        eprintln!("error: {message}");
        self.log.line(&format!("error: {message}"));
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
        self.log.line(text);
    }
}

//! The install log file, which both interfaces append every line to, details included.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::Mutex,
};

pub struct LogFile {
    file: Mutex<File>,
}

impl LogFile {
    /// Opens the log at `path` for appending, so earlier attempts stay in it.
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(LogFile {
            file: Mutex::new(file),
        })
    }

    /// Writes one line. A log that cannot be written loses the line; the interface still
    /// shows it.
    pub fn line(&self, line: &str) {
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{line}");
        }
    }
}

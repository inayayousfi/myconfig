//! The host and the sink together: runs commands and changes files, reports each one, and
//! turns every failure into an `Error`.
use std::path::Path;

use crate::{
    Error, Sink,
    host::{Command, Host, Mode, Output},
};

pub(crate) struct Machine<'a> {
    pub host: &'a dyn Host,
    pub sink: &'a dyn Sink,
}

fn io(action: &'static str, path: &Path) -> impl FnOnce(std::io::Error) -> Error {
    let path = path.to_owned();
    move |error| Error::Io {
        action,
        path,
        error,
    }
}

impl Machine<'_> {
    /// Runs `command` and fails unless it exits 0.
    pub fn run(&self, command: Command) -> Result<Output, Error> {
        let output = self.run_any(&command)?;
        if output.success() {
            Ok(output)
        } else {
            Err(Error::Command {
                command: command.shown(),
                status: output.status(),
                stderr: output.stderr,
            })
        }
    }

    /// Runs `command` and returns how it ended, whatever its exit code.
    pub fn run_any(&self, command: &Command) -> Result<Output, Error> {
        let shown = command.shown();
        match command.mode() {
            Mode::Stream => self.sink.command(&shown),
            Mode::Capture => self.sink.detail(&format!("$ {shown}")),
        }
        let output = self
            .host
            .run(command, self.sink)
            .map_err(|error| Error::Spawn {
                command: shown,
                error,
            })?;
        if command.mode() == Mode::Capture && !command.output_hidden() {
            for line in output.stdout.lines().chain(output.stderr.lines()) {
                self.sink.detail(line);
            }
        }
        Ok(output)
    }

    /// The standard output of a query that must exit 0.
    pub fn query(&self, command: Command) -> Result<String, Error> {
        Ok(self.run(command)?.stdout)
    }

    pub fn exists(&self, path: &Path) -> bool {
        self.host.exists(path)
    }

    pub fn is_dir(&self, path: &Path) -> bool {
        self.host.is_dir(path)
    }

    pub fn read(&self, path: &Path) -> Result<String, Error> {
        self.host.read(path).map_err(io("read", path))
    }

    pub fn write(&self, path: &Path, contents: &str, mode: u32) -> Result<(), Error> {
        self.sink
            .detail(&format!("write {} (mode {mode:o})", path.display()));
        self.host
            .write(path, contents, mode)
            .map_err(io("write", path))
    }

    pub fn copy(&self, from: &Path, to: &Path, mode: u32) -> Result<(), Error> {
        self.sink.detail(&format!(
            "copy {} to {} (mode {mode:o})",
            from.display(),
            to.display()
        ));
        self.host.copy(from, to, mode).map_err(io("copy", from))
    }

    pub fn create_dir(&self, path: &Path, mode: u32) -> Result<(), Error> {
        self.sink
            .detail(&format!("create {} (mode {mode:o})", path.display()));
        self.host
            .create_dir(path, mode)
            .map_err(io("create the folder", path))
    }

    pub fn list(&self, path: &Path) -> Result<Vec<String>, Error> {
        self.host.list(path).map_err(io("list", path))
    }

    pub fn rename(&self, from: &Path, to: &Path) -> Result<(), Error> {
        self.sink
            .detail(&format!("move {} to {}", from.display(), to.display()));
        self.host.rename(from, to).map_err(io("move", from))
    }

    pub fn remove_file(&self, path: &Path) -> Result<(), Error> {
        self.sink.detail(&format!("delete {}", path.display()));
        self.host.remove_file(path).map_err(io("delete", path))
    }

    pub fn remove_all(&self, path: &Path) -> Result<(), Error> {
        self.sink
            .detail(&format!("delete {} and its contents", path.display()));
        self.host.remove_all(path).map_err(io("delete", path))
    }

    pub fn owner(&self, path: &Path) -> Result<(u32, u32), Error> {
        self.host.owner(path).map_err(io("read the owner of", path))
    }

    pub fn chown(&self, path: &Path, uid: u32, gid: u32) -> Result<(), Error> {
        self.sink
            .detail(&format!("give {} to {uid}:{gid}", path.display()));
        self.host
            .chown(path, uid, gid)
            .map_err(io("change the owner of", path))
    }
}

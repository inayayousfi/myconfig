//! The machine the install runs on: every command it runs and every file it reads or changes
//! goes through `Host`, so tests can replace the machine with a recording fake.
use std::{
    fmt::Write as _,
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    process::{self, Stdio},
    thread,
};

use crate::Sink;

/// A program and its arguments. Built with `Command::new(...).arg(...)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    program: String,
    arguments: Vec<Argument>,
    stdin: Option<String>,
    mode: Mode,
    /// Whether the output stays out of the sink, because it holds a secret.
    hidden_output: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Argument {
    Shown(String),
    Secret(String),
}

/// What happens to a command's output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The output comes back in `Output`: for commands that only read the machine.
    Capture,
    /// Each line goes to the sink as it comes: for commands that change the machine.
    Stream,
}

impl Command {
    /// A command whose output comes back to the caller.
    pub fn query(program: &str) -> Self {
        Command {
            program: program.to_owned(),
            arguments: Vec::new(),
            stdin: None,
            mode: Mode::Capture,
            hidden_output: false,
        }
    }

    /// A command that changes the machine, whose output goes to the sink.
    pub fn action(program: &str) -> Self {
        Command {
            mode: Mode::Stream,
            ..Command::query(program)
        }
    }

    pub fn arg(mut self, argument: impl Into<String>) -> Self {
        self.arguments.push(Argument::Shown(argument.into()));
        self
    }

    pub fn args<S: Into<String>>(mut self, arguments: impl IntoIterator<Item = S>) -> Self {
        for argument in arguments {
            self = self.arg(argument);
        }
        self
    }

    /// An argument that never shows in the command's printed form.
    pub fn secret(mut self, argument: impl Into<String>) -> Self {
        self.arguments.push(Argument::Secret(argument.into()));
        self
    }

    /// Text written to the command's standard input. It never shows in the printed form.
    pub fn stdin(mut self, text: impl Into<String>) -> Self {
        self.stdin = Some(text.into());
        self
    }

    /// A query whose output comes back to the caller and never reaches the sink, for output
    /// that holds a secret or binary data. Its command line still shows.
    pub fn hidden_output(mut self) -> Self {
        self.mode = Mode::Capture;
        self.hidden_output = true;
        self
    }

    pub fn output_hidden(&self) -> bool {
        self.hidden_output
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn arguments(&self) -> impl Iterator<Item = &str> {
        self.arguments.iter().map(|argument| match argument {
            Argument::Shown(text) | Argument::Secret(text) => text.as_str(),
        })
    }

    pub fn input(&self) -> Option<&str> {
        self.stdin.as_deref()
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The command as a shell would read it, with secrets replaced by `<hidden>`.
    pub fn shown(&self) -> String {
        let mut text = quote(&self.program);
        for argument in &self.arguments {
            text.push(' ');
            match argument {
                Argument::Shown(argument) => text.push_str(&quote(argument)),
                Argument::Secret(_) => text.push_str("<hidden>"),
            }
        }
        text
    }
}

fn quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:+,@%".contains(c));
    if plain {
        word.to_owned()
    } else {
        let mut quoted = String::from("'");
        for c in word.chars() {
            if c == '\'' {
                quoted.push_str("'\\''");
            } else {
                quoted.push(c);
            }
        }
        quoted.push('\'');
        quoted
    }
}

/// How a command ended. A streamed command's lines went to the sink, so its `stdout` and
/// `stderr` are empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    /// None when a signal ended the command.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    /// The exit status in words, for messages.
    pub fn status(&self) -> String {
        match self.code {
            Some(code) => format!("exit code {code}"),
            None => "a signal".to_owned(),
        }
    }
}

/// The machine. Paths are absolute paths of the live system.
pub trait Host {
    /// Runs `command` to its end. A streamed command sends each output line to `sink`.
    /// Fails only when the command cannot start or its output cannot be read.
    fn run(&self, command: &Command, sink: &dyn Sink) -> io::Result<Output>;
    /// The effective user id of the install.
    fn effective_uid(&self) -> io::Result<u32>;
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn read(&self, path: &Path) -> io::Result<String>;
    /// Creates or replaces the file at `path` with `contents` and permission bits `mode`.
    fn write(&self, path: &Path, contents: &str, mode: u32) -> io::Result<()>;
    /// Copies the file `from` to `to`, replacing `to`, with permission bits `mode`.
    fn copy(&self, from: &Path, to: &Path, mode: u32) -> io::Result<()>;
    /// Creates `path` and its missing parents. `path` gets permission bits `mode`.
    fn create_dir(&self, path: &Path, mode: u32) -> io::Result<()>;
    /// The names inside the folder `path`, sorted.
    fn list(&self, path: &Path) -> io::Result<Vec<String>>;
    /// Renames `from` to `to` inside one filesystem.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Removes `path` and, for a folder, everything inside it. Never follows links.
    fn remove_all(&self, path: &Path) -> io::Result<()>;
    /// The user and group ids that own `path`.
    fn owner(&self, path: &Path) -> io::Result<(u32, u32)>;
    fn chown(&self, path: &Path, uid: u32, gid: u32) -> io::Result<()>;
}

/// The machine the installer runs on.
pub struct RealHost;

impl Host for RealHost {
    fn run(&self, command: &Command, sink: &dyn Sink) -> io::Result<Output> {
        let mut child = process::Command::new(command.program())
            .args(command.arguments())
            .env("LC_ALL", "C")
            .stdin(if command.input().is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let streamed = command.mode() == Mode::Stream;
        let (stdout, stderr) = thread::scope(|scope| -> io::Result<(String, String)> {
            let out = scope.spawn(|| collect(stdout, streamed, sink));
            let err = scope.spawn(|| collect(stderr, streamed, sink));
            if let (Some(mut input), Some(text)) = (stdin.take(), command.input()) {
                // A command that exits without reading its input closes the pipe; its exit
                // status reports the failure.
                let _ = input.write_all(text.as_bytes());
            }
            let out = out
                .join()
                .map_err(|_| io::Error::other("output reader panicked"))?;
            let err = err
                .join()
                .map_err(|_| io::Error::other("output reader panicked"))?;
            Ok((out?, err?))
        })?;
        let status = child.wait()?;
        Ok(Output {
            code: status.code(),
            stdout,
            stderr,
        })
    }

    fn effective_uid(&self) -> io::Result<u32> {
        // The kernel gives /proc/self the effective user id of the process.
        Ok(fs::metadata("/proc/self")?.uid())
    }

    fn exists(&self, path: &Path) -> bool {
        path.symlink_metadata().is_ok()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.symlink_metadata().is_ok_and(|meta| meta.is_dir())
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    fn write(&self, path: &Path, contents: &str, mode: u32) -> io::Result<()> {
        fs::write(path, contents)?;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }

    fn copy(&self, from: &Path, to: &Path, mode: u32) -> io::Result<()> {
        fs::copy(from, to)?;
        fs::set_permissions(to, fs::Permissions::from_mode(mode))
    }

    fn create_dir(&self, path: &Path, mode: u32) -> io::Result<()> {
        fs::create_dir_all(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }

    fn list(&self, path: &Path) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(path)? {
            names.push(entry?.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        Ok(names)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_all(&self, path: &Path) -> io::Result<()> {
        if path.symlink_metadata()?.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
    }

    fn owner(&self, path: &Path) -> io::Result<(u32, u32)> {
        let meta = fs::metadata(path)?;
        Ok((meta.uid(), meta.gid()))
    }

    fn chown(&self, path: &Path, uid: u32, gid: u32) -> io::Result<()> {
        std::os::unix::fs::lchown(path, Some(uid), Some(gid))
    }
}

/// Reads one output pipe to its end. Streamed lines go to the sink and nothing is kept.
fn collect(pipe: impl Read, streamed: bool, sink: &dyn Sink) -> io::Result<String> {
    let mut reader = BufReader::new(pipe);
    let mut kept = String::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            return Ok(kept);
        }
        let text = String::from_utf8_lossy(&line);
        if streamed {
            sink.output(text.trim_end_matches(['\n', '\r']));
        } else {
            let _ = write!(kept, "{text}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Host, RealHost};
    use crate::fake::FakeSink;

    #[test]
    fn a_real_query_returns_its_output_and_reads_its_input() {
        let output = RealHost
            .run(
                &Command::query("cat").stdin("one\ntwo\n"),
                &FakeSink::default(),
            )
            .unwrap();
        assert!(output.success());
        assert_eq!(output.stdout, "one\ntwo\n");
    }

    #[test]
    fn a_real_action_streams_both_outputs_and_reports_its_exit_code() {
        let sink = FakeSink::default();
        let output = RealHost
            .run(
                &Command::action("sh").args(["-c", "echo out; echo err >&2; exit 3"]),
                &sink,
            )
            .unwrap();
        assert_eq!(output.code, Some(3));
        assert!(output.stdout.is_empty());
        let mut lines = sink.all();
        lines.sort();
        assert_eq!(lines, ["output err", "output out"]);
    }

    #[test]
    fn the_printed_form_quotes_words_and_hides_secrets() {
        let command = Command::action("nmcli")
            .args(["device", "wifi", "connect", "My Home"])
            .arg("password")
            .secret("hunter2")
            .stdin("also hidden");
        assert_eq!(
            command.shown(),
            "nmcli device wifi connect 'My Home' password <hidden>"
        );
        assert_eq!(
            command.arguments().collect::<Vec<_>>(),
            [
                "device", "wifi", "connect", "My Home", "password", "hunter2"
            ]
        );
    }
}

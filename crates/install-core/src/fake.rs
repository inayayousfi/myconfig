//! A recording machine for tests: commands return prepared outputs, files live in memory,
//! and every command and change is kept in order.
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, VecDeque},
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::{
    Sink,
    host::{Command, Host, Mode, Output},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    File {
        text: String,
        mode: u32,
        owner: (u32, u32),
    },
    Dir {
        mode: u32,
        owner: (u32, u32),
    },
}

#[derive(Default)]
pub struct FakeHost {
    pub uid: u32,
    /// Every path the fake holds. A path under a held path is a folder even without an entry.
    pub files: RefCell<BTreeMap<PathBuf, Node>>,
    /// Outputs by printed command, used in order; the last one repeats.
    responses: RefCell<HashMap<String, VecDeque<Output>>>,
    /// Each command run, as `! command` for actions and `? command` for queries, and each
    /// file change, in order.
    pub log: RefCell<Vec<String>>,
    /// Every file written, with its contents, in order.
    pub written: RefCell<Vec<(PathBuf, String)>>,
    /// The standard input each command received, by printed command.
    pub inputs: RefCell<Vec<(String, String)>>,
}

pub fn ok(stdout: &str) -> Output {
    Output {
        code: Some(0),
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

pub fn exit(code: i32) -> Output {
    Output {
        code: Some(code),
        ..Output::default()
    }
}

impl FakeHost {
    pub fn respond(&self, command: &str, outputs: impl IntoIterator<Item = Output>) {
        self.responses
            .borrow_mut()
            .insert(command.to_owned(), outputs.into_iter().collect());
    }

    pub fn file(&self, path: &str, text: &str) {
        self.files.borrow_mut().insert(
            path.into(),
            Node::File {
                text: text.to_owned(),
                mode: 0o644,
                owner: (0, 0),
            },
        );
    }

    pub fn dir(&self, path: &str, owner: (u32, u32)) {
        self.files
            .borrow_mut()
            .insert(path.into(), Node::Dir { mode: 0o755, owner });
    }

    pub fn node(&self, path: &str) -> Option<Node> {
        self.files.borrow().get(Path::new(path)).cloned()
    }

    pub fn text(&self, path: &str) -> Option<String> {
        match self.node(path)? {
            Node::File { text, .. } => Some(text),
            Node::Dir { .. } => None,
        }
    }

    /// The commands that change the machine, in order.
    pub fn actions(&self) -> Vec<String> {
        self.log
            .borrow()
            .iter()
            .filter_map(|line| line.strip_prefix("! ").map(str::to_owned))
            .collect()
    }

    fn under(&self, path: &Path) -> Vec<PathBuf> {
        self.files
            .borrow()
            .keys()
            .filter(|key| key.starts_with(path))
            .cloned()
            .collect()
    }

    fn note(&self, line: String) {
        self.log.borrow_mut().push(line);
    }
}

fn missing(path: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, path.display().to_string())
}

impl Host for FakeHost {
    fn run(&self, command: &Command, sink: &dyn Sink) -> io::Result<Output> {
        let shown = command.shown();
        let mark = match command.mode() {
            Mode::Stream => "!",
            Mode::Capture => "?",
        };
        self.note(format!("{mark} {shown}"));
        if let Some(input) = command.input() {
            self.inputs
                .borrow_mut()
                .push((shown.clone(), input.to_owned()));
        }
        let output = match self.responses.borrow_mut().get_mut(&shown) {
            Some(queue) if queue.len() > 1 => queue.pop_front().expect("the queue is not empty"),
            Some(queue) => queue.front().cloned().unwrap_or_default(),
            None => ok(""),
        };
        if command.mode() == Mode::Stream {
            for line in output.stdout.lines() {
                sink.output(line);
            }
            return Ok(Output {
                stdout: String::new(),
                ..output
            });
        }
        Ok(output)
    }

    fn effective_uid(&self) -> io::Result<u32> {
        Ok(self.uid)
    }

    fn exists(&self, path: &Path) -> bool {
        !self.under(path).is_empty()
    }

    fn is_dir(&self, path: &Path) -> bool {
        match self.files.borrow().get(path) {
            Some(Node::Dir { .. }) => true,
            Some(Node::File { .. }) => false,
            None => self.exists(path),
        }
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        match self.files.borrow().get(path) {
            Some(Node::File { text, .. }) => Ok(text.clone()),
            _ => Err(missing(path)),
        }
    }

    fn write(&self, path: &Path, contents: &str, mode: u32) -> io::Result<()> {
        self.note(format!("write {} {mode:o}", path.display()));
        self.written
            .borrow_mut()
            .push((path.to_owned(), contents.to_owned()));
        self.files.borrow_mut().insert(
            path.to_owned(),
            Node::File {
                text: contents.to_owned(),
                mode,
                owner: (0, 0),
            },
        );
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path, mode: u32) -> io::Result<()> {
        let text = self.read(from)?;
        self.note(format!("copy {} {} {mode:o}", from.display(), to.display()));
        self.files.borrow_mut().insert(
            to.to_owned(),
            Node::File {
                text,
                mode,
                owner: (0, 0),
            },
        );
        Ok(())
    }

    fn create_dir(&self, path: &Path, mode: u32) -> io::Result<()> {
        self.note(format!("mkdir {} {mode:o}", path.display()));
        let owner = match self.files.borrow().get(path) {
            Some(Node::Dir { owner, .. }) => *owner,
            _ => (0, 0),
        };
        self.files
            .borrow_mut()
            .insert(path.to_owned(), Node::Dir { mode, owner });
        Ok(())
    }

    fn list(&self, path: &Path) -> io::Result<Vec<String>> {
        if !self.is_dir(path) {
            return Err(missing(path));
        }
        let mut names: Vec<String> = self
            .under(path)
            .iter()
            .filter_map(|key| key.strip_prefix(path).ok()?.components().next())
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let moved = self.under(from);
        if moved.is_empty() {
            return Err(missing(from));
        }
        if self.exists(to) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                to.display().to_string(),
            ));
        }
        self.note(format!("rename {} {}", from.display(), to.display()));
        let mut files = self.files.borrow_mut();
        if !files.contains_key(from) {
            files.insert(
                to.to_owned(),
                Node::Dir {
                    mode: 0o755,
                    owner: (0, 0),
                },
            );
        }
        for key in moved {
            let node = files.remove(&key).expect("the key was listed");
            let relative = key.strip_prefix(from).expect("the key is under from");
            let moved_to = if relative.as_os_str().is_empty() {
                to.to_owned()
            } else {
                to.join(relative)
            };
            files.insert(moved_to, node);
        }
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.note(format!("rm {}", path.display()));
        match self.files.borrow_mut().remove(path) {
            Some(Node::File { .. }) => Ok(()),
            _ => Err(missing(path)),
        }
    }

    fn remove_all(&self, path: &Path) -> io::Result<()> {
        let removed = self.under(path);
        if removed.is_empty() {
            return Err(missing(path));
        }
        self.note(format!("rm -r {}", path.display()));
        let mut files = self.files.borrow_mut();
        for key in removed {
            files.remove(&key);
        }
        Ok(())
    }

    fn owner(&self, path: &Path) -> io::Result<(u32, u32)> {
        match self.files.borrow().get(path) {
            Some(Node::File { owner, .. } | Node::Dir { owner, .. }) => Ok(*owner),
            None if self.exists(path) => Ok((0, 0)),
            None => Err(missing(path)),
        }
    }

    fn chown(&self, path: &Path, uid: u32, gid: u32) -> io::Result<()> {
        self.note(format!("chown {} {uid}:{gid}", path.display()));
        match self.files.borrow_mut().get_mut(path) {
            Some(Node::File { owner, .. } | Node::Dir { owner, .. }) => {
                *owner = (uid, gid);
                Ok(())
            }
            None => Err(missing(path)),
        }
    }
}

/// Keeps every line reported, each with its kind.
#[derive(Default)]
pub struct FakeSink {
    pub lines: Mutex<Vec<String>>,
}

impl FakeSink {
    pub fn all(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

impl Sink for FakeSink {
    fn step(&self, text: &str) {
        self.lines.lock().unwrap().push(format!("step {text}"));
    }
    fn command(&self, command: &str) {
        self.lines
            .lock()
            .unwrap()
            .push(format!("command {command}"));
    }
    fn output(&self, line: &str) {
        self.lines.lock().unwrap().push(format!("output {line}"));
    }
    fn detail(&self, text: &str) {
        self.lines.lock().unwrap().push(format!("detail {text}"));
    }
}

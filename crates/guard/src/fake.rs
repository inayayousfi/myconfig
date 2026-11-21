//! A machine for tests: programs return prepared outputs, files live in memory, and every
//! program run is kept in order.
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, VecDeque},
    io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::system::{Output, System};

#[derive(Default)]
pub struct FakeSystem {
    pub files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    /// Outputs by command line, such as `lsinitcpio /boot/initramfs-linux.img`, given out
    /// in turn: each run takes the first, and the last one stays. A program with no
    /// prepared output is not installed.
    responses: RefCell<HashMap<String, VecDeque<Output>>>,
    /// The file each command line creates when it runs, such as the output of `ukify`.
    creates: RefCell<HashMap<String, PathBuf>>,
    /// Each command line run, in order.
    pub log: RefCell<Vec<String>>,
    /// What the person types when asked for a secret, or None when they give no answer.
    pub secret: RefCell<Option<String>>,
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

/// The start of an x86 kernel image whose boot header points at `version`.
pub fn kernel_image(version: &str) -> Vec<u8> {
    let mut image = vec![0u8; 0x600];
    image[0x202..0x206].copy_from_slice(b"HdrS");
    image[0x20E..0x210].copy_from_slice(&0x300u16.to_le_bytes());
    let text = format!("{version} (linux@archlinux) #1 SMP PREEMPT_DYNAMIC\0");
    image[0x500..0x500 + text.len()].copy_from_slice(text.as_bytes());
    image
}

impl FakeSystem {
    pub fn respond(&self, command: &str, output: Output) {
        self.respond_in_turn(command, vec![output]);
    }

    /// Prepares one output for each run of `command`, in order. The last one answers every
    /// later run.
    pub fn respond_in_turn(&self, command: &str, outputs: Vec<Output>) {
        self.responses
            .borrow_mut()
            .insert(command.to_owned(), outputs.into());
    }

    /// Makes `command` create the file `path` when it runs and succeeds.
    pub fn creates(&self, command: &str, path: &str) {
        self.creates
            .borrow_mut()
            .insert(command.to_owned(), path.into());
    }

    pub fn ran(&self, command: &str) -> bool {
        self.log.borrow().iter().any(|line| line == command)
    }

    pub fn file(&self, path: &str, contents: &[u8]) {
        self.files
            .borrow_mut()
            .insert(path.into(), contents.to_vec());
    }

    pub fn text(&self, path: &str) -> Option<String> {
        self.files
            .borrow()
            .get(Path::new(path))
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    }
}

impl System for FakeSystem {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output> {
        let line = std::iter::once(program)
            .chain(args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ");
        self.log.borrow_mut().push(line.clone());
        let output = {
            let mut responses = self.responses.borrow_mut();
            let outputs = responses.get_mut(&line).ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("{program} not found"))
            })?;
            if outputs.len() > 1 {
                outputs.pop_front()
            } else {
                outputs.front().cloned()
            }
            .unwrap_or_default()
        };
        if output.success()
            && let Some(path) = self.creates.borrow().get(&line)
        {
            self.files.borrow_mut().insert(path.clone(), b"MZ".to_vec());
        }
        Ok(output)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))
    }

    fn exists(&self, path: &Path) -> bool {
        self.files
            .borrow()
            .keys()
            .any(|file| file == path || file.starts_with(path))
    }

    fn list(&self, path: &Path) -> io::Result<Vec<String>> {
        let mut names: Vec<String> = self
            .files
            .borrow()
            .keys()
            .filter_map(|file| file.strip_prefix(path).ok())
            .filter_map(|rest| rest.components().next())
            .map(|first| first.as_os_str().to_string_lossy().into_owned())
            .collect();
        if names.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                path.display().to_string(),
            ));
        }
        names.sort();
        names.dedup();
        Ok(names)
    }

    fn write_atomic(&self, path: &Path, contents: &[u8], _mode: u32) -> io::Result<()> {
        self.files
            .borrow_mut()
            .insert(path.to_owned(), contents.to_vec());
        Ok(())
    }

    fn create_dir_all(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.files.borrow_mut().remove(path);
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut files = self.files.borrow_mut();
        let contents = files
            .remove(from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, from.display().to_string()))?;
        files.insert(to.to_owned(), contents);
        Ok(())
    }

    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_795_000_000)
    }

    fn ask_secret(&self, prompt: &str) -> io::Result<String> {
        self.log.borrow_mut().push(format!("ask {prompt}"));
        self.secret
            .borrow()
            .clone()
            .ok_or_else(|| io::Error::other("no answer"))
    }
}

//! The machine the guard checks: every program it runs and every file it reads or writes
//! goes through `System`, so tests can replace the machine with a fake.
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::{self, Stdio},
    time::SystemTime,
};

/// How a program ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    /// None when a signal ended the program.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// The machine. Paths are absolute.
pub trait System {
    /// Runs `program` with `args` to its end and captures its output. Fails only when the
    /// program cannot start, for example because it is not installed.
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output>;
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn exists(&self, path: &Path) -> bool;
    /// The names inside the folder `path`, sorted.
    fn list(&self, path: &Path) -> io::Result<Vec<String>>;
    /// Replaces the file at `path` with `contents` in one step: a reader sees the old file
    /// or the new one, never part of it. A new file gets permission bits `mode`.
    fn write_atomic(&self, path: &Path, contents: &[u8], mode: u32) -> io::Result<()>;
    /// Creates the folder `path` and its missing parents.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Removes the file at `path`, and succeeds when there is none.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Moves the file `from` to `to` in one step, replacing `to`. Both are on one
    /// filesystem.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn now(&self) -> SystemTime;
    /// Asks the person at the terminal for a secret, without showing what they type, and
    /// returns it.
    fn ask_secret(&self, prompt: &str) -> io::Result<String>;
}

/// The file at `path` as text, with invalid UTF-8 replaced.
pub fn read_text(system: &dyn System, path: &Path) -> io::Result<String> {
    Ok(String::from_utf8_lossy(&system.read(path)?).into_owned())
}

/// The machine the guard runs on.
pub struct RealSystem;

impl System for RealSystem {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output> {
        let output = process::Command::new(program)
            .args(args)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .output()?;
        Ok(Output {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn list(&self, path: &Path) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(path)? {
            names.push(entry?.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        Ok(names)
    }

    fn write_atomic(&self, path: &Path, contents: &[u8], mode: u32) -> io::Result<()> {
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other(format!("{} has no file name", path.display())))?;
        let temporary = path.with_file_name(format!(".{}.new", name.to_string_lossy()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        // The rename reaches the disk with its folder.
        if let Some(parent) = path.parent()
            && let Ok(folder) = fs::File::open(parent)
        {
            let _ = folder.sync_all();
        }
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        match fs::remove_file(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)?;
        if let Some(parent) = to.parent()
            && let Ok(folder) = fs::File::open(parent)
        {
            let _ = folder.sync_all();
        }
        Ok(())
    }

    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn ask_secret(&self, prompt: &str) -> io::Result<String> {
        // systemd-ask-password reads from the terminal and prints the answer on stdout.
        let output = process::Command::new("systemd-ask-password")
            .args(["--timeout=0", "--emoji=no", prompt])
            .stdin(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other("systemd-ask-password got no answer"));
        }
        String::from_utf8(output.stdout)
            .map(|text| text.trim_end_matches('\n').to_owned())
            .map_err(|_| io::Error::other("the answer is not valid text"))
    }
}

/// Where the guard's commands take turns: each command that changes the TPM slots, the
/// status record or rEFInd's entries holds an exclusive lock on this file while it runs.
pub const LOCK_FILE: &str = "/run/inayos-guard.lock";

/// Waits until no other guard command holds the lock, then takes it. The lock lasts as
/// long as the returned file stays open.
pub fn lock() -> io::Result<fs::File> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(LOCK_FILE)?;
    file.lock()?;
    Ok(file)
}

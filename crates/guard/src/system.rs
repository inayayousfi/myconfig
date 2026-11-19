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
    fn now(&self) -> SystemTime;
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

    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

//! Allows one VM at a time on the host.

use std::fs::{self, File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("another xtask VM command is running (it holds {0})")]
    Busy(PathBuf),
    #[error("could not open the lock file {0}: {1}")]
    Io(PathBuf, io::Error),
}

/// An exclusive lock on the lock file. The kernel releases it when this value is dropped
/// or when the process ends, so a crash never leaves a stale lock.
#[derive(Debug)]
pub struct VmLock {
    _file: File,
}

impl VmLock {
    pub fn acquire(path: &Path) -> Result<VmLock, LockError> {
        let io_error = |error| LockError::Io(path.to_owned(), error);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(io_error)?;
        match file.try_lock() {
            Ok(()) => Ok(VmLock { _file: file }),
            Err(TryLockError::WouldBlock) => Err(LockError::Busy(path.to_owned())),
            Err(TryLockError::Error(error)) => Err(io_error(error)),
        }
    }
}

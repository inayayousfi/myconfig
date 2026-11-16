//! Reads web pages and downloads files with curl, and keeps one verified file per guest
//! system in the download folder.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use sha2::{Digest, Sha256};

/// Microsoft's download page offers the ISO only to a browser that is not on Windows, so every
/// request names a desktop Linux browser.
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("could not start curl: {0}")]
    Spawn(io::Error),
    #[error("could not read {url}: curl stopped with {status}: {stderr}")]
    Fetch {
        url: String,
        status: ExitStatus,
        stderr: String,
    },
    #[error(
        "downloading {url} stopped with {status}; the partial file {partial} is kept to resume"
    )]
    Download {
        url: String,
        status: ExitStatus,
        partial: PathBuf,
    },
    #[error(
        "the SHA-256 of {file} is {actual}, not the published {expected}; the file was removed"
    )]
    Checksum {
        file: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("could not change {0}: {1}")]
    Io(PathBuf, io::Error),
}

/// Returns the body of `url`. With `referer`, the request names that page as its origin,
/// which Microsoft's download service requires.
pub fn fetch_text(url: &str, referer: Option<&str>) -> Result<String, DownloadError> {
    let mut command = curl();
    command.arg("--silent");
    if let Some(referer) = referer {
        command.arg("--referer").arg(referer);
    }
    let output = command
        .arg("--")
        .arg(url)
        .stdin(Stdio::null())
        .output()
        .map_err(DownloadError::Spawn)?;
    if !output.status.success() {
        return Err(DownloadError::Fetch {
            url: url.to_owned(),
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Returns `folder/file_name` with the SHA-256 `expected`, downloading it from `url` when the
/// folder has no such file. Every other file in `folder` is removed once the file is verified,
/// so the folder keeps only the current download.
///
/// A download goes to `file_name.part` first and gets its final name only after its checksum
/// matches. An interrupted download resumes from the partial file on the next call.
pub fn fetch_verified(
    url: &str,
    expected: &str,
    folder: &Path,
    file_name: &str,
) -> Result<PathBuf, DownloadError> {
    fs::create_dir_all(folder).map_err(io_error(folder))?;
    let file = folder.join(file_name);
    let partial = folder.join(format!("{file_name}.part"));

    let verified = file.is_file() && sha256(&file).map_err(io_error(&file))? == expected;
    if !verified {
        // A partial file that is already complete makes the server refuse the resume request.
        let complete =
            partial.is_file() && sha256(&partial).map_err(io_error(&partial))? == expected;
        if !complete {
            println!("vm: downloading {url}");
            let status = curl()
                .arg("--progress-bar")
                .args(["--continue-at", "-", "--output"])
                .arg(&partial)
                .arg("--")
                .arg(url)
                .stdin(Stdio::null())
                .status()
                .map_err(DownloadError::Spawn)?;
            if !status.success() {
                return Err(DownloadError::Download {
                    url: strip_query(url).to_owned(),
                    status,
                    partial,
                });
            }
            let actual = sha256(&partial).map_err(io_error(&partial))?;
            if actual != expected {
                fs::remove_file(&partial).map_err(io_error(&partial))?;
                return Err(DownloadError::Checksum {
                    file: partial,
                    expected: expected.to_owned(),
                    actual,
                });
            }
        }
        fs::rename(&partial, &file).map_err(io_error(&file))?;
    }
    keep_only(folder, &file)?;
    Ok(file)
}

/// Returns the file in `folder` with the SHA-256 `expected`, for a file that the user saved
/// there by hand, or `None` when no file matches. Every other file in `folder` is removed once
/// one matches, as `fetch_verified` does.
pub fn find_verified(expected: &str, folder: &Path) -> Result<Option<PathBuf>, DownloadError> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(DownloadError::Io(folder.to_owned(), error)),
    };
    for entry in entries {
        let path = entry.map_err(io_error(folder))?.path();
        if path.is_file() && sha256(&path).map_err(io_error(&path))? == expected {
            keep_only(folder, &path)?;
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Removes every file in `folder` except `kept`, so the folder holds only the current file.
fn keep_only(folder: &Path, kept: &Path) -> Result<(), DownloadError> {
    for entry in fs::read_dir(folder).map_err(io_error(folder))? {
        let path = entry.map_err(io_error(folder))?.path();
        if path != kept {
            fs::remove_file(&path).map_err(io_error(&path))?;
        }
    }
    Ok(())
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> DownloadError {
    let path = path.to_owned();
    move |error| DownloadError::Io(path, error)
}

/// The SHA-256 of a file, in lowercase hexadecimal.
pub fn sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// The URL without its query, which for a Microsoft download link holds a temporary token.
pub fn strip_query(url: &str) -> &str {
    url.split_once('?').map_or(url, |(path, _)| path)
}

fn curl() -> Command {
    let mut command = Command::new("curl");
    // `Accept:` with no value removes the header curl sends by default. The requests follow
    // the ones the Mido script sends to Microsoft's download service, which has no public API.
    command
        .args(["--fail", "--location", "--show-error"])
        .args(["--proto", "=https", "--tlsv1.2"])
        .args(["--user-agent", USER_AGENT, "--header", "Accept:"]);
    command
}

//! A small HTTP server on the host that gives a guest the files of one folder: a test copy of
//! the InayOS pacman repository and a test Arch news feed. QEMU's user network takes the
//! guest's connections to 10.0.2.2 to the host's loopback address, where the server listens.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// The host as the guest's user network reaches it.
pub const HOST_FROM_GUEST: &str = "10.0.2.2";
/// A client that sends nothing is dropped after this time, so it cannot hold the server.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// A running server. Dropping it stops the server.
pub struct FileServer {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FileServer {
    /// Serves the files at the top of `folder` on a free port of the host's loopback address.
    pub fn start(folder: &Path) -> io::Result<FileServer> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let folder = folder.to_owned();
        let stopped = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(Ordering::SeqCst) {
                    break;
                }
                // One client at a time: pacman downloads its files one after another.
                if let Ok(stream) = stream
                    && let Err(error) = answer(stream, &folder)
                {
                    eprintln!("vm: the file server could not answer a request: {error}");
                }
            }
        });
        Ok(FileServer {
            port,
            stop,
            thread: Some(thread),
        })
    }

    /// The address of the served folder from inside the guest.
    pub fn guest_url(&self) -> String {
        format!("http://{HOST_FROM_GUEST}:{}", self.port)
    }

    #[cfg(test)]
    fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for FileServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // A connection wakes the accepting thread, which then sees the stop flag.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What one request line asks for.
#[derive(Debug, PartialEq, Eq)]
struct Request {
    /// Whether the body is wanted: a HEAD request asks for the headers only.
    body: bool,
    /// The file name at the top of the served folder.
    name: String,
}

/// Reads a request line such as `GET /inayos.db HTTP/1.1`. Only GET and HEAD of a plain file
/// name are served, so no request reaches a file outside the folder.
fn request(line: &str) -> Option<Request> {
    let mut words = line.split_whitespace();
    let body = match words.next()? {
        "GET" => true,
        "HEAD" => false,
        _ => return None,
    };
    let path = words.next()?;
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let name = path.strip_prefix('/')?;
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return None;
    }
    Some(Request {
        body,
        name: name.to_owned(),
    })
}

fn answer(stream: TcpStream, folder: &Path) -> io::Result<()> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    // The headers are read and ignored: every answer is the whole file.
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
    }
    let mut stream = stream;
    let file = request(&line).and_then(|request| {
        let path: PathBuf = folder.join(&request.name);
        let file = File::open(&path).ok()?;
        let length = file
            .metadata()
            .ok()
            .filter(|metadata| metadata.is_file())?
            .len();
        Some((request.body, file, length))
    });
    match file {
        Some((body, mut file, length)) => {
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
            )?;
            if body {
                io::copy(&mut file, &mut stream)?;
            }
        }
        None => {
            stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )?;
        }
    }
    stream.flush()?;
    stream.shutdown(Shutdown::Both)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;

    fn get(port: u16, line: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(stream, "{line}\r\nHost: test\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn the_server_gives_the_files_of_its_folder_and_nothing_else() {
        let root = std::env::temp_dir().join(format!("inayos-xtask-server-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let folder = root.join("repo");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("inayos.db"), "database").unwrap();
        fs::write(root.join("secret"), "outside").unwrap();

        let server = FileServer::start(&folder).unwrap();
        let found = get(server.port(), "GET /inayos.db HTTP/1.1");
        assert!(found.starts_with("HTTP/1.1 200 OK\r\n"), "{found}");
        assert!(found.contains("Content-Length: 8\r\n"), "{found}");
        assert!(found.ends_with("\r\n\r\ndatabase"), "{found}");

        let head = get(server.port(), "HEAD /inayos.db HTTP/1.1");
        assert!(head.contains("Content-Length: 8\r\n") && head.ends_with("\r\n\r\n"));

        for line in [
            "GET /missing HTTP/1.1",
            "GET /../secret HTTP/1.1",
            "GET /%2e%2e/secret HTTP/1.1",
            "POST /inayos.db HTTP/1.1",
        ] {
            assert!(
                get(server.port(), line).starts_with("HTTP/1.1 404"),
                "{line}"
            );
        }
        drop(server);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_request_names_one_file_at_the_top_of_the_folder() {
        assert_eq!(
            request("GET /inayos.db.sig?x=1 HTTP/1.1"),
            Some(Request {
                body: true,
                name: "inayos.db.sig".to_owned()
            })
        );
        assert_eq!(request("GET / HTTP/1.1"), None);
        assert_eq!(request("GET /a/b HTTP/1.1"), None);
        assert_eq!(request("GET /.hidden HTTP/1.1"), None);
    }
}

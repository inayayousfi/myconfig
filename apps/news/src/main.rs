//! `inayos-news`: the unread Arch news, so an update that needs a manual step never starts
//! unannounced.
//!
//! - `inayos-news` prints every unread item with its text, then marks them read.
//! - `inayos-news --pacman` runs from a pacman hook before each upgrade. It prints the unread
//!   items that announce a manual step and always exits 0, so the transaction goes on.
//! - `inayos-news --unread-manual` prints the title of each unread item that announces a
//!   manual step, one per line, for the login notification. It fails when the feed cannot
//!   be read, so the notification tries again once the network is up.
//!
//! The feed comes through `curl`, which every Arch system has because pacman needs it.
mod feed;

use std::{
    fs,
    path::Path,
    process::{Command, ExitCode},
};

use feed::Item;

/// The Arch news feed.
const DEFAULT_URL: &str = "https://archlinux.org/feeds/news/";
/// Overrides the feed address, before the configuration file.
const URL_VARIABLE: &str = "INAYOS_NEWS_URL";
/// `url=<address>` there overrides the feed address.
const CONFIG: &str = "/etc/inayos-news.conf";
/// The publication time of the newest item read, in seconds since 1970 UTC. Its folder
/// belongs to root and the wheel group, so the user can mark the news read.
const READ_FOLDER: &str = "/var/lib/inayos-news";
const READ: &str = "/var/lib/inayos-news/read";
/// How long the feed may take, in seconds.
const TIMEOUT: &str = "10";

const USAGE: &str = "usage: inayos-news [--pacman | --unread-manual]

Without an option, prints every unread Arch news item and marks them read.
  --pacman         print the unread items that announce a manual step; never fails
  --unread-manual  print the title of each unread item that announces a manual step";

enum Mode {
    Read,
    Pacman,
    UnreadManual,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = match args.as_slice() {
        [] => Mode::Read,
        [flag] if flag == "--pacman" => Mode::Pacman,
        [flag] if flag == "--unread-manual" => Mode::UnreadManual,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let items = match fetch(&feed_url()).and_then(|xml| feed::parse(&xml)) {
        Ok(items) => items,
        Err(error) => {
            return match mode {
                Mode::Read => {
                    eprintln!("error: could not read the Arch news: {error}");
                    ExitCode::FAILURE
                }
                Mode::Pacman => {
                    println!("inayos-news: could not read the Arch news ({error})");
                    ExitCode::SUCCESS
                }
                Mode::UnreadManual => {
                    eprintln!("inayos-news: could not read the Arch news ({error})");
                    ExitCode::FAILURE
                }
            };
        }
    };
    let read = read_mark();
    let unread = feed::unread(&items, read);
    match mode {
        Mode::Read => show_and_mark(&unread),
        Mode::Pacman => {
            print!("{}", pacman_text(&unread));
            ExitCode::SUCCESS
        }
        Mode::UnreadManual => {
            for item in unread.iter().filter(|item| item.announces_manual_step()) {
                println!("{}", item.title);
            }
            ExitCode::SUCCESS
        }
    }
}

/// The address of the feed: the environment variable, else the configuration file, else
/// the Arch feed.
fn feed_url() -> String {
    if let Ok(url) = std::env::var(URL_VARIABLE)
        && !url.trim().is_empty()
    {
        return url.trim().to_owned();
    }
    fs::read_to_string(CONFIG)
        .ok()
        .and_then(|text| configured_url(&text))
        .unwrap_or_else(|| DEFAULT_URL.to_owned())
}

/// The `url=` value of the configuration text. Lines that start with `#` are comments.
fn configured_url(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "url" && !value.trim().is_empty()).then(|| value.trim().to_owned())
        })
}

fn fetch(url: &str) -> Result<String, String> {
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--max-time",
            TIMEOUT,
            url,
        ])
        .output()
        .map_err(|error| format!("could not start curl: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{url}: {}", stderr.trim()));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("{url} is not UTF-8 text"))
}

/// The read mark. Without one, every item is unread.
fn read_mark() -> i64 {
    fs::read_to_string(READ)
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

/// The lines pacman shows before the upgrade: nothing when no unread item announces a
/// manual step.
fn pacman_text(unread: &[&Item]) -> String {
    let manual: Vec<&&Item> = unread
        .iter()
        .filter(|item| item.announces_manual_step())
        .collect();
    if manual.is_empty() {
        return String::new();
    }
    let mut text = String::from("Unread Arch news announces a manual step for this update:\n");
    for item in manual {
        text.push_str(&format!(
            "  {}  {}\n    {}\n",
            feed::date(item.published),
            item.title,
            item.link
        ));
    }
    text.push_str("Run inayos-news to read it and mark it read.\n");
    text
}

fn show_and_mark(unread: &[&Item]) -> ExitCode {
    let Some(newest) = unread.iter().map(|item| item.published).max() else {
        println!("No unread Arch news.");
        return ExitCode::SUCCESS;
    };
    for item in unread {
        let mark = if item.announces_manual_step() {
            "  [manual step]"
        } else {
            ""
        };
        println!(
            "{}  {}{mark}\n{}\n\n{}\n",
            feed::date(item.published),
            item.title,
            item.link,
            item.body
        );
    }
    match write_read_mark(newest) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "error: could not mark the news read in {READ}: {error}. The folder belongs to \
                 root and the wheel group."
            );
            ExitCode::FAILURE
        }
    }
}

/// Replaces the read mark in one step, so a reader never sees half of it.
fn write_read_mark(published: i64) -> std::io::Result<()> {
    let temporary = Path::new(READ_FOLDER).join("read.new");
    fs::write(&temporary, format!("{published}\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o664))?;
    }
    fs::rename(&temporary, READ)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, published: i64) -> Item {
        Item {
            title: title.to_owned(),
            link: format!("https://archlinux.org/news/{published}/"),
            published,
            body: String::new(),
        }
    }

    #[test]
    fn pacman_names_only_the_items_with_a_manual_step() {
        let valkey = item("Valkey to replace Redis", 1);
        let firmware = item("linux-firmware requires manual intervention", 86_400);
        assert_eq!(pacman_text(&[&valkey]), "");
        assert_eq!(
            pacman_text(&[&valkey, &firmware]),
            "Unread Arch news announces a manual step for this update:\n  \
             1970-01-02  linux-firmware requires manual intervention\n    \
             https://archlinux.org/news/86400/\n\
             Run inayos-news to read it and mark it read.\n"
        );
    }

    #[test]
    fn the_configuration_file_names_the_feed() {
        assert_eq!(
            configured_url("# The feed\nurl = http://10.0.2.2:8080/news.xml\n").as_deref(),
            Some("http://10.0.2.2:8080/news.xml")
        );
        assert_eq!(configured_url("#url=http://example.org\n"), None);
        assert_eq!(configured_url("url=\n"), None);
    }
}

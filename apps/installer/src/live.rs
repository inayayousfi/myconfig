//! What the screens change or read on the live system, carried out through the core's `Host`:
//! the session's keyboard layout, Wi-Fi, the screen file of the VM tests, and the reboot.
use std::path::Path;

use inayos_install_core::{
    Host, Sink,
    host::{Command, Output},
};

/// The folder of the screen file, which holds the name of the screen the installer shows.
pub const SCREEN_FOLDER: &str = "/run/inayos-installer";
pub const SCREEN_FILE: &str = "/run/inayos-installer/screen";

/// One Wi-Fi network in range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    pub ssid: String,
    /// The signal strength in percent.
    pub signal: u8,
    /// Whether it needs a password.
    pub secured: bool,
}

/// Runs a command that only reads, or whose output only the caller needs.
fn query(host: &dyn Host, command: &Command) -> Result<Output, String> {
    let output = host
        .run(command, &Quiet)
        .map_err(|error| format!("could not start {}: {error}", command.program()))?;
    if output.success() {
        Ok(output)
    } else {
        let stderr = output.stderr.trim();
        Err(if stderr.is_empty() {
            format!("`{}` ended with {}", command.shown(), output.status())
        } else {
            stderr.to_owned()
        })
    }
}

/// Applies the XKB `layout` to every keyboard of the Sway session, when the installer runs
/// in one, which `SWAYSOCK` says. Outside Sway it does nothing.
pub fn apply_layout(host: &dyn Host, layout: &str, in_sway: bool) -> Result<(), String> {
    if !in_sway {
        return Ok(());
    }
    let command = Command::query("swaymsg").args(["input", "type:keyboard", "xkb_layout", layout]);
    query(host, &command).map(|_| ())
}

/// Whether NetworkManager says the live system reaches the internet.
pub fn connected(host: &dyn Host) -> bool {
    let command = Command::query("nmcli").args(["-t", "-f", "STATE", "general"]);
    query(host, &command).is_ok_and(|output| output.stdout.trim() == "connected")
}

/// The Wi-Fi networks in range, strongest first, one entry per name.
pub fn networks(host: &dyn Host) -> Result<Vec<Network>, String> {
    let command = Command::query("nmcli").args([
        "-t",
        "-f",
        "SSID,SIGNAL,SECURITY",
        "device",
        "wifi",
        "list",
    ]);
    Ok(parse_networks(&query(host, &command)?.stdout))
}

/// Connects the live system to `ssid`. NetworkManager keeps the connection, which the install
/// copies into the new system.
pub fn connect(host: &dyn Host, ssid: &str, password: Option<&str>) -> Result<(), String> {
    let mut command = Command::query("nmcli").args(["device", "wifi", "connect", ssid]);
    if let Some(password) = password {
        command = command.arg("password").secret(password);
    }
    query(host, &command).map(|_| ())
}

pub fn reboot(host: &dyn Host) -> Result<(), String> {
    query(host, &Command::query("systemctl").arg("reboot")).map(|_| ())
}

/// Writes `name` to the screen file. It replaces the file in one rename, so a reader never
/// sees it half written.
pub fn write_screen(host: &dyn Host, name: &str) -> Result<(), String> {
    let folder = Path::new(SCREEN_FOLDER);
    let file = Path::new(SCREEN_FILE);
    let partial = folder.join("screen.partial");
    host.create_dir(folder, 0o755)
        .and_then(|()| host.write(&partial, &format!("{name}\n"), 0o644))
        .and_then(|()| host.rename(&partial, file))
        .map_err(|error| format!("could not write {SCREEN_FILE}: {error}"))
}

/// Reads `nmcli -t -f SSID,SIGNAL,SECURITY device wifi list`. Terse mode separates fields
/// with `:` and escapes a `:` or `\` inside a field with `\`. Hidden networks have no name and
/// are left out.
pub fn parse_networks(text: &str) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();
    for line in text.lines() {
        let fields = split_terse(line);
        let [ssid, signal, security] = fields.as_slice() else {
            continue;
        };
        if ssid.is_empty() {
            continue;
        }
        let network = Network {
            ssid: ssid.clone(),
            signal: signal.parse().unwrap_or(0),
            secured: !(security.is_empty() || security == "--"),
        };
        match networks.iter_mut().find(|known| known.ssid == network.ssid) {
            Some(known) if known.signal < network.signal => *known = network,
            Some(_) => {}
            None => networks.push(network),
        }
    }
    networks.sort_by_key(|network| std::cmp::Reverse(network.signal));
    networks
}

fn split_terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(escaped) = chars.next() {
                    fields
                        .last_mut()
                        .expect("fields is never empty")
                        .push(escaped);
                }
            }
            ':' => fields.push(String::new()),
            c => fields.last_mut().expect("fields is never empty").push(c),
        }
    }
    fields
}

/// These commands are not part of the install, so their lines go nowhere.
struct Quiet;

impl Sink for Quiet {
    fn step(&self, _: &str) {}
    fn command(&self, _: &str) {}
    fn output(&self, _: &str) {}
    fn detail(&self, _: &str) {}
}

#[cfg(test)]
mod tests {
    use super::{Network, parse_networks};

    #[test]
    fn networks_keep_escaped_colons_drop_hidden_ones_and_keep_the_strongest_of_a_name() {
        let text = "Home\\:5G:70:WPA2\n:90:WPA2\nCafe:40:\nHome\\:5G:85:WPA2\nLab:20:--\n";
        assert_eq!(
            parse_networks(text),
            [
                Network {
                    ssid: "Home:5G".into(),
                    signal: 85,
                    secured: true,
                },
                Network {
                    ssid: "Cafe".into(),
                    signal: 40,
                    secured: false,
                },
                Network {
                    ssid: "Lab".into(),
                    signal: 20,
                    secured: false,
                },
            ]
        );
    }
}

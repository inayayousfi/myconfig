//! Counts the package updates a booted Linux guest has pending, without changing the guest's
//! own package lists.

use crate::os::{GUEST_PASSWORD, Os};
use crate::ssh::{Guest, SshError};

/// Syncs a temporary copy of the pacman database, then lists the upgradable packages with
/// `pacman -Qu` against it. Syncing the real database would leave the guest in a partial
/// upgrade for the installer. `pacman -Qu` exits with 1 when nothing is upgradable.
/// pacman downloads as its unprivileged `alpm` user, which must be able to enter the folder.
const PACMAN: &str = r#"db=$(mktemp -d) && chmod 755 "$db" || exit 1
ln -s /var/lib/pacman/local "$db/local"
printf '%s\n' "$PASSWORD" | sudo -S -p '' pacman -Sy --dbpath "$db" --logfile /dev/null >/dev/null
status=$?
if [ $status -eq 0 ]; then pacman -Qu --dbpath "$db"; [ $? -le 1 ] || status=1; fi
printf '%s\n' "$PASSWORD" | sudo -S -p '' rm -rf -- "$db"
exit $status"#;

/// Refreshes a temporary copy of the apt package lists, then lists the upgradable packages
/// with `apt list --upgradable` against it.
const APT: &str = r#"lists=$(mktemp -d) || exit 1
mkdir -p "$lists/state/partial" "$lists/cache/archives/partial"
options="-o Dir::State::Lists=$lists/state -o Dir::Cache=$lists/cache"
printf '%s\n' "$PASSWORD" | sudo -S -p '' apt-get -qq $options update >/dev/null
status=$?
if [ $status -eq 0 ]; then apt list --upgradable $options 2>/dev/null; fi
printf '%s\n' "$PASSWORD" | sudo -S -p '' rm -rf -- "$lists"
exit $status"#;

/// The number of pending package updates in the guest, or `None` for Windows, which has no
/// package list to compare.
pub fn pending(os: Os, guest: &Guest) -> Result<Option<usize>, SshError> {
    let (script, marker) = match os {
        Os::Cachyos => (PACMAN, " -> "),
        Os::Ubuntu => (APT, "[upgradable from"),
        Os::Windows => return Ok(None),
    };
    let output = guest.run(&format!("PASSWORD='{GUEST_PASSWORD}'\n{script}"))?;
    Ok(Some(
        output.lines().filter(|line| line.contains(marker)).count(),
    ))
}

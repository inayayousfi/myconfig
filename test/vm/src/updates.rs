//! Counts the package updates a booted Linux guest has pending, without changing the guest's
//! own package lists.

use crate::os::{GUEST_PASSWORD, Os};
use crate::ssh::{Guest, SshError};

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

/// The number of pending package updates in the guest, or `None` for a system whose count
/// the xtask does not read: Windows, which has no package list to compare, and Arch, whose
/// image only hosts the USB stick build, which upgrades it first.
pub fn pending(os: Os, guest: &Guest) -> Result<Option<usize>, SshError> {
    let (script, marker) = match os {
        Os::Ubuntu => (APT, "[upgradable from"),
        Os::Windows | Os::Arch => return Ok(None),
    };
    let output = guest.run(&format!("PASSWORD='{GUEST_PASSWORD}'\n{script}"))?;
    Ok(Some(
        output.lines().filter(|line| line.contains(marker)).count(),
    ))
}

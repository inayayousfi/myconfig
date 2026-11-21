//! The answers the screens give the modules' questions without asking: the SSH server on, and
//! no login, which the user does after the install. Each text is the exact question of
//! `crates/modules/src/ssh.rs` and `crates/modules/src/login.rs`.
use std::collections::BTreeMap;

pub const DEFAULTS: &[(&str, bool)] = &[
    ("The OpenSSH server is off. Turn it on now?", true),
    ("GitHub CLI is not authenticated. Log in now?", false),
    ("Tailscale is not authenticated. Log in now?", false),
];

pub fn defaults() -> BTreeMap<String, bool> {
    DEFAULTS
        .iter()
        .map(|(question, yes)| ((*question).to_owned(), *yes))
        .collect()
}

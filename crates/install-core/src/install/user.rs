//! Creates the user in the new system, with its password, sudo rules and SSH keys.
use std::path::{Path, PathBuf};

use super::{Machine, TARGET};
use crate::{Answers, Error, host::Command, parse};

/// The rule that lets the wheel group use sudo with its password.
pub(crate) const WHEEL_RULE: &str = "etc/sudoers.d/10-inayos-wheel";
/// The rule that lets the user run sudo without a password while `inayos install` runs.
/// The finish stage deletes it, also after a failure.
pub(crate) const INSTALLER_RULE: &str = "etc/sudoers.d/99-inayos-installer";

/// The user as the new system knows it.
pub(crate) struct NewUser {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
}

/// Creates the user. `kept_owner` is the owner of the user's kept home folder, found by the
/// checks; the new user takes it over, so the files in that folder stay the user's.
pub(crate) fn create(
    m: &Machine,
    answers: &Answers,
    kept_owner: Option<(u32, u32)>,
) -> Result<NewUser, Error> {
    let user = &answers.user;
    m.sink.step(&format!("Creating the user {}", user.username));
    let home = Path::new(TARGET).join("home").join(&user.username);
    let mut useradd = Command::action("arch-chroot").args([
        TARGET,
        "useradd",
        "-m",
        "-G",
        "wheel",
        "-c",
        &user.full_name,
    ]);
    if let Some((uid, gid)) = kept_owner {
        // `useradd -g` needs the group to exist. A home whose group the new system already
        // has, such as `users`, keeps that group as the user's primary group.
        let groups = m.read(&Path::new(TARGET).join("etc/group"))?;
        if !parse::has_group_id(&groups, gid) {
            m.run(Command::action("arch-chroot").args([
                TARGET.to_owned(),
                "groupadd".to_owned(),
                "-g".to_owned(),
                gid.to_string(),
                user.username.clone(),
            ]))?;
        }
        useradd = useradd.args([
            "-u".to_owned(),
            uid.to_string(),
            "-g".to_owned(),
            gid.to_string(),
        ]);
    }
    m.run(useradd.arg(&user.username))?;
    m.run(
        Command::action("arch-chroot")
            .args([TARGET, "chpasswd"])
            .stdin(format!("{}:{}\n", user.username, user.password.0)),
    )?;

    m.write(
        &Path::new(TARGET).join(WHEEL_RULE),
        "%wheel ALL=(ALL:ALL) ALL\n",
        0o440,
    )?;
    m.write(
        &Path::new(TARGET).join(INSTALLER_RULE),
        &format!("{} ALL=(ALL:ALL) NOPASSWD: ALL\n", user.username),
        0o440,
    )?;

    let passwd = m.read(&Path::new(TARGET).join("etc/passwd"))?;
    let (uid, gid) =
        parse::passwd_ids(&passwd, &user.username).ok_or_else(|| Error::NoUserEntry {
            username: user.username.clone(),
        })?;

    if !user.authorized_keys.is_empty() {
        let ssh: PathBuf = home.join(".ssh");
        m.create_dir(&ssh, 0o700)?;
        m.chown(&ssh, uid, gid)?;
        let keys = ssh.join("authorized_keys");
        // A kept home may already hold keys of its own, which stay.
        let mut text = if m.exists(&keys) {
            m.read(&keys)?
        } else {
            String::new()
        };
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        let present: Vec<String> = text.lines().map(|line| line.trim().to_owned()).collect();
        for key in &user.authorized_keys {
            if !present.iter().any(|line| line == key.trim()) {
                text.push_str(key);
                text.push('\n');
            }
        }
        m.write(&keys, &text, 0o600)?;
        m.chown(&keys, uid, gid)?;
    }
    Ok(NewUser {
        name: user.username.clone(),
        uid,
        gid,
    })
}

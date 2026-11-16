//! Axidev OSK, an on-screen keyboard for the desktop and the login screen.
//!
//! Axidev ships its own lifecycle installer. A fresh install records that installer's
//! `uninstall` as the undo command, so `remove` uninstalls only an Axidev that install added.
use std::{fs, path::Path};

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting,
    support::{
        current_user, input_access_settings, require_executable, require_programs, verify_packages,
        with_temporary_directory,
    },
};

pub struct AxidevOsk;

const PACKAGES: &[Package] = &[
    Package::Python,
    Package::Pyside6,
    Package::Qt6Wayland,
    Package::LayerShellQt,
    Package::Libinput,
    Package::Systemd,
    Package::Libxkbcommon,
];
const LIFECYCLE: &str = "/usr/local/sbin/axidev-osk-install";
const APP: &str = "/usr/local/bin/axidev-osk";
const PAYLOAD: &str = "/opt/axidev-osk";
const LATEST_RELEASE: &str = "https://github.com/axide-dev/axidev-osk/releases/latest";
const INSTALLER_URL: &str =
    "https://github.com/axide-dev/axidev-osk/releases/latest/download/axidev-osk-install";
/// The account of Plasma Login Manager, which the greeter setup adds to the uinput group.
const GREETER_USER: &str = "plasmalogin";
/// Axidev's uninstall runs the lifecycle installer only when it is still there, because
/// `remove` runs this script without checking. The udev rule link `$2` that the permission
/// setup points at /dev/null stays after Axidev's own uninstall, so the script deletes it.
const UNINSTALL: &str = r#"if [ -x "$0" ]; then "$0" uninstall --user "$1" || exit; fi
if [ "$(readlink "$2")" = /dev/null ]; then rm -f "$2"; fi"#;
const UINPUT_RULE_MASK: &str = "/etc/udev/rules.d/70-axidev-io-uinput.rules";

/// The uinput group and the memberships in it that Axidev's setup creates and its own
/// uninstall leaves.
fn uinput_settings(ctx: &Context, user: &str) -> ModuleResult<Vec<Setting>> {
    let mut users = vec![user.to_owned()];
    if ctx.succeeds(cmd!(ctx.shell, "id -u {GREETER_USER}"))? {
        users.push(GREETER_USER.to_owned());
    }
    let group = || "uinput".to_owned();
    let mut settings = vec![Setting::GroupExists { group: group() }];
    settings.extend(users.into_iter().map(|user| Setting::GroupMember {
        group: group(),
        user,
    }));
    Ok(settings)
}

/// The version of the Axidev payload in `payload`, from the name of its package metadata
/// folder, `lib/python/axidev_osk-<version>.dist-info`.
fn installed_version(payload: &Path) -> ModuleResult<Option<String>> {
    let Ok(entries) = fs::read_dir(payload.join("lib/python")) else {
        return Ok(None);
    };
    for entry in entries {
        let name = entry?.file_name();
        if let Some(version) = name
            .to_string_lossy()
            .strip_prefix("axidev_osk-")
            .and_then(|rest| rest.strip_suffix(".dist-info"))
        {
            return Ok(Some(version.to_owned()));
        }
    }
    Ok(None)
}

/// The version of the latest Axidev release. GitHub redirects the latest release page to
/// the page of its tag, `.../releases/tag/v<version>`.
fn latest_version(ctx: &Context) -> ModuleResult<String> {
    let format = "%{url_effective}";
    let url = ctx.read(cmd!(
        ctx.shell,
        "curl --fail --location --show-error --silent --output /dev/null --write-out {format} {LATEST_RELEASE}"
    ))?;
    match url.rsplit_once("/tag/v") {
        Some((_, version)) if !version.is_empty() => Ok(version.to_owned()),
        _ => Err(format!("could not read the latest Axidev version from {url}").into()),
    }
}

/// Installs Axidev through its lifecycle installer at `lifecycle`, or upgrades the payload in
/// `payload` when a newer release exists, then sets up the application at `app` for the
/// desktop and the login screen. Axidev's upgrade reinstalls even the installed version.
///
/// A fresh install records Axidev's uninstall, which covers its permissions, its autostart
/// entry and the login-screen setup. Before it, the install records the uinput group and
/// memberships that are still missing, so undo removes them after Axidev's uninstall.
pub(crate) fn configure(
    ctx: &Context,
    lifecycle: &Path,
    app: &Path,
    payload: &Path,
) -> ModuleResult {
    let sh = ctx.shell;
    ctx.install_packages(PACKAGES)?;
    require_programs(ctx, &["curl", "sudo"])?;
    let user = current_user(ctx)?;
    if require_executable(lifecycle).is_ok() {
        if installed_version(payload)? != Some(latest_version(ctx)?) {
            ctx.run(cmd!(sh, "sudo {lifecycle} upgrade --user {user}"))?;
        }
    } else {
        for setting in uinput_settings(ctx, &user)? {
            if setting.read(ctx)?.is_none() {
                ctx.record_setting_previous(setting, None)?;
            }
        }
        let lifecycle = lifecycle.to_string_lossy();
        ctx.record_undo_command(
            Path::new("sudo"),
            &["sh", "-c", UNINSTALL, &lifecycle, &user, UINPUT_RULE_MASK],
        )?;
        with_temporary_directory("axidev", |directory| {
            let installer = directory.join("axidev-osk-install");
            ctx.run(cmd!(
                sh,
                "curl --fail --location --show-error --silent --output {installer} {INSTALLER_URL}"
            ))?;
            crate::state::set_executable(&installer, true)?;
            ctx.run(cmd!(sh, "sudo {installer} install --user {user}"))
        })?;
    }
    require_executable(app)?;
    ctx.run(cmd!(sh, "sudo {app} linux setup-permissions --user {user}"))?;
    ctx.run(cmd!(sh, "{app} linux setup-autostart --user {user}"))?;
    // The CachyOS profile runs Plasma Login Manager. Naming it skips the greeter setup's
    // menu, its only question.
    ctx.run(cmd!(
        sh,
        "sudo {app} linux setup-greeter --manager plasma-login"
    ))
}

impl Module for AxidevOsk {
    fn name(&self) -> &'static str {
        "axidev-osk"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, ctx: &Context) -> Footprint {
        // Axidev's own permission setup puts the user in the uinput group too.
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: input_access_settings(ctx),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        configure(
            ctx,
            Path::new(LIFECYCLE),
            Path::new(APP),
            Path::new(PAYLOAD),
        )
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, PACKAGES)?;
        require_executable(Path::new(APP))?;
        let user = current_user(ctx)?;
        ctx.run(cmd!(
            sh,
            "sudo {APP} linux status-permissions --user {user}"
        ))?;
        ctx.run(cmd!(sh, "{APP} linux status-autostart --user {user}"))?;
        ctx.run(cmd!(sh, "sudo {APP} linux status-greeter"))
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

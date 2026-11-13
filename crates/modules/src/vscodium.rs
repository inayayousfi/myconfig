//! VSCodium with its settings, theme and extensions, as the default text editor.
use std::{fs, path::Path};

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting,
    deploy::verify_package,
    support::{
        default_application_settings, require_programs, set_default_application, verify_packages,
        with_temporary_directory,
    },
};

pub struct Vscodium;

const PACKAGES: &[Package] = &[Package::Vscodium, Package::Bsdtar];
const ENTRY: &str = "vscodium.desktop";
const TEXT_TYPES: [&str; 6] = [
    "text/plain",
    "text/markdown",
    "application/json",
    "application/x-yaml",
    "text/x-cmake",
    "application/x-docbook+xml",
];
/// Installed beside the list, so a remote machine opens in the editor.
const REMOTE_SSH: &str = "jeanp413.open-remote-ssh";

fn text_types() -> Vec<String> {
    TEXT_TYPES.iter().map(|kind| (*kind).to_owned()).collect()
}

/// VSCodium forgets extension folders it did not install itself, so the theme goes in as
/// a package: a zip with the extension under `extension/`.
fn install_theme(ctx: &Context, config: &Path) -> ModuleResult {
    let theme = config.join("blacknpink-theme");
    with_temporary_directory("vscodium-theme", |directory| {
        let package = directory.join("blacknpink-theme.vsix");
        let rename = ",^\\.,extension,";
        ctx.run(cmd!(
            ctx.shell,
            "bsdtar --format zip -cf {package} -s {rename} -C {theme} ."
        ))
        .map_err(|error| format!("could not package the VSCodium theme: {error}"))?;
        // Forced, so an edited theme replaces the installed one of the same version.
        ctx.run(cmd!(
            ctx.shell,
            "codium --force --install-extension {package}"
        ))
        .map_err(|error| format!("could not install the VSCodium theme: {error}").into())
    })
}

fn install_extensions(ctx: &Context, config: &Path) -> ModuleResult {
    let list = fs::read_to_string(config.join("extensions.txt"))?;
    let mut arguments = vec!["--install-extension".to_owned(), REMOTE_SSH.to_owned()];
    for extension in list.lines().map(str::trim).filter(|line| !line.is_empty()) {
        arguments.push("--install-extension".to_owned());
        arguments.push(extension.to_owned());
    }
    ctx.run(cmd!(ctx.shell, "codium {arguments...}"))
        .map_err(|error| format!("could not install the VSCodium extensions: {error}").into())
}

fn installed_extensions(ctx: &Context) -> ModuleResult<Vec<String>> {
    Ok(ctx
        .read(cmd!(ctx.shell, "codium --list-extensions"))?
        .lines()
        .map(str::to_owned)
        .collect())
}

impl Module for Vscodium {
    fn name(&self) -> &'static str {
        "vscodium"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base"), Need::Hard("plasma-version")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: default_application_settings(&text_types()),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(PACKAGES)?;
        require_programs(ctx, &["codium", "bsdtar"])?;
        ctx.deploy_config("vscodium")?;
        set_default_application(ctx, ENTRY, &text_types())
            .map_err(|error| format!("could not make VSCodium the default text editor: {error}"))?;
        let extensions = ctx.home.join(".vscode-oss");
        if !extensions.exists() {
            ctx.created(&extensions, false)?;
        }
        let config = ctx.home.join(".config/VSCodium");
        let before = installed_extensions(ctx)?;
        let result = install_theme(ctx, &config).and_then(|()| install_extensions(ctx, &config));
        // Only codium knows the identifier of each extension it installs, so the added ones
        // come from comparing its lists, even after a failed install.
        for extension in installed_extensions(ctx)? {
            if !before.contains(&extension) {
                ctx.record_setting_previous(Setting::VscodiumExtension { id: extension }, None)?;
            }
        }
        result
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        verify_package(ctx, "vscodium")?;
        for setting in default_application_settings(&text_types()) {
            if setting.read(ctx)?.as_deref() != Some(ENTRY) {
                return Err(
                    format!("VSCodium is not the default for {}", setting.describe()).into(),
                );
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

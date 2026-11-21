//! VSCodium with its settings, theme and extensions, as the default text editor.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

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
/// The entry that the AUR package `vscodium-bin` installs.
const ENTRY: &str = "codium.desktop";
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

/// The files under `folder`, by their path relative to it.
fn files_under(folder: &Path) -> ModuleResult<BTreeMap<PathBuf, Vec<u8>>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.insert(path.strip_prefix(folder)?.to_path_buf(), fs::read(&path)?);
            }
        }
    }
    Ok(files)
}

/// Whether VSCodium lists the theme in `theme` at its version, in a folder that holds the
/// same files. VSCodium adds a `__metadata` block with the install time to the installed
/// `package.json`, so that file is compared without it.
fn theme_is_installed(ctx: &Context, theme: &Path) -> ModuleResult<bool> {
    let mut source = files_under(theme)?;
    let manifest = Path::new("package.json");
    let package: serde_json::Value =
        serde_json::from_slice(source.remove(manifest).as_deref().unwrap_or_default())?;
    let field = |name: &str| package.get(name).and_then(serde_json::Value::as_str);
    let (Some(publisher), Some(name), Some(version)) =
        (field("publisher"), field("name"), field("version"))
    else {
        return Err(
            "the VSCodium theme's package.json lacks its publisher, name or version".into(),
        );
    };
    let id = format!("{publisher}.{name}");
    let folder_name = format!("{id}-{version}");
    let extensions = ctx.home.join(".vscode-oss/extensions");
    let Ok(listed) = fs::read(extensions.join("extensions.json")) else {
        return Ok(false);
    };
    let listed: serde_json::Value = serde_json::from_slice(&listed)?;
    let registered = listed.as_array().is_some_and(|entries| {
        entries.iter().any(|entry| {
            entry
                .pointer("/identifier/id")
                .and_then(serde_json::Value::as_str)
                == Some(&id)
                && entry.get("version").and_then(serde_json::Value::as_str) == Some(version)
                && entry
                    .get("relativeLocation")
                    .and_then(serde_json::Value::as_str)
                    == Some(&folder_name)
        })
    });
    let folder = extensions.join(&folder_name);
    if !registered || !folder.is_dir() {
        return Ok(false);
    }
    let mut installed = files_under(&folder)?;
    let Some(installed_manifest) = installed.remove(manifest) else {
        return Ok(false);
    };
    let mut installed_package: serde_json::Value = serde_json::from_slice(&installed_manifest)?;
    if let Some(object) = installed_package.as_object_mut() {
        object.remove("__metadata");
    }
    Ok(installed_package == package && installed == source)
}

/// VSCodium forgets extension folders it did not install itself, so the theme goes in as
/// a package: a zip with the extension under `extension/`.
fn install_theme(ctx: &Context, config: &Path) -> ModuleResult {
    let theme = config.join("blacknpink-theme");
    if theme_is_installed(ctx, &theme)? {
        return Ok(());
    }
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
                let setting = Setting::VscodiumExtension {
                    id: extension,
                    editor: "codium".to_owned(),
                };
                ctx.record_setting_previous(setting, None)?;
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

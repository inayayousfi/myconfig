//! VSCodium and VS Code settings, theme and extensions, and the editor that Git and other
//! programs open files with.
use std::path::{Path, PathBuf};

use embedded_dotfiles::VSCODIUM;
use typed_fs_rs::EmbeddedDirectory;
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting,
    support::{copy_embedded, refresh_windows_path, verify_embedded, with_temporary_directory},
};

pub struct WindowsEditor;

struct Editor {
    name: &'static str,
    command: &'static str,
    /// The settings folder under `%APPDATA%`.
    user: &'static str,
    /// The editor's own remote SSH extension.
    remote: &'static str,
}

const EDITORS: [Editor; 2] = [
    Editor {
        name: "VSCodium",
        command: "codium",
        user: "VSCodium/User",
        remote: "jeanp413.open-remote-ssh",
    },
    Editor {
        name: "VS Code",
        command: "code",
        user: "Code/User",
        remote: "ms-vscode-remote.remote-ssh",
    },
];

const PACKAGE: &str = ".config/VSCodium";

fn app_data(ctx: &Context) -> ModuleResult<PathBuf> {
    Ok(PathBuf::from(
        ctx.shell.var_os("APPDATA").ok_or("APPDATA is unset")?,
    ))
}

fn editor_settings() -> Setting {
    Setting::GitConfig {
        key: "core.editor".to_owned(),
    }
}

fn environment(name: &str) -> Setting {
    Setting::UserEnvironment {
        name: name.to_owned(),
    }
}

/// The theme's extension identifier, `publisher.name` from its manifest.
fn theme_id() -> ModuleResult<String> {
    let manifest: serde_json::Value = serde_json::from_slice(
        VSCODIUM
            ._config
            .VSCodium
            .blacknpink_theme
            .package_json
            .content,
    )?;
    let field = |name: &str| {
        manifest[name]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("the theme manifest has no {name}"))
    };
    Ok(format!("{}.{}", field("publisher")?, field("name")?))
}

fn installed_extensions(ctx: &Context, program: &Path) -> ModuleResult<Vec<String>> {
    Ok(ctx
        .read(cmd!(ctx.shell, "{program} --list-extensions"))?
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect())
}

fn record_extension(ctx: &Context, editor: &Editor, id: &str) -> ModuleResult {
    let setting = Setting::VscodiumExtension {
        id: id.to_owned(),
        editor: editor.command.to_owned(),
    };
    ctx.record_setting_previous(setting, None)
}

/// Records each extension that was missing before installing it. An extension can bring
/// others with it, and only the editor knows them, so the ones added beyond the list are
/// recorded from its list afterwards, even after a failed install.
fn install_theme_and_extensions(ctx: &Context, editor: &Editor, program: &Path) -> ModuleResult {
    let list =
        String::from_utf8_lossy(VSCODIUM._config.VSCodium.extensions_txt.content).into_owned();
    let extensions: Vec<&str> = std::iter::once(editor.remote)
        .chain(list.lines().map(str::trim).filter(|line| !line.is_empty()))
        .collect();
    let before = installed_extensions(ctx, program)?;
    let missing = |id: &str| !before.iter().any(|known| known.eq_ignore_ascii_case(id));
    // The theme is installed forced, so it counts as added when it was missing.
    let theme = theme_id()?;
    for id in std::iter::once(theme.as_str()).chain(extensions.iter().copied()) {
        if missing(id) {
            record_extension(ctx, editor, id)?;
        }
    }
    let result = install_package_and_list(ctx, editor, program, &extensions);
    for id in installed_extensions(ctx, program)? {
        let listed = std::iter::once(theme.as_str())
            .chain(extensions.iter().copied())
            .any(|wanted| wanted.eq_ignore_ascii_case(&id));
        if !listed && missing(&id) {
            record_extension(ctx, editor, &id)?;
        }
    }
    result
}

/// The editors forget extension folders they did not install themselves, so the theme goes
/// in as a package: a zip with the extension under `extension\`.
fn install_package_and_list(
    ctx: &Context,
    editor: &Editor,
    program: &Path,
    extensions: &[&str],
) -> ModuleResult {
    let sh = ctx.shell;
    let source = Path::new(PACKAGE).join("blacknpink-theme");
    with_temporary_directory("editor-theme", |directory| {
        let extension = directory.join("extension");
        for file in VSCODIUM._config.VSCodium.blacknpink_theme.files() {
            let path = extension.join(Path::new(file.path_from_root).strip_prefix(&source)?);
            std::fs::create_dir_all(path.parent().ok_or("theme file has no folder")?)?;
            std::fs::write(path, file.content)?;
        }
        let package = directory.join("blacknpink-theme.vsix");
        // tar.exe picks its format from the file suffix, and it does not know .vsix.
        ctx.run(cmd!(
            sh,
            "tar.exe --format zip -cf {package} -C {directory} extension"
        ))?;
        // Forced, so an edited theme replaces the installed one of the same version.
        ctx.run(cmd!(sh, "{program} --force --install-extension {package}"))
            .map_err(|error| format!("{} could not install the theme: {error}", editor.name))?;
        Ok(())
    })?;
    let mut arguments = Vec::new();
    for extension in extensions {
        arguments.push("--install-extension");
        arguments.push(*extension);
    }
    ctx.run(cmd!(sh, "{program} {arguments...}"))
        .map_err(|error| {
            format!("{} could not install its extensions: {error}", editor.name).into()
        })
}

impl Module for WindowsEditor {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Soft(crate::PackageGroup::DEV_TOOLS.name)]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Vscodium],
            settings: vec![
                environment("EDITOR"),
                environment("VISUAL"),
                editor_settings(),
            ],
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        // A winget install earlier in this run changes the stored PATH, not this process's.
        refresh_windows_path(ctx)?;
        let app_data = app_data(ctx)?;
        let user = Path::new(PACKAGE).join("User");
        // --wait keeps Git and other programs waiting until the file is closed.
        let mut chosen = None;
        for editor in &EDITORS {
            copy_embedded(
                ctx,
                VSCODIUM._config.VSCodium.User.files(),
                &user,
                &app_data.join(editor.user),
            )?;
            let Ok(program) = ctx.find_program(editor.command) else {
                ctx.note(&format!(
                    "{} not found; skipping {} theme and extensions",
                    editor.command, editor.name
                ));
                continue;
            };
            install_theme_and_extensions(ctx, editor, &program)?;
            chosen.get_or_insert(format!("{} --wait", editor.command));
        }
        let chosen = chosen.unwrap_or_else(|| "notepad".to_owned());
        ctx.set(environment("EDITOR"), &chosen)?;
        ctx.set(environment("VISUAL"), &chosen)?;
        ctx.set(editor_settings(), &chosen)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let app_data = app_data(ctx)?;
        let user = Path::new(PACKAGE).join("User");
        for editor in &EDITORS {
            verify_embedded(
                VSCODIUM._config.VSCodium.User.files(),
                &user,
                &app_data.join(editor.user),
            )?;
        }
        if editor_settings().read(ctx)?.is_none() {
            return Err("Git has no editor".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

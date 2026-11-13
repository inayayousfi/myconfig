//! VSCodium and VS Code settings, theme and extensions, and the editor that Git and other
//! programs open files with.
use std::path::{Path, PathBuf};

use embedded_dotfiles::DOTFILES;
use typed_fs_rs::EmbeddedDirectory;
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting,
    support::{copy_embedded, verify_embedded, with_temporary_directory},
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

const PACKAGE: &str = "vscodium/.config/VSCodium";

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

/// The editors forget extension folders they did not install themselves, so the theme goes
/// in as a package: a zip with the extension under `extension\`.
fn install_theme_and_extensions(ctx: &Context, editor: &Editor, program: &Path) -> ModuleResult {
    let sh = ctx.shell;
    let source = Path::new(PACKAGE).join("blacknpink-theme");
    with_temporary_directory("editor-theme", |directory| {
        let extension = directory.join("extension");
        for file in DOTFILES.vscodium._config.VSCodium.blacknpink_theme.files() {
            let path = extension.join(Path::new(file.path_from_root).strip_prefix(&source)?);
            std::fs::create_dir_all(path.parent().ok_or("theme file has no folder")?)?;
            std::fs::write(path, file.content)?;
        }
        let package = directory.join("blacknpink-theme.vsix");
        ctx.run(cmd!(
            sh,
            "tar.exe -a -cf {package} -C {directory} extension"
        ))?;
        // Forced, so an edited theme replaces the installed one of the same version.
        ctx.run(cmd!(sh, "{program} --force --install-extension {package}"))
            .map_err(|error| format!("{} could not install the theme: {error}", editor.name))?;
        Ok(())
    })?;
    let list = String::from_utf8_lossy(DOTFILES.vscodium._config.VSCodium.extensions_txt.content)
        .into_owned();
    let mut arguments = vec!["--install-extension".to_owned(), editor.remote.to_owned()];
    for extension in list.lines().map(str::trim).filter(|line| !line.is_empty()) {
        arguments.push("--install-extension".to_owned());
        arguments.push(extension.to_owned());
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
        let app_data = app_data(ctx)?;
        let user = Path::new(PACKAGE).join("User");
        // --wait keeps Git and other programs waiting until the file is closed.
        let mut chosen = None;
        for editor in &EDITORS {
            copy_embedded(
                ctx,
                DOTFILES.vscodium._config.VSCodium.User.files(),
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
                DOTFILES.vscodium._config.VSCodium.User.files(),
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

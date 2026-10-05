//! Windows: copies of the Emacs config into the home directory Emacs reports.
use std::path::{Path, PathBuf};

use embedded_dotfiles::DOTFILES;
use typed_fs_rs::EmbeddedDirectory;
use xshell::cmd;

use super::EmacsOptions;
use crate::{
    Context, Footprint, Module, ModuleResult, Package,
    support::{copy_embedded, verify_embedded},
};

pub struct EmacsCopied(pub EmacsOptions);

const SOURCE: &str = "emacs/.config/emacs";

/// The home directory as Emacs itself expands `~/`, which differs from USERPROFILE on Windows.
fn emacs_home(ctx: &Context) -> ModuleResult<PathBuf> {
    let emacs = ctx.find_program("emacs.exe")?;
    let expression = r#"(princ (expand-file-name "~/"))"#;
    let home = ctx.read(cmd!(ctx.shell, "{emacs} --batch -Q --eval {expression}"))?;
    let home = home.trim();
    if home.is_empty() {
        return Err("Emacs did not return its home directory".into());
    }
    Ok(PathBuf::from(home))
}

impl Module for EmacsCopied {
    fn name(&self) -> &'static str {
        "emacs"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::EmacsWayland],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        if self.0.browser_terminal_firewall {
            return Err("the browser terminal firewall is managed only on Linux".into());
        }
        let home = emacs_home(ctx)?;
        let destination = home.join(".config/emacs");
        copy_embedded(ctx, DOTFILES.emacs.files(), Path::new(SOURCE), &destination)?;

        // A loader you wrote yourself stays untouched.
        let loader = home.join(".emacs");
        if std::fs::symlink_metadata(&loader).is_err() {
            let path = |name: &str| destination.join(name).to_string_lossy().replace('\\', "/");
            let contents = format!(
                "(load-file \"{}\")\n(load-file \"{}\")\n",
                path("early-init.el"),
                path("init.el")
            );
            ctx.write_file(&loader, contents.as_bytes(), false)?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let home = emacs_home(ctx)?;
        verify_embedded(
            DOTFILES.emacs.files(),
            Path::new(SOURCE),
            &home.join(".config/emacs"),
        )?;
        if !home.join(".emacs").is_file() {
            return Err("the ~/.emacs loader is missing".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

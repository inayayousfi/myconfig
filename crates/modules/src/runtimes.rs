//! Language runtimes and build tools.
use std::path::Path;

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting,
    support::{require_executable, verify_packages},
};

pub struct Runtimes {
    /// Installs Bun with its own installer, for systems that have no Bun package.
    pub bun_installer: bool,
}

const PACKAGES: &[Package] = &[
    Package::Go,
    Package::Python,
    Package::PythonVenv,
    Package::Jdk,
    Package::Maven,
    Package::Llvm,
    Package::Make,
    Package::Cmake,
    // npm depends on node-gyp, and remove uninstalls the packages of one install in reverse
    // order, so npm goes first.
    Package::Nodejs,
    Package::NodeGyp,
    Package::Npm,
];

impl Runtimes {
    fn packages(&self) -> Vec<Package> {
        let mut packages = PACKAGES.to_vec();
        if !self.bun_installer {
            packages.push(Package::Bun);
        }
        packages
    }
}

/// Bun's installer puts it in `~/.bun`, where the agent module expects it, and
/// `bun upgrade` updates it later.
fn install_bun(ctx: &Context) -> ModuleResult {
    let sh = ctx.shell;
    let directory = ctx.home.join(".bun");
    if directory.join("bin/bun").is_file() {
        return Ok(());
    }
    let installer = ctx
        .read(cmd!(sh, "curl -fsSL https://bun.sh/install"))
        .map_err(|error| format!("could not download the Bun installer: {error}"))?;
    // Without a known shell the installer leaves the shell files alone; the deployed Zsh
    // configuration already puts ~/.bun/bin on PATH.
    ctx.run(
        cmd!(sh, "bash -c {installer}")
            .env("BUN_INSTALL", &directory)
            .env("SHELL", "/bin/sh"),
    )
    .map_err(|error| format!("the Bun installer failed: {error}"))?;
    require_executable(&directory.join("bin/bun"))
}

impl Module for Runtimes {
    fn name(&self) -> &'static str {
        "runtimes"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: [self.packages(), vec![Package::Rustup]].concat(),
            settings: vec![Setting::RustupDefault],
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        // Bun keeps its global packages, manifest and cache in ~/.bun, beside packages that
        // people add later, so remove asks before deleting it. Bun's installer creates it
        // here, and a packaged Bun creates it on first use, so this module records it on
        // every profile.
        let bun = ctx.home.join(".bun");
        if !bun.exists() {
            ctx.created_user_data(&bun)?;
        }
        ctx.install_packages(&self.packages())?;
        if self.bun_installer {
            install_bun(ctx)?;
        }
        // Selecting stable downloads the whole toolchain when it is missing.
        let (_, toolchains) = ctx.read_unchecked(cmd!(ctx.shell, "rustup toolchain list"))?;
        if !toolchains.lines().any(|line| line.starts_with("stable-")) {
            ctx.record_undo_command(Path::new("rustup"), &["toolchain", "uninstall", "stable"])?;
        }
        ctx.set(Setting::RustupDefault, "stable")
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &self.packages())?;
        if self.bun_installer {
            require_executable(&ctx.home.join(".bun/bin/bun"))?;
        }
        if Setting::RustupDefault.read(ctx)?.is_none() {
            return Err("rustup has no default toolchain".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

//! Coding agents, with the packages and browser tools they use.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package,
    support::{agent_tool_path, require_executable, verify_packages},
};

pub struct AgentsPackages {
    /// Packages only this profile needs, such as `ydotool` or the WSL SSH agent.
    pub extra: &'static [Package],
}

const PACKAGES: &[Package] = &[
    Package::Lsof,
    Package::AtSpi2Core,
    Package::Libxcomposite,
    Package::Libxdamage,
    Package::Libxrandr,
    Package::Libxkbcommon,
];

/// Global Bun packages, with the command each one installs.
const BUN_PACKAGES: [(&str, &str); 2] = [
    ("@playwright/mcp", "playwright-mcp"),
    ("@mariozechner/pi-coding-agent", "pi"),
];

/// Anthropic's installer puts claude in ~/.local/bin, and Claude Code updates itself.
fn install_claude(ctx: &Context) -> ModuleResult {
    let sh = ctx.shell;
    let claude = ctx.home.join(".local/bin/claude");
    if claude.exists() {
        return Ok(());
    }
    let installer = ctx
        .read(cmd!(sh, "curl -fsSL https://claude.ai/install.sh"))
        .map_err(|error| format!("could not download the Claude Code installer: {error}"))?;
    ctx.created(&claude, false)?;
    // The installer keeps the real binaries there, and ~/.local/bin/claude links to them.
    let versions = ctx.home.join(".local/share/claude");
    if !versions.exists() {
        ctx.created(&versions, false)?;
    }
    ctx.run(cmd!(sh, "bash -c {installer}"))
        .map_err(|error| format!("the Claude Code installer failed: {error}"))?;
    require_executable(&claude)
}

impl AgentsPackages {
    fn packages(&self) -> Vec<Package> {
        [PACKAGES, self.extra].concat()
    }

    fn playwright_cli(ctx: &Context) -> std::path::PathBuf {
        ctx.home
            .join(".bun/install/global/node_modules/.bin/playwright")
    }
}

impl Module for AgentsPackages {
    fn name(&self) -> &'static str {
        "agents-packages"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base"), Need::Hard("runtimes")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: [self.packages(), vec![Package::Bun]].concat(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        ctx.install_packages(&self.packages())?;
        let bun = ctx.home.join(".bun");
        let _bun = sh.push_env("BUN_INSTALL", &bun);
        let _path = sh.push_env("PATH", agent_tool_path(ctx)?);
        install_claude(ctx)?;
        let mut packages = Vec::new();
        for (package, command) in BUN_PACKAGES {
            let directory = bun.join("install/global/node_modules").join(package);
            if !directory.exists() {
                ctx.created(&directory, false)?;
                ctx.created(&bun.join("bin").join(command), false)?;
            }
            packages.push(format!("{package}@latest"));
        }
        ctx.run(cmd!(sh, "bun add --global {packages...}"))?;
        require_executable(&bun.join("bin/pi"))?;

        let playwright_cli = Self::playwright_cli(ctx);
        require_executable(&playwright_cli)?;
        let browsers = ctx.home.join(".cache/ms-playwright");
        if !browsers.exists() {
            ctx.created(&browsers, false)?;
        }
        ctx.run(cmd!(sh, "{playwright_cli} install --only-shell chromium"))
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, &self.packages())?;
        let bun = ctx.home.join(".bun");
        let _bun = sh.push_env("BUN_INSTALL", &bun);
        let _path = sh.push_env("PATH", agent_tool_path(ctx)?);
        require_executable(&Self::playwright_cli(ctx))?;
        let module = bun.join("install/global/node_modules/playwright");
        let script = "const { chromium } = require(process.env.INAYOS_PLAYWRIGHT_MODULE); const browser = await chromium.launch({ headless: true }); await browser.close();";
        ctx.run(cmd!(sh, "bun -e {script}").env("INAYOS_PLAYWRIGHT_MODULE", module))
    }

    fn remove(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let bun = ctx.home.join(".bun");
        // Bun also lists each package in its global manifest, so let Bun take them out.
        let packages: Vec<&str> = BUN_PACKAGES
            .iter()
            .map(|(package, _)| *package)
            .filter(|package| {
                let directory = bun.join("install/global/node_modules").join(package);
                ctx.was_created(&directory) && directory.exists()
            })
            .collect();
        if packages.is_empty() {
            return Ok(());
        }
        let _bun = sh.push_env("BUN_INSTALL", &bun);
        let _path = sh.push_env("PATH", agent_tool_path(ctx)?);
        ctx.run(cmd!(sh, "bun remove --global {packages...}"))
    }
}

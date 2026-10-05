//! Packages and browser tools that coding agents use.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package,
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
        let playwright_mcp = bun.join("install/global/node_modules/@playwright/mcp");
        if !playwright_mcp.exists() {
            ctx.created(&playwright_mcp, false)?;
            ctx.created(&bun.join("bin/playwright-mcp"), false)?;
        }
        ctx.run(cmd!(sh, "bun add --global @playwright/mcp@latest"))?;

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
        let script = "const { chromium } = require(process.env.MYCONFIG_PLAYWRIGHT_MODULE); const browser = await chromium.launch({ headless: true }); await browser.close();";
        ctx.run(cmd!(sh, "bun -e {script}").env("MYCONFIG_PLAYWRIGHT_MODULE", module))
    }

    fn remove(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let bun = ctx.home.join(".bun");
        let playwright_mcp = bun.join("install/global/node_modules/@playwright/mcp");
        // Bun also lists the package in its global manifest, so let Bun take it out.
        if !ctx.was_created(&playwright_mcp) || !playwright_mcp.exists() {
            return Ok(());
        }
        let _bun = sh.push_env("BUN_INSTALL", &bun);
        let _path = sh.push_env("PATH", agent_tool_path(ctx)?);
        ctx.run(cmd!(sh, "bun remove --global @playwright/mcp"))
    }
}

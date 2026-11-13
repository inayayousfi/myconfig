//! The `~/environment.md` inventory that agents read and keep up to date.
use std::fs;

use crate::{Context, Footprint, Module, ModuleResult};

const INVENTORY_START: &str =
    "# Environment\n\nThis file describes the capabilities installed for the ";
const INVENTORY_RUNTIMES: &str = ".\n\n## Development environment\n\n\
- **Shell**: Zsh with Oh My Zsh and the shared Black & Pink configuration.\n\
- **Runtimes**: Rust, Go, Bun, Node.js, Python, Java, LLVM, Make, and CMake.\n\
- **Repository tools**: Git, GitHub CLI, and GNU Stow.\n\
- **Terminal tools**: ";
const INVENTORY_AGENTS: &str = ".\n\
- **Coding agents**: Claude Code, installed by Anthropic's installer in `~/.local/bin/claude` and updated by itself. Pi, installed globally by Bun in `~/.bun/bin/pi`.\n\
- **Agent browser tools**: Playwright MCP.\n\
- **Claude configuration**: `claude-config-helper` trusts projects, marks Claude's first-run setup as finished, lists or clears saved approvals, applies the MCP servers listed in `~/.config/claude-config-helper/mcp-servers.json`, and checks files for tokens, email addresses and home paths.\n\
- **Remote access**: OpenSSH server and Tailscale service.\n\
- **T3 Code**: ";
const INVENTORY_END: &str = "\nAll selected installer modules completed successfully.\n";

const T3_CODE_SERVER: &str = "the `t3` server from npm in `~/.local/bin/t3`, run at boot by the user service `t3code.service` on `127.0.0.1:3773`. `t3ctl status`, `remote`, `update` and `repair` manage it; `remote` turns T3 Connect or the Tailscale route (HTTPS port 10000) on or off. Both routes start off.";
const T3_CODE_DESKTOP: &str = "the desktop app (AUR package `t3code-bin`), which runs its own server on `127.0.0.1:3773` while it is open. Its CLI is `~/.t3/bin/t3`. `~/.t3/userdata/client-settings.json` links into the deployed `t3code-desktop` package.";

const CACHYOS_INVENTORY: &str = "- **Editor**: VSCodium with the Black & Pink theme and Open Remote SSH. It opens text files from the desktop, and `EDITOR` is `codium --wait`.\n\
- **On-screen keyboard**: Axidev OSK with desktop and login-screen startup.\n\
- **Keyboard remapping**: Kanata keyboard remapping with a KDE tray profile selector.\n\
- **Dictation**: Handy offline push-to-talk dictation on Ctrl+Space.\n\
- **KDE Plasma**: Breeze Dark with a pink accent color, a top bar holding a session menu and the global menu, and an application dock on every display, for KDE Plasma 6.7 through 6.x.\n\
- **Desktop automation**: ydotool with a persistent user service for virtual keyboard and pointer input.\n";
const ARCH_WSL_INVENTORY: &str = "- **Editor**: This profile does not install an editor. When Windows VSCodium is installed, `EDITOR` is `~/.local/bin/windows-codium`, which opens files in it and waits until they are closed. Otherwise the shell uses an available Vim or Vi fallback.\n";
const UBUNTU_DEVBOX_INVENTORY: &str = "- **Editor**: This profile does not install an editor. VSCodium and VS Code open folders here over SSH with their remote SSH extensions. The shell uses an available Vim or Vi.\n\
- **Containers**: rootless Docker with Buildx and Compose. The Docker daemon runs as the user service `docker.service` and the `rootless` Docker context is selected.\n\
- **Package sources**: Tailscale, GitHub CLI, Yazi and Docker come from their vendors' apt repositories. Bun is in `~/.bun` and updates with `bun upgrade`.\n";

pub struct EnvironmentInventory {
    /// The platform sentence, such as "CachyOS development workstation".
    pub platform: &'static str,
    /// The terminal tools, as a list in a sentence.
    pub terminal_tools: &'static str,
    /// How T3 Code runs on this profile.
    pub t3_code: &'static str,
    /// Profile-specific lines added after the shared development environment.
    pub extra: &'static str,
}

impl EnvironmentInventory {
    pub const CACHYOS: Self = Self {
        platform: "CachyOS development workstation",
        terminal_tools: "Yazi, ripgrep, and jq",
        t3_code: T3_CODE_DESKTOP,
        extra: CACHYOS_INVENTORY,
    };

    pub const ARCH_WSL: Self = Self {
        platform: "Arch WSL development environment",
        terminal_tools: "Yazi, ripgrep, jq, and btop",
        t3_code: T3_CODE_SERVER,
        extra: ARCH_WSL_INVENTORY,
    };

    pub const UBUNTU_DEVBOX: Self = Self {
        platform: "Ubuntu development machine for coding and long-running agents",
        terminal_tools: "Yazi, ripgrep, jq, and btop",
        t3_code: T3_CODE_SERVER,
        extra: UBUNTU_DEVBOX_INVENTORY,
    };

    pub(crate) fn template(&self) -> String {
        [
            INVENTORY_START,
            self.platform,
            INVENTORY_RUNTIMES,
            self.terminal_tools,
            INVENTORY_AGENTS,
            self.t3_code,
            "\n",
            self.extra,
            INVENTORY_END,
        ]
        .concat()
    }
}

impl Module for EnvironmentInventory {
    fn name(&self) -> &'static str {
        "environment-inventory"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        // Agents maintain an existing inventory; only a new machine gets the template.
        let path = ctx.home.join("environment.md");
        if path.exists() {
            ctx.note("Keeping the existing environment inventory");
            return Ok(());
        }
        ctx.created_user_data(&path)?;
        fs::write(path, self.template())?;
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        if !ctx.home.join("environment.md").is_file() {
            return Err("~/environment.md is missing".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

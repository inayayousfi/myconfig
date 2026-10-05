//! The `~/environment.md` inventory that agents read and keep up to date.
use std::fs;

use crate::{Context, Footprint, Module, ModuleResult};

const INVENTORY_START: &str =
    "# Environment\n\nThis file describes the capabilities installed for the ";
const INVENTORY_COMMON: &str = "\n\n## Development environment\n\n\
- **Shell**: Zsh with Oh My Zsh and the shared Black & Pink configuration.\n\
- **Runtimes**: Rust, Go, Bun, Node.js, Python, Java, LLVM, Make, and CMake.\n\
- **Repository tools**: Git, GitHub CLI, and GNU Stow.\n\
- **Terminal tools**: Yazi, ripgrep, jq, and btop.\n\
- **Agent browser tools**: Playwright MCP. Claude Code and Pi are installed separately.\n\
- **Claude configuration**: `claude-config-helper` trusts projects, lists or clears saved approvals, applies the MCP servers listed in `~/.config/claude-config-helper/mcp-servers.json`, and checks files for tokens, email addresses and home paths.\n\
- **Remote access**: OpenSSH server and Tailscale service.\n";
const CACHYOS_INVENTORY: &str = "- **Editor and terminal Atelier**: Native Wayland graphical Emacs with restorable workspace layouts, native buffers, splits, libghostty-powered Ghostel terminals, Git review, and workspace-owned AIPanel coding agents. `M-x remot-set-password` enables its plaintext private-LAN browser terminal at `http://HOSTNAME.local:18080`. Closing its final graphical frame ends the process; reopening starts fresh recorded jobs.\n\
- **On-screen keyboard**: Axidev OSK with desktop and login-screen startup.\n\
- **Keyboard remapping**: Kanata keyboard remapping with a KDE tray profile selector.\n\
- **Dictation**: Handy offline push-to-talk dictation on Ctrl+Space.\n\
- **KDE Plasma**: Black & Pink panels and application dock for KDE Plasma 6.7 through 6.x.\n\
- **Desktop automation**: ydotool with a persistent user service for virtual keyboard and pointer input.\n";
const ARCH_WSL_INVENTORY: &str = "- **Editor**: This profile does not install an editor. The shell uses an available Vim or Vi fallback; Neovim, tmux, Lazygit, and Hunk are removed.\n";
const INVENTORY_END: &str = "\nAll selected installer modules completed successfully.\n";

pub struct EnvironmentInventory {
    /// The platform sentence, such as "CachyOS development workstation".
    pub platform: &'static str,
    /// Profile-specific lines added after the shared development environment.
    pub extra: &'static str,
}

impl EnvironmentInventory {
    pub const CACHYOS: Self = Self {
        platform: "CachyOS development workstation",
        extra: CACHYOS_INVENTORY,
    };

    pub const ARCH_WSL: Self = Self {
        platform: "Arch WSL development environment",
        extra: ARCH_WSL_INVENTORY,
    };

    pub(crate) fn template(&self) -> String {
        [
            INVENTORY_START,
            self.platform,
            ".",
            INVENTORY_COMMON,
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

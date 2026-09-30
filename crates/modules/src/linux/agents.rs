use std::fs;

use crate::{ModuleContext, ModuleResult, Profile};

const INVENTORY_START: &str =
    "# Environment\n\nThis file describes the capabilities installed for the ";
const INVENTORY_COMMON: &str = "\n\n## Development environment\n\n\
- **Shell**: Zsh with Oh My Zsh and the shared Black & Pink configuration.\n\
- **Runtimes**: Rust, Go, Bun, Node.js, Python, Java, LLVM, Make, and CMake.\n\
- **Repository tools**: Git, GitHub CLI, and GNU Stow.\n\
- **Terminal tools**: Yazi, ripgrep, fd, fzf, zoxide, eza, bat, jq, and btop.\n\
- **Agent tools**: OpenCode and Playwright MCP.\n\
- **Remote access**: OpenSSH server and Tailscale service with optional login during setup.\n";
const CACHYOS_INVENTORY: &str = "- **Editor and terminal Atelier**: Native Wayland graphical Emacs with restorable workspace layouts, native buffers, splits, libghostty-powered Ghostel terminals, Git review, and workspace-owned AIPanel coding agents. `M-x remot-set-password` enables its plaintext private-LAN browser terminal at `http://HOSTNAME.local:18080`. Closing its final graphical frame ends the process; reopening starts fresh recorded jobs.\n\
- **On-screen keyboard**: Axidev OSK with desktop and login-screen startup.\n\
- **Keyboard remapping**: Kanata keyboard remapping with a KDE tray profile selector.\n\
- **Dictation**: Handy offline push-to-talk dictation on Ctrl+Space.\n\
- **KDE Plasma**: Black & Pink panels and application dock for KDE Plasma 6.7 through 6.x.\n\
- **Desktop automation**: ydotool with a persistent user service for virtual keyboard and pointer input.\n";
const ARCH_WSL_INVENTORY: &str = "- **Editor**: Unconfigured Neovim is retained as the shell editor; shared Neovim, tmux, Lazygit, and Hunk dotfiles are retired.\n";
const INVENTORY_END: &str = "\nAll selected installer modules completed successfully.\n";

pub trait EnvironmentInventoryModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}

pub struct CachyosEnvironmentInventory;
pub struct ArchWslEnvironmentInventory;

fn write_environment_inventory(context: &ModuleContext<'_>) -> ModuleResult {
    let (platform, extra) = match context.profile {
        Profile::Cachyos => ("CachyOS development workstation", CACHYOS_INVENTORY),
        Profile::ArchWsl => ("Arch WSL development environment", ARCH_WSL_INVENTORY),
        _ => return Err("environment inventory requires CachyOS or Arch WSL".into()),
    };
    let inventory = [
        INVENTORY_START,
        platform,
        ".",
        INVENTORY_COMMON,
        extra,
        INVENTORY_END,
    ]
    .concat();
    fs::write(context.home.join("environment.md"), inventory)?;
    Ok(())
}

impl EnvironmentInventoryModule for CachyosEnvironmentInventory {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Cachyos {
            return Err("CachyosEnvironmentInventory requires CachyOS".into());
        }
        write_environment_inventory(context)
    }
}

impl EnvironmentInventoryModule for ArchWslEnvironmentInventory {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::ArchWsl {
            return Err("ArchWslEnvironmentInventory requires Arch WSL".into());
        }
        write_environment_inventory(context)
    }
}

use std::fs;

use crate::{ModuleContext, ModuleResult, Profile};

const INVENTORY_START: &str =
    "# Environment\n\nThis file describes the capabilities installed for the ";
const INVENTORY_COMMON: &str = "\n\n## Development environment\n\n\
- **Shell**: Zsh with Oh My Zsh and the shared Black & Pink configuration.\n\
- **Runtimes**: Rust, Go, Bun, Node.js, Python, Java, LLVM, Make, and CMake.\n\
- **Repository tools**: Git, GitHub CLI, and GNU Stow.\n\
- **Terminal tools**: Yazi, ripgrep, jq, and btop.\n\
- **Agent browser tools**: Playwright MCP. Claude Code and Pi are installed separately.\n\
- **Claude configuration**: `claude-config-helper` trusts projects, lists or clears saved approvals, applies the MCP servers listed in `~/.config/claude-config-helper/mcp-servers.json`, and checks files for tokens, email addresses and home paths.\n\
- **Remote access**: OpenSSH server and Tailscale service with optional login during setup.\n";
const CACHYOS_INVENTORY: &str = "- **Editor and terminal Atelier**: Native Wayland graphical Emacs with restorable workspace layouts, native buffers, splits, libghostty-powered Ghostel terminals, Git review, and workspace-owned AIPanel coding agents. `M-x remot-set-password` enables its plaintext private-LAN browser terminal at `http://HOSTNAME.local:18080`. Closing its final graphical frame ends the process; reopening starts fresh recorded jobs.\n\
- **On-screen keyboard**: Axidev OSK with desktop and login-screen startup.\n\
- **Keyboard remapping**: Kanata keyboard remapping with a KDE tray profile selector.\n\
- **Dictation**: Handy offline push-to-talk dictation on Ctrl+Space.\n\
- **KDE Plasma**: Black & Pink panels and application dock for KDE Plasma 6.7 through 6.x.\n\
- **Desktop automation**: ydotool with a persistent user service for virtual keyboard and pointer input.\n";
const ARCH_WSL_INVENTORY: &str = "- **Editor**: This profile does not install an editor. The shell uses an available Vim or Vi fallback; Neovim, tmux, Lazygit, and Hunk are removed.\n";
const INVENTORY_END: &str = "\nAll selected installer modules completed successfully.\n";

pub trait EnvironmentInventoryModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}

pub struct CachyosEnvironmentInventory;
pub struct ArchWslEnvironmentInventory;

fn write_environment_inventory(context: &ModuleContext<'_>) -> ModuleResult {
    // Agents maintain an existing inventory; only a new machine gets the template.
    let path = context.home.join("environment.md");
    if path.exists() {
        println!("Keeping the existing environment inventory");
        return Ok(());
    }
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
    fs::write(path, inventory)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use myconfig_utils::PackageSystem;
    use xshell::Shell;

    fn inventory_home(name: &str) -> std::path::PathBuf {
        let home =
            std::env::temp_dir().join(format!("myconfig-inventory-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        home
    }

    #[test]
    fn environment_inventory_keeps_an_existing_file() {
        let home = inventory_home("existing");
        let inventory = home.join("environment.md");
        fs::write(&inventory, "# Environment\n\nMaintained by an agent.\n").unwrap();
        let sh = Shell::new().unwrap();
        let context = ModuleContext {
            profile: Profile::Cachyos,
            package_system: PackageSystem::Arch,
            shell: &sh,
            home: &home,
        };
        CachyosEnvironmentInventory.install(&context).unwrap();
        assert_eq!(
            fs::read_to_string(&inventory).unwrap(),
            "# Environment\n\nMaintained by an agent.\n"
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn environment_inventory_writes_the_template_on_a_new_machine() {
        let home = inventory_home("new");
        let sh = Shell::new().unwrap();
        let context = ModuleContext {
            profile: Profile::ArchWsl,
            package_system: PackageSystem::Arch,
            shell: &sh,
            home: &home,
        };
        ArchWslEnvironmentInventory.install(&context).unwrap();
        let inventory = fs::read_to_string(home.join("environment.md")).unwrap();
        assert!(inventory.starts_with(
            "# Environment\n\nThis file describes the capabilities installed for the Arch WSL development environment.\n"
        ));
        assert!(inventory.contains("- **Editor**: This profile does not install an editor."));
        assert!(inventory.ends_with("\nAll selected installer modules completed successfully.\n"));
        fs::remove_dir_all(home).unwrap();
    }
}

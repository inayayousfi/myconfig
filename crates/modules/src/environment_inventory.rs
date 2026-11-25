//! The `~/environment.md` inventory that agents read and keep up to date.
use std::fs;

use crate::{Context, Footprint, Module, ModuleResult, Need};

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
- **Claude configuration**: `claude-config-helper` trusts projects, marks Claude's first-run setup as finished, lists or clears saved approvals, applies the MCP servers listed in `~/.config/claude-config-helper/mcp-servers.json`, and checks files for tokens, email addresses and home paths.\n";
const INVENTORY_END: &str =
    "\n`inayos verify` checks that the installed modules are still in place.\n";

const DESKTOP_INVENTORY: &[&str] = &[
    "- **System settings**: zram swap as large as RAM, DNS through systemd-resolved, an open-file limit of 2097152, and the `ntsync` module for Wine and Proton.\n",
    "- **Editor**: Neovim 0.12 is `EDITOR`, with the Black & Pink config and a mouse-first layer in `~/.config/nvim/lua/mouse.lua`.\n",
    "- **Terminal workspace**: the Herdr app (AUR `herdr-app-bin`) on herdr-core (`hc`), the background server that keeps terminals and agents running. `hc ls`, `hc attach` and `hc read` list, join and read them; the `herdr` agent skill teaches agents to use it. Its Black & Pink look lives in the `blacknpink` layout and theme under `~/.config/herdr-app`, with telemetry off. Each Herdr shell sends right-clicks to mouse-using programs; Shift+right-click opens Herdr's menu.\n",
    "- **Browser**: Google Chrome from the AUR.\n",
    "- **Math and documents**: LyX with TeX Live, the Qalculate calculator with gnuplot for plots, and Okular as the default PDF viewer.\n",
    "- **On-screen keyboard**: Axidev OSK with desktop and login-screen startup.\n",
    "- **Keyboard remapping**: Kanata keyboard remapping with a KDE tray profile selector.\n",
    "- **Dictation**: Handy offline push-to-talk dictation on Ctrl+Space.\n",
    "- **KDE Plasma**: Breeze Dark with a pink accent color, a top bar holding a session menu and the global menu, and an application dock on every display, for KDE Plasma 6.7 through 6.x.\n",
    "- **Printing**: the CUPS print server, started by `cups.socket`, with KDE's printer settings.\n",
    "- **Desktop automation**: ydotool with a persistent user service for virtual keyboard and pointer input.\n",
    CONTAINERS,
    "- **Remote access**: OpenSSH server and Tailscale service.\n",
];
const ARCH_WSL_INVENTORY: &[&str] = &[
    "- **Editor**: Neovim 0.12 is `EDITOR`, with the Black & Pink config and a mouse-first layer in `~/.config/nvim/lua/mouse.lua`.\n",
    "- **Terminal workspace**: herdr-core (`hc`), the background server that keeps terminals and agents running. `hc ls`, `hc attach` and `hc read` list, join and read them; the `herdr` agent skill teaches agents to use it. A Herdr app on another machine reaches it over SSH.\n",
    "- **Remote access**: OpenSSH server, which accepts key login only, and Tailscale service.\n",
];
const UBUNTU_DEVBOX_INVENTORY: &[&str] = &[
    "- **Editor**: Neovim 0.12 is `EDITOR`, from its release archive in `~/.local/opt`, with the Black & Pink config and a mouse-first layer in `~/.config/nvim/lua/mouse.lua`.\n",
    "- **Terminal workspace**: herdr-core (`hc`), the background server that keeps terminals and agents running. `hc ls`, `hc attach` and `hc read` list, join and read them; the `herdr` agent skill teaches agents to use it. It comes from Herdr's release archive in `~/.local/opt`. A Herdr app on another machine reaches it over SSH.\n",
    CONTAINERS,
    "- **Package sources**: Tailscale, GitHub CLI, Yazi and Docker come from their vendors' apt repositories. Bun is in `~/.bun` and updates with `bun upgrade`.\n",
    "- **Remote access**: Tailscale service. This profile leaves SSH as the machine had it.\n",
];
const CONTAINERS: &str = "- **Containers**: rootless Docker with Buildx and Compose. The Docker daemon runs as the user service `docker.service` and the `rootless` Docker context is selected.\n";

/// A phrase of the template for each module that it describes. The template of a profile
/// holds the phrase exactly when the profile holds the module.
const CLAIMS: [(&str, &str); 16] = [
    ("system-tuning", "zram swap"),
    ("system-monitor", "btop"),
    ("tailscale", "Tailscale service"),
    ("ssh-server", "OpenSSH server"),
    ("docker", "rootless Docker"),
    ("neovim", "Neovim 0.12"),
    ("herdr-core", "herdr-core (`hc`)"),
    ("herdr", "the Herdr app"),
    ("chrome", "Google Chrome from the AUR"),
    ("math", "LyX with TeX Live"),
    ("printing", "CUPS print server"),
    ("axidev-osk", "Axidev OSK"),
    ("kanata", "Kanata keyboard remapping"),
    ("kanata-kde", "KDE tray profile selector"),
    ("kde-plasma", "**KDE Plasma**"),
    ("handy", "Handy"),
];

pub struct EnvironmentInventory {
    /// The platform sentence, such as "Arch WSL development environment".
    pub platform: &'static str,
    /// The terminal tools, as a list in a sentence.
    pub terminal_tools: &'static str,
    /// Profile-specific lines added after the shared development environment.
    pub extra: &'static [&'static str],
}

impl EnvironmentInventory {
    pub const ARCH_DESKTOP: Self = Self {
        platform: "InayOS desktop, an Arch Linux development workstation",
        terminal_tools: "Yazi, ripgrep, fd, fzf, zoxide, eza, bat, lazygit, jq, fastfetch, and tokei",
        extra: DESKTOP_INVENTORY,
    };

    pub const ARCH_WSL: Self = Self {
        platform: "Arch WSL development environment",
        terminal_tools: "Yazi, ripgrep, fd, fzf, zoxide, eza, bat, lazygit, jq, fastfetch, tokei, and btop",
        extra: ARCH_WSL_INVENTORY,
    };

    pub const UBUNTU_DEVBOX: Self = Self {
        platform: "Ubuntu development machine for coding and long-running agents",
        terminal_tools: "Yazi, ripgrep, fd, fzf, zoxide, eza, bat, lazygit, jq, fastfetch, tokei, and btop",
        extra: UBUNTU_DEVBOX_INVENTORY,
    };

    pub(crate) fn template(&self) -> String {
        [
            INVENTORY_START,
            self.platform,
            INVENTORY_RUNTIMES,
            self.terminal_tools,
            INVENTORY_AGENTS,
            &self.extra.concat(),
            INVENTORY_END,
        ]
        .concat()
    }

    /// The modules whose description in the template does not match `profile`: described
    /// but missing from it, or in it but not described. Each app's tests check its
    /// profile with this.
    pub fn claim_problems(&self, profile: &[&dyn Module]) -> Vec<String> {
        let template = self.template();
        CLAIMS
            .iter()
            .filter_map(|(module, phrase)| {
                let described = template.contains(phrase);
                let present = profile.iter().any(|candidate| candidate.name() == *module);
                match (described, present) {
                    (true, false) => Some(format!("describes {module}, which the profile lacks")),
                    (false, true) => Some(format!("does not describe {module}")),
                    _ => None,
                }
            })
            .collect()
    }
}

impl Module for EnvironmentInventory {
    fn name(&self) -> &'static str {
        "environment-inventory"
    }

    /// It describes what the earlier modules installed, so it waits for all of them.
    fn needs(&self) -> &'static [Need] {
        &[Need::EveryEarlier]
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

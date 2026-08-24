#!/usr/bin/env bash

module_agents_packages() {
    myconfig_log "Installing agent tools and browser dependencies"
    install_package_ids \
        herdr opencode at_spi2_core libxcomposite libxdamage libxrandr libxkbcommon

    if [ "$MYCONFIG_PROFILE" = arch-wsl ]; then
        install_package_ids wsl_ssh_agent
    fi

    export BUN_INSTALL="$HOME/.bun"
    export PATH="$HOME/.local/bin:$BUN_INSTALL/bin:$PATH"

    bun add --global @playwright/mcp@latest

    local playwright_cli="$BUN_INSTALL/install/global/node_modules/.bin/playwright"
    local playwright_module="$BUN_INSTALL/install/global/node_modules/playwright"
    [ -x "$playwright_cli" ] || myconfig_fail "Playwright CLI was not installed"

    "$playwright_cli" install --only-shell chromium
    bun -e \
        "const { chromium } = require('$playwright_module'); const browser = await chromium.launch({ headless: true }); await browser.close();"
}

link_agent_config() {
    local skills_dir="$HOME/.agents/skills"
    [ -d "$skills_dir" ] || myconfig_fail "agent skills were not stowed"

    mkdir -p "$HOME/.claude/skills" "$HOME/.config/opencode"

    local skill_dir skill
    for skill_dir in "$skills_dir"/*; do
        [ -d "$skill_dir" ] || continue
        skill="$(basename "$skill_dir")"
        rm -rf "$HOME/.claude/skills/$skill"
        ln -sfn "../../.agents/skills/$skill" "$HOME/.claude/skills/$skill"
    done

    ln -sfn "../../.agents/AGENTS.md" "$HOME/.config/opencode/AGENTS.md"
}

write_environment_inventory() {
    local platform
    case "$MYCONFIG_PROFILE" in
        cachyos) platform="CachyOS development workstation" ;;
        arch-wsl) platform="Arch WSL development environment" ;;
        *) platform="$MYCONFIG_PROFILE" ;;
    esac

    cat >"$HOME/environment.md" <<EOF
# Environment

This file describes the capabilities installed for the $platform.

## Development environment

- **Shell**: Zsh with Oh My Zsh and the shared Black & Pink configuration.
- **Editor**: Neovim with the shared configuration.
- **Runtimes**: Rust, Go, Bun, Node.js, Python, Java, LLVM, Make, and CMake.
- **Repository tools**: Git, GitHub CLI, Lazygit, Hunk, and GNU Stow.
- **Terminal tools**: Yazi, ripgrep, fd, fzf, zoxide, eza, bat, jq, and btop.
- **Agent tools**: OpenCode, Herdr, and Playwright MCP.
- **Remote access**: OpenSSH server and Tailscale service with optional login during setup.
EOF

    if [ "$MYCONFIG_PROFILE" = cachyos ]; then
        cat >>"$HOME/environment.md" <<'EOF'
- **On-screen keyboard**: Axidev OSK with desktop and login-screen startup.
EOF
    fi

    cat >>"$HOME/environment.md" <<'EOF'

All selected installer modules completed successfully.
EOF
}

module_agents_configure() {
    myconfig_log "Configuring agent tools"
    export BUN_INSTALL="$HOME/.bun"
    export PATH="$HOME/.local/bin:$BUN_INSTALL/bin:$PATH"

    opencode debug config >/dev/null
    link_agent_config

    herdr integration install claude
}

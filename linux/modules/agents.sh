#!/usr/bin/env bash

module_agents_packages() {
    myconfig_log "Installing agent tools and browser dependencies"
    local packages=(
        lsof at_spi2_core libxcomposite libxdamage libxrandr libxkbcommon
    )

    if [ "$MYCONFIG_PROFILE" = cachyos ]; then
        packages+=(ydotool)
    fi

    install_package_ids "${packages[@]}"

    if [ "$MYCONFIG_PROFILE" = arch-wsl ]; then
        install_package_ids wsl_ssh_agent
    fi

    export BUN_INSTALL="$HOME/.bun"
    export PATH="$HOME/.local/bin:$BUN_INSTALL/bin:$PATH"

    # Anthropic's installer puts claude in ~/.local/bin and Claude Code updates itself.
    if [ ! -x "$HOME/.local/bin/claude" ]; then
        local claude_installer
        claude_installer="$(curl -fsSL https://claude.ai/install.sh)" \
            || myconfig_fail "Could not download the Claude Code installer"
        bash -c "$claude_installer" \
            || myconfig_fail "Claude Code installer failed"
    fi
    [ -x "$HOME/.local/bin/claude" ] \
        || myconfig_fail "Claude Code was not installed at ~/.local/bin/claude"

    bun add --global @playwright/mcp@latest @mariozechner/pi-coding-agent@latest
    [ -x "$BUN_INSTALL/bin/pi" ] || myconfig_fail "Pi was not installed at $BUN_INSTALL/bin/pi"

    local playwright_cli="$BUN_INSTALL/install/global/node_modules/.bin/playwright"
    local playwright_module="$BUN_INSTALL/install/global/node_modules/playwright"
    [ -x "$playwright_cli" ] || myconfig_fail "Playwright CLI was not installed"

    "$playwright_cli" install --only-shell chromium
    bun -e \
        "const { chromium } = require('$playwright_module'); const browser = await chromium.launch({ headless: true }); await browser.close();"
}

configure_fx_playwright_mcp() {
    require_command jq

    local mcp_config="$HOME/.fx/mcp.json"
    local temporary_config
    mkdir -p "$HOME/.fx"
    temporary_config="$(mktemp "$HOME/.fx/mcp.json.XXXXXX")" \
        || myconfig_fail "Could not create temporary fx MCP configuration"

    if [ -f "$mcp_config" ]; then
        jq --arg command "$BUN_INSTALL/bin/playwright-mcp" '
            .mcp = ((.mcpServers // {}) + (.mcp // {}))
            | .mcp.playwright = {
                "type": "stdio",
                "command": [$command, "--headless"],
                "enabled": true
            }
            | del(.mcpServers)
        ' "$mcp_config" >"$temporary_config" \
            || myconfig_fail "Could not merge fx MCP configuration"
    else
        jq -n --arg command "$BUN_INSTALL/bin/playwright-mcp" '{
            "mcp": {
                "playwright": {
                    "type": "stdio",
                    "command": [$command, "--headless"],
                    "enabled": true
                }
            }
        }' >"$temporary_config" \
            || myconfig_fail "Could not create fx MCP configuration"
    fi

    chmod 600 "$temporary_config"
    mv -f "$temporary_config" "$mcp_config" \
        || myconfig_fail "Could not install fx MCP configuration"
}

link_agent_config() {
    local skills_dir="$HOME/.agents/skills"
    [ -d "$skills_dir" ] || myconfig_fail "agent skills were not stowed"
    [ -f "$HOME/.agents/AGENTS.md" ] || myconfig_fail "global AGENTS.md was not stowed"

    mkdir -p "$HOME/.claude/skills" "$HOME/.config/opencode" "$HOME/.fx" "$HOME/.pi/agent"

    local skill_dir skill opencode_agents fx_agents
    for skill_dir in "$skills_dir"/*; do
        [ -d "$skill_dir" ] || continue
        skill="$(basename "$skill_dir")"
        rm -rf "$HOME/.claude/skills/$skill"
        ln -sfn "../../.agents/skills/$skill" "$HOME/.claude/skills/$skill"
    done

    opencode_agents="$HOME/.config/opencode/AGENTS.md"
    ln -sfnT "$HOME/.agents/AGENTS.md" "$opencode_agents"
    [ -L "$opencode_agents" ] \
        && [ "$(readlink "$opencode_agents")" = "$HOME/.agents/AGENTS.md" ] \
        || myconfig_fail "OpenCode AGENTS bridge was not created"

    fx_agents="$HOME/.fx/AGENTS.md"
    ln -sfnT "$HOME/.agents/AGENTS.md" "$fx_agents"
    [ -L "$fx_agents" ] \
        && [ "$(readlink "$fx_agents")" = "$HOME/.agents/AGENTS.md" ] \
        || myconfig_fail "fx AGENTS bridge was not created"

    ln -sfnT "$HOME/.agents/AGENTS.md" "$HOME/.pi/agent/AGENTS.md"
    [ "$(readlink -f "$HOME/.pi/agent/AGENTS.md")" = "$(readlink -f "$HOME/.agents/AGENTS.md")" ] \
        || myconfig_fail "Pi AGENTS bridge was not created"
}

write_environment_inventory() {
    # Agents maintain an existing inventory; only a new machine gets the template.
    if [ -e "$HOME/environment.md" ]; then
        myconfig_log "Keeping the existing environment inventory"
        return 0
    fi

    local platform terminal_tools="Yazi, ripgrep, and jq"
    case "$MYCONFIG_PROFILE" in
        cachyos) platform="CachyOS development workstation" ;;
        arch-wsl)
            platform="Arch WSL development environment"
            terminal_tools="Yazi, ripgrep, jq, and btop"
            ;;
        ubuntu-devbox)
            platform="Ubuntu development machine for coding and long-running agents"
            terminal_tools="Yazi, ripgrep, jq, and btop"
            ;;
        *) platform="$MYCONFIG_PROFILE" ;;
    esac

    cat >"$HOME/environment.md" <<EOF
# Environment

This file describes the capabilities installed for the $platform.

## Development environment

- **Shell**: Zsh with Oh My Zsh and the shared Black & Pink configuration.
- **Runtimes**: Rust, Go, Bun, Node.js, Python, Java, LLVM, Make, and CMake.
- **Repository tools**: Git, GitHub CLI, and GNU Stow.
- **Terminal tools**: $terminal_tools.
- **Coding agents**: Claude Code, installed by Anthropic's installer in \`~/.local/bin/claude\` and updated by itself. Pi, installed globally by Bun in \`~/.bun/bin/pi\`.
- **Agent browser tools**: Playwright MCP.
- **Claude configuration**: \`claude-config-helper\` trusts projects, marks Claude's first-run setup as finished, lists or clears saved approvals, applies the MCP servers listed in \`~/.config/claude-config-helper/mcp-servers.json\`, and checks files for tokens, email addresses and home paths.
- **Remote access**: OpenSSH server and Tailscale service with optional login during setup.
- **T3 Code**: the \`t3\` server from npm in \`~/.local/bin/t3\`, run at boot by the user service \`t3code.service\` on \`127.0.0.1:3773\`. \`t3ctl status\`, \`remote\`, \`update\` and \`repair\` manage it; \`remote\` turns T3 Connect or the Tailscale route (HTTPS port 10000) on or off. Both routes start off.
EOF

    if [ "$MYCONFIG_PROFILE" = cachyos ]; then
        cat >>"$HOME/environment.md" <<'EOF'
- **Editor**: VSCodium with the Black & Pink theme and Open Remote SSH. It opens text files from the desktop, and `EDITOR` is `codium --wait`.
- **On-screen keyboard**: Axidev OSK with desktop and login-screen startup.
- **Keyboard remapping**: Kanata keyboard remapping with a KDE tray profile selector.
- **Dictation**: Handy offline push-to-talk dictation on Ctrl+Space.
- **KDE Plasma**: Breeze Dark with a pink accent color, a top bar holding a session menu and the global menu, and an application dock on every display, for KDE Plasma 6.7 through 6.x.
- **Desktop automation**: ydotool with a persistent user service for virtual keyboard and pointer input.
EOF
    elif [ "$MYCONFIG_PROFILE" = arch-wsl ]; then
        cat >>"$HOME/environment.md" <<'EOF'
- **Editor**: This profile does not install an editor. When Windows VSCodium is installed, `EDITOR` is `~/.local/bin/windows-codium`, which opens files in it and waits until they are closed. Otherwise the shell uses an available Vim or Vi fallback.
EOF
    elif [ "$MYCONFIG_PROFILE" = ubuntu-devbox ]; then
        cat >>"$HOME/environment.md" <<'EOF'
- **Editor**: This profile does not install an editor. VSCodium and VS Code open folders here over SSH with their remote SSH extensions. The shell uses an available Vim or Vi.
- **Containers**: rootless Docker with Buildx and Compose. The Docker daemon runs as the user service `docker.service` and the `rootless` Docker context is selected.
- **Package sources**: Tailscale, GitHub CLI, Yazi and Docker come from their vendors' apt repositories. Bun is in `~/.bun` and updates with `bun upgrade`.
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

    if command -v opencode >/dev/null 2>&1; then
        opencode debug config >/dev/null
    fi
    link_agent_config
    configure_fx_playwright_mcp
    [ -x "$HOME/.local/bin/claude-config-helper" ] \
        || myconfig_fail "claude-config-helper was not stowed as an executable"
    "$HOME/.local/bin/claude-config-helper" mcp apply \
        || myconfig_fail "Could not apply Claude MCP servers"

    if [ "$MYCONFIG_PROFILE" = cachyos ]; then
        require_command ydotool
        require_command systemctl
        require_function configure_named_input_access
        require_function user_has_active_group

        configure_named_input_access ydotool
        systemctl --user daemon-reload
        systemctl --user enable ydotool.service

        if user_has_active_group input && user_has_active_group uinput; then
            systemctl --user restart ydotool.service
            systemctl --user --quiet is-active ydotool.service \
                || myconfig_fail "ydotool user service did not become active"

            local socket="${YDOTOOL_SOCKET:-${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/.ydotool_socket}"
            local attempt
            for attempt in {1..20}; do
                [ -S "$socket" ] && break
                sleep 0.1
            done
            [ -S "$socket" ] || myconfig_fail "ydotool user service did not create its socket"
        else
            myconfig_log "Log out and back in before ydotool can access uinput; its service is enabled for the next login"
        fi
    fi
}

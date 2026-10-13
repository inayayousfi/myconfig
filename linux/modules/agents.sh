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

    local linked_skill linked_target skill_dir skill opencode_agents fx_agents
    for linked_skill in "$HOME/.claude/skills"/*; do
        [ -L "$linked_skill" ] || continue
        linked_target="$(readlink "$linked_skill")"
        case "$linked_target" in
            ../../.agents/skills/*)
                skill="$(basename "$linked_skill")"
                [ -d "$skills_dir/$skill" ] || rm "$linked_skill"
                ;;
        esac
    done

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

configure_peon_ping() {
    myconfig_log "Installing peon-ping sounds for Claude Code"
    require_command curl
    require_command jq
    require_command python3
    require_command sha256sum

    local source_dir="$HOME/.config/peon-ping"
    local peon_dir="$HOME/.claude/hooks/peon-ping"
    local pack_dir="$peon_dir/packs/plain"
    local settings="$HOME/.claude/settings.json"
    [ -f "$source_dir/config.json" ] && [ -f "$source_dir/packs/plain/openpeon.json" ] \
        || myconfig_fail "peon-ping configuration was not stowed from the ai package"

    # The installer stops on a pack missing from its online catalog, but skips a
    # pack whose sounds match its .checksums file, so the plain pack goes first.
    rm -rf "$pack_dir"
    mkdir -p "$peon_dir/packs"
    cp -RL "$source_dir/packs/plain" "$pack_dir"
    local sound
    for sound in "$pack_dir"/sounds/*; do
        printf '%s %s\n' "$(basename "$sound")" "$(sha256sum "$sound" | cut -d ' ' -f 1)"
    done >"$pack_dir/.checksums"

    # --no-rc keeps the installer out of the stowed shell files.
    local peon_installer
    peon_installer="$(curl -fsSL https://raw.githubusercontent.com/PeonPing/peon-ping/main/install.sh)" \
        || myconfig_fail "Could not download the peon-ping installer"
    bash -c "$peon_installer" peon-ping-install --global --no-rc --packs=plain </dev/null \
        || myconfig_fail "peon-ping installer failed"

    # Only Claude Code gets the sounds; the installer also registers other agents it finds.
    rm -f "$HOME/.copilot/hooks/peon-ping.json"
    if [ -f "$HOME/.codex/config.toml" ]; then
        sed -i '/^# peon-ping Codex hooks begin$/,/^# peon-ping Codex hooks end$/d' "$HOME/.codex/config.toml"
    fi

    local merged_config
    merged_config="$(jq -s '.[0] * .[1]' "$peon_dir/config.json" "$source_dir/config.json")" \
        || myconfig_fail "Could not merge the peon-ping configuration"
    printf '%s\n' "$merged_config" >"$peon_dir/config.json"

    [ "$(jq -r '.default_pack' "$peon_dir/config.json")" = plain ] \
        || myconfig_fail "peon-ping does not use the plain sound pack"
    [ -f "$pack_dir/sounds/done.wav" ] && [ -f "$pack_dir/sounds/needs-you.wav" ] \
        || myconfig_fail "The plain peon-ping pack was not installed"
    jq -e '[.hooks.Stop[]?.hooks[]?.command, .hooks.PermissionRequest[]?.hooks[]?.command]
            | map(select(endswith("/peon-ping/peon.sh"))) | length == 2' "$settings" >/dev/null \
        || myconfig_fail "peon-ping hooks are missing from Claude's settings"
}

write_environment_inventory() {
    # Agents maintain an existing inventory; only a new machine gets the template.
    if [ -e "$HOME/environment.md" ]; then
        myconfig_log "Keeping the existing environment inventory"
        return 0
    fi

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
- **Runtimes**: Rust, Go, Bun, Node.js, Python, Java, LLVM, Make, and CMake.
- **Repository tools**: Git, GitHub CLI, and GNU Stow.
- **Terminal tools**: Yazi, ripgrep, jq, and btop.
- **Coding agents**: Claude Code, installed by Anthropic's installer in \`~/.local/bin/claude\` and updated by itself. Pi, installed globally by Bun in \`~/.bun/bin/pi\`.
- **Agent browser tools**: Playwright MCP.
- **Agent sounds**: peon-ping, installed from its main branch into \`~/.claude/hooks/peon-ping\`, plays the plain pack from \`~/.config/peon-ping\` when Claude Code finishes, needs approval, hits an error or reaches a limit. Its \`peon\` command and \`/peon-ping-*\` skills control it.
- **Claude configuration**: \`claude-config-helper\` trusts projects, marks Claude's first-run setup as finished, lists or clears saved approvals, applies the MCP servers listed in \`~/.config/claude-config-helper/mcp-servers.json\`, and checks files for tokens, email addresses and home paths.
- **Remote access**: OpenSSH server and Tailscale service with optional login during setup.
EOF

    if [ "$MYCONFIG_PROFILE" = cachyos ]; then
        cat >>"$HOME/environment.md" <<'EOF'
- **Editor and terminal Atelier**: Native Wayland graphical Emacs with restorable workspace layouts, native buffers, splits, libghostty-powered Ghostel terminals, Git review, and workspace-owned AIPanel coding agents. `M-x remot-set-password` enables its plaintext private-LAN browser terminal at `http://HOSTNAME.local:18080`. Closing its final graphical frame ends the process; reopening starts fresh recorded jobs.
- **On-screen keyboard**: Axidev OSK with desktop and login-screen startup.
- **Keyboard remapping**: Kanata keyboard remapping with a KDE tray profile selector.
- **Dictation**: Handy offline push-to-talk dictation on Ctrl+Space.
- **KDE Plasma**: Glassy Black & Pink theme, panels and application dock for KDE Plasma 6.7 through 6.x.
- **Desktop automation**: ydotool with a persistent user service for virtual keyboard and pointer input.
EOF
    elif [ "$MYCONFIG_PROFILE" = arch-wsl ]; then
        cat >>"$HOME/environment.md" <<'EOF'
- **Editor**: This profile does not install an editor. The shell uses an available Vim or Vi fallback; Neovim, tmux, Lazygit, and Hunk are removed.
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
    configure_peon_ping

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

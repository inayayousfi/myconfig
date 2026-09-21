#!/usr/bin/env bash

module_agents_packages() {
    myconfig_log "Installing agent tools and browser dependencies"
    local packages=(
        opencode lsof at_spi2_core libxcomposite libxdamage libxrandr libxkbcommon
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
    [ -f "$HOME/.agents/AGENTS.md" ] || myconfig_fail "global AGENTS.md was not stowed"

    mkdir -p "$HOME/.claude/skills" "$HOME/.config/opencode"

    local linked_skill linked_target skill_dir skill opencode_agents
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
- **Runtimes**: Rust, Go, Bun, Node.js, Python, Java, LLVM, Make, and CMake.
- **Repository tools**: Git, GitHub CLI, and GNU Stow.
- **Terminal tools**: Yazi, ripgrep, fd, fzf, zoxide, eza, bat, jq, and btop.
- **Agent tools**: OpenCode and Playwright MCP.
- **Remote access**: OpenSSH server and Tailscale service with optional login during setup.
EOF

    if [ "$MYCONFIG_PROFILE" = cachyos ]; then
        cat >>"$HOME/environment.md" <<'EOF'
- **Editor and terminal Atelier**: Graphical Emacs with restorable workspace layouts, native buffers, splits, libghostty-powered Ghostel terminals, Git review, and workspace-owned AIPanel coding agents. Closing its final frame ends the process; reopening starts fresh recorded jobs.
- **On-screen keyboard**: Axidev OSK with desktop and login-screen startup.
- **Keyboard remapping**: Kanata keyboard remapping with a KDE tray profile selector.
- **Dictation**: Handy offline push-to-talk dictation on Ctrl+Space.
- **KDE Plasma**: Black & Pink panels and application dock for KDE Plasma 6.7 through 6.x.
- **Desktop automation**: ydotool with a persistent user service for virtual keyboard and pointer input.
EOF
    elif [ "$MYCONFIG_PROFILE" = arch-wsl ]; then
        cat >>"$HOME/environment.md" <<'EOF'
- **Editor**: Unconfigured Neovim is retained as the shell editor; shared Neovim, tmux, Lazygit, and Hunk dotfiles are retired.
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

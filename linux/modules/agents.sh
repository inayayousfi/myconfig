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

    bun add --global @playwright/mcp@latest jsonc-parser@latest

    local playwright_cli="$BUN_INSTALL/install/global/node_modules/.bin/playwright"
    local playwright_module="$BUN_INSTALL/install/global/node_modules/playwright"
    [ -x "$playwright_cli" ] || myconfig_fail "Playwright CLI was not installed"

    "$playwright_cli" install --only-shell chromium
    bun -e \
        "const { chromium } = require('$playwright_module'); const browser = await chromium.launch({ headless: true }); await browser.close();"
}

configure_opencode() {
    local config_dir="$HOME/.config/opencode"
    local config_file parser_module config_tmp
    mkdir -p "$config_dir"

    if [ -e "$config_dir/opencode.jsonc" ]; then
        config_file="$config_dir/opencode.jsonc"
    else
        config_file="$config_dir/opencode.json"
    fi

    [ -e "$config_file" ] || printf '{}\n' >"$config_file"
    parser_module="$HOME/.bun/install/global/node_modules/jsonc-parser"
    [ -f "$parser_module/package.json" ] || myconfig_fail "jsonc-parser was not installed"

    OPENCODE_CONFIG="$config_file" bun -e \
        'const parser = require(process.env.HOME + "/.bun/install/global/node_modules/jsonc-parser");
         const text = await Bun.file(process.env.OPENCODE_CONFIG).text();
         const errors = [];
         const config = parser.parse(text, errors, { allowTrailingComma: true });
         if (errors.length || !config || Array.isArray(config) || typeof config !== "object") process.exit(1);
         if (config.mcp != null && (Array.isArray(config.mcp) || typeof config.mcp !== "object")) process.exit(1);'

    config_tmp="$(mktemp "$config_dir/$(basename "$config_file").XXXXXX")"
    trap 'rm -f -- "$config_tmp"' RETURN

    OPENCODE_CONFIG="$config_file" \
        OPENCODE_CONFIG_TMP="$config_tmp" \
        PLAYWRIGHT_COMMAND="$HOME/.bun/bin/playwright-mcp" \
        bun -e \
        'const parser = require(process.env.HOME + "/.bun/install/global/node_modules/jsonc-parser");
         const text = await Bun.file(process.env.OPENCODE_CONFIG).text();
         const value = { type: "local", command: [process.env.PLAYWRIGHT_COMMAND, "--headless"], enabled: true };
         const edits = parser.modify(text, ["mcp", "playwright"], value, {
             formattingOptions: { insertSpaces: true, tabSize: 2, eol: "\n" }
         });
         await Bun.write(process.env.OPENCODE_CONFIG_TMP, parser.applyEdits(text, edits));'

    mv "$config_tmp" "$config_file"
    config_tmp=""
    trap - RETURN
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
- **Agent tools**: OpenCode, Herdr, T3 Code, and Playwright MCP.
- **Remote access**: Tailscale service with optional login during setup.

All selected installer modules completed successfully.
EOF
}

module_agents_configure() {
    myconfig_log "Configuring agent tools"
    export BUN_INSTALL="$HOME/.bun"
    export PATH="$HOME/.local/bin:$BUN_INSTALL/bin:$PATH"

    configure_opencode
    link_agent_config

    herdr integration install claude
}

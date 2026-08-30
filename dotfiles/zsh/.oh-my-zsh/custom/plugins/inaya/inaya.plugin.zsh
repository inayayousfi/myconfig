# Inaya's Oh My Zsh plugin
# Cross-platform compatible (macOS and Linux)

# Detect the operating system
case "$(uname -s)" in
    Darwin)
        IS_MACOS=true
        IS_LINUX=false
        ;;
    Linux)
        IS_MACOS=false
        IS_LINUX=true
        ;;
    *)
        IS_MACOS=false
        IS_LINUX=false
        ;;
esac

if [ -n "$WSL_DISTRO_NAME" ]; then
    IS_WSL=1
fi

has() {
    command -v "$1" >/dev/null 2>&1
}

# ============================================================================
# Environment Variables (Cross-platform)
# ============================================================================

export XDG_CONFIG_HOME="$HOME/.config"
export XDG_CACHE_HOME="$HOME/.cache"

export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8

export VI_MODE_SET_CURSOR=true

if has nvim; then
    export EDITOR="nvim"
    export VISUAL="nvim"
elif has vim; then
    export EDITOR="vim"
    export VISUAL="vim"
else
    export EDITOR="vi"
    export VISUAL="vi"
fi

# ============================================================================
# Platform-specific Environment Variables
# ============================================================================

# Pin bun's global root. Without this bun falls back to $XDG_CACHE_HOME/.bun,
# which splits globals across two dirs and puts binaries under a cache path
# that cleaners are free to wipe.
export BUN_INSTALL="$HOME/.bun"

if $IS_MACOS; then
    export JAVA_HOME="/opt/homebrew/opt/openjdk"
    export PATH="$HOME/.local/bin:$PATH:$(go env GOPATH 2>/dev/null)/bin:$JAVA_HOME/bin:$(bun pm bin -g 2>/dev/null)"
    export VCPKG_ROOT="$HOME/vcpkg"
elif $IS_LINUX; then
    export PATH="$HOME/.local/bin:$PATH:$(go env GOPATH 2>/dev/null)/bin:$(bun pm bin -g 2>/dev/null)"
fi

# ============================================================================
# Aliases (Cross-platform)
# ============================================================================

alias ll='ls -la'
alias gcb='git fetch --prune && git branch -vv | grep ": gone]" | awk "{print \$1}" | xargs -n 1 git branch -d'

alias please='sudo'

unalias gd 2>/dev/null || true

# Tool aliases
if has nvim; then
    alias vim='nvim'
    alias vi='nvim'
    alias v='nvim'
fi

if has eza; then
    alias ls='eza --icons --group-directories-first --git --color=always'
fi

if has fd; then
    alias find='fd'
fi

if has rg; then
    alias rg='rg --color=always --smart-case --hidden --glob "!.git/*" --glob "!.svn/*" --glob "!.hg/*" --glob "!node_modules/*"'
fi

if has bun; then
    alias npm='bun'
    alias npx='bunx'
fi

if has lazygit; then
    alias lg='lazygit'
fi

if has fastfetch; then
    alias ff='fastfetch'
fi

if has hyfetch; then
    alias hf='hyfetch'
fi

if has opencode; then
    alias oc='opencode --auto'
fi

if has codex; then
    alias cx='codex'
fi

if has claude; then
    alias cco='IS_DEMO=1 claude --dangerously-skip-permissions'
    alias ccor='claude remote-control --permission-mode bypassPermissions'
fi

if has zoxide; then
    alias zeze='zoxide edit'
fi

if has hunk; then
    alias hd='hunk diff'
    alias hdc='hunk show'

    hdb() {
        local base fork
        base=$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null)
        if [ -z "$base" ]; then
            base=$(git branch --format='%(refname:short)' --list main master dev | head -n 1)
        fi
        if [ -z "$base" ]; then
            echo "hdb: could not determine the default branch" >&2
            return 1
        fi

        fork=$(git merge-base --fork-point "$base" HEAD 2>/dev/null) || \
            fork=$(git merge-base "$base" HEAD) || return 1
        hunk diff "$fork" "$@"
    }
fi

if has tmux; then
    alias tx='tmux attach-session 2>/dev/null || tmux new-session'
fi

if has systemctl; then
    alias sus='systemctl suspend'
fi

# ============================================================================
# Functions (Cross-platform)
# ============================================================================

mkd() { mkdir -p -- "$1" && cd -P -- "$1"; }

reload-zsh() { source "$HOME/.zshrc" && echo "zsh reloaded"; }

# Fuzzy file picker - opens selection in the configured editor
pf() {
    local file
    file=$(fzf --preview='bat {} --color=always --style=numbers' --bind shift-up:preview-page-up,shift-down:preview-page-down)
    [ -n "$file" ] && $EDITOR "$file"
}

# Yazi file manager wrapper - changes directory on exit
y() {
    local tmp="$(mktemp -t "yazi-cwd.XXXXXX")" cwd
    command yazi "$@" --cwd-file="$tmp"
    IFS= read -r -d '' cwd <"$tmp"
    [ -n "$cwd" ] && [ "$cwd" != "$PWD" ] && builtin cd -- "$cwd"
    rm -f -- "$tmp"
}

# ============================================================================
# Platform-specific Functions
# ============================================================================

if $IS_MACOS; then
    # macOS: Update packages via Homebrew
    update() {
        echo "Updating packages via Homebrew..."
        brew update && brew upgrade && brew cleanup
        echo ""
        echo "Packages updated successfully."
        echo ""

        if has bun; then
            echo "Updating global Bun packages..."
            if bun update -g; then
                echo "Global Bun packages updated."
            else
                echo "Warning: Global Bun package update failed."
            fi
            echo ""

            local playwright_cli="$BUN_INSTALL/install/global/node_modules/.bin/playwright"
            if [[ -x "$playwright_cli" ]]; then
                echo "Updating Chromium Headless Shell for Playwright MCP..."
                "$playwright_cli" install --only-shell chromium \
                    || echo "Warning: Chromium Headless Shell update failed."
                echo ""
            fi
        fi

        if has claude; then
            echo "Updating Claude Code..."
            claude update
            echo "Claude Code updated."
        else
            echo "claude not found, skipping Claude Code update."
        fi
    }

    # macOS-specific: bootout GUI session
    bootout-gui() { launchctl bootout gui/$UID; }

elif $IS_LINUX; then
    # Linux: Update system packages using the available package manager
    update() {
        echo "Updating system and packages..."
        echo ""

        if command -v paru &>/dev/null; then
            echo "Updating system and AUR packages (paru)..."
            paru -Syu --noconfirm
            echo "System and AUR packages updated."
            echo ""
        elif command -v apt &>/dev/null; then
            echo "Updating system packages (apt)..."
            sudo apt update && sudo apt upgrade -y && sudo apt autoremove -y && sudo apt autoclean
            echo "System packages updated."
            echo ""
        else
            echo "No supported system package manager found."
            echo ""
        fi

        if command -v flatpak &>/dev/null; then
            echo "Updating Flatpak apps..."
            flatpak update -y
            flatpak uninstall --unused -y || true
            echo "Flatpak apps updated."
            echo ""
        else
            echo "Flatpak not found, skipping Flatpak updates."
            echo ""
        fi

        if command -v bun &>/dev/null; then
            if command -v paru &>/dev/null; then
                echo "Bun is managed by paru/pacman; skipping 'bun upgrade' (already updated above)."
                echo ""
            else
                echo "Upgrading Bun runtime..."
                bun upgrade
                echo ""
            fi

            echo "Updating global Bun packages..."
            bun update -g || true
            echo "Global Bun packages updated."
            echo ""

            local playwright_cli="$BUN_INSTALL/install/global/node_modules/.bin/playwright"
            if [[ -x "$playwright_cli" ]]; then
                echo "Updating Chromium Headless Shell for Playwright MCP..."
                "$playwright_cli" install --only-shell chromium \
                    || echo "Warning: Chromium Headless Shell update failed."
                echo ""
            fi
        else
            echo "bun not found, skipping Bun updates."
            echo ""
        fi

        if command -v claude &>/dev/null; then
            echo "Updating Claude Code..."
            claude update
            echo "Claude Code updated."
            echo ""
        else
            echo "claude not found, skipping Claude Code update."
            echo ""
        fi

        echo "Update process completed."
    }
fi

# ============================================================================
# Zoxide initialization
# ============================================================================

if has zoxide; then
    eval "$(zoxide init zsh)"
fi

# ============================================================================
# Interactive cleanup utility
# ============================================================================

cleanup() {
    if [[ -z "$PS1" ]]; then
        echo "cleanup: this command is intended for interactive use."
        return 1
    fi

    echo "Welcome to the cleanup ritual... 💫"
    echo "We'll walk through this path item by item and record your choices."
    echo ""

    typeset -A deletions_to_perform

    for item in .* *; do
        if [[ "$item" == "." || "$item" == ".." ]]; then
            continue
        fi

        if [[ ! -e "$item" && ! -L "$item" ]]; then
            continue
        fi

        echo "------------------------------------------------------"
        echo "Do you want to delete '$item'? (y/n/q to quit)"
        read -q "choice? Your choice, shooting star: "
        echo ""

        case "$choice" in
            y | Y)
                if [[ -d "$item" && ! -L "$item" ]]; then
                    echo "Note: '$item' (directory) is marked for recursive deletion. 🌬️"
                    deletions_to_perform["$item"]="directory"
                else
                    echo "Note: '$item' (file) is marked for deletion. 🍂"
                    deletions_to_perform["$item"]="file"
                fi
                ;;
            n | N)
                echo "'$item' will stay for now. 💖"
                ;;
            q | Q)
                echo "The ritual is paused. Nothing will be deleted today. May serenity stay with you. 🌟"
                return 0
                ;;
            *)
                echo "Unknown choice. '$item' will stay. 🤫"
                ;;
        esac
        echo ""
    done

    echo "------------------------------------------------------"
    echo "🌟 Summary of your choices 🌟"
    echo "These are the items you chose to release:"

    if ((${#deletions_to_perform[@]} == 0)); then
        echo "No items were marked for deletion. The path is clear. ✨"
        echo "Process complete. May the light guide your steps. 🌟"
        return 0
    fi

    integer i=1
    for item in ${(k)deletions_to_perform}; do
        local type="${deletions_to_perform[$item]}"
        echo "$((i++)). '$item' (Type: $type)"
    done

    echo ""
    read -q "final_choice?Are you sure you want to proceed with these deletions? (y/n): "
    echo ""

    if [[ "$final_choice" == "y" || "$final_choice" == "Y" ]]; then
        echo ""
        echo "The deletion ritual begins... irreversible once started. 🌌"
        for item in ${(k)deletions_to_perform}; do
            local type="${deletions_to_perform[$item]}"
            if [[ "$type" == "directory" ]]; then
                echo "Releasing directory '$item' and its contents... 🌬️"
                rm -rf -- "$item"
                if [ $? -eq 0 ]; then
                    echo "'$item' has joined the wind. ✨"
                else
                    echo "An invisible force blocked the release of '$item'. 💔"
                fi
            else
                echo "Releasing file '$item'... 🍂"
                rm -- "$item"
                if [ $? -eq 0 ]; then
                    echo "'$item' has dissolved into the ether. 🍃"
                else
                    echo "An invisible force blocked the release of '$item'. 💔"
                fi
            fi
        done
        echo ""
        echo "Every item in this path has been handled according to your wishes. The ritual is complete. May peace reign. 💖"
    else
        echo "The deletion ritual was cancelled. Marked items remain in place. Flexibility is strength, Inaya. 💫"
    fi

    echo "Process complete. May the light guide your steps. 🌟"
}

if has hyfetch; then
    hyfetch
elif has fastfetch; then
    fastfetch
fi

if has tmux && [[ -o interactive && -z "$TMUX" && -z "$ZSH_EXECUTION_STRING" ]]; then
    tx
fi

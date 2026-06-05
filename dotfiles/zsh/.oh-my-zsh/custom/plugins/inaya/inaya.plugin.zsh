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

if has zed; then
    export EDITOR="zed --wait"
    export VISUAL="zed --wait"
elif has nvim; then
    export EDITOR="nvim"
    export VISUAL="nvim"
elif has vim; then
    export EDITOR="vim"
    export VISUAL="vim"
else
    export EDITOR="vi"
    export VISUAL="vi"
fi

export TERM="xterm-256color"

# ============================================================================
# Platform-specific Environment Variables
# ============================================================================

if $IS_MACOS; then
    # macOS-specific paths
    export JAVA_HOME="/opt/homebrew/opt/openjdk"
    export PATH="$HOME/.local/bin:$PATH:$(go env GOPATH 2>/dev/null)/bin:$JAVA_HOME/bin"
    export VCPKG_ROOT="$HOME/vcpkg"
elif $IS_LINUX; then
    export PATH="$HOME/.local/bin:$PATH"
fi

# ============================================================================
# Aliases (Cross-platform)
# ============================================================================

alias vim='nvim'
alias vi='nvim'
alias v='nvim'

alias ll='ls -la'
alias gcb='git fetch --prune && git branch -vv | grep ": gone]" | awk "{print \$1}" | xargs -n 1 git branch -d'

alias please='sudo'

unalias gd 2>/dev/null || true

# Tool aliases
if has eza; then
    alias ls='eza --icons --group-directories-first --git --color=always'
fi

if has fd; then
    alias find='fd'
fi

if has rg; then
    alias grep='rg'
    alias rg='rg --color=always --smart-case --hidden --glob "!.git/*" --glob "!.svn/*" --glob "!.hg/*" --glob "!node_modules/*"'
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
    alias oc='opencode'
fi

if has codex; then
    alias cx='codex'
fi

if has zoxide; then
    alias zeze='zoxide edit'
fi

if has tmux; then
    alias tmux='tmux -f $XDG_CONFIG_HOME/tmux/tmux.conf'
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
  IFS= read -r -d '' cwd < "$tmp"
  [ -n "$cwd" ] && [ "$cwd" != "$PWD" ] && builtin cd -- "$cwd"
  rm -f -- "$tmp"
}

# ============================================================================
# Platform-specific Functions
# ============================================================================

if $IS_MACOS; then
    # macOS: use-tmux with Homebrew tmux path
    use-tmux() { /bin/bash --noprofile --norc -c "/opt/homebrew/bin/tmux has-session 2>/dev/null && /opt/homebrew/bin/tmux attach-session -d || /opt/homebrew/bin/tmux new-session"; }

    # macOS: Update packages via Homebrew
    update() {
        echo "Updating packages via Homebrew..."
        brew update && brew upgrade && brew cleanup
        echo ""
        echo "Packages updated successfully."
    }

    # macOS-specific: bootout GUI session
    bootout-gui() { launchctl bootout gui/$UID }

elif $IS_LINUX; then
    # Linux: use-tmux with system tmux
    use-tmux() { /bin/bash --noprofile --norc -c "tmux has-session 2>/dev/null && tmux attach-session -d || tmux new-session"; }

    # Linux: Update system packages using the available package manager
    update() {
        echo "Updating system and packages..."
        echo ""

        if command -v apt &>/dev/null; then
            echo "Updating system packages (apt)..."
            sudo apt update && sudo apt upgrade -y && sudo apt autoremove -y && sudo apt autoclean
            echo "System packages updated."
            echo ""
        else
            echo "No supported system package manager found."
            echo ""
        fi

        # Update Flatpak apps if flatpak is installed
        if command -v flatpak &>/dev/null; then
            echo "Updating Flatpak apps..."
            flatpak update -y
            # Remove unused runtimes to reclaim disk space.
            flatpak uninstall --unused -y || true
            echo "Flatpak apps updated."
        else
            echo "Flatpak not found, skipping Flatpak updates."
        fi

        echo ""
        echo "All updates completed successfully."
    }
fi

# =========================
# AI Commit
# =========================

aic() {
  local MODEL="${1:-openai/gpt-5.4-mini}"

  local branch gitLog diffStat diff prompt message rawMessage choice

  branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null)"
  gitLog="$(git log -10 --pretty=format:"<commit>%n%h%n%B%n</commit>" 2>/dev/null)"
  diffStat="$(git diff --cached --stat 2>/dev/null)"
  diff="$(git diff --cached 2>/dev/null)"

  # Normalize newlines
  branch="$(printf "%s" "$branch" | tr -d '\r')"
  gitLog="$(printf "%s" "$gitLog" | tr -d '\r')"
  diffStat="$(printf "%s" "$diffStat" | tr -d '\r')"
  diff="$(printf "%s" "$diff" | tr -d '\r')"

  if [[ -z "$diffStat" ]]; then
    echo "❌ No staged changes. Run git add first." >&2
    return 1
  fi

  while true; do
    prompt=$(cat <<EOF
You are writing a git commit message.

IMPORTANT:
- ALWAYS use real newlines when you want a multiline commit message
- DO NOT surround the answer with quotes
- Return ONLY the commit message text, nothing else

Branch:
$branch

Recent commits (subject + body raw):
$gitLog

Diff stat:
$diffStat

Full staged diff:
$diff

Rules:
- concise but descriptive
- infer the commit style from the recent commits
- if recent commits include a body, include a body if useful
- preserve the repository's usual formatting conventions
- include branch name when relevant for ticket/reference context
- no emojis
- no fluff
EOF
)

    rawMessage="$(opencode run --model "$MODEL" "$prompt")"

    if [[ -z "$rawMessage" ]]; then
      echo "❌ Failed to generate commit message." >&2
      return 1
    fi

    message="$(printf "%s" "$rawMessage" | tr -d '\r')"

    if [[ -z "$(printf "%s" "$message" | tr -d '[:space:]')" ]]; then
      echo "❌ opencode returned an empty commit message." >&2
      return 1
    fi

    echo ""
    echo "──────────────── commit preview ────────────────"
    printf "%s\n" "$message"
    echo "───────────────────────────────────────────────"
    echo ""

    echo -n "[Y] yes  |  [R] retry  |  [C] cancel: "
    read choice

    case "${choice:l}" in
      y)
        if printf "%s\n" "$message" | git commit -F -; then
          echo "✨ Commit created. Tiny machine goblin satisfied."
          return 0
        fi

        echo "❌ Commit failed. Nothing was committed." >&2
        return 1
        ;;
      r)
        echo "🔁 Retrying... maybe the robot was feeling silly."
        continue
        ;;
      c)
        echo "🚫 Commit cancelled. Nothing was committed."
        return 0
        ;;
      *)
        echo "🤨 Expected Y, R, or C. Let's try again."
        continue
        ;;
    esac
  done
}

# ============================================================================
# Zoxide initialization
# ============================================================================

if command -v zoxide &>/dev/null; then
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

  echo "Welcome to Ahri's cleanup ritual... 💫"
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
      y|Y)
        if [[ -d "$item" && ! -L "$item" ]]; then
          echo "Note: '$item' (directory) is marked for recursive deletion. 🌬️"
          deletions_to_perform["$item"]="directory"
        else
          echo "Note: '$item' (file) is marked for deletion. 🍂"
          deletions_to_perform["$item"]="file"
        fi
        ;;
      n|N)
        echo "'$item' will stay for now. 💖"
        ;;
      q|Q)
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

  if (( ${#deletions_to_perform[@]} == 0 )); then
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
fi

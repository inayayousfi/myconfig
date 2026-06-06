## Etapes

```powershell
wsl --install archlinux
```

```bash
echo "root:root" | chpasswd

pacman -Syu
pacman -Syu sudo git base-devel wget curl unzip zip man-db man-pages vi rustup

tee -a /etc/wsl.conf >/dev/null <<'EOF'
[interop]
enabled=true
EOF

exit
```

```powershell
wsl --shutdown
wsl -s archlinux
wsl
```

```bash
WINUSER="$(cmd.exe /c echo %USERNAME% 2>/dev/null | tr -d '\r')"

useradd -m -G wheel "$WINUSER"

echo "$WINUSER:$WINUSER" | chpasswd

sed -i 's/^# *%wheel ALL=(ALL:ALL) ALL/%wheel ALL=(ALL:ALL) ALL/' /etc/sudoers

echo "$WINUSER ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

tee -a /etc/wsl.conf >/dev/null <<EOF
[user]
default=$WINUSER
EOF

sed -i 's/^#\s*en_US.UTF-8 UTF-8/en_US.UTF-8 UTF-8/' /etc/locale.gen
locale-gen
echo 'LANG=en_US.UTF-8' | tee /etc/locale.conf > /dev/null

exit
```

```powershell
wsl --shutdown
wsl
```

```bash
cd
rustup default stable

git clone https://aur.archlinux.org/paru.git
cd paru
makepkg -si

cd ..

paru -Syu --noconfirm --skipreview zsh rsync stow wsl2-ssh-agent ripgrep go yazi-git ffmpeg 7zip jq poppler fd ripgrep fzf zoxide resvg imagemagick bat eza llvm nvm python	fastfetch

chsh -s /usr/bin/zsh

RUNZSH=no CHSH=no KEEP_ZSHRC=yes sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)"

DOTFILES_REPO=/mnt/c/Users/ziede/Projects/myconfig/dotfiles
USER_DOTFILES_DIR=${HOME}/dotfiles

log() {
    printf '[INFO] %s\n' "$1"
}

error() {
    printf '[ERROR] %s\n' "$1" >&2
}

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        error "Missing required command: $1"
    fi
}

copy_dotfiles_dir() {
    local source_dir="$1"
    shift

    if [ ! -d "$source_dir" ]; then
        return 0
    fi

    for dir in "$@"; do
        if [ -d "$source_dir/$dir" ]; then
            log "Copying $dir from $source_dir to $USER_DOTFILES_DIR"
            rsync -a --delete "$source_dir/$dir/" "$USER_DOTFILES_DIR/$dir/"
        else
            log "Skipping missing directory: $source_dir/$dir"
        fi
    done
}

force_stow() {
    local package="$1"

    find "$USER_DOTFILES_DIR/$package" -type f -print0 | while IFS= read -r -d '' file; do
        local relative
        relative="${file#$USER_DOTFILES_DIR/$package/}"

        rm -rf "$HOME/$relative"
    done

    stow \
        --dir "$USER_DOTFILES_DIR" \
        --target "$HOME" \
        --restow \
        "$package"
}

stow_packages() {
    if [ ! -d "$USER_DOTFILES_DIR" ]; then
        error "Dotfiles folder does not exist: $USER_DOTFILES_DIR"
    fi

    log "Stowing packages from $USER_DOTFILES_DIR into $HOME"

    while IFS= read -r -d '' package_dir; do
        local package
        package="$(basename "$package_dir")"

        log "Force stowing $package"
        force_stow "$package"
    done < <(find "$USER_DOTFILES_DIR" -mindepth 1 -maxdepth 1 -type d -print0 | sort -z)
}

main() {
    require_command rsync
    require_command find
    require_command sort
    require_command stow

    if [ ! -d "$DOTFILES_REPO" ]; then
        error "Repo clone not found: $DOTFILES_REPO"
        exit 1
    fi

    mkdir -p "$USER_DOTFILES_DIR"

    copy_dotfiles_dir "$DOTFILES_REPO" \
        "zsh" \
        "yazi"

    stow_packages
}

main "$@"

git clone https://github.com/zsh-users/zsh-autosuggestions ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-autosuggestions
git clone https://github.com/zsh-users/zsh-syntax-highlighting.git ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-syntax-highlighting
  
eval "$(/usr/bin/wsl2-ssh-agent)"

rm -rf ~/paru/

mkdir -p "${HOME}/Projects"

exit
```
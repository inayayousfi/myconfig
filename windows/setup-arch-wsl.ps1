param(
  [string]$Distro = "archlinux",
  [string]$DotfilesPath
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

$Colors = @{
  Info    = 'Cyan'
  Success = 'Green'
  Warning = 'Yellow'
  Error   = 'Red'
}

function Write-Log
{
  param(
    [string]$Message,
    [ValidateSet('INFO', 'OK', 'WARNING', 'ERROR')][string]$Level = 'INFO'
  )

  switch ($Level)
  {
    'OK' { Write-Host "[OK] $Message" -ForegroundColor $Colors.Success }
    'WARNING' { Write-Host "[WARNING] $Message" -ForegroundColor $Colors.Warning }
    'ERROR' { Write-Host "[ERROR] $Message" -ForegroundColor $Colors.Error }
    default { Write-Host "[INFO] $Message" -ForegroundColor $Colors.Info }
  }
}

function Test-CommandExists
{
  param([string]$Command)
  return [bool](Get-Command $Command -ErrorAction SilentlyContinue)
}

function ConvertTo-WslPath
{
  param([string]$Path)

  $resolvedPath = [System.IO.Path]::GetFullPath($Path)
  $drive = $resolvedPath.Substring(0, 1).ToLowerInvariant()
  $pathWithoutDrive = $resolvedPath.Substring(2).Replace('\', '/')

  return "/mnt/$drive$pathWithoutDrive"
}

function ConvertTo-Lf
{
  param([string]$Text)

  return $Text.Replace("`r`n", "`n").Replace("`r", "`n")
}

function ConvertTo-Base64Script
{
  param([string]$Script)

  $bytes = [System.Text.Encoding]::UTF8.GetBytes((ConvertTo-Lf $Script))
  return [Convert]::ToBase64String($bytes)
}

function Invoke-WslRootScript
{
  param([string]$Script)
  ConvertTo-Base64Script $Script | wsl -d $Distro -u root -- bash -c 'base64 -d | bash -s'

  if ($LASTEXITCODE -ne 0)
  {
    throw "WSL root script failed with exit code $LASTEXITCODE."
  }
}

function Invoke-WslUserScript
{
  param([string]$Script)
  ConvertTo-Base64Script $Script | wsl -d $Distro -- bash -c 'base64 -d | bash -s'

  if ($LASTEXITCODE -ne 0)
  {
    throw "WSL user script failed with exit code $LASTEXITCODE."
  }
}

function Install-Distro
{
  $installArgs = @('--install', '--distribution', $Distro, '--no-launch')

  try
  {
    & wsl @installArgs
  }
  catch
  {
    Write-Log "WSL install with --no-launch failed; retrying without it..." -Level 'WARNING'
    & wsl --install --distribution $Distro
  }
}

function Unregister-ExistingDistro
{
  $existingDistros = @(wsl --list --quiet 2>$null | ForEach-Object { $_.Trim([char]0xFEFF).Trim() } | Where-Object { $_ })

  if ($existingDistros -contains $Distro)
  {
    Write-Log "Existing WSL distro '$Distro' found. Unregistering it before reinstalling..." -Level 'WARNING'
    wsl --terminate $Distro 2>$null
    wsl --unregister $Distro
  }
}

function Main
{
  if ([string]::IsNullOrWhiteSpace($DotfilesPath))
  {
    $DotfilesPath = Join-Path $RepoRoot "dotfiles"
  }

  $dotfilesWslPath = ConvertTo-WslPath -Path $DotfilesPath
  $wslUser = [Environment]::UserName

  if (-not (Test-CommandExists "wsl"))
  {
    Write-Log "wsl not found. Install or enable Windows Subsystem for Linux first." -Level 'ERROR'
    exit 1
  }

  Write-Log "Using dotfiles path: $DotfilesPath"
  Write-Log "Using WSL dotfiles path: $dotfilesWslPath"

  Unregister-ExistingDistro

  Write-Log "Installing $Distro..."
  Install-Distro

  Write-Log "Phase 1: root bootstrap..."
  Invoke-WslRootScript @'
set -euo pipefail

log() { printf '[arch-wsl][root bootstrap] %s\n' "$*"; }

log "Setting root password"
echo "root:root" | chpasswd

log "Initializing pacman keys"
pacman-key --init
pacman-key --populate archlinux

log "Updating system packages"
pacman -Syu --noconfirm

log "Installing base packages"
pacman -S --noconfirm sudo git base-devel wget curl unzip zip man-db man-pages vi rustup

log "Writing /etc/wsl.conf"
cat >/etc/wsl.conf <<'EOF'
[interop]
enabled=true
EOF
'@

  wsl --shutdown
  wsl -s $Distro

  Write-Log "Phase 2: user setup..."
  $userSetupScript = @'
set -euo pipefail

log() { printf '[arch-wsl][user setup] %s\n' "$*"; }

WINUSER="__WSL_USER__"

log "Creating or updating user $WINUSER"
id "$WINUSER" >/dev/null 2>&1 || useradd -m -G wheel "$WINUSER"
echo "${WINUSER}:${WINUSER}" | chpasswd

log "Configuring sudoers"
sed -i 's/^# *%wheel ALL=(ALL:ALL) ALL/%wheel ALL=(ALL:ALL) ALL/' /etc/sudoers

grep -q "^$WINUSER ALL=(ALL) NOPASSWD:ALL" /etc/sudoers || \
    echo "$WINUSER ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

log "Writing /etc/wsl.conf default user"
cat >/etc/wsl.conf <<EOF
[interop]
enabled=true

[user]
default=$WINUSER
EOF

log "Generating locale"
sed -i 's/^#\s*en_US.UTF-8 UTF-8/en_US.UTF-8 UTF-8/' /etc/locale.gen
locale-gen
echo 'LANG=en_US.UTF-8' > /etc/locale.conf
'@

  Invoke-WslRootScript ($userSetupScript.Replace('__WSL_USER__', $wslUser))

  wsl --shutdown

  Write-Log "Phase 3: user packages and dotfiles..."
  $userPackagesScript = @'
set -euo pipefail

log() { printf '[arch-wsl][user packages] %s\n' "$*"; }

DOTFILES_REPO="__DOTFILES_WSL_PATH__"
USER_DOTFILES_DIR="$HOME/dotfiles"

cd
log "Setting Rust toolchain"
rustup default stable

if [ ! -d paru ]; then
    log "Cloning paru"
    git clone https://aur.archlinux.org/paru.git
fi

log "Building and installing paru"
cd paru
makepkg -si --noconfirm
cd ..

log "Installing user packages"
paru -Syu --noconfirm --skipreview \
    zsh rsync stow wsl2-ssh-agent ripgrep go yazi-git ffmpeg 7zip jq poppler fd fzf zoxide \
    resvg imagemagick bat eza llvm nvm python fastfetch lazygit jdk-openjdk maven make cmake \
    btop tokei

log "Configuring Git core settings"
git config --global core.sshCommand ssh.exe
git config --global core.symlinks true

log "Installing Oh My Zsh"
RUNZSH=no CHSH=no KEEP_ZSHRC=yes \
    sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)"

log "Preparing dotfiles directory"
mkdir -p "$USER_DOTFILES_DIR"

if [ -d "$DOTFILES_REPO" ]; then
    log "Syncing dotfiles from $DOTFILES_REPO"
    for dir in zsh yazi lazygit; do
        if [ -d "$DOTFILES_REPO/$dir" ]; then
            rsync -a --delete "$DOTFILES_REPO/$dir/" "$USER_DOTFILES_DIR/$dir/"
        fi
    done

    log "Stowing dotfiles"
    for package_dir in "$USER_DOTFILES_DIR"/*; do
        [ -d "$package_dir" ] || continue
        package="$(basename "$package_dir")"

        find "$USER_DOTFILES_DIR/$package" -type f -print0 | while IFS= read -r -d '' file; do
            relative="${file#$USER_DOTFILES_DIR/$package/}"
            rm -rf "$HOME/$relative"
        done

        stow --dir "$USER_DOTFILES_DIR" --target "$HOME" --restow "$package"
    done
else
    echo "[WARN] Dotfiles repo not found: $DOTFILES_REPO"
fi

log "Installing zsh plugins"
git clone https://github.com/zsh-users/zsh-autosuggestions "${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}/plugins/zsh-autosuggestions" || true
git clone https://github.com/zsh-users/zsh-syntax-highlighting.git "${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}/plugins/zsh-syntax-highlighting" || true

log "Starting wsl2-ssh-agent if available"
eval "$(/usr/bin/wsl2-ssh-agent)" || true

log "Installing latest LTS Node.js"
export NVM_DIR="$HOME/.nvm"
source /usr/share/nvm/init-nvm.sh
set +u
nvm install --lts
nvm use --lts
set -u

log "Cleaning up"
rm -rf "$HOME/paru"
mkdir -p "$HOME/Projects"

'@

  Invoke-WslUserScript ($userPackagesScript.Replace('__DOTFILES_WSL_PATH__', $dotfilesWslPath))

  Write-Log "Phase 4: enforcing default shell..."
  $shellScript = @'
set -euo pipefail

log() { printf '[arch-wsl][shell] %s\n' "$*"; }

WINUSER="__WSL_USER__"
ZSHPATH="$(command -v zsh)"

if [ -z "$ZSHPATH" ]; then
    echo "zsh is not installed or not on PATH" >&2
    exit 1
fi

log "Setting default shell for $WINUSER to $ZSHPATH"
usermod --shell "$ZSHPATH" "$WINUSER"

ACTUAL_SHELL="$(getent passwd "$WINUSER" | cut -d: -f7)"
if [ "$ACTUAL_SHELL" != "$ZSHPATH" ]; then
    echo "Expected $WINUSER shell to be $ZSHPATH, got $ACTUAL_SHELL" >&2
    exit 1
fi

log "Default shell verified for $WINUSER"
'@

  Invoke-WslRootScript ($shellScript.Replace('__WSL_USER__', $wslUser))

  wsl --shutdown
  Write-Log "Arch WSL setup finished." -Level 'OK'
}

Main

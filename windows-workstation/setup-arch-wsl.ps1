param(
    [string]$Distro,
    [string]$DotfilesPath
)

$ErrorActionPreference = "Stop"

$ArchSourceDistro = "archlinux"
$Distro = if ([string]::IsNullOrWhiteSpace($Distro)) {
    "$(([Net.Dns]::GetHostName()).ToLowerInvariant())-subsystem"
} else {
    $Distro.ToLowerInvariant()
}

if ($Distro -notmatch '^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$') {
    throw "WSL distribution name must also be a valid lowercase Linux hostname."
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

$Colors = @{
    Info    = 'Cyan'
    Success = 'Green'
    Warning = 'Yellow'
    Error   = 'Red'
}

function Write-Log {
    param(
        [string]$Message,
        [ValidateSet('INFO', 'OK', 'WARNING', 'ERROR')][string]$Level = 'INFO'
    )

    switch ($Level) {
        'OK' { Write-Host "[OK] $Message" -ForegroundColor $Colors.Success }
        'WARNING' { Write-Host "[WARNING] $Message" -ForegroundColor $Colors.Warning }
        'ERROR' { Write-Host "[ERROR] $Message" -ForegroundColor $Colors.Error }
        default { Write-Host "[INFO] $Message" -ForegroundColor $Colors.Info }
    }
}

function Test-CommandExists {
    param([string]$Command)
    return [bool](Get-Command $Command -ErrorAction SilentlyContinue)
}

function ConvertTo-WslPath {
    param([string]$Path)

    $resolvedPath = [System.IO.Path]::GetFullPath($Path)
    $drive = $resolvedPath.Substring(0, 1).ToLowerInvariant()
    $pathWithoutDrive = $resolvedPath.Substring(2).Replace('\', '/')

    return "/mnt/$drive$pathWithoutDrive"
}

function ConvertTo-Lf {
    param([string]$Text)

    return $Text.Replace("`r`n", "`n").Replace("`r", "`n")
}

function ConvertTo-Base64Script {
    param([string]$Script)

    $bytes = [System.Text.Encoding]::UTF8.GetBytes((ConvertTo-Lf $Script))
    return [Convert]::ToBase64String($bytes)
}

function Invoke-WslRootScript {
    param([string]$Script)
    ConvertTo-Base64Script $Script | wsl -d $Distro -u root -- bash -c 'base64 -d | bash -s'

    if ($LASTEXITCODE -ne 0) {
        throw "WSL root script failed with exit code $LASTEXITCODE."
    }
}

function Invoke-WslUserScript {
    param([string]$Script)
    ConvertTo-Base64Script $Script | wsl -d $Distro -- bash -c 'base64 -d | bash -s'

    if ($LASTEXITCODE -ne 0) {
        throw "WSL user script failed with exit code $LASTEXITCODE."
    }
}

function Install-Distro {
    $installArgs = @('--install', $ArchSourceDistro, '--name', $Distro, '--no-launch')

    & wsl @installArgs
    if ($LASTEXITCODE -eq 0) {
        return
    }

    Write-Log "WSL install with --no-launch failed; retrying without it..." -Level 'WARNING'
    & wsl --install $ArchSourceDistro --name $Distro
    if ($LASTEXITCODE -ne 0) {
        throw "WSL installation failed with exit code $LASTEXITCODE."
    }
}

function Unregister-ExistingDistro {
    $existingDistros = @(wsl --list --quiet 2>$null | ForEach-Object { $_.Trim([char]0xFEFF).Trim() } | Where-Object { $_ })

    if ($existingDistros -contains $Distro) {
        Write-Log "Existing WSL distro '$Distro' found. Unregistering it before reinstalling..." -Level 'WARNING'
        wsl --terminate $Distro 2>$null
        wsl --unregister $Distro
    }
}

function Install-WslAutostart {
    # The T3 Code backend is a systemd *user* unit. Lingering keeps it alive with no
    # shell open, but only from inside the instance. It cannot stop WSL from tearing
    # the instance down, and two independent timeouts do exactly that:
    #
    #   vmIdleTimeout       [wsl2]     stops the utility VM.       Default 60000 ms.
    #   instanceIdleTimeout [general]  stops the distro instance.  Default 15000 ms.
    #
    # Setting only vmIdleTimeout leaves the instance timeout at its default, so the
    # distro still stops 15-20 s after the last terminal closes. Both are needed.
    $wslConfigPath = Join-Path $env:USERPROFILE ".wslconfig"
    $wslConfig = @"
[general]
instanceIdleTimeout=-1

[wsl2]
vmIdleTimeout=-1
"@

    Write-Log "Writing $wslConfigPath"

    # Not Set-Content -Encoding UTF8: on Windows PowerShell 5.1 that emits a BOM,
    # and WSL's .wslconfig parser rejects the whole file over it, silently ignoring
    # every key. The failure looks exactly like the timeouts never being set.
    [System.IO.File]::WriteAllText($wslConfigPath, $wslConfig, (New-Object System.Text.UTF8Encoding $false))

    # Disabling the timeouts stops WSL shutting the instance down. Nothing starts it
    # either, so a logon shortcut supplies the other half. wsl.exe is a console
    # program, hence the hidden PowerShell wrapper. -NoProfile matters: without it
    # every logon loads the Oh My Posh profile this repo installs, for nothing.
    $startupDir = [Environment]::GetFolderPath("Startup")
    $shortcutPath = Join-Path $startupDir "myconfig-wsl-autostart.lnk"
    $powershellPath = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"

    Write-Log "Creating logon shortcut $shortcutPath"

    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutPath)
    $shortcut.TargetPath = $powershellPath
    $shortcut.Arguments = "-NoLogo -NoProfile -WindowStyle Hidden -Command `"wsl.exe -d $Distro --exec /bin/true`""
    $shortcut.Description = "Boot the $Distro WSL instance at logon so its user services run without a terminal."
    # Belt and braces: powershell.exe paints a console for a fraction of a second
    # before it applies -WindowStyle Hidden. Minimized keeps that off the desktop.
    $shortcut.WindowStyle = 7
    $shortcut.Save()
}

function Main {
    if ([string]::IsNullOrWhiteSpace($DotfilesPath)) {
        $DotfilesPath = Join-Path $RepoRoot "dotfiles"
    }

    $dotfilesWslPath = ConvertTo-WslPath -Path $DotfilesPath
    $repoWslPath = ConvertTo-WslPath -Path $RepoRoot
    $wslUser = [Environment]::UserName

    # Git inside WSL authenticates against the Windows OpenSSH agent, so it must call
    # the Windows client rather than Arch's. A bare "ssh.exe" only resolves while WSL
    # appends the Windows PATH to its own, and that is off on plenty of setups, so the
    # config silently breaks every SSH remote. Resolve it here instead, from the real
    # Windows directory, and keep it empty when the optional OpenSSH client is absent.
    $sshExePath = Join-Path $env:SystemRoot "System32\OpenSSH\ssh.exe"
    $sshExeWslPath = ""

    if (Test-Path -LiteralPath $sshExePath) {
        $sshExeWslPath = ConvertTo-WslPath -Path $sshExePath
    } else {
        Write-Log "Windows OpenSSH client not found at $sshExePath. Git in WSL will use Arch's own ssh." -Level 'WARNING'
    }

    if (-not (Test-CommandExists "wsl")) {
        Write-Log "wsl not found. Install or enable Windows Subsystem for Linux first." -Level 'ERROR'
        exit 1
    }

    Write-Log "Using dotfiles path: $DotfilesPath"
    Write-Log "Using WSL dotfiles path: $dotfilesWslPath"

    Unregister-ExistingDistro

    Write-Log "Installing $Distro..."
    Install-Distro

    Write-Log "Phase 1: root bootstrap..."
    $rootBootstrapScript = @'
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
pacman -S --noconfirm sudo git base-devel wget curl unzip zip man-db man-pages vi rustup openssh polkit

log "Generating SSH host keys"
ssh-keygen -A

# systemd is not running yet on this first boot, so enable offline. This only
# writes the multi-user.target.wants symlink, which the next boot acts on.
log "Enabling sshd for systemd boot"
SYSTEMD_OFFLINE=1 systemctl enable sshd

log "Writing /etc/wsl.conf"
cat >/etc/wsl.conf <<'EOF'
[interop]
enabled=true

[network]
hostname=__WSL_HOSTNAME__

[boot]
systemd=true
EOF
'@

    Invoke-WslRootScript ($rootBootstrapScript.Replace('__WSL_HOSTNAME__', $Distro))

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

# User services must survive with no shell open. loginctl needs logind to be
# up, so fall back to the on-disk marker it would have written.
log "Enabling lingering for $WINUSER"
loginctl enable-linger "$WINUSER" 2>/dev/null || \
    install -Dm644 /dev/null "/var/lib/systemd/linger/$WINUSER"

log "Restricting sshd to loopback and $WINUSER"
cat >/etc/ssh/sshd_config.d/10-local-only.conf <<EOF
ListenAddress 127.0.0.1
ListenAddress ::1
AllowUsers $WINUSER
EOF

log "Writing /etc/wsl.conf default user"
cat >/etc/wsl.conf <<EOF
[interop]
enabled=true

[user]
default=$WINUSER

[network]
hostname=__WSL_HOSTNAME__

[boot]
systemd=true
EOF

log "Generating locale"
sed -i 's/^#\s*en_US.UTF-8 UTF-8/en_US.UTF-8 UTF-8/' /etc/locale.gen
locale-gen
echo 'LANG=en_US.UTF-8' > /etc/locale.conf
'@

    Invoke-WslRootScript ($userSetupScript.Replace('__WSL_USER__', $wslUser).Replace('__WSL_HOSTNAME__', $Distro))

    wsl --shutdown

    Write-Log "Phase 3: shared Arch development profile..."
    $userPackagesScript = @'
set -euo pipefail

export MYCONFIG_DOTFILES_SOURCE="__DOTFILES_WSL_PATH__"
export MYCONFIG_WINDOWS_SSH="__SSH_EXE_WSL_PATH__"
exec bash "__REPO_WSL_PATH__/linux/install.sh" arch-wsl
'@

    Invoke-WslUserScript ($userPackagesScript.Replace('__DOTFILES_WSL_PATH__', $dotfilesWslPath).Replace('__SSH_EXE_WSL_PATH__', $sshExeWslPath).Replace('__REPO_WSL_PATH__', $repoWslPath))

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

    Write-Log "Phase 5: keeping the instance alive across sessions..."
    Install-WslAutostart

    # The shutdown has to come after the config write. WSL reads .wslconfig only when
    # the VM starts, so this is what makes the new timeouts take effect at all.
    wsl --shutdown

    # Bring the instance back up the same way the logon shortcut does, so the T3 Code
    # service is already running when the script ends instead of at the next logon.
    Write-Log "Booting $Distro under the new idle timeouts..."
    wsl -d $Distro --exec /bin/true

    if ($LASTEXITCODE -ne 0) {
        throw "Could not boot $Distro after writing .wslconfig (exit code $LASTEXITCODE)."
    }

    Write-Log "Arch WSL setup finished." -Level 'OK'
}

Main

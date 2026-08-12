# Windows Development Environment Uninstall Script
# This script removes everything installed by install.ps1
# Use with caution - this will remove configurations and installed packages

$ErrorActionPreference = 'Stop'

# Colors
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
        'OK'      { Write-Host "[OK] $Message" -ForegroundColor $Colors.Success }
        'WARNING' { Write-Host "[WARNING] $Message" -ForegroundColor $Colors.Warning }
        'ERROR'   { Write-Host "[ERROR] $Message" -ForegroundColor $Colors.Error }
        default   { Write-Host "[INFO] $Message" -ForegroundColor $Colors.Info }
    }
}

# ============================================================================
# Confirmation
# ============================================================================

function Confirm-Uninstall {
    Write-Host ""
    Write-Host "+================================================================+" -ForegroundColor Red
    Write-Host "|       Windows Development Environment Uninstaller                |" -ForegroundColor Red
    Write-Host "|                     WARNING                                      |" -ForegroundColor Yellow
    Write-Host "+================================================================+" -ForegroundColor Red
    Write-Host ""
    Write-Log "This script will remove:" -Level 'WARNING'
    Write-Host "  - Neovim configuration"
    Write-Host "  - Yazi configuration"
    Write-Host "  - Lazygit configuration"
    Write-Host "  - Oh My Posh configuration"
    Write-Host "  - WezTerm configuration (restored from backup if available)"
    Write-Host "  - GlazeWM and Zebar configuration"
    Write-Host "  - AutoHotkey scripts"
    Write-Host "  - WSL logon autostart shortcut and %UserProfile%\.wslconfig"
    Write-Host "  - PowerShell profile (restored from backup if available)"
    Write-Host "  - PSReadLine module"
    Write-Host "  - GnuWin32 PATH entry"
    Write-Host "  - Registry tweaks"
    Write-Host "  - AI config files (CLAUDE.md, skills)"
    Write-Host "  - (Optionally) Winget packages installed by the setup"
    Write-Host ""
    $confirm = Read-Host "Are you sure you want to continue? (yes/no)"
    if ($confirm -ne "yes") {
        Write-Log "Uninstall cancelled."
        exit 0
    }
    Write-Host ""
}

# ============================================================================
# Remove Neovim Configuration
# ============================================================================

function Remove-NeovimConfig {
    $nvimConfig = Join-Path $env:LOCALAPPDATA "nvim"
    $nvimData = Join-Path $env:LOCALAPPDATA "nvim-data"

    if (Test-Path $nvimConfig) {
        Write-Log "Removing Neovim configuration..."
        Remove-Item -Path $nvimConfig -Recurse -Force
        Write-Log "Neovim configuration removed" -Level 'OK'
    } else {
        Write-Log "Neovim configuration not found, skipping"
    }

    if (Test-Path $nvimData) {
        Write-Log "Removing Neovim data..."
        Remove-Item -Path $nvimData -Recurse -Force
        Write-Log "Neovim data removed" -Level 'OK'
    }
}

# ============================================================================
# Remove Yazi Configuration
# ============================================================================

function Remove-YaziConfig {
    $yaziConfig = Join-Path $env:APPDATA "yazi"

    if (Test-Path $yaziConfig) {
        Write-Log "Removing Yazi configuration..."
        Remove-Item -Path $yaziConfig -Recurse -Force
        Write-Log "Yazi configuration removed" -Level 'OK'
    } else {
        Write-Log "Yazi configuration not found, skipping"
    }
}

# ============================================================================
# Remove Lazygit Configuration
# ============================================================================

function Remove-LazygitConfig {
    $configPath = Join-Path $env:LOCALAPPDATA "lazygit\config.yml"
    $configDir = Split-Path -Parent $configPath

    if (Test-Path $configPath) {
        Write-Log "Removing Lazygit configuration..."
        Remove-Item -Path $configPath -Force
        if ((Test-Path $configDir) -and -not (Get-ChildItem -Path $configDir -Force -ErrorAction SilentlyContinue)) {
            Remove-Item -Path $configDir -Force
        }
        Write-Log "Lazygit configuration removed" -Level 'OK'
    } else {
        Write-Log "Lazygit configuration not found, skipping"
    }
}

# ============================================================================
# Remove Oh My Posh Configuration
# ============================================================================

function Remove-OhMyPoshConfig {
    $ohMyPoshConfig = Join-Path (Split-Path $PROFILE) "black-pink.omp.json"

    if (Test-Path $ohMyPoshConfig) {
        Write-Log "Removing Oh My Posh configuration..."
        Remove-Item -Path $ohMyPoshConfig -Force
        Write-Log "Oh My Posh configuration removed" -Level 'OK'
    } else {
        Write-Log "Oh My Posh configuration not found, skipping"
    }
}

# ============================================================================
# Remove WezTerm Configuration
# ============================================================================

function Remove-WezTermConfig {
    $configPath = Join-Path $env:USERPROFILE ".wezterm.lua"

    if (-not (Test-Path $configPath)) {
        Write-Log "WezTerm config not found, skipping"
        return
    }

    # Try to restore from backup
    $backups = Get-ChildItem -Path (Split-Path -Parent $configPath) -Filter ".wezterm.lua.backup.*" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending
    if ($backups.Count -gt 0) {
        $latestBackup = $backups[0].FullName
        Write-Log "Restoring WezTerm config from backup: $latestBackup"
        Copy-Item -Path $latestBackup -Destination $configPath -Force
        Write-Log "WezTerm config restored from backup" -Level 'OK'

        # Clean up backups
        foreach ($backup in $backups) {
            Remove-Item -Path $backup.FullName -Force
        }
    } else {
        Write-Log "Removing WezTerm config..."
        Remove-Item -Path $configPath -Force
        Write-Log "WezTerm config removed" -Level 'OK'
    }
}

# ============================================================================
# Remove GlazeWM and Zebar Configuration
# ============================================================================

function Remove-GlazeWMConfig {
    $glzrDir = Join-Path $env:USERPROFILE ".glzr"

    if (Test-Path $glzrDir) {
        Write-Log "Removing GlazeWM and Zebar configuration..."
        Remove-Item -Path $glzrDir -Recurse -Force
        Write-Log "GlazeWM and Zebar configuration removed" -Level 'OK'
    } else {
        Write-Log "GlazeWM configuration not found, skipping"
    }
}

# ============================================================================
# Remove AutoHotkey Scripts
# ============================================================================

function Remove-AHKScripts {
    $ahkDir = Join-Path $env:USERPROFILE "AHK"

    if (Test-Path $ahkDir) {
        Write-Log "Removing AutoHotkey scripts..."
        Remove-Item -Path $ahkDir -Recurse -Force
        Write-Log "AutoHotkey scripts removed" -Level 'OK'
    } else {
        Write-Log "AutoHotkey scripts not found, skipping"
    }
}

# ============================================================================
# Remove WSL Autostart
# ============================================================================

function Remove-WslAutostart {
    $shortcutPath = Join-Path ([Environment]::GetFolderPath("Startup")) "myconfig-wsl-autostart.lnk"
    $wslConfigPath = Join-Path $env:USERPROFILE ".wslconfig"

    if (Test-Path $shortcutPath) {
        Write-Log "Removing WSL autostart shortcut..."
        Remove-Item -Path $shortcutPath -Force
        Write-Log "WSL autostart shortcut removed" -Level 'OK'
    } else {
        Write-Log "WSL autostart shortcut not found, skipping"
    }

    # Written wholesale by setup-arch-wsl.ps1, so it is ours to delete. Leaving it
    # behind would keep the WSL VM pinned in memory forever after an uninstall,
    # with nothing on screen explaining why.
    if (Test-Path $wslConfigPath) {
        Write-Log "Removing .wslconfig..."
        Remove-Item -Path $wslConfigPath -Force
        Write-Log ".wslconfig removed" -Level 'OK'
    } else {
        Write-Log ".wslconfig not found, skipping"
    }
}

# ============================================================================
# Remove AI Config (Claude Code CLAUDE.md + skills)
# ============================================================================

function Remove-AIConfig {
    $claudeMd = Join-Path $env:USERPROFILE ".claude\CLAUDE.md"

    # Derive the skill list from the repo instead of hardcoding names. A hardcoded
    # list goes stale the moment a skill is added, renamed or dropped, and the
    # uninstaller then claims to have removed skills it left behind.
    # $PSScriptRoot, not $MyInvocation: inside a function the latter describes the
    # call site, not the script file, so it would resolve to nothing here.
    $repoRoot = Split-Path -Parent $PSScriptRoot
    $skillsSource = Join-Path $repoRoot "dotfiles\ai\.claude\skills"

    if (Test-Path $skillsSource) {
        $skillsToRemove = @(Get-ChildItem -Path $skillsSource -Directory | Select-Object -ExpandProperty Name)
    } else {
        # Without the repo we cannot tell our skills from third-party ones, and
        # deleting the whole skills directory would take plugins with it.
        $skillsToRemove = @()
        Write-Log "Skill source not found: $skillsSource" -Level 'WARNING'
        Write-Log "Skipping skill removal. Delete them by hand under %USERPROFILE%\.claude\skills" -Level 'WARNING'
    }

    if (Test-Path $claudeMd) {
        Write-Log "Removing AI config CLAUDE.md..."
        Remove-Item -Path $claudeMd -Force
        Write-Log "AI config CLAUDE.md removed" -Level 'OK'
    } else {
        Write-Log "AI config CLAUDE.md not found, skipping"
    }

    foreach ($skill in $skillsToRemove) {
        $skillPath = Join-Path $env:USERPROFILE ".claude\skills\$skill"
        if (Test-Path $skillPath) {
            Write-Log "Removing AI config skill: $skill..."
            Remove-Item -Path $skillPath -Recurse -Force
            Write-Log "AI config skill removed: $skill" -Level 'OK'
        } else {
            Write-Log "AI config skill not found, skipping: $skill"
        }
    }
}

# ============================================================================
# Remove PowerShell Profile
# ============================================================================

function Remove-PowerShellProfile {
    if (-not (Test-Path $PROFILE)) {
        Write-Log "PowerShell profile not found, skipping"
        return
    }

    # Try to restore from backup
    $profileDir = Split-Path -Parent $PROFILE
    $profileName = Split-Path -Leaf $PROFILE
    $backups = Get-ChildItem -Path $profileDir -Filter "$profileName.backup.*" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending

    if ($backups.Count -gt 0) {
        $latestBackup = $backups[0].FullName
        Write-Log "Restoring PowerShell profile from backup: $latestBackup"
        Copy-Item -Path $latestBackup -Destination $PROFILE -Force
        Write-Log "PowerShell profile restored from backup" -Level 'OK'

        # Clean up backups
        foreach ($backup in $backups) {
            Remove-Item -Path $backup.FullName -Force
        }
    } else {
        Write-Log "Removing PowerShell profile..."
        Remove-Item -Path $PROFILE -Force
        Write-Log "PowerShell profile removed" -Level 'OK'
    }
}

# ============================================================================
# Remove PSReadLine Module
# ============================================================================

function Remove-PSReadLineModule {
    if (Get-Module -ListAvailable -Name PSReadLine) {
        Write-Log "Removing PSReadLine module..."
        Uninstall-Module -Name PSReadLine -AllVersions -Force -ErrorAction SilentlyContinue
        Write-Log "PSReadLine module removed" -Level 'OK'
    } else {
        Write-Log "PSReadLine module not found, skipping"
    }
}

# ============================================================================
# Remove GnuWin32 PATH
# ============================================================================

function Remove-GnuWin32Path {
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")

    if ($currentPath -like "*GnuWin32*") {
        Write-Log "Removing GnuWin32 from user PATH..."
        $newPath = ($currentPath.Split(';') | Where-Object { $_ -notlike "*GnuWin32*" }) -join ';'
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Log "GnuWin32 removed from PATH" -Level 'OK'
    } else {
        Write-Log "GnuWin32 not found in PATH, skipping"
    }
}

# ============================================================================
# Remove Registry Tweaks
# ============================================================================

function Remove-RegistryTweaks {
    Write-Log "Reverting registry tweaks (Start Menu recommendations)..."

    $regPath = "HKLM:\SOFTWARE\Policies\Microsoft\Windows\Explorer"
    if (Test-Path $regPath) {
        Remove-ItemProperty -Path $regPath -Name "HideRecommendedSection" -ErrorAction SilentlyContinue
    }

    $regPathUser = "HKCU:\Software\Policies\Microsoft\Windows\Explorer"
    if (Test-Path $regPathUser) {
        Remove-ItemProperty -Path $regPathUser -Name "HideRecommendedSection" -ErrorAction SilentlyContinue
    }

    Write-Log "Registry tweaks reverted" -Level 'OK'
}

# ============================================================================
# Remove Winget Packages (Optional)
# ============================================================================

function Remove-WingetPackages {
    $confirm = Read-Host "Do you also want to uninstall winget packages installed by the setup? (yes/no)"
    if ($confirm -ne "yes") {
        Write-Log "Keeping winget packages"
        return
    }

    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Write-Log "winget not found, skipping package removal" -Level 'WARNING'
        return
    }

    Write-Log "Removing winget packages installed by setup..."

    # Packages installed by the setup (from packages.json)
    # Note: We skip critical system tools like PowerShell and WSL
    $packages = @(
        "AutoHotkey.AutoHotkey"
        "ImageMagick.ImageMagick"
        "Neovim.Neovim"
        "Kitware.CMake"
        "LLVM.LLVM"
        "GnuWin32.Make"
        "BurntSushi.ripgrep.MSVC"
        "Discord.Discord"
        "Gyan.FFmpeg"
        "JesseDuffield.lazygit"
        "Rustlang.Rustup"
        "SST.opencode"
        "ajeetdsouza.zoxide"
        "jqlang.jq"
        "junegunn.fzf"
        "oschwartz10612.Poppler"
        "sxyazi.yazi"
        "Python.PythonInstallManager"
        "JanDeDobbeleer.OhMyPosh"
        "wez.wezterm"
        # T3 Code and Claude Code are no longer installed on Windows; both now live
        # only inside the Arch WSL distro. They stay listed here so an uninstall on
        # a machine provisioned by an earlier version still cleans them up. winget
        # treats a package that is not installed as a no-op.
        "T3Tools.T3Code"
        "Anthropic.ClaudeCode"
    )

    foreach ($package in $packages) {
        Write-Log "Removing $package..."
        winget uninstall --id $package --silent 2>$null
    }

    # Note: We don't remove Git, 7zip, Microsoft.VisualStudio.2022.BuildTools,
    # Microsoft.PowerShell, Microsoft.WSL
    # as they may be system dependencies
    Write-Log "Git, 7zip, VS Build Tools, PowerShell, and WSL were NOT removed as they may be system dependencies" -Level 'WARNING'

    Write-Log "Winget packages removed" -Level 'OK'
}

# ============================================================================
# Main Uninstall Flow
# ============================================================================

function Main {
    # Check if running on Windows
    if ($env:OS -ne "Windows_NT") {
        Write-Log "This script is intended for Windows only." -Level 'ERROR'
        exit 1
    }

    Confirm-Uninstall

    # Remove configurations (in reverse order of installation)
    Remove-NeovimConfig
    Remove-YaziConfig
    Remove-LazygitConfig
    Remove-OhMyPoshConfig
    Remove-WezTermConfig
    Remove-GlazeWMConfig
    Remove-AHKScripts
    Remove-WslAutostart
    Remove-AIConfig
    Remove-PowerShellProfile
    Remove-PSReadLineModule
    Remove-GnuWin32Path
    Remove-RegistryTweaks

    # Optionally remove winget packages
    Remove-WingetPackages

    Write-Host ""
    Write-Host "+================================================================+" -ForegroundColor Green
    Write-Host "|       Uninstall Complete!                                        |" -ForegroundColor Green
    Write-Host "+================================================================+" -ForegroundColor Green
    Write-Host ""
    Write-Log "The development environment has been removed."
    Write-Log "Please restart your terminal for all changes to take effect."
    Write-Host ""
}

Main

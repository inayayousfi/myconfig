# Windows Development Environment Setup Script
# This script is idempotent - running it multiple times is safe
# Dotfiles are copied directly to their target locations (no stow on Windows)
# Keep the script readable for future maintenance and review.

# $ErrorActionPreference = 'Stop' (Disabled to ensure script continues even if some packages fail)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
$SharedDotfilesDir = Join-Path $RepoRoot "dotfiles"
$WindowsDotfilesDir = Join-Path $ScriptDir "dotfiles"

# Colors
# Centralize output colors so log messages stay consistent.
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
    'OK'
    { Write-Host "[OK] $Message" -ForegroundColor $Colors.Success 
    }
    'WARNING'
    { Write-Host "[WARNING] $Message" -ForegroundColor $Colors.Warning 
    }
    'ERROR'
    { Write-Host "[ERROR] $Message" -ForegroundColor $Colors.Error 
    }
    default
    { Write-Host "[INFO] $Message" -ForegroundColor $Colors.Info 
    }
  }
}

# ============================================================================
# Helper Functions
# ============================================================================

function Test-CommandExists
{
  param([string]$Command)
  return [bool](Get-Command $Command -ErrorAction SilentlyContinue)
}

function Copy-DotfileSafe
{
  # Copy a file or directory after ensuring the destination tree exists.
  param(
    [string]$Source,
    [string]$Destination,
    [switch]$Recurse
  )

  if (-not (Test-Path $Source))
  {
    Write-Log "Source not found: $Source" -Level 'WARNING'
    return
  }

  $destDir = if ($Recurse)
  { $Destination
  } else
  { Split-Path -Parent $Destination
  }

  if ([string]::IsNullOrWhiteSpace($destDir))
  {
    Write-Log "Destination directory could not be determined for $Destination" -Level 'ERROR'
    return
  }

  try
  {
    New-Item -ItemType Directory -Path $destDir -Force -ErrorAction Stop | Out-Null
  } catch
  {
    Write-Log "Failed to create destination directory ${destDir}: $($_.Exception.Message)" -Level 'ERROR'
    return
  }

  try
  {
    if ($Recurse)
    {
      Copy-Item -Path "$Source\*" -Destination $Destination -Recurse -Force -ErrorAction Stop
    } else
    {
      Copy-Item -Path $Source -Destination $Destination -Force -ErrorAction Stop
    }
    Write-Log "Copied $Source -> $Destination" -Level 'OK'
  } catch
  {
    Write-Log "Failed to copy $Source -> ${Destination}: $($_.Exception.Message)" -Level 'ERROR'
  }
}

function Move-SharedDesktopToCurrentUser
{
    [CmdletBinding(SupportsShouldProcess = $true)]
    param(
        [switch]$IncludeDefaultDesktop,
        [switch]$MoveEverything
    )

    $dest = [Environment]::GetFolderPath("Desktop")
    $sources = @("$env:PUBLIC\Desktop")

    if ($IncludeDefaultDesktop)
    {
        $sources += "C:\Users\Default\Desktop"
    }

    foreach ($source in $sources)
    {
        if (-not (Test-Path $source))
        { continue 
        }

        $items = if ($MoveEverything)
        {
            Get-ChildItem -LiteralPath $source -Force
        } else
        {
            Get-ChildItem -LiteralPath $source -Force -Include *.lnk, *.url
        }

        foreach ($item in $items)
        {
            $target = Join-Path $dest $item.Name

            if (Test-Path $target)
            {
                $base = [IO.Path]::GetFileNameWithoutExtension($item.Name)
                $ext  = [IO.Path]::GetExtension($item.Name)
                $target = Join-Path $dest "$base - déplacé$ext"
            }

            if ($PSCmdlet.ShouldProcess($item.FullName, "Déplacer vers $target"))
            {
                Move-Item -LiteralPath $item.FullName -Destination $target -Force
            }
        }

        Get-ChildItem -LiteralPath $source -Force | ForEach-Object {
            if ($PSCmdlet.ShouldProcess($_.FullName, "Supprimer définitivement du bureau partagé"))
            {
                Remove-Item -LiteralPath $_.FullName -Recurse -Force
            }
        }
    }
}

# ============================================================================
# Winget Packages Installation
# ============================================================================

function Confirm-InstallPackageGroup
{
  param(
    [string]$Name,
    [string]$Description
  )

  $confirm = Read-Host "Do you want to install $Name ($Description)? (yes/no)"
  return ($confirm -eq "yes" -or $confirm -eq "y")
}

function Import-WingetPackageFile
{
  param(
    [string]$PackagesJson,
    [string]$Name
  )

  if (-not (Test-Path $PackagesJson))
  {
    Write-Log "Winget $Name packages file not found: $PackagesJson" -Level 'ERROR'
    return $false
  }

  Write-Log "Installing winget $Name packages from $PackagesJson..."
  winget import -i $PackagesJson --accept-source-agreements --accept-package-agreements --ignore-unavailable | ForEach-Object { Write-Host $_ }
  Write-Log "Winget $Name packages installed" -Level 'OK'
  return $true
}

function Install-WingetPackages
{
  if (-not (Test-CommandExists "winget"))
  {
    Write-Log "winget not found. Please install App Installer from the Microsoft Store." -Level 'ERROR'
    return $false
  }

  $packagesJson = Join-Path $WindowsDotfilesDir "winget\packages.json"
  $devToolsJson = Join-Path $WindowsDotfilesDir "winget\packages_devtools.json"
  $artJson = Join-Path $WindowsDotfilesDir "winget\packages_art.json"
  $supplementaryJson = Join-Path $WindowsDotfilesDir "winget\packages_supplementary.json"

  $installedDevTools = $false
  $installedSupplementary = $false
  Import-WingetPackageFile -PackagesJson $packagesJson -Name "core" | Out-Null

  if (Confirm-InstallPackageGroup -Name "DevTools" -Description "Rust, C/C++ and build tools")
  {
    $installedDevTools = Import-WingetPackageFile -PackagesJson $devToolsJson -Name "DevTools"
  } else
  {
    Write-Log "Skipping winget DevTools packages"
  }

  if (Confirm-InstallPackageGroup -Name "Art" -Description "Blender, Krita, OBS, MuseScore and Kdenlive")
  {
    Import-WingetPackageFile -PackagesJson $artJson -Name "Art" | Out-Null
  } else
  {
    Write-Log "Skipping winget Art packages"
  }

  if (Confirm-InstallPackageGroup -Name "Supplementary" -Description "Handy, VirtualBox, LibreOffice, Windhawk and Ollama")
  {
    $installedSupplementary = Import-WingetPackageFile -PackagesJson $supplementaryJson -Name "Supplementary"
  } else
  {
    Write-Log "Skipping winget Supplementary packages"
  }

  if (Confirm-InstallPackageGroup -Name "Arch WSL" -Description "fresh Arch Linux WSL distro with packages and dotfiles; unregisters an existing archlinux distro first")
  {
    $archWslScript = Join-Path $ScriptDir "setup-arch-wsl.ps1"
    if (Test-Path $archWslScript)
    {
      & $archWslScript -DotfilesPath $SharedDotfilesDir
    } else
    {
      Write-Log "Arch WSL setup script not found: $archWslScript" -Level 'ERROR'
    }
  } else
  {
    Write-Log "Skipping Arch WSL setup"
  }

  return @{
    DevTools = $installedDevTools
    Supplementary = $installedSupplementary
  }
}

# ============================================================================
# PowerShell Profile
# ============================================================================

function Install-PowerShellProfile
{
  # Install the profile early so the next shell session picks up prompt changes.
  $source = Join-Path $WindowsDotfilesDir "PowerShell\Microsoft.PowerShell_profile.ps1"
  $destination = $PROFILE

  if (-not (Test-Path $source))
  {
    Write-Log "PowerShell profile source not found: $source" -Level 'ERROR'
    return
  }

  # Backup existing profile if it exists and does not look like one we created.
  if ((Test-Path $destination) -and -not (Select-String -Path $destination -Pattern "Oh My Posh" -Quiet -ErrorAction SilentlyContinue))
  {
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $backup = "$destination.backup.$timestamp"
    Copy-Item -Path $destination -Destination $backup -Force
    Write-Log "Backed up existing profile to $backup"
  }

  Copy-DotfileSafe -Source $source -Destination $destination
  # Remove the downloaded-file marker from the installed profile.
  Unblock-File -Path $destination
  Write-Log "PowerShell profile installed" -Level 'OK'
}


# ============================================================================
# Oh My Posh Configuration
# ============================================================================

function Install-OhMyPoshConfig
{
  $source = Join-Path $WindowsDotfilesDir ".OhMyPosh"
  $destination = Split-Path $PROFILE

  if (-not (Test-Path $source))
  {
    Write-Log "Oh My Posh config source not found: $source" -Level 'ERROR'
    return
  }

  Copy-DotfileSafe -Source $source -Destination $destination -Recurse
  Write-Log "Oh My Posh configuration installed" -Level 'OK'
}

# ============================================================================
# Zed Configuration
# ============================================================================

function Install-ZedConfig
{
  $source = Join-Path $SharedDotfilesDir "zed\.config\zed"
  $destination = Join-Path $env:APPDATA "Zed"

  if (-not (Test-Path $source))
  {
    Write-Log "Zed config source not found: $source" -Level 'ERROR'
    return
  }

  Copy-DotfileSafe -Source $source -Destination $destination -Recurse
  Write-Log "Zed configuration installed" -Level 'OK'
}

function Install-OllamaModels
{
  if (-not (Test-CommandExists "ollama"))
  {
    Write-Log "Ollama not found, skipping model installation" -Level 'WARNING'
    return
  }

  $models = @(
    "hf.co/unsloth/Qwen3.6-35B-A3B-GGUF:UD-IQ1_M",
    "hf.co/bartowski/zed-industries_zeta-2-GGUF:Q4_0"
  )

  foreach ($model in $models)
  {
    Write-Log "Pulling Ollama model: $model..."
    ollama pull $model
    if ($LASTEXITCODE -eq 0)
    {
      Write-Log "Ollama model installed: $model" -Level 'OK'
    } else
    {
      Write-Log "Failed to pull Ollama model: $model" -Level 'WARNING'
    }
  }
}

# ============================================================================
# Windows Terminal Configuration
# ============================================================================

function Install-WindowsTerminalConfig
{
  $source = Join-Path $WindowsDotfilesDir "WindowsTerminal\settings.json"
  $destination = Join-Path $env:LOCALAPPDATA "Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json"

  if (-not (Test-Path $source))
  {
    Write-Log "Windows Terminal config source not found: $source" -Level 'ERROR'
    return
  }

  if (Test-Path $destination)
  {
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $backup = "$destination.backup.$timestamp"
    Copy-Item -Path $destination -Destination $backup -Force
    Write-Log "Backed up existing Windows Terminal config to $backup"
  }

  Copy-DotfileSafe -Source $source -Destination $destination
  Write-Log "Windows Terminal configuration installed" -Level 'OK'
}

# ============================================================================
# Iosevka Mono Font
# ============================================================================

function Install-IosevkaMonoFont
{
  if (-not (Test-CommandExists "oh-my-posh"))
  {
    Write-Log "oh-my-posh not found, skipping font installation" -Level 'WARNING'
    return
  }

  Write-Log "Installing Iosevka Mono font..."
  oh-my-posh font install Iosevka
  Write-Log "Iosevka Mono font installed" -Level 'OK'
}

# ============================================================================
# AutoHotkey Scripts
# ============================================================================

function Install-AHKScripts
{
  $source = Join-Path $WindowsDotfilesDir "AutoHotkey"
  $destination = Join-Path $env:USERPROFILE "AutoHotkey"
  $exePath = Join-Path $destination "myconfig.exe"
  $startupDir = [Environment]::GetFolderPath("Startup")
  $shortcutPath = Join-Path $startupDir "myconfig-autohotkey.lnk"

  if (-not (Test-Path $source))
  {
    Write-Log "AHK scripts source not found: $source" -Level 'ERROR'
    return
  }

  Copy-DotfileSafe -Source $source -Destination $destination -Recurse
  Write-Log "AutoHotkey scripts installed" -Level 'OK'

  if (-not (Test-Path $exePath))
  {
    Write-Log "AutoHotkey executable not found: $exePath" -Level 'WARNING'
    return
  }

  try
  {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutPath)
    $shortcut.TargetPath = $exePath
    $shortcut.WorkingDirectory = $destination
    $shortcut.Save()
    Write-Log "AutoHotkey startup shortcut installed" -Level 'OK'
  } catch
  {
    Write-Log "Failed to create AutoHotkey startup shortcut: $($_.Exception.Message)" -Level 'ERROR'
  }
}

# ============================================================================
# PSReadLine Module
# ============================================================================

function Install-PSReadLineModule
{
  if (Get-Module -ListAvailable -Name PSReadLine)
  {
    Write-Log "PSReadLine module is already installed" -Level 'OK'
  } else
  {
    Write-Log "Installing PSReadLine module..."
    Install-Module -Name PSReadLine -AllowPrerelease -Force -Scope CurrentUser
    Write-Log "PSReadLine module installed" -Level 'OK'
  }
}

# ============================================================================
# LLVM Path 
# ===========================================================================

function Install-LLVMPath
{
  $llvmBin = "C:\Program Files\LLVM\bin"

  if (-not (Test-Path $llvmBin))
  {
    Write-Log "LLVM bin directory not found, skipping PATH setup" -Level 'WARNING'
    return
  }

  $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if ($currentPath -like "*LLVM*")
  {
    Write-Log "LLVM is already in PATH" -Level 'OK'
  } else
  {
    Write-Log "Adding LLVM to user PATH..."
    [Environment]::SetEnvironmentVariable(
      "Path",
      [Environment]::GetEnvironmentVariable("Path", "Machine") + ";C:\Program Files\LLVM\bin",
      "Machine"
    )
    Write-Log "LLVM added to PATH" -Level 'OK'
  }
}

# ============================================================================
# Registry Tweaks
# ============================================================================

function Install-RegistryTweaks
{
  # Apply checked-in registry files when present.
  $regFiles = @(
    @{ Path = Join-Path $ScriptDir "DisableRecoStartMenu.reg"; Description = "disable Start Menu recommendations" },
    @{ Path = Join-Path $ScriptDir "TaskbarSettings.reg"; Description = "taskbar and Explorer settings" }
  )

  foreach ($regFile in $regFiles)
  {
    $path = $regFile['Path']
    $description = $regFile['Description']

    if (-not (Test-Path $path))
    {
      Write-Log "Registry file not found: $path" -Level 'WARNING'
      continue
    }

    Write-Log "Applying registry tweaks ($description)..."
    reg import $path 2>$null
    Write-Log "Registry tweaks applied: $description" -Level 'OK'
  }
}

# ============================================================================
# Taskbar Auto-hide
# ============================================================================

function Enable-TaskbarAutoHide {
    Add-Type @"
using System;
using System.Runtime.InteropServices;

public class Taskbar {
    [StructLayout(LayoutKind.Sequential)]
    public struct APPBARDATA {
        public int cbSize;
        public IntPtr hWnd;
        public uint uCallbackMessage;
        public uint uEdge;
        public RECT rc;
        public int lParam;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int left;
        public int top;
        public int right;
        public int bottom;
    }

    [DllImport("shell32.dll")]
    public static extern UIntPtr SHAppBarMessage(uint dwMessage, ref APPBARDATA pData);

    [DllImport("user32.dll")]
    public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
}
"@ -ErrorAction SilentlyContinue

    $ABM_SETSTATE = 0x0000000A
    $ABS_AUTOHIDE = 0x0000001
    $ABS_ALWAYSONTOP = 0x0000002

    $taskbar = [Taskbar]::FindWindow("Shell_TrayWnd", $null)

    $data = New-Object Taskbar+APPBARDATA
    $data.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($data)
    $data.hWnd = $taskbar
    $data.lParam = $ABS_AUTOHIDE -bor $ABS_ALWAYSONTOP

    [void][Taskbar]::SHAppBarMessage($ABM_SETSTATE, [ref]$data)
}

# ============================================================================
# Main Installation Flow
# ============================================================================

function Main
{
  Write-Host ""
  Write-Host "+================================================================+" -ForegroundColor Cyan
  Write-Host "|       Windows Development Environment Setup                     |" -ForegroundColor Cyan
  Write-Host "|                (Direct copy dotfiles)                            |" -ForegroundColor Cyan
  Write-Host "+================================================================+" -ForegroundColor Cyan
  Write-Host ""

  # Check if running on Windows
  if ($env:OS -ne "Windows_NT")
  {
    Write-Log "This script is intended for Windows only." -Level 'ERROR'
    exit 1
  }

  # Install winget packages first because later steps depend on them.
  $installedPackages = Install-WingetPackages

  # Copy the core configuration files and directories.
  Install-PowerShellProfile
  Install-OhMyPoshConfig
  Install-ZedConfig
  if ($installedPackages.Supplementary)
  {
    Install-OllamaModels
  }
  Install-WindowsTerminalConfig
  Install-AHKScripts

  # Install the supporting tools and modules that the dotfiles expect.
  Install-PSReadLineModule
  Install-IosevkaMonoFont
  if ($installedPackages.DevTools)
  {
    Install-LLVMPath
  }

  # Apply the Windows shell and Start menu tweaks last.
  Install-RegistryTweaks
  Enable-TaskbarAutoHide

  Move-SharedDesktopToCurrentUser -IncludeDefaultDesktop -MoveEverything
  
  Write-Host ""
  Write-Host "+================================================================+" -ForegroundColor Green
  Write-Host "|       Installation Complete!                                     |" -ForegroundColor Green
  Write-Host "+================================================================+" -ForegroundColor Green
  Write-Host ""
  Write-Log "Dotfiles have been copied to their target locations."
  Write-Log "Please restart your terminal or run '. `$PROFILE' to apply changes."
  Write-Log "You may need to restart your computer for all changes to take effect."
  Write-Host ""
}

Main

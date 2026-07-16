<#
.SYNOPSIS
    Bootstrap Script for Setup Configuration (Windows)
.DESCRIPTION
    Downloads and installs the complete development environment for Windows.
    Usage:
        irm https://raw.githubusercontent.com/inayayousfi/myconfig/main/bootstrap.ps1 | iex
#>

$ErrorActionPreference = 'Stop'

# Repository configuration
# The bootstrap script only orchestrates download, extraction, and handoff.
$RepoOwner = "inayayousfi"
$RepoName = "myconfig"
$GithubRepo = "$RepoOwner/$RepoName"

# Installation directory
# Keep the extracted repository isolated from the working tree.
$InstallDir = Join-Path $HOME ".setup-config"
$TempDir = Join-Path $env:TEMP "setup-config-$pid"

# Colors
$Colors = @{
    Info = 'Cyan'
    Success = 'Green'
    Warning = 'Yellow'
    Error = 'Red'
}

function Write-Log {
    param(
        [string]$Message,
        [ValidateSet('INFO', 'SUCCESS', 'WARNING', 'ERROR')][string]$Level = 'INFO'
    )
    switch ($Level) {
        'SUCCESS' { Write-Host "[OK] $Message" -ForegroundColor $Colors.Success }
        'WARNING' { Write-Host "[WARNING] $Message" -ForegroundColor $Colors.Warning }
        'ERROR'   { Write-Host "[FAIL] $Message" -ForegroundColor $Colors.Error }
        default    { Write-Host "[INFO] $Message" -ForegroundColor $Colors.Info }
    }
}

function Print-Banner {
    Write-Host @"
+-----------------------------------------------------------+
|                                                           |
|        Setup Configuration Bootstrap Script               |
|                                                           |
|        Automated Development Environment Setup            |
|                                                           |
+-----------------------------------------------------------+
"@ -ForegroundColor Cyan
}

function Get-LatestReleaseTag {
    Write-Log "Fetching latest release information..."
    # Prefer the latest release, but fall back to the main branch when needed.
    $url = "https://api.github.com/repos/$GithubRepo/releases/latest"
    try {
        $release = Invoke-RestMethod -Uri $url -ErrorAction Stop
        return $release.tag_name
    } catch {
        Write-Log "No releases found or network error, will download from main branch." -Level 'WARNING'
        return "main"
    }
}

function Test-ZipArchive {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return $false
    }

    try {
        Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction SilentlyContinue
        $archive = [System.IO.Compression.ZipFile]::OpenRead($Path)
        try {
            return $archive.Entries.Count -gt 0
        } finally {
            $archive.Dispose()
        }
    } catch {
        Write-Log "Downloaded archive is not a valid ZIP file: $($_.Exception.Message)" -Level 'ERROR'
        return $false
    }
}

function Expand-ZipArchive {
    param(
        [string]$ZipPath,
        [string]$DestinationPath
    )

    if (Test-Path -LiteralPath $DestinationPath) {
        Remove-Item -LiteralPath $DestinationPath -Recurse -Force
    }

    New-Item -ItemType Directory -Path $DestinationPath -Force | Out-Null

    $expandArchiveError = $null
    try {
        Expand-Archive -LiteralPath $ZipPath -DestinationPath $DestinationPath -Force
        return
    } catch {
        $expandArchiveError = $_.Exception.Message
        Write-Log "Expand-Archive failed, retrying with .NET ZIP extraction..." -Level 'WARNING'
    }

    try {
        if (Test-Path -LiteralPath $DestinationPath) {
            Remove-Item -LiteralPath $DestinationPath -Recurse -Force
        }

        New-Item -ItemType Directory -Path $DestinationPath -Force | Out-Null
        Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction SilentlyContinue
        [System.IO.Compression.ZipFile]::ExtractToDirectory($ZipPath, $DestinationPath)
    } catch {
        if ($expandArchiveError) {
            Write-Log "Expand-Archive error: $expandArchiveError" -Level 'ERROR'
        }
        Write-Log "ZIP extraction failed: $($_.Exception.Message)" -Level 'ERROR'
        exit 1
    }
}

function Download-And-Extract {
    param([string]$Tag)

    if (!(Test-Path $TempDir)) {
        New-Item -ItemType Directory -Path $TempDir | Out-Null
    }

    $zipPath = Join-Path $TempDir "setup-config.zip"

    if ($Tag -eq "main") {
        Write-Log "Downloading repository from main branch..."
        $url = "https://github.com/$GithubRepo/archive/refs/heads/main.zip"
    } else {
        Write-Log "Downloading release $Tag..."
        $url = "https://github.com/$GithubRepo/releases/download/$Tag/setup-config.zip"
        # Fallback to the source archive if the release asset does not exist.
        try {
            $response = Invoke-WebRequest -Uri $url -Method Head -UseBasicParsing -ErrorAction Stop
        } catch {
            Write-Log "Release asset not found, downloading from source archive..." -Level 'WARNING'
            $url = "https://github.com/$GithubRepo/archive/refs/tags/$Tag.zip"
        }
    }

    try {
        Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing
        $downloadedSize = (Get-Item -LiteralPath $zipPath).Length
        Write-Log "Downloaded successfully ($downloadedSize bytes)" -Level 'SUCCESS'
    } catch {
        Write-Log "Failed to download from $url" -Level 'ERROR'
        exit 1
    }

    if (-not (Test-ZipArchive -Path $zipPath)) {
        Write-Log "Archive validation failed for $url" -Level 'ERROR'
        exit 1
    }

    Write-Log "Extracting archive..."
    $extractDir = Join-Path $TempDir "extracted"
    Expand-ZipArchive -ZipPath $zipPath -DestinationPath $extractDir
    try {
        if ([System.IO.File]::Exists($zipPath)) {
            [System.IO.File]::Delete($zipPath)
        }
    } catch {
        Write-Log "Could not remove temporary ZIP file: $($_.Exception.Message)" -Level 'WARNING'
    }

    # GitHub archives usually unpack into a single root folder.
    $extractedDirs = Get-ChildItem -Path $extractDir -Directory
    if ($extractedDirs.Count -eq 1) {
        return $extractedDirs[0].FullName
    }

    return $extractDir
}

function Unblock-PowerShellFiles {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        return
    }

    # Clear the downloaded-file marker from PowerShell content before execution.
    Get-ChildItem -Path $Path -Recurse -File |
        Where-Object { $_.Extension -in @('.ps1', '.psm1', '.psd1') } |
        ForEach-Object {
            Unblock-File -LiteralPath $_.FullName
        }
}

function Run-Installation {
    param(
        [string]$SourceDir
    )

    Write-Log "Preparing installation directory..."
    Write-Log "Source directory: $SourceDir"

    # Preserve any previous install before we replace it.
    if (Test-Path $InstallDir) {
        $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
        $backupDir = "${InstallDir}.backup.$timestamp"
        Write-Log "Backing up existing installation to $backupDir" -Level 'WARNING'
        Rename-Item -Path $InstallDir -NewName (Split-Path $backupDir -Leaf)
    }

    # Stage the repository into the local install directory.
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path "$SourceDir\*" -Destination $InstallDir -Recurse -Force
    Unblock-PowerShellFiles -Path $InstallDir

    # Verify the Windows installer was copied before we execute it.
    if (!(Test-Path (Join-Path $InstallDir "windows\install.ps1"))) {
        Write-Log "File copy failed or source directory was incomplete. Check $InstallDir" -Level 'ERROR'
        exit 1
    }

    Write-Log "Files copied to $InstallDir" -Level 'SUCCESS'

    # Best-effort trust setup: import the code-signing cert (if present) and
    # relax ExecutionPolicy to RemoteSigned. Never fatal - trust/signing is
    # an enhancement, not a hard requirement for install to proceed.
    $trustScript = Join-Path $InstallDir "windows\trust-cert.ps1"
    if (Test-Path $trustScript) {
        Write-Log "Running code-signing trust setup..."
        & $trustScript
        if ($LASTEXITCODE -ne 0) {
            Write-Log "Trust setup exited with code $LASTEXITCODE, continuing with install anyway." -Level 'WARNING'
        }
    } else {
        Write-Log "trust-cert.ps1 not found, skipping trust setup." -Level 'WARNING'
    }

    # Hand off to the Windows installer inside the staged repo.
    Write-Log "Starting Windows installation..."
    Write-Host ""

    $installScript = Join-Path $InstallDir "windows\install.ps1"
    Set-Location (Join-Path $InstallDir "windows")
    & $installScript
}

function Cleanup {
    Write-Log "Cleaning up temporary files..."
    try {
        if (Test-Path -LiteralPath $TempDir -ErrorAction SilentlyContinue) {
            Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction Stop
        }
        Write-Log "Cleanup complete" -Level 'SUCCESS'
    } catch {
        Write-Log "Temporary cleanup failed: $($_.Exception.Message)" -Level 'WARNING'
    }
}

function Main {
    Print-Banner

    $tag = Get-LatestReleaseTag
    $extractedDir = Download-And-Extract -Tag $tag

    Run-Installation -SourceDir $extractedDir

    Write-Host ""
    Write-Log "Installation complete!" -Level 'SUCCESS'
    Write-Log "Configuration installed to: $InstallDir"
    Write-Host ""
    Write-Log "Windows setup complete!"
    Write-Log "GlazeWM and Zebar will start on your next login."
    Write-Log "WSL Ubuntu setup will run automatically if installed."
    Write-Log "Please restart your computer for all changes to take effect."
    Write-Host ""

    # Cleanup is best-effort and should never invalidate a successful install.
    Cleanup
}

# Run main function
Main

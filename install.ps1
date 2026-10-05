<#
.SYNOPSIS
    Installs the myconfig binary for Windows Workstation and opens it.
.DESCRIPTION
    Usage:
        irm https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.ps1 | iex
        & ([scriptblock]::Create((irm https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.ps1))) install

    Every argument goes to the binary; with none, the binary opens its screen.
#>

$ErrorActionPreference = 'Stop'

$ReleaseUrl = if ($env:MYCONFIG_RELEASE_URL) { $env:MYCONFIG_RELEASE_URL } else { 'https://github.com/inayayousfi/myconfig/releases/latest/download' }
$Asset = 'myconfig-windows-x86_64-pc-windows-msvc.exe'
$InstallDir = Join-Path $env:LOCALAPPDATA 'myconfig'
$Binary = Join-Path $InstallDir 'myconfig.exe'

function Write-Log {
    param([string]$Message)
    Write-Host "[myconfig] $Message" -ForegroundColor Cyan
}

if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'Only x86_64 binaries are published.'
}

$download = Join-Path ([IO.Path]::GetTempPath()) "myconfig-$PID"
New-Item -ItemType Directory -Path $download -Force | Out-Null
try {
    $file = Join-Path $download $Asset
    Write-Log "Downloading $Asset"
    Invoke-WebRequest -Uri "$ReleaseUrl/$Asset" -OutFile $file -UseBasicParsing
    Invoke-WebRequest -Uri "$ReleaseUrl/$Asset.sha256" -OutFile "$file.sha256" -UseBasicParsing
    $expected = ((Get-Content -LiteralPath "$file.sha256" -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($expected -ne $actual) {
        throw "$Asset does not match its published SHA-256."
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Move-Item -LiteralPath $file -Destination $Binary -Force
    Unblock-File -LiteralPath $Binary
} finally {
    Remove-Item -LiteralPath $download -Recurse -Force -ErrorAction SilentlyContinue
}

# Keep the binary on the user PATH, so `myconfig verify` and `myconfig remove` work later.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = @($userPath -split ';' | Where-Object { $_ })
if (-not ($entries | Where-Object { $_.TrimEnd('\') -ieq $InstallDir })) {
    [Environment]::SetEnvironmentVariable('Path', (($entries + $InstallDir) -join ';'), 'User')
    Write-Log "Added $InstallDir to your user PATH"
}
if (-not (($env:Path -split ';') | Where-Object { $_.TrimEnd('\') -ieq $InstallDir })) {
    $env:Path = "$env:Path;$InstallDir"
}

# The installed PowerShell profile is a local script, which RemoteSigned allows.
$policy = Get-ExecutionPolicy -Scope CurrentUser
if (@('RemoteSigned', 'Unrestricted', 'Bypass') -notcontains $policy) {
    try {
        Set-ExecutionPolicy RemoteSigned -Scope CurrentUser -Force
        Write-Log 'Set the PowerShell execution policy to RemoteSigned for the current user'
    } catch {
        Write-Log "Could not set the execution policy: $($_.Exception.Message)"
    }
}

Write-Log "Installed $Binary"
& $Binary @args
# No `exit` here: under `irm | iex` it would close your PowerShell window.
if ($LASTEXITCODE -ne 0) {
    Write-Log "myconfig exited with code $LASTEXITCODE"
}

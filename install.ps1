<#
.SYNOPSIS
    Installs the InayOS binary for Windows Workstation and opens it.
.DESCRIPTION
    Usage:
        irm https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.ps1 | iex
        & ([scriptblock]::Create((irm https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.ps1))) install

    Every argument goes to the binary; with none, the binary opens its screen.
#>

$ErrorActionPreference = 'Stop'

$ReleaseUrl = if ($env:INAYOS_RELEASE_URL) { $env:INAYOS_RELEASE_URL } else { 'https://github.com/inayayousfi/myconfig/releases/latest/download' }
$Asset = 'inayos-windows-x86_64-pc-windows-msvc.exe'
$InstallDir = Join-Path $env:LOCALAPPDATA 'inayos'
$Binary = Join-Path $InstallDir 'inayos.exe'

function Write-Log {
    param([string]$Message)
    Write-Host "[inayos] $Message" -ForegroundColor Cyan
}

if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'Only x86_64 binaries are published.'
}

$download = Join-Path ([IO.Path]::GetTempPath()) "inayos-$PID"
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

# Keep the binary on the user PATH, so `inayos verify` and `inayos remove` work later.
# The stored value is read and written raw, with its type: [Environment] would expand its
# %NAME% references and store it as a plain string, which changes every other entry.
$environment = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
try {
    $userPath = [string]$environment.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
    $kind = if ($environment.GetValueNames() -contains 'Path') { $environment.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
    $present = @($userPath -split ';') | Where-Object { [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') -ieq $InstallDir }
    if (-not $present) {
        $separator = if ($userPath -and -not $userPath.EndsWith(';')) { ';' } else { '' }
        $environment.SetValue('Path', "$userPath$separator$InstallDir", $kind)
        # A direct registry write does not tell running programs, such as Explorer, that the
        # environment changed.
        Add-Type -Namespace InayOS -Name Environment -MemberDefinition '[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);'
        $result = [UIntPtr]::Zero
        [void][InayOS.Environment]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
        Write-Log "Added $InstallDir to your user PATH"
    }
} finally {
    $environment.Dispose()
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
    Write-Log "inayos exited with code $LASTEXITCODE"
}

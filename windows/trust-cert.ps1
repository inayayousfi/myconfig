# Trust the myconfig code-signing certificate (Windows)
# This script is idempotent - running it multiple times is safe
# One-time trust step: imports the myconfig Authenticode leaf certificate
# into the CurrentUser TrustedPublisher and Root stores, then relaxes
# ExecutionPolicy from Bypass to RemoteSigned so signed scripts run without
# the -Bypass flag. Safe to run before the cert exists in the repo (logs a
# warning and exits 0) so it never breaks bootstrap.ps1 on a fresh clone.

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
$CertPath = Join-Path $RepoRoot "windows\codesign\myconfig-codesign.cer"

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

function Test-PolicyAtLeastAsPermissiveAsRemoteSigned
{
  # Ordered from most restrictive to least restrictive. Anything at or
  # beyond RemoteSigned in this ordering should not be downgraded.
  param([string]$Policy)

  $permissiveEnough = @('RemoteSigned', 'Unrestricted', 'Bypass')
  return $permissiveEnough -contains $Policy
}

function Test-CertAlreadyTrusted
{
  param([System.Security.Cryptography.X509Certificates.X509Certificate2]$Cert)

  $thumbprint = $Cert.Thumbprint

  $inTrustedPublisher = Get-ChildItem -Path 'Cert:\CurrentUser\TrustedPublisher' -ErrorAction SilentlyContinue |
    Where-Object { $_.Thumbprint -eq $thumbprint }
  $inRoot = Get-ChildItem -Path 'Cert:\CurrentUser\Root' -ErrorAction SilentlyContinue |
    Where-Object { $_.Thumbprint -eq $thumbprint }

  return ([bool]$inTrustedPublisher -and [bool]$inRoot)
}

# ============================================================================
# Main
# ============================================================================

function Main
{
  if (-not (Test-Path $CertPath))
  {
    Write-Log "Code-signing certificate not found at $CertPath - skipping trust setup (see windows/CODESIGNING.md)." -Level 'WARNING'
    exit 0
  }

  $cert = $null
  try
  {
    $cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($CertPath)
  }
  catch
  {
    Write-Log "Failed to load certificate from ${CertPath}: $($_.Exception.Message)" -Level 'ERROR'
    exit 1
  }

  $certAlreadyTrusted = Test-CertAlreadyTrusted -Cert $cert
  $currentPolicy = Get-ExecutionPolicy -Scope CurrentUser
  $policyAlreadyPermissive = Test-PolicyAtLeastAsPermissiveAsRemoteSigned -Policy $currentPolicy

  if ($certAlreadyTrusted -and $policyAlreadyPermissive)
  {
    Write-Log "Certificate already trusted and ExecutionPolicy already $currentPolicy - skipping." -Level 'OK'
    exit 0
  }

  try
  {
    if (-not $certAlreadyTrusted)
    {
      Write-Log "Importing certificate into CurrentUser\TrustedPublisher..."
      $trustedPublisherStore = New-Object System.Security.Cryptography.X509Certificates.X509Store('TrustedPublisher', 'CurrentUser')
      $trustedPublisherStore.Open('ReadWrite')
      $trustedPublisherStore.Add($cert)
      $trustedPublisherStore.Close()

      # A self-signed leaf cert needs to be its own trust anchor, so it must
      # also be imported into Root, not just TrustedPublisher.
      Write-Log "Importing certificate into CurrentUser\Root..."
      $rootStore = New-Object System.Security.Cryptography.X509Certificates.X509Store('Root', 'CurrentUser')
      $rootStore.Open('ReadWrite')
      $rootStore.Add($cert)
      $rootStore.Close()

      Write-Log "Certificate imported into TrustedPublisher and Root." -Level 'OK'
    }

    if (-not $policyAlreadyPermissive)
    {
      Write-Log "Setting ExecutionPolicy to RemoteSigned for CurrentUser..."
      Set-ExecutionPolicy RemoteSigned -Scope CurrentUser -Force
      Write-Log "ExecutionPolicy set to RemoteSigned." -Level 'OK'
    }
  }
  catch
  {
    Write-Log "Failed to trust code-signing certificate: $($_.Exception.Message)" -Level 'ERROR'
    exit 1
  }

  exit 0
}

Main

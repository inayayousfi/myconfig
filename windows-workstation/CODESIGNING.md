# Code Signing Setup (Maintainer, One-Time)

This repo's Windows PowerShell scripts are Authenticode-signed on release. Setting up the signing certificate requires Windows PowerShell's certificate store APIs and a manual trust decision about pushing secrets, so it can't be automated — the repo maintainer must run these steps once, locally, on a Windows machine.

## 1. Generate the certificate

```powershell
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=inayayousfi myconfig" `
  -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(10)
Export-Certificate -Cert $cert -FilePath windows-workstation\codesign\myconfig-codesign.cer
Export-PfxCertificate -Cert $cert -FilePath myconfig-codesign.pfx -Password (Read-Host -AsSecureString)
```

The certificate is self-signed and valid for 10 years. It is only trusted on machines where `windows-workstation/trust-cert.ps1` has explicitly imported it — it is **not** trusted by default on arbitrary machines.

## 2. Commit the public certificate

Commit the resulting `windows-workstation/codesign/myconfig-codesign.cer` (public key only) to the repo. CI and `windows-workstation/trust-cert.ps1` use this file to verify and trust signed scripts.

## 3. Store the private key as GitHub secrets

Base64-encode the `.pfx`:

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes('myconfig-codesign.pfx'))
```

Set it, and its password, as two separate repo secrets:

```powershell
gh secret set CODESIGN_PFX_B64
gh secret set CODESIGN_PFX_PASSWORD
```

(Or via the GitHub web UI: repo Settings -> Secrets and variables -> Actions.) CI uses these to sign release scripts.

## Never commit the private key

Never commit the `.pfx` file or its password to the repo — only the `.cer` (public key) belongs in version control. Delete the local `.pfx` once both secrets are set, or keep it somewhere outside the repo.

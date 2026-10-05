# Code Signing Setup (Maintainer, One-Time)

This repo's Windows PowerShell scripts are Authenticode-signed on release. Setting up the signing certificate requires Windows PowerShell's certificate store APIs and a manual trust decision about pushing secrets, so it can't be automated — the repo maintainer must run these steps once, locally, on a Windows machine.

## 1. Generate the certificate

```powershell
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=inayayousfi myconfig" `
  -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(10)
Export-Certificate -Cert $cert -FilePath myconfig-codesign.cer
Export-PfxCertificate -Cert $cert -FilePath myconfig-codesign.pfx -Password (Read-Host -AsSecureString)
```

The certificate is self-signed and valid for 10 years. Windows does not trust it by default; a machine trusts the signed scripts only after someone imports the certificate there.

## 2. Keep the public certificate

Keep `myconfig-codesign.cer` (public key only) if you want to import it on a machine that should trust the signed scripts. The release workflow signs the PowerShell files embedded in the Windows binary with the secrets below.

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

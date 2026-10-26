param(
    # install.ps1 reports failures itself and must not wait for a key.
    [switch]$FromInstaller
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Package = Join-Path (Split-Path -Parent $ScriptDir) "dotfiles\vscodium\.config\VSCodium"

$Editors = @(
    @{
        Name    = "VSCodium"
        Command = "codium"
        User    = Join-Path $env:APPDATA "VSCodium\User"
        Remote  = "jeanp413.open-remote-ssh"
    },
    @{
        Name    = "VS Code"
        Command = "code"
        User    = Join-Path $env:APPDATA "Code\User"
        Remote  = "ms-vscode-remote.remote-ssh"
    }
)

try {
    $ExtensionList = Get-Content (Join-Path $Package "extensions.txt") | Where-Object { $_.Trim() }

    # The editors forget extension folders they did not install themselves, so
    # the theme goes in as a package: a zip with the extension under extension\.
    $ThemeDir = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
    $ThemePackage = Join-Path $ThemeDir "blacknpink-theme.vsix"
    Copy-Item -Path (Join-Path $Package "blacknpink-theme") -Destination (Join-Path $ThemeDir "extension") -Recurse
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory((Join-Path $ThemeDir "extension"), $ThemePackage, "Optimal", $true)

    foreach ($editor in $Editors) {
        New-Item -ItemType Directory -Path $editor.User -Force | Out-Null
        foreach ($file in "settings.json", "keybindings.json") {
            Copy-Item -Path (Join-Path $Package "User\$file") -Destination $editor.User -Force
        }
        Write-Host "[OK] $($editor.Name) settings copied" -ForegroundColor Green

        if (-not (Get-Command $editor.Command -ErrorAction SilentlyContinue)) {
            Write-Host "[INFO] $($editor.Command) not found; skipping $($editor.Name) theme and extensions" -ForegroundColor Cyan
            continue
        }
        # Forced, so an edited theme replaces the installed one of the same version.
        & $editor.Command --force --install-extension $ThemePackage
        if ($LASTEXITCODE -ne 0) {
            throw "$($editor.Name) could not install the theme"
        }
        $arguments = foreach ($extension in @($editor.Remote) + $ExtensionList) { "--install-extension"; $extension }
        & $editor.Command @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$($editor.Name) could not install its extensions"
        }
        Write-Host "[OK] $($editor.Name) theme and extensions installed" -ForegroundColor Green
    }

    Remove-Item -Path $ThemeDir -Recurse -Force

    # --wait keeps Git and other programs waiting until the file is closed.
    $editor = "notepad"
    foreach ($candidate in $Editors) {
        if (Get-Command $candidate.Command -ErrorAction SilentlyContinue) {
            $editor = "$($candidate.Command) --wait"
            break
        }
    }
    foreach ($name in @('EDITOR', 'VISUAL')) {
        [Environment]::SetEnvironmentVariable($name, $editor, "User")
        Set-Item -Path "env:$name" -Value $editor
    }
    git config --global core.editor $editor
    if ($LASTEXITCODE -ne 0) {
        throw "Could not set Git's editor"
    }
    Write-Host "[OK] Files from Git and other programs open with $editor" -ForegroundColor Green
} catch {
    if ($FromInstaller) {
        throw
    }
    Write-Host "[ERROR] $($_.Exception.Message)" -ForegroundColor Red
}

if (-not $FromInstaller) {
    Read-Host "Press Enter to close" | Out-Null
}

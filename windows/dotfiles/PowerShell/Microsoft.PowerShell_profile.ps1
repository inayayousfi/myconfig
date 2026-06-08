# =========================
# Profile mode detection
# =========================

$ProfileMode = if ($env:PW_PROFILE_MODE)
{ $env:PW_PROFILE_MODE
} else
{ "auto"
}

$IsInteractiveConsole = (
    $Host.Name -eq "ConsoleHost" -and
    -not [Console]::IsInputRedirected -and
    -not [Console]::IsOutputRedirected
)

$UseInteractiveProfile = switch ($ProfileMode.ToLowerInvariant())
{
    "full"
    { $true
    }
    "quiet"
    { $false
    }
    "auto"
    { $IsInteractiveConsole
    }
    default
    { $IsInteractiveConsole
    }
}

$IsQuiet = -not $UseInteractiveProfile

# =========================
# Quiet-safe helpers
# =========================

function Write-Info($msg)
{
    if (-not $IsQuiet)
    {
        Write-Host $msg -ForegroundColor Cyan
    }
}

function Write-Success($msg)
{
    if (-not $IsQuiet)
    {
        Write-Host $msg -ForegroundColor Green
    }
}

function Write-Warn($msg)
{
    if (-not $IsQuiet)
    {
        Write-Host $msg -ForegroundColor Yellow
    }
}

function Write-Err($msg)
{
    Write-Host $msg -ForegroundColor Red
}

# =========================
# UI / Interactive only
# =========================

if ($UseInteractiveProfile)
{
    # --- Oh My Posh ---
    $ohMyPoshConfig = Join-Path (Split-Path $PROFILE) "black-pink.omp.json"
    if (Test-Path $ohMyPoshConfig)
    {
        oh-my-posh init pwsh --config $ohMyPoshConfig | Invoke-Expression
    }

    # --- Terminal Icons ---
    if (Get-Module -ListAvailable -Name Terminal-Icons)
    {
        Import-Module Terminal-Icons
    }

    # --- PSReadLine ---
    if (Get-Module -ListAvailable -Name PSReadLine)
    {
        Import-Module PSReadLine

        Set-PSReadLineOption -EditMode Vi
        Set-PSReadLineKeyHandler -Key Tab -Function MenuComplete

        try
        {
            Set-PSReadLineOption -PredictionSource History
            Set-PSReadLineOption -PredictionViewStyle InlineView
        } catch
        {
        }

        try
        {
            Set-PSReadLineOption -ViModeIndicator Script
            Set-PSReadLineOption -ViModeChangeHandler {
                param($mode)
                switch ($mode)
                {
                    "Insert"
                    { Write-Host -NoNewline "$([char]0x1b)[5 q"
                    }
                    "Command"
                    { Write-Host -NoNewline "$([char]0x1b)[1 q"
                    }
                    default
                    { Write-Host -NoNewline "$([char]0x1b)[5 q"
                    }
                }
            }
        } catch
        {
        }
    }
}

# =========================
# Aliases (always loaded)
# =========================

function lg {
    wsl -u ziede zsh -ic "lazygit $args"
}

function oc {
    wsl -u ziede zsh -ic "opencode $args"
}

Set-Alias which gcm

# =========================
# Core functions
# =========================

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

function repair-user-path
{

    # Read the current user PATH from the registry-backed environment variable
    $currentUserPath = [Environment]::GetEnvironmentVariable("Path", "User")

    # Split into entries, remove surrounding whitespace, and ignore empty items
    $pathEntries = @()
    if ($currentUserPath)
    {
        $pathEntries = $currentUserPath -split ';' |
            ForEach-Object { $_.Trim() } |
            Where-Object { $_ -ne "" }
    }

    # Core paths that are commonly needed on a developer workstation
    $requiredPaths = @(
        "$env:LOCALAPPDATA\Microsoft\WindowsApps",                  # App execution aliases, including winget
        "$env:LOCALAPPDATA\Microsoft\WinGet\Links"                  # WinGet portable package command links
    )

    # Append only missing entries
    foreach ($pathToAdd in $requiredPaths)
    {
        if (-not [string]::IsNullOrWhiteSpace($pathToAdd))
        {
            if ($pathEntries -notcontains $pathToAdd)
            {
                Write-Host "Adding: $pathToAdd"
                $pathEntries += $pathToAdd
            } else
            {
                Write-Host "Already present: $pathToAdd"
            }
        }
    }

    # Remove duplicates while preserving first occurrence order
    $seen = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $cleanPathEntries = foreach ($entry in $pathEntries)
    {
        if ($seen.Add($entry))
        {
            $entry
        }
    }

    # Write the cleaned PATH back to the user environment
    $newUserPath = $cleanPathEntries -join ';'
    [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")

    Write-Host ""
    Write-Host "User PATH repaired successfully."
    Write-Host "Open a new terminal session to reload the updated PATH."
}

function su
{
    $currentDir = (Get-Location).Path

    $wtArgs = @(
        "new-tab",
        "-p", "PowerShell",
        "-d", $currentDir,
        "pwsh",
        "-NoExit"
    )

    Start-Process -FilePath "wt.exe" -Verb RunAs -ArgumentList $wtArgs
}

function msvcenv
{
    $vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"

    if (Test-Path $vsWhere)
    {
        $installPath = & $vsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null

        if ($installPath)
        {
            $launchScript = Join-Path $installPath "Common7\Tools\Launch-VsDevShell.ps1"
            if (Test-Path $launchScript)
            {
                & $launchScript
                Write-Success "✅ MSVC environment loaded"
                return
            }
        }
    }

    Write-Err "❌ MSVC environment not found."
}

function repair-winget
{
    [CmdletBinding()]
    param()

    Set-StrictMode -Version Latest
    $ErrorActionPreference = "Stop"

    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing

    function Test-WingetAvailable
    {
        try
        {
            $null = Get-Command winget -ErrorAction Stop
            return $true
        } catch
        {
            return $false
        }
    }

    function Repair-WingetClient
    {
        try
        {
            Add-AppxPackage -RegisterByFamilyName -MainPackage Microsoft.DesktopAppInstaller_8wekyb3d8bbwe -ErrorAction Stop
            Start-Sleep -Seconds 2
            return $true
        } catch
        {
            return $false
        }
    }

    function Invoke-Winget
    {
        param(
            [Parameter(Mandatory)]
            [string[]]$Arguments
        )

        $psi = New-Object System.Diagnostics.ProcessStartInfo
        $psi.FileName = "winget.exe"
        $psi.Arguments = ($Arguments -join ' ')
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.StandardOutputEncoding = [System.Text.Encoding]::UTF8
        $psi.StandardErrorEncoding = [System.Text.Encoding]::UTF8
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $true
        $psi.EnvironmentVariables["LANG"] = "en_US.UTF-8"
        $psi.EnvironmentVariables["LC_ALL"] = "en_US.UTF-8"
        $psi.EnvironmentVariables["UICulture"] = "en-US"

        $proc = New-Object System.Diagnostics.Process
        $proc.StartInfo = $psi

        [void]$proc.Start()
        $stdout = $proc.StandardOutput.ReadToEnd()
        $stderr = $proc.StandardError.ReadToEnd()
        $proc.WaitForExit()

        [PSCustomObject]@{
            ExitCode = $proc.ExitCode
            StdOut   = $stdout
            StdErr   = $stderr
            Command  = "winget " + ($Arguments -join ' ')
        }
    }

    function Get-WingetExportPackages
    {
        $tempFile = Join-Path $env:TEMP ("winget_export_" + [guid]::NewGuid().ToString() + ".json")

        try
        {
            $result = Invoke-Winget -Arguments @(
                "export",
                "--output", "`"$tempFile`"",
                "--include-versions",
                "--accept-source-agreements",
                "--disable-interactivity"
            )

            if ($result.ExitCode -ne 0)
            {
                throw "winget export failed.`nCommand: $($result.Command)`nError: $($result.StdErr)`nOutput: $($result.StdOut)"
            }

            if (-not (Test-Path $tempFile))
            {
                throw "The JSON export file was not created."
            }

            $json = Get-Content -Path $tempFile -Raw -Encoding UTF8 | ConvertFrom-Json
            $packages = New-Object System.Collections.Generic.List[object]

            foreach ($source in $json.Sources)
            {
                $sourceName = if ($source.SourceDetails -and $source.SourceDetails.Name)
                {
                    $source.SourceDetails.Name
                } elseif ($source.SourceIdentifier)
                {
                    $source.SourceIdentifier
                } else
                {
                    "unknown"
                }

                foreach ($pkg in $source.Packages)
                {
                    $id = $pkg.PackageIdentifier
                    $version = $pkg.Version

                    if ([string]::IsNullOrWhiteSpace($id))
                    {
                        continue
                    }

                    $packages.Add([PSCustomObject]@{
                            Selected = $false
                            Id       = $id
                            Version  = if ($version)
                            { $version
                            } else
                            { ""
                            }
                            Source   = $sourceName
                        })
                }
            }

            $packages | Sort-Object Id -Unique
        } finally
        {
            if (Test-Path $tempFile)
            {
                Remove-Item -Path $tempFile -Force -ErrorAction SilentlyContinue
            }
        }
    }

    function Write-Log
    {
        param(
            [Parameter(Mandatory)][System.Windows.Forms.TextBox]$TextBox,
            [Parameter(Mandatory)][string]$Message
        )

        $timestamp = Get-Date -Format "HH:mm:ss"
        $TextBox.AppendText("[$timestamp] $Message`r`n")
        $TextBox.SelectionStart = $TextBox.TextLength
        $TextBox.ScrollToCaret()
        [System.Windows.Forms.Application]::DoEvents()
    }

    function Reinstall-WingetPackage
    {
        param(
            [Parameter(Mandatory)][string]$Id,
            [Parameter()][string]$Version,
            [Parameter(Mandatory)][System.Windows.Forms.TextBox]$LogBox
        )

        Write-Log -TextBox $LogBox -Message "----------------------------------------"
        Write-Log -TextBox $LogBox -Message "Processing: $Id"

        $uninstallArgs = @(
            "uninstall",
            "--id", "`"$Id`"",
            "--exact",
            "--source", "winget",
            "--accept-source-agreements",
            "--disable-interactivity"
        )

        $uninstallResult = Invoke-Winget -Arguments $uninstallArgs
        Write-Log -TextBox $LogBox -Message $uninstallResult.Command

        if ($uninstallResult.StdOut.Trim())
        {
            Write-Log -TextBox $LogBox -Message $uninstallResult.StdOut.Trim()
        }
        if ($uninstallResult.StdErr.Trim())
        {
            Write-Log -TextBox $LogBox -Message "STDERR: $($uninstallResult.StdErr.Trim())"
        }

        if ($uninstallResult.ExitCode -ne 0)
        {
            Write-Log -TextBox $LogBox -Message "Uninstall failed for $Id (code $($uninstallResult.ExitCode))."
            return
        }

        $installArgs = @(
            "install",
            "--id", "`"$Id`"",
            "--exact",
            "--source", "winget",
            "--accept-source-agreements",
            "--accept-package-agreements",
            "--disable-interactivity"
        )

        if (-not [string]::IsNullOrWhiteSpace($Version))
        {
            $installArgs += @("--version", "`"$Version`"")
        }

        $installResult = Invoke-Winget -Arguments $installArgs
        Write-Log -TextBox $LogBox -Message $installResult.Command

        if ($installResult.StdOut.Trim())
        {
            Write-Log -TextBox $LogBox -Message $installResult.StdOut.Trim()
        }
        if ($installResult.StdErr.Trim())
        {
            Write-Log -TextBox $LogBox -Message "STDERR: $($installResult.StdErr.Trim())"
        }

        if ($installResult.ExitCode -eq 0)
        {
            Write-Log -TextBox $LogBox -Message "OK: $Id reinstalled."
        } else
        {
            Write-Log -TextBox $LogBox -Message "Reinstall failed for $Id (code $($installResult.ExitCode))."
        }
    }

    if (-not (Test-WingetAvailable))
    {
        $repairOk = Repair-WingetClient

        if (-not $repairOk -or -not (Test-WingetAvailable))
        {
            [System.Windows.Forms.MessageBox]::Show(
                "winget.exe was not found, and the App Installer repair attempt failed.`n`nRepair or reinstall App Installer from Microsoft Store, then run this function again.",
                "WinGet Not Found",
                [System.Windows.Forms.MessageBoxButtons]::OK,
                [System.Windows.Forms.MessageBoxIcon]::Error
            ) | Out-Null
            return
        }
    }

    $form = New-Object System.Windows.Forms.Form
    $darkBack = [System.Drawing.Color]::FromArgb(24, 24, 27)
    $darkPanel = [System.Drawing.Color]::FromArgb(39, 39, 42)
    $darkInput = [System.Drawing.Color]::FromArgb(31, 41, 55)
    $darkBorder = [System.Drawing.Color]::FromArgb(63, 63, 70)
    $lightText = [System.Drawing.Color]::FromArgb(244, 244, 245)
    $mutedText = [System.Drawing.Color]::FromArgb(212, 212, 216)
    $accent = [System.Drawing.Color]::FromArgb(59, 130, 246)

    $form.Text = "Winget - Uninstall / Reinstall"
    $form.Size = New-Object System.Drawing.Size(1180, 740)
    $form.StartPosition = "CenterScreen"
    $form.TopMost = $false
    $form.BackColor = $darkBack
    $form.ForeColor = $lightText

    function Set-DarkButton
    {
        param([Parameter(Mandatory)][System.Windows.Forms.Button]$Button)

        $Button.BackColor = $darkPanel
        $Button.ForeColor = $lightText
        $Button.FlatStyle = "Flat"
        $Button.FlatAppearance.BorderColor = $darkBorder
        $Button.FlatAppearance.MouseOverBackColor = $accent
        $Button.FlatAppearance.MouseDownBackColor = [System.Drawing.Color]::FromArgb(37, 99, 235)
        $Button.UseVisualStyleBackColor = $false
    }

    $lblFilter = New-Object System.Windows.Forms.Label
    $lblFilter.Text = "Filter:"
    $lblFilter.Location = New-Object System.Drawing.Point(12, 15)
    $lblFilter.AutoSize = $true
    $lblFilter.ForeColor = $mutedText
    $form.Controls.Add($lblFilter)

    $txtFilter = New-Object System.Windows.Forms.TextBox
    $txtFilter.Location = New-Object System.Drawing.Point(65, 12)
    $txtFilter.Size = New-Object System.Drawing.Size(365, 24)
    $txtFilter.BackColor = $darkInput
    $txtFilter.ForeColor = $lightText
    $txtFilter.BorderStyle = "FixedSingle"
    $form.Controls.Add($txtFilter)

    $btnRefresh = New-Object System.Windows.Forms.Button
    $btnRefresh.Text = "Refresh"
    $btnRefresh.Location = New-Object System.Drawing.Point(445, 10)
    $btnRefresh.Size = New-Object System.Drawing.Size(95, 30)
    Set-DarkButton -Button $btnRefresh
    $form.Controls.Add($btnRefresh)

    $btnSelectAll = New-Object System.Windows.Forms.Button
    $btnSelectAll.Text = "Select All"
    $btnSelectAll.Location = New-Object System.Drawing.Point(550, 10)
    $btnSelectAll.Size = New-Object System.Drawing.Size(105, 30)
    Set-DarkButton -Button $btnSelectAll
    $form.Controls.Add($btnSelectAll)

    $btnUnselectAll = New-Object System.Windows.Forms.Button
    $btnUnselectAll.Text = "Clear All"
    $btnUnselectAll.Location = New-Object System.Drawing.Point(665, 10)
    $btnUnselectAll.Size = New-Object System.Drawing.Size(105, 30)
    Set-DarkButton -Button $btnUnselectAll
    $form.Controls.Add($btnUnselectAll)

    $btnRun = New-Object System.Windows.Forms.Button
    $btnRun.Text = "Reinstall Selected"
    $btnRun.Location = New-Object System.Drawing.Point(785, 10)
    $btnRun.Size = New-Object System.Drawing.Size(170, 30)
    Set-DarkButton -Button $btnRun
    $form.Controls.Add($btnRun)

    $grid = New-Object System.Windows.Forms.DataGridView
    $grid.Location = New-Object System.Drawing.Point(12, 50)
    $grid.Size = New-Object System.Drawing.Size(1140, 390)
    $grid.Anchor = "Top,Left,Right"
    $grid.AutoGenerateColumns = $false
    $grid.AllowUserToAddRows = $false
    $grid.AllowUserToDeleteRows = $false
    $grid.SelectionMode = "FullRowSelect"
    $grid.MultiSelect = $true
    $grid.RowHeadersVisible = $false
    $grid.AutoSizeColumnsMode = "Fill"
    $grid.BackgroundColor = $darkBack
    $grid.BorderStyle = "FixedSingle"
    $grid.GridColor = $darkBorder
    $grid.EnableHeadersVisualStyles = $false
    $grid.DefaultCellStyle.BackColor = $darkPanel
    $grid.DefaultCellStyle.ForeColor = $lightText
    $grid.DefaultCellStyle.SelectionBackColor = $accent
    $grid.DefaultCellStyle.SelectionForeColor = [System.Drawing.Color]::White
    $grid.AlternatingRowsDefaultCellStyle.BackColor = [System.Drawing.Color]::FromArgb(32, 32, 36)
    $grid.ColumnHeadersDefaultCellStyle.BackColor = $darkInput
    $grid.ColumnHeadersDefaultCellStyle.ForeColor = $lightText
    $grid.ColumnHeadersDefaultCellStyle.SelectionBackColor = $darkInput
    $grid.ColumnHeadersDefaultCellStyle.SelectionForeColor = $lightText

    $colCheck = New-Object System.Windows.Forms.DataGridViewCheckBoxColumn
    $colCheck.DataPropertyName = "Selected"
    $colCheck.HeaderText = ""
    $colCheck.Width = 40
    [void]$grid.Columns.Add($colCheck)

    $colId = New-Object System.Windows.Forms.DataGridViewTextBoxColumn
    $colId.DataPropertyName = "Id"
    $colId.HeaderText = "Package ID"
    $colId.FillWeight = 55
    [void]$grid.Columns.Add($colId)

    $colVersion = New-Object System.Windows.Forms.DataGridViewTextBoxColumn
    $colVersion.DataPropertyName = "Version"
    $colVersion.HeaderText = "Version"
    $colVersion.FillWeight = 20
    [void]$grid.Columns.Add($colVersion)

    $colSource = New-Object System.Windows.Forms.DataGridViewTextBoxColumn
    $colSource.DataPropertyName = "Source"
    $colSource.HeaderText = "Source"
    $colSource.FillWeight = 25
    [void]$grid.Columns.Add($colSource)

    $form.Controls.Add($grid)

    $lblLog = New-Object System.Windows.Forms.Label
    $lblLog.Text = "Log:"
    $lblLog.Location = New-Object System.Drawing.Point(12, 450)
    $lblLog.AutoSize = $true
    $lblLog.ForeColor = $mutedText
    $form.Controls.Add($lblLog)

    $txtLog = New-Object System.Windows.Forms.TextBox
    $txtLog.Location = New-Object System.Drawing.Point(12, 470)
    $txtLog.Size = New-Object System.Drawing.Size(1140, 210)
    $txtLog.Multiline = $true
    $txtLog.ScrollBars = "Vertical"
    $txtLog.ReadOnly = $true
    $txtLog.Anchor = "Top,Bottom,Left,Right"
    $txtLog.BackColor = [System.Drawing.Color]::FromArgb(17, 24, 39)
    $txtLog.ForeColor = $lightText
    $txtLog.BorderStyle = "FixedSingle"
    $form.Controls.Add($txtLog)

    $script:AllPackages = New-Object System.Collections.ArrayList
    $bindingListType = "System.ComponentModel.BindingList[object]"

    function Bind-Grid
    {
        param([string]$FilterText = "")

        $filtered = $script:AllPackages | Where-Object {
            [string]::IsNullOrWhiteSpace($FilterText) -or
            $_.Id -like "*$FilterText*" -or
            $_.Version -like "*$FilterText*" -or
            $_.Source -like "*$FilterText*"
        }

        $binding = New-Object $bindingListType
        foreach ($item in $filtered)
        {
            [void]$binding.Add($item)
        }

        $grid.DataSource = $binding
    }

    function Load-Packages
    {
        $form.Cursor = [System.Windows.Forms.Cursors]::WaitCursor
        $btnRefresh.Enabled = $false
        $btnRun.Enabled = $false

        try
        {
            $txtLog.Clear()
            Write-Log -TextBox $txtLog -Message "Loading packages with winget export..."
            $packages = Get-WingetExportPackages

            [void]$script:AllPackages.Clear()
            foreach ($pkg in $packages)
            {
                [void]$script:AllPackages.Add($pkg)
            }

            Bind-Grid -FilterText $txtFilter.Text
            Write-Log -TextBox $txtLog -Message "$($script:AllPackages.Count) package(s) loaded."
        } catch
        {
            Write-Log -TextBox $txtLog -Message "Error: $($_.Exception.Message)"
            [System.Windows.Forms.MessageBox]::Show(
                $_.Exception.Message,
                "Error",
                [System.Windows.Forms.MessageBoxButtons]::OK,
                [System.Windows.Forms.MessageBoxIcon]::Error
            ) | Out-Null
        } finally
        {
            $btnRefresh.Enabled = $true
            $btnRun.Enabled = $true
            $form.Cursor = [System.Windows.Forms.Cursors]::Default
        }
    }

    $txtFilter.Add_TextChanged({
            Bind-Grid -FilterText $txtFilter.Text
        })

    $btnRefresh.Add_Click({
            Load-Packages
        })

    $btnSelectAll.Add_Click({
            foreach ($item in $grid.DataSource)
            {
                $item.Selected = $true
            }
            $grid.Refresh()
        })

    $btnUnselectAll.Add_Click({
            foreach ($item in $grid.DataSource)
            {
                $item.Selected = $false
            }
            $grid.Refresh()
        })

    $btnRun.Add_Click({
            $grid.EndEdit()
            $selected = @($script:AllPackages | Where-Object { $_.Selected })

            if ($selected.Count -eq 0)
            {
                [System.Windows.Forms.MessageBox]::Show(
                    "No packages selected.",
                    "Information",
                    [System.Windows.Forms.MessageBoxButtons]::OK,
                    [System.Windows.Forms.MessageBoxIcon]::Information
                ) | Out-Null
                return
            }

            $msg = "This will uninstall and reinstall $($selected.Count) package(s).`n`nContinue?"
            $confirm = [System.Windows.Forms.MessageBox]::Show(
                $msg,
                "Confirmation",
                [System.Windows.Forms.MessageBoxButtons]::YesNo,
                [System.Windows.Forms.MessageBoxIcon]::Warning
            )

            if ($confirm -ne [System.Windows.Forms.DialogResult]::Yes)
            {
                return
            }

            $form.Cursor = [System.Windows.Forms.Cursors]::WaitCursor
            $btnRun.Enabled = $false
            $btnRefresh.Enabled = $false

            try
            {
                foreach ($pkg in $selected)
                {
                    Reinstall-WingetPackage -Id $pkg.Id -Version $pkg.Version -LogBox $txtLog
                }

                Write-Log -TextBox $txtLog -Message "Finished."
                [System.Windows.Forms.MessageBox]::Show(
                    "Operation finished. Check the log for details.",
                    "Finished",
                    [System.Windows.Forms.MessageBoxButtons]::OK,
                    [System.Windows.Forms.MessageBoxIcon]::Information
                ) | Out-Null
            } finally
            {
                $btnRun.Enabled = $true
                $btnRefresh.Enabled = $true
                $form.Cursor = [System.Windows.Forms.Cursors]::Default
            }
        })

    Load-Packages
    [void]$form.ShowDialog()
}

# =========================
# AI Commit
# =========================

function aic
{
    wsl -u ziede zsh -ic "aic"
}

# =========================
# Misc
# =========================

function update
{
    winget upgrade -r --include-unknown --accept-package-agreements --accept-source-agreements
    Move-SharedDesktopToCurrentUser -IncludeDefaultDesktop -MoveEverything
}

function ..
{
    param([int]$levels = 1)

    if ($levels -lt 1)
    {
        Write-Err "Please provide a positive number."
        return
    }

    $path = (Get-Location).Path
    for ($i = 0; $i -lt $levels; $i++)
    {
        $path = Split-Path $path -Parent
    }

    Set-Location $path
}

function reload
{
    . $PROFILE
    $env:PATH = [System.Environment]::GetEnvironmentVariable("PATH", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("PATH", "User")

    if (-not $IsQuiet)
    {
        Clear-Host
    }
}

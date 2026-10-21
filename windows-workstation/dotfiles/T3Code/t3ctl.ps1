# Manage the T3 Code server on this machine: its install, its boot task, and
# the two remote routes (T3 Connect and Tailscale Serve).
# Written for Windows PowerShell 5.1, which the boot task runs.
param([string]$Command)

$T3Home = if ($env:T3CODE_HOME) { $env:T3CODE_HOME } else { Join-Path $HOME ".t3" }
$TaskName = "T3 Code"
# The server listens only on this machine; each route reaches it from here.
$LocalHost = "127.0.0.1"
$LocalPort = 3773
$TailscalePort = 10000

function Write-Info($Message) { Write-Host "-> $Message" -ForegroundColor Cyan }
function Write-Ok($Message) { Write-Host "OK $Message" -ForegroundColor Green }
function Stop-WithError($Message) {
    Write-Host "ERROR $Message" -ForegroundColor Red
    exit 1
}

function Show-Usage {
    Write-Host @"
Usage: t3ctl <command>

  status    Show the server, its version and both remote routes.
  remote    Turn T3 Connect or Tailscale on or off, signing in and pairing as needed.
  update    Install the newest t3 from npm and restart the server on it.
  repair    Reinstall what is missing and recreate the boot task.
"@
}

function Get-T3Command {
    $command = Get-Command t3.cmd -ErrorAction SilentlyContinue
    if (-not $command) { Stop-WithError "t3 is not installed. Run: t3ctl repair" }
    return $command.Source
}

function Invoke-T3 {
    & (Get-T3Command) @args
}

# The npm launcher downloads each version into the T3 home. The boot task runs
# the newest complete download directly, so it keeps working when nvm switches
# Node versions and npm's global folder changes.
function Get-T3Runtime {
    $versions = Join-Path $T3Home "runtime\versions"
    $latest = Get-ChildItem -Path $versions -Directory -ErrorAction SilentlyContinue |
        Where-Object { (Test-Path (Join-Path $_.FullName ".install-complete")) -and (Test-Path (Join-Path $_.FullName "t3.exe")) } |
        Sort-Object { (Get-Item (Join-Path $_.FullName ".install-complete")).LastWriteTime } |
        Select-Object -Last 1
    if ($latest) { return Join-Path $latest.FullName "t3.exe" }
    return $null
}

function Test-ServerAnswers {
    $client = New-Object Net.Sockets.TcpClient
    try {
        $client.Connect($LocalHost, $LocalPort)
        return $true
    } catch {
        return $false
    } finally {
        $client.Close()
    }
}

# The boot task's loop starts the newest runtime again whenever the server
# exits, so a restart only stops the server process. The task itself starts
# only when it is not running.
function Restart-T3Server {
    $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if (-not $task) { Stop-WithError "The $TaskName boot task is missing. Run: t3ctl repair" }

    if ($task.State -eq "Running") {
        Write-Info "Restarting T3 Code"
        $runtimeRoot = Join-Path $T3Home "runtime"
        Get-Process t3 -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -and $_.Path.StartsWith($runtimeRoot, [StringComparison]::OrdinalIgnoreCase) } |
            Stop-Process -Force
        Start-Sleep -Seconds 1
    } else {
        Write-Info "Starting the $TaskName task"
        try {
            Start-ScheduledTask -TaskName $TaskName -ErrorAction Stop
        } catch {
            # A task registered from an administrator window may need one to start.
            $process = Start-Process -FilePath (Get-Process -Id $PID).Path -Verb RunAs -Wait -PassThru `
                -ArgumentList "-NoProfile", "-Command", "Start-ScheduledTask -TaskName '$TaskName'"
            if ($process.ExitCode -ne 0) { Stop-WithError "The $TaskName task did not start." }
        }
    }

    # t3 pair and t3 connect find the server through its runtime file, which
    # it writes only once it is ready.
    $runtimeFile = Join-Path $T3Home "userdata\server-runtime.json"
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        if ((Test-Path $runtimeFile) -and (Test-ServerAnswers)) {
            Write-Ok "T3 Code answers on http://${LocalHost}:$LocalPort/"
            return
        }
        Start-Sleep -Seconds 1
    }
    Stop-WithError "T3 Code did not answer on ${LocalHost}:$LocalPort. See $T3Home\logs\boot-task.log"
}

function Get-ConnectStatus {
    $json = Invoke-T3 connect status --json --log-level error 2>$null
    if ($LASTEXITCODE -ne 0) { return $null }
    return ($json -join "`n") | ConvertFrom-Json
}

function Test-ConnectEnabled {
    $status = Get-ConnectStatus
    return [bool]($status -and $status.desired)
}

function Test-TailscaleEnabled {
    $json = & tailscale serve status --json 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $json) { return $false }
    $serve = ($json -join "`n") | ConvertFrom-Json
    return [bool]($serve.TCP -and $serve.TCP.PSObject.Properties["$TailscalePort"])
}

function Get-TailscaleUrl {
    $json = & tailscale status --json 2>$null
    if ($LASTEXITCODE -ne 0) { return "" }
    $name = (($json -join "`n") | ConvertFrom-Json).Self.DNSName
    if (-not $name) { return "" }
    return "https://$($name.TrimEnd('.')):$TailscalePort/"
}

function Format-OnOff([bool]$Enabled) {
    if ($Enabled) { return "on" }
    return "off"
}

function Confirm-Yes($Question) {
    $answer = Read-Host "$Question [Y/n]"
    return -not ($answer -eq "n" -or $answer -eq "no")
}

# The server installs the relay client version pinned to its release and
# replaces any other managed version when it starts.
function Test-Relay {
    Write-Info "Waiting for the T3 Connect tunnel"
    $status = $null
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        $status = Get-ConnectStatus
        if ($status -and $status.reachability -and $status.reachability.status -eq "reachable") { break }
        Start-Sleep -Seconds 1
    }
    if (-not ($status -and $status.relayClient -and $status.relayClient.status -eq "available")) {
        Stop-WithError "The T3 relay client is not available. See: t3 connect status"
    }
    if ($status.relayClient.source -ne "managed") {
        Stop-WithError "The T3 relay client is not the one T3 installs. See: t3 connect status"
    }
    if (-not ($status.reachability -and $status.reachability.status -eq "reachable")) {
        Stop-WithError "T3 Connect is not reachable from other devices. See: t3 connect status"
    }
    Write-Ok "T3 Connect is reachable through relay client $($status.relayClient.version)"
}

function Invoke-Status {
    $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    $taskState = if ($task) { $task.State } else { "missing" }
    $answers = if (Test-ServerAnswers) { "answers" } else { "does not answer" }

    Write-Host ""
    Write-Host "T3 Code"
    Write-Host "  Version: $(Invoke-T3 --version 2>$null)"
    Write-Host "  Boot task: $taskState"
    Write-Host "  Local:   http://${LocalHost}:$LocalPort/ ($answers)"

    $tailscale = Test-TailscaleEnabled
    Write-Host ""
    Write-Host "Remote routes"
    Write-Host "  T3 Connect: $(Format-OnOff (Test-ConnectEnabled))"
    $tailscaleLine = "  Tailscale:  $(Format-OnOff $tailscale)"
    if ($tailscale) { $tailscaleLine += "  $(Get-TailscaleUrl)" }
    Write-Host $tailscaleLine

    Write-Host ""
    Write-Host "T3 Connect details"
    Invoke-T3 connect status --log-level error
}

function Enable-Connect {
    $status = Get-ConnectStatus
    if (-not ($status -and $status.authenticated)) {
        Write-Info "Signing in to T3 Connect. Approve the code from any device."
        Invoke-T3 connect login --headless
        if ($LASTEXITCODE -ne 0) { Stop-WithError "T3 Connect sign-in failed." }
    }
    Invoke-T3 connect link --headless
    if ($LASTEXITCODE -ne 0) { Stop-WithError "T3 Connect could not link this machine." }
    Restart-T3Server
    Test-Relay
    Write-Host ""
    Write-Host "Pairing"
    Invoke-T3 pair --log-level error
}

function Disable-Connect {
    Invoke-T3 connect unlink
    if ($LASTEXITCODE -ne 0) { Stop-WithError "T3 Connect could not unlink this machine." }
    Restart-T3Server
    Write-Ok "T3 Connect is off. The sign-in is kept for next time."
}

function Enable-Tailscale {
    & tailscale status *> $null
    if ($LASTEXITCODE -ne 0) {
        Write-Info "This machine is not connected to Tailscale. Signing in."
        & tailscale up
        if ($LASTEXITCODE -ne 0) { Stop-WithError "Tailscale sign-in failed." }
    }
    Write-Host ""
    Write-Host "Pairing"
    Invoke-T3 pair --tailscale --tailscale-serve-port $TailscalePort --log-level error
    if ($LASTEXITCODE -ne 0) { Stop-WithError "T3 could not publish itself through Tailscale Serve." }
}

function Disable-Tailscale {
    & tailscale serve "--https=$TailscalePort" off
    if ($LASTEXITCODE -ne 0) { Stop-WithError "Tailscale Serve did not remove port $TailscalePort." }
    Write-Ok "Tailscale route is off."
}

function Invoke-Remote {
    if (-not (Get-Command tailscale -ErrorAction SilentlyContinue)) { Stop-WithError "tailscale is not installed or is not in PATH." }
    if (-not (Test-ServerAnswers)) { Stop-WithError "T3 Code is not running. Run: t3ctl repair" }

    $connect = Test-ConnectEnabled
    $tailscale = Test-TailscaleEnabled
    Write-Host ""
    Write-Host "Remote routes"
    Write-Host "  1) T3 Connect  $(Format-OnOff $connect)"
    Write-Host "  2) Tailscale   $(Format-OnOff $tailscale)"
    $choice = Read-Host "Which route do you want to change? [1/2]"

    switch ($choice) {
        "1" {
            if ($connect) {
                if (Confirm-Yes "Turn T3 Connect off?") { Disable-Connect }
            } elseif (Confirm-Yes "Turn T3 Connect on?") {
                Enable-Connect
            }
        }
        "2" {
            if ($tailscale) {
                if (Confirm-Yes "Turn the Tailscale route off?") { Disable-Tailscale }
            } elseif (Confirm-Yes "Turn the Tailscale route on?") {
                Enable-Tailscale
            }
        }
        default { Stop-WithError "No route chosen." }
    }
}

function Install-T3Package($Spec) {
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { Stop-WithError "npm is not installed or is not in PATH." }
    Write-Info "Installing $Spec from npm"
    & npm install -g $Spec
    if ($LASTEXITCODE -ne 0) { Stop-WithError "npm could not install $Spec." }
    # The launcher downloads its runtime on first use.
    Invoke-T3 --version | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Get-T3Runtime)) { Stop-WithError "t3 could not download its runtime." }
}

function Register-BootTask {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        Write-Info "Registering the boot task needs administrator rights; a new window will ask."
        $process = Start-Process -FilePath (Get-Process -Id $PID).Path -Verb RunAs -Wait -PassThru `
            -ArgumentList "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$PSCommandPath`"", "register-task"
        if ($process.ExitCode -ne 0) { Stop-WithError "The $TaskName boot task was not registered." }
        return
    }

    $credential = Get-Credential -UserName "$env:USERDOMAIN\$env:USERNAME" `
        -Message "Windows password, stored by Windows so T3 Code starts at boot before you log in"
    if (-not $credential) { Stop-WithError "No password given; the boot task was not registered." }

    $action = New-ScheduledTaskAction -Execute "powershell.exe" -WorkingDirectory $HOME `
        -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$PSCommandPath`" serve"
    $trigger = New-ScheduledTaskTrigger -AtStartup
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable `
        -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings `
        -User $credential.UserName -Password $credential.GetNetworkCredential().Password -RunLevel Limited -Force | Out-Null
    if (-not (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue)) { Stop-WithError "The $TaskName boot task was not registered." }
    Write-Ok "Registered the $TaskName boot task"
}

# The boot task runs this. It restarts the server whenever it exits, which is
# what Restart=always does for the Linux service.
function Invoke-Serve {
    $logs = Join-Path $T3Home "logs"
    New-Item -ItemType Directory -Path $logs -Force | Out-Null
    while ($true) {
        $runtime = Get-T3Runtime
        if ($runtime) {
            Start-Process -FilePath $runtime -WorkingDirectory $HOME -NoNewWindow -Wait `
                -ArgumentList "serve", "--host", $LocalHost, "--port", $LocalPort `
                -RedirectStandardOutput (Join-Path $logs "boot-task.log") `
                -RedirectStandardError (Join-Path $logs "boot-task.error.log")
        }
        Start-Sleep -Seconds 5
    }
}

function Invoke-Update {
    Install-T3Package "t3@latest"
    Restart-T3Server
    if (Test-ConnectEnabled) { Test-Relay }
}

function Invoke-Repair {
    if (-not (Get-Command t3.cmd -ErrorAction SilentlyContinue) -or -not (Get-T3Runtime)) {
        Install-T3Package "t3"
    }
    if (-not (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue)) {
        Register-BootTask
    }
    Restart-T3Server
    if (Test-ConnectEnabled) { Test-Relay }
}

switch ($Command) {
    "status" { Invoke-Status }
    "remote" { Invoke-Remote }
    "update" { Invoke-Update }
    "repair" { Invoke-Repair }
    "serve" { Invoke-Serve }
    "register-task" { Register-BootTask }
    { $_ -in "help", "-h", "--help" } { Show-Usage }
    default {
        Show-Usage
        exit 2
    }
}

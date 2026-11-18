# Runs once at the first logon of the new Windows installation, from the seed disc.
# It keeps the test account logged in, keeps the display awake for the keys the xtask
# sends, and starts an OpenSSH server that accepts the xtask key. It shuts Windows down
# when everything succeeded, which tells the xtask that the installation is complete.
# On an error it writes the failure line that the image builder watches for to COM1, and
# leaves the window open with the error and the log in C:\inayos-setup.log.

$ErrorActionPreference = 'Stop'
Start-Transcript -Path 'C:\inayos-setup.log'
try {
    # The answer file logs on automatically only once. These values keep it on for every boot.
    $winlogon = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
    Set-ItemProperty -Path $winlogon -Name AutoAdminLogon -Value '1'
    Set-ItemProperty -Path $winlogon -Name DefaultUserName -Value '@USER@'
    Set-ItemProperty -Path $winlogon -Name DefaultPassword -Value '@PASSWORD@'
    Remove-ItemProperty -Path $winlogon -Name AutoLogonCount -ErrorAction SilentlyContinue

    powercfg.exe /change monitor-timeout-ac 0
    powercfg.exe /change standby-timeout-ac 0
    powercfg.exe /hibernate off

    $capability = Get-WindowsCapability -Online -Name 'OpenSSH.Server*' | Select-Object -First 1
    if ($capability.State -ne 'Installed') {
        Add-WindowsCapability -Online -Name $capability.Name | Out-Null
    }
    Set-Service -Name sshd -StartupType Automatic
    # The first start creates the host keys and C:\ProgramData\ssh.
    Start-Service -Name sshd

    # OpenSSH reads the keys of administrators from this file, and only when Administrators
    # and SYSTEM alone can access it. The SIDs keep the names independent of the language.
    $keys = 'C:\ProgramData\ssh\administrators_authorized_keys'
    Copy-Item -Path (Join-Path $PSScriptRoot 'authorized_keys') -Destination $keys -Force
    icacls.exe $keys /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "icacls could not restrict $keys" }

    if (-not (Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue)) {
        New-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -DisplayName 'OpenSSH Server (sshd)' `
            -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 -Profile Any | Out-Null
    } else {
        Set-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -Enabled True -Profile Any
    }
    Restart-Service -Name sshd

    Stop-Transcript
    shutdown.exe /s /t 0
} catch {
    $failure = $_
    Write-Host "InayOS image setup failed: $failure"
    # The image builder reads the first serial port, which Windows calls COM1, and stops the
    # build when this line appears.
    try {
        $port = New-Object System.IO.Ports.SerialPort 'COM1', 115200
        $port.Open()
        $port.WriteLine("inayos-image: failed: $failure")
        $port.Close()
    } catch {
        Write-Host "could not report the failure on COM1: $_"
    }
    Stop-Transcript
    Read-Host 'Press Enter to close'
}

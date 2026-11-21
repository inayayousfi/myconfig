# Turns hibernation and Fast Startup on. A hybrid shutdown afterwards hibernates the Windows
# kernel session and leaves C: marked as hibernated, as the Shut down button does on a
# machine with Fast Startup.

$ErrorActionPreference = 'Stop'
powercfg.exe /hibernate on
if ($LASTEXITCODE -ne 0) { throw 'powercfg could not turn hibernation on' }
Set-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power' `
    -Name HiberbootEnabled -Value 1 -Type DWord

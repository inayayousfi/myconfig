# Turns BitLocker on for C: with a TPM protector, encrypting only the used space, and waits
# until the encryption has finished. The hardware test would need a restart first, so it is
# skipped.

$ErrorActionPreference = 'Stop'
if ((Get-BitLockerVolume -MountPoint 'C:').VolumeStatus -eq 'FullyDecrypted') {
    Enable-BitLocker -MountPoint 'C:' -EncryptionMethod XtsAes128 -UsedSpaceOnly `
        -TpmProtector -SkipHardwareTest | Out-Null
}
$deadline = (Get-Date).AddHours(1)
do {
    Start-Sleep -Seconds 10
    $volume = Get-BitLockerVolume -MountPoint 'C:'
    Write-Output "C: is $($volume.VolumeStatus) at $($volume.EncryptionPercentage)%"
} until ($volume.VolumeStatus -eq 'FullyEncrypted' -or (Get-Date) -gt $deadline)
if ($volume.VolumeStatus -ne 'FullyEncrypted') {
    throw "C: is still $($volume.VolumeStatus) after an hour"
}

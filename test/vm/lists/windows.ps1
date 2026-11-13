# Prints the state lists of the Windows guest in the form that test/vm/src/snapshot.rs reads.
# It runs in Windows PowerShell over SSH as the test account, which is an administrator.
#
# Program versions are left out: the installer upgrades packages, and removing a module does
# not downgrade what it upgraded.

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false

function Write-Entry([string] $Key, [string] $Value) {
    if ($Value) { [Console]::Out.WriteLine("$Key`t$Value") } else { [Console]::Out.WriteLine($Key) }
}

function Format-RegistryValue($Key, [string] $Name) {
    $kind = $Key.GetValueKind($Name)
    $data = $Key.GetValue($Name, $null, 'DoNotExpandEnvironmentNames')
    if ($data -is [byte[]]) { $data = ($data | ForEach-Object { $_.ToString('x2') }) -join '' }
    elseif ($data -is [array]) { $data = $data -join '|' }
    "$kind $data"
}

function Write-RegistryArea([string] $Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    $keys = @(Get-Item -LiteralPath $Path) + @(Get-ChildItem -LiteralPath $Path -Recurse -ErrorAction SilentlyContinue)
    foreach ($key in $keys) {
        $name = $key.Name
        Write-Entry $name 'key'
        foreach ($value in $key.GetValueNames()) {
            Write-Entry "$name\$value" (Format-RegistryValue $key $value)
        }
    }
}

[Console]::Out.WriteLine('## programs')
$uninstall = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
)
foreach ($path in $uninstall) {
    if (-not (Test-Path -LiteralPath $path)) { continue }
    foreach ($key in Get-ChildItem -LiteralPath $path) {
        $display = $key.GetValue('DisplayName')
        if ($display) { Write-Entry $display } else { Write-Entry $key.PSChildName }
    }
}

# The areas that the Windows modules change, and where programs register themselves to start.
[Console]::Out.WriteLine('## registry')
$areas = @(
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced',
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Search',
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run',
    'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts',
    'HKCU:\Software\Policies',
    'HKCU:\Console',
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run',
    'HKLM:\SOFTWARE\Policies'
)
foreach ($area in $areas) { Write-RegistryArea $area }

[Console]::Out.WriteLine('## environment')
foreach ($scope in 'User', 'Machine') {
    $variables = [Environment]::GetEnvironmentVariables($scope)
    foreach ($name in $variables.Keys) { Write-Entry "$scope\$name" $variables[$name] }
}

[Console]::Out.WriteLine('## defender-exclusions')
$preference = Get-MpPreference
foreach ($kind in 'ExclusionPath', 'ExclusionProcess', 'ExclusionExtension') {
    foreach ($item in @($preference.$kind)) { if ($item) { Write-Entry "$kind\$item" } }
}

[Console]::Out.WriteLine('## services')
foreach ($service in Get-CimInstance -ClassName Win32_Service) {
    Write-Entry $service.Name $service.StartMode
}

[Console]::Out.WriteLine('## wsl-distributions')
$lxss = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss'
if (Test-Path -LiteralPath $lxss) {
    foreach ($key in Get-ChildItem -LiteralPath $lxss) {
        $name = $key.GetValue('DistributionName')
        if ($name) { Write-Entry $name }
    }
}

# Folders that are reparse points, such as the old "Application Data" junctions, are listed
# without being entered, so no folder is listed twice. A file that cannot be read, such as a
# registry hive in use, is listed as unreadable.
[Console]::Out.WriteLine('## files')
$folders = New-Object System.Collections.Stack
$folders.Push((Get-Item -LiteralPath $env:USERPROFILE -Force))
$sha256 = [System.Security.Cryptography.SHA256]::Create()
while ($folders.Count -gt 0) {
    $folder = $folders.Pop()
    Write-Entry $folder.FullName 'directory'
    try { $children = $folder.GetFileSystemInfos() } catch { continue }
    foreach ($child in $children) {
        if ($child -is [System.IO.DirectoryInfo]) {
            if ($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                Write-Entry $child.FullName 'link'
            } else {
                $folders.Push($child)
            }
            continue
        }
        try {
            $stream = [System.IO.File]::Open($child.FullName, 'Open', 'Read', 'ReadWrite, Delete')
            try { $hash = ($sha256.ComputeHash($stream) | ForEach-Object { $_.ToString('x2') }) -join '' }
            finally { $stream.Dispose() }
            Write-Entry $child.FullName $hash
        } catch {
            Write-Entry $child.FullName 'unreadable'
        }
    }
}

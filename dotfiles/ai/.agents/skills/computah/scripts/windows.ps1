param(
    [Parameter(Position = 0)]
    [string] $Command,

    [Parameter(Position = 1, ValueFromRemainingArguments = $true)]
    [string[]] $CommandArguments
)

$ErrorActionPreference = "Stop"
$source = Join-Path $PSScriptRoot "windows.cs"
$hash = (Get-FileHash -Algorithm SHA256 $source).Hash.Substring(0, 16).ToLowerInvariant()
$cache = Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "computah"
$helper = Join-Path $cache "computah-$hash.exe"

if (-not (Test-Path $helper)) {
    New-Item -ItemType Directory -Force -Path $cache | Out-Null
    Add-Type -Path $source `
        -ReferencedAssemblies System.Windows.Forms, System.Drawing `
        -OutputAssembly $helper `
        -OutputType ConsoleApplication
}

if ($Command -eq "helper-path") {
    $helper
    exit 0
}

if (-not $Command) {
    [Console]::Error.WriteLine("computah: missing command")
    exit 2
}

& $helper $Command @CommandArguments
exit $LASTEXITCODE

param(
    [string]$InstallerSource = (Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot '..\windows-workstation\install.ps1')),
    [string]$AgentScript
)

$ErrorActionPreference = 'Stop'
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseInput($InstallerSource, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
foreach ($name in @('Copy-DotfileSafe', 'Install-EmacsConfig')) {
    $function = $ast.Find({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true)
    if (-not $function) { throw "Missing installer function: $name" }
    Invoke-Expression $function.Extent.Text
}

# Only the extracted functions run. All file changes and executable lookup are mocked.
$script:logs = [System.Collections.Generic.List[string]]::new()
function Write-Log { param($Message, $Level) $script:logs.Add($Message) }
function Test-Path { param($Path) return $true }
function New-Item { param($ItemType, $Path, [switch]$Force, $ErrorAction) }
function Copy-Item { param($Path, $Destination, [switch]$Recurse, [switch]$Force, $ErrorAction) throw 'Injected copy failure' }
function Get-Command { param($Name, $ErrorAction) return [pscustomobject]@{ Source = 'Test-EmacsHome' } }
function Test-EmacsHome { $global:LASTEXITCODE = 0; return 'C:\emacs-test-home\' }
$SharedDotfilesDir = 'C:\source'

$failed = $false
try { Install-EmacsConfig } catch { $failed = $true }
if (-not $failed) { throw 'Emacs installation continued after required copy failure' }
if ($script:logs | Where-Object { $_ -like 'Emacs configuration installed*' }) {
    throw 'Emacs installation reported success after copy failure'
}
Copy-DotfileSafe -Source 'optional' -Destination 'C:\optional'

function Test-Path { param($Path) return $false }
$failed = $false
try { Copy-DotfileSafe -Source 'missing' -Destination 'C:\required' -Required } catch { $failed = $true }
if (-not $failed) { throw 'Required missing source did not fail' }

foreach ($case in @('missing-executable', 'home-query-failure', 'empty-home', 'missing-source')) {
    $script:logs.Clear()
    function Get-Command {
        param($Name, $ErrorAction)
        if ($case -eq 'missing-executable') { return $null }
        return [pscustomobject]@{ Source = 'Test-EmacsHome' }
    }
    function Test-EmacsHome {
        $global:LASTEXITCODE = $(if ($case -eq 'home-query-failure') { 1 } else { 0 })
        if ($case -eq 'empty-home') { return '' }
        return 'C:\emacs-test-home\'
    }
    function Test-Path { param($Path) return ($case -ne 'missing-source') }
    $failed = $false
    $expected = switch ($case) {
        'missing-executable' { 'Emacs executable was not found after package installation' }
        'home-query-failure' { 'Could not resolve Emacs home directory with Emacs itself' }
        'empty-home' { 'Could not resolve Emacs home directory with Emacs itself' }
        'missing-source' { 'Emacs configuration source not found: C:\source\emacs\.config\emacs' }
    }
    try { Install-EmacsConfig } catch {
        if ($_.Exception.Message -ne $expected) { throw }
        $failed = $true
    }
    if (-not $failed) { throw "Emacs installation continued after $case" }
    if ($script:logs | Where-Object { $_ -like 'Emacs configuration installed*' }) {
        throw "Emacs installation reported success after $case"
    }
}

if ($AgentScript) {
    $script:agentRan = $false
    function test-agent { $script:agentRan = $true }
    $failed = $false
    try { & ([scriptblock]::Create($AgentScript)) } catch { $failed = $true }
    if (-not $failed -or $script:agentRan) { throw 'Agent ran after a failed working-directory change' }
}

Write-Output 'Windows parser, prerequisite failures, required-copy failure, optional-copy behavior, and launch failure checks passed.'

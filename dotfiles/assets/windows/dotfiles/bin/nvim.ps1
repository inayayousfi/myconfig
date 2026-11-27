# Opens files in Neovim inside the default WSL distribution and waits until it exits.
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$FileArgs
)

$wslArgs = foreach ($arg in $FileArgs) {
    # Only arguments that look like a Windows path become WSL paths. wslpath accepts any
    # text as a relative path, so flags and Ex commands pass through unchanged.
    if ($arg -match '^[A-Za-z]:[\\/]' -or $arg -match '^\\\\') {
        $converted = & wsl.exe --exec wslpath -a $arg 2>$null
        if ($LASTEXITCODE -eq 0 -and $converted) { $converted } else { $arg }
    } else {
        $arg
    }
}

# --exec skips the login shell, whose history expansion would change Ex commands.
& wsl.exe --exec nvim @wslArgs
exit $LASTEXITCODE

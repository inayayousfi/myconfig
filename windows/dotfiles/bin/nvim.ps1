param(
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$FileArgs
)

$wslArgs = foreach ($arg in $FileArgs)
{
  # Only rewrite arguments that actually look like a Windows path (drive
  # letter or UNC prefix); everything else (flags, ex-commands) passes
  # through untouched, since wslpath happily "succeeds" on arbitrary
  # strings by resolving them as relative paths against the CWD.
  if ($arg -match '^[A-Za-z]:[\\/]' -or $arg -match '^\\\\')
  {
    $converted = & wsl.exe --exec wslpath -a $arg 2>$null
    if ($LASTEXITCODE -eq 0 -and $converted)
    {
      $converted
    } else
    {
      $arg
    }
  } else
  {
    $arg
  }
}

& wsl.exe -u $env:USERNAME --exec nvim @wslArgs
exit $LASTEXITCODE

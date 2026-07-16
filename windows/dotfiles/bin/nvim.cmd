@echo off
powershell.exe -NoLogo -NoProfile -File "%~dp0nvim.ps1" %*
exit /b %ERRORLEVEL%

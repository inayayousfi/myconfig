# VSCodium

This document describes the CachyOS VSCodium module, `linux/modules/vscodium.sh`, the `vscodium` package, and the Windows editor script, `windows-workstation/install-editor.ps1`.

The `vscodium` package holds the editor settings, the key bindings, the extension list and the Black and Pink theme source. VS Code reads the same settings format, so the Windows script applies the same files to both VSCodium and VS Code.

On CachyOS, Stow links the settings, the key bindings, the extension list and the theme source into `~/.config/VSCodium/`. The installer keeps `~/.config/VSCodium/User` a real directory, because VSCodium writes its history and workspace storage there. Changes made in VSCodium's settings screen are written to the deployed package in `~/dotfiles/vscodium`.

The module installs VSCodium, checks that the package was stowed, makes VSCodium the default application for text files, and installs every extension on the list together with Open Remote SSH. VSCodium ignores extension directories that it did not install itself, so the module packs the theme source into a `.vsix` archive and installs it as an extension. The theme therefore changes in the editor only when the module runs again.

On Windows, VSCodium belongs to the optional DevTools packages. The workstation installer always runs the editor script. The script copies the settings and key bindings into the VSCodium and VS Code user directories. For each editor whose command is on the path, it installs the theme archive, the extension list and the editor's own remote SSH extension. It then sets `EDITOR`, `VISUAL` and Git's editor to the first of VSCodium and VS Code whose command is on the path, with `--wait`, or to Notepad when neither is. The script also runs alone on a machine where only the editor configuration is wanted: right-click it and choose Run with PowerShell. Started that way, it waits for Enter before its window closes. The workstation installer passes `-FromInstaller`, so it does not wait and failures stop the installer.

The settings turn off the AI features and the telemetry, experiments and online settings search. They also turn off the folder trust question, and on Windows they let VSCodium open network paths such as `\\wsl.localhost` without asking. Files are saved half a second after typing stops. VSCodium formats only explicit saves, so the Nifty Format on Save extension formats the automatic saves. basedpyright provides Python language support, so the Python extension's own language server stays off.

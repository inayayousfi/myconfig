# Repository instructions

## Purpose

This repository builds configuration environments for CachyOS, Arch WSL, Ubuntu Server, and Windows Workstation. The repository is the source of truth. Inspect the source here before changing a deployed configuration.

`README.md` gives the supported entry points and bootstrap commands.

## Source and deployed state

The repository source is `/home/iy/Projets/myconfig` in this workspace. On Linux, the installer copies the selected packages into `~/dotfiles`, backs up the previous deployed tree, and uses GNU Stow to link their contents into `$HOME`.

`~/dotfiles` is the deployed package tree used by the live Linux environment. It is not an independent source tree. Do not edit it as the primary fix. Trace the package back to `dotfiles/<package>/`, change the repository source, then redeploy or restow it when the task requires a live change.

The deployed package tree is `~/dotfiles`, not `~/.files`. If a task mentions a home-directory file tree, check `~/dotfiles` and the relevant links under `$HOME` before assuming another path.

Some home paths point into `~/dotfiles`. A change or deletion inside the deployed tree can therefore change the live configuration even when the visible path is under `~/.config` or `~`. Check both the visible target and its resolved path with `readlink -f`.

## Repository layout

The top-level areas have different jobs:

- `dotfiles/` contains packages whose paths mirror their destination under the home directory. Linux selects and stows these packages. `dotfiles/old/` contains retired packages and is not installed by current profiles.

- `linux/modules/` contains component installers and validators. A module owns the packages, files, services, and checks for one concern.

- `linux/profiles/` composes modules for a platform profile. Do not put profile-specific branching into a shared module when the profile composition can express it.

- `linux/adapters/` and `linux/lib/` contain package-manager and shared installer mechanisms. `linux/registry/` maps logical package names to platform package identifiers.

- `linux/install.sh` loads the common libraries, adapters, registry, and modules, then runs the selected profile. The profile is the execution plan. Inspect it before assuming that a module runs on every platform.

- `windows-workstation/` is a separate Windows installer and owns Windows-specific files, package setup, PowerShell configuration, and direct-copy behavior. It is not a Linux Stow target.

- `docs/` contains documents that each describe one module or package. Read the document for a module before changing it.

- `test/` contains installer and workbench tests. Run the narrowest relevant test after a change, then run the broader platform test when the change affects shared installer behavior.

## Linux installation model

The installer uses `MYCONFIG_DOTFILES_SOURCE` when set. Otherwise it reads from the repository's `dotfiles/` directory. It stages selected packages, validates the staged copy, replaces `~/dotfiles` with a backed-up tree, and restows the selected packages into `$HOME`.

The installer targets a computer that has never run it. A module only installs and configures: it never undoes what an earlier run of this repository installed. Removing what the distribution itself installs on a fresh machine is part of setting that machine up and stays allowed. Uninstalling belongs to a separate uninstall script, never to a module.

When adding a Linux-managed configuration:

1. Put the source under the correct `dotfiles/<package>/` path so its relative path matches the intended home path.

2. Add or update the owning module if the package needs installation, cleanup, validation, services, or profile selection.

3. Add the package to the relevant profile instead of silently installing it for every profile.

4. Treat generated caches, package stores, editor state, and machine-specific credentials as runtime state unless the repository already defines them as source files.

5. Add a check when the installer can otherwise report success while leaving the required live link, file, or service missing.

After a Linux source change, inspect both the repository path and the deployed path. Use `readlink -f` to confirm that a live file resolves to the intended source or deployed package.

## Agent configuration

The `ai` package owns the shared agent source, Claude configuration, and OpenCode configuration. On Linux, Stow deploys it through `~/.agents/`, `~/.claude/`, and `~/.config/opencode/`. OpenCode also needs the generated link at `~/.config/opencode/AGENTS.md`, which points to `~/.agents/AGENTS.md`.

The `ai` package also ships `claude-config-helper`, a Python command shared by Linux and Windows. `docs/agent-config.md` describes it.

The installer creates the OpenCode link during the agent configuration step. Do not add another stored `AGENTS.md` under the OpenCode configuration. The only instruction source is `dotfiles/ai/.agents/AGENTS.md`.

The live global instruction file is different from this repository instruction file. This root `AGENTS.md` describes work inside this repository. `dotfiles/ai/.agents/AGENTS.md` supplies the user's global agent workflow after deployment.

## KDE Plasma

`docs/kde-plasma.md` describes the KDE module and the `plasma` package.

Plasma 6.8 crashes when a panel of an unplugged display is changed. A script that changes panels must leave the panels of unplugged displays alone.

KDE ignores `Hidden=true` overrides of launcher entries. To hide a launcher entry, add it to `myconfig-launcher-hide`.

## Windows boundary

Windows Workstation is intentionally a separate deployment model. Its installer uses PowerShell, Winget, direct copies, Windows-specific files, and separate handling for the agent configuration. It does not provide the same Linux package tree or GNU Stow ownership model.

Share a source file only when its behavior and path are genuinely portable. Otherwise keep the Windows implementation in `windows-workstation/` and keep Linux behavior in `linux/` and `dotfiles/`. Do not make the Linux installer depend on Windows paths, and do not make the Windows installer depend on Linux links, systemd, GNU Stow, or Bash-only behavior.

## Verification

For changes to Linux installer code or shared package deployment, run:

```bash
bash -n linux/modules/<changed-module>.sh
test/test-linux.sh
git diff --check
```

For OpenCode or agent changes, verify both the repository bridge and the live resolved global file.

Do not overwrite unrelated user changes. Before editing, inspect `git status` and keep pre-existing modifications separate from the task.

# Repository instructions

## Purpose

This repository builds configuration environments for CachyOS, Arch WSL, Ubuntu Server, and Windows Workstation. The repository is the source of truth. Inspect the source here before changing a deployed configuration.

The detailed specification is in `SPECS.md`. `README.md` gives the supported entry points and bootstrap commands. Read the relevant sections of those files when a task depends on a component's documented behavior.

## Source and deployed state

The repository source is `/home/iy/Projets/myconfig` in this workspace. On Linux, the installer copies the selected packages into `~/dotfiles`, backs up the previous deployed tree, and uses GNU Stow to link their contents into `$HOME`.

`~/dotfiles` is the deployed package tree used by the live Linux environment. It is not an independent source tree. Do not edit it as the primary fix. Trace the package back to `dotfiles/<package>/`, change the repository source, then redeploy or restow it when the task requires a live change.

The repository currently uses `~/dotfiles`, not `~/.files`. If a task mentions a home-directory file tree, check `~/dotfiles` and the relevant links under `$HOME` before assuming another path.

Some home paths point into `~/dotfiles`. A change or deletion inside the deployed tree can therefore change the live configuration even when the visible path is under `~/.config` or `~`. Check both the visible target and its resolved path with `readlink -f`.

## Repository layout

The top-level areas have different jobs:

- `dotfiles/` contains packages whose paths mirror their destination under the home directory. Linux selects and stows these packages. `dotfiles/old/` contains retired packages and is not installed by current profiles.

- `linux/modules/` contains component installers and validators. A module owns the packages, files, services, checks, and cleanup for one concern.

- `linux/profiles/` composes modules for a platform profile. Do not put profile-specific branching into a shared module when the profile composition can express it.

- `linux/adapters/` and `linux/lib/` contain package-manager and shared installer mechanisms. `linux/registry/` maps logical package names to platform package identifiers.

- `linux/install.sh` loads the common libraries, adapters, registry, and modules, then runs the selected profile. The profile is the execution plan. Inspect it before assuming that a module runs on every platform.

- `windows-workstation/` is a separate Windows installer and owns Windows-specific files, package setup, PowerShell configuration, and direct-copy behavior. It is not a Linux Stow target.

- `test/` contains installer and workbench tests. Run the narrowest relevant test after a change, then run the broader platform test when the change affects shared installer behavior.

## Linux installation model

Linux profiles currently select these dotfile packages:

- CachyOS: `zsh`, `yazi`, `ai`, `kanata`, `kanata-kde`, `handy`, `kde-plasma`, `emacs`, `phone`, and `pipewire`.

- Arch WSL: `zsh`, `yazi`, and `ai`.

- Ubuntu Server: `zsh` only.

The installer uses `MYCONFIG_DOTFILES_SOURCE` when set. Otherwise it reads from the repository's `dotfiles/` directory. It stages selected packages, validates the staged copy, replaces `~/dotfiles` with a backed-up tree, and restows the selected packages into `$HOME`.

When adding a Linux-managed configuration:

1. Put the source under the correct `dotfiles/<package>/` path so its relative path matches the intended home path.

2. Add or update the owning module if the package needs installation, cleanup, validation, services, or profile selection.

3. Add the package to the relevant profile instead of silently installing it for every profile.

4. Treat generated caches, package stores, editor state, and machine-specific credentials as runtime state unless the repository already defines them as source files.

5. Add a check when the installer can otherwise report success while leaving the required live link, file, or service missing.

After a Linux source change, inspect both the repository path and the deployed path. Use `readlink -f` to confirm that a live file resolves to the intended source or deployed package.

## Agent configuration

The `ai` package owns the shared agent source, Claude configuration, and OpenCode configuration. On Linux, Stow deploys it through `~/.agents/`, `~/.claude/`, and `~/.config/opencode/`. OpenCode also needs the generated link at `~/.config/opencode/AGENTS.md`, which points to `~/.agents/AGENTS.md`.

The installer creates the OpenCode link during the agent configuration step. Do not add another stored `AGENTS.md` under the OpenCode configuration. The only instruction source is `dotfiles/ai/.agents/AGENTS.md`.

The live global instruction file is different from this repository instruction file. This root `AGENTS.md` describes work inside this repository. `dotfiles/ai/.agents/AGENTS.md` supplies the user's global agent workflow after deployment.

## Emacs

Linux manages the Emacs package through GNU Stow. A normal directory such as `~/.config/emacs` can contain generated runtime state, while tracked configuration files or a tracked subdirectory may resolve through links into the deployed `emacs` package. Do not mistake a regular folded directory or generated cache for an unmanaged configuration.

The CachyOS Emacs module validates the stowed configuration and manages the user service and launcher files. Inspect `linux/modules/emacs.sh` and the package contents before changing startup or state paths.

Windows uses a separate direct-copy installer for Emacs. Do not assume that Linux Stow links, Linux service files, or Linux paths apply to the Windows installation.

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

For Emacs changes, also run the relevant Emacs workbench test from `test/` and check that tracked configuration files still resolve to the intended package. For OpenCode or agent changes, verify both the repository bridge and the live resolved global file.

Do not overwrite unrelated user changes. Before editing, inspect `git status` and keep pre-existing modifications separate from the task.

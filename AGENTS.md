# Repository instructions

## Purpose

This repository builds configuration environments for CachyOS, Arch WSL, Ubuntu Devbox, and Windows Workstation. The repository is the source of truth. Inspect the source here before changing a deployed configuration.

`README.md` gives the supported entry points and install commands. The root `install.sh` and `install.ps1` download the released binary for the platform, keep it as `myconfig`, and run it. The release workflow publishes those binaries under fixed names.

The project is moving away from shell scripts. Write new installer logic, tooling and tests in Rust. `install.sh` and `install.ps1` stay scripts because `curl | bash` and `irm | iex` run them before any binary exists.

## Source and deployed state

The repository source is `/home/iy/Projets/myconfig` in this workspace. On Linux, each installer module stages its own config package into `~/dotfiles/<package>` and uses GNU Stow to link its contents into `$HOME`.

`~/dotfiles` is the deployed package tree used by the live Linux environment. It is not an independent source tree. Do not edit it as the primary fix. Trace the package back to `dotfiles/<package>/`, change the repository source, then reinstall the owning module when the task requires a live change.

The deployed package tree is `~/dotfiles`, not `~/.files`. If a task mentions a home-directory file tree, check `~/dotfiles` and the relevant links under `$HOME` before assuming another path.

Some home paths point into `~/dotfiles`. A change or deletion inside the deployed tree can therefore change the live configuration even when the visible path is under `~/.config` or `~`. Check both the visible target and its resolved path with `readlink -f`.

The installers record what they change in `~/.local/state/myconfig/state.json` (`%LOCALAPPDATA%\myconfig\state\state.json` on Windows). `myconfig remove` undoes from that record. Do not edit it by hand.

## Repository layout

The top-level areas have different jobs:

- `dotfiles/` contains packages whose paths mirror their destination under the home directory. The binaries embed only the packages and asset folders that `crates/embedded-dotfiles` lists. `dotfiles/old/` contains retired packages that no profile installs, and `dotfiles/assets/` contains files that modules install somewhere other than the home directory.

- `apps/` contains one installer per platform: `cachyos`, `arch-wsl`, `ubuntu-devbox`, and `windows`. Each `main.rs` holds that profile's module list, which is the execution plan. Inspect it before assuming that a module runs on a platform.

- `crates/` contains the shared Rust crates. `MODULES-DESIGN.md` at the repository root describes their structure. See "Rust installers" below.

- `docs/` contains documents that each describe one module or package. Read the document for a module before changing it.

- `test/` contains the Python and shell tests for files in `dotfiles/`. Rust tests live next to the code they test, and `crates/repository-tests` tests repository files that are not Rust code.

- `test/vm/` contains the VM xtask, run as `cargo xtask vm`. It builds the installers from the working tree and runs them in throwaway QEMU guests. `test/vm/README.md` describes it and says which parts have run on a real guest.

## Installation model

The installer targets a computer that has never run it. A module's `install` only installs and configures: it never undoes what an earlier run of this repository installed. Removing what the distribution itself installs on a fresh machine is part of setting that machine up and stays allowed. Undoing an installation belongs to `remove`, which reverses the changes that `install` recorded.

When adding a managed configuration:

1. Put the source under the correct `dotfiles/<package>/` path so its relative path matches the intended home path.

2. Embed the package or asset folder as its own static in `crates/embedded-dotfiles`. Add a Linux config package to `CONFIG_PACKAGES` there, which `ctx.deploy_config` reads.

3. Deploy it from the module that owns the concern, with `ctx.deploy_config` on Linux or recorded copies on Windows. Add or update that module if the configuration needs packages, services, settings or checks.

4. Add the module to the relevant profile lists instead of to every profile.

5. Treat generated caches, package stores, editor state, and machine-specific credentials as runtime state unless the repository already defines them as source files.

6. Make `verify` fail when the installer could otherwise report success while leaving the required live link, file, or service missing.

## Agent configuration

The `ai` package owns the shared agent source, Claude configuration, and OpenCode configuration. On Linux, the `agent-config` module stows it through `~/.agents/`, `~/.claude/`, and `~/.config/opencode/`. That module also creates the link at `~/.config/opencode/AGENTS.md`, which points to `~/.agents/AGENTS.md`.

The `ai` package also ships `claude-config-helper`, a Python command shared by Linux and Windows. `docs/agent-config.md` describes it.

Do not add another stored `AGENTS.md` under the OpenCode configuration. The only instruction source is `dotfiles/ai/.agents/AGENTS.md`.

The live global instruction file is different from this repository instruction file. This root `AGENTS.md` describes work inside this repository. `dotfiles/ai/.agents/AGENTS.md` supplies the user's global agent workflow after deployment.

## KDE Plasma

`docs/kde-plasma.md` describes the KDE module and the `plasma` package.

Plasma 6.8 crashes when a panel of an unplugged display is changed. A script that changes panels must leave the panels of unplugged displays alone.

KDE ignores `Hidden=true` overrides of launcher entries. To hide a launcher entry, add it to `myconfig-launcher-hide`.

## Windows boundary

Windows modules copy files with recorded writes, install packages with Winget, and change settings through the registry, PowerShell and Windows APIs. They do not provide the Linux package tree or GNU Stow ownership model.

Share a module only when its behavior and paths are genuinely portable. Otherwise give the concept one implementation per mechanism, as `agent_config/stowed.rs` and `agent_config/copied.rs` do. Do not make a Linux module depend on Windows paths, and do not make a Windows module depend on Linux links, systemd, GNU Stow, or Bash.

## Rust installers

`MODULES-DESIGN.md` at the repository root defines the structure, with examples. Do not add code in the earlier shape, where each module had its own trait and one struct per platform.

The design has these rules:

- `crates/modules` holds one `Module` trait with `name`, `footprint`, `needs`, `install`, `verify`, and `remove`. Every module implements all six. There are no per-module traits.

- A module is a struct. Anything that differs between platforms or profiles is a field on that struct, set in the app's profile list. Modules do not test the platform or profile at run time, and `Context` has no profile field.

- A function that only one module needs is an ordinary method on that struct. Add a function to the trait only when the runner or the interface must call it on every module.

- Source files are named after the concept, not the platform. A concept with one implementation is one file, such as `crates/modules/src/zsh.rs`. A concept with several implementations is a folder, with its options in `mod.rs` and one file per implementation, each named after its mechanism, such as `agent_config/stowed.rs` and `agent_config/copied.rs`. Do not add `linux/` or `windows/` folders.

- Each module owns its config package from `dotfiles/<package>/`. Linux implementations stow it, and Windows implementations copy it. There is no central Dotfiles module.

- A module changes the machine only through the recording functions on `Context`: `install_packages`, `uninstall_packages`, `set`, `add`, `unset`, `write_file`, `write_system_file`, `link`, `delete`, `delete_system_file`, `enable_service`, `disable_service`, and `deploy_config`. When a program makes a change by itself, record it around the call with `record_path`, `record_setting`, `record_setting_previous`, `record_service`, `disable_service_previous`, `created`, or `created_user_data`. When only another command can undo that change, record the command with `record_undo_command`. `remove` runs that command without checking the current state, so it must succeed when the change is already undone; otherwise add a `Setting` kind, whose undo reads the current state first. Each one records the previous state in `state.json` before acting, and `remove` undoes from that record. The one exception is a `Setting` kind that names the KDE files its tool writes: `set` records the keys the tool changed right after it runs, because only then are they known. A setting that only exists or not goes through `add`, because `set` refuses it. A change made any other way cannot be removed. When a module needs a new kind of change, add a recording function or a `Setting` kind for it.

- `footprint` lists the packages and settings the module needs. The "still needed?" check in `remove` reads it, so a module that needs a package installed by another module must list it too.

- `needs` names the other modules a module relies on, such as the module that installs a program, runtime or file it uses. Make it a hard need when the module cannot work without the other one, and a soft need when it relies on the other one only when the run includes it. Name a soft need through a name constant of the needed module, because no profile check catches a misspelled soft need. Put every needed module earlier in the profile list.

- Modules never read stdin, open `/dev/tty`, or call xshell's `.run()`, `.read()` or `.output()` directly. They use `ctx.run`, `ctx.read` and `ctx.succeeds` for commands, `ctx.confirm` for yes or no questions, and `ctx.with_terminal` for programs that need the whole terminal, such as the GitHub CLI login. A question asked after the install itself, whose missing answer the module takes as a no, goes through `ctx.confirm_optional`.

- `state.json` carries a `format` number and the `written_by` myconfig version. When you change the state format, increase `format` and add a migration from the previous one.

- `crates/interface` holds the command line (Clap) and the interactive screen (Ratatui). `crates/modules` must not depend on either. Steps that the installer starts by itself, such as an elevated child process, are hidden subcommands under `myconfig internal`.

- Each app's `main.rs` contains only the profile list, its platform check, and one call to `myconfig_interface::main`.

- The workspace version in the root `Cargo.toml` is the version of every crate.

## Verification

After a Rust change, run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets --target x86_64-pc-windows-msvc
git diff --check
```

The module tests in `crates/modules/src/tests/` run modules through the runner against fake system programs in a temporary home, so they never change the live system. `crates/repository-tests` runs programs such as `zsh`, `lua`, `python3`, `jq`, `stow` and `systemd-analyze`, which CI installs.

For OpenCode or agent changes, verify both the repository bridge and the live resolved global file. For changes to `test/vm/`, the workspace tests run its unit tests. A change to how it drives a guest also needs `cargo xtask vm cycle <os>`, run by hand on each guest system it affects. Before trusting a cycle result, read which parts of the xtask have run on a real guest in `test/vm/README.md`.

Do not overwrite unrelated user changes. Before editing, inspect `git status` and keep pre-existing modifications separate from the task.

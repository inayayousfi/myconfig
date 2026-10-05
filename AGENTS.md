# Repository instructions

## Purpose

This repository builds configuration environments for CachyOS, Arch WSL, Ubuntu Server, and Windows Workstation. The repository is the source of truth. Inspect the source here before changing a deployed configuration.

`README.md` gives the supported entry points and install commands. The root `install.sh` and `install.ps1` download the released binary for the platform, keep it as `myconfig`, and run it. The release workflow publishes those binaries under fixed names.

The project is moving away from shell scripts. Write new installer logic, tooling and tests in Rust. `install.sh` and `install.ps1` stay scripts because `curl | bash` and `irm | iex` run them before any binary exists.

## Source and deployed state

The repository source is `/home/iy/Projets/myconfig` in this workspace. On Linux, each installer module stages its own config package into `~/dotfiles/<package>` and uses GNU Stow to link its contents into `$HOME`.

`~/dotfiles` is the deployed package tree used by the live Linux environment. It is not an independent source tree. Do not edit it as the primary fix. Trace the package back to `dotfiles/<package>/`, change the repository source, then reinstall the owning module when the task requires a live change.

Some home paths point into `~/dotfiles`. A change or deletion inside the deployed tree can therefore change the live configuration even when the visible path is under `~/.config` or `~`. Check both the visible target and its resolved path with `readlink -f`.

The installers record what they change in `~/.local/state/myconfig/state.json` (`%LOCALAPPDATA%\myconfig\state\state.json` on Windows). `myconfig remove` undoes from that record. Do not edit it by hand.

## Repository layout

The top-level areas have different jobs:

- `dotfiles/` contains packages whose paths mirror their destination under the home directory. The binaries embed it at build time. `dotfiles/old/` contains retired packages that no profile installs, and `dotfiles/assets/` contains files that modules install somewhere other than the home directory.

- `apps/` contains one installer per platform: `cachyos`, `arch-wsl`, `ubuntu-server`, and `windows`. Each `main.rs` holds that profile's module list, which is the execution plan. Inspect it before assuming that a module runs on a platform.

- `crates/` contains the shared Rust crates. `MODULES-DESIGN.md` at the repository root describes their structure. See "Rust installers" below.

- `test/` contains the Emacs, Python and JavaScript tests for files in `dotfiles/`. Rust tests live next to the code they test, and `crates/repository-tests` tests repository files that are not Rust code.

- `vm/` contains the CachyOS virtual machine harness, which builds the CachyOS installer from the working tree and runs it in a disposable guest.

## Profiles

The profiles currently deploy these config packages:

- CachyOS: `zsh`, `yazi`, `ai`, `ghostty`, `kanata`, `kanata-kde`, `handy`, `kde-plasma`, `emacs`, `phone`, and `pipewire`.

- Arch WSL: `zsh`, `yazi`, and `ai`.

- Ubuntu Server: `zsh` only.

When adding a managed configuration:

1. Put the source under the correct `dotfiles/<package>/` path so its relative path matches the intended home path.

2. Deploy it from the module that owns the concern, with `ctx.deploy_config` on Linux or recorded copies on Windows. Add or update that module if the configuration needs packages, services, settings or checks.

3. Add the module to the relevant profile lists instead of to every profile.

4. Treat generated caches, package stores, editor state, and machine-specific credentials as runtime state unless the repository already defines them as source files.

5. Make `verify` fail when the installer could otherwise report success while leaving the required live link, file, or service missing.

## Agent configuration

The `ai` package owns the shared agent source, Claude configuration, and OpenCode configuration. On Linux, the `agent-config` module stows it through `~/.agents/`, `~/.claude/`, and `~/.config/opencode/`. That module also creates the link at `~/.config/opencode/AGENTS.md`, which points to `~/.agents/AGENTS.md`.

The `ai` package also ships `claude-config-helper`, a Python command shared by Linux and Windows. It trusts projects, lists or clears Claude's saved approvals, applies the MCP servers listed in `dotfiles/ai/.config/claude-config-helper/mcp-servers.json` through Claude's own commands, and checks tracked files for tokens, email addresses and home paths. Only trust and approval changes edit `~/.claude.json` directly.

Do not add another stored `AGENTS.md` under the OpenCode configuration. The only instruction source is `dotfiles/ai/.agents/AGENTS.md`.

The live global instruction file is different from this repository instruction file. This root `AGENTS.md` describes work inside this repository. `dotfiles/ai/.agents/AGENTS.md` supplies the user's global agent workflow after deployment.

## Emacs

On Linux, `EmacsStowed` deploys the `emacs` package through GNU Stow. A normal directory such as `~/.config/emacs` can contain generated runtime state, while tracked configuration files or a tracked subdirectory may resolve through links into the deployed `emacs` package. Do not mistake a regular folded directory or generated cache for an unmanaged configuration.

`EmacsStowed` requests the native Wayland package explicitly and manages the private-LAN browser-terminal firewall policy. Inspect `crates/modules/src/emacs/` and the package contents before changing startup or state paths.

On Windows, `EmacsCopied` copies the same files into the home folder that Emacs reports. Do not assume that Linux Stow links, Linux service files, or Linux paths apply to the Windows installation.

## Windows boundary

Windows modules copy files with recorded writes, install packages with Winget, and change settings through the registry, PowerShell and Windows APIs. They do not provide the Linux package tree or GNU Stow ownership model.

Share a module only when its behavior and paths are genuinely portable. Otherwise give the concept one implementation per mechanism, as `emacs/stowed.rs` and `emacs/copied.rs` do. Do not make a Linux module depend on Windows paths, and do not make a Windows module depend on Linux links, systemd, GNU Stow, or Bash.

## Rust installers

`MODULES-DESIGN.md` at the repository root defines the structure, with examples. Do not add code in the earlier shape, where each module had its own trait and one struct per platform.

The design has these rules:

- `crates/modules` holds one `Module` trait with `name`, `footprint`, `install`, `verify`, and `remove`. Every module implements all five. There are no per-module traits.

- A module is a struct. Anything that differs between platforms or profiles is a field on that struct, set in the app's profile list. Modules do not test the platform or profile at run time, and `Context` has no profile field.

- A function that only one module needs is an ordinary method on that struct. Add a function to the trait only when the runner or the interface must call it on every module.

- Source files are named after the concept, not the platform. A concept with one implementation is one file, such as `crates/modules/src/zsh.rs`. A concept with several implementations is a folder, with its options in `mod.rs` and one file per implementation, each named after its mechanism, such as `emacs/stowed.rs` and `emacs/copied.rs`. Do not add `linux/` or `windows/` folders.

- Each module owns its config package from `dotfiles/<package>/`. Linux implementations stow it, and Windows implementations copy it. There is no central Dotfiles module.

- A module changes the machine only through the recording functions on `Context`: `install_packages`, `uninstall_packages`, `set`, `unset`, `write_file`, `write_system_file`, `link`, `delete`, `delete_system_file`, `enable_service`, `deploy_config`, and `unstow_retired`. When a program makes a change by itself, record it around the call with `record_path`, `record_setting`, `record_setting_previous`, `created`, or `created_user_data`. Each one records the previous state in `state.json` before acting, and `remove` undoes from that record. A change made any other way cannot be removed. When a module needs a new kind of change, add a recording function or a `Setting` kind for it.

- `footprint` lists the packages and settings the module needs. The "still needed?" check in `remove` reads it, so a module that needs a package installed by another module must list it too.

- Modules never read stdin, open `/dev/tty`, or call xshell's `.run()`, `.read()` or `.output()` directly. They use `ctx.run`, `ctx.read` and `ctx.succeeds` for commands, `ctx.confirm` for yes or no questions, and `ctx.with_terminal` for programs that need the whole terminal, such as the Axidev greeter.

- `state.json` carries a `format` number and the `written_by` myconfig version. When you change the state format, increase `format` and add a migration from the previous one.

- `crates/interface` holds the command line (Clap) and the interactive screen (Ratatui). `crates/modules` must not depend on either. Steps that the installer starts by itself, such as an elevated child process or the Glass repair login service, are hidden subcommands under `myconfig internal`.

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

The module tests in `crates/modules/src/tests/` run modules through the runner against fake system programs in a temporary home, so they never change the live system. `crates/repository-tests` runs programs such as `zsh`, `node`, `lua`, `python3`, `resvg`, `jq`, `stow` and `systemd-analyze`, which CI installs.

For Emacs changes, also run the relevant Emacs workbench test from `test/` and check that tracked configuration files still resolve to the intended package. For OpenCode or agent changes, verify both the repository bridge and the live resolved global file. For changes to `vm/`, run `vm/test-cachyos.sh`.

Do not overwrite unrelated user changes. Before editing, inspect `git status` and keep pre-existing modifications separate from the task.

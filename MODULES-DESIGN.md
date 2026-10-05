# Rust module design

This document describes the structure of `crates/modules`, `crates/interface`, and the four apps in `apps/`. It replaces the earlier shape, where each module had its own one-function trait and one empty struct per platform.

## Why the earlier shape was wrong

Every module used to look like this:

```rust
pub trait ZshModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosZsh;
pub struct ArchWslZsh;

impl ZshModule for ArchWslZsh {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::ArchWsl {
            return Err("ArchWslZsh requires Arch WSL".into());
        }
        install_zsh(context) // shared body, which still tests context.profile inside
    }
}
```

This had three problems:

1. No code used `ZshModule` without knowing the concrete type. The trait abstracted nothing and only forced each `main.rs` to import about 50 names.

2. The platform was stated twice, once in the struct name and once in `context.profile`, and a run-time check made sure they agreed. An app would never call `UbuntuServerZsh` anyway.

3. The real platform difference was hidden inside the shared body. `install_zsh` skipped `chsh` with `if context.profile != Profile::ArchWsl`, and `install_agents_packages` added `ydotool` with `if context.profile == Profile::Cachyos`. Reading the profile did not show these differences.

## Crates

- `crates/modules` contains the `Module` trait, `Context`, the recorded state, the runner, and every module. It does not depend on Clap or Ratatui.

- `crates/interface` contains the command line (Clap) and the interactive screen (Ratatui). It depends on `crates/modules`.

- `apps/<platform>` contains only the profile list and one call to `myconfig_interface::main`.

## The `Module` trait

There is one trait, shared by Linux and Windows. Every module implements every operation.

```rust
pub trait Module {
    /// Name used by the command line, the screen and the recorded state.
    fn name(&self) -> &'static str;

    /// What the module needs on the machine. The "still needed?" check reads it.
    fn footprint(&self, ctx: &Context) -> Footprint;

    fn install(&self, ctx: &Context) -> ModuleResult;
    fn verify(&self, ctx: &Context) -> ModuleResult;

    /// Steps that recorded changes cannot express, such as calling an external
    /// uninstaller. The runner then undoes the recorded changes.
    fn remove(&self, ctx: &Context) -> ModuleResult;
}

pub struct Footprint {
    pub packages: Vec<Package>,
    pub settings: Vec<Setting>,
}
```

## `Context`

`Context` has no `profile` field. A module is only in a profile when it belongs there, so there are no run-time "requires CachyOS" checks.

Modules never read stdin, open `/dev/tty`, or call xshell's `.run()`, `.read()` or `.output()` directly. They go through `Context`, so the same module works under the screen and under the plain command line, and every change is recorded:

```rust
impl Context<'_> {
    // Commands. Output goes to the screen's log panel or to the terminal.
    pub fn run(&self, cmd: xshell::Cmd) -> ModuleResult;
    pub fn run_with_input(&self, cmd: xshell::Cmd, input: &[u8]) -> ModuleResult;
    pub fn run_status(&self, cmd: xshell::Cmd) -> ModuleResult<Option<i32>>;
    pub fn read(&self, cmd: xshell::Cmd) -> ModuleResult<String>;
    pub fn read_unchecked(&self, cmd: xshell::Cmd) -> ModuleResult<(bool, String)>;
    pub fn succeeds(&self, cmd: xshell::Cmd) -> ModuleResult<bool>;

    // Interaction.
    pub fn confirm(&self, question: &str) -> Result<bool, Unanswered>;
    pub fn with_terminal(&self, cmd: std::process::Command) -> ModuleResult;
    pub fn note(&self, message: &str);

    // Recorded changes. Each one records the previous state before acting.
    pub fn install_packages(&self, packages: &[Package]) -> ModuleResult;
    pub fn uninstall_packages(&self, packages: &[Package]) -> ModuleResult;
    pub fn set(&self, setting: Setting, value: &str) -> ModuleResult;
    pub fn unset(&self, setting: Setting) -> ModuleResult;
    pub fn write_file(&self, path: &Path, contents: &[u8], executable: bool) -> ModuleResult;
    pub fn write_system_file(&self, path: &Path, contents: &[u8], mode: &str) -> ModuleResult;
    pub fn link(&self, path: &Path, target: &Path) -> ModuleResult;
    pub fn delete(&self, path: &Path) -> ModuleResult;
    pub fn delete_system_file(&self, path: &Path) -> ModuleResult;
    pub fn enable_service(&self, unit: &str, scope: ServiceScope) -> ModuleResult;
    pub fn deploy_config(&self, package: &str) -> ModuleResult;
    pub fn unstow_retired(&self, package: &str) -> ModuleResult;

    // Changes that a program makes by itself, recorded around the call.
    pub fn record_path(&self, path: &Path) -> ModuleResult;
    pub fn record_setting(&self, setting: Setting) -> ModuleResult;
    pub fn record_setting_previous(&self, setting: Setting, previous: Option<String>) -> ModuleResult;
    pub fn created(&self, path: &Path, system: bool) -> ModuleResult;
    pub fn created_user_data(&self, path: &Path) -> ModuleResult;
}
```

`Interaction` is implemented twice, once by the screen and once by the command line, both in `crates/interface`.

`with_terminal` is for programs that talk to the terminal directly. Today only the Axidev greeter needs it. Everything else uses `run` or `confirm`.

A test checks that no module calls `.run()`, `.read()` or `.output()` on a command, reads stdin, or opens `/dev/tty`. The only exception is the elevated shared desktop step on Windows, which runs in its own console window.

## Settings

A `Setting` names one value outside the module's own files, such as a KDE key or a Git option. Each kind knows how to read its current value, write a value, and delete it:

- `KdeKey { file, groups, key }`, through `kreadconfig6` and `kwriteconfig6`.
- `LookAndFeel` and `CursorTheme`, through the KDE apply commands.
- `Gsettings { schema, key }`.
- `GitConfig { key }`, in the global Git configuration.
- `LoginShell { user }`.
- `RustupDefault`.
- `UfwRule { rule }` and `UfwEnabled`.
- `GroupMember { group, user }`.
- `ClaudeMcpServer { name }`.
- `RegistryValue { key, name }`, `MachinePathEntry { entry }` and `TaskbarAutoHide` on Windows.

## Recorded state

Before changing anything, a module records what was there. The state lives in one JSON file, written with serde:

- Linux: `$XDG_STATE_HOME/myconfig/state.json`, which defaults to `~/.local/state/myconfig/state.json`.
- Windows: `%LOCALAPPDATA%\myconfig\state\state.json`.

```json
{
  "format": 1,
  "written_by": "0.1.0",
  "snapshots": [
    {
      "id": 1,
      "action": "install",
      "started": 1790000000,
      "modules": {
        "zsh": [
          { "package_installed": "Zsh" },
          { "created": { "path": "/home/me/.oh-my-zsh", "system": false } },
          { "setting": { "setting": { "LoginShell": { "user": "me" } }, "previous": "/bin/bash" } }
        ],
        "windows-terminal": [
          { "file": { "path": "C:\\...\\settings.json", "system": false, "previous": "<base64>" } }
        ]
      }
    }
  ]
}
```

- **Versions.** `format` is the state format number and `written_by` is the myconfig version that wrote the file. A newer myconfig migrates an older format. An older myconfig refuses a newer format instead of guessing.

- **One snapshot per run.** Each install or remove run adds a snapshot listing what it changed. A setting or file is recorded only the first time a run touches it, so the record always holds the value from before that run.

- **File copies.** When `write_file` replaces a file, the previous contents go into the snapshot, encoded in base64. There are no more `<path>.backup.<date>` files next to your files.

- **Paru.** The first AUR package needs paru. When a configured repository has it, as CachyOS does, it is installed with pacman. Otherwise it comes from the AUR's `paru-bin`, which packages paru's official release binary instead of compiling it. Either way it is recorded under the module that needed it, so removing that module uninstalls paru too.

- **Packages.** `install_packages` records only the packages that were missing. A package that was already there is never uninstalled by `remove`. `uninstall_packages` records the packages it removed, so `remove` can reinstall them.

- **Atomic updates.** The runner writes the whole file to a temporary file in the same directory, flushes it to disk, then renames it over `state.json`. A crash leaves either the old file or the new one, never half of each. A lock file stops two runs from writing at the same time.

## Options live in struct fields

Arguments that differ between modules are not part of the trait. They are fields on the module struct, set by the app when it builds the profile. A difference between platforms is therefore always visible in the profile list.

```rust
pub struct Zsh {
    pub set_login_shell: bool,
}

impl Module for Zsh {
    fn name(&self) -> &'static str { "zsh" }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Zsh])?;
        // oh-my-zsh, plugins ...
        if self.set_login_shell {
            ctx.set(Setting::LoginShell { user }, &zsh)?;
        }
        ctx.deploy_config("zsh")
    }
    // footprint, verify, remove ...
}
```

A function that belongs to only one module is an ordinary method on that struct. A function moves into the trait only when the runner or the interface must call it on every module.

## File layout

There is one file per concept, with no `linux/` or `windows/` folders.

When a concept has a single implementation, it is one file:

```
crates/modules/src/zsh.rs
crates/modules/src/kanata.rs
crates/modules/src/powershell_profile.rs
```

When a concept works differently on different systems, it becomes a folder. The shared options go in `mod.rs`, and each implementation goes in its own file, named after its mechanism rather than its platform:

```
crates/modules/src/emacs/mod.rs      pub struct EmacsOptions { .. }
crates/modules/src/emacs/stowed.rs   pub struct EmacsStowed(pub EmacsOptions);  impl Module
crates/modules/src/emacs/copied.rs   pub struct EmacsCopied(pub EmacsOptions);  impl Module
```

Each implementation's `install` is one straight procedure with no branch on the platform.

## Each module owns its config files

There is no central Dotfiles module. Each module deploys and removes its own config package from `dotfiles/<package>/` with `ctx.deploy_config`:

- On Linux, `deploy_config` stages the package into `~/dotfiles/<package>` and links it into `$HOME` with GNU Stow. A file in the way is copied into the snapshot, then removed so Stow can link. `remove` unstows the package and puts the recorded files back.

- On Windows, a module copies the embedded files into their destination with `ctx.write_file`, so the previous files are recorded.

## Profiles are lists

Each app's `main.rs` is a list of modules plus one call to the interface.

```rust
// apps/arch-wsl/src/main.rs
let modules: &[&dyn Module] = &[
    &Base { packages: Base::ARCH, unwanted: &[] },
    &Ssh,
    &Cli { retired_config: &["hunk", "lazygit", "nvim", "tmux"] },
    &Runtimes,
    &Zsh { set_login_shell: false },
    &TerminalTools,
    &Tailscale,
    &AgentsPackages { extra: &[Package::WslSshAgent] },
    &AgentConfigStowed { ydotool: false },
    &GitConfig { windows_ssh: true },
    &EnvironmentInventory::ARCH_WSL,
];
myconfig_interface::main(Profile {
    title: "Arch WSL",
    package_system: PackageSystem::Arch,
    modules,
})
```

```rust
// apps/cachyos/src/main.rs (excerpt)
let modules: &[&dyn Module] = &[
    &PlasmaVersion,
    &Base { packages: Base::ARCH, unwanted: &[Package::CachyUpdate] },
    &Zsh { set_login_shell: true },
    &AgentsPackages { extra: &[Package::Ydotool] },
    &EmacsStowed(EmacsOptions { browser_terminal_firewall: true }),
    // ...
];
```

On Windows, each Winget package group is its own module, so you choose groups on the screen:

```rust
// apps/windows/src/main.rs (excerpt)
let modules: &[&dyn Module] = &[
    &PackageGroup::BASE,
    &PackageGroup::DEV_TOOLS,
    &PackageGroup::ART,
    &PackageGroup::SUPPLEMENTARY,
    &ArchWsl,
    &PowerShellProfile,
    &EmacsCopied(EmacsOptions { browser_terminal_firewall: false }),
    &LlvmPath,
    // ...
];
```

`LlvmPath` runs only if you leave it checked. The old `dev_tools: bool` that the Winget step returned is gone.

## The runner

The runner is shared code in `crates/modules`. It is the only code that loops over modules. The command line and the screen both call it.

- `install(ctx, modules)` runs `install` and then `verify` on each module, in profile order. An install that leaves a broken state fails right there.

- `verify(ctx, modules)` runs only `verify` on each module.

- `remove(ctx, profile, modules, force)` removes the given modules, as described below.

## Command line

Clap parses the arguments. Every app has the same commands:

```
<app>                      opens the screen
<app> install [module...]  installs all modules, or only the named ones, then verifies each
<app> verify [module...]   verifies all modules, or only the named ones
<app> remove <module...>   removes the named modules
<app> remove --force <module...>
                           also removes packages and settings that are still needed elsewhere
<app> list                 prints the module names in profile order
```

Steps that the installer starts by itself are hidden from `--help` under `<app> internal`:

```
<app> internal kde-plasma repair-glass
                           rebuilds Glass for the running KWin; the Glass repair user
                           service runs it at each Plasma login
<app> internal shared-desktop move <record> <desktop>
<app> internal shared-desktop move-back <change>
                           the elevated child that moves shared desktop items
```

## The screen

Ratatui draws one screen. All four apps use it, and running an app with no arguments opens it.

```
┌ myconfig: CachyOS ───────────────┐
│ [x] base            installed    │
│ [x] zsh             installed    │
│ [ ] emacs           missing      │
│ [x] kanata          broken       │
└──────────────────────────────────┘
 space toggle  i install  v verify
 r remove  q quit
```

- Every module in the profile is listed and checked by default. Space toggles the module under the cursor.

- `i`, `v` and `r` run install, verify or remove on the checked modules.

- The status column stays empty until you press `v`. The screen does not run `verify` when it opens, so it opens instantly and never asks for a password before it appears.

## Running inside the screen

The run stays inside the screen:

- **Permission.** On Linux, before the first action, a pop-up asks for the sudo password and passes it to `sudo -S -v`. A background thread then keeps the permission alive with `sudo -n true` every 60 seconds. Every later `sudo` runs without asking. On Windows, permission comes from the system's own elevation dialog, so there is no pop-up.

- **Output.** Commands started through `ctx.run` stream their output into a log panel below the module list.

- **Questions.** `ctx.confirm` shows a yes or no pop-up.

- **Whole-terminal programs.** `ctx.with_terminal` leaves the screen, runs the program in the normal terminal, then redraws the screen.

The runner works on a separate thread from the screen, and the two exchange output and questions through channels.

## Removing a module

`remove` handles one module at a time:

1. It runs the module's own `remove` steps, such as `axidev-osk-install uninstall`.

2. It undoes every change recorded for that module since its last removal, newest first. Install enables services last and installs packages first, so this order disables services first and uninstalls packages last.

Undoing each kind of change:

- **A setting** gets its recorded previous value back, or is deleted if it did not exist before.
- **A replaced file** gets its recorded previous contents back. A file that did not exist before is deleted.
- **A created path** is deleted.
- **A service** goes back to its recorded enabled state.
- **A deployed config package** is unstowed on Linux.
- **A package that install added** is uninstalled after the check below.
- **A package that install uninstalled** is reinstalled after a confirmation.
- **A package built from this repository**, such as the Glass KWin effect, is uninstalled.

**One-way changes.** Each of these runs only after a confirmation pop-up: reinstalling packages that install uninstalled, moving shared desktop items back, unregistering the Arch WSL distribution, and deleting `~/environment.md`, which agents edit after install.

**The running kernel** is never uninstalled. `remove` reports it and moves on.

**Still needed?** Before undoing a package or a setting, `remove` checks whether something else needs it:

- On every system, whether another module in the profile lists it in its `footprint`. This catches needs the package manager does not know about. For example, the zsh plugins need `git`, but the `zsh` package does not depend on it.

- On Linux, for packages only, whether pacman or APT reports another installed package that depends on it. Winget has no reliable query for this, so Windows uses only the first check.

- Always for `sudo` and `stow`, which `remove` itself runs: uninstalling them first would break every later undo.

If something still needs it, `remove` shows who and asks whether to remove it anyway.

**Questions nobody can answer.** `--force` answers yes to the "still needed?" questions only. The one-way confirmations are always asked. When no one can answer (no terminal, such as a script or a test), `remove` treats the question as no, writes the reason to stderr, and exits with code 3 once it has finished.

**Failure.** If any step fails, `remove` stops at that step. It reports what it removed and what is still in place, and it moves on to no other module. The changes it did undo are recorded, so running `remove` again continues where it stopped.

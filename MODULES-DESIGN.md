# Rust module design

This document describes the structure of `crates/modules`, `crates/interface`, and the four apps in `apps/`.

Every module implements one shared `Module` trait, and there are no per-module traits. A module is one struct, used on every platform where it belongs. A difference between platforms or profiles is a field on that struct, set in the profile list, so reading a profile shows every difference. No module tests the platform or the profile at run time.

## Crates

- `crates/modules` contains the `Module` trait, `Context`, the recorded state, the runner, and every module. It does not depend on Clap or Ratatui.

- `crates/interface` contains the command line (Clap) and the interactive screen (Ratatui). It depends on `crates/modules`.

- `apps/<platform>` contains only the profile list, its platform check, and one call to `myconfig_interface::main`.

## The `Module` trait

There is one trait, shared by Linux and Windows. Every module implements every operation.

```rust
pub trait Module {
    /// Name used by the command line, the screen and the recorded state.
    fn name(&self) -> &'static str;

    /// The modules this one relies on. The runner skips the module when one of them
    /// failed or was skipped.
    fn needs(&self) -> &'static [Need];

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

pub enum Need {
    Hard(&'static str),
    Soft(&'static str),
    EveryEarlier,
}
```

A need names another module that this one relies on, such as the module that installs a program it runs. A hard need is a module the profile must hold. A soft need matters only when the run includes that module. Every need comes earlier in the profile, and each app's tests check its profile with `runner::need_problems`.

That check cannot see a misspelled soft need, because a soft need may name a module that the profile lacks. A soft need therefore names its module through a constant of that module, such as `PlasmaVersion::NAME`, which the compiler checks.

`EveryEarlier` is a soft need on every module that comes earlier in the profile. The environment inventory uses it, because it states that every selected module completed.

A check that must pass before anything changes the machine is a module that `base` needs. Every other module of that profile needs `base` or the check, directly or through other needs, so none of them runs when the check fails. The CachyOS profile checks the KDE Plasma version this way, and its tests check that every module waits for it.

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
    pub fn has_terminal(&self) -> bool;
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
    pub fn disable_service(&self, unit: &str, scope: ServiceScope) -> ModuleResult;
    pub fn service_state(&self, unit: &str, scope: ServiceScope) -> ModuleResult<ServiceState>;
    pub fn deploy_config(&self, package: &str) -> ModuleResult;

    // Changes that a program makes by itself, recorded around the call.
    pub fn record_path(&self, path: &Path) -> ModuleResult;
    pub fn record_setting(&self, setting: Setting) -> ModuleResult;
    pub fn record_setting_previous(&self, setting: Setting, previous: Option<String>) -> ModuleResult;
    pub fn created(&self, path: &Path, system: bool) -> ModuleResult;
    pub fn created_user_data(&self, path: &Path) -> ModuleResult;
    pub fn record_undo_command(&self, program: &Path, args: &[&str]) -> ModuleResult;
    pub fn disable_service_previous(&self, unit: &str, scope: ServiceScope, before: ServiceState) -> ModuleResult;

    // A module that asks before installing.
    pub fn record_declined(&self) -> ModuleResult;
    pub fn record_accepted(&self) -> ModuleResult;
    pub fn declined(&self) -> bool;
}
```

`Interaction` is implemented twice, once by the screen and once by the command line, both in `crates/interface`.

`with_terminal` is for programs that talk to the terminal directly, such as the Axidev greeter and the GitHub CLI and Tailscale logins. Everything else uses `run` or `confirm`.

A module that asks before installing calls `confirm` at the start of `install`. On a no, it records the decline with `record_declined`, and its `verify` passes while `declined` is true. The next install asks again, and a yes marks the decline undone with `record_accepted`. When nobody can answer, the module returns the `Unanswered` error and the runner skips it.

A test checks that no module calls `.run()`, `.read()` or `.output()` on a command, reads stdin, or opens `/dev/tty`. The only exception is the elevated shared desktop step on Windows, which runs in its own console window.

## Settings

A `Setting` names one value outside the module's own files, such as a KDE key, a Git option, a group membership or a Windows registry value. The `Setting` enum in `crates/modules/src/setting/mod.rs` lists every kind.

Each kind lives in its own file under `crates/modules/src/setting/` and implements the private `Kind` trait, which describes, reads, writes and deletes the value. The variant names and fields are the serde names that `state.json` stores, so they never change.

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

- **File copies.** When `write_file` replaces a file, the previous contents go into the snapshot, encoded in base64. No backup file is left next to the replaced file.

- **Paru.** The first AUR package needs paru. When a configured repository has it, as CachyOS does, it is installed with pacman. Otherwise it comes from the AUR's `paru-bin`, which packages paru's official release binary instead of compiling it. Either way it is recorded under the module that needed it, so removing that module uninstalls paru too.

- **APT repositories.** The package catalog names the vendor APT repository of a package that Ubuntu does not carry, or carries only in old versions. `install_packages` adds that repository's key and source file with recorded writes before it installs the package.

- **Declines.** A module that asks before installing records a decline when the person says no. A decline has nothing to undo.

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
crates/modules/src/agent_config/mod.rs      pub use of both implementations
crates/modules/src/agent_config/stowed.rs   pub struct AgentConfigStowed { ydotool: bool };  impl Module
crates/modules/src/agent_config/copied.rs   pub struct AgentConfigCopied;  impl Module
```

Each implementation's `install` is one straight procedure with no branch on the platform.

## Each module owns its config files

There is no central Dotfiles module. Each module deploys and removes its own config package from `dotfiles/<package>/` with `ctx.deploy_config`:

- On Linux, `deploy_config` stages the package into `~/dotfiles/<package>` and links it into `$HOME` with GNU Stow. A file in the way is copied into the snapshot, then removed so Stow can link. `remove` unstows the package and puts the recorded files back.

- On Windows, a module copies the embedded files into their destination with `ctx.write_file`, so the previous files are recorded.

## Profiles are lists

Each app's `main.rs` holds the profile: a static list of modules in run order, each built with its options. After its platform check, it passes that list to `myconfig_interface::main` with the profile title and the package system.

On Windows, each Winget package group is its own module, so you choose groups on the screen. A module that relies on one of those groups names it as a need.

## The runner

The runner is shared code in `crates/modules`. It is the only code that loops over modules. The command line and the screen both call it.

- `install(ctx, profile, modules)` runs `install` and then `verify` on each module, in profile order. A module that fails does not stop the run. A module whose need failed or was skipped is skipped. A module whose `install` returns `Unanswered` is skipped too, with the question in the report. A hard need that the run leaves out must pass its `verify`, or the module is skipped too. A soft need that the run leaves out is fine. When the package database refresh before the first module fails, every module whose `changes_packages` is true is skipped, because installing packages after a failed `pacman -Syu` would leave a partly upgraded system. `changes_packages` is true when the footprint lists packages, and a module that installs a package missing from the catalog overrides it. At the end, `install` reports the failed modules and the skipped modules apart, and the screen marks each skipped module `skipped`. When nothing failed and every skip comes from an unanswered question, the command line exits with code 3, as `remove` does.

- A module whose `verify` passes because its install was declined is reported as declined, and the screen marks it `declined`.

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
│ [ ] ghostty         missing      │
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
- **A disabled service** is enabled and started again when it was enabled or active before.
- **A decline** has nothing to undo.
- **A recorded undo command** runs, without a check of the current state. It undoes a change that a program made by itself and that only another command can undo.
- **A deployed config package** is unstowed on Linux.
- **A package that install added** is uninstalled after the check below.
- **A package that install uninstalled** is reinstalled after a confirmation.
- **A package built from this repository** is uninstalled.

**One-way changes.** Each of these runs only after a confirmation pop-up: reinstalling packages that install uninstalled, moving shared desktop items back, unregistering the Arch WSL distribution, and deleting `~/environment.md`, which agents edit after install.

**The running kernel** is never uninstalled. `remove` reports it and moves on.

**Still needed?** Before undoing a package or a setting, `remove` checks whether something else needs it:

- On every system, whether another module in the profile lists it in its `footprint`. This catches needs the package manager does not know about. For example, the zsh plugins need `git`, but the `zsh` package does not depend on it.

- On Linux, for packages only, whether pacman or APT reports another installed package that depends on it. Winget has no reliable query for this, so Windows uses only the first check.

- Always for `sudo` and `stow`, which `remove` itself runs: uninstalling them first would break every later undo.

If something still needs it, `remove` shows who and asks whether to remove it anyway.

**Questions nobody can answer.** `--force` answers yes to the "still needed?" questions only. The one-way confirmations are always asked. When no one can answer (no terminal, such as a script or a test), `remove` treats the question as no, writes the reason to stderr, and exits with code 3 once it has finished.

**Failure.** If any step fails, `remove` stops at that step. It reports what it removed and what is still in place, and it moves on to no other module. The changes it did undo are recorded, so running `remove` again continues where it stopped.

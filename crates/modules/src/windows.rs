//! Contracts for the steps called by windows-workstation/install.ps1.
use crate::{ModuleContext, ModuleResult, Profile};
use embedded_dotfiles::DOTFILES;
use myconfig_utils::{
    ExistingFilePolicy, PackageSystem, emacs_home, find_program, install_embedded_file_with_policy,
    install_packages,
};
use package_catalog::Package;
use std::{
    fs,
    io::{self, Write},
    path::Path,
};
use typed_fs_rs::EmbeddedDirectory;
use xshell::cmd;

mod wsl;

pub trait WingetPackagesModule {
    fn install(
        &self,
        context: &ModuleContext<'_>,
    ) -> Result<WindowsPackageSelection, Box<dyn std::error::Error>>;
}
pub struct WindowsWingetPackages;
pub struct WindowsPackageSelection {
    pub dev_tools: bool,
}

fn confirm_package_group(name: &str, description: &str) -> io::Result<bool> {
    print!("Do you want to install {name} ({description})? (yes/no) ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "yes" | "y"))
}

impl WingetPackagesModule for WindowsWingetPackages {
    fn install(
        &self,
        context: &ModuleContext<'_>,
    ) -> Result<WindowsPackageSelection, Box<dyn std::error::Error>> {
        if context.profile != Profile::Windows || context.package_system != PackageSystem::Winget {
            return Err("WindowsWingetPackages requires the Windows profile and Winget".into());
        }
        find_program(context.shell, "winget.exe")?;
        install_packages(
            context.shell,
            PackageSystem::Winget,
            &[
                Package::SevenZip,
                Package::Git,
                Package::Powershell,
                Package::WindowsTerminal,
                Package::Wsl,
                Package::OhMyPosh,
                Package::PowerToys,
                Package::EmacsWayland,
                Package::Unzip,
                Package::Python,
            ],
        )?;

        let dev_tools = confirm_package_group("DevTools", "Rust, C/C++ and build tools")?;
        if dev_tools {
            install_packages(
                context.shell,
                PackageSystem::Winget,
                &[
                    Package::Rustup,
                    Package::Llvm,
                    Package::VisualStudioBuildTools,
                    Package::PythonInstallManager,
                    Package::DockerDesktop,
                ],
            )?;
        }
        if confirm_package_group("Art", "Blender, Krita, OBS, MuseScore and Kdenlive")? {
            install_packages(
                context.shell,
                PackageSystem::Winget,
                &[
                    Package::Blender,
                    Package::Krita,
                    Package::Kdenlive,
                    Package::Audacity,
                    Package::ObsStudio,
                    Package::Musescore,
                ],
            )?;
        }
        if confirm_package_group("Supplementary", "Handy, VirtualBox and LibreOffice")? {
            install_packages(
                context.shell,
                PackageSystem::Winget,
                &[Package::Handy, Package::VirtualBox, Package::LibreOffice],
            )?;
        }
        if confirm_package_group(
            "Arch WSL",
            "fresh Arch Linux WSL distro named after this computer; unregisters an existing distro with that name first",
        )? {
            wsl::install(context)?;
        }
        Ok(WindowsPackageSelection { dev_tools })
    }
}
pub(super) fn powershell_7(
    sh: &xshell::Shell,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    if let Ok(program) = find_program(sh, "pwsh.exe") {
        return Ok(program);
    }
    let program_files = sh
        .var_os("ProgramFiles")
        .ok_or("PowerShell 7 is not on PATH and ProgramFiles is unset")?;
    let program = Path::new(&program_files).join("PowerShell/7/pwsh.exe");
    if !program.is_file() {
        return Err("PowerShell 7 was not found after installation".into());
    }
    Ok(program)
}

fn powershell_7_profile(
    sh: &xshell::Shell,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let pwsh = powershell_7(sh)?;
    let command = "$PROFILE";
    let profile = cmd!(sh, "{pwsh} -NoProfile -Command {command}").read()?;
    if profile.is_empty() {
        return Err("PowerShell 7 did not return its profile path".into());
    }
    Ok(profile.into())
}

fn unique_windows_backup(
    sh: &xshell::Shell,
    path: &Path,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let pwsh = powershell_7(sh)?;
    let command = "Get-Date -Format yyyyMMdd_HHmmss";
    let stamp = cmd!(sh, "{pwsh} -NoProfile -Command {command}").read()?;
    let base = format!("{}.backup.{stamp}", path.display());
    let mut backup = std::path::PathBuf::from(&base);
    let mut suffix = 1;
    while fs::symlink_metadata(&backup).is_ok() {
        backup = format!("{base}.{suffix}").into();
        suffix += 1;
    }
    Ok(backup)
}

pub trait PowerShellProfileModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsPowerShellProfile;
impl PowerShellProfileModule for WindowsPowerShellProfile {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsPowerShellProfile requires Windows".into());
        }
        let sh = context.shell;
        let pwsh = powershell_7(sh)?;
        let profile = powershell_7_profile(sh)?;
        match fs::symlink_metadata(&profile) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                let command = "Select-String -LiteralPath $env:MYCONFIG_PROFILE_PATH -Pattern 'Oh My Posh' -Quiet -ErrorAction SilentlyContinue";
                let managed = cmd!(sh, "{pwsh} -NoProfile -Command {command}")
                    .env("MYCONFIG_PROFILE_PATH", &profile)
                    .read()?;
                if managed.trim() != "True" {
                    fs::copy(&profile, unique_windows_backup(sh, &profile)?)?;
                }
            }
            Ok(_) => {
                return Err(format!(
                    "PowerShell profile is not a regular file: {}",
                    profile.display()
                )
                .into());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        install_embedded_file_with_policy(
            &DOTFILES
                .assets
                .windows
                .dotfiles
                .PowerShell
                .Microsoft_PowerShell_profile_ps1,
            &profile,
            ExistingFilePolicy::Replace,
        )?;
        let command = "Unblock-File -LiteralPath $env:MYCONFIG_PROFILE_PATH";
        cmd!(sh, "{pwsh} -NoProfile -Command {command}")
            .env("MYCONFIG_PROFILE_PATH", &profile)
            .run()?;
        Ok(())
    }
}

pub trait OhMyPoshConfigModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsOhMyPoshConfig;
impl OhMyPoshConfigModule for WindowsOhMyPoshConfig {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsOhMyPoshConfig requires Windows".into());
        }
        let profile = powershell_7_profile(context.shell)?;
        let directory = profile
            .parent()
            .ok_or("PowerShell 7 profile has no parent directory")?;
        install_embedded_file_with_policy(
            &DOTFILES
                .assets
                .windows
                .dotfiles
                .PowerShell
                .black_pink_omp_json,
            &directory.join("black-pink.omp.json"),
            ExistingFilePolicy::Replace,
        )?;
        Ok(())
    }
}
pub trait WindowsTerminalConfigModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsTerminalConfig;

impl WindowsTerminalConfigModule for WindowsTerminalConfig {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsTerminalConfig requires Windows".into());
        }
        let local_app_data = context
            .shell
            .var_os("LOCALAPPDATA")
            .ok_or("LOCALAPPDATA is unset")?;
        let destination = Path::new(&local_app_data)
            .join("Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json");
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                fs::copy(
                    &destination,
                    unique_windows_backup(context.shell, &destination)?,
                )?;
            }
            Ok(_) => {
                return Err(format!(
                    "Windows Terminal settings are not a regular file: {}",
                    destination.display()
                )
                .into());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        install_embedded_file_with_policy(
            &DOTFILES
                .assets
                .windows
                .dotfiles
                .WindowsTerminal
                .settings_json,
            &destination,
            ExistingFilePolicy::Replace,
        )?;
        Ok(())
    }
}
pub trait EmacsConfigModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}

pub struct WindowsEmacsConfig;

impl EmacsConfigModule for WindowsEmacsConfig {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsEmacsConfig requires the Windows profile".into());
        }
        let emacs = find_program(context.shell, "emacs.exe")?;
        let home = emacs_home(context.shell, &emacs)?;
        let destination = home.join(".config/emacs");
        let files = DOTFILES.emacs.files();
        for file in files {
            let relative = Path::new(file.path_from_root).strip_prefix("emacs/.config/emacs")?;
            install_embedded_file_with_policy(
                file,
                &destination.join(relative),
                ExistingFilePolicy::Replace,
            )?;
        }

        let loader = home.join(".emacs");
        if let Err(error) = fs::symlink_metadata(&loader) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(error.into());
            }
            let early = destination
                .join("early-init.el")
                .to_string_lossy()
                .replace('\\', "/");
            let init = destination
                .join("init.el")
                .to_string_lossy()
                .replace('\\', "/");
            fs::write(
                loader,
                format!("(load-file \"{early}\")\n(load-file \"{init}\")\n"),
            )?;
        }
        Ok(())
    }
}
pub trait AhkScriptsModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsAhkScripts;
impl AhkScriptsModule for WindowsAhkScripts {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsAhkScripts requires Windows".into());
        }
        let source = Path::new("assets/windows/dotfiles/AutoHotkey");
        let destination = context.home.join("AutoHotkey");
        let mut copied = 0;
        for file in DOTFILES.assets.windows.dotfiles.AutoHotkey.files() {
            let relative = Path::new(file.path_from_root).strip_prefix(source)?;
            install_embedded_file_with_policy(
                file,
                &destination.join(relative),
                ExistingFilePolicy::Replace,
            )?;
            copied += 1;
        }
        if copied == 0 {
            return Err("AutoHotkey resources are missing from the binary".into());
        }
        let executable = destination.join("myconfig.exe");
        if !executable.is_file() {
            return Ok(());
        }
        let pwsh = powershell_7(context.shell)?;
        let startup = "[Environment]::GetFolderPath('Startup')";
        let startup_dir = cmd!(context.shell, "{pwsh} -NoProfile -Command {startup}").read()?;
        if startup_dir.is_empty() {
            return Err("Windows Startup directory is unavailable".into());
        }
        let shortcut = Path::new(&startup_dir).join("myconfig-autohotkey.lnk");
        let script = "$ErrorActionPreference = 'Stop'; $shell = New-Object -ComObject WScript.Shell; $link = $shell.CreateShortcut($env:MYCONFIG_AHK_SHORTCUT); $link.TargetPath = $env:MYCONFIG_AHK_EXE; $link.WorkingDirectory = $env:MYCONFIG_AHK_WORKDIR; $link.Save(); $saved = $shell.CreateShortcut($env:MYCONFIG_AHK_SHORTCUT); if ($saved.TargetPath -ine $env:MYCONFIG_AHK_EXE) { throw 'AutoHotkey shortcut target does not match the installed executable' }";
        cmd!(context.shell, "{pwsh} -NoProfile -Command {script}")
            .env("MYCONFIG_AHK_SHORTCUT", &shortcut)
            .env("MYCONFIG_AHK_EXE", executable)
            .env("MYCONFIG_AHK_WORKDIR", destination)
            .run()?;
        if !shortcut.is_file() {
            return Err("AutoHotkey startup shortcut was not created".into());
        }
        Ok(())
    }
}
pub trait AiConfigModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsAiConfig;

impl AiConfigModule for WindowsAiConfig {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsAiConfig requires Windows".into());
        }
        let home = context.home;
        let mut agents = false;
        let mut claude_doc = false;
        let mut claude_settings = false;
        let mut config_helper = false;
        let mut mcp_servers = false;
        let helper = home.join(".local/bin/claude-config-helper");
        for file in DOTFILES.ai.files() {
            let path = Path::new(file.path_from_root);
            let destination = if path == Path::new("ai/.agents/AGENTS.md") {
                agents = true;
                Some(home.join(".agents/AGENTS.md"))
            } else if let Ok(skill) = path.strip_prefix("ai/.agents/skills") {
                Some(home.join(".agents/skills").join(skill))
            } else if path == Path::new("ai/.claude/CLAUDE.md") {
                claude_doc = true;
                Some(home.join(".claude/CLAUDE.md"))
            } else if path == Path::new("ai/.claude/settings.json") {
                claude_settings = true;
                Some(home.join(".claude/settings.json"))
            } else if path == Path::new("ai/.local/bin/claude-config-helper") {
                config_helper = true;
                Some(helper.clone())
            } else if path == Path::new("ai/.config/claude-config-helper/mcp-servers.json") {
                mcp_servers = true;
                Some(home.join(".config/claude-config-helper/mcp-servers.json"))
            } else {
                None
            };
            if let Some(destination) = destination {
                install_embedded_file_with_policy(file, &destination, ExistingFilePolicy::Replace)?;
                if let Ok(skill) = path.strip_prefix("ai/.agents/skills") {
                    let claude_skill = home.join(".claude/skills").join(skill);
                    install_embedded_file_with_policy(
                        file,
                        &claude_skill,
                        ExistingFilePolicy::Replace,
                    )?;
                }
            }
        }
        if !(agents && claude_doc && claude_settings && config_helper && mcp_servers) {
            return Err("embedded agent or Claude configuration is incomplete".into());
        }
        let sh = context.shell;
        if find_program(sh, "py").is_ok() && find_program(sh, "claude").is_ok() {
            if let Err(error) = cmd!(sh, "py -3 {helper} mcp apply").run() {
                eprintln!("Claude MCP servers were not applied: {error}");
            }
        } else {
            eprintln!("Python or Claude Code not found; run claude-config-helper mcp apply later");
        }
        Ok(())
    }
}
pub trait PsReadLineModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsPsReadLine;
impl PsReadLineModule for WindowsPsReadLine {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsPsReadLine requires Windows".into());
        }
        let sh = context.shell;
        let pwsh = powershell_7(sh)?;
        let check = "(Get-Module -ListAvailable -Name PSReadLine | Measure-Object).Count";
        let present = cmd!(sh, "{pwsh} -NoProfile -Command {check}").read()?;
        if present.trim().parse::<u32>()? == 0 {
            let script = "$ErrorActionPreference = 'Stop'; Install-Module -Name PSReadLine -AllowPrerelease -Force -Scope CurrentUser; if (-not (Get-Module -ListAvailable -Name PSReadLine)) { throw 'PSReadLine was not installed' }";
            cmd!(sh, "{pwsh} -NoProfile -Command {script}").run()?;
        }
        Ok(())
    }
}

pub trait IosevkaMonoFontModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsIosevkaMonoFont;
impl IosevkaMonoFontModule for WindowsIosevkaMonoFont {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsIosevkaMonoFont requires Windows".into());
        }
        if let Ok(program) = find_program(context.shell, "oh-my-posh.exe") {
            cmd!(context.shell, "{program} font install Iosevka").run()?;
        }
        Ok(())
    }
}
pub trait LlvmPathModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsLlvmPath;

impl LlvmPathModule for WindowsLlvmPath {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsLlvmPath requires Windows".into());
        }
        let sh = context.shell;
        let program_files = sh.var_os("ProgramFiles").ok_or("ProgramFiles is unset")?;
        let llvm = Path::new(&program_files).join("LLVM/bin");
        if !llvm.is_dir() {
            eprintln!(
                "LLVM bin directory not found; skipping machine PATH setup: {}",
                llvm.display()
            );
            return Ok(());
        }
        let llvm = llvm.to_str().ok_or("LLVM path is not UTF-8")?;
        let pwsh = powershell_7(sh)?;
        let read_path = "[Environment]::GetEnvironmentVariable('Path', 'Machine')";
        let current = cmd!(sh, "{pwsh} -NoProfile -Command {read_path}").read()?;
        let contains = |path: &str| {
            path.split(';').any(|entry| {
                entry
                    .trim_end_matches(['\\', '/'])
                    .eq_ignore_ascii_case(llvm.trim_end_matches(['\\', '/']))
            })
        };
        if contains(&current) {
            return Ok(());
        }

        let quoted_llvm = llvm.replace('\'', "''");
        let set_path = format!(
            "$llvm = '{quoted_llvm}'; $current = [Environment]::GetEnvironmentVariable('Path', 'Machine'); \
             $entries = @($current -split ';' | Where-Object {{ $_ }}); \
             if (-not ($entries | Where-Object {{ $_.TrimEnd('\\') -ieq $llvm.TrimEnd('\\') }})) {{ \
             [Environment]::SetEnvironmentVariable('Path', (($entries + $llvm) -join ';'), 'Machine') }}"
        );
        let elevate = "$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($env:MYCONFIG_LLVM_SET_PATH)); $process = Start-Process -FilePath $env:MYCONFIG_PWSH -Verb RunAs -ArgumentList '-NoProfile', '-EncodedCommand', $encoded -Wait -PassThru -ErrorAction Stop; if ($process.ExitCode -ne 0) { throw 'Elevated LLVM PATH update failed' }";
        cmd!(sh, "{pwsh} -NoProfile -Command {elevate}")
            .env("MYCONFIG_LLVM_SET_PATH", set_path)
            .env("MYCONFIG_PWSH", &pwsh)
            .run()?;
        let updated = cmd!(sh, "{pwsh} -NoProfile -Command {read_path}").read()?;
        if !contains(&updated) {
            return Err("LLVM was not added to the machine PATH".into());
        }
        Ok(())
    }
}
pub trait RegistryTweaksModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsRegistryTweaks;
impl RegistryTweaksModule for WindowsRegistryTweaks {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsRegistryTweaks requires Windows".into());
        }
        let temporary = std::env::temp_dir().join(format!(
            "myconfig-registry-{}-{}.reg",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let contents = DOTFILES.assets.windows.RegistryPreferences_reg.content;
        use io::Write as _;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        let result = cmd!(context.shell, "reg.exe import {temporary}").run();
        match (result, fs::remove_file(&temporary)) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error.into()),
            (Ok(()), Err(error)) => Err(error.into()),
            (Err(error), Err(cleanup)) => Err(format!(
                "registry import failed: {error}; could not remove {}: {cleanup}",
                temporary.display()
            )
            .into()),
        }
    }
}
pub trait TaskbarAutoHideModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsTaskbarAutoHide;

#[cfg(windows)]
fn enable_taskbar_auto_hide() -> ModuleResult {
    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct AppBarData {
        cb_size: u32,
        window: *mut std::ffi::c_void,
        callback_message: u32,
        edge: u32,
        rect: Rect,
        state: isize,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class_name: *const u16, window_name: *const u16) -> *mut std::ffi::c_void;
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHAppBarMessage(message: u32, data: *mut AppBarData) -> usize;
    }
    let class: Vec<u16> = "Shell_TrayWnd"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // The pointers and struct layout are the Win32 APPBARDATA interface.
    let window = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    if window.is_null() {
        return Err("Windows taskbar was not found".into());
    }
    let mut data = AppBarData {
        cb_size: std::mem::size_of::<AppBarData>() as u32,
        window,
        callback_message: 0,
        edge: 0,
        rect: Rect::default(),
        state: 0x1 | 0x2,
    };
    unsafe {
        SHAppBarMessage(0xA, &mut data);
    }
    let state = unsafe { SHAppBarMessage(0x4, &mut data) };
    if state & 0x3 != 0x3 {
        return Err("Windows taskbar auto-hide could not be verified".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn enable_taskbar_auto_hide() -> ModuleResult {
    Err("the taskbar is available only on Windows".into())
}

impl TaskbarAutoHideModule for WindowsTaskbarAutoHide {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows {
            return Err("WindowsTaskbarAutoHide requires Windows".into());
        }
        enable_taskbar_auto_hide()
    }
}
pub trait SharedDesktopModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct WindowsSharedDesktop;

fn is_windows_administrator(sh: &xshell::Shell) -> Result<bool, Box<dyn std::error::Error>> {
    let pwsh = powershell_7(sh)?;
    let command = "$identity = [Security.Principal.WindowsIdentity]::GetCurrent(); $principal = [Security.Principal.WindowsPrincipal]::new($identity); $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)";
    Ok(cmd!(sh, "{pwsh} -NoProfile -Command {command}")
        .read()?
        .trim()
        == "True")
}

fn elevated_desktop_move(sh: &xshell::Shell) -> ModuleResult {
    let pwsh = powershell_7(sh)?;
    let binary = std::env::current_exe()?;
    let command = "$process = Start-Process -FilePath $env:MYCONFIG_DESKTOP_EXE -Verb RunAs -ArgumentList '--move-shared-desktop' -Wait -PassThru -ErrorAction Stop; if ($process.ExitCode -ne 0) { throw 'Moving shared desktop items with administrator privileges failed' }";
    cmd!(sh, "{pwsh} -NoProfile -Command {command}")
        .env("MYCONFIG_DESKTOP_EXE", binary)
        .run()?;
    Ok(())
}

fn confirm_desktop_replacement(source: &Path, target: &Path) -> io::Result<bool> {
    print!(
        "The shared desktop item {} would replace and delete {}. Type 'yes' to replace it: ",
        source.display(),
        target.display()
    );
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim() == "yes")
}

fn remove_desktop_item(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn copy_desktop_item(source: &Path, target: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        let destination = fs::read_link(source)?;
        #[cfg(windows)]
        {
            if fs::metadata(source)?.is_dir() {
                std::os::windows::fs::symlink_dir(destination, target)?;
            } else {
                std::os::windows::fs::symlink_file(destination, target)?;
            }
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(destination, target)?;
    } else if metadata.is_dir() {
        fs::create_dir(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_desktop_item(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        fs::copy(source, target)?;
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported desktop item: {}", source.display()),
        ));
    }
    Ok(())
}

fn move_desktop_item(source: &Path, target: &Path, replace: bool) -> ModuleResult {
    let backup = if replace {
        let base = format!(
            "{}.myconfig-pending-{}",
            target.display(),
            std::process::id()
        );
        let mut path = std::path::PathBuf::from(&base);
        let mut suffix = 1;
        while fs::symlink_metadata(&path).is_ok() {
            path = format!("{base}.{suffix}").into();
            suffix += 1;
        }
        fs::rename(target, &path)?;
        Some(path)
    } else {
        None
    };

    let move_result = match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            let copied = copy_desktop_item(source, target);
            if copied.is_ok() {
                remove_desktop_item(source)
            } else {
                copied
            }
        }
        Err(error) => Err(error),
    };
    if let Err(error) = move_result {
        if fs::symlink_metadata(target).is_ok() {
            remove_desktop_item(target)?;
        }
        if let Some(previous) = backup {
            fs::rename(previous, target)?;
        }
        return Err(error.into());
    }
    if let Some(previous) = backup {
        remove_desktop_item(&previous)?;
    }
    Ok(())
}

impl SharedDesktopModule for WindowsSharedDesktop {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Windows || !cfg!(windows) {
            return Err("WindowsSharedDesktop requires Windows".into());
        }
        let sh = context.shell;
        if !is_windows_administrator(sh)? {
            return elevated_desktop_move(sh);
        }
        let pwsh = powershell_7(sh)?;
        let desktop_command = "[Environment]::GetFolderPath('Desktop')";
        let desktop = cmd!(sh, "{pwsh} -NoProfile -Command {desktop_command}").read()?;
        if desktop.is_empty() {
            return Err("the Windows desktop directory was not found".into());
        }
        let destination = Path::new(&desktop);
        let public = sh.var_os("PUBLIC").ok_or("PUBLIC is unset")?;
        let system_drive = sh.var_os("SystemDrive").ok_or("SystemDrive is unset")?;
        let default_desktop = format!(
            "{}\\Users\\Default\\Desktop",
            system_drive.to_string_lossy()
        );
        let sources = [Path::new(&public).join("Desktop"), default_desktop.into()];
        for source in &sources {
            if !source.is_dir() {
                continue;
            }
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                let name = entry.file_name();
                if name.to_string_lossy().eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }
                let from = entry.path();
                let target = destination.join(&name);
                let replace = match fs::symlink_metadata(&target) {
                    Ok(_) => {
                        if !confirm_desktop_replacement(&from, &target)? {
                            continue;
                        }
                        true
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => false,
                    Err(error) => return Err(error.into()),
                };
                move_desktop_item(&from, &target, replace)?;
            }
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use myconfig_utils::PackageSystem;
    use std::os::unix::fs::PermissionsExt;
    use xshell::Shell;

    #[test]
    fn emacs_configuration_uses_emacs_reported_home_and_preserves_existing_loader() {
        let home =
            std::env::temp_dir().join(format!("myconfig-windows-emacs-{}", std::process::id()));
        fs::create_dir_all(&home).unwrap();
        let program = home.join("emacs.exe");
        fs::write(
            &program,
            "#!/bin/sh\nprintf '%s\\n' \"$MYCONFIG_EMACS_HOME\"\n",
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let sh = Shell::new().unwrap();
        sh.set_var("PATH", &home);
        sh.set_var("MYCONFIG_EMACS_HOME", &home);
        let context = ModuleContext {
            profile: Profile::Windows,
            package_system: PackageSystem::Winget,
            shell: &sh,
            home: &home,
        };

        WindowsEmacsConfig.install(&context).unwrap();
        assert_eq!(
            fs::read(home.join(".config/emacs/init.el")).unwrap(),
            DOTFILES.emacs._config.emacs.init_el.content
        );
        assert!(
            fs::read_to_string(home.join(".emacs"))
                .unwrap()
                .contains(".config/emacs/init.el")
        );
        fs::write(home.join(".emacs"), "user loader\n").unwrap();
        WindowsEmacsConfig.install(&context).unwrap();
        assert_eq!(
            fs::read_to_string(home.join(".emacs")).unwrap(),
            "user loader\n"
        );
        fs::remove_dir_all(home).unwrap();
    }
}

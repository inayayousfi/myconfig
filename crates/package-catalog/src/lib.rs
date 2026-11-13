#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[repr(usize)]
pub enum Package {
    CaCertificates,
    Sudo,
    Git,
    Curl,
    Lsof,
    Wget,
    Rsync,
    Stow,
    Tar,
    Unzip,
    Zip,
    Xz,
    File,
    ManDb,
    ManPages,
    BaseDevel,
    Rustup,
    Openssh,
    Polkit,
    CachyUpdate,
    CachyosHello,
    CachyosKernelManager,
    LinuxCachyos,
    CachyosZshConfig,
    Firefox,
    FirefoxI18nFr,
    MesloFont,
    NotoFontsCjk,
    CachyosEmeraldKdeTheme,
    CachyosIridescentKde,
    CachyosNordKdeTheme,
    Kate,
    Micro,
    CachyosMicroSettings,
    Nano,
    NanoSyntaxHighlighting,
    Meld,
    Glances,
    Duf,
    Tealdeer,
    Filelight,
    Pavucontrol,
    Kcalc,
    Shelly,
    CachyosPackageinstaller,
    Expac,
    CachyosWallpapers,
    Hwdetect,
    Qtscrcpy,
    Ripgrep,
    Fd,
    Fzf,
    Zoxide,
    Eza,
    Bat,
    Jq,
    Fastfetch,
    Btop,
    Tokei,
    GithubCli,
    Hunk,
    Go,
    Bun,
    Python,
    Jdk,
    Maven,
    Llvm,
    Make,
    Cmake,
    Nodejs,
    Npm,
    NodeGyp,
    Zsh,
    Fish,
    CachyosFishConfig,
    FishAutopair,
    FishPurePrompt,
    Fisher,
    Neovim,
    Vim,
    EmacsWayland,
    Ufw,
    Sshfs,
    Ghostty,
    Kitty,
    Alacritty,
    Wezterm,
    Konsole,
    KdeUtilitiesMeta,
    Yazi,
    Ffmpeg,
    SevenZip,
    Poppler,
    Resvg,
    Imagemagick,
    Lazygit,
    Tmux,
    WlClipboard,
    Ydotool,
    Kanata,
    Handy,
    AtSpi2Core,
    Libxcomposite,
    Libxdamage,
    Libxrandr,
    Libxkbcommon,
    Pyside6,
    Qt6Wayland,
    LayerShellQt,
    Libinput,
    Systemd,
    IosevkaFont,
    DesktopFileUtils,
    WslSshAgent,
    Tailscale,
    AndroidSdkPlatformTools,
    Scrcpy,
    Docker,
    DockerBuildx,
    DockerCompose,
    Powershell,
    WindowsTerminal,
    Wsl,
    OhMyPosh,
    PowerToys,
    Blender,
    Krita,
    Kdenlive,
    Audacity,
    ObsStudio,
    Musescore,
    VisualStudioBuildTools,
    PythonInstallManager,
    DockerDesktop,
    VirtualBox,
    LibreOffice,
    /// In the CachyOS repository; plain Arch gets `paru-bin` from the AUR instead.
    Paru,
    PlexFonts,
    BlexMonoFont,
    Haruna,
    Mpvqt,
    Mpv,
    Kdeconnect,
    Partitionmanager,
    Ark,
    Gwenview,
    Plymouth,
    PlymouthKcm,
    CachyosPlymouthTheme,
    CachyosPlymouthBootanimation,
    Vlc,
    Loupe,
    Flatpak,
    Fuse2,
    PythonVenv,
    Vscodium,
    Bsdtar,
    DockerRootlessExtras,
    Slirp4netns,
    Uidmap,
    Qrencode,
    T3codeDesktop,
    NodejsLts,
}

impl Package {
    const COUNT: usize = 163;
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(usize)]
pub enum Platform {
    Pacman,
    Aur,
    Apt,
    Winget,
}

impl Platform {
    const COUNT: usize = 4;
}

/// A vendor APT repository, for packages that Ubuntu does not carry or carries only in
/// old versions. An APT name written `id:package` comes from the repository with that
/// `id`. `{codename}` in `key_url` and `suite` becomes the release code name of this
/// Ubuntu.
#[derive(Debug, Eq, PartialEq)]
pub struct AptRepository {
    pub id: &'static str,
    /// The signing key, binary or ASCII-armored.
    pub key_url: &'static str,
    pub uri: &'static str,
    pub suite: &'static str,
    pub component: &'static str,
}

pub const APT_REPOSITORIES: &[AptRepository] = &[
    AptRepository {
        id: "tailscale",
        key_url: "https://pkgs.tailscale.com/stable/ubuntu/{codename}.noarmor.gpg",
        uri: "https://pkgs.tailscale.com/stable/ubuntu",
        suite: "{codename}",
        component: "main",
    },
    AptRepository {
        id: "github",
        key_url: "https://cli.github.com/packages/githubcli-archive-keyring.gpg",
        uri: "https://cli.github.com/packages",
        suite: "stable",
        component: "main",
    },
    AptRepository {
        id: "docker",
        key_url: "https://download.docker.com/linux/ubuntu/gpg",
        uri: "https://download.docker.com/linux/ubuntu",
        suite: "{codename}",
        component: "stable",
    },
    AptRepository {
        id: "yazi",
        key_url: "https://yazi-rs.github.io/builds/yazi-keyring.gpg",
        uri: "https://yazi-rs.github.io/builds/",
        suite: "stable",
        component: "main",
    },
];

/// Splits an APT name written `id:package` into its vendor repository and package name.
/// A name without `id:` comes from Ubuntu itself. Returns `None` when `id` names no
/// repository in `APT_REPOSITORIES`.
pub fn apt_source(
    identifier: &'static str,
) -> Option<(Option<&'static AptRepository>, &'static str)> {
    let Some((id, name)) = identifier.split_once(':') else {
        return Some((None, identifier));
    };
    let repository = APT_REPOSITORIES
        .iter()
        .find(|repository| repository.id == id)?;
    Some((Some(repository), name))
}

const PACKAGE_OVERRIDES: &[((Package, Platform), &str)] = &[
    ((Package::CaCertificates, Platform::Apt), "ca-certificates"),
    ((Package::Curl, Platform::Apt), "curl"),
    ((Package::Git, Platform::Apt), "git"),
    ((Package::Rsync, Platform::Apt), "rsync"),
    ((Package::Stow, Platform::Apt), "stow"),
    ((Package::Zsh, Platform::Apt), "zsh"),
    (
        (Package::CaCertificates, Platform::Pacman),
        "ca-certificates",
    ),
    (
        (Package::CachyosKernelManager, Platform::Pacman),
        "cachyos-kernel-manager",
    ),
    (
        (Package::CachyosZshConfig, Platform::Pacman),
        "cachyos-zsh-config",
    ),
    ((Package::Vim, Platform::Pacman), "vim"),
    (
        (Package::CachyosFishConfig, Platform::Pacman),
        "cachyos-fish-config",
    ),
    (
        (Package::FishPurePrompt, Platform::Pacman),
        "fish-pure-prompt",
    ),
    (
        (Package::AndroidSdkPlatformTools, Platform::Pacman),
        "android-sdk-platform-tools",
    ),
    (
        (Package::Powershell, Platform::Winget),
        "Microsoft.PowerShell",
    ),
    (
        (Package::WindowsTerminal, Platform::Winget),
        "Microsoft.WindowsTerminal",
    ),
    (
        (Package::OhMyPosh, Platform::Winget),
        "JanDeDobbeleer.OhMyPosh",
    ),
    (
        (Package::PowerToys, Platform::Winget),
        "Microsoft.PowerToys",
    ),
    (
        (Package::VisualStudioBuildTools, Platform::Winget),
        "Microsoft.VisualStudio.2022.BuildTools",
    ),
    (
        (Package::PythonInstallManager, Platform::Winget),
        "Python.PythonInstallManager",
    ),
    (
        (Package::DockerDesktop, Platform::Winget),
        "Docker.DockerDesktop",
    ),
    (
        (Package::Blender, Platform::Winget),
        "BlenderFoundation.Blender",
    ),
    (
        (Package::ObsStudio, Platform::Winget),
        "OBSProject.OBSStudio",
    ),
    (
        (Package::Musescore, Platform::Winget),
        "Musescore.Musescore",
    ),
    (
        (Package::LibreOffice, Platform::Winget),
        "TheDocumentFoundation.LibreOffice",
    ),
    ((Package::Sudo, Platform::Pacman), "sudo"),
    ((Package::Git, Platform::Pacman), "git"),
    ((Package::Git, Platform::Winget), "Git.Git"),
    ((Package::Blender, Platform::Pacman), "blender"),
    ((Package::Blender, Platform::Apt), "blender"),
    ((Package::Krita, Platform::Pacman), "krita"),
    ((Package::Krita, Platform::Apt), "krita"),
    ((Package::Krita, Platform::Winget), "KDE.Krita"),
    ((Package::Kdenlive, Platform::Pacman), "kdenlive"),
    ((Package::Kdenlive, Platform::Apt), "kdenlive"),
    ((Package::Kdenlive, Platform::Winget), "KDE.Kdenlive"),
    ((Package::Audacity, Platform::Pacman), "audacity"),
    ((Package::Audacity, Platform::Apt), "audacity"),
    ((Package::Audacity, Platform::Winget), "Audacity.Audacity"),
    ((Package::ObsStudio, Platform::Pacman), "obs-studio"),
    ((Package::ObsStudio, Platform::Apt), "obs-studio"),
    ((Package::Rustup, Platform::Winget), "Rustlang.Rustup"),
    ((Package::Llvm, Platform::Winget), "LLVM.LLVM"),
    ((Package::VirtualBox, Platform::Winget), "Oracle.VirtualBox"),
    ((Package::Curl, Platform::Pacman), "curl"),
    ((Package::Lsof, Platform::Pacman), "lsof"),
    ((Package::Wget, Platform::Pacman), "wget"),
    ((Package::Rsync, Platform::Pacman), "rsync"),
    ((Package::Stow, Platform::Pacman), "stow"),
    ((Package::Tar, Platform::Pacman), "tar"),
    ((Package::Unzip, Platform::Pacman), "unzip"),
    ((Package::Unzip, Platform::Winget), "GnuWin32.UnZip"),
    ((Package::Zip, Platform::Pacman), "zip"),
    ((Package::Xz, Platform::Pacman), "xz"),
    ((Package::File, Platform::Pacman), "file"),
    ((Package::ManDb, Platform::Pacman), "man-db"),
    ((Package::ManPages, Platform::Pacman), "man-pages"),
    ((Package::BaseDevel, Platform::Pacman), "base-devel"),
    ((Package::Rustup, Platform::Pacman), "rustup"),
    ((Package::Openssh, Platform::Pacman), "openssh"),
    ((Package::Polkit, Platform::Pacman), "polkit"),
    ((Package::CachyUpdate, Platform::Pacman), "cachy-update"),
    ((Package::CachyosHello, Platform::Pacman), "cachyos-hello"),
    ((Package::LinuxCachyos, Platform::Pacman), "linux-cachyos"),
    ((Package::Firefox, Platform::Pacman), "firefox"),
    (
        (Package::FirefoxI18nFr, Platform::Pacman),
        "firefox-i18n-fr",
    ),
    ((Package::MesloFont, Platform::Pacman), "ttf-meslo-nerd"),
    ((Package::NotoFontsCjk, Platform::Pacman), "noto-fonts-cjk"),
    (
        (Package::CachyosEmeraldKdeTheme, Platform::Pacman),
        "cachyos-emerald-kde-theme-git",
    ),
    (
        (Package::CachyosIridescentKde, Platform::Pacman),
        "cachyos-iridescent-kde",
    ),
    (
        (Package::CachyosNordKdeTheme, Platform::Pacman),
        "cachyos-nord-kde-theme-git",
    ),
    ((Package::Kate, Platform::Pacman), "kate"),
    ((Package::Micro, Platform::Pacman), "micro"),
    (
        (Package::CachyosMicroSettings, Platform::Pacman),
        "cachyos-micro-settings",
    ),
    ((Package::Nano, Platform::Pacman), "nano"),
    (
        (Package::NanoSyntaxHighlighting, Platform::Pacman),
        "nano-syntax-highlighting",
    ),
    ((Package::Meld, Platform::Pacman), "meld"),
    ((Package::Glances, Platform::Pacman), "glances"),
    ((Package::Duf, Platform::Pacman), "duf"),
    ((Package::Tealdeer, Platform::Pacman), "tealdeer"),
    ((Package::Filelight, Platform::Pacman), "filelight"),
    ((Package::Pavucontrol, Platform::Pacman), "pavucontrol"),
    ((Package::Kcalc, Platform::Pacman), "kcalc"),
    ((Package::Shelly, Platform::Pacman), "shelly"),
    (
        (Package::CachyosPackageinstaller, Platform::Pacman),
        "cachyos-packageinstaller",
    ),
    ((Package::Expac, Platform::Pacman), "expac"),
    (
        (Package::CachyosWallpapers, Platform::Pacman),
        "cachyos-wallpapers",
    ),
    ((Package::Hwdetect, Platform::Pacman), "hwdetect"),
    ((Package::Qtscrcpy, Platform::Pacman), "qtscrcpy"),
    ((Package::Ripgrep, Platform::Pacman), "ripgrep"),
    ((Package::Fd, Platform::Pacman), "fd"),
    ((Package::Fd, Platform::Apt), "fd-find"),
    ((Package::Fzf, Platform::Pacman), "fzf"),
    ((Package::Zoxide, Platform::Pacman), "zoxide"),
    ((Package::Eza, Platform::Pacman), "eza"),
    ((Package::Bat, Platform::Pacman), "bat"),
    ((Package::Jq, Platform::Pacman), "jq"),
    ((Package::Fastfetch, Platform::Pacman), "fastfetch"),
    ((Package::Btop, Platform::Pacman), "btop"),
    ((Package::Tokei, Platform::Pacman), "tokei"),
    ((Package::GithubCli, Platform::Aur), "github-cli-git"),
    ((Package::Hunk, Platform::Aur), "hunk-bin"),
    ((Package::Go, Platform::Pacman), "go"),
    ((Package::Bun, Platform::Pacman), "bun"),
    ((Package::Python, Platform::Pacman), "python"),
    ((Package::Python, Platform::Winget), "Python.Python.3.13"),
    ((Package::Jdk, Platform::Pacman), "jdk-openjdk"),
    ((Package::Maven, Platform::Pacman), "maven"),
    ((Package::Llvm, Platform::Pacman), "llvm"),
    ((Package::Make, Platform::Pacman), "make"),
    ((Package::Cmake, Platform::Pacman), "cmake"),
    ((Package::Nodejs, Platform::Pacman), "nodejs"),
    ((Package::Npm, Platform::Pacman), "npm"),
    ((Package::NodeGyp, Platform::Pacman), "node-gyp"),
    ((Package::Zsh, Platform::Pacman), "zsh"),
    ((Package::Fish, Platform::Pacman), "fish"),
    ((Package::FishAutopair, Platform::Pacman), "fish-autopair"),
    ((Package::Fisher, Platform::Pacman), "fisher"),
    ((Package::Neovim, Platform::Pacman), "neovim"),
    ((Package::EmacsWayland, Platform::Pacman), "emacs-wayland"),
    ((Package::EmacsWayland, Platform::Winget), "GNU.Emacs"),
    ((Package::Ufw, Platform::Pacman), "ufw"),
    ((Package::Sshfs, Platform::Pacman), "sshfs"),
    ((Package::Ghostty, Platform::Pacman), "ghostty"),
    ((Package::Kitty, Platform::Pacman), "kitty"),
    ((Package::Alacritty, Platform::Pacman), "alacritty"),
    ((Package::Wezterm, Platform::Pacman), "wezterm"),
    ((Package::Konsole, Platform::Pacman), "konsole"),
    (
        (Package::KdeUtilitiesMeta, Platform::Pacman),
        "kde-utilities-meta",
    ),
    ((Package::Yazi, Platform::Aur), "yazi-git"),
    ((Package::Ffmpeg, Platform::Pacman), "ffmpeg"),
    ((Package::SevenZip, Platform::Pacman), "7zip"),
    ((Package::SevenZip, Platform::Winget), "7zip.7zip"),
    ((Package::Poppler, Platform::Pacman), "poppler"),
    ((Package::Resvg, Platform::Pacman), "resvg"),
    ((Package::Imagemagick, Platform::Pacman), "imagemagick"),
    ((Package::Lazygit, Platform::Pacman), "lazygit"),
    ((Package::Tmux, Platform::Pacman), "tmux"),
    ((Package::WlClipboard, Platform::Pacman), "wl-clipboard"),
    ((Package::Ydotool, Platform::Pacman), "ydotool"),
    ((Package::Kanata, Platform::Aur), "kanata-bin"),
    ((Package::Handy, Platform::Aur), "handy-bin"),
    ((Package::AtSpi2Core, Platform::Pacman), "at-spi2-core"),
    ((Package::Libxcomposite, Platform::Pacman), "libxcomposite"),
    ((Package::Libxdamage, Platform::Pacman), "libxdamage"),
    ((Package::Libxrandr, Platform::Pacman), "libxrandr"),
    ((Package::Libxkbcommon, Platform::Pacman), "libxkbcommon"),
    ((Package::Pyside6, Platform::Pacman), "pyside6"),
    ((Package::Qt6Wayland, Platform::Pacman), "qt6-wayland"),
    ((Package::LayerShellQt, Platform::Pacman), "layer-shell-qt"),
    ((Package::Libinput, Platform::Pacman), "libinput"),
    ((Package::Systemd, Platform::Pacman), "systemd"),
    ((Package::IosevkaFont, Platform::Pacman), "ttf-iosevka-nerd"),
    ((Package::WslSshAgent, Platform::Aur), "wsl2-ssh-agent"),
    ((Package::Tailscale, Platform::Pacman), "tailscale"),
    ((Package::Scrcpy, Platform::Pacman), "scrcpy"),
    ((Package::Docker, Platform::Pacman), "docker"),
    ((Package::DockerBuildx, Platform::Pacman), "docker-buildx"),
    ((Package::DockerCompose, Platform::Pacman), "docker-compose"),
    ((Package::Wsl, Platform::Winget), "Microsoft.WSL"),
    ((Package::Handy, Platform::Winget), "cjpais.Handy"),
    ((Package::Paru, Platform::Pacman), "paru"),
    (
        (Package::DesktopFileUtils, Platform::Pacman),
        "desktop-file-utils",
    ),
    ((Package::PlexFonts, Platform::Aur), "otf-ibm-plex"),
    (
        (Package::BlexMonoFont, Platform::Pacman),
        "ttf-ibmplex-mono-nerd",
    ),
    ((Package::Haruna, Platform::Pacman), "haruna"),
    ((Package::Mpvqt, Platform::Pacman), "mpvqt"),
    ((Package::Mpv, Platform::Pacman), "mpv"),
    ((Package::Kdeconnect, Platform::Pacman), "kdeconnect"),
    (
        (Package::Partitionmanager, Platform::Pacman),
        "partitionmanager",
    ),
    ((Package::Ark, Platform::Pacman), "ark"),
    ((Package::Gwenview, Platform::Pacman), "gwenview"),
    ((Package::Plymouth, Platform::Pacman), "plymouth"),
    ((Package::PlymouthKcm, Platform::Pacman), "plymouth-kcm"),
    (
        (Package::CachyosPlymouthTheme, Platform::Pacman),
        "cachyos-plymouth-theme",
    ),
    (
        (Package::CachyosPlymouthBootanimation, Platform::Pacman),
        "cachyos-plymouth-bootanimation",
    ),
    ((Package::Vlc, Platform::Pacman), "vlc"),
    ((Package::Loupe, Platform::Pacman), "loupe"),
    ((Package::Flatpak, Platform::Pacman), "flatpak"),
    ((Package::Fuse2, Platform::Pacman), "fuse2"),
    ((Package::PythonVenv, Platform::Pacman), "python"),
    ((Package::PythonVenv, Platform::Apt), "python3-venv"),
    ((Package::Vscodium, Platform::Pacman), "vscodium"),
    ((Package::Bsdtar, Platform::Pacman), "libarchive"),
    (
        (Package::DockerRootlessExtras, Platform::Aur),
        "docker-rootless-extras",
    ),
    (
        (Package::DockerRootlessExtras, Platform::Apt),
        "docker:docker-ce-rootless-extras",
    ),
    ((Package::Slirp4netns, Platform::Pacman), "slirp4netns"),
    ((Package::Slirp4netns, Platform::Apt), "slirp4netns"),
    ((Package::Uidmap, Platform::Pacman), "shadow"),
    ((Package::Uidmap, Platform::Apt), "uidmap"),
    ((Package::Qrencode, Platform::Pacman), "qrencode"),
    ((Package::Qrencode, Platform::Apt), "qrencode"),
    ((Package::T3codeDesktop, Platform::Aur), "t3code-bin"),
    ((Package::Sudo, Platform::Apt), "sudo"),
    ((Package::Wget, Platform::Apt), "wget"),
    ((Package::Tar, Platform::Apt), "tar"),
    ((Package::Unzip, Platform::Apt), "unzip"),
    ((Package::Zip, Platform::Apt), "zip"),
    ((Package::Xz, Platform::Apt), "xz-utils"),
    ((Package::File, Platform::Apt), "file"),
    ((Package::ManDb, Platform::Apt), "man-db"),
    ((Package::ManPages, Platform::Apt), "manpages"),
    ((Package::BaseDevel, Platform::Apt), "build-essential"),
    ((Package::Rustup, Platform::Apt), "rustup"),
    ((Package::Polkit, Platform::Apt), "polkitd"),
    ((Package::Ripgrep, Platform::Apt), "ripgrep"),
    ((Package::Jq, Platform::Apt), "jq"),
    ((Package::Fastfetch, Platform::Apt), "fastfetch"),
    ((Package::Tokei, Platform::Apt), "tokei"),
    ((Package::GithubCli, Platform::Apt), "github:gh"),
    ((Package::Btop, Platform::Apt), "btop"),
    ((Package::Go, Platform::Apt), "golang-go"),
    ((Package::Python, Platform::Apt), "python3"),
    ((Package::Jdk, Platform::Apt), "default-jdk"),
    ((Package::Maven, Platform::Apt), "maven"),
    ((Package::Llvm, Platform::Apt), "llvm"),
    ((Package::Make, Platform::Apt), "make"),
    ((Package::Cmake, Platform::Apt), "cmake"),
    ((Package::Nodejs, Platform::Apt), "nodejs"),
    ((Package::Npm, Platform::Apt), "npm"),
    ((Package::NodeGyp, Platform::Apt), "node-gyp"),
    ((Package::Yazi, Platform::Apt), "yazi:yazi"),
    ((Package::Ffmpeg, Platform::Apt), "ffmpeg"),
    ((Package::SevenZip, Platform::Apt), "7zip"),
    ((Package::Poppler, Platform::Apt), "poppler-utils"),
    ((Package::Resvg, Platform::Apt), "resvg"),
    ((Package::Imagemagick, Platform::Apt), "imagemagick"),
    ((Package::Tailscale, Platform::Apt), "tailscale:tailscale"),
    ((Package::Lsof, Platform::Apt), "lsof"),
    ((Package::AtSpi2Core, Platform::Apt), "at-spi2-core"),
    ((Package::Libxcomposite, Platform::Apt), "libxcomposite1"),
    ((Package::Libxdamage, Platform::Apt), "libxdamage1"),
    ((Package::Libxrandr, Platform::Apt), "libxrandr2"),
    ((Package::Libxkbcommon, Platform::Apt), "libxkbcommon0"),
    ((Package::Docker, Platform::Apt), "docker:docker-ce"),
    (
        (Package::DockerBuildx, Platform::Apt),
        "docker:docker-buildx-plugin",
    ),
    (
        (Package::DockerCompose, Platform::Apt),
        "docker:docker-compose-plugin",
    ),
    ((Package::Vscodium, Platform::Winget), "VSCodium.VSCodium"),
    (
        (Package::Tailscale, Platform::Winget),
        "Tailscale.Tailscale",
    ),
    ((Package::NodejsLts, Platform::Winget), "OpenJS.NodeJS.LTS"),
];

const fn build_packages()
-> [((Package, Platform), Option<&'static str>); Package::COUNT * Platform::COUNT] {
    let mut packages =
        [((Package::CaCertificates, Platform::Pacman), None); Package::COUNT * Platform::COUNT];
    let mut package_index = 0;
    while package_index < Package::COUNT {
        let mut platform_index = 0;
        while platform_index < Platform::COUNT {
            let index = package_index * Platform::COUNT + platform_index;
            packages[index] = (
                (
                    package_from_index(package_index),
                    platform_from_index(platform_index),
                ),
                None,
            );
            platform_index += 1;
        }
        package_index += 1;
    }

    let mut override_index = 0;
    while override_index < PACKAGE_OVERRIDES.len() {
        let ((package, platform), identifier) = PACKAGE_OVERRIDES[override_index];
        let index = package as usize * Platform::COUNT + platform as usize;
        packages[index] = ((package, platform), Some(identifier));
        override_index += 1;
    }

    packages
}

const fn package_from_index(index: usize) -> Package {
    [
        Package::CaCertificates,
        Package::Sudo,
        Package::Git,
        Package::Curl,
        Package::Lsof,
        Package::Wget,
        Package::Rsync,
        Package::Stow,
        Package::Tar,
        Package::Unzip,
        Package::Zip,
        Package::Xz,
        Package::File,
        Package::ManDb,
        Package::ManPages,
        Package::BaseDevel,
        Package::Rustup,
        Package::Openssh,
        Package::Polkit,
        Package::CachyUpdate,
        Package::CachyosHello,
        Package::CachyosKernelManager,
        Package::LinuxCachyos,
        Package::CachyosZshConfig,
        Package::Firefox,
        Package::FirefoxI18nFr,
        Package::MesloFont,
        Package::NotoFontsCjk,
        Package::CachyosEmeraldKdeTheme,
        Package::CachyosIridescentKde,
        Package::CachyosNordKdeTheme,
        Package::Kate,
        Package::Micro,
        Package::CachyosMicroSettings,
        Package::Nano,
        Package::NanoSyntaxHighlighting,
        Package::Meld,
        Package::Glances,
        Package::Duf,
        Package::Tealdeer,
        Package::Filelight,
        Package::Pavucontrol,
        Package::Kcalc,
        Package::Shelly,
        Package::CachyosPackageinstaller,
        Package::Expac,
        Package::CachyosWallpapers,
        Package::Hwdetect,
        Package::Qtscrcpy,
        Package::Ripgrep,
        Package::Fd,
        Package::Fzf,
        Package::Zoxide,
        Package::Eza,
        Package::Bat,
        Package::Jq,
        Package::Fastfetch,
        Package::Btop,
        Package::Tokei,
        Package::GithubCli,
        Package::Hunk,
        Package::Go,
        Package::Bun,
        Package::Python,
        Package::Jdk,
        Package::Maven,
        Package::Llvm,
        Package::Make,
        Package::Cmake,
        Package::Nodejs,
        Package::Npm,
        Package::NodeGyp,
        Package::Zsh,
        Package::Fish,
        Package::CachyosFishConfig,
        Package::FishAutopair,
        Package::FishPurePrompt,
        Package::Fisher,
        Package::Neovim,
        Package::Vim,
        Package::EmacsWayland,
        Package::Ufw,
        Package::Sshfs,
        Package::Ghostty,
        Package::Kitty,
        Package::Alacritty,
        Package::Wezterm,
        Package::Konsole,
        Package::KdeUtilitiesMeta,
        Package::Yazi,
        Package::Ffmpeg,
        Package::SevenZip,
        Package::Poppler,
        Package::Resvg,
        Package::Imagemagick,
        Package::Lazygit,
        Package::Tmux,
        Package::WlClipboard,
        Package::Ydotool,
        Package::Kanata,
        Package::Handy,
        Package::AtSpi2Core,
        Package::Libxcomposite,
        Package::Libxdamage,
        Package::Libxrandr,
        Package::Libxkbcommon,
        Package::Pyside6,
        Package::Qt6Wayland,
        Package::LayerShellQt,
        Package::Libinput,
        Package::Systemd,
        Package::IosevkaFont,
        Package::DesktopFileUtils,
        Package::WslSshAgent,
        Package::Tailscale,
        Package::AndroidSdkPlatformTools,
        Package::Scrcpy,
        Package::Docker,
        Package::DockerBuildx,
        Package::DockerCompose,
        Package::Powershell,
        Package::WindowsTerminal,
        Package::Wsl,
        Package::OhMyPosh,
        Package::PowerToys,
        Package::Blender,
        Package::Krita,
        Package::Kdenlive,
        Package::Audacity,
        Package::ObsStudio,
        Package::Musescore,
        Package::VisualStudioBuildTools,
        Package::PythonInstallManager,
        Package::DockerDesktop,
        Package::VirtualBox,
        Package::LibreOffice,
        Package::Paru,
        Package::PlexFonts,
        Package::BlexMonoFont,
        Package::Haruna,
        Package::Mpvqt,
        Package::Mpv,
        Package::Kdeconnect,
        Package::Partitionmanager,
        Package::Ark,
        Package::Gwenview,
        Package::Plymouth,
        Package::PlymouthKcm,
        Package::CachyosPlymouthTheme,
        Package::CachyosPlymouthBootanimation,
        Package::Vlc,
        Package::Loupe,
        Package::Flatpak,
        Package::Fuse2,
        Package::PythonVenv,
        Package::Vscodium,
        Package::Bsdtar,
        Package::DockerRootlessExtras,
        Package::Slirp4netns,
        Package::Uidmap,
        Package::Qrencode,
        Package::T3codeDesktop,
        Package::NodejsLts,
    ][index]
}

const fn platform_from_index(index: usize) -> Platform {
    [
        Platform::Pacman,
        Platform::Aur,
        Platform::Apt,
        Platform::Winget,
    ][index]
}

pub static PACKAGES: [((Package, Platform), Option<&'static str>);
    Package::COUNT * Platform::COUNT] = build_packages();

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde::{
        Deserialize,
        de::value::{Error, U32Deserializer},
    };

    use super::*;

    /// Every `Package` variant in declaration order. Serde reads variants by their
    /// position, so this list does not depend on `Package::COUNT` or `package_from_index`.
    fn declared_packages() -> Vec<Package> {
        (0..)
            .map_while(|index| Package::deserialize(U32Deserializer::<Error>::new(index)).ok())
            .collect()
    }

    #[test]
    fn every_declared_package_has_its_index_and_a_name() {
        let declared = declared_packages();
        assert_eq!(
            declared.len(),
            Package::COUNT,
            "Package::COUNT is not the number of packages"
        );
        for (index, package) in declared.iter().enumerate() {
            assert_eq!(
                package_from_index(index),
                *package,
                "package_from_index({index}) is not {package:?}"
            );
            assert!(
                PACKAGES
                    .iter()
                    .any(|((candidate, _), name)| candidate == package && name.is_some()),
                "{package:?} has no name in PACKAGE_OVERRIDES"
            );
        }
    }

    #[test]
    fn each_package_has_at_most_one_name_per_platform() {
        let mut seen = HashSet::new();
        for (key, _) in PACKAGE_OVERRIDES {
            assert!(seen.insert(key), "{key:?} is listed twice");
        }
    }

    #[test]
    fn apt_names_from_a_vendor_name_a_known_repository() {
        for ((package, platform), name) in PACKAGE_OVERRIDES {
            if *platform == Platform::Apt {
                assert!(
                    apt_source(name).is_some(),
                    "{package:?}: {name} names no repository in APT_REPOSITORIES"
                );
            }
        }
    }
}

//! The parts of `dotfiles/` that modules deploy, embedded at build time.
//!
//! Each config package and each asset folder is its own static, so a binary keeps only
//! the ones that its modules reach. Folders that no module deploys are not embedded.
use typed_fs_rs::{EmbeddedDirectory, embed_dir};

embed_dir!(pub static AI = "../../dotfiles/ai");
embed_dir!(pub static GHOSTTY = "../../dotfiles/ghostty");
embed_dir!(pub static HANDY = "../../dotfiles/handy");
embed_dir!(pub static KANATA = "../../dotfiles/kanata");
embed_dir!(pub static KANATA_KDE = "../../dotfiles/kanata-kde");
embed_dir!(pub static PHONE = "../../dotfiles/phone");
embed_dir!(pub static PIPEWIRE = "../../dotfiles/pipewire");
embed_dir!(pub static PLASMA = "../../dotfiles/plasma");
embed_dir!(pub static T3CODE = "../../dotfiles/t3code");
embed_dir!(pub static T3CODE_DESKTOP = "../../dotfiles/t3code-desktop");
embed_dir!(pub static VSCODIUM = "../../dotfiles/vscodium");
embed_dir!(pub static YAZI = "../../dotfiles/yazi");
embed_dir!(pub static ZSH = "../../dotfiles/zsh");

embed_dir!(pub static BOOT_RESCUE_ASSETS = "../../dotfiles/assets/boot-rescue");
embed_dir!(pub static BOOTVINE_ASSETS = "../../dotfiles/assets/bootvine");
embed_dir!(pub static KDE_PLASMA_ASSETS = "../../dotfiles/assets/kde-plasma");
embed_dir!(pub static REFIND_ASSETS = "../../dotfiles/assets/refind");
embed_dir!(pub static WINDOWS_ASSETS = "../../dotfiles/assets/windows");

/// The config packages that Linux modules stow, by package name. Paths in each package
/// are relative to the package folder.
pub static CONFIG_PACKAGES: [(&str, &(dyn EmbeddedDirectory + Sync)); 13] = [
    ("ai", &AI),
    ("ghostty", &GHOSTTY),
    ("handy", &HANDY),
    ("kanata", &KANATA),
    ("kanata-kde", &KANATA_KDE),
    ("phone", &PHONE),
    ("pipewire", &PIPEWIRE),
    ("plasma", &PLASMA),
    ("t3code", &T3CODE),
    ("t3code-desktop", &T3CODE_DESKTOP),
    ("vscodium", &VSCODIUM),
    ("yazi", &YAZI),
    ("zsh", &ZSH),
];

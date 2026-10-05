//! A group of Winget packages that you choose on the screen.
use crate::{Context, Footprint, Module, ModuleResult, Package, support::verify_packages};

pub struct PackageGroup {
    pub name: &'static str,
    pub packages: &'static [Package],
}

impl PackageGroup {
    /// Tools the other Windows modules rely on, so it comes first.
    pub const BASE: Self = Self {
        name: "base-packages",
        packages: &[
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
    };

    /// Rust, C/C++ and build tools.
    pub const DEV_TOOLS: Self = Self {
        name: "dev-tools",
        packages: &[
            Package::Rustup,
            Package::Llvm,
            Package::VisualStudioBuildTools,
            Package::PythonInstallManager,
            Package::DockerDesktop,
        ],
    };

    /// Blender, Krita, Kdenlive, Audacity, OBS and MuseScore.
    pub const ART: Self = Self {
        name: "art",
        packages: &[
            Package::Blender,
            Package::Krita,
            Package::Kdenlive,
            Package::Audacity,
            Package::ObsStudio,
            Package::Musescore,
        ],
    };

    /// Handy, VirtualBox and LibreOffice.
    pub const SUPPLEMENTARY: Self = Self {
        name: "supplementary",
        packages: &[Package::Handy, Package::VirtualBox, Package::LibreOffice],
    };
}

impl Module for PackageGroup {
    fn name(&self) -> &'static str {
        self.name
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: self.packages.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.find_program("winget.exe")?;
        ctx.install_packages(self.packages)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, self.packages)
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

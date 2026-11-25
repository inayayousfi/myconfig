//! Herdr: herdr-core, the background server that keeps terminals and agents running, and
//! the desktop app that shows them.
use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting,
    deploy::verify_package,
    release_archive::{Build, ReleaseArchive},
    support::verify_packages,
};

/// herdr-core, whose command is `hc`. Another machine's Herdr app reaches it over SSH.
pub struct HerdrCore {
    pub source: HerdrCoreSource,
}

/// Where herdr-core comes from.
pub enum HerdrCoreSource {
    /// The AUR package.
    Package,
    /// Herdr's release archive, for systems without the package.
    Release,
}

/// The desktop app with its Black & Pink look, shown in the `blacknpink` layout.
pub struct Herdr;

impl HerdrCore {
    pub const NAME: &'static str = "herdr-core";
}

const CORE_PACKAGES: &[Package] = &[Package::HerdrCore];
const APP_PACKAGES: &[Package] = &[Package::HerdrApp];

const CORE_RELEASE: ReleaseArchive = ReleaseArchive {
    folder: "herdr-core-0.1.22",
    builds: &[
        Build {
            machine: "x86_64",
            url: "https://releases.herdr.dev/core/0.1.22/herdr-core-linux-x86_64.tar.gz",
            sha256: "3054ec3992ce80c5715d721caa9dcefa6056721d15dc113f4750afce763ba5df",
            strip: 0,
        },
        Build {
            machine: "aarch64",
            url: "https://releases.herdr.dev/core/0.1.22/herdr-core-linux-aarch64.tar.gz",
            sha256: "98757985245d45f8d73dd85e68b139b7f167c28488bb89edd9834c3f5a5cf4cf",
            strip: 0,
        },
    ],
    programs: &[("herdr-core", "herdr-core"), ("hc", "herdr-core")],
};

/// Herdr keeps the shown layout in its shared state. Its own layouts are replaced on
/// every start, so the look lives in a layout of its own, shown from the first start.
fn shown_layout() -> Setting {
    Setting::JsonKey {
        file: ".config/herdr-app/shared.json".to_owned(),
        key: "layout".to_owned(),
    }
}

const LAYOUT: &str = "\"blacknpink\"";

impl Module for HerdrCore {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        let packages = match self.source {
            HerdrCoreSource::Package => CORE_PACKAGES.to_vec(),
            HerdrCoreSource::Release => Vec::new(),
        };
        Footprint {
            packages,
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        match self.source {
            HerdrCoreSource::Package => ctx.install_packages(CORE_PACKAGES),
            HerdrCoreSource::Release => CORE_RELEASE.install(ctx),
        }
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        match self.source {
            HerdrCoreSource::Package => verify_packages(ctx, CORE_PACKAGES),
            HerdrCoreSource::Release => CORE_RELEASE.verify(ctx),
        }
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

impl Module for Herdr {
    fn name(&self) -> &'static str {
        "herdr"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base"), Need::Hard(HerdrCore::NAME)]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            // The app needs the core package, which the herdr-core module installs.
            packages: [CORE_PACKAGES, APP_PACKAGES].concat(),
            settings: vec![shown_layout()],
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        // The app depends on the core, so it is installed once the core is in place.
        ctx.install_packages(APP_PACKAGES)?;
        ctx.deploy_config("herdr")?;
        ctx.set(shown_layout(), LAYOUT)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, APP_PACKAGES)?;
        verify_package(ctx, "herdr")
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

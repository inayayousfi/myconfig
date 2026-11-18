//! LyX with TeX Live, the Qalculate calculator with gnuplot for its plots, and Okular as
//! the default PDF viewer.
use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package,
    support::{default_application_settings, set_default_application, verify_packages},
};

pub struct Math;

/// LyX comes from the AUR, and builds its documents with the system TeX Live.
const PACKAGES: &[Package] = &[
    Package::Lyx,
    Package::TexliveBasic,
    Package::TexliveLatex,
    Package::TexliveLatexrecommended,
    Package::TexliveLatexextra,
    Package::TexliveFontsrecommended,
    Package::TexliveBinextra,
    Package::TexliveLangfrench,
    Package::TexliveMathscience,
    Package::QalculateQt,
    Package::Gnuplot,
    Package::Okular,
];
const OKULAR: &str = "org.kde.okular.desktop";

fn pdf_types() -> Vec<String> {
    vec!["application/pdf".to_owned()]
}

impl Module for Math {
    fn name(&self) -> &'static str {
        "math"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base"), Need::Hard("plasma-version")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: default_application_settings(&pdf_types()),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(PACKAGES)?;
        set_default_application(ctx, OKULAR, &pdf_types()).map_err(|error| {
            format!("could not make Okular the default PDF viewer: {error}").into()
        })
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        for setting in default_application_settings(&pdf_types()) {
            if setting.read(ctx)?.as_deref() != Some(OKULAR) {
                return Err(format!("Okular is not the default for {}", setting.describe()).into());
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

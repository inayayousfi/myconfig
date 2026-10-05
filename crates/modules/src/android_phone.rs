//! Android phone tools and the `phone` command.
use crate::{
    Context, Footprint, Module, ModuleResult, Package,
    deploy::verify_package,
    support::{require_executable, require_programs, verify_packages},
};

pub struct AndroidPhone;

const PACKAGES: &[Package] = &[Package::AndroidSdkPlatformTools, Package::Scrcpy];

impl Module for AndroidPhone {
    fn name(&self) -> &'static str {
        "android-phone"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(PACKAGES)?;
        ctx.deploy_config("phone")
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        require_programs(ctx, &["adb", "scrcpy", "timeout"])?;
        verify_package(ctx, "phone")?;
        require_executable(&ctx.home.join(".local/bin/phone"))
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

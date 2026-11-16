//! Windows: Winget.
use std::fs;

use myconfig_utils::{PackageSpec, PackageTool};
use xshell::cmd;

use super::{PackageManager, names};
use crate::{Context, ModuleResult, support::with_temporary_directory};

pub(super) struct Winget;

impl PackageManager for Winget {
    /// Winget reads its sources on each command, so there is nothing to refresh.
    fn refresh(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }

    fn installed(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult<bool> {
        let name = spec.name;
        ctx.succeeds(cmd!(
            ctx.shell,
            "winget list --id {name} --exact --accept-source-agreements"
        ))
    }

    fn install(&self, ctx: &Context, specs: &[PackageSpec]) -> ModuleResult {
        let names = names(specs, PackageTool::Winget)?;
        if names.is_empty() {
            return Ok(());
        }
        let mut packages = Vec::new();
        for name in names {
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            {
                return Err(format!("invalid Winget package identifier: {name}").into());
            }
            packages.push(format!("{{\"PackageIdentifier\":\"{name}\"}}"));
        }
        let manifest = format!(
            "{{\"$schema\":\"https://aka.ms/winget-packages.schema.2.0.json\",\"Sources\":[{{\"Packages\":[{}],\"SourceDetails\":{{\"Argument\":\"https://cdn.winget.microsoft.com/cache\",\"Identifier\":\"Microsoft.Winget.Source_8wekyb3d8bbwe\",\"Name\":\"winget\",\"Type\":\"Microsoft.PreIndexed.Package\"}}}}]}}",
            packages.join(",")
        );
        with_temporary_directory("winget", |directory| {
            let path = directory.join("packages.json");
            fs::write(&path, manifest)?;
            ctx.run(cmd!(ctx.shell, "winget import -i {path} --accept-source-agreements --accept-package-agreements --ignore-unavailable"))
        })
    }

    /// `winget import --ignore-unavailable` skips a package that this machine cannot
    /// install, so the rest of the install goes on.
    fn still_missing(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult {
        ctx.note(&format!("Winget could not install {}", spec.name));
        Ok(())
    }

    fn uninstall(&self, ctx: &Context, specs: &[PackageSpec]) -> ModuleResult {
        for name in names(specs, PackageTool::Winget)? {
            ctx.run(cmd!(
                ctx.shell,
                "winget uninstall --id {name} --exact --silent --accept-source-agreements"
            ))?;
        }
        Ok(())
    }

    /// Winget has no reliable query for the packages that depend on another.
    /// Winget reports no dependents, so this is a plain uninstall.
    fn uninstall_with_dependents(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult {
        self.uninstall(ctx, &[spec])
    }

    fn dependents(&self, _ctx: &Context, _spec: PackageSpec) -> ModuleResult<Vec<String>> {
        Ok(Vec::new())
    }

    fn running_kernel(&self, _ctx: &Context) -> ModuleResult<Option<String>> {
        Ok(None)
    }

    fn uninstall_local(&self, _ctx: &Context, name: &str) -> ModuleResult {
        Err(
            format!("{name} is a package built from this repository, which only Arch installs")
                .into(),
        )
    }
}

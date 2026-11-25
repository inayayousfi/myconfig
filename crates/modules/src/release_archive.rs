//! A program from its vendor's release archive, for systems whose packages lack it or
//! carry an old version. The archive is checked against a SHA-256 pinned here, unpacked
//! into its own folder under `~/.local/opt`, and its programs are linked into
//! `~/.local/bin`.
use std::path::{Path, PathBuf};

use xshell::cmd;

use crate::{
    Context, ModuleResult,
    support::{require_executable, require_programs, with_temporary_directory},
};

/// One release of a program.
pub(crate) struct ReleaseArchive {
    /// The folder under `~/.local/opt`, which names the version so a new release unpacks
    /// beside the old one.
    pub folder: &'static str,
    pub builds: &'static [Build],
    /// Each program as its name in `~/.local/bin` and its path inside the unpacked folder.
    pub programs: &'static [(&'static str, &'static str)],
}

/// The archive for one processor architecture.
pub(crate) struct Build {
    /// The architecture as `uname -m` prints it.
    pub machine: &'static str,
    /// A `.tar.gz` archive.
    pub url: &'static str,
    pub sha256: &'static str,
    /// Leading folders that the archive puts around its files.
    pub strip: u8,
}

impl ReleaseArchive {
    fn folder_path(&self, ctx: &Context) -> PathBuf {
        ctx.home.join(".local/opt").join(self.folder)
    }

    fn build(&self, ctx: &Context) -> ModuleResult<&'static Build> {
        let machine = ctx.read(cmd!(ctx.shell, "uname -m"))?;
        let machine = machine.trim();
        self.builds
            .iter()
            .find(|build| build.machine == machine)
            .ok_or_else(|| format!("{} has no release for {machine}", self.folder).into())
    }

    pub(crate) fn install(&self, ctx: &Context) -> ModuleResult {
        let folder = self.folder_path(ctx);
        if !folder.exists() {
            require_programs(ctx, &["curl", "sha256sum", "tar", "uname"])?;
            let build = self.build(ctx)?;
            with_temporary_directory(self.folder, |directory| {
                self.unpack(ctx, build, directory, &folder)
            })?;
        }
        for (name, target) in self.programs {
            ctx.link(
                &ctx.home.join(".local/bin").join(name),
                &folder.join(target),
            )?;
        }
        Ok(())
    }

    /// Unpacks the checked archive into `folder`. Nothing is recorded or created there
    /// until the checksum matches.
    fn unpack(
        &self,
        ctx: &Context,
        build: &Build,
        directory: &Path,
        folder: &Path,
    ) -> ModuleResult {
        let sh = ctx.shell;
        let archive = directory.join("release.tar.gz");
        let url = build.url;
        ctx.run(cmd!(
            sh,
            "curl --fail --location --show-error --silent --output {archive} {url}"
        ))
        .map_err(|error| format!("could not download {url}: {error}"))?;
        let sum = ctx.read(cmd!(sh, "sha256sum {archive}"))?;
        if sum.split_whitespace().next() != Some(build.sha256) {
            return Err(format!("{url} does not match its pinned SHA-256").into());
        }
        let opt = folder.parent().ok_or("the release folder has no parent")?;
        if !opt.exists() {
            ctx.created(opt, false)?;
        }
        ctx.created(folder, false)?;
        std::fs::create_dir_all(folder)?;
        let strip = format!("--strip-components={}", build.strip);
        ctx.run(cmd!(sh, "tar -xzf {archive} -C {folder} {strip}"))
            .map_err(|error| format!("could not unpack {url}: {error}").into())
    }

    pub(crate) fn verify(&self, ctx: &Context) -> ModuleResult {
        let folder = self.folder_path(ctx);
        for (name, target) in self.programs {
            let link = ctx.home.join(".local/bin").join(name);
            let target = folder.join(target);
            if std::fs::read_link(&link).ok().as_deref() != Some(target.as_path()) {
                return Err(
                    format!("{} does not point to {}", link.display(), target.display()).into(),
                );
            }
            require_executable(&target)?;
        }
        Ok(())
    }
}

//! IBM Plex Serif, which long reading text uses. Winget has no IBM Plex package, so the
//! fonts come from IBM's release and install for the current user.
use std::fs;

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Setting,
    oh_my_posh_font::{FONTS_KEY, font_directory},
    support::with_temporary_directory,
};

pub struct PlexSerifFont;

const RELEASE: &str =
    "https://github.com/IBM/plex/releases/download/@ibm/plex-serif@2.0.0/ibm-plex-serif.zip";
const FAMILY: &str = "IBM Plex Serif";

fn registration(name: &str) -> Setting {
    Setting::RegistryValue {
        key: FONTS_KEY.to_owned(),
        name: format!("{name} (TrueType)"),
    }
}

impl Module for PlexSerifFont {
    fn name(&self) -> &'static str {
        "plex-serif-font"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let fonts = font_directory(ctx)?;
        with_temporary_directory("plex-serif", |directory| {
            let archive = directory.join("ibm-plex-serif.zip");
            ctx.run(cmd!(sh, "curl.exe -fsSL -o {archive} {RELEASE}"))?;
            ctx.run(cmd!(sh, "tar.exe -xf {archive} -C {directory}"))?;
            let release = directory.join("ibm-plex-serif/fonts/complete/ttf");
            let mut files: Vec<_> = fs::read_dir(&release)?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|extension| extension == "ttf"))
                .collect();
            files.sort();
            if files.is_empty() {
                return Err("the IBM Plex Serif release contains no TrueType fonts".into());
            }
            for file in files {
                let name = file.file_name().ok_or("font file has no name")?;
                // Windows locks a font file that a program has loaded, and the release
                // version is fixed, so a present file is already current.
                let target = fonts.join(name);
                if !target.exists() {
                    ctx.write_file(&target, &fs::read(&file)?, false)?;
                }
                let stem = file.file_stem().ok_or("font file has no name")?;
                ctx.set(
                    registration(&stem.to_string_lossy()),
                    &format!("REG_SZ:{}", target.display()),
                )?;
            }
            Ok(())
        })
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let (_, output) = ctx.read_unchecked(cmd!(ctx.shell, "reg.exe query {FONTS_KEY}"))?;
        if !crate::setting::registry_value::value_names(&output)
            .iter()
            .any(|name| name.starts_with(&FAMILY.replace(' ', "")) || name.starts_with(FAMILY))
        {
            return Err("IBM Plex Serif is not registered for this user".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

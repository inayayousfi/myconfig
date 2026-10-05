//! The Black & Pink rEFInd boot menu theme, when rEFInd is installed.
use std::{
    fs,
    path::{Path, PathBuf},
};

use embedded_dotfiles::DOTFILES;
use xshell::cmd;

use crate::{Context, Footprint, Module, ModuleResult, Package, support::with_temporary_directory};

pub struct Refind;

const CANDIDATES: [&str; 3] = [
    "/boot/EFI/refind/refind.conf",
    "/boot/efi/EFI/refind/refind.conf",
    "/efi/EFI/refind/refind.conf",
];
const BLOCK: &str = "\n# BEGIN MYCONFIG REFIND\ninclude managed.conf\ninclude themes/black-pink/theme.conf\n# END MYCONFIG REFIND\n";

fn refind_config(ctx: &Context) -> ModuleResult<Option<PathBuf>> {
    for candidate in CANDIDATES {
        if ctx.succeeds(cmd!(ctx.shell, "sudo test -f {candidate}"))? {
            return Ok(Some(PathBuf::from(candidate)));
        }
    }
    Ok(None)
}

pub(crate) fn generate_images(ctx: &Context, directory: &Path) -> ModuleResult {
    let sh = ctx.shell;
    let tool = ctx
        .find_program("magick")
        .or_else(|_| ctx.find_program("convert"))?;
    let pink = "#ff4ead";
    let banner = directory.join("banner.png");
    let bottom = "rectangle 0,1076 1920,1080";
    ctx.run(cmd!(
        sh,
        "{tool} -size 1920x1080 xc:#000000 -fill {pink} -draw {bottom} {banner}"
    ))?;
    for (name, size, rect, stroke) in [
        (
            "selection_big.png",
            "144x144",
            "roundrectangle 2,2 142,142 12,12",
            "3",
        ),
        (
            "selection_small.png",
            "64x64",
            "roundrectangle 1,1 63,63 6,6",
            "2",
        ),
    ] {
        let output = directory.join(name);
        let transparent = "xc:none";
        let pink_fill = "rgba(255,78,173,0.16)";
        ctx.run(cmd!(sh, "{tool} -size {size} {transparent} -fill {pink_fill} -draw {rect} -stroke {pink} -strokewidth {stroke} -fill none -draw {rect} {output}"))?;
    }
    Ok(())
}

/// The config with one managed block that includes the theme, replacing older blocks.
fn updated_config(original: &str) -> String {
    let mut new = String::new();
    let mut managed = false;
    for line in original.lines() {
        match line {
            "# BEGIN MYCONFIG BLACKNPINK" | "# BEGIN MYCONFIG REFIND" => managed = true,
            "# END MYCONFIG BLACKNPINK" | "# END MYCONFIG REFIND" => managed = false,
            _ if !managed => {
                new.push_str(line);
                new.push('\n');
            }
            _ => {}
        }
    }
    // Trailing blank lines are dropped so repeated runs leave the file unchanged.
    let mut new = new.trim_end().to_owned();
    new.push('\n');
    new.push_str(BLOCK);
    new
}

impl Module for Refind {
    fn name(&self) -> &'static str {
        "refind"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Imagemagick],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let Some(config) = refind_config(ctx)? else {
            ctx.note("Skipping the rEFInd theme because no installation was found");
            return Ok(());
        };
        let directory = config
            .parent()
            .ok_or("rEFInd configuration has no parent directory")?;
        let theme = directory.join("themes/black-pink");
        with_temporary_directory("refind", |temporary| {
            generate_images(ctx, temporary)?;
            // Output of an earlier version of this setup, replaced by themes/black-pink.
            let legacy = directory.join("themes/blacknpink");
            ctx.run(cmd!(sh, "sudo rm -rf -- {legacy}"))?;
            let theme_conf = DOTFILES
                .assets
                .refind
                .refind
                .themes
                .black_pink
                .theme_conf
                .content;
            ctx.write_system_file(&theme.join("theme.conf"), theme_conf, "0644")?;
            for name in ["banner.png", "selection_big.png", "selection_small.png"] {
                ctx.write_system_file(&theme.join(name), &fs::read(temporary.join(name))?, "0644")?;
            }
            let global = DOTFILES.assets.refind.refind.global_conf.content;
            ctx.write_system_file(&directory.join("managed.conf"), global, "0644")?;
            let original = ctx.read(cmd!(sh, "sudo cat {config}"))?;
            ctx.write_system_file(&config, updated_config(&original).as_bytes(), "0644")
        })
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let Some(config) = refind_config(ctx)? else {
            return Ok(());
        };
        let contents = ctx.read(cmd!(ctx.shell, "sudo cat {config}"))?;
        if !format!("{contents}\n").ends_with(BLOCK) {
            return Err("rEFInd does not include the managed theme block".into());
        }
        let directory = config
            .parent()
            .ok_or("rEFInd configuration has no parent directory")?;
        for path in [
            "managed.conf",
            "themes/black-pink/theme.conf",
            "themes/black-pink/banner.png",
        ] {
            let path = directory.join(path);
            if !ctx.succeeds(cmd!(ctx.shell, "sudo test -f {path}"))? {
                return Err(format!("{} is missing", path.display()).into());
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_managed_block_replaces_older_blocks() {
        let original =
            "timeout 5\n# BEGIN MYCONFIG BLACKNPINK\ninclude old.conf\n# END MYCONFIG BLACKNPINK\n";
        assert_eq!(updated_config(original), format!("timeout 5\n{BLOCK}"));
        assert_eq!(
            updated_config(&updated_config(original)),
            updated_config(original)
        );
    }
}

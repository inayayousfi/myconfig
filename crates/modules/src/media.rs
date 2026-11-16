//! The VLC media player and the Loupe image viewer, as the default applications for
//! their file types.
use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting,
    support::{
        default_application_settings, desktop_entry_types, set_default_application, verify_packages,
    },
};

pub struct Media;

const PACKAGES: &[Package] = &[Package::Vlc, Package::Loupe];
const VLC: &str = "vlc.desktop";
const LOUPE: &str = "org.gnome.Loupe.desktop";

/// VLC also claims disc images and playlists; it only becomes the player.
fn vlc_types(ctx: &Context) -> ModuleResult<Vec<String>> {
    Ok(desktop_entry_types(ctx, VLC)?
        .into_iter()
        .filter(|kind| kind.starts_with("audio/") || kind.starts_with("video/"))
        .collect())
}

fn defaults(ctx: &Context) -> ModuleResult<[(&'static str, Vec<String>); 2]> {
    Ok([
        (VLC, vlc_types(ctx)?),
        (LOUPE, desktop_entry_types(ctx, LOUPE)?),
    ])
}

impl Module for Media {
    fn name(&self) -> &'static str {
        "media"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("plasma-version")]
    }

    fn footprint(&self, ctx: &Context) -> Footprint {
        let settings: Vec<Setting> = defaults(ctx)
            .map(|defaults| {
                defaults
                    .iter()
                    .flat_map(|(_, types)| default_application_settings(types))
                    .collect()
            })
            .unwrap_or_default();
        Footprint {
            packages: PACKAGES.to_vec(),
            settings,
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(PACKAGES)?;
        for (entry, types) in defaults(ctx)? {
            set_default_application(ctx, entry, &types)
                .map_err(|error| format!("could not make {entry} the default: {error}"))?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        for (entry, types) in defaults(ctx)? {
            for setting in default_application_settings(&types) {
                if setting.read(ctx)?.as_deref() != Some(entry) {
                    return Err(
                        format!("{entry} is not the default for {}", setting.describe()).into(),
                    );
                }
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

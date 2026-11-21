//! The catalogs read from excerpts of the real files, laid out under a temporary root.
use std::{fs, path::PathBuf};

use super::{Keyboard, TimeZone, console_keymap, keyboards, locales, time_zones, time_zones_for};

/// A temporary root folder holding the given files, removed when dropped.
struct Root(PathBuf);

impl Root {
    fn new(name: &str, files: &[(&str, &str)]) -> Root {
        let root =
            std::env::temp_dir().join(format!("inayos-catalog-{name}-{}", std::process::id()));
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Root(root)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const BASE_LST: &str = "! model
  pc105           Generic 105-key PC
  us              U.S. Standard

! layout
  us              English (US)
  ara             Arabic
  fr              French
  de              German
  custom          A user-defined custom Layout

! variant
  intl            us: English (US, intl., with dead keys)
  bepo            fr: French (BEPO)
";

#[test]
fn keyboards_are_the_layouts_sorted_by_name_with_their_country() {
    let root = Root::new(
        "keyboards",
        &[("usr/share/X11/xkb/rules/base.lst", BASE_LST)],
    );
    let keyboard = |layout: &str, name: &str, country: Option<&str>| Keyboard {
        layout: layout.into(),
        name: name.into(),
        country: country.map(Into::into),
    };
    assert_eq!(
        keyboards(&root.0).unwrap(),
        [
            keyboard("ara", "Arabic", None),
            keyboard("us", "English (US)", Some("US")),
            keyboard("fr", "French", Some("FR")),
            keyboard("de", "German", Some("DE")),
        ]
    );
}

const KBD_MODEL_MAP: &str = "# consolelayout\t\txlayout\txmodel\t\txvariant\txoptions\t\tbcp47
sg\t\t\tch\tpc105\t\tde_nodeadkeys\tterminate:ctrl_alt_bksp\t\tde-CH
uk\t\t\tgb\tpc105\t\t-\t\tterminate:ctrl_alt_bksp\t\ten-GB
de\t\t\tde\tpc105\t\t-\t\tterminate:ctrl_alt_bksp\t\tde-DE,de-AT,de
ua-utf\t\t\tua,us\tpc105\t\t-\t\tterminate:ctrl_alt_bksp,grp:shifts_toggle\tuk-UA,uk
de-latin1\t\tde\tpc105\t\t-\t\tterminate:ctrl_alt_bksp\t\t-
fr_CH\t\t\tch\tpc105\t\tfr\t\tterminate:ctrl_alt_bksp\t\tfr-CH
";

#[test]
fn the_console_keymap_follows_the_keymap_map_and_falls_back_to_us() {
    let root = Root::new(
        "keymaps",
        &[("usr/share/systemd/kbd-model-map", KBD_MODEL_MAP)],
    );
    assert_eq!(console_keymap(&root.0, "gb"), "uk");
    assert_eq!(console_keymap(&root.0, "de"), "de");
    assert_eq!(console_keymap(&root.0, "ua"), "ua-utf");
    // Switzerland has only variant lines; the first one wins.
    assert_eq!(console_keymap(&root.0, "ch"), "sg");
    assert_eq!(console_keymap(&root.0, "epo"), "us");
    let empty = Root::new("no-keymaps", &[]);
    assert_eq!(console_keymap(&empty.0, "gb"), "us");
}

#[test]
fn locales_are_the_utf8_entries_of_the_supported_list() {
    let root = Root::new(
        "locales",
        &[(
            "usr/share/i18n/SUPPORTED",
            "aa_DJ.UTF-8 UTF-8\naa_DJ ISO-8859-1\naa_ER UTF-8\nfr_FR.UTF-8 UTF-8\nfr_FR ISO-8859-1\nfr_FR@euro ISO-8859-15\n",
        )],
    );
    assert_eq!(
        locales(&root.0).unwrap(),
        ["aa_DJ.UTF-8", "aa_ER", "fr_FR.UTF-8"]
    );
}

const ZONE1970: &str = "# tzdb timezone descriptions
#
#country-
#codes\tcoordinates\tTZ\tcomments
FR,MC\t+4852+00220\tEurope/Paris
CH,DE,LI\t+4723+00832\tEurope/Zurich\tBüsingen
DE,DK,NO,SE,SJ\t+5230+01322\tEurope/Berlin\tmost of Germany
US\t+404251-0740023\tAmerica/New_York\tEastern (most areas)
US\t+340308-1181434\tAmerica/Los_Angeles\tPacific
";

#[test]
fn time_zones_start_with_the_country_of_the_keyboard() {
    let root = Root::new("zones", &[("usr/share/zoneinfo/zone1970.tab", ZONE1970)]);
    let zones = time_zones(&root.0).unwrap();
    let names = |zones: &[TimeZone]| {
        zones
            .iter()
            .map(|zone| zone.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&zones),
        [
            "Europe/Paris",
            "Europe/Zurich",
            "Europe/Berlin",
            "America/New_York",
            "America/Los_Angeles",
            "UTC"
        ]
    );
    assert_eq!(zones[0].countries, ["FR", "MC"]);
    assert_eq!(
        names(&time_zones_for(&zones, Some("DE"))),
        [
            "Europe/Berlin",
            "Europe/Zurich",
            "America/Los_Angeles",
            "America/New_York",
            "Europe/Paris",
            "UTC"
        ]
    );
    assert_eq!(
        names(&time_zones_for(&zones, Some("US")))[..2],
        ["America/New_York", "America/Los_Angeles"]
    );
    assert_eq!(
        names(&time_zones_for(&zones, None)),
        [
            "America/Los_Angeles",
            "America/New_York",
            "Europe/Berlin",
            "Europe/Paris",
            "Europe/Zurich",
            "UTC"
        ]
    );
}

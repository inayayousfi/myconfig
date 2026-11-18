//! The keyboard layouts, languages and time zones an interface can offer, read from the files
//! of the live system under `root`, which is `/` there. Nothing here asks an outside service.
use std::{fs, path::Path};

use crate::Error;

const XKB_RULES: &str = "usr/share/X11/xkb/rules/base.lst";
const KEYMAP_MAP: &str = "usr/share/systemd/kbd-model-map";
const SUPPORTED_LOCALES: &str = "usr/share/i18n/SUPPORTED";
const ZONES: &str = "usr/share/zoneinfo/zone1970.tab";

/// The console keymap when no line of the keymap map fits a layout.
const DEFAULT_KEYMAP: &str = "us";

/// One XKB keyboard layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keyboard {
    /// The XKB layout name, such as `fr`.
    pub layout: String,
    /// Its description, such as `French`.
    pub name: String,
    /// The ISO 3166 country code of a two-letter layout name, such as `FR`, which XKB uses
    /// for country layouts. None for the others, such as `ara` or `epo`.
    pub country: Option<String>,
}

/// One time zone and the ISO 3166 codes of the countries that use it, the main one first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeZone {
    pub name: String,
    pub countries: Vec<String>,
}

fn read(root: &Path, file: &str) -> Result<String, Error> {
    let path = root.join(file);
    fs::read_to_string(&path).map_err(|error| Error::Io {
        action: "read",
        path,
        error,
    })
}

/// The layouts of the `! layout` section of the XKB rules list, sorted by description.
pub fn keyboards(root: &Path) -> Result<Vec<Keyboard>, Error> {
    let text = read(root, XKB_RULES)?;
    let mut keyboards: Vec<Keyboard> = text
        .lines()
        .skip_while(|line| line.trim() != "! layout")
        .skip(1)
        .take_while(|line| !line.starts_with('!'))
        .filter_map(|line| {
            let (layout, name) = line.trim().split_once(char::is_whitespace)?;
            // The custom layout is an empty slot for the user's own XKB files.
            (layout != "custom").then(|| Keyboard {
                layout: layout.to_owned(),
                name: name.trim().to_owned(),
                country: (layout.len() == 2 && layout.chars().all(|c| c.is_ascii_lowercase()))
                    .then(|| layout.to_ascii_uppercase()),
            })
        })
        .collect();
    keyboards.sort_by_key(|keyboard| keyboard.name.to_lowercase());
    Ok(keyboards)
}

/// The console keymap that systemd's keymap map pairs with the XKB `layout`, as `localectl`
/// picks it: a line for the layout without a variant first, then any line for it. `us` when
/// the map has none or cannot be read.
pub fn console_keymap(root: &Path, layout: &str) -> String {
    let Ok(text) = read(root, KEYMAP_MAP) else {
        return DEFAULT_KEYMAP.to_owned();
    };
    // Columns: console keymap, XKB layouts separated by commas, model, variant, options.
    let rows: Vec<Vec<&str>> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .filter(|fields| fields.len() >= 4 && fields[1].split(',').next() == Some(layout))
        .collect();
    rows.iter()
        .find(|fields| fields[3] == "-")
        .or(rows.first())
        .map_or_else(|| DEFAULT_KEYMAP.to_owned(), |fields| fields[0].to_owned())
}

/// The UTF-8 locales that glibc supports, such as `fr_FR.UTF-8`, in the order of its list.
pub fn locales(root: &Path) -> Result<Vec<String>, Error> {
    let text = read(root, SUPPORTED_LOCALES)?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let (name, charset) = (fields.next()?, fields.next()?);
            (charset == "UTF-8").then(|| name.to_owned())
        })
        .collect())
}

/// The time zones of `zone1970.tab` in its order, which lists each country's most populous
/// zones first where geography allows, then `UTC`, which belongs to no country.
pub fn time_zones(root: &Path) -> Result<Vec<TimeZone>, Error> {
    let text = read(root, ZONES)?;
    let mut zones: Vec<TimeZone> = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let countries = fields.next()?;
            let _coordinates = fields.next()?;
            let name = fields.next()?.trim();
            (!name.is_empty()).then(|| TimeZone {
                name: name.to_owned(),
                countries: countries.split(',').map(str::to_owned).collect(),
            })
        })
        .collect();
    zones.push(TimeZone {
        name: "UTC".to_owned(),
        countries: Vec::new(),
    });
    Ok(zones)
}

/// `zones` with the zones of `country` first: those whose main country it is, then those it
/// shares with another country, then the rest, each group in its earlier order.
pub fn time_zones_for(zones: &[TimeZone], country: Option<&str>) -> Vec<TimeZone> {
    let rank = |zone: &TimeZone| match country {
        Some(code) if zone.countries.first().map(String::as_str) == Some(code) => 0,
        Some(code) if zone.countries.iter().any(|other| other == code) => 1,
        _ => 2,
    };
    let mut ordered = zones.to_vec();
    // The sort is stable, so the country's zones keep the order of `zone1970.tab`. The others
    // follow by name.
    ordered.sort_by(|a, b| {
        rank(a).cmp(&rank(b)).then_with(|| match rank(a) {
            2 => a.name.cmp(&b.name),
            _ => std::cmp::Ordering::Equal,
        })
    });
    ordered
}

#[cfg(test)]
mod tests;

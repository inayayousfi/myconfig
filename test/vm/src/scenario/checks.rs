//! Reads what the scenarios check in the installed system: the Plasma panels in the applet
//! configuration, and the order of the partitions on the disk.

use std::collections::BTreeMap;

/// The widgets that the top bar of `dotfiles/plasma`'s default layout holds, in its order.
pub const TOP_WIDGETS: &[&str] = &[
    "inayos.session",
    "org.kde.plasma.appmenu",
    "org.kde.plasma.panelspacer",
    "org.kde.plasma.systemtray",
    "org.kde.plasma.digitalclock",
];
/// The widgets that the dock of that layout holds.
pub const DOCK_WIDGETS: &[&str] = &["org.kde.plasma.icontasks"];

/// One panel in `plasma-org.kde.plasma.desktop-appletsrc`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Panel {
    /// The `inayosRole` that the layout script writes on the panels it builds.
    pub role: Option<String>,
    /// The plugins of the panel's widgets, in the order of their ids.
    pub widgets: Vec<String>,
}

/// The panels in the text of `plasma-org.kde.plasma.desktop-appletsrc`. A panel is a
/// `[Containments][<id>]` group whose plugin is `org.kde.panel`. Its keys can sit in the
/// containment group or in one of its own subgroups; its widgets are the
/// `[Containments][<id>][Applets][<id>]` groups.
pub fn panels(appletsrc: &str) -> Vec<Panel> {
    #[derive(Default)]
    struct Containment {
        plugin: Option<String>,
        role: Option<String>,
        widgets: BTreeMap<u64, String>,
    }
    let mut containments: BTreeMap<u64, Containment> = BTreeMap::new();
    let mut group: Vec<String> = Vec::new();
    for line in appletsrc.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            group = line[1..line.len() - 1]
                .split("][")
                .map(str::to_owned)
                .collect();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(id) = (group.len() >= 2 && group[0] == "Containments")
            .then(|| group[1].parse::<u64>().ok())
            .flatten()
        else {
            continue;
        };
        let containment = containments.entry(id).or_default();
        let applet = (group.len() >= 4 && group[2] == "Applets")
            .then(|| group[3].parse::<u64>().ok())
            .flatten();
        match (applet, group.len(), key.trim()) {
            (Some(applet), 4, "plugin") => {
                containment.widgets.insert(applet, value.trim().to_owned());
            }
            (Some(_), _, _) => {}
            (None, 2, "plugin") => containment.plugin = Some(value.trim().to_owned()),
            (None, _, "inayosRole") => containment.role = Some(value.trim().to_owned()),
            (None, _, _) => {}
        }
    }
    containments
        .into_values()
        .filter(|containment| containment.plugin.as_deref() == Some("org.kde.panel"))
        .map(|containment| Panel {
            role: containment.role,
            widgets: containment.widgets.into_values().collect(),
        })
        .collect()
}

/// What is missing for the panels of the default layout: one top bar and one dock, marked by
/// the layout script and holding its widgets. `None` when nothing is missing.
pub fn missing_layout(panels: &[Panel]) -> Option<String> {
    let mut missing = Vec::new();
    for (role, widgets) in [("top", TOP_WIDGETS), ("dock", DOCK_WIDGETS)] {
        let marked: Vec<&Panel> = panels
            .iter()
            .filter(|panel| panel.role.as_deref() == Some(role))
            .collect();
        match marked.as_slice() {
            [panel] if panel.widgets == *widgets => {}
            [panel] => missing.push(format!(
                "the {role} panel holds {:?}, not {widgets:?}",
                panel.widgets
            )),
            others => missing.push(format!(
                "{} panels have the role {role}, not one",
                others.len()
            )),
        }
    }
    (!missing.is_empty()).then(|| missing.join("; "))
}

/// The file systems of the disk's partitions, in the order of their start sectors. Each line
/// of `listing` holds a start sector, a partition name and its file system type.
pub fn partition_order(listing: &str) -> Vec<String> {
    let mut partitions: Vec<(u64, String)> = listing
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let start = fields.next()?.parse().ok()?;
            let _name = fields.next()?;
            Some((start, fields.next().unwrap_or("").to_owned()))
        })
        .collect();
    partitions.sort();
    partitions.into_iter().map(|(_, fstype)| fstype).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An applets file as plasmashell writes it after the default layout ran: a desktop, the
    /// top bar with its widgets, and the dock.
    const APPLETSRC: &str = "\
[ActionPlugins][0]
RightButton;NoModifier=org.kde.contextmenu

[Containments][1]
activityId=abc
formfactor=0
plugin=org.kde.plasma.folder

[Containments][2]
formfactor=2
inayosLayoutVersion=3
inayosRole=top
inayosScreen=0
location=3
plugin=org.kde.panel

[Containments][2][Applets][3]
plugin=inayos.session

[Containments][2][Applets][4]
plugin=org.kde.plasma.appmenu

[Containments][2][Applets][5]
plugin=org.kde.plasma.panelspacer

[Containments][2][Applets][6]
plugin=org.kde.plasma.systemtray

[Containments][2][Applets][6][Configuration]
SystrayContainmentId=8

[Containments][2][Applets][7]
plugin=org.kde.plasma.digitalclock

[Containments][8]
plugin=org.kde.plasma.private.systemtray

[Containments][9]
location=4
plugin=org.kde.panel

[Containments][9][Configuration]
inayosRole=dock

[Containments][9][Applets][10]
plugin=org.kde.plasma.icontasks

[Containments][9][Applets][10][Configuration][General]
launchers=applications:org.kde.dolphin.desktop
";

    #[test]
    fn the_default_layout_has_its_top_bar_and_its_dock() {
        let panels = panels(APPLETSRC);
        assert_eq!(
            panels,
            [
                Panel {
                    role: Some("top".to_owned()),
                    widgets: TOP_WIDGETS
                        .iter()
                        .map(|widget| (*widget).to_owned())
                        .collect(),
                },
                Panel {
                    role: Some("dock".to_owned()),
                    widgets: vec!["org.kde.plasma.icontasks".to_owned()],
                },
            ]
        );
        assert_eq!(missing_layout(&panels), None);
    }

    #[test]
    fn plasma_s_fallback_panel_is_not_the_default_layout() {
        // The default panel template that the layout script falls back to has no role marks.
        let fallback = "\
[Containments][2]
plugin=org.kde.panel

[Containments][2][Applets][3]
plugin=org.kde.plasma.kickoff

[Containments][2][Applets][4]
plugin=org.kde.plasma.icontasks
";
        let missing = missing_layout(&panels(fallback)).expect("the layout is missing");
        assert!(missing.contains("0 panels have the role top"), "{missing}");
        assert!(missing.contains("0 panels have the role dock"), "{missing}");
    }

    #[test]
    fn a_marked_panel_with_other_widgets_is_reported() {
        let text = APPLETSRC.replace(
            "plugin=org.kde.plasma.appmenu",
            "plugin=org.kde.plasma.kickoff",
        );
        let missing = missing_layout(&panels(&text)).expect("the top bar differs");
        assert!(missing.starts_with("the top panel holds"), "{missing}");
    }

    #[test]
    fn partitions_are_ordered_by_their_start_sector() {
        let listing = "1050624 vda3 btrfs\n2048 vda1 vfat\n4196352 vda2 ext4\n";
        assert_eq!(partition_order(listing), ["vfat", "btrfs", "ext4"]);
    }
}

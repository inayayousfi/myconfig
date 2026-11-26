// Raising this version rebuilds the bars that an older layout made.
const layoutVersion = "2";
// The layout command says whether it has applied this layout before, and
// whether the installer asks to pin the Overview icon again. Plasma also runs
// this file, without those two variables, as the default layout of the InayOS
// global theme, when it starts with no desktops at a new user's first login.
const defaultLayout = typeof inayosFirstRun === "undefined";
const firstRun = defaultLayout ? true : inayosFirstRun;
const ensureOverview = typeof inayosEnsureOverview === "undefined" ? false : inayosEnsureOverview;

// As the default layout, the result only reaches Plasma's debug log, so a
// layout that cannot be built gives way to Plasma's own panel instead of a
// desktop with no panels.
function fallBack() {
    if (defaultLayout) {
        loadTemplate("org.kde.plasma.desktop.defaultPanel");
    }
}

const overviewLauncher = "applications:inayos-overview.desktop";
const earlierOverviewLauncher = "applications:myconfig-overview.desktop";
const herdrLauncher = "applications:herdr-app.desktop";
// A rebuilt dock drops these pins and shows Herdr in place of the first one.
const replacedLaunchers = [
    "applications:t3code.desktop",
    "applications:codium.desktop",
    "applications:vscodium.desktop",
    "applications:com.mitchellh.ghostty.desktop",
];
// A new dock starts with these pins. The task widget shows a blank icon for a
// pin whose application is not installed, so each one needs a module that
// installs it.
const defaultLaunchers = [
    overviewLauncher,
    "applications:google-chrome.desktop",
    herdrLauncher,
    "applications:lyx.desktop",
    "applications:io.github.Qalculate.qalculate-qt.desktop",
];
const wallpaper = "/usr/share/wallpapers/Next/";
const topWidgetTypes = [
    "inayos.session",
    "org.kde.plasma.appmenu",
    "org.kde.plasma.panelspacer",
    "org.kde.plasma.systemtray",
    "org.kde.plasma.digitalclock",
];
const dockWidgetTypes = [
    "org.kde.plasma.icontasks",
];

const missingWidgetTypes = topWidgetTypes.concat(dockWidgetTypes).filter(type => !knownWidgetTypes.includes(type));
if (missingWidgetTypes.length > 0) {
    print(`INAYOS_PLASMA_LAYOUT=missing:${missingWidgetTypes.join(",")}`);
    fallBack();
} else {
    function readPanel(panel, key, fallback) {
        panel.currentConfigGroup = [];
        return String(panel.readConfig(key, fallback));
    }

    // myconfig, the earlier name of InayOS, marked its panels with myconfigRole,
    // myconfigScreen and myconfigLayoutVersion. Such a panel counts as managed, so its
    // dock keeps its pins, and markPanel writes the new keys on it. The scripting
    // interface cannot delete a key, and the new keys take precedence, so the earlier
    // keys stay unread.
    function readMarker(panel, name, fallback) {
        const current = readPanel(panel, `inayos${name}`, "");
        return current !== "" ? current : readPanel(panel, `myconfig${name}`, fallback);
    }

    function markPanel(panel, role, screen) {
        panel.currentConfigGroup = [];
        panel.writeConfig("inayosRole", role);
        panel.writeConfig("inayosScreen", screen);
        panel.writeConfig("inayosLayoutVersion", layoutVersion);
    }

    function hasWidgets(panel, types) {
        const widgets = panel.widgets();
        return widgets.length === types.length && widgets.every((widget, index) => widget.type === types[index]);
    }

    function tasksOf(panel) {
        return panel.widgets().find(widget => widget.type === "org.kde.plasma.icontasks");
    }

    function launchersIn(tasks) {
        if (!tasks) {
            return [];
        }
        tasks.currentConfigGroup = ["General"];
        // The Overview pin of a dock that myconfig made names its earlier desktop file.
        return tasks.readConfig("launchers", [])
            .map(launcher => launcher === earlierOverviewLauncher ? overviewLauncher : launcher);
    }

    function launchersOf(panel) {
        return launchersIn(tasksOf(panel));
    }

    function withOverview(launchers) {
        return launchers.includes(overviewLauncher) ? launchers : [overviewLauncher].concat(launchers);
    }

    function withHerdr(launchers) {
        const first = launchers.findIndex(launcher => replacedLaunchers.includes(launcher));
        const kept = launchers.filter(launcher => !replacedLaunchers.includes(launcher));
        if (first < 0 || kept.includes(herdrLauncher)) {
            return kept;
        }
        const before = launchers.slice(0, first).filter(launcher => !replacedLaunchers.includes(launcher)).length;
        return kept.slice(0, before).concat([herdrLauncher], kept.slice(before));
    }

    function configureTasks(tasks, launchers) {
        tasks.currentConfigGroup = ["General"];
        tasks.writeConfig("launchers", launchers);
        tasks.writeConfig("showOnlyCurrentScreen", true);
        tasks.writeConfig("showOnlyCurrentActivity", true);
        tasks.writeConfig("showOnlyCurrentDesktop", false);
        tasks.writeConfig("fill", false);
    }

    // The tray keeps every Plasma widget in its popup, so the bar shows only
    // the icons of applications. A new tray does not know its widgets yet,
    // so every installed widget type goes in the hidden list: the tray
    // ignores the ones it does not hold.
    function configureTray(panel) {
        const tray = panel.widgets().find(widget => widget.type === "org.kde.plasma.systemtray");
        if (!tray) {
            return;
        }
        tray.currentConfigGroup = ["General"];
        const hidden = tray.readConfig("hiddenItems", []);
        const widgets = knownWidgetTypes
            .concat(tray.readConfig("knownItems", []), tray.readConfig("extraItems", []))
            .filter((type, index, all) => all.indexOf(type) === index && !hidden.includes(type));
        tray.writeConfig("hiddenItems", hidden.concat(widgets));
        tray.writeConfig("shownItems", tray.readConfig("shownItems", []).filter(item => !widgets.includes(item) && !hidden.includes(item)));
    }

    function fillTop(panel) {
        panel.widgets().slice().forEach(widget => widget.remove());
        topWidgetTypes.forEach(type => panel.addWidget(type));
    }

    // Plasma still lists a removed widget for a while, so a lookup by type
    // could find the old task widget: the new one is returned instead.
    function fillDock(panel, launchers) {
        panel.widgets().slice().forEach(widget => widget.remove());
        const tasks = panel.addWidget("org.kde.plasma.icontasks");
        configureTasks(tasks, launchers);
        return tasks;
    }

    function configureTop(panel, screen) {
        panel.screen = screen;
        panel.location = "top";
        panel.alignment = "left";
        panel.lengthMode = "fill";
        panel.height = 32;
        panel.hiding = "none";
        panel.floating = false;
        panel.opacity = "translucent";
        markPanel(panel, "top", screen);
    }

    function configureDock(panel, screen) {
        panel.screen = screen;
        panel.location = "bottom";
        panel.alignment = "center";
        panel.lengthMode = "fit";
        panel.height = 48;
        panel.hiding = "autohide";
        panel.floating = true;
        panel.opacity = "translucent";
        markPanel(panel, "dock", screen);
    }

    const initialPanels = panels();
    const managed = initialPanels.filter(panel => readMarker(panel, "Role", "") !== "");
    // The first run replaces every panel, such as the ones an earlier theme made.
    const rebuildAll = firstRun && managed.length === 0;
    const retained = new Map();
    const stale = [];
    const oldLaunchers = new Map();
    // The pins of the distribution's own panels are dropped with them.
    if (!rebuildAll) {
        managed.forEach(panel => {
            const role = readMarker(panel, "Role", "");
            const screen = Number(readMarker(panel, "Screen", panel.screen));
            // Plasma keeps the panels of an unplugged display, and Plasma 6.8
            // crashes when such a panel is changed, so they are left alone.
            if (screen < 0 || screen >= screenCount) {
                return;
            }
            const key = `${role}:${screen}`;
            if ((role === "top" || role === "dock")
                && readMarker(panel, "LayoutVersion", "") === layoutVersion
                && !retained.has(key)) {
                retained.set(key, panel);
            } else {
                if (role === "dock" && !oldLaunchers.has(screen)) {
                    oldLaunchers.set(screen, launchersOf(panel));
                }
                stale.push(panel);
            }
        });
    }

    const created = [];
    try {
        for (let screen = 0; screen < screenCount; screen += 1) {
            let top = retained.get(`top:${screen}`);
            if (!top) {
                top = new Panel();
                created.push(top);
                fillTop(top);
            } else if (!hasWidgets(top, topWidgetTypes)) {
                fillTop(top);
            }
            configureTop(top, screen);
            configureTray(top);

            let dock = retained.get(`dock:${screen}`);
            let tasks;
            if (!dock) {
                dock = new Panel();
                created.push(dock);
                tasks = fillDock(dock, oldLaunchers.has(screen) ? withOverview(withHerdr(oldLaunchers.get(screen))) : defaultLaunchers);
            } else if (!hasWidgets(dock, dockWidgetTypes)) {
                tasks = fillDock(dock, launchersOf(dock));
            } else {
                tasks = tasksOf(dock);
            }
            const launchers = launchersIn(tasks);
            configureTasks(tasks, ensureOverview ? withOverview(launchers) : launchers);
            configureDock(dock, screen);
        }

        (rebuildAll ? initialPanels : stale).forEach(panel => panel.remove());

        if (rebuildAll) {
            desktops().forEach(desktop => {
                desktop.wallpaperPlugin = "org.kde.image";
                desktop.currentConfigGroup = ["Wallpaper", "org.kde.image", "General"];
                desktop.writeConfig("Image", wallpaper);
            });
        }

        print(`INAYOS_PLASMA_LAYOUT=ok:version=${layoutVersion}:screens=${screenCount}`);
    } catch (error) {
        created.forEach(panel => panel.remove());
        print(`INAYOS_PLASMA_LAYOUT=error:${error}`);
        fallBack();
    }
}

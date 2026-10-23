// Raising this version rebuilds the bars that an older layout made.
const layoutVersion = "1";
// The layout command says whether it has applied this layout before, and
// whether the installer asks to pin the Overview icon again.
const firstRun = typeof myconfigFirstRun === "undefined" ? true : myconfigFirstRun;
const ensureOverview = typeof myconfigEnsureOverview === "undefined" ? false : myconfigEnsureOverview;

const overviewLauncher = "applications:myconfig-overview.desktop";
const wallpaper = "/usr/share/wallpapers/Next/";
const topWidgetTypes = [
    "myconfig.session",
    "org.kde.plasma.appmenu",
    "org.kde.plasma.panelspacer",
    "org.kde.plasma.systemtray",
    "org.kde.plasma.digitalclock",
];
const dockWidgetTypes = [
    "org.kde.plasma.kickerdash",
    "org.kde.plasma.icontasks",
];

const missingWidgetTypes = topWidgetTypes.concat(dockWidgetTypes).filter(type => !knownWidgetTypes.includes(type));
if (missingWidgetTypes.length > 0) {
    print(`MYCONFIG_PLASMA_LAYOUT=missing:${missingWidgetTypes.join(",")}`);
} else {
    function readPanel(panel, key, fallback) {
        panel.currentConfigGroup = [];
        return String(panel.readConfig(key, fallback));
    }

    function markPanel(panel, role, screen) {
        panel.currentConfigGroup = [];
        panel.writeConfig("myconfigRole", role);
        panel.writeConfig("myconfigScreen", screen);
        panel.writeConfig("myconfigLayoutVersion", layoutVersion);
    }

    function hasWidgets(panel, types) {
        const widgets = panel.widgets();
        return widgets.length === types.length && widgets.every((widget, index) => widget.type === types[index]);
    }

    function tasksOf(panel) {
        return panel.widgets().find(widget => widget.type === "org.kde.plasma.icontasks");
    }

    function launchersOf(panel) {
        const tasks = tasksOf(panel);
        if (!tasks) {
            return [];
        }
        tasks.currentConfigGroup = ["General"];
        return tasks.readConfig("launchers", []);
    }

    function withOverview(launchers) {
        return launchers.includes(overviewLauncher) ? launchers : [overviewLauncher].concat(launchers);
    }

    function configureLauncher(launcher) {
        launcher.currentConfigGroup = ["General"];
        launcher.writeConfig("alphaSort", true);
        launcher.writeConfig("highlightNewlyInstalledApps", false);
        launcher.writeConfig("showRecentApps", false);
        launcher.writeConfig("showRecentDocs", false);
    }

    function configureTasks(tasks, launchers) {
        tasks.currentConfigGroup = ["General"];
        tasks.writeConfig("launchers", launchers);
        tasks.writeConfig("showOnlyCurrentScreen", true);
        tasks.writeConfig("showOnlyCurrentActivity", true);
        tasks.writeConfig("showOnlyCurrentDesktop", false);
        tasks.writeConfig("fill", false);
    }

    function fillTop(panel) {
        panel.widgets().slice().forEach(widget => widget.remove());
        topWidgetTypes.forEach(type => panel.addWidget(type));
    }

    function fillDock(panel, launchers) {
        panel.widgets().slice().forEach(widget => widget.remove());
        configureLauncher(panel.addWidget("org.kde.plasma.kickerdash"));
        panel.addWidget("org.kde.plasma.icontasks");
        configureTasks(tasksOf(panel), launchers);
    }

    function configureTop(panel, screen) {
        panel.screen = screen;
        panel.location = "top";
        panel.alignment = "left";
        panel.lengthMode = "fill";
        panel.height = 32;
        panel.hiding = "autohide";
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
    const managed = initialPanels.filter(panel => readPanel(panel, "myconfigRole", "") !== "");
    // The first run replaces every panel, such as the ones an earlier theme made.
    const rebuildAll = firstRun && managed.length === 0;
    const retained = new Map();
    const stale = [];
    const oldLaunchers = new Map();
    if (rebuildAll) {
        initialPanels.forEach(panel => {
            if (!oldLaunchers.has(panel.screen) && tasksOf(panel)) {
                oldLaunchers.set(panel.screen, launchersOf(panel));
            }
        });
    } else {
        managed.forEach(panel => {
            const role = readPanel(panel, "myconfigRole", "");
            const screen = Number(readPanel(panel, "myconfigScreen", panel.screen));
            // Plasma keeps the panels of an unplugged display, and Plasma 6.8
            // crashes when such a panel is changed, so they are left alone.
            if (screen < 0 || screen >= screenCount) {
                return;
            }
            const key = `${role}:${screen}`;
            if ((role === "top" || role === "dock")
                && readPanel(panel, "myconfigLayoutVersion", "") === layoutVersion
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

            let dock = retained.get(`dock:${screen}`);
            if (!dock) {
                dock = new Panel();
                created.push(dock);
                fillDock(dock, withOverview(oldLaunchers.get(screen) || []));
            } else if (!hasWidgets(dock, dockWidgetTypes)) {
                fillDock(dock, launchersOf(dock));
            }
            configureLauncher(dock.widgets()[0]);
            const launchers = launchersOf(dock);
            configureTasks(tasksOf(dock), ensureOverview ? withOverview(launchers) : launchers);
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

        print(`MYCONFIG_PLASMA_LAYOUT=ok:version=${layoutVersion}:screens=${screenCount}`);
    } catch (error) {
        created.forEach(panel => panel.remove());
        print(`MYCONFIG_PLASMA_LAYOUT=error:${error}`);
    }
}

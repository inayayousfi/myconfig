const requiredWidgetTypes = [
    "myconfig.island",
    "org.kde.plasma.systemmonitor.cpu",
    "org.kde.plasma.systemmonitor.memory",
    "org.kde.plasma.systemmonitor.net",
    "org.kde.plasma.calendar",
    "org.kde.plasma.notifications",
    "org.kde.plasma.systemtray",
    "org.kde.plasma.kickerdash",
    "org.kde.plasma.icontasks",
    "myconfig.overview",
];

const missingWidgetTypes = requiredWidgetTypes.filter(type => !knownWidgetTypes.includes(type));
if (missingWidgetTypes.length > 0) {
    print(`MYCONFIG_STATUS=missing:${missingWidgetTypes.join(",")}`);
} else {
    function markPanel(panel, role, screen) {
        panel.currentConfigGroup = [];
        panel.writeConfig("myconfigManaged", "true");
        panel.writeConfig("myconfigRole", role);
        panel.writeConfig("myconfigScreen", screen);
        panel.writeConfig("myconfigLayoutVersion", myconfigLayoutVersion);
    }

    function panelRole(panel) {
        panel.currentConfigGroup = [];
        if (String(panel.readConfig("myconfigManaged", "false")) !== "true") {
            return "";
        }
        return String(panel.readConfig("myconfigRole", ""));
    }

    function panelVersion(panel) {
        panel.currentConfigGroup = [];
        return String(panel.readConfig("myconfigLayoutVersion", ""));
    }

    function panelScreen(panel) {
        panel.currentConfigGroup = [];
        return Number(panel.readConfig("myconfigScreen", panel.screen));
    }

    function addTopPanelWidgets(panel) {
        panel.addWidget("myconfig.island");
    }

    function configureTaskManager(tasks) {
        tasks.currentConfigGroup = ["General"];
        tasks.writeConfig("showOnlyCurrentDesktop", false);
        tasks.writeConfig("fill", false);
    }

    function configureLauncher(launcher) {
        launcher.currentConfigGroup = ["General"];
        launcher.writeConfig("alphaSort", true);
        launcher.writeConfig("highlightNewlyInstalledApps", false);
        launcher.writeConfig("showRecentApps", false);
        launcher.writeConfig("showRecentDocs", false);
    }

    function addDockWidgets(panel, launchers = []) {
        configureLauncher(panel.addWidget("org.kde.plasma.kickerdash"));
        panel.addWidget("myconfig.overview");

        const tasks = panel.addWidget("org.kde.plasma.icontasks");
        tasks.currentConfigGroup = ["General"];
        tasks.writeConfig("launchers", launchers);
        tasks.writeConfig("showOnlyCurrentScreen", true);
        tasks.writeConfig("showOnlyCurrentActivity", true);
        configureTaskManager(tasks);
    }

    function replacePanelWidgets(panel, role) {
        const widgets = panel.widgets();
        if (role === "top" && widgets.length === 1 && widgets[0].type === "myconfig.island") {
            return;
        }
        let launchers = [];
        if (role === "dock") {
            const tasks = panel.widgets().find(widget => widget.type === "org.kde.plasma.icontasks");
            if (tasks) {
                tasks.currentConfigGroup = ["General"];
                launchers = tasks.readConfig("launchers", []);
            }
        }
        panel.widgets().slice().forEach(widget => widget.remove());
        if (role === "top") {
            addTopPanelWidgets(panel);
        } else {
            addDockWidgets(panel, launchers);
        }
    }

    function configureTopPanel(panel, screen) {
        panel.screen = screen;
        panel.location = "top";
        panel.alignment = "center";
        panel.lengthMode = "fit";
        panel.height = 68;
        panel.hiding = "autohide";
        panel.floating = false;
        panel.opacity = "adaptive";
        markPanel(panel, "top", screen);
    }

    function configureDock(panel, screen) {
        panel.screen = screen;
        panel.location = "bottom";
        panel.alignment = "center";
        panel.lengthMode = "fit";
        panel.height = 47;
        panel.hiding = "autohide";
        panel.floating = true;
        panel.opacity = "adaptive";
        panel.widgets().forEach(widget => {
            if (widget.type === "org.kde.plasma.kickerdash") {
                configureLauncher(widget);
            } else if (widget.type === "org.kde.plasma.icontasks") {
                configureTaskManager(widget);
            }
        });
        markPanel(panel, "dock", screen);
    }

    const initialPanels = panels();
    const existingManaged = initialPanels.filter(panel => panelRole(panel) !== "");
    const destructiveFirstRun = myconfigFirstRun && existingManaged.length === 0;
    const managed = destructiveFirstRun ? [] : existingManaged;
    const retained = new Map();
    const stale = [];
    const staleDockLaunchers = new Map();
    managed.forEach(panel => {
        const role = panelRole(panel);
        const screen = panelScreen(panel);
        if (role === "dock") {
            panel.widgets().forEach(widget => {
                if (widget.type === "org.kde.plasma.icontasks") {
                    configureTaskManager(widget);
                }
            });
        }
        if (screen < 0 || screen >= screenCount) {
            return;
        }
        const key = `${role}:${screen}`;
        if ((role !== "top" && role !== "dock") || panelVersion(panel) !== myconfigLayoutVersion || retained.has(key)) {
            if (role === "dock" && !staleDockLaunchers.has(screen)) {
                const tasks = panel.widgets().find(widget => widget.type === "org.kde.plasma.icontasks");
                if (tasks) {
                    tasks.currentConfigGroup = ["General"];
                    staleDockLaunchers.set(screen, tasks.readConfig("launchers", []));
                }
            }
            stale.push(panel);
        } else {
            retained.set(key, panel);
        }
    });

    const created = [];
    try {
        for (let screen = 0; screen < screenCount; screen += 1) {
            const topKey = `top:${screen}`;
            let top = retained.get(topKey);
            if (!top) {
                top = new Panel();
                created.push(top);
                addTopPanelWidgets(top);
            } else {
                replacePanelWidgets(top, "top");
            }
            configureTopPanel(top, screen);

            const dockKey = `dock:${screen}`;
            let dock = retained.get(dockKey);
            if (!dock) {
                dock = new Panel();
                created.push(dock);
                addDockWidgets(dock, staleDockLaunchers.get(screen) || []);
            } else {
                replacePanelWidgets(dock, "dock");
            }
            configureDock(dock, screen);
        }

        if (destructiveFirstRun) {
            initialPanels.forEach(panel => panel.remove());
        } else {
            stale.forEach(panel => panel.remove());
        }

        print(`MYCONFIG_STATUS=ok:screens=${screenCount}`);
    } catch (error) {
        created.forEach(panel => panel.remove());
        print(`MYCONFIG_STATUS=error:${error}`);
    }
}

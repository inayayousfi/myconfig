const requiredWidgetTypes = [
    "org.kde.plasma.panelspacer",
    "org.kde.plasma.digitalclock",
    "org.kde.plasma.systemtray",
    "org.kde.plasma.kickoff",
    "org.kde.plasma.icontasks",
    "myconfig.overview",
    "myconfig.session",
    "myconfig.power",
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
        panel.addWidget("myconfig.overview");
        panel.addWidget("org.kde.plasma.panelspacer");

        const clock = panel.addWidget("org.kde.plasma.digitalclock");
        clock.currentConfigGroup = ["Appearance"];
        clock.writeConfig("showDate", true);
        clock.writeConfig("dateFormat", "shortDate");
        clock.writeConfig("timeFormat", "default");
        clock.writeConfig("use24hFormat", 1);

        panel.addWidget("org.kde.plasma.panelspacer");
        panel.addWidget("org.kde.plasma.systemtray");
        panel.addWidget("myconfig.session");
        panel.addWidget("myconfig.power");
    }

    function addDockWidgets(panel, launchers = []) {
        panel.addWidget("org.kde.plasma.kickoff");

        const tasks = panel.addWidget("org.kde.plasma.icontasks");
        tasks.currentConfigGroup = ["General"];
        tasks.writeConfig("launchers", launchers);
        tasks.writeConfig("showOnlyCurrentScreen", true);
        tasks.writeConfig("showOnlyCurrentDesktop", true);
        tasks.writeConfig("showOnlyCurrentActivity", true);
        tasks.writeConfig("fill", false);
    }

    function configureTopPanel(panel, screen) {
        const geometry = screenGeometry(screen);
        panel.screen = screen;
        panel.location = "top";
        panel.alignment = "center";
        panel.lengthMode = "fill";
        panel.height = Math.round(geometry.height * 0.045);
        panel.hiding = "autohide";
        panel.floating = false;
        panel.opacity = "adaptive";
        markPanel(panel, "top", screen);
    }

    function configureDock(panel, screen) {
        const geometry = screenGeometry(screen);
        panel.screen = screen;
        panel.location = "bottom";
        panel.alignment = "center";
        panel.lengthMode = "fit";
        panel.height = Math.round(geometry.height * 0.06);
        panel.hiding = "autohide";
        panel.floating = true;
        panel.opacity = "adaptive";
        panel.widgets().forEach(widget => {
            if (widget.type === "org.kde.plasma.icontasks") {
                widget.currentConfigGroup = ["General"];
                widget.writeConfig("fill", false);
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
            }
            configureTopPanel(top, screen);

            const dockKey = `dock:${screen}`;
            let dock = retained.get(dockKey);
            if (!dock) {
                dock = new Panel();
                created.push(dock);
                addDockWidgets(dock, staleDockLaunchers.get(screen) || []);
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

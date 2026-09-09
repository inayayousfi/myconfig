#!/usr/bin/env node

const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

class Widget {
    constructor(type) {
        this.type = type;
        this.config = new Map();
    }

    readConfig(key, fallback) {
        return this.config.has(key) ? this.config.get(key) : fallback;
    }

    writeConfig(key, value) {
        this.config.set(key, value);
    }

    remove() {
        this.removed = true;
    }
}

const createdPanels = [];
class Panel {
    constructor() {
        this.config = new Map();
        this.panelWidgets = [];
        this.removed = false;
        createdPanels.push(this);
    }

    addWidget(type) {
        const widget = new Widget(type);
        this.panelWidgets.push(widget);
        return widget;
    }

    widgets() {
        return this.panelWidgets.filter(widget => !widget.removed);
    }

    readConfig(key, fallback) {
        return this.config.has(key) ? this.config.get(key) : fallback;
    }

    writeConfig(key, value) {
        this.config.set(key, value);
    }

    remove() {
        this.removed = true;
    }
}

const staleDock = new Panel();
staleDock.screen = 0;
staleDock.writeConfig("myconfigManaged", "true");
staleDock.writeConfig("myconfigRole", "dock");
staleDock.writeConfig("myconfigScreen", 0);
staleDock.writeConfig("myconfigLayoutVersion", "1");
const staleTasks = staleDock.addWidget("org.kde.plasma.icontasks");
staleTasks.writeConfig("launchers", ["applications:org.kde.dolphin.desktop"]);

const layout = fs.readFileSync(process.argv[2], "utf8");
const knownWidgetTypes = [
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

function runLayout(initialPanels, version, options = {}) {
    const output = [];
    const createdPanelCount = createdPanels.length;
    vm.runInNewContext(layout, {
        Map,
        Number,
        Panel,
        String,
        knownWidgetTypes,
        myconfigFirstRun: options.firstRun || false,
        myconfigLayoutVersion: version,
        panels: () => initialPanels,
        print: value => output.push(value),
        screenCount: options.screenCount || 1,
        screenGeometry: () => ({x: 0, y: 0, width: 1920, height: 1080}),
    });
    return {created: createdPanels.slice(createdPanelCount), output};
}

const upgrade = runLayout([staleDock], "4");

assert.equal(staleDock.removed, true, "stale dock was not replaced");
const replacementTop = upgrade.created.find(panel => panel.readConfig("myconfigRole", "") === "top");
assert.ok(replacementTop, "replacement top panel was not created");
assert.equal(replacementTop.height, 44, "replacement top panel has the wrong height");
assert.equal(replacementTop.lengthMode, "fit", "top panel does not fit the island");
assert.deepEqual(
    replacementTop.widgets().map(widget => widget.type),
    ["myconfig.island"],
    "replacement top panel has the wrong controls",
);
const replacementDock = upgrade.created.find(panel => panel.readConfig("myconfigRole", "") === "dock");
assert.ok(replacementDock, "replacement dock was not created");
assert.equal(replacementDock.height, 47, "replacement dock has the wrong height");
assert.equal(replacementDock.lengthMode, "fit", "replacement dock does not fit its content");
assert.deepEqual(
    replacementDock.widgets().map(widget => widget.type),
    ["org.kde.plasma.kickerdash", "myconfig.overview", "org.kde.plasma.icontasks"],
    "replacement dock does not place Overview next to the application dashboard",
);
const replacementLauncher = replacementDock.widgets().find(widget => widget.type === "org.kde.plasma.kickerdash");
assert.ok(replacementLauncher, "replacement dock does not use the full-screen application dashboard");
assert.equal(replacementLauncher.readConfig("alphaSort", false), true, "application dashboard is not alphabetical");
assert.equal(replacementLauncher.readConfig("showRecentApps", true), false, "application dashboard shows recent apps");
assert.equal(replacementLauncher.readConfig("showRecentDocs", true), false, "application dashboard shows recent documents");
assert.equal(
    replacementLauncher.readConfig("highlightNewlyInstalledApps", true),
    false,
    "application dashboard highlights newly installed apps",
);
const replacementTasks = replacementDock.widgets().find(widget => widget.type === "org.kde.plasma.icontasks");
assert.deepEqual(
    replacementTasks.readConfig("launchers", []),
    ["applications:org.kde.dolphin.desktop"],
    "replacement dock lost its manual launchers",
);
assert.equal(
    replacementTasks.readConfig("showOnlyCurrentDesktop", true),
    false,
    "replacement task manager hides windows from other virtual desktops",
);
assert.equal(replacementTasks.readConfig("fill", true), false, "replacement task manager fills the dock");
assert.match(upgrade.output.at(-1), /MYCONFIG_STATUS=ok:screens=1/);

const disconnectedTop = new Panel();
disconnectedTop.screen = -1;
disconnectedTop.writeConfig("myconfigManaged", "true");
disconnectedTop.writeConfig("myconfigRole", "top");
disconnectedTop.writeConfig("myconfigScreen", 0);
disconnectedTop.writeConfig("myconfigLayoutVersion", "4");
const disconnectedDock = new Panel();
disconnectedDock.screen = -1;
disconnectedDock.writeConfig("myconfigManaged", "true");
disconnectedDock.writeConfig("myconfigRole", "dock");
disconnectedDock.writeConfig("myconfigScreen", 0);
disconnectedDock.writeConfig("myconfigLayoutVersion", "4");
disconnectedDock.addWidget("org.kde.plasma.kickerdash");
disconnectedDock.addWidget("myconfig.overview");
disconnectedDock.addWidget("org.kde.plasma.icontasks");

const reconnect = runLayout([disconnectedTop, disconnectedDock], "4");
assert.equal(disconnectedTop.removed, false, "temporarily unassigned top panel was replaced");
assert.equal(disconnectedDock.removed, false, "temporarily unassigned dock was replaced");
assert.equal(disconnectedTop.screen, 0, "top panel did not return to its intended screen");
assert.equal(disconnectedDock.screen, 0, "dock did not return to its intended screen");
assert.equal(reconnect.created.length, 0, "reconnection created duplicate managed panels");

const unrelated = new Panel();
const managedTop = new Panel();
managedTop.screen = 0;
managedTop.writeConfig("myconfigManaged", "true");
managedTop.writeConfig("myconfigRole", "top");
managedTop.writeConfig("myconfigScreen", 0);
managedTop.writeConfig("myconfigLayoutVersion", "4");
const managedClock = managedTop.addWidget("org.kde.plasma.digitalclock");
const extraDiskWidget = managedTop.addWidget("org.kde.plasma.systemmonitor.diskactivity");
const managedDock = new Panel();
managedDock.screen = 0;
managedDock.writeConfig("myconfigManaged", "true");
managedDock.writeConfig("myconfigRole", "dock");
managedDock.writeConfig("myconfigScreen", 0);
managedDock.writeConfig("myconfigLayoutVersion", "4");
const managedLauncher = managedDock.addWidget("org.kde.plasma.kickerdash");
managedLauncher.writeConfig("alphaSort", false);
managedLauncher.writeConfig("showRecentApps", true);
managedLauncher.writeConfig("showRecentDocs", true);
managedDock.addWidget("myconfig.overview");
const managedTasks = managedDock.addWidget("org.kde.plasma.icontasks");
managedTasks.writeConfig("showOnlyCurrentDesktop", true);

runLayout([unrelated, managedTop, managedDock], "4", {firstRun: true});
assert.equal(unrelated.removed, false, "missing layout state deleted an unrelated panel");
assert.equal(managedTop.removed, false, "missing layout state replaced an existing managed top panel");
assert.equal(extraDiskWidget.removed, true, "extra disk widget was retained");
assert.equal(managedClock.removed, true, "legacy clock was not replaced");
assert.deepEqual(managedTop.widgets().map(widget => widget.type), ["myconfig.island"], "legacy top panel was not migrated in place");
assert.equal(managedDock.removed, false, "missing layout state replaced an existing managed dock");
const updatedLauncher = managedDock.widgets().find(widget => widget.type === "org.kde.plasma.kickerdash");
assert.equal(updatedLauncher.readConfig("alphaSort", false), true, "retained dashboard is not alphabetical");
assert.equal(updatedLauncher.readConfig("showRecentApps", true), false, "retained dashboard shows recent apps");
assert.equal(updatedLauncher.readConfig("showRecentDocs", true), false, "retained dashboard shows recent documents");
assert.equal(
    managedTasks.readConfig("showOnlyCurrentDesktop", true),
    false,
    "retained task manager hides windows from other virtual desktops",
);

const retainedIsland = managedTop.widgets()[0];
retainedIsland.writeConfig("savedWidgetState", "keep");
runLayout([unrelated, managedTop, managedDock], "4");
assert.equal(managedTop.widgets()[0], retainedIsland, "reconciliation recreated the island");
assert.equal(retainedIsland.readConfig("savedWidgetState", ""), "keep", "reconciliation lost island widget state");
assert.equal(managedTop.widgets().length, 1, "reconciliation duplicated the island");

const secondDisplay = runLayout([managedTop, managedDock], "4", {screenCount: 2});
const secondTop = secondDisplay.created.find(panel => panel.screen === 1 && panel.readConfig("myconfigRole", "") === "top");
assert.deepEqual(secondTop.widgets().map(widget => widget.type), ["myconfig.island"], "new display did not receive an island");
assert.equal(managedTop.widgets()[0], retainedIsland, "adding a display recreated the first island");

const missingCalendar = knownWidgetTypes.indexOf("org.kde.plasma.calendar");
knownWidgetTypes.splice(missingCalendar, 1);
const missingWidget = runLayout([managedTop, managedDock], "4");
assert.equal(missingWidget.created.length, 0, "missing island dependency changed the panels");
assert.match(missingWidget.output[0], /MYCONFIG_STATUS=missing:org.kde.plasma.calendar/);
knownWidgetTypes.splice(missingCalendar, 0, "org.kde.plasma.calendar");

const outdatedDisconnectedDock = new Panel();
outdatedDisconnectedDock.screen = -1;
outdatedDisconnectedDock.writeConfig("myconfigManaged", "true");
outdatedDisconnectedDock.writeConfig("myconfigRole", "dock");
outdatedDisconnectedDock.writeConfig("myconfigScreen", 1);
outdatedDisconnectedDock.writeConfig("myconfigLayoutVersion", "1");
const outdatedDisconnectedTasks = outdatedDisconnectedDock.addWidget("org.kde.plasma.icontasks");
outdatedDisconnectedTasks.writeConfig("launchers", ["applications:org.kde.kate.desktop"]);
outdatedDisconnectedTasks.writeConfig("showOnlyCurrentDesktop", true);

runLayout([outdatedDisconnectedDock], "4");
assert.equal(outdatedDisconnectedDock.removed, false, "outdated dock for a disconnected display was removed");
assert.deepEqual(
    outdatedDisconnectedTasks.readConfig("launchers", []),
    ["applications:org.kde.kate.desktop"],
    "outdated disconnected dock lost its launchers",
);
assert.equal(
    outdatedDisconnectedTasks.readConfig("showOnlyCurrentDesktop", true),
    false,
    "disconnected task manager hides windows from other virtual desktops",
);

console.log("KDE Plasma layout tests passed.");

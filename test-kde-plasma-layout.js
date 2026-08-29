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
        return this.panelWidgets;
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
    "org.kde.plasma.panelspacer",
    "org.kde.plasma.digitalclock",
    "org.kde.plasma.systemtray",
    "org.kde.plasma.kickoff",
    "org.kde.plasma.icontasks",
    "myconfig.overview",
    "myconfig.session",
    "myconfig.power",
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

const upgrade = runLayout([staleDock], "2");

assert.equal(staleDock.removed, true, "stale dock was not replaced");
const replacementTop = upgrade.created.find(panel => panel.readConfig("myconfigRole", "") === "top");
assert.ok(replacementTop, "replacement top panel was not created");
assert.equal(replacementTop.height, 34, "replacement top panel has the wrong height");
const replacementClock = replacementTop.widgets().find(widget => widget.type === "org.kde.plasma.digitalclock");
assert.equal(replacementClock.readConfig("dateDisplayFormat", 1), 0, "replacement clock does not use its adaptive layout");
assert.equal(replacementClock.readConfig("autoFontAndSize", false), true, "replacement clock does not size its font automatically");
assert.deepEqual(
    replacementTop.widgets().map(widget => widget.type),
    [
        "myconfig.overview",
        "org.kde.plasma.panelspacer",
        "org.kde.plasma.digitalclock",
        "org.kde.plasma.panelspacer",
        "org.kde.plasma.systemtray",
        "myconfig.session",
        "myconfig.power",
    ],
    "replacement top panel has the wrong controls",
);
const replacementDock = upgrade.created.find(panel => panel.readConfig("myconfigRole", "") === "dock");
assert.ok(replacementDock, "replacement dock was not created");
assert.equal(replacementDock.height, 47, "replacement dock has the wrong height");
assert.equal(replacementDock.lengthMode, "fit", "replacement dock does not fit its content");
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
disconnectedTop.writeConfig("myconfigLayoutVersion", "2");
const disconnectedDock = new Panel();
disconnectedDock.screen = -1;
disconnectedDock.writeConfig("myconfigManaged", "true");
disconnectedDock.writeConfig("myconfigRole", "dock");
disconnectedDock.writeConfig("myconfigScreen", 0);
disconnectedDock.writeConfig("myconfigLayoutVersion", "2");
disconnectedDock.addWidget("org.kde.plasma.kickoff");
disconnectedDock.addWidget("org.kde.plasma.icontasks");

const reconnect = runLayout([disconnectedTop, disconnectedDock], "2");
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
managedTop.writeConfig("myconfigLayoutVersion", "2");
const managedClock = managedTop.addWidget("org.kde.plasma.digitalclock");
const managedDock = new Panel();
managedDock.screen = 0;
managedDock.writeConfig("myconfigManaged", "true");
managedDock.writeConfig("myconfigRole", "dock");
managedDock.writeConfig("myconfigScreen", 0);
managedDock.writeConfig("myconfigLayoutVersion", "2");
const managedTasks = managedDock.addWidget("org.kde.plasma.icontasks");
managedTasks.writeConfig("showOnlyCurrentDesktop", true);

runLayout([unrelated, managedTop, managedDock], "2", {firstRun: true});
assert.equal(unrelated.removed, false, "missing layout state deleted an unrelated panel");
assert.equal(managedTop.removed, false, "missing layout state replaced an existing managed top panel");
assert.equal(managedClock.readConfig("dateDisplayFormat", 1), 0, "retained clock does not use its adaptive layout");
assert.equal(managedClock.readConfig("autoFontAndSize", false), true, "retained clock does not size its font automatically");
assert.equal(managedDock.removed, false, "missing layout state replaced an existing managed dock");
assert.equal(
    managedTasks.readConfig("showOnlyCurrentDesktop", true),
    false,
    "retained task manager hides windows from other virtual desktops",
);

const outdatedDisconnectedDock = new Panel();
outdatedDisconnectedDock.screen = -1;
outdatedDisconnectedDock.writeConfig("myconfigManaged", "true");
outdatedDisconnectedDock.writeConfig("myconfigRole", "dock");
outdatedDisconnectedDock.writeConfig("myconfigScreen", 1);
outdatedDisconnectedDock.writeConfig("myconfigLayoutVersion", "1");
const outdatedDisconnectedTasks = outdatedDisconnectedDock.addWidget("org.kde.plasma.icontasks");
outdatedDisconnectedTasks.writeConfig("launchers", ["applications:org.kde.kate.desktop"]);
outdatedDisconnectedTasks.writeConfig("showOnlyCurrentDesktop", true);

runLayout([outdatedDisconnectedDock], "2");
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

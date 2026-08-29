#!/usr/bin/env node

const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

class Signal {
    constructor() {
        this.handlers = [];
    }

    connect(handler) {
        this.handlers.push(handler);
    }

    emit(...args) {
        this.handlers.forEach(handler => handler(...args));
    }
}

const timers = [];
class Timer {
    constructor() {
        this.interval = 0;
        this.singleShot = false;
        this.active = false;
        this.timeout = new Signal();
        timers.push(this);
    }

    start() {
        this.active = true;
    }

    stop() {
        this.active = false;
    }

    fire() {
        assert.equal(this.active, true, "attempted to fire an inactive timer");
        if (this.singleShot) {
            this.active = false;
        }
        this.timeout.emit();
    }
}

function output(name, x, y, width, height) {
    return {name, geometry: {x, y, width, height}};
}

const left = output("left", 0, 0, 1920, 1080);
const right = output("right", 1920, 0, 1024, 768);
const cursorPosChanged = new Signal();
const screensChanged = new Signal();
const screenOrderChanged = new Signal();
const currentDesktopChanged = new Signal();
const windowAdded = new Signal();
let overviewActive = false;
const workspace = {
    cursorPos: {x: 500, y: 500},
    screens: [left, right],
    cursorPosChanged,
    screensChanged,
    screenOrderChanged,
    currentDesktopChanged,
    windowAdded,
    windows: [],
    windowList() {
        return this.windows;
    },
    isEffectActive(effect) {
        return effect === "overview" && overviewActive;
    },
};

let plasmaResponds = true;
const calls = [];
function callDBus(...args) {
    const callback = typeof args.at(-1) === "function" ? args.pop() : null;
    const call = {
        service: args[0],
        method: args[3],
        args: args.slice(4),
    };
    calls.push(call);

    if (call.service === "org.kde.plasmashell" && callback && plasmaResponds) {
        const mode = call.args[0].match(/const myconfigMode = "([^"]+)";/)[1];
        callback(`MYCONFIG_PANEL_MODE=${mode}:1`);
    }
}

const scriptPath = process.argv[2];
assert.ok(scriptPath, "KWin script path is required");
vm.runInNewContext(fs.readFileSync(scriptPath, "utf8"), {
    Map,
    Set,
    JSON,
    Math,
    String,
    QTimer: Timer,
    workspace,
    callDBus,
});

function plasmaCalls() {
    return calls.filter(call => call.service === "org.kde.plasmashell");
}

function latestPlasmaScript() {
    return plasmaCalls().at(-1).args[0];
}

function activeTimer(interval) {
    return timers.find(timer => timer.interval === interval && timer.active);
}

function mockWindow({fullScreen = false, keepBelow = false, resourceClass = "", skipTaskbar = false} = {}) {
    return {
        fullScreen,
        keepBelow,
        resourceClass,
        skipTaskbar,
        fullScreenChanged: new Signal(),
        closed: new Signal(),
    };
}

workspace.cursorPos = {x: 12, y: 4};
cursorPosChanged.emit();
assert.match(latestPlasmaScript(), /const myconfigRole = "top";/);
assert.match(latestPlasmaScript(), /const myconfigMode = "windowsgobelow";/);

workspace.cursorPos = {x: 500, y: 500};
cursorPosChanged.emit();
activeTimer(400).fire();
assert.match(latestPlasmaScript(), /const myconfigMode = "autohide";/);

workspace.cursorPos = {x: 5, y: 1076};
cursorPosChanged.emit();
assert.match(latestPlasmaScript(), /const myconfigRole = "dock";/);
assert.match(latestPlasmaScript(), /const myconfigMode = "windowsgobelow";/);
assert.equal(
    timers.some(timer => timer.interval === 400 && timer.active),
    false,
    "dock started hiding while the pointer remained in its full-width activation zone",
);

workspace.windows = [{dock: true, hidden: false, output: left, frameGeometry: {x: 250, y: 1000, width: 200, height: 72}}];
workspace.cursorPos = {x: 300, y: 1020};
cursorPosChanged.emit();
assert.equal(
    timers.some(timer => timer.interval === 400 && timer.active),
    false,
    "dock started hiding while the pointer remained inside its live window geometry",
);

workspace.cursorPos = {x: 500, y: 500};
workspace.windows = [{appletPopup: true, hidden: false, output: left}];
cursorPosChanged.emit();
activeTimer(400).fire();
assert.doesNotMatch(latestPlasmaScript(), /const myconfigMode = "autohide";/);
workspace.windows = [];
activeTimer(400).fire();
assert.equal(activeTimer(400).active, true, "popup closure did not start a full hide delay");
activeTimer(400).fire();
assert.match(latestPlasmaScript(), /const myconfigMode = "autohide";/);

const systemdCallsBefore = calls.filter(call => call.service === "org.freedesktop.systemd1").length;
screensChanged.emit();
assert.equal(
    calls.filter(call => call.service === "org.freedesktop.systemd1").length,
    systemdCallsBefore + 1,
    "display change did not start the layout service immediately",
);
activeTimer(1000).fire();
assert.equal(
    calls.filter(call => call.service === "org.freedesktop.systemd1").length,
    systemdCallsBefore + 2,
    "display change did not retry layout reconciliation",
);

const effectCallsBefore = calls.filter(call => call.service === "org.kde.kglobalaccel").length;
currentDesktopChanged.emit();
assert.equal(
    calls.filter(call => call.service === "org.kde.kglobalaccel").length,
    effectCallsBefore,
    "desktop change toggled an inactive Overview",
);
overviewActive = true;
currentDesktopChanged.emit();
assert.deepEqual(calls.at(-1), {
    service: "org.kde.kglobalaccel",
    method: "invokeShortcut",
    args: ["Overview"],
});

const fullscreenWindow = mockWindow();
workspace.windows.push(fullscreenWindow);
windowAdded.emit(fullscreenWindow);
fullscreenWindow.fullScreen = true;
fullscreenWindow.fullScreenChanged.emit();
assert.equal(fullscreenWindow.keepBelow, true, "fullscreen window did not move below desktop overlays");
fullscreenWindow.fullScreen = false;
fullscreenWindow.fullScreenChanged.emit();
assert.equal(fullscreenWindow.keepBelow, false, "window's prior stacking state was not restored");

const alreadyBelowWindow = mockWindow({keepBelow: true});
workspace.windows.push(alreadyBelowWindow);
windowAdded.emit(alreadyBelowWindow);
alreadyBelowWindow.fullScreen = true;
alreadyBelowWindow.fullScreenChanged.emit();
alreadyBelowWindow.fullScreen = false;
alreadyBelowWindow.fullScreenChanged.emit();
assert.equal(alreadyBelowWindow.keepBelow, true, "existing Keep Below state was not preserved");

const dashboardWindow = mockWindow({
    fullScreen: true,
    resourceClass: "org.kde.plasmashell",
    skipTaskbar: true,
});
workspace.windows.push(dashboardWindow);
windowAdded.emit(dashboardWindow);
assert.equal(dashboardWindow.keepBelow, false, "full-screen application dashboard was moved below maximized windows");

plasmaResponds = false;
workspace.cursorPos = {x: 2000, y: 4};
cursorPosChanged.emit();
workspace.cursorPos = {x: 2200, y: 400};
cursorPosChanged.emit();
activeTimer(400).fire();
activeTimer(1000).fire();
assert.match(latestPlasmaScript(), /const myconfigMode = "autohide";/);

console.log("KWin panel automation tests passed.");

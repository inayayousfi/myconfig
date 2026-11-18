// Keeps the top bar visible unless its display shows a fullscreen window.
// Shows the dock, and the top bar over a fullscreen window, while the pointer
// is near their screen edge, and hides them again once the pointer and their
// menus are gone.
const activationDepth = 24;
const hideDelay = 400;
const visibleMode = "windowsgobelow";
const hiddenMode = "autohide";
const alwaysMode = "none";
const states = new Map();
const fullscreenKeepBelow = new Map();

function newTimer(interval, callback) {
    const timer = new QTimer();
    timer.interval = interval;
    timer.singleShot = true;
    timer.timeout.connect(callback);
    return timer;
}

function outputByName(name) {
    return workspace.screens.find(output => output.name === name);
}

function pointInRect(point, rect) {
    return point.x >= rect.x
        && point.x < rect.x + rect.width
        && point.y >= rect.y
        && point.y < rect.y + rect.height;
}

function stateKey(output, role) {
    return `${output.name}:${role}`;
}

function centerOf(output) {
    const geometry = output.geometry;
    return {
        x: geometry.x + Math.floor(geometry.width / 2),
        y: geometry.y + Math.floor(geometry.height / 2),
    };
}

function desiredMode(state) {
    return state.desiredVisible ? visibleMode : state.restMode;
}

function panelModeScript(state, mode) {
    return `
const myconfigRole = ${JSON.stringify(state.role)};
const myconfigMode = ${JSON.stringify(mode)};
const myconfigX = ${state.point.x};
const myconfigY = ${state.point.y};
// Plasma keeps the panels of a disconnected display on that display's number
// without a window. screenGeometry gives the primary display's geometry for
// such a number, and Plasma 6.8 crashes when a panel without a window on a
// missing display is changed, so those panels are left alone.
const myconfigPanels = panels().filter(panel => {
    panel.currentConfigGroup = [];
    if (String(panel.readConfig("myconfigRole", "")) !== myconfigRole
        || panel.screen < 0
        || panel.screen >= screenCount) {
        return false;
    }
    const geometry = screenGeometry(panel.screen);
    return myconfigX >= geometry.x
        && myconfigX < geometry.x + geometry.width
        && myconfigY >= geometry.y
        && myconfigY < geometry.y + geometry.height;
});
myconfigPanels.forEach(panel => {
    panel.hiding = myconfigMode;
});
print("MYCONFIG_PANEL_MODE=" + myconfigMode + ":" + myconfigPanels.length);
`;
}

function requestMode(state, mode) {
    if (state.pendingMode !== null || state.currentMode === mode) {
        return;
    }

    state.pendingMode = mode;
    state.requestTimer.start();
    callDBus(
        "org.kde.plasmashell",
        "/PlasmaShell",
        "org.kde.PlasmaShell",
        "evaluateScript",
        panelModeScript(state, mode),
        response => {
            if (state.pendingMode !== mode) {
                return;
            }

            state.requestTimer.stop();
            state.pendingMode = null;
            if (String(response).includes(`MYCONFIG_PANEL_MODE=${mode}:1`)) {
                state.currentMode = mode;
            }

            if (state.currentMode !== desiredMode(state)) {
                state.requestTimer.start();
            }
        },
    );
}

function createState(output, role) {
    const state = {
        outputName: output.name,
        role,
        point: centerOf(output),
        desiredVisible: false,
        restMode: hiddenMode,
        // The top bar's mode is unknown until the script sets it.
        currentMode: role === "top" ? null : hiddenMode,
        pendingMode: null,
        popupWasOpen: false,
        hideTimer: null,
        requestTimer: null,
    };

    state.hideTimer = newTimer(hideDelay, () => hideWhenReady(state));
    state.requestTimer = newTimer(1000, () => {
        state.pendingMode = null;
        state.currentMode = null;
        requestMode(state, desiredMode(state));
    });
    states.set(stateKey(output, role), state);
    return state;
}

function getState(output, role) {
    return states.get(stateKey(output, role)) || createState(output, role);
}

function onOutput(window, state) {
    return !window.hidden && window.output && window.output.name === state.outputName;
}

// Widget pop-ups and the global menu's menus belong to Plasma.
function popupOpen(state, windows = workspace.windowList()) {
    return windows.some(window => onOutput(window, state)
        && (window.appletPopup || (window.popupWindow && window.resourceClass === "org.kde.plasmashell")));
}

function pointerInActivationZone(state) {
    const output = outputByName(state.outputName);
    const point = workspace.cursorPos;
    if (!output || !pointInRect(point, output.geometry)) {
        return false;
    }

    const geometry = output.geometry;
    return state.role === "top"
        ? point.y < geometry.y + activationDepth
        : point.y >= geometry.y + geometry.height - activationDepth;
}

function pointerInPanel(state, windows = workspace.windowList()) {
    const output = outputByName(state.outputName);
    const point = workspace.cursorPos;
    if (!output || !pointInRect(point, output.geometry)) {
        return false;
    }

    const outputMiddle = output.geometry.y + output.geometry.height / 2;
    return windows.some(window => {
        if (!window.dock || !onOutput(window, state) || !pointInRect(point, window.frameGeometry)) {
            return false;
        }
        const panelMiddle = window.frameGeometry.y + window.frameGeometry.height / 2;
        return state.role === "top" ? panelMiddle < outputMiddle : panelMiddle >= outputMiddle;
    });
}

function hideWhenReady(state) {
    const windows = workspace.windowList();
    if (!state.desiredVisible) {
        return;
    }
    if (pointerInActivationZone(state) || pointerInPanel(state, windows)) {
        state.popupWasOpen = false;
        return;
    }
    if (popupOpen(state, windows)) {
        state.popupWasOpen = true;
        state.hideTimer.start();
        return;
    }
    // Give a closing menu one more delay before the bar hides under it.
    if (state.popupWasOpen) {
        state.popupWasOpen = false;
        state.hideTimer.start();
        return;
    }

    state.desiredVisible = false;
    requestMode(state, state.restMode);
}

function reveal(output, role) {
    const state = getState(output, role);
    if (state.restMode === alwaysMode) {
        return;
    }
    state.point = centerOf(output);
    state.desiredVisible = true;
    state.hideTimer.stop();
    requestMode(state, visibleMode);
}

function updatePointerState() {
    const point = workspace.cursorPos;
    const windows = workspace.windowList();
    const output = workspace.screens.find(candidate => pointInRect(point, candidate.geometry));
    if (output) {
        const geometry = output.geometry;
        if (point.y < geometry.y + activationDepth) {
            reveal(output, "top");
        }
        if (point.y >= geometry.y + geometry.height - activationDepth) {
            reveal(output, "dock");
        }
    }

    states.forEach(state => {
        if (!state.desiredVisible) {
            return;
        }
        if (pointerInActivationZone(state) || pointerInPanel(state, windows)) {
            state.popupWasOpen = false;
            state.hideTimer.stop();
        } else if (popupOpen(state, windows)) {
            state.popupWasOpen = true;
            if (!state.hideTimer.active) {
                state.hideTimer.start();
            }
        } else if (!state.hideTimer.active) {
            state.hideTimer.start();
        }
    });
}

function onCurrentDesktop(window) {
    return (window.onAllDesktops || window.desktops.includes(workspace.currentDesktop))
        && (window.activities.length === 0 || window.activities.includes(workspace.currentActivity));
}

function fullscreenOn(output) {
    return workspace.windowList().some(window => window.fullScreen
        && !window.minimized
        && !window.hidden
        && !isFullscreenException(window)
        && window.output
        && window.output.name === output.name
        && onCurrentDesktop(window));
}

function updateTopBars() {
    workspace.screens.forEach(output => {
        const state = getState(output, "top");
        const restMode = fullscreenOn(output) ? hiddenMode : alwaysMode;
        if (state.restMode === restMode && state.currentMode !== null) {
            return;
        }
        state.restMode = restMode;
        state.point = centerOf(output);
        if (restMode === alwaysMode) {
            state.desiredVisible = false;
            state.hideTimer.stop();
        }
        requestMode(state, desiredMode(state));
    });
}

// The layout command resets the bars, so their modes are set again after it.
const resyncTimer = newTimer(3000, () => {
    states.forEach(state => {
        state.currentMode = null;
        requestMode(state, desiredMode(state));
    });
});

function startLayoutUnit() {
    callDBus(
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
        "StartUnit",
        "myconfig-plasma-layout.service",
        "replace",
    );
}

// A display change runs the layout now and once more after the displays settle.
let layoutChangePending = false;
const layoutRetryTimer = newTimer(1000, () => {
    layoutChangePending = false;
    startLayoutUnit();
    resyncTimer.start();
});
function screensChanged() {
    const outputNames = new Set(workspace.screens.map(output => output.name));
    states.forEach((state, key) => {
        if (!outputNames.has(state.outputName)) {
            state.hideTimer.stop();
            state.requestTimer.stop();
            states.delete(key);
        }
    });
    if (!layoutChangePending) {
        layoutChangePending = true;
        startLayoutUnit();
    }
    layoutRetryTimer.start();
    updateTopBars();
}

function closeOverviewAfterDesktopSwitch() {
    if (workspace.isEffectActive("overview")) {
        callDBus(
            "org.kde.kglobalaccel",
            "/component/kwin",
            "org.kde.kglobalaccel.Component",
            "invokeShortcut",
            "Overview",
        );
    }
}

function isFullscreenException(window) {
    return window.resourceClass === "org.kde.spectacle"
        || (window.resourceClass === "org.kde.plasmashell" && window.skipTaskbar);
}

function updateFullscreenLayer(window) {
    if (window.fullScreen && !isFullscreenException(window)) {
        if (!fullscreenKeepBelow.has(window)) {
            fullscreenKeepBelow.set(window, window.keepBelow);
        }
        // Scripts cannot put fullscreen windows in KWin's normal layer. Keep Below is the
        // exposed override; replace it with normal-layer stacking if KWin adds that option.
        window.keepBelow = true;
    } else if (fullscreenKeepBelow.has(window)) {
        const keepBelow = fullscreenKeepBelow.get(window);
        fullscreenKeepBelow.delete(window);
        window.keepBelow = keepBelow;
    }
}

function watchWindow(window) {
    window.fullScreenChanged.connect(() => {
        updateFullscreenLayer(window);
        updateTopBars();
    });
    window.minimizedChanged.connect(updateTopBars);
    window.outputChanged.connect(updateTopBars);
    window.desktopsChanged.connect(updateTopBars);
    window.activitiesChanged.connect(updateTopBars);
    window.closed.connect(() => {
        fullscreenKeepBelow.delete(window);
        updateTopBars();
    });
    updateFullscreenLayer(window);
}

workspace.cursorPosChanged.connect(updatePointerState);
workspace.screensChanged.connect(screensChanged);
workspace.screenOrderChanged.connect(screensChanged);
workspace.currentDesktopChanged.connect(closeOverviewAfterDesktopSwitch);
workspace.currentDesktopChanged.connect(updateTopBars);
workspace.currentActivityChanged.connect(updateTopBars);
workspace.windowAdded.connect(watchWindow);
workspace.windowList().forEach(watchWindow);
updateTopBars();

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Window
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.kitemmodels as KItemModels
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasmoid
import org.kde.plasma.private.sessions
import org.kde.plasma.private.shell as Shell

ContainmentItem {
    id: island
    implicitWidth: pillWidth + 32
    implicitHeight: pillHeight
    Layout.minimumWidth: pillWidth + 32
    Layout.preferredWidth: pillWidth + 32
    Layout.maximumWidth: pillWidth + 32
    Layout.minimumHeight: pillHeight
    Layout.preferredHeight: pillHeight
    Layout.maximumHeight: pillHeight
    Layout.alignment: Qt.AlignCenter
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    Binding {
        target: Plasmoid.containment
        property: "backgroundHints"
        value: PlasmaCore.Types.NoBackground
        when: island.panelBackgroundReady && Plasmoid.containment && Plasmoid.containment.pluginName === "org.kde.panel"
        restoreMode: Binding.RestoreBindingOrValue
    }

    property string clockText: Qt.formatTime(new Date(), "hh:mm")
    property bool opened: false
    property bool panelBackgroundReady: false
    property real reveal: 0
    readonly property bool requestedVisible: Plasmoid.configuration.edgeVisible || opened || morph.running
    property real appearance: 0
    property bool appearancePending: false
    property bool windowFrameReady: false
    onRequestedVisibleChanged: {
        if (requestedVisible) {
            if (windowFrameReady) appearance = 1;
            else appearancePending = true;
        } else {
            appearancePending = false;
            appearance = 0;
        }
    }
    Behavior on appearance {
        SpringAnimation {
            id: appearanceMotion
            spring: 9
            damping: 0.42
            mass: 0.7
            epsilon: 0.001
        }
    }
    readonly property int pillWidth: Math.ceil(clockMetrics.advanceWidth) + 48
    readonly property int pillHeight: 36
    readonly property real actualPillHeight: pillHeight
    readonly property int openWidth: 680
    readonly property int openHeight: 536
    readonly property font clockFont: Qt.font({family: "Iosevka Nerd Font", pixelSize: 26, weight: Font.Normal})
    readonly property point clockPosition: Qt.point(popup.width / 2, 16)
    readonly property var panelWindow: fullRepresentationItem ? fullRepresentationItem.Window.window : null
    property int panelWindowFlags: 0
    onPanelWindowChanged: {
        if (panelWindow) panelWindowFlags = panelWindow.flags;
    }
    Binding {
        target: island.panelWindow
        property: "flags"
        value: island.panelWindowFlags | Qt.WindowTransparentForInput
        when: island.panelBackgroundReady && island.panelWindow !== null
        restoreMode: Binding.RestoreBindingOrValue
    }
    readonly property real overshoot: Math.max(0, reveal - 1)
    // Smoothly bound the stretch inside the popup without clipping the rebound.
    readonly property real stretch: overshoot / (1 + overshoot / 0.09)
    property var loadedWidgets: ({})
    property int currentPage: 0
    property real pagePosition: currentPage
    readonly property var pageNames: ["Materiel et reglages", "Calendrier", "Notifications", "Applications", "Session et alimentation"]
    property real wheelDistance: 0
    property var settingsTray: null
    property var applicationsTray: null
    readonly property var settingsPopup: settingsTray ? settingsTray.hiddenLayout.Window.window : null
    readonly property var applicationsPopup: applicationsTray ? applicationsTray.hiddenLayout.Window.window : null

    TextMetrics { id: clockMetrics; font: island.clockFont; text: island.clockText }
    Item { id: networkHost; visible: false }

    Behavior on pagePosition {
        SpringAnimation { id: pageMotion; spring: 10; damping: 0.32; mass: 0.8; epsilon: 0.001 }
    }

    Behavior on reveal {
        SpringAnimation {
            id: morph
            spring: island.opened ? 6.5 : 8
            damping: island.opened ? 0.38 : 0.5
            mass: 1
            epsilon: 0.0008
        }
    }

    function closeSubmenus() {
        for (const tray of [settingsTray, applicationsTray]) {
            if (tray) tray.systemTrayState.expanded = false;
        }
    }

    Connections {
        target: island.settingsTray ? island.settingsTray.systemTrayState : null
        function onExpandedChanged() {
            if (target.expanded && (!island.opened || island.currentPage !== 0)) target.expanded = false;
        }
    }
    Connections {
        target: island.applicationsTray ? island.applicationsTray.systemTrayState : null
        function onExpandedChanged() {
            if (target.expanded && (!island.opened || island.currentPage !== 3)) target.expanded = false;
        }
    }

    Binding { target: island.settingsPopup; property: "visualParent"; value: submenuAnchor; when: island.settingsPopup !== null }
    Binding { target: island.settingsPopup; property: "popupDirection"; value: Qt.LeftEdge; when: island.settingsPopup !== null }
    Binding { target: island.settingsPopup; property: "margin"; value: 12; when: island.settingsPopup !== null }
    Binding { target: island.settingsPopup; property: "hideOnWindowDeactivate"; value: true; when: island.settingsPopup !== null }
    Binding { target: island.applicationsPopup; property: "visualParent"; value: submenuAnchor; when: island.applicationsPopup !== null }
    Binding { target: island.applicationsPopup; property: "popupDirection"; value: Qt.LeftEdge; when: island.applicationsPopup !== null }
    Binding { target: island.applicationsPopup; property: "margin"; value: 12; when: island.applicationsPopup !== null }
    Binding { target: island.applicationsPopup; property: "hideOnWindowDeactivate"; value: true; when: island.applicationsPopup !== null }

    Timer {
        interval: 1000
        running: true
        repeat: true
        onTriggered: island.clockText = Qt.formatTime(new Date(), "hh:mm")
    }

    SessionManagement { id: session }
    Shell.WidgetExplorer { id: widgetFactory; containment: Containment }

    function mount(plugin, host, full, key = plugin) {
        const used = Object.values(loadedWidgets);
        let applet = Containment.applets.find(a => a.pluginName === plugin && !used.includes(island.itemFor(a)));
        if (!applet) {
            widgetFactory.addApplet(plugin);
            applet = Containment.applets.find(a => a.pluginName === plugin && !used.includes(island.itemFor(a)));
        }
        const item = applet ? island.itemFor(applet) : null;
        if (!item) {
            console.error("MyConfig Island could not load widget:", plugin);
            return;
        }
        item.parent = host;
        item.anchors.fill = host;
        item.anchors.margins = host.contentMargin || 0;
        if (full && item.fullRepresentation) {
            item.switchWidth = -1;
            item.switchHeight = -1;
            item.preferredRepresentation = item.fullRepresentation;
        }
        item.visible = true;
        if (plugin === "org.kde.plasma.calendar") {
            Qt.callLater(() => {
                const monthView = item.fullRepresentationItem?.children.find(child => child.viewHeader !== undefined);
                if (monthView) {
                    monthView.viewHeader.heading.font.family = "Iosevka Nerd Font";
                    monthView.viewHeader.heading.font.weight = Font.Normal;
                } else {
                    console.warn("MyConfig Island calendar header is not ready");
                }
            });
        }
        loadedWidgets[key] = item;
        return item;
    }

    Component {
        id: trayFilterComponent
        KItemModels.KSortFilterProxyModel {
            property bool applications: false
            filterRoleName: "itemType"
            filterRowCallback: (row, parent) => {
                const index = sourceModel.index(row, 0, parent);
                const type = sourceModel.data(index, filterRole);
                if (applications) return type === "StatusNotifier" || type === "BackgroundApp";
                // Plasma 6.7 BaseModel::ItemId follows ItemType in the role enum.
                return type === "Plasmoid" && sourceModel.data(index, filterRole + 1) !== "org.kde.plasma.notifications";
            }
        }
    }

    function filterTray(item, applications) {
        if (!item) return;
        const active = item.visibleLayout.model;
        const filter = trayFilterComponent.createObject(island, {sourceModel: active.sourceModel, applications: applications});
        active.sourceModel = filter;
        item.hiddenModel.sourceModel = filter;
        item.plasmoid.configuration.scaleIconsToFit = true;
        item.visibleLayout.Layout.fillWidth = true;
        item.visibleLayout.cellWidth = Qt.binding(() => item.visibleLayout.count > 0 ? item.visibleLayout.width / item.visibleLayout.count : item.visibleLayout.width);
        item.systemTrayState.expanded = false;
        trayToolTipTimer.restart();
    }

    function fixTrayToolTips(item) {
        const layout = item ? item.visibleLayout : null;
        if (!layout) return;
        for (let index = 0; index < layout.count; index++) {
            const toolTip = layout.itemAtIndex(index)?.item;
            if (toolTip) toolTip.location = PlasmaCore.Types.Desktop;
        }
    }

    function loadWidgets() {
        if (Object.keys(loadedWidgets).length) return;
        cpuHost.widget = mount("org.kde.plasma.systemmonitor.cpu", cpuHost, true);
        memoryHost.widget = mount("org.kde.plasma.systemmonitor.memory", memoryHost, true);
        mount("org.kde.plasma.systemmonitor.net", networkHost, true);
        mount("org.kde.plasma.calendar", calendarHost, true);
        mount("org.kde.plasma.notifications", notificationsHost, true);
        settingsTray = mount("org.kde.plasma.systemtray", settingsHost, false, "settings");
        applicationsTray = mount("org.kde.plasma.systemtray", applicationsHost, false, "applications");
        filterTray(settingsTray, false);
        filterTray(applicationsTray, true);
        panelBackgroundTimer.start();
    }

    function selectPage(index) {
        closeSubmenus();
        currentPage = Math.max(0, Math.min(pageNames.length - 1, index));
    }

    function pageWheel(wheel) {
        wheel.accepted = true;
        if (wheelCooldown.running) return;
        const delta = wheel.angleDelta.y || wheel.pixelDelta.y;
        if (!delta) return;
        if (wheelDistance * delta < 0) wheelDistance = 0;
        wheelDistance += delta;
        const threshold = wheel.angleDelta.y ? 120 : 40;
        if (Math.abs(wheelDistance) < threshold) return;
        selectPage(currentPage + (wheelDistance < 0 ? 1 : -1));
        wheelDistance = 0;
        wheelCooldown.start();
    }

    Timer { id: wheelCooldown; interval: 280 }

    Timer {
        id: trayToolTipTimer
        interval: 100
        onTriggered: {
            island.fixTrayToolTips(island.settingsTray);
            island.fixTrayToolTips(island.applicationsTray);
        }
    }

    Connections {
        target: island.settingsTray ? island.settingsTray.visibleLayout : null
        function onCountChanged() { trayToolTipTimer.restart(); }
    }

    Connections {
        target: island.applicationsTray ? island.applicationsTray.visibleLayout : null
        function onCountChanged() { trayToolTipTimer.restart(); }
    }

    Timer {
        id: panelBackgroundTimer
        interval: 1000
        // Let the previous instance restore its binding before taking ownership.
        onTriggered: island.panelBackgroundReady = true
    }

    function openIsland() {
        opened = true;
        loadWidgets();
        reveal = 1;
        popup.requestActivate();
    }

    function closeIsland() {
        closeSubmenus();
        opened = false;
        reveal = 0;
    }

    fullRepresentation: Item {
        implicitWidth: island.pillWidth + 32
        implicitHeight: island.pillHeight
    }

    Window {
        id: popup
        title: island.opened ? "MyConfig Island"
            : morph.running ? "MyConfig Island Closing" : "MyConfig Island Compact"
        flags: Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.NoDropShadowWindowHint
        transientParent: null
        color: "transparent"
        screen: Qt.application.screens.find(candidate => candidate.name === island.fullRepresentationItem?.Screen.name) || null
        x: screen ? screen.virtualX + Math.round((screen.width - width) / 2) : 0
        y: screen ? screen.virtualY : 0
        width: Math.ceil(surface.width) + 32
        height: Math.ceil(surface.height) + 32
        visible: island.panelWindow !== null && Plasmoid.containment.screen >= 0
            && (island.requestedVisible || appearanceMotion.running || island.appearance > 0.001)
        onVisibleChanged: {
            if (!visible) island.windowFrameReady = false;
        }
        onFrameSwapped: {
            if (!visible) return;
            island.windowFrameReady = true;
            if (island.appearancePending && island.requestedVisible) {
                island.appearancePending = false;
                island.appearance = 1;
            }
        }
        onActiveChanged: {
            if (!active && island.opened) Qt.callLater(() => {
                if (!popup.active && !island.settingsPopup?.visible && !island.applicationsPopup?.visible)
                    island.closeIsland();
            });
        }
        onClosing: close => {
            close.accepted = false;
            island.closeIsland();
        }

        Item {
            id: canvas
            parent: popup.contentItem
            width: popup.width
            height: popup.height
            opacity: Math.max(0, Math.min(1, island.appearance))
            y: -12 * (1 - island.appearance)
            scale: 0.88 + 0.12 * island.appearance
            transformOrigin: Item.Top
            focus: true
            Keys.onEscapePressed: island.closeIsland()

            MouseArea {
                anchors.fill: parent
                onClicked: island.closeIsland()
                onWheel: wheel => island.pageWheel(wheel)
            }

            Kirigami.ShadowedRectangle {
                anchors.fill: surface
                radius: surface.radius
                color: Qt.rgba(0, 0, 0, 0.85)
                shadow.xOffset: 0
                shadow.yOffset: 0
                shadow.size: 16
                shadow.color: Qt.rgba(160 / 255, 170 / 255, 190 / 255, 0.25)
            }

            Controls.Button {
                id: clockButton
                z: 2
                x: (canvas.width - width) / 2
                y: island.clockPosition.y
                width: island.pillWidth
                height: island.actualPillHeight
                padding: 0
                background: Item {}
                contentItem: ClockText {}
                Accessible.name: island.opened ? "Refermer" : "Ouvrir"
                onClicked: island.opened ? island.closeIsland() : island.openIsland()
            }

            Rectangle {
                id: surface
                x: island.clockPosition.x - width / 2
                y: island.clockPosition.y
                width: island.pillWidth + (island.openWidth - island.pillWidth) * Math.max(0, Math.min(1, island.reveal)) - island.openWidth * 0.6 * island.stretch
                height: island.actualPillHeight + (island.openHeight - island.actualPillHeight) * (Math.max(0, Math.min(1, island.reveal)) + island.stretch)
                radius: island.actualPillHeight / 2 + (30 - island.actualPillHeight / 2) * Math.max(0, Math.min(1, island.reveal)) + island.stretch * 100
                color: "transparent"
                border.color: "#222222"
                border.width: Math.max(0, Math.min(1, island.reveal))
                clip: true

                TapHandler {
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    onPressedChanged: {
                        if (!pressed) return;
                        for (const [tray, host, page] of [[island.settingsTray, settingsHost, 0], [island.applicationsTray, applicationsHost, 3]]) {
                            if (!tray) continue;
                            const pos = surface.mapToItem(host, point.position);
                            if (island.currentPage !== page || pos.x < 0 || pos.y < 0 || pos.x >= host.width || pos.y >= host.height) {
                                tray.systemTrayState.expanded = false;
                            }
                        }
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: mouse => mouse.accepted = true
                    onWheel: wheel => island.pageWheel(wheel)
                }

                Item {
                    id: body
                    width: island.openWidth
                    height: island.openHeight
                    anchors.horizontalCenter: parent.horizontalCenter
                    opacity: Math.max(0, Math.min(1, (island.reveal - 0.2) / 0.65))
                    enabled: island.opened && island.reveal > 0.8

                    Item {
                        id: submenuAnchor
                        x: 0
                        y: 80
                        width: 1
                        height: 380
                    }

                    Item {
                        id: heading
                        x: 24
                        y: island.actualPillHeight
                        width: 584
                        height: 60
                        Text {
                            anchors.centerIn: parent
                            text: island.pageNames[island.currentPage]
                            color: "#f0f2f7"
                            font.family: "Iosevka Nerd Font"
                            font.pixelSize: 22
                            font.weight: Font.Medium
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: (island.currentPage + 1) + " / " + island.pageNames.length
                            color: "#a0aabe"
                            font.family: "Iosevka Nerd Font"
                            font.pixelSize: 13
                        }
                    }

                    Item {
                        id: viewport
                        anchors.top: heading.bottom
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 24
                        x: 24
                        width: 584
                        clip: true

                        Column {
                            width: viewport.width
                            y: -island.pagePosition * viewport.height

                            Item {
                                width: viewport.width
                                height: viewport.height
                                ColumnLayout {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    spacing: 10
                                    RowLayout {
                                        Layout.fillWidth: true
                                        Layout.preferredHeight: Math.max(cpuHost.implicitHeight, memoryHost.implicitHeight)
                                        Layout.minimumHeight: Layout.preferredHeight
                                        Layout.maximumHeight: Layout.preferredHeight
                                        spacing: 12
                                        WidgetCard { id: cpuHost; Layout.fillWidth: true; Layout.fillHeight: true }
                                        WidgetCard { id: memoryHost; Layout.fillWidth: true; Layout.fillHeight: true }
                                    }
                                    SectionTitle { text: "REGLAGES ET PERIPHERIQUES" }
                                    WidgetCard { id: settingsHost; contentMargin: 4; Layout.fillWidth: true; Layout.preferredHeight: 44 }
                                }
                            }

                            WidgetCard { id: calendarHost; width: viewport.width; height: viewport.height }
                            WidgetCard { id: notificationsHost; width: viewport.width; height: viewport.height }

                            Item {
                                width: viewport.width
                                height: viewport.height
                                WidgetCard {
                                    id: applicationsHost
                                    contentMargin: 4
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    height: 44
                                }
                                Text {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    anchors.top: applicationsHost.bottom
                                    anchors.topMargin: 16
                                    text: "Applications en cours"
                                    color: "#a0aabe"
                                    font.family: "Iosevka Nerd Font"
                                    font.pixelSize: 15
                                }
                            }

                            Item {
                                width: viewport.width
                                height: viewport.height
                                ColumnLayout {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    spacing: 12
                                    SectionTitle { text: "SESSION" }
                                    GridLayout {
                                        Layout.fillWidth: true
                                        columns: 2
                                        columnSpacing: 10
                                        rowSpacing: 10
                                        ActionButton { text: "Verrouiller"; icon.name: "system-lock-screen"; enabled: session.canLock; onClicked: session.lock() }
                                        ActionButton { text: "Changer d'utilisateur"; icon.name: "system-switch-user"; enabled: session.canSwitchUser; onClicked: session.switchUser() }
                                        ActionButton { text: "Fermer la session"; icon.name: "system-log-out"; enabled: session.canLogout; onClicked: session.requestLogout() }
                                    }
                                    SectionTitle { text: "ALIMENTATION" }
                                    GridLayout {
                                        Layout.fillWidth: true
                                        columns: 2
                                        columnSpacing: 10
                                        rowSpacing: 10
                                        ActionButton { text: "Veille"; icon.name: "system-suspend"; enabled: session.canSuspend; onClicked: session.suspend() }
                                        ActionButton { text: "Hibernation"; icon.name: "system-suspend-hibernate"; enabled: session.canHibernate; onClicked: session.hibernate() }
                                        ActionButton { text: "Redemarrer"; icon.name: "system-reboot"; enabled: session.canReboot; onClicked: session.requestReboot() }
                                        ActionButton { text: "Eteindre"; icon.name: "system-shutdown"; enabled: session.canShutdown; onClicked: session.requestShutdown() }
                                    }
                                }
                            }
                        }
                    }

                    Column {
                        id: navigation
                        anchors.right: parent.right
                        anchors.rightMargin: 16
                        anchors.verticalCenter: parent.verticalCenter
                        width: 36
                        spacing: 8
                        Repeater {
                            model: island.pageNames
                            Controls.Button {
                                id: pageButton
                                required property int index
                                required property string modelData
                                width: 36
                                height: 36
                                Accessible.name: modelData
                                background: Rectangle {
                                    anchors.centerIn: parent
                                    width: 8
                                    height: 8
                                    radius: width / 2
                                    color: "#4a4f5e"
                                    visible: pageButton.index !== island.currentPage
                                }
                                contentItem: Item {}
                                onClicked: island.selectPage(index)
                            }
                        }
                    }

                    Rectangle {
                        id: pageMarker
                        property real speed: 0
                        property real deformation: Math.min(1, speed / 6)
                        readonly property real stretch: Math.max(-0.25, Math.min(1, deformation))
                        Behavior on deformation {
                            SpringAnimation { spring: 18; damping: 0.38; mass: 0.5; epsilon: 0.001 }
                        }
                        FrameAnimation {
                            property real previousPosition: island.pagePosition
                            running: island.opened && pageMotion.running
                            onRunningChanged: {
                                previousPosition = island.pagePosition;
                                if (!running) pageMarker.speed = 0;
                            }
                            onTriggered: {
                                pageMarker.speed = frameTime > 0
                                    ? Math.abs(island.pagePosition - previousPosition) / frameTime : 0;
                                previousPosition = island.pagePosition;
                            }
                        }
                        x: navigation.x + (navigation.width - width) / 2
                        y: navigation.y + island.pagePosition * 44 + (36 - height) / 2
                        width: 8 - stretch * 5
                        height: 26 + stretch * 26
                        radius: width / 2
                        antialiasing: true
                        color: "#ff4ead"
                    }
                }
            }
        }
    }

    component ClockText: Text {
        text: island.clockText
        color: "#f0f2f7"
        font: island.clockFont
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }

    component SectionTitle: Text {
        color: "#ff4ead"
        font.family: "Iosevka Nerd Font"
        font.pixelSize: 13
        font.weight: Font.DemiBold
        font.letterSpacing: 2
        Layout.topMargin: 8
    }

    component WidgetCard: Rectangle {
        property int contentMargin: 8
        property var widget: null
        implicitHeight: widget?.fullRepresentationItem
            ? Math.max(0, widget.fullRepresentationItem.implicitHeight,
                widget.fullRepresentationItem.Layout.preferredHeight,
                widget.fullRepresentationItem.Layout.minimumHeight) + contentMargin * 2
            : 0
        radius: 18
        color: "#0a0a0a"
    }

    component ActionButton: Controls.Button {
        id: action
        Layout.fillWidth: true
        Layout.minimumWidth: 0
        Layout.preferredWidth: 1
        Layout.preferredHeight: 48
        leftPadding: 12
        rightPadding: 12
        font.family: "Iosevka Nerd Font"
        font.pixelSize: 15
        icon.width: 18
        icon.height: 18
        Accessible.name: text
        palette.buttonText: enabled ? "#d0d6e0" : "#4a4f5e"
        contentItem: RowLayout {
            spacing: 8
            Kirigami.Icon {
                source: action.icon.name
                color: action.enabled ? "#ff4ead" : "#4a4f5e"
                Layout.preferredWidth: 18
                Layout.preferredHeight: 18
            }
            Text {
                text: action.text
                font: action.font
                color: action.enabled ? "#d0d6e0" : "#4a4f5e"
                verticalAlignment: Text.AlignVCenter
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
        background: Rectangle {
            radius: 13
            color: action.down ? "#a0205f" : action.hovered ? "#222222" : "#111111"
            border.width: action.activeFocus ? 1 : 0
            border.color: "#ff4ead"
        }
    }

    Component.onCompleted: {
        Plasmoid.configuration.edgeVisible = false;
        Qt.callLater(loadWidgets);
    }
}

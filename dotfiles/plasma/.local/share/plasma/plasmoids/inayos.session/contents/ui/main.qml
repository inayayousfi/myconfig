pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ksvg as KSvg
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.extras as PlasmaExtras
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.plasma.plasmoid
// A private Plasma module: check it still loads after a Plasma update.
import org.kde.plasma.private.sessions

PlasmoidItem {
    id: root

    preferredRepresentation: fullRepresentation
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    // Like the global menu, take the full bar height without the panel's
    // margins around widgets.
    Plasmoid.constraintHints: Plasmoid.CanFillArea

    SessionManagement {
        id: session
    }

    Plasma5Support.DataSource {
        id: executable
        engine: "executable"
        onNewData: function(sourceName) {
            disconnectSource(sourceName);
        }
    }

    // kstart gives each application its own unit instead of Plasma's.
    function launch(desktopFile) {
        executable.connectSource(`kstart --application ${desktopFile}`);
    }

    // Labels reuse the translations that KDE installs, so the widget stores no
    // text of its own in any language.
    // Drawn like the global menu's entries beside it, which use Plasma's menu
    // bar item background rather than a regular button.
    fullRepresentation: Item {
        id: area

        // The space between this widget and the screen edge, and the bar's
        // space before the first menu entry. The button covers both, so its
        // highlight and its pointer area reach the screen edge and touch the
        // first entry, as the entries touch each other, with Tux in the
        // middle.
        readonly property real edgeGap: {
            void root.x;
            void width;
            return Math.max(0, mapToItem(null, 0, 0).x);
        }
        readonly property real entryGap: Kirigami.Units.smallSpacing

        // The icon keeps the top and bottom margins that the global menu
        // entries leave around their text. They come from the resting frame,
        // so the size stays the same on hover.
        implicitHeight: Kirigami.Units.iconSizes.medium
        implicitWidth: implicitHeight - metrics.margins.top - metrics.margins.bottom + 2 * Kirigami.Units.smallSpacing
        Layout.fillHeight: true
        Layout.minimumWidth: implicitWidth
        Layout.preferredWidth: implicitWidth
        Layout.maximumWidth: implicitWidth

        KSvg.FrameSvgItem {
            id: metrics
            visible: false
            imagePath: "widgets/menubaritem"
            prefix: "normal"
        }

        QQC2.AbstractButton {
            id: button

            readonly property bool menuOpen: menu.status === PlasmaExtras.Menu.Open

            // The menu takes the pointer before the release reaches the
            // button, which then stays pressed, so closing the menu ends the
            // press by turning the button off and on.
            onMenuOpenChanged: {
                if (!menuOpen) {
                    enabled = false;
                    enabled = true;
                }
            }

            x: -area.edgeGap
            width: area.width + area.edgeGap + area.entryGap
            height: area.height
            hoverEnabled: true
            topPadding: metrics.margins.top
            bottomPadding: metrics.margins.bottom
            leftPadding: area.edgeGap + Kirigami.Units.smallSpacing
            rightPadding: area.entryGap + Kirigami.Units.smallSpacing
            Accessible.name: i18nd("libkicker", "Session")
            Accessible.role: Accessible.ButtonMenu
            // Like the global menu, the menu opens on press.
            onPressed: menu.openRelative()

            background: KSvg.FrameSvgItem {
                imagePath: "widgets/menubaritem"
                prefix: button.down || button.menuOpen ? "pressed" : button.hovered ? "hover" : "normal"
            }

            contentItem: Kirigami.Icon {
                source: Qt.resolvedUrl("../icons/tux.svg")
                // Painted like the global menu's labels beside it: the bar's
                // text color, switched at once to the highlighted text color
                // over the highlight, with no other hover effect.
                isMask: true
                animated: false
                color: button.hovered || button.down || button.menuOpen
                    ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
            }

            PlasmaExtras.Menu {
                id: menu

                visualParent: button
                placement: PlasmaExtras.Menu.BottomPosedLeftAlignedPopup

                PlasmaExtras.MenuItem {
                    text: i18nd("systemsettings", "System Information")
                    icon: "help-about"
                    onClicked: root.launch("org.kde.kinfocenter")
                }
                PlasmaExtras.MenuItem {
                    separator: true
                }
                PlasmaExtras.MenuItem {
                    text: i18nd("systemsettings", "System Settings")
                    icon: "systemsettings"
                    onClicked: root.launch("systemsettings")
                }
                PlasmaExtras.MenuItem {
                    text: i18nd("plasma-systemmonitor", "System Monitor")
                    icon: "utilities-system-monitor"
                    onClicked: root.launch("org.kde.plasma-systemmonitor")
                }
                PlasmaExtras.MenuItem {
                    text: i18nd("kwin", "Kill Window")
                    icon: "window-close"
                    onClicked: executable.connectSource("qdbus6 org.kde.kglobalaccel /component/kwin org.kde.kglobalaccel.Component.invokeShortcut 'Kill Window'")
                }
                PlasmaExtras.MenuItem {
                    separator: true
                }
                PlasmaExtras.MenuItem {
                    text: i18ndc("libkicker", "Suspend to RAM", "Sleep")
                    icon: "system-suspend"
                    enabled: session.canSuspend
                    onClicked: session.suspend()
                }
                PlasmaExtras.MenuItem {
                    text: i18nd("libkicker", "Hibernate")
                    icon: "system-suspend-hibernate"
                    enabled: session.canHibernate
                    onClicked: session.hibernate()
                }
                PlasmaExtras.MenuItem {
                    text: i18nd("libkicker", "Restart")
                    icon: "system-reboot"
                    enabled: session.canReboot
                    onClicked: session.requestReboot()
                }
                PlasmaExtras.MenuItem {
                    text: i18nd("libkicker", "Shut Down")
                    icon: "system-shutdown"
                    enabled: session.canShutdown
                    onClicked: session.requestShutdown()
                }
                PlasmaExtras.MenuItem {
                    separator: true
                }
                PlasmaExtras.MenuItem {
                    text: i18nd("libkicker", "Lock")
                    icon: "system-lock-screen"
                    enabled: session.canLock
                    onClicked: session.lock()
                }
                PlasmaExtras.MenuItem {
                    text: i18nd("libkicker", "Log Out")
                    icon: "system-log-out"
                    enabled: session.canLogout
                    onClicked: session.requestLogout()
                }
                PlasmaExtras.MenuItem {
                    text: i18nd("libkicker", "Switch User")
                    icon: "system-switch-user"
                    enabled: session.canSwitchUser
                    onClicked: session.switchUser()
                }
            }
        }
    }
}

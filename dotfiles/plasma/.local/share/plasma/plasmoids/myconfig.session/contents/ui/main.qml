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

    // Drawn like the global menu's entries beside it, which use Plasma's menu
    // bar item background rather than a regular button.
    fullRepresentation: QQC2.AbstractButton {
        id: button

        readonly property bool menuOpen: menu.status === PlasmaExtras.Menu.Open

        // The icon keeps the margins that the global menu entries leave around
        // their text, while the highlight fills the bar height.
        implicitHeight: Kirigami.Units.iconSizes.medium
        implicitWidth: availableHeight + leftPadding + rightPadding
        Layout.fillHeight: true
        Layout.minimumWidth: implicitWidth
        Layout.preferredWidth: implicitWidth
        hoverEnabled: true
        topPadding: frame.margins.top
        bottomPadding: frame.margins.bottom
        leftPadding: frame.margins.left
        rightPadding: frame.margins.right
        Accessible.name: i18n("Session")
        Accessible.description: Plasmoid.metaData.description
        Accessible.role: Accessible.ButtonMenu
        // Like the global menu, the menu opens on press.
        onPressed: menu.openRelative()

        background: KSvg.FrameSvgItem {
            id: frame
            imagePath: "widgets/menubaritem"
            prefix: button.down || button.menuOpen ? "pressed" : button.hovered ? "hover" : "normal"
        }

        contentItem: Kirigami.Icon {
            implicitWidth: button.availableHeight
            implicitHeight: button.availableHeight
            source: "start-here-kde"
            active: button.hovered || button.menuOpen
        }

        PlasmaExtras.Menu {
            id: menu

            visualParent: button
            placement: PlasmaExtras.Menu.BottomPosedLeftAlignedPopup

            PlasmaExtras.MenuItem {
                text: i18n("About This System")
                icon: "help-about"
                onClicked: root.launch("org.kde.kinfocenter")
            }
            PlasmaExtras.MenuItem {
                separator: true
            }
            PlasmaExtras.MenuItem {
                text: i18n("System Settings")
                icon: "systemsettings"
                onClicked: root.launch("systemsettings")
            }
            PlasmaExtras.MenuItem {
                text: i18n("System Monitor")
                icon: "utilities-system-monitor"
                onClicked: root.launch("org.kde.plasma-systemmonitor")
            }
            PlasmaExtras.MenuItem {
                text: i18n("Force Quit…")
                icon: "window-close"
                onClicked: executable.connectSource("qdbus6 org.kde.kglobalaccel /component/kwin org.kde.kglobalaccel.Component.invokeShortcut 'Kill Window'")
            }
            PlasmaExtras.MenuItem {
                separator: true
            }
            PlasmaExtras.MenuItem {
                text: i18n("Sleep")
                icon: "system-suspend"
                enabled: session.canSuspend
                onClicked: session.suspend()
            }
            PlasmaExtras.MenuItem {
                text: i18n("Hibernate")
                icon: "system-suspend-hibernate"
                enabled: session.canHibernate
                onClicked: session.hibernate()
            }
            PlasmaExtras.MenuItem {
                text: i18n("Restart")
                icon: "system-reboot"
                enabled: session.canReboot
                onClicked: session.requestReboot()
            }
            PlasmaExtras.MenuItem {
                text: i18n("Shut Down")
                icon: "system-shutdown"
                enabled: session.canShutdown
                onClicked: session.requestShutdown()
            }
            PlasmaExtras.MenuItem {
                separator: true
            }
            PlasmaExtras.MenuItem {
                text: i18n("Lock")
                icon: "system-lock-screen"
                enabled: session.canLock
                onClicked: session.lock()
            }
            PlasmaExtras.MenuItem {
                text: i18n("Log Out")
                icon: "system-log-out"
                enabled: session.canLogout
                onClicked: session.requestLogout()
            }
            PlasmaExtras.MenuItem {
                text: i18n("Switch User")
                icon: "system-switch-user"
                enabled: session.canSwitchUser
                onClicked: session.switchUser()
            }
        }
    }
}

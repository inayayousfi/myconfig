pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasmoid
import org.kde.plasma.private.sessions

PlasmoidItem {
    id: root

    preferredRepresentation: fullRepresentation
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground

    SessionManagement {
        id: session
    }

    fullRepresentation: QQC2.ToolButton {
        id: button

        icon.name: "system-shutdown"
        display: QQC2.AbstractButton.IconOnly
        Accessible.name: qsTr("Power")
        Accessible.description: qsTr("Restart, shut down, sleep, or hibernate")
        Accessible.role: Accessible.Button
        onClicked: menu.open()

        QQC2.Menu {
            id: menu

            y: button.height
            popupType: QQC2.Popup.Window

            QQC2.MenuItem {
                text: qsTr("Restart")
                icon.name: "system-reboot"
                enabled: session.canReboot
                onTriggered: session.requestReboot()
            }
            QQC2.MenuItem {
                text: qsTr("Shut Down")
                icon.name: "system-shutdown"
                enabled: session.canShutdown
                onTriggered: session.requestShutdown()
            }
            QQC2.MenuItem {
                text: qsTr("Sleep")
                icon.name: "system-suspend"
                enabled: session.canSuspend
                onTriggered: session.suspend()
            }
            QQC2.MenuItem {
                text: qsTr("Hibernate")
                icon.name: "system-suspend-hibernate"
                enabled: session.canHibernate
                onTriggered: session.hibernate()
            }
        }
    }
}

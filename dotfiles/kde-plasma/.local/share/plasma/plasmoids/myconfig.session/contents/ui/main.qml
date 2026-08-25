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

        icon.name: "user-identity"
        display: QQC2.AbstractButton.IconOnly
        Accessible.name: qsTr("Session")
        Accessible.description: qsTr("Lock, log out, or switch user")
        Accessible.role: Accessible.Button
        onClicked: menu.open()

        QQC2.Menu {
            id: menu

            y: button.height
            popupType: QQC2.Popup.Window

            QQC2.MenuItem {
                text: qsTr("Lock")
                icon.name: "system-lock-screen"
                enabled: session.canLock
                onTriggered: session.lock()
            }
            QQC2.MenuItem {
                text: qsTr("Log Out")
                icon.name: "system-log-out"
                enabled: session.canLogout
                onTriggered: session.requestLogout()
            }
            QQC2.MenuItem {
                text: qsTr("Switch User")
                icon.name: "system-switch-user"
                enabled: session.canSwitchUser
                onTriggered: session.switchUser()
            }
        }
    }
}

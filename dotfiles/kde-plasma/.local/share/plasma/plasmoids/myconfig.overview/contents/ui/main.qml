import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.plasma.plasmoid

PlasmoidItem {
    id: root

    preferredRepresentation: fullRepresentation
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    Plasmoid.constraintHints: PlasmaCore.Types.CanFillArea

    fullRepresentation: QQC2.ToolButton {
        text: qsTr("Overview")
        font.pointSize: 14
        padding: 0
        spacing: 0
        Layout.leftMargin: 0
        Layout.rightMargin: 0
        Layout.minimumWidth: implicitWidth
        Layout.preferredWidth: implicitWidth
        Layout.fillHeight: true
        Accessible.name: text
        Accessible.role: Accessible.Button
        onClicked: executable.connectSource("/usr/bin/qdbus6 org.kde.kglobalaccel /component/kwin org.kde.kglobalaccel.Component.invokeShortcut Overview")
    }

    Plasma5Support.DataSource {
        id: executable
        engine: "executable"
        onNewData: function(sourceName) {
            disconnectSource(sourceName);
        }
    }
}

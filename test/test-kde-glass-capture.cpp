#include <QCoreApplication>
#include <QDBusInterface>
#include <QDBusReply>
#include <QDBusUnixFileDescriptor>
#include <QBuffer>
#include <QImage>
#include <QVariantMap>
#include <cstdio>
#include <poll.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    const QString root = qEnvironmentVariable("MYCONFIG_GLASS_TEST_ROOT");
    if (root.isEmpty() || qEnvironmentVariable("XDG_RUNTIME_DIR") != root + "/runtime"
        || qEnvironmentVariable("WAYLAND_DISPLAY") != "myconfig-glass-test" || qEnvironmentVariableIsSet("DISPLAY")) return 1;
    QCoreApplication app(argc, argv);
    int pipeFds[2];
    if (pipe(pipeFds) != 0) return 2;
    QDBusInterface screenshot("org.kde.KWin.ScreenShot2", "/org/kde/KWin/ScreenShot2", "org.kde.KWin.ScreenShot2");
    screenshot.setTimeout(5000);
    const QVariantMap options{{"hide-caller-windows", false}, {"include-cursor", false}};
    QDBusReply<QVariantMap> reply = screenshot.call("CaptureWorkspace", options, QVariant::fromValue(QDBusUnixFileDescriptor(pipeFds[1])));
    close(pipeFds[1]);
    if (!reply.isValid()) {
        fprintf(stderr, "%s\n", qPrintable(reply.error().message()));
        return 3;
    }
    const auto metadata = reply.value();
    QImage image(metadata["width"].toInt(), metadata["height"].toInt(), QImage::Format(metadata["format"].toInt()));
    if (image.isNull() || image.bytesPerLine() != metadata["stride"].toInt()) return 4;
    qsizetype received = 0;
    while (received < image.sizeInBytes()) {
        pollfd descriptor{pipeFds[0], POLLIN, 0};
        if (poll(&descriptor, 1, 5000) <= 0) return 5;
        const auto count = read(pipeFds[0], image.bits() + received, image.sizeInBytes() - received);
        if (count <= 0) return 6;
        received += count;
    }
    close(pipeFds[0]);
    QBuffer buffer;
    buffer.open(QIODevice::WriteOnly);
    if (!image.save(&buffer, "PNG")) return 7;
    return fwrite(buffer.data().data(), 1, buffer.size(), stdout) == size_t(buffer.size()) ? 0 : 8;
}

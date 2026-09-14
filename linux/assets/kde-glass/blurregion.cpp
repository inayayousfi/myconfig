#include <KWindowEffects>
#include <QPainterPath>
#include <QPlatformSurfaceEvent>
#include <QPointer>
#include <QQmlEngine>
#include <QQmlExtensionPlugin>
#include <QQuickWindow>
#include <QTimer>
#include <QWindow>

class BlurRegion : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QWindow *window READ window WRITE setWindow NOTIFY windowChanged)
    Q_PROPERTY(QRectF rect MEMBER m_rect NOTIFY changed)
    Q_PROPERTY(qreal radius MEMBER m_radius NOTIFY changed)
    Q_PROPERTY(bool enabled MEMBER m_enabled NOTIFY changed)

public:
    explicit BlurRegion(QObject *parent = nullptr) : QObject(parent)
    {
        connect(this, &BlurRegion::changed, this, &BlurRegion::update);
    }

    ~BlurRegion() override { setWindow(nullptr); }

    QWindow *window() const { return m_window; }

    void setWindow(QWindow *window)
    {
        if (m_window == window) return;
        if (m_window) {
            m_window->removeEventFilter(this);
            disconnect(m_window, nullptr, this, nullptr);
            KWindowEffects::enableBlurBehind(m_window, false);
            if (auto quick = qobject_cast<QQuickWindow *>(m_window.data())) quick->update();
        }
        m_window = window;
        if (m_window) {
            m_window->installEventFilter(this);
            connect(m_window, &QWindow::visibleChanged, this, &BlurRegion::update);
        }
        Q_EMIT windowChanged();
        update();
    }

Q_SIGNALS:
    void changed();
    void windowChanged();

protected:
    bool eventFilter(QObject *object, QEvent *event) override
    {
        if (event->type() == QEvent::PlatformSurface
            && static_cast<QPlatformSurfaceEvent *>(event)->surfaceEventType() == QPlatformSurfaceEvent::SurfaceCreated) {
            // Reapply after Wayland recreates the native surface on show.
            QTimer::singleShot(0, this, &BlurRegion::update);
        }
        return QObject::eventFilter(object, event);
    }

private:
    void update()
    {
        if (!m_window || !m_window->isVisible()) return;
        QPainterPath shape;
        shape.addRoundedRect(m_rect, m_radius, m_radius);
        const QRegion region(shape.toFillPolygon().toPolygon());
        // An empty region means the entire window in KWindowEffects.
        KWindowEffects::enableBlurBehind(m_window, m_enabled && !region.isEmpty(), region);
        // Wayland applies background effects on the next surface commit, even if pixels did not change.
        if (auto quick = qobject_cast<QQuickWindow *>(m_window.data())) quick->update();
    }

    QPointer<QWindow> m_window;
    QRectF m_rect;
    qreal m_radius = 0;
    bool m_enabled = true;
};

class GlassPlugin : public QQmlExtensionPlugin
{
    Q_OBJECT
    Q_PLUGIN_METADATA(IID QQmlExtensionInterface_iid)
public:
    void registerTypes(const char *uri) override
    {
        qmlRegisterType<BlurRegion>(uri, 1, 0, "BlurRegion");
    }
};

#include "blurregion.moc"

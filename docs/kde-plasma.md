# KDE Plasma

This document describes the KDE Plasma setup of the InayOS desktop profile: the `kde-plasma` module in `crates/modules/src/kde_plasma.rs` and the `plasma` dotfile package.

## Packages

The module installs the Plasma desktop package by package instead of through `plasma-meta`: every package that `plasma-meta` holds except `plasma-browser-integration`, plus `plasma-disks`, `drkonqi` and `flatpak-kcm`. `verify` fails when one is missing.

The module enables Plasma Login Manager, `plasmalogin.service`, without starting it, so the installed system boots to its login screen. `verify` fails when it is not enabled. The `printing` module adds CUPS with KDE's printer settings.

## Theme and settings

The module applies the `inayos.desktop` global theme of the `plasma` package with personal settings: the pink accent color, IBM Plex fonts, input settings and window rules. The theme holds a copy of Breeze Dark's `contents/defaults`, so it applies Breeze Dark's colors, icons, styles and splash screen, and its default layout is the InayOS panel layout.

The session uses a US keyboard layout whose right Alt key is the Compose key: press and release it, then type a sequence such as `'` `e` for é. The login and lock screens keep the system layout from `localectl`.

The interface uses IBM Plex Sans and the fixed-width font is BlexMono Nerd Font Mono, the IBM Plex Mono copy with Nerd Font icons. The `plasma` package's fontconfig file makes IBM Plex Sans, IBM Plex Serif and BlexMono Nerd Font Mono the defaults for programs that ask for sans-serif, serif or monospace text. The module also installs Noto CJK, because the IBM Plex fonts have no Chinese, Japanese or Korean characters.

## Panel layout

The `plasma` package owns the panel layout, the file `contents/layouts/org.kde.plasma.desktop-layout.js` of the `inayos.desktop` global theme in `~/.local/share/plasma/look-and-feel`. `inayos-plasma-layout` sends it to Plasma. The layout gives every display a top bar and a dock. The top bar holds a KDE session menu (the `inayos.session` widget), the global menu, the tray and the clock. The dock holds a one-click Overview icon and the task icons. A new dock pins Overview, Chrome, Herdr, LyX and Qalculate. A rebuilt dock keeps its own pins, except that the pins of T3 Code, VSCodium and Ghostty make way for one Herdr pin. The task widget shows a blank icon for a pin whose application is missing, so the InayOS desktop profile installs each pinned application.

The session menu shows a one-color Tux in the bar's text color. The drawing comes from CachyOS's char-white icon theme, which is GPL, so the widget is GPL. Its labels reuse the translations that KDE installs, so the widget stores no text in any language and follows the system language.

The tray keeps every Plasma widget in its popup, so the bar shows only the icons of applications.

The first run replaces every existing panel. Later runs add bars for new displays and reset the managed ones. `inayos-plasma-layout.service` runs the command at each Plasma login.

The install also runs the command, which changes the panels only while Plasma runs. In installer mode the install does not run it.

When Plasma starts without a panel file, `plasma-org.kde.plasma.desktop-appletsrc`, it runs the default layout of the global theme that `kdeglobals` names, so a new user's first login builds the bars and the dock from the same file. Installer mode never creates the panel file, because a file with desktops in it skips the default layout. In that run Plasma's output only reaches its debug log, so when a widget is missing or the script fails, the layout loads Plasma's own default panel instead of leaving the desktop without panels. The layout command and its service then add bars for displays plugged in later, because Plasma adds desktops, not panels, for a new display.

The layout marks the bars it manages with the panel keys `inayosRole`, `inayosScreen` and `inayosLayoutVersion`. It also counts a bar that myconfig, the earlier name of InayOS, marked with `myconfigRole`, `myconfigScreen` and `myconfigLayoutVersion` as managed, and writes the new keys on it. Such a dock keeps its pins, and its Overview pin moves from `myconfig-overview.desktop` to `inayos-overview.desktop`. Its top bar still holds the `myconfig.session` widget, so the layout fills the top bar again.

The module records the panel file before the first run. Plasma keeps its panels in memory and saves them when it stops, so `remove` stops a running Plasma shell, restores the recorded file, and starts the shell again.

## Bar behavior

The top bar stays visible, and maximized windows stop below it. On a display that shows a fullscreen window, the top bar hides. The dock stays hidden until the pointer reaches the bottom edge.

The KWin script `inayos-panels` handles this behavior. It shows a hidden bar while the pointer is near its screen edge, even over a fullscreen window, and hides it again once the pointer and the bar's menus are gone. It starts `inayos-plasma-layout.service` when displays change, and it closes Overview after a desktop switch.

Plasma 6.8 crashes when a panel of an unplugged display is changed, so both scripts leave those panels alone.

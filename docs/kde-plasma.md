# KDE Plasma

This document describes the CachyOS KDE Plasma setup: the `linux/modules/kde-plasma.sh` module and the `plasma` dotfile package.

## Theme and settings

The module applies KDE's stock Breeze Dark global theme with personal settings: the pink accent color, IBM Plex fonts, input settings and window rules.

The interface uses IBM Plex Sans and the fixed-width font is BlexMono Nerd Font Mono, the IBM Plex Mono copy with Nerd Font icons. The `plasma` package's fontconfig file makes IBM Plex Sans, IBM Plex Serif and BlexMono Nerd Font Mono the defaults for programs that ask for sans-serif, serif or monospace text.

## Panel layout

The `plasma` package owns the panel layout. `myconfig-plasma-layout` sends `~/.local/share/myconfig-plasma/layout.js` to Plasma. The layout gives every display a top bar and a dock. The top bar holds a KDE session menu (the `myconfig.session` widget), the global menu, the tray and the clock. The dock holds the application launcher, a one-click Overview icon and the task icons.

The first run backs up and replaces every existing panel. Later runs add bars for new displays and reset the managed ones. `myconfig-plasma-layout.service` runs the command at each Plasma login.

## Bar behavior

The top bar stays visible, and maximized windows stop below it. On a display that shows a fullscreen window, the top bar hides. The dock stays hidden until the pointer reaches the bottom edge.

The KWin script `myconfig-panels` handles this behavior. It shows a hidden bar while the pointer is near its screen edge, even over a fullscreen window, and hides it again once the pointer and the bar's menus are gone. It starts `myconfig-plasma-layout.service` when displays change, and it closes Overview after a desktop switch.

Plasma 6.8 crashes when a panel of an unplugged display is changed, so both scripts leave those panels alone.

## Hidden launcher entries

`myconfig-launcher-hide` hides the launcher entries of development tools that required packages install. The script lists those entries. KDE ignores `Hidden=true` overrides, so the command copies each installed entry into `~/.local/share/applications/` with `NoDisplay=true`. The KDE module and the zsh `update` command run it, so the copies follow package updates.

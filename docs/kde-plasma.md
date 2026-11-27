# KDE Plasma

This document describes the CachyOS KDE Plasma setup: the `kde-plasma` module in `crates/modules/src/kde_plasma.rs` and the `plasma` dotfile package.

## Packages

The module installs the Plasma desktop package by package instead of through `plasma-meta`: every package that `plasma-meta` holds except `plasma-browser-integration`, plus `plasma-disks`, `drkonqi` and `flatpak-kcm`. `verify` fails when one is missing. The CachyOS setup removes `plasma-browser-integration`, and the `printing` module adds CUPS with KDE's printer settings.

## Theme and settings

The module applies KDE's stock Breeze Dark global theme with personal settings: the Lilas color scheme, the pink accent color, IBM Plex fonts, input settings and window rules.

The `plasma` package installs the Lilas color scheme, a dark lilac scheme that replaces the blue-grey of Breeze Dark. The module applies it after the global theme, because the global theme applies the Breeze Dark colors, and applies the accent after the scheme, because the accent recolors it. Its colors come from the desktop video and from the user's preference for pastel mint and lilac:

| Role                              | Color     |
| --------------------------------- | --------- |
| Window background                 | `#241c33` |
| Lists and text areas              | `#1c1528` |
| Buttons, headers and title bars   | `#2e2442` |
| Text                              | `#f1e8f8` |
| Secondary text                    | `#b8a7cc` |
| Lilac, for focus and active items | `#b296cc` |
| Mint, for positive messages       | `#9ee6c8` |
| Orange, for warnings              | `#ffb38a` |
| Pink, for errors                  | `#ff6b8b` |
| Sky blue, for links               | `#9cc8f0` |
| Accent, for selection             | `#ff4ead` |

Breeze draws menus at 80% opacity, and KWin blurs what shows behind them. The blur strength is 3 out of 15, so the wallpaper stays recognizable behind the bars and menus.

The session uses a US keyboard layout whose right Alt key is the Compose key: press and release it, then type a sequence such as `'` `e` for é. The login and lock screens keep the system layout from `localectl`.

The interface uses IBM Plex Sans and the fixed-width font is BlexMono Nerd Font Mono, the IBM Plex Mono copy with Nerd Font icons. The `plasma` package's fontconfig file makes IBM Plex Sans, IBM Plex Serif and BlexMono Nerd Font Mono the defaults for programs that ask for sans-serif, serif or monospace text.

## Wallpapers

The desktop and the lock screen play muted, looping videos through the Smart Video Wallpaper Reborn plugin, which the module installs from the AUR with the FFmpeg backend of Qt Multimedia.

`dotfiles/assets/wallpapers/original/` keeps the videos as downloaded from MoeWalls. `dotfiles/assets/wallpapers/reencoded/` holds the same videos scaled to a height of 1200 pixels without sound, which costs less to decode. The binary embeds only the re-encoded videos, and the module copies them to `~/.local/share/wallpapers/myconfig/`.

The desktop plays `anime-cat-girl-lying-down-in-bed.mp4`. The layout script sets it on every desktop the first time it runs, together with the plugin and its battery threshold, so a wallpaper the user picks later stays. The video pauses behind a maximized or fullscreen window and on battery below 30%.

The lock screen plays `lofi-pink-town.mp4`. The module sets it in `kscreenlockerrc` on every install, and `verify` checks it.

## Panel layout

The `plasma` package owns the panel layout. `myconfig-plasma-layout` sends `~/.local/share/myconfig-plasma/layout.js` to Plasma. The layout gives every display a top bar and a dock. The top bar holds a KDE session menu (the `myconfig.session` widget), the global menu, the tray and the clock. The dock holds a one-click Overview icon and the task icons. A new dock pins Overview, Chrome, Herdr, LyX and Qalculate. A rebuilt dock keeps its own pins, except that the pins of T3 Code, VSCodium and Ghostty make way for one Herdr pin. The task widget shows a blank icon for a pin whose application is missing, so the CachyOS profile installs each pinned application.

The session menu shows a one-color Tux in the bar's text color. The drawing comes from CachyOS's char-white icon theme, which is GPL, so the widget is GPL. Its labels reuse the translations that KDE installs, so the widget stores no text in any language and follows the system language.

The tray keeps every Plasma widget in its popup, so the bar shows only the icons of applications.

The first run replaces every existing panel. Later runs add bars for new displays and reset the managed ones. `myconfig-plasma-layout.service` runs the command at each Plasma login.

The module records the panel file before the first run. Plasma keeps its panels in memory and saves them when it stops, so `remove` stops a running Plasma shell, restores the recorded file, and starts the shell again.

## Bar behavior

The top bar stays visible, and maximized windows stop below it. On a display that shows a fullscreen window, the top bar hides. The dock stays hidden until the pointer reaches the bottom edge.

The KWin script `myconfig-panels` handles this behavior. It shows a hidden bar while the pointer is near its screen edge, even over a fullscreen window, and hides it again once the pointer and the bar's menus are gone. It starts `myconfig-plasma-layout.service` when displays change, and it closes Overview after a desktop switch.

Plasma 6.8 crashes when a panel of an unplugged display is changed, so both scripts leave those panels alone.

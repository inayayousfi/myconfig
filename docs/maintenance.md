# Maintenance

This document describes the `maintenance` module of the InayOS desktop profile, `crates/modules/src/maintenance.rs`, its files in `dotfiles/assets/maintenance`, and the `inayos-news` program in `apps/news`.

The module takes over three chores that Arch leaves to the user: reading the Arch news before an update that needs a manual step, merging `.pacnew` files, and ranking the mirrors. The snapshot cleanup is the `snapshots` module's, as `docs/snapshots.md` describes. Arch's own `archlinux-keyring-wkd-sync.timer` keeps the keyring current.

## Arch news

`inayos-news` is a small static program, package `inayos-news-app` in `apps/news`. The `inayos` package installs it as `/usr/bin/inayos-news`, so the module needs the `inayos-repository` module and lists the `inayos` package in its footprint.

It fetches the feed with `curl`, which every Arch system has because pacman needs it, with a 10 second limit. The feed address comes from the `INAYOS_NEWS_URL` environment variable, else from the `url=` line of `/etc/inayos-news.conf`, else it is `https://archlinux.org/feeds/news/`. The module installs that file with the Arch feed.

An item announces a manual step when its title or text contains one of the phrases the Arch news uses for it, compared without case: "manual intervention", "intervention required", "requires manual", "action required", "action is required", "needs to be restarted" and "must be restarted".

`/var/lib/inayos-news/read` holds the read mark: the publication time of the newest item read, in seconds since 1970 UTC. An item published after it is unread. Without the file, every item is unread. The folder has mode 2775 and belongs to root and the `wheel` group, so new files in it get that group and the user marks the news read without sudo. `inayos-news` replaces the mark by renaming a new file over it.

The program has three modes:

- `inayos-news` prints every unread item, oldest first, with its date, title, link and text, then moves the read mark to the newest of them. It exits 1 when the feed cannot be read or the mark cannot be written.
- `inayos-news --pacman` prints each unread item that announces a manual step, with its date and link, and the command that reads it. It never changes the read mark. It always exits 0: a network error prints one line.
- `inayos-news --unread-manual` prints the title of each unread item that announces a manual step, one per line. It exits 1 when the feed cannot be read.

The pacman hook `/etc/pacman.d/hooks/05-inayos-news.hook` runs `inayos-news --pacman` before every transaction that upgrades a package. It has no `AbortOnFail`, so the news never stops a transaction. pacman shows what it prints before the packages change.

The user unit `inayos-news-notify.service` starts with the graphical session. It runs `/usr/local/libexec/inayos-news-notify`, which shows a notification with the titles from `inayos-news --unread-manual` when there are any. The network often comes up after login, so while the feed cannot be read the script tries again every 30 seconds, for up to 10 minutes. Its Read button opens `inayos-news` in Ghostty, which marks the news read.

## .pacnew files

`inayos-pacnew.service` runs `pacdiff --output`, from `pacman-contrib`, which lists the `.pacnew` files of the configuration files that the pacman database names. When the list is not empty, the service writes it to `/var/lib/inayos-pacnew/list`; otherwise it deletes that file. systemd creates the folder as the service's state folder. `inayos-pacnew.timer` runs the service every day, and at boot after a missed day. The pacman hook `zz-inayos-pacnew.hook` starts it after every transaction that installs or upgrades a package, without waiting for it.

The user unit `inayos-pacnew-notify.path` starts `inayos-pacnew-notify.service` while the list exists. The service runs `/usr/local/libexec/inayos-pacnew-notify`, which names the files in a notification. Its Merge button opens a Ghostty terminal that runs `DIFFPROG=kompare pacdiff --sudo`, so each file opens in Kompare and pacdiff uses sudo to replace or remove it. The terminal then starts `inayos-pacnew.service` again, which deletes the list when nothing is left. The service stays active while the list exists, so the path unit does not start it again in a loop. While it waits, it reads the list every minute: when a later search lists other files, it shows a new notification, and when the list is gone, it ends.

## Mirrors

`reflector.timer`, from the `reflector` package, runs reflector every week with the options in `/etc/xdg/reflector/reflector.conf`. The module writes that file: the 20 most recently synced HTTPS mirrors, sorted by download rate, saved as `/etc/pacman.d/mirrorlist`. When the time zone that `/etc/localtime` links to belongs to exactly one country in tzdata's `/usr/share/zoneinfo/zone.tab`, the options keep only that country's mirrors. A zone of several countries or none, such as `UTC`, gives no country.

The drop-in `/etc/systemd/system/reflector.service.d/inayos-maintenance.conf` copies the mirror list before reflector runs. When the list reflector leaves holds no `Server` line, such as when no mirror matched, the copy comes back, so pacman always has mirrors.

## Install

Install installs `inayos`, `curl`, `pacman-contrib`, `reflector`, `kompare`, `libnotify` and `ghostty`. It creates the news folder with mode 2775 and the `wheel` group when it is missing, and writes the read mark with the current time when it is missing: the system has current packages, so earlier news announces nothing left to do. It records the `.pacnew` folder, which systemd creates later, so `remove` deletes it.

It then writes every file of `dotfiles/assets/maintenance` at the same path under `/`, with mode 755 for the scripts in `/usr/local/libexec` and 644 for the rest, and writes the reflector options. It enables and starts `reflector.timer` and `inayos-pacnew.timer`, and enables the user units `inayos-news-notify.service` and `inayos-pacnew-notify.path`. In a graphical session it also starts the path unit. In installer mode the timers are only enabled.

`verify` fails when a package is missing, when `/usr/bin/inayos-news` is not executable, when a file differs from what install writes, when the reflector options are not the ones install writes for some country or for none, when the news folder does not have mode 2775 and the `wheel` group, when a timer is not running, or when a user unit is not enabled. In installer mode it checks that the timers are enabled instead.

## Remove

`remove` disables the units, deletes the files, the news folder and the `.pacnew` folder, puts back the reflector options that the package shipped, and uninstalls the packages that install added.

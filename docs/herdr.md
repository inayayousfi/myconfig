# Herdr

This document describes the Herdr modules, `crates/modules/src/herdr.rs`, and the `herdr` package.

[Herdr](https://herdr.dev) has two programs. herdr-core, whose command is `hc`, is a background server that keeps terminals and agents running while no window shows them. The Herdr app is a desktop window on top of it, and it reaches the herdr-core of other machines over SSH.

The `herdr-core` module installs herdr-core on every Linux profile. Arch and CachyOS install the AUR package `herdr-core-bin`. The Ubuntu Devbox profile installs Herdr's release archive through `crates/modules/src/release_archive.rs`, checked against the SHA-256 pinned in the module, and links `herdr-core` and `hc` into `~/.local/bin`.

The `herdr` module installs the app, the AUR package `herdr-app-bin`, on CachyOS only. It stows the `herdr` package into `~/.config/herdr-app`:

- `settings.json` turns telemetry off and sets the Black & Pink theme and the BlexMono font for every layout that leaves them unset.
- `themes/blacknpink.json` holds the Ghostty palette and the panel colors.
- `layouts/blacknpink.json` is a copy of Herdr's `main` layout with the theme, the font, window blur and a see-through background.

Herdr replaces its own layouts and themes on every start, so `~/.config/herdr-app` and its `layouts` and `themes` folders stay real directories. Herdr writes its settings through the links, so a change made in its settings screen lands in the deployed package.

Herdr keeps the shown layout in `shared.json`, beside its other shared state. The module sets its `layout` key to `blacknpink`, and `remove` puts back the earlier value.

Right-clicks in a Herdr terminal open Herdr's menu unless the terminal carries the label `herdr.right-click=program`. The Zsh plugin sets that label in every Herdr shell, so programs that use the mouse, such as Neovim, receive right-clicks. Shift+right-click always opens Herdr's menu.

The `ai` package ships the `herdr` agent skill, which tells agents to run helpers, background tasks and long commands through `hc` when they run inside Herdr.

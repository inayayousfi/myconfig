# Neovim

This document describes the Neovim module, `crates/modules/src/neovim.rs`, and the `nvim` package, which Stow links into `~/.config/nvim`.

The config needs Neovim 0.12, because it installs its plugins with Neovim's built-in `vim.pack`. Arch and CachyOS install the `neovim` package. Ubuntu carries an older Neovim, so the Ubuntu Devbox profile installs the official release archive instead. `crates/modules/src/release_archive.rs` downloads it, checks it against the SHA-256 pinned in the module, unpacks it into `~/.local/opt/nvim-<version>`, and links `~/.local/bin/nvim` to it.

`~/.config/nvim` stays a real directory, because Neovim writes its plugin lockfile `nvim-pack-lock.json` beside the config. Plugins live in `~/.local/share/nvim/site/pack/core/opt` and install on the first start.

## The mouse layer

`lua/mouse.lua` makes Neovim usable with the mouse, like a desktop editor:

- File buffers stay in insert mode. A click places the cursor, dragging or Shift with the arrow keys selects, and typing replaces the selection. F2, or the mode label at the left of the bottom bar, switches to Vim's normal mode and back.
- Ctrl+C, Ctrl+X and Ctrl+V copy, cut and paste with the system clipboard. Ctrl+Z, Ctrl+Y, Ctrl+A and Ctrl+S undo, redo, select everything and save. Escape only closes the completion menu.
- The bottom bar holds clickable buttons for a command menu, the file tree, the search, the code actions, formatting, undo and redo, then the file name and position.
- The right-click menu adds code actions, rename, references, formatting and the file tree to Neovim's own entries.
- Every choice that Neovim asks through `vim.ui.select`, such as a code action, opens as a native popup menu at the mouse.
- satellite.nvim draws a scrollbar that also marks diagnostics, Git changes and search matches.

Files save after one second without typing. Formatting runs when a file or the window loses focus and from the Format button, never while typing, because it would move the text under the cursor.

In [Herdr](herdr.md), right-clicks reach Neovim because the Zsh plugin labels every Herdr terminal to send them to the program.

# Neovim

This document describes the Neovim module, `crates/modules/src/neovim.rs`, and the `nvim` package, which Stow links into `~/.config/nvim`.

The config needs Neovim 0.12, because it installs its plugins with Neovim's built-in `vim.pack`. Arch and CachyOS install the `neovim` package. Ubuntu carries an older Neovim, so the Ubuntu Devbox profile installs the official release archive instead. `crates/modules/src/release_archive.rs` downloads it, checks it against the SHA-256 pinned in the module, unpacks it into `~/.local/opt/nvim-<version>`, and links `~/.local/bin/nvim` to it.

The config uses its own Lilas color scheme, `colors/lilas.lua`, which follows `stylesheet.md` at the repository root and leaves window and float backgrounds transparent, so the terminal's see-through background shows behind the text.

`~/.config/nvim` stays a real directory, because Neovim writes its plugin lockfile `nvim-pack-lock.json` beside the config. Plugins live in `~/.local/share/nvim/site/pack/core/opt` and install on the first start.

## Mouse-first editing

The config uses [normie.nvim](https://github.com/inayayousfi/normie.nvim), a separate plugin that makes Neovim usable with the mouse like a desktop editor. Its README and `:help normie` describe it. In short: files stay in insert mode (Normie mode) until F2 or the mode label switches to Vim mode, the top line shows open files as tabs, the bottom bar holds buttons, the right-click menu follows what lies under the mouse, and the scrollbar can be pressed and dragged anywhere.

`init.lua` loads it after its supporting plugins, which it uses when they are present: satellite.nvim draws the scrollbar with marks for diagnostics, Git changes and search matches, nvim-tree serves the Files button, fff.nvim the Search button, conform.nvim the Format button, and blink.cmp's menu closes on Escape. The config turns on `format_on_leave`.

Files save after one second without typing. Formatting runs when a file or the window loses focus and from the Format button, never while typing, because it would move the text under the cursor.

In [Herdr](herdr.md), right-clicks reach Neovim because the Zsh plugin labels every Herdr terminal to send them to the program.

-- Vim defaults
vim.g.loaded_netrw = 1
vim.g.loaded_netrwPlugin = 1
vim.g.mapleader = " "
vim.g.maplocalleader = "\\"
vim.opt.number = true
vim.opt.relativenumber = false
vim.opt.wrap = false
vim.opt.tabstop = 2
vim.opt.shiftwidth = 2
vim.opt.expandtab = true
vim.opt.clipboard = "unnamedplus"
vim.opt.ttimeout = true
vim.opt.ttimeoutlen = 10
vim.opt.timeout = false
vim.opt.signcolumn = "yes"
vim.opt.updatetime = 1000 -- pause length before saving while typing
vim.opt.shell = "zsh"
vim.opt.shellcmdflag = "-ic"

-- Plugin manager
local function gh(repo, branch)
  return {
    src = "https://github.com/" .. repo,
    version = branch,
  }
end

vim.api.nvim_create_augroup("NativePackHooks", { clear = true })

vim.api.nvim_create_autocmd("PackChanged", {
  group = "NativePackHooks",
  callback = function(ev)
    local data = ev.data
    if not data or not data.spec or (data.kind ~= "install" and data.kind ~= "update") then
      return
    end

    if data.spec.name == "fff.nvim" then
      vim.cmd.packadd({ data.spec.name, magic = { file = false } })
      local ok, downloader = pcall(require, "fff.download")
      if ok then
        downloader.download_or_build_binary()
      end
      return
    end

    if data.spec.name == "nvim-treesitter" then
      vim.cmd.packadd({ data.spec.name, magic = { file = false } })
      pcall(vim.cmd.TSUpdate)
    end
  end,
})

vim.api.nvim_create_user_command("Update", function()
  vim.pack.update(nil, { force = true })
end, {})

vim.pack.add({
  gh("blacknpink/blacknpink.nvim", "main"),
  gh("ccntrq/autoreload.nvim", "master"),
  gh("Pocco81/auto-save.nvim", "main"),
  gh("rachartier/tiny-inline-diagnostic.nvim", "main"),
  gh("folke/flash.nvim", "main"),
  gh("dmtrKovalenko/fff.nvim", "main"),
  gh("nvim-tree/nvim-web-devicons", "master"),
  gh("nvim-tree/nvim-tree.lua", "master"),
  gh("mason-org/mason.nvim", "main"),
  gh("mason-org/mason-lspconfig.nvim", "main"),
  gh("folke/lazydev.nvim", "main"),
  gh("neovim/nvim-lspconfig", "master"),
  gh("rafamadriz/friendly-snippets", "main"),
  gh("Saghen/blink.cmp", "v1"),
  gh("nvim-treesitter/nvim-treesitter", "main"),
  gh("stevearc/conform.nvim", "master"),
  gh("lewis6991/gitsigns.nvim", "main"),
  gh("lewis6991/satellite.nvim", "main"),
  gh("inayayousfi/normie.nvim", "main"),
}, { confirm = false })

-- Keymaps
vim.keymap.set("n", "<leader>uv", "<cmd>ASToggle<CR>", { desc = "Toggle autosave" })

vim.keymap.set("n", "<leader><leader>", function()
  require("fff").live_grep()
end, { desc = "Search content or files" })

vim.keymap.set({ "n", "x", "o" }, "f", function()
  require("flash").jump()
end, { desc = "Flash" })

vim.keymap.set("n", "<leader>l", function()
  require("nvim-tree.api").tree.toggle()
end, { desc = "Toggle file tree" })

vim.keymap.set("n", "<leader>j", "<cmd>wincmd l<CR>", { desc = "Move to right window" })
vim.keymap.set("n", "<leader>k", "<cmd>wincmd h<CR>", { desc = "Move to left window" })
vim.keymap.set("n", "<leader>x", "<cmd>bdelete!<CR>", { desc = "Close buffer" })
vim.keymap.set("t", "<C-Esc>", [[<C-\><C-n>]], { desc = "Enter normal mode" })

-- Plugin setup
require("gitsigns").setup({
  signcolumn = true,
  current_line_blame = true,
  current_line_blame_opts = {
    delay = 50,
    virt_text_pos = "eol",
  },
})

require("autoreload").setup({})

-- The terminal draws a see-through, blurred background, so black and
-- near-black backgrounds, such as the menu's, are left to it.
local function near_black(color)
  return color
    and math.floor(color / 0x10000) <= 0x10
    and math.floor(color / 0x100) % 0x100 <= 0x10
    and color % 0x100 <= 0x10
end

vim.api.nvim_create_autocmd("ColorScheme", {
  group = vim.api.nvim_create_augroup("transparent-background", {}),
  callback = function()
    for name, spec in pairs(vim.api.nvim_get_hl(0, {})) do
      if near_black(spec.bg) and not spec.link then
        spec.bg = nil
        vim.api.nvim_set_hl(0, name, spec)
      end
    end
  end,
})
vim.cmd.colorscheme("blacknpink")

require("auto-save").setup({
  enabled = true,
  debounce_delay = 500,
  execution_message = {
    message = function()
      return "Auto-saved at " .. vim.fn.strftime("%H:%M:%S")
    end,
  },
  write_all_buffers = true,
  -- Typing never leaves insert mode, so save on pauses while typing too.
  trigger_events = { "InsertLeave", "TextChanged", "CursorHoldI" },
})

require("tiny-inline-diagnostic").setup()
vim.diagnostic.config({ virtual_text = false })

require("flash").setup({})

require("nvim-tree").setup({
  view = {
    side = "right",
    width = "20%",
  },
  actions = {
    open_file = {
      quit_on_open = true,
    },
  },
  update_focused_file = {
    enable = true,
    update_root = { enable = true },
  },
})

require("blink.cmp").setup({
  keymap = { preset = "enter" },
  appearance = {
    use_nvim_cmp_as_default = true,
    nerd_font_variant = "mono",
  },
  completion = {
    documentation = { auto_show = true },
  },
  sources = {
    default = { "lsp", "path", "snippets", "buffer" },
  },
  fuzzy = {
    implementation = "prefer_rust_with_warning",
    prebuilt_binaries = {
      force_version = "v*",
    },
  },
})

require("conform").setup({})

require("mason").setup({})

require("mason-lspconfig").setup({
  ensure_installed = { "lua_ls", "stylua" },
  automatic_enable = false,
})

require("lazydev").setup({
  library = {
    { path = "${3rd}/luv/library", words = { "vim%.uv" } },
  },
})

-- LSP setup
vim.lsp.config("*", {
  capabilities = require("blink.cmp").get_lsp_capabilities(),
})

vim.api.nvim_create_autocmd("LspAttach", {
  group = vim.api.nvim_create_augroup("UserLspConfig", { clear = true }),
  callback = function(ev)
    local opts = { buffer = ev.buf }
    vim.keymap.set("n", "gd", vim.lsp.buf.definition, opts)
    vim.keymap.set("n", "gD", vim.lsp.buf.declaration, opts)
    vim.keymap.set("n", "gk", vim.lsp.buf.hover, opts)
  end,
})

for _, server_name in ipairs(require("mason-lspconfig").get_installed_servers()) do
  local ok, err = pcall(vim.lsp.enable, server_name)
  if not ok then
    vim.notify(("Failed to enable LSP server %s: %s"):format(server_name, err), vim.log.levels.WARN)
  end
end

require("satellite").setup({ winblend = 0 })

-- Mouse-first editing: Normie mode, clickable bar and tabs, menus at the mouse.
require("normie").setup({ format_on_leave = true })

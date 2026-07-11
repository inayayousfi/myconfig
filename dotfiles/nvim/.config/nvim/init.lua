vim.g.loaded_netrw = 1
vim.g.loaded_netrwPlugin = 1
vim.g.mapleader = " "
vim.g.maplocalleader = "\\"
vim.opt.number = true
vim.opt.relativenumber = true
vim.opt.wrap = false
vim.opt.tabstop = 2
vim.opt.shiftwidth = 2
vim.opt.expandtab = true
vim.opt.clipboard = "unnamedplus"
vim.opt.ttimeout = true
vim.opt.ttimeoutlen = 10
vim.opt.timeout = false
vim.opt.hidden = false
vim.opt.signcolumn = "yes"
vim.opt.shell = "zsh"
vim.opt.shellcmdflag = "-ic"

vim.keymap.set("n", "<leader>uv", "<cmd>ASToggle<CR>", { desc = "Toggle autosave" })
vim.keymap.set("n", "<leader><leader>", function()
  require("fff").find_files()
end, { desc = "Find files" })
vim.keymap.set({ "n", "x", "o" }, "f", function()
  require("flash").jump()
end, { desc = "Flash" })
vim.keymap.set("n", "<leader>l", function()
  require("nvim-tree.api").tree.toggle()
end, { desc = "Toggle file tree" })

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
  gh("github/copilot.vim", "release"),
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
}, { confirm = false })

vim.cmd.colorscheme("blacknpink")

require("auto-save").setup({
  debounce_delay = 500,
  execution_message = {
    message = function()
      return ""
    end,
  },
})

require("tiny-inline-diagnostic").setup()
vim.diagnostic.config({ virtual_text = false })

require("flash").setup({})

require("nvim-tree").setup({
  on_attach = function(bufnr)
    local api = require("nvim-tree.api")
    api.config.mappings.default_on_attach(bufnr)

    vim.keymap.set("n", "<LeftRelease>", function()
      local node = api.tree.get_node_under_cursor()
      if node then
        api.node.open.edit()
      end
    end, { buffer = bufnr, noremap = true, silent = true, nowait = true })
  end,
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
    update_root = true,
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

require("conform").setup({
  format_on_save = {
    timeout_ms = 500,
    lsp_format = "fallback",
  },
})

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

vim.lsp.config("*", {
  capabilities = require("blink.cmp").get_lsp_capabilities(),
})

vim.api.nvim_create_autocmd("LspAttach", {
  group = vim.api.nvim_create_augroup("UserLspConfig", { clear = true }),
  callback = function(ev)
    local opts = { buffer = ev.buf }
    vim.keymap.set("n", "gd", vim.lsp.buf.definition, opts)
    vim.keymap.set("n", "gD", vim.lsp.buf.declaration, opts)
    vim.keymap.set("n", "K", vim.lsp.buf.hover, opts)
  end,
})

for _, server_name in ipairs(require("mason-lspconfig").get_installed_servers()) do
  local ok, err = pcall(vim.lsp.enable, server_name)
  if not ok then
    vim.notify(("Failed to enable LSP server %s: %s"):format(server_name, err), vim.log.levels.WARN)
  end
end

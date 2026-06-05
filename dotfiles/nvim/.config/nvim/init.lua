vim.g.loaded_netrw = 1
vim.g.loaded_netrwPlugin = 1
vim.g.mapleader = " "
vim.g.maplocalleader = "\\"

if vim.fn.has("win32") == 1 then
  local py = vim.fn.exepath("py")
  if py == "" then
    py = vim.fn.exepath("python")
  end
  if py ~= "" then
    local real_python = vim.fn.system('py -c "import sys; print(sys.executable)"'):gsub("%s+$", "")
    if real_python ~= "" then
      local real_dir = vim.fn.fnamemodify(real_python, ":h")
      vim.env.PATH = real_dir .. ";" .. vim.env.PATH
    end
  end
end

vim.opt.number = true
vim.opt.relativenumber = true
vim.opt.wrap = false
vim.opt.tabstop = 2
vim.opt.shiftwidth = 2
vim.opt.expandtab = true
vim.opt.clipboard = "unnamedplus"
vim.opt.ttimeout = false
vim.opt.timeout = false
vim.opt.hidden = false
vim.opt.signcolumn = "yes"

if vim.fn.has("win32") == 1 or vim.fn.has("win64") == 1 then
  vim.opt.shell = "pwsh"
  vim.opt.shellcmdflag = "-NoLogo -Command"
  vim.opt.shellquote = ""
  vim.opt.shellxquote = ""
else
  vim.opt.shell = "zsh"
  vim.opt.shellcmdflag = "-ic"
end

vim.api.nvim_create_autocmd("BufEnter", {
  callback = function()
    local current = vim.api.nvim_get_current_buf()
    local buftype = vim.api.nvim_get_option_value("buftype", { buf = current })

    if buftype ~= "" then
      return
    end

    local jumplist = vim.fn.getjumplist()[1]
    for i = #jumplist, 1, -1 do
      local entry = jumplist[i]
      local entry_buftype = vim.api.nvim_get_option_value("buftype", { buf = entry.bufnr })
      if entry_buftype ~= "" then
        vim.cmd(i .. "clearjumps")
      end
    end

    for _, buf in ipairs(vim.api.nvim_list_bufs()) do
      if
          buf ~= current
          and vim.api.nvim_buf_is_valid(buf)
          and vim.api.nvim_get_option_value("buflisted", { buf = buf })
      then
        local entry_buftype = vim.api.nvim_get_option_value("buftype", { buf = buf })
        local modified = vim.api.nvim_get_option_value("modified", { buf = buf })

        if entry_buftype == "" and not modified and vim.fn.getbufinfo(buf)[1].hidden == 1 then
          vim.cmd.bdelete(buf)
        end
      end
    end
  end,
})

vim.keymap.set("n", "<leader>ce", "<cmd>Copilot enable<CR>", { desc = "Toggle Copilot on" })
vim.keymap.set("n", "<leader>cd", "<cmd>Copilot disable<CR>", { desc = "Toggle Copilot off" })
vim.keymap.set("n", "<leader>uv", "<cmd>ASToggle<CR>", { desc = "Toggle autosave" })
vim.keymap.set("n", "<leader><leader>", function()
  require("fff").find_files()
end, { desc = "Find files" })
vim.keymap.set({ "n", "x", "o" }, "f", function()
  require("flash").jump()
end, { desc = "Flash" })
vim.keymap.set("n", "<leader>e", function()
  local api = require("nvim-tree.api")
  local view = require("nvim-tree.view")

  if view.is_visible() then
    api.tree.close()
    return
  end

  local current = vim.api.nvim_get_current_buf()
  api.tree.open()

  local is_normal = vim.bo[current].buftype == ""
  local is_valid = vim.api.nvim_buf_is_valid(current)
  local is_modified = vim.bo[current].modified

  if is_normal and is_valid and not is_modified then
    vim.cmd("bdelete " .. current)
  end
end, { desc = "Open file tree and kill buffer" })

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

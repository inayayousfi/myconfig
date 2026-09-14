-- Vim defaults
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
vim.opt.mouse = "a"
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
}, { confirm = false })

-- OpenCode integration
local opencode = {
  job_id = nil,
  buffer = nil,
}

local function opencode_context(buf, cursor, cwd)
  local path = vim.api.nvim_buf_get_name(buf)
  if path == "" then
    return nil
  end

  local relative_path = vim.fs.relpath(cwd, path) or path
  return relative_path .. string.format(":L%d:C%d", cursor[1], cursor[2] + 1)
end

local function opencode_send(text)
  vim.api.nvim_chan_send(opencode.job_id, "\27[200~" .. text .. "\27[201~")
end

local function opencode_open_context()
  local buf = vim.api.nvim_get_current_buf()
  local cursor = vim.api.nvim_win_get_cursor(0)
  local cwd = vim.fn.getcwd()
  local context = opencode_context(buf, cursor, cwd)
  if not context then
    return
  end

  if not opencode.job_id or not opencode.buffer or not vim.api.nvim_buf_is_valid(opencode.buffer) then
    vim.cmd("leftabove vnew")
    vim.cmd("vertical resize " .. math.max(1, math.floor(vim.o.columns * 0.3)))
    vim.wo.winfixwidth = true
    local job_id
    job_id = vim.fn.jobstart({ "opencode", cwd, "--auto" }, {
      term = true,
      on_exit = function()
        vim.schedule(function()
          if opencode.job_id == job_id then
            opencode.job_id = nil
            opencode.buffer = nil
          end
        end)
      end,
    })
    opencode.job_id = job_id
    opencode.buffer = vim.api.nvim_get_current_buf()
    vim.api.nvim_create_autocmd("BufWipeout", {
      buffer = opencode.buffer,
      once = true,
      callback = function()
        if opencode.job_id == job_id then
          vim.fn.jobstop(job_id)
          opencode.job_id = nil
          opencode.buffer = nil
        end
      end,
    })
    vim.keymap.set("n", "<LeftRelease>", "<Cmd>startinsert<CR>", { buffer = opencode.buffer, silent = true })
    vim.defer_fn(function()
      if opencode.job_id == job_id then
        opencode_send(context .. ": ")
      end
    end, 2000)
  else
    local window = vim.fn.bufwinid(opencode.buffer)
    if window ~= -1 then
      vim.api.nvim_set_current_win(window)
    end
    opencode_send(context .. ": ")
  end
  vim.cmd("startinsert")
end

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
vim.keymap.set({ "n", "v", "i" }, "<RightMouse>", "<Nop>")
vim.keymap.set({ "n", "v", "i" }, "<RightDrag>", "<Nop>")
vim.keymap.set({ "n", "v", "i" }, "<RightRelease>", "<Nop>")
vim.keymap.set({ "n", "v", "i" }, "<LeftDrag>", "<Nop>")
vim.keymap.set("t", "<C-Esc>", [[<C-\><C-n>]], { desc = "Enter normal mode" })

vim.keymap.set({ "n", "x" }, "<leader>a", function()
  opencode_open_context()
end, { desc = "Open OpenCode context" })

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
  callbacks = {
    before_saving = function()
      require("conform").format({ lsp_format = "fallback" })
    end,
  },
})

require("tiny-inline-diagnostic").setup()
vim.diagnostic.config({ virtual_text = false })

require("flash").setup({})

require("nvim-tree").setup({
  on_attach = function(bufnr)
    local api = require("nvim-tree.api")
    api.map.on_attach.default(bufnr)

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

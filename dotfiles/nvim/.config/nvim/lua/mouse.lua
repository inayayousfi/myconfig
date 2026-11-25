-- Mouse-first layer: always typing, a clickable bar, right-click menus and a scrollbar.

vim.opt.relativenumber = false
vim.opt.mousemodel = "popup_setpos"
vim.opt.selectmode = { "mouse", "key" } -- dragging or Shift+arrows select; typing replaces
vim.opt.keymodel = { "startsel", "stopsel" }
vim.opt.laststatus = 3 -- one bar at the bottom for the whole window
vim.opt.showmode = false
vim.opt.updatetime = 1000 -- pause length before saving while typing

local map = vim.keymap.set

-- Typing mode ---------------------------------------------------------------
-- File buffers go back to insert mode whenever they land in normal mode,
-- unless Vim mode is switched on (F2 or the bar button).

vim.g.vim_mode = false

local function is_file_buffer()
  return vim.bo.buftype == "" and vim.bo.modifiable
end

local function back_to_typing()
  vim.schedule(function()
    if not vim.g.vim_mode and vim.api.nvim_get_mode().mode == "n" and is_file_buffer() then
      vim.cmd.startinsert()
    end
  end)
end

vim.api.nvim_create_autocmd("ModeChanged", { pattern = "*:n", callback = back_to_typing })
vim.api.nvim_create_autocmd({ "BufEnter", "WinEnter", "VimEnter" }, { callback = back_to_typing })

local function toggle_vim_mode()
  vim.g.vim_mode = not vim.g.vim_mode
  if vim.g.vim_mode then
    vim.cmd.stopinsert()
  else
    back_to_typing()
  end
  vim.cmd.redrawstatus()
end

map({ "n", "i", "v" }, "<F2>", toggle_vim_mode, { desc = "Toggle Vim mode" })

-- Escape only closes the completion menu while typing.
map("i", "<Esc>", function()
  if vim.g.vim_mode then
    return "<Esc>"
  end
  pcall(function()
    require("blink.cmp").hide()
  end)
  return ""
end, { expr = true })

-- Usual editor shortcuts.
map("i", "<C-z>", "<Cmd>undo<CR>")
map("i", "<C-y>", "<Cmd>redo<CR>")
map("i", "<C-s>", "<Cmd>update<CR>")
map("i", "<C-a>", "<C-\\><C-n>gg0vG$")
map("i", "<C-v>", "<C-r><C-o>+")
map("v", "<C-c>", '"+y')
map("v", "<C-x>", '"+d')
map("v", "<C-v>", '"+P')
map("v", "<BS>", '"_d')
map("v", "<Del>", '"_d')

-- Formatting -----------------------------------------------------------------
-- Formatting while typing would move text under the cursor, so it runs when
-- leaving a file or the window, and from the Format button.

local function format()
  require("conform").format({ lsp_format = "fallback" })
end

vim.api.nvim_create_autocmd({ "BufLeave", "FocusLost" }, {
  callback = function(ev)
    if vim.bo[ev.buf].buftype == "" and vim.bo[ev.buf].modified then
      require("conform").format({ bufnr = ev.buf, lsp_format = "fallback" })
      vim.api.nvim_buf_call(ev.buf, function()
        vim.cmd("silent! update")
      end)
    end
  end,
})

-- Clickable menus --------------------------------------------------------------
-- vim.ui.select (code actions, LSP choices) and the Command button open a
-- native popup menu at the mouse, so every choice is one click.

_G.MouseBar = {}
local pending = {}

local function menu_label(text)
  return (tostring(text):gsub("[\n\r]", " "):gsub("&", "&&"):gsub("([%s%.\\|])", "\\%1"))
end

-- Menus need their insert-mode entries too, because typing is the usual mode.
local function add_menu(name, rhs)
  vim.cmd(("anoremenu %s %s"):format(name, rhs))
  vim.cmd(("inoremenu %s %s"):format(name, rhs))
end

local function popup(items)
  pending = items
  pcall(vim.cmd, "aunmenu ]Pick")
  for i, item in ipairs(items) do
    add_menu(("1.%d ]Pick.%s"):format(i, menu_label(item.label)), ("<Cmd>lua MouseBar.pick(%d)<CR>"):format(i))
  end
  vim.cmd.popup("]Pick")
end

function MouseBar.pick(i)
  local item = pending[i]
  pending = {}
  if item then
    vim.schedule(item.run)
  end
end

vim.ui.select = function(choices, opts, on_choice)
  opts = opts or {}
  local format_item = opts.format_item or tostring
  local items = {}
  for i, choice in ipairs(choices) do
    items[i] = {
      label = format_item(choice),
      run = function()
        on_choice(choice, i)
      end,
    }
  end
  if #items == 0 then
    on_choice(nil, nil)
    return
  end
  popup(items)
end

local function type_command()
  vim.api.nvim_feedkeys(vim.keycode("<C-\\><C-n>:"), "n", false)
end

local function command_menu()
  popup({
    { label = "Type a command…", run = type_command },
    {
      label = "Save",
      run = function()
        vim.cmd("silent! update")
      end,
    },
    { label = "Format", run = format },
    { label = "Rename symbol", run = vim.lsp.buf.rename },
    { label = "Find references", run = vim.lsp.buf.references },
    { label = "Go to definition", run = vim.lsp.buf.definition },
    {
      label = "Toggle line wrap",
      run = function()
        vim.wo.wrap = not vim.wo.wrap
      end,
    },
    {
      label = "Close file",
      run = function()
        vim.cmd("bdelete")
      end,
    },
    {
      label = "Quit",
      run = function()
        vim.cmd("confirm qall")
      end,
    },
  })
end

-- Right-click menu: Neovim's own, plus code actions and the rest.
add_menu("PopUp.-mouse-", "<Nop>")
add_menu("PopUp.Code\\ actions", "<Cmd>lua vim.lsp.buf.code_action()<CR>")
add_menu("PopUp.Rename\\ symbol", "<Cmd>lua vim.lsp.buf.rename()<CR>")
add_menu("PopUp.Find\\ references", "<Cmd>lua vim.lsp.buf.references()<CR>")
add_menu("PopUp.Format", '<Cmd>lua require("conform").format({ lsp_format = "fallback" })<CR>')
add_menu("PopUp.Files", '<Cmd>lua require("nvim-tree.api").tree.toggle()<CR>')

-- Bottom bar -----------------------------------------------------------------

local buttons = {
  { icon = "", label = "Command", run = command_menu },
  {
    icon = "󰉋",
    label = "Files",
    run = function()
      require("nvim-tree.api").tree.toggle()
    end,
  },
  {
    icon = "",
    label = "Search",
    run = function()
      require("fff").live_grep()
    end,
  },
  { icon = "󰌵", label = "Actions", run = vim.lsp.buf.code_action },
  { icon = "󰉼", label = "Format", run = format },
  {
    icon = "",
    label = "Undo",
    run = function()
      vim.cmd.undo()
    end,
  },
  {
    icon = "",
    label = "Redo",
    run = function()
      vim.cmd.redo()
    end,
  },
}

function MouseBar.click(index)
  if index == 0 then
    toggle_vim_mode()
  elseif buttons[index] then
    vim.schedule(buttons[index].run)
  end
end

vim.api.nvim_set_hl(0, "MouseBar", { fg = "#a0aabe", bg = "#0a0a0a" })
vim.api.nvim_set_hl(0, "MouseBarButton", { fg = "#d0d6e0", bg = "#1a1a1a" })
vim.api.nvim_set_hl(0, "MouseBarMode", { fg = "#000000", bg = "#ff4ead", bold = true })
vim.api.nvim_set_hl(0, "MouseBarModeOff", { fg = "#d0d6e0", bg = "#2a0016" })

function MouseBar.render()
  local parts = {}
  local mode = vim.g.vim_mode and "%#MouseBarMode# VIM " or "%#MouseBarModeOff# TYPING "
  parts[#parts + 1] = "%0@v:lua.MouseBar.click@" .. mode .. "%X%#MouseBar# "
  for i, b in ipairs(buttons) do
    parts[#parts + 1] = ("%%%d@v:lua.MouseBar.click@%%#MouseBarButton# %s %s %%X%%#MouseBar# "):format(
      i,
      b.icon,
      b.label
    )
  end
  parts[#parts + 1] = "%=%<%t%m  %l:%c "
  return table.concat(parts)
end

vim.opt.statusline = "%!v:lua.MouseBar.render()"

-- Scrollbar --------------------------------------------------------------------

pcall(function()
  require("satellite").setup({})
end)

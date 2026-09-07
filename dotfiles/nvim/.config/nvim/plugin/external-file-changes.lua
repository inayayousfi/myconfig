vim.opt.autoread = false

local function disk_fingerprint(bufnr)
  if not vim.api.nvim_buf_is_loaded(bufnr) or vim.bo[bufnr].buftype ~= "" then
    return nil
  end

  local path = vim.api.nvim_buf_get_name(bufnr)
  if path == "" then
    return nil
  end

  local file = io.open(path, "rb")
  if not file then
    return nil
  end

  local contents = file:read("*a")
  file:close()
  return vim.fn.sha256(contents)
end

local external_file_changes = vim.api.nvim_create_augroup("ExternalFileChanges", { clear = true })

vim.api.nvim_create_autocmd({ "BufReadPost", "BufWritePost" }, {
  group = external_file_changes,
  callback = function(ev)
    vim.b[ev.buf].disk_fingerprint = disk_fingerprint(ev.buf)
  end,
})

vim.api.nvim_create_autocmd({ "FocusGained", "BufEnter", "CursorHold", "CursorHoldI" }, {
  group = external_file_changes,
  callback = function()
    for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
      local current = disk_fingerprint(bufnr)
      local previous = vim.b[bufnr].disk_fingerprint

      if current and previous and current ~= previous then
        local ok, err = pcall(vim.api.nvim_buf_call, bufnr, function()
          vim.cmd("silent edit!")
        end)
        if ok then
          vim.b[bufnr].disk_fingerprint = current
        else
          vim.notify(("Failed to reload %s: %s"):format(vim.api.nvim_buf_get_name(bufnr), err), vim.log.levels.WARN)
        end
      elseif current and not previous then
        vim.b[bufnr].disk_fingerprint = current
      end
    end
  end,
})

vim.api.nvim_create_autocmd("FileChangedShell", {
  group = external_file_changes,
  callback = function()
    vim.v.fcs_choice = ""
  end,
})

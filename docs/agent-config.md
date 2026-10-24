# Agent configuration

This document describes `claude-config-helper`, the Python command that the `ai` package ships for both Linux and Windows.

The command trusts projects, marks Claude's first-run setup as finished, and lists or clears Claude's saved approvals. It applies the MCP servers listed in `dotfiles/ai/.config/claude-config-helper/mcp-servers.json` through Claude's own commands. It also checks tracked files for tokens, email addresses and home paths.

Only the trust, setup and approval changes edit `~/.claude.json` directly.

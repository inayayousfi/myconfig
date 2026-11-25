---
name: herdr
description: Use when you want sub-agents or helpers, to start, watch, read or talk to another agent or terminal, to run a background task, a server or a long command, to work on another machine through Herdr, or to show or arrange something in the Herdr window. Also use it whenever `$HC_TERMINAL_ID` is set.
---

# herdr

Herdr runs terminals and agents in herdr-core, a background server whose command is `hc`, and shows them in the Herdr app. A terminal started with `hc` keeps running when its window closes, shows up in my Herdr window under the agent that started it, and can be read, waited on and answered from any shell.

When `$HC_TERMINAL_ID` is set, you run inside Herdr: start helpers, background tasks and long commands with `hc` instead of your own sub-agent tool, `&`, `nohup` or tmux. When it is unset and `hc` exists, ask me once per session whether to use it.

Before the first `hc` command of a session, run `hc --skill` and follow it. For the window itself, such as spaces, tabs, layouts or mods, run `herdr-app --skill`. Use `hc --help` for one command's details.

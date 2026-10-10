---
name: remote-emacs
description: "Use whenever something could be done in the user's running Emacs: showing them a file, a diff, a search result or a buffer, reading or editing what they have open, or running any Emacs command, package or Lisp for them. Explains how to reach that Emacs from this environment and what to avoid."
---

# Running Emacs

The user's graphical Emacs runs a server named `remot`. `emacsclient` sends it Lisp expressions and files, and they act on the Emacs the user is looking at.

{{environment-note}}

Command prefix:

```
{{command}}
```

- Check that it answers: append `--eval t`. It prints `t`. Otherwise Emacs or its server is not running and nothing here applies.
- Evaluate Lisp: append `--eval '(buffer-name (window-buffer (selected-window)))'`. It prints the result.
- Open a file in the window the user is using: append `--no-wait FILE`.

Everything Emacs can do is available through `--eval`: run a command with `(call-interactively #'magit-status)`, show a buffer with `display-buffer`, read variables, or call any loaded package.

Act on this Emacs only when the user asks to observe or change it. It holds the user's open work: never kill buffers, save files, or evaluate code with lasting effects unless asked. Pass code with `--eval`; loading a file without a `lexical-binding` cookie opens a visible warning window. Run tests in a separate Emacs, never this one.

# Emacs

This document describes the CachyOS Emacs module, `linux/modules/emacs.sh`.

The module installs the native Wayland Emacs package explicitly, together with its fonts and tools. It checks that the `emacs` package was stowed: the early init file, the init file, the `lisp` modules, the `emt` terminal command and the Emacs client socket environment. It makes Emacs the default text editor.

The module also manages the firewall policy that lets devices on the private network reach the Emacs browser terminal.

;;; early-init.el --- Early graphical defaults -*- lexical-binding: t; -*-

(defconst myconfig-early-config-directory
  (file-name-directory (or load-file-name buffer-file-name)))
(add-to-list 'load-path (expand-file-name "lisp" myconfig-early-config-directory))
(require 'myconfig-platform)

(setq package-enable-at-startup nil
      frame-inhibit-implied-resize t
      inhibit-startup-message t
      inhibit-startup-screen t
      initial-scratch-message nil)

(defconst myconfig-cache-directory
  (myconfig-platform-path 'cache "myconfig-emacs"))
(make-directory myconfig-cache-directory t)
(startup-redirect-eln-cache (expand-file-name "eln-cache" myconfig-cache-directory))

(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)

(add-to-list 'default-frame-alist
             `(font . ,(if (eq system-type 'windows-nt)
                          "Iosevka NFM-15"
                        "Iosevka Nerd Font Mono-15")))
(add-to-list 'default-frame-alist '(background-color . "#000000"))
(add-to-list 'default-frame-alist '(foreground-color . "#d0d6e0"))

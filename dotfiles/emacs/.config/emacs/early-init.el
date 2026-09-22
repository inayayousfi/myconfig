;;; early-init.el --- Early graphical defaults -*- lexical-binding: t; -*-

(defconst myconfig-early-config-directory
  (file-name-directory (or load-file-name buffer-file-name)))
(add-to-list 'load-path (expand-file-name "lisp" myconfig-early-config-directory))
(require 'univers)

(setq package-enable-at-startup nil
      frame-inhibit-implied-resize t
      inhibit-startup-message t
      inhibit-startup-screen t
      initial-scratch-message nil)

(defconst myconfig-cache-directory
  (universel-standard-path 'cache "myconfig-emacs"))
(make-directory myconfig-cache-directory t)
(startup-redirect-eln-cache (expand-file-name "eln-cache" myconfig-cache-directory))

(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)

(add-to-list 'default-frame-alist
             `(font . ,(universel-select
                        '((windows . "Iosevka NFM-15")
                          (t . "Iosevka Nerd Font Mono-15"))
                        (universel-host-platform))))
(add-to-list 'default-frame-alist '(background-color . "#000000"))
(add-to-list 'default-frame-alist '(foreground-color . "#d0d6e0"))

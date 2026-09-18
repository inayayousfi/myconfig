;;; init.el --- Stateful Emacs Atelier -*- lexical-binding: t; -*-

(defconst myconfig-config-directory
  (file-name-directory (or load-file-name buffer-file-name)))
(add-to-list 'load-path (expand-file-name "lisp" myconfig-config-directory))
(require 'myconfig-platform)
(require 'ls-lisp)
(setq ls-lisp-dirs-first t)
(defconst myconfig-data-directory
  (myconfig-platform-path 'data "myconfig-emacs"))
(defconst myconfig-runtime-state-directory
  (myconfig-platform-path 'state "myconfig-emacs"))

(dolist (directory (list myconfig-data-directory myconfig-runtime-state-directory))
  (make-directory directory t))
(make-directory (expand-file-name "url" myconfig-runtime-state-directory) t)

(setq user-emacs-directory myconfig-data-directory
      custom-file (expand-file-name "custom.el" myconfig-runtime-state-directory)
      package-user-dir (expand-file-name "elpa" myconfig-data-directory)
      ;; Do not use Emacs' recovery autosaves: they create #...# files and
      ;; autosave-list entries.  myconfig-editing.el saves the visited file
      ;; itself after a short idle period, so disk always follows the buffer.
      make-backup-files nil
      backup-inhibited t
      auto-save-default nil
      auto-save-visited-file-name nil
      auto-save-list-file-prefix nil
      create-lockfiles nil
      project-list-file (expand-file-name "projects.eld" myconfig-runtime-state-directory)
      tramp-persistency-file-name (expand-file-name "tramp" myconfig-runtime-state-directory)
      savehist-file (expand-file-name "history" myconfig-runtime-state-directory)
      bookmark-default-file (expand-file-name "bookmarks" myconfig-runtime-state-directory)
      url-history-file (expand-file-name "url/history" myconfig-runtime-state-directory)
      load-prefer-newer t
       evil-want-keybinding nil
       evil-want-integration t)

(let ((user-bin (expand-file-name "~/.local/bin")))
  (when (file-directory-p user-bin)
    (add-to-list 'exec-path user-bin)
    (setenv "PATH" (mapconcat #'identity
                               (delete-dups
                                (cons user-bin (parse-colon-path (getenv "PATH"))))
                               path-separator))))

(require 'cl-lib)
(require 'package)
(require 'package-vc)
(setq package-archives
      '(("gnu" . "https://elpa.gnu.org/packages/")
        ("nongnu" . "https://elpa.nongnu.org/nongnu/")
        ("melpa" . "https://melpa.org/packages/"))
      package-archive-priorities '(("gnu" . 30) ("nongnu" . 20) ("melpa" . 10)))
(defconst myconfig-evil-revision "6a3e1ddd04ac504a016590940d0af2a3361b9efd")
(defconst myconfig-evil-source
  '(evil :url "https://github.com/emacs-evil/evil.git" :vc-backend Git))
(setq package-vc-selected-packages (list myconfig-evil-source))
(let ((package-load-list '((evil nil) (evil-collection nil) (evil-ghostel nil) all)))
  (package-initialize))

(defun myconfig-evil-vc-description ()
  (cl-find-if
   (lambda (description)
     (file-directory-p (expand-file-name ".git" (package-desc-dir description))))
   (cdr (assq 'evil package-alist))))

(let ((description (myconfig-evil-vc-description)))
  (unless (and description
               (equal (alist-get :commit (package-desc-extras description))
                      myconfig-evil-revision))
    (when description (package-delete description t t))
    (let ((checkout (expand-file-name "evil" package-user-dir)))
      (when (and (file-directory-p checkout)
                 (null (directory-files checkout nil directory-files-no-dot-files-regexp)))
        (delete-directory checkout)))
    (package-vc-install myconfig-evil-source myconfig-evil-revision)
    (setq description (myconfig-evil-vc-description)))
  (dolist (candidate (copy-sequence (cdr (assq 'evil package-alist))))
    (unless (eq candidate description)
      (package-delete candidate t t)))
  (package-activate-1 description nil nil)
  (package-activate 'evil-collection t))

(defconst myconfig-required-packages
  '(evil evil-collection vertico orderless marginalia consult corfu cape
     yasnippet yasnippet-capf avy ghostel evil-ghostel magit diff-hl blamer flyover apheleia eldoc-box
     treesit-auto mason multiple-cursors)
   "Elisp packages required by the live Atelier.")

(unless (cl-every #'package-installed-p myconfig-required-packages)
  (package-refresh-contents)
  (dolist (package myconfig-required-packages)
    (unless (package-installed-p package)
      (package-install package))))

(package-activate 'evil-ghostel t)

(require 'myconfig-core)
(require 'myconfig-ui)
(require 'myconfig-windows)
(require 'atelier)
(require 'myconfig-persist)
(require 'myconfig-editing)
(require 'myconfig-terminal)
(when (require 'aipan nil t)
  (require 'aipanel-atelier nil t))
(require 'myconfig-git)
(require 'myconfig-bindings)

(when (file-readable-p custom-file)
  (load custom-file nil t))

(myconfig-initialize)

;;; init.el --- Stateful Emacs Atelier -*- lexical-binding: t; -*-

(defconst myconfig-config-directory
  (file-name-directory (or load-file-name buffer-file-name)))
(let ((lisp-directory (expand-file-name "lisp" myconfig-config-directory)))
  (add-to-list 'load-path lisp-directory)
  (add-to-list 'load-path (expand-file-name "atelier" lisp-directory)))
(require 'univers)
(require 'ls-lisp)
(setq ls-lisp-use-insert-directory-program nil
      ls-lisp-dirs-first t)
(defconst myconfig-data-directory
  (universel-standard-path 'data "myconfig-emacs"))
(defconst myconfig-runtime-state-directory
  (universel-standard-path 'state "myconfig-emacs"))

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
      ;; Never offer package-created/non-file buffers (Magit, Ediff, etc.)
      ;; for saving when Emacs exits.  File-visiting buffers are unaffected.
      buffer-offer-save nil
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
    (universel-prepend-exec-path user-bin)))

(require 'cl-lib)
(require 'package)
(require 'package-vc)
(setq package-archives
      '(("gnu" . "https://elpa.gnu.org/packages/")
        ("nongnu" . "https://elpa.nongnu.org/nongnu/")
        ("jcs" . "https://jcs-emacs.github.io/jcs-elpa/packages/")
        ("melpa" . "https://melpa.org/packages/"))
      package-archive-priorities '(("gnu" . 30) ("nongnu" . 20)
                                   ("jcs" . 15) ("melpa" . 10)))
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
  `(evil evil-collection vertico orderless marginalia consult corfu cape
         yasnippet yasnippet-capf avy ghostel evil-ghostel magit diff-hl blamer flyover apheleia eldoc-box
         treesit-auto mason dape multiple-cursors simple-httpd websocket
         ,@(universel-select '((windows treesit-langs))
                             (universel-host-platform)))
  "Elisp packages required by the live Atelier.")

(unless (cl-every #'package-installed-p myconfig-required-packages)
  (package-refresh-contents)
  (dolist (package myconfig-required-packages)
    (unless (package-installed-p package)
      (package-install package))))

(package-activate 'evil-ghostel t)

(require 'myconfig-compilation)
(myconfig-compilation-setup
 (expand-file-name "lisp" myconfig-config-directory)
 myconfig-cache-directory)

;; Initialize Mason before packages which use it are loaded.
(require 'mason)
(defun myconfig-mason-utf8-command (command)
  "Make Mason's batch COMMAND write Unicode data without prompting."
  (append (butlast command)
          (list (prin1-to-string
                 `(let ((coding-system-for-write 'utf-8-unix))
                    ,(read (car (last command))))))))
(when (eq system-type 'windows-nt)
  ;; Mason starts a separate Emacs with -Q, so it does not inherit this
  ;; process's coding preferences.  Windows otherwise prompts for a coding
  ;; system when the registry contains characters outside Latin-1.
  (advice-add 'mason--emacs-cmd :filter-return #'myconfig-mason-utf8-command))
(let ((original-path (getenv "PATH")))
  (mason-setup)
  (universel-repair-mason-path original-path (expand-file-name "bin" mason-dir)))

(require 'myconfig-core)
(require 'myconfig-ui)
(require 'atelier)
(require 'dired-atelier)
(dired-atelier-setup)
(setq atelier-state-directory myconfig-runtime-state-directory
      atelier-close-without-asking t
      atelier-stack-limit 5
      atelier-workspace-inactive-timeout myconfig-workspace-inactive-timeout)
(require 'universel-atelier)
(universel-atelier-setup myconfig-runtime-state-directory)
(require 'atelier-persist)
(require 'xref-atelier)
(require 'myconfig-editing)
(require 'vertico-atelier)
(vertico-atelier-setup)
(require 'myconfig-terminal)
(require 'ghostel-atelier)
(ghostel-atelier-setup)
(setq ghostel-atelier-default-shell-function #'universel-default-shell)
(when (require 'aipan nil t)
  (require 'universel-aipanel)
  (universel-aipanel-setup)
  (require 'aipanel-atelier)
  (aipanel-atelier-setup)
  (myconfig-aipanel-setup))
(require 'myconfig-git)
(require 'jumel-atelier)
(jumel-atelier-setup)
(require 'myconfig-bindings)
(require 'remot)
(require 'remot-atelier)
(remot-atelier-setup)
(setq remot-state-directory
      (expand-file-name "remot/" myconfig-runtime-state-directory)
      remot-exit-with-last-graphical-frame t)

(when (file-readable-p custom-file)
  (load custom-file nil t))

(require 'myconfig-init)
(myconfig-initialize)
(remot-setup)
(when (featurep 'aipan)
  (require 'remot-aipanel)
  (remot-aipanel-setup))

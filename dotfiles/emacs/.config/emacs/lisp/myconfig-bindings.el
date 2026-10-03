;;; myconfig-bindings.el --- Evil workbench keys -*- lexical-binding: t; -*-

(require 'evil)
(require 'multiple-cursors)
(require 'atelier)
(require 'dired-atelier)
(require 'eglot)

(declare-function avy-goto-char-timer "avy")
(declare-function eldoc-box-help-at-point "eldoc-box")
(declare-function myconfig-search "myconfig-editing")
(declare-function consult-mark "consult")
(declare-function myconfig-terminal "myconfig-terminal")
(declare-function myconfig-toggle-auto-format-save "myconfig-editing")
(declare-function atelier-set-job-policy "atelier-persist")
(declare-function dape "dape")
(declare-function magit-status "magit")
(declare-function myconfig-compile "myconfig-editing")

(defvar myconfig-leader-map (make-sparse-keymap))

(defun myconfig-multiple-cursors-toggle ()
  (interactive)
  (if (bound-and-true-p multiple-cursors-mode)
      (mc/keyboard-quit)
    (call-interactively #'mc/edit-lines)))

(defun myconfig-directory-chooser-keymaps ()
  (when (bound-and-true-p evil-local-mode) (evil-normalize-keymaps)))

(defun myconfig-paste ()
  (interactive)
  (cond
   ((derived-mode-p 'ghostel-mode)
    (ghostel-paste-string (current-kill 0)))
   ((and (bound-and-true-p evil-local-mode)
         (memq evil-state '(normal visual motion operator)))
    (call-interactively #'evil-paste-after))
   (t (call-interactively #'yank))))

(defun myconfig-bindings-setup ()
  (add-hook 'atelier-directory-chooser-mode-hook #'myconfig-directory-chooser-keymaps)
  (add-hook 'atelier-file-browser-mode-hook #'myconfig-directory-chooser-keymaps)
  (evil-define-key 'normal atelier-file-browser-mode-map
    (kbd "q") #'atelier-file-browser-quit)
  (evil-make-intercept-map atelier-file-browser-mode-map 'normal t)
  (evil-define-key '(normal visual motion) 'global (kbd "SPC") myconfig-leader-map)
  (evil-define-key '(normal visual operator) 'global (kbd "f") #'avy-goto-char-timer)
  (evil-define-key 'normal 'global (kbd ":") #'evil-ex)
  (evil-define-key '(normal insert visual) 'global (kbd "C-r") #'evil-redo)
  (evil-define-key 'normal 'global (kbd "g k") #'eldoc-box-help-at-point)
  (evil-set-initial-state 'atelier-navigator-mode 'normal)
  (evil-set-initial-state 'atelier-choice-mode 'normal)
  (evil-set-initial-state 'dired-mode 'normal)
  (define-key dired-mode-map (kbd "W") #'atelier-dired-flag-workspace)
  (define-key dired-mode-map (kbd "x") #'atelier-dired-execute-flags)
  (dolist (binding '(("RET" . atelier-directory-chooser-enter)
                     ("<return>" . atelier-directory-chooser-enter)
                     ("l" . atelier-directory-chooser-enter)
                     ("h" . atelier-directory-chooser-up-directory)
                     ("H" . atelier-directory-chooser-up-directory)
                     ("^" . atelier-directory-chooser-up-directory)
                     ("W" . atelier-dired-flag-workspace)
                     ("x" . atelier-dired-execute-flags)
                     ("q" . abort-recursive-edit)))
    (define-key atelier-directory-chooser-mode-map (kbd (car binding)) (cdr binding)))
  (define-key atelier-directory-chooser-mode-map [mouse-1] #'atelier-directory-chooser-mouse-enter)
  (define-key atelier-directory-chooser-mode-map [mouse-2] #'atelier-directory-chooser-mouse-enter)
  (evil-define-key 'normal dired-mode-map
    (kbd "SPC") myconfig-leader-map
    (kbd "RET") #'atelier-dired-open
    (kbd "<return>") #'atelier-dired-open
    (kbd "l") #'atelier-dired-open
    (kbd "h") #'atelier-dired-up-directory
    (kbd "n") #'atelier-dired-create
    (kbd "W") #'atelier-dired-flag-workspace
    (kbd "x") #'atelier-dired-execute-flags
    [mouse-1] #'atelier-dired-mouse-open
    [mouse-2] #'atelier-dired-mouse-open)
  (evil-define-key 'normal eglot-mode-map
    (kbd "g d") #'atelier-xref-find-definitions
    (kbd "g D") #'atelier-xref-find-implementation)
  (evil-define-key 'normal atelier-directory-chooser-mode-map
    (kbd "RET") #'atelier-directory-chooser-enter
    (kbd "<return>") #'atelier-directory-chooser-enter
    (kbd "l") #'atelier-directory-chooser-enter
    (kbd "h") #'atelier-directory-chooser-up-directory
    (kbd "H") #'atelier-directory-chooser-up-directory
    (kbd "^") #'atelier-directory-chooser-up-directory
    (kbd "W") #'atelier-dired-flag-workspace
    (kbd "x") #'atelier-dired-execute-flags
    (kbd "q") #'abort-recursive-edit
    [mouse-1] #'atelier-directory-chooser-mouse-enter
    [mouse-2] #'atelier-directory-chooser-mouse-enter)
  (evil-make-intercept-map atelier-directory-chooser-mode-map 'normal t)
  (global-set-key (kbd "C-S-v") #'myconfig-paste)
  (global-set-key (kbd "C-=") #'text-scale-increase)
  (global-set-key (kbd "C--") #'text-scale-decrease)
  (global-unset-key (kbd "C-x"))
  (global-unset-key (kbd "C-c"))
  (global-unset-key (kbd "C-u"))
  (global-unset-key (kbd "C-v"))
  (evil-define-key '(normal insert visual motion operator replace emacs) 'global
    (kbd "C-S-v") #'myconfig-paste)
  (evil-define-key '(normal insert visual motion operator replace emacs) 'global
    (kbd "C-=") #'text-scale-increase
    (kbd "C--") #'text-scale-decrease)
  ;; Do not let keys without an Evil meaning fall through to Emacs prefix
  ;; maps.  C-u and C-v keep their native Evil bindings in editing states.
  (evil-define-key '(normal insert visual motion operator replace emacs) 'global
    (kbd "C-x") #'ignore
    (kbd "C-c") #'ignore)
  (evil-define-key 'emacs 'global
    (kbd "C-u") #'ignore
    (kbd "C-v") #'ignore)
  (evil-define-key '(normal insert visual motion operator replace emacs) 'global
    (kbd "C-l") #'myconfig-multiple-cursors-toggle)
  (evil-define-key 'normal atelier-choice-mode-map
    (kbd "j") #'atelier-choice-next
    (kbd "k") #'atelier-choice-previous
    (kbd "<down>") #'atelier-choice-next
    (kbd "<up>") #'atelier-choice-previous
    (kbd "RET") #'atelier-choice-select
    (kbd "q") #'abort-recursive-edit)
  (evil-define-key 'normal atelier-navigator-mode-map
    (kbd "j") #'atelier-navigator-next
    (kbd "k") #'atelier-navigator-previous
    (kbd "h") #'atelier-navigator-stack-previous
    (kbd "l") #'atelier-navigator-stack-next
    (kbd "<down>") #'atelier-navigator-next
    (kbd "<up>") #'atelier-navigator-previous
    (kbd "RET") #'atelier-navigator-open
    (kbd "o") #'atelier-navigator-toggle-fold
    (kbd "s") #'atelier-navigator-stop-workspace
    (kbd "a") #'atelier-navigator-attach
    (kbd "d") #'atelier-navigator-detach
    (kbd "f") #'isearch-forward
    (kbd "F") #'isearch-forward
    (kbd "x") #'atelier-navigator-close
    (kbd "X") #'atelier-navigator-close-entry
    (kbd "r") #'atelier-navigator-rename
    (kbd "R") #'atelier-navigator-rename
    (kbd "q") #'atelier-navigator-quit)
  (define-key myconfig-leader-map (kbd "SPC") #'myconfig-search)
  (define-key myconfig-leader-map (kbd "RET") #'toggle-frame-fullscreen)
  (define-key myconfig-leader-map (kbd "b") #'atelier-new-scratch-buffer)
  (define-key myconfig-leader-map (kbd "f") #'atelier-file-browser)
  (define-key myconfig-leader-map (kbd "m") #'set-mark-command)
  (define-key myconfig-leader-map (kbd "n") #'consult-mark)
  (define-key myconfig-leader-map (kbd "C") #'execute-extended-command)
  (define-key myconfig-leader-map (kbd "c") #'execute-extended-command)
  (when (fboundp 'aipanel-toggle)
    (define-key myconfig-leader-map (kbd "a") #'aipanel-toggle))
  (define-key myconfig-leader-map (kbd "t") #'myconfig-terminal)
  (define-key myconfig-leader-map (kbd "=") #'atelier-split-right)
  (define-key myconfig-leader-map (kbd "-") #'atelier-split-below)
  (define-key myconfig-leader-map (kbd "<left>") (lambda () (interactive) (atelier-resize-split 'left 3)))
  (define-key myconfig-leader-map (kbd "<right>") (lambda () (interactive) (atelier-resize-split 'right 3)))
  (define-key myconfig-leader-map (kbd "<up>") (lambda () (interactive) (atelier-resize-split 'up 2)))
  (define-key myconfig-leader-map (kbd "<down>") (lambda () (interactive) (atelier-resize-split 'down 2)))
  (define-key myconfig-leader-map (kbd "h") #'windmove-left)
  (define-key myconfig-leader-map (kbd "j") #'windmove-down)
  (define-key myconfig-leader-map (kbd "k") #'windmove-up)
  (define-key myconfig-leader-map (kbd "l") #'windmove-right)
  (define-key myconfig-leader-map (kbd "x") #'atelier-close-current-view)
  (define-key myconfig-leader-map (kbd "u v") #'myconfig-toggle-auto-format-save)
  (define-key myconfig-leader-map (kbd "u r") #'atelier-set-job-policy)
  (define-key myconfig-leader-map (kbd "w") #'atelier-navigator)
  (define-key myconfig-leader-map (kbd "g a") #'eglot-code-actions)
  (define-key myconfig-leader-map (kbd "g f") #'dape)
  (define-key myconfig-leader-map (kbd "g g") #'magit-status)
  (define-key myconfig-leader-map (kbd "g d") #'diff)
  (define-key myconfig-leader-map (kbd "g c") #'myconfig-compile))

(provide 'myconfig-bindings)
;;; myconfig-bindings.el ends here

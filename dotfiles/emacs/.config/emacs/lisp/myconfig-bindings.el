;;; myconfig-bindings.el --- Evil workbench keys -*- lexical-binding: t; -*-

(require 'evil)
(require 'multiple-cursors)
(require 'atelier)

(defvar myconfig-leader-map (make-sparse-keymap))

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
  (evil-define-key '(normal visual motion) 'global (kbd "SPC") myconfig-leader-map)
  (evil-define-key '(normal visual operator) 'global (kbd "f") #'avy-goto-char-timer)
  (evil-define-key 'normal 'global (kbd ":") #'execute-extended-command)
  (evil-define-key 'normal 'global (kbd "g k") #'eldoc-box-help-at-point)
  (evil-set-initial-state 'atelier-navigator-mode 'normal)
  (evil-set-initial-state 'atelier-choice-mode 'normal)
  (evil-set-initial-state 'dired-mode 'normal)
  (evil-define-key 'normal dired-mode-map
    (kbd "SPC") myconfig-leader-map
     (kbd "RET") #'atelier-dired-open
     (kbd "<return>") #'atelier-dired-open
      (kbd "l") #'atelier-dired-open
      (kbd "h") #'dired-up-directory)
  (define-key dired-mode-map [mouse-1] #'atelier-dired-mouse-open)
  (define-key dired-mode-map [mouse-2] #'atelier-dired-mouse-open)
  (evil-define-key 'normal atelier-directory-chooser-mode-map
    (kbd "RET") #'atelier-directory-chooser-enter
    (kbd "<return>") #'atelier-directory-chooser-enter
    (kbd "h") #'atelier-directory-chooser-up-directory
    (kbd "H") #'atelier-directory-chooser-up-directory
    (kbd "^") #'atelier-directory-chooser-up-directory
    (kbd "q") #'abort-recursive-edit)
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
    (kbd "C-l") #'atelier-multiple-cursors-toggle)
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
    (kbd "<down>") #'atelier-navigator-next
    (kbd "<up>") #'atelier-navigator-previous
    (kbd "RET") #'atelier-navigator-open
    (kbd "a") #'atelier-navigator-attach
    (kbd "d") #'atelier-navigator-detach
    (kbd "f") #'isearch-forward
    (kbd "F") #'isearch-forward
    (kbd "x") #'atelier-navigator-close
    (kbd "X") #'atelier-navigator-close
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
  (define-key myconfig-leader-map (kbd "u r") #'myconfig-set-job-policy)
  (define-key myconfig-leader-map (kbd "w") #'atelier-navigator)
  (define-key myconfig-leader-map (kbd "g f") #'dape)
  (define-key myconfig-leader-map (kbd "g g") #'magit-status)
  (define-key myconfig-leader-map (kbd "g d") #'diff)
   (define-key myconfig-leader-map (kbd "g c") #'myconfig-compile))

 (provide 'myconfig-bindings)
;;; myconfig-bindings.el ends here

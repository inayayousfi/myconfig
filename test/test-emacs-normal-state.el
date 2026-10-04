;;; test-emacs-normal-state.el --- Shared normal-mode shortcut -*- lexical-binding: t; -*-

(require 'ert)
(require 'cl-lib)
(defvar myconfig-data-directory temporary-file-directory)
(let ((lisp (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/"
                              (file-name-directory (or load-file-name buffer-file-name)))))
  (add-to-list 'load-path lisp)
  (add-to-list 'load-path (expand-file-name "atelier" lisp)))
(require 'myconfig-terminal)
(require 'myconfig-bindings)
(myconfig-terminal-setup)
(myconfig-bindings-setup)
(evil-mode 1)

(ert-deftest myconfig-normal-shortcut-leaves-editing-states ()
  (save-window-excursion
    (with-temp-buffer
      (switch-to-buffer (current-buffer))
      (insert "some text")
      (dolist (state '(insert replace visual operator emacs normal))
        (evil-change-state state)
        (execute-kbd-macro (kbd "M-x"))
        (should (eq evil-state 'normal))
        (should (equal (buffer-string) "some text"))))))

(ert-deftest myconfig-normal-shortcut-keeps-project-search ()
  (with-temp-buffer
    (evil-normal-state)
    (should (eq (key-binding (kbd "SPC SPC")) #'myconfig-search))
    (should (eq (key-binding (kbd "/")) #'evil-search-forward))))

(ert-deftest myconfig-normal-shortcut-cancels-prompt ()
  :tags '(:graphical)
  (skip-unless (display-graphic-p))
  (save-window-excursion
    (with-temp-buffer
      (switch-to-buffer (current-buffer))
      (evil-insert-state)
      (let ((cancelled nil))
        (minibuffer-with-setup-hook
            (lambda ()
              (setq unread-command-events
                    (append (listify-key-sequence (kbd "unfinished M-x"))
                            unread-command-events)))
          (condition-case nil
              (progn (read-string "Test: ")
                     (ert-fail "Alt+x accepted the prompt instead of cancelling"))
            (quit (setq cancelled t))))
        (should cancelled)
        (should (eq evil-state 'normal))))))

(ert-deftest myconfig-normal-shortcut-hands-terminal-input-to-editor ()
  :tags '(:graphical)
  (skip-unless (display-graphic-p))
  (save-window-excursion
    (let ((buffer (ghostel-atelier-exec-buffer
                   "*normal-test*" default-directory "/bin/sh" nil
                   '((kind . aipanel)))))
      (unwind-protect
          (progn
            (switch-to-buffer buffer)
            (should (eq ghostel--input-mode 'char))
            (should-not evil-local-mode)
            (dolist (key '("ESC" "<escape>" "C-c"))
              (should (eq (key-binding (kbd key)) #'ghostel--send-event)))
            (execute-kbd-macro (kbd "M-x"))
            (should (eq ghostel--input-mode 'emacs))
            (should (eq evil-state 'normal))
            (should evil-local-mode)
            (should evil-ghostel-mode)
            (execute-kbd-macro (kbd "i"))
            (should (eq ghostel--input-mode 'char))
            (should-not evil-local-mode))
        (when (buffer-live-p buffer)
          (kill-buffer buffer))))))

;;; test-emacs-normal-state.el ends here

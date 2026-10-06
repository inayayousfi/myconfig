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

(defmacro myconfig-normal-workspace-test (&rest body)
  "Run BODY with `terminal', a live shell shown in an isolated workspace.
`workspace' and its directory `root' are bound; Atelier's naming timer runs."
  (declare (indent 0))
  `(let* ((root (make-temp-file "normal-workspace-" t))
          (workspace (list :id "normal-test" :name "work" :destination "local"
                           :path root :status 'running :entries nil))
          (atelier-workspaces (list workspace))
          (atelier-content-live-buffers (make-hash-table :test #'equal))
          (atelier-entry-owners (make-hash-table :test #'eq))
          (atelier-change-hook nil)
          (atelier-buffer-activate-functions nil)
          (atelier-buffer-kind-functions nil)
          (atelier-naming-timer nil)
          (buffer-list-update-hook (list #'atelier-schedule-naming))
          (window-buffer-change-functions (list #'atelier-record-changed-windows))
          (old-selection (frame-parameter nil 'atelier-workspace-id))
          (old-buffers (buffer-list))
          ;; Buffer names are global: buffers from outside the test stay untouched.
          (atelier-internal-buffers
           (let ((table (make-hash-table :test #'eq)))
             (dolist (buffer old-buffers table) (puthash buffer t table))))
          (terminal nil))
     (unwind-protect
         (save-window-excursion
           (delete-other-windows)
           (ghostel-atelier-setup)
           (atelier-select-workspace workspace)
           (setq terminal (ghostel-atelier-exec-buffer "terminal" root "/bin/sh" nil))
           (atelier-show-buffer terminal workspace)
           (with-current-buffer terminal
             (should (eq ghostel--input-mode 'char))
             (execute-kbd-macro (kbd "M-x"))
             (should (eq ghostel--input-mode 'emacs)))
           ,@body)
       (when (timerp atelier-naming-timer) (cancel-timer atelier-naming-timer))
       (set-frame-parameter nil 'atelier-workspace-id old-selection)
       (dolist (buffer (buffer-list))
         (unless (memq buffer old-buffers)
           (kill-buffer buffer)))
       (delete-directory root t))))

(defun myconfig-normal-test-terminal-normal-p (terminal)
  (with-current-buffer terminal
    (and (eq ghostel--input-mode 'emacs) evil-local-mode (eq evil-state 'normal))))

(ert-deftest myconfig-normal-shortcut-survives-workspace-bookkeeping ()
  "Alt+x keeps a workspace terminal in normal state after Atelier's naming pass."
  :tags '(:graphical)
  (skip-unless (display-graphic-p))
  (myconfig-normal-workspace-test
    ;; The naming timer starts after the command and its display.
    (sit-for 0.5)
    (should-not atelier-naming-timer)
    (should (myconfig-normal-test-terminal-normal-p terminal))))

(ert-deftest myconfig-normal-terminal-stays-when-another-split-changes ()
  "Showing a file in another split leaves a normal-state terminal in normal state.
The workspace still records the file in that split's view."
  :tags '(:graphical)
  (skip-unless (display-graphic-p))
  (myconfig-normal-workspace-test
    (let ((file (expand-file-name "other.txt" root)))
      (with-temp-file file (insert "other"))
      (select-window (split-window-right))
      (find-file file)
      (redisplay t)
      (sit-for 0.5)
      (should (myconfig-normal-test-terminal-normal-p terminal))
      (let ((view (atelier-entry-by-id
                   workspace (window-parameter (selected-window) 'atelier-view-id))))
        (should (eq (atelier-entry-live-buffer view) (current-buffer)))))))

;;; test-emacs-normal-state.el ends here

;;; dired-atelier.el --- Native Dired selection for Atelier -*- lexical-binding: t; -*-

(require 'dired)
(require 'atelier)

(defvar atelier-directory-chooser-multiple nil)
(defconst atelier-dired-workspace-marker ?W
  "Native Dired flag character for workspace addition, not a key binding.")
(defvar atelier-directory-chooser-mode-map (make-sparse-keymap)
  "Directory picker map; callers supply their own bindings.")

(define-minor-mode atelier-directory-chooser-mode
  "Choose workspace directories using native Dired workspace flags."
  :lighter " Choose directories"
  :keymap atelier-directory-chooser-mode-map)

(defun atelier-dired-workspace-flags ()
  "Return only directories carrying the native workspace flag, without fallback."
  (unless (derived-mode-p 'dired-mode)
    (user-error "This command requires Dired"))
  (let ((dired-marker-char atelier-dired-workspace-marker))
    (dired-get-marked-files nil 'marked)))

(defun atelier-dired-flag-workspace (arg)
  "Flag directories for workspace addition with native Dired marking.
ARG and an active region work as they do for ordinary Dired flags."
  (interactive (list current-prefix-arg))
  (let ((previous (atelier-dired-workspace-flags))
        (dired-marker-char atelier-dired-workspace-marker))
    (atomic-change-group
      (dired-mark arg t)
      (dolist (directory (cl-set-difference (atelier-dired-workspace-flags) previous
                                          :test #'equal))
        (unless (file-directory-p directory)
          (user-error "Not a directory: %s" directory))))))

(defun atelier-dired-clear-workspace-flags (buffer directories)
  "Clear successful DIRECTORY flags in BUFFER without changing other marks."
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (save-excursion
        (dolist (directory directories)
          (when (and (dired-goto-file directory)
                     (eq (char-after (line-beginning-position))
                         atelier-dired-workspace-marker))
            (dired-unmark 1)))))))

(atelier-define-operation atelier-dired-add-flagged-workspaces (directories buffer)
    (list (atelier-current-workspace-id)) nil
  "Add DIRECTORY workspaces and clear BUFFER's flags only after publication."
  (unless (buffer-live-p buffer) (user-error "The flagged Dired buffer was closed"))
  (atelier-add-workspaces directories)
  (atelier-operation-after
   (lambda () (atelier-dired-clear-workspace-flags buffer directories))))

(defun atelier-dired-execute-flags ()
  "Execute native deletion flags first, then existing workspace directories.
Keep Dired's deletion confirmation and error handling.  Declined or failed
deletions do not block independent workspace additions.  Ordinary marks are
not workspace flags.  In the picker, return the surviving directory selection."
  (interactive)
  (let* ((buffer (current-buffer))
         (flagged (atelier-dired-workspace-flags)))
    (dired-do-flagged-delete (and flagged t))
    (let* ((directories (cl-remove-if-not #'file-directory-p flagged))
           (unavailable (cl-set-difference flagged directories :test #'equal)))
      (when unavailable
        (message "Skipped unavailable workspace directories: %s"
                 (string-join unavailable ", ")))
      (when directories
        (if atelier-directory-chooser-mode
            (progn
              (when (and (not atelier-directory-chooser-multiple) (cdr directories))
                (user-error "Choose exactly one directory to edit a workspace"))
              (atelier-operation-after
               (lambda () (atelier-dired-clear-workspace-flags buffer directories)))
              (setq atelier-directory-choice-result directories)
              (exit-recursive-edit))
          (atelier-dired-add-flagged-workspaces directories buffer))))))

(defun atelier-directory-chooser-setup ()
  (unless atelier-directory-chooser-mode
    (setq-local atelier-directory-chooser-header-was-local
                (local-variable-p 'header-line-format)
                atelier-directory-chooser-original-header header-line-format)
    (cl-pushnew (current-buffer) atelier-directory-chooser-buffers))
  (atelier-directory-chooser-mode 1)
  (setq-local header-line-format
              '(:eval (substitute-command-keys
                       "\\[atelier-dired-flag-workspace] flag workspace directories; \\[atelier-dired-execute-flags] execute flags"))))

(defun atelier-directory-chooser-cleanup (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (atelier-directory-chooser-mode -1)
      (if atelier-directory-chooser-header-was-local
          (setq-local header-line-format atelier-directory-chooser-original-header)
        (kill-local-variable 'header-line-format)))))

(defun atelier-directory-chooser-up-directory ()
  (interactive)
  (dired-up-directory)
  (atelier-directory-chooser-setup))

(defun atelier-directory-chooser-enter ()
  "Browse the directory at point without completing the selection."
  (interactive)
  (let ((path (dired-get-file-for-visit)))
    (unless (file-directory-p path)
      (user-error "Choose a directory; this is a file: %s" path))
    (switch-to-buffer (atelier-new-dired-buffer path))
    (atelier-directory-chooser-setup)))

(defun atelier-read-directories-with-dired (directory multiple)
  "Choose DIRECTORY paths through Dired; allow several when MULTIPLE is non-nil."
  (let ((atelier-directory-choice-result nil)
        (atelier-directory-chooser-active t)
        (atelier-directory-chooser-multiple multiple)
        (atelier-directory-chooser-buffers nil)
        (existing-buffers (buffer-list)))
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer (atelier-new-dired-buffer directory))
          (atelier-directory-chooser-setup)
          (recursive-edit))
      (mapc #'atelier-directory-chooser-cleanup atelier-directory-chooser-buffers)
      (dolist (buffer atelier-directory-chooser-buffers)
        (when (and (buffer-live-p buffer) (not (memq buffer existing-buffers)))
          (kill-buffer buffer))))
    atelier-directory-choice-result))

(defun atelier-directory-chooser-mouse-enter (event)
  (interactive "e")
  (mouse-set-point event)
  (atelier-directory-chooser-enter))

(defun dired-atelier-setup ()
  "Use Dired for Atelier directory selection, without assigning shortcuts."
  (setq atelier-read-directories-function #'atelier-read-directories-with-dired))

(provide 'dired-atelier)
;;; dired-atelier.el ends here

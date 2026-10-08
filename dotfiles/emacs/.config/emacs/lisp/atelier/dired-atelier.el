;;; dired-atelier.el --- Native Dired selection for Atelier -*- lexical-binding: t; -*-

(require 'dired)
(require 'atelier)
(require 'atelier-persist)

(defvar auto-revert-notify-watch-descriptor)
(declare-function auto-revert-notify-rm-watch "autorevert")
(declare-function auto-revert-notify-add-watch "autorevert")

(defvar atelier-directory-chooser-multiple nil)
(defvar atelier-directory-choice-result nil)
(defvar atelier-directory-chooser-buffers nil)
(defvar-local atelier-directory-chooser-original-header nil)
(defvar-local atelier-directory-chooser-header-was-local nil)
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
        (atelier-inhibit-buffer-ownership t)
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

;;; The Dired type

(defun atelier-dired-buffer-p (buffer)
  "Return non-nil when BUFFER is a Dired listing."
  (with-current-buffer buffer (derived-mode-p 'dired-mode)))

(defun atelier-dired-capture (_buffer)
  (list :kind 'directory :persistent t
        :directory (file-name-as-directory (expand-file-name default-directory))))

(defun atelier-dired-matches-p (content buffer)
  (with-current-buffer buffer
    (and (derived-mode-p 'dired-mode)
         (equal (atelier-content-field content :directory)
                (file-name-as-directory (expand-file-name default-directory))))))

(defun atelier-dired-restore (content workspace)
  (let ((directory (atelier-content-field content :directory)))
    (unless (and directory (file-directory-p directory))
      (signal 'atelier-restore-unavailable (list "Directory is missing or inaccessible")))
    (atelier-new-dired-buffer directory nil workspace)))

(defun atelier-dired-base-name (content buffer)
  "Name a Dired buffer after the folder it lists, live or saved."
  (if buffer
      (with-current-buffer buffer
        (when (derived-mode-p 'dired-mode)
          (atelier-buffer-folder-name default-directory)))
    (when-let* ((directory (atelier-content-field content :directory)))
      (atelier-buffer-folder-name directory))))

(defun atelier-dired-start-buffer (workspace)
  "List WORKSPACE's folder in a new Dired buffer that WORKSPACE owns."
  (let ((buffer (atelier-new-dired-buffer (atelier-workspace-directory workspace) t workspace)))
    (atelier-assign-buffer-to-workspace buffer workspace 'dired)
    buffer))

;;; Folder browsing in workspace views

(defun atelier-new-dired-buffer (directory &optional force-new workspace)
  (setq directory (file-name-as-directory (expand-file-name directory))
        workspace (or workspace (atelier-current-workspace)))
  (let ((buffer
         (or (unless force-new
               (atelier-find-workspace-buffer
                (lambda (buffer _workspace)
                  (with-current-buffer buffer
                    (and (derived-mode-p 'dired-mode)
                         (condition-case nil
                             (file-equal-p default-directory directory)
                           (error nil)))))
                workspace))
             (let ((buffer (unless force-new (dired-noselect directory))))
               (if (and buffer
                        (not (atelier-buffer-owned-by-other-workspace-p
                              buffer workspace)))
                   buffer
                  (let ((buffer (atelier-operation-track-buffer
                                 (generate-new-buffer
                                  (atelier-buffer-folder-name directory)))))
                   (with-current-buffer buffer
                     (setq default-directory directory)
                     (dired-mode directory)
                     (dired-readin))
                   buffer))))))
    buffer))

(defun atelier-file-browser ()
  (interactive)
  (when (window-parameter nil 'window-side)
    (select-window (atelier-main-window)))
  (let* ((frame (selected-frame))
         (workspace (atelier-current-workspace))
         (directory (unless (atelier-buffer-internal-p (current-buffer))
                      default-directory))
         (existing (atelier-workspace-buffer-by-type workspace 'dired)))
    (atelier-uncover-frame frame)
    (setq directory (if (and directory (file-directory-p directory))
                        directory
                      (if (file-directory-p default-directory)
                          default-directory
                        (atelier-workspace-directory))))
    (atelier-show-buffer (or existing (atelier-new-dired-buffer directory nil workspace))
                         workspace)))

(defun atelier-dired-create (name)
  "Create a file named NAME, or a directory if NAME ends in a slash.
Create it relative to the current Dired directory and refresh the listing."
  (interactive (list (read-string "New file or directory (end with / for directory): ")))
  (when (string-empty-p name)
    (user-error "Enter a file or directory name"))
  (let* ((directory-p (eq (aref name (1- (length name))) ?/))
         (path (expand-file-name name (dired-current-directory))))
    (when (or (file-exists-p path) (file-symlink-p path))
      (user-error "Already exists: %s" path))
    (if directory-p
        (make-directory path)
      (write-region "" nil path nil 'silent nil 'excl))
    (revert-buffer)
    (dired-goto-file path)))

(defun atelier-dired-open ()
  (interactive)
  (let ((file (dired-get-file-for-visit)))
    (if (file-directory-p file)
        (atelier-dired-change-directory file)
      (atelier-open-file file))))

(defun atelier-dired-watch-current-directory ()
  "Move Auto-Revert's folder watch to the folder this Dired buffer lists.
While a buffer has a watch, Auto-Revert re-reads it only when that watch
reports a change, so a watch left on the previous folder hides every change
in the folder now listed."
  (when (bound-and-true-p auto-revert-notify-watch-descriptor)
    (auto-revert-notify-rm-watch)
    (auto-revert-notify-add-watch)))

(atelier-define-operation atelier-dired-change-directory (directory &optional target)
    (delete-dups (mapcar (lambda (pair) (atelier-workspace-id (car pair)))
                        (atelier-entries-for-buffer (current-buffer)))) nil
  "Read DIRECTORY into the current Dired buffer and keep its workspace entry.
On entry, stay near the same listing row; on return, select TARGET."
  (let ((buffer (current-buffer))
        (text (buffer-string))
        (position (point))
        (modified (buffer-modified-p))
        (old-directory default-directory)
        (old-dired-directory (copy-tree dired-directory))
        (subdirs (mapcar (lambda (item) (cons (car item) (marker-position (cdr item))))
                         dired-subdir-alist)))
    (atelier-operation-cleanup
     (lambda ()
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (let ((inhibit-read-only t) (buffer-undo-list t))
             (erase-buffer)
             (insert text)
             (setq default-directory old-directory dired-directory old-dired-directory
                   dired-subdir-alist
                   (mapcar (lambda (item) (cons (car item) (copy-marker (cdr item)))) subdirs))
             (goto-char position)
             (set-buffer-modified-p modified))
           (atelier-dired-watch-current-directory))))))
  (let ((line (line-number-at-pos)))
    (setq directory (file-name-as-directory (expand-file-name directory)))
    (setq dired-directory directory
          default-directory directory)
    (dired-readin)
    (atelier-dired-watch-current-directory)
    (unless (and target (dired-goto-file target))
      (goto-char (point-min))
      (forward-line (1- line))
      (when (eobp) (forward-line -1))
      (unless (dired-move-to-filename)
        (dired-next-line 1))))
  (atelier-refresh-current-buffer-entries)
  (current-buffer))

(defun atelier-dired-up-directory ()
  "Read the parent directory into the current Dired buffer."
  (interactive)
  (let* ((directory (dired-current-directory))
         (parent (file-name-directory (directory-file-name directory))))
    (atelier-dired-change-directory parent directory)))

(defun atelier-dired-mouse-open (event)
  "Open the Dired item clicked by EVENT in the current window and buffer."
  (interactive "e")
  (mouse-set-point event)
  (atelier-dired-open))

;;; Saved state from older formats

(defun atelier-dired-upgrade-descriptors (data)
  "Give format 3 folder descriptors, which visit no file, the directory kind."
  (let ((copy (copy-tree data)))
    (dolist (workspace (plist-get copy :workspaces) copy)
      (dolist (descriptor (append (plist-get workspace :buffers)
                                  (plist-get workspace :owned-buffers)))
        (when (and (plist-get descriptor :dired) (not (plist-get descriptor :file)))
          (atelier-legacy-set descriptor :kind 'directory))))))

(defun atelier-dired-upgrade-types (data)
  "Type format 5 folder entries as Dired before the core types the rest."
  (atelier-legacy-map-entries
   (copy-tree data)
   (lambda (entry)
     (when (and (eq (plist-get entry :kind) 'directory)
                (not (plist-member entry :type)))
       (atelier-legacy-set entry :type 'dired)))))

(defun dired-atelier-setup ()
  "Register the Dired type and use Dired to choose and start workspace folders.
Assign no shortcuts."
  (atelier-define-type 'dired
    :tracked t
    :buffer-p #'atelier-dired-buffer-p
    :capture #'atelier-dired-capture
    :matches #'atelier-dired-matches-p
    :restore #'atelier-dired-restore
    :missing-paths (lambda (content) (list (atelier-content-field content :directory)))
    :base-name #'atelier-dired-base-name)
  (atelier-define-upgrade-step 3 #'atelier-dired-upgrade-descriptors)
  (atelier-define-upgrade-step 5 #'atelier-dired-upgrade-types)
  (setq atelier-read-directories-function #'atelier-read-directories-with-dired
        atelier-workspace-start-function #'atelier-dired-start-buffer))

(provide 'dired-atelier)
;;; dired-atelier.el ends here

;;; atelier.el --- Workspace entry trees and restoration -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'dired)
(require 'project)
(require 'subr-x)
(require 'tramp)
(require 'myconfig-core)
(require 'atelier-model)

(defvar atelier-directory-function #'atelier-default-directory
  "Function mapping a workspace record to an Emacs directory.")
(defvar atelier-execution-directory-function #'atelier-default-execution-directory
  "Function mapping workspace and Emacs path to an execution path.")
(defvar atelier-target-directory-function #'atelier-default-execution-directory
  "Function mapping workspace and Emacs path to a stored connection path.")
(defvar atelier-terminal-command-function #'atelier-default-terminal-command
  "Function returning a workspace terminal's launch plist.")
(defvar atelier-release-function #'ignore
  "Function called with workspace and optional force flag to release files.")
(defvar atelier-extra-destinations nil
  "Additional destinations supplied by an optional environment adapter.")

(defun atelier-default-directory (workspace)
  "Resolve WORKSPACE with standard Emacs local and remote file handling."
  (let ((destination (plist-get workspace :destination))
        (path (plist-get workspace :path)))
    (if (equal destination "local") (file-name-as-directory (expand-file-name path))
      (format "/%s:%s:%s" (if (eq (plist-get workspace :platform) 'wsl) "wsl" "ssh")
              destination (file-name-as-directory path)))))

(defun atelier-default-execution-directory (_workspace directory)
  (or (file-remote-p directory 'localname) directory))

(defun atelier-default-terminal-command (workspace)
  (list :program nil :shell shell-file-name :arguments nil
        :directory (atelier-workspace-directory workspace)))

(defcustom atelier-shell-history-files
  (delete-dups
   (delq nil
         (list (getenv "HISTFILE")
               "~/.zsh_history"
               "~/.bash_history"
               "~/.history"
               "~/.local/share/zsh/history"
               "~/.local/share/fish/fish_history"
               "~/.config/fish/fish_history")))
  "Shell history files inspected for previously used SSH destinations."
  :type '(repeat file)
  :group 'myconfig)

(defcustom atelier-shell-history-read-limit (* 4 1024 1024)
  "Maximum number of bytes read from the end of each shell history file."
  :type 'integer
  :group 'myconfig)

(defface atelier-navigator-active
  '((t (:inherit font-lock-keyword-face :weight bold)))
  "Selected workspace in the navigator.")

(defface atelier-navigator-live
  '((t (:inherit default :weight bold)))
  "Live inactive workspace in the navigator.")

(defface atelier-navigator-saved
  '((t (:inherit shadow)))
  "Stopped workspace in the navigator.")

(defface atelier-navigator-hover
  '((t (:background "#ff4ead" :foreground "#000000" :weight bold)))
  "Readable pointer hover for navigator controls.")

(defface atelier-navigator-current
  '((t (:background "#ff4ead" :foreground "#000000" :weight bold :extend t)))
  "Keyboard-selected navigator row.")

(defface atelier-navigator-section
  '((t (:inherit font-lock-comment-face :weight bold :height 0.9)))
  "Navigator section headings.")

(defface atelier-navigator-current-status
  '((t (:inherit success :weight bold)))
  "Current workspace status label.")

(defface atelier-navigator-running-status
  '((t (:inherit font-lock-constant-face)))
  "Background running workspace status label.")

(defface atelier-navigator-buffer
  '((t (:inherit default)))
  "Workspace buffer rows.")

(defface atelier-navigator-branch
  '((t (:inherit shadow)))
  "Tree branches and secondary navigator text.")

(defun atelier-workspace-directory (&optional workspace)
  (let ((workspace (or workspace (atelier-current-workspace))))
    (unless workspace (user-error "No workspace is open"))
    (funcall atelier-directory-function workspace)))

(defun atelier-title ()
  (let ((workspace (atelier-current-workspace)))
    (if workspace
        (format "%s@%s"
                (plist-get workspace :name)
                (plist-get workspace :destination))
      "Emacs")))

(defun atelier-mode-line-status ()
  (cond (buffer-read-only "  RO")
        ((buffer-modified-p) "  *")
        (t "")))

(defun atelier-mode-line-position ()
  (format "Ln %d  Col %d" (line-number-at-pos) (1+ (current-column))))

(defun atelier-multiple-cursors-toggle ()
  (interactive)
  (if (bound-and-true-p multiple-cursors-mode)
      (mc/keyboard-quit)
    (call-interactively #'mc/edit-lines)))

(defun atelier-mode-line-navigator ()
  (let ((label "[Navigator]"))
    (concat
     (propertize " " 'display
                 `(space :align-to (- center ,(/ (string-width label) 2))))
     (atelier-clickable-label label #'atelier-navigator nil
                              'success "Open workbench navigator (SPC w)"))))

(defun atelier-buffer-entry-kind (buffer)
  (with-current-buffer buffer
    (cond ((derived-mode-p 'ghostel-mode) 'terminal)
          (buffer-file-name 'file)
          ((derived-mode-p 'dired-mode) 'directory)
          ((string-prefix-p "*scratch" (buffer-name)) 'scratch)
          (t 'transient))))

(defun atelier-buffer-entry-persistent-p (buffer)
  (memq (atelier-buffer-entry-kind buffer) '(file directory scratch terminal)))

(defun atelier-dired-entry-buffer-p (buffer)
  "Return non-nil when BUFFER is a Dired entry."
  (with-current-buffer buffer
    (derived-mode-p 'dired-mode)))

(defun atelier-terminal-entry-buffer-p (buffer)
  "Return non-nil when BUFFER is a Ghostel terminal entry."
  (with-current-buffer buffer
    (derived-mode-p 'ghostel-mode)))

(defun atelier-buffer-entry-type (buffer)
  "Return BUFFER's first matching registered entry type, if any."
  (cl-loop for definition in atelier-entry-types
           for predicate = (plist-get (cdr definition) :buffer-p)
           when (and predicate (funcall predicate buffer))
           return (car definition)))

(defun atelier-buffer-owned-by-other-workspace-p (buffer workspace)
  "Return non-nil when BUFFER has an entry outside WORKSPACE."
  (cl-some (lambda (pair) (not (eq (car pair) workspace)))
           (atelier-entries-for-buffer buffer)))

(defun atelier-buffer-registerable-p (buffer workspace)
  "Return non-nil when BUFFER may become an entry of WORKSPACE."
  (and (atelier-buffer-ownable-p buffer workspace)
       (not (atelier-buffer-owned-by-other-workspace-p buffer workspace))))

(defun atelier-entry-matches-buffer-p (entry buffer)
  (or (eq (atelier-entry-live-buffer entry) buffer)
      (and (not (atelier-entry-live-buffer entry))
           (with-current-buffer buffer
             (pcase (plist-get entry :kind)
               ('file (and buffer-file-name
                           (equal (plist-get entry :file)
                                  (expand-file-name buffer-file-name))))
               ('directory
                (and (derived-mode-p 'dired-mode)
                     (equal (plist-get entry :directory)
                            (file-name-as-directory
                             (expand-file-name default-directory)))))
               (_ nil))))))

(defun atelier-workspace-entry-for-buffer (workspace buffer)
  (cl-find-if (lambda (entry) (atelier-entry-matches-buffer-p entry buffer))
              (atelier-workspace-entries workspace)))

(defun atelier-workspace-for-buffer (&optional buffer frame)
  "Return BUFFER's workspace in FRAME, or nil when it is not an entry there.

The frame selects the candidate workspace, but the workspace's canonical
entry collection decides whether BUFFER belongs to it.  BUFFER itself carries
no Atelier ownership metadata."
  (let* ((buffer (or buffer (current-buffer)))
         (workspace (atelier-current-workspace frame)))
    (and workspace
         (atelier-workspace-entry-for-buffer workspace buffer)
         workspace)))

(defun atelier-update-entry-from-buffer (entry buffer &optional type)
  (with-current-buffer buffer
    (setf (plist-get entry :name) (buffer-name)
          (plist-get entry :kind) (atelier-buffer-entry-kind buffer)
          (plist-get entry :persistent) (atelier-buffer-entry-persistent-p buffer))
    (when type (setf (plist-get entry :type) type))
    (pcase (plist-get entry :kind)
      ('file
       (setf (plist-get entry :file) (expand-file-name buffer-file-name)
             (plist-get entry :directory) default-directory))
      ('directory
       (setf (plist-get entry :directory)
             (file-name-as-directory (expand-file-name default-directory))))
      ('scratch
       (setf (plist-get entry :directory) default-directory
             (plist-get entry :contents)
             (buffer-substring-no-properties (point-min) (point-max))))
      ('terminal
       (setf (plist-get entry :directory) default-directory)))
    (when-let* ((job (atelier-entry-job entry)))
      (setf (plist-get job :buffer) (buffer-name)))
    (setf (plist-get entry :point) (point)))
  (atelier-entry-set-live-buffer entry buffer)
  entry)

(defun atelier-register-buffer (buffer &optional workspace no-notify type allow-duplicate-type)
  "Register BUFFER as an entry of WORKSPACE and return that entry.

WORKSPACE defaults to the workspace selected by the current frame.  The
workspace record is authoritative; BUFFER receives no ownership metadata."
  (setq workspace (or workspace (atelier-current-workspace))
        type (or type (atelier-buffer-entry-type buffer)))
  (when (and workspace (atelier-buffer-registerable-p buffer workspace))
    (let ((entry (atelier-workspace-entry-for-buffer workspace buffer))
          (typed-entry (and type (atelier-workspace-entry-by-type workspace type))))
      (when (and typed-entry (not entry) (not allow-duplicate-type)
                 (not (atelier-entry-live-buffer typed-entry)))
        (setq entry typed-entry))
      (unless (and typed-entry (not entry) (not allow-duplicate-type))
        (unless entry
          (setq entry (list :id (atelier-new-entry-id) :job nil))
          (setq entry (atelier-update-entry-from-buffer entry buffer type))
          (atelier-entry-add workspace entry no-notify))
        (atelier-update-entry-from-buffer entry buffer type)))))

(defvar atelier-capturing-layout-p nil)
(defvar atelier-capture-used-entry-ids nil)
(defvar atelier-capture-layout-entries nil)

(defun atelier-capture-buffer (buffer &optional window workspace)
  (setq workspace (or workspace (atelier-current-workspace)))
  (let ((type (atelier-buffer-entry-type buffer)))
    (when-let* ((_ (atelier-buffer-registerable-p buffer workspace))
                (_ (or (atelier-workspace-entry-for-buffer workspace buffer)
                       (not type)
                       (not (atelier-workspace-entry-by-type workspace type))))
                (entry
                 (if atelier-capturing-layout-p
                     (or (cl-find-if
                          (lambda (candidate)
                            (and (not (member (plist-get candidate :id)
                                              atelier-capture-used-entry-ids))
                                 (atelier-entry-matches-buffer-p candidate buffer)))
                          (atelier-workspace-entries workspace))
                         (let ((entry (list :id (atelier-new-entry-id) :job nil)))
                           (setq entry (atelier-update-entry-from-buffer entry buffer type))
                           (atelier-entry-add workspace entry t)))
                   (atelier-register-buffer buffer workspace t type))))
      (with-current-buffer buffer
        (setq entry (atelier-update-entry-from-buffer entry buffer))
        (setf (plist-get entry :point) (point)
              (plist-get entry :start) (and (window-live-p window) (window-start window))
              (plist-get entry :selected) (and (window-live-p window)
                                               (eq window (selected-window))))
        (atelier-plist-clear! entry :displayed))
      (push (plist-get entry :id) atelier-capture-used-entry-ids)
      entry)))

(defun atelier-window-tree-span (tree orientation)
  (let ((edges (if (windowp tree) (window-edges tree) (nth 1 tree))))
    (if (eq orientation 'horizontal)
        (- (nth 2 edges) (nth 0 edges))
      (- (nth 3 edges) (nth 1 edges)))))

(defun atelier-capture-layout-entry (orientation ratio children &optional entry)
  (setq entry (or entry (list :id (atelier-new-entry-id))))
  (setf (plist-get entry :kind) 'layout
        (plist-get entry :orientation) orientation
        (plist-get entry :ratio) ratio
        (plist-get entry :children) children
        (plist-get entry :persistent) t)
  (atelier-plist-clear! entry :displayed)
  entry)

(defun atelier-capture-layout-chain (items orientation)
  "Build a binary layout from (ENTRY . SPAN) ITEMS."
  (if (= (length items) 1)
      (caar items)
    (let* ((layout (pop atelier-capture-layout-entries))
           (first (car items))
           (rest (cdr items))
           (first-span (cdr first))
           (rest-span (apply #'+ (mapcar #'cdr rest)))
           (rest-entry (atelier-capture-layout-chain rest orientation)))
      (atelier-capture-layout-entry
       orientation (/ (float first-span) (+ first-span rest-span))
       (list (car first) rest-entry) layout))))

(defun atelier-capture-window-tree (tree workspace)
  (cond
   ((windowp tree)
    (unless (window-parameter tree 'window-side)
      (atelier-capture-buffer (window-buffer tree) tree workspace)))
   ((consp tree)
    (let* ((orientation (if (car tree) 'vertical 'horizontal))
           (items
            (delq nil
                  (mapcar
                   (lambda (child)
                     (when-let* ((entry (atelier-capture-window-tree child workspace)))
                       (cons entry (atelier-window-tree-span child orientation))))
                   (cddr tree)))))
      (cond ((null items) nil)
            ((null (cdr items)) (caar items))
            (t (atelier-capture-layout-chain items orientation)))))))

(defun atelier-capture-current-workspace ()
  (when-let* ((_ (display-graphic-p (selected-frame)))
              (workspace (atelier-current-workspace)))
    (let* ((atelier-capturing-layout-p t)
           (atelier-capture-used-entry-ids nil)
           (atelier-capture-layout-entries
            (let (layouts)
              (cl-labels ((collect (entry)
                            (when (atelier-layout-entry-p entry)
                              (mapc #'collect (atelier-entry-children entry))
                              (push entry layouts))))
                (mapc #'collect (atelier-workspace-top-level-entries workspace)))
              (nreverse layouts)))
           (old-leaves (copy-sequence (atelier-workspace-entries workspace)))
           (tree (car (window-tree (selected-frame))))
           (displayed (atelier-capture-window-tree tree workspace))
           (unplaced
            (cl-remove-if
             (lambda (entry)
               (member (plist-get entry :id) atelier-capture-used-entry-ids))
             old-leaves)))
      (when displayed (setf (plist-get displayed :displayed) t))
      ;; Entries which are no longer visible remain in the workspace as
      ;; unplaced buffers, but must not retain the old layout's display flag.
      (dolist (entry unplaced)
        (atelier-plist-clear! entry :displayed))
      (setf (plist-get workspace :entries)
            (append (and displayed (list displayed)) unplaced))
      (cl-remf workspace :state)
      (cl-remf workspace :layout))
    workspace))

(defun atelier-mark-internal-buffer (&optional buffer)
  "Exclude BUFFER from user workspace entry registration."
  (puthash (or buffer (current-buffer)) t atelier-internal-buffers))

(defun atelier-assign-buffer-to-workspace
    (&optional buffer workspace type allow-duplicate-type)
  "Compatibility wrapper for registering BUFFER in WORKSPACE."
  (let ((buffer (or buffer (current-buffer))))
    (remhash buffer atelier-internal-buffers)
    (atelier-register-buffer buffer (or workspace (atelier-current-workspace))
                             nil type allow-duplicate-type)
    buffer))

(defun atelier-buffer-ownable-p (buffer &optional workspace)
  (with-current-buffer buffer
    (let ((name (buffer-name)))
      (and (or workspace (atelier-current-workspace))
           (not atelier-inhibit-buffer-ownership)
           (not atelier-directory-chooser-active)
           (not (gethash buffer atelier-internal-buffers))
           (not (minibufferp buffer))
           (not (string-prefix-p " " name))
           (not (member name atelier-global-buffer-names))
           (not (string-prefix-p atelier-empty-buffer-prefix name))
           (not (member name (list atelier-navigator-buffer atelier-choice-buffer)))))))

(defun atelier-own-current-buffer ()
  (when (atelier-buffer-ownable-p (current-buffer))
    (atelier-register-buffer (current-buffer) (atelier-current-workspace))))

(defun atelier-register-visible-frame-buffers (frame)
  "Register buffers displayed by FRAME in that frame's workspace."
  (when-let* (((frame-live-p frame))
              (workspace (atelier-current-workspace frame)))
    (dolist (window (window-list frame 'no-minibuffer))
      (unless (window-parameter window 'window-side)
        (atelier-register-buffer (window-buffer window) workspace)))))

(defun atelier-refresh-current-buffer-entries ()
  "Refresh every workspace entry resolving to the current buffer."
  (dolist (pair (atelier-entries-for-buffer (current-buffer)))
    (atelier-update-entry-from-buffer (nth 1 pair) (current-buffer)))
  (when (atelier-entries-for-buffer (current-buffer))
    (atelier-notify-change)))

(defun atelier-current-buffer-killed ()
  "Update entry runtime state when the current buffer is killed natively."
  (let ((pairs (atelier-entries-for-buffer (current-buffer)))
        (current-workspace (atelier-current-workspace))
        changed)
    (dolist (pair pairs)
      (let ((workspace (car pair))
            (entry (nth 1 pair)))
        (atelier-entry-set-live-buffer entry nil)
        (when (and (not atelier-preserve-job-recipe)
                   (or (eq workspace current-workspace)
                       (eq (plist-get entry :kind) 'transient)))
          (atelier-entry-remove workspace entry t)
          (setq changed t))))
    (when changed (atelier-notify-change))))

(defun atelier-empty-workspace-buffer (workspace)
  (let ((buffer (get-buffer-create
                 (format "%s%s*" atelier-empty-buffer-prefix
                         (plist-get workspace :name)))))
    (with-current-buffer buffer
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert (format "Workspace %s has no buffers.\n\nOpen the Navigator to create a scratch buffer."
                        (plist-get workspace :name)))
        (special-mode))
      (atelier-mark-internal-buffer))
    buffer))

(defun atelier-find-workspace-buffer (predicate &optional workspace)
  (let ((workspace (or workspace (atelier-current-workspace))))
    (when workspace
      (cl-find-if
       (lambda (buffer)
         (and (buffer-live-p buffer) (funcall predicate buffer workspace)))
       (delq nil (mapcar #'atelier-entry-live-buffer
                         (atelier-workspace-entries workspace)))))))

(defun atelier-buffer-visits-file-p (buffer file)
  (when-let* ((visited (buffer-local-value 'buffer-file-name buffer)))
    (condition-case nil
        (file-equal-p visited file)
      (error nil))))

(defun atelier-create-file-buffer (file)
  "Visit FILE in a new live buffer even when another buffer visits it."
  (let* ((file (abbreviate-file-name (expand-file-name file)))
         (buffer (create-file-buffer file)))
    (condition-case error
        (with-current-buffer buffer
          (let ((inhibit-read-only t))
            (insert-file-contents file t))
          (setq default-directory (file-name-directory file))
          (after-find-file nil nil t)
          buffer)
      (error
       (when (buffer-live-p buffer) (kill-buffer buffer))
       (signal (car error) (cdr error))))))

(defun atelier-file-buffer (file &optional workspace)
  "Return WORKSPACE's buffer for FILE, creating an independent one if needed."
  (setq workspace (or workspace (atelier-current-workspace)))
  (or (atelier-find-workspace-buffer
       (lambda (buffer _workspace)
         (atelier-buffer-visits-file-p buffer file))
       workspace)
      (cl-find-if
       (lambda (buffer)
         (and (atelier-buffer-visits-file-p buffer file)
              (null (atelier-entries-for-buffer buffer))))
       (buffer-list))
      (if (cl-find-if (lambda (buffer) (atelier-buffer-visits-file-p buffer file))
                      (buffer-list))
          (atelier-create-file-buffer file)
        (find-file-noselect file))))

(defun atelier-open-file (file &optional workspace)
  "Open FILE as an entry of WORKSPACE in the selected window."
  (setq workspace (or workspace (atelier-current-workspace)))
  (let ((buffer (atelier-file-buffer file workspace)))
    (atelier-assign-buffer-to-workspace buffer workspace)
    (switch-to-buffer buffer)
    buffer))

(defun atelier-restore-buffer (entry &optional workspace)
  "Restore basic ENTRY in WORKSPACE and return its live buffer."
  (let* ((workspace (or workspace (atelier-current-workspace)))
         (file (plist-get entry :file))
         (name (plist-get entry :name))
         (directory (plist-get entry :directory)))
    (condition-case error
        (let ((buffer (or (atelier-entry-live-buffer entry)
                          (cond
                           ((eq (plist-get entry :kind) 'terminal) nil)
                            ((and file (file-readable-p file))
                             (atelier-file-buffer file workspace))
                           ((and (eq (plist-get entry :kind) 'directory)
                                 directory (file-directory-p directory))
                            (atelier-new-dired-buffer directory t workspace))
                           ((eq (plist-get entry :kind) 'scratch)
                            (let ((buffer (generate-new-buffer (or name "*scratch*"))))
                              (with-current-buffer buffer
                                (funcall initial-major-mode)
                                (insert (or (plist-get entry :contents) "")))
                              buffer))
                           ((and name (get-buffer name)) (get-buffer name))
                           ((eq (plist-get entry :kind) 'transient) nil)
                           (t (get-buffer-create (or name "*atelier entry*")))))))
          (when buffer
            (with-current-buffer buffer
              (cond
               ((and directory (file-directory-p directory))
                (setq default-directory directory))
               ((and directory (not (file-remote-p directory)))
                (setq default-directory (atelier-workspace-directory))
                (myconfig-log "Missing split directory %s; using workspace root %s"
                              directory default-directory)))
              (goto-char (min (point-max)
                              (max (point-min)
                                   (or (plist-get entry :point) 1)))))
            (atelier-entry-set-live-buffer entry buffer)
            (run-hook-with-args 'atelier-entry-restored-hook workspace entry buffer))
          buffer)
      (error
       (run-hook-with-args 'atelier-entry-restore-failed-hook workspace entry error)
       (myconfig-log "Could not restore %s: %s" (or file name) error)
       nil))))

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
                 (let ((buffer (generate-new-buffer
                                (atelier-entry-buffer-name 'dired workspace))))
                   (with-current-buffer buffer
                     (dired-mode directory)
                     (dired-readin))
                   buffer))))))
    (with-current-buffer buffer
      (unless (string-prefix-p (atelier-entry-buffer-name 'dired workspace)
                               (buffer-name))
        (rename-buffer (atelier-entry-buffer-name 'dired workspace) t)))
    buffer))

(defun atelier-register-dired-buffer (buffer workspace &optional explicit)
  "Register BUFFER as WORKSPACE's Dired type.
When EXPLICIT is non-nil, permit another Dired entry of the same type."
  (atelier-assign-buffer-to-workspace buffer workspace 'dired explicit)
  buffer)

(defun atelier-clean-window-buffer-history ()
  (dolist (window (window-list nil 'no-minibuffer))
    (set-window-prev-buffers
     window
     (cl-remove-if
      (lambda (entry)
        (member (buffer-name (car entry))
                (list atelier-navigator-buffer atelier-choice-buffer)))
      (window-prev-buffers window)))
    (set-window-next-buffers
     window
     (cl-remove-if
      (lambda (buffer)
        (member (buffer-name buffer)
                (list atelier-navigator-buffer atelier-choice-buffer)))
      (window-next-buffers window)))))

(defun atelier-layout-split-size (window orientation ratio)
  (let* ((horizontal (eq orientation 'horizontal))
         (total (if horizontal (window-total-width window)
                  (window-total-height window)))
         (minimum (if horizontal window-min-width window-min-height)))
    (max minimum
         (min (- total minimum)
              (round (* total (or ratio 0.5)))))))

(defun atelier-restore-entry-layout (entry window workspace)
  "Materialize ENTRY's recursive layout into WINDOW."
  (if (atelier-layout-entry-p entry)
      (pcase (atelier-entry-children entry)
        (`(,first ,second)
         (let* ((orientation (plist-get entry :orientation))
                (side (if (eq orientation 'horizontal) 'right 'below))
                (size (atelier-layout-split-size
                       window orientation (plist-get entry :ratio)))
                (other (split-window window size side)))
           (atelier-restore-entry-layout first window workspace)
           (atelier-restore-entry-layout second other workspace)))
        (_ (error "Layout entry %s is not binary" (plist-get entry :id))))
    (let ((buffer (or (atelier-entry-live-buffer entry)
                      (atelier-restore-buffer entry workspace))))
      (when (buffer-live-p buffer)
        (set-window-buffer window buffer)
        (set-window-point
         window (min (with-current-buffer buffer (point-max))
                     (max 1 (or (plist-get entry :point) 1))))
        (when-let* ((start (plist-get entry :start)))
          (set-window-start window start t))
        (when (plist-get entry :selected) (select-window window))))))

(defun atelier-restore-workspace (workspace)
  (let ((default-directory (atelier-workspace-directory workspace))
        restored-buffers)
    (unless (file-directory-p default-directory)
      (user-error "Workspace root is unavailable: %s" default-directory))
    (dolist (entry (atelier-workspace-entries workspace))
      (when-let* ((buffer (atelier-restore-buffer entry workspace)))
        (push buffer restored-buffers)))
    (delete-other-windows)
    (if-let* ((entry (atelier-workspace-displayed-entry workspace)))
        (condition-case error
            (atelier-restore-entry-layout entry (window-main-window) workspace)
          (error
           (myconfig-log "Workspace layout restore failed: %s" error)
           (switch-to-buffer (or (car (nreverse restored-buffers))
                                 (atelier-new-dired-buffer default-directory)))))
      (switch-to-buffer (or (car (nreverse restored-buffers))
                            (atelier-empty-workspace-buffer workspace))))
    (atelier-clean-window-buffer-history)
    (when (fboundp 'myconfig-terminal-activate)
      (dolist (window (window-list nil 'no-minibuffer))
        (myconfig-terminal-activate (window-buffer window))))
    (setq default-directory (atelier-workspace-directory workspace))))

(defun atelier-notify-change ()
  (force-mode-line-update t)
  (unless (assq (selected-frame) atelier-navigator-window-configurations)
    (run-hooks 'atelier-change-hook)))

(defun atelier-workspace-stop-jobs (workspace &optional forget)
  (dolist (entry (copy-sequence (atelier-workspace-job-entries workspace)))
    (let ((buffer (atelier-entry-live-buffer entry)))
      (when (buffer-live-p buffer)
        (let ((atelier-preserve-job-recipe (not forget)))
          (when-let* ((process (get-buffer-process buffer)))
            (when (process-live-p process) (delete-process process)))
          (kill-buffer buffer)))
      (atelier-entry-set-live-buffer entry nil)
      (when forget (atelier-entry-remove workspace entry t)))))

(defun atelier-register-job-buffer
    (buffer shell directory &optional direct-command policy agent type explicit)
  (let* ((workspace (or atelier-job-owner-workspace (atelier-current-workspace)))
         (name (buffer-name buffer))
         (entry (or atelier-job-owner-entry
                     (atelier-workspace-entry-for-buffer workspace buffer)
                    (atelier-register-buffer buffer workspace t type explicit)))
         (existing (atelier-entry-job entry))
         (job (or existing
                  (list :id (format "job-%s-%06x" (float-time) (random #xffffff))
                        :buffer name :policy (or policy 'auto) :recipe nil :agent nil))))
    (setf (plist-get job :shell) shell
          (plist-get job :directory) (or directory default-directory)
          (plist-get job :direct-command) direct-command)
    (when agent (setf (plist-get job :agent) (copy-tree agent)))
    (unless (eq (plist-get job :policy) 'never)
      (setf (plist-get job :recipe)
            (if direct-command
                (list :executable (car direct-command)
                      :argv (copy-sequence direct-command)
                      :directory (or directory default-directory))
              (atelier-shell-restart-recipe shell
                                            (or directory default-directory)))))
    (setf (plist-get entry :job) job
          (plist-get entry :kind) 'terminal
          (plist-get entry :persistent) t)
    (when type (setf (plist-get entry :type) type))
    (atelier-entry-set-live-buffer entry buffer)
    (atelier-notify-change)))

(defun atelier-shell-restart-recipe (shell directory)
  (when-let* ((executable (plist-get shell :executable)))
    (let ((arguments (copy-sequence (plist-get shell :login))))
      (list :executable executable
            :argv (cons executable arguments)
            :directory directory
            :shell (copy-tree shell)))))

(defun atelier-find-job-for-buffer (buffer-name)
  (when-let* ((buffer (get-buffer buffer-name)))
    (cl-loop for workspace in atelier-workspaces
             thereis
             (cl-loop for entry in (atelier-workspace-job-entries workspace)
                      for job = (atelier-entry-job entry)
                      when (eq (atelier-entry-live-buffer entry) buffer)
                      return (list workspace job entry)))))

(defconst atelier-ssh-options-with-arguments
  '("-B" "-b" "-c" "-D" "-E" "-e" "-F" "-I" "-i" "-J" "-L"
    "-l" "-m" "-O" "-o" "-P" "-p" "-Q" "-R" "-S" "-W" "-w"))

(defun atelier-ssh-destination-valid-p (destination)
  (and (stringp destination)
       (string-match-p
        (rx string-start
            (optional (+ (any alnum "_.+-")) "@")
            (or (+ (any alnum "_.-"))
                (seq "[" (+ (any xdigit ":.")) "]"))
            string-end)
        destination)
       (not (member destination '("ssh" "localhost")))))

(defun atelier-ssh-destination-from-command (command)
  "Return the OpenSSH destination used by shell COMMAND, if recognizable."
  (condition-case nil
      (let* ((tokens (split-string-shell-command command))
             (ssh-position (cl-position "ssh" tokens :test #'equal))
             (prefix (and ssh-position (cl-subseq tokens 0 ssh-position)))
             (invocation-p
              (and ssh-position
                   (cl-every
                    (lambda (token)
                      (or (member token '("command" "sudo" "env" "exec" "nohup"
                                          "time" "tailscale"))
                          (string-prefix-p "-" token)
                          (string-match-p "=" token)))
                    prefix)))
             (arguments (and invocation-p (nthcdr (1+ ssh-position) tokens)))
             destination)
        (while (and arguments (not destination))
          (let ((argument (pop arguments)))
            (cond
             ((equal argument "--")
              (setq destination (pop arguments)))
             ((member argument atelier-ssh-options-with-arguments)
              (pop arguments))
             ((string-prefix-p "-" argument))
             ((atelier-ssh-destination-valid-p argument)
              (setq destination argument)))))
        (and (atelier-ssh-destination-valid-p destination) destination))
    (error nil)))

(defun atelier-shell-history-commands (file)
  (when-let* ((expanded (expand-file-name file))
              ((file-readable-p expanded)))
    (with-temp-buffer
      (let* ((size (file-attribute-size (file-attributes expanded)))
             (start (max 0 (- size atelier-shell-history-read-limit))))
        (insert-file-contents expanded nil start size)
        (when (> start 0)
          (goto-char (point-min))
          (delete-region (point-min) (min (point-max) (1+ (line-end-position)))))
        (goto-char (point-min))
        (let (commands)
          (while (not (eobp))
            (let ((line (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position))))
              (cond
               ((string-match (rx string-start ": " (+ digit) ":" (+ digit) ";"
                                  (group (* any))) line)
                (push (match-string 1 line) commands))
               ((string-match (rx string-start (* blank) "- cmd:" (* blank)
                                  (group (* any))) line)
                (push (replace-regexp-in-string "\\\\n" " " (match-string 1 line) t t)
                      commands))
               ((not (string-match-p (rx string-start "#" (+ digit) string-end) line))
                (push line commands))))
            (forward-line 1))
          (nreverse commands))))))

(defun atelier-shell-history-ssh-destinations ()
  "Return SSH destinations found in configured shell histories, newest first."
  (let ((files
         (sort (cl-remove-if-not #'file-readable-p
                                 (mapcar #'expand-file-name atelier-shell-history-files))
               (lambda (left right)
                 (time-less-p (file-attribute-modification-time (file-attributes right))
                              (file-attribute-modification-time (file-attributes left))))))
        destinations)
    (dolist (file files)
      (let (file-destinations)
        (dolist (command (atelier-shell-history-commands file))
          (when-let* ((destination (atelier-ssh-destination-from-command command)))
            (push destination file-destinations)))
        (setq destinations (append destinations (delete-dups file-destinations)))))
    (delete-dups destinations)))

(defun atelier-ssh-aliases ()
  (let ((files (list (expand-file-name "~/.ssh/config"))) aliases)
    (while files
      (let ((file (pop files)))
        (when (file-readable-p file)
          (with-temp-buffer
            (insert-file-contents file)
            (goto-char (point-min))
            (while (re-search-forward "^[[:space:]]*Host[[:space:]]+\\(.+\\)$" nil t)
              (dolist (host (split-string (match-string 1)))
                (unless (string-match-p "[*?!]" host) (push host aliases))))
            (goto-char (point-min))
            (while (re-search-forward "^[[:space:]]*Include[[:space:]]+\\(.+\\)$" nil t)
              (dolist (pattern (split-string (match-string 1)))
                (setq files (append (file-expand-wildcards
                                     (expand-file-name pattern
                                                        (expand-file-name "~/.ssh/")))
                                    files))))))))
    (delete-dups (nreverse aliases))))

(defun atelier-read-workspace-target ()
  (let* ((destinations (delete-dups
                        (append '("local")
                                atelier-extra-destinations
                                '("Enter SSH destination")
                                (atelier-ssh-aliases)
                                (atelier-shell-history-ssh-destinations)
                                atelier-remembered-ssh-destinations)))
         (choice (atelier-read-buffer-choice "Machine" destinations))
         (destination (if (equal choice "Enter SSH destination")
                          (read-string "SSH destination: ") choice))
         (platform (cond
                    ((equal destination "local") nil)
                    ((equal destination "WSL") 'wsl)
                    ((equal (atelier-read-buffer-choice
                             "Remote system" '("POSIX" "Windows"))
                            "Windows")
                     'windows)
                    (t 'posix)))
         (destination (if (eq platform 'wsl)
                          (read-string "WSL distribution: " (or (getenv "WSL_DISTRO_NAME") "Ubuntu"))
                        destination))
         (mount-root (when (eq platform 'windows)
                       (format "/%s:/" (upcase (read-string "Windows drive: " "C")))))
         (probe (when (memq platform '(windows wsl))
                  (list :destination destination :platform 'windows
                        :mount-root mount-root :path (if (eq platform 'wsl) "/" mount-root))))
         (remote-prefix (cond ((eq platform 'wsl) (format "/wsl:%s:" destination))
                              ((eq platform 'posix) (format "/ssh:%s:" destination))))
         (directory (atelier-read-directory-with-dired
                     (cond ((eq platform 'wsl) remote-prefix)
                            (probe (funcall atelier-directory-function probe))
                           (remote-prefix remote-prefix)
                            (t (expand-file-name "~/"))))))
    (unless (file-directory-p directory)
      (when (yes-or-no-p (format "Create %s? " directory)) (make-directory directory t)))
    (unless (file-directory-p directory) (user-error "Directory does not exist: %s" directory))
    (when remote-prefix
      (cl-pushnew destination atelier-remembered-ssh-destinations :test #'equal))
    (list destination
          (cond ((eq platform 'wsl) (file-remote-p directory 'localname))
                (probe (funcall atelier-target-directory-function probe directory))
                (remote-prefix (file-remote-p directory 'localname))
                (t directory))
          platform mount-root)))

(defun atelier-edit-workspace ()
  (interactive)
  (let ((workspace (atelier-current-workspace)))
    (unless workspace (user-error "No workspace is open"))
    (when (atelier-detached-workspace-p workspace)
      (user-error "The Detached workspace target cannot be changed"))
    (let ((old-directory (atelier-workspace-directory workspace)))
      (pcase-let ((`(,destination ,path ,platform ,mount-root) (atelier-read-workspace-target)))
        (funcall atelier-release-function workspace)
        (setf (plist-get workspace :destination) destination
              (plist-get workspace :path) path
              (plist-get workspace :platform) platform
              (plist-get workspace :mount-root) mount-root)
        (dolist (entry (atelier-workspace-entries workspace))
          (when (equal (plist-get entry :directory) old-directory)
            (setf (plist-get entry :directory) (atelier-workspace-directory workspace))))
        (atelier-restore-workspace workspace)
        (atelier-notify-change)))))

(defun atelier-create-workspace ()
  (interactive)
  (pcase-let* ((`(,destination ,path ,platform ,mount-root) (atelier-read-workspace-target))
               (suggested-name
                (myconfig-safe-name (file-name-nondirectory (directory-file-name path))))
               (suggestion (if (string-empty-p suggested-name) "home" suggested-name))
               (name (read-string "Workspace name: " suggestion)))
    (when (string-empty-p name) (user-error "Workspace name cannot be empty"))
    (when (atelier-workspace-get name) (user-error "Workspace already exists: %s" name))
    (let ((workspace (list :id (atelier-new-workspace-id)
                           :name name :destination destination :path path
                           :platform platform :mount-root mount-root
                           :created (float-time) :status 'running
                           :entries nil)))
      (atelier-capture-current-workspace)
      (setq atelier-workspaces (append atelier-workspaces (list workspace)))
      (run-hook-with-args 'atelier-workspace-created-hook workspace)
      (atelier-select-workspace workspace)
      (delete-other-windows)
      (let ((buffer (atelier-new-dired-buffer (atelier-workspace-directory workspace)
                                              t workspace)))
        (atelier-register-dired-buffer buffer workspace t)
        (switch-to-buffer buffer))
      (atelier-notify-change))))

(defun atelier-unique-workspace-name (root)
  (let* ((base (or (myconfig-safe-name
                    (file-name-nondirectory (directory-file-name root)))
                   "workspace"))
         (name base)
         (number 2))
    (while (atelier-workspace-get name)
      (setq name (format "%s-%d" base number)
            number (1+ number)))
    name))

(defun atelier-workspace-project-root (workspace)
  (let ((destination (plist-get workspace :destination))
        (path (plist-get workspace :path)))
    (cond
     ((equal destination "local") (myconfig-normalize-directory path))
     ((memq (plist-get workspace :platform) '(windows wsl)) nil)
     (t (myconfig-normalize-directory (format "/ssh:%s:%s" destination path))))))

(defun atelier-known-project-roots ()
  (cl-remove-if
   (lambda (root)
     (and (boundp 'package-user-dir)
          (file-in-directory-p root package-user-dir)))
   (project-known-project-roots)))

(defun atelier-open-project-workspace (root)
  (let* ((root (file-name-as-directory root))
         (existing
          (cl-find-if
           (lambda (workspace)
             (equal (atelier-workspace-project-root workspace)
                    (myconfig-normalize-directory root)))
           atelier-workspaces)))
    (if existing
        (atelier-switch-workspace (plist-get existing :name))
      (unless (file-directory-p root)
        (user-error "Project directory does not exist: %s" root))
      (let* ((remote (file-remote-p root))
             (name (atelier-unique-workspace-name root))
             (workspace
              (list :id (atelier-new-workspace-id) :name name
                    :destination (or (file-remote-p root 'host) "local")
                    :path (if remote (file-remote-p root 'localname) root)
                    :platform (and remote 'posix)
                    :mount-root nil :created (float-time) :status 'running
                    :entries nil)))
        (atelier-capture-current-workspace)
        (setq atelier-workspaces (append atelier-workspaces (list workspace)))
        (run-hook-with-args 'atelier-workspace-created-hook workspace)
        (atelier-select-workspace workspace)
        (delete-other-windows)
        (let ((buffer (atelier-new-dired-buffer root t workspace)))
          (atelier-register-dired-buffer buffer workspace t)
          (switch-to-buffer buffer))
        (atelier-notify-change)))))

(defun atelier-current-entry (&optional buffer workspace)
  "Return BUFFER's entry in WORKSPACE.
BUFFER defaults to the current buffer and WORKSPACE to the current frame's
workspace."
  (atelier-workspace-entry-for-buffer
   (or workspace (atelier-current-workspace))
   (or buffer (current-buffer))))

(defun atelier-open-entry (entry &optional workspace)
  "Open ENTRY from WORKSPACE in the selected window.
Interactively, choose an entry from the current workspace."
  (interactive
   (let* ((workspace (or (atelier-current-workspace)
                         (user-error "No workspace is selected")))
          (choices
           (mapcar (lambda (entry)
                     (cons (format "%s  [%s]"
                                   (or (plist-get entry :name) "Untitled")
                                   (plist-get entry :kind))
                           entry))
                   (atelier-workspace-entries workspace))))
     (unless choices (user-error "The current workspace has no entries"))
     (list (cdr (assoc-string
                 (completing-read "Workspace entry: " choices nil t)
                 choices))
           workspace)))
  (setq workspace (or workspace (atelier-entry-workspace entry)))
  (unless workspace (user-error "Entry has no workspace"))
  (unless (eq workspace (atelier-current-workspace))
    (atelier-switch-workspace (plist-get workspace :name)))
  (if-let* ((buffer (or (atelier-entry-live-buffer entry)
                        (atelier-restore-buffer entry workspace))))
      (progn
        (switch-to-buffer buffer)
        (when (fboundp 'myconfig-terminal-activate)
          (myconfig-terminal-activate buffer))
        buffer)
    (user-error "Could not restore entry %s" (or (plist-get entry :name)
                                                 (plist-get entry :id)))))

(defun atelier-move-current-entry (workspace-name)
  "Move the current entry to WORKSPACE-NAME without changing its stable ID."
  (interactive
   (list (completing-read
          "Move entry to workspace: "
          (mapcar (lambda (workspace) (plist-get workspace :name))
                  atelier-workspaces)
          nil t)))
  (let* ((source-workspace (or (atelier-current-workspace)
                               (user-error "No workspace is selected")))
         (entry (or (atelier-current-entry (current-buffer) source-workspace)
                    (user-error "Current buffer is not a workspace entry")))
         (target-workspace (or (atelier-workspace-get workspace-name)
                               (user-error "Unknown workspace: %s" workspace-name))))
    (unless (eq source-workspace target-workspace)
      (atelier-entry-move entry source-workspace target-workspace))
    entry))

(defun atelier-switch-workspace (name)
  (interactive (list (completing-read "Workspace: " (mapcar (lambda (w) (plist-get w :name)) atelier-workspaces) nil t)))
  (let* ((frame (selected-frame))
         (old-workspace (atelier-current-workspace frame))
         (old-configuration (current-window-configuration frame))
         (workspace (atelier-workspace-get name))
         (was-running (and workspace (eq (atelier-workspace-status workspace) 'running))))
    (unless workspace (user-error "Unknown workspace: %s" name))
    (unless (eq old-workspace workspace)
      (run-hook-with-args 'atelier-before-switch-workspace-hook
                          frame old-workspace workspace)
      (when old-workspace (atelier-capture-current-workspace))
      (condition-case error
          (progn
            (atelier-select-workspace workspace frame)
            (atelier-set-workspace-status workspace 'running)
            (when (and (not was-running) (fboundp 'myconfig-restart-saved-jobs))
              (myconfig-restart-saved-jobs workspace))
            (atelier-restore-workspace workspace)
            (run-hook-with-args 'atelier-after-switch-workspace-hook
                                frame old-workspace workspace)
            (atelier-notify-change))
        (error
         (atelier-select-workspace old-workspace frame)
         (set-window-configuration old-configuration)
         (signal (car error) (cdr error)))))))

(defun atelier-rename-workspace ()
  (interactive)
  (let* ((workspace (atelier-current-workspace))
         (old (plist-get workspace :name)))
    (when (atelier-detached-workspace-p workspace)
      (user-error "The Detached workspace cannot be renamed"))
    (let ((new (read-string "New workspace name: " old)))
      (when (string-empty-p new) (user-error "Workspace name cannot be empty"))
      (when (and (not (equal old new)) (atelier-workspace-get new))
        (user-error "Workspace already exists: %s" new))
      (setf (plist-get workspace :name) new)
      (run-hook-with-args 'atelier-workspace-renamed-hook workspace old new)
      (atelier-notify-change))))

(defun atelier-show-detached-workspace ()
  (let ((workspace (atelier-select-workspace nil)))
    (atelier-set-workspace-status workspace 'running)
    (delete-other-windows)
    (atelier-restore-workspace workspace)))

(defun atelier-close-workspace (&optional confirmed)
  (interactive)
  (let ((workspace (atelier-current-workspace)))
    (when (atelier-detached-workspace-p workspace)
      (user-error "The Detached workspace cannot be closed"))
    (unless (or confirmed
                (y-or-n-p (format "Close workspace %s and stop its jobs? "
                                  (plist-get workspace :name))))
      (user-error "Cancelled"))
    (atelier-capture-current-workspace)
    (atelier-workspace-stop-jobs workspace)
    (atelier-set-workspace-status workspace 'stopped)
    (funcall atelier-release-function workspace)
    (let ((next (or (cl-find-if (lambda (item)
                                  (and (not (eq item workspace))
                                       (eq (atelier-workspace-status item) 'running)))
                                atelier-workspaces)
                    (cl-find-if (lambda (item) (not (eq item workspace))) atelier-workspaces))))
      (when (and next (eq (atelier-workspace-status next) 'stopped)
                 (fboundp 'myconfig-restart-saved-jobs))
        (atelier-set-workspace-status next 'running)
        (myconfig-restart-saved-jobs next))
      (dolist (frame (frame-list))
        (when (eq (atelier-current-workspace frame) workspace)
          (with-selected-frame frame
            (if next
                (progn
                  (atelier-select-workspace next frame)
                  (atelier-set-workspace-status next 'running)
                  (when (display-graphic-p frame) (atelier-restore-workspace next)))
              (atelier-show-detached-workspace)))))
      (atelier-notify-change))))

(defun atelier-delete-workspace-record (workspace)
  (when (atelier-detached-workspace-p workspace)
    (user-error "The Detached workspace cannot be deleted"))
  (let ((id (atelier-workspace-id workspace)))
    (atelier-workspace-stop-jobs workspace t)
    (funcall atelier-release-function workspace t)
    (setq atelier-workspaces (delq workspace atelier-workspaces))
    (dolist (frame (frame-list))
      (when (equal (atelier-current-workspace-id frame) id)
        (with-selected-frame frame
          (if-let* ((next (car atelier-workspaces)))
              (progn
                (atelier-select-workspace next frame)
                (when (display-graphic-p frame) (atelier-restore-workspace next)))
            (atelier-show-detached-workspace)))))
    (run-hook-with-args 'atelier-workspace-deleted-hook workspace)
    (atelier-notify-change)))

(defun atelier-delete-workspace (&optional confirmed)
  (interactive)
  (let ((workspace (atelier-current-workspace)))
    (unless (or confirmed
                (yes-or-no-p (format "Delete workspace definition %s? "
                                     (plist-get workspace :name))))
      (user-error "Cancelled"))
    (atelier-delete-workspace-record workspace)))

(defun atelier-split-right ()
  (interactive)
  (let* ((workspace (atelier-current-workspace))
         (directory default-directory)
         (window (split-window-right)))
    (condition-case error
        (let ((buffer (atelier-new-dired-buffer directory t workspace)))
          (atelier-register-dired-buffer buffer workspace t)
          (set-window-buffer window buffer)
          (select-window window)
          (atelier-notify-change))
      (error
       (when (window-live-p window) (delete-window window))
       (signal (car error) (cdr error))))))

(defun atelier-file-browser ()
  (interactive)
  (when (window-parameter nil 'window-side)
    (select-window (window-main-window)))
  (let* ((workspace (atelier-current-workspace))
         (in-dired (derived-mode-p 'dired-mode))
         (directory (unless (derived-mode-p 'atelier-navigator-mode)
                      default-directory))
         (existing (and (not in-dired)
                        (atelier-workspace-buffer-by-type workspace 'dired))))
    (when (assq (selected-frame) atelier-navigator-window-configurations)
      (atelier-navigator-quit))
    (setq directory (if (and directory (file-directory-p directory))
                        directory
                      (if (file-directory-p default-directory)
                          default-directory
                        (atelier-workspace-directory))))
    (let* ((explicit in-dired)
           (buffer (cond
                   (in-dired (atelier-new-dired-buffer directory t workspace))
                   (existing existing)
                   (t (atelier-new-dired-buffer directory nil workspace)))))
      (unless existing
        (atelier-register-dired-buffer buffer workspace explicit))
      (switch-to-buffer buffer))))

(defun atelier-dired-open ()
  (interactive)
  (if atelier-directory-chooser-mode
      (atelier-directory-chooser-enter)
    (let ((file (dired-get-file-for-visit)))
      (if (file-directory-p file)
          (atelier-dired-change-directory file)
        (atelier-open-file file)))))

(defun atelier-dired-change-directory (directory &optional target)
  "Read DIRECTORY into the current Dired buffer and keep its workspace entry."
  (setq directory (file-name-as-directory (expand-file-name directory)))
  (setq dired-directory directory
        default-directory directory)
  (dired-readin)
  (when target (dired-goto-file target))
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

(defun atelier-split-below ()
  (interactive)
  (let* ((workspace (atelier-current-workspace))
         (directory default-directory)
         (window (split-window-below)))
    (condition-case error
        (let ((buffer (atelier-new-dired-buffer directory t workspace)))
          (atelier-register-dired-buffer buffer workspace t)
          (set-window-buffer window buffer)
          (select-window window)
          (atelier-notify-change))
      (error
       (when (window-live-p window) (delete-window window))
       (signal (car error) (cdr error))))))

(defun atelier-close-split ()
  (interactive)
  (if (one-window-p) (user-error "The workspace has only one split")
    (delete-window)
    (atelier-notify-change)))

(defun atelier-resize-split (direction amount)
  (interactive)
  (pcase direction
    ('left (shrink-window-horizontally amount))
    ('right (enlarge-window-horizontally amount))
    ('up (shrink-window amount))
    ('down (enlarge-window amount)))
  (atelier-notify-change))

(defun atelier-move-workspace (delta)
  (interactive "p")
  (let* ((workspace (atelier-current-workspace))
         (index (cl-position workspace atelier-workspaces))
         (target (mod (+ index delta) (length atelier-workspaces))))
    (when (atelier-detached-workspace-p workspace)
      (user-error "The Detached workspace cannot be reordered"))
    (setq atelier-workspaces (delete workspace atelier-workspaces))
    (setq atelier-workspaces
          (append (cl-subseq atelier-workspaces 0 target) (list workspace)
                  (cl-subseq atelier-workspaces target)))
    (atelier-notify-change)))

(defun atelier-workspace-menu (event name)
  (interactive "e")
  (atelier-switch-workspace name)
  (popup-menu
   '("Workspace"
     ["Rename" atelier-rename-workspace t]
     ["Edit target" atelier-edit-workspace t]
     ["Close" atelier-close-workspace t]
     ["Delete" atelier-delete-workspace t])))

(defun atelier-clickable-label (label action &optional context-action face help)
  (let ((map (make-sparse-keymap)))
    (define-key map [mouse-1] action)
    (define-key map [mouse-2] action)
    (define-key map [mode-line mouse-1] action)
    (define-key map [mode-line mouse-2] action)
    (define-key map [header-line mouse-1] action)
    (define-key map [header-line mouse-2] action)
    (when context-action (define-key map [mouse-3] context-action))
    (propertize label 'face face 'mouse-face 'atelier-navigator-hover 'help-echo help
                'follow-link t 'keymap map)))

(defun atelier-create-default ()
  (let* ((root (myconfig-project-root))
         (name (or (myconfig-safe-name (file-name-nondirectory (directory-file-name root))) "home")))
    (let ((workspace
           (list :id (atelier-new-workspace-id)
                 :name name :destination "local" :path root :platform 'local
                 :mount-root nil :created (float-time)
                 :status 'running :entries nil)))
      (setq atelier-workspaces (list workspace))
      (atelier-ensure-detached-workspace)
      (atelier-select-workspace workspace)
      (run-hook-with-args 'atelier-workspace-created-hook workspace))))

(defun atelier-capture-closing-frame (frame)
  (when (and (frame-live-p frame) (display-graphic-p frame)
             (not (assq frame atelier-navigator-window-configurations)))
    (with-selected-frame frame
      (atelier-capture-current-workspace)
      (atelier-notify-change)))
  (setq atelier-navigator-window-configurations
        (assq-delete-all frame atelier-navigator-window-configurations)))

(defun atelier-restore-new-frame (frame)
  (when (and (frame-live-p frame) (display-graphic-p frame) atelier-workspaces)
    (with-selected-frame frame
      (let ((workspace (or (atelier-current-workspace) (car atelier-workspaces))))
        (atelier-select-workspace workspace frame)
        (atelier-restore-workspace workspace))
      (atelier-navigator))))

(defun atelier-navigator-at-startup ()
  (when (display-graphic-p)
    (atelier-navigator)))

(defun atelier-setup ()
  (unless atelier-workspaces (atelier-create-default))
  (atelier-ensure-detached-workspace)
  (unless (atelier-current-workspace)
    (atelier-select-workspace (car atelier-workspaces)))
  (setq tramp-connection-timeout 10)
  (setq evil-normal-state-tag (propertize " NORMAL " 'face 'myconfig-mode-line-state)
        evil-insert-state-tag (propertize " INSERT " 'face 'myconfig-mode-line-state)
        evil-visual-state-tag (propertize " VISUAL " 'face 'myconfig-mode-line-state)
        evil-replace-state-tag (propertize " REPLACE " 'face 'myconfig-mode-line-state)
        evil-operator-state-tag (propertize " OPERATOR " 'face 'myconfig-mode-line-state)
        evil-motion-state-tag (propertize " MOTION " 'face 'myconfig-mode-line-state)
        evil-emacs-state-tag (propertize " EMACS " 'face 'myconfig-mode-line-state))
  (setq evil-mode-line-format '(before . atelier-evil-mode-line-anchor))
  (setq-default
   mode-line-format
   '("%e" atelier-evil-mode-line-anchor
     "  " mode-line-buffer-identification
     (:eval (atelier-mode-line-status))
     (:eval (atelier-mode-line-navigator))
     mode-line-format-right-align
     (:eval (format "%s  " (if-let* ((workspace (atelier-current-workspace)))
                               (plist-get workspace :name)
                             "No workspace")))
     (:eval (atelier-mode-line-position))
     "  "))
  (dolist (name (append atelier-global-buffer-names
                        (list atelier-navigator-buffer atelier-choice-buffer)))
    (when-let* ((buffer (get-buffer name)))
      (atelier-mark-internal-buffer buffer)))
  ;; Emacs creates *scratch* before Atelier knows which workspace to restore.
  ;; It is a bootstrap buffer, never evidence of workspace ownership.
  (when-let* ((scratch (get-buffer "*scratch*")))
    (atelier-mark-internal-buffer scratch))
  (add-hook 'window-configuration-change-hook #'atelier-notify-change)
  (add-hook 'window-buffer-change-functions #'atelier-register-visible-frame-buffers)
  (add-hook 'after-rename-buffer-hook #'atelier-refresh-current-buffer-entries)
  (add-hook 'kill-buffer-hook #'atelier-current-buffer-killed)
  (add-hook 'delete-frame-functions #'atelier-capture-closing-frame)
  (add-hook 'after-make-frame-functions #'atelier-restore-new-frame)
  (add-hook 'emacs-startup-hook #'atelier-navigator-at-startup)
  nil)

;; UI modules depend on the complete service layer above, while the service
;; layer only calls their commands at runtime.
(require 'atelier-choice)
(require 'atelier-navigator)
(defalias 'atelier-close-current-entry #'atelier-close-current-view
  "Close the current workspace entry and its live buffer.")

(provide 'atelier)
;;; atelier.el ends here

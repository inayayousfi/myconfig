;;; atelier.el --- Workspace and split state -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'dired)
(require 'project)
(require 'subr-x)
(require 'tramp)
(require 'myconfig-core)
(require 'myconfig-windows)

(defvar atelier-workspaces nil)
(defvar atelier-current-workspace-name nil)
(defvar atelier-remembered-ssh-destinations nil)
(defvar atelier-change-hook nil)
(defvar atelier-job-owner-workspace nil)
(defvar atelier-preserve-job-recipe nil)
(defvar atelier-navigator-window-configurations nil)
(defvar atelier-agent-restored-functions nil)
(defvar atelier-detached-buffers nil)
(defvar atelier-directory-choice-result nil)
(defvar atelier-directory-chooser-active nil)
(defvar atelier-directory-chooser-buffers nil)
(defvar atelier-choice-result nil)
(defvar atelier-navigator-attach-source nil)
(defvar atelier-evil-mode-line-anchor nil)
(defvar atelier-command-source-buffer nil)
(defvar atelier-command-source-orphaned-p nil)
(defvar-local atelier-buffer-workspace nil)
(defvar-local atelier-buffer-global nil)
(defconst atelier-workspace-buffer-limit 3)
(defconst atelier-navigator-buffer "*Atelier*")
(defconst atelier-choice-buffer "*Atelier choice*")
(defconst atelier-global-buffer-names
  '("*Messages*" "*Warnings*" "*Completions*" "*Native-compile-Log*"))
(defvar-local atelier-navigator-first-position nil)
(defvar-local atelier-directory-chooser-original-header nil)
(defvar-local atelier-directory-chooser-header-was-local nil)
(defvar-local atelier-directory-chooser-original-modified nil)

(defface atelier-navigator-active
  '((t (:inherit mode-line-highlight :underline t)))
  "Selected workspace in the navigator.")

(defface atelier-navigator-live
  '((t (:inherit shadow)))
  "Live inactive workspace in the navigator.")

(defface atelier-navigator-saved
  '((t (:inherit shadow :strike-through t)))
  "Stopped workspace in the navigator.")

(defface atelier-navigator-hover
  '((t (:background "#ff4ead" :foreground "#000000" :weight bold)))
  "Readable pointer hover for navigator controls.")

(defface atelier-navigator-current
  '((t (:background "#ff4ead" :foreground "#000000" :weight bold :extend t)))
  "Keyboard-selected navigator row.")

(defun atelier-workspace-get (name)
  (cl-find name atelier-workspaces :key (lambda (workspace) (plist-get workspace :name)) :test #'equal))

(defun atelier-current-workspace ()
  (atelier-workspace-get atelier-current-workspace-name))

(defun atelier-workspace-directory (&optional workspace)
  (let* ((workspace (or workspace (atelier-current-workspace)))
         (destination (plist-get workspace :destination))
         (path (plist-get workspace :path)))
    (unless workspace (user-error "No workspace is open"))
    (cond
     ((equal destination "local") (myconfig-normalize-directory path))
      ((myconfig-wsl-workspace-p workspace)
       (myconfig-wsl-workspace-directory workspace))
      ((myconfig-windows-workspace-p workspace)
       (myconfig-windows-workspace-directory workspace))
      (t (format "/ssh:%s:%s" destination (file-name-as-directory path))))))

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

(defun atelier-capture-buffer (buffer &optional window)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (list :name (buffer-name)
            :file buffer-file-name
            :directory default-directory
            :dired (derived-mode-p 'dired-mode)
            :scratch (string-prefix-p "*scratch*" (buffer-name))
            :point (point)
            :start (and (window-live-p window) (window-start window))
            :selected (and (window-live-p window) (eq window (selected-window)))))))

(defun atelier-buffer-persistent-p (buffer)
  (with-current-buffer buffer
    (or buffer-file-name
        (derived-mode-p 'dired-mode)
        (string-prefix-p "*scratch*" (buffer-name)))))

(defun atelier-live-owned-buffer-descriptors (workspace &optional persistent-only)
  (let ((name (plist-get workspace :name)) descriptors)
    (dolist (buffer (buffer-list))
      (when (and (equal (buffer-local-value 'atelier-buffer-workspace buffer) name)
                 (not (atelier-find-job-for-buffer (buffer-name buffer)))
                 (or (not persistent-only) (atelier-buffer-persistent-p buffer)))
        (push (atelier-capture-buffer buffer) descriptors)))
    (nreverse (delq nil descriptors))))

(defun atelier-capture-current-workspace ()
  (when-let* ((_ (display-graphic-p (selected-frame)))
              (workspace (atelier-current-workspace)))
    (setf (plist-get workspace :state) (window-state-get (window-main-window) t)
          (plist-get workspace :buffers)
          (delq nil (mapcar (lambda (window) (atelier-capture-buffer (window-buffer window) window))
                            (cl-remove-if (lambda (window) (window-parameter window 'window-side))
                                          (window-list nil 'no-minibuffer))))
          (plist-get workspace :owned-buffers)
          (atelier-live-owned-buffer-descriptors workspace t))
    workspace))

(defun atelier-forget-buffer-owner ()
  (when-let* ((workspace (atelier-workspace-get atelier-buffer-workspace)))
    (setf (plist-get workspace :owned-buffers)
          (cl-remove (buffer-name) (plist-get workspace :owned-buffers)
                     :key (lambda (descriptor) (plist-get descriptor :name))
                     :test #'equal))))

(defun atelier-assign-buffer-to-workspace (&optional buffer workspace)
  (let ((buffer (or buffer (current-buffer)))
        (workspace (or workspace (atelier-current-workspace))))
    (when (and (buffer-live-p buffer) workspace)
      (with-current-buffer buffer
        (when-let* ((old-workspace (and atelier-buffer-workspace
                                        (atelier-workspace-get atelier-buffer-workspace))))
          (unless (eq old-workspace workspace)
            (setf (plist-get old-workspace :owned-buffers)
                  (cl-remove (buffer-name) (plist-get old-workspace :owned-buffers)
                             :key (lambda (descriptor) (plist-get descriptor :name))
                             :test #'equal))))
        (setq-local atelier-buffer-global nil
                    atelier-buffer-workspace (plist-get workspace :name))
        (add-hook 'kill-buffer-hook #'atelier-forget-buffer-owner nil t)
        (when (atelier-buffer-persistent-p buffer)
          (let ((descriptor (atelier-capture-buffer buffer)))
            (setf (plist-get workspace :owned-buffers)
                  (cons descriptor
                         (cl-remove (buffer-name buffer) (plist-get workspace :owned-buffers)
                                    :key (lambda (item) (plist-get item :name))
                                    :test #'equal)))))))
    (when workspace
      (atelier-prune-workspace-buffers workspace buffer))
    buffer))

(defun atelier-prune-workspace-buffers (workspace keep)
  (let ((buffers
         (cl-remove-if
          (lambda (buffer)
            (or (not (equal (buffer-local-value 'atelier-buffer-workspace buffer)
                            (plist-get workspace :name)))
                (when-let* ((owner (atelier-find-job-for-buffer (buffer-name buffer))))
                  (plist-get (nth 1 owner) :agent))))
          (buffer-list))))
    (while (> (length buffers) atelier-workspace-buffer-limit)
      (let ((oldest (cl-find-if (lambda (buffer) (not (eq buffer keep)))
                                (reverse buffers))))
        (unless oldest (setq buffers nil))
        (when oldest
          (atelier-close-buffer (buffer-name oldest))
          (setq buffers (delq oldest buffers)))))))

(defun atelier-buffer-ownable-p (buffer)
  (with-current-buffer buffer
    (let ((name (buffer-name)))
      (and (atelier-current-workspace)
           (not atelier-directory-chooser-active)
           (not atelier-buffer-global)
            (not (minibufferp buffer))
            (not (string-prefix-p " " name))
            (not (string-prefix-p "*scratch*" name))
            (not (member name atelier-global-buffer-names))
            (not (member name (list atelier-navigator-buffer atelier-choice-buffer)))))))

(defun atelier-buffer-orphaned-p (&optional buffer)
  (with-current-buffer (or buffer (current-buffer))
    (and atelier-buffer-global (null atelier-buffer-workspace))))

(defun atelier-command-source-capture ()
  (setq atelier-command-source-buffer (current-buffer)
        atelier-command-source-orphaned-p (atelier-buffer-orphaned-p)))

(defun atelier-command-source-clear ()
  (setq atelier-command-source-buffer nil
        atelier-command-source-orphaned-p nil))

(defun atelier-own-current-buffer ()
  (when (atelier-buffer-ownable-p (current-buffer))
    (if (and atelier-command-source-orphaned-p
             (not (eq (current-buffer) atelier-command-source-buffer))
             (null atelier-buffer-workspace))
        (setq-local atelier-buffer-global t)
      (when (or (null atelier-buffer-workspace)
                (null (atelier-workspace-get atelier-buffer-workspace)))
        (atelier-assign-buffer-to-workspace)))))

(defun atelier-find-workspace-buffer (predicate &optional workspace)
  (let ((workspace (or workspace (atelier-current-workspace))))
    (when workspace
      (cl-find-if
       (lambda (buffer)
         (and (buffer-live-p buffer)
              (with-current-buffer buffer
                (or (equal atelier-buffer-workspace
                           (plist-get workspace :name))
                    (when-let* ((owner (atelier-find-job-for-buffer (buffer-name buffer))))
                      (eq (car owner) workspace)))
              (funcall predicate buffer workspace))))
       (buffer-list)))))

(defun atelier-file-buffer (file)
  (or (cl-find-if
       (lambda (buffer)
         (when-let* ((visited (buffer-local-value 'buffer-file-name buffer)))
           (condition-case nil
               (file-equal-p visited file)
             (error nil))))
       (buffer-list))
      (find-file-noselect file)))

(defun atelier-restore-buffer (descriptor &optional workspace)
  (let ((file (plist-get descriptor :file))
        (name (plist-get descriptor :name))
        (directory (plist-get descriptor :directory)))
    (condition-case error
        (let ((buffer
               (cond
                  ((and file (file-readable-p file)) (atelier-file-buffer file))
                 ((get-buffer name) (get-buffer name))
                  ((and (plist-get descriptor :dired)
                        directory (file-directory-p directory))
                    (atelier-new-dired-buffer directory nil workspace))
                  ((and workspace
                        (cl-find name (plist-get workspace :jobs)
                                 :key (lambda (job) (plist-get job :buffer))
                                 :test #'equal))
                   nil)
                  (t (get-buffer-create name)))))
          (when buffer
            (with-current-buffer buffer
              (when (and (plist-get descriptor :scratch)
                         (eq major-mode 'fundamental-mode))
                (funcall initial-major-mode))
              (cond
               ((and directory (file-directory-p directory))
                (setq default-directory directory))
               ((and directory (not (file-remote-p directory)))
                (setq default-directory (atelier-workspace-directory))
                (myconfig-log "Missing split directory %s; using workspace root %s"
                              directory default-directory)))
              (goto-char (min (point-max) (max (point-min) (or (plist-get descriptor :point) 1)))))
            (when workspace (atelier-assign-buffer-to-workspace buffer workspace)))
          buffer)
      (error
       (myconfig-log "Could not restore %s: %s" (or file name) error)
       nil))))

(defun atelier-new-dired-buffer (directory &optional force-new workspace)
  (or (unless force-new
        (atelier-find-workspace-buffer
         (lambda (buffer _workspace)
           (with-current-buffer buffer
             (and (derived-mode-p 'dired-mode)
                  (condition-case nil
                      (file-equal-p default-directory directory)
                    (error nil)))))
         workspace))
      (if force-new
          (let ((buffer (generate-new-buffer
                         (file-name-nondirectory (directory-file-name directory)))))
            (with-current-buffer buffer
              (dired-mode directory))
            buffer)
        (dired-noselect directory))))

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

(defun atelier-remove-empty-restored-scratch-buffers (workspace)
  (dolist (buffer (buffer-list))
    (when (and (buffer-live-p buffer)
               (string-prefix-p "*scratch*" (buffer-name buffer))
               (not (equal (buffer-name buffer) "*scratch*"))
               (zerop (buffer-size buffer))
               (not (buffer-local-value 'atelier-buffer-global buffer)))
      (when (eq buffer (current-buffer))
        (switch-to-buffer (atelier-new-dired-buffer
                           (atelier-workspace-directory workspace))))
      (kill-buffer buffer))))

(defun atelier-restore-workspace (workspace)
  (let ((default-directory (atelier-workspace-directory workspace)))
    (unless (file-directory-p default-directory)
      (user-error "Workspace root is unavailable: %s" default-directory))
    (dolist (descriptor (delete-dups
                         (append (copy-sequence (plist-get workspace :buffers))
                                 (copy-sequence (plist-get workspace :owned-buffers)))))
      (atelier-restore-buffer descriptor workspace))
    (delete-other-windows)
    (if-let* ((state (plist-get workspace :state)))
        (condition-case error
            (window-state-put state (window-main-window) 'safe)
      (error
            (myconfig-log "Workspace layout restore failed: %s" error)
            (switch-to-buffer (atelier-new-dired-buffer default-directory))))
      (switch-to-buffer (atelier-new-dired-buffer default-directory)))
    (atelier-clean-window-buffer-history)
     (when (fboundp 'myconfig-terminal-activate)
       (dolist (window (window-list nil 'no-minibuffer))
         (myconfig-terminal-activate (window-buffer window))))
     (atelier-remove-empty-restored-scratch-buffers workspace)
     (setq default-directory (atelier-workspace-directory workspace))))

(defun atelier-notify-change ()
  (force-mode-line-update t)
  (unless (assq (selected-frame) atelier-navigator-window-configurations)
    (run-hooks 'atelier-change-hook)))

(defun atelier-workspace-stop-jobs (workspace &optional forget)
  (dolist (buffer-name
           (delete-dups
            (append (mapcar (lambda (job) (plist-get job :buffer)) (plist-get workspace :jobs))
                    (list (plist-get workspace :agent-buffer)))))
      (when-let* ((buffer (and buffer-name (get-buffer buffer-name))))
      (when-let* ((process (get-buffer-process buffer)))
        (when (process-live-p process)
          (let ((atelier-preserve-job-recipe (not forget)))
            (delete-process process))))
      (kill-buffer buffer)))
  (when forget (setf (plist-get workspace :jobs) nil))
  (setf (plist-get workspace :agent-buffer) nil))

(defun atelier-register-job-buffer (buffer shell directory &optional direct-command policy agent)
  (let* ((workspace (or atelier-job-owner-workspace (atelier-current-workspace)))
         (name (buffer-name buffer))
         (existing (cl-find name (plist-get workspace :jobs)
                            :key (lambda (job) (plist-get job :buffer)) :test #'equal))
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
    (unless existing (push job (plist-get workspace :jobs)))
    (atelier-assign-buffer-to-workspace buffer workspace)
    (atelier-notify-change)))

(defun atelier-shell-restart-recipe (shell directory)
  (when-let* ((executable (plist-get shell :executable)))
    (let ((arguments (copy-sequence (plist-get shell :login))))
      (list :executable executable
            :argv (cons executable arguments)
            :directory directory
            :shell (copy-tree shell)))))

(defun atelier-find-job-for-buffer (buffer-name)
  (cl-loop for workspace in atelier-workspaces
           for job = (cl-find buffer-name (plist-get workspace :jobs)
                              :key (lambda (item) (plist-get item :buffer))
                              :test #'equal)
           when job return (list workspace job)))

(defun atelier-ssh-aliases ()
  (let ((files (list (myconfig-ssh-config-file))) aliases)
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
                                                       (file-name-directory (myconfig-ssh-config-file))))
                                    files))))))))
    (delete-dups (nreverse aliases))))

(defun atelier-read-workspace-target ()
   (let* ((destinations (delete-dups
                         (append '("local")
                                 (when (myconfig-windows-host-p) '("WSL"))
                                 '("Enter SSH destination")
                                 (atelier-ssh-aliases)
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
                                   (probe (myconfig-windows-workspace-directory probe))
                                   (remote-prefix remote-prefix)
                                     (t (myconfig-home-directory))))))
    (unless (file-directory-p directory)
      (when (yes-or-no-p (format "Create %s? " directory)) (make-directory directory t)))
    (unless (file-directory-p directory) (user-error "Directory does not exist: %s" directory))
    (when remote-prefix
      (cl-pushnew destination atelier-remembered-ssh-destinations :test #'equal))
    (list destination
           (cond ((eq platform 'wsl) (file-remote-p directory 'localname))
                 (probe (myconfig-windows-remote-path probe directory))
                 (remote-prefix (file-remote-p directory 'localname))
                (t directory))
          platform mount-root)))

(defun atelier-edit-workspace ()
  (interactive)
  (let* ((workspace (atelier-current-workspace))
         (old-directory (atelier-workspace-directory workspace)))
    (unless workspace (user-error "No workspace is open"))
    (pcase-let ((`(,destination ,path ,platform ,mount-root) (atelier-read-workspace-target)))
      (when (myconfig-windows-workspace-p workspace)
        (myconfig-windows-unmount-unused workspace))
      (setf (plist-get workspace :destination) destination
            (plist-get workspace :path) path
            (plist-get workspace :platform) platform
            (plist-get workspace :mount-root) mount-root)
      (dolist (descriptor (plist-get workspace :buffers))
        (when (equal (plist-get descriptor :directory) old-directory)
          (setf (plist-get descriptor :directory) (atelier-workspace-directory workspace))))
      (atelier-restore-workspace workspace)
      (atelier-notify-change))))

(defun atelier-create-workspace ()
  (interactive)
  (pcase-let* ((`(,destination ,path ,platform ,mount-root) (atelier-read-workspace-target))
                (suggested-name
                 (myconfig-safe-name (file-name-nondirectory (directory-file-name path))))
                (suggestion (if (string-empty-p suggested-name) "home" suggested-name))
                (name (read-string "Workspace name: " suggestion)))
    (when (string-empty-p name) (user-error "Workspace name cannot be empty"))
    (when (atelier-workspace-get name) (user-error "Workspace already exists: %s" name))
    (let ((workspace (list :name name :destination destination :path path
                           :platform platform :mount-root mount-root
                            :created (float-time) :live t :state nil :buffers nil
                            :owned-buffers nil
                           :jobs nil :agent-buffer nil :agent-directory nil)))
      (atelier-capture-current-workspace)
      (setq atelier-workspaces (append atelier-workspaces (list workspace))
            atelier-current-workspace-name name)
      (atelier-restore-workspace workspace)
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
              (list :name name
                    :destination (or (file-remote-p root 'host) "local")
                    :path (if remote (file-remote-p root 'localname) root)
                    :platform (and remote 'posix)
                    :mount-root nil :created (float-time) :live t
                     :state nil :buffers nil :owned-buffers nil :jobs nil
                    :agent-buffer nil :agent-directory nil)))
        (atelier-capture-current-workspace)
        (setq atelier-workspaces (append atelier-workspaces (list workspace))
              atelier-current-workspace-name name)
        (atelier-restore-workspace workspace)
        (atelier-notify-change)))))

(defvar-keymap atelier-navigator-mode-map
  :parent special-mode-map
  "j" #'atelier-navigator-next
  "k" #'atelier-navigator-previous
  "<down>" #'atelier-navigator-next
  "<up>" #'atelier-navigator-previous
  "RET" #'atelier-navigator-open
  "a" #'atelier-navigator-attach
  "d" #'atelier-navigator-detach
  "f" #'isearch-forward
  "F" #'isearch-forward
  "x" #'atelier-navigator-close
  "X" #'atelier-navigator-close
  "r" #'atelier-navigator-rename
  "R" #'atelier-navigator-rename
  "q" #'atelier-navigator-quit)

(define-derived-mode atelier-navigator-mode special-mode "Atelier"
  (setq-local header-line-format
              '(:eval (if atelier-navigator-attach-source
                          " Select a workspace or buffer with Enter   q cancel"
                        " j/k move   Enter focus   a attach   d detach   x close   r rename   q quit")))
  (setq-local hl-line-face 'atelier-navigator-current
              cursor-type 'box
              truncate-lines t
              display-line-numbers-type 'relative)
  (hl-line-mode 1)
  (display-line-numbers-mode 1))

(defvar-keymap atelier-detached-view-mode-map
  "q" #'atelier-navigator-quit)

(define-minor-mode atelier-detached-view-mode
  "Mark a buffer as temporarily displayed outside the saved workspace layout."
  :lighter nil
  :keymap atelier-detached-view-mode-map)

(defun atelier-navigator-click (event)
  (interactive "e")
  (let* ((start (event-start event))
         (window (posn-window start))
         (position (posn-point start)))
    (when (and (window-live-p window) (integer-or-marker-p position))
      (select-window window)
      (goto-char position)
      (atelier-navigator-open))))

(defun atelier-navigator-insert (text target &optional face)
  (let* ((map (make-sparse-keymap))
         (newline (string-suffix-p "\n" text))
         (label (if newline (substring text 0 -1) text)))
    (define-key map [mouse-1] #'atelier-navigator-click)
    (define-key map [mouse-2] #'atelier-navigator-click)
    (insert (propertize label 'atelier-navigator-target target
                        'face face 'mouse-face 'atelier-navigator-hover
                        'follow-link t 'keymap map 'rear-nonsticky t))
    (when newline (insert "\n"))))

(defun atelier-navigator-target ()
  (get-text-property (point) 'atelier-navigator-target))

(defun atelier-navigator-positions ()
  (let ((position (point-min)) positions)
    (while (< position (point-max))
      (if (get-text-property position 'atelier-navigator-target)
          (progn
            (push position positions)
            (setq position (or (next-single-property-change
                                position 'atelier-navigator-target nil (point-max))
                               (point-max))))
        (setq position (or (next-single-property-change
                            position 'atelier-navigator-target nil (point-max))
                           (point-max)))))
    (nreverse positions)))

(defun atelier-navigator-move (delta)
  (let* ((positions (atelier-navigator-positions))
         (next (cl-position-if (lambda (position) (> position (point))) positions))
         (current (max 0 (1- (or next (length positions)))))
         (target (and positions (nth (mod (+ current delta) (length positions)) positions))))
    (when target (goto-char target))))

(defun atelier-navigator-next (&optional count linewise)
  (interactive (list (prefix-numeric-value current-prefix-arg)
                     current-prefix-arg))
  (if linewise
      (forward-line (or count 1))
    (atelier-navigator-move 1)))

(defun atelier-navigator-previous (&optional count linewise)
  (interactive (list (prefix-numeric-value current-prefix-arg)
                     current-prefix-arg))
  (if linewise
      (forward-line (- (or count 1)))
    (atelier-navigator-move -1)))

(defun atelier-workspace-assigned-buffer-names (workspace)
  (delete-dups
   (delq nil
          (append (mapcar (lambda (descriptor) (plist-get descriptor :name))
                          (plist-get workspace :buffers))
                  (mapcar (lambda (descriptor) (plist-get descriptor :name))
                          (atelier-workspace-owned-descriptors workspace))
                  (mapcar (lambda (job) (plist-get job :buffer))
                         (plist-get workspace :jobs))
                  (list (plist-get workspace :agent-buffer))))))

(defun atelier-workspace-owned-descriptors (workspace)
  (let (names descriptors)
    (dolist (descriptor
             (append (atelier-live-owned-buffer-descriptors workspace)
                     (plist-get workspace :owned-buffers)))
      (let ((name (plist-get descriptor :name)))
        (unless (member name names)
          (push name names)
          (push descriptor descriptors))))
    (nreverse descriptors)))

(defun atelier-assigned-buffer-names ()
  (let (names)
    (dolist (workspace atelier-workspaces)
      (setq names (append (atelier-workspace-assigned-buffer-names workspace) names)))
    (delete-dups names)))

(defun atelier-buffer-list ()
  (let ((current-workspace (atelier-current-workspace)))
    (cl-remove-if
     (lambda (buffer)
       (or (when-let* ((owner (atelier-find-job-for-buffer (buffer-name buffer))))
             (not (eq (car owner) current-workspace)))
           (when-let* ((owner-name (buffer-local-value 'atelier-buffer-workspace buffer)))
             (not (equal owner-name atelier-current-workspace-name)))))
     (buffer-list))))

(defun atelier-navigator-buffers ()
  (let ((assigned-names (atelier-assigned-buffer-names))
        (internal (list atelier-navigator-buffer atelier-choice-buffer
                        "*Messages*" "*atelier-log*")))
    (cl-remove-if
     (lambda (buffer)
       (let ((name (buffer-name buffer)))
          (or (string-prefix-p " " name)
               (member name assigned-names)
              (member name internal))))
      (buffer-list))))

(defun atelier-navigator-buffer-name (name)
  (let* ((buffer (get-buffer name))
         (title (and buffer
                     (local-variable-p 'ghostel-title buffer)
                     (buffer-local-value 'ghostel-title buffer))))
    (if (and (stringp title) (not (string-empty-p (string-trim title))))
        (string-trim (replace-regexp-in-string "[[:cntrl:]]+" " " title))
      name)))

(defun atelier-new-scratch-buffer ()
  (interactive)
  (let ((buffer (generate-new-buffer "*scratch*")))
    (with-current-buffer buffer
      (funcall initial-major-mode)
      (setq-local atelier-buffer-global t))
    (atelier-navigator-detach-buffer buffer t)))

(defun atelier-cleanup-candidate-buffers ()
  (delete-dups
   (append
    (cl-remove-if-not
     (lambda (buffer) (string-prefix-p "*scratch*" (buffer-name buffer)))
     (buffer-list))
    (atelier-navigator-buffers))))

(defun atelier-clear-scratch-and-unowned-buffers (&optional confirmed)
  (interactive)
  (let ((buffers (atelier-cleanup-candidate-buffers))
        (killed 0))
    (if (null buffers)
        (message "No scratch or unowned buffers to clear")
      (unless (or confirmed
                  (y-or-n-p (format "Kill %d scratch or unowned buffer%s? "
                                    (length buffers)
                                    (if (= (length buffers) 1) "" "s"))))
        (user-error "Cancelled"))
      (dolist (buffer buffers)
        (when (and (buffer-live-p buffer) (kill-buffer buffer))
          (setq killed (1+ killed))))
      (atelier-notify-change)
      (message "Cleared %d scratch or unowned buffer%s"
               killed (if (= killed 1) "" "s")))))

(defun atelier-render-navigator ()
  (let ((buffer (get-buffer-create atelier-navigator-buffer))
        workspace-roots first-item)
    (with-current-buffer buffer
      (unless (derived-mode-p 'atelier-navigator-mode)
        (atelier-navigator-mode))
      (let ((inhibit-read-only t))
        (erase-buffer)
        (dolist (workspace atelier-workspaces)
          (let* ((workspace-name (plist-get workspace :name))
                 (active (equal workspace-name atelier-current-workspace-name))
                 (live (plist-get workspace :live)))
            (unless first-item (setq first-item (point)))
            (atelier-navigator-insert
             (format "%s%s/" (cond (active "> ") (live "+ ") (t "~ ")) workspace-name)
             (list 'workspace workspace-name)
             (cond (active 'atelier-navigator-active)
                   (live 'atelier-navigator-live)
                   (t 'atelier-navigator-saved)))
            (insert "\n")
            (when-let* ((root (atelier-workspace-project-root workspace)))
              (push root workspace-roots))
            (let* ((descriptors (plist-get workspace :buffers))
                   (multiple-splits (> (length descriptors) 1))
                   descriptor-names)
              (cl-loop for descriptor in descriptors
                        for index from 0
                        do
                        (push (plist-get descriptor :name) descriptor-names)
                        (atelier-navigator-insert
                         (if multiple-splits
                              (format "  %sSplit %d: %s\n"
                                      (if (and active (plist-get descriptor :selected)) "> " "- ")
                                      (1+ index)
                                      (atelier-navigator-buffer-name
                                       (plist-get descriptor :name)))
                            (format "  %s%s\n"
                                    (if (and active (plist-get descriptor :selected)) "> " ". ")
                                    (atelier-navigator-buffer-name
                                     (plist-get descriptor :name))))
                         (list 'workspace-buffer workspace-name index
                               (plist-get descriptor :name))))
              (dolist (descriptor (atelier-workspace-owned-descriptors workspace))
                (let ((name (plist-get descriptor :name)))
                  (unless (member name descriptor-names)
                    (atelier-navigator-insert
                      (format "  . %s\n" (atelier-navigator-buffer-name name))
                     (list 'workspace-owned-buffer workspace-name name)))))
              (dolist (job (plist-get workspace :jobs))
                (let ((name (plist-get job :buffer)))
                  (unless (member name descriptor-names)
                    (atelier-navigator-insert
                      (format "  . %s\n" (atelier-navigator-buffer-name name))
                     (list 'workspace-job workspace-name name))))))))
        (dolist (root
                 (cl-remove-if
                  (lambda (item) (member (myconfig-normalize-directory item) workspace-roots))
                  (atelier-known-project-roots)))
          (unless first-item (setq first-item (point)))
          (atelier-navigator-insert
           (format "+ %s/  %s\n"
                   (file-name-nondirectory (directory-file-name root))
                   (abbreviate-file-name root))
           (list 'project root) 'font-lock-keyword-face))
        (atelier-navigator-insert "[+ New workspace]\n" '(new-workspace) 'success)
        (let ((buffers (atelier-navigator-buffers)))
          (insert "\nBuffers\n")
          (dolist (item buffers)
            (atelier-navigator-insert
              (format "  . %s\n" (atelier-navigator-buffer-name (buffer-name item)))
               (list 'buffer (buffer-name item))))
          (atelier-navigator-insert "[+ New scratch buffer]\n" '(new-scratch) 'success)
          (atelier-navigator-insert "[Clear scratch and unowned buffers]\n"
                                     '(clear-buffers) 'warning))
        (when (eq (char-before (point-max)) ?\n)
          (delete-region (1- (point-max)) (point-max)))
        (setq atelier-navigator-first-position
              (or first-item (car (atelier-navigator-positions)) (point-min)))
        (goto-char atelier-navigator-first-position)))
    buffer))

(defun atelier-cleanup-detached-buffer (frame)
  (let ((detached (alist-get frame atelier-detached-buffers nil nil #'eq)))
    (setq atelier-detached-buffers (assq-delete-all frame atelier-detached-buffers))
    (when detached
      (pcase-let ((`(,buffer ,header-local ,header ,decorated) detached))
        (when (and decorated (buffer-live-p buffer))
          (with-current-buffer buffer
            (atelier-detached-view-mode -1)
            (if header-local
                (setq-local header-line-format header)
              (kill-local-variable 'header-line-format))))))))

(defun atelier-navigator-quit ()
  (interactive)
  (setq atelier-navigator-attach-source nil)
  (let* ((frame (selected-frame))
         (configuration (alist-get frame atelier-navigator-window-configurations nil nil #'eq)))
    (setq atelier-navigator-window-configurations
          (assq-delete-all frame atelier-navigator-window-configurations))
    (atelier-cleanup-detached-buffer frame)
    (when configuration
      (set-window-configuration configuration)
      (atelier-clean-window-buffer-history))))

(defun atelier-navigator-detach-buffer (buffer &optional hidden)
  (let ((frame (selected-frame)))
    (atelier-cleanup-detached-buffer frame)
    (with-current-buffer buffer
      (setq atelier-detached-buffers
            (cons (list frame buffer (local-variable-p 'header-line-format)
                        header-line-format (not hidden))
                  (assq-delete-all frame atelier-detached-buffers)))
      (unless hidden
        (setq-local header-line-format " Detached buffer   q return   Space x close buffer")
        (atelier-detached-view-mode 1)))
    (switch-to-buffer buffer)))

(defun atelier-navigator ()
  (interactive)
  (let* ((frame (selected-frame))
         (existing (assq frame atelier-navigator-window-configurations))
         (window (cl-find-if (lambda (item) (not (window-parameter item 'window-side)))
                             (window-list frame 'no-minibuffer))))
    (unless existing
      (atelier-capture-current-workspace)
      (push (cons frame (current-window-configuration frame))
            atelier-navigator-window-configurations))
    (when window
      (delete-other-windows window)
      (select-window window)
      (switch-to-buffer (atelier-render-navigator))
      (goto-char atelier-navigator-first-position)
      (set-window-point window atelier-navigator-first-position))))

(defun atelier-focus-workspace-split (workspace-name index)
  (unless (equal workspace-name atelier-current-workspace-name)
    (atelier-switch-workspace workspace-name))
  (let* ((windows (cl-remove-if (lambda (window) (window-parameter window 'window-side))
                                (window-list nil 'no-minibuffer)))
         (window (nth index windows)))
    (unless (window-live-p window)
      (user-error "Split %d no longer exists" (1+ index)))
    (select-window window)
    window))

(defun atelier-workspace-buffer (workspace-name index name)
  (when-let* ((workspace (atelier-workspace-get workspace-name))
              (descriptor (nth index (plist-get workspace :buffers))))
    (unless (equal name (plist-get descriptor :name))
      (user-error "Split assignment changed for %s" name))
    (atelier-restore-buffer descriptor workspace)))

(defun atelier-workspace-owned-buffer (workspace name)
  (when-let* ((descriptor (cl-find name (plist-get workspace :owned-buffers)
                                    :key (lambda (item) (plist-get item :name))
                                    :test #'equal)))
    (atelier-restore-buffer descriptor workspace)))

(defun atelier-navigator-assign-buffer (buffer)
  (unless (buffer-live-p buffer) (user-error "Buffer no longer exists"))
  (atelier-navigator-quit)
  (atelier-assign-buffer-to-workspace buffer)
  (switch-to-buffer buffer)
  (when (fboundp 'myconfig-terminal-activate)
    (myconfig-terminal-activate buffer))
  (atelier-notify-change))

(defun atelier-navigator-target-buffer (target)
  (pcase target
    (`(workspace-buffer ,workspace-name ,index ,name)
     (atelier-workspace-buffer workspace-name index name))
    (`(workspace-owned-buffer ,workspace-name ,name)
     (let ((workspace (atelier-workspace-get workspace-name)))
       (or (get-buffer name)
           (atelier-workspace-owned-buffer workspace name))))
    (`(workspace-job ,_ ,name) (get-buffer name))
    (`(buffer ,name) (get-buffer name))))

(defun atelier-navigator-target-workspace (target buffer)
  (pcase target
    (`(workspace ,name) (atelier-workspace-get name))
    (`(workspace-buffer ,name . ,_) (atelier-workspace-get name))
    (`(workspace-owned-buffer ,name . ,_) (atelier-workspace-get name))
    (`(workspace-job ,name . ,_) (atelier-workspace-get name))
    (_ (or (atelier-workspace-get
            (and buffer (buffer-local-value 'atelier-buffer-workspace buffer)))
           (atelier-current-workspace)))))

(defun atelier-navigator-attach ()
  (interactive)
  (let ((target (atelier-navigator-target)))
    (unless (memq (car-safe target)
                  '(buffer workspace-buffer workspace-owned-buffer workspace-job))
      (user-error "Select a buffer to attach"))
    (setq atelier-navigator-attach-source target)
    (force-mode-line-update t)
    (message "Select a workspace or another buffer with Enter")))

(defun atelier-navigator-finish-attach (target)
  (let* ((source-target atelier-navigator-attach-source)
         (source (atelier-navigator-target-buffer source-target))
         (target-buffer (atelier-navigator-target-buffer target))
         (workspace (atelier-navigator-target-workspace target target-buffer)))
    (unless (or (eq (car-safe target) 'workspace) target-buffer)
      (user-error "Select a workspace or buffer"))
    (unless (buffer-live-p source) (user-error "Source buffer no longer exists"))
    (when (eq source target-buffer) (user-error "Choose another buffer"))
    (unless workspace (user-error "No target workspace"))
    (setq atelier-navigator-attach-source nil)
    (atelier-navigator-quit)
    (unless (equal (plist-get workspace :name) atelier-current-workspace-name)
      (atelier-switch-workspace (plist-get workspace :name)))
    (let* ((target-window (and target-buffer (get-buffer-window target-buffer)))
           (source-window (get-buffer-window source))
           (left (or target-window (selected-window))))
      (when (and (eq (car-safe target) 'workspace) (eq source-window left))
        (if-let* ((other (cl-find-if (lambda (window) (not (eq window source-window)))
                                     (window-list nil 'no-minibuffer))))
            (setq left other)
          (set-window-buffer left
                             (atelier-new-dired-buffer
                              (atelier-workspace-directory workspace)))))
      (when (and source-window (not (eq source-window left))
                 (not (one-window-p)))
        (delete-window source-window))
      (when target-buffer (set-window-buffer left target-buffer))
      (atelier-assign-buffer-to-workspace (window-buffer left) workspace)
      (when-let* ((owner (atelier-find-job-for-buffer (buffer-name source)))
                  (old-workspace (car owner))
                  (job (nth 1 owner))
                  ((not (eq old-workspace workspace))))
        (setf (plist-get old-workspace :jobs)
              (delq job (plist-get old-workspace :jobs)))
        (when (equal (plist-get old-workspace :agent-buffer) (buffer-name source))
          (setf (plist-get old-workspace :agent-buffer) nil
                (plist-get workspace :agent-buffer) (buffer-name source)))
        (push job (plist-get workspace :jobs)))
      (atelier-assign-buffer-to-workspace source workspace)
      (let ((right (split-window left nil 'right)))
        (set-window-buffer right source)
        (select-window right)))
    (atelier-capture-current-workspace)
    (atelier-notify-change)
    (atelier-navigator)))

(defun atelier-navigator-detach ()
  (interactive)
  (pcase (atelier-navigator-target)
    (`(workspace-buffer ,workspace-name ,index ,name)
     (setq atelier-navigator-attach-source nil)
     (atelier-navigator-quit)
     (unless (equal workspace-name atelier-current-workspace-name)
       (atelier-switch-workspace workspace-name))
     (let ((window (atelier-focus-workspace-split workspace-name index))
           (buffer (get-buffer name)))
       (unless (buffer-live-p buffer) (user-error "Buffer no longer exists: %s" name))
       (atelier-assign-buffer-to-workspace buffer (atelier-workspace-get workspace-name))
       (if (one-window-p)
           (set-window-buffer window
                              (atelier-new-dired-buffer
                               (atelier-workspace-directory)))
         (delete-window window)))
     (atelier-capture-current-workspace)
     (atelier-notify-change)
     (atelier-navigator))
    (`(workspace-owned-buffer ,workspace-name ,name)
     (let ((workspace (atelier-workspace-get workspace-name))
           (buffer (get-buffer name)))
       (setf (plist-get workspace :owned-buffers)
             (cl-remove name (plist-get workspace :owned-buffers)
                        :key (lambda (item) (plist-get item :name)) :test #'equal))
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (kill-local-variable 'atelier-buffer-workspace)))
       (atelier-notify-change)
       (atelier-render-navigator)))
    (_ (user-error "Select a split or workspace-owned buffer"))))

(defun atelier-navigator-open ()
  (interactive)
  (let ((target (atelier-navigator-target)))
    (if atelier-navigator-attach-source
        (atelier-navigator-finish-attach target)
      (pcase target
     ('nil (user-error "No item on this line"))
     (`(new-workspace) (atelier-navigator-quit) (atelier-create-workspace))
     (`(new-scratch) (atelier-new-scratch-buffer))
     (`(clear-buffers)
       (atelier-clear-scratch-and-unowned-buffers)
       (atelier-navigator-quit)
       (atelier-capture-current-workspace)
       (atelier-navigator))
     (`(workspace ,name) (atelier-navigator-quit) (atelier-switch-workspace name))
     (`(split ,workspace-name ,index)
      (atelier-navigator-quit)
      (atelier-focus-workspace-split workspace-name index))
     (`(workspace-buffer ,workspace-name ,index ,name)
       (atelier-navigator-quit)
       (let ((buffer (atelier-workspace-buffer workspace-name index name))
             (window (atelier-focus-workspace-split workspace-name index)))
          (unless (buffer-live-p buffer) (user-error "Buffer no longer exists: %s" name))
          (set-window-buffer window buffer)
          (when (fboundp 'myconfig-terminal-activate)
            (myconfig-terminal-activate buffer))))
      (`(workspace-job ,workspace-name ,name)
       (atelier-navigator-quit)
       (unless (equal workspace-name atelier-current-workspace-name)
         (atelier-switch-workspace workspace-name))
       (if-let* ((buffer (get-buffer name)))
           (progn
             (switch-to-buffer buffer)
             (when (fboundp 'myconfig-terminal-activate)
               (myconfig-terminal-activate buffer))
             (atelier-notify-change))
         (user-error "Job buffer no longer exists: %s" name)))
      (`(workspace-owned-buffer ,workspace-name ,name)
       (atelier-navigator-quit)
       (unless (equal workspace-name atelier-current-workspace-name)
         (atelier-switch-workspace workspace-name))
       (let* ((workspace (atelier-workspace-get workspace-name))
              (buffer (or (get-buffer name)
                          (atelier-workspace-owned-buffer workspace name))))
          (unless (buffer-live-p buffer)
            (user-error "Workspace buffer no longer exists: %s" name))
          (switch-to-buffer buffer)
          (when (fboundp 'myconfig-terminal-activate)
            (myconfig-terminal-activate buffer))
          (atelier-notify-change)))
     (`(project ,root) (atelier-navigator-quit) (atelier-open-project-workspace root))
      (`(buffer ,name)
       (if-let* ((buffer (get-buffer name)))
           (atelier-navigator-assign-buffer buffer)
         (user-error "Buffer no longer exists: %s" name)))))))

(defun atelier-close-current-view ()
  (interactive)
  (let* ((frame (selected-frame))
         (detached (alist-get frame atelier-detached-buffers nil nil #'eq))
         (buffer (current-buffer))
         (windows (cl-remove-if (lambda (window) (window-parameter window 'window-side))
                                 (window-list frame 'no-minibuffer))))
    (when detached
      (atelier-navigator-quit))
    (atelier-close-buffer (buffer-name buffer) (unless detached (selected-window)))
    (when (and (not detached) (= (length windows) 1))
      (switch-to-buffer (atelier-new-dired-buffer (atelier-workspace-directory)))
      (set-window-prev-buffers (selected-window) nil)
      (set-window-next-buffers (selected-window) nil))
     (atelier-notify-change)))

(defun atelier-kill-buffer-saved (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (when (and buffer-file-name (buffer-modified-p))
        (save-buffer)))
    (let ((kill-buffer-query-functions nil))
      (kill-buffer buffer))))

(defun atelier-close-workspace-job (workspace name)
  (let ((job (cl-find name (plist-get workspace :jobs)
                      :key (lambda (item) (plist-get item :buffer))
                      :test #'equal)))
    (unless job (user-error "Job no longer exists: %s" name))
    (setf (plist-get workspace :jobs) (delq job (plist-get workspace :jobs)))
    (when (equal (plist-get workspace :agent-buffer) name)
      (setf (plist-get workspace :agent-buffer) nil))
    (when-let* ((buffer (get-buffer name)))
      (when-let* ((process (get-buffer-process buffer)))
        (set-process-query-on-exit-flag process nil)
        (when (process-live-p process)
          (delete-process process)))
      (atelier-kill-buffer-saved buffer))
    (atelier-notify-change)))

(defun atelier-close-buffer (name &optional window)
  (let ((window (or window (get-buffer-window name (selected-frame)))))
    (if-let* ((owner (atelier-find-job-for-buffer name)))
        (atelier-close-workspace-job (car owner) name)
      (when-let* ((buffer (get-buffer name)))
        (when-let* ((process (get-buffer-process buffer)))
          (set-process-query-on-exit-flag process nil)
          (when (process-live-p process)
            (delete-process process)))
         (atelier-kill-buffer-saved buffer)))
    (when (and (window-live-p window)
               (> (length (cl-remove-if
                           (lambda (item) (window-parameter item 'window-side))
                           (window-list (window-frame window) 'no-minibuffer)))
                  1))
      (delete-window window))))

(defun atelier-navigator-close ()
  (interactive)
  (let ((target (atelier-navigator-target)))
    (unless target (user-error "No item on this line"))
    (unless (memq (car target) '(workspace buffer workspace-buffer workspace-owned-buffer workspace-job project))
      (user-error "This item cannot be closed"))
    (unless (y-or-n-p (format "%s? "
                              (pcase (car target)
                                  ('workspace "Close and remove this workspace")
                                  ('buffer "Kill this buffer")
                                  ('workspace-buffer "Kill this buffer")
                                  ('workspace-owned-buffer "Kill this buffer")
                                  ('workspace-job "Stop and close this job")
                                ('project "Forget this project")
                                (_ "This item cannot be closed"))))
      (user-error "Cancelled"))
    (atelier-navigator-quit)
    (pcase target
      (`(workspace ,name)
       (atelier-switch-workspace name)
       (atelier-delete-workspace t))
      (`(buffer ,name)
       (atelier-close-buffer name))
      (`(workspace-buffer ,_ ,_ ,name)
       (atelier-close-buffer name))
      (`(workspace-owned-buffer ,_ ,name)
       (atelier-close-buffer name))
      (`(workspace-job ,workspace-name ,name)
       (if (atelier-workspace-get workspace-name)
           (atelier-close-buffer name)
         (user-error "Workspace no longer exists: %s" workspace-name)))
      (`(project ,root) (project-forget-project root))
      (_ (user-error "This item cannot be closed")))
    (atelier-navigator)))

(defun atelier-navigator-rename ()
  (interactive)
  (let ((target (atelier-navigator-target)))
    (unless target (user-error "No item on this line"))
    (atelier-navigator-quit)
    (pcase target
      (`(workspace ,name)
       (atelier-switch-workspace name)
       (atelier-rename-workspace))
      (`(buffer ,name)
       (if-let* ((buffer (get-buffer name)))
           (with-current-buffer buffer
             (let ((new-name (rename-buffer (read-string "New buffer name: " name) t)))
               (when-let* ((workspace (atelier-workspace-get atelier-buffer-workspace))
                           (descriptor (cl-find name (plist-get workspace :owned-buffers)
                                                :key (lambda (item) (plist-get item :name))
                                                :test #'equal)))
                 (setf (plist-get descriptor :name) new-name))))
         (user-error "Buffer no longer exists: %s" name)))
      (`(workspace-buffer ,_ ,_ ,name)
       (if-let* ((buffer (get-buffer name)))
           (with-current-buffer buffer
             (rename-buffer (read-string "New buffer name: " name) t))
          (user-error "Buffer no longer exists: %s" name)))
      (`(workspace-owned-buffer ,_ ,name)
       (if-let* ((buffer (get-buffer name)))
           (with-current-buffer buffer
             (let ((new-name (rename-buffer (read-string "New buffer name: " name) t)))
               (when-let* ((workspace (atelier-workspace-get atelier-buffer-workspace))
                           (descriptor (cl-find name (plist-get workspace :owned-buffers)
                                                :key (lambda (item) (plist-get item :name))
                                                :test #'equal)))
                 (setf (plist-get descriptor :name) new-name))))
         (user-error "Buffer no longer exists: %s" name)))
      (_ (user-error "This item cannot be renamed")))
    (atelier-notify-change)
    (atelier-navigator)))

(defun atelier-switch-workspace (name)
  (interactive (list (completing-read "Workspace: " (mapcar (lambda (w) (plist-get w :name)) atelier-workspaces) nil t)))
  (let* ((workspace (atelier-workspace-get name))
         (was-live (and workspace (plist-get workspace :live))))
    (unless workspace (user-error "Unknown workspace: %s" name))
    (atelier-capture-current-workspace)
    (setq atelier-current-workspace-name name)
    (setf (plist-get workspace :live) t)
    (when (and (not was-live) (fboundp 'myconfig-restart-saved-jobs))
      (myconfig-restart-saved-jobs workspace))
    (atelier-restore-workspace workspace)
    (atelier-notify-change)))

(defun atelier-rename-workspace ()
  (interactive)
  (let* ((workspace (atelier-current-workspace))
         (old (plist-get workspace :name))
         (new (read-string "New workspace name: " old)))
    (when (string-empty-p new) (user-error "Workspace name cannot be empty"))
    (when (and (not (equal old new)) (atelier-workspace-get new))
      (user-error "Workspace already exists: %s" new))
    (setf (plist-get workspace :name) new)
    (dolist (buffer (buffer-list))
      (when (equal (buffer-local-value 'atelier-buffer-workspace buffer) old)
        (with-current-buffer buffer
          (setq-local atelier-buffer-workspace new))))
    (setq atelier-current-workspace-name new)
    (atelier-notify-change)))

(defun atelier-close-workspace (&optional confirmed)
  (interactive)
  (let ((workspace (atelier-current-workspace)))
    (unless (or confirmed
                (y-or-n-p (format "Close workspace %s and stop its jobs? "
                                  (plist-get workspace :name))))
      (user-error "Cancelled"))
    (atelier-capture-current-workspace)
    (atelier-workspace-stop-jobs workspace)
    (setf (plist-get workspace :live) nil)
    (when (myconfig-windows-workspace-p workspace)
      (myconfig-windows-unmount-unused workspace))
    (let ((next (or (cl-find-if (lambda (item) (and (not (eq item workspace)) (plist-get item :live))) atelier-workspaces)
                    (cl-find-if (lambda (item) (not (eq item workspace))) atelier-workspaces))))
      (if next
          (atelier-switch-workspace (plist-get next :name))
        (setq atelier-current-workspace-name nil)
        (delete-other-windows)
        (switch-to-buffer (get-buffer-create "*scratch*"))
        (atelier-notify-change)))))

(defun atelier-delete-workspace (&optional confirmed)
  (interactive)
  (let ((workspace (atelier-current-workspace)))
    (unless (or confirmed
                (yes-or-no-p (format "Delete workspace definition %s? "
                                     (plist-get workspace :name))))
      (user-error "Cancelled"))
    (atelier-workspace-stop-jobs workspace t)
    (dolist (buffer (buffer-list))
      (when (equal (buffer-local-value 'atelier-buffer-workspace buffer)
                   (plist-get workspace :name))
        (with-current-buffer buffer
          (kill-local-variable 'atelier-buffer-workspace))))
    (when (myconfig-windows-workspace-p workspace)
      (myconfig-windows-unmount workspace))
    (setq atelier-workspaces (delq workspace atelier-workspaces))
    (if atelier-workspaces
        (atelier-switch-workspace (plist-get (car atelier-workspaces) :name))
      (setq atelier-current-workspace-name nil))
    (atelier-notify-change)))

(defun atelier-split-right ()
  (interactive)
  (let* ((directory default-directory)
         (window (split-window-right)))
    (condition-case error
        (let ((buffer (atelier-new-dired-buffer directory)))
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
  (let* ((in-dired (derived-mode-p 'dired-mode))
         (directory (unless (derived-mode-p 'atelier-navigator-mode)
                      (if (atelier-buffer-orphaned-p)
                          (myconfig-home-directory)
                        default-directory)))
         (existing (and (not in-dired)
          (atelier-find-workspace-buffer
           (lambda (buffer _workspace)
             (with-current-buffer buffer
               (derived-mode-p 'dired-mode)))))))
    (when (assq (selected-frame) atelier-navigator-window-configurations)
      (atelier-navigator-quit))
    (setq directory (if (and directory (file-directory-p directory))
                        directory
                       (if (atelier-buffer-orphaned-p)
                           (myconfig-home-directory)
                         (if (file-directory-p default-directory)
                             default-directory
                         (atelier-workspace-directory)))))
    (cond
     (in-dired
      (switch-to-buffer (atelier-new-dired-buffer directory t)))
     (existing
      (switch-to-buffer existing))
      (t
       (switch-to-buffer (atelier-new-dired-buffer directory))))))

(defun atelier-dired-open ()
  (interactive)
  (if atelier-directory-chooser-mode
      (atelier-directory-chooser-enter)
    (if (file-directory-p (dired-get-file-for-visit))
        (dired-find-alternate-file)
      (dired-find-file))))

(defvar-keymap atelier-directory-chooser-mode-map
  "RET" #'atelier-directory-chooser-enter
  "<return>" #'atelier-directory-chooser-enter
  "q" #'abort-recursive-edit
  "<mouse-2>" #'atelier-directory-chooser-mouse-enter)

(define-minor-mode atelier-directory-chooser-mode
  "Choose a workspace directory from a full Dired buffer."
  :lighter " Choose directory"
  :keymap atelier-directory-chooser-mode-map)

(defun atelier-directory-chooser-select (&optional _event)
  (interactive)
  (setq atelier-directory-choice-result default-directory)
  (exit-recursive-edit))

(defun atelier-directory-chooser-insert ()
  (when atelier-directory-chooser-mode
    (let ((inhibit-read-only t)
          (map (make-sparse-keymap)))
      (goto-char (point-min))
       (unless (text-property-search-forward 'atelier-directory-choice t t)
        (goto-char (point-min))
        (if (dired-goto-file (directory-file-name default-directory))
            (beginning-of-line)
          (forward-line 2))
        (define-key map [mouse-1] #'atelier-directory-chooser-select)
        (define-key map [mouse-2] #'atelier-directory-chooser-select)
        (insert (propertize "[Select this directory]"
                            'atelier-directory-choice t
                            'face 'success
                            'mouse-face 'atelier-navigator-hover
                            'follow-link t
                            'keymap map
                            'rear-nonsticky t)
                "\n")))
    (goto-char (point-min))
    (text-property-search-forward 'atelier-directory-choice t t)
    (beginning-of-line)))

(defun atelier-directory-chooser-setup ()
  (unless atelier-directory-chooser-mode
    (setq-local atelier-directory-chooser-header-was-local
                (local-variable-p 'header-line-format)
                atelier-directory-chooser-original-header header-line-format
                atelier-directory-chooser-original-modified (buffer-modified-p))
    (cl-pushnew (current-buffer) atelier-directory-chooser-buffers))
  (atelier-directory-chooser-mode 1)
  (when (bound-and-true-p evil-local-mode)
    (evil-normalize-keymaps))
  (setq-local header-line-format
              " Enter open/select   ^ parent   + new directory   q cancel")
  (add-hook 'dired-after-readin-hook #'atelier-directory-chooser-insert nil t)
  (atelier-directory-chooser-insert)
  (set-buffer-modified-p atelier-directory-chooser-original-modified))

(defun atelier-directory-chooser-cleanup (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (let ((inhibit-read-only t))
        (goto-char (point-min))
        (while (text-property-search-forward 'atelier-directory-choice t t)
          (delete-region (line-beginning-position)
                         (min (point-max) (1+ (line-end-position))))))
      (remove-hook 'dired-after-readin-hook #'atelier-directory-chooser-insert t)
      (atelier-directory-chooser-mode -1)
      (if atelier-directory-chooser-header-was-local
          (setq-local header-line-format atelier-directory-chooser-original-header)
        (kill-local-variable 'header-line-format))
      (set-buffer-modified-p atelier-directory-chooser-original-modified))))

(defun atelier-directory-chooser-enter ()
  (interactive)
  (let ((path (dired-get-filename nil t)))
    (cond
     ((or (get-text-property (line-beginning-position) 'atelier-directory-choice)
          (null path))
      (atelier-directory-chooser-select))
     ((file-directory-p path)
      (switch-to-buffer (atelier-new-dired-buffer path))
      (atelier-directory-chooser-setup))
     (t
      (user-error "Choose a directory; this is a file: %s" path)))))

(defun atelier-directory-chooser-mouse-enter (event)
  (interactive "e")
  (mouse-set-point event)
  (atelier-directory-chooser-enter))

(defun atelier-read-directory-with-dired (directory)
  (let ((atelier-directory-choice-result nil)
         (atelier-directory-chooser-active t)
        (atelier-directory-chooser-buffers nil)
        (existing-buffers (buffer-list)))
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer (atelier-new-dired-buffer directory))
          (atelier-directory-chooser-setup)
          (recursive-edit))
      (mapc #'atelier-directory-chooser-cleanup
            atelier-directory-chooser-buffers)
      (dolist (buffer atelier-directory-chooser-buffers)
        (when (and (buffer-live-p buffer) (not (memq buffer existing-buffers)))
          (kill-buffer buffer))))
    atelier-directory-choice-result))

(defvar-keymap atelier-choice-mode-map
  :parent special-mode-map
  "j" #'atelier-choice-next
  "k" #'atelier-choice-previous
  "<down>" #'atelier-choice-next
  "<up>" #'atelier-choice-previous
  "RET" #'atelier-choice-select
  "q" #'abort-recursive-edit)

(define-derived-mode atelier-choice-mode special-mode "Atelier choice"
  (setq-local header-line-format " j/k move   Enter select   q cancel"
              hl-line-face 'atelier-navigator-current
              cursor-type 'box)
  (hl-line-mode 1)
  (display-line-numbers-mode -1))

(defun atelier-choice-positions ()
  (let ((position (point-min)) positions)
    (while (< position (point-max))
      (when (get-text-property position 'atelier-choice-value)
        (push position positions))
      (setq position (or (next-single-property-change
                          position 'atelier-choice-value nil (point-max))
                         (point-max))))
    (nreverse positions)))

(defun atelier-choice-move (delta)
  (let* ((positions (atelier-choice-positions))
         (next (cl-position-if (lambda (position) (> position (point))) positions))
         (current (max 0 (1- (or next (length positions))))))
    (when positions
      (goto-char (nth (mod (+ current delta) (length positions)) positions)))))

(defun atelier-choice-next ()
  (interactive)
  (atelier-choice-move 1))

(defun atelier-choice-previous ()
  (interactive)
  (atelier-choice-move -1))

(defun atelier-choice-select (&optional event)
  (interactive (list last-input-event))
  (when (mouse-event-p event)
    (mouse-set-point event))
  (if-let* ((value (get-text-property (line-beginning-position) 'atelier-choice-value)))
      (progn
        (setq atelier-choice-result value)
        (exit-recursive-edit))
    (user-error "No choice on this line")))

(defun atelier-read-buffer-choice (title choices)
  (let ((atelier-choice-result nil)
        (buffer (get-buffer-create atelier-choice-buffer)))
    (save-window-excursion
      (switch-to-buffer buffer)
      (atelier-choice-mode)
      (let ((inhibit-read-only t)
            (map (make-sparse-keymap)))
        (erase-buffer)
        (insert title "\n\n")
        (define-key map [mouse-1] #'atelier-choice-select)
        (define-key map [mouse-2] #'atelier-choice-select)
        (dolist (choice choices)
          (insert (propertize (format "%s" choice)
                              'atelier-choice-value choice
                              'mouse-face 'atelier-navigator-hover
                              'follow-link t
                              'keymap map
                              'rear-nonsticky t)
                  "\n"))
        (delete-region (1- (point-max)) (point-max))
        (set-buffer-modified-p nil)
        (goto-char (car (atelier-choice-positions))))
      (recursive-edit))
    atelier-choice-result))

(defun atelier-split-below ()
  (interactive)
  (let* ((directory default-directory)
         (window (split-window-below)))
    (condition-case error
        (let ((buffer (atelier-new-dired-buffer directory)))
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
    (when context-action (define-key map [mouse-3] context-action))
    (propertize label 'face face 'mouse-face 'atelier-navigator-hover 'help-echo help
                'follow-link t 'keymap map)))

(defun atelier-create-default ()
  (let* ((root (myconfig-project-root))
         (name (or (myconfig-safe-name (file-name-nondirectory (directory-file-name root))) "home")))
    (setq atelier-workspaces
           (list (list :name name :destination "local" :path root :platform 'local
                       :mount-root nil :created (float-time)
                        :live t :state nil :buffers nil :owned-buffers nil :jobs nil
                       :agent-buffer nil :agent-directory nil))
          atelier-current-workspace-name name)))

(defun atelier-capture-closing-frame (frame)
  (when (and (frame-live-p frame) (display-graphic-p frame)
             (not (assq frame atelier-navigator-window-configurations)))
    (with-selected-frame frame
      (atelier-capture-current-workspace)
      (atelier-notify-change)))
  (atelier-cleanup-detached-buffer frame)
  (setq atelier-navigator-window-configurations
        (assq-delete-all frame atelier-navigator-window-configurations)))

(defun atelier-restore-new-frame (frame)
  (when (and (frame-live-p frame) (display-graphic-p frame)
              (atelier-current-workspace))
    (with-selected-frame frame
      (atelier-restore-workspace (atelier-current-workspace))
      (atelier-navigator))))

(defun atelier-navigator-at-startup ()
  (when (display-graphic-p)
    (atelier-navigator)))

(defun atelier-setup ()
  (unless atelier-workspaces (atelier-create-default))
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
     (:eval (format "%s  " (or atelier-current-workspace-name "No workspace")))
     (:eval (atelier-mode-line-position))
     "  "))
   (add-hook 'window-configuration-change-hook #'atelier-notify-change)
   (add-hook 'pre-command-hook #'atelier-command-source-capture)
   (add-hook 'post-command-hook #'atelier-command-source-clear)
   (add-hook 'buffer-list-update-hook #'atelier-own-current-buffer)
  (add-hook 'find-file-hook #'atelier-own-current-buffer)
  (add-hook 'dired-mode-hook #'atelier-own-current-buffer)
  (add-hook 'delete-frame-functions #'atelier-capture-closing-frame)
  (add-hook 'after-make-frame-functions #'atelier-restore-new-frame)
  (add-hook 'emacs-startup-hook #'atelier-navigator-at-startup)
  nil)

(provide 'atelier)
;;; atelier.el ends here

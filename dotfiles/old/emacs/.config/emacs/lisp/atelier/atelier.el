;;; atelier.el --- Workspace entry trees and restoration -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'project)
(require 'subr-x)
(require 'tramp)
(require 'atelier-core)
(require 'atelier-model)
(require 'atelier-operation)

(defvar atelier-buffer-title-functions nil
  "Functions called with a buffer; the first non-nil result supplies its title.")
(defvar atelier-workspace-start-function #'atelier-empty-workspace-buffer
  "Function of a new WORKSPACE returning the buffer it shows first.
It also supplies the buffer shown when a saved layout cannot be restored.")

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
(defvar atelier-local-path-p-function #'atelier-default-local-path-p
  "Function called with workspace and path to identify local storage.
It must not open a connection; nil means absence cannot be checked locally.")
(defvar atelier-extra-destinations nil
  "Additional machines supplied by an optional environment adapter.
Each entry is (LABEL :destination DESTINATION :platform PLATFORM).  The value
may also be a function returning such entries when the choice is offered.")
(defvar atelier-file-path-function #'identity
  "Function translating a saved file or directory path before it is restored.")
(defvar atelier-read-choice-function #'atelier-default-read-choice
  "Function called with a TITLE and CHOICES, strings, returning the chosen one.")
(defvar atelier-read-directories-function #'atelier-default-read-directories
  "Function called with DIRECTORY and MULTIPLE to choose directory paths.
Return a nonempty list; when MULTIPLE is nil, return exactly one path.")
(defvar atelier-directory-target-function #'atelier-default-directory-target
  "Function mapping an Emacs directory to (DESTINATION PATH PLATFORM MOUNT-ROOT).")

(defun atelier-default-read-choice (title choices)
  "Choose one of CHOICES using ordinary Emacs completion."
  (completing-read (format "%s: " title) choices nil t))

(defun atelier-default-read-directories (directory _multiple)
  "Choose one directory using ordinary Emacs completion."
  (list (read-directory-name "Workspace directory: " directory nil t)))

(defun atelier-default-directory-target (directory)
  "Describe DIRECTORY using standard local, SSH, or WSL file handling."
  (let ((prefix (file-remote-p directory))
        (method (file-remote-p directory 'method)))
    (if prefix
        (progn
          (unless (member method '("ssh" "wsl"))
            (user-error "Unsupported workspace connection method: %s" method))
          (list (substring prefix (+ 2 (length method)) -1)
                (file-remote-p directory 'localname)
                (if (equal method "wsl") 'wsl 'posix) nil))
      (list "local" (expand-file-name directory) nil nil))))

(defun atelier-default-directory (workspace)
  "Resolve WORKSPACE with standard Emacs local and remote file handling."
  (let ((destination (plist-get workspace :destination))
        (path (plist-get workspace :path)))
    (if (equal destination "local") (file-name-as-directory (expand-file-name path))
      (format "/%s:%s:%s" (if (eq (plist-get workspace :platform) 'wsl) "wsl" "ssh")
              destination (file-name-as-directory path)))))

(defun atelier-default-execution-directory (_workspace directory)
  (or (file-remote-p directory 'localname) directory))

(defun atelier-default-local-path-p (_workspace path)
  "Return non-nil when PATH uses standard local Emacs file handling."
  (not (file-remote-p path)))

(defun atelier-default-terminal-command (workspace)
  (list :program nil :shell shell-file-name :arguments nil
        :directory (atelier-workspace-directory workspace)))

(defcustom atelier-stack-limit nil
  "Most buffers one workspace stack keeps, or nil for no limit.
Adding a buffer past the limit closes the least recently used ones.  A buffer
a view selects, or a file buffer with unsaved changes, is never closed."
  :type '(choice (const :tag "No limit" nil) natnum)
  :group 'atelier)

(declare-function atelier-dispose-unreferenced-buffer "atelier-views")

(defvar atelier-frame-covered-functions nil
  "Functions of a frame, non-nil when an interface covers that frame.
A covering interface keeps the frame's real layout aside until it leaves, so
Atelier neither captures nor announces the covered layout.")
(defvar atelier-uncover-frame-functions nil
  "Functions called with a frame to remove the interface covering it.")
(defvar atelier-covered-frame-refresh-functions nil
  "Functions called with a covered frame after workspace state changed under it.")

(defun atelier-frame-covered-p (&optional frame)
  "Whether an interface covers FRAME, the selected frame by default."
  (run-hook-with-args-until-success 'atelier-frame-covered-functions
                                    (or frame (selected-frame))))

(defun atelier-uncover-frame (&optional frame)
  "Remove any interface covering FRAME, restoring its real layout."
  (when (atelier-frame-covered-p frame)
    (run-hook-with-args 'atelier-uncover-frame-functions (or frame (selected-frame)))))

(defun atelier-buffer-internal-p (buffer)
  "Whether BUFFER is an interface or notice that workspaces never own."
  (and (gethash buffer atelier-internal-buffers) t))

(defvar atelier-interface-buffers (make-hash-table :test #'eq :weakness 'key)
  "Buffers of Atelier's own interfaces, such as the navigator.")

(defun atelier-mark-interface-buffer (&optional buffer)
  "Mark BUFFER, the current one by default, as an Atelier interface.
Workspaces never own it, clearing buffers spares it, and window history
drops it."
  (let ((buffer (or buffer (current-buffer))))
    (atelier-mark-internal-buffer buffer)
    (puthash buffer t atelier-interface-buffers)))

(defun atelier-interface-buffer-p (buffer)
  "Whether BUFFER is one of Atelier's own interfaces."
  (and (gethash buffer atelier-interface-buffers) t))

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

(define-error 'atelier-restore-unavailable "Saved content is unavailable")

(defun atelier-buffer-capture (buffer)
  "Return the content properties describing BUFFER, from BUFFER's own type.
A type without :capture describes a transient buffer."
  (with-current-buffer buffer
    (if-let* ((capture (atelier-type-get (atelier-buffer-type buffer) :capture)))
        (funcall capture buffer)
      (list :kind 'transient :persistent nil))))

(defun atelier-job-capture ()
  "Return the content properties of the current buffer's restartable job."
  (list :kind 'terminal :persistent t :directory default-directory))

(defun atelier-file-capture (_buffer)
  (list :kind 'file :persistent t
        :file (expand-file-name buffer-file-name)
        :directory default-directory))

(defun atelier-file-matches-p (content buffer)
  (when-let* ((file (buffer-local-value 'buffer-file-name buffer)))
    (equal (plist-get content :file) (expand-file-name file))))

(defun atelier-file-restore (content workspace)
  (let ((file (plist-get content :file)))
    (unless (file-readable-p file)
      (signal 'atelier-restore-unavailable (list "File is missing or unreadable")))
    (atelier-file-buffer file workspace)))

(defun atelier-buffer-type-capture (buffer)
  "Describe a scratch BUFFER with its text, and any other buffer as transient."
  (if (string-prefix-p "*scratch" (buffer-name buffer))
      (list :kind 'scratch :persistent t :directory default-directory
            :contents (buffer-substring-no-properties (point-min) (point-max)))
    (list :kind 'transient :persistent nil)))

(defun atelier-buffer-type-restore (content _workspace)
  "Recreate scratch CONTENT, or find a transient buffer by its saved name."
  (if (eq (plist-get content :kind) 'scratch)
      (let ((buffer (atelier-operation-track-buffer
                     (generate-new-buffer (or (plist-get content :name) "*scratch*")))))
        (with-current-buffer buffer
          (funcall initial-major-mode)
          (insert (or (plist-get content :contents) "")))
        buffer)
    (atelier-restore-by-name content)))

(defun atelier-restore-by-name (content)
  "Return the live buffer named like CONTENT, the default restoration."
  (or (and (plist-get content :name) (get-buffer (plist-get content :name)))
      (signal 'atelier-restore-unavailable (list "No saved content could be restored"))))

(atelier-define-type 'file
  :tracked t
  :buffer-p (lambda (buffer) (buffer-local-value 'buffer-file-name buffer))
  :capture #'atelier-file-capture
  :matches #'atelier-file-matches-p
  :restore #'atelier-file-restore
  :missing-paths (lambda (content) (list (plist-get content :file))))

(atelier-define-type 'buffer
  :capture #'atelier-buffer-type-capture
  :restore #'atelier-buffer-type-restore)

(defun atelier-buffer-owned-by-other-workspace-p (buffer workspace)
  "Return non-nil when BUFFER is owned outside WORKSPACE, even in a stack."
  (cl-some (lambda (candidate)
             (and (not (eq candidate workspace))
                  (cl-some (lambda (entry)
                             (or (eq buffer (atelier-entry-live-buffer entry))
                                 (atelier-entry-inactive-buffer-p entry buffer)))
                           (atelier-workspace-entries candidate))))
           atelier-workspaces))

(defun atelier-buffer-registerable-p (buffer workspace)
  "Return non-nil when BUFFER may become an entry of WORKSPACE."
  (and (atelier-buffer-ownable-p buffer workspace)
       (not (atelier-buffer-owned-by-other-workspace-p buffer workspace))))

(defun atelier-entry-matches-buffer-p (entry buffer)
  "Whether ENTRY shows BUFFER, live or by its type's :matches rule."
  (or (eq (atelier-entry-live-buffer entry) buffer)
      (and (not (atelier-entry-live-buffer entry))
           (when-let* ((matches (atelier-type-get (atelier-entry-value entry :type) :matches)))
             (funcall matches (atelier-entry-content entry) buffer)))))

(defun atelier-workspace-entry-for-buffer (workspace buffer)
  (or (cl-find-if (lambda (entry) (atelier-entry-matches-buffer-p entry buffer))
                 (atelier-workspace-view-entries workspace))
      (cl-loop for content in (plist-get workspace :contents)
               when (atelier-entry-matches-buffer-p (atelier-content-reference workspace content) buffer)
               return (atelier-content-reference workspace content))))

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
  "Record BUFFER's state in ENTRY: a job's own description, else its type's."
  (with-current-buffer buffer
    (let ((job (atelier-entry-job entry)))
      (atelier-entry-set-value entry :name (buffer-name))
      (cl-loop for (property value) on (if job (atelier-job-capture)
                                         (atelier-buffer-capture buffer))
               by #'cddr
               do (atelier-entry-set-value entry property value))
      (when type (atelier-entry-set-value entry :type type))
      (when job (setf (plist-get job :buffer) (buffer-name)))
      (atelier-entry-set-value entry :point (point))))
  (atelier-entry-set-live-buffer entry buffer)
  entry)

(atelier-define-operation atelier-register-buffer (buffer &optional workspace no-notify type)
    (list (atelier-workspace-id (or workspace (atelier-current-workspace))))
    ((workspace (atelier-operation-workspace (or workspace (atelier-current-workspace)))))
  "Register BUFFER as an entry of WORKSPACE and return that entry.

WORKSPACE defaults to the workspace selected by the current frame.  The
workspace record is authoritative; BUFFER receives no ownership metadata."
  (setq workspace (or workspace (atelier-current-workspace))
        type (or type (atelier-buffer-type buffer)))
  (when (and workspace (atelier-buffer-registerable-p buffer workspace))
    (let ((entry (atelier-workspace-entry-for-buffer workspace buffer))
          added)
      (unless entry
        (setq added t)
        (let ((id (atelier-workspace-store-content
                   workspace (list :type type
                                   :kind (plist-get (atelier-buffer-capture buffer) :kind)))))
          (setq entry (atelier-content-reference workspace
                                                 (atelier-workspace-content workspace id)))
          (atelier-workspace-trim-stack workspace id)))
      (atelier-update-entry-from-buffer entry buffer type)
      (when (and added (not no-notify))
        (atelier-operation-notify 'atelier-entry-added-hook workspace entry)
        (atelier-operation-notify 'atelier-change-hook))
      entry)))

(defun atelier-workspace-trim-stack (workspace keep)
  "Close least recently used contents beyond `atelier-stack-limit'.
Trim the stack holding content KEEP in WORKSPACE, never closing KEEP."
  (when-let* ((limit atelier-stack-limit)
              (stack (atelier-workspace-stack-record
                      workspace (plist-get (atelier-workspace-content workspace keep) :stack-id))))
    (let ((excess (- (length (plist-get stack :content-ids)) limit))
          (selected (mapcar (lambda (view) (plist-get view :content-id))
                            (atelier-workspace-view-entries workspace))))
      (dolist (id (reverse (plist-get stack :content-ids)))
        (let ((buffer (gethash (atelier-content-cache-key workspace id)
                               atelier-content-live-buffers)))
          (unless (or (<= excess 0) (equal id keep) (member id selected)
                      (and (buffer-live-p buffer)
                           (buffer-local-value 'buffer-file-name buffer)
                           (buffer-modified-p buffer)))
            (atelier-workspace-drop-content workspace id)
            (atelier-dispose-unreferenced-buffer buffer)
            (cl-decf excess)))))))

(defvar atelier-capturing-layout-p nil)
(defvar atelier-capture-used-entry-ids nil)
(defvar atelier-capture-layout-entries nil)

(defun atelier-capture-buffer (buffer &optional window workspace)
  (setq workspace (or workspace (atelier-current-workspace)))
  (if-let* ((id (and (window-live-p window) (window-parameter window 'atelier-view-id)))
             (view (atelier-entry-by-id workspace id))
             (_ (and (or (plist-get view :unassigned)
                         (and (plist-get view :stack-id) (not (plist-get view :content-id))))
                     (not (atelier-buffer-registerable-p buffer workspace)))))
      (progn (push id atelier-capture-used-entry-ids) view)
    (let ((type (atelier-buffer-type buffer)))
    (when-let* ((_ (atelier-buffer-registerable-p buffer workspace))
                (entry
                 (if atelier-capturing-layout-p
                      (or (let* ((id (and (window-live-p window)
                                         (window-parameter window 'atelier-view-id)))
                                 (view (and id (atelier-entry-by-id workspace id))))
                            (when (and view (not (plist-get view :content-reference))
                                       (not (member id atelier-capture-used-entry-ids)))
                              (let ((content (atelier-register-buffer buffer workspace t type)))
                                (atelier-entry-activate-content view (plist-get content :content-id) t))
                              view))
                          (cl-find-if
                          (lambda (candidate)
                            (and (not (member (plist-get candidate :id)
                                              atelier-capture-used-entry-ids))
                                 (atelier-entry-matches-buffer-p candidate buffer)))
                           (atelier-workspace-view-entries workspace))
                           (let* ((shared (atelier-register-buffer buffer workspace t type))
                                  (entry (list :id (atelier-new-entry-id)
                                               :content-id (plist-get shared :content-id))))
                           (atelier-entry-add workspace entry t)))
                   (atelier-register-buffer buffer workspace t type))))
      (with-current-buffer buffer
        (setq entry (atelier-update-entry-from-buffer entry buffer))
        (atelier-entry-set-value entry :point (point))
        (atelier-entry-set-value entry :start (and (window-live-p window) (window-start window)))
        (setf (plist-get entry :selected) (and (window-live-p window)
                                               (eq window (selected-window))))
        (atelier-plist-clear! entry :displayed))
      (push (plist-get entry :id) atelier-capture-used-entry-ids)
      (when (window-live-p window)
        (set-window-parameter window 'atelier-view-id (plist-get entry :id)))
      entry))))

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

(atelier-define-operation atelier-capture-current-workspace ()
    (list (atelier-current-workspace-id)) nil
  (when-let* ((_ (display-graphic-p (selected-frame)))
              ;; The navigator covers the frame; its saved configuration
              ;; holds the real layout until it quits.
              (_ (not (atelier-frame-covered-p)))
              (workspace (atelier-current-workspace))
              (_ (not (cl-some
                       (lambda (window)
                         (equal (atelier-workspace-id workspace)
                                (buffer-local-value 'atelier-unavailable-workspace-id
                                                    (window-buffer window))))
                       (atelier-main-windows)))))
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
            (old-leaves (copy-sequence (atelier-workspace-view-entries workspace)))
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
      (atelier-workspace-refresh-parent-ids workspace)
      (cl-remf workspace :state)
      (cl-remf workspace :layout))
    workspace))

(defun atelier-set-buffer-excluded (excluded &optional buffer)
  "When EXCLUDED, leave BUFFER to its own owner instead of Atelier.
Clear the exclusion when that owner is done.  BUFFER defaults to the
current buffer; this does not create or remove workspace entries."
  (let ((buffer (or buffer (current-buffer))))
    (if excluded
        (puthash buffer t atelier-internal-buffers)
      (remhash buffer atelier-internal-buffers))))

(defun atelier-mark-internal-buffer (&optional buffer)
  "Exclude Atelier's own BUFFER from user workspace entry registration."
  (atelier-set-buffer-excluded t buffer))

(defun atelier-assign-buffer-to-workspace
    (&optional buffer workspace type)
  "Compatibility wrapper for registering BUFFER in WORKSPACE."
  (let ((buffer (or buffer (current-buffer))))
    (remhash buffer atelier-internal-buffers)
    (atelier-register-buffer buffer (or workspace (atelier-current-workspace))
                             nil type)
    buffer))

(defun atelier-buffer-ownable-p (buffer &optional workspace)
  (with-current-buffer buffer
    (let ((name (buffer-name)))
      (and (or workspace (atelier-current-workspace))
           (not atelier-inhibit-buffer-ownership)
           (not (gethash buffer atelier-internal-buffers))
           (not (minibufferp buffer))
           (not (string-prefix-p " " name))
           (not (member name atelier-global-buffer-names))
           (not (string-prefix-p atelier-empty-buffer-prefix name))))))

(defun atelier-own-current-buffer ()
  (when (and (not atelier-operation-current)
             (atelier-buffer-ownable-p (current-buffer)))
    (atelier-register-buffer (current-buffer) (atelier-current-workspace))))

(defun atelier-record-changed-windows (frame)
  "Record each FRAME window whose buffer changed since the last run.
Emacs compares every window with its buffer at the previous run, so no change
is missed.  A change seen while an operation prepares its records is recorded
once they are published."
  (if atelier-operation-current
      (atelier-operation-after (lambda () (atelier-record-changed-windows frame)))
    (when-let* (((frame-live-p frame))
                (workspace (atelier-current-workspace frame)))
      (dolist (window (window-list frame 'no-minibuffer))
        ;; The old buffer is nil for a new window and t for a restored one.
        (unless (or (window-parameter window 'window-side)
                    (eq (window-old-buffer window) (window-buffer window)))
          (when (atelier-buffer-registerable-p (window-buffer window) workspace)
            (atelier-record-window-buffer (window-buffer window) workspace window)))))))

(atelier-define-operation atelier-refresh-current-buffer-entries ()
    (delete-dups (mapcar (lambda (pair) (atelier-workspace-id (car pair)))
                        (atelier-entries-for-buffer (current-buffer)))) nil
  "Refresh every workspace entry resolving to the current buffer."
  (dolist (pair (atelier-entries-for-buffer (current-buffer)))
    (atelier-update-entry-from-buffer (nth 1 pair) (current-buffer)))
  (when (atelier-entries-for-buffer (current-buffer))
    (atelier-notify-change)))

(defun atelier-current-buffer-killed ()
  "Record a native buffer exit in live state, not a prepared copy."
  (let ((buffer (current-buffer)))
    (atelier-operation-live-event
     (lambda ()
       (dolist (operation atelier-operation-active)
         (when (and (not atelier-operation-owned-effect)
                    (cl-some (lambda (pair)
                         (or (memq :all (atelier-operation-ids operation))
                             (member (atelier-workspace-id (car pair))
                                     (atelier-operation-ids operation))))
                             (atelier-entries-for-buffer buffer)))
           (setf (atelier-operation-invalid operation) t)))
       (atelier--current-buffer-killed)))))

(defun atelier--current-buffer-killed ()
  "Update entry runtime state when the current buffer is killed natively."
  (let (changed)
    (dolist (workspace atelier-workspaces)
      (dolist (content (copy-sequence (plist-get workspace :contents)))
        (let ((key (atelier-content-cache-key workspace (plist-get content :id))))
          (when (eq (gethash key atelier-content-live-buffers) (current-buffer))
            (remhash key atelier-content-live-buffers)
            ;; Detached cannot stop, so a kill there never preserves content
            ;; for a later restart.
            (unless (or atelier-preserve-job-recipe
                        (and (not (eq workspace (atelier-current-workspace)))
                             (not (atelier-detached-workspace-p workspace))
                             (plist-get content :persistent)))
              (atelier-workspace-drop-content workspace (plist-get content :id)))
            (setq changed t)))))
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
        (atelier-workspace-live-content-buffers workspace)))))

(defun atelier-buffer-visits-file-p (buffer file)
  (when-let* ((visited (buffer-local-value 'buffer-file-name buffer)))
    (condition-case nil
        (file-equal-p visited file)
      (error nil))))

(defun atelier-create-file-buffer (file)
  "Visit FILE in a new live buffer even when another buffer visits it."
  (let* ((file (abbreviate-file-name (expand-file-name file)))
          (buffer (atelier-operation-track-buffer (create-file-buffer file))))
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
              (not (atelier-buffer-referenced-p buffer))))
       (buffer-list))
      (if (cl-find-if (lambda (buffer) (atelier-buffer-visits-file-p buffer file))
                      (buffer-list))
          (atelier-create-file-buffer file)
        (find-file-noselect file))))

(atelier-define-operation atelier-push-buffer (buffer &optional workspace type preferred-entry)
    (list (atelier-workspace-id (or workspace (atelier-current-workspace))))
    ((workspace (atelier-operation-workspace (or workspace (atelier-current-workspace))))
     (preferred-entry (and preferred-entry (atelier-operation-entry workspace preferred-entry))))
  "Register BUFFER in WORKSPACE's TYPE stack and optionally select it in a view."
  (let ((reference (atelier-register-buffer buffer workspace nil type)))
    (unless reference (user-error "Buffer cannot be owned by this workspace"))
    (when preferred-entry
      (atelier-entry-activate-content preferred-entry (plist-get reference :content-id)))
    reference))

(defun atelier-drop-undisplayed-views (workspace buffer kept)
  "Remove WORKSPACE's views of BUFFER that no window shows, except KEPT.
A closed split leaves its view behind; once BUFFER shows in another view,
that leftover would list BUFFER a second time.  BUFFER stays in its stack."
  (let ((shown (delq nil (mapcar (lambda (window) (window-parameter window 'atelier-view-id))
                                 (window-list-1 nil 'nomini t)))))
    (dolist (view (atelier-workspace-view-entries workspace))
      (when (and (not (eq view kept))
                 (not (plist-get view :content-reference))
                 (not (member (plist-get view :id) shown))
                 (eq (atelier-entry-live-buffer view) buffer))
        (atelier-entry-remove workspace view t)))))

(defun atelier-view-record-buffer (buffer workspace window &optional type)
  "Register BUFFER in WORKSPACE and select it in WINDOW's view.
Create the view when WINDOW has none."
  (let* ((window (or window (selected-window)))
         (reference (atelier-push-buffer buffer workspace type))
         (id (window-parameter window 'atelier-view-id))
         (view (and id (atelier-entry-by-id workspace id))))
    (unless (and view (not (plist-get view :content-reference)))
      (setq view (atelier-entry-add
                  workspace (list :id (atelier-new-entry-id)
                                  :content-id (plist-get reference :content-id)) t)))
    (atelier-entry-activate-content view (plist-get reference :content-id))
    (set-window-parameter window 'atelier-view-id (plist-get view :id))
    (atelier-drop-undisplayed-views workspace buffer view)
    ;; Registration trims too, but before this view leaves its old buffer.
    (atelier-workspace-trim-stack workspace (plist-get reference :content-id))
    view))

(atelier-define-operation atelier-show-buffer (buffer &optional workspace window type)
    (list (atelier-workspace-id (or workspace (atelier-current-workspace))))
    ((workspace (atelier-operation-workspace (or workspace (atelier-current-workspace)))))
  "Display BUFFER in WINDOW, retaining that view's identity and other views."
  (let* ((window (or window (selected-window)))
         (view (atelier-view-record-buffer buffer workspace window type)))
    (set-window-buffer window buffer)
    (when (eq window (selected-window)) (set-buffer buffer))
    (atelier-notify-change)
    view))

(atelier-define-operation atelier-record-window-buffer (buffer &optional workspace window)
    (list (atelier-workspace-id (or workspace (atelier-current-workspace))))
    ((workspace (atelier-operation-workspace (or workspace (atelier-current-workspace)))))
  "Point WINDOW's view at the BUFFER it already shows.
Neither display nor activate BUFFER."
  (prog1 (atelier-view-record-buffer buffer workspace window)
    (atelier-notify-change)))

(atelier-define-operation atelier-open-file (file &optional workspace)
    (list (atelier-workspace-id (or workspace (atelier-current-workspace))))
    ((workspace (atelier-operation-workspace (or workspace (atelier-current-workspace)))))
  "Open FILE in WORKSPACE, stacking it in the current entry when possible."
  (setq workspace (or workspace (atelier-current-workspace)))
  (let ((buffer (atelier-file-buffer file workspace)))
    (atelier-show-buffer buffer workspace nil 'file)
    buffer))

(defun atelier-path-confirmed-missing-p (path)
  "Whether PATH is absent beneath a directory we can inspect.
An inaccessible directory or a file-handler error is not proof of absence."
  (condition-case nil
      (and (not (file-exists-p path))
           (let ((parent (file-name-directory (directory-file-name path))))
             (and parent (not (equal (directory-file-name path)
                                     (directory-file-name parent)))
                  (or (file-accessible-directory-p parent)
                      (atelier-path-confirmed-missing-p parent)))))
    (error nil)))

(defun atelier-content-unrestorable-p (workspace content &optional connected)
  "Identify confirmed dead CONTENT, without connecting unless CONNECTED.
Permission, connection and editor setup failures do not prove a record dead."
  (let* ((job (plist-get content :job))
         (recipe (plist-get job :recipe))
         (executable (plist-get recipe :executable))
         (paths (if job
                    (list (plist-get recipe :directory)
                          (and executable (file-name-absolute-p executable) executable))
                  (when-let* ((missing (atelier-type-get (plist-get content :type)
                                                         :missing-paths)))
                    (funcall missing content)))))
    (and (not (buffer-live-p
               (gethash (atelier-content-cache-key workspace (plist-get content :id))
                        atelier-content-live-buffers)))
         (or (and (eq (plist-get content :kind) 'terminal) (not executable))
             (and (eq (plist-get content :kind) 'transient)
                  (not (get-buffer (or (plist-get content :name) ""))))
             (cl-some
              (lambda (path)
                (and path
                     (or connected (funcall atelier-local-path-p-function workspace path))
                     (atelier-path-confirmed-missing-p path)))
              paths)))))

(defun atelier-prune-workspace-contents (workspace &optional connected)
  "Prune only confirmed dead contents and empty views in WORKSPACE."
  (let ((atelier-model-workspace workspace) changed)
    (atelier-workspace-entries workspace)
    (dolist (content (copy-sequence (plist-get workspace :contents)))
      (when (atelier-content-unrestorable-p workspace content connected)
        (atelier-forget-failed-content
         workspace (plist-get content :id) "Saved file, directory, program or buffer is no longer available")
        (setq changed t)))
    changed))

(defun atelier-forget-failed-content (workspace content-id reason)
  "Remove failed CONTENT-ID and every reference, reporting REASON."
  (when-let* ((content (atelier-workspace-content workspace content-id)))
    (let ((label (or (plist-get content :file) (plist-get content :name)
                     (plist-get content :directory) content-id))
          (stack-id (plist-get content :stack-id)))
      (atelier-workspace-drop-content workspace content-id)
      (when-let* ((stack (atelier-workspace-stack-record workspace stack-id))
                  (_ (not (plist-get stack :content-ids))))
        (dolist (view (copy-sequence (atelier-workspace-view-entries workspace)))
          (when (equal stack-id (plist-get view :stack-id))
            (atelier-entry-remove workspace view t)))
        (atelier-plist-set! workspace :stacks (remq stack (plist-get workspace :stacks))))
      (when (fboundp 'atelier-persist-record-pruning)
        (atelier-persist-record-pruning))
      (atelier-notify-change)
      (atelier-log "Removed unrestorable content %s: %s" label reason)))
  nil)

(defun atelier-restore-entry-content (entry workspace content-id)
  "Prepare CONTENT-ID's restoration and publish selection only on success."
  (let ((workspace-id (atelier-workspace-id workspace))
        (entry-id (plist-get entry :id)))
    (atelier-operation-call
     'select-content (list workspace-id)
     (lambda ()
       (let* ((workspace (atelier-operation-workspace workspace-id))
              (entry (atelier-operation-entry workspace entry-id))
              (buffer (atelier--restore-entry-content entry workspace content-id)))
         buffer)) t
     (lambda ()
       (let ((workspace (atelier-operation-workspace workspace-id)))
         (atelier-restore-entry-content (atelier-operation-entry workspace entry-id)
                                        workspace content-id))))))

(defun atelier--restore-entry-content (entry workspace content-id)
  "Restore CONTENT-ID, removing every reference when restoration fails."
  (atelier-entry-stack entry workspace)
  (unless (member content-id (atelier-entry-content-ids entry workspace))
    (user-error "Content no longer belongs to this entry"))
  (let ((previous (plist-get entry :content-id)) buffer)
    (unwind-protect
        (progn
          (atelier-entry-activate-content entry content-id)
          (setq buffer (condition-case error
                           (atelier-restore-buffer entry workspace)
                         (error
                          (atelier-forget-failed-content
                           workspace content-id (error-message-string error)))))
          (unless (buffer-live-p buffer)
            (atelier-forget-failed-content
             workspace content-id "Restoration returned no live buffer")
            (when (atelier-workspace-content workspace previous)
              (atelier-plist-set! entry :content-id previous))
            (setq previous (plist-get entry :content-id)))
          buffer)
      (unless (buffer-live-p buffer)
        (atelier-plist-set! entry :content-id previous)))))

(defun atelier-restore-entry-buffer (entry workspace)
  "Restore the first usable content of ENTRY, removing failed contents."
  (cl-loop for id in (atelier-entry-content-ids entry)
           thereis (atelier--restore-entry-content entry workspace id)))

(defvar-local atelier-unavailable-workspace-id nil
  "Workspace whose saved layout must not be replaced by this internal notice.")

(defun atelier-unavailable-workspace-buffer (workspace)
  "Show an unavailable WORKSPACE without creating saved content or a new view."
  (let ((buffer (get-buffer-create
                 (format " *Atelier unavailable workspace:%s*" (atelier-workspace-id workspace)))))
    (with-current-buffer buffer
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert (format "Workspace %s is unavailable.\n\n" (plist-get workspace :name))
                "Its saved layout and contents are retained. Retry opening it or choose another workspace in the navigator.\n")
        (special-mode))
      (setq-local atelier-unavailable-workspace-id (atelier-workspace-id workspace))
      (atelier-mark-internal-buffer))
    buffer))

(defun atelier-display-entry-buffer (entry workspace window &optional buffer)
  "Display BUFFER, removing ENTRY if it has no usable content."
  (if (or (plist-get entry :unassigned) (not (plist-get entry :stack-id)))
      (atelier-close-unassigned-view workspace entry window)
    (let ((buffer (or buffer
                      (and (not (atelier-entry-stack entry))
                           (atelier-empty-view-buffer workspace entry))
                      (atelier-entry-live-buffer entry))))
      (set-window-parameter window 'atelier-view-id (plist-get entry :id))
      (set-window-parameter window 'atelier-unavailable-entry nil)
      (unless (buffer-live-p buffer)
        (atelier-entry-remove workspace entry)
        (when (fboundp 'atelier-persist-record-pruning)
          (atelier-persist-record-pruning)))
      (set-window-buffer window (if (buffer-live-p buffer) buffer
                                 (atelier-empty-workspace-buffer workspace))))))

(defun atelier-close-unassigned-view (workspace view window &optional excluded-stack-id)
  "Remove VIEW and its split without closing any stack or buffer.
For the final split, show another stack or switch to another workspace."
  (atelier-entry-remove workspace view t)
  (set-window-parameter window 'atelier-view-id nil)
  (set-window-parameter window 'atelier-unavailable-entry nil)
  (if (> (length (atelier-main-windows (window-frame window))) 1)
      (delete-window window)
    (let ((replacement
           (cl-loop for entry in (atelier-workspace-replacement-entries workspace)
                    unless (equal excluded-stack-id (plist-get entry :stack-id))
                    thereis (or (atelier-entry-live-buffer entry)
                                (atelier-restore-entry-buffer entry workspace)))))
      (if replacement
          (atelier-show-buffer replacement workspace window)
        ;; Clear the old display so capture cannot recreate the removed view.
        (set-window-buffer window (atelier-empty-workspace-buffer workspace))
        (when-let* ((next (atelier-workspace-successor workspace)))
          (atelier-switch-workspace (plist-get next :name)))))))

(defun atelier-empty-view-buffer (workspace view)
  "Return the internal notice for VIEW without creating managed content."
  (let ((buffer (get-buffer-create
                 (format "%s%s/%s*" atelier-empty-buffer-prefix
                         (atelier-workspace-id workspace) (plist-get view :id)))))
    (with-current-buffer buffer
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert "Assigned stack is empty.\n")
        (special-mode))
      (atelier-mark-internal-buffer))
    buffer))

(defun atelier-restore-content (content workspace)
  "Return a live buffer for saved CONTENT through its type's :restore rule.
A job's buffer cannot be recreated here; jobs restart through their recipe."
  (if (eq (plist-get content :kind) 'terminal)
      (signal 'atelier-restore-unavailable (list "No live terminal could be restored"))
    (if-let* ((restore (atelier-type-get (plist-get content :type) :restore)))
        (funcall restore content workspace)
      (atelier-restore-by-name content))))

(defun atelier-restore-buffer (entry &optional workspace)
  "Restore ENTRY, or forget its failed content and report the reason."
  (let* ((workspace (or workspace (atelier-current-workspace)))
         (content (copy-sequence (atelier-entry-content entry workspace)))
         (directory (when-let* ((saved (plist-get content :directory)))
                      (funcall atelier-file-path-function saved)))
         (content-id (plist-get entry :content-id))
         failure
         (buffer
          (condition-case error
              (let ((buffer
                     (or (atelier-entry-live-buffer entry)
                         (progn
                           (when-let* ((saved (plist-get content :file)))
                             (setq content (plist-put content :file
                                                      (funcall atelier-file-path-function saved))))
                           (setq content (plist-put content :directory directory))
                           (atelier-restore-content content workspace)))))
                (when (buffer-live-p buffer)
                  (with-current-buffer buffer
                    (cond
                     ((and directory (file-directory-p directory))
                      (setq default-directory directory))
                     ((and directory (not (file-remote-p directory)))
                      (setq default-directory (atelier-workspace-directory))
                      (atelier-log "Missing split directory %s; using workspace root %s"
                                   directory default-directory)))
                    (goto-char (min (point-max)
                                    (max (point-min)
                                         (or (atelier-entry-value entry :point) 1)))))
                  (atelier-entry-set-live-buffer entry buffer)
                  (atelier-operation-notify 'atelier-entry-restored-hook workspace entry buffer))
                buffer)
            (atelier-restore-unavailable
             (setq failure (cadr error))
             nil)
            (error
             (atelier-operation-notify 'atelier-entry-restore-failed-hook workspace entry error)
             (setq failure (error-message-string error))
             nil))))
    (if (buffer-live-p buffer)
        buffer
      (atelier-forget-failed-content
       workspace content-id (or failure "No saved content could be restored")))))

(defun atelier-clean-window-buffer-history ()
  (dolist (window (window-list nil 'no-minibuffer))
    (set-window-prev-buffers
     window
     (cl-remove-if
      (lambda (entry)
        (atelier-navigation-buffer-p (car entry)))
      (window-prev-buffers window)))
    (set-window-next-buffers
     window
     (cl-remove-if
      (lambda (buffer)
        (atelier-navigation-buffer-p buffer))
      (window-next-buffers window)))))

(defun atelier-navigation-buffer-p (buffer)
  "Whether BUFFER is an Atelier interface, to drop from window history."
  (and (buffer-live-p buffer) (atelier-interface-buffer-p buffer)))

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
    (let ((buffer (atelier-entry-live-buffer entry)))
      (atelier-display-entry-buffer entry workspace window buffer)
      (when (and (window-live-p window) (buffer-live-p buffer))
        (set-window-point
         window (min (with-current-buffer buffer (point-max))
                     (max 1 (or (atelier-entry-value entry :point) 1))))
        (when-let* ((start (atelier-entry-value entry :start)))
          (set-window-start window start t)))
      (when (and (window-live-p window) (plist-get entry :selected))
        (select-window window)))))

(atelier-define-operation atelier-restore-workspace (workspace)
    (list (atelier-workspace-id workspace))
    ((workspace (atelier-operation-workspace workspace)))
  (let ((default-directory (atelier-workspace-directory workspace))
        restored-buffers)
    (unless (file-directory-p default-directory)
      (user-error "Workspace root is unavailable: %s" default-directory))
    (atelier-prune-workspace-contents workspace t)
    (dolist (entry (copy-sequence (atelier-workspace-entries workspace)))
      (when-let* ((_ (atelier-entry-by-id workspace (plist-get entry :id)))
                  (buffer (atelier-restore-entry-buffer entry workspace)))
        (push buffer restored-buffers)))
    (delete-other-windows)
    (if-let* ((entry (atelier-workspace-displayed-entry workspace)))
        (condition-case error
            (atelier-restore-entry-layout entry (atelier-main-window) workspace)
          (error
           (atelier-log "Workspace layout restore failed: %s" error)
           (switch-to-buffer (or (car (nreverse restored-buffers))
                                 (funcall atelier-workspace-start-function workspace)))))
      (switch-to-buffer (or (car (nreverse restored-buffers))
                            (atelier-empty-workspace-buffer workspace))))
    (atelier-clean-window-buffer-history)
    (setq default-directory (atelier-workspace-directory workspace))))

(defun atelier-notify-change ()
  (force-mode-line-update t)
  (unless (atelier-frame-covered-p)
    (atelier-operation-notify 'atelier-change-hook)))

(defun atelier-workspace-stop-jobs (workspace &optional forget preserve-shared)
  "Stop WORKSPACE's jobs, optionally retaining jobs used by running workspaces."
  (let* ((entries (cl-remove-if
                   (lambda (entry)
                     (and preserve-shared
                          (atelier-buffer-shared-with-running-workspace-p
                           (atelier-entry-live-buffer entry) workspace)))
                   (atelier-workspace-job-entries workspace)))
         (atelier-approved-buffer-closes
          (append (mapcar #'atelier-prepare-buffer-close
                          (delq nil (mapcar #'atelier-entry-live-buffer
                                            entries)))
                  atelier-approved-buffer-closes)))
    (atelier-validate-buffer-closes)
    (dolist (entry entries)
      (let ((buffer (atelier-entry-live-buffer entry)))
        (when (buffer-live-p buffer)
          (let ((atelier-preserve-job-recipe (not forget)))
            (atelier-kill-buffer buffer)))
        (atelier-entry-set-live-buffer entry nil)
        (when forget (atelier-entry-remove workspace entry t))))))

(atelier-define-operation atelier-register-job-buffer
    (buffer shell directory &optional direct-command policy agent type)
    (list (atelier-workspace-id (or atelier-job-owner-workspace (atelier-current-workspace))))
    ((atelier-job-owner-workspace
      (atelier-operation-workspace (or atelier-job-owner-workspace (atelier-current-workspace))))
     (atelier-job-owner-entry
      (and atelier-job-owner-entry (atelier-operation-entry atelier-job-owner-workspace atelier-job-owner-entry))))
  (let* ((workspace (or atelier-job-owner-workspace (atelier-current-workspace)))
         (name (buffer-name buffer))
         (entry (or atelier-job-owner-entry
                     (atelier-workspace-entry-for-buffer workspace buffer)
                    (atelier-register-buffer buffer workspace t type)))
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
    (atelier-entry-set-value entry :job job)
    (atelier-entry-set-value entry :kind 'terminal)
    (atelier-entry-set-value entry :persistent t)
    (when type (atelier-entry-set-value entry :type type))
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

(defvar atelier-destination-suggestion-functions nil
  "Functions returning SSH destinations to suggest when choosing a machine.
Each returns a list of destination strings, most relevant first.")

(defun atelier-read-workspace-target (&optional multiple)
  "Choose a connection and directory, or directory targets when MULTIPLE."
  (let* ((extra (if (functionp atelier-extra-destinations)
                    (funcall atelier-extra-destinations)
                  atelier-extra-destinations))
         (destinations (delete-dups
                        (append '("local")
                                (mapcar #'car extra)
                                '("Enter SSH destination")
                                (apply #'append (mapcar #'funcall atelier-destination-suggestion-functions))
                                atelier-remembered-ssh-destinations)))
         (choice (funcall atelier-read-choice-function "Machine" destinations))
         (target (cdr (assoc choice extra)))
         (destination (cond (target (plist-get target :destination))
                            ((equal choice "Enter SSH destination")
                             (read-string "SSH destination: "))
                            (t choice)))
         (platform (cond
                    (target (plist-get target :platform))
                    ((equal destination "local") nil)
                    ((equal (funcall atelier-read-choice-function
                             "Remote system" '("POSIX" "Windows"))
                            "Windows")
                     'windows)
                    (t 'posix)))
         (mount-root (when (eq platform 'windows)
                       (format "/%s:/" (upcase (read-string "Windows drive: " "C")))))
         (probe (when (memq platform '(windows wsl))
                  (list :destination destination :platform platform
                        :mount-root mount-root :path (if (eq platform 'wsl) "/" mount-root))))
         (remote-prefix (when (eq platform 'posix) (format "/ssh:%s:" destination)))
         (directories (funcall atelier-read-directories-function
                      (cond (probe (funcall atelier-directory-function probe))
                            (remote-prefix remote-prefix)
                             (t (expand-file-name "~/"))) multiple))
         targets)
    (unless directories (user-error "Choose a directory"))
    (when (and (not multiple) (cdr directories))
      (user-error "Choose exactly one directory to edit a workspace"))
    (dolist (directory directories)
      (unless (file-directory-p directory)
        (user-error "Directory does not exist: %s" directory))
      (push (list destination
                  (cond (probe (funcall atelier-target-directory-function probe directory))
                        (remote-prefix (file-remote-p directory 'localname))
                        (t directory))
                  platform mount-root) targets))
    (when remote-prefix
       (atelier-operation-after
        (lambda () (cl-pushnew destination atelier-remembered-ssh-destinations :test #'equal))))
    (if multiple (nreverse targets) (car targets))))

(atelier-define-operation atelier-edit-workspace ()
    (list (atelier-current-workspace-id)) nil
  (interactive)
  (let ((workspace (atelier-current-workspace)))
    (unless workspace (user-error "No workspace is open"))
    (when (atelier-detached-workspace-p workspace)
      (user-error "The Detached workspace target cannot be changed"))
    (let ((old-directory (atelier-workspace-directory workspace)))
      (pcase-let ((`(,destination ,path ,platform ,mount-root) (atelier-read-workspace-target)))
        (let ((old (copy-tree workspace)))
          (atelier-operation-after (lambda () (funcall atelier-release-function old))))
        (setf (plist-get workspace :destination) destination
              (plist-get workspace :path) path
              (plist-get workspace :platform) platform
              (plist-get workspace :mount-root) mount-root)
        (dolist (entry (atelier-workspace-entries workspace))
          (when (equal (atelier-entry-value entry :directory) old-directory)
            (atelier-entry-set-value entry :directory (atelier-workspace-directory workspace))))
        (atelier-restore-workspace workspace)
        (atelier-notify-change)))))

(atelier-define-operation atelier-create-workspace ()
    (list (atelier-current-workspace-id)) nil
  (interactive)
  (atelier-create-workspace-targets (atelier-read-workspace-target t)))

(atelier-define-operation atelier-add-workspaces (directories)
    (list (atelier-current-workspace-id)) nil
  "Add workspaces from Emacs DIRECTORY paths, independently of a file interface.
Ask for each name and select the last workspace.  The entire addition is one
prepared operation, so cancellation or failure publishes no partial additions."
  (unless directories (user-error "Choose a directory"))
  (dolist (directory directories)
    (unless (file-directory-p directory)
      (user-error "Not a directory: %s" directory)))
  (atelier-create-workspace-targets
   (mapcar atelier-directory-target-function (delete-dups (copy-sequence directories)))))

(defun atelier-create-workspace-targets (targets)
  "Prepare TARGETS within the caller's workspace operation."
  (atelier-capture-current-workspace)
  (dolist (target targets)
    (pcase-let* ((`(,destination ,path ,platform ,mount-root) target)
                 (suggested-name
                  (atelier-safe-name (file-name-nondirectory (directory-file-name path))))
                 (suggestion (if (string-empty-p suggested-name) "home" suggested-name))
                 (name (read-string "Workspace name: " suggestion)))
      (when (string-empty-p name) (user-error "Workspace name cannot be empty"))
      (when (atelier-workspace-get name) (user-error "Workspace already exists: %s" name))
      (let ((workspace (list :id (atelier-new-workspace-id)
                             :name name :destination destination :path path
                             :platform platform :mount-root mount-root
                             :created (float-time) :status 'running
                             :entries nil)))
        (setq atelier-workspaces (append atelier-workspaces (list workspace)))
        (atelier-operation-notify 'atelier-workspace-created-hook workspace)
        (atelier-select-workspace workspace)
        (delete-other-windows)
        (switch-to-buffer (funcall atelier-workspace-start-function workspace))
        (atelier-notify-change)))))

(defun atelier-unique-workspace-name (root)
  (let* ((base (or (atelier-safe-name
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
     ((equal destination "local") (atelier-normalize-directory path))
     ((eq (plist-get workspace :platform) 'windows) nil)
     ((eq (plist-get workspace :platform) 'wsl)
      (atelier-normalize-directory (funcall atelier-directory-function workspace)))
     (t (atelier-normalize-directory (format "/ssh:%s:%s" destination path))))))

(defun atelier-known-project-roots ()
  (cl-remove-if
   (lambda (root)
     (and (boundp 'package-user-dir)
          (file-in-directory-p root package-user-dir)))
   (project-known-project-roots)))

(atelier-define-operation atelier-open-project-workspace (root)
    (delete-dups
     (list (atelier-current-workspace-id)
           (when-let* ((workspace (cl-find-if
                                  (lambda (item)
                                    (equal (atelier-workspace-project-root item)
                                           (atelier-normalize-directory root)))
                                  atelier-workspaces)))
             (atelier-workspace-id workspace)))) nil
  (let* ((root (file-name-as-directory root))
         (existing
          (cl-find-if
           (lambda (workspace)
             (equal (atelier-workspace-project-root workspace)
                    (atelier-normalize-directory root)))
           atelier-workspaces)))
    (if existing
        (atelier-switch-workspace (plist-get existing :name))
      (unless (file-directory-p root)
        (user-error "Project directory does not exist: %s" root))
      (pcase-let* ((`(,destination ,path ,platform ,mount-root)
                    (funcall atelier-directory-target-function root))
                   (name (atelier-unique-workspace-name root))
                   (workspace
                    (list :id (atelier-new-workspace-id) :name name
                          :destination destination :path path :platform platform
                          :mount-root mount-root :created (float-time) :status 'running
                          :entries nil)))
        (atelier-capture-current-workspace)
        (setq atelier-workspaces (append atelier-workspaces (list workspace)))
        (atelier-operation-notify 'atelier-workspace-created-hook workspace)
        (atelier-select-workspace workspace)
        (delete-other-windows)
        (switch-to-buffer (funcall atelier-workspace-start-function workspace))
        (atelier-notify-change)))))

(defun atelier-current-entry (&optional buffer workspace)
  "Return BUFFER's entry in WORKSPACE.
BUFFER defaults to the current buffer and WORKSPACE to the current frame's
workspace."
  (let* ((workspace (or workspace (atelier-current-workspace)))
         (buffer (or buffer (current-buffer)))
         (view (when-let* ((id (window-parameter nil 'atelier-view-id)))
                 (atelier-entry-by-id workspace id))))
    (if (and view (or (eq buffer (atelier-entry-live-buffer view))
                     (and (not (plist-get view :content-id)) (eq buffer (window-buffer))))) view
      (atelier-workspace-entry-for-buffer workspace buffer))))

(atelier-define-operation atelier-open-entry (entry &optional workspace)
    (delete-dups (list (atelier-current-workspace-id)
                       (atelier-workspace-id (or workspace (atelier-entry-workspace entry)))))
    ((workspace (atelier-operation-workspace (or workspace (atelier-entry-workspace entry))))
     (entry (atelier-operation-entry workspace entry)))
  "Open ENTRY from WORKSPACE in the selected window.
Interactively, choose an entry from the current workspace."
  (interactive
   (let* ((workspace (or (atelier-current-workspace)
                         (user-error "No workspace is selected")))
          (choices
           (mapcar (lambda (entry)
                     (cons (format "%s  [%s]"
                                   (or (atelier-entry-value entry :name) "Untitled")
                                   (atelier-entry-value entry :kind))
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
    (atelier--open-workspace workspace (selected-frame)))
  (if-let* ((buffer (or (atelier-entry-live-buffer entry)
                        (atelier-restore-buffer entry workspace))))
      (progn
        (atelier-show-buffer buffer workspace)
        buffer)
    nil))

(atelier-define-operation atelier-move-current-entry (workspace-name)
    (list (atelier-current-workspace-id)
          (atelier-workspace-id (or (atelier-workspace-get workspace-name)
                                    (user-error "Unknown workspace: %s" workspace-name)))) nil
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

(defun atelier-open-workspace (workspace &optional frame)
  "Prepare opening WORKSPACE, then publish its records and frame selection."
  (let ((id (atelier-workspace-id workspace))
        (frame (or frame (selected-frame))))
    (atelier-operation-call
     'open-workspace (list id)
     (lambda () (atelier--open-workspace (atelier-operation-workspace id) frame)) t
     (lambda () (atelier-open-workspace (atelier-operation-workspace id) frame)))))

(defun atelier--open-workspace (workspace frame)
  "Prepare WORKSPACE's opening inside the owning core operation."
  (with-selected-frame frame
    (atelier-operation-touch-frame frame)
    (let ((directory (atelier-workspace-directory workspace)))
      (unless (file-directory-p directory)
        (user-error "Workspace root is unavailable: %s" directory)))
    (atelier-select-workspace workspace frame)
    (unless (eq (atelier-workspace-status workspace) 'running)
      (when (fboundp 'atelier-restart-saved-jobs)
        (atelier-restart-saved-jobs workspace)))
    (atelier-restore-workspace workspace)
    (atelier-set-workspace-status workspace 'running)
    (atelier-operation-after #'atelier-track-inactive-workspaces)
    workspace))

(defun atelier-switch-workspace (name)
  (interactive (list (completing-read "Workspace: " (mapcar (lambda (w) (plist-get w :name)) atelier-workspaces) nil t)))
  (let* ((frame (selected-frame))
         (old-id (atelier-current-workspace-id frame))
         (workspace (atelier-workspace-get name)))
    (unless workspace (user-error "Unknown workspace: %s" name))
    (let ((id (atelier-workspace-id workspace)))
      (atelier-operation-call
       'switch-workspace (delete-dups (list old-id id))
       (lambda ()
         (let ((old-workspace (atelier-workspace-by-id old-id))
               (workspace (atelier-operation-workspace id)))
           (run-hook-with-args 'atelier-before-switch-workspace-hook
                               frame old-workspace workspace)
           (when old-workspace (atelier-capture-current-workspace))
           (atelier--open-workspace workspace frame)
           (atelier-operation-notify 'atelier-after-switch-workspace-hook
                                     frame old-workspace workspace)
           (atelier-notify-change))) (not (called-interactively-p 'any))
       (lambda () (atelier-switch-workspace (plist-get (atelier-operation-workspace id) :name)))))))

(atelier-define-operation atelier-rename-workspace ()
    (list (atelier-current-workspace-id)) nil
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
      (atelier-operation-notify 'atelier-workspace-renamed-hook workspace old new)
      (atelier-notify-change))))

(defun atelier-show-detached-workspace ()
  (let ((workspace (atelier-select-workspace nil)))
    (atelier-open-workspace workspace)))

(defcustom atelier-workspace-inactive-timeout nil
  "Seconds not current in any frame before stopping a workspace, or nil.
Stopping retains saved views and job recipes, not process-internal state."
  :type '(choice (const :tag "Disabled" nil) (number :tag "Seconds"))
  :group 'atelier)

(defvar atelier-workspace-inactive-since (make-hash-table :test #'equal)
  "Runtime-only times at which running workspaces ceased to be current.")
(defvar atelier-workspace-inactive-timer nil)
(defvar atelier-workspace-process-buffers-functions nil
  "Functions called with a workspace to return attached process buffers.
Optional integrations supply buffers without becoming Atelier dependencies.")

(defun atelier-workspace-current-p (workspace)
  "Whether WORKSPACE is current in any live frame."
  (cl-some (lambda (frame)
             (equal (atelier-workspace-id workspace)
                    (atelier-current-workspace-id frame)))
           (frame-list)))

(defun atelier-track-inactive-workspaces ()
  "Track time away from all frames, without treating process output as use."
  (let ((now (float-time)) ids)
    (dolist (workspace (atelier-user-workspaces))
      (let ((id (atelier-workspace-id workspace)))
        (push id ids)
        (if (or (not (eq (atelier-workspace-status workspace) 'running))
                (atelier-workspace-current-p workspace))
            (remhash id atelier-workspace-inactive-since)
          (unless (gethash id atelier-workspace-inactive-since)
            (puthash id now atelier-workspace-inactive-since)))))
    (dolist (id (hash-table-keys atelier-workspace-inactive-since))
      (unless (member id ids) (remhash id atelier-workspace-inactive-since)))))

(defun atelier-workspace-inactive-expired-p (workspace)
  "Whether WORKSPACE is still eligible for automatic stopping now."
  (and (numberp atelier-workspace-inactive-timeout)
       (> atelier-workspace-inactive-timeout 0)
       (not (atelier-detached-workspace-p workspace))
       (eq (atelier-workspace-status workspace) 'running)
       (not (atelier-workspace-current-p workspace))
       (when-let* ((since (gethash (atelier-workspace-id workspace)
                                  atelier-workspace-inactive-since)))
         (>= (- (float-time) since) atelier-workspace-inactive-timeout))))

(defun atelier-stop-inactive-workspaces ()
  "Stop expired workspaces through the same operation as manual stopping."
  (atelier-track-inactive-workspaces)
  (dolist (workspace (atelier-user-workspaces))
    (when (atelier-workspace-inactive-expired-p workspace)
      (condition-case error
          (atelier-stop-workspace workspace t)
        ((error quit)
         (atelier-log "Automatic stop of %s failed: %s"
                      (plist-get workspace :name) (error-message-string error)))))))

(defun atelier-workspace-live-content-buffers (workspace)
  "Return every live content buffer of WORKSPACE, including inactive stacks."
  (delete-dups
   (delq nil
         (mapcar (lambda (content)
                   (let ((buffer (gethash (atelier-content-cache-key
                                          workspace (plist-get content :id))
                                         atelier-content-live-buffers)))
                     (and (buffer-live-p buffer) buffer)))
                 (plist-get workspace :contents)))))

(defun atelier-buffer-shared-with-running-workspace-p (buffer workspace)
  "Whether another running workspace needs BUFFER."
  (cl-some (lambda (other)
             (and (not (equal (atelier-workspace-id other)
                              (atelier-workspace-id workspace)))
                  (eq (atelier-workspace-status other) 'running)
                  (memq buffer (atelier-workspace-live-content-buffers other))))
           atelier-workspaces))

(atelier-define-operation atelier-stop-workspace (workspace &optional inactive-only)
    (let ((next (atelier-workspace-successor workspace)))
      (delete-dups (list (atelier-workspace-id workspace)
                         (if next (atelier-workspace-id next) atelier-detached-workspace-id))))
    ((workspace (atelier-operation-workspace workspace)))
  "Stop WORKSPACE's processes, preserving views, buffers, and restart recipes.
INACTIVE-ONLY rechecks the timeout when an automatic request was queued."
  (interactive (list (atelier-current-workspace)))
  (when (atelier-detached-workspace-p workspace)
    (user-error "The Detached workspace cannot be stopped"))
  (when (and (eq (atelier-workspace-status workspace) 'running)
             (or (not inactive-only)
                 (atelier-workspace-inactive-expired-p workspace)))
    (dolist (frame (frame-list))
      (when (equal (atelier-current-workspace-id frame) (atelier-workspace-id workspace))
        (with-selected-frame frame (atelier-capture-current-workspace))))
    (let* ((buffers (atelier-workspace-live-content-buffers workspace))
           (attached (delete-dups
                      (cl-loop for function in atelier-workspace-process-buffers-functions
                               append (funcall function workspace)))))
      (atelier-workspace-stop-jobs workspace nil t)
      (dolist (buffer attached)
        (when (buffer-live-p buffer)
          (atelier-prepare-buffer-close buffer)
          (atelier-kill-buffer buffer)))
      (dolist (buffer buffers)
        (unless (atelier-buffer-shared-with-running-workspace-p buffer workspace)
          (when-let* ((process (and (buffer-live-p buffer) (get-buffer-process buffer)))
                      ((process-live-p process)))
            (atelier-operation-after
             (lambda ()
               (when (process-live-p process)
                 (set-process-query-on-exit-flag process nil)
                 (delete-process process))))))))
    (atelier-set-workspace-status workspace 'stopped)
    (let ((old (copy-tree workspace)))
      (atelier-operation-after (lambda () (funcall atelier-release-function old))))
    (let ((next (atelier-workspace-successor workspace)))
      (dolist (frame (frame-list))
        (when (equal (atelier-current-workspace-id frame) (atelier-workspace-id workspace))
          (with-selected-frame frame
            (if next (atelier-open-workspace next frame)
              (atelier-show-detached-workspace))))))
    (atelier-operation-after
     (lambda ()
       (atelier-track-inactive-workspaces)
       (dolist (frame (frame-list))
         (when (atelier-frame-covered-p frame)
           (run-hook-with-args 'atelier-covered-frame-refresh-functions frame)))))
    (atelier-notify-change)))

(atelier-define-operation atelier-close-workspace (&optional confirmed)
    (let* ((workspace (atelier-current-workspace))
           (next (atelier-workspace-successor workspace)))
      (delete-dups (list (atelier-workspace-id workspace)
                         (if next (atelier-workspace-id next) atelier-detached-workspace-id)))) nil
  (interactive)
  (let ((workspace (atelier-current-workspace)))
    (when (atelier-detached-workspace-p workspace)
      (user-error "The Detached workspace cannot be closed"))
    (unless (or confirmed
                (y-or-n-p (format "Close workspace %s and stop its jobs? "
                                  (plist-get workspace :name))))
      (user-error "Cancelled"))
    (atelier-stop-workspace workspace)))

(atelier-define-operation atelier-delete-workspace-record (workspace)
    (let ((next (atelier-workspace-successor workspace t)))
      (delete-dups (list (atelier-workspace-id workspace)
                         (if next (atelier-workspace-id next) atelier-detached-workspace-id))))
    ((workspace (atelier-operation-workspace workspace)))
  (when (atelier-detached-workspace-p workspace)
    (user-error "The Detached workspace cannot be deleted"))
  (let ((id (atelier-workspace-id workspace)))
    (atelier-workspace-stop-jobs workspace t)
    (let ((old (copy-tree workspace)))
      (atelier-operation-after (lambda () (funcall atelier-release-function old))))
    (setq atelier-workspaces (delq workspace atelier-workspaces))
    (dolist (frame (frame-list))
      (when (equal (atelier-current-workspace-id frame) id)
        (with-selected-frame frame
          (if-let* ((next (car atelier-workspaces)))
              (progn
                (atelier-open-workspace next frame))
            (atelier-show-detached-workspace)))))
    (atelier-operation-notify 'atelier-workspace-deleted-hook workspace)
    (atelier-notify-change)))

(defun atelier-delete-workspace (&optional confirmed)
  (interactive)
  (let ((workspace (atelier-current-workspace)))
    (unless (or confirmed
                (yes-or-no-p (format "Delete workspace definition %s? "
                                     (plist-get workspace :name))))
      (user-error "Cancelled"))
    (atelier-delete-workspace-record workspace)))

(atelier-define-operation atelier-split-right (&optional buffer)
    (list (atelier-current-workspace-id)) nil
  "Split the current view, opening a view on its right.
The new view shows BUFFER, or else the current content."
  (interactive)
  (atelier-split-view 'right buffer))

(defun atelier-split-view (side &optional buffer)
  "Create a view on SIDE, displaying BUFFER or else the current content.
No content is created."
  (atelier-capture-current-workspace)
  (let ((buffer (or buffer (window-buffer)))
        (window (split-window nil nil side)))
    (set-window-parameter window 'atelier-view-id nil)
    (select-window window)
    (when (atelier-buffer-registerable-p buffer (atelier-current-workspace))
      (atelier-show-buffer buffer nil window))
    (atelier-capture-current-workspace)
    (atelier-notify-change)
    window))

(atelier-define-operation atelier-split-below ()
    (list (atelier-current-workspace-id)) nil
  (interactive)
  (atelier-split-view 'below))

(atelier-define-operation atelier-close-split ()
    (list (atelier-current-workspace-id)) nil
  (interactive)
  (if (one-window-p) (user-error "The workspace has only one split")
    (delete-window)
    (atelier-notify-change)))

(atelier-define-operation atelier-resize-split (direction amount)
    (list (atelier-current-workspace-id)) nil
  (interactive)
  (pcase direction
    ('left (shrink-window-horizontally amount))
    ('right (enlarge-window-horizontally amount))
    ('up (shrink-window amount))
    ('down (enlarge-window amount)))
  (atelier-notify-change))

(atelier-define-operation atelier-move-workspace (delta)
    (list :all) nil
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

(defun atelier-create-default ()
  (let* ((root (atelier-project-root))
         (name (or (atelier-safe-name (file-name-nondirectory (directory-file-name root))) "home")))
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
  (when (and (frame-live-p frame) (display-graphic-p frame))
    (with-selected-frame frame
      (atelier-capture-current-workspace)
      (atelier-notify-change))))

(defun atelier-restore-new-frame (frame)
  (when (and (frame-live-p frame) (display-graphic-p frame) atelier-workspaces)
    (with-selected-frame frame
      (let ((workspace (or (atelier-current-workspace) (car atelier-workspaces))))
        (atelier-open-workspace workspace frame)))))

(defun atelier-setup ()
  (unless atelier-workspaces (atelier-create-default))
  (atelier-ensure-detached-workspace)
  (unless (atelier-current-workspace)
    (atelier-select-workspace (car atelier-workspaces)))
  (dolist (name atelier-global-buffer-names)
    (when-let* ((buffer (get-buffer name)))
      (atelier-mark-internal-buffer buffer)))
  ;; Emacs creates *scratch* before Atelier knows which workspace to restore.
  ;; It is a bootstrap buffer, never evidence of workspace ownership.
  (when-let* ((scratch (get-buffer "*scratch*")))
    (atelier-mark-internal-buffer scratch))
  (add-hook 'window-configuration-change-hook #'atelier-notify-change)
  (add-hook 'window-buffer-change-functions #'atelier-record-changed-windows)
  (add-hook 'after-rename-buffer-hook #'atelier-refresh-current-buffer-entries)
  (add-hook 'kill-buffer-hook #'atelier-current-buffer-killed)
  (add-hook 'delete-frame-functions #'atelier-capture-closing-frame)
  (add-hook 'after-make-frame-functions #'atelier-restore-new-frame)
  (add-hook 'atelier-change-hook #'atelier-track-inactive-workspaces)
  (atelier-naming-setup)
  (setq atelier-read-choice-function #'atelier-read-buffer-choice)
  (atelier-navigator-setup)
  (atelier-track-inactive-workspaces)
  (when (timerp atelier-workspace-inactive-timer)
    (cancel-timer atelier-workspace-inactive-timer))
  (setq atelier-workspace-inactive-timer
        (run-with-timer 60 60 #'atelier-stop-inactive-workspaces))
  nil)

;; UI modules depend on the complete service layer above, while the service
;; layer only calls their commands at runtime.
(require 'atelier-views)
(require 'atelier-choice)
(require 'atelier-navigator)
(require 'atelier-naming)
(require 'atelier-traveller)
(defalias 'atelier-close-current-entry #'atelier-close-current-view
  "Close the current workspace entry and its live buffer.")

(provide 'atelier)
;;; atelier.el ends here

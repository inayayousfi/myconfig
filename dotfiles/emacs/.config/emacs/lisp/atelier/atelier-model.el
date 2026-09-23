;;; atelier-model.el --- Atelier workspace and entry model -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'subr-x)

(defconst atelier-detached-workspace-id "atelier-detached")
(defconst atelier-detached-workspace-name "Detached")

(defvar atelier-workspaces nil)
(defvar atelier-remembered-ssh-destinations nil)
(defvar atelier-job-owner-workspace nil)
(defvar atelier-job-owner-entry nil)
(defvar atelier-preserve-job-recipe nil)
(defvar atelier-inhibit-entry-removed-hook nil)
(defvar atelier-navigator-window-configurations nil)
(defvar atelier-agent-restored-functions nil)
(defvar atelier-directory-choice-result nil)
(defvar atelier-directory-chooser-active nil)
(defvar atelier-directory-chooser-buffers nil)
(defvar atelier-choice-result nil)
(defvar atelier-navigator-attach-source nil)
(defvar atelier-inhibit-buffer-ownership nil)
(defvar atelier-evil-mode-line-anchor nil)
(defvar atelier-directory-chooser-mode nil)
(defvar atelier-internal-buffers (make-hash-table :test #'eq :weakness 'key))
(defconst atelier-navigator-buffer "*Atelier*")
(defconst atelier-choice-buffer "*Atelier choice*")
(defconst atelier-empty-buffer-prefix "*Atelier empty:")
(defconst atelier-global-buffer-names
  '("*Messages*" "*Warnings*" "*Completions*" "*Native-compile-Log*"))
(defvar atelier-entry-types
  '((file :buffer-name "file" :buffer-p atelier-file-entry-buffer-p)
    (dired :buffer-name "dired" :buffer-p atelier-dired-entry-buffer-p)
    (terminal :buffer-name "terminal" :buffer-p atelier-terminal-entry-buffer-p)
    (aipanel :buffer-name "aipanel"))
  "Registered workspace entry types and their shared behavior.")
(defvar-local atelier-navigator-first-position nil)
(defvar-local atelier-directory-chooser-original-header nil)
(defvar-local atelier-directory-chooser-header-was-local nil)
(defvar-local atelier-directory-chooser-original-modified nil)
(defvar atelier-change-hook nil
  "Hook run after a completed mutation of Atelier's public state.")
(defvar atelier-before-switch-workspace-hook nil
  "Hook run with FRAME, OLD-WORKSPACE and NEW-WORKSPACE before switching.")
(defvar atelier-after-switch-workspace-hook nil
  "Hook run with FRAME, OLD-WORKSPACE and NEW-WORKSPACE after switching.")
(defvar atelier-workspace-created-hook nil
  "Hook run with the newly created workspace.")
(defvar atelier-workspace-deleted-hook nil
  "Hook run with the workspace that was deleted.")
(defvar atelier-workspace-renamed-hook nil
  "Hook run with WORKSPACE, OLD-NAME and NEW-NAME.")
(defvar atelier-entry-added-hook nil
  "Hook run with WORKSPACE and ENTRY after an entry is added.")
(defvar atelier-entry-removed-hook nil
  "Hook run with WORKSPACE and ENTRY after an entry is removed.")
(defvar atelier-entry-moved-hook nil
  "Hook run with ENTRY, OLD-WORKSPACE and NEW-WORKSPACE after a move.")
(defvar atelier-entry-restored-hook nil
  "Hook run with WORKSPACE, ENTRY and BUFFER after restoration.")
(defvar atelier-entry-restore-failed-hook nil
  "Hook run with WORKSPACE, ENTRY and ERROR after restoration fails.")
(defvar atelier-before-save-hook nil
  "Hook run immediately before Atelier state is saved.")
(defvar atelier-after-save-hook nil
  "Hook run after Atelier state is saved successfully.")
(defvar atelier-after-restore-hook nil
  "Hook run after a complete persisted state has been restored.")

;; This table is a disposable runtime cache.  Persistent ownership exists only
;; in each workspace's recursive :entries tree.  Layout entries own child
;; entries, basic entries resolve to live buffers, and buffers carry no Atelier
;; metadata.
(defvar atelier-entry-live-buffers (make-hash-table :test #'equal))

(defun atelier-plist-set! (plist property value)
  "Set PROPERTY to VALUE in mutable PLIST without changing its identity."
  (if (plist-member plist property)
      (setf (plist-get plist property) value)
    (nconc plist (list property value)))
  value)

(defun atelier-plist-clear! (plist property)
  "Set PROPERTY to nil in PLIST when it is already present.
Unlike `plist-put', this always preserves PLIST's cons identity."
  (when-let* ((tail (memq property plist)))
    (setcar (cdr tail) nil))
  plist)

(defun atelier-new-workspace-id ()
  (format "workspace-%s-%06x" (float-time) (random #xffffff)))

(defun atelier-new-entry-id ()
  (format "entry-%s-%06x" (float-time) (random #xffffff)))

(defun atelier-workspace-id (workspace)
  (or (plist-get workspace :id)
      (let ((id (atelier-new-workspace-id)))
        (nconc workspace (list :id id))
        id)))

(defun atelier-workspace-get (name)
  (cl-find name atelier-workspaces
           :key (lambda (workspace) (plist-get workspace :name))
           :test #'equal))

(defun atelier-workspace-by-id (id)
  (and (stringp id)
       (cl-find id atelier-workspaces :key #'atelier-workspace-id :test #'equal)))

(defun atelier-detached-workspace-p (workspace)
  (equal (and workspace (atelier-workspace-id workspace))
         atelier-detached-workspace-id))

(defun atelier-detached-workspace ()
  (atelier-workspace-by-id atelier-detached-workspace-id))

(defun atelier-user-workspaces ()
  "Return ordinary workspaces, excluding the reserved Detached workspace."
  (cl-remove-if #'atelier-detached-workspace-p atelier-workspaces))

(defun atelier-ensure-detached-workspace ()
  "Return the canonical reserved Detached workspace, creating it if needed."
  (let ((workspace (atelier-detached-workspace)))
    (unless workspace
      ;; Preserve a pre-existing user workspace called "Detached" by giving it
      ;; an unambiguous ordinary name before reserving the display name.
      (when-let* ((collision (atelier-workspace-get atelier-detached-workspace-name)))
        (let ((index 2)
              (name (format "%s-2" atelier-detached-workspace-name)))
          (while (atelier-workspace-get name)
            (setq index (1+ index)
                  name (format "%s-%d" atelier-detached-workspace-name index)))
          (setf (plist-get collision :name) name)))
      (setq workspace
            (list :id atelier-detached-workspace-id
                  :name atelier-detached-workspace-name
                   :destination "local"
                   :path (file-name-as-directory (expand-file-name "~/"))
                   :platform 'local :mount-root nil :created 0
                  :status 'running :entries nil :reserved t)
            atelier-workspaces (cons workspace atelier-workspaces)))
    ;; These fields are invariants, not user-editable workspace settings.
    (setf (plist-get workspace :name) atelier-detached-workspace-name
          (plist-get workspace :destination) "local"
          (plist-get workspace :path) (file-name-as-directory (expand-file-name "~/"))
          (plist-get workspace :platform) 'local
          (plist-get workspace :mount-root) nil
          (plist-get workspace :status) 'running
          (plist-get workspace :reserved) t)
    workspace))

(defun atelier-current-workspace-id (&optional frame)
  "Return the workspace ID selected by FRAME.
Frames without an explicit selection belong to the reserved Detached workspace."
  (or (frame-parameter (or frame (selected-frame)) 'atelier-workspace-id)
      atelier-detached-workspace-id))

(defun atelier-current-workspace (&optional frame)
  "Return the workspace selected by FRAME.

This is the public query for integrations which need the current Atelier
context.  A frame points at a workspace; buffers carry no workspace owner."
  (or (atelier-workspace-by-id (atelier-current-workspace-id frame))
      (atelier-select-workspace (atelier-ensure-detached-workspace) frame)))

(defun atelier-show-current-workspace (&optional frame)
  "Display and return the workspace selected by FRAME."
  (interactive)
  (let ((workspace (atelier-current-workspace frame)))
    (if workspace
        (message "Current workspace: %s (%s)"
                 (plist-get workspace :name) (atelier-workspace-id workspace))
      (message "Current workspace: none"))
    workspace))

(defun atelier-select-workspace (workspace &optional frame)
  "Make WORKSPACE the context of FRAME and return WORKSPACE.
Nil selects the reserved Detached workspace."
  (setq workspace (or workspace (atelier-ensure-detached-workspace)))
  (set-frame-parameter (or frame (selected-frame)) 'atelier-workspace-id
                       (atelier-workspace-id workspace))
  workspace)

(defun atelier-workspace-status (workspace)
  (or (plist-get workspace :status)
      (if (plist-get workspace :live) 'running 'stopped)))

(defun atelier-set-workspace-status (workspace status)
  (atelier-plist-set! workspace :status status)
  (cl-remf workspace :live)
  status)

(defun atelier-layout-entry-p (entry)
  "Return non-nil when ENTRY is an internal split-layout entry."
  (eq (plist-get entry :kind) 'layout))

(defun atelier-entry-children (entry)
  "Return ENTRY's direct children when it is a layout entry."
  (and (atelier-layout-entry-p entry) (plist-get entry :children)))

(defun atelier-entry-leaves (entry)
  "Return the basic entries recursively owned by ENTRY."
  (if (atelier-layout-entry-p entry)
      (cl-mapcan #'atelier-entry-leaves (atelier-entry-children entry))
    (list entry)))

(defun atelier-workspace-top-level-entries (workspace)
  "Return the entries directly owned by WORKSPACE."
  (plist-get workspace :entries))

(defun atelier-workspace-entries (workspace)
  "Return all basic entries recursively owned by WORKSPACE."
  (cl-mapcan #'atelier-entry-leaves
             (atelier-workspace-top-level-entries workspace)))

(defun atelier-entry-type-definition (type)
  "Return the registered definition for TYPE."
  (or (assq type atelier-entry-types)
      (error "Unknown Atelier entry type: %s" type)))

(defun atelier-register-entry-type (type buffer-name &optional buffer-p)
  "Register TYPE with BUFFER-NAME and optional BUFFER-P predicate.
BUFFER-P receives a live buffer and identifies automatic registrations of TYPE."
  (unless (and (symbolp type) (stringp buffer-name)
               (not (string-empty-p buffer-name)))
    (error "Invalid Atelier entry type registration: %S %S" type buffer-name))
  (let ((definition (list type :buffer-name buffer-name)))
    (when buffer-p
      (setq definition (append definition (list :buffer-p buffer-p))))
    (if-let* ((existing (assq type atelier-entry-types)))
        (setcdr existing (cdr definition))
      (setq atelier-entry-types (append atelier-entry-types (list definition))))
    definition))

(defun atelier-entry-buffer-name (type workspace)
  "Return the canonical buffer name for TYPE in WORKSPACE."
  (let ((label (plist-get (cdr (atelier-entry-type-definition type)) :buffer-name)))
    (format "*%s:%s*" label (plist-get workspace :name))))

(defun atelier-workspace-entry-by-type (workspace type)
  "Return the newest entry of TYPE in WORKSPACE."
  (cl-find type (reverse (atelier-workspace-entries workspace))
           :key (lambda (entry) (plist-get entry :type))))

(defun atelier-workspace-buffer-by-type (workspace type)
  "Return the newest live buffer of TYPE in WORKSPACE."
  (cl-loop for entry in (reverse (atelier-workspace-entries workspace))
           when (eq (plist-get entry :type) type)
           thereis (atelier-entry-live-buffer entry)))

(defun atelier-workspace-displayed-entry (workspace)
  "Return WORKSPACE's top-level entry that describes its visible layout."
  (cl-find-if (lambda (entry) (plist-get entry :displayed))
              (atelier-workspace-top-level-entries workspace)))

(defun atelier-workspace-displayed-entries (workspace)
  "Return WORKSPACE's visible basic entries in window traversal order."
  (when-let* ((entry (atelier-workspace-displayed-entry workspace)))
    (atelier-entry-leaves entry)))

(defun atelier-entry-find (entry id)
  "Find ID in ENTRY's recursive ownership tree."
  (if (equal id (plist-get entry :id))
      entry
    (cl-loop for child in (atelier-entry-children entry)
             thereis (atelier-entry-find child id))))

(defun atelier-entry-by-id (workspace id)
  (cl-loop for entry in (atelier-workspace-top-level-entries workspace)
           thereis (atelier-entry-find entry id)))

(defun atelier-entry-workspace (entry-or-id)
  "Return the workspace containing ENTRY-OR-ID."
  (let ((id (if (stringp entry-or-id) entry-or-id
              (plist-get entry-or-id :id))))
    (cl-find-if (lambda (workspace) (atelier-entry-by-id workspace id))
                atelier-workspaces)))

(defun atelier-entry-live-buffer (entry-or-id)
  "Return ENTRY-OR-ID's live Emacs buffer, if any."
  (let* ((id (if (stringp entry-or-id) entry-or-id
               (plist-get entry-or-id :id)))
         (buffer (and id (gethash id atelier-entry-live-buffers))))
    (and (buffer-live-p buffer) buffer)))

(defun atelier-entry-set-live-buffer (entry buffer)
  "Associate ENTRY with BUFFER in the disposable runtime cache."
  (let ((id (plist-get entry :id)))
    (unless id (error "Entry has no stable ID"))
    (if (buffer-live-p buffer)
        (puthash id buffer atelier-entry-live-buffers)
      (remhash id atelier-entry-live-buffers))
    buffer))

(defun atelier-entries-for-buffer (buffer)
  "Return all (WORKSPACE ENTRY) pairs currently resolving to BUFFER."
  (let (matches)
    (dolist (workspace atelier-workspaces)
      (dolist (entry (atelier-workspace-entries workspace))
        (when (eq (atelier-entry-live-buffer entry) buffer)
          (push (list workspace entry) matches))))
    (nreverse matches)))

(defun atelier-entry-add (workspace entry &optional no-notify)
  "Add ENTRY to WORKSPACE, which becomes its sole persistent owner.
Entries of the same type form a stack in insertion order."
  (unless (plist-get entry :id)
    (setq entry (plist-put entry :id (atelier-new-entry-id))))
  (unless (atelier-entry-by-id workspace (plist-get entry :id))
    (atelier-plist-set!
     workspace :entries
     (append (atelier-workspace-top-level-entries workspace) (list entry)))
    (unless no-notify
      (run-hook-with-args 'atelier-entry-added-hook workspace entry)
      (run-hooks 'atelier-change-hook)))
  entry)

(defun atelier-entry-with-display-state (entry displayed)
  (if displayed
      (plist-put entry :displayed t)
    (atelier-plist-clear! entry :displayed)))

(defun atelier-entry-remove-from-tree (tree wanted-id)
  "Remove WANTED-ID from TREE, collapsing layouts with one child."
  (cond
   ((equal wanted-id (plist-get tree :id)) nil)
   ((not (atelier-layout-entry-p tree)) tree)
   (t
    (let* ((displayed (plist-get tree :displayed))
           (children (delq nil
                           (mapcar (lambda (child)
                                     (atelier-entry-remove-from-tree child wanted-id))
                                   (atelier-entry-children tree)))))
      (pcase (length children)
        (0 nil)
        (1 (atelier-entry-with-display-state (car children) displayed))
        (_ (setf (plist-get tree :children) children)
           tree))))))

(defun atelier-entry-remove (workspace entry &optional no-notify)
  "Remove ENTRY from WORKSPACE's recursive ownership tree."
  (let ((id (plist-get entry :id)))
    (atelier-plist-set!
     workspace :entries
     (delq nil
           (mapcar (lambda (tree) (atelier-entry-remove-from-tree tree id))
                   (atelier-workspace-top-level-entries workspace))))
    (dolist (leaf (atelier-entry-leaves entry))
      (remhash (plist-get leaf :id) atelier-entry-live-buffers)))
  (unless atelier-inhibit-entry-removed-hook
    (run-hook-with-args 'atelier-entry-removed-hook workspace entry))
  (unless no-notify
    (run-hooks 'atelier-change-hook))
  entry)

(defun atelier-entry-move (entry old-workspace new-workspace)
  "Move ENTRY from OLD-WORKSPACE to NEW-WORKSPACE atomically."
  (unless (eq old-workspace new-workspace)
    (let ((buffer (atelier-entry-live-buffer entry)))
      (let ((atelier-inhibit-entry-removed-hook t))
        (atelier-entry-remove old-workspace entry t))
      (atelier-plist-clear! entry :displayed)
      (atelier-plist-clear! entry :selected)
      (atelier-entry-add new-workspace entry t)
      (when buffer (atelier-entry-set-live-buffer entry buffer)))
    (run-hook-with-args 'atelier-entry-moved-hook
                        entry old-workspace new-workspace)
    (run-hooks 'atelier-change-hook))
  entry)

(defun atelier-entry-job (entry)
  "Return ENTRY's terminal restart job, if any."
  (plist-get entry :job))

(defun atelier-workspace-job-entries (workspace)
  "Return terminal entries in WORKSPACE which carry restart jobs."
  (cl-remove-if-not #'atelier-entry-job (atelier-workspace-entries workspace)))

(defun atelier-entry-persistent-copy (entry)
  "Return ENTRY's persistent recursive representation, or nil."
  (if (atelier-layout-entry-p entry)
      (let* ((copy (copy-tree entry))
             (children (delq nil (mapcar #'atelier-entry-persistent-copy
                                         (atelier-entry-children entry))))
             (displayed (plist-get entry :displayed)))
        (pcase (length children)
          (0 nil)
          (1 (atelier-entry-with-display-state (car children) displayed))
          (_ (setf (plist-get copy :children) children)
             copy)))
    (and (plist-get entry :persistent) (copy-tree entry))))

(defun atelier-workspace-persistent-copy (workspace)
  "Return WORKSPACE without runtime-only state."
  (let ((copy (copy-tree workspace)))
    (setf (plist-get copy :entries)
          (delq nil (mapcar #'atelier-entry-persistent-copy
                            (atelier-workspace-top-level-entries workspace))))
    (cl-remf copy :layout)
    (cl-remf copy :state)
    copy))

(provide 'atelier-model)
;;; atelier-model.el ends here

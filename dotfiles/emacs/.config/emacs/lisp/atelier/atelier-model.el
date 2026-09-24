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
(defvar atelier-navigator-selection-by-frame nil
  "Last selected navigator target per frame, restored after navigator refreshes.")
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

;; A workspace :entries list owns its top-level entry roots directly.  Its
;; displayed top-level layout entry is the disposition root; that root itself
;; is the disposition (there is no separate disposition node).  Other
;; top-level basic entries are unplaced content entries.  Layout entries own
;; recursive child entries; basic entries resolve to live buffers.  Entry :id
;; values identify workspace references, not buffers, so distinct entries may
;; reference the same live buffer.  Buffers carry no Atelier ownership
;; metadata.
;; Content records belong to workspaces.  Leaves contain only ordered IDs and
;; layout metadata; a content ID may be shared by several views in one workspace.
;; The cache is disposable and keyed by (workspace-id . content-id).
(defvar atelier-content-live-buffers (make-hash-table :test #'equal))
(defvar atelier-model-workspace nil
  "Dynamically bound owner when copying a workspace outside the live registry.")
(defvar atelier-entry-owners (make-hash-table :test #'eq :weakness 'key)
  "Disposable owners for entries in isolated runtime copies.")
(defconst atelier-content-properties
  '(:name :kind :type :persistent :file :directory :contents :point :start :job))

(defun atelier-entry-owner (entry)
  (or atelier-model-workspace (gethash entry atelier-entry-owners)
      (atelier-entry-workspace entry)
      (error "Entry %s has no workspace owner" (plist-get entry :id))))

(defun atelier-workspace-index-entries (workspace)
  "Remember the owner of WORKSPACE's runtime entries, including isolated copies."
  (cl-labels ((visit (entry)
                (puthash entry workspace atelier-entry-owners)
                (mapc #'visit (atelier-entry-children entry))))
    (mapc #'visit (atelier-workspace-top-level-entries workspace)))
  workspace)

(defun atelier-plist-remove! (plist property)
  "Remove PROPERTY from PLIST without replacing its head cons."
  (let ((tail plist) previous)
    (while (and tail (not (eq (car tail) property)))
      (setq previous (cdr tail) tail (cddr tail)))
    (when tail
      (if previous
          (setcdr previous (cddr tail))
        ;; Normalization adds :content-ids before removing inline fields.
        (setcar plist (caddr plist))
        (setcdr plist (cdddr plist)))))
  plist)

(defun atelier-entry-ensure-content (entry workspace)
  "Move an inline leaf and its inactive stack into WORKSPACE's content records.
Transfer disposable buffers from legacy entry/content keys before dropping them."
  (unless (atelier-layout-entry-p entry)
    (let* ((entry-id (plist-get entry :id))
           (stack (plist-get entry :stack))
           (new-ids nil))
      (unless (plist-get entry :content-ids)
        (let* ((active (cl-loop for (key value) on entry by #'cddr
                                when (memq key atelier-content-properties)
                                append (list key value)))
               (old-id (plist-get entry :content-id))
               (new-id (atelier-workspace-store-content
                        workspace (append (list :content-id old-id) active)))
               (buffer (or (gethash (cons entry-id old-id)
                                    atelier-content-live-buffers)
                           (gethash (cons :unowned entry-id)
                                    atelier-content-live-buffers)
                           (and (boundp 'atelier-entry-live-buffers)
                                (hash-table-p atelier-entry-live-buffers)
                                (gethash entry-id atelier-entry-live-buffers)))))
          (push new-id new-ids)
          (when (buffer-live-p buffer)
            (puthash (atelier-content-cache-key workspace new-id) buffer
                     atelier-content-live-buffers))
          (remhash (cons :unowned entry-id) atelier-content-live-buffers)
          (remhash (cons entry-id old-id) atelier-content-live-buffers)
          (dolist (key atelier-content-properties)
            (atelier-plist-remove! entry key))
          (atelier-plist-remove! entry :content-id)))
      (dolist (item stack)
        (let* ((old-id (or (plist-get item :content-id) (plist-get item :id)))
               (id (atelier-workspace-store-content workspace item))
               (buffer (gethash (cons entry-id old-id)
                                atelier-content-live-buffers)))
          (push id new-ids)
          (when (buffer-live-p buffer)
            (puthash (atelier-content-cache-key workspace id) buffer
                     atelier-content-live-buffers))
          (remhash (cons entry-id old-id) atelier-content-live-buffers)))
      (when new-ids
        (atelier-plist-set! entry :content-ids
                           (append (plist-get entry :content-ids) (nreverse new-ids))))
      (atelier-plist-remove! entry :stack)))
  entry)

(defun atelier-workspace-content (workspace id)
  (cl-find id (plist-get workspace :contents)
           :key (lambda (content) (plist-get content :id)) :test #'equal))

(defun atelier-entry-content (entry &optional workspace)
  "Return ENTRY's active workspace-owned content record (mutable), or nil."
  (unless (atelier-layout-entry-p entry)
    (let ((workspace (or workspace (atelier-entry-owner entry))))
      (atelier-entry-ensure-content entry workspace)
      (atelier-workspace-content workspace (car (plist-get entry :content-ids))))))

(defun atelier-entry-value (entry property &optional workspace)
  "Read PROPERTY from ENTRY's active content (layout properties stay on ENTRY)."
  (plist-get (if (atelier-layout-entry-p entry) entry
               (atelier-entry-content entry workspace)) property))

(defun atelier-entry-set-value (entry property value &optional workspace)
  "Set PROPERTY on ENTRY's active content, preserving record identity."
  (unless (memq property atelier-content-properties)
    (error "Not a content property: %S" property))
  (let ((content (atelier-entry-content entry workspace)))
    (unless content (error "Entry has no active content"))
    (atelier-plist-set! content property value)))

(defun atelier-entry-stack (entry &optional workspace)
  "Return ENTRY's ordered content records, active first; records are mutable."
  (unless (atelier-layout-entry-p entry)
    (let ((workspace (or workspace (atelier-entry-owner entry))))
      (atelier-entry-ensure-content entry workspace)
      (mapcar (lambda (id) (or (atelier-workspace-content workspace id)
                               (error "Missing content %s" id)))
              (plist-get entry :content-ids)))))

(defun atelier-content-new-id ()
  (format "content-%s" (atelier-new-entry-id)))

(defun atelier-content-cache-key (workspace id)
  (cons (atelier-workspace-id workspace) id))

(defun atelier-workspace-store-content (workspace content)
  "Store CONTENT in WORKSPACE, returning its ID; copy on ID collision."
  (let* ((copy (copy-tree content))
         (id (or (plist-get copy :id) (plist-get copy :content-id)
                 (atelier-content-new-id))))
    (while (atelier-workspace-content workspace id)
      (setq id (atelier-content-new-id)))
    (cl-remf copy :content-id)
    (atelier-plist-set! copy :id id)
    (atelier-plist-set! workspace :contents
                       (append (plist-get workspace :contents) (list copy)))
    id))

(defun atelier-entry-replace-content (entry content)
  "Replace ENTRY's active content in its owning workspace."
  (let* ((workspace (atelier-entry-owner entry))
         (old (car (plist-get entry :content-ids)))
         (id (atelier-workspace-store-content workspace content)))
    (atelier-plist-set! entry :content-ids
                       (cons id (cdr (plist-get entry :content-ids))))
    (unless (cl-some (lambda (leaf) (member old (plist-get leaf :content-ids)))
                     (atelier-workspace-entries workspace))
      (atelier-workspace-drop-content workspace old))
    entry))

(defun atelier-workspace-drop-content (workspace id)
  (when id
    (atelier-plist-set! workspace :contents
                       (cl-remove id (plist-get workspace :contents)
                                  :key (lambda (content) (plist-get content :id))
                                  :test #'equal))
    (remhash (atelier-content-cache-key workspace id)
             atelier-content-live-buffers)))

(defun atelier-entry-push-content (entry content &optional buffer)
  "Put CONTENT first in ENTRY's stack; jobs cannot be stacked."
  (when (or (atelier-layout-entry-p entry) (atelier-entry-job entry)
            (plist-get content :job)
            (memq (atelier-entry-value entry :type) '(terminal aipanel))
            (memq (plist-get content :type) '(terminal aipanel)))
    (error "Only ordinary content may be stacked; jobs own their entry"))
  (let* ((workspace (atelier-entry-owner entry))
         (id (atelier-workspace-store-content workspace content)))
    (atelier-plist-set! entry :content-ids (cons id (plist-get entry :content-ids)))
    (atelier-entry-set-live-buffer entry buffer)
    entry))

(defun atelier-entry-activate-content (entry content-id)
  "Rotate ENTRY so CONTENT-ID becomes active without changing its view identity."
  (let* ((ids (plist-get entry :content-ids))
         (index (cl-position content-id ids :test #'equal)))
    (when (and index (> index 0))
      (atelier-plist-set! entry :content-ids
                         (append (nthcdr index ids) (cl-subseq ids 0 index)))
      entry)))

(defun atelier-entry-activate-buffer (entry buffer)
  "Activate ENTRY's content corresponding to BUFFER, if inactive."
  (let ((workspace (atelier-entry-owner entry)))
    (cl-loop for id in (cdr (plist-get entry :content-ids))
             when (eq buffer (gethash (atelier-content-cache-key workspace id)
                                      atelier-content-live-buffers))
             return (atelier-entry-activate-content entry id))))

(defun atelier-entry-pop-content (entry)
  "Remove active content and reveal the next one; return removed record."
  (when (cdr (plist-get entry :content-ids))
    (let* ((workspace (atelier-entry-owner entry))
           (removed (atelier-entry-content entry workspace))
           (id (car (plist-get entry :content-ids))))
      (atelier-plist-set! entry :content-ids (cdr (plist-get entry :content-ids)))
      (unless (cl-some (lambda (leaf) (member id (plist-get leaf :content-ids)))
                       (atelier-workspace-entries workspace))
        (atelier-workspace-drop-content workspace id))
      removed)))

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

(defun atelier-workspace-refresh-parent-ids (workspace)
  "Rebuild runtime parent-ID links throughout WORKSPACE's entry trees."
  (cl-labels ((visit (entry parent-id)
                (setf (plist-get entry :parent-id) parent-id)
                (dolist (child (atelier-entry-children entry))
                  (visit child (plist-get entry :id)))))
    (dolist (root (atelier-workspace-top-level-entries workspace)
                   workspace)
      (visit root nil))))

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
           :key (lambda (entry) (atelier-entry-value entry :type workspace))))

(defun atelier-workspace-buffer-by-type (workspace type)
  "Return the newest live buffer of TYPE in WORKSPACE."
  (cl-loop for entry in (reverse (atelier-workspace-entries workspace))
           when (eq (atelier-entry-value entry :type workspace) type)
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

(defun atelier-workspace-entry-root (workspace entry-or-id)
  "Return the root under WORKSPACE that contains ENTRY-OR-ID.

This top-level entry is the disposition to materialize when an entry nested
inside its recursive split tree is selected.  Root-ness is structural, not a
separate entry kind."
  (let ((id (if (stringp entry-or-id)
                entry-or-id
              (plist-get entry-or-id :id))))
    (cl-find-if (lambda (root) (atelier-entry-find root id))
                (atelier-workspace-top-level-entries workspace))))

(defun atelier-entry-workspace (entry-or-id)
  "Return the workspace containing ENTRY-OR-ID."
  (let ((id (if (stringp entry-or-id) entry-or-id
              (plist-get entry-or-id :id))))
    (cl-find-if (lambda (workspace) (atelier-entry-by-id workspace id))
                atelier-workspaces)))

(defun atelier-entry-live-buffer (entry)
  "Return the active content's live buffer, if any."
  (when (and (listp entry) (not (atelier-layout-entry-p entry)))
    (let* ((workspace (or atelier-model-workspace (gethash entry atelier-entry-owners)
                          (atelier-entry-workspace entry)))
           (_ (when workspace (atelier-entry-ensure-content entry workspace)))
           (id (car (plist-get entry :content-ids)))
           (buffer (gethash (if workspace
                                (atelier-content-cache-key workspace id)
                              (cons :unowned (plist-get entry :id)))
                            atelier-content-live-buffers)))
      (and (buffer-live-p buffer) buffer))))

(defun atelier-entry-set-live-buffer (entry buffer)
  "Cache BUFFER by content ID, deferring isolated entries until given an owner."
  (let* ((workspace (or atelier-model-workspace (gethash entry atelier-entry-owners)
                          (atelier-entry-workspace entry)))
         (_ (when workspace (atelier-entry-ensure-content entry workspace)))
         (id (car (plist-get entry :content-ids)))
         (key (if workspace
                  (progn (unless id (error "Entry has no active content ID"))
                         (atelier-content-cache-key workspace id))
                (cons :unowned (plist-get entry :id)))))
    (if (buffer-live-p buffer)
        (puthash key buffer atelier-content-live-buffers)
      (unless (and workspace
                   (cl-some (lambda (other)
                              (and (not (eq other entry))
                                   (member id (plist-get other :content-ids))))
                            (atelier-workspace-entries workspace)))
        (remhash key atelier-content-live-buffers)))
    buffer))

(defun atelier-entry-inactive-buffer-p (entry buffer)
  "Whether BUFFER is retained below the active content of ENTRY."
  (let ((workspace (atelier-entry-owner entry)))
    (cl-some (lambda (id)
               (eq buffer (gethash (atelier-content-cache-key workspace id)
                                   atelier-content-live-buffers)))
             (cdr (plist-get entry :content-ids)))))

(defun atelier-buffer-referenced-p (buffer)
  "Return non-nil when any entry still owns BUFFER, active or inactive."
  (cl-some (lambda (workspace)
             (cl-some (lambda (entry)
                        (or (eq buffer (atelier-entry-live-buffer entry))
                            (atelier-entry-inactive-buffer-p entry buffer)))
                      (atelier-workspace-entries workspace)))
           atelier-workspaces))

(defun atelier-entries-for-buffer (buffer)
  "Return all (WORKSPACE ENTRY) pairs whose active content resolves to BUFFER."
  (let (matches)
    (dolist (workspace atelier-workspaces)
      (dolist (entry (atelier-workspace-entries workspace))
        (when (eq (atelier-entry-live-buffer entry) buffer)
          (push (list workspace entry) matches))))
    (nreverse matches)))

(defun atelier-entry-add (workspace entry &optional no-notify)
  "Add ENTRY to WORKSPACE, converting legacy inline content at this boundary."
  (unless (plist-get entry :id)
    (setq entry (plist-put entry :id (atelier-new-entry-id))))
  (unless (atelier-entry-by-id workspace (plist-get entry :id))
    (dolist (leaf (atelier-entry-leaves entry))
      (atelier-entry-ensure-content leaf workspace))
    (atelier-plist-set! workspace :entries
                       (append (atelier-workspace-top-level-entries workspace)
                               (list entry)))
    (atelier-workspace-refresh-parent-ids workspace)
    (atelier-workspace-index-entries workspace)
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
    (atelier-workspace-refresh-parent-ids workspace)
    (dolist (leaf (atelier-entry-leaves entry))
      (dolist (content-id (plist-get leaf :content-ids))
        (unless (cl-some (lambda (remaining)
                           (member content-id (plist-get remaining :content-ids)))
                         (atelier-workspace-entries workspace))
          (atelier-workspace-drop-content workspace content-id)))))
  (unless atelier-inhibit-entry-removed-hook
    (run-hook-with-args 'atelier-entry-removed-hook workspace entry))
  (unless no-notify
    (run-hooks 'atelier-change-hook))
  entry)

(defun atelier-entry-move (entry old-workspace new-workspace)
  "Move ENTRY and its content records to NEW-WORKSPACE atomically."
  (unless (eq old-workspace new-workspace)
    (let ((moved (make-hash-table :test #'equal)) buffers replacements)
      ;; Copy while the old IDs still belong to ENTRY.  Removal must see those
      ;; IDs so it can drop unreferenced old records and cache entries.
      (dolist (leaf (atelier-entry-leaves entry))
        (atelier-entry-ensure-content leaf old-workspace)
        (let (new-ids)
          (dolist (id (plist-get leaf :content-ids))
            (let* ((content (atelier-workspace-content old-workspace id))
                   (buffer (gethash (atelier-content-cache-key old-workspace id)
                                    atelier-content-live-buffers))
                   (new-id (or (gethash id moved)
                               (let ((created (atelier-workspace-store-content
                                               new-workspace content)))
                                 (puthash id created moved)
                                 created))))
              (push new-id new-ids)
              (when (buffer-live-p buffer) (push (cons new-id buffer) buffers))))
          (push (cons leaf (nreverse new-ids)) replacements)))
      (let ((atelier-inhibit-entry-removed-hook t))
        (atelier-entry-remove old-workspace entry t))
      (dolist (pair replacements)
        (atelier-plist-set! (car pair) :content-ids (cdr pair)))
      (atelier-plist-clear! entry :displayed)
      (atelier-plist-clear! entry :selected)
      (atelier-entry-add new-workspace entry t)
      (dolist (pair buffers)
        (puthash (atelier-content-cache-key new-workspace (car pair))
                 (cdr pair) atelier-content-live-buffers)))
    (run-hook-with-args 'atelier-entry-moved-hook entry old-workspace new-workspace)
    (run-hooks 'atelier-change-hook))
  entry)

(defun atelier-entry-job (entry)
  "Return ENTRY's terminal restart job, if any."
  (atelier-entry-value entry :job))

(defun atelier-workspace-job-entries (workspace)
  "Return terminal entries in WORKSPACE which carry restart jobs."
  (cl-remove-if-not #'atelier-entry-job (atelier-workspace-entries workspace)))

(defun atelier-content-persistent-copy (workspace content)
  "Snapshot CONTENT, including live scratch changes."
  (let* ((copy (copy-tree content))
         (buffer (gethash (atelier-content-cache-key workspace
                                                     (plist-get content :id))
                          atelier-content-live-buffers)))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (atelier-plist-set! copy :name (buffer-name buffer))
        (atelier-plist-set! copy :point (point))
        (when (eq (plist-get copy :kind) 'scratch)
          (atelier-plist-set! copy :contents
                             (buffer-substring-no-properties (point-min) (point-max))))))
    copy))

(defun atelier-entry-persistent-copy (entry)
  "Return ENTRY with only persistent content IDs, collapsing empty layouts."
  (if (atelier-layout-entry-p entry)
      (let* ((copy (copy-tree entry))
             (children (delq nil (mapcar #'atelier-entry-persistent-copy
                                         (atelier-entry-children entry))))
             (displayed (plist-get entry :displayed)))
        (pcase (length children)
          (0 nil)
          (1 (atelier-entry-with-display-state (car children) displayed))
          (_ (setf (plist-get copy :children) children) copy)))
    (let ((ids (cl-loop for content in (atelier-entry-stack entry)
                        when (plist-get content :persistent)
                        collect (plist-get content :id))))
      (when ids
        (let ((copy (copy-tree entry)))
          (atelier-plist-set! copy :content-ids ids)
          copy)))))

(defun atelier-entry-flatten (entry parent-id records)
  "Append ENTRY and its descendants to flat RECORDS using stable IDs only."
  (let* ((copy (copy-tree entry))
         (children (atelier-entry-children entry))
         (id (plist-get entry :id)))
    (setf (plist-get copy :parent-id) parent-id)
    (cl-remf copy :children)
    (if (atelier-layout-entry-p entry)
        (setf (plist-get copy :child-ids)
              (mapcar (lambda (child) (plist-get child :id)) children))
      (cl-remf copy :child-ids))
    (push copy (car records))
    (dolist (child children) (atelier-entry-flatten child id records))))

(defun atelier-workspace-flat-copy (workspace &optional persistent-only)
  "Return WORKSPACE as flat entries plus its owned content records."
  (let* ((atelier-model-workspace workspace)
         (_ (dolist (entry (atelier-workspace-entries workspace))
              (atelier-entry-ensure-content entry workspace)))
         (copy (copy-tree workspace))
         (roots (if persistent-only
                    (delq nil (mapcar #'atelier-entry-persistent-copy
                                      (atelier-workspace-top-level-entries workspace)))
                  (copy-tree (atelier-workspace-top-level-entries workspace))))
         (records (list nil)))
    (dolist (root roots) (atelier-entry-flatten root nil records))
    (setf (plist-get copy :entries) (nreverse (car records))
          (plist-get copy :entry-root-ids)
          (mapcar (lambda (root) (plist-get root :id)) roots))
    (let ((ids (cl-loop for entry in (plist-get copy :entries)
                        append (plist-get entry :content-ids))))
      (atelier-plist-set! copy :contents
                         (cl-loop for content in (plist-get workspace :contents)
                                  when (member (plist-get content :id) ids)
                                  collect (if persistent-only
                                              (atelier-content-persistent-copy workspace content)
                                            (copy-tree content)))))
    (cl-remf copy :layout)
    (cl-remf copy :state)
    copy))

(defun atelier-workspace-persistent-copy (workspace)
  "Return WORKSPACE as flat persistent ID-linked records."
  (atelier-workspace-flat-copy workspace t))

(defun atelier-workspace-runtime-copy (workspace)
  "Reconstruct WORKSPACE's runtime tree from its flat ID-linked records."
  (let* ((copy (copy-tree workspace))
         (records (plist-get workspace :entries))
         (roots (plist-get workspace :entry-root-ids))
         (by-id (make-hash-table :test #'equal))
         (visiting (make-hash-table :test #'equal))
         (visited (make-hash-table :test #'equal)))
    (dolist (record records)
      (let ((id (plist-get record :id)))
        (unless (and (stringp id) (not (string-empty-p id))
                     (not (gethash id by-id)))
          (error "Invalid or duplicate flat entry ID: %S" id))
        (puthash id record by-id)))
    (cl-labels
        ((build (id parent-id)
           (let ((record (gethash id by-id)))
             (unless record (error "Missing child entry %s" id))
             (when (gethash id visiting) (error "Cycle in flat entry graph at %s" id))
             (when (gethash id visited) (error "Entry %s has multiple parents" id))
             (unless (equal (plist-get record :parent-id) parent-id)
               (error "Incorrect parent ID for entry %s" id))
             (puthash id t visiting)
             (puthash id t visited)
             (let* ((entry (copy-tree record))
                    (child-ids (plist-get record :child-ids))
                    (children (mapcar (lambda (child-id) (build child-id id))
                                      child-ids)))
               (cl-remf entry :child-ids)
               (when (eq (plist-get entry :kind) 'layout)
                 (setf (plist-get entry :children) children))
               (remhash id visiting)
               entry))))
      (setf (plist-get copy :entries)
            (mapcar (lambda (id) (build id nil)) roots)))
    (unless (= (hash-table-count visited) (hash-table-count by-id))
      (error "Flat workspace contains unreachable entries"))
    (cl-remf copy :entry-root-ids)
    (atelier-workspace-index-entries copy)))

(provide 'atelier-model)
;;; atelier-model.el ends here

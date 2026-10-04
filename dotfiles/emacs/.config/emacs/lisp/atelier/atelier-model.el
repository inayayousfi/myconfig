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
    ;; Labels remain readable for saved jobs even without their runtime adapter.
    (aipanel :buffer-name "aipanel")
    (terminal :buffer-name "terminal")
    (buffer :buffer-name "buffer"))
  "Registered workspace entry types and their shared behavior.")
(defvar-local atelier-navigator-first-position nil)
(defvar-local atelier-directory-chooser-original-header nil)
(defvar-local atelier-directory-chooser-header-was-local nil)
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
;; A workspace owns stacks with stable identities and fixed content membership.
;; Views select a stack and one of its contents independently.  The :contents
;; list is the workspace's content-record index, not a second membership list.
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
  (unless (or (atelier-layout-entry-p entry)
              (plist-get entry :stack-reference)
              (plist-get entry :unassigned)
              (and (or (plist-get entry :stack-id) (plist-get entry :content-id))
                   (not (plist-get entry :kind))))
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
        (setq new-ids (nreverse new-ids))
        (atelier-plist-set! workspace :contents
                            (append (mapcar (lambda (id) (atelier-workspace-content workspace id)) new-ids)
                                    (cl-remove-if (lambda (content)
                                                    (member (plist-get content :id) new-ids))
                                                  (plist-get workspace :contents))))
        (atelier-plist-set! entry :content-ids
                            (append (plist-get entry :content-ids) new-ids))
        (dolist (stack (plist-get workspace :stacks))
          (atelier-plist-set! stack :content-ids
                              (cl-loop for content in (plist-get workspace :contents)
                                       when (equal (plist-get content :stack-id) (plist-get stack :id))
                                       collect (plist-get content :id)))))
      (atelier-plist-remove! entry :stack)
      (unless (plist-get entry :content-id)
        (atelier-plist-set! entry :content-id (car (plist-get entry :content-ids))))
      (atelier-plist-remove! entry :content-ids)))
  (atelier-workspace-ensure-stacks workspace)
  (when-let* ((content (atelier-workspace-content workspace (plist-get entry :content-id))))
    (atelier-plist-set! entry :stack-id (plist-get content :stack-id)))
  (puthash entry workspace atelier-entry-owners)
  entry)

(defun atelier-content-default-type (content)
  "Classify legacy untyped content; BUFFER is the ordinary fallback type."
  (or (plist-get content :type)
      (pcase (plist-get content :kind)
        ('file 'file) ('directory 'dired) ('terminal 'terminal) (_ 'buffer))))

(defun atelier-workspace-stack-record (workspace stack-id)
  "Return WORKSPACE's stack with STACK-ID."
  (cl-find stack-id (plist-get workspace :stacks) :key (lambda (stack) (plist-get stack :id))
           :test #'equal))

(defun atelier-workspace-ensure-stacks (workspace)
  "Normalize legacy workspace contents into explicit typed stacks once."
  (unless (plist-member workspace :stacks)
    (atelier-plist-set! workspace :stacks nil)
    (dolist (content (plist-get workspace :contents))
      (let* ((type (atelier-content-default-type content))
             (stack (or (cl-find type (plist-get workspace :stacks)
                                 :key (lambda (item) (plist-get item :type)))
                        (let ((new (list :id (atelier-new-entry-id) :type type :content-ids nil)))
                          (atelier-plist-set! workspace :stacks
                                              (append (plist-get workspace :stacks) (list new)))
                          new))))
        (atelier-plist-set! content :type type)
        (atelier-plist-set! content :stack-id (plist-get stack :id))
        (atelier-plist-set! stack :content-ids
                            (append (plist-get stack :content-ids) (list (plist-get content :id)))))))
  workspace)

(defun atelier-workspace-stack (workspace type)
  "Return WORKSPACE's content records of TYPE, newest first."
  (atelier-workspace-ensure-stacks workspace)
  (when-let* ((stack (cl-find (or type 'buffer) (plist-get workspace :stacks)
                            :key (lambda (item) (plist-get item :type)))))
    (mapcar (lambda (id) (atelier-workspace-content workspace id))
            (plist-get stack :content-ids))))

(defun atelier-validate-workspace-stacks (workspace)
  "Enforce typed stacks, exclusive buffer membership and valid view assignments."
  (let (stack-ids types members)
    (dolist (stack (plist-get workspace :stacks))
      (let ((id (plist-get stack :id)) (type (plist-get stack :type)))
        (unless (and (stringp id) (not (string-empty-p id)) (not (member id stack-ids))
                     type (assq type atelier-entry-types))
          (error "Invalid stack identity or type"))
        (when (and (not (atelier-detached-workspace-p workspace)) (memq type types))
          (error "Workspace has more than one stack of type %s" type))
        (push id stack-ids)
        (push type types)
        (dolist (content-id (plist-get stack :content-ids))
          (let ((content (atelier-workspace-content workspace content-id)))
            (unless (and content (not (member content-id members))
                         (eq type (plist-get content :type))
                         (equal id (plist-get content :stack-id)))
              (error "Invalid or duplicate buffer membership in stack %s" id))
            (push content-id members)))))
    (dolist (content (plist-get workspace :contents))
      (unless (member (plist-get content :id) members)
        (error "Buffer %s has no stack" (plist-get content :id))))
    (dolist (view (atelier-workspace-view-entries workspace))
      (if (plist-get view :unassigned)
          (when (or (plist-get view :stack-id) (plist-get view :content-id))
            (error "Unassigned view still references a stack or buffer"))
        (let ((stack (atelier-workspace-stack-record workspace (plist-get view :stack-id))))
          (unless (and stack (if (plist-get stack :content-ids)
                                (member (plist-get view :content-id) (plist-get stack :content-ids))
                              (not (plist-get view :content-id))))
            (error "View selects a buffer outside its assigned stack")))))
    t))

(defun atelier-content-reference (workspace content)
  "Return a transient handle for workspace CONTENT, not a stored view."
  (let ((entry (list :id (plist-get content :id) :content-id (plist-get content :id)
                     :stack-id (plist-get content :stack-id) :content-reference t)))
    (puthash entry workspace atelier-entry-owners)
    entry))

(defun atelier-validate-stack-identities (workspaces)
  "Require each stack identity to have exactly one owner or Detached location."
  (let ((seen (make-hash-table :test #'equal)))
    (dolist (workspace workspaces)
      (dolist (stack (plist-get workspace :stacks))
        (let ((id (plist-get stack :id)))
          (when (gethash id seen) (error "Stack %s has more than one owner" id))
          (puthash id t seen))))
    t))

(defun atelier-stack-reference (workspace stack)
  "Return a navigator handle for STACK, including an empty stack."
  (let ((entry (list :id (plist-get stack :id) :stack-id (plist-get stack :id)
                     :content-id (car (plist-get stack :content-ids))
                     :content-reference t :stack-reference t)))
    (puthash entry workspace atelier-entry-owners)
    entry))

(defun atelier-entry-content-ids (entry &optional workspace)
  "Return ENTRY's selected content followed by its workspace stack."
  (mapcar (lambda (content) (plist-get content :id))
          (atelier-entry-stack entry workspace)))

(defun atelier-workspace-content (workspace id)
  (cl-find id (plist-get workspace :contents)
           :key (lambda (content) (plist-get content :id)) :test #'equal))

(defun atelier-entry-content (entry &optional workspace)
  "Return ENTRY's active workspace-owned content record (mutable), or nil."
  (unless (atelier-layout-entry-p entry)
    (let ((workspace (or workspace (atelier-entry-owner entry))))
      (atelier-entry-ensure-content entry workspace)
      (atelier-workspace-content workspace (plist-get entry :content-id)))))

(defun atelier-entry-value (entry property &optional workspace)
  "Read PROPERTY from ENTRY's active content (layout properties stay on ENTRY)."
  (or (plist-get (if (atelier-layout-entry-p entry) entry
                    (atelier-entry-content entry workspace)) property)
      (when (plist-get entry :stack-reference)
        (let ((stack (atelier-workspace-stack-record (or workspace (atelier-entry-owner entry))
                                                     (plist-get entry :stack-id))))
          (pcase property
            (:type (plist-get stack :type))
            (:name (format "%s stack (empty)" (plist-get stack :type))))))))

(defun atelier-entry-set-value (entry property value &optional workspace)
  "Set PROPERTY on ENTRY's active content, preserving record identity."
  (unless (memq property atelier-content-properties)
    (error "Not a content property: %S" property))
  (let ((content (atelier-entry-content entry workspace)))
    (unless content (error "Entry has no active content"))
    (when (and (eq property :type) (not (eq value (plist-get content :type))))
      (error "Buffer stack type cannot change"))
    (atelier-plist-set! content property value)))

(defun atelier-entry-stack (entry &optional workspace)
  "Return the workspace stack selected by ENTRY, its selected content first."
  (unless (atelier-layout-entry-p entry)
    (let ((workspace (or workspace (atelier-entry-owner entry))))
      (atelier-entry-ensure-content entry workspace)
      (when-let* ((active (atelier-entry-content entry workspace)))
        (let ((stack (atelier-workspace-stack-record workspace (plist-get entry :stack-id))))
          (cons active (cl-loop for id in (plist-get stack :content-ids)
                               for content = (atelier-workspace-content workspace id)
                               unless (eq active content) collect content)))))))

(defun atelier-content-new-id ()
  (format "content-%s" (atelier-new-entry-id)))

(defun atelier-content-cache-key (workspace id)
  (cons (atelier-workspace-id workspace) id))

(defun atelier-workspace-store-content (workspace content)
  "Store CONTENT in WORKSPACE, returning its ID; copy on ID collision."
  (atelier-workspace-ensure-stacks workspace)
  (let* ((copy (copy-tree content))
         (id (or (plist-get copy :id) (plist-get copy :content-id)
                 (atelier-content-new-id))))
    (while (atelier-workspace-content workspace id)
      (setq id (atelier-content-new-id)))
    (cl-remf copy :content-id)
    (atelier-plist-set! copy :id id)
    (let* ((type (atelier-content-default-type copy))
           (stack (or (atelier-workspace-stack-record workspace (plist-get copy :stack-id))
                      (cl-find type (plist-get workspace :stacks)
                               :key (lambda (item) (plist-get item :type)))
                      (let ((new (list :id (atelier-new-entry-id) :type type :content-ids nil)))
                        (atelier-plist-set! workspace :stacks
                                            (cons new (plist-get workspace :stacks)))
                        new))))
      (unless (eq type (plist-get stack :type)) (error "Content type differs from stack type"))
      (atelier-plist-set! copy :type type)
      (atelier-plist-set! copy :stack-id (plist-get stack :id))
      (atelier-plist-set! stack :content-ids (cons id (plist-get stack :content-ids))))
    (atelier-plist-set! workspace :contents
                        (cons copy (plist-get workspace :contents)))
    id))

(defun atelier-entry-replace-content (entry content)
  "Replace ENTRY's active content in its owning workspace."
  (let* ((workspace (atelier-entry-owner entry))
          (old (plist-get entry :content-id))
         (id (atelier-workspace-store-content workspace content)))
    (atelier-plist-set! entry :content-id id)
    (atelier-plist-set! entry :stack-id (plist-get (atelier-workspace-content workspace id) :stack-id))
    (unless (cl-some (lambda (leaf) (equal old (plist-get leaf :content-id)))
                      (atelier-workspace-view-entries workspace))
      (atelier-workspace-drop-content workspace old))
    entry))

(defun atelier-workspace-prune-empty-stacks (workspace)
  "Delete empty stacks and their views from WORKSPACE."
  (let* ((empty (cl-remove-if (lambda (stack) (plist-get stack :content-ids))
                              (plist-get workspace :stacks)))
         (ids (mapcar (lambda (stack) (plist-get stack :id)) empty))
         (atelier-inhibit-entry-removed-hook t))
    (when empty
      (dolist (view (copy-sequence (atelier-workspace-view-entries workspace)))
        (when (member (plist-get view :stack-id) ids)
          (atelier-entry-remove workspace view t)))
      (atelier-plist-set! workspace :stacks
                          (cl-set-difference (plist-get workspace :stacks) empty :test #'eq))))
  workspace)

(defun atelier-workspace-drop-content (workspace id)
  (when id
    (when-let* ((content (atelier-workspace-content workspace id))
                (stack (atelier-workspace-stack-record workspace (plist-get content :stack-id))))
      (atelier-plist-set! stack :content-ids (delete id (copy-sequence (plist-get stack :content-ids)))))
    (let* ((content (atelier-workspace-content workspace id))
           (replacement (cl-find-if
                         (lambda (other)
                           (and (not (equal id (plist-get other :id)))
                                 (equal (plist-get content :stack-id) (plist-get other :stack-id))))
                         (plist-get workspace :contents))))
      (dolist (view (copy-sequence (atelier-workspace-view-entries workspace)))
        (when (equal id (plist-get view :content-id))
          (if replacement
              (atelier-plist-set! view :content-id (plist-get replacement :id))
            (atelier-entry-remove workspace view t)))))
    (atelier-plist-set! workspace :contents
                       (cl-remove id (plist-get workspace :contents)
                                  :key (lambda (content) (plist-get content :id))
                                  :test #'equal))
    (remhash (atelier-content-cache-key workspace id)
             atelier-content-live-buffers)
    (atelier-workspace-prune-empty-stacks workspace)))

(defun atelier-entry-prune-contents (workspace entry predicate)
  "Remove contents matching PREDICATE, dropping ENTRY only when it is empty.
PREDICATE receives each content record.  Other views retain shared records."
  (let* ((contents (atelier-entry-stack entry workspace))
         (removed (cl-remove-if-not predicate contents))
          (ids (mapcar (lambda (content) (plist-get content :id)) removed)))
    (when removed
      (dolist (id ids)
        (atelier-workspace-drop-content workspace id)))
    (and removed t)))

(defun atelier-entry-push-content (entry content &optional buffer)
  "Add CONTENT to its workspace type stack and select it in ENTRY."
  (when (atelier-layout-entry-p entry) (error "A layout cannot select content"))
  (let* ((workspace (atelier-entry-owner entry))
         (id (atelier-workspace-store-content workspace content)))
    (atelier-plist-set! entry :content-id id)
    (atelier-plist-set! entry :stack-id (plist-get (atelier-workspace-content workspace id) :stack-id))
    (atelier-entry-set-live-buffer entry buffer)
    entry))

(defun atelier-entry-activate-content (entry content-id)
  "Select CONTENT-ID in ENTRY without changing another view or stack order."
  (unless (atelier-workspace-content (atelier-entry-owner entry) content-id)
    (error "Missing content %s" content-id))
  (atelier-plist-set! entry :content-id content-id)
  (atelier-plist-remove! entry :unassigned)
  (atelier-plist-set! entry :stack-id
                      (plist-get (atelier-workspace-content (atelier-entry-owner entry) content-id) :stack-id))
  entry)

(defun atelier-view-unassign-stack (workspace view)
  "Detach VIEW from its stack without changing that stack or any buffer."
  (unless (memq view (atelier-workspace-view-entries workspace))
    (user-error "Select a view to unassign its stack"))
  (atelier-plist-remove! view :stack-id)
  (atelier-plist-remove! view :content-id)
  (atelier-plist-set! view :unassigned t)
  view)

(defun atelier-view-assign-stack (workspace view stack-id &optional content-id)
  "Assign WORKSPACE's STACK-ID to VIEW without moving or creating any buffer."
  (let* ((stack (or (atelier-workspace-stack-record workspace stack-id)
                    (user-error "Stack no longer exists")))
         (selected (or content-id (car (plist-get stack :content-ids)))))
    (unless (or (and (not selected) (not (plist-get stack :content-ids)))
                (member selected (plist-get stack :content-ids)))
      (user-error "Buffer is not a member of this stack"))
    (atelier-plist-remove! view :unassigned)
    (atelier-plist-set! view :stack-id stack-id)
    (atelier-plist-set! view :content-id selected)
    view))

(defun atelier-entry-activate-buffer (entry buffer)
  "Activate ENTRY's content corresponding to BUFFER, if inactive."
  (let ((workspace (atelier-entry-owner entry)))
    (cl-loop for id in (atelier-entry-content-ids entry workspace)
             when (eq buffer (gethash (atelier-content-cache-key workspace id)
                                      atelier-content-live-buffers))
             return (atelier-entry-activate-content entry id))))

(defun atelier-entry-pop-content (entry)
  "Remove active content and reveal the next one; return removed record."
  (when (cdr (atelier-entry-stack entry))
    (let* ((workspace (atelier-entry-owner entry))
           (removed (atelier-entry-content entry workspace))
            (id (plist-get entry :content-id)))
      (atelier-workspace-drop-content workspace id)
      (when (plist-get entry :content-reference)
        (atelier-plist-set! entry :content-id
                            (plist-get (car (atelier-workspace-stack workspace
                                                                   (plist-get removed :type))) :id)))
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

(defun atelier-workspace-successor (workspace &optional first)
  "Choose another workspace; FIRST uses registry order rather than running first."
  (let ((others (cl-remove-if
                 (lambda (candidate)
                   (equal (atelier-workspace-id candidate) (atelier-workspace-id workspace)))
                 atelier-workspaces)))
    (or (and (not first)
             (cl-find-if (lambda (candidate) (eq (atelier-workspace-status candidate) 'running)) others))
        (car others))))

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
  (or (and (bound-and-true-p atelier-operation-current)
           (alist-get (or frame (selected-frame))
                      (atelier-operation-frames atelier-operation-current)))
      (frame-parameter (or frame (selected-frame)) 'atelier-workspace-id)
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
  (if (bound-and-true-p atelier-operation-current)
      (atelier-operation-select-frame (atelier-workspace-id workspace)
                                      (or frame (selected-frame)))
    (set-frame-parameter (or frame (selected-frame)) 'atelier-workspace-id
                         (atelier-workspace-id workspace)))
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

(defun atelier-workspace-view-entries (workspace)
  "Return WORKSPACE's stored views, including undisplayed views."
  (cl-mapcan #'atelier-entry-leaves
              (atelier-workspace-top-level-entries workspace)))

(defun atelier-workspace-entries (workspace)
  "Return views and one reference for each otherwise undisplayed type stack."
  (let* ((views (atelier-workspace-view-entries workspace))
         (_ (dolist (view views) (atelier-entry-ensure-content view workspace)))
         (assigned (mapcar (lambda (view) (plist-get view :stack-id)) views)))
    (append views
            (cl-loop for stack in (plist-get workspace :stacks)
                     unless (member (plist-get stack :id) assigned)
                     collect (atelier-stack-reference workspace stack)))))

(defun atelier-workspace-refresh-parent-ids (workspace)
  "Rebuild runtime parent-ID links throughout WORKSPACE's entry trees."
  (cl-labels ((visit (entry parent-id)
                 (atelier-plist-set! entry :parent-id parent-id)
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
  (cl-loop for content in (atelier-workspace-stack workspace type)
           for buffer = (gethash (atelier-content-cache-key workspace (plist-get content :id))
                                atelier-content-live-buffers)
           when (buffer-live-p buffer) return buffer))

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
  (or (cl-loop for entry in (atelier-workspace-top-level-entries workspace)
               thereis (atelier-entry-find entry id))
      (when-let* ((stack (atelier-workspace-stack-record workspace id)))
        (atelier-stack-reference workspace stack))
      (when-let* ((content (atelier-workspace-content workspace id)))
        (atelier-content-reference workspace content))))

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
          (id (plist-get entry :content-id))
           (buffer (gethash (if workspace
                                (atelier-content-cache-key workspace id)
                              (cons :unowned (plist-get entry :id)))
                            atelier-content-live-buffers)))
      (and (buffer-live-p buffer) buffer))))

(defun atelier-entry-set-live-buffer (entry buffer)
  "Cache BUFFER by content ID, deferring isolated entries until given an owner."
  (when (and (bound-and-true-p atelier-operation-current)
             (buffer-live-p buffer)
             (not (memq buffer (atelier-operation-buffers atelier-operation-current))))
    (atelier-operation-track-buffer buffer))
  (let* ((workspace (or atelier-model-workspace (gethash entry atelier-entry-owners)
                          (atelier-entry-workspace entry)))
         (_ (when workspace (atelier-entry-ensure-content entry workspace)))
          (id (plist-get entry :content-id))
         (key (if workspace
                  (progn (unless id (error "Entry has no active content ID"))
                         (atelier-content-cache-key workspace id))
                (cons :unowned (plist-get entry :id)))))
    (if (buffer-live-p buffer)
        (puthash key buffer atelier-content-live-buffers)
      (unless (and workspace
                   (cl-some (lambda (other)
                              (and (not (eq other entry))
                                    (equal id (plist-get other :content-id))))
                            (atelier-workspace-entries workspace)))
        (remhash key atelier-content-live-buffers)))
    buffer))

(defun atelier-entry-inactive-buffer-p (entry buffer)
  "Whether BUFFER is retained below the active content of ENTRY."
  (let ((workspace (atelier-entry-owner entry)))
    (cl-some (lambda (id)
               (eq buffer (gethash (atelier-content-cache-key workspace id)
                                   atelier-content-live-buffers)))
              (cdr (atelier-entry-content-ids entry workspace)))))

(defun atelier-buffer-referenced-p (buffer)
  "Return non-nil when any workspace stack owns BUFFER."
  (cl-some (lambda (workspace)
              (cl-some (lambda (content)
                         (eq buffer (gethash (atelier-content-cache-key workspace (plist-get content :id))
                                            atelier-content-live-buffers)))
                       (plist-get workspace :contents)))
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
      (atelier-operation-notify 'atelier-entry-added-hook workspace entry)
      (atelier-operation-notify 'atelier-change-hook)))
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
  "Remove a view, or close content when ENTRY is a workspace content reference."
  (let ((id (plist-get entry :id)))
    (atelier-plist-set!
     workspace :entries
     (delq nil
           (mapcar (lambda (tree) (atelier-entry-remove-from-tree tree id))
                   (atelier-workspace-top-level-entries workspace))))
    (atelier-workspace-refresh-parent-ids workspace)
    (when (plist-get entry :content-reference)
      (atelier-workspace-drop-content workspace (plist-get entry :content-id))))
  (unless atelier-inhibit-entry-removed-hook
    (atelier-operation-notify 'atelier-entry-removed-hook workspace entry))
  (unless no-notify
    (atelier-operation-notify 'atelier-change-hook))
  entry)

(defun atelier-check-entry-move (entry old-workspace new-workspace)
  "Require an unassigned whole stack and an available destination type slot."
  (unless (eq old-workspace new-workspace)
    (atelier-workspace-entries old-workspace)
    (atelier-workspace-ensure-stacks new-workspace)
    (let* ((stack-id (plist-get entry :stack-id))
           (stack (atelier-workspace-stack-record old-workspace stack-id)))
      (unless stack (user-error "Stack no longer exists"))
      (when (cl-some (lambda (view) (equal stack-id (plist-get view :stack-id)))
                     (atelier-workspace-view-entries old-workspace))
        (user-error "Detach this stack from every view in %s first; nothing was moved"
                    (plist-get old-workspace :name)))
      (unless (or (atelier-detached-workspace-p old-workspace)
                  (atelier-detached-workspace-p new-workspace))
        (user-error "Detach this stack from its workspace before attaching it elsewhere"))
      (when (and (not (atelier-detached-workspace-p new-workspace))
                 (cl-find (plist-get stack :type) (plist-get new-workspace :stacks)
                          :key (lambda (other) (plist-get other :type))))
        (user-error "Workspace %s already owns a %s stack; buffers cannot move between stacks"
                    (plist-get new-workspace :name) (plist-get stack :type))))))

(defun atelier-entry-move (entry old-workspace new-workspace)
  "Prepare a checked move using stable workspace and entry IDs."
  (let ((old-id (atelier-workspace-id old-workspace))
        (new-id (atelier-workspace-id new-workspace))
        (entry-id (plist-get entry :id)))
    (atelier-operation-call
     'move-entry (delete-dups (list old-id new-id))
     (lambda ()
       (let* ((old (atelier-operation-workspace old-id))
              (new (atelier-operation-workspace new-id))
              (entry (atelier-operation-entry old entry-id)))
         (atelier--entry-move entry old new))) t)))

(defun atelier--entry-move (entry old-workspace new-workspace)
  "Transfer ENTRY's entire unassigned stack, retaining every stable identity."
  (unless (eq old-workspace new-workspace)
    (atelier-check-entry-move entry old-workspace new-workspace)
    (let* ((stack (atelier-workspace-stack-record old-workspace (plist-get entry :stack-id)))
           (ids (plist-get stack :content-ids))
           (contents (mapcar (lambda (id) (atelier-workspace-content old-workspace id)) ids)))
      (when (cl-some (lambda (id) (atelier-workspace-content new-workspace id)) ids)
        (user-error "Destination contains a duplicate buffer identity"))
      (atelier-plist-set! old-workspace :stacks (remq stack (plist-get old-workspace :stacks)))
      (atelier-plist-set! old-workspace :contents
                          (cl-set-difference (plist-get old-workspace :contents) contents :test #'eq))
      (atelier-plist-set! new-workspace :stacks (cons stack (plist-get new-workspace :stacks)))
      (atelier-plist-set! new-workspace :contents (append contents (plist-get new-workspace :contents)))
      (dolist (id ids)
        (let* ((key (atelier-content-cache-key old-workspace id))
               (buffer (gethash key atelier-content-live-buffers)))
          (remhash key atelier-content-live-buffers)
          (when (buffer-live-p buffer)
            (puthash (atelier-content-cache-key new-workspace id) buffer atelier-content-live-buffers))))
      (puthash entry new-workspace atelier-entry-owners))
    (atelier-operation-notify 'atelier-entry-moved-hook entry old-workspace new-workspace)
    (atelier-operation-notify 'atelier-change-hook))
  entry)

(defun atelier-entry-job (entry)
  "Return ENTRY's terminal restart job, if any."
  (atelier-entry-value entry :job))

(defun atelier-workspace-job-entries (workspace)
  "Return one reference per restart job, independent of visible selection."
  (cl-loop for content in (plist-get workspace :contents)
           when (plist-get content :job)
           collect (atelier-content-reference workspace content)))

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
    (let ((content (atelier-entry-content entry)))
      (when (or (plist-get entry :unassigned)
                (and (plist-get entry :stack-id) (not content))
                (plist-get content :persistent))
        (let ((copy (copy-tree entry)))
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
    (setf (plist-get copy :entries) roots)
    (atelier-plist-set! copy :contents
                        (cl-loop for content in (plist-get workspace :contents)
                                 when (or (not persistent-only) (plist-get content :persistent))
                                 collect (if persistent-only
                                             (atelier-content-persistent-copy workspace content)
                                            (copy-tree content))))
    (dolist (stack (plist-get copy :stacks))
      (atelier-plist-set! stack :content-ids
                          (cl-remove-if-not (lambda (id) (atelier-workspace-content copy id))
                                            (plist-get stack :content-ids))))
    ;; Filtering transient contents can empty a stack in the saved copy only.
    (let ((atelier-model-workspace copy))
      (atelier-workspace-prune-empty-stacks copy))
    (setq roots (atelier-workspace-top-level-entries copy))
    (dolist (root roots) (atelier-entry-flatten root nil records))
    (setf (plist-get copy :entries) (nreverse (car records))
          (plist-get copy :entry-root-ids)
          (mapcar (lambda (root) (plist-get root :id)) roots))
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
    (atelier-workspace-prune-empty-stacks copy)
    (atelier-workspace-index-entries copy)))

(provide 'atelier-model)
;;; atelier-model.el ends here

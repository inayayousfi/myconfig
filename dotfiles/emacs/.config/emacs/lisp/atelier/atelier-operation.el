;;; atelier-operation.el --- Prepared Atelier state changes -*- lexical-binding: t; -*-

(require 'atelier-core)
(require 'atelier-model)

(cl-defstruct (atelier-operation (:constructor atelier-operation-create))
  name ids original frames windows navigator-states notifications cleanups completion invalid buffers checks acquired-buffers)

(defvar atelier-operation-current nil
  "Preparation currently executing; its records are private copies.")
(defvar atelier-operation-active nil
  "Operations retaining exclusive access to their affected workspace IDs.")
(defvar atelier-operation-queue nil
  "Conflicting requests, in arrival order, with stable targets and frame context.")
(defvar atelier-operation-live-workspaces nil)
(defvar atelier-operation-live-cache nil)
(defvar atelier-operation-live-state nil
  "Shared live registry cell, including native events during a failed preparation.")
(defvar atelier-operation-draining nil)
(defvar atelier-operation-closing-immediately nil
  "A job replacement explicitly needs the previous process stopped first.")
(defvar atelier-operation-owned-effect nil
  "Native callback belongs to an effect explicitly performed by this operation.")

(defun atelier-operation-workspace (workspace)
  "Resolve WORKSPACE's stable ID in the current record set."
  (or (atelier-workspace-by-id (if (stringp workspace) workspace
                               (atelier-workspace-id workspace)))
      (user-error "Workspace no longer exists")))

(defun atelier-operation-entry (workspace entry)
  "Resolve ENTRY by stable ID, never by its old position or record address."
  (or (atelier-entry-by-id (atelier-operation-workspace workspace)
                           (if (stringp entry) entry (plist-get entry :id)))
      (user-error "Workspace entry no longer exists")))

(defun atelier-operation-notify (hook &rest arguments)
  "Run HOOK after publication; one observer's error does not stop the others."
  (if atelier-operation-current
      (push (cons hook arguments)
            (atelier-operation-notifications atelier-operation-current))
    (apply #'run-hook-wrapped hook
           (lambda (function &rest args)
             (condition-case error
                 (apply function args)
               ((error quit)
                (atelier-log "Notification %s failed: %s" hook
                             (error-message-string error))))
             nil)
           arguments)))

(defun atelier-operation-cleanup (function)
  "Register FUNCTION to release a newly acquired resource if preparation fails."
  (when atelier-operation-current
    (push function (atelier-operation-cleanups atelier-operation-current))))

(defun atelier-operation-track-buffer (buffer)
  "Track a newly created BUFFER without claiming an existing buffer."
  (when (and atelier-operation-current
             (not (memq buffer (atelier-operation-buffers atelier-operation-current)))
             (not (memq buffer (atelier-operation-acquired-buffers atelier-operation-current))))
    (push buffer (atelier-operation-acquired-buffers atelier-operation-current))
    (atelier-operation-cleanup
     (lambda ()
       (when (buffer-live-p buffer)
         (let ((atelier-close-without-asking t)
               (atelier-preserve-job-recipe t))
           (atelier-kill-buffer buffer))))))
  buffer)

(defmacro atelier-define-operation (name arguments scope bindings &rest body)
  "Define NAME's explicit prepared operation and its private implementation.
SCOPE lists affected workspace IDs.  BINDINGS resolve record arguments inside
preparation so queued calls use stable targets, not obsolete record addresses."
  (declare (indent 4) (doc-string 5))
  (let* ((doc (when (stringp (car body)) (pop body)))
         (interactive (when (eq (caar body) 'interactive) (pop body)))
         (worker (intern (if (string-prefix-p "atelier-" (symbol-name name))
                             (concat "atelier--" (string-remove-prefix "atelier-" (symbol-name name)))
                           (concat (symbol-name name) "--prepare"))))
         (parameters (cl-remove-if (lambda (symbol) (memq symbol '(&optional &rest))) arguments))
         (rest-position (cl-position '&rest arguments))
         (call (if rest-position
                   `(apply #',worker ,@(cl-subseq parameters 0 (1- rest-position))
                           ,(car (last parameters)))
                 `(,worker ,@parameters)))
         (retry (if rest-position
                    `(apply #',name ,@(cl-subseq parameters 0 (1- rest-position))
                            ,(car (last parameters)))
                  `(,name ,@parameters))))
    `(progn
       (defun ,worker ,arguments ,@body)
       (defun ,name ,arguments
         ,@(when doc (list doc))
         ,@(when interactive (list interactive))
         (let* ,bindings
           (atelier-operation-call
            ',name ,scope
            (lambda () (let* ,bindings ,call))
            (not (called-interactively-p 'any))
            (lambda () ,retry)))))))

(defun atelier-operation-after (function)
  "Run FUNCTION after record publication, not while preparing copies."
  (if atelier-operation-current
      (push function (atelier-operation-completion atelier-operation-current))
    (funcall function)))

(defun atelier-operation-check (function)
  "Require FUNCTION to succeed immediately before publication."
  (if atelier-operation-current
      (push function (atelier-operation-checks atelier-operation-current))
    (funcall function)))

(defun atelier-operation-touch-frame (frame)
  "Remember FRAME's display before preparing changes to it."
  (when (and atelier-operation-current (frame-live-p frame)
              (not (assq frame (atelier-operation-windows atelier-operation-current))))
    (let ((navigator (and (fboundp 'atelier-navigator-frame-buffer)
                          (atelier-navigator-frame-buffer frame))))
      (push (list frame
                  (alist-get frame atelier-navigator-window-configurations nil nil #'eq)
                  (alist-get frame atelier-navigator-selection-by-frame nil nil #'eq)
                  navigator
                  (and (buffer-live-p navigator)
                       (copy-tree (buffer-local-value 'atelier-navigator-changed-views navigator))))
            (atelier-operation-navigator-states atelier-operation-current)))
    (push (cons frame (current-window-configuration frame))
          (atelier-operation-windows atelier-operation-current))))

(defun atelier-operation-select-frame (workspace-id frame)
  "Stage FRAME's selection, publishing it only with the prepared records."
  (atelier-operation-touch-frame frame)
  (setf (alist-get frame (atelier-operation-frames atelier-operation-current)) workspace-id))

(defun atelier-operation-live-event (function)
  "Record a native event in live state, even while private copies are prepared.
Events are facts, not queued user intentions.  Affected preparations are checked
against these updated records before they may publish."
  (if (not atelier-operation-current) (funcall function)
    (let ((atelier-workspaces (if atelier-operation-live-state
                                 (car atelier-operation-live-state)
                               atelier-operation-live-workspaces))
          (atelier-content-live-buffers atelier-operation-live-cache)
          (atelier-operation-current nil))
      (unwind-protect (funcall function)
        (setq atelier-operation-live-workspaces atelier-workspaces)
        (when atelier-operation-live-state
          (setcar atelier-operation-live-state atelier-workspaces))))))

(defun atelier-operation-conflicts-p (ids operation)
  (or (memq :all ids) (memq :all (atelier-operation-ids operation))
      (cl-intersection ids (atelier-operation-ids operation) :test #'equal)))

(defun atelier-operation-validate (workspaces)
  "Check the live model's ownership rules without persistence or connections."
  (let ((workspace-ids nil) names)
    (dolist (workspace workspaces)
      (let ((id (plist-get workspace :id))
            (name (plist-get workspace :name))
             entry-ids content-ids)
        (unless (and (stringp id) (not (string-empty-p id)) (not (member id workspace-ids)))
          (error "Invalid or duplicate workspace ID: %S" id))
        (push id workspace-ids)
        (when name
          (unless (and (stringp name) (not (string-empty-p name)))
            (error "Invalid workspace name: %S" name))
          (when (member name names) (error "Duplicate workspace name: %s" name))
          (push name names))
        (dolist (content (plist-get workspace :contents))
          (let ((content-id (plist-get content :id)))
            (unless (and (stringp content-id) (not (string-empty-p content-id))
                         (not (member content-id content-ids)))
              (error "Invalid or duplicate content ID: %S" content-id))
            (push content-id content-ids)))
        (cl-labels
            ((visit (entry parent)
               (let ((entry-id (plist-get entry :id)))
                 (unless (and (stringp entry-id) (not (string-empty-p entry-id))
                              (not (member entry-id entry-ids)))
                   (error "Invalid, repeated or cyclic entry ID: %S" entry-id))
                 (push entry-id entry-ids)
                 (unless (equal (plist-get entry :parent-id) parent)
                   (error "Wrong parent for entry %s" entry-id))
                 (if (atelier-layout-entry-p entry)
                     (progn
                       (unless (= (length (atelier-entry-children entry)) 2)
                         (error "Layout %s must have two children" entry-id))
                       (dolist (child (atelier-entry-children entry)) (visit child entry-id)))
                    (unless (or (plist-get entry :unassigned) (plist-get entry :stack-id))
                      (error "View %s has no content" entry-id))
                    (unless (or (not (plist-get entry :content-id))
                                (member (plist-get entry :content-id) content-ids))
                      (error "Invalid content reference in %s" entry-id))))))
          (dolist (entry (plist-get workspace :entries)) (visit entry nil)))
        (atelier-validate-workspace-stacks workspace))))
  (atelier-validate-stack-identities workspaces)
  t)

(defun atelier-operation-reconcile (old new mapping)
  "Publish NEW into OLD, retaining compatible record identities by stable ID.
Preparation never touches OLD.  MAPPING resolves notifications and results to
the published records; callers must still resolve IDs after a removal."
  (let ((old-entries (make-hash-table :test #'equal))
        (old-contents (make-hash-table :test #'equal)))
    (cl-labels ((index (entry)
                 (puthash (plist-get entry :id) entry old-entries)
                 (mapc #'index (atelier-entry-children entry))))
      (mapc #'index (plist-get old :entries)))
    (dolist (content (plist-get old :contents))
      (puthash (plist-get content :id) content old-contents))
    (cl-labels
        ((publish (draft table)
           (let* ((target (or (gethash (plist-get draft :id) table) draft))
                  (values (copy-sequence draft)))
             (when (atelier-layout-entry-p draft)
               (setq values (plist-put values :children
                                       (mapcar (lambda (child) (publish child old-entries))
                                               (atelier-entry-children draft)))))
             (puthash draft target mapping)
             (unless (eq target draft)
               (setcar target (car values)) (setcdr target (cdr values)))
             target)))
      (let ((values (copy-sequence new)))
        (setq values (plist-put values :entries
                                (mapcar (lambda (entry) (publish entry old-entries))
                                        (plist-get new :entries)))
              values (plist-put values :contents
                                (mapcar (lambda (content) (publish content old-contents))
                                        (plist-get new :contents))))
        (puthash new old mapping)
        (setcar old (car values)) (setcdr old (cdr values))
        old))))

(defun atelier-operation-validate-cache (workspaces cache)
  "Reject a live buffer claimed by different workspace owners."
  (let ((owners (make-hash-table :test #'eq)))
    (dolist (workspace workspaces)
      (let ((id (atelier-workspace-id workspace)))
        (dolist (content (plist-get workspace :contents))
          (let ((buffer (gethash (atelier-content-cache-key workspace (plist-get content :id)) cache)))
            (when (buffer-live-p buffer)
              (when-let* ((other (gethash buffer owners)) (_ (not (equal other id))))
                (error "Buffer %s cannot belong to both %s and %s" (buffer-name buffer) other id))
              (puthash buffer id owners)))))))
  t)

(defun atelier-operation-result (value mapping)
  (or (gethash value mapping)
      (if (consp value)
          (cons (atelier-operation-result (car value) mapping)
                (atelier-operation-result (cdr value) mapping))
        value)))

(defun atelier-operation-drain ()
  "Run ready queued requests in arrival order, checking targets anew."
  (unless atelier-operation-draining
    (let ((atelier-operation-draining t) retained)
      (while atelier-operation-queue
        (let ((request (pop atelier-operation-queue)))
          (if (or (cl-some (lambda (operation)
                            (atelier-operation-conflicts-p (nth 1 request) operation))
                          atelier-operation-active)
                  (cl-some (lambda (earlier)
                             (or (memq :all (nth 1 earlier)) (memq :all (nth 1 request))
                                 (cl-intersection (nth 1 earlier) (nth 1 request) :test #'equal)))
                           retained))
              (push request retained)
            (condition-case error
                (progn
                  (unless (frame-live-p (nth 3 request))
                    (user-error "Originating frame no longer exists"))
                  (unless (equal (frame-parameter (nth 3 request) 'atelier-workspace-id)
                                 (nth 5 request))
                    (user-error "Originating frame changed workspace while the action waited"))
                  (with-selected-frame (nth 3 request)
                    (if (buffer-live-p (nth 4 request))
                        (with-current-buffer (nth 4 request)
                          (funcall (nth 2 request)))
                      (user-error "Originating buffer no longer exists"))))
              ((error quit)
               (atelier-log "Queued operation %s failed: %s" (car request)
                            (error-message-string error)))))))
      (setq atelier-operation-queue (nreverse retained)))))

(defun atelier-operation-call (name ids function &optional compose retry)
  "Prepare FUNCTION's changes to IDS, validate, and publish them together.
Conflicting independent requests queue; explicit COMPOSE joins preparation.
Record publication cannot undo already executed external process commands."
  (cond
   ((and compose atelier-operation-current)
    (unless (memq :all (atelier-operation-ids atelier-operation-current))
      (dolist (id ids)
        (unless (member id (atelier-operation-ids atelier-operation-current))
          (unless (and (atelier-workspace-by-id id)
                       (not (cl-find id (atelier-operation-original atelier-operation-current)
                                     :key #'atelier-workspace-id :test #'equal)))
            (error "Operation %s did not declare all affected workspaces" name))
          (push id (atelier-operation-ids atelier-operation-current)))))
    (funcall function))
   ((or (cl-some (lambda (operation) (atelier-operation-conflicts-p ids operation))
                 atelier-operation-active)
        (and (not atelier-operation-draining)
             (cl-some (lambda (request)
                        (or (memq :all ids) (memq :all (nth 1 request))
                            (cl-intersection ids (nth 1 request) :test #'equal)))
                      atelier-operation-queue)))
    (setq atelier-operation-queue
          (append atelier-operation-queue
                  (list (list name ids (or retry (lambda () (atelier-operation-call name ids function)))
                              (selected-frame) (current-buffer)
                              (atelier-current-workspace-id)))))
    (message "Queued Atelier operation: %s" name)
    :queued)
   (t
    (let* ((live (if atelier-operation-live-state (car atelier-operation-live-state)
                  (or atelier-operation-live-workspaces atelier-workspaces)))
           (live-cache (or atelier-operation-live-cache atelier-content-live-buffers))
           (baseline (copy-tree live))
           (operation (atelier-operation-create :name name :ids ids :original baseline
                                                 :buffers (buffer-list)))
           (mapping (make-hash-table :test #'eq))
           (live-state (or atelier-operation-live-state (list live)))
           draft cache result published)
      (push operation atelier-operation-active)
      (unwind-protect
          (progn
            (let ((atelier-operation-current operation)
                  (atelier-operation-live-workspaces live)
                  (atelier-operation-live-cache live-cache)
                  (atelier-operation-live-state live-state)
                  (atelier-workspaces (copy-tree live))
                  (atelier-content-live-buffers (copy-hash-table live-cache))
                  (atelier-model-workspace nil)
                  (atelier-entry-owners (make-hash-table :test #'eq :weakness 'key)))
              (dolist (workspace atelier-workspaces)
                (atelier-workspace-index-entries workspace)
                (when (or (memq :all ids) (member (atelier-workspace-id workspace) ids))
                  (dolist (entry (atelier-workspace-entries workspace))
                    (atelier-entry-ensure-content entry workspace))))
              (atelier-operation-touch-frame (selected-frame))
              (setq result (funcall function)
                    ids (atelier-operation-ids operation)
                    draft atelier-workspaces cache atelier-content-live-buffers
                    live (car live-state))
              (dolist (check (atelier-operation-checks operation)) (funcall check))
              (setq live (car live-state))
              (when (atelier-operation-invalid operation)
                (user-error "Operation cancelled: a process or buffer changed during preparation"))
              (let ((all (memq :all ids)))
                (dolist (old baseline)
                  (let* ((id (plist-get old :id))
                         (current (cl-find id live :key #'atelier-workspace-id :test #'equal))
                         (candidate (cl-find id draft :key #'atelier-workspace-id :test #'equal)))
                    (if (or all (member id ids))
                        (unless (equal old current)
                          (user-error "Operation cancelled: workspace %s changed during preparation" id))
                      (unless (equal old candidate)
                        (error "Operation %s changed undeclared workspace %s" name id)))))
                (let ((merged
                        (if all draft (append
                        (cl-loop for old in live
                                 for id = (atelier-workspace-id old)
                                 for candidate = (cl-find id draft :key #'atelier-workspace-id :test #'equal)
                                 unless (and (or all (member id ids)) (null candidate))
                                 collect (if (or all (member id ids)) candidate old))
                        (cl-remove-if (lambda (workspace)
                                        (cl-find (atelier-workspace-id workspace) baseline
                                                 :key #'atelier-workspace-id :test #'equal)) draft)))))
                  (dolist (workspace merged)
                    (when (or all (member (atelier-workspace-id workspace) ids))
                      (atelier-workspace-refresh-parent-ids workspace)))
                  (atelier-operation-validate merged)
                  (let ((merged-cache (copy-hash-table live-cache)))
                    (maphash (lambda (key _buffer)
                               (when (or all (member (car key) ids)) (remhash key merged-cache)))
                             live-cache)
                    (maphash (lambda (key buffer)
                               (when (or all (member (car key) ids)
                                         (not (cl-find (car key) baseline :key #'atelier-workspace-id :test #'equal)))
                                 (puthash key buffer merged-cache))) cache)
                    (atelier-operation-validate-cache merged merged-cache))
                  (let ((inhibit-quit t))
                  (setq live
                        (mapcar
                         (lambda (workspace)
                           (let ((old (cl-find (atelier-workspace-id workspace) live
                                               :key #'atelier-workspace-id :test #'equal)))
                             (if (and old (not (eq old workspace)))
                                 (atelier-operation-reconcile old workspace mapping)
                                (puthash workspace workspace mapping)
                                workspace))) merged))
                  (maphash (lambda (key _buffer)
                             (when (or all (member (car key) ids)) (remhash key live-cache))) live-cache)
                  (maphash (lambda (key buffer)
                             (when (or all (member (car key) ids)
                                       (not (cl-find (car key) baseline :key #'atelier-workspace-id :test #'equal)))
                                (puthash key buffer live-cache))) cache))))
              (dolist (selection (atelier-operation-frames operation))
                (when (frame-live-p (car selection))
                  (set-frame-parameter (car selection) 'atelier-workspace-id (cdr selection))))
              (setq published t))
            (setcar live-state live)
            (if atelier-operation-current
                (setq atelier-operation-live-workspaces live)
              (setq atelier-workspaces live))
            (dolist (workspace live) (atelier-workspace-index-entries workspace))
            (atelier-operation-live-event
             (lambda ()
               (dolist (function (nreverse (atelier-operation-completion operation)))
                 (condition-case error (funcall function)
                   ((error quit) (atelier-log "Completed operation %s cleanup failed: %s" name
                                             (error-message-string error)))))
               (dolist (notification (nreverse (atelier-operation-notifications operation)))
                 (apply #'atelier-operation-notify (car notification)
                        (mapcar (lambda (argument) (atelier-operation-result argument mapping))
                                (cdr notification))))))
            (atelier-operation-result result mapping))
        (unless published
          (setq live (car live-state))
          (if atelier-operation-current
              (setq atelier-operation-live-workspaces live)
            (setq atelier-workspaces live))
          (let ((atelier-operation-current nil)
                (atelier-workspaces live)
                (atelier-content-live-buffers live-cache))
             (dolist (cleanup (atelier-operation-cleanups operation))
              (condition-case error (funcall cleanup)
                ((error quit) (atelier-log "Operation %s cleanup failed: %s" name
                                           (error-message-string error)))))
            (dolist (state (atelier-operation-navigator-states operation))
              (when (frame-live-p (car state))
                (setq atelier-navigator-window-configurations
                      (assq-delete-all (car state) atelier-navigator-window-configurations)
                      atelier-navigator-selection-by-frame
                      (assq-delete-all (car state) atelier-navigator-selection-by-frame))
                (when (nth 1 state)
                  (push (cons (car state) (nth 1 state)) atelier-navigator-window-configurations))
                (when (nth 2 state)
                  (push (cons (car state) (nth 2 state)) atelier-navigator-selection-by-frame))
                (when (buffer-live-p (nth 3 state))
                  (with-current-buffer (nth 3 state)
                    (setq atelier-navigator-changed-views (nth 4 state))))))
            (dolist (configuration (atelier-operation-windows operation))
              (when (frame-live-p (car configuration))
                (set-window-configuration (cdr configuration))))))
        (setq atelier-operation-active (delq operation atelier-operation-active))
        (atelier-operation-drain))))))

(provide 'atelier-operation)
;;; atelier-operation.el ends here

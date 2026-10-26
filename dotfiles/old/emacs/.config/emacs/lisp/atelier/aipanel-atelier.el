;;; aipanel-atelier.el --- Buffer-attached panels beside Atelier -*- lexical-binding: t; -*-

(require 'aipan)
(require 'atelier)
(require 'atelier-persist)

(defalias 'aipanel-atelier-owner #'aipanel-default-owner
  "Compatibility name for AIPanel's source-owned attachment.")
(defalias 'aipanel-atelier-context #'aipanel-default-context
  "Compatibility name for AIPanel's source-relative context.")

(defun aipanel-atelier-buffer-created ()
  "Prevent Atelier from capturing the current AIPanel terminal."
  (atelier-set-buffer-excluded t))

(defun aipanel-atelier-buffer-exited ()
  "Release the current panel's Atelier exclusion on exit."
  (atelier-set-buffer-excluded nil))

(defun aipanel-atelier-workspace-process-buffers (workspace)
  "Return panels attached to WORKSPACE's unshared live source buffers."
  (let (panels)
    (dolist (source (atelier-workspace-live-content-buffers workspace))
      (unless (atelier-buffer-shared-with-running-workspace-p source workspace)
        (dolist (id (buffer-local-value 'aipanel-attached-panel-ids source))
          (when-let* ((panel (get-buffer (gethash id aipanel-sessions))))
            (push panel panels)))))
    (delete-dups panels)))

(defun aipanel-atelier-buffer-owner (buffer)
  "Name a panel after the workspace of the source buffer it sits beside."
  (when (aipanel-buffer-p buffer)
    (let ((source (plist-get (buffer-local-value 'aipanel-owner buffer) :source-buffer)))
      (cons (or (and (buffer-live-p source) (car (atelier-buffer-owner source)))
                (atelier-ensure-detached-workspace))
            'aipanel))))

(defun aipanel-atelier-travel (buffer)
  "Show panel BUFFER beside its source, in the workspace Traveller selected."
  (when (aipanel-buffer-p buffer)
    (let ((source (plist-get (buffer-local-value 'aipanel-owner buffer) :source-buffer)))
      (unless (buffer-live-p source) (user-error "The panel's source buffer was closed"))
      (atelier-show-buffer source (or (car (atelier-buffer-owner source))
                                      (atelier-current-workspace)))
      (with-current-buffer buffer (setq aipanel-hidden nil))
      (aipanel-sync-source-visibility)
      (when-let* ((window (get-buffer-window buffer)))
        (select-window window))
      t)))

;;; Saved panels

(defun aipanel-atelier-agent-job-p (job)
  "Whether a pre-format-6 terminal JOB ran a coding agent."
  (or (consp (plist-get job :agent))
      (and (plist-get job :agent)
           (member (file-name-nondirectory (or (car (plist-get job :direct-command)) ""))
                   '("opencode" "claude" "codex" "fx")))))

(defun aipanel-atelier-upgrade-types (data)
  "Type format 5 agent jobs as panels before the core types them as terminals."
  (atelier-legacy-map-entries
   (copy-tree data)
   (lambda (entry)
     (when (and (eq (plist-get entry :kind) 'terminal)
                (not (plist-member entry :type))
                (plist-get entry :job)
                (aipanel-atelier-agent-job-p (plist-get entry :job)))
       (atelier-legacy-set entry :type 'aipanel)))))

(defun aipanel-atelier-attached-p (entry)
  "Whether saved panel ENTRY records the source entry it sits beside."
  (stringp (plist-get (plist-get (plist-get (plist-get entry :job) :agent) :attachment)
                      :entry-id)))

(defun aipanel-atelier-upgrade-attachments (data)
  "Drop format 6 and 7 workspace-level panels, which had no source buffer."
  (let ((data (atelier-legacy-remove-entries
               data (lambda (entry)
                      (and (eq (plist-get entry :type) 'aipanel)
                           (not (aipanel-atelier-attached-p entry)))))))
    (dolist (workspace (plist-get data :workspaces) data)
      (cl-remf workspace :agent-directory))))

(defun aipanel-atelier-validate (workspace entry data)
  "Reject saved state where panel ENTRY is not attached to another buffer."
  (let* ((attachment (plist-get (plist-get (atelier-entry-job entry) :agent) :attachment))
         (source-id (plist-get attachment :entry-id))
         (workspaces (plist-get data :workspaces))
         (source (cl-loop for candidate in workspaces
                          thereis (atelier-entry-by-id candidate source-id))))
    (unless (and (stringp source-id) source
                 (not (cl-some (lambda (owner)
                                 (and (atelier-entry-by-id owner source-id)
                                      (eq (atelier-entry-value source :type owner) 'aipanel)))
                               workspaces)))
      (error "Invalid AIPanel attachment in workspace %s" (atelier-workspace-name workspace)))))

(defun aipanel-atelier-setup ()
  "Keep AIPanel side windows separate from workspace entries and jobs."
  (atelier-define-type 'aipanel :tracked t :buffer-p #'aipanel-buffer-p
                       :validate #'aipanel-atelier-validate)
  (atelier-define-upgrade-step 5 #'aipanel-atelier-upgrade-types)
  (atelier-define-upgrade-step 6 #'aipanel-atelier-upgrade-attachments)
  (atelier-define-upgrade-step 7 #'aipanel-atelier-upgrade-attachments)
  (setq aipanel-owner-function #'aipanel-atelier-owner
        aipanel-context-function #'aipanel-atelier-context)
  (add-hook 'aipanel-buffer-created-hook #'aipanel-atelier-buffer-created)
  (add-hook 'aipanel-buffer-exited-hook #'aipanel-atelier-buffer-exited)
  (add-hook 'atelier-workspace-process-buffers-functions
            #'aipanel-atelier-workspace-process-buffers)
  (add-hook 'atelier-buffer-owner-functions #'aipanel-atelier-buffer-owner)
  (add-hook 'atelier-traveller-open-functions #'aipanel-atelier-travel)
  (aipanel-follow-source-setup))

(provide 'aipanel-atelier)
;;; aipanel-atelier.el ends here

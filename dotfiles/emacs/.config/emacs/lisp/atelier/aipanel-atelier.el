;;; aipanel-atelier.el --- Buffer-attached panels beside Atelier -*- lexical-binding: t; -*-

(require 'aipan)
(require 'atelier)

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

(defun aipanel-atelier-setup ()
  "Keep AIPanel side windows separate from workspace entries and jobs."
  (atelier-register-entry-type 'aipanel "aipanel" #'aipanel-buffer-p)
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

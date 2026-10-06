;;; atelier-traveller.el --- Travel to any workspace buffer -*- lexical-binding: t; -*-

;; Traveller lists every buffer a workspace owns, open or saved, as
;; "WORKSPACE | TYPE | NAME".  Typed words match anywhere in that text, each
;; allowing gaps between its letters.  Choosing a buffer first selects its
;; workspace, as the navigator does, then shows the buffer there.

(require 'cl-lib)
(require 'subr-x)
(require 'atelier-model)
(require 'atelier-naming)

(declare-function atelier-switch-workspace "atelier")
(declare-function atelier-show-buffer "atelier")
(declare-function atelier-restore-entry-content "atelier")
(declare-function atelier-main-window "atelier-navigator")
(defvar orderless-matching-styles)

(defvar atelier-traveller-open-functions nil
  "Functions called with a live buffer that no stack holds.
Traveller has already selected the buffer's workspace.  The first function
returning non-nil has shown the buffer; otherwise Traveller switches to it.")

(defun atelier-traveller-log-buffer-names ()
  "Emacs's own log buffers; Detached owns them without storing them."
  (remove "*Completions*" atelier-global-buffer-names))

(defun atelier-traveller-content-name (content type buffer)
  "Return the NAME part shown for CONTENT, live in BUFFER or only saved."
  (cond
   (buffer (atelier-buffer-strip-qualifier (buffer-name buffer) type))
   ((and (eq type 'dired) (plist-get content :directory))
    (atelier-buffer-folder-name (plist-get content :directory)))
   ((plist-get content :name)
    (atelier-buffer-strip-qualifier (plist-get content :name) type))
   ((plist-get content :file) (file-name-nondirectory (plist-get content :file)))
   (t "unnamed")))

(defun atelier-traveller-targets ()
  "Return every buffer Traveller can reach, as plists with a :label."
  (let (targets held)
    (dolist (workspace atelier-workspaces)
      (dolist (content (plist-get workspace :contents))
        (let* ((type (or (plist-get content :type) 'buffer))
               (buffer (gethash (atelier-content-cache-key workspace (plist-get content :id))
                                atelier-content-live-buffers))
               (buffer (and (buffer-live-p buffer) buffer)))
          (when buffer (push buffer held))
          (push (list :label (atelier-buffer-qualified-name
                              workspace type (atelier-traveller-content-name content type buffer))
                      :workspace-id (atelier-workspace-id workspace)
                      :content-id (plist-get content :id)
                      :saved (not buffer))
                targets))))
    (dolist (buffer (buffer-list))
      (unless (memq buffer held)
        (when-let* ((owner (if (member (buffer-name buffer) (atelier-traveller-log-buffer-names))
                               (list (atelier-ensure-detached-workspace) 'buffer)
                             (atelier-buffer-owner buffer))))
          (let ((type (or (nth 1 owner) 'buffer)))
            (push (list :label (atelier-buffer-qualified-name
                                (car owner) type
                                (atelier-buffer-strip-qualifier (buffer-name buffer) type))
                        :workspace-id (atelier-workspace-id (car owner))
                        :buffer buffer)
                  targets)))))
    (setq targets (nreverse targets))
    (let ((seen (make-hash-table :test #'equal)))
      (dolist (target targets targets)
        (let* ((label (plist-get target :label))
               (count (cl-incf (gethash label seen 0))))
          (when (> count 1)
            (setf (plist-get target :label) (format "%s <%d>" label count))))))))

(defun atelier-traveller-read (targets)
  "Choose one of TARGETS by label, matching letters with gaps."
  (let* ((labels (mapcar (lambda (target) (plist-get target :label)) targets))
         (annotate (lambda (label)
                     (when (plist-get (cl-find label targets :key (lambda (target)
                                                                    (plist-get target :label))
                                               :test #'equal)
                                      :saved)
                       "  saved")))
         (table (lambda (string predicate action)
                  (if (eq action 'metadata)
                      `(metadata (category . atelier-traveller)
                                 (annotation-function . ,annotate))
                    (complete-with-action action labels string predicate))))
         (choice (atelier-traveller-with-matching
                  (lambda () (completing-read "Traveller: " table nil t)))))
    (cl-find choice targets :key (lambda (target) (plist-get target :label)) :test #'equal)))

(defun atelier-traveller-with-matching (function)
  "Call FUNCTION with each typed word matching letters in order, with gaps."
  (if (featurep 'orderless)
      (let ((completion-styles '(orderless))
            (orderless-matching-styles '(orderless-flex)))
        (funcall function))
    (let ((completion-styles '(flex)))
      (funcall function))))

(atelier-define-operation atelier-traveller-show-content (workspace-id content-id)
    (list workspace-id) nil
  "Show CONTENT-ID of WORKSPACE-ID, restoring it first when only saved."
  (let* ((workspace (atelier-operation-workspace workspace-id))
         (content (or (atelier-workspace-content workspace content-id)
                      (user-error "Buffer no longer exists")))
         (buffer (atelier-restore-entry-content
                  (atelier-content-reference workspace content) workspace content-id)))
    (when (buffer-live-p buffer)
      (atelier-show-buffer buffer workspace))
    buffer))

(defun atelier-traveller-open (target)
  "Select TARGET's workspace, then show TARGET there."
  (let ((workspace (or (atelier-workspace-by-id (plist-get target :workspace-id))
                       (user-error "Workspace no longer exists"))))
    (when (window-parameter nil 'window-side)
      (select-window (atelier-main-window)))
    (unless (equal (atelier-current-workspace-id) (atelier-workspace-id workspace))
      (atelier-switch-workspace (plist-get workspace :name))
      (unless (equal (atelier-current-workspace-id) (atelier-workspace-id workspace))
        (user-error "Workspace %s is not open yet; travel again once it is"
                    (plist-get workspace :name))))
    (if-let* ((content-id (plist-get target :content-id)))
        (atelier-traveller-show-content (atelier-workspace-id workspace) content-id)
      (let ((buffer (plist-get target :buffer)))
        (unless (buffer-live-p buffer) (user-error "Buffer no longer exists"))
        (unless (run-hook-with-args-until-success 'atelier-traveller-open-functions buffer)
          (switch-to-buffer buffer))))))

(defun atelier-traveller ()
  "Travel to any workspace's buffer, open or saved."
  (interactive)
  (atelier-traveller-open
   (or (atelier-traveller-read (atelier-traveller-targets))
       (user-error "No buffer chosen"))))

(provide 'atelier-traveller)
;;; atelier-traveller.el ends here

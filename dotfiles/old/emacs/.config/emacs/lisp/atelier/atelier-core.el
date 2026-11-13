;;; atelier-core.el --- Atelier's private support operations -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'subr-x)
(require 'project)
(defvar read-eval)
(defvar atelier-approved-buffer-closes nil)
(defvar atelier-operation-closing-immediately)
(defvar atelier-preserve-job-recipe)
(declare-function atelier-operation-after "atelier-operation")
(declare-function atelier-operation-check "atelier-operation")

(defgroup atelier nil "Workspace entries and restoration." :group 'environment)
(defcustom atelier-state-directory
  (expand-file-name "atelier/" (or (getenv "XDG_STATE_HOME")
                                  (expand-file-name ".local/state/" "~")))
  "Private directory for Atelier snapshots and restore journals."
  :type 'directory :group 'atelier)
(defcustom atelier-close-without-asking nil
  "Discard unsaved edits and bypass buffer/process questions when closing.
When nil, closing follows the normal Emacs buffer and process controls."
  :type 'boolean :group 'atelier)

(defun atelier-log (format-string &rest args)
  (let ((text (apply #'format format-string args)))
    (with-current-buffer (get-buffer-create "*atelier-log*")
      (goto-char (point-max))
      (insert (format-time-string "[%Y-%m-%d %H:%M:%S] ") text "\n"))
    (message "%s" text)))

(defun atelier-normalize-directory (directory)
  (file-name-as-directory (expand-file-name directory)))

(defun atelier-safe-name (value)
  (string-trim (replace-regexp-in-string "[^[:alnum:]_-]+" "-" (downcase value)) "-+" "-+"))

(defun atelier-project-root (&optional directory)
  (let* ((default-directory (or directory default-directory))
         (project (project-current nil default-directory)))
    (atelier-normalize-directory (if project (project-root project) default-directory))))

(defun atelier-ensure-private-directory (directory)
  (make-directory directory t)
  (set-file-modes directory #o700))

(defun atelier-write-data-atomically (file value)
  (atelier-ensure-private-directory (file-name-directory file))
  (let ((temporary (make-temp-file (expand-file-name ".state-" (file-name-directory file)))))
    (unwind-protect
        (progn
          (with-temp-file temporary
            (let ((print-length nil) (print-level nil) (print-circle t))
              (prin1 value (current-buffer))
              (insert "\n")))
          (set-file-modes temporary #o600)
          (rename-file temporary file t))
      (when (file-exists-p temporary) (delete-file temporary)))))

(defun atelier-read-data (file)
  (when (file-readable-p file)
    (with-temp-buffer
      (insert-file-contents file)
      (let ((read-eval nil) (read-circle t)) (read (current-buffer))))))

(defun atelier-buffer-close-state (buffer)
  "Return the file state covered by BUFFER's close permission."
  (with-current-buffer buffer
    (list buffer-file-name
          (and buffer-file-name (buffer-chars-modified-tick))
          (and buffer-file-name (buffer-modified-p)))))

(defun atelier-validate-buffer-closes (&optional approvals)
  "Reject stale close permission before changing buffer ownership."
  (unless atelier-close-without-asking
    (dolist (approval (or approvals atelier-approved-buffer-closes))
      (when (and (buffer-live-p (car approval))
                 (not (equal (cdr approval) (atelier-buffer-close-state (car approval)))))
        (user-error "Close cancelled because buffer %s changed after its close question"
                    (buffer-name (car approval)))))))

(defun atelier-kill-buffer (buffer)
  "Close BUFFER using the configured file and process question policy."
  (if (and (bound-and-true-p atelier-operation-current)
           (not atelier-operation-closing-immediately))
      (let ((approvals (if (or (not (buffer-live-p buffer))
                              (assq buffer atelier-approved-buffer-closes))
                          atelier-approved-buffer-closes
                        (cons (atelier-prepare-buffer-close buffer)
                              atelier-approved-buffer-closes)))
            (without-asking atelier-close-without-asking)
            (preserve (bound-and-true-p atelier-preserve-job-recipe)))
        (atelier-operation-check
         (lambda ()
           (let ((atelier-close-without-asking without-asking))
             (atelier-validate-buffer-closes approvals))))
        (atelier-operation-after
         (lambda ()
           (let ((atelier-approved-buffer-closes approvals)
                 (atelier-close-without-asking without-asking)
                 (atelier-preserve-job-recipe preserve))
             (atelier--kill-buffer buffer))))
        t)
    (atelier--kill-buffer buffer)))

(defun atelier--kill-buffer (buffer)
  "Perform a checked close after preparation or as an explicit job replacement."
  (when (buffer-live-p buffer)
    (when-let* ((approval (assq buffer atelier-approved-buffer-closes)))
      (atelier-validate-buffer-closes (list approval)))
    (if (or atelier-close-without-asking (assq buffer atelier-approved-buffer-closes))
        (progn
          (when atelier-close-without-asking
            (when-let* ((process (get-buffer-process buffer)))
              (set-process-query-on-exit-flag process nil)
              (when (process-live-p process) (delete-process process))))
          (if (not (buffer-live-p buffer))
              t
            (with-current-buffer buffer
              (when (and buffer-file-name (buffer-modified-p))
                (set-buffer-modified-p nil))
              (let ((kill-buffer-query-functions nil)) (kill-buffer buffer)))))
      (or (kill-buffer buffer) (user-error "Buffer close cancelled")))))

(defun atelier-prepare-buffer-close (buffer)
  "Return permission tied to BUFFER's state before removing its references."
  (when (buffer-live-p buffer)
    (if-let* ((approval (assq buffer atelier-approved-buffer-closes)))
        (progn (atelier-validate-buffer-closes (list approval)) approval)
      (with-current-buffer buffer
        (unless (or atelier-close-without-asking
                    (not (and buffer-file-name (buffer-modified-p)))
                    (kill-buffer--possibly-save buffer))
          (user-error "Buffer close cancelled"))
        (let ((approval (cons buffer (atelier-buffer-close-state buffer))))
          (unless (or atelier-close-without-asking
                      (run-hook-with-args-until-failure 'kill-buffer-query-functions))
            (user-error "Buffer close cancelled"))
          (atelier-validate-buffer-closes (list approval))
          approval)))))

(provide 'atelier-core)
;;; atelier-core.el ends here

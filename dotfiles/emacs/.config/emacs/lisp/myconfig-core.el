;;; myconfig-core.el --- Shared workbench foundations -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'subr-x)

(defgroup myconfig nil "One stateful Emacs workbench." :group 'environment)

(defvar read-eval)
(defvar myconfig-data-directory)
(declare-function myconfig-ui-setup "myconfig-ui")
(declare-function myconfig-editing-setup "myconfig-editing")
(declare-function myconfig-terminal-setup "myconfig-terminal")
(declare-function myconfig-git-setup "myconfig-git")
(declare-function myconfig-bindings-setup "myconfig-bindings")
(declare-function atelier-persist-setup "atelier-persist")

(defconst myconfig-state-directory
  (or (bound-and-true-p myconfig-runtime-state-directory)
      (expand-file-name "myconfig-emacs/" user-emacs-directory)))
(defconst myconfig-log-buffer "*atelier-log*")

(declare-function atelier-persist-setup "atelier-persist")

(defvar myconfig-initialized-p nil)
(defvar myconfig-after-initialize-hook nil)

(defun myconfig-log (format-string &rest args)
  (let ((message (apply #'format format-string args)))
    (with-current-buffer (get-buffer-create myconfig-log-buffer)
      (goto-char (point-max))
      (insert (format-time-string "[%Y-%m-%d %H:%M:%S] ") message "\n"))
    (message "%s" message)))

(defun myconfig-ensure-private-directory (directory)
  (make-directory directory t)
  (set-file-modes directory #o700))

(defun myconfig-write-data-atomically (file value)
  (myconfig-ensure-private-directory (file-name-directory file))
  (let ((temporary (make-temp-file (expand-file-name ".state-" (file-name-directory file)))))
    (unwind-protect
        (progn
          (with-temp-file temporary
            (let ((print-length nil) (print-level nil) (print-circle t))
              (prin1 value (current-buffer))
              (insert "\n")))
          (set-file-modes temporary #o600)
          (rename-file temporary file t))
      (when (file-exists-p temporary)
        (delete-file temporary)))))

(defun myconfig-read-data (file)
  (when (file-readable-p file)
    (with-temp-buffer
      (insert-file-contents file)
      (let ((read-eval nil)
            (read-circle t))
        (read (current-buffer))))))

(defun myconfig-local-path-p (path)
  (not (file-remote-p path)))

(defun myconfig-normalize-directory (directory)
  (file-name-as-directory (expand-file-name directory)))

(defun myconfig-safe-name (value)
  (let ((name (replace-regexp-in-string "[^[:alnum:]_-]+" "-" (downcase value))))
    (string-trim name "-+" "-+")))

(defun myconfig-project-root (&optional directory)
  (let* ((default-directory (or directory default-directory))
         (project (project-current nil default-directory)))
    (if project
        (myconfig-normalize-directory (project-root project))
      (myconfig-normalize-directory default-directory))))

(defun myconfig-initialize ()
  (unless myconfig-initialized-p
    (condition-case error
        (progn
          (myconfig-ensure-private-directory myconfig-state-directory)
          (myconfig-ui-setup)
          (atelier-setup)
          (myconfig-editing-setup)
          (myconfig-terminal-setup)
          (when (fboundp 'aipanel-atelier-setup)
            (aipanel-atelier-setup))
          (myconfig-git-setup)
          (myconfig-bindings-setup)
          (atelier-persist-setup)
          (run-hooks 'myconfig-after-initialize-hook)
          (setq myconfig-initialized-p t)
          (myconfig-log "Atelier initialized"))
      (error
       (myconfig-log "Atelier initialization failed: %s" (error-message-string error))
       (signal (car error) (cdr error))))))

(provide 'myconfig-core)
;;; myconfig-core.el ends here

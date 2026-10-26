;;; myconfig-core.el --- Shared workbench foundations -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'subr-x)

(declare-function project-root "project")

(defgroup myconfig nil "One stateful Emacs workbench." :group 'environment)

(defvar myconfig-data-directory)

(defcustom myconfig-workspace-inactive-timeout (* 4 60 60)
  "Seconds away from every frame before stopping a workspace, or nil."
  :type '(choice (const :tag "Disabled" nil) (number :tag "Seconds"))
  :group 'myconfig)

(defconst myconfig-state-directory
  (or (bound-and-true-p myconfig-runtime-state-directory)
      (expand-file-name "myconfig-emacs/" user-emacs-directory)))
(defconst myconfig-log-buffer "*myconfig-log*")

(defun myconfig-log (format-string &rest args)
  (let ((message (apply #'format format-string args)))
    (with-current-buffer (get-buffer-create myconfig-log-buffer)
      (goto-char (point-max))
      (insert (format-time-string "[%Y-%m-%d %H:%M:%S] ") message "\n"))
    (message "%s" message)))

(defun myconfig-ensure-private-directory (directory)
  (make-directory directory t)
  (set-file-modes directory #o700))

(defun myconfig-normalize-directory (directory)
  (file-name-as-directory (expand-file-name directory)))

(defun myconfig-project-root (&optional directory)
  (let* ((default-directory (or directory default-directory))
         (project (project-current nil default-directory)))
    (if project
        (myconfig-normalize-directory (project-root project))
      (myconfig-normalize-directory default-directory))))

(provide 'myconfig-core)
;;; myconfig-core.el ends here

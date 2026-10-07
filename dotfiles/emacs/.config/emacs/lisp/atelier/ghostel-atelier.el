;;; ghostel-atelier.el --- Ghostel jobs and buffers in Atelier -*- lexical-binding: t; -*-

(require 'atelier)
(require 'atelier-persist)
(require 'ghostel)
(defvar ghostel-mode-hook nil)

(defvar ghostel-atelier-default-shell-function (lambda () shell-file-name))

(defun ghostel-atelier-exec-buffer (name directory program args &optional identity)
  "Start a Ghostel buffer without registering an Atelier job."
  (let ((buffer (atelier-operation-track-buffer
                 (generate-new-buffer (generate-new-buffer-name name)))))
    (condition-case error
        (progn
          (with-current-buffer buffer (setq-local default-directory directory))
          (ghostel-exec buffer program args identity)
          (with-current-buffer buffer
            (when-let* ((_ atelier-close-without-asking)
                        (process (get-buffer-process buffer)))
              (set-process-query-on-exit-flag process nil)))
          buffer)
      (error
       (when (buffer-live-p buffer) (kill-buffer buffer))
       (signal (car error) (cdr error))))))

(atelier-define-operation ghostel-atelier-buffer
    (&optional name directory command args owner-workspace shell agent type)
    (list (atelier-workspace-id (or owner-workspace (atelier-current-workspace))))
    ((owner-workspace (atelier-operation-workspace (or owner-workspace (atelier-current-workspace))))
     (atelier-job-owner-entry
      (and atelier-job-owner-entry (atelier-operation-entry owner-workspace atelier-job-owner-entry))))
  "Start a terminal and register its restartable job through Atelier."
  (let* ((desired-directory (or directory (atelier-workspace-directory)))
         (default-directory desired-directory)
         (program (or command (plist-get shell :executable)
                      (funcall ghostel-atelier-default-shell-function)))
         (name (or name (and type (atelier-entry-buffer-name type)) "terminal"))
         (buffer (if command
                     (ghostel-atelier-exec-buffer name desired-directory program args)
                    (let* ((ghostel-mode-hook
                            (cons (lambda () (atelier-operation-track-buffer (current-buffer)))
                                  ghostel-mode-hook))
                           (ghostel-shell (cons program args))
                          (created (ghostel-create name)))
                     created))))
    (with-current-buffer buffer
      (setq-local default-directory desired-directory)
      (when atelier-close-without-asking
        (setq-local kill-buffer-query-functions
                    (remq #'process-kill-buffer-query-function kill-buffer-query-functions)))
      (atelier-set-buffer-excluded nil buffer)
      (when-let* ((_ atelier-close-without-asking)
                  (process (get-buffer-process buffer)))
        (set-process-query-on-exit-flag process nil))
      (add-hook 'ghostel-exit-functions #'ghostel-atelier-process-exited nil t))
    (let ((atelier-job-owner-workspace owner-workspace)
          (shell (or shell (unless command
                            (list :executable program :login (member "-l" args))))))
      (atelier-register-job-buffer
       buffer shell desired-directory
       (when (and command (null shell)) (cons program args)) nil agent type))
    buffer))

(defun ghostel-atelier-process-exited (buffer _event)
  (atelier-job-process-exited buffer))

(defun ghostel-atelier-buffer-p (buffer)
  (with-current-buffer buffer (derived-mode-p 'ghostel-mode)))

(defun ghostel-atelier-kind (buffer)
  (when (ghostel-atelier-buffer-p buffer) 'terminal))

(defun ghostel-atelier-title (buffer)
  (when (local-variable-p 'ghostel-title buffer)
    (buffer-local-value 'ghostel-title buffer)))

(defun ghostel-atelier-process-id (buffer)
  (with-current-buffer buffer
    (or (and (boundp 'ghostel--pid) ghostel--pid)
        (when-let* ((process (get-buffer-process buffer))) (process-id process)))))

(defun ghostel-atelier-setup ()
  (atelier-register-entry-type 'terminal "terminal" #'ghostel-atelier-buffer-p)
  (add-hook 'atelier-buffer-kind-functions #'ghostel-atelier-kind)
  (add-hook 'atelier-buffer-title-functions #'ghostel-atelier-title)
  (setq atelier-job-start-function #'ghostel-atelier-buffer
        atelier-job-process-id-function #'ghostel-atelier-process-id))

(provide 'ghostel-atelier)
;;; ghostel-atelier.el ends here

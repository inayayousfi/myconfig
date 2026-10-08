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
         (name (or name (and type (atelier-type-label type)) "terminal"))
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

(defun ghostel-atelier-compile-buffer-p (&optional buffer)
  "Return non-nil when BUFFER shows a `ghostel-compile' run, not a terminal.
Ghostel sets this identity after its mode hooks, so check it when acting."
  (eq (alist-get 'kind (buffer-local-value 'ghostel-identity
                                           (or buffer (current-buffer))))
      'compile))

(defun ghostel-atelier-buffer-p (buffer)
  "Return non-nil when BUFFER is a plain terminal or a program Ghostel runs.
Compile runs and panels that other packages create are not terminals."
  (with-current-buffer buffer
    (and (derived-mode-p 'ghostel-mode)
         (memq (alist-get 'kind ghostel-identity) '(nil term exec)))))

(defun ghostel-atelier-capture (_buffer)
  (atelier-job-capture))

(defun ghostel-atelier-title (buffer)
  (when (local-variable-p 'ghostel-title buffer)
    (buffer-local-value 'ghostel-title buffer)))

(defvar-local ghostel-atelier-start-base nil
  "The NAME part the terminal had before its first title.")
(defvar-local ghostel-atelier-title-base nil
  "The NAME part this adapter last gave; another one is a name given by hand.")

(defun ghostel-atelier-apply-title (buffer title)
  "Make TITLE the NAME part of terminal BUFFER.
A nil TITLE restores the name the terminal started with."
  (with-current-buffer buffer
    (let ((title (and title (string-trim (replace-regexp-in-string "[[:cntrl:]]+" " " title)))))
      (setq atelier-buffer-base-name
            (if (and title (not (string-empty-p title))) title ghostel-atelier-start-base)
            ghostel-atelier-title-base atelier-buffer-base-name))
    (if (or atelier-operation-active atelier-operation-queue)
        (atelier-schedule-naming)
      (atelier-name-buffer buffer))))

(defun ghostel-atelier-buffer-name (title)
  "Follow terminal TITLE in the Atelier name; Ghostel never renames.
Ghostel calls it as `ghostel-buffer-name-function' on title and folder changes."
  (let ((buffer (current-buffer)))
    (when (eq (nth 1 (atelier-buffer-owner buffer)) 'terminal)
      (let ((base (atelier-buffer-base buffer 'terminal)))
        (unless ghostel-atelier-start-base
          (setq ghostel-atelier-start-base base
                ghostel-atelier-title-base base))
        (when (equal base ghostel-atelier-title-base)
          (ghostel-atelier-apply-title buffer title)))))
  nil)

(defun ghostel-atelier-resume-title ()
  "Drop the name given by hand so this terminal follows its title again."
  (interactive)
  (unless (and (ghostel-atelier-buffer-p (current-buffer))
               (eq (nth 1 (atelier-buffer-owner (current-buffer))) 'terminal))
    (user-error "This buffer is not a workspace terminal"))
  (unless ghostel-atelier-start-base
    (setq ghostel-atelier-start-base (atelier-buffer-base (current-buffer) 'terminal)))
  (ghostel-atelier-apply-title (current-buffer) ghostel-title))

(defun ghostel-atelier-process-id (buffer)
  (with-current-buffer buffer
    (or (and (boundp 'ghostel--pid) ghostel--pid)
        (when-let* ((process (get-buffer-process buffer))) (process-id process)))))

(defun ghostel-atelier-setup ()
  (atelier-define-type 'terminal
    :tracked t
    :buffer-p #'ghostel-atelier-buffer-p
    :capture #'ghostel-atelier-capture)
  (add-hook 'atelier-buffer-title-functions #'ghostel-atelier-title)
  (setq ghostel-buffer-name-function #'ghostel-atelier-buffer-name
        atelier-job-start-function #'ghostel-atelier-buffer
        atelier-job-process-id-function #'ghostel-atelier-process-id))

(provide 'ghostel-atelier)
;;; ghostel-atelier.el ends here

;;; myconfig-terminal.el --- Ghostel terminal integration -*- lexical-binding: t; -*-

(require 'myconfig-core)
(defvar myconfig-data-directory)
(defvar ghostel-module-directory)
(defvar ghostel-module-auto-install)
(declare-function myconfig-paste "myconfig-bindings")
(setq ghostel-module-directory
      (expand-file-name "ghostel-module/" myconfig-data-directory)
      ghostel-module-auto-install 'download)
(require 'ghostel)
(require 'evil-ghostel)

(defcustom myconfig-terminal-escape-key (kbd "M-x")
  "Key sequence that returns a Ghostel terminal to Evil normal state."
  :type 'key-sequence
  :group 'myconfig)
(require 'atelier)
(require 'univers)

(defun myconfig-terminal-buffer-name (name)
  (let ((name (or name "terminal")))
    (if (string-prefix-p "*" name)
        name
      (format "*%s*" (string-trim name "*" "*")))))

(defun myconfig-terminal-buffer
    (&optional name directory command args owner-workspace shell agent type explicit)
  (let* ((desired-directory (or directory (atelier-workspace-directory)))
         (default-directory desired-directory)
         (program (or command (plist-get shell :executable) (universel-default-shell)))
         (name (if (and owner-workspace type)
                   (atelier-entry-buffer-name type owner-workspace)
                 (myconfig-terminal-buffer-name name)))
         buffer)
    (condition-case error
        (setq buffer
              (if command
                  (let ((buffer (generate-new-buffer (generate-new-buffer-name name))))
                    (with-current-buffer buffer
                      (setq-local default-directory desired-directory))
                    (condition-case error
                        (progn
                          (ghostel-exec buffer program args)
                          buffer)
                      (error
                       (when (buffer-live-p buffer) (kill-buffer buffer))
                       (signal (car error) (cdr error)))))
                (let ((ghostel-shell (cons program args)))
                  (ghostel-create name))))
      (error (signal (car error) (cdr error))))
    (with-current-buffer buffer
       (setq-local default-directory desired-directory
                    myconfig-terminal-command (cons program args)
                   kill-buffer-query-functions
                   (remq #'process-kill-buffer-query-function kill-buffer-query-functions))
       (remhash buffer atelier-internal-buffers)
      (when-let* ((process (get-buffer-process buffer)))
        (set-process-query-on-exit-flag process nil))
      (add-hook 'ghostel-exit-functions #'myconfig-terminal-process-exited nil t))
    (let ((atelier-job-owner-workspace owner-workspace)
          (shell (or shell (unless command
                              (list :executable program :login (member "-l" args))))))
      (atelier-register-job-buffer
       buffer shell desired-directory
       (when (and command (null shell)) (cons program args)) nil agent type explicit))
    buffer))

(defun myconfig-terminal-process-exited (buffer _event)
  (when (fboundp 'atelier-job-process-exited)
    (atelier-job-process-exited buffer)))

(defun myconfig-terminal ()
  (interactive)
  (when (window-parameter nil 'window-side)
    (select-window (window-main-window)))
  (let* ((workspace (atelier-current-workspace))
         (in-terminal (derived-mode-p 'ghostel-mode))
          (existing (and workspace (not in-terminal)
                         (atelier-workspace-buffer-by-type workspace 'terminal)))
          (launch (unless existing (funcall atelier-terminal-command-function workspace)))
         (name (atelier-entry-buffer-name 'terminal workspace))
         (buffer (or existing
                     (myconfig-terminal-buffer
                      (if in-terminal
                          (generate-new-buffer-name (myconfig-terminal-buffer-name name))
                        name)
                        (plist-get launch :directory)
                        (plist-get launch :program)
                        (plist-get launch :arguments)
                         workspace
                         (when (plist-get launch :shell)
                           (list :executable (plist-get launch :shell)
                                 :login (member "-l" (plist-get launch :arguments))))
                         nil 'terminal in-terminal))))
    (switch-to-buffer buffer)))

(defun myconfig-terminal-split-right ()
  (interactive)
  (let ((window (split-window-right)))
    (select-window window)
    (myconfig-terminal)))

(defun myconfig-terminal-split-below ()
  (interactive)
  (let ((window (split-window-below)))
    (select-window window)
    (myconfig-terminal)))

(defun myconfig-terminal-escape ()
  (interactive)
  (ghostel-emacs-mode)
  (evil-local-mode 1)
  (evil-ghostel-mode 1)
  (evil-normal-state))

(defun myconfig-terminal-enter-input ()
  "Give the terminal process all keyboard input through Ghostel char mode."
  (interactive)
  (when (bound-and-true-p evil-ghostel-mode)
    (evil-ghostel-mode -1))
  (when (bound-and-true-p evil-local-mode)
    (evil-local-mode -1))
  (ghostel-char-mode)
  (setq buffer-read-only nil))

(defun myconfig-terminal-display-setup ()
  (setq buffer-read-only nil)
  (add-hook 'post-command-hook #'myconfig-terminal-keep-writable nil t)
  (display-line-numbers-mode -1)
  (hl-line-mode -1)
  (myconfig-terminal-enter-input))

(defun myconfig-terminal-keep-writable ()
  (when (derived-mode-p 'ghostel-mode)
    (setq buffer-read-only nil)))

(defun myconfig-terminal-activate (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (when (and (derived-mode-p 'ghostel-mode)
                 (process-live-p (get-buffer-process buffer)))
        (myconfig-terminal-enter-input)))))

(defun myconfig-terminal-setup ()
  (setq ghostel-kill-buffer-on-exit t
        ghostel-query-before-killing nil
        ghostel-term "xterm-256color"
        evil-ghostel-escape 'terminal
        evil-ghostel-initial-state 'normal
        confirm-kill-processes nil)
  (setq-default kill-buffer-query-functions
                (remq #'process-kill-buffer-query-function
                      (default-value 'kill-buffer-query-functions)))
  (evil-set-initial-state 'ghostel-mode 'normal)
  (add-hook 'ghostel-mode-hook #'myconfig-terminal-display-setup)
  (define-key ghostel-mode-map myconfig-terminal-escape-key #'myconfig-terminal-escape)
  (define-key ghostel-char-mode-map (kbd "C-S-v") #'myconfig-paste)
  (define-key evil-ghostel-mode-map myconfig-terminal-escape-key #'myconfig-terminal-escape)
  (evil-define-key 'normal evil-ghostel-mode-map
    (kbd "i") #'myconfig-terminal-enter-input
    (kbd "a") #'myconfig-terminal-enter-input
    (kbd "I") #'myconfig-terminal-enter-input
    (kbd "A") #'myconfig-terminal-enter-input))

(provide 'myconfig-terminal)
;;; myconfig-terminal.el ends here

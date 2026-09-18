;;; myconfig-terminal.el --- Ghostel terminal integration -*- lexical-binding: t; -*-

(require 'myconfig-core)
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
(require 'myconfig-windows)

(defun myconfig-terminal-buffer-name (name)
  (let ((name (or name "terminal")))
    (if (string-prefix-p "*" name)
        name
      (format "*%s*" (string-trim name "*" "*")))))

(defun myconfig-terminal-buffer (&optional name directory command args owner-workspace shell agent orphan)
  (let* ((desired-directory (or directory (atelier-workspace-directory)))
         (wsl (and owner-workspace (myconfig-wsl-workspace-p owner-workspace)))
         (process-directory (if wsl (myconfig-home-directory) desired-directory))
         (default-directory process-directory)
         (program (or command (myconfig-default-shell)))
         (name (myconfig-terminal-buffer-name name))
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
                   atelier-buffer-global orphan
                   myconfig-terminal-command (cons program args)
                  kill-buffer-query-functions
                  (remq #'process-kill-buffer-query-function kill-buffer-query-functions))
      (when-let* ((process (get-buffer-process buffer)))
        (set-process-query-on-exit-flag process nil))
      (add-hook 'ghostel-exit-functions #'myconfig-terminal-process-exited nil t))
    (let ((atelier-job-owner-workspace owner-workspace)
          (shell (or shell (unless command
                             (list :executable program :login (member "-l" args))))))
       (unless orphan
         (atelier-register-job-buffer
          buffer shell desired-directory
          (when (and command (null shell)) (cons program args)) nil agent)))
    buffer))

(defun myconfig-terminal-process-exited (buffer _event)
  (when (fboundp 'myconfig-job-process-exited)
    (myconfig-job-process-exited buffer)))

(defun myconfig-terminal ()
  (interactive)
  (when (window-parameter nil 'window-side)
    (select-window (window-main-window)))
  (let* ((workspace (atelier-current-workspace))
         (in-terminal (derived-mode-p 'ghostel-mode))
         (existing (and (not in-terminal)
                        (atelier-find-workspace-buffer
                         (lambda (buffer workspace)
                           (with-current-buffer buffer
                              (and (derived-mode-p 'ghostel-mode)
                                   (process-live-p (get-buffer-process buffer))
                                   (not (when-let* ((owner (atelier-find-job-for-buffer
                                                            (buffer-name buffer))))
                                          (plist-get (nth 1 owner) :agent)))))))))
          (remote (not (equal (plist-get workspace :destination) "local")))
          (wsl (myconfig-wsl-workspace-p workspace))
          (windows (myconfig-windows-workspace-p workspace))
          (orphan (atelier-buffer-orphaned-p))
         (name (format "terminal:%s" atelier-current-workspace-name))
         (buffer (or existing
                     (myconfig-terminal-buffer
                      (if in-terminal
                          (generate-new-buffer-name (myconfig-terminal-buffer-name name))
                        name)
                        (if (or orphan wsl remote)
                            (myconfig-home-directory)
                          (atelier-workspace-directory))
                       (cond (wsl "wsl.exe")
                             (windows "ssh")
                             (remote "ssh"))
                       (cond (wsl (list "-d" (plist-get workspace :destination)
                                        "--cd" (plist-get workspace :path) "--" "bash" "-l"))
                             (windows (myconfig-windows-powershell-arguments workspace))
                              (remote (list (plist-get workspace :destination)))
                              (t '("-l")))
                       workspace nil t orphan))))
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
  (evil-normal-state))

(defun myconfig-terminal-display-setup ()
  (display-line-numbers-mode -1)
  (hl-line-mode -1))

(defun myconfig-terminal-activate (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (when (and (derived-mode-p 'ghostel-mode)
                 (process-live-p (get-buffer-process buffer)))
        (ghostel-semi-char-mode)
        (evil-insert-state)))))

(defun myconfig-terminal-setup ()
  (setq ghostel-kill-buffer-on-exit t
        ghostel-query-before-killing nil
        ghostel-term "xterm-256color"
        ghostel-keymap-exceptions (delete "C-x" (copy-sequence ghostel-keymap-exceptions))
        evil-ghostel-escape 'terminal
        evil-ghostel-initial-state 'insert
        confirm-kill-processes nil)
  (ghostel--rebuild-semi-char-keymap)
  (setq-default kill-buffer-query-functions
                (remq #'process-kill-buffer-query-function
                      (default-value 'kill-buffer-query-functions)))
  (evil-set-initial-state 'ghostel-mode 'insert)
  (add-hook 'ghostel-mode-hook #'evil-ghostel-mode)
  (add-hook 'ghostel-mode-hook #'myconfig-terminal-display-setup)
   (define-key ghostel-mode-map myconfig-terminal-escape-key #'myconfig-terminal-escape)
   (define-key evil-ghostel-mode-map myconfig-terminal-escape-key #'myconfig-terminal-escape))

(provide 'myconfig-terminal)
;;; myconfig-terminal.el ends here

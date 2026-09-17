;;; myconfig-platform.el --- Platform-aware paths -*- lexical-binding: t; -*-

(defun myconfig-windows-host-p ()
  (eq system-type 'windows-nt))

(defun myconfig-wsl-host-p ()
  (and (eq system-type 'gnu/linux)
       (or (getenv "WSL_DISTRO_NAME")
           (and (file-readable-p "/proc/sys/kernel/osrelease")
                (with-temp-buffer
                  (insert-file-contents "/proc/sys/kernel/osrelease")
                  (goto-char (point-min))
                  (search-forward "microsoft" nil t))))))

(defun myconfig-ssh-config-file ()
  (expand-file-name ".ssh/config" (myconfig-home-directory)))

(defun myconfig-home-directory ()
  (file-name-as-directory (expand-file-name "~/")))

(defun myconfig-platform-directory (kind)
  (let ((xdg (getenv (pcase kind
                       ('data "XDG_DATA_HOME")
                       ('state "XDG_STATE_HOME")
                       ('cache "XDG_CACHE_HOME")))))
    (file-name-as-directory
     (expand-file-name
      (or xdg
          (and (myconfig-windows-host-p)
               (getenv (pcase kind
                         ('data "LOCALAPPDATA")
                         ('state "LOCALAPPDATA")
                         ('cache "LOCALAPPDATA"))))
          (pcase kind
            ('data "~/.local/share")
            ('state "~/.local/state")
            ('cache "~/.cache")))))))

(defun myconfig-platform-path (kind &rest components)
  (expand-file-name (mapconcat #'identity components "/")
                    (myconfig-platform-directory kind)))

(defun myconfig-default-shell ()
  (if (myconfig-windows-host-p)
      (or (executable-find "pwsh.exe")
          (executable-find "powershell.exe")
          "powershell.exe")
    (or (getenv "SHELL") shell-file-name "/bin/sh")))

(defun myconfig-platform-powershell-quote (value)
  (concat "'" (replace-regexp-in-string "'" "''" value t t) "'"))

(defun myconfig-platform-run-command-lines (command timeout)
  "Run COMMAND and return its non-empty output lines.

Native Windows WSL output needs to pass through PowerShell because `wsl.exe'
does not reliably expose its redirected stdout to Emacs.  The selected
PowerShell executable is explicit, so the caller's parent shell is irrelevant."
  (with-temp-buffer
    (let* ((output-file (when (myconfig-windows-host-p)
                          (make-temp-file "myconfig-process-" nil ".txt")))
           (windows-command
            (when output-file
              (format "& %s 2>&1 | Set-Content -Encoding utf8 %s"
                      (concat (myconfig-platform-powershell-quote (car command)) " "
                              (mapconcat
                               (lambda (argument)
                                 (if (string-match-p "[/ ]" argument)
                                     (myconfig-platform-powershell-quote argument)
                                   argument))
                               (cdr command) " "))
                      (myconfig-platform-powershell-quote output-file))))
           (process (make-process
                     :name "myconfig-platform-command"
                     :buffer (current-buffer)
                     :command (if windows-command
                                  (list (or (executable-find "pwsh.exe")
                                            "powershell.exe")
                                        "-NoProfile" "-NonInteractive"
                                        "-Command" windows-command)
                                command)
                     :connection-type 'pipe
                     :noquery t
                     :sentinel #'ignore))
           (deadline (+ (float-time) timeout)))
      (while (and (process-live-p process) (< (float-time) deadline))
        (accept-process-output process 0.05))
      (when (process-live-p process)
        (delete-process process))
      (when output-file
        (when (file-exists-p output-file)
          (insert-file-contents output-file))
        (delete-file output-file))
      (when (or output-file
                (and (eq (process-status process) 'exit)
                     (zerop (process-exit-status process))))
        (let ((lines (split-string (buffer-string) "[\r\n]+" t)))
          (when (and lines (string-prefix-p "\ufeff" (car lines)))
            (setcar lines (string-remove-prefix "\ufeff" (car lines))))
          lines)))))

(provide 'myconfig-platform)
;;; myconfig-platform.el ends here

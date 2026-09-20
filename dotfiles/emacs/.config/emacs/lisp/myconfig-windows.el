;;; myconfig-windows.el --- Native Windows remote adapter -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'subr-x)
(require 'myconfig-core)

(when (myconfig-windows-host-p)
  (require 'tramp)
  (add-to-list
   'tramp-methods
   '("wsl"
     (tramp-login-program "C:/Windows/System32/wsl.exe")
     (tramp-login-args
      (("-d") ("%h") ("-u" "%u")
       ("-e" "/bin/sh" "-c" "\"exec 2>&1" "%l" "\"")))
     (tramp-remote-shell "/bin/sh")
     (tramp-remote-shell-login ("-l"))
     (tramp-remote-shell-args ("-c")))))

(defvar myconfig-windows-mounts (make-hash-table :test #'equal))

(defun myconfig-wsl-workspace-p (workspace)
  (eq (plist-get workspace :platform) 'wsl))

(defun myconfig-wsl-workspace-directory (workspace)
  (format "/wsl:%s:%s" (plist-get workspace :destination)
          (file-name-as-directory (plist-get workspace :path))))

(defun myconfig-wsl-remote-path (workspace local-path)
  (file-remote-p local-path 'localname))

(defun myconfig-windows-workspace-p (workspace)
  (eq (plist-get workspace :platform) 'windows))

(defun myconfig-windows-mount-key (workspace)
  (concat (plist-get workspace :destination) "\0"
          (or (plist-get workspace :mount-root) "/C:/")))

(defun myconfig-windows-mount-point (workspace)
  (let* ((key (myconfig-windows-mount-key workspace))
         (name (myconfig-safe-name (plist-get workspace :destination)))
         (digest (substring (secure-hash 'sha256 key) 0 12)))
    (expand-file-name (format "mounts/%s-%s/" name digest) myconfig-state-directory)))

(defun myconfig-windows-mounted-p (directory)
  (zerop (process-file "mountpoint" nil nil nil "--quiet" directory)))

(defun myconfig-windows-mount-sentinel (process event)
  (unless (process-live-p process)
    (when-let* ((key (process-get process 'myconfig-windows-mount-key)))
      (when (eq process (gethash key myconfig-windows-mounts))
        (remhash key myconfig-windows-mounts)))
    (unless (or (process-get process 'myconfig-windows-intentional-stop)
                (string-match-p "finished" event))
      (myconfig-log "Windows SFTP mount exited: %s" (string-trim event)))))

(defun myconfig-windows-ensure-mount (workspace)
  (unless (executable-find "sshfs")
    (user-error "Windows workspace requires the sshfs package"))
  (let* ((key (myconfig-windows-mount-key workspace))
         (existing (gethash key myconfig-windows-mounts))
         (mount-point (myconfig-windows-mount-point workspace))
         (source (format "%s:%s" (plist-get workspace :destination)
                         (or (plist-get workspace :mount-root) "/C:/"))))
    (if (and existing (process-live-p existing)
             (myconfig-windows-mounted-p mount-point))
        mount-point
      (myconfig-ensure-private-directory mount-point)
      (let* ((buffer (get-buffer-create
                      (format "*windows-mount:%s*" (plist-get workspace :destination))))
             (process
              (make-process
               :name (format "windows-mount:%s" (plist-get workspace :destination))
               :buffer buffer
               :command
               (list "sshfs" "-f" source mount-point
                     "-o" (string-join
                           '("BatchMode=yes" "ConnectTimeout=10" "ServerAliveInterval=5"
                             "ServerAliveCountMax=2" "auto_unmount" "idmap=user")
                           ","))
               :connection-type 'pipe
               :noquery t
               :sentinel #'myconfig-windows-mount-sentinel))
             (deadline (+ (float-time) 10)))
        (process-put process 'myconfig-windows-mount-key key)
        (puthash key process myconfig-windows-mounts)
        (while (and (process-live-p process)
                    (not (myconfig-windows-mounted-p mount-point))
                    (< (float-time) deadline))
          (accept-process-output process 0.1))
        (unless (myconfig-windows-mounted-p mount-point)
          (when (process-live-p process) (delete-process process))
          (remhash key myconfig-windows-mounts)
          (when (and (file-directory-p mount-point)
                     (null (directory-files mount-point nil directory-files-no-dot-files-regexp)))
            (delete-directory mount-point))
          (user-error "Windows SFTP mount failed for %s; see %s"
                      (plist-get workspace :destination) (buffer-name buffer)))
        mount-point))))

(defun myconfig-windows-workspace-directory (workspace)
  (let* ((mount-root (or (plist-get workspace :mount-root) "/C:/"))
         (path (file-name-as-directory (plist-get workspace :path)))
         (relative (file-relative-name path mount-root)))
    (expand-file-name relative (myconfig-windows-ensure-mount workspace))))

(defun myconfig-windows-remote-path (workspace local-path)
  (let* ((local-root (myconfig-windows-workspace-directory workspace))
         (relative (file-relative-name local-path local-root)))
    (expand-file-name relative (file-name-as-directory (plist-get workspace :path)))))

(defun myconfig-windows-native-path (path)
  (replace-regexp-in-string "/" (string ?\\) (string-remove-prefix "/" path) t t))

(defun myconfig-windows-powershell-arguments (workspace)
  (let* ((destination (plist-get workspace :destination))
         (path (replace-regexp-in-string
                "'" "''" (myconfig-windows-native-path (plist-get workspace :path)) t t))
         (command (format "powershell.exe -NoLogo -NoExit -Command \"Set-Location -LiteralPath '%s'\""
                          path)))
    (list destination command)))

(defun myconfig-windows-unmount (workspace)
  (let* ((key (myconfig-windows-mount-key workspace))
         (process (gethash key myconfig-windows-mounts))
         (mount-point (myconfig-windows-mount-point workspace)))
    (when (and process (process-live-p process))
      (process-put process 'myconfig-windows-intentional-stop t)
      (delete-process process))
    (when (myconfig-windows-mounted-p mount-point)
      (process-file "fusermount3" nil nil nil "--unmount" mount-point))
    (remhash key myconfig-windows-mounts)
    (when (and (file-directory-p mount-point)
               (null (directory-files mount-point nil directory-files-no-dot-files-regexp)))
      (delete-directory mount-point))))

(defun myconfig-windows-unmount-unused (workspace)
  (let ((key (myconfig-windows-mount-key workspace)))
    (unless (cl-some (lambda (other)
                       (and (not (eq other workspace))
                            (eq (atelier-workspace-status other) 'running)
                            (myconfig-windows-workspace-p other)
                            (equal key (myconfig-windows-mount-key other))))
                     atelier-workspaces)
      (myconfig-windows-unmount workspace))))

(provide 'myconfig-windows)
;;; myconfig-windows.el ends here

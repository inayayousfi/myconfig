;;; universel-atelier.el --- Optional Universel connection for Atelier -*- lexical-binding: t; -*-

(require 'univers)
(require 'atelier)
(require 'atelier-persist)

(defvar universel-atelier-state-directory nil
  "Directory for Atelier's remote mounts; configured by the application.")

(defun universel-atelier-environment (workspace)
  "Translate WORKSPACE's existing record into a Universel environment."
  (let* ((platform (plist-get workspace :platform))
         (destination (plist-get workspace :destination))
         (local (equal destination "local"))
         (port (when (and (not local) (not (eq platform 'wsl))
                          (string-match "#\\([0-9]+\\)\\'" destination))
                 (prog1 (match-string 1 destination)
                   (setq destination (substring destination 0 (match-beginning 0)))))))
    (list :platform (if local (universel-host-platform)
                      (if (eq platform 'windows) 'windows 'posix))
          :transport (cond (local 'local) ((eq platform 'wsl) 'wsl) (t 'ssh))
          :destination (unless local destination)
          :port port
          :directory (plist-get workspace :path)
          :mount-root (plist-get workspace :mount-root))))

(defun universel-atelier-directory (workspace)
  (let* ((environment (universel-atelier-environment workspace))
         (state-directory universel-atelier-state-directory)
         (connected (universel-files-connected-p environment state-directory)))
    (prog1 (universel-file-directory (plist-get workspace :path) environment state-directory)
      (when (and atelier-operation-current (not connected)
                 (universel-files-connected-p environment state-directory))
        (let ((owner (or (atelier-operation-live-event
                          (lambda () (atelier-workspace-by-id (plist-get workspace :id))))
                         (copy-tree workspace))))
          (atelier-operation-cleanup
           (lambda () (universel-atelier-release owner))))))))

(defun universel-atelier-execution-directory (workspace directory)
  (if (equal (plist-get workspace :destination) "local") directory
    (universel-execution-path directory (universel-atelier-environment workspace)
                              universel-atelier-state-directory)))

(defun universel-atelier-target-directory (workspace directory)
  (universel-file-path directory (universel-atelier-environment workspace)
                       universel-atelier-state-directory))

(defun universel-atelier-terminal-command (workspace)
  (universel-shell-command (plist-get workspace :path)
                            (universel-atelier-environment workspace)))

(defun universel-atelier-local-path-p (workspace path)
  "Identify local PATH without connecting to WORKSPACE's mounted files."
  (and (atelier-default-local-path-p workspace path)
       (not (and universel-atelier-state-directory
                 (universel-mounted-environment
                  (file-name-as-directory path)
                  (universel-atelier-environment workspace)
                  universel-atelier-state-directory)))))

(defun universel-atelier-release (workspace &optional force)
  "Release WORKSPACE's files, preserving sharing unless FORCE is non-nil."
  (let ((environment (universel-atelier-environment workspace)))
    (when (and (eq (plist-get environment :transport) 'ssh)
               (eq (plist-get environment :platform) 'windows)
               (or force
                   (not (cl-some
                         (lambda (other)
                           (and (not (eq other workspace))
                                (eq (atelier-workspace-status other) 'running)
                                (eq (plist-get other :platform) 'windows)
                                (equal (universel-mount-key environment)
                                       (universel-mount-key (universel-atelier-environment other)))))
                         atelier-workspaces))))
      (universel-release-files environment universel-atelier-state-directory))))

(defun universel-atelier-detect-directory (directory)
  "Identify a Windows mount without opening any connection."
  (when universel-atelier-state-directory
    (cl-loop for workspace in atelier-workspaces
             for environment = (universel-atelier-environment workspace)
             thereis (universel-mounted-environment
                      directory environment universel-atelier-state-directory))))

(defun universel-atelier-setup (state-directory)
  "Connect Atelier to Universel, storing mounts below STATE-DIRECTORY."
  (setq universel-atelier-state-directory state-directory
        atelier-directory-function #'universel-atelier-directory
        atelier-execution-directory-function #'universel-atelier-execution-directory
        atelier-target-directory-function #'universel-atelier-target-directory
        atelier-terminal-command-function #'universel-atelier-terminal-command
        atelier-release-function #'universel-atelier-release
        atelier-local-path-p-function #'universel-atelier-local-path-p
        atelier-process-observation-function #'universel-atelier-process-observation-p
        atelier-process-table-function #'universel-atelier-process-table
        atelier-foreground-process-function #'universel-foreground-process
        atelier-process-runtime-function #'universel-atelier-process-runtime
        atelier-extra-destinations
        (universel-select '((windows "WSL")) (universel-host-platform)))
  (universel-register-wsl)
  (add-hook 'universel-environment-functions #'universel-atelier-detect-directory))

(defun universel-atelier-process-observation-p ()
  (universel-process-observation-p (universel-host-environment)))

(defun universel-atelier-process-table ()
  (universel-process-table (universel-host-environment)))

(defun universel-atelier-process-runtime (process)
  (universel-process-runtime process (universel-host-environment)))

(provide 'universel-atelier)
;;; universel-atelier.el ends here

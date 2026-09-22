;;; universel-atelier.el --- Optional Universel connection for Atelier -*- lexical-binding: t; -*-

(require 'univers)
(require 'atelier)

(defvar universel-atelier-state-directory nil
  "Directory for Atelier's remote mounts; configured by the application.")

(defun universel-atelier-environment (workspace)
  "Translate WORKSPACE's existing record into a Universel environment."
  (let* ((platform (plist-get workspace :platform))
         (destination (plist-get workspace :destination))
         (local (equal destination "local")))
    (list :platform (if local (universel-host-platform)
                      (if (eq platform 'windows) 'windows 'posix))
          :transport (cond (local 'local) ((eq platform 'wsl) 'wsl) (t 'ssh))
          :destination (unless local destination)
          :directory (plist-get workspace :path)
          :mount-root (plist-get workspace :mount-root))))

(defun universel-atelier-directory (workspace)
  (universel-file-directory (plist-get workspace :path)
                            (universel-atelier-environment workspace)
                            universel-atelier-state-directory))

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
        atelier-extra-destinations
        (universel-select '((windows "WSL")) (universel-host-platform)))
  (universel-register-wsl)
  (add-hook 'universel-environment-functions #'universel-atelier-detect-directory))

(provide 'universel-atelier)
;;; universel-atelier.el ends here

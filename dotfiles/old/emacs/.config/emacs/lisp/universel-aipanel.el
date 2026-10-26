;;; universel-aipanel.el --- Optional Universel connection for AIPanel -*- lexical-binding: t; -*-

(require 'univers)
(require 'aipan)

(defun universel-aipanel-source-owner (owner)
  "Resolve OWNER's source connection, including registered Windows mounts."
  (let* ((directory (plist-get owner :emacs-directory))
         (environment (universel-environment nil directory))
         (transport (plist-get environment :transport)))
    (setf (plist-get owner :platform) (plist-get environment :platform)
          (plist-get owner :location) (if (eq transport 'local) 'host transport)
          (plist-get owner :destination) (or (plist-get environment :destination) "local")
          (plist-get owner :port) (plist-get environment :port)
          (plist-get owner :directory)
          (universel-native-path (plist-get environment :directory) environment))
    owner))

(defun universel-aipanel-environment (owner &optional selection)
  "Translate AIPanel OWNER and SELECTION into a Universel environment."
  (let* ((location (or (plist-get selection :location) (plist-get owner :location) 'host))
         (platform (plist-get owner :platform)))
    (list :platform (if (eq location 'host) (universel-host-platform)
                      (if (eq platform 'windows) 'windows 'posix))
          :transport (if (eq location 'host) 'local location)
          :destination (or (plist-get selection :distribution)
                           (plist-get selection :destination)
                           (plist-get owner :destination))
          :port (or (plist-get selection :port) (plist-get owner :port))
          :directory (plist-get owner :directory))))

(defun universel-aipanel-probe (programs owner timeout)
  (universel-find-programs programs timeout (universel-aipanel-environment owner)))

(defun universel-aipanel-command (owner selection arguments)
  (universel-command (plist-get (plist-get selection :agent) :program)
                      arguments (plist-get owner :directory)
                      (universel-aipanel-environment owner selection)))

(defun universel-aipanel-skill-environments ()
  "Return this computer and every WSL distribution that reports its home."
  (let ((home (universel-home-directory)))
    (cons (list :location 'host :platform (universel-host-platform)
                :destination "local" :directory home :emacs-directory home)
          (cl-loop for distribution in (universel-wsl-distributions)
                   for home = (universel-wsl-home distribution)
                   when home
                   collect (list :location 'wsl :platform 'posix
                                 :destination distribution :directory home
                                 :emacs-directory
                                 (universel-file-directory
                                  home (list :platform 'posix :transport 'wsl
                                             :destination distribution
                                             :directory home)))))))

(defun universel-aipanel-setup ()
  "Supply cross-platform execution without coupling AIPanel to Universel."
  (setq aipanel-source-owner-function #'universel-aipanel-source-owner
        aipanel-program-probe-function #'universel-aipanel-probe
        aipanel-process-command-function #'universel-aipanel-command
        aipanel-skill-environments-function #'universel-aipanel-skill-environments))

(provide 'universel-aipanel)
;;; universel-aipanel.el ends here

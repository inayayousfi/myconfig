;;; universel-aipanel.el --- Optional Universel connection for AIPanel -*- lexical-binding: t; -*-

(require 'univers)
(require 'aipan)

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

(defun universel-aipanel-setup ()
  "Supply cross-platform execution without coupling AIPanel to Universel."
  (setq aipanel-program-probe-function #'universel-aipanel-probe
        aipanel-process-command-function #'universel-aipanel-command))

(provide 'universel-aipanel)
;;; universel-aipanel.el ends here

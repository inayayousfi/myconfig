;;; myconfig-init.el --- Personal application startup -*- lexical-binding: t; -*-

(require 'myconfig-core)

(defvar myconfig-initialized-p nil)
(defvar myconfig-after-initialize-hook nil)

(defun myconfig-initialize ()
  (unless myconfig-initialized-p
    (condition-case error
        (progn
          (myconfig-ensure-private-directory myconfig-state-directory)
          (myconfig-ui-setup)
          (setq tramp-connection-timeout 10)
          (atelier-setup)
          (xref-atelier-setup)
          (myconfig-editing-setup)
          (myconfig-terminal-setup)
          (myconfig-git-setup)
          (myconfig-bindings-setup)
          (atelier-persist-setup)
          (run-hooks 'myconfig-after-initialize-hook)
          (setq myconfig-initialized-p t)
          (myconfig-log "Atelier initialized"))
      (error
       (myconfig-log "Atelier initialization failed: %s" (error-message-string error))
       (signal (car error) (cdr error))))))

(provide 'myconfig-init)
;;; myconfig-init.el ends here

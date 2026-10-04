;;; remot-atelier.el --- Atelier context for browser frames -*- lexical-binding: t; -*-

(require 'remot)
(require 'atelier)

(defun remot-atelier-context (frame)
  (when-let* ((workspace (atelier-current-workspace frame)))
    (atelier-workspace-id workspace)))

(defun remot-atelier-initialize-frame (workspace-id frame)
  (when-let* ((workspace (atelier-workspace-by-id workspace-id)))
    (with-selected-frame frame
      (atelier-open-workspace workspace frame)
      (atelier-navigator))))

(defun remot-atelier-setup ()
  (setq remot-context-function #'remot-atelier-context
        remot-initialize-frame-function #'remot-atelier-initialize-frame))

(provide 'remot-atelier)
;;; remot-atelier.el ends here

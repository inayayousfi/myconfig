;;; aipanel-workbench.el --- Keep side panels outside Atelier -*- lexical-binding: t; -*-

(require 'aipan)
(require 'atelier)
(require 'myconfig-terminal)

(defun aipanel-workbench-owner ()
  "Attach to this buffer, using its workspace only for execution context."
  (let* ((buffer (current-buffer))
         (workspace (atelier-current-workspace))
         (emacs-directory (file-name-as-directory (expand-file-name default-directory)))
         (directory (funcall atelier-execution-directory-function
                             workspace emacs-directory)))
    (list :id buffer :name (buffer-name buffer) :source-buffer buffer
          :directory directory :emacs-directory emacs-directory
          :destination (plist-get workspace :destination)
          :platform (or (plist-get workspace :platform) 'local)
          :location (cond ((eq (plist-get workspace :platform) 'wsl) 'wsl)
                          ((equal (plist-get workspace :destination) "local") 'host)
                          (t 'ssh)))))

(defun aipanel-workbench-context (owner _panel)
  "Return the attached buffer's file position in its execution directory."
  (when-let* ((source (plist-get owner :source-buffer))
              ((buffer-live-p source)))
    (with-current-buffer source
      (when buffer-file-name
        (format "%s:L%d:C%d: "
                (file-relative-name buffer-file-name (plist-get owner :emacs-directory))
                (line-number-at-pos) (1+ (current-column)))))))

(defun aipanel-workbench-terminal (name directory program arguments _owner _selection)
  "Start the agent with the same terminal launcher as ordinary terminals."
  (myconfig-terminal-exec-buffer name directory program arguments
                                 '((kind . aipanel))))

(defun aipanel-workbench-activate-terminal ()
  "Keep the displayed agent in the same input mode as ordinary terminals."
  (myconfig-terminal-activate (current-buffer)))

(defun aipanel-workbench-sync-visibility ()
  "Show the running panel beside its selected source buffer, nowhere else."
  (dolist (frame (frame-list))
    (when (frame-live-p frame)
      (let* ((selected (frame-selected-window frame))
             (main (if (window-parameter selected 'window-side)
                       (window-main-window frame)
                     selected))
             (source (window-buffer main))
             (name (gethash source aipanel-sessions))
             (panel (and name (get-buffer name)))
             (wanted (and panel
                          (not (buffer-local-value 'aipanel-hidden panel))
                          (process-live-p (get-buffer-process panel))
                          panel)))
        (maphash (lambda (attached name)
                   (unless (eq attached source)
                     (when-let* ((buffer (get-buffer name)))
                       (with-current-buffer buffer (setq aipanel-hidden nil)))))
                 aipanel-sessions)
        (dolist (window (window-list frame 'no-minibuffer))
          (when (and (window-parameter window 'window-side)
                     (buffer-local-value 'aipanel-owner (window-buffer window))
                     (not (eq (window-buffer window) wanted)))
            (delete-window window)))
        (when (and wanted (not (get-buffer-window wanted frame)))
          (with-selected-frame frame
            (display-buffer-in-side-window
             wanted `((side . ,aipanel-side) (slot . 0)
                      (window-width . ,(max window-min-width
                                            (floor (* (frame-width) 0.3))))))
            (myconfig-terminal-activate wanted)))))))

(defun aipanel-workbench-buffer-created ()
  "Prevent Atelier from capturing the current AIPanel terminal."
  (atelier-set-buffer-excluded t))

(defun aipanel-workbench-buffer-exited ()
  "Release the current panel's Atelier exclusion on exit."
  (atelier-set-buffer-excluded nil))

(defun aipanel-workbench-setup ()
  "Keep AIPanel side windows separate from workspace entries and jobs."
  (setq aipanel-owner-function #'aipanel-workbench-owner
        aipanel-context-function #'aipanel-workbench-context
        aipanel-terminal-function #'aipanel-workbench-terminal)
  (add-hook 'aipanel-buffer-created-hook #'aipanel-workbench-buffer-created)
  (add-hook 'aipanel-buffer-exited-hook #'aipanel-workbench-buffer-exited)
  (add-hook 'aipanel-window-change-hook #'aipanel-workbench-activate-terminal)
  (add-hook 'post-command-hook #'aipanel-workbench-sync-visibility))

(provide 'aipanel-workbench)
;;; aipanel-workbench.el ends here

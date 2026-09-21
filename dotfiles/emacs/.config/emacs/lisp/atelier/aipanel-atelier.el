;;; aipanel-atelier.el --- Optional Atelier integration for AIPanel -*- lexical-binding: t; -*-

;;; Commentary:

;; Connects standalone AIPanel sessions to Atelier workspace ownership,
;; terminal jobs, remote context, and saved-job restoration.

;;; Code:

(require 'aipan)
(require 'atelier)
(require 'myconfig-terminal)
(require 'myconfig-windows)

(defun aipanel-atelier-local-directory (workspace)
  (or (plist-get workspace :agent-directory)
      (let* ((root (atelier-workspace-directory workspace))
             (current (and default-directory (myconfig-normalize-directory default-directory)))
             (choices (list (cons (format "Workspace root: %s" root) root)
                            (cons (format "Current directory: %s" current) current)))
             (choice (if (or (file-remote-p current) (equal current root))
                         root
                       (completing-read "AIPanel directory: " choices nil t nil nil
                                        (caar choices)))))
        (when-let* ((pair (and (stringp choice) (assoc-string choice choices))))
          (setq choice (cdr pair)))
        (setf (plist-get workspace :agent-directory) choice)
        choice)))

(defun aipanel-atelier-owner ()
  (let* ((workspace (atelier-current-workspace))
         (wsl (myconfig-wsl-workspace-p workspace))
         (remote (not (equal (plist-get workspace :destination) "local")))
         (directory (cond (wsl (plist-get workspace :path))
                          (remote (myconfig-home-directory))
                          (t (aipanel-atelier-local-directory workspace)))))
    (list :id (atelier-workspace-id workspace)
          :name (plist-get workspace :name)
          :directory directory
          :destination (plist-get workspace :destination)
          :path (plist-get workspace :path)
          :platform (plist-get workspace :platform)
          :workspace-id (atelier-workspace-id workspace))))

(defun aipanel-atelier-workspace (owner)
  (atelier-workspace-by-id (plist-get owner :workspace-id)))

(defun aipanel-atelier-command (owner selection mini)
  (let* ((agent (plist-get selection :agent))
         (wsl (eq (plist-get selection :location) 'wsl))
         (remote (not (equal (plist-get owner :destination) "local")))
         (workspace-wsl (eq (plist-get owner :platform) 'wsl))
         (directory (plist-get owner :directory))
         (wsl-directory (if (and remote (not workspace-wsl)) "~" directory))
         (arguments (aipanel-agent-arguments agent directory mini wsl)))
    (if wsl
        (list :program "wsl.exe" :directory (myconfig-home-directory)
              :arguments
              (append (when-let* ((distribution (plist-get selection :distribution)))
                        (list "-d" distribution))
                      (list "--cd" wsl-directory "--" (plist-get agent :program))
                      arguments))
      (list :program (plist-get agent :program) :directory directory
            :arguments arguments))))

(defun aipanel-atelier-context (owner buffer)
  (when-let* ((file buffer-file-name))
    (let* ((line (line-number-at-pos))
           (column (1+ (current-column)))
           (destination (plist-get owner :destination))
            (workspace (aipanel-atelier-workspace owner))
           (selection (buffer-local-value 'aipanel-selection buffer))
           (same-wsl (and (eq (plist-get owner :platform) 'wsl)
                          (eq (plist-get selection :location) 'wsl)
                          (equal destination (plist-get selection :distribution))))
           (root (if (myconfig-windows-workspace-p workspace)
                     (myconfig-windows-native-path (plist-get owner :path))
                   (plist-get owner :path))))
      (cond
       ((or (equal destination "local") same-wsl)
        (format "%s:L%d:C%d: "
                (if same-wsl
                    (file-remote-p file 'localname)
                  (or (file-relative-name file (atelier-workspace-directory workspace)) file))
                line column))
       ((eq (plist-get owner :platform) 'wsl)
        (format (concat "Workspace in WSL distribution %s. Its root is %s and the current "
                        "file is %s:L%d:C%d: ")
                destination root (file-remote-p file 'localname) line column))
       (t
        (let ((remote-file (if (myconfig-windows-workspace-p workspace)
                               (myconfig-windows-native-path
                                (myconfig-windows-remote-path workspace file))
                             (or (file-remote-p file 'localname) file))))
          (format (concat "Remote workspace accessible through OpenSSH destination %s. "
                          "Its root is %s and the current file is %s:L%d:C%d. "
                          "Run shell operations for this project through SSH to %s%s: ")
                  destination root remote-file line column destination
                  (if (myconfig-windows-workspace-p workspace)
                      " with PowerShell" ""))))))))

(defun aipanel-atelier-terminal (name directory program arguments owner selection)
  (let ((agent (plist-get selection :agent)))
    (myconfig-terminal-buffer
     name directory program arguments (aipanel-atelier-workspace owner) nil
     (list :id (plist-get agent :id)
           :location (plist-get selection :location)
           :distribution (plist-get selection :distribution))
      'aipanel nil)))

(defun aipanel-atelier-buffer-created ()
  (atelier-notify-change))

(defun aipanel-atelier-buffer-exited ()
  (unless (bound-and-true-p atelier-preserve-job-recipe)
    (when-let* ((owner (atelier-find-job-for-buffer (buffer-name)))
                (workspace (car owner))
                (entry (nth 2 owner)))
      (atelier-entry-remove workspace entry t)))
  (atelier-notify-change)
  (when (fboundp 'myconfig-persist-schedule) (myconfig-persist-schedule)))

(defun aipanel-atelier-window-changed ()
  (atelier-notify-change))

(defun aipanel-atelier-restore-buffer (buffer agent workspace)
  (when-let* ((configuration
               (cl-find (plist-get agent :id) aipanel-agents
                        :key (lambda (item) (plist-get item :id)))))
    (let ((owner (list :id (atelier-workspace-id workspace)
                        :name (plist-get workspace :name)
                       :directory (plist-get (nth 1 (atelier-find-job-for-buffer
                                                     (buffer-name buffer))) :directory)
                       :destination (plist-get workspace :destination)
                       :path (plist-get workspace :path)
                        :platform (plist-get workspace :platform)
                        :workspace-id (atelier-workspace-id workspace)))
          (selection (list :agent configuration
                           :location (plist-get agent :location)
                           :distribution (plist-get agent :distribution))))
      (aipanel-adopt-buffer buffer owner selection
                            (buffer-local-value 'myconfig-terminal-command buffer)))))

(defun aipanel-atelier-setup ()
  (setq aipanel-owner-function #'aipanel-atelier-owner
        aipanel-command-function #'aipanel-atelier-command
        aipanel-context-function #'aipanel-atelier-context
        aipanel-terminal-function #'aipanel-atelier-terminal)
  (add-hook 'aipanel-buffer-created-hook #'aipanel-atelier-buffer-created)
  (add-hook 'aipanel-buffer-exited-hook #'aipanel-atelier-buffer-exited)
  (add-hook 'aipanel-window-change-hook #'aipanel-atelier-window-changed)
  (add-hook 'atelier-agent-restored-functions #'aipanel-atelier-restore-buffer))

(provide 'aipanel-atelier)
;;; aipanel-atelier.el ends here

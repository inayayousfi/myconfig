;;; aipanel-atelier.el --- Optional Atelier integration for AIPanel -*- lexical-binding: t; -*-

;;; Commentary:

;; Connects each AIPanel session to one Atelier leaf entry and runs the agent
;; in that entry's execution environment and directory.

;;; Code:

(require 'aipan)
(require 'atelier)
(require 'myconfig-terminal)
(require 'myconfig-windows)

(defun aipanel-atelier-entry-directory (workspace entry)
  "Return ENTRY's live Emacs directory in WORKSPACE."
  (let* ((buffer (atelier-entry-live-buffer entry))
         (directory (or (and buffer (buffer-local-value 'default-directory buffer))
                        (plist-get entry :directory)
                        (atelier-workspace-directory workspace))))
    (file-name-as-directory (expand-file-name directory))))

(defun aipanel-atelier-execution-directory (workspace emacs-directory)
  "Map EMACS-DIRECTORY to the directory used inside WORKSPACE's agent host."
  (cond
   ((myconfig-windows-workspace-p workspace)
    (myconfig-windows-native-path
     (myconfig-windows-remote-path workspace emacs-directory)))
   ((file-remote-p emacs-directory)
    (file-name-as-directory (file-remote-p emacs-directory 'localname)))
   (t emacs-directory)))

(defun aipanel-atelier-owner ()
  "Return an AIPanel attachment for the current Atelier leaf entry."
  (let* ((workspace (atelier-current-workspace))
         (buffer (current-buffer))
         (entry (or (atelier-current-entry buffer workspace)
                    (atelier-register-buffer buffer workspace))))
    (unless entry
      (user-error "The current buffer is not an Atelier entry"))
    (when (eq (plist-get entry :type) 'aipanel)
      (user-error "An AIPanel cannot be attached to another AIPanel"))
    (atelier-update-entry-from-buffer entry buffer)
    (let* ((emacs-directory (aipanel-atelier-entry-directory workspace entry))
           (directory (aipanel-atelier-execution-directory workspace emacs-directory)))
      (list :id (plist-get entry :id)
            :name (or (plist-get entry :name) (buffer-name buffer))
            :source-buffer buffer
            :entry-id (plist-get entry :id)
            :workspace-id (atelier-workspace-id workspace)
            :directory directory
            :emacs-directory emacs-directory
            :destination (plist-get workspace :destination)
            :platform (or (plist-get workspace :platform) 'local)
            :location (cond ((myconfig-wsl-workspace-p workspace) 'wsl)
                            ((equal (plist-get workspace :destination) "local") 'host)
                            (t 'ssh))))))

(defun aipanel-atelier-workspace (owner)
  "Return the current workspace containing OWNER's attached entry."
  (or (atelier-entry-workspace (plist-get owner :entry-id))
      (atelier-workspace-by-id (plist-get owner :workspace-id))))

(defun aipanel-atelier-powershell-quote (value)
  (concat "'" (replace-regexp-in-string "'" "''" value t t) "'"))

(defun aipanel-atelier-powershell-encoded-command (script)
  (base64-encode-string (encode-coding-string script 'utf-16le t) t))

(defun aipanel-atelier-run-windows-probe (destination)
  "Return installed agent programs on Windows SSH DESTINATION."
  (when-let* ((ssh (executable-find "ssh")))
    (let* ((clauses
            (mapcar
             (lambda (program)
               (format "if (Get-Command -Name %s -ErrorAction SilentlyContinue) { Write-Output %s }"
                       (aipanel-atelier-powershell-quote program)
                       (aipanel-atelier-powershell-quote program)))
             (aipanel-programs)))
           (encoded (aipanel-atelier-powershell-encoded-command
                     (string-join clauses "; "))))
      (myconfig-platform-run-command-lines
       (list ssh destination
             (format "powershell.exe -NoProfile -NonInteractive -EncodedCommand %s"
                     encoded))
       aipanel-wsl-probe-timeout))))

(defun aipanel-atelier-candidates (owner)
  "Return installed agents in OWNER's Atelier execution environment."
  (if (eq (plist-get owner :platform) 'windows)
      (let ((destination (plist-get owner :destination)))
        (aipanel-candidates-for-programs
         (aipanel-atelier-run-windows-probe destination)
         'ssh destination (format "SSH: %s" destination)))
    (aipanel-default-candidates owner)))

(defun aipanel-atelier-command (owner selection mini)
  "Build the matching-environment command for OWNER and SELECTION."
  (if (eq (plist-get owner :platform) 'windows)
      (let* ((agent (plist-get selection :agent))
             (directory (plist-get owner :directory))
             (arguments (aipanel-agent-arguments agent directory mini t))
             (script
              (format "Set-Location -LiteralPath %s; & %s %s"
                      (aipanel-atelier-powershell-quote directory)
                      (aipanel-atelier-powershell-quote (plist-get agent :program))
                      (mapconcat #'aipanel-atelier-powershell-quote arguments " ")))
             (encoded (aipanel-atelier-powershell-encoded-command script)))
        (list :program "ssh" :directory (myconfig-home-directory)
              :arguments
              (list "-t" (plist-get owner :destination)
                    (format "powershell.exe -NoLogo -NoProfile -EncodedCommand %s"
                            encoded))))
    (aipanel-default-command owner selection mini)))

(defun aipanel-atelier-context (owner _buffer)
  "Return source context relative to OWNER's actual agent directory."
  (when-let* ((entry (atelier-entry-by-id (aipanel-atelier-workspace owner)
                                           (plist-get owner :entry-id)))
              (source (atelier-entry-live-buffer entry))
              ((buffer-live-p source)))
    (with-current-buffer source
      (when-let* ((file buffer-file-name))
        (format "%s:L%d:C%d: "
                (file-relative-name file (plist-get owner :emacs-directory))
                (line-number-at-pos) (1+ (current-column)))))))

(defun aipanel-atelier-persistent-attachment (owner)
  "Return OWNER fields needed to restore its Atelier attachment."
  (list :entry-id (plist-get owner :entry-id)
        :directory (plist-get owner :directory)
        :emacs-directory (plist-get owner :emacs-directory)
        :destination (plist-get owner :destination)
        :platform (plist-get owner :platform)
        :location (plist-get owner :location)))

(defun aipanel-atelier-terminal (name directory program arguments owner selection)
  (let* ((agent (plist-get selection :agent))
         (workspace (aipanel-atelier-workspace owner))
         (source (atelier-entry-by-id workspace (plist-get owner :entry-id)))
         (buffer
          (myconfig-terminal-buffer
           name directory program arguments workspace nil
           (list :id (plist-get agent :id)
                 :location (plist-get selection :location)
                 :destination (plist-get selection :destination)
                 :distribution (plist-get selection :distribution)
                 :attachment (aipanel-atelier-persistent-attachment owner))
           'aipanel t)))
    (when (bufferp buffer)
      (when-let* ((job-owner (atelier-find-job-for-buffer (buffer-name buffer)))
                  (panel-entry (nth 2 job-owner)))
        (setf (plist-get panel-entry :persistent)
              (and source (plist-get source :persistent)))))
    buffer))

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

(defun aipanel-atelier-panel-for-entry (entry)
  "Return the live panel attached to ENTRY, if any."
  (when-let* ((name (gethash (plist-get entry :id) aipanel-sessions)))
    (get-buffer name)))

(defun aipanel-atelier-entry-removed (_workspace removed)
  "Stop panels whose source is contained in REMOVED."
  (dolist (leaf (atelier-entry-leaves removed))
    (when-let* ((panel (aipanel-atelier-panel-for-entry leaf)))
      (aipanel-stop-buffer panel))))

(defun aipanel-atelier-entry-moved (entry _old-workspace new-workspace)
  "Move ENTRY's panel record to NEW-WORKSPACE while retaining its process."
  (when-let* ((panel (aipanel-atelier-panel-for-entry entry))
              (owner (buffer-local-value 'aipanel-owner panel)))
    (setf (plist-get owner :workspace-id) (atelier-workspace-id new-workspace))
    (when-let* ((job-owner (atelier-find-job-for-buffer (buffer-name panel)))
                (panel-workspace (car job-owner))
                (panel-entry (nth 2 job-owner)))
      (unless (eq panel-workspace new-workspace)
        (atelier-entry-move panel-entry panel-workspace new-workspace)))))

(defun aipanel-atelier-owner-from-attachment (attachment _workspace)
  "Rebuild a live owner from saved ATTACHMENT in WORKSPACE."
  (when-let* ((entry-id (plist-get attachment :entry-id))
              (source-workspace (atelier-entry-workspace entry-id))
              (entry (atelier-entry-by-id source-workspace entry-id)))
    (list :id entry-id
          :name (or (plist-get entry :name) entry-id)
          :source-buffer (atelier-entry-live-buffer entry)
          :entry-id entry-id
          :workspace-id (atelier-workspace-id source-workspace)
          :directory (plist-get attachment :directory)
          :emacs-directory (plist-get attachment :emacs-directory)
          :destination (plist-get attachment :destination)
          :platform (plist-get attachment :platform)
          :location (plist-get attachment :location))))

(defun aipanel-atelier-restore-buffer (buffer agent workspace)
  (when-let* ((configuration
               (cl-find (plist-get agent :id) aipanel-agents
                        :key (lambda (item) (plist-get item :id))))
              (attachment (plist-get agent :attachment))
              (owner (aipanel-atelier-owner-from-attachment attachment workspace)))
    (let ((selection (list :agent configuration
                           :location (plist-get agent :location)
                           :destination (plist-get agent :destination)
                           :distribution (plist-get agent :distribution))))
      (aipanel-adopt-buffer buffer owner selection
                            (buffer-local-value 'myconfig-terminal-command buffer)))))

(defun aipanel-atelier-entry-restored (_workspace entry buffer)
  "Reconnect BUFFER as the source of ENTRY's restored panel."
  (when-let* ((panel (aipanel-atelier-panel-for-entry entry)))
    (with-current-buffer panel
      (setf (plist-get aipanel-owner :source-buffer) buffer))
    (with-current-buffer buffer
      (cl-pushnew (plist-get entry :id) aipanel-attached-panel-ids :test #'equal)
      (add-hook 'kill-buffer-hook #'aipanel-source-buffer-killed nil t))))

(defun aipanel-atelier-setup ()
  (setq aipanel-owner-function #'aipanel-atelier-owner
        aipanel-candidates-function #'aipanel-atelier-candidates
        aipanel-command-function #'aipanel-atelier-command
        aipanel-context-function #'aipanel-atelier-context
        aipanel-terminal-function #'aipanel-atelier-terminal)
  (add-hook 'aipanel-buffer-created-hook #'aipanel-atelier-buffer-created)
  (add-hook 'aipanel-buffer-exited-hook #'aipanel-atelier-buffer-exited)
  (add-hook 'aipanel-window-change-hook #'aipanel-atelier-window-changed)
  (add-hook 'atelier-entry-removed-hook #'aipanel-atelier-entry-removed)
  (add-hook 'atelier-entry-moved-hook #'aipanel-atelier-entry-moved)
  (add-hook 'atelier-entry-restored-hook #'aipanel-atelier-entry-restored)
  (add-hook 'atelier-agent-restored-functions #'aipanel-atelier-restore-buffer))

(provide 'aipanel-atelier)
;;; aipanel-atelier.el ends here

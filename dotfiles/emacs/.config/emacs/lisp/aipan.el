;;; aipan.el --- Coding-agent side panel -*- lexical-binding: t; -*-

;; Package-Requires: ((emacs "29.1"))
;; Keywords: tools, convenience

;;; Commentary:

;; AIPanel attaches one coding-agent CLI to one source buffer, starts it in a
;; Ghostel side window, and supplies source context after the terminal becomes
;; ready.  Integrations can replace its attachment, discovery, command,
;; context, and terminal functions without making AIPanel depend on a
;; workspace manager.

;;; Code:

(require 'cl-lib)
(require 'subr-x)
(require 'tramp)
(require 'ghostel)
(require 'myconfig-platform)

(defgroup aipanel nil
  "Coding-agent side panel."
  :group 'tools)

(defcustom aipanel-agents
  '((:id opencode :name "OpenCode" :program "opencode"
     :arguments ("--auto") :mini-arguments ("--mini")
     :project-argument t :ready-delay 1.5)
    (:id claude :name "Claude Code" :program "claude"
     :arguments nil :ready-delay 1.5)
    (:id codex :name "Codex" :program "codex"
     :arguments nil :ready-delay 1.5)
    (:id fx :name "fx" :program "fx"
     :arguments nil :ready-delay 1.5))
  "Coding agents offered when their executable is installed.

Each entry is a plist.  `:id', `:name', and `:program' are required.
`:arguments' and `:mini-arguments' are specific to that agent.
`:project-argument' adds the working directory as the first argument, and
`:ready-delay' controls how long its terminal must be stable before context
is pasted."
  :type '(repeat sexp)
  :group 'aipanel)

(defcustom aipanel-ready-timeout 15
  "Seconds to wait for a newly started agent to become ready."
  :type 'number
  :group 'aipanel)

(defcustom aipanel-wsl-probe-timeout 15
  "Seconds to wait while checking the default WSL distribution."
  :type 'number
  :group 'aipanel)

(defcustom aipanel-side 'left
  "Frame side where AIPanel is displayed."
  :type '(choice (const left) (const right))
  :group 'aipanel)

(defvar aipanel-owner-function #'aipanel-default-owner)
(defvar aipanel-candidates-function #'aipanel-default-candidates)
(defvar aipanel-command-function #'aipanel-default-command)
(defvar aipanel-context-function #'aipanel-default-context)
(defvar aipanel-terminal-function #'aipanel-default-terminal)
(defvar aipanel-buffer-created-hook nil)
(defvar aipanel-buffer-exited-hook nil)
(defvar aipanel-window-change-hook nil)
(defvar aipanel-sessions (make-hash-table :test #'equal))

(defvar-local aipanel-agent-id nil)
(defvar-local aipanel-command nil)
(defvar-local aipanel-context-generation 0)
(defvar-local aipanel-owner nil)
(defvar-local aipanel-selection nil)
(defvar-local aipanel-cleaned-up nil)
(defvar-local aipanel-attached-panel-ids nil)

(defun aipanel-default-owner ()
  "Return an attachment describing the current source buffer."
  (let* ((buffer (current-buffer))
         (emacs-directory (file-name-as-directory (expand-file-name default-directory)))
         (method (file-remote-p emacs-directory 'method))
         (remote (and method (tramp-dissect-file-name emacs-directory)))
         (user (and remote (tramp-file-name-user remote)))
         (host (and remote (tramp-file-name-host remote)))
         (port (and remote (tramp-file-name-port remote)))
         (destination (and host (if user (format "%s@%s" user host) host)))
         (directory (if method (file-remote-p emacs-directory 'localname)
                      emacs-directory))
         (location (cond ((equal method "wsl") 'wsl)
                         ((member method '("ssh" "sshx" "scp" "scpx")) 'ssh)
                         (method (user-error "AIPanel does not support %s remote buffers"
                                             method))
                         (t 'host))))
    (list :id buffer :name (buffer-name buffer) :source-buffer buffer
          :directory (file-name-as-directory directory)
          :emacs-directory emacs-directory :location location
          :destination (or destination "local") :port port)))

(defun aipanel-run-wsl-probe (&optional distribution)
  (when-let* ((wsl (executable-find "wsl.exe"))
              (programs (delete-dups
                         (delq nil (mapcar (lambda (agent) (plist-get agent :program))
                                           aipanel-agents)))))
    (let* ((script (concat "printf \"__distribution__%s\\n\" \"$WSL_DISTRO_NAME\"; "
                           "for command do command -v -- \"$command\" >/dev/null 2>&1 "
                           "&& printf \"%s\\n\" \"$command\"; done"))
           (command (append (list wsl)
                            (when distribution (list "-d" distribution))
                            (list "-e" "sh" "-lc" script "aipanel") programs))
           (lines (myconfig-platform-run-command-lines
                   command aipanel-wsl-probe-timeout))
           (header (car lines)))
      (when (and header (string-prefix-p "__distribution__" header))
        (list :distribution (string-remove-prefix "__distribution__" header)
               :programs (cdr lines))))))

(defun aipanel-programs ()
  (delete-dups (delq nil (mapcar (lambda (agent) (plist-get agent :program))
                                  aipanel-agents))))

(defun aipanel-run-ssh-probe (destination &optional port)
  "Return installed agent programs at POSIX SSH DESTINATION."
  (when-let* ((ssh (executable-find "ssh"))
              (destination (and (stringp destination) destination)))
    (let* ((script "for command do command -v -- \"$command\" >/dev/null 2>&1 && printf '%s\\n' \"$command\"; done")
           (remote-command
            (mapconcat #'shell-quote-argument
                       (append (list "sh" "-lc" script "aipanel") (aipanel-programs))
                       " ")))
      (myconfig-platform-run-command-lines
       (append (list ssh) (when port (list "-p" (format "%s" port)))
               (list destination remote-command))
       aipanel-wsl-probe-timeout))))

(defun aipanel-candidates-for-programs
    (programs location &optional destination label port)
  "Return configured candidates found in PROGRAMS at LOCATION."
  (let (candidates)
    (dolist (agent aipanel-agents)
      (let ((program (plist-get agent :program))
            (name (plist-get agent :name)))
        (when (member program programs)
          (push (cons (format "%s (%s)" name (or label (symbol-name location)))
                      (list :agent agent :location location
                            :destination destination
                            :port port
                            :distribution (and (eq location 'wsl) destination)))
                candidates))))
    (nreverse candidates)))

(defun aipanel-default-candidates (owner)
  "Return installed agents in OWNER's execution environment."
  (pcase (plist-get owner :location)
    ('host
     (aipanel-candidates-for-programs
      (cl-remove-if-not #'executable-find (aipanel-programs)) 'host nil "host"))
    ('wsl
     (let* ((destination (plist-get owner :destination))
            (probe (aipanel-run-wsl-probe destination)))
       (aipanel-candidates-for-programs
        (plist-get probe :programs) 'wsl destination
        (format "WSL: %s" destination))))
    ('ssh
     (let ((destination (plist-get owner :destination)))
       (aipanel-candidates-for-programs
        (aipanel-run-ssh-probe destination (plist-get owner :port))
        'ssh destination (format "SSH: %s" destination)
        (plist-get owner :port))))
    (_ nil)))

(defun aipanel-candidates (&optional owner)
  "Return installed agents matching OWNER's execution environment."
  (funcall aipanel-candidates-function (or owner (aipanel-default-owner))))

(defun aipanel-read-agent (owner)
  (let ((candidates (aipanel-candidates owner)))
    (pcase candidates
      ('nil (user-error "No configured coding agent is installed"))
      (`((,_ . ,selection)) selection)
      (_ (cdr (assoc-string (completing-read "Coding agent: " candidates nil t)
                            candidates))))))

(defun aipanel-agent-arguments (agent directory mini &optional relative-project)
  (append (when (plist-get agent :project-argument)
            (list (if relative-project "." directory)))
          (copy-sequence (plist-get agent :arguments))
          (when mini (copy-sequence (plist-get agent :mini-arguments)))))

(defun aipanel-default-command (owner selection mini)
  (let* ((agent (plist-get selection :agent))
         (location (plist-get selection :location))
         (directory (plist-get owner :directory))
         (remote (memq location '(wsl ssh)))
         (arguments (aipanel-agent-arguments agent directory mini remote)))
    (pcase location
      ('wsl
       (list :program "wsl.exe" :directory (expand-file-name "~/")
             :arguments
             (append (when-let* ((distribution (plist-get selection :distribution)))
                       (list "-d" distribution))
                     (list "--cd" directory "--" (plist-get agent :program))
                     arguments)))
      ('ssh
       (let ((remote-command
              (format "cd -- %s && exec %s%s"
                      (shell-quote-argument directory)
                      (shell-quote-argument (plist-get agent :program))
                      (if arguments
                          (concat " " (mapconcat #'shell-quote-argument arguments " "))
                        ""))))
         (list :program "ssh" :directory (expand-file-name "~/")
               :arguments
               (append (list "-t")
                       (when-let* ((port (plist-get selection :port)))
                         (list "-p" (format "%s" port)))
                       (list (plist-get selection :destination) remote-command)))))
      (_
       (list :program (plist-get agent :program) :directory directory
             :arguments arguments)))))

(defun aipanel-default-context (owner _buffer)
  (when-let* ((source (plist-get owner :source-buffer))
              ((buffer-live-p source)))
    (with-current-buffer source
      (when-let* ((file buffer-file-name))
        (let ((file (if (file-remote-p file) (file-remote-p file 'localname) file)))
          (format "%s:L%d:C%d: "
                  (file-relative-name file (plist-get owner :directory))
                  (line-number-at-pos) (1+ (current-column))))))))

(defun aipanel-default-terminal (name directory program arguments _owner _selection)
  (let* ((default-directory directory)
         (buffer (generate-new-buffer (generate-new-buffer-name (format "*%s*" name)))))
    (condition-case error
        (progn
          (with-current-buffer buffer (setq-local default-directory directory))
          (ghostel-exec buffer program arguments)
          (when-let* ((process (get-buffer-process buffer)))
            (set-process-query-on-exit-flag process nil))
          buffer)
      (error
       (when (buffer-live-p buffer) (kill-buffer buffer))
       (signal (car error) (cdr error))))))

(defun aipanel-adopt-buffer (buffer owner selection &optional command)
  (let ((agent (plist-get selection :agent)))
    (with-current-buffer buffer
      (setq-local aipanel-agent-id (plist-get agent :id)
                  aipanel-command command
                  aipanel-owner (copy-tree owner)
                  aipanel-selection (copy-tree selection)
                  aipanel-cleaned-up nil)
      (add-hook 'ghostel-exit-functions #'aipanel-process-exited nil t)
      (add-hook 'kill-buffer-hook #'aipanel-current-buffer-exited nil t))
    (puthash (plist-get owner :id) (buffer-name buffer) aipanel-sessions)
    (when-let* ((source (plist-get owner :source-buffer))
                ((buffer-live-p source))
                ((not (eq source buffer))))
      (with-current-buffer source
        (cl-pushnew (plist-get owner :id) aipanel-attached-panel-ids :test #'equal)
        (add-hook 'kill-buffer-hook #'aipanel-source-buffer-killed nil t)))
    buffer))

(defun aipanel-live-buffer (owner)
  (when-let* ((name (gethash (plist-get owner :id) aipanel-sessions))
              (buffer (get-buffer name))
              (process (get-buffer-process buffer))
              ((process-live-p process)))
    buffer))

(defun aipanel-start (owner mini &optional selection)
  (let* ((selection (or selection (aipanel-read-agent owner)))
         (agent (plist-get selection :agent))
         (command (funcall aipanel-command-function owner selection mini))
         (program (plist-get command :program))
         (arguments (plist-get command :arguments))
         (directory (plist-get command :directory))
         (buffer (funcall aipanel-terminal-function
                          (format "aipanel:%s:%s" (plist-get agent :id)
                                  (plist-get owner :name))
                          directory program arguments owner selection)))
    (aipanel-adopt-buffer buffer owner selection (cons program arguments))
    (with-current-buffer buffer (run-hooks 'aipanel-buffer-created-hook))
    buffer))

(defun aipanel-send-context (buffer context)
  (when (and context (buffer-live-p buffer))
    (with-current-buffer buffer
      (when-let* ((process (get-buffer-process buffer))
                  ((process-live-p process)))
        (ghostel-paste-string context)))))

(defun aipanel-agent-for-buffer (buffer)
  (with-current-buffer buffer
    (cl-find aipanel-agent-id aipanel-agents
             :key (lambda (agent) (plist-get agent :id)))))

(defun aipanel-ready-p (buffer)
  (and (buffer-live-p buffer)
       (with-current-buffer buffer
         (and (process-live-p (get-buffer-process buffer))
              (or (> (buffer-size) 0)
                  (and (stringp ghostel-title) (not (string-empty-p ghostel-title)))
                  (ghostel-alt-screen-p))))))

(defun aipanel-send-context-when-ready
    (buffer context deadline generation &optional stable-tick stable-since)
  (when (and (buffer-live-p buffer)
             (= generation (buffer-local-value 'aipanel-context-generation buffer)))
    (let* ((tick (buffer-chars-modified-tick buffer))
           (stable (and stable-tick (= tick stable-tick)))
           (since (and stable (or stable-since (float-time))))
           (delay (or (plist-get (aipanel-agent-for-buffer buffer) :ready-delay) 1.5)))
      (cond
       ((and since (aipanel-ready-p buffer) (>= (- (float-time) since) delay))
        (aipanel-send-context buffer context))
       ((> (float-time) deadline)
        (message "AIPanel agent did not become ready; context was not sent"))
       (t
        (run-at-time 0.1 nil #'aipanel-send-context-when-ready
                     buffer context deadline generation tick
                     (and (aipanel-ready-p buffer) since)))))))

(defun aipanel-queue-context (buffer context)
  (with-current-buffer buffer
    (cl-incf aipanel-context-generation)
    (aipanel-send-context-when-ready
     buffer context (+ (float-time) aipanel-ready-timeout)
     aipanel-context-generation)))

(defun aipanel-visible-window (buffer)
  (get-buffer-window buffer (selected-frame)))

(defun aipanel-close-windows (buffer)
  (dolist (window (get-buffer-window-list buffer nil t))
    (set-window-dedicated-p window nil)
    (if (> (length (window-list (window-frame window) 'no-minibuffer)) 1)
        (delete-window window)
      (quit-window nil window))))

(defun aipanel-cleanup-buffer (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (unless aipanel-cleaned-up
        (setq aipanel-cleaned-up t)
        (when (equal (gethash (plist-get aipanel-owner :id) aipanel-sessions)
                      (buffer-name buffer))
          (remhash (plist-get aipanel-owner :id) aipanel-sessions))
        (when-let* ((source (plist-get aipanel-owner :source-buffer))
                    ((buffer-live-p source)))
          (with-current-buffer source
            (setq aipanel-attached-panel-ids
                  (delete (plist-get aipanel-owner :id) aipanel-attached-panel-ids))))
        (aipanel-close-windows buffer)
        (run-hooks 'aipanel-buffer-exited-hook)))))

(defun aipanel-stop-buffer (buffer)
  "Stop and remove a live AIPanel BUFFER."
  (when (buffer-live-p buffer)
    (when-let* ((process (get-buffer-process buffer))
                ((process-live-p process)))
      (set-process-query-on-exit-flag process nil)
      (delete-process process))
    (aipanel-cleanup-buffer buffer)
    (aipanel-kill-buffer buffer)))

(defun aipanel-source-buffer-killed ()
  "Stop every panel attached to the current source buffer."
  (dolist (id (copy-sequence aipanel-attached-panel-ids))
    (when-let* ((name (gethash id aipanel-sessions))
                (panel (get-buffer name)))
      (aipanel-stop-buffer panel)))
  (setq aipanel-attached-panel-ids nil))

(defun aipanel-process-exited (buffer _event)
  (aipanel-cleanup-buffer buffer)
  (run-at-time 0 nil #'aipanel-kill-buffer buffer))

(defun aipanel-kill-buffer (buffer)
  (when (buffer-live-p buffer)
    (let ((kill-buffer-query-functions nil))
      (kill-buffer buffer))))

(defun aipanel-current-buffer-exited ()
  (aipanel-cleanup-buffer (current-buffer)))

(defun aipanel-display-buffer (buffer width)
  (let ((window (display-buffer-in-side-window
                 buffer `((side . ,aipanel-side) (slot . 0) (window-width . ,width)))))
    (select-window window)
    (goto-char (point-max))
    window))

(defun aipanel-toggle ()
  (interactive)
  (let* ((owner (or (and (bound-and-true-p aipanel-owner) aipanel-owner)
                    (funcall aipanel-owner-function)))
         (source-buffer (current-buffer))
         (width (max window-min-width (floor (* (frame-width) 0.30))))
         (buffer (or (aipanel-live-buffer owner) (aipanel-start owner (< width 45))))
         (visible (aipanel-visible-window buffer)))
    (if visible
        (delete-window visible)
      (with-current-buffer source-buffer
        (when-let* ((context (funcall aipanel-context-function owner buffer)))
          (aipanel-queue-context buffer context)))
      (aipanel-display-buffer buffer width))
    (run-hooks 'aipanel-window-change-hook)))

(provide 'aipan)
;;; aipan.el ends here

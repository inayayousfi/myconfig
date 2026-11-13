;;; universel-atelier.el --- Optional Universel connection for Atelier -*- lexical-binding: t; -*-

(require 'univers)
(require 'atelier)
(require 'atelier-persist)

(defvar universel-atelier-state-directory nil
  "Directory for Atelier's remote mounts; configured by the application.")

(defvar universel-atelier-terminal-variable "UNIVERSEL_TERMINAL"
  "Variable marking a WSL terminal's shell so it can be found inside WSL.")

(defun universel-atelier-environment (workspace)
  "Translate WORKSPACE's existing record into a Universel environment."
  (let* ((platform (atelier-workspace-field workspace :platform))
         (destination (atelier-workspace-field workspace :destination))
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
          :directory (atelier-workspace-field workspace :path)
          :mount-root (atelier-workspace-field workspace :mount-root))))

(defun universel-atelier-directory (workspace)
  (let* ((environment (universel-atelier-environment workspace))
         (state-directory universel-atelier-state-directory)
         (connected (universel-files-connected-p environment state-directory)))
    (prog1 (universel-file-directory (atelier-workspace-field workspace :path) environment state-directory)
      (when (and atelier-operation-current (not connected)
                 (universel-files-connected-p environment state-directory))
        (let ((owner (or (atelier-operation-live-event
                          (lambda () (atelier-workspace-by-id (atelier-workspace-field workspace :id))))
                         (copy-tree workspace))))
          (atelier-operation-cleanup
           (lambda () (universel-atelier-release owner))))))))

(defun universel-atelier-execution-directory (workspace directory)
  (if (equal (atelier-workspace-field workspace :destination) "local") directory
    (universel-execution-path directory (universel-atelier-environment workspace)
                              universel-atelier-state-directory)))

(defun universel-atelier-target-directory (workspace directory)
  (universel-file-path directory (universel-atelier-environment workspace)
                       universel-atelier-state-directory))

(defun universel-atelier--terminal-marker ()
  "Return a new variable that lets a WSL terminal's shell be found inside WSL."
  (format "%s=%s-%06x" universel-atelier-terminal-variable (float-time) (random #xffffff)))

(defun universel-atelier-terminal-command (workspace)
  (let ((environment (universel-atelier-environment workspace)))
    (universel-shell-command (atelier-workspace-field workspace :path) environment
                             (when (eq (plist-get environment :transport) 'wsl)
                               (list (universel-atelier--terminal-marker))))))

(defun universel-atelier-local-path-p (workspace path)
  "Identify local PATH without connecting to WORKSPACE's mounted files."
  (and (atelier-default-local-path-p workspace path)
       (not (and universel-atelier-state-directory
                 (universel-mounted-environment
                  (file-name-as-directory path)
                  (universel-atelier-environment workspace)
                  universel-atelier-state-directory)))))

(defun universel-atelier-release (workspace &optional force)
  "Release WORKSPACE's files, preserving sharing unless FORCE is non-nil.
A WSL distribution's helper stops with its last running workspace."
  (let ((environment (universel-atelier-environment workspace)))
    (when (and (eq (plist-get environment :transport) 'wsl)
               (not (cl-some (lambda (other)
                               (and (not (eq other workspace))
                                    (eq (atelier-workspace-status other) 'running)
                                    (eq (atelier-workspace-field other :platform) 'wsl)
                                    (equal (atelier-workspace-field other :destination)
                                           (atelier-workspace-field workspace :destination))))
                             (atelier-workspace-list))))
      (universel-wsl-stop-helper (atelier-workspace-field workspace :destination)))
    (when (and (eq (plist-get environment :transport) 'ssh)
               (eq (plist-get environment :platform) 'windows)
               (or force
                   (not (cl-some
                         (lambda (other)
                           (and (not (eq other workspace))
                                (eq (atelier-workspace-status other) 'running)
                                (eq (atelier-workspace-field other :platform) 'windows)
                                (equal (universel-mount-key environment)
                                       (universel-mount-key (universel-atelier-environment other)))))
                         (atelier-workspace-list)))))
      (universel-release-files environment universel-atelier-state-directory))))

(defun universel-atelier-detect-directory (directory)
  "Identify a Windows mount without opening any connection."
  (when universel-atelier-state-directory
    (cl-loop for workspace in (atelier-workspace-list)
             for environment = (universel-atelier-environment workspace)
             thereis (universel-mounted-environment
                       directory environment universel-atelier-state-directory))))

(defun universel-atelier-directory-target (directory)
  "Convert DIRECTORY to an Atelier target, retaining mounted Windows ownership."
  (let ((wsl (universel-environment nil directory)))
    (cond
     ((universel-atelier-detect-directory directory)
      (let ((environment (universel-atelier-detect-directory directory)))
        (list (concat (plist-get environment :destination)
                      (when-let* ((port (plist-get environment :port)))
                        (format "#%s" port)))
              (plist-get environment :directory) 'windows
              (plist-get environment :mount-root))))
     ((and (eq (plist-get wsl :transport) 'wsl) (not (file-remote-p directory)))
      (list (plist-get wsl :destination) (plist-get wsl :directory) 'wsl nil))
     (t (atelier-default-directory-target directory)))))

(defun universel-atelier-wsl-destinations ()
  "Offer each installed WSL distribution as a workspace machine."
  (mapcar (lambda (distribution)
            (list (format "WSL: %s" distribution) :destination distribution :platform 'wsl))
          (universel-wsl-distributions)))

(defun universel-atelier-file-path (path)
  "Move a saved WSL path from the old remote-file form to the Windows share."
  (if (and (eq (universel-host-platform) 'windows)
           (string-match "\\`/wsl:\\([^:/]+\\):\\(.*\\)\\'" path))
      (format "//wsl.localhost/%s%s" (match-string 1 path) (match-string 2 path))
    path))

(defun universel-atelier-resolve-wsl-path (path)
  "Return PATH on a WSL share with its symbolic links resolved inside WSL.
Windows cannot follow Linux links through the share, so linked files open
through their real path.  Other paths are returned unchanged."
  (let ((environment (and (stringp path) (universel-environment nil path))))
    (if-let* (((eq (plist-get environment :transport) 'wsl))
              ((not (file-remote-p path)))
              (real (universel-real-path (plist-get environment :directory) environment)))
        (concat (format "//wsl.localhost/%s" (plist-get environment :destination))
                real
                (if (and (directory-name-p path) (not (string-suffix-p "/" real))) "/" ""))
      path)))

(defun universel-atelier--resolve-first-argument (arguments)
  (if (stringp (car arguments))
      (cons (universel-atelier-resolve-wsl-path (car arguments)) (cdr arguments))
    arguments))

(defun universel-atelier--group-foreground (owner table)
  "Return the root of OWNER's terminal foreground process group in TABLE."
  (let ((tpgid (plist-get owner :tpgid))
        (pgrp (plist-get owner :pgrp)))
    (unless (or (<= tpgid 0) (= tpgid pgrp))
      (let* ((group (cl-remove-if-not
                     (lambda (entry)
                       (and (= (plist-get entry :tty) (plist-get owner :tty))
                            (= (plist-get entry :pgrp) tpgid)))
                     table))
             (pids (mapcar (lambda (entry) (plist-get entry :pid)) group))
             (roots (cl-remove-if (lambda (entry) (member (plist-get entry :ppid) pids)) group)))
        (when (= (length roots) 1) (car roots))))))

(defun universel-atelier--wsl-terminal (owner)
  "Return (DISTRIBUTION . MARKER) when Windows record OWNER is a WSL terminal."
  (let ((argv (plist-get owner :argv)))
    (when-let* (((let ((case-fold-search t))
                   (string-match-p "\\(?:\\`\\|[/\\\\]\\)wsl\\(?:\\.exe\\)?\\'" (or (car argv) ""))))
                (distribution (cadr (member "-d" argv)))
                (marker (cl-find-if (lambda (argument)
                                      (string-prefix-p (concat universel-atelier-terminal-variable "=")
                                                       argument))
                                    argv)))
      (cons distribution marker))))

(defun universel-atelier-foreground-process (pid table &optional direct)
  "Find PID's foreground process in TABLE, or PID itself for DIRECT launches.
Linux records follow the terminal's foreground process group.  Windows has no
such group, so the only child started after the shell is its foreground.  A
WSL terminal's shell is found inside its distribution by its marker.  The
returned record's :owner is the shell it runs under."
  (when-let* ((owner (cl-find pid table :key (lambda (entry) (plist-get entry :pid)))))
    (let ((foreground
           (cond
            (direct owner)
            ((plist-member owner :tpgid) (universel-atelier--group-foreground owner table))
            ((universel-atelier--wsl-terminal owner)
             (pcase-let* ((`(,distribution . ,marker) (universel-atelier--wsl-terminal owner))
                          (environment (list :platform 'posix :transport 'wsl
                                             :destination distribution))
                          (shells (universel-processes-with-variable marker environment)))
               ;; The shell's children inherit its marker; the shell is the only
               ;; marked process whose parent is not marked.
               (let* ((wsl-table (universel-process-table environment))
                      (roots (cl-remove-if-not
                              (lambda (entry)
                                (and (memq (plist-get entry :pid) shells)
                                     (not (memq (plist-get entry :ppid) shells))))
                              wsl-table)))
                 (when (= (length roots) 1)
                   (setq owner (car roots))
                   (universel-atelier--group-foreground owner wsl-table)))))
            (t
             (let ((children (cl-remove-if-not
                              (lambda (entry)
                                (and (eql (plist-get entry :ppid) pid)
                                     (>= (plist-get entry :start-time) (plist-get owner :start-time))))
                              table)))
               (when (= (length children) 1) (car children)))))))
      (when foreground
        (append foreground (list :owner owner))))))

;;; SSH destinations used before, suggested when choosing a machine

(defcustom universel-atelier-shell-history-files
  (delete-dups
   (delq nil
         (list (getenv "HISTFILE")
               "~/.zsh_history"
               "~/.bash_history"
               "~/.history"
               "~/.local/share/zsh/history"
               "~/.local/share/fish/fish_history"
               "~/.config/fish/fish_history")))
  "Shell history files inspected for previously used SSH destinations."
  :type '(repeat file)
  :group 'universel)

(defcustom universel-atelier-shell-history-read-limit (* 4 1024 1024)
  "Maximum number of bytes read from the end of each shell history file."
  :type 'integer
  :group 'universel)

(defconst universel-atelier-ssh-options-with-arguments
  '("-B" "-b" "-c" "-D" "-E" "-e" "-F" "-I" "-i" "-J" "-L"
    "-l" "-m" "-O" "-o" "-P" "-p" "-Q" "-R" "-S" "-W" "-w"))

(defun universel-atelier-ssh-destination-valid-p (destination)
  (and (stringp destination)
       (string-match-p
        (rx string-start
            (optional (+ (any alnum "_.+-")) "@")
            (or (+ (any alnum "_.-"))
                (seq "[" (+ (any xdigit ":.")) "]"))
            string-end)
        destination)
       (not (member destination '("ssh" "localhost")))))

(defun universel-atelier-ssh-destination-from-command (command)
  "Return the OpenSSH destination used by shell COMMAND, if recognizable."
  (condition-case nil
      (let* ((tokens (split-string-shell-command command))
             (ssh-position (cl-position "ssh" tokens :test #'equal))
             (prefix (and ssh-position (cl-subseq tokens 0 ssh-position)))
             (invocation-p
              (and ssh-position
                   (cl-every
                    (lambda (token)
                      (or (member token '("command" "sudo" "env" "exec" "nohup"
                                          "time" "tailscale"))
                          (string-prefix-p "-" token)
                          (string-match-p "=" token)))
                    prefix)))
             (arguments (and invocation-p (nthcdr (1+ ssh-position) tokens)))
             destination)
        (while (and arguments (not destination))
          (let ((argument (pop arguments)))
            (cond
             ((equal argument "--")
              (setq destination (pop arguments)))
             ((member argument universel-atelier-ssh-options-with-arguments)
              (pop arguments))
             ((string-prefix-p "-" argument))
             ((universel-atelier-ssh-destination-valid-p argument)
              (setq destination argument)))))
        (and (universel-atelier-ssh-destination-valid-p destination) destination))
    (error nil)))

(defun universel-atelier-shell-history-commands (file)
  (when-let* ((expanded (expand-file-name file))
              ((file-readable-p expanded)))
    (with-temp-buffer
      (let* ((size (file-attribute-size (file-attributes expanded)))
             (start (max 0 (- size universel-atelier-shell-history-read-limit))))
        (insert-file-contents expanded nil start size)
        (when (> start 0)
          (goto-char (point-min))
          (delete-region (point-min) (min (point-max) (1+ (line-end-position)))))
        (goto-char (point-min))
        (let (commands)
          (while (not (eobp))
            (let ((line (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position))))
              (cond
               ((string-match (rx string-start ": " (+ digit) ":" (+ digit) ";"
                                  (group (* anychar))) line)
                (push (match-string 1 line) commands))
               ((string-match (rx string-start (* blank) "- cmd:" (* blank)
                                  (group (* anychar))) line)
                (push (replace-regexp-in-string "\\\\n" " " (match-string 1 line) t t)
                      commands))
               ((not (string-match-p (rx string-start "#" (+ digit) string-end) line))
                (push line commands))))
            (forward-line 1))
          (nreverse commands))))))

(defun universel-atelier-shell-history-ssh-destinations ()
  "Return SSH destinations found in configured shell histories, newest first."
  (let ((files
         (sort (cl-remove-if-not #'file-readable-p
                                 (mapcar #'expand-file-name universel-atelier-shell-history-files))
               (lambda (left right)
                 (time-less-p (file-attribute-modification-time (file-attributes right))
                              (file-attribute-modification-time (file-attributes left))))))
        destinations)
    (dolist (file files)
      (let (file-destinations)
        (dolist (command (universel-atelier-shell-history-commands file))
          (when-let* ((destination (universel-atelier-ssh-destination-from-command command)))
            (push destination file-destinations)))
        (setq destinations (append destinations (delete-dups file-destinations)))))
    (delete-dups destinations)))

(defun universel-atelier-ssh-aliases ()
  (let ((files (list (expand-file-name "~/.ssh/config"))) aliases)
    (while files
      (let ((file (pop files)))
        (when (file-readable-p file)
          (with-temp-buffer
            (insert-file-contents file)
            (goto-char (point-min))
            (while (re-search-forward "^[[:space:]]*Host[[:space:]]+\\(.+\\)$" nil t)
              (dolist (host (split-string (match-string 1)))
                (unless (string-match-p "[*?!]" host) (push host aliases))))
            (goto-char (point-min))
            (while (re-search-forward "^[[:space:]]*Include[[:space:]]+\\(.+\\)$" nil t)
              (dolist (pattern (split-string (match-string 1)))
                (setq files (append (file-expand-wildcards
                                     (expand-file-name pattern
                                                        (expand-file-name "~/.ssh/")))
                                    files))))))))
    (delete-dups (nreverse aliases))))

(defun universel-atelier-ssh-suggestions ()
  "Suggest SSH aliases, then destinations found in shell histories."
  (append (universel-atelier-ssh-aliases) (universel-atelier-shell-history-ssh-destinations)))

(defun universel-atelier-setup (state-directory)
  "Connect Atelier to Universel, storing mounts below STATE-DIRECTORY."
  (setq universel-atelier-state-directory state-directory
        atelier-directory-function #'universel-atelier-directory
        atelier-execution-directory-function #'universel-atelier-execution-directory
        atelier-target-directory-function #'universel-atelier-target-directory
        atelier-directory-target-function #'universel-atelier-directory-target
        atelier-terminal-command-function #'universel-atelier-terminal-command
        atelier-release-function #'universel-atelier-release
        atelier-local-path-p-function #'universel-atelier-local-path-p
        atelier-process-observation-function #'universel-atelier-process-observation-p
        atelier-process-table-function #'universel-atelier-process-table
        atelier-foreground-process-function #'universel-atelier-foreground-process
        atelier-process-environment-function #'universel-atelier-process-environment
        atelier-shell-environment-function #'universel-atelier-shell-environment
        atelier-shell-launch-function #'universel-atelier-shell-launch
        atelier-process-runtime-function #'universel-atelier-process-runtime
        atelier-extra-destinations #'universel-atelier-wsl-destinations
        atelier-file-path-function #'universel-atelier-file-path)
  (when (eq (universel-host-platform) 'windows)
    (advice-add 'find-file-noselect :filter-args #'universel-atelier--resolve-first-argument)
    (advice-add 'dired-noselect :filter-args #'universel-atelier--resolve-first-argument))
  (add-hook 'atelier-destination-suggestion-functions #'universel-atelier-ssh-suggestions)
  (add-hook 'universel-environment-functions #'universel-atelier-detect-directory))

(defun universel-atelier-process-observation-p ()
  (universel-process-observation-p (universel-host-environment)))

(defun universel-atelier-process-table ()
  (universel-process-table (universel-host-environment)))

(defun universel-atelier-process-environment (record)
  (universel-process-environment (plist-get record :pid)
                                 (or (plist-get record :location) (universel-host-environment))))

(defun universel-atelier-shell-environment (shell start)
  (universel-shell-environment shell start
                               (or (plist-get shell :location) (universel-host-environment))))

(defun universel-atelier-shell-launch (shell directory argv variables)
  "Return (PROGRAM . ARGUMENTS) restarting SHELL; a WSL shell gets a new marker."
  (let* ((environment (or (plist-get shell :location) (universel-host-environment)))
         (launch (universel-shell-launch
                  shell directory argv variables
                  (when (eq (plist-get environment :transport) 'wsl)
                    (list (universel-atelier--terminal-marker)))
                  environment)))
    (cons (plist-get launch :program) (plist-get launch :arguments))))

(defun universel-atelier-process-runtime (process)
  (universel-process-runtime process (universel-host-environment)))

(provide 'universel-atelier)
;;; universel-atelier.el ends here

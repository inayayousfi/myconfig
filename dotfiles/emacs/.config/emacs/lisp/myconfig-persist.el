;;; myconfig-persist.el --- Private snapshots and restart recipes -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'myconfig-core)
(require 'atelier)

(defvar myconfig-persist-timer nil)
(defvar myconfig-persist-restoring nil)
(defvar myconfig-snapshot-generation nil)
(defvar myconfig-clock-ticks-per-second 100)
(defvar myconfig-job-observer-timer nil)
(defvar myconfig-defer-job-restart nil)
(defvar myconfig-restart-topology-guard nil)
(defconst myconfig-process-denylist
  '("awk" "bash" "basename" "cat" "chmod" "chown" "cmake" "cp" "curl" "cut"
    "date" "dd" "diff" "dirname" "du" "echo" "env" "false" "fd" "find" "fish"
    "fzf" "git" "go" "grep" "head" "install" "kill" "less" "ln" "ls" "make"
    "man" "mkdir" "mv" "ninja" "node" "npm" "pacman" "pnpm" "printf" "pwd"
    "python" "python3" "readlink" "realpath" "rg" "rm" "rmdir" "rsync" "ruby"
    "scp" "sed" "sh" "sleep" "sort" "ssh" "stat" "tail" "tar" "tee" "test"
    "tmux" "touch" "tr" "true" "uname" "uniq" "wc" "wget" "xargs" "zsh"))

(defun myconfig-new-generation ()
  (format "%s-%08x" (float-time) (random #xffffffff)))

(defun myconfig-read-proc-file (file &optional literally)
  (with-temp-buffer
    (if literally (insert-file-contents-literally file) (insert-file-contents file))
    (buffer-string)))

(defun myconfig-proc-entry (pid)
  (condition-case nil
      (let* ((stat (myconfig-read-proc-file (format "/proc/%d/stat" pid)))
             (close (string-match ") " stat))
             (fields (split-string (substring stat (+ close 2))))
             (raw-argv (myconfig-read-proc-file (format "/proc/%d/cmdline" pid) t))
             (argv (mapcar (lambda (arg) (decode-coding-string arg 'utf-8))
                           (split-string raw-argv "\0" t)))
             (executable (file-truename (format "/proc/%d/exe" pid))))
        (list :pid pid :state (nth 0 fields)
              :ppid (string-to-number (nth 1 fields))
              :pgrp (string-to-number (nth 2 fields))
              :tty (string-to-number (nth 4 fields))
              :tpgid (string-to-number (nth 5 fields))
              :start-ticks (string-to-number (nth 19 fields))
              :executable executable :argv argv))
    (error nil)))

(defun myconfig-proc-table ()
  (let (entries)
    (dolist (path (directory-files "/proc" t "\\`[0-9]+\\'"))
      (when-let* ((entry (myconfig-proc-entry
                          (string-to-number (file-name-nondirectory path)))))
        (push entry entries)))
    entries))

(defun myconfig-process-runtime (entry)
  (let ((uptime (string-to-number (car (split-string (myconfig-read-proc-file "/proc/uptime"))))))
    (max 0 (- uptime (/ (float (plist-get entry :start-ticks))
                        myconfig-clock-ticks-per-second)))))

(defun myconfig-job-foreground (job table)
  (when-let* ((buffer (get-buffer (plist-get job :buffer)))
              (process (get-buffer-process buffer))
              (pid (with-current-buffer buffer
                     (or (and (boundp 'ghostel--pid) ghostel--pid)
                         (process-id process))))
              (owner (cl-find pid table :key (lambda (entry) (plist-get entry :pid)))))
    (if (plist-get job :direct-command)
        owner
      (let* ((tpgid (plist-get owner :tpgid))
             (pgrp (plist-get owner :pgrp)))
        (unless (or (<= tpgid 0) (= tpgid pgrp))
          (let* ((group (cl-remove-if-not
                         (lambda (entry)
                           (and (= (plist-get entry :tty) (plist-get owner :tty))
                                (= (plist-get entry :pgrp) tpgid))) table))
                 (pids (mapcar (lambda (entry) (plist-get entry :pid)) group))
                 (roots (cl-remove-if
                         (lambda (entry) (member (plist-get entry :ppid) pids)) group)))
            (when (= (length roots) 1) (car roots))))))))

(defun myconfig-job-recipe (job foreground directory)
  (let ((policy (plist-get job :policy))
        (program (file-name-nondirectory (plist-get foreground :executable)))
        (fallback (atelier-shell-restart-recipe (plist-get job :shell) directory)))
    (cond
     ((eq policy 'never) nil)
     ((and (eq policy 'auto)
           (< (myconfig-process-runtime foreground) 5)) fallback)
     ((and (eq policy 'auto) (member program myconfig-process-denylist)) fallback)
     (t (list :executable (plist-get foreground :executable)
              :argv (copy-sequence (plist-get foreground :argv))
              :directory directory
              :shell (copy-tree (plist-get job :shell)))))))

(defun myconfig-observe-jobs ()
  (when (eq system-type 'gnu/linux)
    (condition-case error
        (let ((table (myconfig-proc-table)) changed)
          (dolist (workspace atelier-workspaces)
            (when (equal (plist-get workspace :destination) "local")
              (dolist (job (plist-get workspace :jobs))
                (let* ((buffer (get-buffer (plist-get job :buffer)))
                       (directory (and buffer (buffer-local-value 'default-directory buffer)))
                       (foreground (myconfig-job-foreground job table))
                       (recipe (if foreground
                                   (myconfig-job-recipe job foreground directory)
                                 (unless (eq (plist-get job :policy) 'never)
                                   (atelier-shell-restart-recipe
                                    (plist-get job :shell) directory)))))
                  (unless (equal recipe (plist-get job :recipe))
                    (setf (plist-get job :recipe) recipe)
                    (setq changed t))))))
          (when changed (myconfig-persist-now)))
      (error (myconfig-log "Foreground job observation failed: %s" error)))))

(defun myconfig-live-process-state ()
  (let ((table (and (eq system-type 'gnu/linux) (myconfig-proc-table))) state)
    (dolist (workspace atelier-workspaces)
      (dolist (job (plist-get workspace :jobs))
        (let ((foreground (and table (myconfig-job-foreground job table))))
          (push (list :workspace (plist-get workspace :name)
                      :buffer (plist-get job :buffer)
                      :program (and foreground (plist-get foreground :executable)))
                state))))
    (nreverse state)))

(defun myconfig-saved-process-state (data)
  (let (state)
    (dolist (workspace (plist-get data :workspaces))
      (dolist (job (plist-get workspace :jobs))
        (push (list :workspace (plist-get workspace :name)
                    :buffer (plist-get job :buffer)
                    :program (plist-get (plist-get job :recipe) :executable))
              state)))
    (nreverse state)))

(defun myconfig-process-replacements (live saved)
  (cl-loop for wanted in saved
           for current = (cl-find-if
                           (lambda (item)
                             (and (equal (plist-get item :workspace) (plist-get wanted :workspace))
                                  (equal (plist-get item :buffer) (plist-get wanted :buffer)))) live)
           when (and (plist-get wanted :program) (plist-get current :program)
                     (not (equal (plist-get wanted :program) (plist-get current :program))))
           collect (list :workspace (plist-get wanted :workspace)
                         :buffer (plist-get wanted :buffer)
                         :current (plist-get current :program)
                         :wanted (plist-get wanted :program))))

(defun myconfig-stop-all-live-jobs ()
  (dolist (workspace atelier-workspaces)
    (atelier-workspace-stop-jobs workspace)))

(defun myconfig-job-process-exited (buffer)
  (when-let* ((owner (atelier-find-job-for-buffer (buffer-name buffer))))
    (let ((workspace (car owner))
          (job (nth 1 owner)))
      (unless atelier-preserve-job-recipe
        (setf (plist-get workspace :jobs) (delq job (plist-get workspace :jobs)))
        (when (equal (plist-get workspace :agent-buffer) (buffer-name buffer))
          (setf (plist-get workspace :agent-buffer) nil)))
      (myconfig-log "Job in %s exited: %s"
                    (plist-get workspace :name) (buffer-name buffer)))
    (atelier-notify-change)
    (myconfig-persist-schedule)))

(defun myconfig-set-job-policy ()
  (interactive)
  (let* ((owner (or (atelier-find-job-for-buffer (buffer-name))
                    (user-error "This buffer is not a workbench job")))
         (job (nth 1 owner))
         (choice (completing-read "Restart policy: " '("Auto" "Always" "Never") nil t))
         (policy (intern (downcase choice))))
    (setf (plist-get job :policy) policy)
    (when (eq policy 'never) (setf (plist-get job :recipe) nil))
    (myconfig-observe-jobs)
    (message "Restart policy for %s: %s" (buffer-name) choice)))

(defun myconfig-build-restart-plan (workspace)
  (cl-loop for job in (plist-get workspace :jobs)
           for recipe = (plist-get job :recipe)
           when (plist-get recipe :executable)
           collect (list :workspace workspace :job job
                         :recipe (copy-tree recipe)
                         :buffer (plist-get job :buffer))))

(defun myconfig-restart-entry-valid-p (entry)
  (let ((workspace (plist-get entry :workspace))
        (job (plist-get entry :job)))
    (and (memq workspace atelier-workspaces)
         (equal (plist-get workspace :destination) "local")
         (memq job (plist-get workspace :jobs))
         (equal (plist-get job :buffer) (plist-get entry :buffer))
         (equal (plist-get job :recipe) (plist-get entry :recipe))
         (not (when-let* ((buffer (get-buffer (plist-get entry :buffer)))
                          (process (get-buffer-process buffer)))
                (process-live-p process))))))

(defun myconfig-validate-restart-plan (plan)
  (when (and myconfig-restart-topology-guard
             (not (equal myconfig-restart-topology-guard
                         (myconfig-data-topology (myconfig-snapshot-data)))))
    (error "Topology changed before restarting saved jobs"))
  (unless (cl-every #'myconfig-restart-entry-valid-p plan)
    (error "A saved split or job changed before restarting saved jobs")))

(defun myconfig-restart-saved-jobs (&optional workspace)
  (when (fboundp 'myconfig-terminal-buffer)
    (let ((workspace (or workspace (atelier-current-workspace))))
      (when (and workspace (equal (plist-get workspace :destination) "local"))
        (let ((plan (myconfig-build-restart-plan workspace)))
          (atelier-workspace-stop-jobs workspace)
          (myconfig-validate-restart-plan plan)
          (dolist (entry plan)
            (unless (myconfig-restart-entry-valid-p entry)
              (error "Split %s changed before restart" (plist-get entry :buffer)))
            (let* ((job (plist-get entry :job))
                   (recipe (plist-get entry :recipe))
                   (executable (plist-get recipe :executable))
                   (saved-name (plist-get job :buffer)))
              (condition-case error
                   (let* ((arguments (cdr (plist-get recipe :argv)))
                          (shell (plist-get recipe :shell))
                          (shell-restart
                           (and shell
                                (equal executable (plist-get shell :executable))))
                           (name saved-name)
                           (buffer (myconfig-terminal-buffer
                                   name (plist-get recipe :directory) executable arguments workspace
                                   (and shell-restart shell))))
                     (setf (plist-get job :buffer) (buffer-name buffer))
                     (when-let* ((agent (plist-get job :agent)))
                       (setf (plist-get workspace :agent-buffer) (buffer-name buffer))
                       (run-hook-with-args 'atelier-agent-restored-functions
                                           buffer agent workspace))
                    (myconfig-log "Accepted restart of %s in workspace %s"
                                  (file-name-nondirectory executable)
                                  (plist-get workspace :name)))
                (error
                 (myconfig-log "Job restart failed for %s in workspace %s: %s"
                               (file-name-nondirectory executable)
                               (plist-get workspace :name) error))))))))))

(defun myconfig-snapshot-data ()
  (unless (or myconfig-persist-restoring atelier-navigator-window-configurations)
    (atelier-capture-current-workspace))
  (list :version 2
        :generation (or myconfig-snapshot-generation (myconfig-new-generation))
        :current-workspace atelier-current-workspace-name
        :ssh-destinations atelier-remembered-ssh-destinations
        :workspaces atelier-workspaces))

(defun myconfig-workspace-topology (workspace)
  (list :name (plist-get workspace :name)
         :destination (plist-get workspace :destination)
         :path (plist-get workspace :path)
         :platform (plist-get workspace :platform)
         :mount-root (plist-get workspace :mount-root)
          :live (plist-get workspace :live)
          :state (plist-get workspace :state)
          :buffers (plist-get workspace :buffers)
          :owned-buffers (plist-get workspace :owned-buffers)
          :jobs (plist-get workspace :jobs)
         :agent-directory (plist-get workspace :agent-directory)))

(defun myconfig-data-topology (data)
  (list :current-workspace (plist-get data :current-workspace)
        :workspaces (mapcar #'myconfig-workspace-topology (plist-get data :workspaces))))

(defun myconfig-affected-workspaces (left right)
  (let* ((left-workspaces (plist-get left :workspaces))
         (right-workspaces (plist-get right :workspaces))
         (names (delete-dups
                 (append (mapcar (lambda (item) (plist-get item :name)) left-workspaces)
                         (mapcar (lambda (item) (plist-get item :name)) right-workspaces)))))
    (cl-remove-if-not
     (lambda (name)
       (not (equal (cl-find name left-workspaces :key (lambda (item) (plist-get item :name)) :test #'equal)
                   (cl-find name right-workspaces :key (lambda (item) (plist-get item :name)) :test #'equal))))
     names)))

(defun myconfig-migrate-state (data)
  (pcase (plist-get data :version)
    (2 data)
    (1
     (list :version 2
           :generation (plist-get data :generation)
           :current-workspace (plist-get data :current-workspace)
           :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
           :workspaces
           (mapcar
            (lambda (workspace)
              (let* ((tabs (plist-get workspace :tabs))
                     (current-id (plist-get workspace :current-tab))
                     (current (or (cl-find current-id tabs
                                           :key (lambda (tab) (plist-get tab :id))
                                           :test #'equal)
                                  (car tabs)))
                     jobs)
                (dolist (tab tabs)
                  (dolist (job (plist-get tab :jobs))
                    (cl-pushnew job jobs :key (lambda (item) (plist-get item :id))
                                :test #'equal)))
                (list :name (plist-get workspace :name)
                      :destination (plist-get workspace :destination)
                      :path (plist-get workspace :path)
                      :platform (plist-get workspace :platform)
                      :mount-root (plist-get workspace :mount-root)
                      :created (plist-get workspace :created)
                      :live (plist-get workspace :live)
                       :state (copy-tree (plist-get current :state))
                       :buffers (copy-tree (plist-get current :buffers))
                       :owned-buffers nil
                       :jobs (nreverse (copy-tree jobs))
                      :agent-buffer nil
                      :agent-directory (plist-get current :agent-directory))))
            (plist-get data :workspaces))))
    (_ data)))

(defun myconfig-validate-state (data)
  (setq data (myconfig-migrate-state data))
  (unless (and (listp data) (equal (plist-get data :version) 2)
               (stringp (plist-get data :generation))
               (listp (plist-get data :workspaces)))
    (error "Invalid state header"))
  (let (names)
    (dolist (workspace (plist-get data :workspaces))
      (let ((name (plist-get workspace :name))
            (destination (plist-get workspace :destination))
            (path (plist-get workspace :path)))
        (unless (and (stringp name) (not (string-empty-p name))
                      (stringp destination) (not (string-empty-p destination))
                       (stringp path) (not (string-empty-p path))
                       (listp (plist-get workspace :buffers))
                       (listp (plist-get workspace :owned-buffers))
                       (listp (plist-get workspace :jobs)))
          (error "Invalid workspace definition"))
        (when (member name names) (error "Duplicate workspace name: %s" name))
        (push name names)
        (when (and (equal destination "local") (not (file-directory-p path)))
          (error "Workspace root is missing: %s" path))
        (dolist (job (plist-get workspace :jobs))
          (unless (and (listp job) (stringp (plist-get job :id))
                       (stringp (plist-get job :buffer))
                       (memq (plist-get job :policy) '(auto always never)))
            (error "Invalid job record in workspace %s" name)))))
    data))

(defun myconfig-apply-state (data)
  (setq atelier-workspaces (copy-tree (plist-get data :workspaces))
        atelier-current-workspace-name (plist-get data :current-workspace)
        atelier-remembered-ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
        myconfig-snapshot-generation (plist-get data :generation))
  (dolist (workspace atelier-workspaces)
    (setf (plist-get workspace :live) nil)
    (let (kept-jobs dropped-names)
      (dolist (job (plist-get workspace :jobs))
        (unless (or (plist-get job :recipe)
                    (eq (plist-get job :policy) 'never))
          (setf (plist-get job :recipe)
                (atelier-shell-restart-recipe
                 (plist-get job :shell) (plist-get job :directory))))
        (if (plist-get job :recipe)
            (push job kept-jobs)
          (push (plist-get job :buffer) dropped-names)))
      (setf (plist-get workspace :jobs) (nreverse kept-jobs))
      (when (cl-some (lambda (descriptor)
                       (member (plist-get descriptor :name) dropped-names))
                     (plist-get workspace :buffers))
        (setf (plist-get workspace :state) nil
               (plist-get workspace :buffers) nil
               (plist-get workspace :owned-buffers) nil)))
    (setf (plist-get workspace :agent-buffer) nil))
  (myconfig-persist-open-saved-state))

(defun myconfig-persist-now ()
  (interactive)
  (unless myconfig-persist-restoring
    (condition-case error
        (progn
          (setq myconfig-snapshot-generation (myconfig-new-generation))
          (myconfig-write-data-atomically myconfig-state-file (myconfig-snapshot-data)))
      (error (myconfig-log "Snapshot failed: %s" error)))))

(defun myconfig-persist-schedule ()
  (unless myconfig-persist-restoring
    (when myconfig-persist-timer (cancel-timer myconfig-persist-timer))
    (setq myconfig-persist-timer (run-with-idle-timer 0.5 nil #'myconfig-persist-now))))

(defun myconfig-persist-load ()
  (condition-case error
      (when-let* ((data (myconfig-read-data myconfig-state-file)))
        (setq data (myconfig-validate-state data))
        (myconfig-apply-state data)
        t)
    (error
     (myconfig-log "Stored state rejected: %s" error)
     nil)))

(defun myconfig-persist-open-saved-state ()
  (when (and atelier-workspaces atelier-current-workspace-name)
    (let ((workspace (atelier-workspace-get atelier-current-workspace-name)))
      (when workspace
        (setf (plist-get workspace :live) t)
        (unless myconfig-defer-job-restart
          (myconfig-restart-saved-jobs workspace)
          (when (display-graphic-p (selected-frame))
            (atelier-restore-workspace workspace)))))))

(defun myconfig-restore-journal-recover ()
  (when-let* ((journal (myconfig-read-data myconfig-restore-journal-file))
              (original (plist-get journal :original)))
    (setq original (myconfig-validate-state original))
    (myconfig-write-data-atomically myconfig-state-file original)
    (delete-file myconfig-restore-journal-file)
    (myconfig-log "Recovered the state that existed before an interrupted restore")))

(defun myconfig-restore-snapshot ()
  (interactive)
  (let* ((saved (myconfig-validate-state (or (myconfig-read-data myconfig-state-file)
                                              (user-error "No saved workbench state"))))
         (live (myconfig-snapshot-data))
         (expected (myconfig-data-topology live))
         (wanted (myconfig-data-topology saved))
         (affected (myconfig-affected-workspaces expected wanted))
         (expected-processes (myconfig-live-process-state))
         (saved-processes (myconfig-saved-process-state saved))
         (replacements (myconfig-process-replacements expected-processes saved-processes)))
    (when (equal expected wanted) (user-error "Live and saved topology already match"))
    (unless (yes-or-no-p
             (format "Replace live state for workspace%s %s? "
                     (if (= (length affected) 1) "" "s")
                     (string-join affected ", ")))
      (myconfig-persist-now)
      (user-error "Saved the unchanged live state"))
    (when replacements
      (unless
          (yes-or-no-p
           (concat
            "Replace these live jobs? "
            (string-join
             (mapcar (lambda (change)
                       (format "%s in %s/%s with %s"
                               (file-name-nondirectory (plist-get change :current))
                               (plist-get change :workspace) (plist-get change :buffer)
                               (file-name-nondirectory (plist-get change :wanted))))
                     replacements)
             "; ")
            "? "))
        (myconfig-persist-now)
        (user-error "Saved the unchanged live state")))
    (unless (equal expected (myconfig-data-topology (myconfig-snapshot-data)))
      (user-error "Restore cancelled because live topology changed"))
    (unless (equal expected-processes (myconfig-live-process-state))
      (user-error "Restore cancelled because a live job changed"))
    (let ((token (myconfig-new-generation))
          (myconfig-persist-restoring t)
          (myconfig-defer-job-restart t))
      (myconfig-write-data-atomically
       myconfig-restore-journal-file (list :token token :original live :replacement saved))
      (condition-case error
          (progn
            (unless (equal expected (myconfig-data-topology (myconfig-snapshot-data)))
              (error "Live topology changed before replacement"))
            (unless (equal expected-processes (myconfig-live-process-state))
              (error "A live job changed before replacement"))
            (myconfig-stop-all-live-jobs)
            (myconfig-apply-state saved)
            (let ((myconfig-restart-topology-guard wanted))
              (myconfig-restart-saved-jobs))
            (when (display-graphic-p (selected-frame))
              (atelier-restore-workspace (atelier-current-workspace)))
            (let ((journal (myconfig-read-data myconfig-restore-journal-file)))
              (when (equal token (plist-get journal :token))
                (delete-file myconfig-restore-journal-file)))
            (myconfig-persist-now)
            (message "Restored workbench state"))
        (error
          (myconfig-apply-state live)
          (let ((myconfig-restart-topology-guard expected))
            (myconfig-restart-saved-jobs))
          (when (display-graphic-p (selected-frame))
            (atelier-restore-workspace (atelier-current-workspace)))
         (myconfig-log "Restore failed; original live state recovered: %s" error)
         (signal (car error) (cdr error)))))))

(defun myconfig-persist-setup ()
  (let ((myconfig-persist-restoring t))
    (condition-case error
        (myconfig-restore-journal-recover)
      (error (myconfig-log "Restore journal recovery failed: %s" error)))
    (myconfig-persist-load))
  (add-hook 'atelier-change-hook #'myconfig-persist-schedule)
  (add-hook 'kill-emacs-hook #'myconfig-persist-now)
  (setq myconfig-job-observer-timer (run-with-timer 5 5 #'myconfig-observe-jobs)))

(provide 'myconfig-persist)
;;; myconfig-persist.el ends here

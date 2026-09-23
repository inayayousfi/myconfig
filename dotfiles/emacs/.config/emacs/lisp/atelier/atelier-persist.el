;;; atelier-persist.el --- Private snapshots and restart recipes -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'myconfig-core)
(require 'atelier)
(require 'univers)

(defconst atelier-persist-state-file
  (expand-file-name "workbench-state.el" myconfig-state-directory))
(defconst atelier-persist-restore-journal-file
  (expand-file-name "restore-journal.el" myconfig-state-directory))
(defvar atelier-persist-timer nil)
(defvar atelier-persist-restoring nil)
(defvar atelier-snapshot-generation nil)
(defvar atelier-job-observer-timer nil)
(defvar atelier-defer-job-restart nil)
(defvar atelier-restart-topology-guard nil)
(defconst atelier-process-denylist
  '("awk" "bash" "basename" "cat" "chmod" "chown" "cmake" "cp" "curl" "cut"
    "date" "dd" "diff" "dirname" "du" "echo" "env" "false" "fd" "find" "fish"
    "fzf" "git" "go" "grep" "head" "install" "kill" "less" "ln" "ls" "make"
    "man" "mkdir" "mv" "ninja" "node" "npm" "pacman" "pnpm" "printf" "pwd"
    "python" "python3" "readlink" "realpath" "rg" "rm" "rmdir" "rsync" "ruby"
    "scp" "sed" "sh" "sleep" "sort" "ssh" "stat" "tail" "tar" "tee" "test"
    "tmux" "touch" "tr" "true" "uname" "uniq" "wc" "wget" "xargs" "zsh"))

(defun atelier-new-generation ()
  (format "%s-%08x" (float-time) (random #xffffffff)))

(defun atelier-job-foreground (job table)
  (when-let* ((buffer (get-buffer (plist-get job :buffer)))
              (process (get-buffer-process buffer))
               (pid (with-current-buffer buffer
                      (or (and (boundp 'ghostel--pid) ghostel--pid)
                          (process-id process)))))
    (universel-foreground-process pid table (plist-get job :direct-command))))

(defun atelier-job-recipe (job foreground directory)
  (let ((policy (plist-get job :policy))
        (program (file-name-nondirectory (plist-get foreground :executable)))
        (fallback (atelier-shell-restart-recipe (plist-get job :shell) directory)))
    (cond
     ((eq policy 'never) nil)
     ((and (eq policy 'auto)
            (< (universel-process-runtime foreground (universel-host-environment)) 5)) fallback)
     ((and (eq policy 'auto) (member program atelier-process-denylist)) fallback)
     (t (list :executable (plist-get foreground :executable)
              :argv (copy-sequence (plist-get foreground :argv))
              :directory directory
              :shell (copy-tree (plist-get job :shell)))))))

(defun atelier-observe-jobs ()
  (when (universel-process-observation-p (universel-host-environment))
    (condition-case error
        (let ((table (universel-process-table (universel-host-environment))) changed)
          (dolist (workspace atelier-workspaces)
            (dolist (entry (atelier-workspace-job-entries workspace))
              (when-let* ((buffer (atelier-entry-live-buffer entry)))
                (let* ((job (atelier-entry-job entry))
                       (directory (buffer-local-value 'default-directory buffer))
                       (foreground (atelier-job-foreground job table))
                       (recipe (if foreground
                                   (atelier-job-recipe job foreground directory)
                                 (unless (eq (plist-get job :policy) 'never)
                                   (atelier-shell-restart-recipe
                                    (plist-get job :shell) directory)))))
                  (unless (equal recipe (plist-get job :recipe))
                    (setf (plist-get job :recipe) recipe)
                    (setq changed t))))))
          (when changed (atelier-persist-now)))
      (error (myconfig-log "Foreground job observation failed: %s" error)))))

(defun atelier-live-process-state ()
  (let ((table (universel-process-table (universel-host-environment))) state)
    (dolist (workspace atelier-workspaces)
      (dolist (entry (atelier-workspace-job-entries workspace))
        (let* ((job (atelier-entry-job entry))
               (foreground (and table (atelier-job-foreground job table))))
          (push (list :workspace (plist-get workspace :name)
                      :buffer (plist-get job :buffer)
                      :program (and foreground (plist-get foreground :executable)))
                state))))
    (nreverse state)))

(defun atelier-saved-process-state (data)
  (let (state)
    (dolist (workspace (mapcar #'atelier-workspace-runtime-copy
                               (plist-get data :workspaces)))
      (dolist (entry (atelier-workspace-job-entries workspace))
        (let ((job (atelier-entry-job entry)))
          (push (list :workspace (plist-get workspace :name)
                      :buffer (plist-get job :buffer)
                      :program (plist-get (plist-get job :recipe) :executable))
                state))))
    (nreverse state)))

(defun atelier-process-replacements (live saved)
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

(defun atelier-stop-all-live-jobs ()
  (dolist (workspace atelier-workspaces)
    (atelier-workspace-stop-jobs workspace)))

(defun atelier-job-process-exited (buffer)
  (when-let* ((owner (atelier-find-job-for-buffer (buffer-name buffer))))
    (let ((workspace (car owner))
          (entry (nth 2 owner)))
      (unless atelier-preserve-job-recipe
        (atelier-entry-remove workspace entry t))
      (myconfig-log "Job in %s exited: %s"
                    (plist-get workspace :name) (buffer-name buffer)))
    (atelier-notify-change)
    (atelier-persist-schedule)))

(defun atelier-set-job-policy ()
  (interactive)
  (let* ((owner (or (atelier-find-job-for-buffer (buffer-name))
                    (user-error "This buffer is not a workbench job")))
         (job (nth 1 owner))
         (choice (completing-read "Restart policy: " '("Auto" "Always" "Never") nil t))
         (policy (intern (downcase choice))))
    (setf (plist-get job :policy) policy)
    (when (eq policy 'never) (setf (plist-get job :recipe) nil))
    (atelier-observe-jobs)
    (message "Restart policy for %s: %s" (buffer-name) choice)))

(defun atelier-build-restart-plan (workspace)
  (cl-loop for entry in (atelier-workspace-job-entries workspace)
           for job = (atelier-entry-job entry)
           for recipe = (plist-get job :recipe)
           when (plist-get recipe :executable)
           collect (list :workspace workspace :entry entry :job job
                         :recipe (copy-tree recipe)
                         :buffer (plist-get job :buffer))))

(defun atelier-restart-entry-valid-p (entry)
  (let ((workspace (plist-get entry :workspace))
        (job (plist-get entry :job))
        (workspace-entry (plist-get entry :entry)))
    (and (memq workspace atelier-workspaces)
          (memq workspace-entry (atelier-workspace-entries workspace))
         (equal (plist-get job :buffer) (plist-get entry :buffer))
         (equal (plist-get job :recipe) (plist-get entry :recipe))
         (not (when-let* ((buffer (get-buffer (plist-get entry :buffer)))
                          (process (get-buffer-process buffer)))
                (process-live-p process))))))

(defun atelier-validate-restart-plan (plan)
  (when (and atelier-restart-topology-guard
             (not (equal atelier-restart-topology-guard
                         (atelier-data-topology (atelier-snapshot-data)))))
    (error "Topology changed before restarting saved jobs"))
  (unless (cl-every #'atelier-restart-entry-valid-p plan)
    (error "A saved split or job changed before restarting saved jobs")))

(defun atelier-restart-saved-jobs (&optional workspace)
  (when (fboundp 'myconfig-terminal-buffer)
    (let ((workspace (or workspace (atelier-current-workspace))))
      (when workspace
        (let ((plan (atelier-build-restart-plan workspace)))
          (atelier-workspace-stop-jobs workspace)
          (atelier-validate-restart-plan plan)
          (dolist (entry plan)
            (unless (atelier-restart-entry-valid-p entry)
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
                           (atelier-job-owner-entry (plist-get entry :entry))
                            (buffer (myconfig-terminal-buffer
                                     name (plist-get recipe :directory) executable arguments workspace
                                     (and shell-restart shell)
                                     (plist-get job :agent)
                                     (plist-get (plist-get entry :entry) :type))))
                      (setf (plist-get job :buffer) (buffer-name buffer))
                      (when-let* ((agent (plist-get job :agent)))
                        (run-hook-with-args 'atelier-agent-restored-functions
                                           buffer agent workspace))
                    (myconfig-log "Accepted restart of %s in workspace %s"
                                  (file-name-nondirectory executable)
                                  (plist-get workspace :name)))
                (error
                 (myconfig-log "Job restart failed for %s in workspace %s: %s"
                               (file-name-nondirectory executable)
                               (plist-get workspace :name) error))))))))))

(defun atelier-snapshot-data ()
  (atelier-ensure-detached-workspace)
  (unless (or atelier-persist-restoring atelier-navigator-window-configurations)
    (atelier-capture-current-workspace))
  (list :version 8
        :generation (or atelier-snapshot-generation (atelier-new-generation))
        :current-workspace-id (atelier-current-workspace-id)
        :ssh-destinations atelier-remembered-ssh-destinations
        :workspaces (mapcar #'atelier-workspace-persistent-copy atelier-workspaces)))

(defun atelier-workspace-topology (workspace)
  (list :id (atelier-workspace-id workspace)
        :name (plist-get workspace :name)
         :destination (plist-get workspace :destination)
         :path (plist-get workspace :path)
         :platform (plist-get workspace :platform)
         :mount-root (plist-get workspace :mount-root)
         :status (atelier-workspace-status workspace)
         :entries (plist-get workspace :entries)))

(defun atelier-data-topology (data)
  (list :current-workspace-id (plist-get data :current-workspace-id)
        :workspaces (mapcar #'atelier-workspace-topology (plist-get data :workspaces))))

(defun atelier-affected-workspaces (left right)
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

(defun atelier-migrate-entry-kind (descriptor)
  (cond ((plist-get descriptor :file) 'file)
        ((plist-get descriptor :dired) 'directory)
        ((plist-get descriptor :scratch) 'scratch)
        (t 'transient)))

(defun atelier-migrate-workspace-v3 (saved-workspace)
  "Convert one legacy workspace's parallel buffer/job lists to entries."
  (let ((workspace (copy-tree saved-workspace)) entries
        (name-to-entry (make-hash-table :test #'equal)) layout)
    (dolist (descriptor (append (plist-get workspace :buffers)
                                (plist-get workspace :owned-buffers)))
      (let* ((name (plist-get descriptor :name))
             (entry (gethash name name-to-entry)))
        (unless entry
          (setq entry (list :id (atelier-new-entry-id)
                            :job nil
                            :kind (atelier-migrate-entry-kind descriptor)
                            :name name
                            :file (plist-get descriptor :file)
                            :directory (plist-get descriptor :directory)
                            :point (plist-get descriptor :point)
                            :persistent t))
          (puthash name entry name-to-entry)
          (push entry entries))))
    (dolist (descriptor (plist-get workspace :buffers))
      (when-let* ((entry (and name-to-entry
                              (gethash (plist-get descriptor :name) name-to-entry))))
        (push (list :entry-id (plist-get entry :id)
                    :point (plist-get descriptor :point)
                    :start (plist-get descriptor :start)
                    :selected (plist-get descriptor :selected))
              layout)))
    (dolist (job (plist-get workspace :jobs))
      (let* ((name (plist-get job :buffer))
             (entry (and name-to-entry (gethash name name-to-entry))))
        (unless entry
          (setq entry (list :id (atelier-new-entry-id) :kind 'terminal
                            :name name :directory (plist-get job :directory)
                            :persistent t :job job))
          (push entry entries))
        (setf (plist-get entry :kind) 'terminal
              (plist-get entry :job) job)))
    (cl-remf workspace :buffers)
    (cl-remf workspace :owned-buffers)
    (cl-remf workspace :jobs)
    (cl-remf workspace :agent-buffer)
    (setf (plist-get workspace :entries) (nreverse entries)
          (plist-get workspace :layout) (nreverse layout))
    workspace))

(defun atelier-migration-layout-chain (items &optional orientation)
  "Return a binary layout from (ENTRY . SPAN) ITEMS."
  (setq orientation (or orientation 'horizontal))
  (if (null (cdr items))
      (caar items)
    (let* ((first (car items))
           (rest (cdr items))
           (first-span (cdr first))
           (rest-span (apply #'+ (mapcar #'cdr rest))))
      (list :id (atelier-new-entry-id) :kind 'layout :orientation orientation
            :ratio (/ (float first-span) (+ first-span rest-span)) :persistent t
            :children
            (list (car first)
                  (atelier-migration-layout-chain rest orientation))))))

(defvar atelier-migration-layout-leaves nil)

(defun atelier-window-state-node-p (item)
  (and (listp item) (memq (nth 1 item) '(hc vc leaf))))

(defun atelier-window-state-span (state orientation)
  (or (alist-get (if (eq orientation 'horizontal) 'pixel-width 'pixel-height)
                 (cddr state))
      (alist-get (if (eq orientation 'horizontal) 'total-width 'total-height)
                 (cddr state))
      1))

(defun atelier-migration-layout-from-state (state)
  "Convert one V4 window STATE node using `atelier-migration-layout-leaves'."
  (pcase (nth 1 state)
    ('leaf (pop atelier-migration-layout-leaves))
    ((or 'hc 'vc)
     (let* ((orientation (if (eq (nth 1 state) 'hc) 'horizontal 'vertical))
            (children (cl-remove-if-not #'atelier-window-state-node-p (cddr state)))
            (items
             (delq nil
                   (mapcar (lambda (child)
                             (when-let* ((entry (atelier-migration-layout-from-state child)))
                               (cons entry (atelier-window-state-span child orientation))))
                           children))))
       (atelier-migration-layout-chain items orientation)))
    (_ nil)))

(defun atelier-migrate-workspace-v4 (saved-workspace)
  "Move V4's parallel layout records into its recursive entry hierarchy."
  (let* ((workspace (copy-tree saved-workspace))
         (entries (plist-get workspace :entries))
         (layout (plist-get workspace :layout))
         displayed displayed-ids)
    (dolist (descriptor layout)
      (when-let* ((id (plist-get descriptor :entry-id))
                  ((not (member id displayed-ids)))
                  (entry (cl-find id entries
                                  :key (lambda (item) (plist-get item :id))
                                  :test #'equal)))
        (setf (plist-get entry :point) (or (plist-get descriptor :point)
                                           (plist-get entry :point))
              (plist-get entry :start) (plist-get descriptor :start)
              (plist-get entry :selected) (plist-get descriptor :selected))
        (push id displayed-ids)
        (push entry displayed)))
    (setq displayed (nreverse displayed))
    (let* ((atelier-migration-layout-leaves (copy-sequence displayed))
           (from-state (and (atelier-window-state-node-p (plist-get workspace :state))
                            (atelier-migration-layout-from-state
                             (plist-get workspace :state))))
           (root (if (and from-state (null atelier-migration-layout-leaves))
                     from-state
                   (atelier-migration-layout-chain
                    (mapcar (lambda (entry) (cons entry 1)) displayed)))))
      (when root (setf (plist-get root :displayed) t))
      (setf (plist-get workspace :entries)
            (append (and root (list root))
                    (cl-remove-if
                     (lambda (entry) (member (plist-get entry :id) displayed-ids))
                     entries))))
    (cl-remf workspace :layout)
    (cl-remf workspace :state)
    workspace))

(defun atelier-entry-v6-type (entry)
  "Infer registered type metadata for a pre-V6 ENTRY."
  (pcase (plist-get entry :kind)
    ('directory 'dired)
    ('terminal
     (when-let* ((job (atelier-entry-job entry)))
       (if (or (consp (plist-get job :agent))
               (and (plist-get job :agent)
                    (member (file-name-nondirectory
                             (or (car (plist-get job :direct-command)) ""))
                            '("opencode" "claude" "codex" "fx"))))
           'aipanel
         'terminal)))))

(defun atelier-migrate-entry-v6 (entry)
  "Add registered type metadata recursively to pre-V6 ENTRY."
  (let ((entry (copy-tree entry)))
    (if (atelier-layout-entry-p entry)
        (setf (plist-get entry :children)
              (mapcar #'atelier-migrate-entry-v6 (atelier-entry-children entry)))
      (unless (plist-member entry :type)
        (when-let* ((type (atelier-entry-v6-type entry)))
          (setf (plist-get entry :type) type))))
    entry))

(defun atelier-migrate-workspace-v6 (workspace)
  "Add registered entry types to a pre-V6 WORKSPACE."
  (let ((workspace (copy-tree workspace)))
    (setf (plist-get workspace :entries)
          (mapcar #'atelier-migrate-entry-v6 (plist-get workspace :entries)))
    workspace))

(defun atelier-aipanel-entry-attached-p (entry)
  "Return non-nil when AIPanel ENTRY records a source entry attachment."
  (let* ((agent (plist-get (atelier-entry-job entry) :agent))
         (attachment (plist-get agent :attachment)))
    (and (eq (plist-get entry :type) 'aipanel)
         (stringp (plist-get attachment :entry-id)))))

(defun atelier-migrate-entry-v7 (entry)
  "Remove a legacy unattached AIPanel ENTRY and repair its layout tree."
  (if (atelier-layout-entry-p entry)
      (let* ((copy (copy-tree entry))
             (children (delq nil (mapcar #'atelier-migrate-entry-v7
                                         (atelier-entry-children entry))))
             (displayed (plist-get entry :displayed)))
        (pcase (length children)
          (0 nil)
          (1 (atelier-entry-with-display-state (car children) displayed))
          (_ (setf (plist-get copy :children) children)
             copy)))
    (unless (and (eq (plist-get entry :type) 'aipanel)
                 (not (atelier-aipanel-entry-attached-p entry)))
      (copy-tree entry))))

(defun atelier-migrate-workspace-v7 (workspace)
  "Discard legacy workspace-level panels which have no source attachment."
  (let ((workspace (copy-tree workspace)))
    (setf (plist-get workspace :entries)
          (delq nil (mapcar #'atelier-migrate-entry-v7
                            (plist-get workspace :entries))))
    (cl-remf workspace :agent-directory)
    workspace))

(defun atelier-migrate-data-v7 (data)
  (list :version 7
        :generation (plist-get data :generation)
        :current-workspace-id (plist-get data :current-workspace-id)
        :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
        :workspaces (mapcar #'atelier-migrate-workspace-v7
                            (plist-get data :workspaces))))

(defun atelier-migrate-data-v8 (data)
  "Migrate the v7 recursive entry trees to flat ID-linked records."
  (let* ((data (atelier-migrate-data-v7 data))
         (copy (copy-tree data)))
    (setf (plist-get copy :version) 8
          (plist-get copy :workspaces)
          (mapcar #'atelier-workspace-flat-copy
                  (plist-get data :workspaces)))
    copy))

(defun atelier-migrate-state (data)
  (pcase (plist-get data :version)
    (8 data)
    (7 (atelier-migrate-data-v8 data))
    (6 (atelier-migrate-state (atelier-migrate-data-v7 data)))
    (5
     (atelier-migrate-state
      (list :version 6
            :generation (plist-get data :generation)
            :current-workspace-id (plist-get data :current-workspace-id)
            :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
            :workspaces (mapcar #'atelier-migrate-workspace-v6
                                (plist-get data :workspaces)))))
    (4
     (atelier-migrate-state
      (list :version 5
            :generation (plist-get data :generation)
            :current-workspace-id (plist-get data :current-workspace-id)
            :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
            :workspaces (mapcar #'atelier-migrate-workspace-v4
                                (plist-get data :workspaces)))))
    (3
     (atelier-migrate-state
      (list :version 4
            :generation (plist-get data :generation)
            :current-workspace-id (plist-get data :current-workspace-id)
            :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
            :workspaces (mapcar #'atelier-migrate-workspace-v3
                                (plist-get data :workspaces)))))
    (2
     (let* ((workspaces
             (mapcar
              (lambda (saved-workspace)
                (let ((workspace (copy-tree saved-workspace)))
                  (setq workspace
                        (plist-put workspace :id (atelier-new-workspace-id)))
                  (setq workspace
                        (plist-put workspace :status
                                   (if (plist-get workspace :live)
                                       'running
                                     'stopped)))
                  (cl-remf workspace :live)
                  workspace))
              (plist-get data :workspaces)))
            (current-name (plist-get data :current-workspace)))
       (let ((current (cl-find current-name workspaces
                               :key (lambda (workspace) (plist-get workspace :name))
                               :test #'equal)))
          (atelier-migrate-state
           (list :version 3
                 :generation (plist-get data :generation)
                 :current-workspace-id (and current (plist-get current :id))
                 :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
                 :workspaces workspaces)))))
    (1
     (atelier-migrate-state
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
                       :jobs (nreverse (copy-tree jobs)))))
             (plist-get data :workspaces)))))
    (_ data)))

(defun atelier-validate-runtime-state (data)
  ;; Older snapshots could leave several top-level entries marked as the
  ;; displayed layout when a window was replaced.  Keep the first displayed
  ;; root and make the remaining entries unplaced so the state can recover.
  (dolist (workspace (plist-get data :workspaces))
    (let ((displayed-seen nil))
      (dolist (entry (plist-get workspace :entries))
        (when (plist-get entry :displayed)
          (if displayed-seen
              (atelier-plist-clear! entry :displayed)
            (setq displayed-seen t))))))
  (unless (and (listp data) (equal (plist-get data :version) 7)
               (stringp (plist-get data :generation))
               (listp (plist-get data :workspaces)))
    (error "Invalid state header"))
  (let (names)
    (dolist (workspace (plist-get data :workspaces))
       (let ((id (plist-get workspace :id))
             (name (plist-get workspace :name))
            (destination (plist-get workspace :destination))
            (path (plist-get workspace :path)))
         (unless (and (stringp id) (not (string-empty-p id))
                       (stringp name) (not (string-empty-p name))
                       (memq (plist-get workspace :status) '(running stopped))
                      (stringp destination) (not (string-empty-p destination))
                       (stringp path) (not (string-empty-p path))
                         (listp (plist-get workspace :entries)))
          (error "Invalid workspace definition"))
        (when (member name names) (error "Duplicate workspace name: %s" name))
        (push name names)
        (when (and (equal destination "local") (not (file-directory-p path)))
          (error "Workspace root is missing: %s" path))
        (let ((top-level (atelier-workspace-top-level-entries workspace))
              entry-ids)
          (when (> (cl-count-if (lambda (entry) (plist-get entry :displayed))
                                top-level)
                   1)
            (error "Workspace %s has multiple displayed entry roots" name))
          (cl-labels
              ((validate-entry
                (entry nested)
                 (let ((entry-id (plist-get entry :id))
                       (kind (plist-get entry :kind))
                       (type (plist-get entry :type)))
                  (unless (and (stringp entry-id) (not (string-empty-p entry-id))
                               (symbolp kind))
                    (error "Invalid entry record in workspace %s" name))
                  (when (member entry-id entry-ids)
                    (error "Duplicate entry ID in workspace %s" name))
                   (push entry-id entry-ids)
                   (when type (atelier-entry-type-definition type))
                  (when (and nested (plist-get entry :displayed))
                    (error "Nested entry is marked displayed in workspace %s" name))
                  (if (eq kind 'layout)
                      (progn
                        (unless (and (= (length (atelier-entry-children entry)) 2)
                                     (memq (plist-get entry :orientation)
                                           '(horizontal vertical))
                                     (numberp (plist-get entry :ratio))
                                     (> (plist-get entry :ratio) 0)
                                     (< (plist-get entry :ratio) 1))
                          (error "Invalid layout entry in workspace %s" name))
                        (dolist (child (atelier-entry-children entry))
                          (validate-entry child t)))
                    (when-let* ((job (atelier-entry-job entry)))
                      (unless (and (listp job) (stringp (plist-get job :id))
                                   (stringp (plist-get job :buffer))
                                   (memq (plist-get job :policy) '(auto always never)))
                        (error "Invalid terminal job in workspace %s" name)))))))
            (dolist (entry top-level) (validate-entry entry nil))))))
    (dolist (workspace (plist-get data :workspaces))
      (dolist (entry (atelier-workspace-entries workspace))
        (when (eq (plist-get entry :type) 'aipanel)
          (let* ((attachment (plist-get (plist-get (atelier-entry-job entry) :agent)
                                        :attachment))
                 (source-id (plist-get attachment :entry-id))
                 (source
                  (cl-loop for candidate-workspace in (plist-get data :workspaces)
                           thereis (atelier-entry-by-id candidate-workspace source-id))))
            (unless (and (stringp source-id) source
                         (not (eq (plist-get source :type) 'aipanel)))
              (error "Invalid AIPanel attachment in workspace %s"
                     (plist-get workspace :name)))))))
    data))

(defun atelier-validate-state (data)
  "Validate flat v8 state by rebuilding its runtime entry trees."
  (setq data (atelier-migrate-state data))
  (unless (and (listp data) (equal (plist-get data :version) 8)
               (listp (plist-get data :workspaces)))
    (error "Invalid flat state header"))
  (let ((runtime (copy-tree data)))
    (setf (plist-get runtime :version) 7
          (plist-get runtime :workspaces)
          (mapcar #'atelier-workspace-runtime-copy
                  (plist-get data :workspaces)))
    (setq runtime (atelier-validate-runtime-state runtime))
    (let ((normalized (copy-tree data)))
      (setf (plist-get normalized :workspaces)
            (mapcar #'atelier-workspace-flat-copy
                    (plist-get runtime :workspaces)))
      normalized)))

(defun atelier-apply-state (data)
  (setq atelier-workspaces
        (mapcar #'atelier-workspace-runtime-copy (plist-get data :workspaces))
        atelier-remembered-ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
        atelier-snapshot-generation (plist-get data :generation))
  (atelier-ensure-detached-workspace)
  (atelier-select-workspace
   (atelier-workspace-by-id (plist-get data :current-workspace-id)))
  (clrhash atelier-entry-live-buffers)
  (dolist (workspace atelier-workspaces)
    (atelier-set-workspace-status workspace 'stopped)
    (dolist (entry (atelier-workspace-job-entries workspace))
      (let ((job (atelier-entry-job entry)))
        (unless (or (plist-get job :recipe)
                    (eq (plist-get job :policy) 'never))
          (setf (plist-get job :recipe)
                (atelier-shell-restart-recipe
                 (plist-get job :shell) (plist-get job :directory))))
        (unless (or (plist-get job :recipe) (eq (plist-get job :policy) 'never))
          (myconfig-log "Terminal entry %s has no usable restart recipe"
                        (plist-get entry :name))))))
  (atelier-persist-open-saved-state)
  (run-hooks 'atelier-after-restore-hook))

(defun atelier-persist-now ()
  (interactive)
  (unless atelier-persist-restoring
    (condition-case error
        (progn
          (run-hooks 'atelier-before-save-hook)
          (setq atelier-snapshot-generation (atelier-new-generation))
          (myconfig-write-data-atomically atelier-persist-state-file (atelier-snapshot-data))
          (run-hooks 'atelier-after-save-hook))
      (error (myconfig-log "Snapshot failed: %s" error)))))

(defun atelier-persist-schedule ()
  (unless atelier-persist-restoring
    (when atelier-persist-timer (cancel-timer atelier-persist-timer))
    (setq atelier-persist-timer (run-with-idle-timer 0.5 nil #'atelier-persist-now))))

(defun atelier-persist-load ()
  (condition-case error
      (when-let* ((data (myconfig-read-data atelier-persist-state-file)))
        (setq data (atelier-validate-state data))
        (atelier-apply-state data)
        t)
    (error
     (myconfig-log "Stored state rejected: %s" error)
     nil)))

(defun atelier-persist-open-saved-state ()
  (when atelier-workspaces
    (let ((workspace (or (atelier-current-workspace) (car atelier-workspaces))))
      (when workspace
        (atelier-select-workspace workspace)
        (atelier-set-workspace-status workspace 'running)
        (unless atelier-defer-job-restart
          (atelier-restart-saved-jobs workspace)
          (when (display-graphic-p (selected-frame))
            (atelier-restore-workspace workspace)))))))

(defun atelier-restore-journal-recover ()
  (when-let* ((journal (myconfig-read-data atelier-persist-restore-journal-file))
              (original (plist-get journal :original)))
    (setq original (atelier-validate-state original))
    (myconfig-write-data-atomically atelier-persist-state-file original)
    (delete-file atelier-persist-restore-journal-file)
    (myconfig-log "Recovered the state that existed before an interrupted restore")))

(defun atelier-restore-snapshot ()
  (interactive)
  (let* ((saved (atelier-validate-state (or (myconfig-read-data atelier-persist-state-file)
                                              (user-error "No saved workbench state"))))
         (live (atelier-snapshot-data))
         (expected (atelier-data-topology live))
         (wanted (atelier-data-topology saved))
         (affected (atelier-affected-workspaces expected wanted))
         (expected-processes (atelier-live-process-state))
         (saved-processes (atelier-saved-process-state saved))
         (replacements (atelier-process-replacements expected-processes saved-processes)))
    (when (equal expected wanted) (user-error "Live and saved topology already match"))
    (unless (yes-or-no-p
             (format "Replace live state for workspace%s %s? "
                     (if (= (length affected) 1) "" "s")
                     (string-join affected ", ")))
      (atelier-persist-now)
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
        (atelier-persist-now)
        (user-error "Saved the unchanged live state")))
    (unless (equal expected (atelier-data-topology (atelier-snapshot-data)))
      (user-error "Restore cancelled because live topology changed"))
    (unless (equal expected-processes (atelier-live-process-state))
      (user-error "Restore cancelled because a live job changed"))
    (let ((token (atelier-new-generation))
          (atelier-persist-restoring t)
          (atelier-defer-job-restart t))
      (myconfig-write-data-atomically
       atelier-persist-restore-journal-file (list :token token :original live :replacement saved))
      (condition-case error
          (progn
            (unless (equal expected (atelier-data-topology (atelier-snapshot-data)))
              (error "Live topology changed before replacement"))
            (unless (equal expected-processes (atelier-live-process-state))
              (error "A live job changed before replacement"))
            (atelier-stop-all-live-jobs)
            (atelier-apply-state saved)
            (let ((atelier-restart-topology-guard wanted))
              (atelier-restart-saved-jobs))
            (when (display-graphic-p (selected-frame))
              (atelier-restore-workspace (atelier-current-workspace)))
            (let ((journal (myconfig-read-data atelier-persist-restore-journal-file)))
              (when (equal token (plist-get journal :token))
                (delete-file atelier-persist-restore-journal-file)))
            (atelier-persist-now)
            (message "Restored workbench state"))
        (error
          (atelier-apply-state live)
          (let ((atelier-restart-topology-guard expected))
            (atelier-restart-saved-jobs))
          (when (display-graphic-p (selected-frame))
            (atelier-restore-workspace (atelier-current-workspace)))
         (myconfig-log "Restore failed; original live state recovered: %s" error)
         (signal (car error) (cdr error)))))))

(defun atelier-persist-setup ()
  (let ((atelier-persist-restoring t))
    (condition-case error
        (atelier-restore-journal-recover)
      (error (myconfig-log "Restore journal recovery failed: %s" error)))
    (atelier-persist-load))
  (add-hook 'atelier-change-hook #'atelier-persist-schedule)
  (add-hook 'kill-emacs-hook #'atelier-persist-now)
  (setq atelier-job-observer-timer (run-with-timer 5 5 #'atelier-observe-jobs)))

(provide 'atelier-persist)
;;; atelier-persist.el ends here

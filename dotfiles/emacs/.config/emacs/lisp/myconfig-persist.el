;;; myconfig-persist.el --- Private snapshots and restart recipes -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'myconfig-core)
(require 'atelier)
(require 'univers)

(defvar myconfig-persist-timer nil)
(defvar myconfig-persist-restoring nil)
(defvar myconfig-snapshot-generation nil)
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

(defun myconfig-job-foreground (job table)
  (when-let* ((buffer (get-buffer (plist-get job :buffer)))
              (process (get-buffer-process buffer))
               (pid (with-current-buffer buffer
                      (or (and (boundp 'ghostel--pid) ghostel--pid)
                          (process-id process)))))
    (universel-foreground-process pid table (plist-get job :direct-command))))

(defun myconfig-job-recipe (job foreground directory)
  (let ((policy (plist-get job :policy))
        (program (file-name-nondirectory (plist-get foreground :executable)))
        (fallback (atelier-shell-restart-recipe (plist-get job :shell) directory)))
    (cond
     ((eq policy 'never) nil)
     ((and (eq policy 'auto)
            (< (universel-process-runtime foreground (universel-host-environment)) 5)) fallback)
     ((and (eq policy 'auto) (member program myconfig-process-denylist)) fallback)
     (t (list :executable (plist-get foreground :executable)
              :argv (copy-sequence (plist-get foreground :argv))
              :directory directory
              :shell (copy-tree (plist-get job :shell)))))))

(defun myconfig-observe-jobs ()
  (when (universel-process-observation-p (universel-host-environment))
    (condition-case error
        (let ((table (universel-process-table (universel-host-environment))) changed)
          (dolist (workspace atelier-workspaces)
            (dolist (entry (atelier-workspace-job-entries workspace))
              (when-let* ((buffer (atelier-entry-live-buffer entry)))
                (let* ((job (atelier-entry-job entry))
                       (directory (buffer-local-value 'default-directory buffer))
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
  (let ((table (universel-process-table (universel-host-environment))) state)
    (dolist (workspace atelier-workspaces)
      (dolist (entry (atelier-workspace-job-entries workspace))
        (let* ((job (atelier-entry-job entry))
               (foreground (and table (myconfig-job-foreground job table))))
          (push (list :workspace (plist-get workspace :name)
                      :buffer (plist-get job :buffer)
                      :program (and foreground (plist-get foreground :executable)))
                state))))
    (nreverse state)))

(defun myconfig-saved-process-state (data)
  (let (state)
    (dolist (workspace (plist-get data :workspaces))
      (dolist (entry (atelier-workspace-job-entries workspace))
        (let ((job (atelier-entry-job entry)))
          (push (list :workspace (plist-get workspace :name)
                      :buffer (plist-get job :buffer)
                      :program (plist-get (plist-get job :recipe) :executable))
                state))))
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
          (entry (nth 2 owner)))
      (unless atelier-preserve-job-recipe
        (atelier-entry-remove workspace entry t))
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
  (cl-loop for entry in (atelier-workspace-job-entries workspace)
           for job = (atelier-entry-job entry)
           for recipe = (plist-get job :recipe)
           when (plist-get recipe :executable)
           collect (list :workspace workspace :entry entry :job job
                         :recipe (copy-tree recipe)
                         :buffer (plist-get job :buffer))))

(defun myconfig-restart-entry-valid-p (entry)
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
      (when workspace
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

(defun myconfig-snapshot-data ()
  (atelier-ensure-detached-workspace)
  (unless (or myconfig-persist-restoring atelier-navigator-window-configurations)
    (atelier-capture-current-workspace))
  (list :version 7
        :generation (or myconfig-snapshot-generation (myconfig-new-generation))
        :current-workspace-id (atelier-current-workspace-id)
        :ssh-destinations atelier-remembered-ssh-destinations
        :workspaces (mapcar #'atelier-workspace-persistent-copy atelier-workspaces)))

(defun myconfig-workspace-topology (workspace)
  (list :id (atelier-workspace-id workspace)
        :name (plist-get workspace :name)
         :destination (plist-get workspace :destination)
         :path (plist-get workspace :path)
         :platform (plist-get workspace :platform)
         :mount-root (plist-get workspace :mount-root)
         :status (atelier-workspace-status workspace)
         :entries (plist-get workspace :entries)))

(defun myconfig-data-topology (data)
  (list :current-workspace-id (plist-get data :current-workspace-id)
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

(defun myconfig-migrate-entry-kind (descriptor)
  (cond ((plist-get descriptor :file) 'file)
        ((plist-get descriptor :dired) 'directory)
        ((plist-get descriptor :scratch) 'scratch)
        (t 'transient)))

(defun myconfig-migrate-workspace-v3 (saved-workspace)
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
                            :kind (myconfig-migrate-entry-kind descriptor)
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

(defun myconfig-migration-layout-chain (items &optional orientation)
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
                  (myconfig-migration-layout-chain rest orientation))))))

(defvar myconfig-migration-layout-leaves nil)

(defun myconfig-window-state-node-p (item)
  (and (listp item) (memq (nth 1 item) '(hc vc leaf))))

(defun myconfig-window-state-span (state orientation)
  (or (alist-get (if (eq orientation 'horizontal) 'pixel-width 'pixel-height)
                 (cddr state))
      (alist-get (if (eq orientation 'horizontal) 'total-width 'total-height)
                 (cddr state))
      1))

(defun myconfig-migration-layout-from-state (state)
  "Convert one V4 window STATE node using `myconfig-migration-layout-leaves'."
  (pcase (nth 1 state)
    ('leaf (pop myconfig-migration-layout-leaves))
    ((or 'hc 'vc)
     (let* ((orientation (if (eq (nth 1 state) 'hc) 'horizontal 'vertical))
            (children (cl-remove-if-not #'myconfig-window-state-node-p (cddr state)))
            (items
             (delq nil
                   (mapcar (lambda (child)
                             (when-let* ((entry (myconfig-migration-layout-from-state child)))
                               (cons entry (myconfig-window-state-span child orientation))))
                           children))))
       (myconfig-migration-layout-chain items orientation)))
    (_ nil)))

(defun myconfig-migrate-workspace-v4 (saved-workspace)
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
    (let* ((myconfig-migration-layout-leaves (copy-sequence displayed))
           (from-state (and (myconfig-window-state-node-p (plist-get workspace :state))
                            (myconfig-migration-layout-from-state
                             (plist-get workspace :state))))
           (root (if (and from-state (null myconfig-migration-layout-leaves))
                     from-state
                   (myconfig-migration-layout-chain
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

(defun myconfig-entry-v6-type (entry)
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

(defun myconfig-migrate-entry-v6 (entry)
  "Add registered type metadata recursively to pre-V6 ENTRY."
  (let ((entry (copy-tree entry)))
    (if (atelier-layout-entry-p entry)
        (setf (plist-get entry :children)
              (mapcar #'myconfig-migrate-entry-v6 (atelier-entry-children entry)))
      (unless (plist-member entry :type)
        (when-let* ((type (myconfig-entry-v6-type entry)))
          (setf (plist-get entry :type) type))))
    entry))

(defun myconfig-migrate-workspace-v6 (workspace)
  "Add registered entry types to a pre-V6 WORKSPACE."
  (let ((workspace (copy-tree workspace)))
    (setf (plist-get workspace :entries)
          (mapcar #'myconfig-migrate-entry-v6 (plist-get workspace :entries)))
    workspace))

(defun myconfig-aipanel-entry-attached-p (entry)
  "Return non-nil when AIPanel ENTRY records a source entry attachment."
  (let* ((agent (plist-get (atelier-entry-job entry) :agent))
         (attachment (plist-get agent :attachment)))
    (and (eq (plist-get entry :type) 'aipanel)
         (stringp (plist-get attachment :entry-id)))))

(defun myconfig-migrate-entry-v7 (entry)
  "Remove a legacy unattached AIPanel ENTRY and repair its layout tree."
  (if (atelier-layout-entry-p entry)
      (let* ((copy (copy-tree entry))
             (children (delq nil (mapcar #'myconfig-migrate-entry-v7
                                         (atelier-entry-children entry))))
             (displayed (plist-get entry :displayed)))
        (pcase (length children)
          (0 nil)
          (1 (atelier-entry-with-display-state (car children) displayed))
          (_ (setf (plist-get copy :children) children)
             copy)))
    (unless (and (eq (plist-get entry :type) 'aipanel)
                 (not (myconfig-aipanel-entry-attached-p entry)))
      (copy-tree entry))))

(defun myconfig-migrate-workspace-v7 (workspace)
  "Discard legacy workspace-level panels which have no source attachment."
  (let ((workspace (copy-tree workspace)))
    (setf (plist-get workspace :entries)
          (delq nil (mapcar #'myconfig-migrate-entry-v7
                            (plist-get workspace :entries))))
    (cl-remf workspace :agent-directory)
    workspace))

(defun myconfig-migrate-data-v7 (data)
  (list :version 7
        :generation (plist-get data :generation)
        :current-workspace-id (plist-get data :current-workspace-id)
        :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
        :workspaces (mapcar #'myconfig-migrate-workspace-v7
                            (plist-get data :workspaces))))

(defun myconfig-migrate-state (data)
  (pcase (plist-get data :version)
    (7 data)
    (6 (myconfig-migrate-data-v7 data))
    (5
     (myconfig-migrate-state
      (list :version 6
            :generation (plist-get data :generation)
            :current-workspace-id (plist-get data :current-workspace-id)
            :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
            :workspaces (mapcar #'myconfig-migrate-workspace-v6
                                (plist-get data :workspaces)))))
    (4
     (myconfig-migrate-state
      (list :version 5
            :generation (plist-get data :generation)
            :current-workspace-id (plist-get data :current-workspace-id)
            :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
            :workspaces (mapcar #'myconfig-migrate-workspace-v4
                                (plist-get data :workspaces)))))
    (3
     (myconfig-migrate-state
      (list :version 4
            :generation (plist-get data :generation)
            :current-workspace-id (plist-get data :current-workspace-id)
            :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
            :workspaces (mapcar #'myconfig-migrate-workspace-v3
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
          (myconfig-migrate-state
           (list :version 3
                 :generation (plist-get data :generation)
                 :current-workspace-id (and current (plist-get current :id))
                 :ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
                 :workspaces workspaces)))))
    (1
     (myconfig-migrate-state
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

(defun myconfig-validate-state (data)
  (setq data (myconfig-migrate-state data))
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

(defun myconfig-apply-state (data)
  (setq atelier-workspaces (copy-tree (plist-get data :workspaces))
        atelier-remembered-ssh-destinations (copy-sequence (plist-get data :ssh-destinations))
        myconfig-snapshot-generation (plist-get data :generation))
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
  (myconfig-persist-open-saved-state)
  (run-hooks 'atelier-after-restore-hook))

(defun myconfig-persist-now ()
  (interactive)
  (unless myconfig-persist-restoring
    (condition-case error
        (progn
          (run-hooks 'atelier-before-save-hook)
          (setq myconfig-snapshot-generation (myconfig-new-generation))
          (myconfig-write-data-atomically myconfig-state-file (myconfig-snapshot-data))
          (run-hooks 'atelier-after-save-hook))
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
  (when atelier-workspaces
    (let ((workspace (or (atelier-current-workspace) (car atelier-workspaces))))
      (when workspace
        (atelier-select-workspace workspace)
        (atelier-set-workspace-status workspace 'running)
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

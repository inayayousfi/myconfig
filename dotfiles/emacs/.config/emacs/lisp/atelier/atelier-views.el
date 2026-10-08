;;; atelier-views.el --- Open, close and move workspace contents in views -*- lexical-binding: t; -*-

;; The operations that change which contents and stacks a workspace's views
;; show: opening, closing, attaching and detaching them, and the windows that
;; hold those views.  Interfaces such as the navigator call these operations;
;; they never change workspace records themselves.

(require 'cl-lib)
(require 'subr-x)
(require 'atelier-model)
(require 'atelier-operation)

(declare-function atelier-assign-buffer-to-workspace "atelier")
(declare-function atelier-buffer-shared-with-running-workspace-p "atelier")
(declare-function atelier-capture-current-workspace "atelier")
(declare-function atelier-close-unassigned-view "atelier")
(declare-function atelier-display-entry-buffer "atelier")
(declare-function atelier-empty-workspace-buffer "atelier")
(declare-function atelier-interface-buffer-p "atelier")
(declare-function atelier-notify-change "atelier")
(declare-function atelier-register-buffer "atelier")
(declare-function atelier-restore-buffer "atelier")
(declare-function atelier-restore-entry-buffer "atelier")
(declare-function atelier-restore-entry-content "atelier")
(declare-function atelier-show-buffer "atelier")
(declare-function atelier-switch-workspace "atelier")
(declare-function atelier-uncover-frame "atelier")
(declare-function atelier-workspace-entry-for-buffer "atelier")
(declare-function atelier-workspace-stop-jobs "atelier")
(declare-function atelier--open-workspace "atelier")

(defun atelier-buffer-list ()
  (let ((workspace (atelier-current-workspace)))
    (delete-dups
     (append
      (delq nil (mapcar #'atelier-entry-live-buffer
                        (and workspace (atelier-workspace-entries workspace))))
      (cl-remove-if-not
       (lambda (buffer)
         (member (buffer-name buffer) atelier-global-buffer-names))
       (buffer-list))))))

(atelier-define-operation atelier-new-scratch-buffer (&optional workspace)
    (list (atelier-workspace-id (or workspace (atelier-current-workspace))))
    ((workspace (atelier-operation-workspace (or workspace (atelier-current-workspace)))))
  (interactive)
  (let* ((workspace (or workspace (atelier-current-workspace)))
         (_ (unless (eq workspace (atelier-current-workspace))
              (atelier-switch-workspace (plist-get workspace :name))))
         (buffer (generate-new-buffer "*scratch*")))
    (with-current-buffer buffer
      (funcall initial-major-mode))
    (atelier-assign-buffer-to-workspace buffer workspace)
    (atelier-show-buffer buffer workspace)
    buffer))

(defun atelier-cleanup-candidate-entries ()
  (delete-dups
   (append (copy-sequence
             (mapcar (lambda (content) (atelier-content-reference
                                       (atelier-ensure-detached-workspace) content))
                     (plist-get (atelier-ensure-detached-workspace) :contents)))
            (cl-loop for workspace in (atelier-user-workspaces) append
                     (cl-loop for content in (plist-get workspace :contents)
                              when (eq (plist-get content :kind) 'scratch)
                              collect (atelier-content-reference workspace content))))))

(atelier-define-operation atelier-clear-scratch-and-detached-entries (&optional confirmed)
    (mapcar #'atelier-workspace-id atelier-workspaces) nil
  (interactive)
  (let ((entries (atelier-cleanup-candidate-entries))
        (killed 0))
    (if (null entries)
        (message "No scratch or detached entries to clear")
      (unless (or confirmed
                  (y-or-n-p (format "Close %d scratch or detached entr%s? "
                                    (length entries)
                                    (if (= (length entries) 1) "y" "ies"))))
        (user-error "Cancelled"))
      (dolist (entry entries)
        (when-let* ((workspace (atelier-entry-workspace entry)))
           (when (atelier-workspace-content workspace (plist-get entry :content-id))
             (atelier-close-entry workspace entry nil))
          (setq killed (1+ killed))))
      (atelier-notify-change)
      (message "Cleared %d scratch or detached entr%s"
               killed (if (= killed 1) "" "s")))))

(atelier-define-operation atelier-clear-all-buffers (&optional confirmed)
    (list :all) nil
  "Clear every user buffer and saved workspace buffer descriptor.
Modified file buffers are saved and running workspace jobs are stopped first."
  (interactive)
  (unless (or confirmed
              (yes-or-no-p
               "Clear all workspace, job, scratch, and detached buffers? "))
    (user-error "Cancelled"))
  (let* ((atelier-inhibit-buffer-ownership t)
         (internal atelier-global-buffer-names)
         (cleared 0)
         (buffers
          (cl-remove-if-not
           (lambda (buffer)
             (let ((name (buffer-name buffer)))
               (and (buffer-live-p buffer)
                    (not (minibufferp buffer))
                    (not (string-prefix-p " " name))
                    (not (member name internal))
                    (not (atelier-interface-buffer-p buffer))
                    (not (string-prefix-p atelier-empty-buffer-prefix name)))))
           (buffer-list))))
    (let ((atelier-approved-buffer-closes
           (append
            (mapcar #'atelier-prepare-buffer-close
                    (delete-dups
                     (append buffers
                             (cl-loop for workspace in atelier-workspaces append
                                      (mapcar #'atelier-entry-live-buffer
                                              (atelier-workspace-job-entries workspace))))))
            atelier-approved-buffer-closes)))
      (atelier-validate-buffer-closes)
      (dolist (workspace atelier-workspaces)
        (atelier-workspace-stop-jobs workspace t))
      (dolist (buffer buffers)
        (when (atelier-kill-buffer buffer)
          (setq cleared (1+ cleared))))
       (dolist (workspace atelier-workspaces)
         (setf (plist-get workspace :entries) nil)
         (dolist (content (copy-sequence (plist-get workspace :contents)))
           (atelier-workspace-drop-content workspace (plist-get content :id))))
      (atelier-notify-change)
      (message "Cleared %d buffer%s" cleared (if (= cleared 1) "" "s"))
      cleared)))

(defun atelier-focus-workspace-split (workspace-name index)
  (unless (eq (atelier-workspace-get workspace-name) (atelier-current-workspace))
    (atelier-switch-workspace workspace-name))
  (let* ((windows (atelier-main-windows))
         (window (nth index windows)))
    (unless (window-live-p window)
      (user-error "Split %d no longer exists" (1+ index)))
    (select-window window)
    window))

(defun atelier-workspace-buffer (workspace-name index entry-id)
  "Resolve a displayed leaf and ensure it belongs to the workspace's root.
The top-level entry containing the leaf is the disposition; selecting a nested
entry never invents a nested disposition."
  (when-let* ((workspace (atelier-workspace-get workspace-name))
              (entry (nth index (atelier-workspace-displayed-entries workspace))))
    (unless (equal entry-id (plist-get entry :id))
      (user-error "View assignment changed"))
    (unless (eq (atelier-workspace-entry-root workspace entry)
                (atelier-workspace-displayed-entry workspace))
      (user-error "Entry no longer belongs to the displayed disposition"))
    (atelier-restore-buffer entry workspace)))

(defun atelier-workspace-owned-buffer (workspace entry-id)
  (when-let* ((entry (atelier-entry-by-id workspace entry-id)))
    (atelier-restore-buffer entry workspace)))

(atelier-define-operation atelier-close-current-view (&optional window workspace entry-id buffer)
    (list (atelier-workspace-id workspace))
    ((window (or window (selected-window)))
     (workspace (atelier-operation-workspace
                 (or workspace (atelier-current-workspace (window-frame window)))))
     (entry-id (or entry-id
                   (when-let* ((index (cl-position window (atelier-main-windows (window-frame window)))))
                     (plist-get (nth index (atelier-workspace-displayed-entries workspace)) :id))))
     (buffer (or buffer (window-buffer window))))
  (interactive)
  (unless (and (window-live-p window) (buffer-live-p buffer)
               (eq (window-buffer window) buffer)
               (equal (atelier-current-workspace-id (window-frame window))
                      (atelier-workspace-id workspace)))
    (user-error "Original view changed while its close action waited"))
  (let* ((index (cl-position window (atelier-main-windows (window-frame window))))
         (entry (and entry-id (atelier-operation-entry workspace entry-id))))
    (when (and entry
               (not (and index (equal entry-id
                                      (plist-get (nth index (atelier-workspace-displayed-entries workspace)) :id))
                         (eq (atelier-entry-live-buffer entry) buffer))))
      (user-error "Original view assignment changed while its close action waited"))
    (with-selected-window window
      (let ((atelier-inhibit-buffer-ownership t))
        (if (cl-some (lambda (other)
                       (and (not (eq other window))
                            (equal (atelier-current-workspace-id (window-frame other))
                                   (atelier-workspace-id workspace))))
                     (get-buffer-window-list buffer nil t))
            (progn
              (when entry (atelier-entry-remove workspace entry t))
              (atelier-close-entry-window workspace window nil t)
              (atelier-capture-current-workspace)
              (atelier-notify-change))
          (if entry
              (atelier-close-entry workspace entry window)
            (atelier-close-buffer (buffer-name buffer) window)))))))

(defun atelier-workspace-replacement-entries (workspace)
  "Return WORKSPACE entries with recently used live entries first."
  (let* ((entries (mapcar (lambda (content) (atelier-content-reference workspace content))
                          (plist-get workspace :contents)))
         (recent
          (cl-loop for buffer in (buffer-list)
                   for entry = (cl-find-if
                                (lambda (candidate)
                                  (eq (atelier-entry-live-buffer candidate) buffer))
                                entries)
                   when entry collect entry)))
    (append recent (cl-set-difference entries recent :test #'eq))))

(defun atelier-workspace-replacement-buffer (workspace &optional type)
  "Return or restore the best replacement buffer in WORKSPACE.
Prefer the previous entry of TYPE when one remains."
  (let* ((entries (atelier-workspace-replacement-entries workspace))
          (same-type (cl-remove-if-not
                           (lambda (entry) (eq (atelier-entry-value entry :type) type))
                           entries)))
    (cl-loop for entry in (delete-dups (append same-type entries))
             when (atelier-workspace-content workspace (plist-get entry :content-id))
             thereis (or (atelier-entry-live-buffer entry)
                         (atelier-restore-buffer entry workspace)))))

(defun atelier-main-windows (&optional frame)
  "Return FRAME's ordinary windows in layout-tree order."
  (cl-labels ((leaves (node)
                (cond ((windowp node)
                       (unless (window-parameter node 'window-side) (list node)))
                      ((consp node) (cl-mapcan #'leaves (cddr node))))))
     (leaves (car (window-tree (or frame (selected-frame)))))))

(defun atelier-main-window (&optional frame)
  "Return FRAME's selected main window, or its most recently used main leaf."
  (let* ((frame (or frame (selected-frame)))
         (selected (frame-selected-window frame)))
    (if (and (not (window-minibuffer-p selected))
             (not (window-parameter selected 'window-side)))
        selected
      (car (sort (atelier-main-windows frame)
                 (lambda (left right) (> (window-use-time left) (window-use-time right))))))))

(defun atelier-close-entry-window (workspace window &optional type close-view-only)
  "Remove WINDOW's view, retaining its content when CLOSE-VIEW-ONLY is non-nil.
Otherwise step back within TYPE's stack, then remove or replace WINDOW."
  (when (window-live-p window)
    (let ((same-type-buffer (and (not close-view-only)
                                 (atelier-workspace-stack workspace type)
                                  (atelier-workspace-replacement-buffer workspace type))))
      (cond
       ((and close-view-only
             (> (length (atelier-main-windows (window-frame window))) 1))
        (delete-window window))
       (same-type-buffer
        (set-window-buffer window same-type-buffer))
       ((> (length (atelier-main-windows (window-frame window))) 1)
        (delete-window window))
       (t
        (set-window-buffer
         window
         (or (atelier-workspace-replacement-buffer workspace)
             (atelier-empty-workspace-buffer workspace)))))
      (when (window-live-p window)
        (set-window-prev-buffers window nil)
        (set-window-next-buffers window nil)))))

(defun atelier-dispose-unreferenced-buffer (buffer)
  "Stop BUFFER only after its last active or stacked entry reference is gone."
  (when (and (buffer-live-p buffer) (not (atelier-buffer-referenced-p buffer)))
    (let ((atelier-preserve-job-recipe nil))
      (atelier-kill-buffer buffer))))

(defun atelier-prepare-content-close (workspace _entry contents)
  "Confirm closing CONTENTS before removing their final buffer references."
  (unless atelier-close-without-asking
    (cl-loop for content in contents
             for buffer = (gethash (atelier-content-cache-key workspace (plist-get content :id))
                                   atelier-content-live-buffers)
             when (buffer-live-p buffer) collect (atelier-prepare-buffer-close buffer))))

(atelier-define-operation atelier-close-entry (workspace entry &optional window entire-entry)
    (list (atelier-workspace-id workspace))
    ((workspace (atelier-operation-workspace workspace))
     (entry (atelier-operation-entry workspace entry)))
  "Close selected workspace content.
With ENTIRE-ENTRY, delete ENTRY's whole stack, every view of it and
their splits."
  (let ((windows (and window (list window))))
    (if (and (not (plist-get entry :content-reference))
             (or (not (plist-get entry :content-id))
                 (and entire-entry (not (plist-get entry :stack-id)))))
        (progn
          (atelier-entry-remove workspace entry t)
          (when window (atelier-close-entry-window workspace window nil t)))
      (let* ((stack-id (plist-get entry :stack-id))
             (stack-windows
              (and entire-entry (eq workspace (atelier-current-workspace))
                   (cl-loop for view in (atelier-workspace-displayed-entries workspace)
                            for view-window in (atelier-main-windows)
                            when (equal stack-id (plist-get view :stack-id))
                            collect view-window)))
             (contents (if entire-entry (atelier-entry-stack entry)
                         (list (atelier-entry-content entry))))
             (type (atelier-entry-value entry :type))
             (buffers (mapcar (lambda (content)
                                (gethash (atelier-content-cache-key workspace (plist-get content :id))
                                         atelier-content-live-buffers)) contents))
             (atelier-approved-buffer-closes
              (append (atelier-prepare-content-close workspace entry contents)
                      atelier-approved-buffer-closes)))
        (atelier-validate-buffer-closes)
        (dolist (content contents)
          (atelier-workspace-drop-content workspace (plist-get content :id)))
        (when entire-entry
          (atelier-plist-set! workspace :stacks
                              (cl-remove stack-id (plist-get workspace :stacks)
                                         :key (lambda (stack) (plist-get stack :id)) :test #'equal)))
        (dolist (buffer buffers) (atelier-dispose-unreferenced-buffer buffer))
        (if entire-entry
            (progn
              (setq windows (delete-dups (append windows stack-windows)))
              (dolist (stack-window windows)
                (atelier-close-entry-window workspace stack-window nil t)))
          (when window (atelier-close-entry-window workspace window type)))))
    (when (and windows (eq workspace (atelier-current-workspace)))
      (atelier-capture-current-workspace)))
  (atelier-notify-change))

(defun atelier-close-buffer (name &optional window)
  (let ((window (or window (get-buffer-window name (selected-frame)))))
    (if-let* ((buffer (get-buffer name))
              (workspace (atelier-current-workspace))
              (index (and window (cl-position window (atelier-main-windows))))
              (entry (or (and index
                              (let ((candidate (nth index (atelier-workspace-displayed-entries
                                                           workspace))))
                                (and (eq buffer (atelier-entry-live-buffer candidate))
                                     candidate)))
                         (atelier-workspace-entry-for-buffer workspace buffer)
                         (atelier-register-buffer buffer workspace t))))
        (atelier-close-entry workspace entry window)
      (when-let* ((buffer (get-buffer name)))
        (atelier-kill-buffer buffer)))))

(defun atelier-workspace-entry-window (workspace index entry)
  "Return the live window at INDEX when it still displays ENTRY in WORKSPACE."
  (when (eq workspace (atelier-current-workspace))
    (when-let* ((window (nth index (atelier-main-windows)))
                (buffer (atelier-entry-live-buffer entry))
                ((eq (window-buffer window) buffer)))
      window)))

(defun atelier-remove-saved-workspace-buffer (workspace entry-id)
  "Remove ENTRY-ID from WORKSPACE even when it has no live buffer."
  (if-let* ((entry (atelier-entry-by-id workspace entry-id)))
      (atelier-close-entry workspace entry)
    (user-error "Workspace entry no longer exists")))

(atelier-define-operation atelier-close-entry-content (workspace entry content-id &optional window)
    (list (atelier-workspace-id workspace))
    ((workspace (atelier-operation-workspace workspace))
     (entry (atelier-operation-entry workspace entry)))
  "Remove CONTENT-ID from ENTRY, preserving its view and other contents."
  (let* ((active (plist-get (atelier-entry-content entry) :id))
         (content (cl-find content-id (cdr (atelier-entry-stack entry))
                           :key (lambda (item) (plist-get item :id))
                           :test #'equal)))
    (cond
     ((equal content-id active) (atelier-close-entry workspace entry window))
     ((not content) (user-error "Content no longer belongs to this entry"))
     (t
       (let* ((atelier-approved-buffer-closes
               (append (atelier-prepare-content-close workspace entry (list content))
                       atelier-approved-buffer-closes))
              (key (atelier-content-cache-key workspace content-id))
              (buffer (gethash key atelier-content-live-buffers)))
         (atelier-validate-buffer-closes)
         (atelier-workspace-drop-content workspace content-id)
        (atelier-dispose-unreferenced-buffer buffer)
        (atelier-notify-change))))))

;;; Moving stacks between workspaces and views

(atelier-define-operation atelier-attach-stack (source-workspace entry-id workspace &optional view-id index)
    (delete-dups (list (atelier-current-workspace-id)
                       (atelier-workspace-id source-workspace)
                       (atelier-workspace-id workspace)))
    ((source-workspace (atelier-operation-workspace source-workspace))
     (workspace (atelier-operation-workspace workspace)))
  "Attach SOURCE-WORKSPACE's stack ENTRY-ID to WORKSPACE.
With VIEW-ID, also show that stack in WORKSPACE's view VIEW-ID, which split
INDEX displays.  Attaching to a workspace changes ownership only; showing the
stack in a view never creates a split or another buffer."
  (let ((source (or (atelier-entry-by-id source-workspace entry-id)
                    (user-error "Select a workspace stack to attach")))
        (view (and view-id (or (atelier-entry-by-id workspace view-id)
                               (user-error "Select a workspace or view")))))
    (unless (eq source-workspace workspace)
      (atelier-entry-move source source-workspace workspace))
    (when view
      (atelier-view-assign-stack workspace view (plist-get source :stack-id)
                                 (plist-get source :content-id))
      (when (eq workspace (atelier-current-workspace))
        (atelier-uncover-frame)
        (let ((window (atelier-focus-workspace-split (plist-get workspace :name) index)))
          (atelier-display-entry-buffer view workspace window
                                        (or (atelier-entry-live-buffer view)
                                            (atelier-restore-entry-buffer view workspace))))))
    (atelier-notify-change)))

(atelier-define-operation atelier-detach-view (workspace view-id index)
    (delete-dups (list (atelier-current-workspace-id) atelier-detached-workspace-id
                       (atelier-workspace-id
                        (or workspace (user-error "Workspace no longer exists")))))
    ((workspace (atelier-operation-workspace workspace)))
  "Take the stack off WORKSPACE's view VIEW-ID, shown in split INDEX.
Close that split; closing the last one shows another workspace, Detached when
nothing else runs."
  (when (atelier-detached-workspace-p workspace)
    (user-error "Entry is already detached"))
  (atelier-ensure-detached-workspace)
  (atelier-uncover-frame)
  (unless (eq workspace (atelier-current-workspace))
    (atelier-switch-workspace (plist-get workspace :name)))
  (let* ((workspace (atelier-operation-workspace workspace))
         (entry (atelier-entry-by-id workspace view-id))
         (window (atelier-focus-workspace-split (plist-get workspace :name) index))
         (stack-id (plist-get entry :stack-id)))
    (unless entry (user-error "View no longer exists"))
    (atelier-view-unassign-stack workspace entry)
    (atelier-close-unassigned-view workspace entry window stack-id))
  (atelier-capture-current-workspace)
  (atelier-notify-change))

(atelier-define-operation atelier-detach-stack (workspace entry-id)
    (delete-dups (list (atelier-current-workspace-id) atelier-detached-workspace-id
                       (atelier-workspace-id
                        (or workspace (user-error "Workspace no longer exists")))))
    ((workspace (atelier-operation-workspace workspace)))
  "Detach WORKSPACE's ENTRY-ID: a stack no view shows moves to Detached, and a
view gives up its stack."
  (when (atelier-detached-workspace-p workspace)
    (user-error "Entry is already detached"))
  (let ((entry (or (atelier-entry-by-id workspace entry-id)
                   (user-error "Entry no longer exists"))))
    (if (plist-get entry :content-reference)
        (atelier-entry-move entry workspace (atelier-ensure-detached-workspace))
      (atelier-view-unassign-stack workspace entry)))
  (atelier-notify-change))

(atelier-define-operation atelier-open-content (workspace entry-id content-id &optional index)
    (delete-dups (list (atelier-current-workspace-id)
                       (atelier-workspace-id
                        (or workspace (user-error "Workspace no longer exists")))))
    ((workspace (atelier-operation-workspace workspace)))
  "Show CONTENT-ID of WORKSPACE's ENTRY-ID: in that view, displayed by split
INDEX, when INDEX is given, or else where the workspace shows buffers."
  (let ((entry (atelier-entry-by-id workspace entry-id)))
    (unless (and entry (cl-find content-id (atelier-entry-stack entry)
                                :key (lambda (item) (plist-get item :id))
                                :test #'equal))
      (user-error "Content no longer belongs to this entry"))
    (when index
      (unless (cl-find entry-id (atelier-workspace-displayed-entries workspace)
                       :key (lambda (view) (plist-get view :id)) :test #'equal)
        (user-error "View is no longer displayed")))
    (atelier-uncover-frame)
    (unless (eq workspace (atelier-current-workspace))
      (atelier--open-workspace workspace (selected-frame)))
    (setq entry (atelier-entry-by-id workspace entry-id))
    (let ((buffer (and entry (atelier-workspace-content workspace content-id)
                       (atelier-restore-entry-content entry workspace content-id))))
      (when (buffer-live-p buffer)
        (if index
            (let ((current-index (cl-position entry (atelier-workspace-displayed-entries workspace))))
              (unless current-index (user-error "View is no longer displayed"))
              (set-window-buffer (atelier-focus-workspace-split (plist-get workspace :name)
                                                                current-index)
                                 buffer))
          (atelier-show-buffer buffer workspace)))
      (atelier-notify-change))))

;;; Renaming buffers

(defun atelier-rename-live-buffer (buffer name)
  "Rename BUFFER to NAME, or a unique variant, and restore the old name if the
current operation fails."
  (let ((old (buffer-name buffer)))
    (atelier-operation-cleanup
     (lambda ()
       (when (buffer-live-p buffer)
         (with-current-buffer buffer (rename-buffer old)))))
    (with-current-buffer buffer (rename-buffer name t))))

(atelier-define-operation atelier-rename-content (workspace entry-id name)
    (list (atelier-workspace-id (or workspace (user-error "Workspace no longer exists"))))
    ((workspace (atelier-operation-workspace workspace)))
  "Rename the live buffer of WORKSPACE's ENTRY-ID to NAME and record that name."
  (let* ((entry (or (atelier-entry-by-id workspace entry-id)
                    (user-error "Entry no longer exists")))
         (buffer (or (atelier-entry-live-buffer entry)
                     (user-error "Entry buffer no longer exists"))))
    (atelier-entry-set-value entry :name (atelier-rename-live-buffer buffer name))))

(defun atelier-entry-buffer (workspace entry-id)
  "Return the live buffer of WORKSPACE's ENTRY-ID, or nil."
  (when-let* ((entry (and workspace (atelier-entry-by-id workspace entry-id))))
    (atelier-entry-live-buffer entry)))

(provide 'atelier-views)
;;; atelier-views.el ends here

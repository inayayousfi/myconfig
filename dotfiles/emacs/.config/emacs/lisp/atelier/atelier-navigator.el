;;; atelier-navigator.el --- Atelier navigator UI -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'dired)
(require 'subr-x)
(require 'atelier-model)

(defvar-keymap atelier-navigator-mode-map
  :parent special-mode-map
  "j" #'atelier-navigator-next
  "k" #'atelier-navigator-previous
  "<down>" #'atelier-navigator-next
  "<up>" #'atelier-navigator-previous
  "RET" #'atelier-navigator-open
  "a" #'atelier-navigator-attach
  "d" #'atelier-navigator-detach
  "f" #'isearch-forward
  "F" #'isearch-forward
  "x" #'atelier-navigator-close
  "X" #'atelier-navigator-close
  "r" #'atelier-navigator-rename
  "R" #'atelier-navigator-rename
  "q" #'atelier-navigator-quit)

(define-derived-mode atelier-navigator-mode special-mode "Atelier"
  (atelier-mark-internal-buffer)
  (setq-local header-line-format
              '(:eval (atelier-navigator-header)))
  (setq-local hl-line-face 'atelier-navigator-current
              cursor-type 'box
              truncate-lines t
              line-spacing 0.12
              display-line-numbers-type 'relative)
  (hl-line-mode 1)
  (display-line-numbers-mode 1))

(defun atelier-navigator-header-button (label command help)
  (concat " " (atelier-clickable-label
               (format "[%s]" label) command nil 'font-lock-keyword-face help)))

(defun atelier-navigator-header ()
  (if atelier-navigator-attach-source
      (list (propertize "  ATTACH" 'face 'success)
            (atelier-navigator-header-button "Choose" #'atelier-navigator-open
                                             "Attach to the selected item")
            (atelier-navigator-header-button "Cancel" #'atelier-navigator-quit
                                             "Cancel attachment"))
    (list (propertize "  NAVIGATOR" 'face 'atelier-navigator-section)
          (atelier-navigator-header-button "Prev" #'atelier-navigator-previous
                                           "Select the previous item")
          (atelier-navigator-header-button "Next" #'atelier-navigator-next
                                           "Select the next item")
          (atelier-navigator-header-button "Open" #'atelier-navigator-open
                                           "Open the selected item")
          (atelier-navigator-header-button "Attach" #'atelier-navigator-attach
                                           "Attach the selected buffer")
          (atelier-navigator-header-button "Detach" #'atelier-navigator-detach
                                           "Detach the selected buffer")
          (atelier-navigator-header-button "Close" #'atelier-navigator-close
                                           "Close the selected item")
          (atelier-navigator-header-button "Rename" #'atelier-navigator-rename
                                           "Rename the selected item")
          (atelier-navigator-header-button "Quit" #'atelier-navigator-quit
                                           "Close the navigator"))))

(defun atelier-navigator-click (event)
  (interactive "e")
  (let* ((start (event-start event))
         (window (posn-window start))
         (position (posn-point start)))
    (when (and (window-live-p window) (integer-or-marker-p position))
      (select-window window)
      (goto-char position)
      (atelier-navigator-open))))

(defun atelier-navigator-insert (text target &optional face)
  (let* ((map (make-sparse-keymap))
         (newline (string-suffix-p "\n" text))
         (label (copy-sequence (if newline (substring text 0 -1) text)))
         (properties (list 'atelier-navigator-target target
                           'mouse-face 'atelier-navigator-hover
                           'follow-link t 'keymap map 'rear-nonsticky t)))
    (define-key map [mouse-1] #'atelier-navigator-click)
    (define-key map [mouse-2] #'atelier-navigator-click)
    (when face (setq properties (append properties (list 'face face))))
    (add-text-properties 0 (length label) properties label)
    (insert label)
    (when newline (insert "\n"))))

(defun atelier-navigator-section (title &optional detail)
  (unless (= (point) (point-min)) (insert "\n"))
  (insert (propertize (format "  %s" (upcase title)) 'face 'atelier-navigator-section))
  (when detail
    (insert (propertize (format "  %s" detail) 'face 'atelier-navigator-branch)))
  (insert "\n\n"))

(defun atelier-navigator-workspace-label (workspace active)
  (let* ((name (plist-get workspace :name))
         (status (if active 'current (atelier-workspace-status workspace)))
         (icon (pcase status ('current "●") ('running "◉") (_ "○")))
         (name-face (cond (active 'atelier-navigator-active)
                          ((eq status 'running) 'atelier-navigator-live)
                          (t 'atelier-navigator-saved)))
         (status-face (pcase status
                        ('current 'atelier-navigator-current-status)
                        ('running 'atelier-navigator-running-status)
                        (_ 'atelier-navigator-saved))))
    (concat "  "
            (propertize icon 'face status-face)
            "  "
            (propertize (format "%s/" name) 'face name-face)
            "  "
            (propertize (format "(%s)" status) 'face status-face))))

(defun atelier-navigator-target ()
  (get-text-property (point) 'atelier-navigator-target))

(defun atelier-navigator-positions ()
  (let ((position (point-min)) positions)
    (while (< position (point-max))
      (if (get-text-property position 'atelier-navigator-target)
          (progn
            (push position positions)
            (setq position (or (next-single-property-change
                                position 'atelier-navigator-target nil (point-max))
                               (point-max))))
        (setq position (or (next-single-property-change
                            position 'atelier-navigator-target nil (point-max))
                           (point-max)))))
    (nreverse positions)))

(defun atelier-navigator-move (delta)
  (let* ((positions (atelier-navigator-positions))
         (next (cl-position-if (lambda (position) (> position (point))) positions))
         (current (max 0 (1- (or next (length positions)))))
         (target (and positions (nth (mod (+ current delta) (length positions)) positions))))
    (when target (goto-char target))))

(defun atelier-navigator-next (&optional count linewise)
  (interactive (list (prefix-numeric-value current-prefix-arg)
                     current-prefix-arg))
  (if linewise
      (forward-line (or count 1))
    (atelier-navigator-move 1)))

(defun atelier-navigator-previous (&optional count linewise)
  (interactive (list (prefix-numeric-value current-prefix-arg)
                     current-prefix-arg))
  (if linewise
      (forward-line (- (or count 1)))
    (atelier-navigator-move -1)))

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

(defun atelier-navigator-buffer-name (name)
  (let* ((buffer (get-buffer name))
         (title (and buffer
                     (local-variable-p 'ghostel-title buffer)
                     (buffer-local-value 'ghostel-title buffer))))
    (if (and (stringp title) (not (string-empty-p (string-trim title))))
        (string-trim (replace-regexp-in-string "[[:cntrl:]]+" " " title))
      name)))

(defun atelier-new-scratch-buffer (&optional workspace)
  (interactive)
  (let* ((workspace (or workspace (atelier-current-workspace)))
         (_ (unless (eq workspace (atelier-current-workspace))
              (atelier-switch-workspace (plist-get workspace :name))))
         (buffer (generate-new-buffer "*scratch*")))
    (with-current-buffer buffer
      (funcall initial-major-mode))
    (atelier-assign-buffer-to-workspace buffer workspace)
    (switch-to-buffer buffer)
    buffer))

(defun atelier-cleanup-candidate-entries ()
  (delete-dups
   (append (copy-sequence
            (atelier-workspace-entries (atelier-ensure-detached-workspace)))
           (cl-loop for workspace in (atelier-user-workspaces) append
                    (cl-remove-if-not
                     (lambda (entry) (eq (plist-get entry :kind) 'scratch))
                     (atelier-workspace-entries workspace))))))

(defun atelier-clear-scratch-and-detached-entries (&optional confirmed)
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
          (atelier-close-entry workspace entry)
          (setq killed (1+ killed))))
      (atelier-notify-change)
      (message "Cleared %d scratch or detached entr%s"
               killed (if (= killed 1) "" "s")))))

(defun atelier-clear-all-buffers (&optional confirmed)
  "Clear every user buffer and saved workspace buffer descriptor.
Modified file buffers are saved and running workspace jobs are stopped first."
  (interactive)
  (unless (or confirmed
              (yes-or-no-p
               "Clear all workspace, job, scratch, and detached buffers? "))
    (user-error "Cancelled"))
  (let ((atelier-inhibit-buffer-ownership t)
        (internal (append atelier-global-buffer-names
                          (list atelier-navigator-buffer atelier-choice-buffer)))
        (cleared 0))
    (dolist (workspace atelier-workspaces)
      (atelier-workspace-stop-jobs workspace t)
      (setf (plist-get workspace :entries) nil))
    (dolist (buffer (buffer-list))
      (let ((name (buffer-name buffer)))
        (when (and (buffer-live-p buffer)
                   (not (minibufferp buffer))
                   (not (string-prefix-p " " name))
                   (not (member name internal))
                   (not (string-prefix-p atelier-empty-buffer-prefix name)))
          (when (atelier-kill-buffer-without-save buffer)
            (setq cleared (1+ cleared))))))
    (atelier-notify-change)
    (message "Cleared %d buffer%s" cleared (if (= cleared 1) "" "s"))
    cleared))

(defun atelier-entry-id-less-p (left right)
  "Return non-nil when LEFT's permanent ID sorts before RIGHT's."
  (string-lessp (plist-get left :id) (plist-get right :id)))

(defun atelier-sort-entries-by-id (entries)
  "Return a copy of ENTRIES sorted by permanent entry ID."
  (sort (copy-sequence entries) #'atelier-entry-id-less-p))

(defun atelier-navigator-layout-label (entry)
  "Return a readable label for layout ENTRY's split direction."
  (pcase (plist-get entry :orientation)
    ('horizontal "Entry (side-by-side)")
    ('vertical "Entry (stacked)")
    (_ "Entry")))

(defun atelier-navigator-render-entry-tree
    (entry workspace-name displayed active prefix last-child)
  "Render ENTRY and its children for WORKSPACE-NAME.
DISPLAYED contains visible leaves in window traversal order.  PREFIX and
LAST-CHILD describe the current branch position in the rendered tree."
  (let* ((branch (if last-child "╰─" "├─"))
         (child-prefix (concat prefix (if last-child "   " "│  "))))
    (if (atelier-layout-entry-p entry)
        (progn
          (insert prefix
                  (propertize branch 'face 'atelier-navigator-branch)
                  " "
                  (propertize (atelier-navigator-layout-label entry)
                              'face 'atelier-navigator-branch)
                  "\n")
          (let ((children (atelier-entry-children entry)))
            (cl-loop for child in children
                     for tail on children
                     do (atelier-navigator-render-entry-tree
                         child workspace-name displayed active child-prefix
                         (null (cdr tail))))))
      (let* ((entry-id (plist-get entry :id))
             (index (cl-position entry-id displayed
                                 :key (lambda (candidate)
                                        (plist-get candidate :id))
                                 :test #'equal))
             (visible (integerp index))
             (live (atelier-entry-live-buffer entry))
             (name (or (and live (buffer-name live))
                       (plist-get entry :name) "Unavailable entry"))
             (selected (and visible active (plist-get entry :selected))))
        (atelier-navigator-insert
         (format "%s%s %s%s%s\n"
                 prefix
                 (propertize branch 'face 'atelier-navigator-branch)
                 (if selected "▸ " "")
                 (if visible (format "Split %d: " (1+ index)) "")
                 (atelier-navigator-buffer-name name))
         (if visible
             (list 'workspace-buffer workspace-name index entry-id)
           (list 'workspace-owned-buffer workspace-name entry-id))
         'atelier-navigator-buffer)))))

(defun atelier-render-navigator ()
  (let ((buffer (get-buffer-create atelier-navigator-buffer))
        workspace-roots first-item)
    (with-current-buffer buffer
      (unless (derived-mode-p 'atelier-navigator-mode)
        (atelier-navigator-mode))
      (let ((inhibit-read-only t))
        (erase-buffer)
        (atelier-navigator-section
         "Workspaces" (format "%d total" (length (atelier-user-workspaces))))
        (dolist (workspace (atelier-user-workspaces))
          (let* ((workspace-name (plist-get workspace :name))
                 (active (eq workspace (atelier-current-workspace))))
            (unless first-item (setq first-item (point)))
            (atelier-navigator-insert
             (atelier-navigator-workspace-label workspace active)
             (list 'workspace workspace-name))
            (insert "\n")
            (when-let* ((root (atelier-workspace-project-root workspace)))
              (push root workspace-roots))
             (let* ((displayed-root (atelier-workspace-displayed-entry workspace))
                    (displayed-entries (atelier-workspace-displayed-entries workspace))
                    (displayed-ids
                     (mapcar (lambda (entry) (plist-get entry :id)) displayed-entries))
                    (hidden
                     (atelier-sort-entries-by-id
                      (cl-remove-if
                       (lambda (entry)
                         (member (plist-get entry :id) displayed-ids))
                       (atelier-workspace-entries workspace)))))
               (when displayed-root
                 (atelier-navigator-render-entry-tree
                  displayed-root workspace-name displayed-entries active "     " nil))
               (dolist (entry hidden)
                 (atelier-navigator-render-entry-tree
                  entry workspace-name displayed-entries active "     " nil))
                (atelier-navigator-insert
                 "     ╰─ ＋ New scratch buffer\n"
                (list 'workspace-scratch workspace-name) 'success))
            (insert "\n")))
        (atelier-navigator-insert "  ＋ New workspace\n" '(new-workspace) 'success)
        (let ((projects
               (cl-remove-if
                (lambda (item) (member (myconfig-normalize-directory item) workspace-roots))
                (atelier-known-project-roots))))
          (when projects
            (atelier-navigator-section "Known projects" (format "%d available" (length projects)))
            (dolist (root projects)
              (unless first-item (setq first-item (point)))
              (atelier-navigator-insert
               (concat "  ◇  "
                       (propertize
                        (format "%s/" (file-name-nondirectory (directory-file-name root)))
                        'face 'font-lock-keyword-face)
                       (propertize (format "  %s" (abbreviate-file-name root))
                                   'face 'atelier-navigator-branch)
                       "\n")
               (list 'project root)))))
        (let* ((workspace (atelier-ensure-detached-workspace))
                (entries (atelier-sort-entries-by-id
                          (atelier-workspace-entries workspace))))
          (atelier-navigator-section "Detached buffers"
                                     (if entries (format "%d total" (length entries)) "none"))
          (if entries
              (dolist (entry entries)
                (let* ((buffer (atelier-entry-live-buffer entry))
                       (name (or (and buffer (buffer-name buffer))
                                 (plist-get entry :name) "Unavailable entry")))
                  (atelier-navigator-insert
                   (format "  •  %s\n" (atelier-navigator-buffer-name name))
                   (list 'workspace-owned-buffer atelier-detached-workspace-name
                         (plist-get entry :id))
                   'atelier-navigator-buffer)))
            (insert (propertize "  No detached buffers\n" 'face 'atelier-navigator-branch)))
          (atelier-navigator-insert "  ＋ New detached scratch buffer\n"
                                    (list 'workspace-scratch atelier-detached-workspace-name)
                                    'success))
        (atelier-navigator-section "Actions")
        (atelier-navigator-insert "  Clear scratch and detached buffers\n"
                                  '(clear-buffers) 'warning)
        (atelier-navigator-insert "  Clear all buffers\n"
                                  '(clear-all-buffers) 'error)
        (when (eq (char-before (point-max)) ?\n)
          (delete-region (1- (point-max)) (point-max)))
        (setq atelier-navigator-first-position
              (or first-item (car (atelier-navigator-positions)) (point-min)))
        (goto-char atelier-navigator-first-position)))
    buffer))

(defun atelier-navigator-quit ()
  (interactive)
  (setq atelier-navigator-attach-source nil)
  (let* ((frame (selected-frame))
         (configuration (alist-get frame atelier-navigator-window-configurations nil nil #'eq)))
    (setq atelier-navigator-window-configurations
          (assq-delete-all frame atelier-navigator-window-configurations))
    (when configuration
      (set-window-configuration configuration)
      (atelier-clean-window-buffer-history))))

(defun atelier-navigator ()
  (interactive)
  (let* ((frame (selected-frame))
         (existing (assq frame atelier-navigator-window-configurations))
         (window (cl-find-if (lambda (item) (not (window-parameter item 'window-side)))
                             (window-list frame 'no-minibuffer))))
    (unless existing
      (atelier-capture-current-workspace)
      (push (cons frame (current-window-configuration frame))
            atelier-navigator-window-configurations))
    (when window
      (delete-other-windows window)
      (select-window window)
      (switch-to-buffer (atelier-render-navigator))
      (goto-char atelier-navigator-first-position)
      (set-window-point window atelier-navigator-first-position))))

(defun atelier-focus-workspace-split (workspace-name index)
  (unless (eq (atelier-workspace-get workspace-name) (atelier-current-workspace))
    (atelier-switch-workspace workspace-name))
  (let* ((windows (cl-remove-if (lambda (window) (window-parameter window 'window-side))
                                (window-list nil 'no-minibuffer)))
         (window (nth index windows)))
    (unless (window-live-p window)
      (user-error "Split %d no longer exists" (1+ index)))
    (select-window window)
    window))

(defun atelier-workspace-buffer (workspace-name index entry-id)
  (when-let* ((workspace (atelier-workspace-get workspace-name))
              (entry (nth index (atelier-workspace-displayed-entries workspace))))
    (unless (equal entry-id (plist-get entry :id))
      (user-error "Split assignment changed"))
    (atelier-restore-buffer entry workspace)))

(defun atelier-workspace-owned-buffer (workspace entry-id)
  (when-let* ((entry (atelier-entry-by-id workspace entry-id)))
    (atelier-restore-buffer entry workspace)))

(defun atelier-navigator-assign-buffer (buffer)
  (unless (buffer-live-p buffer) (user-error "Buffer no longer exists"))
  (atelier-navigator-quit)
  (atelier-assign-buffer-to-workspace buffer)
  (switch-to-buffer buffer)
  (when (fboundp 'myconfig-terminal-activate)
    (myconfig-terminal-activate buffer))
  (atelier-notify-change))

(defun atelier-navigator-target-buffer (target)
  (pcase target
    (`(workspace-buffer ,workspace-name ,index ,entry-id)
     (atelier-workspace-buffer workspace-name index entry-id))
    (`(workspace-owned-buffer ,workspace-name ,entry-id)
     (let ((workspace (atelier-workspace-get workspace-name)))
       (or (atelier-entry-live-buffer (atelier-entry-by-id workspace entry-id))
           (atelier-workspace-owned-buffer workspace entry-id))))
    (`(buffer ,name) (get-buffer name))))

(defun atelier-navigator-target-workspace (target buffer)
  (pcase target
    (`(workspace ,name) (atelier-workspace-get name))
    (`(workspace-buffer ,name . ,_) (atelier-workspace-get name))
    (`(workspace-owned-buffer ,name . ,_) (atelier-workspace-get name))
    (_ (or (and buffer
                (car (car (atelier-entries-for-buffer buffer))))
           (atelier-current-workspace)))))

(defun atelier-navigator-attach ()
  (interactive)
  (let ((target (atelier-navigator-target)))
    (unless (memq (car-safe target)
                  '(buffer workspace-buffer workspace-owned-buffer))
      (user-error "Select a buffer to attach"))
    (setq atelier-navigator-attach-source target)
    (force-mode-line-update t)
    (message "Select a workspace or another buffer with Enter")))

(defun atelier-navigator-finish-attach (target)
  (let* ((source-target atelier-navigator-attach-source)
         (source (atelier-navigator-target-buffer source-target))
         (target-buffer (atelier-navigator-target-buffer target))
         (workspace (atelier-navigator-target-workspace target target-buffer)))
    (unless (or (eq (car-safe target) 'workspace) target-buffer)
      (user-error "Select a workspace or buffer"))
    (unless (buffer-live-p source) (user-error "Source buffer no longer exists"))
    (when (eq source target-buffer) (user-error "Choose another buffer"))
    (unless workspace (user-error "No target workspace"))
    (setq atelier-navigator-attach-source nil)
    (atelier-navigator-quit)
    (unless (eq workspace (atelier-current-workspace))
      (atelier-switch-workspace (plist-get workspace :name)))
    (let* ((target-window (and target-buffer (get-buffer-window target-buffer)))
           (source-window (get-buffer-window source))
           (left (or target-window (selected-window))))
      (when (and (eq (car-safe target) 'workspace) (eq source-window left))
        (if-let* ((other (cl-find-if (lambda (window) (not (eq window source-window)))
                                     (window-list nil 'no-minibuffer))))
            (setq left other)
          (let ((buffer (atelier-new-dired-buffer
                         (atelier-workspace-directory workspace) t workspace)))
            (atelier-register-dired-buffer buffer workspace t)
            (set-window-buffer left buffer))))
      (when (and source-window (not (eq source-window left))
                 (not (one-window-p)))
        (delete-window source-window))
      (when target-buffer (set-window-buffer left target-buffer))
      (atelier-assign-buffer-to-workspace (window-buffer left) workspace)
      (when-let* ((pair (car (atelier-entries-for-buffer source)))
                  (old-workspace (car pair))
                  (entry (nth 1 pair))
                  ((not (eq old-workspace workspace))))
        (atelier-entry-move entry old-workspace workspace))
      (atelier-register-buffer source workspace)
      (let ((right (split-window left nil 'right)))
        (set-window-buffer right source)
        (select-window right)))
    (atelier-capture-current-workspace)
    (atelier-notify-change)
    (atelier-navigator)))

(defun atelier-navigator-detach ()
  (interactive)
  (let ((detached-workspace (atelier-ensure-detached-workspace)))
    (pcase (atelier-navigator-target)
      (`(workspace-buffer ,workspace-name ,index ,entry-id)
       (when (equal workspace-name atelier-detached-workspace-name)
         (user-error "Entry is already detached"))
       (setq atelier-navigator-attach-source nil)
       (atelier-navigator-quit)
       (unless (eq (atelier-workspace-get workspace-name) (atelier-current-workspace))
         (atelier-switch-workspace workspace-name))
       (let* ((workspace (atelier-workspace-get workspace-name))
              (entry (atelier-entry-by-id workspace entry-id))
              (window (atelier-focus-workspace-split workspace-name index))
              (buffer (and entry (atelier-entry-live-buffer entry))))
         (unless (buffer-live-p buffer) (user-error "Entry buffer no longer exists"))
          (atelier-entry-move entry workspace detached-workspace)
          (atelier-close-entry-window workspace window))
       (atelier-capture-current-workspace)
       (atelier-notify-change)
       (atelier-navigator))
      (`(workspace-owned-buffer ,workspace-name ,entry-id)
       (when (equal workspace-name atelier-detached-workspace-name)
         (user-error "Entry is already detached"))
       (let* ((workspace (atelier-workspace-get workspace-name))
              (entry (atelier-entry-by-id workspace entry-id)))
         (unless entry (user-error "Entry no longer exists"))
         (atelier-entry-move entry workspace detached-workspace)
         (atelier-notify-change)
         (atelier-render-navigator)))
      (_ (user-error "Select a split or workspace-owned buffer")))))

(defun atelier-navigator-open ()
  (interactive)
  (let ((target (atelier-navigator-target)))
    (if atelier-navigator-attach-source
        (atelier-navigator-finish-attach target)
      (pcase target
        ('nil (user-error "No item on this line"))
        (`(new-workspace) (atelier-navigator-quit) (atelier-create-workspace))
        (`(workspace-scratch ,workspace-name)
         (let ((workspace (atelier-workspace-get workspace-name)))
           (unless workspace (user-error "Workspace no longer exists: %s" workspace-name))
           (atelier-navigator-quit)
           (unless (eq workspace (atelier-current-workspace))
             (atelier-switch-workspace workspace-name))
           (atelier-new-scratch-buffer workspace)))
        (`(clear-buffers)
         (atelier-clear-scratch-and-detached-entries)
         (atelier-navigator-quit)
         (atelier-capture-current-workspace)
         (atelier-navigator))
        (`(clear-all-buffers)
         (atelier-clear-all-buffers)
         (atelier-navigator-quit)
         (when-let* ((workspace (atelier-current-workspace)))
           (delete-other-windows)
           (switch-to-buffer (atelier-empty-workspace-buffer workspace)))
         (atelier-navigator))
        (`(workspace ,name) (atelier-navigator-quit) (atelier-switch-workspace name))
        (`(split ,workspace-name ,index)
         (atelier-navigator-quit)
         (atelier-focus-workspace-split workspace-name index))
        (`(workspace-buffer ,workspace-name ,index ,entry-id)
         (atelier-navigator-quit)
         (let ((buffer (atelier-workspace-buffer workspace-name index entry-id))
               (window (atelier-focus-workspace-split workspace-name index)))
           (unless (buffer-live-p buffer) (user-error "Entry buffer could not be restored"))
           (set-window-buffer window buffer)
           (when (fboundp 'myconfig-terminal-activate)
             (myconfig-terminal-activate buffer))))
        (`(workspace-owned-buffer ,workspace-name ,entry-id)
         (atelier-navigator-quit)
         (unless (eq (atelier-workspace-get workspace-name) (atelier-current-workspace))
           (atelier-switch-workspace workspace-name))
         (let* ((workspace (atelier-workspace-get workspace-name))
                (buffer (or (atelier-entry-live-buffer
                             (atelier-entry-by-id workspace entry-id))
                            (atelier-workspace-owned-buffer workspace entry-id))))
           (unless (buffer-live-p buffer)
             (user-error "Workspace entry could not be restored"))
           (switch-to-buffer buffer)
           (when (fboundp 'myconfig-terminal-activate)
             (myconfig-terminal-activate buffer))
           (atelier-notify-change)))
        (`(project ,root) (atelier-navigator-quit) (atelier-open-project-workspace root))
        (`(buffer ,name)
         (if-let* ((buffer (get-buffer name)))
             (atelier-navigator-assign-buffer buffer)
           (user-error "Buffer no longer exists: %s" name)))))))

(defun atelier-close-current-view ()
  (interactive)
  (let ((buffer (current-buffer)))
    (let ((atelier-inhibit-buffer-ownership t))
      (atelier-close-buffer (buffer-name buffer) (selected-window)))))

(defun atelier-kill-buffer-without-save (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (when (and buffer-file-name (buffer-modified-p))
        (set-buffer-modified-p nil)))
    (let ((kill-buffer-query-functions nil))
      (kill-buffer buffer))))

(defun atelier-workspace-replacement-entries (workspace)
  "Return WORKSPACE entries with recently used live entries first."
  (let* ((entries (atelier-workspace-entries workspace))
         (recent
          (cl-loop for buffer in (buffer-list)
                   for entry = (cl-find-if
                                (lambda (candidate)
                                  (eq (atelier-entry-live-buffer candidate) buffer))
                                entries)
                   when entry collect entry)))
    (append recent (cl-set-difference entries recent :test #'eq))))

(defun atelier-workspace-replacement-buffer (workspace)
  "Return or restore the best replacement buffer in WORKSPACE."
  (cl-loop for entry in (atelier-workspace-replacement-entries workspace)
           thereis (or (atelier-entry-live-buffer entry)
                       (atelier-restore-buffer entry workspace))))

(defun atelier-main-windows (&optional frame)
  "Return FRAME's ordinary windows, excluding side windows."
  (cl-remove-if (lambda (window) (window-parameter window 'window-side))
                (window-list (or frame (selected-frame)) 'no-minibuffer)))

(defun atelier-close-entry-window (workspace window)
  "Remove WINDOW or show another WORKSPACE entry when it is the last window."
  (when (window-live-p window)
    (if (> (length (atelier-main-windows (window-frame window))) 1)
        (delete-window window)
      (set-window-buffer
       window
       (or (atelier-workspace-replacement-buffer workspace)
           (atelier-empty-workspace-buffer workspace)))
      (set-window-prev-buffers window nil)
      (set-window-next-buffers window nil))))

(defun atelier-close-entry (workspace entry &optional window)
  "Close ENTRY and remove it from WORKSPACE using one entry lifecycle."
  (let ((buffer (atelier-entry-live-buffer entry)))
    (when (buffer-live-p buffer)
      (when-let* ((process (get-buffer-process buffer)))
        (set-process-query-on-exit-flag process nil)
        (when (process-live-p process)
          (let ((atelier-preserve-job-recipe nil)) (delete-process process))))
      (atelier-entry-remove workspace entry t)
      ;; A file buffer may be represented by a separate entry in another
      ;; workspace.  Do not kill the shared live object in that case.
      (unless (atelier-entries-for-buffer buffer)
        (atelier-kill-buffer-without-save buffer)))
    (unless (buffer-live-p buffer)
      (atelier-entry-remove workspace entry t))
    (when window
      (atelier-close-entry-window workspace window)
      (when (eq workspace (atelier-current-workspace))
        (atelier-capture-current-workspace)))
    (atelier-notify-change)))

(defun atelier-close-buffer (name &optional window)
  (let ((window (or window (get-buffer-window name (selected-frame)))))
    (if-let* ((buffer (get-buffer name))
              (workspace (atelier-current-workspace))
              (entry (or (atelier-workspace-entry-for-buffer workspace buffer)
                         (atelier-register-buffer buffer workspace t))))
        (atelier-close-entry workspace entry window)
      (when-let* ((buffer (get-buffer name)))
        (when-let* ((process (get-buffer-process buffer)))
          (set-process-query-on-exit-flag process nil)
          (when (process-live-p process)
            (delete-process process)))
        (atelier-kill-buffer-without-save buffer)))))

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

(defun atelier-navigator-close ()
  (interactive)
  (let ((target (atelier-navigator-target)))
    (unless target (user-error "No item on this line"))
    (unless (memq (car target) '(workspace buffer workspace-buffer workspace-owned-buffer project))
      (user-error "This item cannot be closed"))
    (unless (y-or-n-p (format "%s? "
                              (pcase (car target)
                                ('workspace "Close and remove this workspace")
                                ('buffer "Kill this buffer")
                                ('workspace-buffer "Kill this buffer")
                                ('workspace-owned-buffer "Kill this buffer")
                                ('project "Forget this project")
                                (_ "This item cannot be closed"))))
      (user-error "Cancelled"))
    (atelier-navigator-quit)
    (pcase target
      (`(workspace ,name)
       (if-let* ((workspace (atelier-workspace-get name)))
           (atelier-delete-workspace-record workspace)
         (user-error "Workspace no longer exists: %s" name)))
      (`(buffer ,name)
       (atelier-close-buffer name))
      (`(workspace-buffer ,workspace-name ,index ,entry-id)
       (let* ((workspace
               (or (atelier-workspace-get workspace-name)
                   (user-error "Workspace no longer exists: %s" workspace-name)))
              (entry
               (or (atelier-entry-by-id workspace entry-id)
                   (user-error "Workspace entry no longer exists")))
              (window (atelier-workspace-entry-window workspace index entry)))
         (atelier-close-entry workspace entry window)))
      (`(workspace-owned-buffer ,workspace-name ,entry-id)
       (atelier-remove-saved-workspace-buffer
        (or (atelier-workspace-get workspace-name)
            (user-error "Workspace no longer exists: %s" workspace-name))
        entry-id))
      (`(project ,root) (project-forget-project root))
      (_ (user-error "This item cannot be closed")))
    (atelier-navigator)))

(defun atelier-navigator-rename ()
  (interactive)
  (let ((target (atelier-navigator-target)))
    (unless target (user-error "No item on this line"))
    (atelier-navigator-quit)
    (pcase target
      (`(workspace ,name)
       (atelier-switch-workspace name)
       (atelier-rename-workspace))
      (`(buffer ,name)
       (if-let* ((buffer (get-buffer name)))
           (with-current-buffer buffer
             (rename-buffer (read-string "New buffer name: " name) t))
         (user-error "Buffer no longer exists: %s" name)))
      (`(workspace-buffer ,workspace-name ,_ ,entry-id)
       (if-let* ((workspace (atelier-workspace-get workspace-name))
                 (entry (atelier-entry-by-id workspace entry-id))
                 (buffer (atelier-entry-live-buffer entry)))
           (with-current-buffer buffer
             (setf (plist-get entry :name)
                   (rename-buffer (read-string "New buffer name: " (buffer-name)) t)))
         (user-error "Entry buffer no longer exists")))
      (`(workspace-owned-buffer ,workspace-name ,entry-id)
       (if-let* ((workspace (atelier-workspace-get workspace-name))
                 (entry (atelier-entry-by-id workspace entry-id))
                 (buffer (atelier-entry-live-buffer entry)))
           (with-current-buffer buffer
             (setf (plist-get entry :name)
                   (rename-buffer (read-string "New buffer name: " (buffer-name)) t)))
         (user-error "Entry buffer no longer exists")))
      (_ (user-error "This item cannot be renamed")))
    (atelier-notify-change)
    (atelier-navigator)))

(provide 'atelier-navigator)
;;; atelier-navigator.el ends here

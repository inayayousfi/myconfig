;;; atelier-navigator.el --- Atelier navigator UI -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'dired)
(require 'subr-x)
(require 'atelier-model)

(defvar-keymap atelier-navigator-mode-map
  :parent special-mode-map
  "j" #'atelier-navigator-next
  "k" #'atelier-navigator-previous
  "h" #'atelier-navigator-stack-previous
  "l" #'atelier-navigator-stack-next
  "<down>" #'atelier-navigator-next
  "<up>" #'atelier-navigator-previous
  "RET" #'atelier-navigator-open
  "o" #'atelier-navigator-toggle-fold
  "s" #'atelier-navigator-stop-workspace
  "a" #'atelier-navigator-attach
  "d" #'atelier-navigator-detach
  "f" #'isearch-forward
  "F" #'isearch-forward
  "x" #'atelier-navigator-close
  "X" #'atelier-navigator-close-entry
  "r" #'atelier-navigator-rename
  "R" #'atelier-navigator-rename
  "q" #'atelier-navigator-quit)

(defvar-local atelier-navigator-changed-views nil
  "Entries whose restored windows need their newly active content.")
(defvar-local atelier-navigator-workspace-expansions nil
  "Workspace ID to (status . expanded), local to this frame's navigator.")
(defvar-local atelier-navigator-stopped-expanded nil
  "Whether this navigator shows the stopped workspace headings.")

(defun atelier-navigator-workspace-expanded-p (workspace)
  "Return navigator-only expansion, resetting on a lifecycle transition."
  (let* ((id (atelier-workspace-id workspace))
         (status (atelier-workspace-status workspace))
         (state (alist-get id atelier-navigator-workspace-expansions nil nil #'equal)))
    (unless (eq (car state) status)
      (setq state (cons status (eq status 'running)))
      (setf (alist-get id atelier-navigator-workspace-expansions nil nil #'equal) state))
    (cdr state)))

(defun atelier-navigator-toggle-fold ()
  "Toggle the stopped group or the workspace under point without opening it."
  (interactive)
  (atelier-navigator-commit-stack-selection)
  (let ((target (atelier-navigator-target)))
    (pcase target
      (`(stopped-workspaces)
       (setq atelier-navigator-stopped-expanded
             (not atelier-navigator-stopped-expanded)))
      (_
       (let ((workspace (and (memq (car-safe target)
                                  '(workspace workspace-buffer workspace-owned-buffer
                                    workspace-content workspace-owned-content workspace-scratch))
                             (atelier-navigator-target-workspace target))))
         (unless (and workspace (not (atelier-detached-workspace-p workspace)))
           (user-error "Select a workspace or the stopped workspaces group"))
         (let ((expanded (atelier-navigator-workspace-expanded-p workspace)))
           (setf (alist-get (atelier-workspace-id workspace)
                           atelier-navigator-workspace-expansions nil nil #'equal)
                 (cons (atelier-workspace-status workspace) (not expanded))))
         (setq target (list 'workspace (plist-get workspace :name))))))
    (setf (alist-get (selected-frame) atelier-navigator-selection-by-frame nil nil #'eq)
          target)
    (atelier-render-navigator)))

(defun atelier-navigator-stop-workspace ()
  "Stop the workspace under point without removing its saved record."
  (interactive)
  (let* ((target (atelier-navigator-target))
         (workspace (and (memq (car-safe target)
                               '(workspace workspace-buffer workspace-owned-buffer
                                 workspace-content workspace-owned-content workspace-scratch))
                         (atelier-navigator-target-workspace target))))
    (unless (and workspace (not (atelier-detached-workspace-p workspace)))
      (user-error "Select a workspace to stop"))
    (atelier-navigator-quit)
    (atelier-stop-workspace workspace)
    (atelier-navigator)))
(defvar-local atelier-navigator-stack-timer nil
  "Idle timer for activating the selected stack content.")
(defvar-local atelier-navigator-stack-pending-target nil
  "Content target chosen by h/l, retained until activation.")
(defcustom atelier-navigator-stack-delay 0.45
  "Seconds of idle time before h/l activates the selected content."
  :type 'number :group 'atelier)

(defun atelier-navigator-frame-buffer (&optional frame create)
  "Return FRAME's own navigator buffer, creating it when CREATE is non-nil."
  (let* ((frame (or frame (selected-frame)))
         (buffer (frame-parameter frame 'atelier-navigator-buffer)))
    (if (buffer-live-p buffer) buffer
      (when create
        (setq buffer (get-buffer-create
                      (generate-new-buffer-name atelier-navigator-buffer)))
        (set-frame-parameter frame 'atelier-navigator-buffer buffer)
        buffer))))

(defun atelier-navigator-frame-closed (frame)
  "Dispose of FRAME's private navigator and pending choice."
  (when-let* ((buffer (atelier-navigator-frame-buffer frame)))
    (with-current-buffer buffer
      (when atelier-navigator-stack-timer
        (cancel-timer atelier-navigator-stack-timer)))
    (kill-buffer buffer))
  (setq atelier-navigator-selection-by-frame
        (assq-delete-all frame atelier-navigator-selection-by-frame)))

(define-derived-mode atelier-navigator-mode special-mode "Atelier"
  (atelier-mark-internal-buffer)
  (setq-local header-line-format
              '(:eval (atelier-navigator-header)))
  (setq-local cursor-type 'box
              truncate-lines t
              line-spacing 0.12
              display-line-numbers-type 'relative)
  (hl-line-mode -1)
  (display-line-numbers-mode 1))

(defun atelier-navigator-header-button (label command help)
  (let ((button (atelier-clickable-label
                 (format "[%s]" label) command nil 'font-lock-keyword-face help)))
    (remove-text-properties 0 (length button) '(mouse-face nil) button)
    (concat " " button)))

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
          (atelier-navigator-header-button "Stack h/l" #'atelier-navigator-stack-next
                                            "Select another content on this row")
          (atelier-navigator-header-button "Fold o" #'atelier-navigator-toggle-fold
                                           "Fold or expand without opening")
          (atelier-navigator-header-button "Stop s" #'atelier-navigator-stop-workspace
                                           "Stop the selected workspace's processes")
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
      (atelier-navigator-commit-stack-selection)
      (goto-char position)
      (atelier-navigator-open))))

(defun atelier-navigator-insert (text target &optional face)
  (let* ((map (make-sparse-keymap))
         (newline (string-suffix-p "\n" text))
         (label (copy-sequence (if newline (substring text 0 -1) text)))
         (properties (list 'atelier-navigator-target target
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
    (concat (if (atelier-navigator-workspace-expanded-p workspace) "  ▾ " "  ▸ ")
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
      (if (and (get-text-property position 'atelier-navigator-target)
               (not (get-text-property position 'atelier-navigator-stack-item)))
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

(defun atelier-navigator-stack-move (delta)
  "Select another content on the same entry row, wrapping at either end."
  (let* ((begin (line-beginning-position))
         (start (cl-find-if (lambda (position)
                              (<= begin position (line-end-position)))
                            (atelier-navigator-positions)))
         (end (line-end-position))
         positions)
    (when start
      (push start positions)
      (let ((position start))
        (while (and (setq position (next-single-property-change
                                    position 'atelier-navigator-target nil end))
                    (< position end))
          (when (get-text-property position 'atelier-navigator-stack-item)
            (push position positions))))
      (setq positions (nreverse positions))
      (when (> (length positions) 1)
        (let* ((current (or (cl-position-if (lambda (position) (>= position (point)))
                                             positions)
                            0))
               (current (if (= (nth current positions) (point)) current
                          (max 0 (1- current)))))
          (goto-char (nth (mod (+ current delta) (length positions)) positions))
          (atelier-navigator-schedule-stack-activation))))))

(defun atelier-navigator-commit-stack-selection ()
  "Activate a pending stack choice, if any."
  (when atelier-navigator-stack-timer
    (cancel-timer atelier-navigator-stack-timer)
    (setq atelier-navigator-stack-timer nil)
    (let ((target atelier-navigator-stack-pending-target))
      (setq atelier-navigator-stack-pending-target nil)
      (atelier-navigator-activate-selected-content target))))

(defun atelier-navigator-schedule-stack-activation ()
  "Wait for a pause in h/l navigation before reordering the stack."
  (when atelier-navigator-stack-timer
    (cancel-timer atelier-navigator-stack-timer))
  (setq atelier-navigator-stack-pending-target (atelier-navigator-target))
  (let ((buffer (current-buffer))
        (frame (selected-frame)))
    (setq atelier-navigator-stack-timer
          (run-with-idle-timer
           atelier-navigator-stack-delay nil
           (lambda ()
             (when (and (frame-live-p frame) (buffer-live-p buffer))
               (with-selected-frame frame
                 (with-current-buffer buffer
                   (atelier-operation-live-event #'atelier-navigator-commit-stack-selection)))))))))

(atelier-define-operation atelier-navigator-activate-selected-content (&optional target)
    (let* ((choice (or target (atelier-navigator-target)))
           (workspace (atelier-workspace-get (nth 1 choice))))
      (list (atelier-workspace-id (or workspace (user-error "Workspace no longer exists")))))
    ((target (or target (atelier-navigator-target))))
  "Make TARGET (or the selected horizontal content) top of its entry's stack."
  (pcase (or target (atelier-navigator-target))
    ((or `(workspace-content ,workspace-name ,_ ,entry-id ,content-id)
         `(workspace-owned-content ,workspace-name ,entry-id ,content-id))
     (let* ((workspace (atelier-workspace-get workspace-name))
            (entry (and workspace (atelier-entry-by-id workspace entry-id))))
       (unless entry
         (user-error "Content no longer belongs to this entry"))
       (let ((buffer (atelier-restore-entry-content entry workspace content-id)))
         (unless (buffer-live-p buffer)
           (user-error "Content could not be restored")))
       (cl-pushnew (cons (atelier-workspace-id workspace) (plist-get entry :id))
                   atelier-navigator-changed-views
                   :test (lambda (left right) (eq (cdr left) (cdr right))))
       (setf (alist-get (selected-frame) atelier-navigator-selection-by-frame
                        nil nil #'eq)
             (if-let* ((index (cl-position entry
                                            (atelier-workspace-displayed-entries workspace))))
                 (list 'workspace-buffer workspace-name index entry-id)
               (list 'workspace-owned-buffer workspace-name entry-id)))
       (atelier-render-navigator)
       (atelier-notify-change)))))

(defun atelier-navigator-stack-previous ()
  (interactive)
  (atelier-navigator-stack-move -1))

(defun atelier-navigator-stack-next ()
  (interactive)
  (atelier-navigator-stack-move 1))

(defun atelier-navigator-next (&optional count linewise)
  (interactive (list (prefix-numeric-value current-prefix-arg)
                     current-prefix-arg))
  (atelier-navigator-commit-stack-selection)
  (if linewise
      (forward-line (or count 1))
    (atelier-navigator-move 1)))

(defun atelier-navigator-previous (&optional count linewise)
  (interactive (list (prefix-numeric-value current-prefix-arg)
                     current-prefix-arg))
  (atelier-navigator-commit-stack-selection)
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
  "Show Emacs duplicate suffixes as readable qualifiers, without renaming buffers."
  (let* ((buffer (get-buffer name))
         (title (and buffer
                     (run-hook-with-args-until-success
                      'atelier-buffer-title-functions buffer))))
    (if (and (stringp title) (not (string-empty-p (string-trim title))))
        (string-trim (replace-regexp-in-string "[[:cntrl:]]+" " " title))
      (if (string-match "\\`\\(.*\\)<\\([^<>]+\\)>\\'" name)
          (format "%s (%s)" (match-string 1 name) (match-string 2 name))
        name))))

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
    (switch-to-buffer buffer)
    buffer))

(defun atelier-cleanup-candidate-entries ()
  (delete-dups
   (append (copy-sequence
            (atelier-workspace-entries (atelier-ensure-detached-workspace)))
           (cl-loop for workspace in (atelier-user-workspaces) append
                    (cl-remove-if-not
                     (lambda (entry) (eq (atelier-entry-value entry :kind) 'scratch))
                     (atelier-workspace-entries workspace))))))

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
          (atelier-close-entry workspace entry nil t)
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
         (internal (append atelier-global-buffer-names
                           (list atelier-navigator-buffer atelier-choice-buffer)))
         (cleared 0)
         (buffers
          (cl-remove-if-not
           (lambda (buffer)
             (let ((name (buffer-name buffer)))
               (and (buffer-live-p buffer)
                    (not (minibufferp buffer))
                    (not (string-prefix-p " " name))
                    (not (member name internal))
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

(defun atelier-entry-id-less-p (left right)
  "Return non-nil when LEFT's permanent ID sorts before RIGHT's."
  (string-lessp (plist-get left :id) (plist-get right :id)))

(defun atelier-sort-entries-by-id (entries)
  "Return a copy of ENTRIES sorted by permanent entry ID."
  (sort (copy-sequence entries) #'atelier-entry-id-less-p))

(defun atelier-navigator-layout-label (entry)
  "Return a readable label for layout ENTRY's split direction."
  (pcase (plist-get entry :orientation)
    ('horizontal "Split (side-by-side)")
    ('vertical "Split (stacked)")
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
                       (atelier-entry-value entry :name) "Unavailable entry"))
             (selected (and visible active (plist-get entry :selected))))
        (atelier-navigator-insert
         (format "%s%s %s%s%s"
                 prefix
                 (propertize branch 'face 'atelier-navigator-branch)
                 (if selected "▸ " "")
                 (if visible (format "View %d: " (1+ index)) "")
                 (if (cdr (atelier-entry-stack entry))
                     (format "[%s]" (atelier-navigator-buffer-name name))
                   (atelier-navigator-buffer-name name)))
         (if visible
             (list 'workspace-buffer workspace-name index entry-id)
           (list 'workspace-owned-buffer workspace-name entry-id))
         'atelier-navigator-buffer)
        (dolist (content (cdr (atelier-entry-stack entry)))
          (let ((start (point)))
            (atelier-navigator-insert
             (format "  [%s]"
                     (atelier-navigator-buffer-name
                      (or (plist-get content :name) "Unavailable content")))
             (if visible
                 (list 'workspace-content workspace-name index entry-id
                       (plist-get content :id))
               (list 'workspace-owned-content workspace-name entry-id
                     (plist-get content :id)))
             'atelier-navigator-saved)
            (put-text-property start (point) 'atelier-navigator-stack-item t)))
        (insert "\n")))))

(defun atelier-navigator-render-workspace (workspace)
  "Render WORKSPACE's heading and, when expanded, its saved contents."
  (let* ((workspace-name (plist-get workspace :name))
         (active (eq workspace (atelier-current-workspace))))
    (atelier-navigator-insert
     (atelier-navigator-workspace-label workspace active)
     (list 'workspace workspace-name))
    (insert "\n")
    (when (atelier-navigator-workspace-expanded-p workspace)
      (let* ((displayed-root (atelier-workspace-displayed-entry workspace))
             (displayed-entries (atelier-workspace-displayed-entries workspace))
             (displayed-ids (mapcar (lambda (entry) (plist-get entry :id)) displayed-entries))
             (hidden (cl-remove-if
                      (lambda (entry) (member (plist-get entry :id) displayed-ids))
                      (atelier-workspace-entries workspace))))
        (when displayed-root
          (atelier-navigator-render-entry-tree
           displayed-root workspace-name displayed-entries active "     " nil))
        (cl-loop for entry in hidden for tail on hidden
                 do (atelier-navigator-render-entry-tree
                     entry workspace-name displayed-entries active "     " (null (cdr tail))))
        (atelier-navigator-insert
         "     ╰─ ＋ New scratch buffer\n"
         (list 'workspace-scratch workspace-name) 'success)))
    (insert "\n")))

(defun atelier-render-navigator ()
  (let ((buffer (atelier-navigator-frame-buffer nil t))
        workspace-roots first-item)
    (with-current-buffer buffer
      (unless (derived-mode-p 'atelier-navigator-mode)
        (atelier-navigator-mode))
      (let ((inhibit-read-only t))
        (erase-buffer)
        (atelier-navigator-section
         "Workspaces" (format "%d total" (length (atelier-user-workspaces))))
        (dolist (workspace (atelier-user-workspaces))
          (when-let* ((root (atelier-workspace-project-root workspace)))
            (push root workspace-roots))
          (when (eq (atelier-workspace-status workspace) 'running)
            (unless first-item (setq first-item (point)))
            (atelier-navigator-render-workspace workspace)))
        (when-let* ((stopped (cl-remove-if
                             (lambda (workspace)
                               (eq (atelier-workspace-status workspace) 'running))
                             (atelier-user-workspaces))))
          (unless first-item (setq first-item (point)))
          (atelier-navigator-insert
           (format "  %s Stopped workspaces (%d)\n\n"
                   (if atelier-navigator-stopped-expanded "▾" "▸") (length stopped))
           '(stopped-workspaces) 'atelier-navigator-saved)
          (when atelier-navigator-stopped-expanded
            (dolist (workspace stopped)
              (atelier-navigator-render-workspace workspace))))
        (atelier-navigator-insert "  ＋ New workspace\n" '(new-workspace) 'success)
        (let ((projects
               (cl-remove-if
                (lambda (item) (member (atelier-normalize-directory item) workspace-roots))
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
                                 (atelier-entry-value entry :name) "Unavailable entry")))
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
        (let* ((frame (selected-frame))
               (wanted (alist-get frame atelier-navigator-selection-by-frame
                                  nil nil #'eq))
               (position
                (or (and wanted
                         (cl-find-if
                          (lambda (candidate)
                            (equal wanted
                                   (get-text-property
                                    candidate 'atelier-navigator-target)))
                          (atelier-navigator-positions)))
                    atelier-navigator-first-position)))
          (goto-char position))))
    buffer))

(defun atelier-navigator-quit ()
  (interactive)
  (when-let* ((navigator (atelier-navigator-frame-buffer)))
    (with-current-buffer navigator (atelier-navigator-commit-stack-selection)))
  (setq atelier-navigator-attach-source nil)
  (let* ((frame (selected-frame))
         (navigator (atelier-navigator-frame-buffer))
         (target (and navigator
                      (with-current-buffer navigator
                        (when (derived-mode-p 'atelier-navigator-mode)
                          (atelier-navigator-target)))))
         (configuration (alist-get frame atelier-navigator-window-configurations nil nil #'eq))
         (changed (and navigator (buffer-local-value 'atelier-navigator-changed-views
                                                    navigator))))
    (when navigator
      (with-current-buffer navigator (setq atelier-navigator-changed-views nil)))
    (when target
      (setf (alist-get frame atelier-navigator-selection-by-frame nil nil #'eq)
            target))
    (setq atelier-navigator-window-configurations
          (assq-delete-all frame atelier-navigator-window-configurations))
    (when configuration
      (set-window-configuration configuration)
      (dolist (pair changed)
         (let* ((workspace (atelier-workspace-by-id (car pair)))
                (entry (and workspace (atelier-entry-by-id workspace (cdr pair)))))
          (when (eq workspace (atelier-current-workspace))
            (when-let* ((index (cl-position entry
                                            (atelier-workspace-displayed-entries workspace)))
                        (window (nth index (atelier-main-windows)))
                        (buffer (atelier-entry-live-buffer entry)))
              (set-window-buffer window buffer)))))
      (atelier-clean-window-buffer-history))
    (when changed (atelier-notify-change))))

(defun atelier-navigator ()
  (interactive)
  ;; Side windows (including AIPanel) are not Atelier views.  Save and
  ;; restore navigation from the main view, never from the side panel.
  (when (window-parameter nil 'window-side)
    (select-window (atelier-main-window)))
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
      (set-window-point window (with-current-buffer (window-buffer window) (point))))))

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

(atelier-define-operation atelier-navigator-assign-buffer (buffer)
    (list (atelier-current-workspace-id)) nil
  (unless (buffer-live-p buffer) (user-error "Buffer no longer exists"))
  (atelier-navigator-quit)
  (atelier-assign-buffer-to-workspace buffer)
  (switch-to-buffer buffer)
  (atelier-activate-buffer buffer)
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

(defun atelier-navigator-target-workspace (target &optional buffer)
  (pcase target
    (`(workspace ,name) (atelier-workspace-get name))
    (`(workspace-buffer ,name . ,_) (atelier-workspace-get name))
    (`(workspace-owned-buffer ,name . ,_) (atelier-workspace-get name))
    (`(workspace-content ,name . ,_) (atelier-workspace-get name))
    (`(workspace-owned-content ,name . ,_) (atelier-workspace-get name))
    (`(workspace-scratch ,name) (atelier-workspace-get name))
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

(atelier-define-operation atelier-navigator-finish-attach (target)
    (delete-dups
     (delq nil
           (list (atelier-current-workspace-id)
                 (when-let* ((workspace (atelier-navigator-target-workspace target nil)))
                   (atelier-workspace-id workspace))
                 (when-let* ((workspace (atelier-navigator-target-workspace
                                        atelier-navigator-attach-source
                                        (and (eq (car-safe atelier-navigator-attach-source) 'buffer)
                                             (get-buffer (nth 1 atelier-navigator-attach-source))))))
                   (atelier-workspace-id workspace))))) nil
  (let* ((source-target atelier-navigator-attach-source)
         (source (atelier-navigator-target-buffer source-target))
         (target-buffer (atelier-navigator-target-buffer target))
         (workspace (atelier-navigator-target-workspace target target-buffer)))
    (unless (or (eq (car-safe target) 'workspace) target-buffer)
      (user-error "Select a workspace or buffer"))
    (unless (buffer-live-p source) (user-error "Source buffer no longer exists"))
    (when (eq source target-buffer) (user-error "Choose another buffer"))
    (unless workspace (user-error "No target workspace"))
    (when-let* ((pair (car (atelier-entries-for-buffer source))))
      (atelier-check-entry-move (nth 1 pair) (car pair) workspace))
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

(atelier-define-operation atelier-navigator-detach (&optional target)
    (delete-dups
     (list (atelier-current-workspace-id) atelier-detached-workspace-id
           (atelier-workspace-id
             (or (atelier-workspace-get (nth 1 target))
                 (user-error "Workspace no longer exists")))))
    ((target (or target (atelier-navigator-target))))
  (interactive)
  (let ((detached-workspace (atelier-ensure-detached-workspace)))
    (pcase target
      (`(workspace-buffer ,workspace-name ,index ,entry-id)
       (when (equal workspace-name atelier-detached-workspace-name)
         (user-error "Entry is already detached"))
       (let* ((workspace (atelier-workspace-get workspace-name))
              (entry (atelier-entry-by-id workspace entry-id)))
         (unless entry (user-error "Entry no longer exists"))
         (atelier-check-entry-move entry workspace detached-workspace))
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

(atelier-define-operation atelier-navigator-open-content (workspace-name entry-id content-id &optional index)
    (delete-dups (list (atelier-current-workspace-id)
                       (atelier-workspace-id
                        (or (atelier-workspace-get workspace-name)
                            (user-error "Workspace no longer exists"))))) nil
  "Open CONTENT-ID in ENTRY-ID's existing view, or display its hidden entry."
  (let* ((workspace (atelier-workspace-get workspace-name))
         (entry (and workspace (atelier-entry-by-id workspace entry-id))))
    (unless (and entry (cl-find content-id (atelier-entry-stack entry)
                                :key (lambda (item) (plist-get item :id))
                                :test #'equal))
      (user-error "Content no longer belongs to this entry"))
    (when index
      (unless (cl-find entry-id (atelier-workspace-displayed-entries workspace)
                       :key (lambda (view) (plist-get view :id)) :test #'equal)
        (user-error "View is no longer displayed")))
    (atelier-navigator-quit)
    (unless (eq workspace (atelier-current-workspace))
      (atelier--open-workspace workspace (selected-frame)))
    (setq entry (atelier-operation-entry workspace entry-id))
    (let ((buffer (atelier-restore-entry-content entry workspace content-id)))
      (unless (buffer-live-p buffer)
        (user-error "Content could not be restored"))
      (if index
          (let ((current-index (cl-position entry (atelier-workspace-displayed-entries workspace))))
            (unless current-index (user-error "View is no longer displayed"))
            (set-window-buffer (atelier-focus-workspace-split workspace-name current-index) buffer))
        (switch-to-buffer buffer))
      (atelier-activate-buffer buffer)
      (atelier-notify-change))))

(defun atelier-navigator-open ()
  (interactive)
  (atelier-navigator-commit-stack-selection)
  (let ((target (atelier-navigator-target)))
    (if atelier-navigator-attach-source
        (atelier-navigator-finish-attach target)
      (pcase target
        ('nil (user-error "No item on this line"))
        (`(stopped-workspaces) (atelier-navigator-toggle-fold))
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
         (let* ((workspace (atelier-operation-workspace
                            (or (atelier-workspace-get workspace-name)
                                (user-error "Workspace no longer exists"))))
                (entry (atelier-operation-entry workspace entry-id)))
           (atelier-navigator-open-content workspace-name entry-id
                                            (car (plist-get entry :content-ids)) index)))
        (`(workspace-content ,workspace-name ,index ,entry-id ,content-id)
         (atelier-navigator-open-content workspace-name entry-id content-id index))
        (`(workspace-owned-content ,workspace-name ,entry-id ,content-id)
         (atelier-navigator-open-content workspace-name entry-id content-id))
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
           (atelier-activate-buffer buffer)
           (atelier-notify-change)))
        (`(project ,root) (atelier-navigator-quit) (atelier-open-project-workspace root))
        (`(buffer ,name)
         (if-let* ((buffer (get-buffer name)))
             (atelier-navigator-assign-buffer buffer)
           (user-error "Buffer no longer exists: %s" name)))))))

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
        (if entry
            (atelier-close-entry workspace entry window)
          (atelier-close-buffer (buffer-name buffer) window))))))

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

(defun atelier-workspace-replacement-buffer (workspace &optional type)
  "Return or restore the best replacement buffer in WORKSPACE.
Prefer the previous entry of TYPE when one remains."
  (let* ((entries (atelier-workspace-replacement-entries workspace))
         (same-type (and type
                         (cl-remove-if-not
                          (lambda (entry) (eq (atelier-entry-value entry :type) type))
                          entries))))
    (cl-loop for entry in (append same-type entries)
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
    (let ((same-type-buffer (and (not close-view-only) type
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

(defun atelier-prepare-content-close (workspace entry contents)
  "Confirm closing CONTENTS before removing their final buffer references."
  (unless atelier-close-without-asking
    (let* ((ids (mapcar (lambda (content) (plist-get content :id)) contents))
           (buffers (delete-dups
                     (mapcar (lambda (id)
                               (gethash (atelier-content-cache-key workspace id)
                                        atelier-content-live-buffers)) ids))))
      (cl-loop for buffer in buffers
               when (and (buffer-live-p buffer)
                         (not (cl-some
                               (lambda (owner)
                                 (cl-some
                                  (lambda (other)
                                    (cl-some
                                     (lambda (content)
                                       (let ((id (plist-get content :id)))
                                         (and (not (and (eq workspace owner)
                                                        (eq entry other) (member id ids)))
                                              (eq buffer (gethash (atelier-content-cache-key owner id)
                                                                  atelier-content-live-buffers)))))
                                     (atelier-entry-stack other)))
                                  (atelier-workspace-entries owner)))
                               atelier-workspaces)))
               collect (atelier-prepare-buffer-close buffer)))))

(atelier-define-operation atelier-close-entry (workspace entry &optional window entire-entry)
    (list (atelier-workspace-id workspace))
    ((workspace (atelier-operation-workspace workspace))
     (entry (atelier-operation-entry workspace entry)))
  "Remove ENTRY's active content, or the entire view when ENTIRE-ENTRY is non-nil.
Other explicit views and inactive stack buffers retain their ownership."
  (let* ((stack (atelier-entry-stack entry))
         (atelier-approved-buffer-closes
          (append (atelier-prepare-content-close
                   workspace entry (if (and (not entire-entry) (cdr stack))
                                       (list (car stack)) stack))
                  atelier-approved-buffer-closes))
         (buffer (atelier-entry-live-buffer entry))
         (type (atelier-entry-value entry :type))
         (other-visible-view
          (and (buffer-live-p buffer) window
               (cl-some (lambda (candidate)
                          (and (not (eq candidate window))
                               (eq (window-buffer candidate) buffer)))
                        (window-list (window-frame window) 'no-minibuffer)))))
    (atelier-validate-buffer-closes)
    (if (and (not entire-entry) (cdr (atelier-entry-stack entry)))
        (progn
          (atelier-entry-pop-content entry)
          (when (and window (window-live-p window))
            (set-window-buffer
             window (or (atelier-entry-live-buffer entry)
                        (atelier-restore-buffer entry workspace)
                        (atelier-empty-workspace-buffer workspace)))
            (set-window-prev-buffers window nil)
            (set-window-next-buffers window nil))
          (atelier-dispose-unreferenced-buffer buffer))
      (let ((inactive (mapcar (lambda (content)
                                (gethash (atelier-content-cache-key workspace (plist-get content :id))
                                         atelier-content-live-buffers))
                              (cdr (atelier-entry-stack entry)))))
        (atelier-entry-remove workspace entry t)
        (let ((retained (and (buffer-live-p buffer)
                             (atelier-buffer-referenced-p buffer))))
          (atelier-dispose-unreferenced-buffer buffer)
          (dolist (old inactive)
            (atelier-dispose-unreferenced-buffer old))
          (when window
            (atelier-close-entry-window workspace window type
                                         (or other-visible-view retained))))))
    (when (and window (eq workspace (atelier-current-workspace)))
      (atelier-capture-current-workspace))
    (atelier-notify-change)))

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
        (atelier-plist-set! entry :content-ids
                           (delete content-id (copy-sequence (plist-get entry :content-ids))))
        (unless (cl-some (lambda (other) (member content-id (plist-get other :content-ids)))
                         (atelier-workspace-entries workspace))
          (atelier-workspace-drop-content workspace content-id))
        (atelier-dispose-unreferenced-buffer buffer)
        (atelier-notify-change))))))

(defun atelier-navigator-close-entry ()
  "Remove the selected entry, including all of its contents."
  (interactive)
  (atelier-navigator-close t))

(defun atelier-navigator-close (&optional entire-entry)
  (interactive)
  (let ((target (atelier-navigator-target))
        (position-index (cl-position-if (lambda (position) (<= position (point)))
                                        (atelier-navigator-positions)
                                        :from-end t)))
    (unless target (user-error "No item on this line"))
    (unless (memq (car target) '(workspace buffer workspace-buffer workspace-owned-buffer
                                  workspace-content workspace-owned-content project))
      (user-error "This item cannot be closed"))
    (unless (y-or-n-p (format "%s? "
                              (pcase (car target)
                                ('workspace "Close and remove this workspace")
                                ('buffer "Kill this buffer")
                                ('workspace-buffer (if entire-entry "Remove this entry"
                                                     "Close this content"))
                                ('workspace-owned-buffer (if entire-entry "Remove this entry"
                                                           "Close this content"))
                                ('workspace-content (if entire-entry "Remove this entry"
                                                      "Close this content"))
                                ('workspace-owned-content (if entire-entry "Remove this entry"
                                                            "Close this content"))
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
         (atelier-close-entry workspace entry window entire-entry)))
      (`(workspace-owned-buffer ,workspace-name ,entry-id)
       (let* ((workspace (or (atelier-workspace-get workspace-name)
                             (user-error "Workspace no longer exists: %s" workspace-name)))
              (entry (or (atelier-entry-by-id workspace entry-id)
                         (user-error "Workspace entry no longer exists"))))
         (atelier-close-entry workspace entry nil entire-entry)))
      (`(workspace-content ,workspace-name ,index ,entry-id ,content-id)
       (let* ((workspace (atelier-workspace-get workspace-name))
              (entry (and workspace (atelier-entry-by-id workspace entry-id))))
         (unless entry (user-error "Workspace entry no longer exists"))
         (if entire-entry
             (atelier-close-entry workspace entry
                                  (atelier-workspace-entry-window workspace index entry) t)
           (atelier-close-entry-content workspace entry content-id
                                        (atelier-workspace-entry-window workspace index entry)))))
      (`(workspace-owned-content ,workspace-name ,entry-id ,content-id)
       (let* ((workspace (atelier-workspace-get workspace-name))
              (entry (and workspace (atelier-entry-by-id workspace entry-id))))
         (unless entry (user-error "Workspace entry no longer exists"))
         (if entire-entry (atelier-close-entry workspace entry nil t)
           (atelier-close-entry-content workspace entry content-id))))
      (`(project ,root) (project-forget-project root))
      (_ (user-error "This item cannot be closed")))
    (atelier-navigator)
    (when position-index
      (let* ((positions (atelier-navigator-positions))
             (position (nth (min position-index (1- (length positions))) positions)))
        (when position
          (goto-char position)
          (set-window-point (selected-window) position)
          (setf (alist-get (selected-frame) atelier-navigator-selection-by-frame
                           nil nil #'eq)
                (atelier-navigator-target)))))))

(atelier-define-operation atelier-navigator-rename (&optional target)
    (delete-dups
     (delq nil
           (list (atelier-current-workspace-id)
                 (when-let* ((workspace (atelier-navigator-target-workspace
                                        target (and (eq (car-safe target) 'buffer)
                                                    (get-buffer (nth 1 target))))))
                   (atelier-workspace-id workspace)))))
    ((target (or target (atelier-navigator-target))))
  (interactive)
  (let ((buffer (pcase target
                  (`(buffer ,name) (get-buffer name))
                  (`(,(or 'workspace-buffer 'workspace-owned-buffer) ,name . ,rest)
                   (let* ((workspace (atelier-workspace-get name))
                          (entry (and workspace (atelier-entry-by-id workspace (car (last rest))))))
                     (and entry (atelier-entry-live-buffer entry)))))))
    (when (buffer-live-p buffer)
      (let ((name (buffer-name buffer)))
        (atelier-operation-cleanup
         (lambda ()
           (when (buffer-live-p buffer)
             (with-current-buffer buffer (rename-buffer name))))))))
  (let ((target target))
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
             (atelier-entry-set-value entry :name
                                    (rename-buffer (read-string "New buffer name: " (buffer-name)) t)))
         (user-error "Entry buffer no longer exists")))
      (`(workspace-owned-buffer ,workspace-name ,entry-id)
       (if-let* ((workspace (atelier-workspace-get workspace-name))
                 (entry (atelier-entry-by-id workspace entry-id))
                 (buffer (atelier-entry-live-buffer entry)))
           (with-current-buffer buffer
             (atelier-entry-set-value entry :name
                                    (rename-buffer (read-string "New buffer name: " (buffer-name)) t)))
         (user-error "Entry buffer no longer exists")))
      (_ (user-error "This item cannot be renamed")))
    (atelier-notify-change)
    (atelier-navigator)))

(provide 'atelier-navigator)
;;; atelier-navigator.el ends here

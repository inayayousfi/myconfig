;;; atelier-navigator.el --- Atelier navigator UI -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'subr-x)
(require 'atelier-model)
(require 'atelier-operation)

(declare-function atelier-assign-buffer-to-workspace "atelier")
(declare-function atelier-capture-current-workspace "atelier")
(declare-function atelier-clean-window-buffer-history "atelier")
(declare-function atelier-close-unassigned-view "atelier")
(declare-function atelier-create-workspace "atelier")
(declare-function atelier-delete-workspace-record "atelier")
(declare-function atelier-display-entry-buffer "atelier")
(declare-function atelier-empty-workspace-buffer "atelier")
(declare-function atelier-known-project-roots "atelier")
(declare-function atelier-mark-internal-buffer "atelier")
(declare-function atelier-notify-change "atelier")
(declare-function atelier-open-project-workspace "atelier")
(declare-function atelier-register-buffer "atelier")
(declare-function atelier-rename-workspace "atelier")
(declare-function atelier-restore-buffer "atelier")
(declare-function atelier-restore-entry-buffer "atelier")
(declare-function atelier-restore-entry-content "atelier")
(declare-function atelier-show-buffer "atelier")
(declare-function atelier-stop-workspace "atelier")
(declare-function atelier-switch-workspace "atelier")
(declare-function atelier-workspace-entry-for-buffer "atelier")
(declare-function atelier-workspace-project-root "atelier")
(declare-function atelier-workspace-stop-jobs "atelier")
(declare-function atelier-buffer-editable-name "atelier-naming")
(declare-function atelier-content-base-name "atelier-naming")
(declare-function atelier-log-buffer-names "atelier-naming")

(defconst atelier-navigator-buffer "*Atelier*")
(defvar atelier-navigator-window-configurations nil
  "Each frame's real window configuration while the navigator covers it.")
(defvar atelier-navigator-selection-by-frame nil
  "Last selected navigator target per frame, restored after navigator refreshes.")
(defvar atelier-navigator-attach-source nil
  "The navigator target being attached, while choosing where it goes.")
(defvar-local atelier-navigator-first-position nil)

(defface atelier-navigator-active
  '((t (:inherit font-lock-keyword-face :weight bold)))
  "Selected workspace in the navigator."
  :group 'atelier)

(defface atelier-navigator-live
  '((t (:inherit default :weight bold)))
  "Live inactive workspace in the navigator."
  :group 'atelier)

(defface atelier-navigator-saved
  '((t (:inherit shadow)))
  "Stopped workspace in the navigator."
  :group 'atelier)

(defface atelier-navigator-hover
  '((t (:inherit highlight :weight bold)))
  "Readable pointer hover for navigator controls."
  :group 'atelier)

(defface atelier-navigator-current
  '((t (:inherit highlight :weight bold :extend t)))
  "Keyboard-selected navigator row."
  :group 'atelier)

(defface atelier-navigator-section
  '((t (:inherit font-lock-comment-face :weight bold :height 0.9)))
  "Navigator section headings."
  :group 'atelier)

(defface atelier-navigator-current-status
  '((t (:inherit success :weight bold)))
  "Current workspace status label."
  :group 'atelier)

(defface atelier-navigator-running-status
  '((t (:inherit font-lock-constant-face)))
  "Background running workspace status label."
  :group 'atelier)

(defface atelier-navigator-buffer
  '((t (:inherit default)))
  "Workspace buffer rows."
  :group 'atelier)

(defface atelier-navigator-branch
  '((t (:inherit shadow)))
  "Tree branches and secondary navigator text."
  :group 'atelier)

(defun atelier-workspace-menu (_event name)
  (interactive "e")
  (atelier-switch-workspace name)
  (popup-menu
   '("Workspace"
     ["Rename" atelier-rename-workspace t]
     ["Edit target" atelier-edit-workspace t]
     ["Close" atelier-close-workspace t]
     ["Delete" atelier-delete-workspace t])))

(defun atelier-clickable-label (label action &optional context-action face help)
  (let ((map (make-sparse-keymap)))
    (define-key map [mouse-1] action)
    (define-key map [mouse-2] action)
    (define-key map [mode-line mouse-1] action)
    (define-key map [mode-line mouse-2] action)
    (define-key map [header-line mouse-1] action)
    (define-key map [header-line mouse-2] action)
    (when context-action (define-key map [mouse-3] context-action))
    (propertize label 'face face 'mouse-face 'atelier-navigator-hover 'help-echo help
                'follow-link t 'keymap map)))

(defface atelier-navigator-view
  '((t (:inherit font-lock-constant-face)))
  "The view that shows a stack in the navigator."
  :group 'atelier)

(defface atelier-navigator-type
  '((t (:inherit font-lock-function-name-face)))
  "The type of a stack in the navigator."
  :group 'atelier)

(defface atelier-navigator-top
  '((t (:inherit font-lock-type-face)))
  "The buffer on top of a stack, which its view shows."
  :group 'atelier)

(defface atelier-navigator-tree
  '((t (:inherit font-lock-comment-face :slant normal)))
  "Tree lines and separators between the buffers of a stack."
  :group 'atelier)

(defvar-keymap atelier-navigator-mode-map
  :parent special-mode-map
  "j" #'atelier-navigator-next
  "k" #'atelier-navigator-previous
  "h" #'atelier-navigator-stack-previous
  "l" #'atelier-navigator-stack-next
  "{" #'atelier-navigator-previous-group
  "}" #'atelier-navigator-next-group
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
         (setq target (list 'workspace (atelier-workspace-name workspace))))))
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
  "Dispose of FRAME's private navigator and its remembered selection."
  (when-let* ((buffer (atelier-navigator-frame-buffer frame)))
    (kill-buffer buffer))
  (setq atelier-navigator-selection-by-frame
        (assq-delete-all frame atelier-navigator-selection-by-frame)))

(define-derived-mode atelier-navigator-mode special-mode "Atelier"
  (atelier-mark-interface-buffer)
  (setq-local header-line-format
              '(:eval (atelier-navigator-header)))
  (setq-local cursor-type 'box
              truncate-lines t
              line-spacing 0.12
              display-line-numbers-type 'relative)
  (hl-line-mode -1)
  (display-line-numbers-mode 1)
  (add-hook 'post-command-hook #'atelier-navigator-highlight-item nil t))

(defun atelier-navigator-header-shortcuts (command &optional other-command)
  "Describe active keyboard bindings for COMMAND and OTHER-COMMAND."
  (let ((maps (current-active-maps t))
        keys)
    (dolist (action (delq nil (list command other-command)))
      (dolist (key (where-is-internal action maps nil nil t))
        ;; A higher-priority map or command remapping can hide a binding.
        (when (and (not (cl-some
                         (lambda (event)
                           ;; Evil stores auxiliary maps under synthetic
                           ;; STATE-state prefixes, not keyboard events.
                           (or (mouse-event-p event)
                               (and (symbolp event)
                                    (string-suffix-p "-state" (symbol-name event)))))
                         key))
                   (eq (key-binding key t) action))
          (push (key-description key) keys))))
    (if keys
        (string-join (delete-dups (nreverse keys)) "/")
      "unbound")))

(defun atelier-navigator-header-button (label command help &optional other-command)
  (let ((button (atelier-clickable-label
                 (format "[%s %s]" label
                         (atelier-navigator-header-shortcuts command other-command))
                 command nil 'font-lock-keyword-face help)))
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
          (atelier-navigator-header-button "Item" #'atelier-navigator-stack-next
                                             "Move to another buffer on this row"
                                             #'atelier-navigator-stack-previous)
          (atelier-navigator-header-button "Group" #'atelier-navigator-next-group
                                             "Move to another workspace or section"
                                             #'atelier-navigator-previous-group)
          (atelier-navigator-header-button "Fold" #'atelier-navigator-toggle-fold
                                           "Fold or expand without opening")
          (atelier-navigator-header-button "Stop" #'atelier-navigator-stop-workspace
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
      (goto-char position)
      (atelier-navigator-open))))

(defvar atelier-navigator-group-pending nil
  "Whether the next inserted item starts a group that { and } jump to.")

(defun atelier-navigator-insert (text target &optional face)
  "Insert TEXT as an item that opens TARGET.
Text marked `atelier-navigator-decoration' only frames the item: the
cursor rests after it."
  (let* ((map (make-sparse-keymap))
         (newline (string-suffix-p "\n" text))
         (label (copy-sequence (if newline (substring text 0 -1) text)))
         (properties (list 'atelier-navigator-target target
                           'follow-link t 'keymap map 'rear-nonsticky t)))
    (define-key map [mouse-1] #'atelier-navigator-click)
    (define-key map [mouse-2] #'atelier-navigator-click)
    (when face (setq properties (append properties (list 'face face))))
    (when atelier-navigator-group-pending
      (setq properties (append properties (list 'atelier-navigator-group t))
            atelier-navigator-group-pending nil))
    (add-text-properties 0 (length label) properties label)
    (insert label)
    (when newline (insert "\n"))))

(defun atelier-navigator-section (title &optional detail)
  (unless (= (point) (point-min)) (insert "\n"))
  (insert (propertize (format "  %s" (upcase title)) 'face 'atelier-navigator-section))
  (when detail
    (insert (propertize (format "  %s" detail) 'face 'atelier-navigator-branch)))
  (insert "\n\n")
  (setq atelier-navigator-group-pending t))

(defun atelier-navigator-decoration (text &optional face)
  "Return TEXT as framing that the cursor skips, in FACE."
  (propertize text 'atelier-navigator-decoration t 'face (or face 'atelier-navigator-branch)))

(defun atelier-navigator-workspace-label (workspace active)
  (let* ((name (atelier-workspace-name workspace))
         (status (if active 'current (atelier-workspace-status workspace)))
         (icon (pcase status ('current "●") ('running "◉") (_ "○")))
         (name-face (cond (active 'atelier-navigator-active)
                          ((eq status 'running) 'atelier-navigator-live)
                          (t 'atelier-navigator-saved)))
         (status-face (pcase status
                        ('current 'atelier-navigator-current-status)
                        ('running 'atelier-navigator-running-status)
                        (_ 'atelier-navigator-saved))))
    (concat (atelier-navigator-decoration
             (if (atelier-navigator-workspace-expanded-p workspace) "  ▾ " "  ▸ ")
             'atelier-navigator-buffer)
            (atelier-navigator-decoration icon status-face)
            "  "
            (propertize (format "%s/" name) 'face name-face)
            "  "
            (propertize (format "(%s)" status) 'face status-face))))

(defun atelier-navigator-target ()
  (get-text-property (point) 'atelier-navigator-target))

(defun atelier-navigator-label-start (position)
  "Return where the cursor rests on the item whose text starts at POSITION.
That is its first character that is neither blank nor decoration."
  (let ((end (or (next-single-property-change position 'atelier-navigator-target)
                 (point-max)))
        (rest position))
    (while (and (< rest end)
                (or (memq (char-after rest) '(?\s ?\t))
                    (get-text-property rest 'atelier-navigator-decoration)))
      (setq rest (1+ rest)))
    (if (< rest end) rest position)))

(defun atelier-navigator-positions ()
  "Return the cursor position of each row's first item, in buffer order."
  (let ((position (point-min)) positions)
    (while (< position (point-max))
      (when (and (get-text-property position 'atelier-navigator-target)
                 (not (get-text-property position 'atelier-navigator-stack-item)))
        (push (atelier-navigator-label-start position) positions))
      (setq position (or (next-single-property-change
                          position 'atelier-navigator-target nil (point-max))
                         (point-max))))
    (nreverse positions)))

(defun atelier-navigator-move (delta &optional positions)
  "Move DELTA steps through POSITIONS, by default every row, wrapping around."
  (let* ((positions (or positions (atelier-navigator-positions)))
         (next (cl-position-if (lambda (position) (> position (point))) positions))
         (current (max 0 (1- (or next (length positions)))))
         (target (and positions (nth (mod (+ current delta) (length positions)) positions))))
    (when target (goto-char target))))

(defun atelier-navigator-row-items ()
  "Return the cursor position of each item on the current row."
  (let ((position (line-beginning-position))
        (end (line-end-position))
        items)
    (while (< position end)
      (when (get-text-property position 'atelier-navigator-target)
        (push (atelier-navigator-label-start position) items))
      (setq position (or (next-single-property-change
                          position 'atelier-navigator-target nil end)
                         end)))
    (nreverse items)))

(defun atelier-navigator-stack-move (delta)
  "Move the cursor DELTA buffers along the current stack row, wrapping around.
Only the cursor moves; RET opens the buffer under it."
  (let ((items (atelier-navigator-row-items)))
    (when (cdr items)
      (atelier-navigator-move delta items))))

(defun atelier-navigator-group-move (forward)
  "Move to the next workspace or section start, or the previous one unless
FORWARD, wrapping around."
  (let ((groups (cl-remove-if-not (lambda (position)
                                    (get-text-property position 'atelier-navigator-group))
                                  (atelier-navigator-positions))))
    (when groups
      (goto-char (if forward
                     (or (cl-find-if (lambda (position) (> position (point))) groups)
                         (car groups))
                   (or (cl-find-if (lambda (position) (< position (point))) groups
                                   :from-end t)
                       (car (last groups))))))))

(defun atelier-navigator-next-group ()
  "Move to the next workspace or section."
  (interactive)
  (atelier-navigator-group-move t))

(defun atelier-navigator-previous-group ()
  "Move to the previous workspace or section."
  (interactive)
  (atelier-navigator-group-move nil))

(defvar-local atelier-navigator-highlight nil
  "Overlay marking the item under the cursor.")

(defun atelier-navigator-highlight-item ()
  "Highlight the item under the cursor, from its label to its end."
  (let ((target (get-text-property (point) 'atelier-navigator-target)))
    (if (not target)
        (when atelier-navigator-highlight
          (delete-overlay atelier-navigator-highlight))
      (let* ((begin (or (previous-single-property-change
                         (1+ (point)) 'atelier-navigator-target)
                        (point-min)))
             (start (atelier-navigator-label-start begin))
             (end (or (next-single-property-change (point) 'atelier-navigator-target)
                      (point-max))))
        (unless atelier-navigator-highlight
          (setq atelier-navigator-highlight (make-overlay start end))
          (overlay-put atelier-navigator-highlight 'face 'atelier-navigator-current))
        (move-overlay atelier-navigator-highlight start end)))))

(defun atelier-navigator-stack-previous ()
  (interactive)
  (atelier-navigator-stack-move -1))

(defun atelier-navigator-stack-next ()
  (interactive)
  (atelier-navigator-stack-move 1))

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

(defun atelier-navigator-buffer-name (name &optional buffer)
  "Show Emacs duplicate suffixes as readable qualifiers, without renaming buffers.
A title supplied by BUFFER, the live buffer of NAME, replaces NAME."
  (let* ((title (and buffer
                     (run-hook-with-args-until-success
                      'atelier-buffer-title-functions buffer))))
    (if (and (stringp title) (not (string-empty-p (string-trim title))))
        (string-trim (replace-regexp-in-string "[[:cntrl:]]+" " " title))
      (if (string-match "\\`\\(.*\\)<\\([^<>]+\\)>\\'" name)
          (format "%s (%s)" (match-string 1 name) (match-string 2 name))
        name))))

(defun atelier-entry-id-less-p (left right)
  "Return non-nil when LEFT's permanent ID sorts before RIGHT's."
  (string-lessp (atelier-entry-field left :id) (atelier-entry-field right :id)))

(defun atelier-sort-entries-by-id (entries)
  "Return a copy of ENTRIES sorted by permanent entry ID."
  (sort (copy-sequence entries) #'atelier-entry-id-less-p))

(defun atelier-navigator-layout-label (entry)
  "Return a readable label for layout ENTRY's split direction."
  (pcase (atelier-entry-field entry :orientation)
    ('horizontal "Split (side-by-side)")
    ('vertical "Split (stacked)")
    (_ "Entry")))

(defun atelier-navigator-content-name (workspace content type)
  "Return CONTENT's NAME part as the navigator shows it, without its
workspace and TYPE, which the navigator shows elsewhere."
  (let ((buffer (atelier-content-buffer workspace content)))
    (setq buffer (and (buffer-live-p buffer) buffer))
    (atelier-navigator-buffer-name (atelier-content-base-name content type buffer) buffer)))

(defconst atelier-navigator-log-label "logs"
  "Type label of the row listing Emacs's own log buffers.")
(defconst atelier-navigator-stack-separator "  ·  ")

(defun atelier-navigator-type-label (label)
  "Return LABEL, a stack's type, shown once before the names of its stack.
Labels share one width so that the names after them line up."
  (let ((width (apply #'max (length atelier-navigator-log-label)
                      (mapcar #'length (atelier-type-labels)))))
    (atelier-navigator-decoration (format (format "%%-%ds  " width) label)
                                  'atelier-navigator-type)))

(defun atelier-navigator-insert-row (lead label items)
  "Insert LEAD, then LABEL, then the name of each of ITEMS, ending the line.
ITEMS is a list of (NAME . TARGET); the first is on top of its stack, and
h and l move the cursor between them."
  (atelier-navigator-insert
   (concat lead (atelier-navigator-type-label label)
           (propertize (car (car items)) 'face 'atelier-navigator-top))
   (cdr (car items)))
  (dolist (item (cdr items))
    (insert (propertize atelier-navigator-stack-separator 'face 'atelier-navigator-tree))
    (let ((start (point)))
      (atelier-navigator-insert (car item) (cdr item) 'atelier-navigator-buffer)
      (put-text-property start (point) 'atelier-navigator-stack-item t)))
  (insert "\n"))

(defun atelier-navigator-stack-items (workspace stack type first-target content-target)
  "Return STACK's contents as row items: the first opens FIRST-TARGET, and
CONTENT-TARGET, called with a content's ID, gives each other's target."
  (if stack
      (cons (cons (atelier-navigator-content-name workspace (car stack) type) first-target)
            (mapcar (lambda (content)
                      (cons (atelier-navigator-content-name workspace content type)
                            (funcall content-target (atelier-content-field content :id))))
                    (cdr stack)))
    (list (cons "(empty)" first-target))))

(defun atelier-navigator-render-entry-tree
    (entry workspace-name displayed active prefix last-child)
  "Render ENTRY and its children for WORKSPACE-NAME.
DISPLAYED contains visible leaves in window traversal order.  PREFIX and
LAST-CHILD describe the current branch position in the rendered tree."
  (let* ((branch (if last-child "╰─" "├─"))
         (child-prefix (concat prefix (if last-child "   " "│  "))))
    (if (atelier-layout-entry-p entry)
        (progn
          (insert (propertize (concat prefix branch) 'face 'atelier-navigator-tree)
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
      (let* ((entry-id (atelier-entry-field entry :id))
             (index (cl-position entry-id displayed
                                 :key (lambda (candidate)
                                        (atelier-entry-field candidate :id))
                                 :test #'equal))
             (visible (integerp index))
             (workspace (atelier-entry-owner entry))
             (type (or (atelier-entry-value entry :type workspace) 'buffer))
             (stack (atelier-entry-stack entry workspace))
             (selected (and visible active (atelier-entry-field entry :selected))))
        (atelier-navigator-insert-row
         (concat (atelier-navigator-decoration (concat prefix branch " ") 'atelier-navigator-tree)
                 (if selected (atelier-navigator-decoration "▸ " 'atelier-navigator-active) "")
                 (if visible
                     (atelier-navigator-decoration (format "View %d  " (1+ index))
                                                   'atelier-navigator-view)
                   ""))
         (atelier-type-label type)
         (atelier-navigator-stack-items
          workspace stack type
          (if visible
              (list 'workspace-buffer workspace-name index entry-id)
            (list 'workspace-owned-buffer workspace-name entry-id))
          (lambda (content-id)
            (if visible
                (list 'workspace-content workspace-name index entry-id content-id)
              (list 'workspace-owned-content workspace-name entry-id content-id)))))))))

(defun atelier-navigator-render-workspace (workspace &optional indent)
  "Render WORKSPACE's heading and, when expanded, its saved contents.
INDENT, a string, shifts the whole workspace to the right."
  (atelier-workspace-entries workspace)
  (let* ((workspace-name (atelier-workspace-name workspace))
         (active (eq workspace (atelier-current-workspace)))
         (indent (or indent ""))
         (prefix (concat indent "     ")))
    (setq atelier-navigator-group-pending t)
    (atelier-navigator-insert
     (concat (atelier-navigator-decoration indent)
             (atelier-navigator-workspace-label workspace active))
     (list 'workspace workspace-name))
    (insert "\n")
    (when (atelier-navigator-workspace-expanded-p workspace)
      (let* ((displayed-root (atelier-workspace-displayed-entry workspace))
             (displayed-entries (atelier-workspace-displayed-entries workspace))
             (displayed-ids (mapcar (lambda (entry) (atelier-entry-field entry :id)) displayed-entries))
              (visible-types (mapcar (lambda (entry) (atelier-entry-field entry :stack-id))
                                     displayed-entries))
              (hidden (let ((seen visible-types))
                        (cl-remove-if
                         (lambda (entry)
                           (let ((type (atelier-entry-field entry :stack-id)))
                             (if (member type seen) t (push type seen) nil)))
                         (cl-remove-if
                          (lambda (entry) (member (atelier-entry-field entry :id) displayed-ids))
                          (atelier-workspace-entries workspace))))))
        (when displayed-root
          (atelier-navigator-render-entry-tree
           displayed-root workspace-name displayed-entries active prefix nil))
        ;; The scratch action below always closes the branch.
        (dolist (entry hidden)
          (atelier-navigator-render-entry-tree
           entry workspace-name displayed-entries active prefix nil))
        (atelier-navigator-insert
         (concat (atelier-navigator-decoration (concat prefix "╰─ ") 'atelier-navigator-tree)
                 "＋ New scratch buffer\n")
         (list 'workspace-scratch workspace-name) 'success)))
    (insert "\n")))

(defun atelier-render-navigator ()
  (let ((buffer (atelier-navigator-frame-buffer nil t))
        workspace-roots first-item)
    (with-current-buffer buffer
      (unless (derived-mode-p 'atelier-navigator-mode)
        (atelier-navigator-mode))
      (let ((inhibit-read-only t)
            (atelier-navigator-group-pending nil))
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
          (setq atelier-navigator-group-pending t)
          (atelier-navigator-insert
           (concat (atelier-navigator-decoration
                    (if atelier-navigator-stopped-expanded "  ▾ " "  ▸ ")
                    'atelier-navigator-saved)
                   (propertize (format "Stopped workspaces (%d)\n\n" (length stopped))
                               'face 'atelier-navigator-saved))
           '(stopped-workspaces))
          (when atelier-navigator-stopped-expanded
            (dolist (workspace stopped)
              (atelier-navigator-render-workspace workspace "  "))))
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
               (concat (atelier-navigator-decoration "  ◇  ")
                       (propertize
                        (format "%s/" (file-name-nondirectory (directory-file-name root)))
                        'face 'font-lock-keyword-face)
                       (propertize (format "  %s" (abbreviate-file-name root))
                                   'face 'atelier-navigator-branch)
                       "\n")
               (list 'project root)))))
        (let* ((workspace (atelier-ensure-detached-workspace))
               ;; Views showing the same stack share one row.
               (stacks (let (seen)
                         (cl-loop for entry in (atelier-sort-entries-by-id
                                                (atelier-workspace-entries workspace))
                                  for stack-id = (atelier-entry-field entry :stack-id)
                                  unless (and stack-id (member stack-id seen))
                                  collect (progn (push stack-id seen)
                                                 (cons entry (atelier-entry-stack entry workspace))))))
               (logs (cl-remove-if-not #'get-buffer (atelier-log-buffer-names)))
               (total (+ (length logs)
                         (apply #'+ (mapcar (lambda (stack) (length (cdr stack))) stacks)))))
          (atelier-navigator-section "Detached buffers"
                                     (if (> total 0) (format "%d total" total) "none"))
          (dolist (stack stacks)
            (let* ((entry (car stack))
                   (entry-id (atelier-entry-field entry :id))
                   (type (or (atelier-entry-value entry :type workspace) 'buffer)))
              (atelier-navigator-insert-row
               (atelier-navigator-decoration "  •  ")
               (atelier-type-label type)
               (atelier-navigator-stack-items
                workspace (cdr stack) type
                (list 'workspace-owned-buffer atelier-detached-workspace-name entry-id)
                (lambda (content-id)
                  (list 'workspace-owned-content atelier-detached-workspace-name
                        entry-id content-id))))))
          (when logs
            (atelier-navigator-insert-row
             (atelier-navigator-decoration "  •  ")
             atelier-navigator-log-label
             (mapcar (lambda (name) (cons name (list 'detached-log name))) logs)))
          (when (= total 0)
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
          (goto-char position)
          (atelier-navigator-highlight-item))))
    buffer))

(defun atelier-navigator-quit ()
  (interactive)
  (setq atelier-navigator-attach-source nil)
  (let* ((frame (selected-frame))
         (navigator (atelier-navigator-frame-buffer))
         (target (and navigator
                      (with-current-buffer navigator
                        (when (derived-mode-p 'atelier-navigator-mode)
                          (atelier-navigator-target)))))
         (configuration (alist-get frame atelier-navigator-window-configurations nil nil #'eq)))
    (when target
      (setf (alist-get frame atelier-navigator-selection-by-frame nil nil #'eq)
            target))
    (setq atelier-navigator-window-configurations
          (assq-delete-all frame atelier-navigator-window-configurations))
    (when configuration
      (set-window-configuration configuration)
      (atelier-clean-window-buffer-history))))

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

(atelier-define-operation atelier-navigator-assign-buffer (buffer)
    (list (atelier-current-workspace-id)) nil
  (unless (buffer-live-p buffer) (user-error "Buffer no longer exists"))
  (atelier-navigator-quit)
  (atelier-assign-buffer-to-workspace buffer)
  (switch-to-buffer buffer)
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
                  '(workspace-buffer workspace-owned-buffer))
      (user-error "Select a stack to attach"))
    (setq atelier-navigator-attach-source target)
    (force-mode-line-update t)
    (message "Select a workspace or view with Enter")))

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
  "Attach the stack chosen with `atelier-navigator-attach' to TARGET."
  (let* ((source-target atelier-navigator-attach-source)
         (source-workspace (atelier-navigator-target-workspace source-target))
         (workspace (atelier-navigator-target-workspace target))
         (source-id (pcase source-target
                      (`(workspace-owned-buffer ,_ ,id) id)
                      (`(workspace-buffer ,_ ,_ ,id) id)))
         (view (pcase target (`(workspace-buffer ,_ ,index ,id) (cons id index)))))
    (unless (and workspace (or view (eq (car-safe target) 'workspace)))
      (user-error "Select a workspace or view"))
    (unless source-id (user-error "Select a workspace stack to attach"))
    (setq atelier-navigator-attach-source nil)
    (atelier-navigator-quit)
    (atelier-attach-stack source-workspace source-id workspace (car view) (cdr view))
    (atelier-navigator)))

(atelier-define-operation atelier-navigator-detach (&optional target)
    (delete-dups
     (list (atelier-current-workspace-id) atelier-detached-workspace-id
           (atelier-workspace-id
             (or (atelier-workspace-get (nth 1 target))
                 (user-error "Workspace no longer exists")))))
    ((target (or target (atelier-navigator-target))))
  (interactive)
  (pcase target
    (`(workspace-buffer ,workspace-name ,index ,entry-id)
     (setq atelier-navigator-attach-source nil)
     (atelier-detach-view (atelier-workspace-get workspace-name) entry-id index)
     (atelier-navigator))
    (`(workspace-owned-buffer ,workspace-name ,entry-id)
     (atelier-detach-stack (atelier-workspace-get workspace-name) entry-id)
     (atelier-render-navigator))
    (_ (user-error "Select a split or workspace-owned buffer"))))

(atelier-define-operation atelier-navigator-open-content (workspace-name entry-id content-id &optional index)
    (delete-dups (list (atelier-current-workspace-id)
                       (atelier-workspace-id
                        (or (atelier-workspace-get workspace-name)
                            (user-error "Workspace no longer exists"))))) nil
  "Open CONTENT-ID in ENTRY-ID's existing view, or display its hidden entry."
  (atelier-open-content (atelier-workspace-get workspace-name) entry-id content-id index))

(defun atelier-navigator-open ()
  (interactive)
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
                                             (atelier-entry-field entry :content-id) index)))
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
           (when (buffer-live-p buffer)
             (atelier-show-buffer buffer workspace))
           (atelier-notify-change)))
        (`(project ,root) (atelier-navigator-quit) (atelier-open-project-workspace root))
        (`(detached-log ,name)
         ;; Like Traveller: Detached shows Emacs's logs without storing them.
         (let ((buffer (or (get-buffer name) (user-error "Buffer no longer exists: %s" name))))
           (atelier-navigator-quit)
           (unless (atelier-detached-workspace-p (atelier-current-workspace))
             (atelier-switch-workspace atelier-detached-workspace-name))
           (switch-to-buffer buffer)))
        (`(buffer ,name)
         (if-let* ((buffer (get-buffer name)))
             (atelier-navigator-assign-buffer buffer)
           (user-error "Buffer no longer exists: %s" name)))))))

(defun atelier-navigator-close-entry ()
  "Delete the selected entry's whole stack, every view of it and their splits."
  (interactive)
  (atelier-navigator-close t))

(defun atelier-navigator-close (&optional entire-entry)
  (interactive)
  (let ((target (atelier-navigator-target))
        (position-index (cl-position-if (lambda (position) (<= position (point)))
                                        (atelier-navigator-positions)
                                        :from-end t)))
    (unless target (user-error "No item on this line"))
    (unless (memq (car target) '(workspace buffer detached-log workspace-buffer workspace-owned-buffer
                                  workspace-content workspace-owned-content project))
      (user-error "This item cannot be closed"))
    (unless (y-or-n-p (format "%s? "
                              (pcase (car target)
                                ('workspace "Close and remove this workspace")
                                ((or 'buffer 'detached-log) "Kill this buffer")
                                ('workspace-buffer (if entire-entry "Delete this view, its stack and all its contents"
                                                     "Close this content"))
                                ('workspace-owned-buffer (if entire-entry "Remove this entry"
                                                           "Close this content"))
                                ('workspace-content (if entire-entry "Delete this view, its stack and all its contents"
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
      (`(detached-log ,name)
       (when-let* ((buffer (get-buffer name)))
         (kill-buffer buffer)))
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
  (unless target (user-error "No item on this line"))
  (atelier-navigator-quit)
  (pcase target
    (`(workspace ,name)
     (atelier-switch-workspace name)
     (atelier-rename-workspace))
    (`(buffer ,name)
     (let ((buffer (or (get-buffer name) (user-error "Buffer no longer exists: %s" name))))
       (atelier-rename-live-buffer
        buffer (read-string "New buffer name: " (atelier-buffer-editable-name buffer)))))
    (`(,(or 'workspace-buffer 'workspace-owned-buffer) ,workspace-name . ,rest)
     (let* ((workspace (atelier-workspace-get workspace-name))
            (entry-id (car (last rest)))
            (buffer (or (atelier-entry-buffer workspace entry-id)
                        (user-error "Entry buffer no longer exists"))))
       (atelier-rename-content workspace entry-id
                               (read-string "New buffer name: "
                                            (atelier-buffer-editable-name buffer)))))
    (_ (user-error "This item cannot be renamed")))
  (atelier-notify-change)
  (atelier-navigator))

(defun atelier-navigator-covers-frame-p (frame)
  (and (assq frame atelier-navigator-window-configurations) t))

(defun atelier-navigator-uncover-frame (frame)
  (with-selected-frame frame (atelier-navigator-quit)))

(defun atelier-navigator-refresh-frame (frame)
  (with-selected-frame frame (atelier-render-navigator)))

(defun atelier-navigator-save-frame-state (frame)
  (list (alist-get frame atelier-navigator-window-configurations nil nil #'eq)
        (alist-get frame atelier-navigator-selection-by-frame nil nil #'eq)))

(defun atelier-navigator-restore-frame-state (frame state)
  "Return FRAME's navigator records to STATE, from before a failed operation."
  (setq atelier-navigator-window-configurations
        (assq-delete-all frame atelier-navigator-window-configurations)
        atelier-navigator-selection-by-frame
        (assq-delete-all frame atelier-navigator-selection-by-frame))
  (when (nth 0 state)
    (push (cons frame (nth 0 state)) atelier-navigator-window-configurations))
  (when (nth 1 state)
    (push (cons frame (nth 1 state)) atelier-navigator-selection-by-frame)))

(defun atelier-navigator-close-frame (frame)
  "Forget FRAME's real layout and navigator once FRAME closes."
  (setq atelier-navigator-window-configurations
        (assq-delete-all frame atelier-navigator-window-configurations))
  (atelier-navigator-frame-closed frame))

(defun atelier-navigator-show-in-new-frame (frame)
  (when (and (frame-live-p frame) (display-graphic-p frame) (atelier-workspace-list))
    (with-selected-frame frame (atelier-navigator))))

(defun atelier-navigator-at-startup ()
  (when (display-graphic-p)
    (atelier-navigator)))

(defun atelier-navigator-setup ()
  "Show the navigator at startup and in new frames, and tell Atelier when it
covers a frame.  Its frame hooks run after Atelier's own."
  (add-hook 'atelier-frame-covered-functions #'atelier-navigator-covers-frame-p)
  (add-hook 'atelier-uncover-frame-functions #'atelier-navigator-uncover-frame)
  (add-hook 'atelier-covered-frame-refresh-functions #'atelier-navigator-refresh-frame)
  (cl-pushnew (cons #'atelier-navigator-save-frame-state #'atelier-navigator-restore-frame-state)
              atelier-frame-state-functions :test #'equal)
  (add-hook 'delete-frame-functions #'atelier-navigator-close-frame 90)
  (add-hook 'after-make-frame-functions #'atelier-navigator-show-in-new-frame 90)
  (add-hook 'emacs-startup-hook #'atelier-navigator-at-startup))

(provide 'atelier-navigator)
;;; atelier-navigator.el ends here

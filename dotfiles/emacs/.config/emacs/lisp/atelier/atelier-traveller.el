;;; atelier-traveller.el --- Travel to any workspace buffer -*- lexical-binding: t; -*-

;; Traveller lists every buffer a workspace owns, open or saved, as
;; "WORKSPACE | TYPE | NAME", most recently used first, saved-only buffers
;; next and the current buffer last.  Typed words match anywhere in that text,
;; each allowing gaps between its letters, without changing that order.
;; Locked segments lead the input as "WORKSPACE | " or "WORKSPACE | TYPE | ",
;; and the list keeps only buffers under them.  TAB fills in the next segment
;; of the highlighted buffer, dropping typed words that matched that segment;
;; further TABs, or Shift+TAB backward, fill in the next segment of each
;; following matching workspace or type, then each matching buffer.  Any other
;; key keeps what was filled in and then does its own work: typing searches
;; under it, Enter opens the highlighted buffer.  The input stays ordinary
;; text, so Backspace into a filled-in segment unlocks it.  Choosing a buffer
;; first selects its workspace, as the navigator does, then shows the buffer
;; there.

(require 'cl-lib)
(require 'subr-x)
(require 'atelier-model)
(require 'atelier-naming)

(declare-function atelier-switch-workspace "atelier")
(declare-function atelier-show-buffer "atelier")
(declare-function atelier-restore-entry-content "atelier")
(declare-function atelier-main-window "atelier-navigator")
(declare-function atelier-current-entry "atelier")
(defvar orderless-matching-styles)
(defvar orderless-component-separator)

(defvar atelier-traveller-open-functions nil
  "Functions called with a live buffer that no stack holds.
Traveller has already selected the buffer's workspace.  The first function
returning non-nil has shown the buffer; otherwise Traveller switches to it.")

(defun atelier-traveller-segments (workspace type)
  "Return the lockable WORKSPACE and TYPE segments of a label."
  (list (atelier-workspace-name workspace) (atelier-type-label type)))

(defun atelier-traveller-targets ()
  "Return every buffer Traveller can reach, as plists with a :label."
  (let (targets held)
    (dolist (workspace (atelier-workspace-list))
      (dolist (content (atelier-workspace-contents workspace))
        (let* ((type (or (atelier-content-field content :type) 'buffer))
               (buffer (atelier-content-buffer workspace content))
               (buffer (and (buffer-live-p buffer) buffer)))
          (when buffer (push buffer held))
          (push (list :label (atelier-buffer-qualified-name
                              workspace type (atelier-content-base-name content type buffer))
                      :segments (atelier-traveller-segments workspace type)
                      :workspace-id (atelier-workspace-id workspace)
                      :content-id (atelier-content-field content :id)
                      :buffer buffer
                      :saved (not buffer))
                targets))))
    (dolist (buffer (buffer-list))
      (unless (memq buffer held)
        (when-let* ((owner (if (member (buffer-name buffer) (atelier-log-buffer-names))
                               (list (atelier-ensure-detached-workspace) 'buffer)
                             (atelier-buffer-owner buffer))))
          (let ((type (or (nth 1 owner) 'buffer)))
            (push (list :label (atelier-buffer-qualified-name
                                (car owner) type
                                (atelier-buffer-strip-qualifier (buffer-name buffer) type))
                        :segments (atelier-traveller-segments (car owner) type)
                        :workspace-id (atelier-workspace-id (car owner))
                        :buffer buffer)
                  targets)))))
    (setq targets (nreverse targets))
    (let ((seen (make-hash-table :test #'equal)))
      (dolist (target targets targets)
        (let* ((label (plist-get target :label))
               (count (cl-incf (gethash label seen 0))))
          (when (> count 1)
            (setf (plist-get target :label) (format "%s <%d>" label count))))))))

(defun atelier-traveller-by-recency (targets current)
  "Return TARGETS with open buffers by last use, then saved ones, then CURRENT."
  (let ((ranks (make-hash-table :test #'eq))
        (rank 0))
    (dolist (buffer (buffer-list))
      (puthash buffer (cl-incf rank) ranks))
    (cl-stable-sort (copy-sequence targets) #'<
                    :key (lambda (target)
                           (let ((buffer (plist-get target :buffer)))
                             (cond ((and buffer (eq buffer current)) (+ rank 2))
                                   ((and buffer (gethash buffer ranks)))
                                   (t (1+ rank))))))))

(defvar atelier-traveller--targets nil
  "Targets of the Traveller prompt being read.")

(defvar-local atelier-traveller--cycle nil
  "The TAB cycle in progress, as (INPUTS . INDEX), or nil.")

(defvar-keymap atelier-traveller-minibuffer-map
  :doc "Keys of the Traveller prompt, above the completion list's own keys."
  "TAB" #'atelier-traveller-cycle-forward
  "<backtab>" '(menu-item "" atelier-traveller-cycle-backward
                          :filter atelier-traveller-when-cycling))

(defun atelier-traveller-lock-prefixes (targets)
  "Return every \"WORKSPACE | \" and \"WORKSPACE | TYPE | \" of TARGETS."
  (delete-dups
   (cl-loop for target in targets
            for (workspace type) = (plist-get target :segments)
            collect (concat workspace atelier-buffer-name-separator)
            collect (concat workspace atelier-buffer-name-separator
                            type atelier-buffer-name-separator))))

(defun atelier-traveller-locked (input targets)
  "Return the longest locked segments that INPUT starts with, or \"\"."
  (let ((locked ""))
    (dolist (prefix (atelier-traveller-lock-prefixes targets) locked)
      (when (and (> (length prefix) (length locked)) (string-prefix-p prefix input))
        (setq locked prefix)))))

(defun atelier-traveller-table (targets)
  "Complete TARGETS' labels, listing only those under the locked segments."
  (let ((labels (mapcar (lambda (target)
                          (let ((label (copy-sequence (plist-get target :label))))
                            (when (plist-get target :saved)
                              (put-text-property 0 (length label) 'atelier-traveller-saved t label))
                            label))
                        targets)))
    (lambda (string predicate action)
      (if (eq action 'metadata)
          `(metadata (category . atelier-traveller)
                     ;; Keep the recency order while typing.
                     (display-sort-function . identity)
                     (cycle-sort-function . identity)
                     (annotation-function
                      . ,(lambda (candidate)
                           (when (get-text-property 0 'atelier-traveller-saved candidate)
                             "  saved"))))
        (let ((locked (atelier-traveller-locked string targets)))
          (completion-table-with-context
           locked
           (cl-loop for label in labels
                    when (string-prefix-p locked label)
                    collect (substring label (length locked)))
           (substring string (length locked)) predicate action))))))

(defun atelier-traveller-setup-prompt ()
  "Put Traveller's keys above the completion list's own keys in this prompt."
  (use-local-map (make-composed-keymap atelier-traveller-minibuffer-map (current-local-map)))
  (add-hook 'pre-command-hook #'atelier-traveller-end-cycle nil t))

(defun atelier-traveller-read (targets)
  "Choose one of TARGETS by label, matching letters with gaps."
  (let* ((atelier-traveller--targets targets)
         (choice (atelier-traveller-with-matching
                  (lambda ()
                    ;; Appended so it runs after the completion list installs its keys.
                    (minibuffer-with-setup-hook (:append #'atelier-traveller-setup-prompt)
                      (completing-read "Traveller: " (atelier-traveller-table targets)
                                       nil t))))))
    (cl-find choice targets :key (lambda (target) (plist-get target :label)) :test #'equal)))

(defun atelier-traveller-words (input)
  "Split typed INPUT into the words matched separately.
A word made only of the segment separator matches every label, so it is
left out."
  (cl-remove-if (lambda (word)
                  (string-empty-p (string-trim word "[ |]+" "[ |]+")))
                (if (featurep 'orderless)
                    (let ((separator orderless-component-separator))
                      (if (functionp separator)
                          (funcall separator input)
                        (split-string input separator)))
                  (list input))))

(defun atelier-traveller-word-matches-p (word text)
  "Return non-nil when typed WORD matches TEXT as the list matches labels."
  (atelier-traveller-with-matching
   (lambda () (completion-all-completions word (list text) nil (length word)))))

(defun atelier-traveller-locked-input (input label targets)
  "Return INPUT once the next segment of LABEL is locked.
Typed words that match the newly locked segment are dropped; LABEL still
matches what remains.  When both segments are already locked, return LABEL."
  (let* ((target (or (cl-find label targets :key (lambda (target) (plist-get target :label))
                              :test #'equal)
                     (user-error "No buffer selected")))
         (locked (atelier-traveller-locked input targets))
         (segments (plist-get target :segments))
         (depth (cl-count-if (lambda (prefix) (string-prefix-p prefix locked))
                             (atelier-traveller-lock-prefixes (list target)))))
    (if (>= depth 2)
        label
      (let* ((next (concat (string-join (cl-subseq segments 0 (1+ depth))
                                        atelier-buffer-name-separator)
                           atelier-buffer-name-separator))
             (segment (nth depth segments)))
        (concat next
                (string-join
                 (cl-remove-if (lambda (word) (atelier-traveller-word-matches-p word segment))
                               (atelier-traveller-words (substring input (length locked))))
                 " "))))))

(defun atelier-traveller-replace-input (input)
  (delete-minibuffer-contents)
  (insert input))

(defun atelier-traveller-cycle-inputs (input labels targets)
  "Return the inputs TAB goes through from INPUT, for matching LABELS in order.
Each filled-in workspace or type appears once, for its first label; at the
buffer level every label appears."
  (let (seen inputs)
    (dolist (label labels (nreverse inputs))
      (let* ((next (atelier-traveller-locked-input input label targets))
             (key (if (equal next label) label (atelier-traveller-locked next targets))))
        (unless (member key seen)
          (push key seen)
          (push next inputs))))))

(defvar atelier-traveller-highlighted-function nil
  "Function returning the full label the completion list highlights, or nil.
A completion interface supplies it; without one, TAB starts from the first
match.")

(defun atelier-traveller-matching-labels (input)
  "Return the full labels matching INPUT, in the order the list shows them."
  (let* ((table (atelier-traveller-table atelier-traveller--targets))
         (matches (atelier-traveller-with-matching
                   (lambda () (completion-all-completions input table nil (length input)))))
         (base (substring input 0 (car (completion-boundaries input table nil "")))))
    (when matches (setcdr (last matches) nil))
    (mapcar (lambda (match) (concat base (substring-no-properties match))) matches)))

(defun atelier-traveller-listed-labels ()
  "Return the full labels the completion list shows, from the highlighted one on."
  (let* ((labels (atelier-traveller-matching-labels (minibuffer-contents-no-properties)))
         (highlighted (and atelier-traveller-highlighted-function
                           (funcall atelier-traveller-highlighted-function)))
         (index (or (and highlighted (cl-position highlighted labels :test #'equal)) 0)))
    (append (nthcdr index labels) (take index labels))))

(defun atelier-traveller-cycle (step)
  "Fill in the cycle input STEP places after the current one."
  (unless atelier-traveller--cycle
    (setq atelier-traveller--cycle
          (cons (atelier-traveller-cycle-inputs (minibuffer-contents-no-properties)
                                                (atelier-traveller-listed-labels)
                                                atelier-traveller--targets)
                -1)))
  (pcase-let ((`(,inputs . ,index) atelier-traveller--cycle))
    (if (null inputs)
        (setq atelier-traveller--cycle nil)
      (setq index (mod (+ index step) (length inputs)))
      (setcdr atelier-traveller--cycle index)
      (atelier-traveller-replace-input (nth index inputs)))))

(defun atelier-traveller-cycle-forward ()
  "Fill in the next matching workspace, type or buffer."
  (interactive)
  (atelier-traveller-cycle 1))

(defun atelier-traveller-cycle-backward ()
  "Fill in the previous cycled value."
  (interactive)
  (atelier-traveller-cycle -1))

(defun atelier-traveller-when-cycling (command)
  "Return COMMAND while a TAB cycle is in progress."
  (and atelier-traveller--cycle command))

(defun atelier-traveller-end-cycle ()
  "Keep what a TAB cycle filled in once any other command runs."
  (unless (memq this-command '(atelier-traveller-cycle-forward atelier-traveller-cycle-backward))
    (setq atelier-traveller--cycle nil)))

(defun atelier-traveller-with-matching (function)
  "Call FUNCTION with each typed word matching letters in order, with gaps."
  (if (featurep 'orderless)
      (let ((completion-styles '(orderless))
            (orderless-matching-styles '(orderless-flex)))
        (funcall function))
    (let ((completion-styles '(flex)))
      (funcall function))))

(atelier-define-operation atelier-traveller-show-content (workspace-id content-id)
    (list workspace-id) nil
  "Show CONTENT-ID of WORKSPACE-ID, restoring it first when only saved."
  (let* ((workspace (atelier-operation-workspace workspace-id))
         (content (or (atelier-workspace-content workspace content-id)
                      (user-error "Buffer no longer exists")))
         (buffer (atelier-restore-entry-content
                  (atelier-content-reference workspace content) workspace content-id)))
    (when (buffer-live-p buffer)
      (atelier-show-buffer buffer workspace))
    buffer))

(defun atelier-traveller-open (target)
  "Select TARGET's workspace, then show TARGET there."
  (let ((workspace (or (atelier-workspace-by-id (plist-get target :workspace-id))
                       (user-error "Workspace no longer exists"))))
    (when (window-parameter nil 'window-side)
      (select-window (atelier-main-window)))
    (unless (equal (atelier-current-workspace-id) (atelier-workspace-id workspace))
      (atelier-switch-workspace (atelier-workspace-name workspace))
      (unless (equal (atelier-current-workspace-id) (atelier-workspace-id workspace))
        (user-error "Workspace %s is not open yet; travel again once it is"
                    (atelier-workspace-name workspace))))
    (if-let* ((content-id (plist-get target :content-id)))
        (atelier-traveller-show-content (atelier-workspace-id workspace) content-id)
      (let ((buffer (plist-get target :buffer)))
        (unless (buffer-live-p buffer) (user-error "Buffer no longer exists"))
        (unless (run-hook-with-args-until-success 'atelier-traveller-open-functions buffer)
          (switch-to-buffer buffer))))))

(defun atelier-traveller ()
  "Travel to any workspace's buffer, open or saved."
  (interactive)
  (atelier-traveller-open
   (or (atelier-traveller-read (atelier-traveller-by-recency (atelier-traveller-targets)
                                                             (current-buffer)))
       (user-error "No buffer chosen"))))

(defvar atelier-traveller--step nil
  "The stack walk in progress, as (WORKSPACE-ID CONTENT-IDS . INDEX), or nil.")

(defun atelier-traveller-step (delta)
  "Show the buffer DELTA places away in the current view's stack, wrapping.
Consecutive steps walk the stack's order from the first step: showing a
buffer moves it to the top of its stack, so a fresh order would only flip
between two buffers."
  (when (window-parameter nil 'window-side)
    (select-window (atelier-main-window)))
  (let* ((workspace (or (atelier-current-workspace) (user-error "No workspace is selected")))
         (workspace-id (atelier-workspace-id workspace))
         (entry (atelier-current-entry)))
    (unless (and (memq last-command '(atelier-traveller-step-forward
                                      atelier-traveller-step-backward))
                 (equal (car atelier-traveller--step) workspace-id)
                 (equal (nth (cddr atelier-traveller--step) (cadr atelier-traveller--step))
                        (and entry (atelier-entry-field entry :content-id))))
      (let ((ids (mapcar (lambda (content) (atelier-content-field content :id))
                         (and entry (atelier-entry-stack entry workspace)))))
        (unless ids (user-error "This view shows no workspace stack"))
        (setq atelier-traveller--step (cons workspace-id (cons ids 0)))))
    (pcase-let ((`(,_ ,ids . ,index) atelier-traveller--step))
      (when (< (length ids) 2) (user-error "This stack holds only this buffer"))
      (setq index (mod (+ index delta) (length ids)))
      (setcdr (cdr atelier-traveller--step) index)
      (atelier-traveller-show-content workspace-id (nth index ids)))))

(defun atelier-traveller-step-forward ()
  "Show the next buffer of the current view's stack."
  (interactive)
  (atelier-traveller-step 1))

(defun atelier-traveller-step-backward ()
  "Show the previous buffer of the current view's stack."
  (interactive)
  (atelier-traveller-step -1))

(provide 'atelier-traveller)
;;; atelier-traveller.el ends here

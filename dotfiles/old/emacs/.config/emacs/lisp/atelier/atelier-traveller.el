;;; atelier-traveller.el --- Travel to any workspace buffer -*- lexical-binding: t; -*-

;; Traveller lists every buffer a workspace owns, open or saved, as
;; "WORKSPACE | TYPE | NAME".  Typed words match anywhere in that text, each
;; allowing gaps between its letters, and the best matches come first: letters
;; next to each other, at word starts and in the NAME part score more, gaps
;; cost.  Equal matches, and the whole list before typing, go by recency: most
;; recently used first, saved-only buffers next and the current buffer last.
;; Locked segments lead the input as "WORKSPACE | " or "WORKSPACE | TYPE | ",
;; and the list keeps only buffers under them.  TAB fills in the next segment
;; of the highlighted buffer, dropping typed words that matched that segment;
;; further TABs, or Shift+TAB backward, fill in the next segment of each
;; following matching workspace or type, then each matching buffer.  While
;; TAB cycles, the list shows the values it goes through, the filled-in one
;; highlighted.  Any other key brings back the buffer list, keeps what was
;; filled in and then does its own work: typing searches under it, Enter
;; opens the highlighted buffer.  The input stays ordinary text, so Backspace
;; into a filled-in segment unlocks it.  Choosing a buffer first selects its
;; workspace, as the navigator does, then shows the buffer there.

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

(defun atelier-traveller-target (workspace type name &rest properties)
  "Return the target for NAME of TYPE in WORKSPACE, with PROPERTIES added.
Its :name-start is where NAME begins in its :label."
  (let ((label (atelier-buffer-qualified-name workspace type name)))
    (append (list :label label
                  :name-start (- (length label) (length name))
                  :segments (atelier-traveller-segments workspace type))
            properties)))

(defun atelier-traveller-targets ()
  "Return every buffer Traveller can reach, as plists with a :label."
  (let (targets held)
    (dolist (workspace (atelier-workspace-list))
      (dolist (content (atelier-workspace-contents workspace))
        (let* ((type (or (atelier-content-field content :type) 'buffer))
               (buffer (atelier-content-buffer workspace content))
               (buffer (and (buffer-live-p buffer) buffer)))
          (when buffer (push buffer held))
          (push (atelier-traveller-target
                 workspace type (atelier-content-base-name content type buffer)
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
            (push (atelier-traveller-target
                   (car owner) type
                   (atelier-buffer-strip-qualifier (buffer-name buffer) type)
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

(defvar-local atelier-traveller--prompt nil
  "Non-nil in the buffer reading Traveller's input.")

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

(defun atelier-traveller-word-starts (text)
  "Return a bool vector marking the letters of TEXT that start a word.
A letter starts a word at the start of TEXT, after a character that is
neither a letter nor a digit, and as a capital after a small letter."
  (let ((starts (make-bool-vector (length text) nil))
        (alnum '(Lu Ll Lt Lm Lo Nd Nl No))
        (before nil))
    (dotimes (index (length text))
      (let ((category (get-char-code-property (aref text index) 'general-category)))
        (aset starts index (or (= index 0)
                               (not (memq before alnum))
                               (and (eq before 'Ll) (eq category 'Lu))))
        (setq before category)))
    starts))

(defun atelier-traveller-word-score (word text starts name-start)
  "Return how well WORD's letters match TEXT in order, or nil when they do not.
STARTS marks TEXT's word starts.  Each matched letter scores 16, plus 12 when
it directly follows the previous one, 8 when it starts a word and 4 at or
after NAME-START.  Each gap costs 8, plus 1 for every skipped letter after
its first.  The best placement counts.  Like the matching, WORD ignores case
unless it holds a capital."
  (let* ((fold (string= word (downcase word)))
         (n (length text))
         (previous (make-vector n nil))
         (current (make-vector n nil))
         (best nil))
    ;; PREVIOUS holds, for each letter of TEXT, the best score of the word so far
    ;; ending on that letter; GAP the best score reaching letter J after a gap.
    (dotimes (i (length word))
      (let ((letter (aref word i))
            (gap nil))
        (fillarray current nil)
        (dotimes (j n)
          (when (and (> i 0) (>= j 2))
            (let ((opened (and (aref previous (- j 2)) (- (aref previous (- j 2)) 8)))
                  (kept (and gap (1- gap))))
              (setq gap (if (and opened kept) (max opened kept) (or opened kept)))))
          (when (if fold
                    (eq letter (downcase (aref text j)))
                  (eq letter (aref text j)))
            (let* ((adjacent (and (> i 0) (>= j 1) (aref previous (1- j))
                                  (+ (aref previous (1- j)) 12)))
                   (reach (cond ((= i 0) 0)
                                ((and adjacent gap) (max adjacent gap))
                                (t (or adjacent gap)))))
              (when reach
                (aset current j (+ reach 16
                                   (if (aref starts j) 8 0)
                                   (if (>= j name-start) 4 0)))))))
        (cl-rotatef previous current)))
    (dotimes (j n)
      (when-let* ((score (aref previous j)))
        (setq best (if best (max best score) score))))
    best))

(defun atelier-traveller-rank (input candidates name-start)
  "Return CANDIDATES best match for typed INPUT first, keeping their order on ties.
NAME-START returns where a candidate's NAME part begins."
  (if-let* ((words (atelier-traveller-words input)))
      (mapcar #'car
              (sort (mapcar (lambda (candidate)
                              (let ((starts (atelier-traveller-word-starts candidate))
                                    (name (funcall name-start candidate)))
                                (cons candidate
                                      (apply #'+ (mapcar (lambda (word)
                                                           (or (atelier-traveller-word-score
                                                                word candidate starts name)
                                                               0))
                                                         words)))))
                            candidates)
                    (lambda (a b) (> (cdr a) (cdr b)))))
    candidates))

(defun atelier-traveller-table (targets)
  "Complete TARGETS' labels, listing only those under the locked segments.
The list shows the best matches for the typed words first."
  (let ((name-starts (make-hash-table :test #'equal))
        (labels (mapcar (lambda (target)
                          (let ((label (copy-sequence (plist-get target :label))))
                            (when (plist-get target :saved)
                              (put-text-property 0 (length label) 'atelier-traveller-saved t label))
                            label))
                        targets)))
    (dolist (target targets)
      (puthash (plist-get target :label) (plist-get target :name-start) name-starts))
    (lambda (string predicate action)
      (if (eq action 'metadata)
          `(metadata (category . atelier-traveller)
                     ;; TARGETS come by recency, which settles equal matches.
                     (display-sort-function
                      . ,(lambda (candidates)
                           (if atelier-traveller--cycle
                               candidates
                             ;; STRING stops at the cursor, but every typed word filters.
                             (let* ((input (if atelier-traveller--prompt
                                               (minibuffer-contents-no-properties)
                                             string))
                                    (locked (atelier-traveller-locked input targets)))
                               (atelier-traveller-rank
                                (substring input (length locked)) candidates
                                (lambda (candidate)
                                  (- (gethash (concat locked (substring-no-properties candidate))
                                              name-starts 0)
                                     (length locked))))))))
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
  (setq-local atelier-traveller--prompt t)
  (add-hook 'pre-command-hook #'atelier-traveller-end-cycle nil t))

(defun atelier-traveller-read (targets)
  "Choose one of TARGETS by label, matching letters with gaps."
  (let* ((atelier-traveller--targets targets)
         (choice (atelier-traveller-with-matching
                  (lambda ()
                    ;; Appended so it runs after the completion list installs its keys.
                    (minibuffer-with-setup-hook (:append #'atelier-traveller-setup-prompt)
                      (completing-read "Traveller: " (atelier-traveller-table targets)
                                       nil t)))
                  t)))
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

(defun atelier-traveller-cycle-entry (input)
  "Return the entry the list shows for cycle INPUT.
That is the workspace or type INPUT fills in, or the whole label at the
buffer level."
  (if (cl-find input atelier-traveller--targets
               :key (lambda (target) (plist-get target :label)) :test #'equal)
      input
    (car (last (split-string (atelier-traveller-locked input atelier-traveller--targets)
                             (regexp-quote atelier-buffer-name-separator) t)))))

(defun atelier-traveller-cycle-try (&rest _)
  "Leave completing the input to the next completion style."
  nil)

(defun atelier-traveller-cycle-all (&rest _)
  "Return the values of the TAB cycle in progress, or nil when none is."
  (when atelier-traveller--cycle
    (nconc (mapcar #'atelier-traveller-cycle-entry (car atelier-traveller--cycle)) 0)))

(add-to-list 'completion-styles-alist
             '(atelier-traveller-cycle atelier-traveller-cycle-try atelier-traveller-cycle-all
               "List the values a Traveller TAB cycle goes through."))

(defvar atelier-traveller-list-refresh-function nil
  "Function recomputing the completion list now, called with an entry or nil.
It highlights the entry when one is given.  A completion interface supplies
it, so the list switches between buffers and cycle values as soon as a cycle
starts, moves or ends.  Without one, the list keeps showing buffers while
TAB cycles.")

(defun atelier-traveller-refresh-list (entry)
  "Recompute the completion list now, highlighting ENTRY when non-nil.
A failure is reported without stopping the command that asked for it."
  (when atelier-traveller-list-refresh-function
    (with-demoted-errors "Traveller could not refresh its list: %S"
      (funcall atelier-traveller-list-refresh-function entry))))

(defvar atelier-traveller-highlighted-function nil
  "Function returning the full label the completion list highlights, or nil.
A completion interface supplies it; without one, TAB starts from the first
match.")

(defun atelier-traveller-matching-labels (input)
  "Return the full labels matching INPUT, in the order the list shows them."
  (let* ((table (atelier-traveller-table atelier-traveller--targets))
         (matches (atelier-traveller-with-matching
                   (lambda () (completion-all-completions input table nil (length input)))))
         (base (substring input 0 (car (completion-boundaries input table nil ""))))
         (rank (completion-metadata-get (completion-metadata input table nil)
                                        'display-sort-function)))
    (when matches (setcdr (last matches) nil))
    (mapcar (lambda (match) (concat base (substring-no-properties match)))
            (funcall rank matches))))

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
      (atelier-traveller-replace-input (nth index inputs))
      (atelier-traveller-refresh-list (atelier-traveller-cycle-entry (nth index inputs))))))

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
  "Keep what a TAB cycle filled in once any other command runs.
The list shows buffers again before that command acts on it."
  (unless (or (null atelier-traveller--cycle)
              (memq this-command '(atelier-traveller-cycle-forward
                                   atelier-traveller-cycle-backward)))
    (setq atelier-traveller--cycle nil)
    (atelier-traveller-refresh-list nil)))

;; Emacs's own flex style sorts the list by its own score; this copy leaves
;; the order to Traveller's table.
(add-to-list 'completion-styles-alist
             '(atelier-traveller-flex completion-flex-try-completion
               completion-flex-all-completions
               "Match letters in order with gaps, keeping the table's order."))

(defun atelier-traveller-with-matching (function &optional cycle)
  "Call FUNCTION with each typed word matching letters in order, with gaps.
With CYCLE, a TAB cycle in progress lists its values instead, when a
completion interface can refresh the list as the cycle moves."
  (let ((completion-styles
         (append (and cycle atelier-traveller-list-refresh-function '(atelier-traveller-cycle))
                 (if (featurep 'orderless) '(orderless) '(atelier-traveller-flex))))
        (orderless-matching-styles '(orderless-flex)))
    (funcall function)))

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

;;; jum.el --- Side-by-side diffs in plain buffers -*- lexical-binding: t; -*-

;; Package-Requires: ((emacs "29.1"))
;; Keywords: tools, vc

;;; Commentary:

;; Jumel shows old and new texts as two plain read-only buffers side by
;; side, holding only the hunks.  Each changed line faces the line it
;; replaces, changed words are marked, lines wrap, and each file and hunk
;; starts on the same screen row on both sides.  There is no mode of its
;; own: the usual motions scroll both sides together, and closing either
;; side closes both and gives the windows back.  It needs only the git
;; program, to compute the hunks.

;;; Code:

(require 'cl-lib)
(require 'subr-x)
(require 'diff-mode)

(defvar-local jumel--peer nil
  "The other side of this review.")
(defvar-local jumel--anchors nil
  "Positions of this side's file headers and hunk starts.
Each one faces the same anchor of the peer.")
(defvar-local jumel--restore nil
  "Window configuration to restore once this review closes.")
(defvar-local jumel--sides nil
  "The review's buffers, as (LEFT . RIGHT).")
(defvar-local jumel--refresh nil
  "Function returning the review's entries anew, or nil.")
(defvar-local jumel--title nil
  "The title naming this review's buffers.")
(defvar jumel--following nil)

(defvar jumel-buffer-hook nil
  "Hook run in each side of a new review, for example to add local keys.")

(defvar jumel-arrange-function #'jumel-arrange-frame
  "Function putting a review on screen, called with SIDE, WINDOW and OLD.
SIDE is to show in WINDOW, which showed OLD (or nil) before, with the other
side beside it in its place: the left side on the left.  The function
selects SIDE's window.  A window manager can replace it to place reviews
its own way.")

(defvar jumel-leave-function #'jumel-leave-frame
  "Function taking a review off screen, called with PEER-WINDOW, RESTORE, NEW.
One side left the screen: PEER-WINDOW, when live, still shows the other
side; RESTORE is what `jumel-arrange-frame' saved, or nil; NEW is the
buffer that replaced the side, or nil when the review was closed.")

(defun jumel--fontified (path text)
  "Return TEXT with the syntax colours of the mode PATH's name selects."
  (with-temp-buffer
    (insert text)
    (setq-local buffer-file-name path)
    (set-auto-mode)
    (font-lock-ensure)
    (prog1 (buffer-substring (point-min) (point-max))
      ;; A modified buffer visiting a file asks before being killed.
      (set-buffer-modified-p nil)
      (setq buffer-file-name nil))))

(defun jumel--hunks (old new)
  "Return the hunks of Git's diff from text OLD to text NEW.
Each hunk is (OLD-START NEW-START LINES), LINES being its diff lines."
  (let ((directory (make-temp-file "jumel-" t)))
    (unwind-protect
        (let ((default-directory directory)
              (coding-system-for-write 'utf-8-unix)
              (coding-system-for-read 'utf-8-unix)
              hunks)
          (write-region old nil "a" nil 'silent)
          (write-region new nil "b" nil 'silent)
          (with-temp-buffer
            (process-file "git" nil t nil "diff" "--no-index" "--no-color"
                          "--no-ext-diff" "-U3" "a" "b")
            (goto-char (point-min))
            (while (re-search-forward
                    "^@@ -\\([0-9]+\\)\\(?:,[0-9]+\\)? \\+\\([0-9]+\\)\\(?:,[0-9]+\\)? @@.*\n"
                    nil t)
              (let ((old-start (string-to-number (match-string 1)))
                    (new-start (string-to-number (match-string 2)))
                    lines)
                (while (memq (char-after) '(?\s ?- ?+ ?\\))
                  (unless (eq (char-after) ?\\)
                    (push (buffer-substring (point) (line-end-position)) lines))
                  (forward-line 1))
                (push (list old-start new-start (nreverse lines)) hunks))))
          (nreverse hunks))
      (delete-directory directory t))))

(defun jumel--rows (hunk)
  "Pair HUNK's lines into rows (OLD NEW CHANGED).
OLD and NEW are line numbers, or nil for filler.  Removed lines face the
added lines that follow them, in order."
  (let ((old (nth 0 hunk)) (new (nth 1 hunk)) removed added rows)
    (cl-flet ((flush ()
                (dotimes (i (max (length removed) (length added)))
                  (push (list (nth i removed) (nth i added) t) rows))
                (setq removed nil added nil)))
      (dolist (line (nth 2 hunk))
        (pcase (aref line 0)
          (?- (setq removed (append removed (list old)) old (1+ old)))
          (?+ (setq added (append added (list new)) new (1+ new)))
          (_ (flush)
             (push (list old new nil) rows)
             (setq old (1+ old) new (1+ new)))))
      (flush))
    (nreverse rows)))

(defun jumel--tokens (line)
  "Split LINE into words, runs of spaces and single other characters.
Each token is (TEXT START END)."
  (let ((start 0) tokens)
    (while (string-match "[[:alnum:]_]+\\|[[:space:]]+\\|." line start)
      (push (list (match-string 0 line) (match-beginning 0) (match-end 0)) tokens)
      (setq start (match-end 0)))
    (vconcat (nreverse tokens))))

(defun jumel--word-changes (old new)
  "Return (OLD-RANGES NEW-RANGES), the parts of lines OLD and NEW that differ.
Ranges are (START END).  Lines sharing no token, or too long to compare
quickly, return nil: a whole-line highlight would only repeat the row's."
  (let* ((a (jumel--tokens old))
         (b (jumel--tokens new))
         (n (length a))
         (m (length b)))
    (when (<= (* n m) 250000)
      (let ((common (make-vector (1+ n) nil)))
        (dotimes (i (1+ n)) (aset common i (make-vector (1+ m) 0)))
        (cl-loop for i from (1- n) downto 0 do
                 (cl-loop for j from (1- m) downto 0 do
                          (aset (aref common i) j
                                (if (equal (car (aref a i)) (car (aref b j)))
                                    (1+ (aref (aref common (1+ i)) (1+ j)))
                                  (max (aref (aref common (1+ i)) j)
                                       (aref (aref common i) (1+ j)))))))
        (when (> (aref (aref common 0) 0) 0)
          (let ((i 0) (j 0) old-ranges new-ranges)
            (while (or (< i n) (< j m))
              (cond ((and (< i n) (< j m) (equal (car (aref a i)) (car (aref b j))))
                     (setq i (1+ i) j (1+ j)))
                    ((and (< i n) (or (= j m) (>= (aref (aref common (1+ i)) j)
                                                  (aref (aref common i) (1+ j)))))
                     (push (cdr (aref a i)) old-ranges)
                     (setq i (1+ i)))
                    (t (push (cdr (aref b j)) new-ranges)
                       (setq j (1+ j)))))
            (list (nreverse old-ranges) (nreverse new-ranges))))))))

(defun jumel--insert-row (text file line &optional background ranges word-background)
  "Insert TEXT as the row showing LINE of FILE, LINE being nil for filler.
BACKGROUND goes behind the row and WORD-BACKGROUND behind RANGES, both
under the syntax colours, which keep their foreground."
  (let ((start (point)))
    (insert (or text "") "\n")
    (add-text-properties start (point) (list 'jumel-file file 'jumel-line line))
    (when background
      (add-face-text-property start (point) `(:background ,background :extend t) t))
    (dolist (range ranges)
      (add-face-text-property (+ start (car range)) (+ start (cadr range))
                              `(:background ,word-background)))))

(defun jumel--insert-anchor (left right left-text right-text face file left-line right-line)
  "Insert a line starting a block on LEFT and RIGHT, and record it as anchor.
The line leads to LEFT-LINE and RIGHT-LINE of FILE."
  (dolist (side (list (list left left-text left-line) (list right right-text right-line)))
    (with-current-buffer (nth 0 side)
      (push (point) jumel--anchors)
      (jumel--insert-row (propertize (nth 1 side) 'face face) file (nth 2 side)))))

(defun jumel--hunk-label (start)
  "Label a hunk starting at line START; Git numbers an empty side 0."
  (if (zerop start) "··· empty" (format "··· line %d" start)))

(defun jumel--fill (left right entries)
  "Fill LEFT and RIGHT with the hunks of ENTRIES, (PATH OLD-TEXT NEW-TEXT FILE)."
  (let ((removed (face-background 'diff-removed nil t))
        (added (face-background 'diff-added nil t))
        (word-removed (face-background 'diff-refine-removed nil t))
        (word-added (face-background 'diff-refine-added nil t))
        (inhibit-read-only t))
    (dolist (entry entries)
      (let* ((path (nth 0 entry))
             (old-lines (vconcat (split-string (jumel--fontified path (nth 1 entry)) "\n")))
             (new-lines (vconcat (split-string (jumel--fontified path (nth 2 entry)) "\n")))
             (hunks (jumel--hunks (nth 1 entry) (nth 2 entry)))
             (file (nth 3 entry))
             (header (format "===== %s =====" path)))
        (cl-flet ((line (lines number)
                    (and number (<= number (length lines)) (aref lines (1- number)))))
          (jumel--insert-anchor left right header header 'font-lock-keyword-face file 1 1)
          (unless hunks
            (dolist (buffer (list left right))
              (with-current-buffer buffer
                (jumel--insert-row
                 (propertize "No text changes (binary or identical content)" 'face 'shadow)
                 file 1))))
          (dolist (hunk hunks)
            (jumel--insert-anchor
             left right
             (jumel--hunk-label (nth 0 hunk)) (jumel--hunk-label (nth 1 hunk))
             'shadow file (max 1 (nth 0 hunk)) (max 1 (nth 1 hunk)))
            (dolist (row (jumel--rows hunk))
              (let* ((old (line old-lines (nth 0 row)))
                     (new (line new-lines (nth 1 row)))
                     (changed (nth 2 row))
                     (words (and old new changed
                                 (jumel--word-changes
                                  (substring-no-properties old) (substring-no-properties new)))))
                (with-current-buffer left
                  (jumel--insert-row old file (nth 0 row) (and old changed removed)
                                     (nth 0 words) word-removed))
                (with-current-buffer right
                  (jumel--insert-row new file (nth 1 row) (and new changed added)
                                     (nth 1 words) word-added))))))))
    (dolist (buffer (list left right))
      (with-current-buffer buffer
        (setq jumel--anchors (nreverse jumel--anchors))
        (set-buffer-modified-p nil)
        (goto-char (point-min))))))

(defun jumel--align (&optional window)
  "Pad the shorter side of each block so the next block faces its peer.
Wrapped lines make a block taller on one side; rows inside it may drift.
WINDOW, when given, shows one of the sides."
  (let* ((left (if window (window-buffer window) (current-buffer)))
         (right (buffer-local-value 'jumel--peer left))
         (left-window (get-buffer-window left))
         (right-window (and (buffer-live-p right) (get-buffer-window right))))
    (when (and left-window right-window)
      (cl-flet ((heights (buffer window)
                  (with-current-buffer buffer
                    (remove-overlays nil nil 'jumel--pad t)
                    (cl-loop for (start end) on jumel--anchors
                             when end
                             collect (cons end (count-screen-lines start end nil window)))))
                (pad (buffer position rows)
                  (with-current-buffer buffer
                    (let ((overlay (make-overlay position position)))
                      (overlay-put overlay 'jumel--pad t)
                      (overlay-put overlay 'before-string (make-string rows ?\n))))))
        (cl-loop for (left-end . left-height) in (heights left left-window)
                 for (right-end . right-height) in (heights right right-window)
                 do (cond ((< left-height right-height)
                           (pad left left-end (- right-height left-height)))
                          ((< right-height left-height)
                           (pad right right-end (- left-height right-height)))))))))

(defun jumel--follow (window &optional _start)
  "Put the peer of WINDOW's buffer on WINDOW's cursor row and top row."
  (unless jumel--following
    (with-current-buffer (window-buffer window)
      (when-let* ((peer jumel--peer)
                  ((buffer-live-p peer))
                  (peer-window (get-buffer-window peer)))
        (let ((jumel--following t)
              (row (line-number-at-pos (window-start window)))
              (point-row (line-number-at-pos (window-point window)))
              (column (save-excursion (goto-char (window-point window)) (current-column))))
          (with-current-buffer peer
            (save-excursion
              (goto-char (point-min))
              (forward-line (1- point-row))
              (move-to-column column)
              (set-window-point peer-window (point))
              (unless (= row (line-number-at-pos (window-start peer-window)))
                (goto-char (point-min))
                (forward-line (1- row))
                (set-window-start peer-window (point))))))))))

(defun jumel--mirror ()
  "After a command in this side, put the peer on the same rows."
  (when (eq (window-buffer) (current-buffer))
    (jumel--follow (selected-window))))

(defun jumel--refill (sides entries)
  "Replace the content of SIDES, (LEFT . RIGHT), with the hunks of ENTRIES."
  (dolist (buffer (list (car sides) (cdr sides)))
    (with-current-buffer buffer
      (let ((inhibit-read-only t)) (erase-buffer))
      (setq jumel--anchors nil)))
  (jumel--fill (car sides) (cdr sides) entries)
  (with-current-buffer (car sides) (jumel--align)))

(defun jumel--revert (&rest _)
  "Fill both sides again with fresh entries, staying on the same rows.
The sides stay read-only; only this rebuild writes to them."
  (unless jumel--refresh (user-error "This review has nothing to reload from"))
  (let* ((entries (funcall jumel--refresh))
         (sides jumel--sides)
         (window (get-buffer-window (current-buffer)))
         (row (and window (line-number-at-pos (window-start window))))
         (point-row (line-number-at-pos (point)))
         (column (current-column)))
    (jumel--refill sides entries)
    (goto-char (point-min))
    (forward-line (1- point-row))
    (move-to-column column)
    (when window
      (set-window-point window (point))
      (set-window-start window (save-excursion (goto-char (point-min))
                                               (forward-line (1- row))
                                               (point)))
      (jumel--follow window))
    (message "Reloaded: %d changed file%s" (length entries)
             (if (= (length entries) 1) "" "s"))))

(defun jumel-buffer-p (buffer)
  "Return non-nil when BUFFER is a side of a review."
  (and (buffer-live-p buffer) (buffer-local-value 'jumel--sides buffer) t))

(defun jumel--close ()
  "Close the other side too, then give back the windows the review took."
  (let* ((peer jumel--peer)
         (configuration jumel--restore)
         (peer-window (and (buffer-live-p peer) (get-buffer-window peer)))
         (shown (or (get-buffer-window (current-buffer)) peer-window)))
    (setq jumel--peer nil jumel--restore nil)
    (when (buffer-live-p peer)
      (with-current-buffer peer
        (setq jumel--peer nil jumel--restore nil))
      (kill-buffer peer))
    ;; Closing runs inside the command that kills the buffer, which may
    ;; still be rearranging windows; leave once it has finished.  A review
    ;; no longer on screen has nothing to give back.
    (when shown
      (run-at-time 0 nil jumel-leave-function peer-window configuration nil))))

(defun jumel--buffer (name title)
  "Return a new empty review side called NAME, for the review TITLE."
  (with-current-buffer (generate-new-buffer name)
    (fundamental-mode)
    (font-lock-mode -1)
    ;; Wrap even in a narrow half: by default Emacs truncates lines in
    ;; windows under 50 columns.
    (setq-local truncate-lines nil
                truncate-partial-width-windows nil
                buffer-read-only t
                buffer-offer-save nil
                jumel--title title)
    (setq-local revert-buffer-function #'jumel--revert)
    (add-hook 'kill-buffer-hook #'jumel--close nil t)
    (add-hook 'post-command-hook #'jumel--mirror nil t)
    (add-hook 'window-scroll-functions #'jumel--follow nil t)
    (add-hook 'window-size-change-functions #'jumel--align nil t)
    (run-hooks 'jumel-buffer-hook)
    (current-buffer)))

(defun jumel--previous (title)
  "Return the left side of the open review called TITLE, if any."
  (seq-find (lambda (buffer)
              (and (jumel-buffer-p buffer)
                   (equal (buffer-local-value 'jumel--title buffer) title)
                   (eq buffer (car (buffer-local-value 'jumel--sides buffer)))))
            (buffer-list)))

(defun jumel-arrange-frame (side window old)
  "Give WINDOW's frame to SIDE's review, the left side left, and select SIDE.
The windows as they were, with OLD back in WINDOW, are saved for leaving."
  (when (and (buffer-live-p old) (not (jumel-buffer-p old)))
    (set-window-buffer window old))
  (pcase-let ((configuration (current-window-configuration))
              (`(,left . ,right) (buffer-local-value 'jumel--sides side)))
    (dolist (buffer (list left right))
      (with-current-buffer buffer (setq jumel--restore configuration)))
    (delete-other-windows window)
    (set-window-buffer window left)
    (let ((right-window (split-window window nil 'right)))
      (set-window-buffer right-window right)
      (select-window (if (eq side right) right-window window)))
    (with-current-buffer left (jumel--align))))

(defun jumel-leave-frame (peer-window restore new)
  "Give back the windows RESTORE saved, showing NEW, or close PEER-WINDOW.
NEW goes in the selected window unless the restored windows already show it."
  (if (window-configuration-p restore)
      (progn
        (set-window-configuration restore)
        (when (buffer-live-p new)
          (if-let* ((window (get-buffer-window new)))
              (select-window window)
            (set-window-buffer (selected-window) new))))
    (when (and (window-live-p peer-window) (not (one-window-p nil (window-frame peer-window))))
      (delete-window peer-window))))

;; Atelier and other window managers move buffers one window at a time, so
;; one side could stay on screen next to an unrelated buffer.  The two sides
;; leave and return together instead.

(defun jumel--window-change (frame)
  "Keep the two sides of each review on FRAME's screen together."
  (dolist (window (window-list frame 'nomini))
    (let ((old (window-old-buffer window))
          (new (window-buffer window)))
      (unless (or (eq old new) (window-parameter window 'window-side))
        (cond
         ((and (jumel-buffer-p old)
               (not (eq new (buffer-local-value 'jumel--peer old)))
               (get-buffer-window (buffer-local-value 'jumel--peer old) frame))
          (run-at-time 0 nil #'jumel--leave old new))
         ((and (jumel-buffer-p new)
               (not (get-buffer-window (buffer-local-value 'jumel--peer new) frame)))
          (run-at-time 0 nil #'jumel--return new window old)))))))

(defun jumel--leave (side new)
  "SIDE left its window for NEW: take its peer off screen too."
  (when-let* (((jumel-buffer-p side))
              ((not (get-buffer-window side)))
              (peer-window (get-buffer-window (buffer-local-value 'jumel--peer side))))
    (funcall jumel-leave-function peer-window (buffer-local-value 'jumel--restore side) new)))

(defun jumel--return (side window old)
  "SIDE came back in WINDOW in place of OLD: bring its peer back beside it."
  (when-let* (((jumel-buffer-p side))
              ((window-live-p window))
              ((eq (window-buffer window) side))
              ((not (get-buffer-window (buffer-local-value 'jumel--peer side)))))
    (funcall jumel-arrange-function side window old)))

(defun jumel--nearest-line (file)
  "Return the line number of FILE's nearest numbered row below point, else above."
  (cl-flet ((scan (step)
              (save-excursion
                (cl-loop while (and (zerop (forward-line step)) (not (eobp))
                                    (equal (get-text-property (point) 'jumel-file) file))
                         thereis (get-text-property (point) 'jumel-line)))))
    (or (scan 1) (scan -1))))

(defun jumel-visit ()
  "Open the file of the row at point, at the line the new side shows there.
On the old side that is the facing line; a deleted line leads to where it
was removed."
  (interactive)
  (let ((file (get-text-property (line-beginning-position) 'jumel-file))
        (row (line-number-at-pos))
        (column (current-column)))
    (unless (and file (file-exists-p file))
      (user-error "No file to open here"))
    (let ((line (with-current-buffer (cdr jumel--sides)
                  (save-excursion
                    (goto-char (point-min))
                    (forward-line (1- row))
                    (or (get-text-property (point) 'jumel-line)
                        (jumel--nearest-line file)
                        1)))))
      (find-file file)
      (goto-char (point-min))
      (forward-line (1- line))
      (move-to-column column))))

(defun jumel-show (title entries &optional refresh)
  "Show ENTRIES side by side in buffers named \"before TITLE\" and \"after TITLE\".
ENTRIES are (PATH OLD-TEXT NEW-TEXT FILE): PATH heads the file's hunks and
selects its syntax colours, and FILE, when non-nil, is what `jumel-visit'
opens.  REFRESH, when given, returns the entries anew for `revert-buffer'.
An open review with the same TITLE is refilled and shown again.
`jumel-arrange-function' places the review, and its two sides leave and
return together."
  (let* ((previous (jumel--previous title))
         (sides (or (and previous (buffer-local-value 'jumel--sides previous))
                    (cons (jumel--buffer (format "before %s" title) title)
                          (jumel--buffer (format "after %s" title) title)))))
    (pcase-let ((`(,left . ,right) sides))
      (dolist (side (list (cons left right) (cons right left)))
        (with-current-buffer (car side)
          (setq jumel--peer (cdr side)
                jumel--sides sides
                jumel--refresh refresh)))
      (jumel--refill sides entries)
      (add-hook 'window-buffer-change-functions #'jumel--window-change)
      (let ((left-window (get-buffer-window left)))
        (if (and left-window (get-buffer-window right))
            (select-window left-window)
          (funcall jumel-arrange-function left (or left-window (selected-window)) nil)))
      (message "%s diff: %d changed file%s" title (length entries)
               (if (= (length entries) 1) "" "s")))))

(provide 'jum)
;;; jum.el ends here

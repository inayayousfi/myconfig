;;; test-emacs-diff.el --- Side-by-side Git review -*- lexical-binding: t; -*-

(require 'ert)
(require 'cl-lib)
(let ((lisp (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/"
                              (file-name-directory (or load-file-name buffer-file-name)))))
  (add-to-list 'load-path lisp)
  (add-to-list 'load-path (expand-file-name "atelier" lisp)))
(require 'myconfig-git)

(defun myconfig-diff-test-backgrounds (buffer text &optional offset)
  "Return the backgrounds at TEXT's first match in BUFFER, plus OFFSET.
The first one is what shows."
  (with-current-buffer buffer
    (save-excursion
      (goto-char (point-min))
      (search-forward text)
      (let ((face (get-text-property (+ (match-beginning 0) (or offset 0)) 'face)))
        (delq nil (mapcar (lambda (spec) (and (consp spec) (plist-get spec :background)))
                          (if (or (atom face) (keywordp (car face))) (list face) face)))))))

(defun myconfig-diff-test-row (buffer text)
  (with-current-buffer buffer
    (save-excursion
      (goto-char (point-min))
      (search-forward text)
      (line-number-at-pos))))

(defun myconfig-diff-test-screen-row (buffer text)
  "Return the screen row, counted from the top, where TEXT's line starts.
Rows are the wrapped rows of the text above it plus the empty rows that
padding inserts up to it; Emacs's own row counting skips the latter."
  (with-current-buffer buffer
    (save-excursion
      (goto-char (point-min))
      (search-forward text)
      (let ((target (line-beginning-position)))
        (+ (count-screen-lines (point-min) target nil (get-buffer-window buffer))
           (cl-loop for overlay in (overlays-in (point-min) (1+ target))
                    when (<= (overlay-start overlay) target)
                    sum (cl-count ?\n (or (overlay-get overlay 'before-string) ""))))))))

;; Given a commit and then a changed word, a long added line, a deleted line
;; and an untracked file, the working review shows only the hunks side by
;; side, each row facing its counterpart, each hunk starting on the same screen
;; row despite wrapping, the changed word marked, and closing one side closes
;; both and gives the windows back.
(ert-deftest myconfig-review-shows-facing-hunks-and-closes-both ()
  (let* ((root (file-name-as-directory (file-truename (make-temp-file "review-" t))))
         (file (expand-file-name "f.el" root))
         (long (concat "(setq long \"" (make-string 200 ?x) "\")"))
         (buffers (buffer-list))
         (default-directory root))
    (cl-flet ((git (&rest arguments)
                (should (zerop (apply #'call-process "git" nil nil nil
                                      "-c" "user.name=test" "-c" "user.email=test@example.invalid"
                                      "-c" "commit.gpgsign=false" arguments))))
              (lines (numbers)
                (mapconcat (lambda (n) (if (stringp n) (concat n "\n") (format "(setq line-%d %d)\n" n n)))
                           numbers "")))
      (unwind-protect
          (save-window-excursion
            (delete-other-windows)
            (git "init" "-q")
            (write-region (lines (number-sequence 1 30)) nil file)
            (git "add" "f.el")
            (git "commit" "-q" "-m" "start")
            (write-region (lines (append '(1 2 3 4 "(setq line-5 50)") (number-sequence 6 17)
                                         (list long) (number-sequence 18 27) '(29 30)))
                          nil file)
            (write-region "(new)\n" nil (expand-file-name "new.el" root))
            (myconfig-git-review-working)
            (let ((left (get-buffer "before working"))
                  (right (get-buffer "after working")))
              (should (equal (mapcar #'window-buffer (window-list)) (list left right)))
              ;; Only the hunks: line 12 is more than 3 lines from any change.
              (should (with-current-buffer left (string-search "(setq line-2 2)" (buffer-string))))
              (should-not (with-current-buffer left (string-search "line-12" (buffer-string))))
              ;; The changed line faces its replacement, and only the changed
              ;; word is marked inside the row's background.
              (should (= (myconfig-diff-test-row left "(setq line-5 5)")
                         (myconfig-diff-test-row right "(setq line-5 50)")))
              (should (equal (car (myconfig-diff-test-backgrounds right "(setq line-5 50)" 13))
                             (face-background 'diff-refine-added nil t)))
              (should (equal (car (myconfig-diff-test-backgrounds right "(setq line-5 50)" 1))
                             (face-background 'diff-added nil t)))
              (should (equal (car (myconfig-diff-test-backgrounds left "(setq line-5 5)" 1))
                             (face-background 'diff-removed nil t)))
              (should-not (myconfig-diff-test-backgrounds left "(setq line-4 4)" 1))
              ;; A deleted line faces an empty filler row; an untracked file is
              ;; all added, facing filler.
              (with-current-buffer right
                (goto-char (point-min))
                (forward-line (1- (myconfig-diff-test-row left "(setq line-28 28)")))
                (should (looking-at-p "$")))
              (should (= (myconfig-diff-test-row left "===== new.el =====")
                         (myconfig-diff-test-row right "===== new.el =====")))
              (should (string-search "(new)" (with-current-buffer right (buffer-string))))
              (should (string-search "··· empty" (with-current-buffer left (buffer-string))))
              ;; The long line wraps on the right only, yet the next hunk and
              ;; the next file start on the same screen row on both sides.
              (should (> (with-current-buffer right
                           (count-screen-lines (point-min) (point-max) nil
                                               (get-buffer-window right)))
                         (with-current-buffer right (count-lines (point-min) (point-max)))))
              (should (= (myconfig-diff-test-screen-row left "··· line 25")
                         (myconfig-diff-test-screen-row right "··· line 26")))
              (should (= (myconfig-diff-test-screen-row left "===== new.el =====")
                         (myconfig-diff-test-screen-row right "===== new.el =====")))
              ;; Scrolling one side brings the other to the same row.
              (let ((left-window (get-buffer-window left))
                    (right-window (get-buffer-window right)))
                (with-current-buffer left
                  (goto-char (point-min))
                  (forward-line 9)
                  (set-window-point left-window (point))
                  (set-window-start left-window (point))
                  (run-hook-with-args 'window-scroll-functions left-window (point)))
                (should (= (with-current-buffer right
                             (line-number-at-pos (window-start right-window)))
                           10)))
;; Moving the cursor on one side puts the other side's cursor on
              ;; the same row and column.
              (with-selected-window (get-buffer-window left)
                (goto-char (point-min))
                (forward-line 4)
                (forward-char 3)
                (run-hooks 'post-command-hook))
              (with-current-buffer right
                (save-excursion
                  (goto-char (window-point (get-buffer-window right)))
                  (should (= (line-number-at-pos) 5))
                  (should (= (current-column) 3))))
              ;; Reloading rebuilds both read-only sides from what Git shows
              ;; now, keeping the cursor row.
              (write-region (lines (append '(1 2 3 4 "(setq line-5 500)") (number-sequence 6 17)
                                           (list long) (number-sequence 18 27) '(29 30)))
                            nil file)
              (with-selected-window (get-buffer-window right)
                (revert-buffer)
                (should (= (line-number-at-pos) 5)))
              (should (string-search "(setq line-5 500)" (with-current-buffer right (buffer-string))))
              (should (= (myconfig-diff-test-row left "(setq line-5 5)")
                         (myconfig-diff-test-row right "(setq line-5 500)")))
              (should (with-current-buffer left buffer-read-only))
              (should (with-current-buffer right buffer-read-only))
              (should (equal (mapcar #'window-buffer (window-list)) (list left right)))
              ;; Enter opens the file at the line the new side shows on that
              ;; row; a deleted line leads to where it was removed.
              (with-selected-window (get-buffer-window right)
                (goto-char (point-min))
                (search-forward "(setq line-5 500)")
                (jumel-visit)
                (should (equal buffer-file-name file))
                (should (= (line-number-at-pos) 5)))
              (with-selected-window (get-buffer-window left)
                (set-window-buffer nil left)
                (goto-char (point-min))
                (search-forward "(setq line-28 28)")
                (beginning-of-line)
                (jumel-visit)
                (should (equal buffer-file-name file))
                (should (looking-at-p "(setq line-29 29)"))
                (set-window-buffer nil left))
              (set-window-buffer (next-window (get-buffer-window left)) right)
              ;; Closing one side closes both, without a question, and gives
              ;; the single window back.
              (kill-buffer left)
              (should-not (buffer-live-p right))
              (accept-process-output nil 0.1)
              (should (= (length (window-list)) 1))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers) (kill-buffer buffer)))
        (delete-directory root t)))))

(ert-run-tests-batch-and-exit)

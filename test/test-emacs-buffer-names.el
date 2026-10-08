;;; test-emacs-buffer-names.el --- Workspace ownership and buffer names -*- lexical-binding: t; -*-

(require 'ert)
(require 'cl-lib)
(require 'ls-lisp)
;; The live configuration matches typed words separately through Orderless.
(require 'orderless)
(defgroup myconfig nil "Test configuration." :group 'environment)
(provide 'ghostel)
(let ((lisp (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/"
                              (file-name-directory (or load-file-name buffer-file-name)))))
  (add-to-list 'load-path lisp)
  (add-to-list 'load-path (expand-file-name "atelier" lisp)))
(require 'atelier-persist)
(require 'aipan)
(require 'aipanel-atelier)
;; Only the terminal emulator is absent in batch Emacs; panels are identified by its mode.
(unless (fboundp 'ghostel-mode)
  (define-derived-mode ghostel-mode fundamental-mode "Ghostel"))
(defvar-local ghostel-title nil)
(defvar ghostel-buffer-name-function nil)
(require 'ghostel-atelier)
(add-hook 'atelier-buffer-owner-functions #'aipanel-atelier-buffer-owner)
(add-hook 'atelier-traveller-open-functions #'aipanel-atelier-travel)

(defmacro atelier-names-test (&rest body)
  "Run BODY with an isolated workspace, Detached workspace, files and buffers.
The naming timer is not started; BODY runs its work with `atelier-name-buffers'."
  (declare (indent 0))
  `(let* ((root (make-temp-file "atelier-names-" t))
          (workspace (list :id "names-test" :name "work" :destination "local"
                           :path root :status 'running :entries nil))
          (atelier-workspaces (list workspace))
          (detached (atelier-ensure-detached-workspace))
          (atelier-content-live-buffers (make-hash-table :test #'equal))
          (atelier-entry-owners (make-hash-table :test #'eq))
          (atelier-change-hook nil)
          (kill-buffer-hook (cons #'atelier-current-buffer-killed kill-buffer-hook))
          (atelier-navigator-window-configurations nil)
          (atelier-navigator-selection-by-frame nil)
          (atelier-close-without-asking t)
          (ls-lisp-use-insert-directory-program nil)
          (old-selection (frame-parameter nil 'atelier-workspace-id))
          (old-buffers (buffer-list))
          ;; Buffer names are global: buffers from outside the test stay untouched.
          (atelier-internal-buffers
           (let ((table (make-hash-table :test #'eq)))
             (dolist (buffer old-buffers table) (puthash buffer t table)))))
     (ignore detached)
     (unwind-protect
         (save-window-excursion
           (delete-other-windows)
           (atelier-select-workspace workspace)
           (cl-letf (((symbol-function 'atelier-schedule-naming) #'ignore))
             ,@body))
       (set-frame-parameter nil 'atelier-workspace-id old-selection)
       (dolist (buffer (buffer-list))
         (unless (memq buffer old-buffers)
           (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer)))
       (delete-directory root t))))

(defun atelier-names-test-owner (buffer)
  (plist-get (car (atelier-buffer-owner buffer)) :name))

(ert-deftest atelier-names-qualify-workspace-buffers-and-follow-workspace-renames ()
  "Owned buffers read WORKSPACE | TYPE | NAME; star-named package buffers keep theirs."
  (atelier-names-test
    (let* ((file (expand-file-name "example.txt" root))
           (_ (with-temp-file file (insert "example")))
           (visited (atelier-open-file file workspace))
           (scratch (generate-new-buffer "*scratch*"))
           (scratch-name (buffer-name scratch))
           (help (get-buffer-create "*names-help*")))
      (atelier-register-buffer scratch workspace)
      (atelier-register-buffer help workspace)
      (atelier-name-buffers)
      (should (equal (buffer-name visited) "work | file | example.txt"))
      (should (equal (buffer-name scratch) (concat "work | buffer | " scratch-name)))
      (should (equal (buffer-name help) "*names-help*"))
      (should (equal (atelier-names-test-owner help) "work"))
      (setf (plist-get workspace :name) "renamed")
      (atelier-name-buffers)
      (should (equal (buffer-name visited) "renamed | file | example.txt"))
      (should (equal (buffer-name scratch) (concat "renamed | buffer | " scratch-name))))))

(ert-deftest atelier-names-keep-the-name-the-user-gives ()
  "A rename sets the NAME part; a typed qualifier is not repeated."
  (atelier-names-test
    (let* ((file (expand-file-name "example.txt" root))
           (_ (with-temp-file file (insert "example")))
           (visited (atelier-open-file file workspace)))
      (atelier-name-buffers)
      (with-current-buffer visited (rename-buffer "notes"))
      (atelier-name-buffers)
      (should (equal (buffer-name visited) "work | file | notes"))
      (should (equal (atelier-buffer-editable-name visited) "notes"))
      (with-current-buffer visited (rename-buffer "work | file | draft"))
      (atelier-name-buffers)
      (should (equal (buffer-name visited) "work | file | draft")))))

(ert-deftest atelier-names-give-unshown-buffers-to-detached ()
  "Buffers no workspace showed belong to Detached; Emacs's own logs stay outside."
  (atelier-names-test
    (let* ((file (expand-file-name "background.txt" root))
           (_ (with-temp-file file (insert "background")))
           (background (find-file-noselect file))
           (log (get-buffer-create "*names-log*"))
           (shown (generate-new-buffer "shown")))
      (switch-to-buffer shown)
      ;; Redisplay runs the window-change listener; batch Emacs does not redisplay.
      (atelier-record-changed-windows (selected-frame))
      (atelier-name-buffers)
      (should (equal (atelier-names-test-owner background) "Detached"))
      (should (equal (buffer-name background) "Detached | file | background.txt"))
      (should (equal (atelier-names-test-owner log) "Detached"))
      (should (equal (buffer-name log) "*names-log*"))
      (should (equal (atelier-names-test-owner shown) "work"))
      (should (equal (buffer-name shown) "work | buffer | shown"))
      (should-not (atelier-buffer-owner (get-buffer "*Messages*")))
      (should (get-buffer "*Messages*"))
      ;; Detached never stops, so killing its buffer must not keep it for a restart.
      (kill-buffer background)
      (should-not (cl-find-if (lambda (content) (equal (plist-get content :file) file))
                              (plist-get (atelier-detached-workspace) :contents))))))

(ert-deftest atelier-names-show-folder-for-directory-buffers ()
  "A directory browser is named after the folder it currently shows."
  (atelier-names-test
    (let* ((child (file-name-as-directory (expand-file-name "child" root)))
           (_ (make-directory child))
           (buffer (atelier-new-dired-buffer root t workspace)))
      (atelier-register-buffer buffer workspace)
      (atelier-name-buffers)
      (should (equal (buffer-name buffer)
                     (format "work | dired | %s" (file-name-nondirectory root))))
      (with-current-buffer buffer (atelier-dired-change-directory child))
      (atelier-name-buffers)
      (should (equal (buffer-name buffer) "work | dired | child")))))

(ert-deftest atelier-names-stay-stable-across-restored-and-legacy-names ()
  "Restored qualified names and old *TYPE:WORKSPACE* names are not qualified twice."
  (atelier-names-test
    (let ((restored (generate-new-buffer "old | terminal | build"))
          (legacy (generate-new-buffer "*terminal:work*"))
          (scratch (generate-new-buffer "work | buffer | *scratch*")))
      (atelier-register-buffer restored workspace nil 'terminal)
      (atelier-register-buffer legacy workspace nil 'terminal)
      (atelier-register-buffer scratch workspace)
      (atelier-name-buffers)
      (should (equal (buffer-name restored) "work | terminal | build"))
      (should (equal (buffer-name legacy) "work | terminal | terminal"))
      (should (equal (buffer-name scratch) "work | buffer | *scratch*"))
      (atelier-name-buffers)
      (should (equal (buffer-name legacy) "work | terminal | terminal")))))

(ert-deftest atelier-names-place-panels-in-their-source-workspace ()
  "An AI panel, outside every stack, is named after its source buffer's workspace."
  (atelier-names-test
    (let* ((file (expand-file-name "source.txt" root))
           (_ (with-temp-file file (insert "source")))
           (source (atelier-open-file file workspace))
           (panel (generate-new-buffer "*Claude Code*")))
      (with-current-buffer panel
        (ghostel-mode)
        (setq-local aipanel-owner (list :source-buffer source))
        (atelier-set-buffer-excluded t))
      (atelier-name-buffers)
      (should (equal (buffer-name panel) "work | aipanel | *Claude Code*"))
      (should-not (gethash panel (atelier-buffer-owner-index))))))

;; Ghostel calls `ghostel-buffer-name-function' with each terminal title.
(ert-deftest atelier-names-follow-terminal-titles ()
  "A terminal's NAME part follows its title until the user names it by hand."
  (atelier-names-test
    (let ((terminal (generate-new-buffer "terminal"))
          (atelier-buffer-kind-functions nil)
          (atelier-buffer-title-functions nil)
          (atelier-job-start-function nil)
          (atelier-job-process-id-function nil)
          (ghostel-buffer-name-function nil))
      (ghostel-atelier-setup)
      (cl-flet ((title (buffer value)
                  (with-current-buffer buffer
                    (setq ghostel-title value)
                    (should-not (funcall ghostel-buffer-name-function value)))))
        (with-current-buffer terminal (ghostel-mode))
        (atelier-register-buffer terminal workspace nil 'terminal)
        (atelier-name-buffers)
        (should (equal (buffer-name terminal) "work | terminal | terminal"))
        (title terminal "◐ Fix\nthe build")
        (should (equal (buffer-name terminal) "work | terminal | ◐ Fix the build"))
        (title terminal "◑ Fix the build")
        (should (equal (buffer-name terminal) "work | terminal | ◑ Fix the build"))
        (title terminal "")
        (should (equal (buffer-name terminal) "work | terminal | terminal"))
        (title terminal "◐ Fix the build")
        (with-current-buffer terminal (rename-buffer "notes"))
        (atelier-name-buffers)
        (title terminal "◑ Something else")
        (should (equal (buffer-name terminal) "work | terminal | notes"))
        (with-current-buffer terminal (ghostel-atelier-resume-title))
        (should (equal (buffer-name terminal) "work | terminal | ◑ Something else"))
        (let* ((file (expand-file-name "source.txt" root))
               (_ (with-temp-file file (insert "source")))
               (source (atelier-open-file file workspace))
               (panel (generate-new-buffer "*Claude Code*")))
          (with-current-buffer panel
            (ghostel-mode)
            (setq-local aipanel-owner (list :source-buffer source))
            (atelier-set-buffer-excluded t))
          (atelier-name-buffers)
          (should (equal (buffer-name panel) "work | aipanel | *Claude Code*"))
          (title panel "◐ Panel work")
          (should (equal (buffer-name panel) "work | aipanel | *Claude Code*")))))))

(defun butlast-completions (completions)
  "Return COMPLETIONS without the base size Emacs stores in the last cell."
  (when completions (setcdr (last completions) nil))
  completions)

(defun atelier-traveller-test-target (label)
  (or (cl-find label (atelier-traveller-targets)
               :key (lambda (target) (plist-get target :label)) :test #'equal)
      (error "No Traveller target %s" label)))

(defun atelier-traveller-test-stopped-workspace (root file)
  "Add a stopped workspace holding FILE only as saved content."
  (let ((other (list :id "other-test" :name "other" :destination "local"
                     :path root :status 'stopped :entries nil)))
    (setq atelier-workspaces (append atelier-workspaces (list other)))
    (atelier-workspace-store-content
     other (list :type 'file :kind 'file :persistent t :file file
                 :name (concat "other | file | " (file-name-nondirectory file))))
    other))

(ert-deftest atelier-navigator-shows-stack-type-once-before-bare-names ()
  "A stack row names its type once; its buffers show only their NAME part.
The workspace is already the row's heading."
  (atelier-names-test
    (let ((files (mapcar (lambda (name) (expand-file-name name root)) '("a.txt" "b.txt"))))
      (dolist (file files)
        (with-temp-file file (insert file))
        (atelier-open-file file workspace))
      (atelier-name-buffers)
      (should (get-buffer "work | file | a.txt"))
      (with-current-buffer (atelier-render-navigator)
        (goto-char (point-min))
        (should (search-forward "a.txt" nil t))
        (should (equal (buffer-substring-no-properties
                        (line-beginning-position) (line-end-position))
                       "     ├─ file      b.txt  ·  a.txt"))))))

(ert-deftest atelier-navigator-lists-every-detached-buffer ()
  "Detached shows every buffer of its stacks and Emacs's own logs; a log
opens in Detached without being stored."
  (atelier-names-test
    (let ((first (generate-new-buffer "*first-detached*"))
          (second (generate-new-buffer "*second-detached*")))
      (atelier-register-buffer first detached)
      (atelier-register-buffer second detached)
      (get-buffer-create "*Messages*")
      (atelier-name-buffers)
      (let ((contents (length (plist-get detached :contents))))
        (with-current-buffer (atelier-render-navigator)
          (goto-char (point-min))
          (search-forward "DETACHED BUFFERS")
          (forward-line 2)
          (should (looking-at-p
                   (regexp-quote "  •  buffer    *second-detached*  ·  *first-detached*")))
          (forward-line 1)
          (should (looking-at-p (regexp-quote "  •  logs      *Messages*")))
          (search-forward "*Messages*")
          (goto-char (match-beginning 0))
          (atelier-navigator-open))
        (should (eq (window-buffer) (get-buffer "*Messages*")))
        (should (eq (atelier-current-workspace) detached))
        (should (= contents (length (plist-get detached :contents))))))))

(ert-deftest atelier-navigator-detached-shows-each-stack-once-and-closes-buffers ()
  "Two Detached views of one stack give one row; x closes a stack buffer or a log."
  (atelier-names-test
    (let ((first (generate-new-buffer "*first-detached*"))
          (second (generate-new-buffer "*second-detached*")))
      (atelier-register-buffer first detached)
      (atelier-register-buffer second detached)
      (atelier-switch-workspace atelier-detached-workspace-name)
      (atelier-show-buffer first detached)
      (atelier-capture-current-workspace)
      (atelier-split-right)
      (atelier-show-buffer second detached)
      (atelier-capture-current-workspace)
      (get-buffer-create "*Messages*")
      (atelier-name-buffers)
      (cl-flet ((detached-section ()
                  (with-current-buffer (atelier-render-navigator)
                    (goto-char (point-min))
                    (search-forward "DETACHED BUFFERS")
                    (buffer-substring-no-properties
                     (line-beginning-position) (search-forward "＋ New detached")))))
        (let ((section (detached-section)))
          (should (string-match-p "BUFFERS  3 total" section))
          (should (= 1 (cl-count-if (lambda (line) (string-match-p "•  buffer" line))
                                    (split-string section "\n")))))
        (cl-letf (((symbol-function 'y-or-n-p) (lambda (&rest _) t)))
          (dolist (name '("*second-detached*" "*Messages*"))
            (atelier-navigator)
            (goto-char (point-min))
            (search-forward "DETACHED BUFFERS")
            (search-forward name)
            (goto-char (match-beginning 0))
            (atelier-navigator-close)
            (atelier-navigator-quit)))
        (should-not (buffer-live-p second))
        (should-not (get-buffer "*Messages*"))
        (should (buffer-live-p first))
        (should-not (string-match-p "second-detached" (detached-section)))))))

(ert-deftest atelier-traveller-lists-every-workspace-buffer ()
  "Open, saved, star-named, log and panel buffers appear as WORKSPACE | TYPE | NAME."
  (atelier-names-test
    (let* ((file (expand-file-name "example.txt" root))
           (_ (with-temp-file file (insert "example")))
           (visited (atelier-open-file file workspace))
           (help (get-buffer-create "*names-help*"))
           (panel (generate-new-buffer "*Claude Code*")))
      (atelier-register-buffer help workspace)
      (with-current-buffer panel
        (ghostel-mode)
        (setq-local aipanel-owner (list :source-buffer visited))
        (atelier-set-buffer-excluded t))
      (atelier-traveller-test-stopped-workspace root (expand-file-name "saved.txt" root))
      (atelier-name-buffers)
      (let ((saved (mapcar (lambda (target)
                             (cons (plist-get target :label) (plist-get target :saved)))
                           (atelier-traveller-targets))))
        (should (equal (assoc "work | file | example.txt" saved) '("work | file | example.txt")))
        (should (equal (assoc "other | file | saved.txt" saved) '("other | file | saved.txt" . t)))
        (should (assoc "work | buffer | *names-help*" saved))
        (should (assoc "Detached | buffer | *Messages*" saved))
        (should (assoc "work | aipanel | *Claude Code*" saved))
        (should-not (assoc "Detached | buffer | *Completions*" saved))))))

(ert-deftest atelier-traveller-matches-letters-with-gaps ()
  "A typed word matches when its letters appear in order, with gaps."
  (atelier-traveller-with-matching
   (lambda ()
     (let* ((labels '("myconfig | file | init.el" "notes | terminal | terminal"))
            (matches (completion-all-completions "mcfgini" labels nil 7)))
       (when matches (setcdr (last matches) nil))
       (should (equal (mapcar #'substring-no-properties matches)
                      '("myconfig | file | init.el")))))))

(ert-deftest atelier-traveller-lists-only-buffers-under-locked-segments ()
  "A locked workspace lists only its buffers, shown without the locked part."
  (atelier-names-test
    (let ((file (expand-file-name "example.txt" root)))
      (with-temp-file file (insert "example"))
      (atelier-open-file file workspace)
      (atelier-traveller-test-stopped-workspace root (expand-file-name "saved.txt" root))
      (atelier-name-buffers)
      (let ((table (atelier-traveller-table (atelier-traveller-targets))))
        (atelier-traveller-with-matching
         (lambda ()
           (let ((work (all-completions "work | " table))
                 (other (all-completions "other | file | " table)))
             (should (member "file | example.txt" work))
             (should-not (cl-find "saved.txt" work :test #'string-search))
             (should (equal other '("saved.txt")))
             (should (equal (funcall (completion-metadata-get
                                      (completion-metadata "other | file | " table nil)
                                      'annotation-function)
                                     (car other))
                            "  saved"))
             (should (test-completion "work | file | example.txt" table)))))))))

;; Batch Emacs cannot type into a real prompt, so these tests type into a buffer
;; set up as Traveller sets up its prompt.  Keys run through the command loop
;; and Traveller's key map; after each key the list is recomputed from
;; Traveller's completion table, in its order, as the completion list would.
;; Enter and Down act as the completion list's keys do: Enter chooses the
;; highlighted entry and Down highlights the next.  The Vim-style layer is not
;; exercised.
(defvar vertico--base)
(defvar vertico--candidates)
(defvar vertico--index)

(defun atelier-traveller-test-exit ()
  "Choose the highlighted entry, as the completion list's Enter does."
  (interactive)
  (atelier-traveller-replace-input (concat vertico--base (nth vertico--index vertico--candidates)))
  (exit-minibuffer))

(defun atelier-traveller-test-next ()
  "Highlight the next entry, as the completion list's Down does."
  (interactive)
  (setq vertico--index (min (1+ vertico--index) (1- (length vertico--candidates)))))

(defvar-keymap atelier-traveller-test-list-map
  :parent minibuffer-local-map
  "RET" #'atelier-traveller-test-exit
  "<down>" #'atelier-traveller-test-next)

(defun atelier-traveller-test-keys (targets keys)
  "Type KEYS into a simulated Traveller prompt over TARGETS.
Return (INPUTS . CHOSEN): the input after each key that kept the prompt open,
and the input Enter chose, or nil when the prompt stayed open."
  (with-temp-buffer
    (let* ((atelier-traveller--targets targets)
           (table (atelier-traveller-table targets))
           (inputs nil)
           (shown nil)
           (refresh (lambda ()
                      (let* ((input (buffer-string))
                             (matches (atelier-traveller-with-matching
                                       (lambda () (completion-all-completions
                                                   input table nil (length input))))))
                        (when matches (setcdr (last matches) nil))
                        (setq-local vertico--base
                                    (substring input 0 (car (completion-boundaries
                                                             input table nil ""))))
                        (setq-local vertico--candidates
                                    (mapcar #'substring-no-properties matches))
                        ;; The highlight returns to the top only when the input changes.
                        (unless (equal input shown)
                          (setq-local vertico--index (if matches 0 -1))
                          (setq shown input))))))
      (switch-to-buffer (current-buffer))
      (use-local-map atelier-traveller-test-list-map)
      (atelier-traveller-setup-prompt)
      (funcall refresh)
      (add-hook 'post-command-hook
                (lambda () (funcall refresh) (push (buffer-string) inputs)) nil t)
      (let ((open (catch 'exit (execute-kbd-macro (kbd keys)) t)))
        ;; The command loop runs the hook once before the first key.
        (cons (cdr (nreverse inputs)) (unless open (buffer-string)))))))

(defun atelier-traveller-test-workspace (workspace root)
  "Open in WORKSPACE 2 files named like it under ROOT, and a saved workspace."
  (dolist (name '("work.txt" "work-notes.txt"))
    (let ((file (expand-file-name name root)))
      (with-temp-file file (insert name))
      (atelier-open-file file workspace)))
  (atelier-traveller-test-stopped-workspace root (expand-file-name "saved.txt" root))
  (atelier-name-buffers)
  (atelier-traveller-targets))

(ert-deftest atelier-traveller-typing-after-tab-searches-under-it ()
  "TAB fills in the workspace, dropping the word typed for it even when a buffer
there also matches it; typing then searches under it, and Enter opens the
highlighted buffer without typing its name."
  (atelier-names-test
    (let ((targets (atelier-traveller-test-workspace workspace root)))
      (should (equal (atelier-traveller-test-keys targets "w o TAB f TAB RET")
                     '(("w" "wo" "work | " "work | f" "work | file | ")
                       . "work | file | work-notes.txt"))))))

(ert-deftest atelier-traveller-tab-starts-from-the-highlighted-buffer ()
  "TAB fills in the workspace of the highlighted buffer, then the following ones."
  (atelier-names-test
    (let ((targets (atelier-traveller-test-workspace workspace root)))
      (should (equal (cl-subseq (mapcar (lambda (target) (plist-get target :label)) targets) 0 3)
                     '("work | file | work-notes.txt" "work | file | work.txt"
                       "other | file | saved.txt")))
      (should (equal (atelier-traveller-test-keys targets "<down> <down> TAB TAB")
                     '(("" "" "other | " "Detached | ")))))))

(ert-deftest atelier-traveller-lists-recent-buffers-first-and-current-last ()
  "Open buffers come by last use, saved-only buffers after them, the current one last."
  (atelier-names-test
    (let* ((targets (atelier-traveller-test-workspace workspace root))
           (label (lambda (name)
                    (cl-find-if (lambda (candidate) (string-suffix-p name candidate))
                                (mapcar (lambda (target) (plist-get target :label)) targets))))
           (work (get-file-buffer (expand-file-name "work.txt" root)))
           (notes (get-file-buffer (expand-file-name "work-notes.txt" root))))
      (switch-to-buffer work)
      (switch-to-buffer notes)
      (let ((order (mapcar (lambda (target) (plist-get target :label))
                           (atelier-traveller-by-recency targets notes))))
        (should (equal (car order) (funcall label "work.txt")))
        (should (equal (car (last order)) (funcall label "work-notes.txt")))
        (should (equal (nth (- (length order) 2) order) (funcall label "saved.txt")))))))

(ert-deftest atelier-traveller-tab-cycles-matching-workspaces ()
  "TAB visits each workspace holding a match once and Shift+TAB goes back;
typing keeps the last one filled in, and Shift+TAB then does nothing."
  (atelier-names-test
    (let ((targets (atelier-traveller-test-workspace workspace root)))
      (should (equal (atelier-traveller-test-keys targets "TAB TAB TAB <backtab> x <backtab>")
                     '(("work | " "other | " "Detached | " "other | " "other | x"
                        "other | x"))))
      (should (equal (atelier-traveller-test-keys targets "s a v TAB TAB")
                     '(("s" "sa" "sav" "other | sav" "other | sav")))))))

(ert-deftest atelier-traveller-backspace-edits-filled-in-text ()
  "Backspace deletes characters of a filled-in segment like any text, so TAB
then cycles again from what is left."
  (atelier-names-test
    (let ((targets (atelier-traveller-test-workspace workspace root)))
      (should (equal (atelier-traveller-test-keys targets "TAB DEL TAB")
                     '(("work | " "work |" "work | "))))
      (should (equal (atelier-traveller-test-keys targets "TAB DEL DEL DEL DEL DEL DEL DEL TAB TAB")
                     '(("work | " "work |" "work " "work" "wor" "wo" "w" "" "work | " "other | ")))))))

(ert-deftest atelier-traveller-opens-saved-buffer-in-its-workspace ()
  "Choosing a saved buffer starts its workspace there and restores the buffer."
  (atelier-names-test
    (let ((saved (expand-file-name "saved.txt" root)))
      (with-temp-file saved (insert "saved"))
      (atelier-traveller-test-stopped-workspace root saved)
      (atelier-traveller-open (atelier-traveller-test-target "other | file | saved.txt"))
      (should (equal (atelier-current-workspace-id) "other-test"))
      (should (eq (atelier-workspace-status (atelier-workspace-by-id "other-test")) 'running))
      (should (equal (buffer-file-name (window-buffer)) saved)))))

(ert-deftest atelier-traveller-opens-logs-in-detached-without-storing-them ()
  "A log is shown in Detached but never becomes stack content."
  (atelier-names-test
    (atelier-traveller-open (atelier-traveller-test-target "Detached | buffer | *Messages*"))
    (should (equal (atelier-current-workspace-id) atelier-detached-workspace-id))
    (should (eq (window-buffer) (get-buffer "*Messages*")))
    (should-not (gethash (get-buffer "*Messages*") (atelier-buffer-owner-index)))))

(ert-deftest atelier-traveller-opens-panel-beside-its-source ()
  "Choosing a panel shows its source in the main window and selects the panel."
  (atelier-names-test
    (let* ((source-file (expand-file-name "source.txt" root))
           (other-file (expand-file-name "other.txt" root))
           (_ (with-temp-file source-file (insert "source")))
           (_ (with-temp-file other-file (insert "other")))
           (source (atelier-open-file source-file workspace))
           (_ (atelier-open-file other-file workspace))
           (panel (generate-new-buffer "*Claude Code*"))
           (aipanel-sessions (make-hash-table :test #'eq)))
      (with-current-buffer panel
        (ghostel-mode)
        (setq-local aipanel-owner (list :id source :source-buffer source))
        (atelier-set-buffer-excluded t))
      (puthash source panel aipanel-sessions)
      (atelier-name-buffers)
      ;; Batch Emacs runs no agent process; only its liveness check is replaced.
      (cl-letf (((symbol-function 'get-buffer-process)
                 (lambda (buffer) (and (eq buffer panel) 'agent-process)))
                ((symbol-function 'process-live-p)
                 (lambda (process) (eq process 'agent-process))))
        (atelier-traveller-open (atelier-traveller-test-target "work | aipanel | *Claude Code*"))
        (should (eq (window-buffer (atelier-main-window)) source))
        (should (eq (window-buffer (selected-window)) panel))))))

;;; test-emacs-buffer-names.el ends here

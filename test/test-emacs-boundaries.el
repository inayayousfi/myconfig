;;; test-emacs-boundaries.el --- Package ownership and close policy -*- lexical-binding: t; -*-

(require 'ert)
(require 'cl-lib)
(let ((lisp (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/"
                              (file-name-directory (or load-file-name buffer-file-name)))))
  (add-to-list 'load-path lisp)
  (add-to-list 'load-path (expand-file-name "atelier" lisp)))
(require 'atelier)
(require 'atelier-persist)

;; Recorded before any test loads an adapter, since tests run in name order.
(defconst atelier-test-core-features (copy-sequence features))
(defconst atelier-test-core-types (mapcar #'car atelier-types))
(require 'dired-atelier)
(dired-atelier-setup)
(atelier-navigator-setup)

(ert-deftest atelier-loads-without-application-or-integration-packages ()
  "The core alone loads no adapter or the packages they adapt, and registers
only the types it implements itself."
  (dolist (feature '(myconfig-core ghostel evil aipan univers remot dired dired-atelier
                     vertico vertico-atelier))
    (should-not (memq feature atelier-test-core-features)))
  (should (equal atelier-test-core-types '(file buffer)))
  (should-not atelier-close-without-asking)
  (should-not atelier-job-start-function))

;; Interfaces and adapters read and change workspaces only through the core's
;; functions, so the core alone decides how its records look and change.
(defconst atelier-test-client-files
  '("atelier/atelier-navigator.el" "atelier/atelier-traveller.el" "atelier/atelier-naming.el"
    "atelier/atelier-choice.el" "atelier/dired-atelier.el" "atelier/ghostel-atelier.el"
    "atelier/aipanel-atelier.el" "atelier/universel-atelier.el" "atelier/xref-atelier.el"
    "atelier/remot-atelier.el" "atelier/vertico-atelier.el" "myconfig-ui.el" "myconfig-editing.el"
    "myconfig-terminal.el" "myconfig-bindings.el")
  "Files outside the core that use Atelier.")

(defconst atelier-test-private-patterns
  '("\\_<atelier--" "\\_<atelier-workspaces\\_>" "\\_<atelier-content-live-buffers\\_>"
    "\\_<atelier-entry-owners\\_>" "\\_<atelier-plist-" "\\_<atelier-entry-set-value\\_>"
    "\\_<atelier-model-workspace\\_>" "\\_<atelier-content-cache-key\\_>"
    "(plist-get \\(workspace\\|view\\|content\\|stack\\) :"
    "(plist-get entry :\\(id\\|stack-id\\|content-id\\|selected\\|orientation\\|content-reference\\|stack-reference\\)\\_>")
  "Private core names, and direct reads of record fields.")

(defun atelier-test-private-uses (file)
  "Return \"FILE:LINE: TEXT\" for each private use in FILE.
Upgrade steps, whose names contain \"-upgrade-\", read old saved data on purpose."
  (with-temp-buffer
    (insert-file-contents file)
    (set-syntax-table emacs-lisp-mode-syntax-table)
    (goto-char (point-min))
    (let (uses)
      (while (progn (forward-comment most-positive-fixnum) (not (eobp)))
        (let* ((start (point))
               (form (read (current-buffer)))
               (end (point))
               (name (and (consp form) (symbolp (nth 1 form)) (symbol-name (nth 1 form)))))
          (unless (and name (string-match-p "-upgrade-" name))
            (save-excursion
              (dolist (pattern atelier-test-private-patterns)
                (goto-char start)
                (while (re-search-forward pattern end t)
                  (push (format "%s:%d: %s" (file-name-nondirectory file)
                                (line-number-at-pos)
                                (string-trim (buffer-substring (line-beginning-position)
                                                               (line-end-position))))
                        uses)))))))
      (nreverse uses))))

(ert-deftest atelier-clients-use-only-core-functions ()
  (let ((lisp (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/"
                                (file-name-directory (or load-file-name buffer-file-name
                                                         (locate-library "atelier"))))))
    (should (equal (cl-mapcan (lambda (file)
                                (let ((path (expand-file-name file lisp)))
                                  (when (file-exists-p path) (atelier-test-private-uses path))))
                              atelier-test-client-files)
                   nil))))

(ert-deftest atelier-private-files-have-private-permissions ()
  (let* ((directory (make-temp-file "atelier-private-" t))
         (file (expand-file-name "snapshot.el" directory))
         (data '(:version 10 :workspaces nil)))
    (unwind-protect
        (progn
          (atelier-write-data-atomically file data)
          (should (equal (atelier-read-data file) data))
          (should (= (logand (file-modes directory) #o777) #o700))
          (should (= (logand (file-modes file) #o777) #o600)))
      (when (file-exists-p file) (delete-file file))
      (delete-directory directory))))

(ert-deftest atelier-private-write-failure-keeps-the-previous-file ()
  (let* ((directory (make-temp-file "atelier-atomic-" t))
         (file (expand-file-name "snapshot.el" directory)))
    (unwind-protect
        (progn
          (atelier-write-data-atomically file 'old)
          (cl-letf (((symbol-function 'rename-file)
                     (lambda (&rest _) (error "Cannot replace snapshot"))))
            (should-error (atelier-write-data-atomically file 'new)))
          (should (eq (atelier-read-data file) 'old))
          (should (equal (directory-files directory nil "^[^.]") '("snapshot.el")))
          (should-not (directory-files directory nil "^\\.state-")))
      (when (file-exists-p file) (delete-file file))
      (delete-directory directory))))

(ert-deftest atelier-default-close-respects-buffer-query-functions ()
  (let ((buffer (generate-new-buffer "atelier-query"))
        (atelier-close-without-asking nil))
    (unwind-protect
        (progn
          (with-current-buffer buffer
            (setq-local kill-buffer-query-functions (list (lambda () nil))))
          (should-error (atelier-kill-buffer buffer) :type 'user-error)
          (should (buffer-live-p buffer)))
      (with-current-buffer buffer (setq-local kill-buffer-query-functions nil))
      (kill-buffer buffer))))

(ert-deftest atelier-personal-close-bypasses-buffer-query-functions ()
  (let ((buffer (generate-new-buffer "atelier-discard"))
        (atelier-close-without-asking t))
    (unwind-protect
        (progn
          (with-current-buffer buffer
            (setq-local buffer-file-name "/tmp/atelier-unwritten"
                        kill-buffer-query-functions
                        (list (lambda () (ert-fail "Unexpected question"))))
            (insert "unsaved"))
          (atelier-kill-buffer buffer)
          (should-not (buffer-live-p buffer)))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (set-buffer-modified-p nil)
          (setq-local kill-buffer-query-functions nil))
        (kill-buffer buffer)))))

(ert-deftest atelier-cancelled-close-retains-entry-and-contents ()
  (let* ((buffer (generate-new-buffer "atelier-cancelled"))
         (workspace (list :id "cancel" :name "cancel" :destination "local"
                          :path temporary-file-directory :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-close-without-asking nil)
         entry before)
    (unwind-protect
        (progn
          (setq entry (atelier-register-buffer buffer workspace))
          (with-current-buffer buffer
            (setq-local kill-buffer-query-functions (list (lambda () nil))))
          (setq before (copy-tree workspace))
          (should-error (atelier-close-entry workspace entry) :type 'user-error)
          (should (equal workspace before))
          (should (eq (atelier-entry-live-buffer entry) buffer))
          (should (buffer-live-p buffer)))
      (with-current-buffer buffer (setq-local kill-buffer-query-functions nil))
      (kill-buffer buffer))))

(ert-deftest atelier-confirmed-close-asks-only-once ()
  (let* ((buffer (generate-new-buffer "atelier-confirmed"))
         (workspace (list :id "confirmed" :name "confirmed" :destination "local"
                          :path temporary-file-directory :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-close-without-asking nil)
         (questions 0))
    (unwind-protect
        (let ((entry (atelier-register-buffer buffer workspace)))
          (with-current-buffer buffer
            (setq-local kill-buffer-query-functions
                        (list (lambda () (cl-incf questions) t))))
          (atelier-close-entry workspace entry)
          (should (= questions 1))
          (should-not (buffer-live-p buffer))
          (should-not (plist-get workspace :contents)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-job-observation-uses-supplied-operations ()
  (let* ((buffer (generate-new-buffer "atelier-observed"))
         (job (list :buffer (buffer-name buffer) :direct-command t))
         (atelier-job-process-id-function (lambda (_buffer) 123))
         (atelier-foreground-process-function
          (lambda (pid table direct) (list pid table direct))))
    (unwind-protect
        (cl-letf (((symbol-function 'get-buffer-process) (lambda (_buffer) 'handle)))
          (should (equal (atelier-job-foreground job '(processes)) '(123 (processes) t))))
      (kill-buffer buffer))))

(ert-deftest atelier-cancelled-clear-all-retains-all-buffers-and-entries ()
  (let* ((accepted (generate-new-buffer "atelier-clear-accepted"))
         (refused (generate-new-buffer "atelier-clear-refused"))
         (workspace (list :id "clear" :name "clear" :destination "local"
                          :path temporary-file-directory :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-close-without-asking nil)
         entries before)
    (unwind-protect
        (progn
          (setq entries (list (atelier-register-buffer accepted workspace)
                              (atelier-register-buffer refused workspace))
                before (copy-tree workspace))
          (with-current-buffer refused
            (setq-local kill-buffer-query-functions (list (lambda () nil))))
          (cl-letf (((symbol-function 'buffer-list) (lambda (&rest _) (list accepted refused))))
            (should-error (atelier-clear-all-buffers t) :type 'user-error))
          (should (equal workspace before))
          (should (eq (atelier-entry-live-buffer (car entries)) accepted))
          (should (eq (atelier-entry-live-buffer (cadr entries)) refused))
          (should (buffer-live-p accepted))
          (should (buffer-live-p refused)))
      (dolist (buffer (list accepted refused))
        (when (buffer-live-p buffer)
          (with-current-buffer buffer (setq-local kill-buffer-query-functions nil))
          (kill-buffer buffer))))))

(ert-deftest atelier-cancelled-snapshot-restore-retains-live-job-references ()
  (let* ((accepted (generate-new-buffer "atelier-restore-accepted"))
         (refused (generate-new-buffer "atelier-restore-refused"))
         (first (list :id "first" :name "first" :destination "local"
                      :path temporary-file-directory :entries nil))
         (second (list :id "second" :name "second" :destination "local"
                       :path temporary-file-directory :entries nil))
         (atelier-workspaces (list first second))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-close-without-asking nil)
         entries before)
    (unwind-protect
        (progn
          (setq entries (list (atelier-register-buffer accepted first)
                              (atelier-register-buffer refused second))
                before (copy-tree atelier-workspaces))
          (with-current-buffer refused
            (setq-local kill-buffer-query-functions (list (lambda () nil))))
          (cl-letf (((symbol-function 'atelier-read-data) (lambda (_) 'saved))
                    ((symbol-function 'atelier-validate-state) #'identity)
                    ((symbol-function 'atelier-snapshot-data) (lambda () 'live))
                    ((symbol-function 'atelier-data-topology) #'identity)
                    ((symbol-function 'atelier-data-restorable-state) #'identity)
                    ((symbol-function 'atelier-affected-workspaces) (lambda (&rest _) '("first" "second")))
                    ((symbol-function 'atelier-live-process-state) #'ignore)
                    ((symbol-function 'atelier-saved-process-state) #'ignore)
                    ((symbol-function 'atelier-workspace-job-entries) #'atelier-workspace-entries)
                    ((symbol-function 'yes-or-no-p) (lambda (&rest _) t))
                    ((symbol-function 'atelier-write-data-atomically)
                     (lambda (&rest _) (ert-fail "Cancellation must not write a restore journal")))
                    ((symbol-function 'atelier-stop-all-live-jobs)
                     (lambda () (ert-fail "Cancellation must not stop any job")))
                    ((symbol-function 'atelier-apply-state)
                     (lambda (_) (ert-fail "Cancellation must not replace live state"))))
            (should-error (atelier-restore-snapshot) :type 'user-error))
          (should (equal atelier-workspaces before))
          (should (eq (car atelier-workspaces) first))
          (should (eq (atelier-entry-live-buffer (car entries)) accepted))
          (should (eq (atelier-entry-live-buffer (cadr entries)) refused))
          (should (buffer-live-p accepted))
          (should (buffer-live-p refused)))
      (dolist (buffer (list accepted refused))
        (when (buffer-live-p buffer)
          (with-current-buffer buffer (setq-local kill-buffer-query-functions nil))
          (kill-buffer buffer))))))

(ert-deftest atelier-reused-close-permission-asks-only-once ()
  (let ((buffer (generate-new-buffer "atelier-reused-permission"))
        (atelier-close-without-asking nil)
        (questions 0))
    (unwind-protect
        (progn
          (with-current-buffer buffer
            (setq-local kill-buffer-query-functions
                        (list (lambda () (cl-incf questions) t))))
          (let ((atelier-approved-buffer-closes (list (atelier-prepare-buffer-close buffer))))
            (atelier-prepare-buffer-close buffer)
            (atelier-kill-buffer buffer))
          (should (= questions 1))
          (should-not (buffer-live-p buffer)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-bulk-close-rejects-edits-made-by-later-close-questions ()
  (dolist (operation '(clear restore))
    (let* ((file (generate-new-buffer "atelier-approved-file"))
           (later (generate-new-buffer "atelier-later-question"))
           (workspace (list :id "stale" :name "stale" :destination "local"
                            :path temporary-file-directory :entries nil))
           (atelier-workspaces (list workspace))
           (atelier-content-live-buffers (make-hash-table :test #'equal))
           (atelier-close-without-asking nil)
           entries before)
      (unwind-protect
          (progn
            (with-current-buffer file
              (setq buffer-file-name "/unused/atelier-approved-file"))
            (setq entries (list (atelier-register-buffer file workspace)
                                (atelier-register-buffer later workspace))
                  before (copy-tree workspace))
            (with-current-buffer later
              (setq-local kill-buffer-query-functions
                          (list (lambda ()
                                  (with-current-buffer file (insert "new unsaved edits"))
                                  t))))
            (cl-letf (((symbol-function 'buffer-list) (lambda (&rest _) (list file later)))
                      ((symbol-function 'atelier-read-data) (lambda (_) 'saved))
                      ((symbol-function 'atelier-validate-state) #'identity)
                      ((symbol-function 'atelier-snapshot-data) (lambda () 'live))
                      ((symbol-function 'atelier-data-topology) #'identity)
                      ((symbol-function 'atelier-data-restorable-state) #'identity)
                      ((symbol-function 'atelier-affected-workspaces) (lambda (&rest _) '("stale")))
                      ((symbol-function 'atelier-live-process-state) #'ignore)
                      ((symbol-function 'atelier-saved-process-state) #'ignore)
                      ((symbol-function 'atelier-workspace-job-entries)
                       (lambda (_) entries))
                      ((symbol-function 'yes-or-no-p) (lambda (&rest _) t))
                      ((symbol-function 'atelier-write-data-atomically)
                       (lambda (&rest _) (ert-fail "Stale permission must not write a journal")))
                      ((symbol-function 'atelier-workspace-stop-jobs)
                       (lambda (&rest _) (ert-fail "Stale permission must not stop jobs"))))
              (should-error (if (eq operation 'clear)
                                (atelier-clear-all-buffers t)
                              (atelier-restore-snapshot))
                            :type 'user-error))
            (should (equal workspace before))
            (should (eq (atelier-entry-live-buffer (car entries)) file))
            (should (eq (atelier-entry-live-buffer (cadr entries)) later))
            (should (buffer-live-p later))
            (with-current-buffer file
              (should (buffer-modified-p))
              (should (equal (buffer-string) "new unsaved edits"))))
        (dolist (buffer (list file later))
          (when (buffer-live-p buffer)
            (with-current-buffer buffer
              (set-buffer-modified-p nil)
              (setq-local kill-buffer-query-functions nil))
            (kill-buffer buffer)))))))

(ert-deftest atelier-close-permission-rejects-a-query-editing-its-own-buffer ()
  (let ((buffer (generate-new-buffer "atelier-self-editing-query"))
        (atelier-close-without-asking nil))
    (unwind-protect
        (progn
          (with-current-buffer buffer
            (setq buffer-file-name "/unused/atelier-self-editing-file")
            (setq-local kill-buffer-query-functions
                        (list (lambda () (insert "new edits") t))))
          (should-error (atelier-prepare-buffer-close buffer) :type 'user-error)
          (should (buffer-live-p buffer))
          (with-current-buffer buffer (should (equal (buffer-string) "new edits"))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (set-buffer-modified-p nil)
          (setq-local kill-buffer-query-functions nil))
        (kill-buffer buffer)))))

(ert-deftest atelier-kill-rejects-close-permission-after-later-edits ()
  (let ((buffer (generate-new-buffer "atelier-stale-permission"))
        (atelier-close-without-asking nil))
    (unwind-protect
        (progn
          (with-current-buffer buffer (setq buffer-file-name "/unused/atelier-stale-file"))
          (let ((atelier-approved-buffer-closes (list (atelier-prepare-buffer-close buffer))))
            (with-current-buffer buffer (insert "new edits"))
            (should-error (atelier-kill-buffer buffer) :type 'user-error))
          (should (buffer-live-p buffer))
          (with-current-buffer buffer (should (buffer-modified-p))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer)))))

(ert-deftest atelier-close-permission-allows-new-non-file-output ()
  (let ((buffer (generate-new-buffer "atelier-terminal-output"))
        (atelier-close-without-asking nil)
        (questions 0))
    (unwind-protect
        (progn
          (with-current-buffer buffer
            (setq-local kill-buffer-query-functions
                        (list (lambda () (cl-incf questions) t))))
          (let ((atelier-approved-buffer-closes (list (atelier-prepare-buffer-close buffer))))
            (with-current-buffer buffer (insert "more process output"))
            (atelier-validate-buffer-closes)
            (atelier-kill-buffer buffer))
          (should (= questions 1))
          (should-not (buffer-live-p buffer)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-successful-clear-preserves-both-question-policies ()
  (dolist (without-asking '(nil t))
    (let* ((buffer (generate-new-buffer "atelier-successful-clear"))
           (workspace (list :id "clear" :name "clear" :destination "local"
                            :path temporary-file-directory :entries nil))
           (atelier-workspaces (list workspace))
           (atelier-content-live-buffers (make-hash-table :test #'equal))
           (atelier-close-without-asking without-asking)
           (questions 0))
      (unwind-protect
          (progn
            (atelier-register-buffer buffer workspace)
            (with-current-buffer buffer
              (setq-local kill-buffer-query-functions
                          (list (lambda () (cl-incf questions) t))))
            (cl-letf (((symbol-function 'buffer-list) (lambda (&rest _) (list buffer))))
              (should (= (atelier-clear-all-buffers t) 1)))
            (should (= questions (if without-asking 0 1)))
            (should-not (buffer-live-p buffer))
            (should-not (plist-get workspace :contents)))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest atelier-shared-view-move-is-rejected-before-changing-state ()
  (dolist (case '(active inactive stopped))
    (let* ((buffer (generate-new-buffer "atelier-shared-move-buffer"))
           (one (list :id "one" :name "Workspace A" :entries nil))
           (two (list :id "two" :name "Workspace B" :entries nil))
           (atelier-workspaces (list one two))
           (atelier-content-live-buffers (make-hash-table :test #'equal))
           (atelier-entry-owners (make-hash-table :test #'eq))
           (atelier-entry-moved-hook (list (lambda (&rest _) (ert-fail "Unexpected move hook"))))
           (atelier-change-hook (list (lambda () (ert-fail "Unexpected state change")))))
      (unwind-protect
          (let* ((reference (atelier-register-buffer buffer one t))
                 (shared-id (plist-get reference :content-id))
                 (entry (atelier-entry-add one (list :id "moving" :content-id shared-id) t))
                 (other-id (when (eq case 'inactive)
                             (atelier-workspace-store-content one '(:kind scratch :name "other"))))
                 (mirror (list :id "mirror" :content-id (or other-id shared-id))))
            (atelier-entry-add one mirror t)
            (when (eq case 'stopped)
              (remhash (atelier-content-cache-key one shared-id) atelier-content-live-buffers))
            (let ((before (copy-tree atelier-workspaces))
                  (cache (copy-hash-table atelier-content-live-buffers)))
              (let ((failure (should-error (atelier-entry-move entry one two) :type 'user-error)))
                (should (string-match-p "Detach this stack from every view in Workspace A first"
                                        (error-message-string failure)))
                (should (string-match-p "nothing was moved" (error-message-string failure))))
              (should (equal atelier-workspaces before))
              (should (= (hash-table-count cache) (hash-table-count atelier-content-live-buffers)))
              (maphash (lambda (key value) (should (eq value (gethash key atelier-content-live-buffers)))) cache)))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest atelier-moving-layout-does-not-bypass-stack-unassignment ()
  (let* ((buffer (generate-new-buffer "atelier-shared-root-buffer"))
         (one (list :id "one" :name "one" :entries nil))
         (two (list :id "two" :name "two" :entries nil))
         (atelier-workspaces (list one two))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-entry-owners (make-hash-table :test #'eq)))
    (unwind-protect
        (let* ((reference (atelier-register-buffer buffer one t))
               (entry (atelier-entry-add one (list :id "first" :content-id
                                                   (plist-get reference :content-id)) t))
               (mirror (list :id "mirror" :content-id (plist-get entry :content-id)))
               (root (list :id "root" :kind 'layout :orientation 'horizontal
                           :children (list entry mirror))))
          (atelier-entry-add one mirror t)
          (setf (plist-get one :entries) (list root))
          (let ((before (copy-tree atelier-workspaces)))
            (should-error (atelier-entry-move root one two) :type 'user-error)
            (should (equal before atelier-workspaces)))
          (should-not (atelier-workspace-entries two))
          (should (eq (atelier-entry-live-buffer entry) buffer))
          (should (eq (atelier-entry-live-buffer mirror) buffer))
          (should (eq (atelier-entry-workspace entry) one)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-shared-view-detach-rejects-before-leaving-navigator ()
  (let* ((buffer (generate-new-buffer "atelier-shared-detach-buffer"))
         (one (list :id "one" :name "Workspace A" :entries nil))
         (detached (list :id "detached" :name atelier-detached-workspace-name :entries nil))
         (atelier-workspaces (list one detached))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-entry-owners (make-hash-table :test #'eq))
         (atelier-navigator-attach-source 'retained))
    (unwind-protect
        (let* ((reference (atelier-register-buffer buffer one t))
               (entry (atelier-entry-add one (list :id "first" :content-id
                                                   (plist-get reference :content-id)) t))
               (mirror (list :id "mirror" :content-id (plist-get entry :content-id))))
          (atelier-entry-add one mirror t)
          (cl-letf (((symbol-function 'atelier-navigator-target)
                     (lambda () (list 'workspace-owned-buffer "Workspace A" (plist-get reference :id))))
                    ((symbol-function 'atelier-navigator-quit)
                     (lambda () (ert-fail "A rejected detach must leave the navigator open"))))
            (should-error (atelier-navigator-detach) :type 'user-error))
          (should (eq atelier-navigator-attach-source 'retained))
          (should (eq (atelier-entry-workspace entry) one))
          (should-not (atelier-workspace-entries detached)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-close-accepts-a-process-sentinel-closing-its-buffer ()
  (let ((buffer (generate-new-buffer " *sentinel-close*"))
        (atelier-close-without-asking t))
    (unwind-protect
        (cl-letf (((symbol-function 'get-buffer-process) (lambda (_) 'terminal))
                  ((symbol-function 'process-live-p) (lambda (_) t))
                  ((symbol-function 'set-process-query-on-exit-flag) #'ignore)
                  ((symbol-function 'delete-process)
                   (lambda (_) (let ((kill-buffer-query-functions nil)) (kill-buffer buffer)))))
          (should (atelier-kill-buffer buffer))
          (should-not (buffer-live-p buffer)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-cancelled-workspace-opening-recovers-frame-and-stopped-status ()
  (let* ((old (list :id "cancel-old" :name "old" :status 'running))
         (target (list :id "cancel-target" :name "target" :status 'stopped
                       :destination "local" :path temporary-file-directory))
         (atelier-workspaces (list old target))
          cancelled)
    (save-window-excursion
      (atelier-select-workspace old)
      (cl-letf (((symbol-function 'atelier-restart-saved-jobs) #'ignore)
                ((symbol-function 'atelier-restore-workspace)
                 (lambda (_) (signal 'quit nil))))
        (condition-case nil (atelier-open-workspace target)
          (quit (setq cancelled t)))
        (should cancelled)
        (should (eq (atelier-current-workspace) old))
        (should (eq (atelier-workspace-status target) 'stopped))))))

(ert-deftest atelier-frame-owned-navigators-are-removed-from-window-history ()
  (let ((navigator (generate-new-buffer "*Atelier*<other-frame>"))
        (ordinary (generate-new-buffer "ordinary-history")))
    (unwind-protect
        (save-window-excursion
          (with-current-buffer navigator (atelier-navigator-mode))
          (set-window-prev-buffers nil (list (list navigator nil 1) (list ordinary nil 1)))
          (set-window-next-buffers nil (list navigator ordinary))
          (atelier-clean-window-buffer-history)
          (should (equal (mapcar #'car (window-prev-buffers)) (list ordinary)))
          (should (equal (window-next-buffers) (list ordinary))))
      (kill-buffer navigator)
      (kill-buffer ordinary))))

(ert-deftest atelier-preparation-does-not-touch-live-records-before-publication ()
  (let* ((workspace (list :id "prepared" :name "before" :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal)))
    (atelier-operation-call
     'rename '("prepared")
     (lambda ()
       (let ((draft (atelier-operation-workspace "prepared")))
         (should-not (eq draft workspace))
         (atelier-plist-set! draft :name "after")
         (atelier-operation-live-event
          (lambda () (should (equal (plist-get (atelier-workspace-by-id "prepared") :name) "before"))))
         (should (equal (plist-get workspace :name) "before")))))
    (should (equal (plist-get workspace :name) "after"))))

(ert-deftest atelier-failed-preparation-discards-record-changes-and-new-buffers ()
  (let* ((workspace (list :id "discard" :name "original" :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         acquired)
    (should-error
     (atelier-operation-call
      'failure '("discard")
      (lambda ()
        (atelier-plist-set! (atelier-operation-workspace "discard") :name "partial")
        (setq acquired (atelier-operation-track-buffer (generate-new-buffer " *prepared-resource*")))
        (error "Injected later failure"))))
    (should (equal (plist-get workspace :name) "original"))
    (should-not (buffer-live-p acquired))
    (should-not atelier-operation-active)))

(ert-deftest atelier-notification-error-keeps-publication-and-runs-later-observers ()
  (let* ((workspace (list :id "notify" :name "before" :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         errors observed
         (atelier-workspace-renamed-hook
          (list (lambda (&rest _) (error "Observer failed"))
                (lambda (record &rest _)
                  (should (eq record workspace))
                  (push (plist-get record :name) observed)))))
    (cl-letf (((symbol-function 'atelier-log) (lambda (&rest args) (push args errors))))
      (atelier-operation-call
       'notify '("notify")
       (lambda ()
         (let ((draft (atelier-operation-workspace "notify")))
           (atelier-plist-set! draft :name "after")
           (atelier-operation-notify 'atelier-workspace-renamed-hook draft "before" "after")))))
    (should (equal observed '("after")))
    (should (= (length errors) 1))
    (should (equal (plist-get workspace :name) "after"))))

(ert-deftest atelier-conflicting-operations-queue-in-order-against-published-records ()
  (let* ((workspace (list :id "queue" :name "queue" :entries nil :step 0))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-operation-queue nil) steps)
    (atelier-operation-call
     'first '("queue")
     (lambda ()
       (dolist (step '(2 3))
         (let ((step step))
           (should
            (eq (atelier-operation-call
                 'later '("queue")
                 (lambda ()
                   (let ((draft (atelier-operation-workspace "queue")))
                     (push (plist-get draft :step) steps)
                     (atelier-plist-set! draft :step step)))) :queued))))
       (atelier-plist-set! (atelier-operation-workspace "queue") :step 1)))
    (should (equal (nreverse steps) '(1 2)))
    (should (= (plist-get workspace :step) 3))
    (should-not atelier-operation-queue)))

(ert-deftest atelier-unrelated-operation-can-publish-during-preparation ()
  (let* ((one (list :id "one" :name "one" :entries nil))
         (two (list :id "two" :name "two" :entries nil))
         (atelier-workspaces (list one two))
         (atelier-content-live-buffers (make-hash-table :test #'equal)))
    (atelier-operation-call
     'one '("one")
     (lambda ()
       (atelier-plist-set! (atelier-operation-workspace "one") :name "one-done")
       (atelier-operation-live-event
        (lambda ()
          (atelier-operation-call
           'two '("two")
           (lambda () (atelier-plist-set! (atelier-operation-workspace "two") :name "two-done")))))
       (should (equal (plist-get two :name) "two-done"))))
    (should (equal (plist-get one :name) "one-done"))
    (should (equal (plist-get two :name) "two-done"))))

(ert-deftest atelier-native-events-survive-a-failed-preparation ()
  (let* ((workspace (list :id "event" :name "event" :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal)))
    (should-error
     (atelier-operation-call
      'event '("event")
      (lambda ()
        (atelier-plist-set! (atelier-operation-workspace "event") :name "partial")
        (atelier-operation-live-event
         (lambda ()
           (atelier-plist-set! (atelier-workspace-by-id "event") :status 'stopped)
           (setq atelier-workspaces
                 (cons (list :id "native-added" :name "native-added" :entries nil) atelier-workspaces))))
        (error "Injected failure"))))
    (should (eq (plist-get workspace :status) 'stopped))
    (should (equal (plist-get workspace :name) "event"))
    (should (atelier-workspace-by-id "native-added"))))

(ert-deftest atelier-preparation-rejects-undeclared-owner-changes ()
  (let* ((one (list :id "declared" :name "declared" :entries nil))
         (two (list :id "other" :name "other" :entries nil))
         (atelier-workspaces (list one two))
         (atelier-content-live-buffers (make-hash-table :test #'equal)))
    (should-error
     (atelier-operation-call
      'bad-scope '("declared")
      (lambda () (atelier-plist-set! (atelier-operation-workspace "other") :name "unexpected"))))
    (should (equal (plist-get two :name) "other"))))

(ert-deftest atelier-preparation-rejects-broken-content-references ()
  (let* ((workspace (list :id "invalid" :name "invalid" :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal)))
    (should-error
     (atelier-operation-call
      'bad-reference '("invalid")
      (lambda ()
        (atelier-plist-set! (atelier-operation-workspace "invalid") :entries
                            '((:id "view" :content-ids ("missing")))))))
    (should-not (plist-get workspace :entries))))

(ert-deftest atelier-running-workspace-opening-failure-keeps-its-original-content-order ()
  (let* ((old (list :id "running-old" :name "old" :status 'running :entries nil))
         (entry (list :id "running-view" :content-ids '("good" "bad")))
         (target (list :id "running-target" :name "target" :status 'running
                       :entries (list entry)
                       :contents '((:id "good" :kind scratch :contents "good")
                                   (:id "bad" :kind scratch :contents "bad"))))
         (atelier-workspaces (list old target))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (previous (frame-parameter nil 'atelier-workspace-id)))
    (unwind-protect
        (save-window-excursion
          (atelier-select-workspace old)
          (cl-letf (((symbol-function 'atelier-workspace-directory) (lambda (_) temporary-file-directory))
                    ((symbol-function 'atelier-restore-workspace)
                     (lambda (workspace)
                       (atelier-entry-activate-content (atelier-entry-by-id workspace "running-view") "bad")
                       (error "Later opening failure"))))
            (should-error (atelier-open-workspace target)))
          (should (eq (atelier-current-workspace) old))
          (should (equal (plist-get entry :content-ids) '("good" "bad"))))
      (set-frame-parameter nil 'atelier-workspace-id previous))))

(ert-deftest atelier-navigator-opening-follows-view-id-after-an-earlier-view-is-pruned ()
  (let* ((directory (make-temp-file "atelier-id-open-" t))
         (one (expand-file-name "one.txt" directory))
         (two (expand-file-name "two.txt" directory))
         (tail-file (expand-file-name "tail.txt" directory))
         (dead (list :id "dead-view" :content-ids '("dead")))
         (chosen (list :id "chosen-view" :content-ids '("one" "two")))
         (tail (list :id "tail-view" :content-ids '("tail")))
         (branch (list :id "branch" :kind 'layout :orientation 'vertical :ratio 0.5
                       :children (list chosen tail)))
         (root (list :id "root" :kind 'layout :orientation 'horizontal :ratio 0.5
                     :displayed t :children (list dead branch)))
         (old (list :id "id-old" :name "old" :destination "local" :path directory
                    :status 'running :entries nil))
         (target (list :id "id-target" :name "target" :destination "local" :path directory
                       :status 'stopped :entries (list root)
                       :contents
                       (mapcar (lambda (pair)
                                 (if (equal (car pair) "dead")
                                     (list :id "dead" :kind 'directory :type 'dired
                                           :directory (cdr pair) :persistent t)
                                   (list :id (car pair) :kind 'file :type 'file
                                         :file (cdr pair) :persistent t)))
                               (list (cons "dead" (expand-file-name "gone.txt" directory))
                                     (cons "one" one) (cons "two" two) (cons "tail" tail-file)))))
         (atelier-workspaces (list old target))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-change-hook nil)
         (previous (frame-parameter nil 'atelier-workspace-id)))
    (unwind-protect
        (save-window-excursion
          (dolist (file (list one two tail-file)) (with-temp-file file (insert file)))
          (atelier-select-workspace old)
          (cl-letf (((symbol-function 'atelier-navigator-quit) #'ignore)
                    ((symbol-function 'atelier-persist-record-pruning) #'ignore))
            (atelier-navigator-open-content "target" "chosen-view" "two" 1))
          (should (equal (mapcar (lambda (entry) (plist-get entry :id))
                                 (atelier-workspace-displayed-entries target))
                          '("chosen-view" "tail-view")))
          (should (equal (buffer-file-name (window-buffer (car (atelier-main-windows)))) two))
          (should (equal (buffer-file-name (window-buffer (cadr (atelier-main-windows)))) tail-file)))
      (set-frame-parameter nil 'atelier-workspace-id previous)
      (dolist (buffer (buffer-list))
        (when-let* ((file (buffer-local-value 'buffer-file-name buffer))
                    (_ (string-prefix-p directory file)))
          (kill-buffer buffer)))
      (delete-directory directory t))))

(ert-deftest atelier-save-during-preparation-writes-only-published-records ()
  (let* ((directory (make-temp-file "atelier-published-save-" t))
         (workspace (list :id "save-prepared" :name "before" :status 'running
                          :destination "local" :path directory :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-persist-state-file (expand-file-name "state.el" directory))
         (atelier-before-save-hook nil) (atelier-after-save-hook nil)
         (atelier-persist-restoring nil)
         (previous (frame-parameter nil 'atelier-workspace-id)))
    (unwind-protect
        (progn
          (atelier-select-workspace workspace)
          (atelier-operation-call
           'save-private '("save-prepared")
           (lambda ()
             (atelier-plist-set! (atelier-operation-workspace "save-prepared") :name "after")
             (atelier-persist-now)
             (let ((saved (atelier-read-data atelier-persist-state-file)))
               (should (equal (plist-get (cl-find "save-prepared" (plist-get saved :workspaces)
                                                 :key (lambda (record) (plist-get record :id))
                                                 :test #'equal) :name) "before")))))
          (should (equal (plist-get workspace :name) "after")))
      (set-frame-parameter nil 'atelier-workspace-id previous)
      (delete-directory directory t))))

(ert-deftest atelier-created-workspace-can-compose-buffer-registration ()
  (let* ((old (list :id "creation-old" :name "old" :entries nil))
         (atelier-workspaces (list old))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-change-hook nil)
         (previous (frame-parameter nil 'atelier-workspace-id))
         created-buffer)
    (unwind-protect
        (save-window-excursion
          (atelier-select-workspace old)
           (cl-letf (((symbol-function 'atelier-read-workspace-target)
                      (lambda (&optional multiple)
                        (let ((target (list "local" temporary-file-directory 'local nil)))
                          (if multiple (list target) target))))
                    ((symbol-function 'read-string) (lambda (&rest _) "created")))
            (atelier-create-workspace))
          (let* ((workspace (atelier-workspace-get "created"))
                 (entry (car (atelier-workspace-entries workspace))))
            (should workspace)
            (should (eq (atelier-current-workspace) workspace))
            (setq created-buffer (atelier-entry-live-buffer entry))
            (should (buffer-live-p created-buffer))))
      (set-frame-parameter nil 'atelier-workspace-id previous)
      (when (buffer-live-p created-buffer) (kill-buffer created-buffer)))))

(ert-deftest atelier-publication-preserves-explicit-workspace-order ()
  (let* ((one (list :id "order-one" :name "one" :entries nil))
         (two (list :id "order-two" :name "two" :entries nil))
         (atelier-workspaces (list one two))
         (atelier-content-live-buffers (make-hash-table :test #'equal)))
    (atelier-operation-call 'reorder '(:all)
                            (lambda () (setq atelier-workspaces (reverse atelier-workspaces))))
    (should (equal (mapcar #'atelier-workspace-id atelier-workspaces) '("order-two" "order-one")))))

(ert-deftest atelier-queued-operation-rejects-a-deleted-stable-target ()
  (let* ((workspace (list :id "deleted-queued" :name "queued" :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-operation-queue nil) errors ran)
    (cl-letf (((symbol-function 'atelier-log) (lambda (&rest args) (push args errors))))
      (atelier-operation-call
       'delete '("deleted-queued")
       (lambda ()
         (atelier-operation-call
          'use-deleted '("deleted-queued")
          (lambda ()
            (atelier-operation-workspace "deleted-queued")
            (setq ran t)))
         (setq atelier-workspaces nil))))
    (should-not ran)
    (should-not atelier-workspaces)
    (should-not atelier-operation-queue)
    (should (= (length errors) 1))))

(ert-deftest atelier-failing-native-event-keeps-its-published-registry-update ()
  (let* ((workspace (list :id "event-error" :name "event" :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal)))
    (should-error
     (atelier-operation-call
      'event-error '("event-error")
      (lambda ()
        (atelier-operation-live-event
         (lambda ()
           (setq atelier-workspaces nil)
           (error "Native event observer failed"))))))
    (should-not atelier-workspaces)))

(ert-deftest atelier-failed-dired-navigation-restores-listing-and-records ()
  (let* ((directory (make-temp-file "atelier-dired-operation-" t))
         (next (expand-file-name "next/" directory))
         (workspace (list :id "dired-operation" :name "dired" :destination "local"
                          :path directory :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-change-hook nil) buffer)
    (unwind-protect
        (progn
          (make-directory next)
          (setq buffer (atelier-new-dired-buffer directory t workspace))
          (atelier-assign-buffer-to-workspace buffer workspace 'dired)
          (let ((entry (atelier-workspace-entry-for-buffer workspace buffer)))
            (with-current-buffer buffer
              (let ((before (buffer-string)) (old-directory default-directory))
                (cl-letf (((symbol-function 'dired-readin)
                           (lambda ()
                             (let ((inhibit-read-only t)) (erase-buffer) (insert "partial listing"))
                             (error "Directory read failed"))))
                  (should-error (atelier-dired-change-directory next)))
                (should (equal (buffer-string) before))
                (should (equal default-directory old-directory))
                (should (equal (atelier-entry-value entry :directory) old-directory))))))
      (when (buffer-live-p buffer) (kill-buffer buffer))
      (delete-directory directory t))))

(ert-deftest atelier-failed-navigator-renaming-restores-native-buffer-name ()
  (let* ((workspace (list :id "rename-operation" :name "rename" :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-change-hook nil)
         (buffer (generate-new-buffer "atelier-before-rename")))
    (unwind-protect
        (let ((entry (atelier-register-buffer buffer workspace t)))
          (cl-letf (((symbol-function 'atelier-navigator-quit) #'ignore)
                    ((symbol-function 'atelier-navigator) #'ignore)
                    ((symbol-function 'read-string) (lambda (&rest _) "atelier-after-rename"))
                    ((symbol-function 'atelier-operation-validate) (lambda (_) (error "Late check failed"))))
            (should-error
             (atelier-navigator-rename
              (list 'workspace-owned-buffer "rename" (plist-get entry :id)))))
          (should (equal (buffer-name buffer) "atelier-before-rename"))
          (should (equal (atelier-entry-value entry :name) "atelier-before-rename")))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-failed-state-restoration-keeps-published-metadata ()
  (let* ((workspace (list :id "metadata-original" :name "original" :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-remembered-ssh-destinations '("original-host"))
         (atelier-snapshot-generation "original-generation"))
    (cl-letf (((symbol-function 'atelier-operation-validate)
               (lambda (_) (error "Restoration failed"))))
      (should-error
       (atelier-apply-state
        '(:workspaces nil :ssh-destinations ("partial-host") :generation "partial-generation"))))
    (should (equal atelier-workspaces (list workspace)))
    (should (equal atelier-remembered-ssh-destinations '("original-host")))
    (should (equal atelier-snapshot-generation "original-generation"))))

(ert-deftest atelier-startup-retains-registry-and-selection-after-unavailable-root ()
  (dolist (remote '(nil t))
    (let* ((missing (format "/tmp/opencode/atelier-missing-%08x/" (random #xffffffff)))
           (saved-workspace
            (list :id "startup-blocked" :name "blocked" :status 'running
                  :destination (if remote "offline-host" "local")
                  :platform (if remote 'windows 'local) :path (if remote "/C:/work/" missing)
                  :entries '((:id "saved-layout" :kind layout :orientation horizontal
                                  :ratio 0.5 :displayed t
                                  :children ((:id "saved-one" :content-ids ("one"))
                                             (:id "saved-two" :content-ids ("two")))))
                  :contents '((:id "one" :kind scratch :persistent t :contents "first")
                              (:id "two" :kind scratch :persistent t :contents "second"))))
           (usable (list :id "startup-usable" :name "usable" :status 'running
                         :destination "local" :path temporary-file-directory :entries nil))
           (saved (list :version 11 :generation "saved-generation"
                        :current-workspace-id "startup-blocked"
                        :workspaces (mapcar #'atelier-workspace-flat-copy (list saved-workspace usable))))
           (atelier-workspaces (list (list :id "fresh" :name "fresh" :entries nil)))
           (atelier-content-live-buffers (make-hash-table :test #'equal))
           (atelier-change-hook nil) (atelier-before-save-hook nil) (atelier-after-save-hook nil)
           (atelier-after-restore-hook nil) (kill-emacs-hook nil)
           (atelier-persist-initialized-p nil) (atelier-job-observer-timer nil)
           (atelier-persist-timer nil) (atelier-persist-restoring nil)
           (atelier-defer-job-restart nil) (atelier-navigator-window-configurations nil)
           (previous (frame-parameter nil 'atelier-workspace-id))
           attempts written notice)
      (unwind-protect
          (save-window-excursion
            (cl-letf (((symbol-function 'atelier-read-data) (lambda (_) (copy-tree saved)))
                      ((symbol-function 'atelier-restore-journal-recover) #'ignore)
                      ((symbol-function 'run-with-timer) (lambda (&rest _) nil))
                      ((symbol-function 'display-graphic-p) (lambda (&rest _) t))
                      ((symbol-function 'atelier-workspace-directory)
                       (lambda (workspace)
                         (push (atelier-workspace-id workspace) attempts)
                         (if remote (error "Connection unavailable") missing)))
                      ((symbol-function 'atelier-write-data-atomically)
                       (lambda (_file data) (setq written data))))
              (atelier-persist-setup)
              (let ((workspace (atelier-workspace-by-id "startup-blocked")))
                (should workspace)
                (should (atelier-workspace-by-id "startup-usable"))
                (should-not (atelier-workspace-by-id "fresh"))
                (should (eq (atelier-current-workspace) workspace))
                (should (eq (atelier-workspace-status workspace) 'stopped))
                (setq notice (window-buffer))
                (should (equal (buffer-local-value 'atelier-unavailable-workspace-id notice)
                               "startup-blocked"))
                (should (= (length (atelier-workspace-entries workspace)) 2))
                (atelier-capture-current-workspace)
                (should (equal (plist-get (atelier-workspace-displayed-entry workspace) :id)
                               "saved-layout")))
              (atelier-persist-now)
              (should (equal attempts '("startup-blocked")))
              (should (equal (plist-get written :current-workspace-id) "startup-blocked"))
              (let ((retained (cl-find "startup-blocked" (plist-get written :workspaces)
                                       :key (lambda (record) (plist-get record :id)) :test #'equal)))
                (should (equal (plist-get retained :entry-root-ids) '("saved-layout")))
                (should (= (length (plist-get retained :contents)) 2)))
              (should (cl-find "startup-usable" (plist-get written :workspaces)
                               :key (lambda (record) (plist-get record :id)) :test #'equal))))
        (set-frame-parameter nil 'atelier-workspace-id previous)
        (when (buffer-live-p notice) (kill-buffer notice))))))

(ert-deftest atelier-queued-close-keeps-its-initiating-view-after-selection-changes ()
  (dolist (replace-original '(nil t))
    (let* ((one (generate-new-buffer "atelier-queued-close-one"))
           (two (generate-new-buffer "atelier-queued-close-two"))
           (workspace (list :id "queued-close" :name "queued-close" :entries nil))
           (atelier-workspaces (list workspace))
           (atelier-content-live-buffers (make-hash-table :test #'equal))
           (atelier-close-without-asking t) (atelier-change-hook nil)
           (atelier-operation-queue nil)
           (previous (frame-parameter nil 'atelier-workspace-id)) errors)
      (unwind-protect
          (save-window-excursion
            (delete-other-windows)
            (atelier-select-workspace workspace)
            (set-window-buffer (selected-window) one)
            (let ((first-window (selected-window)) (second-window (split-window-right)))
              (set-window-buffer second-window two)
              (atelier-register-buffer one workspace t)
              (atelier-register-buffer two workspace t)
              (cl-letf (((symbol-function 'display-graphic-p) (lambda (&rest _) t))
                        ((symbol-function 'atelier-log) (lambda (&rest args) (push args errors))))
                (atelier-capture-current-workspace)
                (atelier-operation-call
                 'waiting '("queued-close")
                 (lambda ()
                   (atelier-operation-live-event
                    (lambda ()
                      (with-selected-window first-window
                        (should (eq (call-interactively #'atelier-close-current-view) :queued)))))
                   (if replace-original
                       (set-window-buffer first-window two)
                     (select-window second-window))))))
            (should (buffer-live-p two))
            (if replace-original
                (progn (should (buffer-live-p one)) (should errors))
              (should-not (buffer-live-p one))
              (should-not errors))
            (should-not atelier-operation-queue))
        (set-frame-parameter nil 'atelier-workspace-id previous)
        (dolist (buffer (list one two)) (when (buffer-live-p buffer) (kill-buffer buffer)))))))

(ert-deftest atelier-job-observation-during-preparation-publishes-unrelated-recipe ()
  (let* ((buffer (generate-new-buffer "atelier-observed-job"))
         (one (list :id "observer-one" :name "one" :entries nil))
         (job (list :id "observed-job" :buffer (buffer-name buffer) :policy 'always
                    :recipe '(:executable "old")))
         (entry (list :id "observed-view" :content-ids '("observed-content")))
         (two (list :id "observer-two" :name "two" :entries (list entry)
                    :contents (list (list :id "observed-content" :kind 'terminal :job job))))
         (atelier-workspaces (list one two))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-process-observation-function (lambda () t))
         (atelier-process-table-function #'ignore)
         (atelier-change-hook nil) observed-saved)
    (unwind-protect
        (progn
          (atelier-entry-set-live-buffer entry buffer)
          (cl-letf (((symbol-function 'atelier-job-foreground)
                     (lambda (&rest _) '(:executable "new" :argv ("new"))))
                    ((symbol-function 'atelier-persist-now)
                     (lambda () (setq observed-saved (plist-get (atelier-entry-job entry) :recipe)))))
            (atelier-operation-call
             'prepare-one '("observer-one")
             (lambda ()
               (atelier-plist-set! (atelier-operation-workspace "observer-one") :name "one-done")
               (atelier-observe-jobs))))
          (should (equal (plist-get one :name) "one-done"))
          (should (equal (plist-get (plist-get (atelier-entry-job entry) :recipe) :executable) "new"))
          (should (equal (plist-get observed-saved :executable) "new")))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-failed-navigator-open-keeps-real-quit-return-layout ()
  (let* ((workspace (list :id "navigator-failure" :name "navigator-failure" :status 'running
                          :destination "local" :path temporary-file-directory :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-content-live-buffers (make-hash-table :test #'equal))
         (atelier-navigator-window-configurations nil)
         (atelier-navigator-selection-by-frame nil)
         (atelier-change-hook nil)
         (one (generate-new-buffer "atelier-navigator-return-one"))
         (two (generate-new-buffer "atelier-navigator-return-two"))
         (previous (frame-parameter nil 'atelier-workspace-id)) navigator)
    (unwind-protect
        (save-window-excursion
          (delete-other-windows)
          (atelier-select-workspace workspace)
          (set-window-buffer nil one)
          (set-window-buffer (split-window-right) two)
          (let ((entry (atelier-register-buffer one workspace t)))
            (atelier-register-buffer two workspace t)
            (cl-letf (((symbol-function 'display-graphic-p) (lambda (&rest _) t))
                      ((symbol-function 'atelier-known-project-roots) #'ignore))
              (atelier-navigator)
              (setq navigator (atelier-navigator-frame-buffer))
              (cl-letf (((symbol-function 'atelier-restore-entry-content)
                         (lambda (&rest _) (error "Content temporarily unavailable"))))
                (should-error
                 (atelier-navigator-open-content "navigator-failure" (plist-get entry :id)
                                                  (car (plist-get entry :content-ids)) 0)))
              (should (eq (window-buffer) navigator))
              (should (assq (selected-frame) atelier-navigator-window-configurations))
              (atelier-navigator-quit)
              (should (equal (mapcar #'window-buffer (atelier-main-windows)) (list one two)))
              (should-not (assq (selected-frame) atelier-navigator-window-configurations)))))
      (set-frame-parameter nil 'atelier-workspace-id previous)
      (dolist (buffer (list one two navigator)) (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-run-tests-batch-and-exit)
;;; test-emacs-boundaries.el ends here

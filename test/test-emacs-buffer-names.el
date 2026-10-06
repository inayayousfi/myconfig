;;; test-emacs-buffer-names.el --- Workspace ownership and buffer names -*- lexical-binding: t; -*-

(require 'ert)
(require 'cl-lib)
(require 'ls-lisp)
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

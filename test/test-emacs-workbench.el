;;; test-emacs-workbench.el --- Focused workbench tests -*- lexical-binding: t; -*-

(require 'ert)
(require 'cl-lib)
(require 'ls-lisp)
(require 'subr-x)

(defgroup myconfig nil "Test workbench." :group 'environment)
(provide 'myconfig-core)
(provide 'myconfig-terminal)
(provide 'myconfig-windows)
(provide 'ghostel)
(provide 'evil-ghostel)
(defvar myconfig-data-directory temporary-file-directory)
(defvar atelier-preserve-job-recipe nil)
(defvar atelier-agent-restored-functions nil)

(add-to-list 'load-path
             (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/"
                               (file-name-directory (or load-file-name buffer-file-name))))

(let ((lisp-directory
       (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/"
                         (file-name-directory (or load-file-name buffer-file-name)))))
  (add-to-list 'load-path (expand-file-name "atelier" lisp-directory))
  (load (expand-file-name "atelier/atelier.el" lisp-directory) nil t)
  (load (expand-file-name "myconfig-terminal.el" lisp-directory) nil t)
  (load (expand-file-name "aipan.el" lisp-directory) nil t)
  (load (expand-file-name "atelier/aipanel-atelier.el" lisp-directory) nil t)
  (load (expand-file-name "myconfig-persist.el" lisp-directory) nil t))

(ert-deftest atelier-new-buffer-inherits-frame-workspace ()
  (let* ((workspace-one (list :id "one-id" :name "one" :entries nil))
         (workspace-two (list :id "two-id" :name "two" :entries nil))
         (atelier-workspaces (list workspace-one workspace-two))
         (old-selection (atelier-current-workspace-id))
         (source (generate-new-buffer "source"))
         (created (generate-new-buffer "created")))
    (unwind-protect
        (progn
          (atelier-select-workspace workspace-two)
          (atelier-assign-buffer-to-workspace source workspace-one)
          (with-current-buffer created
            (atelier-own-current-buffer)
            (should-not (atelier-workspace-entry-for-buffer workspace-one created))
            (should (atelier-workspace-entry-for-buffer workspace-two created))
            (should-not (local-variable-p 'atelier-buffer-workspace created))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (dolist (buffer (list source created))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest atelier-detached-buffer-has-canonical-workspace-context ()
  (let* ((atelier-workspaces nil)
         (old-selection (atelier-current-workspace-id))
         (source (generate-new-buffer "detached-source")))
    (unwind-protect
        (let ((detached (atelier-ensure-detached-workspace)))
          (atelier-select-workspace detached)
          (atelier-assign-buffer-to-workspace source detached)
          (should (eq (atelier-workspace-for-buffer source) detached))
          (should (atelier-detached-workspace-p detached))
          (should (equal (plist-get detached :name) "Detached")))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (when (buffer-live-p source) (kill-buffer source)))))

(ert-deftest atelier-entry-move-to-detached-preserves-live-buffer ()
  (let* ((source (list :id "move-source" :name "source" :entries nil))
         (atelier-workspaces (list source))
         (detached (atelier-ensure-detached-workspace))
         (buffer (generate-new-buffer "detached-move-buffer")))
    (unwind-protect
        (let ((entry (atelier-register-buffer buffer source)))
          (setf (plist-get entry :displayed) t
                (plist-get entry :selected) t)
          (atelier-entry-move entry source detached)
          (should-not (atelier-workspace-entries source))
          (should (equal (atelier-workspace-entries detached) (list entry)))
          (should-not (plist-get entry :displayed))
          (should-not (plist-get entry :selected))
          (should (eq (atelier-entry-live-buffer entry) buffer)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-workspace-captures-and-restores-recursive-entry-layout ()
  (let* ((workspace (list :id "tree-workspace" :name "tree"
                          :destination "local" :path "/tmp/"
                          :status 'running :entries nil))
         (atelier-workspaces (list workspace))
         (old-selection (atelier-current-workspace-id))
         (one (generate-new-buffer "*scratch-tree-one*"))
         (two (generate-new-buffer "*scratch-tree-two*"))
         (three (generate-new-buffer "*scratch-tree-three*")))
    (unwind-protect
        (save-window-excursion
          (atelier-select-workspace workspace)
          (delete-other-windows)
          (set-window-buffer (selected-window) one)
          (let ((right (split-window-right)))
            (set-window-buffer right two)
            (set-window-buffer (split-window right nil 'below) three))
          (cl-letf (((symbol-function 'display-graphic-p) (lambda (&optional _) t)))
            (atelier-capture-current-workspace))
          (let* ((root (atelier-workspace-displayed-entry workspace))
                 (ids (mapcar (lambda (entry) (plist-get entry :id))
                              (atelier-workspace-displayed-entries workspace)))
                 (layout-ids
                  (list (plist-get root :id)
                        (plist-get (nth 1 (atelier-entry-children root)) :id))))
            (should (atelier-layout-entry-p root))
            (should (eq (plist-get root :orientation) 'horizontal))
            (should (atelier-layout-entry-p (nth 1 (atelier-entry-children root))))
            (should (eq (plist-get (nth 1 (atelier-entry-children root)) :orientation)
                        'vertical))
            (should-not (plist-member workspace :layout))
            (should-not (plist-member workspace :state))
            (let ((copy (atelier-workspace-persistent-copy workspace)))
              (should (atelier-layout-entry-p
                       (atelier-workspace-displayed-entry copy)))
              (should (= (length (atelier-workspace-displayed-entries copy)) 3)))
            (delete-other-windows)
            (cl-letf (((symbol-function 'atelier-workspace-directory)
                       (lambda (&optional _) "/tmp/")))
              (atelier-restore-workspace workspace))
            (should (= (length (window-list nil 'no-minibuffer)) 3))
            (should (equal (mapcar #'window-buffer (window-list nil 'no-minibuffer))
                           (list one two three)))
            (cl-letf (((symbol-function 'display-graphic-p) (lambda (&optional _) t)))
              (atelier-capture-current-workspace))
            (should (equal ids
                           (mapcar (lambda (entry) (plist-get entry :id))
                                   (atelier-workspace-displayed-entries workspace))))
            (let ((recaptured (atelier-workspace-displayed-entry workspace)))
              (should (equal layout-ids
                             (list (plist-get recaptured :id)
                                   (plist-get (nth 1 (atelier-entry-children recaptured))
                                              :id)))))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (dolist (buffer (list one two three))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest atelier-detached-workspace-is-reserved ()
  (let* ((atelier-workspaces nil)
         (old-selection (atelier-current-workspace-id))
         (detached (atelier-ensure-detached-workspace)))
    (unwind-protect
        (progn
          (atelier-select-workspace detached)
          (should-error (atelier-rename-workspace) :type 'user-error)
          (should-error (atelier-edit-workspace) :type 'user-error)
          (should-error (atelier-close-workspace t) :type 'user-error)
          (should-error (atelier-delete-workspace-record detached) :type 'user-error))
      (set-frame-parameter nil 'atelier-workspace-id old-selection))))

(ert-deftest atelier-snapshot-always-persists-detached-workspace ()
  (let* ((workspace (list :id "snapshot-workspace" :name "work"
                          :destination "local" :path "/tmp/" :platform 'local
                          :status 'running :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-navigator-window-configurations t)
         (myconfig-snapshot-generation "generation")
         (old-selection (atelier-current-workspace-id)))
    (unwind-protect
        (progn
          (atelier-select-workspace workspace)
          (let* ((data (myconfig-snapshot-data))
                 (saved (plist-get data :workspaces)))
            (should (= (plist-get data :version) 6))
            (should (cl-find atelier-detached-workspace-id saved
                             :key (lambda (item) (plist-get item :id))
                             :test #'equal))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection))))

(ert-deftest atelier-terminal-in-detached-frame-belongs-to-detached-workspace ()
  (let* ((atelier-workspaces nil)
         (old-selection (atelier-current-workspace-id))
         (source (generate-new-buffer "detached-terminal-source"))
         (terminal (generate-new-buffer "detached-terminal-result"))
         captured-workspace captured-type captured-explicit)
    (unwind-protect
        (let ((detached (atelier-ensure-detached-workspace)))
          (atelier-select-workspace detached)
          (atelier-assign-buffer-to-workspace source detached)
          (cl-letf (((symbol-function 'myconfig-terminal-buffer)
                     (lambda (_name _directory _command _args owner-workspace
                                    _shell _agent type explicit)
                       (setq captured-workspace owner-workspace
                             captured-type type
                             captured-explicit explicit)
                       terminal))
                    ((symbol-function 'myconfig-home-directory) (lambda () "/tmp/"))
                    ((symbol-function 'myconfig-normalize-directory) #'identity)
                    ((symbol-function 'myconfig-wsl-workspace-p) (lambda (_workspace) nil))
                    ((symbol-function 'myconfig-windows-workspace-p) (lambda (_workspace) nil)))
           (with-current-buffer source (myconfig-terminal)))
          (should (eq captured-workspace detached))
          (should (eq captured-type 'terminal))
          (should-not captured-explicit))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (dolist (buffer (list source terminal))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest atelier-reserved-buffers-cannot-be-captured ()
  (let* ((workspace (list :name "empty"))
         (buffer (atelier-empty-workspace-buffer workspace)))
    (unwind-protect
        (with-current-buffer buffer
          (should (gethash buffer atelier-internal-buffers))
          (should-not (atelier-capture-buffer buffer)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-current-workspace-is-a-frame-query ()
  (let* ((workspace (list :id "frame-workspace" :name "frame" :entries nil))
         (atelier-workspaces (list workspace))
         (old-selection (atelier-current-workspace-id)))
    (unwind-protect
        (progn
          (atelier-select-workspace workspace)
          (should (eq (atelier-current-workspace) workspace))
          (should (eq (atelier-current-workspace (selected-frame)) workspace))
          (should (equal (atelier-current-workspace-id) "frame-workspace"))
          (atelier-select-workspace nil)
          (should (equal (atelier-current-workspace-id)
                         atelier-detached-workspace-id))
          (should (atelier-detached-workspace-p (atelier-current-workspace))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection))))

(ert-deftest atelier-entry-survives-live-buffer-rename ()
  (let* ((workspace (list :id "rename-workspace" :name "rename" :entries nil))
         (atelier-workspaces (list workspace))
         (old-selection (atelier-current-workspace-id))
         (buffer (generate-new-buffer "entry-before-rename")))
    (unwind-protect
        (progn
          (atelier-select-workspace workspace)
          (let* ((entry (atelier-register-buffer buffer workspace))
                 (entry-id (plist-get entry :id)))
            (with-current-buffer buffer
              (rename-buffer "entry-after-rename")
              (atelier-refresh-current-buffer-entries))
            (should (eq (atelier-entry-by-id workspace entry-id) entry))
            (should (eq (atelier-entry-live-buffer entry) buffer))
            (should (equal (plist-get entry :name) "entry-after-rename"))
            (should-not (local-variable-p 'atelier-buffer-workspace buffer))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-same-live-buffer-cannot-enter-two-workspaces ()
  (let* ((file (make-temp-file "atelier-shared-file"))
         (buffer (find-file-noselect file))
         (one (list :id "one" :name "one" :entries nil))
         (two (list :id "two" :name "two" :entries nil))
         (atelier-workspaces (list one two)))
    (unwind-protect
        (let ((one-entry (atelier-register-buffer buffer one)))
          (should (eq (atelier-entry-live-buffer one-entry) buffer))
          (should-not (atelier-register-buffer buffer two))
          (should-not (atelier-workspace-entries two)))
      (when (buffer-live-p buffer) (kill-buffer buffer))
      (delete-file file))))

(ert-deftest atelier-same-file-uses-separate-workspace-buffers ()
  (let* ((file (make-temp-file "atelier-private-file"))
         (one (list :id "file-one" :name "one" :entries nil))
         (two (list :id "file-two" :name "two" :entries nil))
         (atelier-workspaces (list one two))
         first second)
    (unwind-protect
        (progn
          (setq first (atelier-file-buffer file one))
          (atelier-register-buffer first one)
          (setq second (atelier-file-buffer file two))
          (atelier-register-buffer second two)
          (should-not (eq first second))
          (should (eq (caar (atelier-entries-for-buffer first)) one))
          (should (eq (caar (atelier-entries-for-buffer second)) two))
          (should (= (length (atelier-entries-for-buffer first)) 1))
          (should (= (length (atelier-entries-for-buffer second)) 1)))
      (dolist (buffer (delete-dups (list first second)))
        (when (buffer-live-p buffer) (kill-buffer buffer)))
      (delete-file file))))

(ert-deftest atelier-non-file-buffer-keeps-its-first-workspace-owner ()
  (let* ((buffer (generate-new-buffer "owned-transient"))
         (one (list :id "owner-one" :name "one" :entries nil))
         (two (list :id "owner-two" :name "two" :entries nil))
         (atelier-workspaces (list one two))
         (old-selection (atelier-current-workspace-id)))
    (unwind-protect
        (progn
          (atelier-register-buffer buffer one)
          (atelier-select-workspace two)
          (set-window-buffer (selected-window) buffer)
          (atelier-register-visible-frame-buffers (selected-frame))
          (should (atelier-workspace-entry-for-buffer one buffer))
          (should-not (atelier-workspace-entry-for-buffer two buffer))
          (let ((atelier-capturing-layout-p t))
            (should-not (atelier-capture-buffer buffer nil two))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-dired-buffer-is-private-to-its-workspace ()
  (let* ((directory (make-temp-file "atelier-dired-private-" t))
         (ls-lisp-use-insert-directory-program nil)
         (ls-lisp-dirs-first t)
         (one (list :id "dired-one" :name "one" :entries nil))
         (two (list :id "dired-two" :name "two" :entries nil))
         (atelier-workspaces (list one two))
         first second)
    (unwind-protect
        (progn
          (make-directory (expand-file-name "b-dir" directory))
          (make-directory (expand-file-name "a-dir" directory))
          (write-region "" nil (expand-file-name "b-file" directory) nil 'silent)
          (write-region "" nil (expand-file-name "a-file" directory) nil 'silent)
          (setq first (atelier-new-dired-buffer directory nil one))
          (atelier-register-buffer first one)
          (setq second (atelier-new-dired-buffer directory nil two))
          (should-not (eq first second))
          (should (equal (buffer-name first) "*dired:one*"))
          (should (equal (buffer-name second) "*dired:two*"))
          (with-current-buffer second
            (goto-char (point-min))
            (let (names)
              (while (not (eobp))
                (when-let* ((name (dired-get-filename 'no-dir t)))
                  (unless (member name '("." ".."))
                    (push name names)))
                (forward-line 1))
              (should (equal (nreverse names)
                             '("a-dir" "b-dir" "a-file" "b-file")))))
          (atelier-register-buffer second two)
          (should (atelier-workspace-entry-for-buffer one first))
          (should (atelier-workspace-entry-for-buffer two second))
          (should-not (atelier-workspace-entry-for-buffer one second))
          (should-not (atelier-workspace-entry-for-buffer two first)))
      (dolist (buffer (delete-dups (list first second)))
        (when (buffer-live-p buffer) (kill-buffer buffer)))
      (delete-directory directory t))))

(ert-deftest atelier-entry-types-reuse-by-default-and-allow-explicit-duplicates ()
  (let* ((directory (make-temp-file "atelier-dired-type-" t))
         (workspace (list :id "typed-workspace" :name "typed" :entries nil))
         (atelier-workspaces (list workspace))
         (ls-lisp-use-insert-directory-program nil)
         (ls-lisp-dirs-first t)
         first second first-entry second-entry)
    (unwind-protect
        (progn
          (setq first (atelier-new-dired-buffer directory t workspace)
                first-entry (atelier-register-buffer first workspace nil 'dired))
          (setq second (atelier-new-dired-buffer directory t workspace))
          (should-not (atelier-register-buffer second workspace nil 'dired))
          (setq second-entry (atelier-register-buffer second workspace nil 'dired t))
          (should (eq (atelier-workspace-entry-by-type workspace 'dired) first-entry))
          (should (eq (atelier-workspace-buffer-by-type workspace 'dired) first))
          (should (eq (plist-get first-entry :type) 'dired))
          (should (eq (plist-get second-entry :type) 'dired))
          (should (equal (buffer-name first) "*dired:typed*"))
          (should (equal (buffer-name second) "*dired:typed*<2>"))
          (atelier-entry-set-live-buffer first-entry nil)
          (should (eq (atelier-register-buffer first workspace nil 'dired)
                      first-entry))
          (should (eq (atelier-entry-live-buffer first-entry) first))
          (let ((atelier-entry-types (copy-tree atelier-entry-types)))
            (atelier-register-entry-type
             'preview "preview" (lambda (buffer) (eq buffer second)))
            (should (equal (atelier-entry-buffer-name 'preview workspace)
                           "*preview:typed*"))
            (should (eq (atelier-buffer-entry-type second) 'dired))
            (setf (plist-get (cdr (assq 'dired atelier-entry-types)) :buffer-p) nil)
            (should (eq (atelier-buffer-entry-type second) 'preview))))
      (dolist (buffer (delete-dups (list first second)))
        (when (buffer-live-p buffer) (kill-buffer buffer)))
      (delete-directory directory t))))

(ert-deftest atelier-migrates-v5-entry-types-without-removing-duplicates ()
  (let* ((data '(:version 5 :generation "typed-v5"
                 :current-workspace-id "typed-id" :ssh-destinations nil
                 :workspaces
                 ((:id "typed-id" :name "typed" :destination "local"
                   :path "/tmp/" :status running
                   :entries
                   ((:id "dired-one" :kind directory :name "one")
                    (:id "dired-two" :kind directory :name "two")
                    (:id "terminal" :kind terminal :name "terminal"
                     :job (:id "terminal-job" :buffer "terminal"
                           :policy auto :agent t))
                    (:id "aipanel" :kind terminal :name "aipanel"
                     :job (:id "aipanel-job" :buffer "aipanel"
                           :policy auto :agent (:id opencode)))
                    (:id "legacy-aipanel" :kind terminal :name "legacy-aipanel"
                     :job (:id "legacy-aipanel-job" :buffer "legacy-aipanel"
                           :policy auto :agent t
                           :direct-command ("/usr/bin/opencode" "--auto")))
                    (:id "leaked" :kind terminal :name "leaked"))))))
         (migrated (myconfig-validate-state data))
         (entries (atelier-workspace-entries
                   (car (plist-get migrated :workspaces)))))
    (should (= (plist-get migrated :version) 6))
    (should (equal (mapcar (lambda (entry) (plist-get entry :type)) entries)
                   '(dired dired terminal aipanel aipanel nil)))))

(ert-deftest atelier-dired-navigation-keeps-one-buffer-and-entry ()
  (let* ((root (make-temp-file "atelier-dired-navigation-" t))
         (child (file-name-as-directory (expand-file-name "child" root)))
         (workspace (list :id "dired-navigation" :name "navigation"
                          :entries nil))
         (atelier-workspaces (list workspace))
         (ls-lisp-use-insert-directory-program nil)
         (ls-lisp-dirs-first t)
         buffer entry)
    (unwind-protect
        (progn
          (make-directory child)
          (write-region "" nil (expand-file-name "inside" child) nil 'silent)
          (setq buffer (atelier-new-dired-buffer root t workspace)
                entry (atelier-register-buffer buffer workspace))
          (with-current-buffer buffer
            (should (equal (buffer-name) "*dired:navigation*"))
            (atelier-dired-change-directory child)
            (should (eq (current-buffer) buffer))
            (should (equal (buffer-name) "*dired:navigation*"))
            (should (equal default-directory child))
            (should (dired-goto-file (expand-file-name "inside" child)))
            (should (equal (atelier-workspace-entries workspace) (list entry)))
            (should (equal (plist-get entry :directory) child))
            (atelier-dired-up-directory)
            (should (eq (current-buffer) buffer))
            (should (equal (buffer-name) "*dired:navigation*"))
            (should (equal default-directory (file-name-as-directory root)))
            (should (equal (atelier-workspace-entries workspace) (list entry)))
            (should (equal (plist-get entry :directory)
                           (file-name-as-directory root)))))
      (when (buffer-live-p buffer) (kill-buffer buffer))
      (delete-directory root t))))

(ert-deftest atelier-scratch-entry-persists-its-text ()
  (let* ((workspace (list :id "scratch-workspace" :name "scratch" :entries nil))
         (atelier-workspaces (list workspace))
         (buffer (generate-new-buffer "*scratch-entry-test*")))
    (unwind-protect
        (with-current-buffer buffer
          (insert "persistent notes")
          (let ((entry (atelier-register-buffer buffer workspace)))
            (should (eq (plist-get entry :kind) 'scratch))
            (should (equal (plist-get entry :contents) "persistent notes"))))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-native-buffer-kill-removes-only-current-workspace-entry ()
  (let* ((buffer (generate-new-buffer "*scratch-native-kill-entry*"))
         (other-buffer (generate-new-buffer "*scratch-native-kill-entry*"))
         (one (list :id "kill-one" :name "one" :entries nil))
         (two (list :id "kill-two" :name "two" :entries nil))
         (atelier-workspaces (list one two))
         (old-selection (atelier-current-workspace-id)))
    (unwind-protect
        (progn
          (atelier-select-workspace one)
          (atelier-register-buffer buffer one)
          (let ((other-entry (atelier-register-buffer other-buffer two)))
            (with-current-buffer buffer (atelier-current-buffer-killed))
            (should-not (atelier-workspace-entries one))
            (should (equal (atelier-workspace-entries two) (list other-entry)))
            (should (eq (atelier-entry-live-buffer other-entry) other-buffer))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (dolist (item (list buffer other-buffer))
        (when (buffer-live-p item) (kill-buffer item))))))

(ert-deftest atelier-dired-mouse-open-uses-the-current-buffer-path ()
  (let (point-set opened)
    (cl-letf (((symbol-function 'mouse-set-point)
               (lambda (event) (setq point-set event)))
              ((symbol-function 'atelier-dired-open)
               (lambda () (setq opened t))))
      (atelier-dired-mouse-open 'click)
      (should (eq point-set 'click))
      (should opened))))

(ert-deftest atelier-dired-open-uses-detached-workspace-normally ()
  (let* ((atelier-workspaces nil)
         (old-selection (atelier-current-workspace-id))
         (source (generate-new-buffer "detached-dired-source"))
         (opened (generate-new-buffer "detached-dired-opened")))
    (unwind-protect
        (let ((detached (atelier-ensure-detached-workspace)))
          (atelier-select-workspace detached)
          (atelier-assign-buffer-to-workspace source detached)
          (with-current-buffer source
            (cl-letf (((symbol-function 'dired-get-file-for-visit)
                       (lambda () "/path/that/is/not/a/directory"))
                      ((symbol-function 'atelier-open-file)
                       (lambda (_file &optional workspace)
                         (atelier-assign-buffer-to-workspace
                          opened (or workspace (atelier-current-workspace))))))
              (atelier-dired-open)))
          (should (atelier-workspace-entry-for-buffer detached opened)))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (dolist (buffer (list source opened))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest atelier-discovers-ssh-destinations-in-shell-histories ()
  (let ((zsh-history (make-temp-file "atelier-zsh-history"))
        (fish-history (make-temp-file "atelier-fish-history")))
    (unwind-protect
        (progn
          (with-temp-file zsh-history
            (insert ": 1700000000:0;git status\n"
                    ": 1700000001:0;ssh -p 2222 first@example.com\n"
                    ": 1700000002:0;ssh -J jump@proxy latest@10.0.0.8\n"))
          (with-temp-file fish-history
            (insert "- cmd: echo ssh fake@example.com\n"
                    "  when: 1700000003\n"
                    "- cmd: ssh fish@server.internal\n"))
          (let ((atelier-shell-history-files (list zsh-history fish-history)))
            (should
             (equal (sort (atelier-shell-history-ssh-destinations) #'string-lessp)
                    '("first@example.com" "fish@server.internal" "latest@10.0.0.8")))))
      (delete-file zsh-history)
      (delete-file fish-history))))

(ert-deftest atelier-parses-ssh-options-before-the-destination ()
  (should (equal (atelier-ssh-destination-from-command
                  "command ssh -i ~/.ssh/work -o StrictHostKeyChecking=no user@192.0.2.5")
                 "user@192.0.2.5"))
  (should-not (atelier-ssh-destination-from-command "printf 'ssh fake@example.com'")))

(ert-deftest atelier-migrates-global-name-state-to-stable-ids ()
  (let* ((data '(:version 2 :generation "old" :current-workspace "two"
                          :ssh-destinations nil
                          :workspaces ((:name "one" :destination "local" :path "/tmp/"
                                              :live nil :buffers nil :owned-buffers nil :jobs nil)
                                       (:name "two" :destination "local" :path "/tmp/"
                                              :live t :buffers nil :owned-buffers nil :jobs nil))))
         (migrated (myconfig-validate-state data))
         (workspaces (plist-get migrated :workspaces))
         (selected (cl-find (plist-get migrated :current-workspace-id) workspaces
                            :key (lambda (workspace) (plist-get workspace :id))
                            :test #'equal)))
    (should (= (plist-get migrated :version) 6))
    (should (equal (plist-get selected :name) "two"))
    (should (cl-every (lambda (workspace)
                        (and (stringp (plist-get workspace :id))
                             (memq (plist-get workspace :status) '(running stopped))
                             (not (plist-member workspace :live))))
                      workspaces))))

(ert-deftest atelier-migrates-v4-layout-into-entry-tree ()
  (let* ((data '(:version 4 :generation "v4" :current-workspace-id "work-id"
                          :ssh-destinations nil
                          :workspaces ((:id "work-id" :name "work" :destination "local"
                                            :path "/tmp/" :status running
                                            :state (nil hc
                                                        (nil leaf)
                                                        (nil vc (nil leaf) (nil leaf)))
                                            :layout ((:entry-id "one" :selected t)
                                                     (:entry-id "two")
                                                     (:entry-id "three"))
                                            :entries ((:id "one" :kind file :name "one" :persistent t)
                                                      (:id "two" :kind scratch :name "two"
                                                           :persistent t)
                                                      (:id "three" :kind directory :name "three"
                                                           :persistent t))))))
         (migrated (myconfig-validate-state data))
         (workspace (car (plist-get migrated :workspaces)))
         (root (atelier-workspace-displayed-entry workspace)))
    (should (= (plist-get migrated :version) 6))
    (should (atelier-layout-entry-p root))
    (should (equal (mapcar (lambda (entry) (plist-get entry :id))
                           (atelier-workspace-displayed-entries workspace))
                   '("one" "two" "three")))
    (should (eq (plist-get root :orientation) 'horizontal))
    (should (eq (plist-get (nth 1 (atelier-entry-children root)) :orientation)
                'vertical))
    (should-not (plist-member workspace :layout))
    (should-not (plist-member workspace :state))))

(ert-deftest atelier-removes-dead-saved-entry-from-layout-tree ()
  (let* ((workspace (list :id "saved-id" :name "saved"
                          :entries '((:id "layout-id" :kind layout :displayed t
                                          :orientation horizontal :ratio 0.5
                                          :children ((:id "gone-id" :kind file :name "gone")
                                                     (:id "kept-id" :kind file :name "kept"))))))
         (atelier-workspaces (list workspace))
         (atelier-change-hook nil))
    (atelier-remove-saved-workspace-buffer workspace "gone-id")
    (should (equal (mapcar (lambda (item) (plist-get item :name))
                           (atelier-workspace-entries workspace))
                   '("kept")))
    (should (eq (atelier-workspace-displayed-entry workspace)
                (car (atelier-workspace-entries workspace))))))

(ert-deftest atelier-deletes-stopped-workspace-without-opening-it ()
  (let* ((current (list :id "current-id" :name "current" :status 'running
                        :entries nil))
         (stopped (list :id "stopped-id" :name "stopped" :status 'stopped
                        :entries nil))
         (atelier-workspaces (list current stopped))
         (atelier-change-hook nil)
         (old-selection (atelier-current-workspace-id)))
    (unwind-protect
        (progn
          (atelier-select-workspace current)
          (cl-letf (((symbol-function 'myconfig-windows-workspace-p) (lambda (_) nil)))
            (atelier-delete-workspace-record stopped))
          (should (equal atelier-workspaces (list current)))
          (should (eq (atelier-current-workspace) current)))
      (set-frame-parameter nil 'atelier-workspace-id old-selection))))

(ert-deftest atelier-navigator-writes-workspace-statuses ()
  (let* ((current (list :id "current-id" :name "current" :status 'running
                        :entries nil))
         (stopped (list :id "stopped-id" :name "stopped" :status 'stopped
                        :entries nil))
         (atelier-workspaces (list current stopped))
         (old-selection (atelier-current-workspace-id)))
    (unwind-protect
        (progn
          (atelier-select-workspace current)
          (cl-letf (((symbol-function 'atelier-workspace-project-root) (lambda (_) nil))
                    ((symbol-function 'atelier-known-project-roots) (lambda () nil)))
            (with-current-buffer (atelier-render-navigator)
              (should (string-match-p "current/  (current)" (buffer-string)))
              (should (string-match-p "stopped/  (stopped)" (buffer-string)))
              (goto-char (point-min))
              (should (search-forward "Clear all buffers" nil t))
              (should (eq (get-text-property (match-beginning 0) 'face) 'error)))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (when-let* ((buffer (get-buffer atelier-navigator-buffer)))
        (kill-buffer buffer)))))

(ert-deftest atelier-navigator-renders-terminal-as-an-entry ()
  (let* ((entry '(:id "terminal-entry" :kind terminal :name "terminal:work"
                      :persistent t
                      :job (:id "terminal-job" :buffer "terminal:work"
                                :policy always :recipe (:executable "bash"))))
         (workspace (list :id "work-id" :name "work" :status 'running
                          :entries (list entry)))
         (atelier-workspaces (list workspace))
         (old-selection (atelier-current-workspace-id)))
    (unwind-protect
        (progn
          (atelier-select-workspace workspace)
          (cl-letf (((symbol-function 'atelier-workspace-project-root) (lambda (_) nil))
                    ((symbol-function 'atelier-known-project-roots) (lambda () nil)))
            (with-current-buffer (atelier-render-navigator)
              (goto-char (point-min))
              (should (search-forward "terminal:work" nil t))
              (should (equal (get-text-property (match-beginning 0)
                                                'atelier-navigator-target)
                             '(workspace-owned-buffer "work" "terminal-entry"))))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (when-let* ((buffer (get-buffer atelier-navigator-buffer)))
        (kill-buffer buffer)))))

(ert-deftest atelier-navigator-detach-moves-entry-to-reserved-workspace ()
  (let* ((entry '(:id "detach-entry" :kind scratch :name "notes"
                      :persistent t :contents "text"))
         (workspace (list :id "detach-source" :name "work" :status 'running
                          :entries (list entry)))
         (atelier-workspaces (list workspace))
         (old-selection (atelier-current-workspace-id)))
    (unwind-protect
        (progn
          (atelier-select-workspace workspace)
          (cl-letf (((symbol-function 'atelier-workspace-project-root) (lambda (_) nil))
                    ((symbol-function 'atelier-known-project-roots) (lambda () nil)))
            (with-current-buffer (atelier-render-navigator)
              (goto-char (point-min))
              (search-forward "notes")
              (goto-char (match-beginning 0))
              (atelier-navigator-detach)))
          (should-not (atelier-workspace-entries workspace))
          (should (equal (atelier-workspace-entries (atelier-detached-workspace))
                         (list entry))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (when-let* ((buffer (get-buffer atelier-navigator-buffer)))
        (kill-buffer buffer)))))

(ert-deftest atelier-navigator-header-controls-are-clickable ()
  (let* ((atelier-navigator-attach-source nil)
         (header (apply #'concat (atelier-navigator-header)))
         (position (string-match (regexp-quote "[Open]") header))
         (map (and position (get-text-property position 'keymap header))))
    (should position)
    (should (keymapp map))
    (should (eq (lookup-key map [header-line mouse-1])
                #'atelier-navigator-open))))

(ert-deftest aipanel-builds-provider-specific-arguments ()
  (let ((agent '(:arguments ("--auto") :mini-arguments ("--mini")
                            :project-argument t)))
    (should (equal (aipanel-agent-arguments agent "/project/" nil)
                   '("/project/" "--auto")))
    (should (equal (aipanel-agent-arguments agent "/project/" t)
                   '("/project/" "--auto" "--mini")))
    (should (equal (aipanel-agent-arguments agent "/project/" t t)
                   '("." "--auto" "--mini")))))

(ert-deftest aipanel-lists-installed-host-and-wsl-agents ()
  (let ((aipanel-agents
         '((:id opencode :name "OpenCode" :program "opencode")
           (:id fx :name "fx" :program "fx")
           (:id missing :name "Missing" :program "missing"))))
    (cl-letf (((symbol-function 'executable-find)
               (lambda (program) (and (member program '("opencode" "wsl.exe")) program)))
              ((symbol-function 'aipanel-run-wsl-probe)
               (lambda (&optional _distribution)
                 '(:distribution "Ubuntu" :programs ("fx")))))
      (let ((labels (mapcar #'car (aipanel-candidates))))
        (should (equal labels '("OpenCode (host)" "fx (WSL: Ubuntu)")))))))

(ert-deftest aipanel-uses-the-only-installed-agent-without-prompting ()
  (cl-letf (((symbol-function 'aipanel-candidates)
             (lambda () '(("fx (host)" . (:agent (:id fx) :location host)))))
            ((symbol-function 'completing-read)
             (lambda (&rest _arguments) (ert-fail "A single agent should not prompt"))))
    (should (equal (plist-get (aipanel-read-agent) :location) 'host))))

(ert-deftest aipanel-cleanup-emits-one-lifecycle-event ()
  (let ((buffer (generate-new-buffer " *aipanel-cleanup-test*"))
        (aipanel-sessions (make-hash-table :test #'equal))
        (aipanel-buffer-exited-hook nil)
        (events 0))
    (unwind-protect
        (progn
          (add-hook 'aipanel-buffer-exited-hook (lambda () (cl-incf events)))
          (aipanel-adopt-buffer
           buffer '(:id "project" :name "project" :directory "/tmp/")
           '(:agent (:id fx :program "fx") :location host))
          (aipanel-cleanup-buffer buffer)
          (aipanel-cleanup-buffer buffer)
          (should (= events 1))
          (should-not (gethash "project" aipanel-sessions)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest aipanel-atelier-setup-uses-explicit-strategies-and-hooks ()
  (let ((aipanel-owner-function #'aipanel-default-owner)
        (aipanel-command-function #'aipanel-default-command)
        (aipanel-context-function #'aipanel-default-context)
        (aipanel-terminal-function #'aipanel-default-terminal)
        (aipanel-buffer-created-hook nil)
        (aipanel-buffer-exited-hook nil)
        (aipanel-window-change-hook nil)
        (atelier-agent-restored-functions nil))
    (aipanel-atelier-setup)
    (should (eq aipanel-owner-function #'aipanel-atelier-owner))
    (should (eq aipanel-command-function #'aipanel-atelier-command))
    (should (memq #'aipanel-atelier-buffer-created aipanel-buffer-created-hook))
    (should (memq #'aipanel-atelier-buffer-exited aipanel-buffer-exited-hook))
    (should (memq #'aipanel-atelier-restore-buffer atelier-agent-restored-functions))))

(ert-deftest aipanel-atelier-resolves-owner-by-stable-workspace-id ()
  (let* ((workspace '(:id "stable-workspace" :name "renamable" :entries nil))
         (atelier-workspaces (list workspace))
         captured-owner captured-type)
    (cl-letf (((symbol-function 'myconfig-terminal-buffer)
                (lambda (_name _directory _program _arguments owner-workspace
                               _shell _agent type &rest _arguments)
                  (setq captured-owner owner-workspace
                        captured-type type)
                  'terminal-buffer)))
      (should
       (eq (aipanel-atelier-terminal
            "agent" "/tmp/" "agent" nil
            '(:workspace-id "stable-workspace")
            '(:agent (:id fx)))
            'terminal-buffer))
      (should (eq captured-owner workspace))
      (should (eq captured-type 'aipanel)))))

(ert-deftest aipanel-wraps-wsl-launch ()
  (let* ((owner '(:name "work" :directory "/project/"))
         (agent '(:id fx :name "fx" :program "fx" :arguments ("--safe")))
         (selection (list :agent agent :location 'wsl :distribution "Ubuntu"))
         (command (aipanel-default-command owner selection nil)))
    (should (equal (plist-get command :program) "wsl.exe"))
    (should (equal (plist-get command :arguments)
                   '("-d" "Ubuntu" "--cd" "/project/" "--" "fx" "--safe")))))

(ert-deftest aipanel-ignores-stale-context-request ()
  (let ((buffer (generate-new-buffer " *aipanel-context-test*")) sent)
    (unwind-protect
        (progn
          (with-current-buffer buffer
            (setq-local aipanel-context-generation 2))
          (cl-letf (((symbol-function 'aipanel-send-context)
                     (lambda (_buffer context) (setq sent context))))
            (aipanel-send-context-when-ready
             buffer "stale" (+ (float-time) 10) 1)
            (should-not sent)))
      (kill-buffer buffer))))

(ert-deftest aipanel-closes-its-dedicated-window ()
  (save-window-excursion
    (delete-other-windows)
    (let* ((buffer (generate-new-buffer " *aipanel-window-test*"))
           (window (split-window-right)))
      (unwind-protect
          (progn
            (set-window-buffer window buffer)
            (set-window-parameter window 'window-side 'right)
            (set-window-dedicated-p window t)
            (aipanel-close-windows buffer)
            (should-not (window-live-p window)))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest aipanel-uses-a-real-side-window ()
  (save-window-excursion
    (delete-other-windows)
    (let ((buffer (generate-new-buffer " *aipanel-side-window-test*")))
      (unwind-protect
          (let ((window (aipanel-display-buffer buffer 30)))
            (should (eq (window-parameter window 'window-side) 'left))
            (should (window-dedicated-p window)))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest aipanel-exit-removes-the-atelier-job ()
  (let* ((buffer (generate-new-buffer " *aipanel-exit-test*"))
         (job (list :buffer (buffer-name buffer) :agent '(:id fx) :recipe '(:executable "fx")))
         (entry (list :id "agent-entry" :kind 'terminal :name (buffer-name buffer) :job job))
         (workspace (list :name "work" :entries (list entry)))
         (atelier-preserve-job-recipe nil))
    (unwind-protect
        (cl-letf (((symbol-function 'atelier-find-job-for-buffer)
                   (lambda (_name) (list workspace job entry)))
                  ((symbol-function 'atelier-notify-change) #'ignore)
                  ((symbol-function 'myconfig-log) #'ignore)
                  ((symbol-function 'myconfig-persist-schedule) #'ignore))
          (myconfig-job-process-exited buffer)
          (should-not (atelier-workspace-entries workspace)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-deftest atelier-job-observer-preserves-stopped-terminal-recipe ()
  (let* ((recipe '(:executable "/bin/bash" :argv ("/bin/bash" "-l")
                               :directory "/tmp/"))
         (job (list :id "stopped-job" :buffer "*stopped-terminal*"
                    :policy 'auto :recipe recipe :shell nil))
         (entry (list :id "stopped-entry" :kind 'terminal
                      :name "*stopped-terminal*" :persistent t :job job))
         (atelier-workspaces
          (list (list :id "stopped-workspace" :name "stopped"
                      :destination "local" :entries (list entry)))))
    (cl-letf (((symbol-function 'myconfig-proc-table) (lambda () nil))
              ((symbol-function 'myconfig-persist-now)
               (lambda () (ert-fail "A stopped entry should not change"))))
      (myconfig-observe-jobs)
      (should (equal (plist-get job :recipe) recipe)))))

(ert-run-tests-batch-and-exit)
;;; test-emacs-workbench.el ends here

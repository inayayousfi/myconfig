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
(provide 'simple-httpd)
(cl-defstruct websocket origin negotiated-protocols protocols ready-state)
(provide 'websocket)
(defvar myconfig-data-directory temporary-file-directory)
(defvar myconfig-state-directory temporary-file-directory)
(defvar myconfig-config-directory temporary-file-directory)
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
  (load (expand-file-name "myconfig-persist.el" lisp-directory) nil t)
  (load (expand-file-name "remot.el" lisp-directory) nil t))

(ert-deftest remot-generates-a-128-bit-hex-salt ()
  (let ((salt (remot-random-salt)))
    (should (= (length salt) 32))
    (should (string-match-p "\\`[0-9a-f]\\{32\\}\\'" salt))))

(ert-deftest remot-scrypt-verifier-is-stable ()
  (should
   (equal
    (remot-derive-password
     "test" "00112233445566778899aabbccddeeff")
    "36ca484c00a95223a59a83b73b06097297881884328639ff89e96ed252c4661e")))

(ert-deftest remot-set-password-stores-only-a-verifier ()
  (let ((passwords (list (copy-sequence "anything") (copy-sequence "anything")))
        (remot-state-directory "/test/remot/")
        (remot-started-p nil)
        written disconnected started)
    (cl-letf (((symbol-function 'read-passwd) (lambda (&rest _) (pop passwords)))
              ((symbol-function 'remot-random-salt)
               (lambda () "00112233445566778899aabbccddeeff"))
              ((symbol-function 'remot-derive-password)
               (lambda (_password _salt) (make-string 64 ?a)))
              ((symbol-function 'remot--write-data-atomically)
                (lambda (file value) (setq written (list file value))))
              ((symbol-function 'remot-disconnect-controller)
               (lambda () (setq disconnected t)))
              ((symbol-function 'remot-close-pending-websockets)
               #'ignore)
              ((symbol-function 'display-graphic-p) (lambda (&optional _) t))
              ((symbol-function 'remot-start)
               (lambda () (setq started t))))
      (remot-set-password)
      (let ((record (cadr written)))
        (should (equal (car written) "/test/remot/password.el"))
        (should (equal (plist-get record :algorithm) 'scrypt))
        (should (= (length (plist-get record :verifier)) 64))
        (should-not (member "anything" record)))
      (should disconnected)
      (should started))))

(ert-deftest remot-password-check-throttles-failures ()
  (let ((remot-password-record
         (list :version 1 :algorithm 'scrypt
               :salt "00112233445566778899aabbccddeeff"
               :verifier (make-string 64 ?a)))
        (remot-authentication-failures 0)
        (remot-next-authentication-time 0))
    (cl-letf (((symbol-function 'remot-derive-password)
               (lambda (password _salt)
                 (if (equal password "right") (make-string 64 ?a)
                   (make-string 64 ?b)))))
      (should-not (remot-password-valid-p "wrong"))
      (should (= remot-authentication-failures 1))
      (should (> remot-next-authentication-time (float-time)))
      (setq remot-next-authentication-time 0)
      (should (remot-password-valid-p "right"))
      (should (= remot-authentication-failures 0)))))

(ert-deftest remot-waits-for-password-before-creating-terminal ()
  (let ((websocket (make-websocket :origin "http://workbench.local:18080"))
        (remot-pending-websockets nil)
        started)
    (cl-letf (((symbol-function 'remot-start-terminal)
               (lambda () (setq started t))))
      (remot-websocket-open websocket)
      (should (memq websocket remot-pending-websockets))
      (should-not started))))

(ert-deftest remot-without-password-opens-no-listeners ()
  (let ((remot-password-record nil)
        started)
    (cl-letf (((symbol-function 'display-graphic-p) (lambda (&optional _) t))
              ((symbol-function 'remot-read-password-record)
               #'ignore)
              ((symbol-function 'add-hook) #'ignore)
              ((symbol-function 'myconfig-log) #'ignore)
              ((symbol-function 'remot-start)
               (lambda () (setq started t))))
      (remot-setup)
      (should-not started))))

(ert-deftest remot-authentication-replaces-controller ()
  (let* ((old (make-websocket :ready-state 'open))
         (new (make-websocket :ready-state 'open))
         (remot-websocket old)
         (remot-pending-websockets (list new))
         disconnected started sent)
    (cl-letf (((symbol-function 'remot-password-valid-p)
               (lambda (password) (equal password "right")))
              ((symbol-function 'remot-disconnect-controller)
               (lambda () (setq disconnected t
                                remot-websocket nil)))
              ((symbol-function 'remot-start-terminal)
               (lambda () (setq started t)))
              ((symbol-function 'websocket-send-text)
               (lambda (websocket text) (setq sent (list websocket text)))))
      (remot-authenticate
       new (list '(username . "emacs")
                 (cons 'password (copy-sequence "right"))))
      (should disconnected)
      (should started)
      (should (eq remot-websocket new))
      (should-not remot-pending-websockets)
      (should (equal sent (list new "{\"type\":\"authenticated\"}"))))))

(ert-deftest remot-wrong-password-creates-no-terminal ()
  (let ((websocket (make-websocket :ready-state 'open)) closed sent started)
    (cl-letf (((symbol-function 'remot-password-valid-p) #'ignore)
              ((symbol-function 'websocket-close)
               (lambda (candidate) (setq closed candidate)))
              ((symbol-function 'websocket-send-text)
               (lambda (candidate text) (setq sent (list candidate text))))
              ((symbol-function 'remot-start-terminal)
               (lambda () (setq started t))))
      (remot-authenticate
       websocket (list '(username . "emacs")
                       (cons 'password (copy-sequence "wrong"))))
      (should (eq closed websocket))
      (should (equal sent
                     (list websocket "{\"type\":\"authentication-failed\"}")))
      (should-not started))))

(ert-deftest remot-resize-validates-bounds ()
  (let ((remot-websocket 'websocket)
        (remot-process 'terminal-process)
        sizes)
    (cl-letf (((symbol-function 'websocket-frame-opcode) (lambda (_) 'text))
              ((symbol-function 'websocket-frame-text) #'identity)
              ((symbol-function 'process-live-p) (lambda (_) t))
              ((symbol-function 'set-process-window-size)
               (lambda (_process rows cols) (push (list rows cols) sizes))))
      (remot-websocket-message
       'websocket "{\"type\":\"resize\",\"cols\":120,\"rows\":40}")
      (remot-websocket-message
       'websocket "{\"type\":\"resize\",\"cols\":10,\"rows\":3}")
      (should (equal sizes '((40 120)))))))

(ert-deftest remot-disconnect-destroys-process-and-frame ()
  (let ((remot-process 'terminal-process)
        deleted-process deleted-frame)
    (cl-letf (((symbol-function 'process-live-p) (lambda (_) t))
              ((symbol-function 'delete-process)
               (lambda (process) (setq deleted-process process)))
              ((symbol-function 'remot-remote-frames)
               (lambda () '(terminal-frame)))
              ((symbol-function 'frame-live-p) (lambda (_) t))
              ((symbol-function 'delete-frame)
               (lambda (frame &optional _) (setq deleted-frame frame))))
      (remot-stop-terminal)
      (should (eq deleted-process 'terminal-process))
      (should (eq deleted-frame 'terminal-frame))
      (should-not remot-process))))

(ert-deftest remot-frame-calls-configured-initializer-with-context ()
  (let ((remot-context "work-id")
        initialized)
    (let ((remot-initialize-frame-function
           (lambda (context frame)
             (setq initialized (list context frame)))))
      (cl-letf (((symbol-function 'frame-parameter)
                 (lambda (_frame parameter) (eq parameter 'remot)))
                ((symbol-function 'selected-frame) (lambda () 'terminal-frame)))
        (remot-initialize-frame)
        (should (equal initialized '("work-id" terminal-frame)))))))

(ert-deftest remot-embeds-password-manager-form-and-runtime-settings ()
  (let ((remot-username "remote-user")
        (remot-websocket-port 19081))
    (cl-letf (((symbol-function 'httpd-escape-html) #'identity))
      (let ((html (decode-coding-string (remot--index-html) 'utf-8))
            (javascript (decode-coding-string (remot--app-js) 'utf-8)))
        (should (string-match-p
                 "name=\"username\"[^>]+value=\"remote-user\"[^>]+autocomplete=\"username\""
                 html))
        (should (string-match-p
                 "name=\"password\"[^>]+autocomplete=\"current-password\""
                 html))
        (should (string-match-p ":19081/terminal" javascript))
        (should (string-match-p "remot-v1" javascript))))))

(ert-deftest remot-last-graphical-frame-ends-emacs ()
  (let ((remot-exit-with-last-graphical-frame t)
        ended)
    (cl-letf (((symbol-function 'display-graphic-p)
               (lambda (frame) (eq frame 'graphical)))
              ((symbol-function 'frame-list)
               (lambda () '(graphical terminal)))
              ((symbol-function 'frame-live-p) (lambda (_) t))
              ((symbol-function 'kill-emacs) (lambda (&rest _) (setq ended t))))
      (remot-last-graphical-frame-closing 'graphical)
      (should ended))))

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
            (should (= (plist-get data :version) 7))
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

(ert-deftest atelier-migrates-v5-entry-types-and-discards-unattached-panels ()
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
    (should (= (plist-get migrated :version) 7))
    (should (equal (mapcar (lambda (entry) (plist-get entry :type)) entries)
                   '(dired dired terminal nil)))))

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

(ert-deftest atelier-close-only-view-selects-most-recent-workspace-entry ()
  (let* ((workspace (list :id "close-mru" :name "close-mru"
                          :destination "local" :path "/tmp/" :entries nil))
         (atelier-workspaces (list workspace))
         (old-selection (atelier-current-workspace-id))
         (older (generate-new-buffer "close-older"))
         (recent (generate-new-buffer "close-recent"))
         (closing (generate-new-buffer "close-current")))
    (unwind-protect
        (save-window-excursion
          (atelier-select-workspace workspace)
          (delete-other-windows)
          (dolist (buffer (list older recent closing))
            (atelier-register-buffer buffer workspace)
            (switch-to-buffer buffer))
          (cl-letf (((symbol-function 'display-graphic-p) (lambda (&optional _) t)))
            (atelier-capture-current-workspace)
            (atelier-close-current-view))
          (should-not (buffer-live-p closing))
          (should (eq (window-buffer) recent))
          (should (= (length (atelier-workspace-entries workspace)) 2))
          (should-not (string-prefix-p atelier-empty-buffer-prefix
                                       (buffer-name (window-buffer)))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (dolist (buffer (list older recent closing))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest atelier-close-only-view-restores-a-saved-workspace-entry ()
  (let* ((workspace (list :id "close-restore" :name "close-restore"
                          :destination "local" :path "/tmp/" :entries nil))
         (atelier-workspaces (list workspace))
         (old-selection (atelier-current-workspace-id))
         (closing (generate-new-buffer "close-before-restore"))
         (saved (list :id "saved-scratch" :kind 'scratch :name "*saved-scratch*"
                      :directory "/tmp/" :contents "saved text" :persistent t)))
    (unwind-protect
        (save-window-excursion
          (atelier-select-workspace workspace)
          (atelier-entry-add workspace saved)
          (atelier-register-buffer closing workspace)
          (delete-other-windows)
          (switch-to-buffer closing)
          (cl-letf (((symbol-function 'display-graphic-p) (lambda (&optional _) t)))
            (atelier-capture-current-workspace)
            (atelier-close-current-view))
          (should (eq (window-buffer) (atelier-entry-live-buffer saved)))
          (should (equal (with-current-buffer (window-buffer) (buffer-string))
                         "saved text"))
          (should (= (length (atelier-workspace-entries workspace)) 1)))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (dolist (entry (atelier-workspace-entries workspace))
        (when-let* ((buffer (atelier-entry-live-buffer entry)))
          (kill-buffer buffer)))
      (when (buffer-live-p closing) (kill-buffer closing)))))

(ert-deftest atelier-close-visible-entry-removes-its-split ()
  (let* ((workspace (list :id "close-split" :name "close-split"
                          :destination "local" :path "/tmp/" :entries nil))
         (atelier-workspaces (list workspace))
         (old-selection (atelier-current-workspace-id))
         (left (generate-new-buffer "close-split-left"))
         (right (generate-new-buffer "close-split-right")))
    (unwind-protect
        (save-window-excursion
          (atelier-select-workspace workspace)
          (delete-other-windows)
          (set-window-buffer (selected-window) left)
          (set-window-buffer (split-window-right) right)
          (cl-letf (((symbol-function 'display-graphic-p) (lambda (&optional _) t)))
            (atelier-capture-current-workspace)
            (select-window (get-buffer-window right))
            (atelier-close-current-view))
          (should-not (buffer-live-p right))
          (should (= (length (atelier-main-windows)) 1))
          (should (eq (window-buffer) left))
          (should (= (length (atelier-workspace-entries workspace)) 1)))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (dolist (buffer (list left right))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest atelier-navigator-close-visible-entry-keeps-a-workspace-replacement ()
  (let* ((workspace (list :id "navigator-close" :name "navigator-close"
                          :destination "local" :path "/tmp/" :entries nil))
         (atelier-workspaces (list workspace))
         (atelier-navigator-window-configurations nil)
         (old-selection (atelier-current-workspace-id))
         (replacement (generate-new-buffer "navigator-close-replacement"))
         (closing (generate-new-buffer "navigator-close-current")))
    (unwind-protect
        (save-window-excursion
          (atelier-select-workspace workspace)
          (delete-other-windows)
          (dolist (buffer (list replacement closing))
            (atelier-register-buffer buffer workspace)
            (switch-to-buffer buffer))
          (cl-letf (((symbol-function 'display-graphic-p) (lambda (&optional _) t))
                    ((symbol-function 'myconfig-normalize-directory)
                     #'file-name-as-directory)
                    ((symbol-function 'y-or-n-p) (lambda (&rest _) t)))
            (atelier-navigator)
            (goto-char (point-min))
            (let ((position
                   (cl-find-if
                    (lambda (candidate)
                      (pcase (get-text-property candidate 'atelier-navigator-target)
                        (`(workspace-buffer ,_ ,_ ,entry-id)
                         (eq (atelier-entry-live-buffer
                              (atelier-entry-by-id workspace entry-id))
                             closing))))
                    (atelier-navigator-positions))))
              (should position)
              (goto-char position)
              (atelier-navigator-close)
              (atelier-navigator-quit)))
          (should-not (buffer-live-p closing))
          (should (eq (window-buffer) replacement))
          (should (= (length (atelier-workspace-entries workspace)) 1)))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (dolist (buffer (list replacement closing))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest atelier-navigator-renders-recursive-layout-tree ()
  (let* ((first-split '(:id "entry-z" :kind scratch :name "first split"))
         (second-split '(:id "entry-a" :kind scratch :name "second split"))
         (third-split '(:id "entry-b" :kind scratch :name "third split"))
         (hidden '(:id "entry-m" :kind scratch :name "hidden"))
         (nested (list :id "nested" :kind 'layout :orientation 'vertical
                       :children (list second-split third-split)))
         (layout (list :id "layout" :kind 'layout :orientation 'horizontal
                       :displayed t :children (list first-split nested)))
         (workspace (list :id "sorted-workspace" :name "sorted"
                           :destination "local" :path "/tmp/"
                           :entries (list layout hidden)))
         (atelier-workspaces (list workspace))
         targets text)
    (cl-letf (((symbol-function 'myconfig-normalize-directory)
                #'file-name-as-directory))
      (with-current-buffer (atelier-render-navigator)
        (setq text (buffer-string))
        (dolist (position (atelier-navigator-positions))
          (when-let* ((target (get-text-property position 'atelier-navigator-target))
                      ((memq (car target) '(workspace-buffer workspace-owned-buffer))))
            (push target targets)))))
    (let ((position 0))
      (dolist (label '("Entry (side-by-side)" "Split 1: first split"
                       "Entry (stacked)" "Split 2: second split"
                       "Split 3: third split" "hidden"))
        (setq position (string-match (regexp-quote label) text position))
        (should position)
        (setq position (match-end 0))))
    (should
     (equal (nreverse targets)
             '((workspace-buffer "sorted" 0 "entry-z")
               (workspace-buffer "sorted" 1 "entry-a")
               (workspace-buffer "sorted" 2 "entry-b")
               (workspace-owned-buffer "sorted" "entry-m"))))))

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
    (should (= (plist-get migrated :version) 7))
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
    (should (= (plist-get migrated :version) 7))
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

(ert-deftest atelier-navigator-detach-reuses-existing-replacement-entry ()
  (let* ((source-entry '(:id "detach-source-entry" :kind scratch
                             :name "source" :displayed t))
         (replacement-entry '(:id "detach-replacement-entry" :kind scratch
                                  :name "replacement"))
         (workspace (list :id "detach-replacement" :name "work"
                          :destination "local" :path "/tmp/"
                          :entries (list source-entry replacement-entry)))
         (atelier-workspaces (list workspace))
         (atelier-entry-live-buffers (make-hash-table :test #'equal))
         (old-selection (atelier-current-workspace-id))
         (source (generate-new-buffer "detach-source-buffer"))
         (replacement (generate-new-buffer "detach-replacement-buffer")))
    (unwind-protect
        (save-window-excursion
          (atelier-select-workspace workspace)
          (atelier-entry-set-live-buffer source-entry source)
          (atelier-entry-set-live-buffer replacement-entry replacement)
          (delete-other-windows)
          (set-window-buffer (selected-window) source)
          (cl-letf (((symbol-function 'display-graphic-p) (lambda (&optional _) t))
                    ((symbol-function 'atelier-navigator-target)
                     (lambda () '(workspace-buffer "work" 0
                                                   "detach-source-entry")))
                    ((symbol-function 'atelier-navigator-quit) #'ignore)
                    ((symbol-function 'atelier-navigator) #'ignore)
                    ((symbol-function 'atelier-notify-change) #'ignore))
            (atelier-navigator-detach))
          (should (eq (window-buffer) replacement))
          (should
           (equal (mapcar (lambda (entry) (plist-get entry :id))
                          (atelier-workspace-entries workspace))
                  '("detach-replacement-entry")))
          (should
           (equal (mapcar (lambda (entry) (plist-get entry :id))
                          (atelier-workspace-entries
                           (atelier-detached-workspace)))
                  '("detach-source-entry"))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (dolist (buffer (list source replacement))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

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

(ert-deftest aipanel-lists-only-agents-in-the-attached-environment ()
  (let ((aipanel-agents
         '((:id opencode :name "OpenCode" :program "opencode")
           (:id fx :name "fx" :program "fx")
           (:id missing :name "Missing" :program "missing"))))
    (cl-letf (((symbol-function 'executable-find)
               (lambda (program) (and (member program '("opencode" "wsl.exe")) program)))
              ((symbol-function 'aipanel-run-wsl-probe)
               (lambda (&optional _distribution)
                 '(:distribution "Ubuntu" :programs ("fx")))))
      (should
       (equal (mapcar #'car (aipanel-candidates '(:location host)))
              '("OpenCode (host)")))
      (should
       (equal (mapcar #'car
                      (aipanel-candidates
                       '(:location wsl :destination "Ubuntu")))
              '("fx (WSL: Ubuntu)"))))))

(ert-deftest aipanel-uses-the-only-installed-agent-without-prompting ()
  (cl-letf (((symbol-function 'aipanel-candidates)
              (lambda (&optional _owner)
                '(("fx (host)" . (:agent (:id fx) :location host)))))
            ((symbol-function 'completing-read)
             (lambda (&rest _arguments) (ert-fail "A single agent should not prompt"))))
    (should (equal (plist-get (aipanel-read-agent '(:location host)) :location)
                   'host))))

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

(ert-deftest aipanel-standalone-attachment-uses-buffer-directory-and-identity ()
  (let ((source (generate-new-buffer "aipanel-standalone-source")))
    (unwind-protect
        (with-current-buffer source
          (setq default-directory "/tmp/project/src/")
          (let ((owner (aipanel-default-owner)))
            (should (eq (plist-get owner :id) source))
            (should (eq (plist-get owner :source-buffer) source))
            (should (equal (plist-get owner :directory) "/tmp/project/src/"))
            (should (eq (plist-get owner :location) 'host))))
      (when (buffer-live-p source) (kill-buffer source)))))

(ert-deftest aipanel-context-is-relative-to-the-process-working-directory ()
  (let ((source (generate-new-buffer "aipanel-relative-context")))
    (unwind-protect
        (with-current-buffer source
          (setq default-directory "/tmp/project/src/"
                buffer-file-name "/tmp/project/src/lib/example.el")
          (insert "first\nsecond")
          (goto-char (point-min))
          (forward-line 1)
          (forward-char 2)
          (should
           (equal (aipanel-default-context (aipanel-default-owner) nil)
                  "lib/example.el:L2:C3: ")))
      (when (buffer-live-p source) (kill-buffer source)))))

(ert-deftest aipanel-killing-standalone-source-removes-its-panel ()
  (let ((source (generate-new-buffer "aipanel-killed-source"))
        (panel (generate-new-buffer "aipanel-killed-panel"))
        (aipanel-sessions (make-hash-table :test #'equal))
        (aipanel-buffer-exited-hook nil))
    (unwind-protect
        (let ((owner (with-current-buffer source (aipanel-default-owner))))
          (aipanel-adopt-buffer
           panel owner '(:agent (:id fx :program "fx") :location host))
          (kill-buffer source)
          (should-not (buffer-live-p panel))
          (should-not (gethash source aipanel-sessions)))
      (dolist (buffer (list source panel))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest aipanel-atelier-attaches-to-the-current-layout-leaf ()
  (let* ((source (generate-new-buffer "aipanel-leaf-source"))
         (other '(:id "other-leaf" :kind scratch :name "other"))
         (leaf '(:id "source-leaf" :kind scratch :name "source"
                      :directory "/tmp/project/src/"))
         (layout (list :id "layout-root" :kind 'layout :displayed t
                       :orientation 'horizontal :ratio 0.5
                       :children (list other leaf)))
         (workspace (list :id "leaf-workspace" :name "leaf-workspace"
                          :destination "local" :path "/tmp/project/"
                          :platform 'local :entries (list layout)))
         (atelier-workspaces (list workspace))
         (old-selection (atelier-current-workspace-id)))
    (unwind-protect
        (progn
          (atelier-select-workspace workspace)
          (with-current-buffer source
            (setq default-directory "/tmp/project/src/")
            (atelier-entry-set-live-buffer leaf source)
            (cl-letf (((symbol-function 'myconfig-windows-workspace-p)
                       (lambda (_) nil))
                      ((symbol-function 'myconfig-wsl-workspace-p)
                       (lambda (_) nil)))
              (let ((owner (aipanel-atelier-owner)))
                (should (equal (plist-get owner :id) "source-leaf"))
                (should (equal (plist-get owner :entry-id) "source-leaf"))
                (should (equal (plist-get owner :directory) "/tmp/project/src/"))))))
      (set-frame-parameter nil 'atelier-workspace-id old-selection)
      (when (buffer-live-p source) (kill-buffer source)))))

(ert-deftest aipanel-atelier-terminal-records-attachment-and-allows-duplicates ()
  (let (captured-agent captured-explicit)
    (cl-letf (((symbol-function 'myconfig-terminal-buffer)
               (lambda (_name _directory _program _arguments _workspace
                              _shell agent _type explicit)
                 (setq captured-agent agent captured-explicit explicit)
                 'panel-buffer))
              ((symbol-function 'aipanel-atelier-workspace)
               (lambda (_owner) '(:id "workspace"))))
      (should
       (eq (aipanel-atelier-terminal
            "panel" "/tmp/" "fx" nil
            '(:entry-id "source-entry" :directory "/tmp/project/"
              :emacs-directory "/tmp/project/" :destination "local"
              :platform local :location host)
            '(:agent (:id fx) :location host))
           'panel-buffer))
      (should captured-explicit)
      (should
       (equal (plist-get (plist-get captured-agent :attachment) :entry-id)
              "source-entry")))))

(ert-deftest aipanel-atelier-panel-persistence-follows-its-source-entry ()
  (let* ((source '(:id "transient-source" :kind transient :name "source"
                        :persistent nil))
         (workspace (list :id "transient-workspace" :name "transient"
                          :entries (list source)))
         (atelier-workspaces (list workspace))
         (panel (generate-new-buffer "aipanel-transient-panel"))
         panel-entry)
    (unwind-protect
        (cl-letf (((symbol-function 'myconfig-terminal-buffer)
                   (lambda (_name _directory _program _arguments owner-workspace
                                  _shell agent type _explicit)
                     (setq panel-entry
                           (list :id "transient-panel" :kind 'terminal :type type
                                 :name (buffer-name panel) :persistent t
                                 :job (list :id "transient-job"
                                            :buffer (buffer-name panel)
                                            :policy 'auto :agent agent)))
                     (atelier-entry-add owner-workspace panel-entry t)
                     (atelier-entry-set-live-buffer panel-entry panel)
                     panel)))
          (aipanel-atelier-terminal
           "panel" "/tmp/" "fx" nil
           '(:entry-id "transient-source" :workspace-id "transient-workspace"
             :directory "/tmp/" :emacs-directory "/tmp/"
             :destination "local" :platform local :location host)
           '(:agent (:id fx) :location host))
          (should-not (plist-get panel-entry :persistent)))
      (when (buffer-live-p panel) (kill-buffer panel)))))

(ert-deftest aipanel-atelier-removing-source-removes-attached-panel ()
  (let* ((source-buffer (generate-new-buffer "aipanel-remove-source"))
         (panel-buffer (generate-new-buffer "aipanel-remove-panel"))
         (source (list :id "remove-source" :kind 'scratch :name "source"))
         (job (list :id "remove-panel-job" :buffer (buffer-name panel-buffer)
                    :policy 'auto :agent '(:id fx)))
         (panel-entry (list :id "remove-panel-entry" :kind 'terminal
                            :type 'aipanel :name (buffer-name panel-buffer)
                            :job job))
         (workspace (list :id "remove-workspace" :name "remove"
                          :entries (list source panel-entry)))
         (atelier-workspaces (list workspace))
         (aipanel-sessions (make-hash-table :test #'equal))
         (aipanel-buffer-exited-hook '(aipanel-atelier-buffer-exited))
         (atelier-entry-removed-hook '(aipanel-atelier-entry-removed)))
    (unwind-protect
        (cl-letf (((symbol-function 'atelier-notify-change) #'ignore)
                  ((symbol-function 'myconfig-persist-schedule) #'ignore))
          (atelier-entry-set-live-buffer source source-buffer)
          (atelier-entry-set-live-buffer panel-entry panel-buffer)
          (aipanel-adopt-buffer
           panel-buffer
           (list :id "remove-source" :entry-id "remove-source"
                 :source-buffer source-buffer)
           '(:agent (:id fx :program "fx") :location host))
          (atelier-entry-remove workspace source t)
          (should-not (buffer-live-p panel-buffer))
          (should-not (atelier-entry-by-id workspace "remove-panel-entry")))
      (dolist (buffer (list source-buffer panel-buffer))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest aipanel-atelier-moving-source-moves-its-panel-entry ()
  (let* ((source-buffer (generate-new-buffer "aipanel-move-source"))
         (panel-buffer (generate-new-buffer "aipanel-move-panel"))
         (source (list :id "move-panel-source" :kind 'scratch :name "source"))
         (job (list :id "move-panel-job" :buffer (buffer-name panel-buffer)
                    :policy 'auto :agent '(:id fx)))
         (panel-entry (list :id "moving-panel-entry" :kind 'terminal
                            :type 'aipanel :name (buffer-name panel-buffer)
                            :job job))
         (old (list :id "old-panel-workspace" :name "old"
                    :entries (list source panel-entry)))
         (new (list :id "new-panel-workspace" :name "new" :entries nil))
         (atelier-workspaces (list old new))
         (aipanel-sessions (make-hash-table :test #'equal))
         (atelier-entry-moved-hook '(aipanel-atelier-entry-moved)))
    (unwind-protect
        (progn
          (atelier-entry-set-live-buffer source source-buffer)
          (atelier-entry-set-live-buffer panel-entry panel-buffer)
          (aipanel-adopt-buffer
           panel-buffer
           (list :id "move-panel-source" :entry-id "move-panel-source"
                 :workspace-id "old-panel-workspace" :source-buffer source-buffer)
           '(:agent (:id fx :program "fx") :location host))
          (atelier-entry-move source old new)
          (should-not (atelier-entry-by-id old "move-panel-source"))
          (should-not (atelier-entry-by-id old "moving-panel-entry"))
          (should (eq (atelier-entry-by-id new "move-panel-source") source))
          (should (eq (atelier-entry-by-id new "moving-panel-entry") panel-entry))
          (should
           (equal (plist-get (buffer-local-value 'aipanel-owner panel-buffer)
                             :workspace-id)
                  "new-panel-workspace")))
      (dolist (buffer (list source-buffer panel-buffer))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))

(ert-deftest atelier-v7-migration-keeps-only-source-attached-panels ()
  (let* ((source '(:id "migration-source" :kind scratch :name "source"
                        :persistent t))
         (legacy '(:id "legacy-panel" :kind terminal :type aipanel
                        :name "legacy" :persistent t
                        :job (:id "legacy-job" :buffer "legacy" :policy auto
                              :agent (:id fx))))
         (attached '(:id "attached-panel" :kind terminal :type aipanel
                          :name "attached" :persistent t
                          :job (:id "attached-job" :buffer "attached" :policy auto
                                :agent (:id fx :attachment
                                       (:entry-id "migration-source")))))
         (data (list :version 6 :generation "migration-v7"
                     :current-workspace-id "migration-workspace"
                     :ssh-destinations nil
                     :workspaces
                     (list (list :id "migration-workspace" :name "migration"
                                 :destination "local" :path "/tmp/" :status 'running
                                 :entries (list source legacy attached)))))
         (migrated (myconfig-validate-state data))
         (entries (atelier-workspace-entries (car (plist-get migrated :workspaces)))))
    (should (= (plist-get migrated :version) 7))
    (should (cl-find "migration-source" entries :key (lambda (entry) (plist-get entry :id))
                     :test #'equal))
    (should-not (cl-find "legacy-panel" entries :key (lambda (entry) (plist-get entry :id))
                         :test #'equal))
    (should (cl-find "attached-panel" entries :key (lambda (entry) (plist-get entry :id))
                     :test #'equal))))

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

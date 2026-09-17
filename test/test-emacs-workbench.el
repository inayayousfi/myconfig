;;; test-emacs-workbench.el --- Focused workbench tests -*- lexical-binding: t; -*-

(require 'ert)
(require 'cl-lib)
(require 'subr-x)

(defgroup myconfig nil "Test workbench." :group 'environment)
(provide 'myconfig-core)
(provide 'atelier)
(provide 'myconfig-terminal)
(provide 'myconfig-windows)
(provide 'ghostel)
(defvar atelier-preserve-job-recipe nil)
(defvar atelier-agent-restored-functions nil)

(add-to-list 'load-path
             (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/"
                               (file-name-directory (or load-file-name buffer-file-name))))

(let ((lisp-directory
       (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/"
                         (file-name-directory (or load-file-name buffer-file-name)))))
  (load (expand-file-name "aipan.el" lisp-directory) nil t)
  (load (expand-file-name "aipanel-atelier.el" lisp-directory) nil t)
  (load (expand-file-name "myconfig-persist.el" lisp-directory) nil t))

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
         (workspace (list :name "work" :jobs (list job) :agent-buffer (buffer-name buffer)))
         (atelier-preserve-job-recipe nil))
    (unwind-protect
        (cl-letf (((symbol-function 'atelier-find-job-for-buffer)
                   (lambda (_name) (list workspace job)))
                  ((symbol-function 'atelier-notify-change) #'ignore)
                  ((symbol-function 'myconfig-log) #'ignore)
                  ((symbol-function 'myconfig-persist-schedule) #'ignore))
          (myconfig-job-process-exited buffer)
          (should-not (plist-get workspace :jobs))
          (should-not (plist-get workspace :agent-buffer)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))

(ert-run-tests-batch-and-exit)
;;; test-emacs-workbench.el ends here

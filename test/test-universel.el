;;; test-universel.el --- Cross-platform operation contracts -*- lexical-binding: t; -*-

(require 'ert)
(require 'cl-lib)
(add-to-list 'load-path
             (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/"
                               (file-name-directory (or load-file-name buffer-file-name))))
(require 'univers)

(ert-deftest universel-loads-without-application-packages ()
  (dolist (feature '(myconfig-core atelier aipan ghostel mason treesit-auto))
    (should-not (featurep feature))))

(ert-deftest universel-detection-follows-the-operation-and-explicit-platform-wins ()
  (let ((system-type 'windows-nt)
        (default-directory "/ssh:alice@example:/work/"))
    (should (eq (universel-platform) 'posix))
    (should (eq (universel-platform 'windows) 'windows))
    (should (eq (universel-host-platform) 'windows))
    (let ((environment (universel-environment)))
      (should (eq (plist-get environment :transport) 'ssh))
      (should (equal (plist-get environment :destination) "alice@example"))
      (should (equal (plist-get environment :directory) "/work/")))))

(ert-deftest universel-local-environment-does-not-inherit-the-current-connection ()
  (let ((system-type 'gnu/linux)
        (default-directory "/ssh:elsewhere:/work/"))
    (should-not (universel-process-observation-p))
    (should (universel-process-observation-p (universel-host-environment)))
    (should (eq (plist-get (universel-environment '(:platform windows)) :transport) 'local))
    (should-not (plist-get (universel-environment '(:platform windows)) :destination))))

(ert-deftest universel-explicit-connections-ignore-unrelated-buffer-connections ()
  (let* ((default-directory "/ssh:wrong:/wrong/")
         (environment (universel-environment
                       '(:platform windows :transport ssh :destination "right"
                         :directory "C:/project/") "C:/project/src/")))
    (should (equal (plist-get environment :directory) "C:/project/src/"))
    (should (equal (plist-get environment :destination) "right"))
    (should (eq (plist-get environment :platform) 'windows))))

(ert-deftest universel-registered-path-detection-retains-translated-directory ()
  (let ((universel-environment-functions
         (list (lambda (directory)
                 (when (equal directory "/mounted/project/src/")
                   '(:platform windows :transport ssh :destination "work"
                     :directory "C:/project/src/"))))))
    (let ((command (universel-command "agent" nil "/mounted/project/src/")))
      (should (equal (plist-get command :program) "ssh"))
      (should (equal (nth 1 (plist-get command :arguments)) "work"))
      (let ((script (decode-coding-string
                     (base64-decode-string (car (last (split-string
                                                       (car (last (plist-get command :arguments)))))))
                     'utf-16le)))
        (should (string-match-p (regexp-quote "C:\\project\\src\\") script))))))

(ert-deftest universel-posix-quoting-survives-a-windows-host ()
  (let* ((argument "a b'\";$HOME`printf bad`\nend")
         (quoted (let ((system-type 'windows-nt))
                   (universel-quote-argument argument 'posix))))
    (with-temp-buffer
      (should (zerop (process-file "sh" nil t nil "-c" (concat "printf %s " quoted))))
      (should (equal (buffer-string) argument)))))

(ert-deftest universel-windows-quoting-does-not-use-the-host-shell ()
  (let ((system-type 'gnu/linux))
    (should (equal (universel-quote-argument "C:\\O'Brien\\$work" 'windows)
                   "'C:\\O''Brien\\$work'"))))

(ert-deftest universel-posix-agent-launch-retains-cwd-port-and-arguments ()
  (let* ((command (universel-command
                   "fx" '("." "--safe") "/project/source dir/"
                   '(:platform posix :transport ssh :destination "alice@host" :port 2222)))
         (arguments (plist-get command :arguments)))
    (should (equal (cl-subseq arguments 0 4) '("-t" "-p" "2222" "alice@host")))
    (should (equal (car (last arguments)) "cd -- /project/source\\ dir/ && exec fx . --safe"))
    (should (equal (plist-get command :directory) (universel-home-directory)))))

(ert-deftest universel-windows-agent-launch-encodes-the-destination-command ()
  (let* ((command (universel-command
                   "fx" '("." "--safe") "C:\\O'Brien\\src"
                   '(:platform windows :transport ssh :destination "work")))
         (remote (car (last (plist-get command :arguments))))
         (script (decode-coding-string (base64-decode-string
                                        (car (last (split-string remote)))) 'utf-16le)))
    (should (equal (plist-get command :program) "ssh"))
    (should (equal script "Set-Location -LiteralPath 'C:\\O''Brien\\src'; & 'fx' '.' '--safe'"))))

(ert-deftest universel-wsl-launch-is-a-connection-not-a-host-platform ()
  (let* ((command (universel-command
                   "agent" '("--safe") "/home/user/project/"
                   '(:platform posix :transport wsl :destination "Ubuntu"))))
    (should (equal (plist-get command :program) "wsl.exe"))
    (should (equal (plist-get command :arguments)
                   '("-d" "Ubuntu" "--cd" "/home/user/project/" "--" "agent" "--safe")))))

(ert-deftest universel-local-launch-does-not-shell-quote-argv ()
  (let ((command (universel-command "agent" '("a b" "$x") "C:/work/"
                                    '(:platform windows :transport local))))
    (should (equal command '(:program "agent" :arguments ("a b" "$x") :directory "C:/work/")))))

(ert-deftest universel-terminal-recipes-preserve-existing-policy ()
  (let ((local (universel-shell-command "/work/" '(:platform linux :transport local)))
        (posix (universel-shell-command "/work/" '(:platform posix :transport ssh :destination "server")))
        (windows (universel-shell-command "/C:/O'Brien/" '(:platform windows :transport ssh :destination "work"))))
    (should-not (plist-get local :program))
    (should (equal (plist-get local :arguments) '("-l")))
    (should (equal (plist-get posix :arguments) '("server")))
    (should (equal (plist-get windows :arguments)
                   '("work" "powershell.exe -NoLogo -NoExit -Command \"Set-Location -LiteralPath 'C:\\O''Brien\\'\"")))))

(ert-deftest universel-file-paths-and-native-paths-remain-distinct ()
  (let ((environment '(:platform windows :transport ssh :destination "work"
                      :directory "/C:/project/" :mount-root "/C:/")))
    (cl-letf (((symbol-function 'universel--ensure-mount)
               (lambda (&rest _) "/mount/")))
      (should (equal (universel-file-directory "/C:/project/" environment "/state/")
                     "/mount/project/"))
      (should (equal (universel-file-path "/mount/project/src/" environment "/state/")
                     "/C:/project/src/"))
      (should (equal (universel-execution-path "/mount/project/src/" environment "/state/")
                     "C:\\project\\src\\")))))

(ert-deftest universel-standard-paths-preserve-windows-and-xdg-precedence ()
  (let ((process-environment (copy-sequence process-environment))
        (default-directory "/ssh:elsewhere:/work/"))
    (setenv "XDG_DATA_HOME" nil)
    (setenv "LOCALAPPDATA" "/windows-local")
    (should (equal (universel-standard-path 'data "myconfig-emacs" 'windows)
                   "/windows-local/myconfig-emacs"))
    (setenv "XDG_DATA_HOME" "/override")
    (should (equal (universel-standard-path 'data "myconfig-emacs" 'windows)
                   "/override/myconfig-emacs"))))

(ert-deftest universel-native-command-output-failure-and-timeout ()
  (should (equal (universel-run-command-lines '("sh" "-c" "printf 'one\\n\\ntwo\\n'") 2 'linux)
                 '("one" "two")))
  (should-not (universel-run-command-lines '("sh" "-c" "exit 3") 2 'linux))
  (should-not (universel-run-command-lines '("sh" "-c" "exec sleep 5") 0.01 'linux))
  (should-not (get-process "universel-command")))

(ert-deftest universel-windows-launch-failure-cleans-its-temporary-file ()
  (let (output-file)
    (cl-letf (((symbol-function 'make-process)
               (lambda (&rest arguments)
                 (let ((command (plist-get arguments :command)))
                   (should (equal (cl-subseq command 1 4)
                                  '("-NoProfile" "-NonInteractive" "-Command")))
                   (should (string-match "Set-Content -Encoding utf8 '\\([^']+\\)'" (car (last command))))
                   (setq output-file (match-string 1 (car (last command))))
                   (should (file-exists-p output-file)))
                 (error "Simulated spawn failure"))))
      (should-error (universel-run-command-lines '("wsl.exe" "-l") 1 'windows)))
    (should output-file)
    (should-not (file-exists-p output-file))))

(ert-deftest universel-non-linux-process-inspection-does-not-touch-proc ()
  (cl-letf (((symbol-function 'directory-files)
             (lambda (&rest _) (ert-fail "Must not inspect /proc"))))
    (should-not (universel-process-table '(:platform windows :transport local)))
    (should-not (universel-process-table '(:platform posix :transport ssh :destination "server")))))

(ert-deftest universel-foreground-process-selection-preserves-group-rules ()
  (let* ((shell '(:pid 10 :pgrp 10 :tty 2 :tpgid 20))
         (foreground '(:pid 20 :ppid 10 :pgrp 20 :tty 2))
         (child '(:pid 21 :ppid 20 :pgrp 20 :tty 2))
         (other-terminal '(:pid 30 :ppid 10 :pgrp 20 :tty 3))
         (table (list shell foreground child other-terminal)))
    (should (eq (universel-foreground-process 10 table) foreground))
    (should (eq (universel-foreground-process 10 table t) shell))
    (should-not (universel-foreground-process 999 table))
    (should-not (universel-foreground-process 10 (cons '(:pid 22 :ppid 10 :pgrp 20 :tty 2) table)))))

(ert-run-tests-batch-and-exit)
;;; test-universel.el ends here

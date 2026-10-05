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
    (should (equal script "$ErrorActionPreference = 'Stop'; Set-Location -LiteralPath 'C:\\O''Brien\\src' -ErrorAction Stop; & 'fx' '.' '--safe'"))))

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
    (should (equal (plist-get posix :arguments)
                   '("-t" "server" "cd -- /work/ && exec \"${SHELL:-/bin/sh}\" -l")))
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
    ;; A Windows host answers through its PowerShell helper instead.
    (let ((table (universel-process-table '(:platform windows :transport local))))
      (unless (eq system-type 'windows-nt) (should-not table)))
    (should-not (universel-process-table '(:platform posix :transport ssh :destination "server")))))

(ert-deftest universel-process-environment-reads-a-live-process ()
  (skip-unless (memq system-type '(gnu/linux windows-nt)))
  (let* ((process-environment (cons "UNIVERSEL_PROBE=seen" process-environment))
         (output "")
         ;; Read only after the child has started, not while it is still a copy of Emacs.
         (process (make-process :name "environment-probe"
                                :command (if (eq system-type 'windows-nt)
                                             (list (universel-default-shell (universel-host-environment))
                                                   "-NoLogo" "-NoProfile" "-Command"
                                                   "'ready'; Start-Sleep -Seconds 30")
                                           '("sh" "-c" "echo ready; sleep 30; exit"))
                                :connection-type 'pipe
                                :filter (lambda (_ text) (setq output (concat output text)))
                                :noquery t)))
    (unwind-protect
        (progn
          (while (not (string-match-p "ready" output))
            (accept-process-output process 5))
          (should (member "UNIVERSEL_PROBE=seen"
                          (universel-process-environment (process-id process)
                                                         (universel-host-environment)))))
      (delete-process process))))

(ert-deftest universel-windows-fresh-shell-starts-from-emacs-variables ()
  (skip-unless (eq system-type 'windows-nt))
  (let* ((process-environment (cons "UNIVERSEL_FRESH=seen" process-environment))
         (shell (universel-default-shell (universel-host-environment)))
         (environment (universel-shell-environment
                       (list :executable shell :login nil) '("IGNORED=start")
                       (universel-host-environment))))
    (should (member "UNIVERSEL_FRESH=seen" environment))
    (should-not (member "IGNORED=start" environment))
    (should (cl-every (lambda (variable) (string-match-p "\\`[^=\n]+=" variable)) environment))))

(ert-deftest universel-shell-environment-includes-startup-file-exports ()
  (skip-unless (and (eq system-type 'gnu/linux) (file-executable-p "/bin/sh")))
  (let ((startup (make-temp-file "universel-shell-startup-")))
    (unwind-protect
        (progn
          (with-temp-file startup (insert "printf banner\nexport FROM_STARTUP=1\n"))
          (let ((environment (universel-shell-environment
                              '(:executable "/bin/sh" :login nil)
                              (list "PATH=/usr/bin:/bin" (concat "ENV=" startup) "KEPT=yes")
                              (universel-host-environment))))
            (should (member "FROM_STARTUP=1" environment))
            (should (member "KEPT=yes" environment))
            (should-not (cl-find-if (lambda (variable) (string-match-p "banner" variable))
                                    environment))))
      (delete-file startup))))

(ert-deftest universel-windows-command-line-splits-like-windows ()
  (should (equal (universel-windows-split-command-line
                  "\"C:\\WINDOWS\\system32\\wsl.exe\" -u ziede zsh -ic \"cco \"")
                 '("C:\\WINDOWS\\system32\\wsl.exe" "-u" "ziede" "zsh" "-ic" "cco ")))
  (should (equal (universel-windows-split-command-line
                  "C:\\tools\\a.exe \"a b\" c\\\"d \"e\\\\\" f\\g \"\"")
                 '("C:\\tools\\a.exe" "a b" "c\"d" "e\\" "f\\g" ""))))

(ert-deftest universel-wsl-share-paths-round-trip ()
  (let ((environment (universel-environment nil "//wsl.localhost/archlinux/home/ziede/project/")))
    (should (eq (plist-get environment :transport) 'wsl))
    (should (equal (plist-get environment :destination) "archlinux"))
    (should (equal (plist-get environment :directory) "/home/ziede/project/")))
  (should (equal (plist-get (universel-environment nil "\\\\wsl$\\Ubuntu\\srv") :directory) "/srv"))
  (let ((workspace '(:platform posix :transport wsl :destination "archlinux" :directory "/home/ziede")))
    (should (equal (universel-file-directory "/home/ziede" workspace)
                   "//wsl.localhost/archlinux/home/ziede/"))
    (should (equal (universel-file-path "//wsl.localhost/archlinux/home/ziede/a.txt" workspace)
                   "/home/ziede/a.txt"))))

(ert-deftest universel-shell-launch-keeps-the-shell-open-on-each-platform ()
  (should (equal (universel-shell-launch
                  '(:executable "C:/Program Files/PowerShell/7/pwsh.exe" :login ("-l")) "C:/work/"
                  '("C:\\WINDOWS\\system32\\wsl.exe" "-ic" "cco 'x'") '("IS_DEMO=1") nil 'windows)
                 '(:program "C:/Program Files/PowerShell/7/pwsh.exe" :directory "C:/work/"
                   :arguments ("-l" "-NoLogo" "-NoExit" "-Command"
                               "[Environment]::SetEnvironmentVariable('IS_DEMO', '1'); & 'C:\\WINDOWS\\system32\\wsl.exe' '-ic' 'cco ''x'''"))))
  (should (equal (plist-get (universel-shell-launch
                             '(:executable "/usr/bin/zsh" :login ("-l")) "/work/"
                             '("claude" "--x") '("IS_DEMO=1") nil 'linux)
                            :arguments)
                 '("-l" "-i" "-c" "env IS_DEMO\\=1 claude --x; exec /usr/bin/zsh -l")))
  (let ((launch (universel-shell-launch
                 '(:executable "/usr/sbin/zsh" :login ("-l")) "//wsl.localhost/archlinux/work/"
                 '("claude") '("IS_DEMO=1") '("UNIVERSEL_TERMINAL=t1")
                 '(:platform posix :transport wsl :destination "archlinux"))))
    (should (equal (plist-get launch :program) "wsl.exe"))
    (should (equal (plist-get launch :arguments)
                   '("-d" "archlinux" "--cd" "/work/" "--" "env" "UNIVERSEL_TERMINAL=t1"
                     "/usr/sbin/zsh" "-l" "-i" "-c"
                     "env IS_DEMO\\=1 claude; exec /usr/sbin/zsh -l")))))

(ert-deftest universel-wsl-terminal-starts-the-login-shell-with-variables ()
  (let ((universel--wsl-shells (make-hash-table :test #'equal)))
    (cl-letf (((symbol-function 'universel--wsl-request)
               (lambda (distribution request)
                 (should (equal distribution "archlinux"))
                 (should (equal request "shell"))
                 "/usr/sbin/zsh\n")))
      (let ((launch (universel-shell-command
                     "/home/ziede/" '(:platform posix :transport wsl :destination "archlinux")
                     '("UNIVERSEL_TERMINAL=t1"))))
        (should (equal (plist-get launch :shell) "/usr/sbin/zsh"))
        (should (equal (plist-get (plist-get launch :location) :destination) "archlinux"))
        (should (equal (plist-get launch :arguments)
                       '("-d" "archlinux" "--cd" "/home/ziede/" "--" "env" "UNIVERSEL_TERMINAL=t1"
                         "/usr/sbin/zsh" "-l")))))))

(ert-deftest universel-wsl-primitives-answer-from-a-real-distribution ()
  (skip-unless (and (eq system-type 'windows-nt) (member "archlinux" (universel-wsl-distributions))))
  (let* ((environment '(:platform posix :transport wsl :destination "archlinux"))
         (marker (format "UNIVERSEL_TEST=%06x" (random #xffffff)))
         (process (make-process :name "wsl-probe"
                                :command (list "wsl.exe" "-d" "archlinux" "-e" "env" marker
                                               "sh" "-c" "echo ready; sleep 30; exit")
                                :connection-type 'pipe :noquery t
                                :filter (lambda (process text) (process-put process 'output text)))))
    (unwind-protect
        (progn
          (while (not (process-get process 'output)) (accept-process-output process 5))
          (should (string-prefix-p "/" (universel-default-shell environment)))
          (let* ((pids (universel-processes-with-variable marker environment))
                 (record (cl-find-if (lambda (entry)
                                       (and (memq (plist-get entry :pid) pids)
                                            (equal (car (plist-get entry :argv)) "sh")))
                                     (universel-process-table environment))))
            ;; The shell and its sleep child both carry the variable.
            (should (= (length pids) 2))
            (should (equal (plist-get record :argv) '("sh" "-c" "echo ready; sleep 30; exit")))
            (should (>= (universel-process-runtime record environment) 0))
            (should (member marker (universel-process-environment (plist-get record :pid)
                                                                  environment))))
          ;; /proc/self is itself a link to the reading process.
          (should (string-match-p "\\`/proc/[0-9]+\\'"
                                  (universel-real-path "/proc/self/../self/." environment)))
          (should (member "sh" (plist-get (universel-find-programs '("sh" "no-such-program") 10
                                                                   environment)
                                          :programs)))
          (should (member "UNIVERSEL_START=1"
                          (universel-shell-environment
                           (list :executable "/bin/sh" :login nil)
                           '("PATH=/usr/bin:/bin" "UNIVERSEL_START=1") environment))))
      (delete-process process)
      (universel-wsl-stop-helper "archlinux")
      (should-not (gethash "archlinux" universel--wsl-helpers)))))

(ert-deftest universel-windows-helper-lists-a-child-with-its-arguments ()
  (skip-unless (eq system-type 'windows-nt))
  (let* ((shell (universel-default-shell (universel-host-environment)))
         (process (make-process :name "windows-probe"
                                :command (list shell "-NoLogo" "-NoProfile" "-Command"
                                               "Start-Sleep -Seconds 30; 'a b'")
                                :connection-type 'pipe :noquery t)))
    (unwind-protect
        (let* ((table (universel-process-table (universel-host-environment)))
               (record (cl-find (process-id process) table
                                :key (lambda (entry) (plist-get entry :pid)))))
          (should record)
          (should (equal (last (plist-get record :argv) 2)
                         '("-Command" "Start-Sleep -Seconds 30; 'a b'")))
          (should (>= (universel-process-runtime record (universel-host-environment)) 0)))
      (delete-process process))))

(ert-deftest universel-windows-restart-runs-program-then-keeps-powershell ()
  (skip-unless (eq system-type 'windows-nt))
  (let* ((directory (file-name-as-directory (make-temp-file "universel-restart-" t)))
         (shell (universel-default-shell (universel-host-environment)))
         (default-directory directory)
         (input (expand-file-name "input" directory)))
    (unwind-protect
        (progn
          (with-temp-file input
            (insert "Set-Content -LiteralPath shell -Value continued\nexit\n"))
          (apply #'call-process shell input nil nil
                 (plist-get (universel-shell-launch
                             (list :executable shell :login nil) directory
                             (list shell "-NoLogo" "-NoProfile" "-Command"
                                   "Set-Content -LiteralPath program -Value \"a b $env:PROBE\"")
                             '("PROBE=restored") nil (universel-host-environment))
                            :arguments))
          (should (equal (string-trim
                          (with-temp-buffer
                            (insert-file-contents (expand-file-name "program" directory))
                            (buffer-string)))
                         "a b restored"))
          (should (file-exists-p (expand-file-name "shell" directory))))
      (delete-directory directory t))))

(ert-deftest universel-ssh-port-and-directory-survive-shell-and-file-paths ()
  (let* ((environment '(:platform posix :transport ssh :destination "alice@host" :port "2222"))
         (command (universel-shell-command "/work with space/" environment)))
    (should (equal (plist-get command :arguments)
                   '("-p" "2222" "-t" "alice@host"
                     "cd -- /work\\ with\\ space/ && exec \"${SHELL:-/bin/sh}\" -l")))
    (should (equal (universel-file-directory "/work/" environment)
                   "/ssh:alice@host#2222:/work/"))))

(ert-deftest universel-mount-keys-preserve-default-port-and-separate-other-ports ()
  (let* ((default '(:platform windows :transport ssh :destination "host" :mount-root "/C:/"))
         (explicit (append default '(:port 22)))
         (other (append default '(:port 2222))))
    (should (equal (universel-mount-key default) "host\0/C:/"))
    (should (equal (universel-mount-key default) (universel-mount-key explicit)))
    (should-not (equal (universel-mount-key default) (universel-mount-key other)))))

(ert-deftest universel-sshfs-launch-retains-explicit-port ()
  (let ((environment '(:platform windows :transport ssh :destination "host" :port 2222 :mount-root "/C:/"))
        (universel--mounts (make-hash-table :test #'equal))
        command)
    (cl-letf (((symbol-function 'executable-find) (lambda (_) "sshfs"))
              ((symbol-function 'make-directory) #'ignore)
              ((symbol-function 'set-file-modes) #'ignore)
              ((symbol-function 'make-process) (lambda (&rest args) (setq command (plist-get args :command)) 'mount))
              ((symbol-function 'process-put) #'ignore)
              ((symbol-function 'process-live-p) (lambda (_) t))
              ((symbol-function 'universel--mounted-p) (lambda (_) t)))
      (universel--ensure-mount environment "/unused-test-state/")
      (should (equal (cl-subseq command 0 4) '("sshfs" "-f" "-p" "2222"))))))

(ert-deftest universel-mounted-check-does-not-use-an-unavailable-buffer-directory ()
  (let ((default-directory "/unavailable-test-mount/"))
    (cl-letf (((symbol-function 'process-file)
               (lambda (&rest _)
                 (should (equal default-directory (universel-home-directory))) 1)))
      (should-not (universel--mounted-p "/unavailable-test-mount/")))))

(ert-deftest universel-release-unmounts-before-stopping-from-a-local-directory ()
  (let* ((environment '(:platform windows :transport ssh :destination "test"))
         (universel--mounts (make-hash-table :test #'equal))
         (default-directory "/unavailable-test-mount/")
         (mounted t) events)
    (puthash (universel-mount-key environment) 'sshfs universel--mounts)
    (cl-letf (((symbol-function 'universel--mounted-p) (lambda (_) mounted))
              ((symbol-function 'process-file)
               (lambda (&rest _)
                 (should (equal default-directory (universel-home-directory)))
                 (push 'unmount events) (setq mounted nil) 0))
              ((symbol-function 'process-live-p) (lambda (_) t))
              ((symbol-function 'process-put) #'ignore)
              ((symbol-function 'delete-process) (lambda (_) (push 'stop events)))
              ((symbol-function 'file-directory-p) (lambda (_) nil)))
      (universel-release-files environment "/tmp/test-state/")
      (should (equal (nreverse events) '(unmount stop)))
      (should (= (hash-table-count universel--mounts) 0)))))

(ert-deftest universel-release-retains-process-when-unmount-fails ()
  (let* ((environment '(:platform windows :transport ssh :destination "test"))
         (universel--mounts (make-hash-table :test #'equal)))
    (puthash (universel-mount-key environment) 'sshfs universel--mounts)
    (cl-letf (((symbol-function 'universel--mounted-p) (lambda (_) t))
              ((symbol-function 'process-file) (lambda (&rest _) 1))
              ((symbol-function 'delete-process) (lambda (_) (ert-fail "SSHFS stopped before unmount"))))
      (should-error (universel-release-files environment "/tmp/test-state/") :type 'user-error)
      (should (eq (gethash (universel-mount-key environment) universel--mounts) 'sshfs)))))

(ert-run-tests-batch-and-exit)
;;; test-universel.el ends here

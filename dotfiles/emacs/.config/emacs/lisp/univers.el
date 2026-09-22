;;; univers.el --- Operations across local and remote systems -*- lexical-binding: t; -*-

;; Package-Requires: ((emacs "29.1"))
;; Keywords: processes, files

;;; Commentary:

;; A PLATFORM argument is nil (detect from the operation), a symbol such as
;; `windows', `posix', or `linux', or an environment plist.  Plists carry
;; :platform, :transport (local, ssh, wsl), :destination, :port, :directory,
;; and optionally :mount-root.  An explicit platform wins over detection;
;; it does not change the machine on which a command executes.
;;
;; File operations infer their environment from the supplied directory or
;; `default-directory'.  Local configuration paths and local process launch
;; machinery explicitly use `universel-host-platform'.  In particular, a
;; Windows SSH destination must not select Windows local launch machinery
;; when Emacs itself runs on Linux.
;;
;; This file depends only on Emacs.  Optional adapters translate other
;; packages' records into environments; no workspace or agent records belong
;; here.  Loading this library performs no downloads, mounts, or registration.

;;; Code:

(require 'cl-lib)
(require 'subr-x)
(require 'tramp)

(defgroup universel nil "Operations across operating systems." :group 'environment)

(defvar universel-environment-functions nil
  "Functions called with a directory to identify an otherwise local-looking path.
The first non-nil environment plist wins.  Used by remote mount adapters.")

(defvar universel--mounts (make-hash-table :test #'equal))
(defvar universel-clock-ticks-per-second 100)
(defvar universel-log-function #'message
  "Function accepting a format string and arguments for connection messages.")

(defun universel-host-platform ()
  "Return the operating system running Emacs, independently of the buffer."
  (pcase system-type
    ('windows-nt 'windows)
    ('gnu/linux 'linux)
    ('darwin 'macos)
    (_ 'posix)))

(defun universel--normalize-platform (platform)
  (pcase platform
    ((or 'windows 'windows-nt) 'windows)
    ((or 'linux 'gnu/linux) 'linux)
    ((or 'macos 'darwin) 'macos)
    ('posix 'posix)
    (_ (error "Unknown Universel platform: %S" platform))))

(defun universel-environment (&optional platform directory)
  "Resolve PLATFORM for DIRECTORY, or the current operation when nil.
PLATFORM may be a symbol or an environment plist.  SSH TRAMP methods describe
POSIX connections; native Windows SSH connections must identify themselves
explicitly or through `universel-environment-functions'."
  (let* ((requested-directory directory)
         (explicit (and (consp platform) (copy-sequence platform)))
         (directory (or directory (plist-get explicit :directory) default-directory))
         (method (file-remote-p directory 'method))
         (detected
          (or explicit
              (run-hook-with-args-until-success 'universel-environment-functions directory)
              (when method
                (let* ((remote (tramp-dissect-file-name directory))
                       (user (tramp-file-name-user remote))
                       (host (tramp-file-name-host remote)))
                  (list :platform 'posix
                        :transport (if (equal method "wsl") 'wsl 'ssh)
                        :destination (if user (format "%s@%s" user host) host)
                        :port (tramp-file-name-port remote)
                        :directory (file-remote-p directory 'localname))))
              (list :platform (universel-host-platform) :transport 'local
                    :directory directory))))
    (setq detected (copy-sequence detected))
    (unless (plist-get detected :transport)
      (setf (plist-get detected :transport) 'local))
    (unless (plist-get detected :platform)
      (setf (plist-get detected :platform)
            (if (eq (plist-get detected :transport) 'local)
                (universel-host-platform) 'posix)))
    (when (and platform (symbolp platform))
      (setq detected (plist-put detected :platform platform)))
    (when (or (and explicit requested-directory) (not (plist-get detected :directory)))
      (setf (plist-get detected :directory)
            (or (file-remote-p directory 'localname) directory)))
    (setf (plist-get detected :platform)
          (universel--normalize-platform (plist-get detected :platform)))
    detected))

(defun universel-platform (&optional platform directory)
  "Return the operating system for PLATFORM and DIRECTORY."
  (cond ((and platform (symbolp platform)) (universel--normalize-platform platform))
        ((plist-get platform :platform)
         (universel--normalize-platform (plist-get platform :platform)))
        (t (plist-get (universel-environment platform directory) :platform))))

(defun universel-platform-p (wanted &optional platform directory)
  "Whether PLATFORM for DIRECTORY matches WANTED.
POSIX includes Linux and macOS."
  (let ((actual (universel-platform platform directory)))
    (if (eq wanted 'posix) (memq actual '(posix linux macos))
      (eq actual (universel--normalize-platform wanted)))))

(defun universel-select (choices &optional platform)
  "Select a value from CHOICES for PLATFORM, falling back to its `t' entry."
  (let ((actual (universel-platform platform)))
    (cdr (or (assq actual choices)
             (and (memq actual '(linux macos posix)) (assq 'posix choices))
             (assq t choices)))))

(defun universel-home-directory ()
  "Return the local home directory as understood by Emacs."
  (let ((default-directory temporary-file-directory))
    (file-name-as-directory (expand-file-name "~/"))))

(defun universel-host-environment ()
  "Return an explicitly local environment, even from a remote buffer."
  (list :platform (universel-host-platform) :transport 'local
        :directory (universel-home-directory)))

(defun universel-standard-directory (kind &optional platform)
  "Return the local data, state, or cache directory for KIND and PLATFORM.
Environment variables belong to the running Emacs process, not an SSH host."
  (let ((platform (or platform (universel-host-platform)))
        (default-directory (universel-home-directory)))
    (file-name-as-directory
     (expand-file-name
      (or (getenv (pcase kind
                    ('data "XDG_DATA_HOME") ('state "XDG_STATE_HOME")
                    ('cache "XDG_CACHE_HOME") (_ (error "Unknown path kind: %S" kind))))
          (and (universel-platform-p 'windows platform) (getenv "LOCALAPPDATA"))
          (pcase kind ('data "~/.local/share") ('state "~/.local/state")
                 ('cache "~/.cache")))))))

(defun universel-standard-path (kind relative &optional platform)
  "Return RELATIVE below the standard KIND directory for PLATFORM."
  (expand-file-name relative (universel-standard-directory kind platform)))

(defun universel-default-shell (&optional platform)
  "Return the shell for PLATFORM, detecting the current operation when nil."
  (let ((environment (universel-environment platform)))
    (cond
     ((universel-platform-p 'windows environment)
      (if (eq (plist-get environment :transport) 'local)
          (or (executable-find "pwsh.exe") (executable-find "powershell.exe")
              "powershell.exe")
        "powershell.exe"))
     ((eq (plist-get environment :transport) 'local)
      (or (getenv "SHELL") shell-file-name "/bin/sh"))
     (t "/bin/sh"))))

(defun universel-quote-argument (argument &optional platform)
  "Quote ARGUMENT for PLATFORM's shell, not the launching computer's shell."
  (if (universel-platform-p 'windows platform)
      (concat "'" (replace-regexp-in-string "'" "''" argument t t) "'")
    (shell-quote-argument argument t)))

(defun universel--powershell-encoded (script)
  (base64-encode-string (encode-coding-string script 'utf-16le t) t))

(defun universel-native-path (path &optional platform)
  "Express PATH in PLATFORM's native syntax."
  (if (universel-platform-p 'windows platform path)
      (replace-regexp-in-string "/" (string ?\\) (string-remove-prefix "/" path) t t)
    path))

(defun universel-run-command-lines (command timeout &optional platform)
  "Run local argv COMMAND with TIMEOUT and return nonempty output lines.
PLATFORM selects the local launch convention; nil detects the Emacs host.
Use `universel-command' first to construct an SSH or WSL launch."
  (let ((default-directory (universel-home-directory)))
   (with-temp-buffer
    (let* ((windows (universel-platform-p 'windows (or platform (universel-host-platform))))
           (output-file (and windows (make-temp-file "universel-process-" nil ".txt")))
           (windows-command
            (when windows
              (format "& %s 2>&1 | Set-Content -Encoding utf8 %s"
                      (mapconcat (lambda (arg) (universel-quote-argument arg 'windows))
                                 command " ")
                      (universel-quote-argument output-file 'windows))))
           process)
      (unwind-protect
          (progn
            (setq process
                  (make-process
                   :name "universel-command" :buffer (current-buffer)
                   :command (if windows-command
                                (list (or (executable-find "pwsh.exe") "powershell.exe")
                                      "-NoProfile" "-NonInteractive" "-Command" windows-command)
                              command)
                   :connection-type 'pipe :noquery t :sentinel #'ignore))
            (let ((deadline (+ (float-time) timeout)))
              (while (and (process-live-p process) (< (float-time) deadline))
                (accept-process-output process 0.05)))
            (when (process-live-p process) (delete-process process))
            (when (and output-file (file-exists-p output-file))
              (insert-file-contents output-file))
            (when (or output-file
                      (and (eq (process-status process) 'exit)
                           (zerop (process-exit-status process))))
              (let ((lines (split-string (buffer-string) "[\r\n]+" t)))
                (when lines (setcar lines (string-remove-prefix "\ufeff" (car lines))))
                lines)))
        (when (and process (process-live-p process)) (delete-process process))
        (when (and output-file (file-exists-p output-file)) (delete-file output-file)))))))

(defun universel-command (program arguments directory &optional platform)
  "Return a launch plist for PROGRAM and ARGUMENTS in DIRECTORY on PLATFORM.
The result contains :program, :arguments, and the local launch :directory."
  (let* ((environment (universel-environment platform directory))
         (transport (plist-get environment :transport))
         (destination (plist-get environment :destination))
         (port (plist-get environment :port))
         (directory (if (and (eq transport 'ssh)
                             (universel-platform-p 'windows environment))
                        (universel-native-path (plist-get environment :directory) 'windows)
                      (plist-get environment :directory))))
    (pcase transport
      ('local (list :program program :arguments arguments :directory directory))
      ('wsl
       (list :program "wsl.exe" :directory (universel-home-directory)
             :arguments (append (when destination (list "-d" destination))
                                (list "--cd" directory "--" program) arguments)))
      ('ssh
       (let ((remote-command
              (if (universel-platform-p 'windows environment)
                  (format "powershell.exe -NoLogo -NoProfile -EncodedCommand %s"
                          (universel--powershell-encoded
                           (format "Set-Location -LiteralPath %s; & %s %s"
                                   (universel-quote-argument directory 'windows)
                                   (universel-quote-argument program 'windows)
                                   (mapconcat (lambda (arg) (universel-quote-argument arg 'windows))
                                              arguments " "))))
                (format "cd -- %s && exec %s%s"
                        (universel-quote-argument directory 'posix)
                        (universel-quote-argument program 'posix)
                        (if arguments
                            (concat " " (mapconcat (lambda (arg) (universel-quote-argument arg 'posix))
                                                   arguments " ")) "")))))
         (list :program "ssh" :directory (universel-home-directory)
               :arguments (append '("-t") (when port (list "-p" (format "%s" port)))
                                  (list destination remote-command)))))
      (_ (error "Unsupported transport: %S" transport)))))

(defun universel-shell-command (directory &optional platform)
  "Return the existing interactive terminal launch for DIRECTORY on PLATFORM.
Local launches carry :shell and leave :program nil for a terminal package's
normal shell creation.  Arguments intentionally retain the existing policy."
  (let* ((environment (universel-environment platform directory))
         (destination (plist-get environment :destination)))
    (pcase (plist-get environment :transport)
      ('local (list :program nil :shell (universel-default-shell environment)
                    :arguments '("-l") :directory directory))
      ('wsl (universel-command "bash" '("-l") directory environment))
      ('ssh
       (list :program "ssh" :directory (universel-home-directory)
             :arguments
             (if (universel-platform-p 'windows environment)
                 (list destination
                       (format "powershell.exe -NoLogo -NoExit -Command \"Set-Location -LiteralPath %s\""
                               (universel-quote-argument
                                (universel-native-path directory 'windows) 'windows)))
               (list destination))))
      (_ (error "Unsupported terminal environment: %S" environment)))))

(defun universel-find-programs (programs timeout &optional platform)
  "Find PROGRAMS on PLATFORM within TIMEOUT.
Return :programs and, for WSL discovery, :distribution."
  (let* ((environment (universel-environment platform))
         (destination (plist-get environment :destination))
         (port (plist-get environment :port))
         (script "for command do command -v -- \"$command\" >/dev/null 2>&1 && printf '%s\\n' \"$command\"; done"))
    (pcase (plist-get environment :transport)
      ('local (list :programs (cl-remove-if-not #'executable-find programs)))
      ('wsl
       (when-let* ((wsl (executable-find "wsl.exe"))
                   (lines (universel-run-command-lines
                           (append (list wsl) (when destination (list "-d" destination))
                                   (list "-e" "sh" "-lc"
                                         (concat "printf \"__distribution__%s\\n\" \"$WSL_DISTRO_NAME\"; " script)
                                         "aipanel") programs) timeout))
                   (header (car lines))
                   ((string-prefix-p "__distribution__" header)))
         (list :distribution (string-remove-prefix "__distribution__" header)
               :programs (cdr lines))))
      ('ssh
       (when-let* ((ssh (executable-find "ssh")))
         (let ((remote-command
                (if (universel-platform-p 'windows environment)
                    (format "powershell.exe -NoProfile -NonInteractive -EncodedCommand %s"
                            (universel--powershell-encoded
                             (mapconcat
                              (lambda (program)
                                (let ((quoted (universel-quote-argument program 'windows)))
                                  (format "if (Get-Command -Name %s -ErrorAction SilentlyContinue) { Write-Output %s }"
                                          quoted quoted))) programs "; ")))
                  (mapconcat (lambda (arg) (universel-quote-argument arg 'posix))
                             (append (list "sh" "-lc" script "aipanel") programs) " "))))
           (list :programs
                 (universel-run-command-lines
                  (append (list ssh) (when port (list "-p" (format "%s" port)))
                          (list destination remote-command)) timeout)))))
      (_ (error "Unsupported discovery environment: %S" environment)))))

(defun universel-register-wsl ()
  "Register the existing Windows-to-WSL Emacs file connection method."
  (when (universel-platform-p 'windows (universel-host-platform))
    (add-to-list 'tramp-methods
                 '("wsl" (tramp-login-program "C:/Windows/System32/wsl.exe")
                   (tramp-login-args (("-d") ("%h") ("-u" "%u")
                                      ("-e" "/bin/sh" "-c" "\"exec 2>&1" "%l" "\"")))
                   (tramp-remote-shell "/bin/sh")
                   (tramp-remote-shell-login ("-l")) (tramp-remote-shell-args ("-c"))))))

(defun universel-mount-key (environment)
  "Return the sharing key for ENVIRONMENT's remote filesystem."
  (concat (plist-get environment :destination) "\0"
          (or (plist-get environment :mount-root) "/C:/")))

(defun universel-mount-point (environment state-directory)
  "Return ENVIRONMENT's mount directory below STATE-DIRECTORY."
  (let ((default-directory (universel-home-directory))
        (name (string-trim (replace-regexp-in-string
                            "[^[:alnum:]_-]+" "-"
                            (downcase (plist-get environment :destination))) "-+" "-+"))
        (digest (substring (secure-hash 'sha256 (universel-mount-key environment)) 0 12)))
    (expand-file-name (format "mounts/%s-%s/" name digest) state-directory)))

(defun universel-mounted-environment (directory environment state-directory)
  "Identify DIRECTORY inside ENVIRONMENT's mount without opening a connection."
  (when (and (eq (plist-get environment :transport) 'ssh)
             (universel-platform-p 'windows environment))
    (let* ((default-directory (universel-home-directory))
           (mount (universel-mount-point environment state-directory))
           (path (expand-file-name directory)))
      (when (string-prefix-p mount path)
        (plist-put (copy-sequence environment) :directory
                   (expand-file-name (substring path (length mount))
                                     (or (plist-get environment :mount-root) "/C:/")))))))

(defun universel--mounted-p (directory)
  (zerop (process-file "mountpoint" nil nil nil "--quiet" directory)))

(defun universel--mount-sentinel (process event)
  (unless (process-live-p process)
    (when-let* ((key (process-get process 'universel-mount-key)))
      (when (eq process (gethash key universel--mounts)) (remhash key universel--mounts)))
    (unless (or (process-get process 'universel-intentional-stop)
                (string-match-p "finished" event))
      (funcall universel-log-function "Windows SFTP mount exited: %s" (string-trim event)))))

(defun universel--ensure-mount (environment state-directory)
  (unless (executable-find "sshfs") (user-error "Windows workspace requires the sshfs package"))
  (let* ((default-directory (universel-home-directory))
         (key (universel-mount-key environment))
         (existing (gethash key universel--mounts))
         (mount-point (universel-mount-point environment state-directory))
         (destination (plist-get environment :destination))
         (source (format "%s:%s" destination (or (plist-get environment :mount-root) "/C:/"))))
    (if (and existing (process-live-p existing) (universel--mounted-p mount-point))
        mount-point
      (make-directory mount-point t)
      (set-file-modes mount-point #o700)
      (let* ((buffer (get-buffer-create (format "*windows-mount:%s*" destination)))
             (process (make-process
                       :name (format "windows-mount:%s" destination) :buffer buffer
                       :command (list "sshfs" "-f" source mount-point "-o"
                                      "BatchMode=yes,ConnectTimeout=10,ServerAliveInterval=5,ServerAliveCountMax=2,auto_unmount,idmap=user")
                       :connection-type 'pipe :noquery t :sentinel #'universel--mount-sentinel))
             (deadline (+ (float-time) 10)))
        (process-put process 'universel-mount-key key)
        (puthash key process universel--mounts)
        (while (and (process-live-p process) (not (universel--mounted-p mount-point))
                    (< (float-time) deadline))
          (accept-process-output process 0.1))
        (unless (universel--mounted-p mount-point)
          (when (process-live-p process) (delete-process process))
          (remhash key universel--mounts)
          (when (and (file-directory-p mount-point)
                     (null (directory-files mount-point nil directory-files-no-dot-files-regexp)))
            (delete-directory mount-point))
          (user-error "Windows SFTP mount failed for %s; see %s" destination (buffer-name buffer)))
        mount-point))))

(defun universel-file-directory (directory &optional platform state-directory)
  "Expose DIRECTORY on PLATFORM to Emacs.
STATE-DIRECTORY is required for the existing SSHFS Windows connection."
  (let* ((environment (universel-environment platform directory))
         (directory (plist-get environment :directory))
         (destination (plist-get environment :destination)))
    (pcase (plist-get environment :transport)
      ('local (file-name-as-directory (expand-file-name directory)))
      ('wsl (format "/wsl:%s:%s" destination (file-name-as-directory directory)))
      ('ssh
       (if (universel-platform-p 'windows environment)
           (let ((root (or (plist-get environment :mount-root) "/C:/")))
             (unless state-directory (error "A mount state directory is required"))
             (expand-file-name (file-relative-name (file-name-as-directory directory) root)
                               (universel--ensure-mount environment state-directory)))
         (format "/ssh:%s:%s" destination (file-name-as-directory directory))))
      (_ (error "Unsupported file environment: %S" environment)))))

(defun universel-file-path (path &optional platform state-directory)
  "Translate Emacs PATH into a connection path on PLATFORM."
  (let ((environment (if platform (universel-environment platform)
                       (universel-environment nil path))))
    (cond
     ((and (eq (plist-get environment :transport) 'ssh)
           (universel-platform-p 'windows environment))
      (expand-file-name
        (file-relative-name path (universel-file-directory
                                  (plist-get environment :directory) environment state-directory))
        (file-name-as-directory (plist-get environment :directory))))
     ((file-remote-p path) (file-remote-p path 'localname))
     (t path))))

(defun universel-execution-path (path &optional platform state-directory)
  "Translate Emacs PATH into native execution syntax on PLATFORM."
  (let ((platform (or platform (universel-environment nil path))))
    (universel-native-path (universel-file-path path platform state-directory) platform)))

(defun universel-release-files (environment state-directory)
  "Release ENVIRONMENT's mounted files below STATE-DIRECTORY."
  (when (and (eq (plist-get environment :transport) 'ssh)
             (universel-platform-p 'windows environment))
    (let* ((key (universel-mount-key environment))
           (process (gethash key universel--mounts))
           (mount-point (universel-mount-point environment state-directory)))
      (when (and process (process-live-p process))
        (process-put process 'universel-intentional-stop t)
        (delete-process process))
      (when (universel--mounted-p mount-point)
        (process-file "fusermount3" nil nil nil "--unmount" mount-point))
      (remhash key universel--mounts)
      (when (and (file-directory-p mount-point)
                 (null (directory-files mount-point nil directory-files-no-dot-files-regexp)))
        (delete-directory mount-point)))))

(defun universel-process-observation-p (&optional platform)
  "Whether the existing foreground-process observer supports PLATFORM."
  (let ((environment (universel-environment platform)))
    (and (eq (plist-get environment :transport) 'local)
         (universel-platform-p 'linux environment))))

(defun universel--read-proc (file &optional literally)
  (with-temp-buffer
    (if literally (insert-file-contents-literally file) (insert-file-contents file))
    (buffer-string)))

(defun universel--process-entry (pid)
  (condition-case nil
      (let* ((stat (universel--read-proc (format "/proc/%d/stat" pid)))
             (fields (split-string (substring stat (+ 2 (string-match ") " stat)))))
             (argv (mapcar (lambda (arg) (decode-coding-string arg 'utf-8))
                           (split-string (universel--read-proc (format "/proc/%d/cmdline" pid) t) "\0" t))))
        (list :pid pid :state (nth 0 fields) :ppid (string-to-number (nth 1 fields))
              :pgrp (string-to-number (nth 2 fields)) :tty (string-to-number (nth 4 fields))
              :tpgid (string-to-number (nth 5 fields))
              :start-ticks (string-to-number (nth 19 fields))
              :executable (file-truename (format "/proc/%d/exe" pid)) :argv argv))
    (error nil)))

(defun universel-process-table (&optional platform)
  "Return process observations for PLATFORM, or nil when unsupported."
  (when (universel-process-observation-p platform)
    (let (entries)
      (dolist (path (directory-files "/proc" t "\\`[0-9]+\\'"))
        (when-let* ((entry (universel--process-entry (string-to-number (file-name-nondirectory path)))))
          (push entry entries)))
      entries)))

(defun universel-foreground-process (pid table &optional direct)
  "Find PID's foreground process in TABLE, or PID itself for DIRECT launches."
  (when-let* ((owner (cl-find pid table :key (lambda (entry) (plist-get entry :pid)))))
    (if direct owner
      (let ((tpgid (plist-get owner :tpgid))
            (pgrp (plist-get owner :pgrp)))
        (unless (or (<= tpgid 0) (= tpgid pgrp))
          (let* ((group (cl-remove-if-not
                         (lambda (entry)
                           (and (= (plist-get entry :tty) (plist-get owner :tty))
                                (= (plist-get entry :pgrp) tpgid))) table))
                 (pids (mapcar (lambda (entry) (plist-get entry :pid)) group))
                 (roots (cl-remove-if
                         (lambda (entry) (member (plist-get entry :ppid) pids)) group)))
            (when (= (length roots) 1) (car roots))))))))

(defun universel-process-runtime (entry &optional platform)
  "Return ENTRY's running time on PLATFORM."
  (unless (universel-process-observation-p platform)
    (error "Process observation is unavailable on this platform"))
  (let ((uptime (string-to-number (car (split-string (universel--read-proc "/proc/uptime"))))))
    (max 0 (- uptime (/ (float (plist-get entry :start-ticks)) universel-clock-ticks-per-second)))))

(defun universel-prepend-exec-path (directory)
  "Add local DIRECTORY to Emacs and subprocess executable search paths."
  (add-to-list 'exec-path directory)
  (setenv "PATH" (mapconcat #'identity
                            (delete-dups (cons directory (parse-colon-path (getenv "PATH"))))
                            path-separator)))

(defun universel-repair-mason-path (original-path bin-directory)
  "Repair Mason's Windows PATH using ORIGINAL-PATH and BIN-DIRECTORY."
  (when (universel-platform-p 'windows (universel-host-platform))
    (setenv "PATH" (mapconcat #'identity
                              (delete-dups (cons bin-directory (parse-colon-path original-path)))
                              path-separator))))

(defun universel-grammar-compilers ()
  "Return the existing host-specific C and C++ grammar compiler overrides."
  (when (universel-platform-p 'windows (universel-host-platform))
    (let ((cc (cond ((executable-find "gcc") "gcc") ((executable-find "clang") "clang")))
          (c++ (cond ((executable-find "g++") "g++") ((executable-find "clang++") "clang++"))))
      (when cc (cons cc c++)))))

(provide 'univers)
;;; univers.el ends here

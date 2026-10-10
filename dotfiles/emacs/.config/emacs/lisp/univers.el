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
(defvar universel--wsl-helpers (make-hash-table :test #'equal))
(defvar universel--wsl-shells (make-hash-table :test #'equal))
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

(defconst universel--wsl-share-regexp
  "\\`[/\\\\][/\\\\]wsl\\(?:\\.localhost\\|\\$\\)[/\\\\]\\([^/\\\\]+\\)\\(.*\\)\\'"
  "Windows path of a WSL distribution's files, as //wsl.localhost/NAME/PATH.")

(defun universel--wsl-share-environment (path)
  "Describe PATH under a WSL distribution's Windows share, or return nil."
  (when (stringp path)
    (let ((case-fold-search t))
      (when (string-match universel--wsl-share-regexp path)
        (let ((distribution (match-string 1 path))
              (rest (replace-regexp-in-string "\\\\" "/" (match-string 2 path) t t)))
          (list :platform 'posix :transport 'wsl :destination distribution
                :directory (if (string-empty-p rest) "/" rest)))))))

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
              (universel--wsl-share-environment directory)
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
     ((eq (plist-get environment :transport) 'wsl)
      (let ((distribution (plist-get environment :destination)))
        (or (gethash distribution universel--wsl-shells)
            (when-let* ((answer (universel--wsl-request distribution "shell"))
                        ((not (string-empty-p (string-trim answer)))))
              (puthash distribution (string-trim answer) universel--wsl-shells))
            "/bin/sh")))
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

(defun universel-command (program arguments directory &optional platform variables)
  "Return a launch plist for PROGRAM and ARGUMENTS in DIRECTORY on PLATFORM.
The result contains :program, :arguments, and the local launch :directory.
VARIABLES are NAME=VALUE strings PROGRAM starts with on a POSIX system."
  (when variables
    (setq arguments (append variables (cons program arguments))
          program "env"))
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
                           (format "$ErrorActionPreference = 'Stop'; Set-Location -LiteralPath %s -ErrorAction Stop; & %s %s"
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

(defun universel-shell-command (directory &optional platform variables)
  "Return the existing interactive terminal launch for DIRECTORY on PLATFORM.
Local launches carry :shell and leave :program nil for a terminal package's
 normal shell creation.  WSL launches also carry :shell and its :location, and
start with the NAME=VALUE VARIABLES.  Remote launches honor the supplied
directory and port."
  (let* ((environment (universel-environment platform directory))
          (destination (plist-get environment :destination))
          (port (plist-get environment :port)))
    (pcase (plist-get environment :transport)
      ('local (list :program nil :shell (universel-default-shell environment)
                    :arguments '("-l") :directory directory))
      ('wsl
       (let ((shell (universel-default-shell environment)))
         (append (universel-command shell '("-l") (plist-get environment :directory)
                                    environment variables)
                 (list :shell shell :location environment))))
      ('ssh
       (list :program "ssh" :directory (universel-home-directory)
             :arguments
             (append (when port (list "-p" (format "%s" port)))
              (if (universel-platform-p 'windows environment)
                 (list destination
                       (format "powershell.exe -NoLogo -NoExit -Command \"Set-Location -LiteralPath %s\""
                               (universel-quote-argument
                                (universel-native-path directory 'windows) 'windows)))
               (list "-t" destination
                     (format "cd -- %s && exec \"${SHELL:-/bin/sh}\" -l"
                             (universel-quote-argument (plist-get environment :directory) 'posix)))))))
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
      ((and 'wsl (guard destination))
       (when-let* ((answer (universel--wsl-request
                            destination (mapconcat #'identity (cons "programs" programs) " "))))
         (list :distribution destination :programs (split-string answer "\n" t))))
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

(defun universel-mount-key (environment)
  "Return the sharing key for ENVIRONMENT's remote filesystem."
  (concat (plist-get environment :destination) "\0"
         (if (and (plist-get environment :port)
                  (not (equal (format "%s" (plist-get environment :port)) "22")))
             (format "port=%s\0" (plist-get environment :port)) "")
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
  (let ((default-directory (universel-home-directory)))
    (zerop (process-file "mountpoint" nil nil nil "--quiet" directory))))

(defun universel-files-connected-p (environment state-directory)
  "Whether ENVIRONMENT already owns a usable Windows file connection.
This query never opens a connection.  Non-mounted transports are not owned by
Universel's file-connection lifecycle and return nil."
  (and state-directory
       (eq (plist-get environment :transport) 'ssh)
       (universel-platform-p 'windows environment)
       (when-let* ((process (gethash (universel-mount-key environment) universel--mounts)))
         (and (process-live-p process)
              (universel--mounted-p (universel-mount-point environment state-directory))))))

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
                       :command (append (list "sshfs" "-f")
                                        (when (plist-get environment :port)
                                          (list "-p" (format "%s" (plist-get environment :port))))
                                        (list source mount-point "-o"
                                              "BatchMode=yes,ConnectTimeout=10,ServerAliveInterval=5,ServerAliveCountMax=2,auto_unmount,idmap=user"))
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
      ('wsl (format "//wsl.localhost/%s%s" destination (file-name-as-directory directory)))
      ('ssh
       (if (universel-platform-p 'windows environment)
           (let ((root (or (plist-get environment :mount-root) "/C:/")))
             (unless state-directory (error "A mount state directory is required"))
             (expand-file-name (file-relative-name (file-name-as-directory directory) root)
                               (universel--ensure-mount environment state-directory)))
         (format "/ssh:%s%s:%s" destination
                 (if (plist-get environment :port) (format "#%s" (plist-get environment :port)) "")
                 (file-name-as-directory directory))))
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
     ((universel--wsl-share-environment path)
      (plist-get (universel--wsl-share-environment path) :directory))
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
    (let* ((default-directory (universel-home-directory))
           (key (universel-mount-key environment))
           (process (gethash key universel--mounts))
           (mount-point (universel-mount-point environment state-directory)))
      (when (universel--mounted-p mount-point)
        (unless (and (zerop (process-file "fusermount3" nil nil nil "--unmount" mount-point))
                     (not (universel--mounted-p mount-point)))
          (user-error "Windows SFTP unmount failed for %s" mount-point)))
      (when (and process (process-live-p process))
        (process-put process 'universel-intentional-stop t)
        (delete-process process))
      (remhash key universel--mounts)
      (when (and (file-directory-p mount-point)
                 (null (directory-files mount-point nil directory-files-no-dot-files-regexp)))
        (delete-directory mount-point)))))

(defun universel-wsl-distributions ()
  "Return the installed WSL distribution names, or nil outside a Windows host."
  (when-let* (((eq (universel-host-platform) 'windows))
              (wsl (executable-find "wsl.exe")))
    (let ((process-environment (cons "WSL_UTF8=1" process-environment)))
      (mapcar #'string-trim (universel-run-command-lines (list wsl "-l" "-q") 10)))))

(defun universel-wsl-home (distribution)
  "Return DISTRIBUTION's home directory, or nil when it does not answer."
  (when-let* ((answer (universel--wsl-request distribution "home"))
              (home (string-trim answer))
              ((string-prefix-p "/" home)))
    (file-name-as-directory home)))

(defconst universel--wsl-helper-script
  "while IFS= read -r request; do
  printf '\\nuniversel-begin\\n'
  case $request in
    table)
      cut -d' ' -f1 /proc/uptime
      for directory in /proc/[0-9]*; do
        printf '\\036%s\\037' \"${directory#/proc/}\"
        cat \"$directory/stat\" 2>/dev/null
        printf '\\037'
        readlink \"$directory/exe\" 2>/dev/null
        printf '\\037'
        tr '\\000' '\\001' < \"$directory/cmdline\" 2>/dev/null
      done ;;
    'environ '*) tr '\\000' '\\001' < \"/proc/${request#environ }/environ\" 2>/dev/null ;;
    'with '*)
      grep -lzxF -e \"${request#with }\" /proc/[0-9]*/environ 2>/dev/null |
        sed 's|^/proc/\\([0-9]*\\)/environ$|\\1|' ;;
    shell) getent passwd \"$(id -un)\" | cut -d: -f7 ;;
    home) printf '%s\\n' \"$HOME\" ;;
    'realpath '*) realpath -m -- \"${request#realpath }\" 2>/dev/null ;;
    'programs '*)
      set -f
      for program in ${request#programs }; do
        command -v -- \"$program\" >/dev/null 2>&1 && printf '%s\\n' \"$program\"
      done
      set +f ;;
  esac
  printf '\\nuniversel-end\\n'
done
"
  "Shell loop answering one request per input line inside a WSL distribution.")

(defvar universel-wsl-helper-timeout 5
  "Seconds to wait for a WSL helper before giving up on one request.")

(defun universel--wsl-helper (distribution)
  "Return DISTRIBUTION's running helper, starting it when needed."
  (let ((helper (gethash distribution universel--wsl-helpers)))
    (unless (process-live-p helper)
      (when helper (kill-buffer (process-buffer helper)))
      (setq helper
            (let ((default-directory (universel-home-directory)))
              (make-process
               :name (format "universel-wsl-%s" distribution)
               :buffer (generate-new-buffer (format " *universel-wsl-%s*" distribution))
               :command (list (or (executable-find "wsl.exe") "wsl.exe")
                              "-d" distribution "-e" "sh" "-l" "-s")
               :connection-type 'pipe :coding 'utf-8-unix :noquery t)))
      (process-send-string helper universel--wsl-helper-script)
      (puthash distribution helper universel--wsl-helpers))
    helper))

(defun universel-wsl-stop-helper (distribution)
  "Stop DISTRIBUTION's helper, if one is running."
  (when-let* ((helper (gethash distribution universel--wsl-helpers)))
    (remhash distribution universel--wsl-helpers)
    (when (process-live-p helper) (delete-process helper))
    (kill-buffer (process-buffer helper))))

(defun universel--wsl-request (distribution request)
  "Return the helper's answer to REQUEST in DISTRIBUTION, or nil after a timeout."
  (let* ((helper (universel--wsl-helper distribution))
         (buffer (process-buffer helper))
         (deadline (+ (float-time) universel-wsl-helper-timeout))
         (finished (lambda ()
                     (with-current-buffer buffer
                       (goto-char (point-min))
                       (search-forward "\nuniversel-end\n" nil t)))))
    ;; Text a login shell prints at startup comes before the begin marker.
    (with-current-buffer buffer (erase-buffer))
    (process-send-string helper (concat request "\n"))
    (while (and (process-live-p helper) (not (funcall finished))
                (< (float-time) deadline))
      (accept-process-output helper 0.05))
    (if (funcall finished)
        (with-current-buffer buffer
          (let ((end (match-beginning 0)))
            (goto-char (point-min))
            (when (search-forward "\nuniversel-begin\n" end t)
              (buffer-substring (point) end))))
      (universel-wsl-stop-helper distribution)
      nil)))

(defun universel--stat-record (pid stat executable argv)
  "Build a Linux process record for PID from its /proc stat line."
  (let ((fields (split-string (substring stat (+ 2 (string-match ") " stat))))))
    (list :pid pid :state (nth 0 fields) :ppid (string-to-number (nth 1 fields))
          :pgrp (string-to-number (nth 2 fields)) :tty (string-to-number (nth 4 fields))
          :tpgid (string-to-number (nth 5 fields))
          :start-ticks (string-to-number (nth 19 fields))
          :executable executable :argv argv)))

(defun universel--wsl-process-records (environment)
  "Return ENVIRONMENT's WSL process records, each carrying its running time."
  (when-let* ((answer (universel--wsl-request (plist-get environment :destination) "table"))
              (parts (split-string answer "\036"))
              (uptime (string-to-number (car parts))))
    (delq nil
          (mapcar (lambda (part)
                    (let ((fields (split-string part "\037")))
                      (when (and (= (length fields) 4) (string-match-p ") " (nth 1 fields)))
                        (let ((record (universel--stat-record
                                       (string-to-number (nth 0 fields)) (nth 1 fields)
                                       (string-trim-right (nth 2 fields))
                                       (split-string (nth 3 fields) "\001" t))))
                          (append record
                                  (list :runtime (max 0 (- uptime (/ (float (plist-get record :start-ticks))
                                                                    universel-clock-ticks-per-second)))
                                        :location environment))))))
                  (cdr parts)))))

(defun universel-processes-with-variable (variable &optional platform)
  "Return the IDs of processes on PLATFORM whose variables include VARIABLE.
VARIABLE is a NAME=VALUE string."
  (let ((environment (universel-environment platform)))
    (pcase (plist-get environment :transport)
      ('wsl
       (mapcar #'string-to-number
               (split-string (or (universel--wsl-request (plist-get environment :destination)
                                                         (concat "with " variable))
                                 "")
                             "\n" t)))
      ('local
       (when (universel-platform-p 'linux environment)
         (cl-loop for path in (directory-files "/proc" t "\\`[0-9]+\\'")
                  for pid = (string-to-number (file-name-nondirectory path))
                  when (member variable (universel-process-environment pid environment))
                  collect pid))))))

(defun universel-real-path (path &optional platform)
  "Return PATH on PLATFORM with every symbolic link resolved, or nil.
PATH is a connection path; a WSL path is resolved inside its distribution."
  (let ((environment (universel-environment platform)))
    (pcase (plist-get environment :transport)
      ('wsl (let ((answer (universel--wsl-request (plist-get environment :destination)
                                                  (concat "realpath " path))))
              (and answer (not (string-empty-p (string-trim answer))) (string-trim answer))))
      ('local (file-truename path)))))

(defun universel-process-observation-p (&optional platform)
  "Whether the foreground-process observer supports PLATFORM.
Linux reads /proc.  Windows asks a PowerShell helper, and WSL a helper inside
its distribution, so both need a Windows host."
  (let ((environment (universel-environment platform)))
    (or (and (eq (plist-get environment :transport) 'local)
             (or (universel-platform-p 'linux environment)
                 (and (universel-platform-p 'windows environment)
                      (eq (universel-host-platform) 'windows))))
        (and (eq (plist-get environment :transport) 'wsl)
             (eq (universel-host-platform) 'windows)))))

(defun universel--read-proc (file &optional literally)
  (with-temp-buffer
    (if literally (insert-file-contents-literally file) (insert-file-contents file))
    (buffer-string)))

(defun universel--process-entry (pid)
  (condition-case nil
      (universel--stat-record
       pid (universel--read-proc (format "/proc/%d/stat" pid))
       (file-truename (format "/proc/%d/exe" pid))
       (mapcar (lambda (arg) (decode-coding-string arg 'utf-8))
               (split-string (universel--read-proc (format "/proc/%d/cmdline" pid) t) "\0" t)))
    (error nil)))

(defun universel-process-environment (pid &optional platform)
  "Return PID's environment on PLATFORM as NAME=VALUE strings, or nil.
Windows keeps hidden =C:-style entries for each drive's current folder; they
are not variables and are left out."
  (when (universel-process-observation-p platform)
    (cond
     ((eq (plist-get (universel-environment platform) :transport) 'wsl)
      (when-let* ((answer (universel--wsl-request
                           (plist-get (universel-environment platform) :destination)
                           (format "environ %d" pid))))
        (split-string answer "\001" t)))
     ((universel-platform-p 'windows (universel-environment platform))
      (cl-remove-if (lambda (variable) (string-prefix-p "=" variable))
                    (universel--windows-helper-request (format "environment %d" pid))))
     (t
      (condition-case nil
          (mapcar (lambda (variable) (decode-coding-string variable 'utf-8))
                  (split-string (universel--read-proc (format "/proc/%d/environ" pid) t) "\0" t))
        (error nil))))))

(defun universel-shell-environment (shell start &optional platform)
  "Return a fresh interactive SHELL's variables on PLATFORM, or nil.
SHELL is a plist with `:executable' and `:login'; START holds the NAME=VALUE
variables it starts with.  Startup files may print to standard output, so
only the text after a marker is read.  Windows shows a shell's current
variables rather than its starting ones, so a Windows shell starts from
Emacs's own variables, as a new terminal does, instead of START.  A WSL shell
starts inside its distribution with exactly START."
  (when (universel-process-observation-p platform)
    (with-temp-buffer
      (let* ((environment (universel-environment platform))
             (wsl (eq (plist-get environment :transport) 'wsl))
             (windows (and (not wsl) (universel-platform-p 'windows environment)))
             (process-environment (if (or windows wsl) process-environment start))
             (default-directory (universel-home-directory))
             (coding-system-for-read 'utf-8)
             (marker "\0universel-environment\0")
             (arguments
              (if windows
                  (list "-NoLogo" "-NonInteractive" "-EncodedCommand"
                        (universel--powershell-encoded
                         (concat "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)\n"
                                 "[Console]::Out.Write(\"`0universel-environment`0\")\n"
                                 "Get-ChildItem env: | ForEach-Object "
                                 "{ [Console]::Out.Write(\"$($_.Name)=$($_.Value)`0\") }")))
                '("-i" "-c" "printf '\\0universel-environment\\0'; env -0"))))
        (when (and (eql 0 (if wsl
                              (apply #'call-process (or (executable-find "wsl.exe") "wsl.exe")
                                     nil '(t nil) nil
                                     (append (list "-d" (plist-get environment :destination)
                                                   "-e" "env" "-i")
                                             start (list (plist-get shell :executable))
                                             (plist-get shell :login) arguments))
                            (apply #'call-process (plist-get shell :executable) nil '(t nil) nil
                                   (append (plist-get shell :login) arguments))))
                   (progn (goto-char (point-min)) (search-forward marker nil t)))
          (split-string (buffer-substring (point) (point-max)) "\0" t))))))

(defconst universel--windows-process-script
  "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class UniverselEnvironment {
  [StructLayout(LayoutKind.Sequential)]
  struct BasicInformation {
    public IntPtr ExitStatus; public IntPtr PebBaseAddress; public IntPtr AffinityMask;
    public IntPtr BasePriority; public IntPtr UniqueProcessId; public IntPtr ParentProcessId;
  }
  [DllImport(\"ntdll.dll\")]
  static extern int NtQueryInformationProcess(IntPtr process, int kind, ref BasicInformation information, int length, out int returned);
  [DllImport(\"kernel32.dll\", SetLastError = true)]
  static extern IntPtr OpenProcess(int access, bool inherit, int pid);
  [DllImport(\"kernel32.dll\", SetLastError = true)]
  static extern bool ReadProcessMemory(IntPtr process, IntPtr address, byte[] buffer, IntPtr size, out IntPtr read);
  [DllImport(\"kernel32.dll\")]
  static extern bool IsWow64Process(IntPtr process, out bool wow64);
  [DllImport(\"kernel32.dll\")]
  static extern bool CloseHandle(IntPtr handle);

  static IntPtr ReadPointer(IntPtr process, IntPtr address) {
    var buffer = new byte[8]; IntPtr read;
    return ReadProcessMemory(process, address, buffer, (IntPtr)8, out read)
      ? (IntPtr)BitConverter.ToInt64(buffer, 0) : IntPtr.Zero;
  }

  // 64-bit layout: PEB.ProcessParameters at 0x20, then Environment at 0x80
  // and EnvironmentSize at 0x3F0 in RTL_USER_PROCESS_PARAMETERS.
  public static string[] Read(int pid) {
    IntPtr process = OpenProcess(0x1000 | 0x10, false, pid);
    if (process == IntPtr.Zero) return null;
    try {
      bool wow64;
      if (!Environment.Is64BitProcess || (IsWow64Process(process, out wow64) && wow64)) return null;
      var information = new BasicInformation(); int returned;
      if (NtQueryInformationProcess(process, 0, ref information, Marshal.SizeOf(information), out returned) != 0)
        return null;
      IntPtr parameters = ReadPointer(process, information.PebBaseAddress + 0x20);
      if (parameters == IntPtr.Zero) return null;
      IntPtr block = ReadPointer(process, parameters + 0x80);
      long size = ReadPointer(process, parameters + 0x3F0).ToInt64();
      if (block == IntPtr.Zero || size <= 0 || size > (1 << 24)) return null;
      var buffer = new byte[size]; IntPtr read;
      if (!ReadProcessMemory(process, block, buffer, (IntPtr)size, out read)) return null;
      string text = System.Text.Encoding.Unicode.GetString(buffer, 0, (int)read);
      int end = text.IndexOf(\"\\0\\0\", StringComparison.Ordinal);
      if (end >= 0) text = text.Substring(0, end);
      return text.Split(new[] { '\\0' }, StringSplitOptions.RemoveEmptyEntries);
    } finally { CloseHandle(process); }
  }
}
'@
while ($null -ne ($request = [Console]::In.ReadLine())) {
  if ($request -match '^environment (\\d+)$') {
    $answer = [UniverselEnvironment]::Read([int]$Matches[1])
    [Console]::Out.WriteLine((ConvertTo-Json -InputObject $answer -Compress))
  } else {
    $records = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,ExecutablePath,CommandLine,CreationDate |
      Where-Object CommandLine | ForEach-Object {
        @{ pid = $_.ProcessId; ppid = $_.ParentProcessId; executable = $_.ExecutablePath
           command = $_.CommandLine
           start = if ($_.CreationDate) { ([DateTimeOffset]$_.CreationDate).ToUnixTimeMilliseconds() } else { 0 } } })
    [Console]::Out.WriteLine((ConvertTo-Json -InputObject $records -Compress))
  }
  [Console]::Out.WriteLine('universel-end')
  [Console]::Out.Flush()
}"
  "PowerShell loop answering `query' with the process list and `environment PID'
with that process's variables, both as JSON.")

(defvar universel-windows-process-timeout 3
  "Seconds to wait for the Windows process helper before skipping an observation.")
(defvar universel--windows-process-helper nil)

(defun universel--windows-process-helper ()
  "Return the running PowerShell process helper, starting it when needed."
  (unless (process-live-p universel--windows-process-helper)
    (when universel--windows-process-helper
      (kill-buffer (process-buffer universel--windows-process-helper)))
    (setq universel--windows-process-helper
          (let ((default-directory (universel-home-directory)))
            (make-process
             :name "universel-processes"
             :buffer (generate-new-buffer " *universel-processes*")
             :command (list (universel-default-shell (universel-host-environment))
                            "-NoLogo" "-NoProfile" "-NonInteractive" "-EncodedCommand"
                            (universel--powershell-encoded universel--windows-process-script))
             :connection-type 'pipe :coding 'utf-8 :noquery t))))
  universel--windows-process-helper)

(defun universel--windows-helper-request (request)
  "Send REQUEST to the helper and return its parsed JSON answer, or nil.
A helper that does not answer in time is replaced on the next request."
  (let* ((process (universel--windows-process-helper))
         (buffer (process-buffer process))
         (deadline (+ (float-time) universel-windows-process-timeout))
         (finished (lambda ()
                     (with-current-buffer buffer
                       (goto-char (point-min))
                       (re-search-forward "^universel-end" nil t)))))
    (with-current-buffer buffer (erase-buffer))
    (process-send-string process (concat request "\n"))
    (while (and (process-live-p process) (not (funcall finished))
                (< (float-time) deadline))
      (accept-process-output process 0.05))
    (if (funcall finished)
        (with-current-buffer buffer
          (json-parse-string (buffer-substring (point-min) (match-beginning 0))
                             :object-type 'plist :array-type 'list :null-object nil))
      (delete-process process)
      nil)))

(defun universel--windows-process-records ()
  "Return Windows process records from the helper, or nil."
  (mapcar (lambda (record)
            (let ((argv (universel-windows-split-command-line (plist-get record :command))))
              (list :pid (plist-get record :pid) :ppid (plist-get record :ppid)
                    :executable (or (plist-get record :executable) (car argv))
                    :argv argv
                    :start-time (/ (plist-get record :start) 1000.0))))
          (universel--windows-helper-request "query")))

(defun universel-windows-split-command-line (line)
  "Split Windows command LINE into arguments as CommandLineToArgvW does.
The program name ends at its closing quote or first blank, without escapes."
  (when line
    (let ((index 0) (end (length line)) arguments)
      (if (and (< index end) (eq (aref line index) ?\"))
          (let ((start (1+ index)))
            (setq index start)
            (while (and (< index end) (not (eq (aref line index) ?\"))) (cl-incf index))
            (push (substring line start index) arguments)
            (when (< index end) (cl-incf index)))
        (while (and (< index end) (not (memq (aref line index) '(?\s ?\t)))) (cl-incf index))
        (push (substring line 0 index) arguments))
      (while (< index end)
        (while (and (< index end) (memq (aref line index) '(?\s ?\t))) (cl-incf index))
        (when (< index end)
          (let ((argument "") quoted)
            (while (and (< index end) (or quoted (not (memq (aref line index) '(?\s ?\t)))))
              (pcase (aref line index)
                (?\\
                 (let ((count 0))
                   (while (and (< index end) (eq (aref line index) ?\\))
                     (cl-incf count) (cl-incf index))
                   (if (and (< index end) (eq (aref line index) ?\"))
                       (progn
                         (setq argument (concat argument (make-string (/ count 2) ?\\)))
                         (when (cl-oddp count)
                           (setq argument (concat argument "\"") index (1+ index))))
                     (setq argument (concat argument (make-string count ?\\))))))
                (?\"
                 (if (and quoted (< (1+ index) end) (eq (aref line (1+ index)) ?\"))
                     (setq argument (concat argument "\"") index (+ index 2))
                   (setq quoted (not quoted) index (1+ index))))
                (char (setq argument (concat argument (string char)) index (1+ index)))))
            (push argument arguments))))
      (nreverse arguments))))

(defun universel-process-table (&optional platform)
  "Return process observations for PLATFORM, or nil when unsupported."
  (when (universel-process-observation-p platform)
    (cond
     ((eq (plist-get (universel-environment platform) :transport) 'wsl)
      (universel--wsl-process-records (universel-environment platform)))
     ((universel-platform-p 'windows (universel-environment platform))
      (universel--windows-process-records))
     (t
      (let (entries)
        (dolist (path (directory-files "/proc" t "\\`[0-9]+\\'"))
          (when-let* ((entry (universel--process-entry (string-to-number (file-name-nondirectory path)))))
            (push entry entries)))
        entries)))))

(defun universel-process-runtime (entry &optional platform)
  "Return ENTRY's running time on PLATFORM."
  (unless (universel-process-observation-p platform)
    (error "Process observation is unavailable on this platform"))
  (cond
   ((plist-get entry :runtime))
   ((plist-get entry :start-time) (max 0 (- (float-time) (plist-get entry :start-time))))
   (t
    (let ((uptime (string-to-number (car (split-string (universel--read-proc "/proc/uptime"))))))
      (max 0 (- uptime (/ (float (plist-get entry :start-ticks)) universel-clock-ticks-per-second)))))))

(defun universel-shell-launch (shell directory &optional argv variables shell-variables platform)
  "Return a launch plist on PLATFORM that starts SHELL in DIRECTORY.
SHELL is a plist with `:executable' and `:login'.  When ARGV is non-nil, the
shell runs it with the NAME=VALUE VARIABLES and then stays open.  The shell
itself starts with SHELL-VARIABLES; Windows shells receive none.  PowerShell
cannot give one command its own variables, so on Windows VARIABLES are set in
the shell, where they were originally set too.  A WSL launch goes through
wsl.exe; DIRECTORY is an Emacs path."
  (let* ((environment (universel-environment platform))
         (executable (plist-get shell :executable))
         (login (plist-get shell :login)))
    (if (universel-platform-p 'windows environment)
        (let ((quote (lambda (argument) (universel-quote-argument argument 'windows))))
          (list :program executable :directory directory
                :arguments
                (append login
                        (when argv
                          (list "-NoLogo" "-NoExit" "-Command"
                                (concat
                                 (mapconcat
                                  (lambda (variable)
                                    (let ((split (string-search "=" variable)))
                                      (format "[Environment]::SetEnvironmentVariable(%s, %s); "
                                              (funcall quote (substring variable 0 split))
                                              (funcall quote (substring variable (1+ split))))))
                                  variables "")
                                 "& " (mapconcat quote argv " ")))))))
      (let* ((quote (lambda (argument) (universel-quote-argument argument 'posix)))
             (arguments
              (append login
                      (when argv
                        (list "-i" "-c"
                              (concat (mapconcat quote (append (when variables (cons "env" variables))
                                                               argv)
                                                 " ")
                                      "; exec "
                                      (mapconcat quote (cons executable login) " ")))))))
        (if (eq (plist-get environment :transport) 'wsl)
            (universel-command executable arguments
                               (universel-file-path directory environment)
                               environment shell-variables)
          (list :program (if shell-variables "env" executable)
                :arguments (if shell-variables
                               (append shell-variables (cons executable arguments))
                             arguments)
                :directory directory))))))

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

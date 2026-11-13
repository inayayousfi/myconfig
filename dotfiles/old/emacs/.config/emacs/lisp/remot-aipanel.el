;;; remot-aipanel.el --- Remot skill for AIPanel agents -*- lexical-binding: t; -*-

;;; Commentary:

;; Tells coding agents how to reach this Emacs through Remot's server: from
;; this computer, and on Windows also from WSL through the Windows program.

;;; Code:

(require 'remot)
(require 'aipan)

(defun remot-aipanel-native-path (file)
  "Return FILE with this system's directory separators."
  (if (eq system-type 'windows-nt) (subst-char-in-string ?/ ?\\ file) file))

(defun remot-aipanel-server-argument ()
  "Return the emacsclient argument that reaches Remot's server."
  (if server-use-tcp
      (format "--server-file=%s"
              (remot-aipanel-native-path
               (expand-file-name remot-server-name server-auth-dir)))
    (format "--socket-name=%s" remot-server-name)))

(defun remot-aipanel-skill (environment)
  "Return the remote-emacs skill for ENVIRONMENT, or nil when unreachable."
  (when (and (process-live-p server-process)
             (equal server-name remot-server-name))
    (let* ((windows (eq system-type 'windows-nt))
           (client (remot-aipanel-native-path
                    (expand-file-name (if windows "emacsclient.exe" "emacsclient")
                                      invocation-directory)))
           (location (plist-get environment :location))
           (program (pcase location
                      ('host (format "'%s'" client))
                      ((and 'wsl (guard windows))
                       (format "\"$(wslpath -u '%s')\"" client)))))
      (when program
        (aipanel-skill-folder
         "skills/remote-emacs"
         `(("command" . ,(format "%s '%s'" program (remot-aipanel-server-argument)))
           ("environment-note"
            . ,(cond ((eq location 'wsl)
                      (concat "That Emacs runs on Windows, outside this WSL distribution. "
                              "Call its Windows `emacsclient.exe` from here. File paths you "
                              "pass must be Windows paths: convert them with `wslpath -w FILE`."))
                     (windows "In PowerShell, put `& ` before the command prefix.")
                     (t "")))))))))

(defun remot-aipanel-setup ()
  "Register the Remot skill and install skills once startup has finished."
  (aipanel-add-skill "remote-emacs" #'remot-aipanel-skill)
  (run-with-idle-timer 1 nil #'aipanel-install-skills))

(provide 'remot-aipanel)
;;; remot-aipanel.el ends here

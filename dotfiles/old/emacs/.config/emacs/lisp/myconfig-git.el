;;; myconfig-git.el --- Git control and native side-by-side reviews -*- lexical-binding: t; -*-

(require 'magit)
(require 'transient)
(require 'diff)
(require 'jum)
(require 'myconfig-core)
(require 'filenotify)

(declare-function diff-hl-dired-update "diff-hl-dired")
(declare-function ediff-setup-windows-plain "ediff-wind")
(defvar ediff-window-setup-function)
(defvar ediff-split-window-function)


(declare-function myconfig-emacs-diff "myconfig-git")
(unless (fboundp 'myconfig-emacs-diff)
  (defalias 'myconfig-emacs-diff (symbol-function 'diff)))

(defun myconfig-git-root ()
  (or (magit-toplevel) (user-error "Not inside a Git repository")))

(defun myconfig-git-lines (root &rest arguments)
  (let ((default-directory root))
    (with-temp-buffer
      (let ((status (apply #'process-file "git" nil t nil arguments)))
        (unless (zerop status)
          (user-error "%s" (string-trim (buffer-string))))
        (split-string (buffer-string) "\n" t)))))

(defun myconfig-git-text (root &rest arguments)
  (let ((default-directory root))
    (with-temp-buffer
      (let ((status (apply #'process-file "git" nil (list t nil) nil arguments)))
        (if (zerop status) (buffer-string) "")))))

(defun myconfig-git-first-line-quiet (root &rest arguments)
  (let ((default-directory root))
    (with-temp-buffer
      (when (zerop (apply #'process-file "git" nil t nil arguments))
        (car (split-string (buffer-string) "\n" t))))))

(defun myconfig-review-entries (root files left-revision right-revision)
  (mapcar
   (lambda (path)
     (let ((right-file (and (null right-revision) (expand-file-name path root))))
       (list path
             (if left-revision
                 (myconfig-git-text root "show" (format "%s:%s" left-revision path))
               "")
             (cond
              (right-revision
               (myconfig-git-text root "show" (format "%s:%s" right-revision path)))
               ((file-regular-p right-file)
                (with-temp-buffer (insert-file-contents right-file) (buffer-string)))
               ((file-directory-p right-file)
                (let ((status (myconfig-git-text root "submodule" "status" "--" path)))
                  (if (string-empty-p status)
                      (format "[Directory: %s]\n" path)
                    status)))
               (t ""))
             (expand-file-name path root))))
   files))

(defun myconfig-changed-files (root &rest arguments)
  (let ((files (delete-dups (apply #'myconfig-git-lines root arguments))))
    (unless files (user-error "No changed files"))
    files))

(defun myconfig-git-working-entries (root)
  (let* ((tracked (myconfig-git-lines root "diff" "--name-only" "HEAD" "--"))
         (untracked (myconfig-git-lines root "ls-files" "--others" "--exclude-standard"))
         (files (delete-dups (append tracked untracked))))
    (unless files (user-error "No changed files"))
    (myconfig-review-entries root files "HEAD" nil)))

(defun myconfig-git-review-working ()
  (interactive)
  (let ((root (myconfig-git-root)))
    (jumel-show "working" (myconfig-git-working-entries root)
                (lambda () (myconfig-git-working-entries root)))))

(defun myconfig-git-commit-entries (root)
  (let* ((revision (or (myconfig-git-first-line-quiet root "rev-parse" "--verify" "HEAD")
                       (user-error "The repository has no current commit")))
         (parent (myconfig-git-first-line-quiet root "rev-parse" "--verify" "HEAD^"))
         (files (myconfig-changed-files
                 root "diff-tree" "--root" "--no-commit-id" "--name-only" "-r" revision)))
    (myconfig-review-entries root files parent revision)))

(defun diff-current-commit ()
  (interactive)
  (let ((root (myconfig-git-root)))
    (jumel-show "commit" (myconfig-git-commit-entries root)
                (lambda () (myconfig-git-commit-entries root)))))

(defun myconfig-default-branch (root)
  (or (myconfig-git-first-line-quiet
       root "symbolic-ref" "--quiet" "--short" "refs/remotes/origin/HEAD")
      (cl-find-if (lambda (branch)
                    (zerop (let ((default-directory root))
                             (process-file "git" nil nil nil "show-ref" "--verify" "--quiet"
                                           (format "refs/heads/%s" branch)))))
                  '("main" "master" "dev"))
      (user-error "Could not determine the default branch")))

(defun myconfig-git-branch-fork (root)
  (let ((base (myconfig-default-branch root)))
    (or (myconfig-git-first-line-quiet root "merge-base" "--fork-point" base "HEAD")
        (myconfig-git-first-line-quiet root "merge-base" base "HEAD")
        (user-error "Could not compute a fork point or merge base from %s" base))))

(defun myconfig-git-branch-entries (root fork)
  (let* ((tracked (myconfig-git-lines root "diff" "--name-only" fork "--"))
         (untracked (myconfig-git-lines root "ls-files" "--others" "--exclude-standard"))
         (files (delete-dups (append tracked untracked))))
    (unless files (user-error "No changed files"))
    (myconfig-review-entries root files fork nil)))

(defun diff-branch ()
  (interactive)
  (let* ((root (myconfig-git-root))
         (fork (myconfig-git-branch-fork root)))
    (jumel-show "branch" (myconfig-git-branch-entries root fork)
                (lambda ()
                  (myconfig-git-branch-entries root (myconfig-git-branch-fork root))))))

(transient-define-prefix myconfig-diff-menu ()
  "Choose the Git range to review."
  [["Diff"
    ("w" "Working tree" myconfig-git-review-working)
    ("c" "Current commit" diff-current-commit)
    ("b" "Branch" diff-branch)]])

(defun diff (&optional old new switches no-async)
  (interactive)
  (if (called-interactively-p 'interactive)
      (myconfig-diff-menu)
    (funcall #'myconfig-emacs-diff old new switches no-async)))

(defun myconfig-git-quit-buffer (&optional _kill-buffer)
  "Kill the current Magit buffer, or every Magit buffer of the repository
from status."
  (let ((others (and (derived-mode-p 'magit-status-mode)
                     (delq (current-buffer) (magit-mode-get-buffers)))))
    (magit-mode-quit-window t)
    (mapc #'kill-buffer others)))

;;; Git marks in Dired

;; diff-hl redraws a Dired buffer's Git marks only when Dired re-reads it, and
;; Dired re-reads only when the folder itself changes.  Staging, committing or
;; editing an existing file changes no folder, so the marks went stale.

(defvar myconfig-git-dired-watches nil
  "Watched repositories, as (GIT-DIRECTORY TOPLEVEL DESCRIPTOR TIMER).")
(defvar-local myconfig-git-dired-checked-directory nil
  "The folder whose repository this Dired buffer last looked up.")

(defun myconfig-git-dired-buffers (toplevel)
  "Return the Dired buffers with Git marks that list a folder inside TOPLEVEL."
  (seq-filter (lambda (buffer)
                (with-current-buffer buffer
                  (and (bound-and-true-p diff-hl-dired-mode)
                       (file-in-directory-p default-directory toplevel))))
              (buffer-list)))

(defun myconfig-git-dired-refresh (git-directory)
  "Redraw the Git marks of GIT-DIRECTORY's Dired buffers.
Stop watching the repository once none of its Dired buffers remains."
  (when-let* ((watch (assoc git-directory myconfig-git-dired-watches)))
    (setf (nth 3 watch) nil)
    (if-let* ((buffers (myconfig-git-dired-buffers (nth 1 watch))))
        (dolist (buffer buffers)
          (with-current-buffer buffer (diff-hl-dired-update)))
      (file-notify-rm-watch (nth 2 watch))
      (setq myconfig-git-dired-watches (delq watch myconfig-git-dired-watches)))))

(defun myconfig-git-dired-schedule (watch)
  "Refresh WATCH's repository once its burst of changes settles.
One Git command writes several files, and each would otherwise refresh."
  (when (timerp (nth 3 watch)) (cancel-timer (nth 3 watch)))
  (setf (nth 3 watch)
        (run-at-time 0.3 nil #'myconfig-git-dired-refresh (car watch))))

(defun myconfig-git-dired-watch-repository ()
  "Watch the Git folder of the repository the current Dired buffer lists.
Any Git command, from Emacs or any other program, writes there."
  (when (and (bound-and-true-p diff-hl-dired-mode)
             (not (file-remote-p default-directory))
             (not (equal default-directory myconfig-git-dired-checked-directory)))
    (setq myconfig-git-dired-checked-directory default-directory)
    (pcase-let ((`(,git-directory ,toplevel)
                 (split-string (myconfig-git-text default-directory "rev-parse"
                                                  "--absolute-git-dir" "--show-toplevel")
                               "\n" t)))
      (when (and toplevel (not (assoc git-directory myconfig-git-dired-watches)))
        (let ((watch (list git-directory (file-name-as-directory toplevel) nil nil)))
          (when-let* ((descriptor
                       (ignore-errors
                         (file-notify-add-watch
                          git-directory '(change)
                          (lambda (event)
                            ;; Git takes a lock file even to read, as the
                            ;; refresh's own status does.  Only a write, which
                            ;; ends on the real file, changes what is shown.
                            (unless (string-suffix-p ".lock" (or (nth 3 event) (nth 2 event)))
                              (myconfig-git-dired-schedule watch)))))))
            (setf (nth 2 watch) descriptor)
            (push watch myconfig-git-dired-watches)))))))

(defun myconfig-git-dired-file-saved ()
  "Refresh the Git marks of the repository holding the file just saved.
Saving an existing file changes neither its folder nor the Git folder."
  (when-let* ((file buffer-file-name)
              (watch (seq-find (lambda (watch) (file-in-directory-p file (nth 1 watch)))
                               myconfig-git-dired-watches)))
    (myconfig-git-dired-schedule watch)))

(defun myconfig-git-setup ()
  (add-hook 'dired-after-readin-hook #'myconfig-git-dired-watch-repository)
  (add-hook 'after-save-hook #'myconfig-git-dired-file-saved)
  (setq magit-display-buffer-function #'magit-display-buffer-same-window-except-diff-v1
        magit-bury-buffer-function #'myconfig-git-quit-buffer
        magit-save-repository-buffers nil
        ediff-window-setup-function #'ediff-setup-windows-plain
        ediff-split-window-function #'split-window-horizontally))

(provide 'myconfig-git)
;;; myconfig-git.el ends here

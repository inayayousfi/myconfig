;;; myconfig-git.el --- Git control and native side-by-side reviews -*- lexical-binding: t; -*-

(require 'magit)
(require 'transient)
(require 'diff)
(require 'ediff)
(require 'myconfig-core)

(defvar-local myconfig-review-peer nil)

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

(defun myconfig-fontified-source (path text)
  (with-temp-buffer
    (insert text)
    (setq-local buffer-file-name path)
    (set-auto-mode)
    (font-lock-ensure)
    (buffer-substring (point-min) (point-max))))

(defun myconfig-review-buffer (name entries side)
  (let ((buffer (get-buffer-create name)))
    (with-current-buffer buffer
      (let ((inhibit-read-only t))
        (fundamental-mode)
        (font-lock-mode -1)
        (erase-buffer)
        (dolist (entry entries)
          (let ((path (nth 0 entry))
                (text (nth side entry)))
            (insert (propertize (format "\n===== %s =====\n" path)
                                'face 'font-lock-keyword-face))
            (insert (myconfig-fontified-source path text))
            (unless (bolp) (insert "\n"))))
        (setq-local truncate-lines nil
                    buffer-read-only t)
        (goto-char (point-min))))
    buffer))

(defun myconfig-review-cleanup (buffers window-configuration)
  (dolist (buffer buffers)
    (when (buffer-live-p buffer)
      (kill-buffer buffer)))
  (when (window-configuration-p window-configuration)
    (set-window-configuration window-configuration)))

(defun myconfig-show-review (title entries)
  (let* ((window-configuration (current-window-configuration))
         (left (myconfig-review-buffer (format "*%s:before*" title) entries 1))
         (right (myconfig-review-buffer (format "*%s:after*" title) entries 2)))
    (with-current-buffer left (setq myconfig-review-peer right))
    (with-current-buffer right (setq myconfig-review-peer left))
    (let ((ediff-window-setup-function #'ediff-setup-windows-plain)
          (ediff-split-window-function #'split-window-horizontally)
          (ediff-keep-variants t))
      (ediff-buffers
       left right
       (list
        (lambda ()
          (add-hook
           'ediff-after-quit-hook-internal
           (lambda ()
             (myconfig-review-cleanup (list left right) window-configuration))
           nil t)))))
    (message "%s: %d changed file%s" title (length entries)
             (if (= (length entries) 1) "" "s"))))

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
               (t "")))))
   files))

(defun myconfig-changed-files (root &rest arguments)
  (let ((files (delete-dups (apply #'myconfig-git-lines root arguments))))
    (unless files (user-error "No changed files"))
    files))

(defun myconfig-git-review-working ()
  (interactive)
  (let* ((root (myconfig-git-root))
          (tracked (myconfig-git-lines root "diff" "--name-only" "HEAD" "--"))
          (untracked (myconfig-git-lines root "ls-files" "--others" "--exclude-standard"))
          (files (delete-dups (append tracked untracked))))
    (unless files (user-error "No changed files"))
    (myconfig-show-review "Working diff"
                          (myconfig-review-entries root files "HEAD" nil))))

(defun diff-current-commit ()
  (interactive)
  (let* ((root (myconfig-git-root))
         (revision (or (myconfig-git-first-line-quiet root "rev-parse" "--verify" "HEAD")
                       (user-error "The repository has no current commit")))
         (parent (myconfig-git-first-line-quiet root "rev-parse" "--verify" "HEAD^"))
         (files (myconfig-changed-files
                 root "diff-tree" "--root" "--no-commit-id" "--name-only" "-r" revision)))
    (myconfig-show-review "Current commit"
                          (myconfig-review-entries root files parent revision))))

(defun myconfig-default-branch (root)
  (or (myconfig-git-first-line-quiet
       root "symbolic-ref" "--quiet" "--short" "refs/remotes/origin/HEAD")
      (cl-find-if (lambda (branch)
                    (zerop (let ((default-directory root))
                             (process-file "git" nil nil nil "show-ref" "--verify" "--quiet"
                                           (format "refs/heads/%s" branch)))))
                  '("main" "master" "dev"))
      (user-error "Could not determine the default branch")))

(defun diff-branch ()
  (interactive)
  (let* ((root (myconfig-git-root))
         (base (myconfig-default-branch root))
          (fork (or (myconfig-git-first-line-quiet root "merge-base" "--fork-point" base "HEAD")
                    (myconfig-git-first-line-quiet root "merge-base" base "HEAD")
                    (user-error "Could not compute a fork point or merge base from %s" base)))
          (tracked (myconfig-git-lines root "diff" "--name-only" fork "--"))
          (untracked (myconfig-git-lines root "ls-files" "--others" "--exclude-standard"))
          (files (delete-dups (append tracked untracked))))
    (unless files (user-error "No changed files"))
    (myconfig-show-review (format "Branch diff from %s" fork)
                          (myconfig-review-entries root files fork nil))))

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

(defun myconfig-git-setup ()
  (setq magit-display-buffer-function #'magit-display-buffer-same-window-except-diff-v1
        magit-save-repository-buffers nil
        ediff-window-setup-function #'ediff-setup-windows-plain
        ediff-split-window-function #'split-window-horizontally)
  (evil-set-initial-state 'magit-status-mode 'normal)
  (evil-set-initial-state 'magit-log-mode 'normal)
  (evil-set-initial-state 'magit-diff-mode 'normal))

(provide 'myconfig-git)
;;; myconfig-git.el ends here

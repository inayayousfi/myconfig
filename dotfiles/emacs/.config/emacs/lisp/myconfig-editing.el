;;; myconfig-editing.el --- Editing and discovery -*- lexical-binding: t; -*-

(require 'use-package)
(require 'univers)

(defvar-local myconfig-save-timer nil)
(defvar-local myconfig-eglot-warning-shown nil)
(defvar myconfig-auto-format-save t)
(defvar myconfig-yank-highlight-active nil)

(defun myconfig-buffer-stale-p (&optional _noconfirm)
  "Return non-nil when the visited file changed on disk.

Unlike Emacs' default stale check, this deliberately ignores whether the
buffer has unsaved edits.  Auto-Revert can therefore replace those edits with
the current file contents on disk."
  (and buffer-file-name
       (file-readable-p buffer-file-name)
       (not (verify-visited-file-modtime (current-buffer)))))

(defun myconfig-highlight-yank (original-function &rest arguments)
  "Briefly highlight text inserted by a yank or paste command."
  (if myconfig-yank-highlight-active
      (apply original-function arguments)
    (let ((start (point)) result)
      (let ((myconfig-yank-highlight-active t))
        (setq result (apply original-function arguments)))
      (when (> (point) start)
        (pulse-momentary-highlight-region start (point))
        (redisplay t))
      result)))

(defun myconfig-never-offer-temporary-buffer-for-saving ()
  "Keep non-file buffers out of Emacs' save-on-exit questions.

Some packages make their temporary buffers offer-save buffers themselves,
so the default value alone is not sufficient."
  (when (and (not buffer-file-name) (not (minibufferp)))
    (setq-local buffer-offer-save nil)))

(defun myconfig-buffer-save-eligible-p ()
  (and buffer-file-name
       (buffer-modified-p)
       (not buffer-read-only)
       (not (and (bound-and-true-p evil-local-mode)
                 (eq (bound-and-true-p evil-state) 'insert)))))

(defun myconfig-eglot-server-command ()
  "Return the configured Eglot server executable for the current buffer."
  (condition-case nil
      (let* ((contact (nth 3 (eglot--guess-contact)))
             (program (and (listp contact) (stringp (car contact)) (car contact))))
        (and program (list program (executable-find program))))
    (error nil)))

(defun myconfig-eglot-server-available-p ()
  (when-let* ((server (myconfig-eglot-server-command)))
    (executable-find (car server))))

(defun myconfig-warn-missing-eglot-server ()
  (unless myconfig-eglot-warning-shown
    (setq myconfig-eglot-warning-shown t)
    (let* ((server (myconfig-eglot-server-command))
           (program (car server)))
      (display-warning
       'myconfig
       (format "No language server found for %s%s. Install it with M-x mason."
               major-mode
               (if program (format " (expected command: %s)" program) ""))
       :warning))))

(defun myconfig-eglot-ensure-if-server-available ()
  (when buffer-file-name
    (if (myconfig-eglot-server-available-p)
        (eglot-ensure)
      (myconfig-warn-missing-eglot-server))))

(defun myconfig-save-all-file-buffers ()
  (dolist (buffer (buffer-list))
    (with-current-buffer buffer
      (when (myconfig-buffer-save-eligible-p)
        (condition-case error
            (save-buffer)
          (error (myconfig-log "Save failed for %s: %s" buffer-file-name error)))))))

(defun myconfig-never-save-some-buffers (&optional _argument _predicate)
  "Never save buffers through the multi-buffer save command."
  nil)

(defun myconfig-never-save-before-kill (_buffer)
  "Kill a modified buffer without saving or asking."
  t)

(defun myconfig-never-save-on-exit (original-function &optional argument restart)
  "Discard modified buffers before exiting without asking or saving."
  (dolist (buffer (buffer-list))
    (with-current-buffer buffer
      (when (buffer-modified-p)
        (set-buffer-modified-p nil))))
  (let ((confirm-kill-emacs nil)
        (confirm-kill-processes nil))
    (funcall original-function argument restart)))

(defun myconfig-format-and-save-buffer (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (setq myconfig-save-timer nil)
      (when (and myconfig-auto-format-save
                 (myconfig-buffer-save-eligible-p))
        (condition-case error
            (cond
             ((and (bound-and-true-p eglot--managed-mode)
                   (eglot-current-server))
              (eglot-format-buffer)
              (myconfig-save-all-file-buffers))
             ((and (fboundp 'apheleia-format-buffer)
                   (or (bound-and-true-p apheleia-formatter)
                       (alist-get major-mode apheleia-mode-alist)))
              (apheleia-format-buffer
               (or (bound-and-true-p apheleia-formatter)
                   (alist-get major-mode apheleia-mode-alist))
               #'myconfig-save-all-file-buffers))
             (t (myconfig-save-all-file-buffers)))
          (error (myconfig-log "Format/save failed for %s: %s" buffer-file-name error)))))))

(defun myconfig-schedule-format-save (&rest _)
  (when myconfig-save-timer
    (cancel-timer myconfig-save-timer))
  (setq myconfig-save-timer
        (run-with-idle-timer 0.5 nil #'myconfig-format-and-save-buffer (current-buffer))))

(defun myconfig-save-after-evil-insert ()
  (when (and myconfig-auto-format-save buffer-file-name (buffer-modified-p))
    (myconfig-schedule-format-save)))

(defun myconfig-toggle-auto-format-save ()
  (interactive)
  (setq myconfig-auto-format-save (not myconfig-auto-format-save))
  (message "Automatic format and save %s" (if myconfig-auto-format-save "enabled" "disabled")))

(defun myconfig-compile ()
  (interactive)
  (compile (read-shell-command "Compile command: " compile-command)))

(defun myconfig-project-file-candidates (root)
  (if-let* ((project (project-current nil root)))
      (project-files project)
    (directory-files-recursively root directory-files-no-dot-files-regexp)))

(defun myconfig-search-grep-source (root)
  (let* ((ripgrep (executable-find "rg"))
         (make-builder (funcall (if ripgrep
                                   #'consult--ripgrep-make-builder
                                 #'consult--grep-make-builder)
                                (list root))))
    (list :name (if ripgrep "Text (rg)" "Text (grep)")
          :narrow ?t
          :category 'consult-grep
          :async (consult--process-collection
                  make-builder
                  :transform (consult--grep-format make-builder)
                  :file-handler t)
          :state #'consult--grep-state
          :action (lambda (candidate)
                    (consult--jump (consult--grep-position candidate))))))

(defun myconfig-search ()
  (interactive)
  (let* ((root (myconfig-project-root))
         (_grep (or (executable-find "rg") (executable-find "grep")
                    (user-error "Neither rg nor grep is installed")))
         (selected
          (let ((default-directory root))
            (consult--multi
             (list
               (list :name "Files" :narrow ?f :category 'file
                     :items (lambda () (myconfig-project-file-candidates root))
                     :action (lambda (file) (atelier-open-file file)))
              (myconfig-search-grep-source root))
             :prompt "Search: "
             :require-match t
             :sort nil))))
    (unless selected
      (user-error "No search result"))))

(defun myconfig-update-file-wrap-margin (window)
  "Use four fifths of WINDOW's available width for a visited file."
  (let* ((margins (window-margins window))
         (left (car margins))
         (right (cdr margins)))
    (if (window-parameter window 'myconfig-file-wrap-margin)
        (unless (buffer-file-name (window-buffer window))
          (set-window-margins window left nil)
          (set-window-parameter window 'myconfig-file-wrap-margin nil))
      (when (buffer-file-name (window-buffer window))
        (set-window-parameter window 'myconfig-file-wrap-margin t)))
    (when (and (window-parameter window 'myconfig-file-wrap-margin)
               (not (window-minibuffer-p window)))
      ;; Include the current right margin so repeated updates do not shrink
      ;; the text area each time a window changes size.
      (let ((target (/ (+ (window-width window) (or right 0)) 5)))
        (unless (equal right (and (> target 0) target))
          (set-window-margins window left (and (> target 0) target)))))))

(defun myconfig-update-file-wrap-margins (frame)
  "Update visited-file windows on FRAME after a size or buffer change."
  (walk-windows #'myconfig-update-file-wrap-margin nil frame))

(defun myconfig-editing-setup ()
  (setq-default indent-tabs-mode nil
                tab-width 2
                standard-indent 2
                truncate-lines nil)
  (add-hook 'after-change-major-mode-hook
            #'myconfig-never-offer-temporary-buffer-for-saving)
  (add-hook 'window-size-change-functions #'myconfig-update-file-wrap-margins)
  (dolist (frame (frame-list))
    (myconfig-update-file-wrap-margins frame))
  (setq display-line-numbers-type 'relative
        select-enable-clipboard t
        kill-do-not-save-duplicates t
        compile-command ""
        completion-cycle-threshold 3
        read-extended-command-predicate #'command-completion-default-include-p)
  (require 'pulse)
  (advice-add 'yank :around #'myconfig-highlight-yank)
  (advice-add 'myconfig-paste :around #'myconfig-highlight-yank)
  (advice-add 'save-some-buffers :override #'myconfig-never-save-some-buffers)
  (when (fboundp 'kill-buffer--possibly-save)
    (advice-add 'kill-buffer--possibly-save :override #'myconfig-never-save-before-kill))
  (advice-add 'save-buffers-kill-emacs :around #'myconfig-never-save-on-exit)
  (global-display-line-numbers-mode 1)
  (setq global-auto-revert-non-file-buffers t
        auto-revert-avoid-polling nil
        auto-revert-check-vc-info t)
  (setq-default buffer-stale-function #'myconfig-buffer-stale-p)
  (global-auto-revert-mode 1)
  (add-hook 'after-change-functions #'myconfig-schedule-format-save)

  (use-package evil
    :init (setq evil-want-minibuffer t
                evil-want-C-u-scroll t
                evil-want-C-u-delete t)
    :config
    (setq evil-want-keybinding nil
          evil-want-integration t
          evil-undo-system 'undo-redo
          evil-search-module 'isearch)
    (evil-mode 1))
  (use-package evil-collection
    :after evil
    :init (setq evil-collection-setup-minibuffer t)
    :config (evil-collection-init))
  (add-hook 'evil-insert-state-exit-hook #'myconfig-save-after-evil-insert)
  (use-package vertico :config (vertico-mode 1))
  (use-package orderless
    :config (setq completion-styles '(orderless basic)
                  completion-category-defaults nil
                  completion-category-overrides '((file (styles partial-completion)))))
  (use-package marginalia :config (marginalia-mode 1))
  (use-package consult
    :config (setq consult-preview-key 'any
                  consult-buffer-list-function #'atelier-buffer-list
                  consult-ripgrep-args
                  "rg --null --line-buffered --color=never --max-columns=1000 --path-separator / --smart-case --hidden --glob=!.git/* --glob=!.svn/* --glob=!.hg/* --glob=!node_modules/* --no-heading --line-number ."))
  (use-package corfu
    :config
    (setq corfu-auto t corfu-auto-delay 0.1 corfu-cycle t corfu-preselect 'prompt)
    (global-corfu-mode 1))
  (use-package cape
    :config
    (add-to-list 'completion-at-point-functions #'cape-file)
    (add-to-list 'completion-at-point-functions #'cape-dabbrev))
  (use-package yasnippet :config (yas-global-mode 1))
  (use-package yasnippet-capf
    :after yasnippet
    :config (add-to-list 'completion-at-point-functions #'yasnippet-capf))
  (use-package avy)
  (use-package apheleia)
  ;; Windows users get the pre-built grammar bundle first.  It avoids the
  ;; compiler requirement for the common languages; treesit-auto remains the
  ;; fallback for languages which are not in the bundle.
  (use-package treesit-langs
    :if (universel-platform-p 'windows (universel-host-platform))
    :demand t)
  (use-package treesit-auto
    :custom (treesit-auto-install 'prompt)
    :config
    ;; Native Windows Emacs does not always provide a `cc' command.  Prefer
    ;; GCC when it is available and otherwise use LLVM's clang, which is part
    ;; of the optional Windows DevTools package group.
    (when-let* ((compilers (universel-grammar-compilers)))
      (dolist (recipe treesit-auto-recipe-list)
        (setf (treesit-auto-recipe-cc recipe) (car compilers))
        (when (cdr compilers)
          (setf (treesit-auto-recipe-c++ recipe) (cdr compilers)))))
    (treesit-auto-add-to-auto-mode-alist 'all)
    (global-treesit-auto-mode 1))
  (use-package mason :demand t)
  (use-package dape)
  (use-package diff-hl
    :config
    (global-diff-hl-mode 1)
    (diff-hl-flydiff-mode 1)
    (add-hook 'magit-post-refresh-hook #'diff-hl-magit-post-refresh))
  (use-package blamer
    :config
    (setq blamer-idle-time 0.05
          blamer-min-offset 30
          blamer-author-formatter "  %s"
          blamer-datetime-formatter "[%s]"
          blamer-commit-formatter " %s")
    (global-blamer-mode 1))
  (use-package flyover
    :hook (flymake-mode . flyover-mode)
    :custom
    (flyover-checkers '(flymake))
    (flyover-levels '(error warning info))
    (flyover-show-at-eol t)
    (flyover-show-virtual-line nil)
    (flyover-display-mode 'always)
    (flyover-hide-checker-name t)
    (flyover-border-style 'none)
    (flyover-error-icon "E")
    (flyover-warning-icon "W")
    (flyover-info-icon "I")
    (flyover-wrap-messages nil)
    (flyover-hide-during-completion t)
    (flyover-debounce-interval 0.2))
  (use-package eldoc-box :demand t)

  (require 'eldoc)
  (setq-default eldoc-display-functions '(eldoc-display-in-buffer))
  (require 'eglot)
  (setq eglot-autoshutdown t
        eglot-confirm-server-edits nil)
  (add-hook 'prog-mode-hook #'myconfig-eglot-ensure-if-server-available))

(provide 'myconfig-editing)
;;; myconfig-editing.el ends here

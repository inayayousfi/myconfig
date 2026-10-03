;;; myconfig-editing.el --- Editing and discovery -*- lexical-binding: t; -*-

(require 'use-package)
(require 'univers)

(defvar-local myconfig-save-timer nil)
(defvar-local myconfig-eglot-warning-shown nil)
(defvar myconfig-auto-format-save t)
(defconst myconfig-search-ripgrep-args
  "rg --null --line-buffered --color=never --max-columns=1000 --path-separator / --smart-case --hidden --glob=!.git/* --glob=!.svn/* --glob=!.hg/* --glob=!node_modules/* --no-heading --line-number")

(defun myconfig-buffer-stale-p (&optional _noconfirm)
  "Return non-nil when the visited file changed on disk.

Unlike Emacs' default stale check, this deliberately ignores whether the
buffer has unsaved edits.  Auto-Revert can therefore replace those edits with
the current file contents on disk."
  (and buffer-file-name
       (file-readable-p buffer-file-name)
       (not (verify-visited-file-modtime (current-buffer)))))

(defun myconfig-highlight-copy (beg end &rest _)
  "Briefly highlight the region copied by Evil's yank operator."
  (when (< beg end)
    (pulse-momentary-highlight-region beg end)
    (redisplay t)))

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
        (and program (list program (executable-find program t))))
    (error nil)))

(defun myconfig-eglot-server-available-p ()
  (when-let* ((server (myconfig-eglot-server-command)))
    (cadr server)))

(defun myconfig-warn-missing-eglot-server ()
  (unless myconfig-eglot-warning-shown
    (setq myconfig-eglot-warning-shown t)
    (let* ((server (myconfig-eglot-server-command))
           (program (car server)))
      (display-warning
       'myconfig
       (format "No language server found for %s%s. Install it with M-x mason-manager"
               major-mode
               (if program (format " (expected command: %s)" program) ""))
       :warning))))

(defun myconfig-eglot-ensure-if-server-available ()
  (when (and buffer-file-name
             (not (derived-mode-p 'emacs-lisp-mode 'lisp-mode)))
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

(defun myconfig-flyover-refresh-eob ()
  "Hide end-of-buffer diagnostics only while point is at their anchor."
  (let ((eob (point-max)))
    (dolist (overlay (overlays-in eob (1+ eob)))
      (when-let* ((text (overlay-get overlay 'myconfig-flyover-eob-text)))
        (overlay-put overlay 'before-string (unless (= (point) eob) text))))))

(defun myconfig-flyover-use-margin (overlay &rest _)
  "Show Flyover after its line without placing the cursor after it."
  (when-let* ((text (or (overlay-get overlay 'display)
                        (overlay-get overlay 'after-string)))
              ((stringp text))
              (buffer (overlay-buffer overlay)))
    (with-current-buffer buffer
      (save-excursion
        (goto-char (overlay-start overlay))
        (let* ((eol (line-end-position))
               (message-text (if (string-prefix-p "\n" text)
                                 (substring text 1)
                               text))
               (rendered (concat (propertize " " 'cursor 1) message-text)))
          (overlay-put overlay 'after-string nil)
          (overlay-put overlay 'before-string nil)
          (overlay-put overlay 'display nil)
          (if (< eol (point-max))
              ;; Replacing the newline makes point at EOL fall within the
              ;; display string, so its cursor anchor is actually respected.
              (progn
                (move-overlay overlay eol (1+ eol))
                (overlay-put overlay 'display (concat rendered "\n")))
            ;; At EOB there is no character to replace.  Hide the message
            ;; only at that position rather than drawing the cursor behind it.
            (overlay-put overlay 'evaporate nil)
            (move-overlay overlay eol eol)
            (overlay-put overlay 'myconfig-flyover-eob-text rendered)
            (add-hook 'post-command-hook #'myconfig-flyover-refresh-eob nil t)))))
    (when (overlay-get overlay 'myconfig-flyover-eob-text)
      (myconfig-flyover-refresh-eob))))

(defun myconfig-compile ()
  (interactive)
  (compile (read-shell-command "Compile command: " compile-command)))

(defun myconfig-project-file-candidates (root)
  (if-let* ((project (project-current nil root)))
      (project-files project)
    (directory-files-recursively root directory-files-no-dot-files-regexp)))

(defun myconfig-search-text-candidates (root files input)
  "Search FILES below ROOT for INPUT using only Emacs."
  (require 'consult)
  (when-let* ((query (car (consult--command-split input)))
              ((not (string-empty-p query)))
              (regexp (condition-case nil
                          (let ((parts (car (consult--compile-regexp
                                             query 'emacs nil))))
                            (and parts (consult--join-regexps parts 'emacs)))
                        (invalid-regexp nil))))
    (let (candidates)
      (dolist (file files)
        (let ((path (expand-file-name file root)))
          (when (file-readable-p path)
            (condition-case nil
                (with-temp-buffer
                  (insert-file-contents path)
                  (goto-char (point-min))
                  (while (re-search-forward regexp nil t)
                    (let* ((position (match-beginning 0))
                           (line (line-number-at-pos position))
                           (text (buffer-substring-no-properties
                                  (line-beginning-position) (line-end-position)))
                           (candidate (format "%s:%d:%s"
                                              (file-relative-name path root) line text)))
                      (put-text-property 0 (length candidate) 'myconfig-search-location
                                         (cons path position) candidate)
                      (push candidate candidates))))
              (file-error nil)))))
      (nreverse candidates))))

(defun myconfig-search-text-source (root)
  "Return a live project text source for the combined search in ROOT."
  (if-let* ((program (cond ((executable-find "rg") 'rg)
                           ((executable-find "grep") 'grep)))
            (builder (funcall (if (eq program 'rg)
                                  #'consult--ripgrep-make-builder
                                #'consult--grep-make-builder)
                              (list root))))
      (list :name (if (eq program 'rg) "Text (rg)" "Text (grep)")
            :narrow ?t :category 'consult-grep
            :async (consult--process-collection
                       builder :transform (consult--grep-format builder) :file-handler t)
            :state #'consult--grep-state
            :action (lambda (candidate)
                      (consult--jump (consult--grep-position candidate))))
    (let ((files (myconfig-project-file-candidates root)))
      (list :name "Text (Emacs)" :narrow ?t :category 'consult-grep
            :async (consult--async-dynamic
                    (lambda (input)
                      (myconfig-search-text-candidates root files input)))
            :action (lambda (candidate)
                      (pcase-let ((`(,file . ,position)
                                   (get-text-property 0 'myconfig-search-location candidate)))
                        (atelier-open-file file)
                        (goto-char position)))))))

(defun myconfig-search-combined (root)
  "Search file names and file contents together below ROOT."
  (let ((default-directory root))
    (consult--multi
     (list (myconfig-search-text-source root)
           (list :name "Files" :narrow ?f
                 :items (lambda ()
                          (mapcar (lambda (file) (file-relative-name file root))
                                  (myconfig-project-file-candidates root)))
                 :action (lambda (file)
                           (atelier-open-file (expand-file-name file root)))))
     :prompt "Search: " :require-match t :sort nil)))

(defun myconfig-search-text (root &optional selected-text)
  "Search ROOT for text, starting with SELECTED-TEXT when present."
  (let ((default-directory root)
        (initial (and selected-text
                      (replace-regexp-in-string
                       " " "\\ " (regexp-quote selected-text) t t))))
    (cond
     ((executable-find "rg") (consult-ripgrep root initial))
     ((executable-find "grep") (consult-grep root initial))
     (t
      ;; `project-search' and `rgrep' also need external programs.  Occur
      ;; searches Emacs buffers directly, including files not yet visited.
      (let ((regexp (or initial (read-regexp "Search text: "))))
        (multi-occur
         (mapcar (lambda (file) (find-file-noselect (expand-file-name file root)))
                 (myconfig-project-file-candidates root))
         regexp))))))

(defun myconfig-search ()
  "Search project files and text together; a selection searches text directly."
  (interactive)
  (let* ((root (myconfig-project-root))
         (selected-text (when (use-region-p)
                          (buffer-substring-no-properties
                           (region-beginning) (region-end)))))
    (if selected-text
        (myconfig-search-text root selected-text)
      (myconfig-search-combined root))))

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
  (advice-add 'evil-yank :after #'myconfig-highlight-copy)
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
  (electric-pair-mode 1)

  (use-package evil
    :init (setq evil-want-minibuffer t
                evil-want-C-u-scroll t
                evil-want-C-u-delete t)
    :config
    (setq evil-want-keybinding nil
          evil-want-integration t
          evil-search-module 'isearch)
    (customize-set-variable 'evil-undo-system 'undo-redo)
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
                  consult-ripgrep-args myconfig-search-ripgrep-args))
  (use-package corfu
    :config
    (setq corfu-auto t corfu-auto-delay 0.1 corfu-auto-prefix 1
          corfu-cycle t corfu-preselect 'first)
    (keymap-set corfu-map "TAB" #'corfu-insert)
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
    :demand t
    :hook (dired-mode . diff-hl-dired-mode)
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
    (flyover-debounce-interval 0.2)
    :config
    (advice-add 'flyover--configure-overlay-display :after
                #'myconfig-flyover-use-margin)
    (advice-add 'flyover--configure-overlay :after
                #'myconfig-flyover-use-margin))
  (use-package eldoc-box :demand t)

  (require 'eldoc)
  (setq-default eldoc-display-functions '(eldoc-display-in-buffer))
  (require 'eglot)
  (setq eglot-autoshutdown t
        eglot-confirm-server-edits nil)
  (add-hook 'prog-mode-hook #'myconfig-eglot-ensure-if-server-available))

(provide 'myconfig-editing)
;;; myconfig-editing.el ends here

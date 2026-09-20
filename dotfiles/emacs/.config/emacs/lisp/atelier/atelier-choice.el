;;; atelier-choice.el --- Atelier choice and directory UI -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'dired)
(require 'subr-x)
(require 'atelier-model)

(defvar-keymap atelier-directory-chooser-mode-map
  "RET" #'atelier-directory-chooser-enter
  "<return>" #'atelier-directory-chooser-enter
  "h" #'atelier-directory-chooser-up-directory
  "H" #'atelier-directory-chooser-up-directory
  "^" #'atelier-directory-chooser-up-directory
  "q" #'abort-recursive-edit
  "<mouse-2>" #'atelier-directory-chooser-mouse-enter)

(defun atelier-directory-chooser-up-directory ()
  (interactive)
  (dired-up-directory)
  (atelier-directory-chooser-setup))

(define-minor-mode atelier-directory-chooser-mode
  "Choose a workspace directory from a full Dired buffer."
  :lighter " Choose directory"
  :keymap atelier-directory-chooser-mode-map)

(defun atelier-directory-chooser-select (&optional _event)
  (interactive)
  (setq atelier-directory-choice-result default-directory)
  (exit-recursive-edit))

(defun atelier-directory-chooser-insert ()
  (when atelier-directory-chooser-mode
    (let ((inhibit-read-only t)
          (map (make-sparse-keymap)))
      (goto-char (point-min))
       (unless (text-property-search-forward 'atelier-directory-choice t t)
        (goto-char (point-min))
        (if (dired-goto-file (directory-file-name default-directory))
            (beginning-of-line)
          (forward-line 2))
        (define-key map [mouse-1] #'atelier-directory-chooser-select)
        (define-key map [mouse-2] #'atelier-directory-chooser-select)
        (insert (propertize "[Select this directory]"
                            'atelier-directory-choice t
                            'face 'success
                            'mouse-face 'atelier-navigator-hover
                            'follow-link t
                            'keymap map
                            'rear-nonsticky t)
                "\n")))
    (goto-char (point-min))
    (text-property-search-forward 'atelier-directory-choice t t)
    (beginning-of-line)))

(defun atelier-directory-chooser-setup ()
  (unless atelier-directory-chooser-mode
    (setq-local atelier-directory-chooser-header-was-local
                (local-variable-p 'header-line-format)
                atelier-directory-chooser-original-header header-line-format
                atelier-directory-chooser-original-modified (buffer-modified-p))
    (cl-pushnew (current-buffer) atelier-directory-chooser-buffers))
  (atelier-directory-chooser-mode 1)
  (when (bound-and-true-p evil-local-mode)
    (evil-normalize-keymaps))
  (setq-local header-line-format
              " Enter open/select   h/H/^ parent   + new directory   q cancel")
  (add-hook 'dired-after-readin-hook #'atelier-directory-chooser-insert nil t)
  (atelier-directory-chooser-insert)
  (set-buffer-modified-p atelier-directory-chooser-original-modified))

(defun atelier-directory-chooser-cleanup (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (let ((inhibit-read-only t))
        (goto-char (point-min))
        (while (text-property-search-forward 'atelier-directory-choice t t)
          (delete-region (line-beginning-position)
                         (min (point-max) (1+ (line-end-position))))))
      (remove-hook 'dired-after-readin-hook #'atelier-directory-chooser-insert t)
      (atelier-directory-chooser-mode -1)
      (if atelier-directory-chooser-header-was-local
          (setq-local header-line-format atelier-directory-chooser-original-header)
        (kill-local-variable 'header-line-format))
      (set-buffer-modified-p atelier-directory-chooser-original-modified))))

(defun atelier-directory-chooser-enter ()
  (interactive)
  (let ((path (dired-get-filename nil t)))
    (cond
     ((or (get-text-property (line-beginning-position) 'atelier-directory-choice)
          (null path))
      (atelier-directory-chooser-select))
     ((file-directory-p path)
      (switch-to-buffer (atelier-new-dired-buffer path))
      (atelier-directory-chooser-setup))
     (t
      (user-error "Choose a directory; this is a file: %s" path)))))

(defun atelier-directory-chooser-mouse-enter (event)
  (interactive "e")
  (mouse-set-point event)
  (atelier-directory-chooser-enter))

(defun atelier-read-directory-with-dired (directory)
  (let ((atelier-directory-choice-result nil)
         (atelier-directory-chooser-active t)
        (atelier-directory-chooser-buffers nil)
        (existing-buffers (buffer-list)))
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer (atelier-new-dired-buffer directory))
          (atelier-directory-chooser-setup)
          (recursive-edit))
      (mapc #'atelier-directory-chooser-cleanup
            atelier-directory-chooser-buffers)
      (dolist (buffer atelier-directory-chooser-buffers)
        (when (and (buffer-live-p buffer) (not (memq buffer existing-buffers)))
          (kill-buffer buffer))))
    atelier-directory-choice-result))

(defvar-keymap atelier-choice-mode-map
  :parent special-mode-map
  "j" #'atelier-choice-next
  "k" #'atelier-choice-previous
  "<down>" #'atelier-choice-next
  "<up>" #'atelier-choice-previous
  "RET" #'atelier-choice-select
  "q" #'abort-recursive-edit)

(define-derived-mode atelier-choice-mode special-mode "Atelier choice"
  (atelier-mark-internal-buffer)
  (setq-local header-line-format " j/k move   Enter select   q cancel"
              hl-line-face 'atelier-navigator-current
              cursor-type 'box)
  (hl-line-mode 1)
  (display-line-numbers-mode -1))

(defun atelier-choice-positions ()
  (let ((position (point-min)) positions)
    (while (< position (point-max))
      (when (get-text-property position 'atelier-choice-value)
        (push position positions))
      (setq position (or (next-single-property-change
                          position 'atelier-choice-value nil (point-max))
                         (point-max))))
    (nreverse positions)))

(defun atelier-choice-move (delta)
  (let* ((positions (atelier-choice-positions))
         (next (cl-position-if (lambda (position) (> position (point))) positions))
         (current (max 0 (1- (or next (length positions))))))
    (when positions
      (goto-char (nth (mod (+ current delta) (length positions)) positions)))))

(defun atelier-choice-next ()
  (interactive)
  (atelier-choice-move 1))

(defun atelier-choice-previous ()
  (interactive)
  (atelier-choice-move -1))

(defun atelier-choice-select (&optional event)
  (interactive (list last-input-event))
  (when (mouse-event-p event)
    (mouse-set-point event))
  (if-let* ((value (get-text-property (line-beginning-position) 'atelier-choice-value)))
      (progn
        (setq atelier-choice-result value)
        (exit-recursive-edit))
    (user-error "No choice on this line")))

(defun atelier-read-buffer-choice (title choices)
  (let ((atelier-choice-result nil)
        (buffer (get-buffer-create atelier-choice-buffer)))
    (save-window-excursion
      (switch-to-buffer buffer)
      (atelier-choice-mode)
      (let ((inhibit-read-only t)
            (map (make-sparse-keymap)))
        (erase-buffer)
        (insert title "\n\n")
        (define-key map [mouse-1] #'atelier-choice-select)
        (define-key map [mouse-2] #'atelier-choice-select)
        (dolist (choice choices)
          (insert (propertize (format "%s" choice)
                              'atelier-choice-value choice
                              'mouse-face 'atelier-navigator-hover
                              'follow-link t
                              'keymap map
                              'rear-nonsticky t)
                  "\n"))
        (delete-region (1- (point-max)) (point-max))
        (set-buffer-modified-p nil)
        (goto-char (car (atelier-choice-positions))))
      (recursive-edit))
    atelier-choice-result))

(provide 'atelier-choice)
;;; atelier-choice.el ends here

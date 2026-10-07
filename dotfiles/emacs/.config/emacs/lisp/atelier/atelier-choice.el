;;; atelier-choice.el --- Atelier choice UI -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'subr-x)
(require 'atelier-model)

(declare-function atelier-mark-internal-buffer "atelier")

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

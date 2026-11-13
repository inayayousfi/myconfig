;;; xref-atelier.el --- Code navigation in Atelier file stacks -*- lexical-binding: t; -*-

(require 'atelier)
(require 'xref)

(declare-function eglot-find-implementation "eglot")

(defvar-local atelier-xref-source nil
  "Stable workspace and entry IDs for choosing from an Xref results buffer.")

(defun atelier-xref-stack-result (source buffer)
  "Resolve SOURCE's stable IDs before placing BUFFER in its original view."
  (let* ((workspace (atelier-operation-workspace (car source)))
         (entry (atelier-operation-entry workspace (cdr source))))
    (atelier-push-buffer buffer workspace 'file entry)))

(defun atelier-xref-follow (command)
  "Run Xref COMMAND and stack a visited file in the originating file entry."
  (let* ((workspace (atelier-current-workspace))
          (entry (and workspace
                      (atelier-buffer-registerable-p (current-buffer) workspace)
                      (atelier-show-buffer (current-buffer) workspace)))
           (source (and entry (cons (atelier-workspace-id workspace) (atelier-entry-field entry :id))))
         (result (let ((atelier-inhibit-buffer-ownership (and source t)))
                   (call-interactively command))))
    (when source
      (cond
       ((and (bufferp result)
             (with-current-buffer result (derived-mode-p 'xref--xref-buffer-mode)))
        (with-current-buffer result (setq-local atelier-xref-source source))
        (when (buffer-file-name (current-buffer))
           (atelier-xref-stack-result source (current-buffer))))
       ((buffer-file-name (current-buffer))
         (atelier-xref-stack-result source (current-buffer)))))
    result))

(defun atelier-xref-find-definitions ()
  (interactive)
  (atelier-xref-follow #'xref-find-definitions))

(defun atelier-xref-find-implementation ()
  (interactive)
  (atelier-xref-follow #'eglot-find-implementation))

(defun atelier-xref-select (original &rest arguments)
  "Stack the file chosen from an Atelier-owned Xref results buffer."
  (let* ((source atelier-xref-source)
         (result (let ((atelier-inhibit-buffer-ownership (and source t)))
                   (apply original arguments))))
    (when (and source (buffer-file-name (current-buffer)))
       (atelier-xref-stack-result source (current-buffer)))
    result))

(defun atelier-xref-preview (original &rest arguments)
  "Do not register a preview from an Atelier-owned Xref results buffer."
  (let ((atelier-inhibit-buffer-ownership
         (or (and (derived-mode-p 'xref--xref-buffer-mode) atelier-xref-source)
             atelier-inhibit-buffer-ownership)))
    (apply original arguments)))

(defun xref-atelier-setup ()
  (dolist (target '(xref-goto-xref xref--next-error-function))
    (unless (advice-member-p #'atelier-xref-select target)
      (advice-add target :around #'atelier-xref-select)))
  (unless (advice-member-p #'atelier-xref-preview 'xref--show-pos-in-buf)
    (advice-add 'xref--show-pos-in-buf :around #'atelier-xref-preview)))

(provide 'xref-atelier)
;;; xref-atelier.el ends here

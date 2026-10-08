;;; atelier-naming.el --- Workspace-qualified buffer names -*- lexical-binding: t; -*-

;; Every buffer has a workspace: the one whose stack holds it, the one an
;; integration attaches it to, or the reserved Detached workspace.  Buffers
;; are named "WORKSPACE | TYPE | NAME".  Names wrapped in stars belong to
;; buffers that Emacs and packages find by exact name, so those keep their
;; names unless Atelier tracks the buffer itself.

(require 'cl-lib)
(require 'subr-x)
(require 'atelier-model)
(require 'atelier-operation)

(declare-function atelier-buffer-ownable-p "atelier")
(declare-function atelier-buffer-registerable-p "atelier")
(declare-function atelier-register-buffer "atelier")

(defconst atelier-buffer-name-separator " | ")
(defvar atelier-buffer-owner-functions nil
  "Functions called with a buffer that no workspace stack holds.
The first non-nil result, (WORKSPACE . TYPE), names that buffer.")
(defvar-local atelier-buffer-base-name nil
  "BUFFER's name without its workspace and type qualifier.")
(put 'atelier-buffer-base-name 'permanent-local t)
(defvar-local atelier-buffer-assigned-name nil
  "The name Atelier last gave the buffer; another name means it was renamed.")
(put 'atelier-buffer-assigned-name 'permanent-local t)
(defvar atelier-naming-timer nil)

(defun atelier-buffer-owner-index ()
  "Map each live buffer held by a workspace stack to (WORKSPACE TYPE KIND)."
  (let ((index (make-hash-table :test #'eq)))
    (dolist (workspace (atelier-workspace-list))
      (dolist (content (atelier-workspace-contents workspace))
        (let ((buffer (atelier-content-buffer workspace content)))
          (when (and (buffer-live-p buffer) (not (gethash buffer index)))
            (puthash buffer (list workspace (atelier-content-field content :type) (atelier-content-field content :kind))
                     index)))))
    index))

(defun atelier-buffer-owner (buffer &optional index)
  "Return (WORKSPACE TYPE KIND) for BUFFER, or nil when nothing owns it."
  (or (gethash buffer (or index (atelier-buffer-owner-index)))
      (when-let* ((owner (run-hook-with-args-until-success
                          'atelier-buffer-owner-functions buffer)))
        (list (car owner) (cdr owner) nil))))

(defun atelier-buffer-strip-qualifier (name type)
  "Return NAME without a leading \"WORKSPACE | TYPE | \" qualifier.
A legacy Atelier name such as \"*terminal:WORKSPACE*\" becomes its type label."
  (let ((label (atelier-type-label type)))
    (cond
     ((string-match (concat "\\`.+?" (regexp-quote atelier-buffer-name-separator)
                            (regexp-quote label)
                            (regexp-quote atelier-buffer-name-separator))
                    name)
      (substring name (match-end 0)))
     ((string-match-p (concat "\\`\\*" (regexp-quote label) ":.*\\*\\(<[0-9]+>\\)?\\'") name)
      label)
     (t name))))

(defun atelier-buffer-folder-name (directory)
  (let ((name (file-name-nondirectory (directory-file-name (expand-file-name directory)))))
    (if (string-empty-p name) "/" name)))

(defun atelier-log-buffer-names ()
  "Emacs's own log buffers; Detached owns them without storing them."
  (remove "*Completions*" atelier-global-buffer-names))

(defun atelier-content-base-name (content type buffer)
  "Return the NAME part of CONTENT, live in BUFFER or only saved."
  (cond
   (buffer (atelier-buffer-strip-qualifier (buffer-name buffer) type))
   ((when-let* ((base-name (atelier-type-get type :base-name)))
      (funcall base-name content nil)))
   ((atelier-content-field content :name)
    (atelier-buffer-strip-qualifier (atelier-content-field content :name) type))
   ((atelier-content-field content :file) (file-name-nondirectory (atelier-content-field content :file)))
   (t "unnamed")))

(defun atelier-buffer-base (buffer type)
  "Return BUFFER's name without qualifier, remembering it on first use.
A name given since Atelier last named BUFFER replaces it.  Until then, a
TYPE with :base-name may supply the name, such as a listed folder's."
  (with-current-buffer buffer
    (when (and atelier-buffer-assigned-name
               (not (equal (buffer-name) atelier-buffer-assigned-name)))
      (setq atelier-buffer-base-name (atelier-buffer-strip-qualifier (buffer-name) type)
            atelier-buffer-assigned-name nil))
    (or atelier-buffer-base-name
        (when-let* ((base-name (atelier-type-get type :base-name)))
          (funcall base-name nil buffer))
        (setq atelier-buffer-base-name
              (atelier-buffer-strip-qualifier (buffer-name) type)))))

(defun atelier-buffer-name-keeps-native-p (base type kind)
  "Whether a star-named BASE is found by exact name and must keep it."
  (and (not (atelier-type-get type :tracked))
       (not (eq kind 'scratch))
       (string-match-p "\\`\\*.*\\*\\(<[0-9]+>\\)?\\'" base)))

(defun atelier-buffer-qualified-name (workspace type base)
  (string-join (list (atelier-workspace-name workspace) (atelier-type-label type) base)
               atelier-buffer-name-separator))

(defun atelier-name-buffer (buffer &optional index)
  "Rename BUFFER to \"WORKSPACE | TYPE | NAME\" when its owner permits it."
  (pcase-let ((`(,workspace ,type ,kind) (atelier-buffer-owner buffer index)))
    (when workspace
      (let* ((type (or type 'buffer))
             (base (atelier-buffer-base buffer type)))
        (unless (atelier-buffer-name-keeps-native-p base type kind)
          (let ((wanted (atelier-buffer-qualified-name workspace type base))
                (current (buffer-name buffer)))
            (with-current-buffer buffer
              (unless (or (equal current wanted)
                          (string-match-p (concat "\\`" (regexp-quote wanted) "<[0-9]+>\\'")
                                          current))
                ;; Without UNIQUE, uniquify stops managing the new name.
                (rename-buffer (generate-new-buffer-name wanted)))
              (setq atelier-buffer-assigned-name (buffer-name)))))))))

(defun atelier-buffer-editable-name (buffer)
  "Return the part of BUFFER's name that the user edits."
  (if-let* ((owner (atelier-buffer-owner buffer)))
      (atelier-buffer-strip-qualifier (buffer-name buffer) (or (nth 1 owner) 'buffer))
    (buffer-name buffer)))

(defun atelier-buffer-visible-in-workspace-p (buffer)
  "Whether BUFFER is shown in the main area of any frame."
  (cl-some (lambda (window) (not (window-parameter window 'window-side)))
           (get-buffer-window-list buffer 'no-minibuffer t)))

(atelier-define-operation atelier-adopt-unowned-buffers (buffers)
    (list atelier-detached-workspace-id) nil
  "Give BUFFERS, which no workspace has shown, to the reserved Detached workspace."
  (let ((detached (atelier-ensure-detached-workspace)))
    (dolist (buffer buffers)
      (when (and (buffer-live-p buffer) (atelier-buffer-registerable-p buffer detached))
        (atelier-register-buffer buffer detached)))))

(defun atelier-name-buffers ()
  "Give unowned buffers a workspace, then qualify every renamable buffer name."
  (setq atelier-naming-timer nil)
  (if (or atelier-operation-active atelier-operation-queue)
      (atelier-schedule-naming)
    (let* ((detached (atelier-ensure-detached-workspace))
           (index (atelier-buffer-owner-index))
           (unowned (cl-loop for buffer in (buffer-list)
                             unless (or (gethash buffer index)
                                        (atelier-buffer-visible-in-workspace-p buffer)
                                        (not (atelier-buffer-ownable-p buffer detached)))
                             collect buffer)))
      (when unowned
        (atelier-adopt-unowned-buffers unowned)))
    (let ((index (atelier-buffer-owner-index)))
      (dolist (buffer (buffer-list))
        (when (buffer-live-p buffer)
          (atelier-name-buffer buffer index))))))

(defun atelier-schedule-naming (&rest _)
  "Name buffers shortly after the current command and its display finish."
  (unless (timerp atelier-naming-timer)
    (setq atelier-naming-timer (run-at-time 0.2 nil #'atelier-name-buffers))))

(defun atelier-naming-setup ()
  ;; Emacs runs this hook after creating, killing or renaming a buffer.
  (add-hook 'buffer-list-update-hook #'atelier-schedule-naming)
  (add-hook 'atelier-change-hook #'atelier-schedule-naming)
  (atelier-schedule-naming))

(provide 'atelier-naming)
;;; atelier-naming.el ends here

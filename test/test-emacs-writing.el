;;; test-emacs-writing.el --- Writing behavior tests -*- lexical-binding: t; -*-

(require 'ert)
(require 'use-package)
(add-to-list 'load-path
             (expand-file-name "../dotfiles/emacs/.config/emacs/lisp"
                               (file-name-directory (or load-file-name buffer-file-name))))
(require 'myconfig-editing)
(require 'flyover)

(ert-deftest myconfig-writing-flyover-stays-out-of-editable-text ()
  (dolist (case '(("bad text\nnext line\n" display "\n virtual error")
                  ("bad text" display "\n virtual error")
                  ("bad text\nnext line\n" after-string " virtual error")
                  ("bad text" after-string " virtual error")))
    (with-temp-buffer
      (insert (nth 0 case))
      (goto-char 4)
      (let ((point-before (point))
            (overlay (make-overlay 4 4)))
        (overlay-put overlay (nth 1 case) (nth 2 case))
        (myconfig-flyover-use-margin overlay)
        (should (= (point) point-before))
        (should (= (overlay-start overlay) (line-end-position)))
        (should-not (overlay-get overlay 'after-string))
        (let ((rendered (if (< (line-end-position) (point-max))
                            (overlay-get overlay 'display)
                          (overlay-get overlay 'before-string))))
          (should (equal (get-text-property 0 'cursor rendered) 1))
          (should (equal (substring-no-properties rendered)
                         (if (< (line-end-position) (point-max))
                             "  virtual error\n"
                           "  virtual error"))))
        (should (equal (buffer-string) (nth 0 case)))))))

(ert-deftest myconfig-writing-real-flyover-overlay-never-follows-point ()
  ;; The graphical theme supplies these colors in the live environment.
  (set-face-attribute 'flyover-error nil :foreground "#fbb8b8" :background "#ba5454")
  (advice-add 'flyover--configure-overlay-display :after
              #'myconfig-flyover-use-margin)
  (advice-add 'flyover--configure-overlay :after
              #'myconfig-flyover-use-margin)
  (unwind-protect
      (dolist (case '(("broken" t) ("broken\nnext" t)
                      ("broken\nnext" nil)))
        (with-temp-buffer
          (insert (car case))
          (goto-char 4)
          (let* ((flyover-show-at-eol (cadr case))
                 (flyover-use-theme-colors nil)
                 (point-before (point))
                 (error (flyover-error-create :line 1 :column 1 :beg 1 :end 7
                                              :level 'error :message "virtual error"))
                 (overlay (flyover--create-overlay '(1 . 7) 'error
                                                   "virtual error" error)))
            (should (= (point) point-before))
            (should (= (overlay-start overlay) (line-end-position)))
            (should-not (overlay-get overlay 'after-string))
            (myconfig-flyover-refresh-eob)
            (let ((rendered (if (< (line-end-position) (point-max))
                                (overlay-get overlay 'display)
                              (overlay-get overlay 'before-string))))
              (should (equal (get-text-property 0 'cursor rendered) 1))
              (should (string-match-p "virtual error" rendered)))
            (when (= (line-end-position) (point-max))
              (goto-char (point-max))
              (myconfig-flyover-refresh-eob)
              (should-not (overlay-get overlay 'before-string))
              (goto-char 4)
              (myconfig-flyover-refresh-eob)
              (should (overlay-get overlay 'before-string))))))
    (advice-remove 'flyover--configure-overlay-display
                   #'myconfig-flyover-use-margin)
    (advice-remove 'flyover--configure-overlay
                   #'myconfig-flyover-use-margin)))

(ert-run-tests-batch-and-exit)
;;; test-emacs-writing.el ends here

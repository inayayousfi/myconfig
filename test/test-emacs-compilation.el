;;; test-emacs-compilation.el --- Personal compilation cache -*- lexical-binding: t; -*-

(require 'ert)
(load (expand-file-name "../dotfiles/emacs/.config/emacs/lisp/myconfig-compilation.el"
                        (file-name-directory load-file-name)) nil t)

(ert-deftest myconfig-compilation-reuses-and-refreshes-loaded-code ()
  (let* ((root (make-temp-file "myconfig-compilation-" t))
         (source (expand-file-name "source" root))
         (cache (expand-file-name "cache" root))
         (file (expand-file-name "sample.el" source)))
    (unwind-protect
        (progn
          (make-directory source)
          (with-temp-file file
            (insert ";;; -*- lexical-binding: t; -*-\n(defun compilation-sample () 42)\n"))
          (let* ((paths (myconfig-compilation-prepare source cache))
                 (compiled (expand-file-name "sample.elc" (car paths)))
                 (mtime (file-attribute-modification-time (file-attributes compiled)))
                 (load-path (append paths load-path))
                 (native-comp-jit-compilation nil))
            (load "sample" nil t)
            (should (byte-code-function-p (symbol-function 'compilation-sample)))
            (should (= (compilation-sample) 42))
            (should (equal paths (myconfig-compilation-prepare source cache)))
            (should (equal mtime (file-attribute-modification-time (file-attributes compiled))))
            (with-temp-file file
              (insert ";;; -*- lexical-binding: t; -*-\n(defun compilation-sample () 84)\n"))
            (let* ((fresh (myconfig-compilation-prepare source cache))
                   (load-path (append fresh load-path)))
              (should-not (equal paths fresh))
              (load "sample" nil t)
              (should (= (compilation-sample) 84))))
          (should-not (file-exists-p (concat file "c"))))
      (fmakunbound 'compilation-sample)
      (delete-directory root t))))

(ert-deftest myconfig-compilation-failure-retains-source-loading ()
  (let* ((root (make-temp-file "myconfig-compilation-" t))
         (source (expand-file-name "source" root))
         (cache (expand-file-name "cache" root)))
    (unwind-protect
        (progn
          (make-directory source)
          (with-temp-file (expand-file-name "fallback.el" source)
            (insert ";;; -*- lexical-binding: t; -*-\n(eval-when-compile (when (bound-and-true-p byte-compile-current-file) (error \"Cannot compile\")))\n(defun compilation-fallback () 17)\n"))
          (let* ((paths (myconfig-compilation-prepare source cache))
                 (load-path (append paths load-path)))
            (should-not (file-exists-p (expand-file-name "fallback.elc" (car paths))))
            (load "fallback" nil t)
            (should (= (compilation-fallback) 17))
            (cl-letf (((symbol-function 'call-process)
                       (lambda (&rest _) (ert-fail "Unchanged failed file was recompiled"))))
              (should (equal paths (myconfig-compilation-prepare source cache))))))
      (fmakunbound 'compilation-fallback)
      (delete-directory root t))))

(ert-deftest myconfig-compilation-reuses-native-code-when-available ()
  (skip-unless (and (fboundp 'native-comp-available-p) (native-comp-available-p)))
  (let* ((root (make-temp-file "myconfig-native-" t))
         (source (expand-file-name "source" root))
         (cache (expand-file-name "cache" root))
         (load-path load-path)
         (native-comp-jit-compilation nil)
         (native-comp-eln-load-path (list (expand-file-name "eln" root)))
         (native-comp-async-jobs-number 1))
    (unwind-protect
        (progn
          (make-directory source)
          (make-directory (car native-comp-eln-load-path))
          (with-temp-file (expand-file-name "native-sample.el" source)
            (insert ";;; -*- lexical-binding: t; -*-\n(defun compilation-native-sample (x) (+ x 7))\n"))
          (myconfig-compilation-setup source cache)
          (should native-comp-jit-compilation)
          ;; Batch Emacs disables JIT; build once and verify native cache lookup.
          (require 'comp)
          (native-compile (expand-file-name "native-sample.el" (car load-path)))
          (load "native-sample" nil t)
          (should (native-comp-function-p (symbol-function 'compilation-native-sample)))
          (should (= (compilation-native-sample 35) 42)))
      (fmakunbound 'compilation-native-sample)
      (delete-directory root t))))

;;; test-emacs-compilation.el ends here

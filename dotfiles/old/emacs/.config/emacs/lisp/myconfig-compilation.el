;;; myconfig-compilation.el --- Cached personal libraries -*- lexical-binding: t; -*-

(require 'cl-lib)

(defun myconfig-compilation-prepare (library-directory cache-directory)
  "Compile libraries in LIBRARY-DIRECTORY under CACHE-DIRECTORY.
Compilation runs in a separate Emacs so compile-time requires cannot change
the application's load order.  Failed files remain available as source.
Return the cache directories to place before the original load path."
  (let* ((library-directory (file-name-as-directory library-directory))
         (files (directory-files-recursively library-directory "\\.el\\'"))
         ;; A dependency can supply macros: rebuild consumers too when it changes.
         (signature
          (with-temp-buffer
            (insert emacs-version "\n" system-configuration "\n")
            (when (boundp 'package-alist)
              (prin1 (symbol-value 'package-alist) (current-buffer)))
            (dolist (directory load-path)
              (when (and directory
                         (not (file-in-directory-p directory cache-directory))
                         (not (file-in-directory-p directory library-directory)))
                (insert directory "\n")))
            (dolist (file files)
              (insert (file-relative-name file library-directory) "\n")
              (insert-file-contents-literally file))
            (secure-hash 'sha256 (current-buffer))))
         (destination (expand-file-name (concat "byte-code/" signature) cache-directory))
         (marker (expand-file-name ".complete" destination))
         (directories nil))
    (dolist (file files)
      (let* ((relative (file-relative-name file library-directory))
             (copy (expand-file-name relative destination))
             (directory (file-name-directory copy)))
        (cl-pushnew directory directories :test #'equal)
        (unless (file-exists-p marker)
          (make-directory directory t)
          (copy-file file copy t t))))
    (unless (file-exists-p marker)
      (let* ((worker (expand-file-name ".compile.el" destination))
             (output (get-buffer-create "*Myconfig compilation*")))
        (with-temp-file worker
          (insert ";;; -*- lexical-binding: t; -*-\n")
          (prin1
           `(progn
              (setq load-path ',(append directories load-path)
                    native-comp-jit-compilation nil)
              (startup-redirect-eln-cache
               ,(expand-file-name "eln-cache" cache-directory))
              (require 'bytecomp)
              (let ((failed nil))
                (dolist (file ',(mapcar
                                (lambda (file)
                                  (expand-file-name
                                   (file-relative-name file library-directory) destination))
                                files))
                  (condition-case error
                      (unless (byte-compile-file file) (setq failed t))
                    (error
                     (setq failed t)
                     (message "Compilation failed for %s: %s" file error))))
                (kill-emacs (if failed 1 0))))
           (current-buffer)))
        (with-current-buffer output (erase-buffer))
        (let ((status (call-process
                       (expand-file-name invocation-name invocation-directory)
                       nil output nil "-Q" "--batch" "-l" worker)))
          (when (memq status '(0 1))
            (with-temp-file marker (insert signature)))
          (unless (equal status 0)
            (display-warning
             'myconfig-compilation
             "Some personal libraries could not be compiled; using source for those files. See *Myconfig compilation*.")))))
    (nreverse directories)))

(defun myconfig-compilation-setup (library-directory cache-directory)
  "Enable cached personal libraries, with native compilation when available."
  (condition-case error
      (let ((directories (myconfig-compilation-prepare library-directory cache-directory)))
        (setq load-path (append directories load-path))
        (when (and (fboundp 'native-comp-available-p) (native-comp-available-p))
          (setq native-comp-jit-compilation t)))
    (error
     (display-warning 'myconfig-compilation
                      (format "Compilation cache unavailable; using source: %s" error)))))

(provide 'myconfig-compilation)
;;; myconfig-compilation.el ends here

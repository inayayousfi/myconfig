;;; vertico-atelier.el --- Vertico's highlighted entry for Traveller -*- lexical-binding: t; -*-

;; Vertico keeps the entry it highlights in private variables.  This adapter is
;; the one place that reads them, so a Vertico update can break only this file,
;; and Traveller works with any completion interface.

(require 'atelier-traveller)

(defvar vertico--base)
(defvar vertico--candidates)
(defvar vertico--index)

(defun vertico-atelier-highlighted ()
  "Return the full label Vertico highlights in the current prompt, or nil."
  (when-let* ((candidates (bound-and-true-p vertico--candidates))
              ((>= vertico--index 0))
              (candidate (nth vertico--index candidates)))
    (substring-no-properties (concat vertico--base candidate))))

(defun vertico-atelier-setup ()
  "Let Traveller's TAB start from the entry Vertico highlights."
  (setq atelier-traveller-highlighted-function #'vertico-atelier-highlighted))

(provide 'vertico-atelier)
;;; vertico-atelier.el ends here

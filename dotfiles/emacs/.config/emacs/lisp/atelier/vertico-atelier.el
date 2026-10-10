;;; vertico-atelier.el --- Vertico's list and highlight for Traveller -*- lexical-binding: t; -*-

;; Vertico keeps its list and the entry it highlights in private variables.
;; Among the configuration files, this adapter is the one place that reads or
;; sets them, so a Vertico update can break only this file and the Traveller
;; tests that drive Vertico, and Traveller works with any completion interface.

(require 'atelier-traveller)

(defvar vertico--base)
(defvar vertico--candidates)
(defvar vertico--index)
(defvar vertico--input)
(defvar vertico--lock-candidate)
(declare-function vertico--update "vertico")

(defun vertico-atelier-highlighted ()
  "Return the full label Vertico highlights in the current prompt, or nil."
  (when-let* ((candidates (bound-and-true-p vertico--candidates))
              ((>= vertico--index 0))
              (candidate (nth vertico--index candidates)))
    (substring-no-properties (concat vertico--base candidate))))

(defun vertico-atelier-refresh (entry)
  "Recompute Vertico's list now and highlight ENTRY when non-nil.
Vertico otherwise recomputes only after a command changes the input, so a
command could act on the list from before a TAB cycle ended.  ENTRY is found
by its text, since Vertico may move an entry equal to the input to the top."
  ;; Vertico sets its input state once it runs in this prompt.
  (when (bound-and-true-p vertico--input)
    (setq vertico--input nil)
    (vertico--update)
    (when-let* ((index (and entry (seq-position vertico--candidates entry))))
      (setq vertico--index index
            vertico--lock-candidate t))))

(defun vertico-atelier-setup ()
  "Let Traveller's TAB start from the entry Vertico highlights and show its cycle."
  (setq atelier-traveller-highlighted-function #'vertico-atelier-highlighted
        atelier-traveller-list-refresh-function #'vertico-atelier-refresh))

(provide 'vertico-atelier)
;;; vertico-atelier.el ends here

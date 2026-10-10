;;; jumel-atelier.el --- Jumel reviews as Atelier content -*- lexical-binding: t; -*-

;;; Commentary:

;; Jumel's review sides become Atelier's jumel type, named
;; "WORKSPACE | jumel | before TITLE" and "... | after TITLE".  They are
;; transient: a restart does not bring them back.  Atelier places them: the
;; old side in the current view and the new side in a split on its right,
;; and a side leaving the screen closes the other side's split.

;;; Code:

(require 'atelier)
(require 'jum)

(defun jumel-atelier-capture (_buffer)
  "Describe a review side as transient content."
  (list :kind 'transient :persistent nil))

(defun jumel-atelier-arrange (side window _old)
  "Show SIDE's review in WINDOW's view and a new view on its right.
Select SIDE's window."
  (pcase-let ((`(,left . ,right) (buffer-local-value 'jumel--sides side)))
    (select-window window)
    (atelier-show-buffer left nil window)
    (let ((right-window (atelier-split-right right)))
      (select-window (if (eq side right) right-window window)))
    (with-current-buffer left (jumel--align))))

(defun jumel-atelier-leave (peer-window _restore _new)
  "Close the split of PEER-WINDOW, which shows the side left behind."
  (when (and (window-live-p peer-window)
             (not (one-window-p nil (window-frame peer-window))))
    (with-selected-window peer-window
      (atelier-close-split))))

(defun jumel-atelier-setup ()
  "Register review sides as Atelier's jumel type, placed by Atelier."
  (atelier-define-type 'jumel :tracked t :buffer-p #'jumel-buffer-p
                       :capture #'jumel-atelier-capture)
  (setq jumel-arrange-function #'jumel-atelier-arrange
        jumel-leave-function #'jumel-atelier-leave))

(provide 'jumel-atelier)
;;; jumel-atelier.el ends here

;;; myconfig-terminal.el --- Ghostel terminal integration -*- lexical-binding: t; -*-

(require 'myconfig-core)
(defvar myconfig-data-directory)
(defvar ghostel-module-directory)
(defvar ghostel-module-auto-install)
(defvar aipanel-terminal-function)
(declare-function myconfig-paste "myconfig-bindings")
(setq ghostel-module-directory
      (expand-file-name "ghostel-module/" myconfig-data-directory)
      ghostel-module-auto-install 'download)
(require 'ghostel)
(require 'evil-ghostel)

(defcustom myconfig-terminal-escape-key (kbd "M-x")
  "Shared key sequence that returns editor views to Evil normal state."
  :type 'key-sequence
  :group 'myconfig)
(require 'ghostel-atelier)

(atelier-define-operation myconfig-terminal ()
    (list (atelier-current-workspace-id)) nil
  (interactive)
  (when (window-parameter nil 'window-side)
    (select-window (atelier-main-window)))
  (let* ((workspace (atelier-current-workspace))
         (in-terminal (derived-mode-p 'ghostel-mode))
          (existing (and workspace (not in-terminal)
                         (atelier-workspace-buffer-by-type workspace 'terminal)))
          (launch (unless existing (funcall atelier-terminal-command-function workspace)))
         (name (atelier-entry-buffer-name 'terminal))
         (buffer (or existing
                     (ghostel-atelier-buffer
                      (if in-terminal
                          (generate-new-buffer-name name)
                        name)
                        (plist-get launch :directory)
                        (plist-get launch :program)
                        (plist-get launch :arguments)
                         workspace
                         (when (plist-get launch :shell)
                           (append (list :executable (plist-get launch :shell)
                                         :login (member "-l" (plist-get launch :arguments)))
                                   (when (plist-get launch :location)
                                     (list :location (plist-get launch :location)))))
                         nil 'terminal))))
    (atelier-show-buffer buffer workspace)))

(atelier-define-operation myconfig-terminal-split-right ()
    (list (atelier-current-workspace-id)) nil
  (interactive)
  (atelier-split-right)
  (myconfig-terminal))

(atelier-define-operation myconfig-terminal-split-below ()
    (list (atelier-current-workspace-id)) nil
  (interactive)
  (atelier-split-below)
  (myconfig-terminal))

(defun myconfig-normal-state ()
  "Cancel active input and return the current editor view to normal state."
  (interactive)
  (cond
   ((minibufferp)
    (when-let* ((window (minibuffer-selected-window)))
      (with-selected-window window
        (myconfig-normal-state)))
    (abort-recursive-edit))
   ((derived-mode-p 'ghostel-mode)
    (myconfig-terminal-escape))
   (t (evil-force-normal-state))))

(defun myconfig-terminal-escape ()
  (interactive)
  (kill-local-variable 'meta-prefix-char)
  (ghostel-emacs-mode)
  (evil-local-mode 1)
  (evil-ghostel-mode 1)
  (evil-normal-state))

(defun myconfig-terminal-enter-input ()
  "Give the terminal process all keyboard input through Ghostel char mode."
  (interactive)
  (when (bound-and-true-p evil-ghostel-mode)
    (evil-ghostel-mode -1))
  (when (bound-and-true-p evil-local-mode)
    (evil-local-mode -1))
  (ghostel-char-mode)
  ;; In graphical Emacs, preserve Meta as a distinct event so literal ESC
  ;; can go to the PTY without stealing M-x from the editor.
  (setq-local meta-prefix-char nil)
  (setq buffer-read-only nil))

(defun myconfig-terminal-enter-input-later (buffer)
  "Give BUFFER terminal input once Ghostel has spawned, unless Alt+x came first."
  (run-at-time 0 nil
               (lambda ()
                 (when (buffer-live-p buffer)
                   (with-current-buffer buffer
                     (unless (eq ghostel--input-mode 'emacs)
                       (myconfig-terminal-enter-input)))))))

(defun myconfig-terminal-insert-means-input ()
  "Turn Evil's insert state in a terminal into Ghostel input."
  (when (derived-mode-p 'ghostel-mode)
    ;; Keys go to the program, so Evil must not repeat its own insert record.
    (setq evil-insert-count nil
          evil-insert-vcount nil
          evil-insert-lines nil
          evil-insert-repeat-info nil)
    (let ((buffer (current-buffer)))
      (run-at-time 0 nil
                   (lambda ()
                     (when (buffer-live-p buffer)
                       (with-current-buffer buffer
                         (when (and (bound-and-true-p evil-local-mode)
                                    (eq evil-state 'insert))
                           (myconfig-terminal-enter-input)))))))))

(defun myconfig-terminal-display-setup ()
  (setq buffer-read-only nil)
  (add-hook 'evil-local-mode-hook #'myconfig-terminal-keep-input nil t)
  (add-hook 'post-command-hook #'myconfig-terminal-keep-writable nil t)
  (display-line-numbers-mode -1)
  (hl-line-mode -1)
  ;; Ghostel resets the input mode when it spawns, after this hook.
  (myconfig-terminal-enter-input-later (current-buffer)))

(defun myconfig-terminal-keep-input ()
  "Do not let automatic Evil activation take input from a char-mode terminal."
  (when (and (derived-mode-p 'ghostel-mode)
             (eq ghostel--input-mode 'char)
             (bound-and-true-p evil-local-mode))
    (myconfig-terminal-enter-input)))

(defun myconfig-terminal-keep-writable ()
  (when (derived-mode-p 'ghostel-mode)
    (setq buffer-read-only nil)))

(defun myconfig-terminal-configure-char-keys ()
  "Send terminal input by default; reserve only the exit and paste keys."
  ;; Ghostel handles ordinary input.  Cover modified printable keys as a
  ;; class so none fall through to Emacs bindings, including future ones.
  ;; Keeping Meta distinct from ESC also lets literal ESC reach the PTY.
  (let ((meta-prefix-char nil))
    (ghostel--define-terminal-keys ghostel-char-mode-map 'no-exceptions)
    (dolist (modifier '("C-" "M-" "C-M-" "C-S-" "M-S-" "C-M-S-"))
      (dolist (character (number-sequence ?! ?~))
        (ignore-errors
          (define-key ghostel-char-mode-map
            (kbd (format "%s%c" modifier character)) #'ghostel--send-event))))
    (dolist (modifier '("" "S-" "C-" "M-" "C-S-" "M-S-" "C-M-" "C-M-S-"))
      (dolist (number (number-sequence 1 35))
        (define-key ghostel-char-mode-map (kbd (format "<%sf%d>" modifier number))
                    #'ghostel--send-event)))
    (define-key ghostel-char-mode-map [remap ghostel-semi-char-mode]
                #'ghostel--send-event)
    (define-key ghostel-char-mode-map (kbd "ESC") #'ghostel--send-event)
    (define-key ghostel-char-mode-map (kbd "C-S-v") #'myconfig-paste)
    (define-key ghostel-char-mode-map myconfig-terminal-escape-key
                #'myconfig-normal-state)))

(defun myconfig-terminal-setup ()
  (setq ghostel-kill-buffer-on-exit t
        ghostel-query-before-killing nil
        ghostel-term "xterm-256color"
        evil-ghostel-escape 'terminal
        evil-ghostel-initial-state 'normal
        ghostel-readonly-fast-exit nil
        confirm-kill-processes nil)
  (setq-default kill-buffer-query-functions
                (remq #'process-kill-buffer-query-function
                      (default-value 'kill-buffer-query-functions)))
  (evil-set-initial-state 'ghostel-mode 'normal)
  (add-hook 'ghostel-mode-hook #'myconfig-terminal-display-setup)
  (add-hook 'evil-insert-state-entry-hook #'myconfig-terminal-insert-means-input)
  (define-key ghostel-mode-map myconfig-terminal-escape-key #'myconfig-normal-state)
  (myconfig-terminal-configure-char-keys)
  (define-key evil-ghostel-mode-map myconfig-terminal-escape-key #'myconfig-normal-state)
  (evil-define-key 'normal evil-ghostel-mode-map
    (kbd "i") #'myconfig-terminal-enter-input
    (kbd "a") #'myconfig-terminal-enter-input
    (kbd "I") #'myconfig-terminal-enter-input
     (kbd "A") #'myconfig-terminal-enter-input))

(defun myconfig-aipanel-terminal (name directory program arguments _owner _selection)
  (ghostel-atelier-exec-buffer (format "*%s*" name) directory program arguments
                               '((kind . aipanel))))

(defun myconfig-aipanel-setup ()
  (setq aipanel-terminal-function #'myconfig-aipanel-terminal))

(provide 'myconfig-terminal)
;;; myconfig-terminal.el ends here

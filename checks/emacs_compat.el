;;; Does csvdt.el keep to the Emacs it says it needs?  -*- lexical-binding: t; -*-
;;
;; The package header declares a minimum, and nothing enforces it: an Emacs new
;; enough to run the tests is new enough to hide every function added since.
;; This looks for calls to functions newer than the declared minimum, asking
;; the NEWS files Emacs ships rather than a list of names kept here.
;;
;; The rule.  A function is newer than a version when that version's NEWS
;; announces it as new: the first sentence of an entry says a new function,
;; macro or command and names it in quotes ("** New function 'string-search'.").
;; The earliest version that does so is its floor.  Nothing else in NEWS
;; counts.  `help-fns--first-release',
;; which dates a symbol by its earliest mention of any kind, reports `>=',
;; `nreverse' and `nthcdr' as 30.1 because NEWS.30 mentions them in passing
;; (an alias, a warning, a sample), although all three are far older than the
;; Emacs 27.1 this package asks for, and it dates `count-lines' to 28.1 for an
;; entry about a new argument.  A function nobody announced, which is every
;; one that predates NEWS being kept, has no floor and is not flagged.
;;
;; The rule can miss a function whose announcement is worded some other way.
;; It does not raise false alarms for old ones, which is the property a check
;; run before a release needs.  It tests itself: if NEWS stops being worded as
;; the rule expects, the functions listed in `compat-known-announced' stop
;; being found and the check fails rather than reporting nothing.
;;
;; The exact answer is byte-compiling under the oldest supported Emacs, which
;; is worth doing in CI if it is ever worth doing at all.

(defconst compat-known-announced
  '((string-search . "28.1") (string-replace . "28.1") (ensure-list . "28.1")
    (flatten-tree . "27.1") (proper-list-p . "27.1"))
  "Functions and the version NEWS announces them in, to test the rule on.")

(defconst compat-known-unannounced '(>= nreverse nthcdr count-lines)
  "Old functions that NEWS mentions without announcing, to test the rule on.")

(defconst compat-new-thing
  "\\<new\\>[^']*\\<\\(?:functions?\\|macros?\\|commands?\\|special forms?\\|modes?\\|aliases\\|alias\\)\\>"
  "Matches a sentence that says something new is a function, before it names it.
\"New function 'a' and 'b'\" and \"New polymorphic comparison function 'value<'\"
match.  \"New ':type' for 'defcustom'\" and \"New value 'point' for 'ielm'\",
which only mention a function, do not.")

(defun compat-announcements ()
  "The entries of the NEWS files that announce something new.
A list of (VERSION . SENTENCE), SENTENCE being the first sentence of an
entry that matches `compat-new-thing' and is not about an argument."
  (let ((case-fold-search nil)
        (found '()))
    (with-temp-buffer
      (dolist (file (directory-files data-directory t "\\`NEWS\\(\\'\\|\\.\\)"))
        (erase-buffer)
        (insert-file-contents file)
        (goto-char (point-min))
        (let ((version nil))
          (while (re-search-forward
                  "^\\(?:\\* \\(?:.* \\)?Emacs \\([0-9.]+[0-9]\\)\\|\\*\\*\\*? \\)"
                  nil t)
            (if (match-string 1)
                (setq version (match-string 1))
              (let ((sentence (buffer-substring
                               (match-beginning 0)
                               (if (re-search-forward "\\.\\(?:$\\|[ \n]\\)" nil t)
                                   (point)
                                 (point-max)))))
                (when (and version
                           (string-match-p compat-new-thing (downcase sentence))
                           (not (string-match-p "argument" sentence)))
                  (push (cons version sentence) found))))))))
    found))

(defun compat-announced-in (symbol announcements)
  "The earliest version whose ANNOUNCEMENTS name SYMBOL, or nil.
ANNOUNCEMENTS is the list `compat-announcements' returns."
  (let ((needle (concat "'" (symbol-name symbol) "'"))
        (first nil))
    (dolist (entry announcements)
      (when (and (string-match-p (regexp-quote needle) (cdr entry))
                 (or (null first) (version< (car entry) first)))
        (setq first (car entry))))
    first))

(defun compat-declared-minimum (file)
  "The Emacs version FILE's own header asks for."
  (with-temp-buffer
    (insert-file-contents file)
    (goto-char (point-min))
    (if (re-search-forward
         "Package-Requires:.*(emacs +\"\\([0-9.]+\\)\")" nil t)
        (match-string 1)
      (error "No Package-Requires line in %s" file))))

(defun compat-functions-in (file)
  "Every symbol FILE mentions that names a function in this Emacs."
  (let ((seen (make-hash-table :test 'eq)))
    (with-temp-buffer
      (insert-file-contents file)
      (goto-char (point-min))
      (condition-case nil
          (while t
            (letrec ((walk (lambda (form)
                             (cond
                              ((consp form)
                               (funcall walk (car form))
                               (funcall walk (cdr form)))
                              ((and form (symbolp form) (fboundp form))
                               (puthash form t seen))))))
              (funcall walk (read (current-buffer)))))
        (end-of-file nil)))
    seen))

(let* ((file (expand-file-name "csvdt.el"
                               (expand-file-name "emacs"
                                                 (or (getenv "CSVDT_ROOT")
                                                     default-directory))))
       (minimum (compat-declared-minimum file))
       (symbols (compat-functions-in file))
       (announcements (compat-announcements))
       (flagged '())
       (broken '()))
  ;; The rule is only worth its answers while it still finds what it should.
  (dolist (known compat-known-announced)
    (unless (equal (compat-announced-in (car known) announcements) (cdr known))
      (push (format "%s should be announced in %s, got %s" (car known)
                    (cdr known) (compat-announced-in (car known) announcements))
            broken)))
  (dolist (known compat-known-unannounced)
    (when (compat-announced-in known announcements)
      (push (format "%s should not be announced, but is in %s" known
                    (compat-announced-in known announcements))
            broken)))
  (maphash
   (lambda (symbol _)
     (let ((introduced (compat-announced-in symbol announcements)))
       (when (and introduced (version< minimum introduced))
         (push (cons symbol introduced) flagged))))
   symbols)
  (princ (format "emacs compatibility: %d functions against the declared %s, %d newer\n"
                 (hash-table-count symbols) minimum (length flagged)))
  (dolist (entry (sort flagged (lambda (a b) (string< (car a) (car b)))))
    (princ (format "  %s was introduced in %s, later than the declared %s\n"
                   (car entry) (cdr entry) minimum)))
  (dolist (line (nreverse broken))
    (princ (format "  the rule is out of step with NEWS: %s\n" line)))
  (kill-emacs (if (or flagged broken) 1 0)))

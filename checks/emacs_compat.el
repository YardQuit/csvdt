;;; Does csvdt.el keep to the Emacs it says it needs?  -*- lexical-binding: t; -*-
;;
;; The package header declares a minimum, and nothing enforces it: an Emacs
;; new enough to run the tests is new enough to hide every function added
;; since. `string-search' reached the file that way and was caught by hand,
;; after which two names were written into the test suite -- which pins those
;; two and says nothing about the next one.
;;
;; This asks Emacs instead. `help-fns--first-release' reports the version a
;; symbol was first mentioned in, from the NEWS files Emacs ships, so every
;; function the file uses can be asked at once rather than remembered.
;;
;; It is a check and not a test because the answer is not exact in either
;; direction:
;;
;;   Too old.  The version is the earliest NEWS mention of the name, and a
;;   name that reads as an ordinary word matches ordinary prose -- `always'
;;   arrived in 28.1 and is reported as 25.1. So this can miss one.
;;
;;   Too new.  A NEWS entry about an argument added to an old function
;;   reports that function as new: `count-lines' has been there since long
;;   before the 28.1 entry about its third argument. Those are listed below
;;   with the reason, rather than left to be rediscovered.
;;
;; A real answer would be byte-compiling under the oldest supported Emacs,
;; which is worth doing in CI if it is ever worth doing at all. This is the
;; cheap version, and it would have caught the one that got through.

(require 'help-fns)

(defconst compat-known-older
  '((count-lines 2 . "there since Emacs 19; the 28.1 entry adds an optional
     third argument, which this package does not pass"))
  "Symbols NEWS dates later than they really are: name, the arity the
excuse covers, and why.
The arity is the point.  An excuse here is a claim about how the package
calls the symbol, and the claim is what makes it safe: `count-lines\=' is
older than its NEWS entry only because that entry is about a third
argument nobody here passes.  Pass it and the excuse becomes a lie that
hides a real finding -- which is why the excuse was written down with its
condition rather than as a bare name, and why the condition is now
checked instead of trusted.")

(defun compat-widest-call (file symbol)
  "The most arguments FILE passes to SYMBOL in a call position.
Nil when FILE never calls it.  A symbol quoted or passed around rather
than called is not a call and does not count."
  (let ((widest nil))
    (with-temp-buffer
      (insert-file-contents file)
      (goto-char (point-min))
      (condition-case nil
          (while t
            (letrec ((walk (lambda (form)
                             (when (consp form)
                               (when (eq (car form) symbol)
                                 (let ((count (safe-length (cdr form))))
                                   (when (or (null widest) (> count widest))
                                     (setq widest count))))
                               (funcall walk (car form))
                               (funcall walk (cdr form))))))
              (funcall walk (read (current-buffer)))))
        (end-of-file nil)))
    widest))

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
       (flagged '())
       (excused '()))
  (maphash
   (lambda (symbol _)
     (let ((introduced (ignore-errors (help-fns--first-release symbol))))
       (when (and introduced (version< minimum introduced))
         (let* ((excuse (assq symbol compat-known-older))
                (covers (and excuse (cadr excuse)))
                (passes (and excuse (compat-widest-call file symbol))))
           (cond
            ((null excuse) (push (cons symbol introduced) flagged))
            ;; The excuse holds only while the package keeps to the arity it
            ;; was written about.
            ((and passes (> passes covers))
             (push (cons symbol (format "%s (called with %d arguments, and \
the excuse covers %d)" introduced passes covers))
                   flagged))
            (t (push symbol excused)))))))
   symbols)
  (princ (format "emacs compatibility: %d functions against the declared %s"
                 (hash-table-count symbols) minimum))
  (when excused
    (princ (format ", %d excused" (length excused))))
  (princ (format ", %d newer\n" (length flagged)))
  (dolist (entry (sort flagged (lambda (a b) (string< (car a) (car b)))))
    (princ (format "  %s was introduced in %s, later than the declared %s\n"
                   (car entry) (cdr entry) minimum)))
  (kill-emacs (if flagged 1 0)))

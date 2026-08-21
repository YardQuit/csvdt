;;; test-csvdt.el --- Batch tests for csvdt.el  -*- lexical-binding: t; -*-

;; SPDX-License-Identifier: GPL-3.0-or-later

;;; Commentary:

;; Run from this directory with:
;;
;;     ./run-tests.sh
;;
;; or against a particular binary:
;;
;;     CSVDT=/path/to/csvdt ./run-tests.sh
;;
;; These tests drive a real csvdt rather than a mock, since the whole point of
;; the package is that it asks the binary what it can do.  The one exception is
;; the compatibility test, which needs a csvdt that refuses `--list-options'
;; and so uses a stub.

;;; Code:

(require 'seq)
(require 'cl-lib)

(defvar test-csvdt-directory
  (file-name-directory (or load-file-name buffer-file-name))
  "Directory holding csvdt.el, so the tests do not depend on the cwd.")

(add-to-list 'load-path test-csvdt-directory)
(require 'csvdt)

(setq csvdt-executable
      (or (getenv "CSVDT")
          (expand-file-name "../target/release/csvdt" test-csvdt-directory)))

(unless (file-executable-p csvdt-executable)
  (message "No csvdt at %s -- build it, or set CSVDT" csvdt-executable)
  (kill-emacs 2))

(defvar test-csvdt-failures 0)
(defvar test-csvdt-ran 0
  "How many checks were reached.")

(defconst test-csvdt-expected 300
  "How many checks this file makes.
Raise it with the count.  A run reporting fewer has not passed, whatever
the ones it did reach said.

Counted, not just tallied, for the reason tty-test.el gives about
itself: ALL PASS was printed whenever nothing had failed, which is also
true of a run where nothing was tried -- a check commented out while
something was being chased, a `let\=' that returned early, an edit that
left the calls unreachable.  Twelve checks deleted from this file went
on printing ALL PASS and exiting 0, with nothing anywhere saying that
298 had become 286.")

(defun test-csvdt-check (name got want)
  "Report NAME as passing when GOT equals WANT, and as failing otherwise."
  (setq test-csvdt-ran (1+ test-csvdt-ran))
  (if (equal got want)
      (message "  ok   %s" name)
    (setq test-csvdt-failures (1+ test-csvdt-failures))
    (message "  FAIL %s\n    got:  %S\n    want: %S" name got want)))

(message "csvdt.el tests against %s\n  %s"
         csvdt-executable (csvdt-version-string))

;;; Option discovery comes from --list-options

(let* ((records (csvdt--parse-option-list (csvdt--output-of "--list-options")))
       (from-help (csvdt--parse-options (csvdt--help-text)))
       (forms (lambda (list)
                (sort (seq-remove
                       #'string-empty-p
                       (append (mapcar (lambda (r) (or (plist-get r :short) "")) list)
                               (mapcar (lambda (r) (or (plist-get r :long) "")) list)))
                      #'string<))))
  (test-csvdt-check "--list-options parses into records"
                    (and (listp records) (> (length records) 20)
                         (plist-member (car records) :short)
                         (plist-member (car records) :value)
                         t)
                    t)
  (test-csvdt-check "same option set as parsing --help gives"
                    (funcall forms records) (funcall forms from-help))
  (test-csvdt-check "the list says which options take a value"
                    (and (csvdt--shorts-taking-a-separate-value) t) t)
  (test-csvdt-check "help parsing does not claim to know"
                    (seq-remove #'null (mapcar (lambda (r) (plist-get r :value))
                                               from-help))
                    nil)
  (test-csvdt-check "csvdt-options offers an equals candidate"
                    (and (seq-filter (lambda (option) (string-suffix-p "=" option))
                                     (csvdt-options))
                         t)
                    t))

;;; An unreadable format is declined rather than misread

(test-csvdt-check
 "an unknown format version returns nil"
 (csvdt--parse-option-list
  "# csvdt-option-list 9999\n-H\t--has-header\tnone\tnone\t\n")
 nil)
(test-csvdt-check "text that is not a list returns nil"
                  (csvdt--parse-option-list "Usage: csvdt\n") nil)
(test-csvdt-check "no output at all returns nil"
                  (csvdt--parse-option-list nil) nil)

;;; A csvdt without --list-options still completes

(let ((stub (make-temp-file "old-csvdt" nil ".sh")))
  (with-temp-file stub
    (insert "#!/bin/sh\n"
            "case \"$1\" in\n"
            "  --version) echo 'csvdt 0.9.0 (IANA tzdata 2025b)' ;;\n"
            "  --help) exec " csvdt-executable " --help ;;\n"
            "  *) echo 'error: unexpected argument' >&2; exit 2 ;;\n"
            "esac\n"))
  (set-file-modes stub #o755)
  (let ((csvdt-executable stub)
        (csvdt--options-cache nil))
    (test-csvdt-check "falls back to --help when --list-options is refused"
                      (and (member "-H" (csvdt-options))
                           (> (length (csvdt-options)) 20)
                           t)
                      t))
  (delete-file stub))

;;; The cache is keyed on the binary, not on its version
;;
;; It once held (VERSION . RECORDS) and was checked by running `csvdt
;; --version', which cost a subprocess per lookup; it now holds (PROGRAM
;; MTIME VERSION . RECORDS) and is checked against the file itself.  A test
;; written for the old shape put a version string where the program path
;; goes, so the cache could never match and the check passed whatever the
;; keying did.  A marker record that a matching cache really does serve is
;; what makes the two misses below mean something.

(let* ((program (file-truename (csvdt--program)))
       (mtime (file-attribute-modification-time (file-attributes program)))
       (stale (list :short "" :long "--stale" :value "none" :separator "none"))
       (version "csvdt 0.0.1 (IANA tzdata 1970a)"))

  ;; Matching path and mtime: the records are taken as they stand, marker
  ;; and all, so a miss below is a miss and not a shape this file got wrong.
  (setq csvdt--options-cache (list program mtime version stale))
  (test-csvdt-check "a cache matching the binary is used as it stands"
                    (and (member "--stale" (csvdt-options)) t) t)

  ;; Another path, this binary's own mtime: the path alone invalidates it.
  (setq csvdt--options-cache
        (list (concat program "-not-this-one") mtime version stale))
  (test-csvdt-check "a cache recorded against another program is re-read"
                    (member "--stale" (csvdt-options)) nil)

  ;; This binary's path, another mtime -- what a rebuild leaves behind.
  ;; A time in the past rather than a touch, so the file is not written to.
  (setq csvdt--options-cache
        (list program (time-subtract mtime 5) version stale))
  (test-csvdt-check "and a cache recorded against another mtime likewise"
                    (member "--stale" (csvdt-options)) nil)

  ;; What the refill holds is this binary's own, version included -- the
  ;; version is carried for anything that wants to say which csvdt the
  ;; records came from, and is no longer what the lookup is keyed on.
  (test-csvdt-check "the refill is this binary's options"
                    (and (member "-H" (csvdt-options)) t) t)
  (test-csvdt-check "and its version, not the one the stale cache claimed"
                    (nth 2 csvdt--options-cache) (csvdt-version-string)))

;;; Splitting a command line
;;
;; This is where a comma-separated read went wrong: every one of csvdt's own
;; value lists uses commas, so a reader that split on them handed csvdt half an
;; argument.  Each of these was broken by that and is checked here as typed.

(dolist (case '(("-H -p" ("-H" "-p"))
                ("-a 1,0" ("-a" "1,0"))
                ("-a1,0" ("-a1,0"))
                ("--remove 0,3,4" ("--remove" "0,3,4"))
                ("-i 0,replace" ("-i" "0,replace"))
                ("-o0,+02:00" ("-o0,+02:00"))
                ("-d0,1" ("-d0,1"))
                ("-f=fix" ("-f=fix"))
                ("  -H   -p  " ("-H" "-p"))
                ("" nil)))
  (test-csvdt-check (format "splits %S" (car case))
                    (csvdt--split-arguments (car case))
                    (cadr case)))

;; A quoted argument holds a space, which a whitespace split alone would lose.
(test-csvdt-check "a quoted argument keeps its space"
                  (csvdt--split-arguments "--print-delimiter \" \"")
                  '("--print-delimiter" " "))

(test-csvdt-check
 "unbalanced quotes are reported rather than signalling raw"
 (condition-case err
     (progn (csvdt--split-arguments "--comment \"") nil)
   (user-error (and (string-match-p "Unbalanced quotes" (cadr err)) t)))
 t)

;;; Either quote character, because the examples people paste come from a shell
;;
;; Only the double one used to be understood, so a pasted '#' arrived as three
;; characters and csvdt refused it -- loudly, and with the stray quotes visible
;; in the message, but for a reason about this prompt rather than the option.

(dolist (case '(("--comment '#'" ("--comment" "#"))
                ("-H -p --comment '#'" ("-H" "-p" "--comment" "#"))
                ("--trim 'all'" ("--trim" "all"))
                ("--print-delimiter ';'" ("--print-delimiter" ";"))
                ("-i '0,replace' -u1" ("-i" "0,replace" "-u1"))
                ("'a b'" ("a b"))
                ("''" (""))
                ;; Quotes need not be the whole argument.
                ("a'b'c" ("abc"))))
  (test-csvdt-check (format "splits %S" (car case))
                    (csvdt--split-arguments (car case))
                    (cadr case)))

(test-csvdt-check
 "an unbalanced single quote is reported too"
 (condition-case err
     (progn (csvdt--split-arguments "--comment '") nil)
   (user-error (and (string-match-p "Unbalanced quotes" (cadr err)) t)))
 t)

;; The two differ as they do in a shell: a single-quoted run is literal
;; throughout, a double-quoted one honours backslash escapes.  That difference
;; is how a lone quote character is written -- by wrapping it in the other kind.
(dolist (case '(("'a\\b'" ("a\\b"))
                ("'\"'" ("\""))
                ("\"'\" " ("'"))))
  (test-csvdt-check (format "a single-quoted run is literal: %S" (car case))
                    (csvdt--split-arguments (car case))
                    (cadr case)))

;; Deliberately NOT the shell's rule, and the reason the split is not simply
;; handed to one: outside quotes a backslash stays as itself, so a Windows path
;; typed as a value is one argument rather than C:datax.csv.  Left as it was
;; when single quotes were added, which is the half of that change most easily
;; broken by "make it more like a shell".
(dolist (case '(("C:\\data\\x.csv" ("C:\\data\\x.csv"))
                ("--print-delimiter \\t" ("--print-delimiter" "\\t"))
                ("a\\ b" ("a\\" "b"))))
  (test-csvdt-check (format "a backslash outside quotes is itself: %S" (car case))
                    (csvdt--split-arguments (car case))
                    (cadr case)))

;; And inside a double-quoted run the escapes are the shell-familiar set --
;; \t \n \r \\ \" -- read by `split-string-and-unquote' once they pass, so
;; each means what it would in a shell and nothing else gets to mean anything.
(dolist (case '(("--print-delimiter \"\\t\"" ("--print-delimiter" "\t"))
                ("\"a\\nb\"" ("a\nb"))
                ("\"a\\rb\"" ("a\rb"))
                ("--print-delimiter \"\\\\\"" ("--print-delimiter" "\\"))
                ("\"a\\\"b\"" ("a\"b"))
                ("\"\"" (""))))
  (test-csvdt-check (format "double-quoted escapes are the shell set: %S" (car case))
                    (csvdt--split-arguments (car case))
                    (cadr case)))

;; End to end: the split line reaches csvdt intact and does the right thing.
;; The old tests passed pre-split lists and so never exercised this at all.
(dolist (case '(("-H -p -a 1,0" "a,b,c\n0,1,2\n" "b,a,c\n1,0,2\n")
                ("-H -p -a1,0" "a,b,c\n0,1,2\n" "b,a,c\n1,0,2\n")
                ("-H -p --remove 0,2" "a,b,c\n0,1,2\n" "b\n1\n")
                ("-H -p -u0 -i 0,replace" "ts\n2026-07-25T12:00:00+02:00\n"
                 "to_utc\n2026-07-25T10:00:00+00:00\n")
                ("-H -p -o0,+02:00 -i 0,replace" "ts\n2026-07-25T12:00:00Z\n"
                 "to_offset\n2026-07-25T14:00:00+02:00\n")
                ;; csvdt marks a derived column with '<', so the expected
                ;; header is its own, not a guess at one.
                ("-H -p -d0,1" "a,b\n2026-07-25T12:00:00Z,2026-07-25T13:30:00Z\n"
                 "a,b,<row_duration\n2026-07-25T12:00:00Z,2026-07-25T13:30:00Z,PT1H30M\n")
                ;; Single-quoted, as the documentation's own examples are
                ;; written. This is the case that reached csvdt as three
                ;; characters and was refused; the comment line is skipped
                ;; here only if the quotes really came off.
                ("-H -p --comment '#'" "a,b\n# skipped\n0,1\n" "a,b\n0,1\n")
                ("-H -p --quote 'always'" "a,b\n0,1\n"
                 "\"a\",\"b\"\n\"0\",\"1\"\n")
                ;; A single quote is a legal delimiter and can only be written
                ;; by wrapping it in the other kind, which is the whole reason
                ;; the two are not treated alike.
                ("-H -p --print-delimiter \"'\"" "a,b\n0,1\n" "a'b\n0'1\n")
                ;; And a single-quoted run holds a space just as a double-quoted
                ;; one does.
                ("-H -p --print-delimiter ' '" "a,b\n0,1\n" "a b\n0 1\n")))
  (let ((line (nth 0 case)) (input (nth 1 case)) (want (nth 2 case)))
    (with-temp-buffer
      (insert input)
      (csvdt--run (list (cons (point-min) (point-max))) (csvdt--split-arguments line)))
    (test-csvdt-check (format "a typed %S runs" line)
                      (with-current-buffer "*csvdt output*" (buffer-string))
                      want)))

;;; Completing one argument rather than the whole line

(with-temp-buffer
  (insert "-H -p --arr")
  (let ((capf (csvdt--complete-argument)))
    (test-csvdt-check "completion covers only the argument at point"
                      (buffer-substring (nth 0 capf) (nth 1 capf))
                      "--arr")
    ;; That the candidates really are csvdt's own options, and that there are
    ;; some.  Comparing this element with `csvdt-options' was comparing the
    ;; call with the same call, so it held just as well when the element was
    ;; nil and nothing was offered at all.
    (test-csvdt-check "candidates are the options csvdt reports"
                      (and (member "-H" (nth 2 capf))
                           (> (length (nth 2 capf)) 20)
                           t)
                      t)))

;; A comma is part of the argument, not a boundary, so completion inside one
;; does not start over at the comma.
(with-temp-buffer
  (insert "-H -p -a 1,0")
  (let ((capf (csvdt--complete-argument)))
    (test-csvdt-check "a comma does not split the argument being completed"
                      (buffer-substring (nth 0 capf) (nth 1 capf))
                      "1,0")))

;;; The prefill, selected so that typing replaces it
;;
;; Whether a keystroke really replaces it is a question for tty-test.el, which
;; can type.  What is checkable here is the state it is left in.

(with-temp-buffer
  (insert "-H -p")
  (csvdt--select-prefill)
  (test-csvdt-check "the prefill is left selected"
                    (list (region-beginning) (region-end) (point))
                    (list (point-min) (point-max) (point-max)))
  (test-csvdt-check "and the region counts as active"
                    (and (use-region-p) t) t)
  (test-csvdt-check "with the replacing hook in place"
                    (and (memq #'csvdt--replace-selection pre-command-hook) t) t))

(with-temp-buffer
  ;; Nothing prefilled, so there is nothing to select and no hook to add.
  (csvdt--select-prefill)
  (test-csvdt-check "an empty prefill selects nothing"
                    (list (use-region-p)
                          (memq #'csvdt--replace-selection pre-command-hook))
                    (list nil nil)))

(with-temp-buffer
  (insert "-H -p")
  (csvdt--select-prefill)
  (csvdt--keep-prefill 1)
  (test-csvdt-check "keeping it deactivates the region, point at the end"
                    (list (use-region-p) (point) (buffer-string))
                    (list nil (point-max) "-H -p")))

(with-temp-buffer
  (insert "-H -p")
  (goto-char (point-min))
  (csvdt--keep-prefill 1)
  (test-csvdt-check "with nothing selected it is an ordinary forward-char"
                    (point) (1+ (point-min))))

;;; Running

(with-temp-buffer
  (insert "ts,x\n2026-07-25T12:00:00+02:00,1\n")
  (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p" "-u0" "-i" "0,replace"))
  (test-csvdt-check "the source buffer is not modified"
                    (buffer-string)
                    "ts,x\n2026-07-25T12:00:00+02:00,1\n"))
(test-csvdt-check "output lands in its own buffer"
                  (with-current-buffer "*csvdt output*" (buffer-string))
                  "to_utc,x\n2026-07-25T10:00:00+00:00,1\n")

;; A region run is the same code path with a narrower range, which is the point
;; of it: trying an argument on a few rows before the whole file.
(with-temp-buffer
  (insert "ts\n2026-07-25T12:00:00Z\n2026-07-26T12:00:00Z\n2026-07-27T12:00:00Z\n")
  (goto-char (point-min))
  (forward-line 2)
  (csvdt--run (list (cons (point-min) (point))) '("-H" "-p" "-u0" "-i" "0,replace")))
(test-csvdt-check "a partial range is all that gets processed"
                  (with-current-buffer "*csvdt output*" (buffer-string))
                  "to_utc\n2026-07-25T12:00:00+00:00\n")

(let ((csvdt-executable "csvdt-that-does-not-exist"))
  (test-csvdt-check
   "a missing binary says which variable to set"
   (condition-case err
       (progn (csvdt--program) nil)
     (user-error (and (string-match-p "Cannot find" (cadr err))
                      (string-match-p "csvdt-executable" (cadr err))
                      t)))
   t))

(with-temp-buffer
  (insert "a,b\n2026-07-25T12:00:00Z,1\n")
  (test-csvdt-check
   "csvdt's own message surfaces on failure"
   (condition-case err
       (progn (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p" "-u9")) nil)
     (user-error (and (string-match-p "exited 1" (cadr err))
                      (string-match-p "out of bound" (cadr err))
                      t)))
   t))

;;; An equals-attached argument, built from the offered candidate, runs

(let ((equals-option
       (car (seq-filter (lambda (option) (string-suffix-p "=" option))
                        (csvdt-options)))))
  (test-csvdt-check "an equals candidate exists to test" (and equals-option t) t)
  (when equals-option
    (with-temp-buffer
      ;; "fix" is the mode being asked for; the option name itself still comes
      ;; from the binary rather than from this file.
      (insert "a,b,c\n0,1\n")
      (csvdt--run (list (cons (point-min) (point-max)))
                  (list "-H" "-p" (concat equals-option "fix"))))
    (test-csvdt-check "an equals-attached argument runs"
                      (with-current-buffer "*csvdt output*" (buffer-string))
                      "a,b,c\n0,1,\n")))

;;; Feeding a result back through csvdt
;;
;; The output buffer can be the source buffer, and erasing it before the
;; process had read it lost the input.

(with-temp-buffer
  (insert "a,b,c\n0,1,2\n")
  (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p" "-a" "1,0")))
(with-current-buffer "*csvdt output*"
  (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p" "--remove" "0")))
(test-csvdt-check "a result can be fed back through csvdt"
                  (with-current-buffer "*csvdt output*" (buffer-string))
                  "a,c\n0,2\n")

;;; What actually reaches csvdt on the wire
;;
;; csvdt requires UTF-8 and refuses anything else, so the encoding cannot be
;; left to the locale: under a non-UTF-8 one the default process coding would
;; hand it cp1252 bytes from a buffer Emacs had decoded perfectly.

(let ((dump (make-temp-file "csvdt-dump" nil ".sh")))
  (with-temp-file dump
    (insert "#!/bin/sh\nod -An -tx1 | tr -s ' \\n' ' '\n"))
  (set-file-modes dump #o755)
  (let ((csvdt-executable dump)
        ;; What a latin-1 locale gives you.
        (default-process-coding-system '(iso-latin-1-unix . iso-latin-1-unix)))
    (with-temp-buffer
      ;; U+00E9 by code point, so the test does not depend on how this file
      ;; itself was decoded.  UTF-8 is c3 a9; latin-1 would be e9.
      (insert "caf" (string #x00E9) "\n")
      (csvdt--run (list (cons (point-min) (point-max))) '()))
    (test-csvdt-check "non-ASCII reaches csvdt as UTF-8 whatever the locale"
                      (string-trim
                       (with-current-buffer "*csvdt output*" (buffer-string)))
                      "63 61 66 c3 a9 0a")

    ;; A Windows CSV opened in Emacs has dos line endings, which would go back
    ;; out as CRLF and put a carriage return inside csvdt's last field.
    (let ((dos (make-temp-file "csvdt-dos" nil ".csv")))
      (let ((coding-system-for-write 'utf-8-dos))
        (with-temp-file dos (insert "a,b\n0,1\n")))
      (find-file dos)
      (csvdt--run (list (cons (point-min) (point-max))) '())
      (test-csvdt-check "dos line endings are normalised on the way in"
                        (string-trim
                         (with-current-buffer "*csvdt output*" (buffer-string)))
                        "61 2c 62 0a 30 2c 31 0a")
      (kill-buffer)
      (delete-file dos)))
  (delete-file dump))

;;; Another package's overlays do not colour the next result
;;
;; Emptying a buffer collapses an overlay to zero width rather than removing
;; it, and one created to advance with text inserted at its end then grows
;; over whatever replaces it.  A line highlighted for having been banked in
;; the output buffer coloured the entire next result that way.

(with-temp-buffer
  (insert "a,b
1,2
")
  (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p")))
(with-current-buffer "*csvdt output*"
  ;; Rear-advancing and not evaporating, as a banking package's overlay is.
  (let ((overlay (make-overlay (point-min)
                               (save-excursion (goto-char (point-min))
                                               (line-beginning-position 2))
                               nil nil t)))
    (overlay-put overlay 'face 'secondary-selection)))
(with-temp-buffer
  (insert "a,b
3,4
")
  (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p")))
(with-current-buffer "*csvdt output*"
  (test-csvdt-check "a stale overlay does not survive into the next result"
                    (overlays-in (point-min) (point-max)) nil)
  (test-csvdt-check "and the result itself is the new one"
                    (buffer-string) "a,b\n3,4\n"))

;; Nor do text properties, whatever the source carried.
(with-temp-buffer
  (insert (propertize "a,b\n1,2\n" 'face 'bold))
  (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p")))
(with-current-buffer "*csvdt output*"
  (test-csvdt-check "output text carries no properties"
                    (text-properties-at (point-min)) nil))

;;; A misconfigured output mode does not lose the run

(let ((csvdt-output-mode 'csvdt-no-such-mode))
  (with-temp-buffer
    (insert "a,b\n0,1\n")
    (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p")))
  (test-csvdt-check "an unavailable output mode still shows the output"
                    (with-current-buffer "*csvdt output*" (buffer-string))
                    "a,b\n0,1\n")
  (test-csvdt-check "and leaves the buffer plain"
                    (with-current-buffer "*csvdt output*" major-mode)
                    'fundamental-mode))

;;; An empty buffer

(with-temp-buffer
  (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p")))
(test-csvdt-check "an empty buffer produces empty output"
                  (with-current-buffer "*csvdt output*" (buffer-string))
                  "")

;;; Help

(let ((help (csvdt--help-text)))
  (test-csvdt-check "the help text is csvdt's own"
                    (and (string-match-p "KNOWN LIMITS" help)
                         (string-match-p "IANA tzdata" help)
                         t)
                    t))


;;; The commands themselves, through their interactive forms
;;
;; Exercising `csvdt--run' alone is what let a broken argument reader ship: it
;; took an already-split list, so the path a keystroke actually travels was
;; never run.  These go in at the top of each command instead, with only the
;; minibuffer read stood in for.

(defmacro test-csvdt-typed (line &rest body)
  "Evaluate BODY with LINE standing in for what is typed at the prompt."
  (declare (indent 1))
  `(cl-letf (((symbol-function 'read-from-minibuffer) (lambda (&rest _) ,line)))
     ,@body))

(defconst test-csvdt-headed
  "ts,x\n2026-07-25T12:00:00+02:00,a\n2026-07-26T12:00:00+02:00,b\n"
  "A header and two rows, so that what became of the header is visible.")

(defconst test-csvdt-converted
  "to_utc,x\n2026-07-25T10:00:00+00:00,a\n2026-07-26T10:00:00+00:00,b\n"
  "All of `test-csvdt-headed' converted, header intact.")

(defconst test-csvdt-line "-H -p -u0 -i 0,replace"
  "A command line whose result shows whether the header was understood.")

(defmacro current-message-of (&rest body)
  "Evaluate BODY and return the text it passes to `message'.
`current-message' is nil under --batch, where the echo area is stderr."
  `(let (captured)
     (cl-letf (((symbol-function 'message)
                (lambda (format &rest args)
                  (setq captured (apply #'format format args)))))
       ,@body)
     captured))

(defun test-csvdt-output ()
  "Return what the last run produced."
  (with-current-buffer "*csvdt output*" (buffer-string)))

(defun test-csvdt-select-rows ()
  "Select everything below the header of the current buffer."
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (forward-line 1)
  (push-mark (point) t t)
  (goto-char (point-max))
  (activate-mark))

(with-temp-buffer
  (insert test-csvdt-headed)
  (test-csvdt-typed test-csvdt-line (call-interactively #'csvdt-run-buffer))
  (test-csvdt-check "csvdt-run-buffer covers the whole buffer"
                    (test-csvdt-output) test-csvdt-converted))

(with-temp-buffer
  (insert test-csvdt-headed)
  (test-csvdt-select-rows)
  (test-csvdt-typed test-csvdt-line (call-interactively #'csvdt-run-region))
  ;; Both rows converted and the real column name kept: the header reached
  ;; csvdt even though the region began below it.
  (test-csvdt-check "csvdt-run-region carries the header through"
                    (test-csvdt-output) test-csvdt-converted))

(with-temp-buffer
  (insert test-csvdt-headed)
  (test-csvdt-typed test-csvdt-line (call-interactively #'csvdt-run-dwim))
  (test-csvdt-check "csvdt-run-dwim with no region takes the buffer"
                    (test-csvdt-output) test-csvdt-converted))

(with-temp-buffer
  (insert test-csvdt-headed)
  (test-csvdt-select-rows)
  (test-csvdt-typed test-csvdt-line (call-interactively #'csvdt-run-dwim))
  (test-csvdt-check "csvdt-run-dwim with a region takes the region"
                    (test-csvdt-output) test-csvdt-converted))

;; The region really is only the region: dropping a row from it drops it from
;; the output, so the four checks above are not all quietly running the buffer.
(with-temp-buffer
  (insert test-csvdt-headed)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (forward-line 1)
  (push-mark (point) t t)
  (forward-line 1)
  (activate-mark)
  (test-csvdt-typed test-csvdt-line (call-interactively #'csvdt-run-dwim))
  (test-csvdt-check "and stops where the region stops"
                    (test-csvdt-output)
                    "to_utc,x\n2026-07-25T10:00:00+00:00,a\n"))

;; A command line typed with a comma in it, all the way through the reader.
(with-temp-buffer
  (insert "a,b,c\n0,1,2\n")
  (test-csvdt-typed "-H -p -a 1,0" (call-interactively #'csvdt-run-buffer))
  (test-csvdt-check "a comma-bearing line survives the interactive path"
                    (test-csvdt-output) "b,a,c\n1,0,2\n"))

;;; The options named here, and the only ones

(test-csvdt-check
 "every configured header option is one csvdt actually reports"
 (seq-remove (lambda (option) (member option (csvdt-options)))
             csvdt-header-options)
 nil)

;; The same for the two the record-boundary scan needs: which character
;; quotes a field, and which one separates fields, decide where a record
;; begins, and a rename would leave the scan reading the wrong dialect
;; without a word.
(test-csvdt-check
 "every configured single-quote option is one csvdt actually reports"
 (seq-remove (lambda (option) (member option (csvdt-options)))
             csvdt-single-quote-options)
 nil)

(test-csvdt-check
 "every configured read-delimiter option is one csvdt actually reports"
 (seq-remove (lambda (option) (member option (csvdt-options)))
             csvdt-read-delimiter-options)
 nil)

;; How many of them there are is written down in three places outside this
;; file -- csvdt.el's own commentary, the README, and the description of the
;; emacs-csvdt subpackage in the rpm spec -- and all three said the wrong
;; number for as long as there were more than one.  The checks above hold
;; each list to options csvdt still reports, and the check below holds the
;; file to naming no others, but neither notices a fourth list being added
;; beside them, which is what the prose would then be wrong about.
;;
;; So the count is pinned.  A fourth is not forbidden: it fails here, and the
;; sentence in each of those three places is what wants writing before this
;; number is raised.
(test-csvdt-check
 "three defcustoms name a csvdt option, as the prose about them says"
 (length (seq-filter
          (lambda (symbol)
            ;; A list of strings, one of which opens with a dash. The other
            ;; defcustoms hold a string, a symbol, t or nil, and a symbol is
            ;; not a sequence to look through -- hence the `listp' first.
            (let ((value (symbol-value symbol)))
              (and (listp value)
                   (seq-some (lambda (item)
                               (and (stringp item)
                                    (string-prefix-p "-" item)))
                             value))))
          '(csvdt-header-options
            csvdt-single-quote-options
            csvdt-read-delimiter-options
            csvdt-default-arguments
            csvdt-output-mode
            csvdt-region-carries-header
            csvdt-banked-lines-function
            csvdt-executable)))
 3)

(dolist (case '(("-H" t)
                ("--has-header" t)
                ;; clap lets flags be written together.
                ("-Hp" t)
                ("-pH" t)
                ;; A short option that takes a value consumes the rest of the
                ;; argument, so an H after one is part of that value, not a flag.
                ("-o0,+02:00" nil)
                ("-i0,replace" nil)
                ("-f=fix" nil)
                ("-p" nil)
                ("--print-header" nil)
                ("--remove" nil)))
  (test-csvdt-check (format "header requested by %S" (car case))
                    (and (csvdt--header-requested-p (list (car case))) t)
                    (cadr case)))

;;; A region below the header is sent with the header in front of it

(let ((content "ts,x\n2026-07-25T12:00:00+02:00,a\n2026-07-26T12:00:00+02:00,b\n")
      (arguments '("-H" "-p" "-u0" "-i" "0,replace")))

  ;; The rows below the header, selected on their own.
  (with-temp-buffer
    (insert content)
    (goto-char (point-min))
    (forward-line 1)
    (csvdt--run (list (cons (point) (point-max))) arguments))
  (test-csvdt-check "a region below the header keeps the real header"
                    (with-current-buffer "*csvdt output*" (buffer-string))
                    (concat "to_utc,x\n"
                            "2026-07-25T10:00:00+00:00,a\n"
                            "2026-07-26T10:00:00+00:00,b\n"))

  ;; Without it, csvdt takes the first selected row for a header: it is exempt
  ;; from the conversion, and -i 0,replace puts the generated name where its
  ;; timestamp was, so the row is gone.
  (with-temp-buffer
    (insert content)
    (goto-char (point-min))
    (forward-line 1)
    (let ((csvdt-region-carries-header nil))
      (csvdt--run (list (cons (point) (point-max))) arguments)))
  (test-csvdt-check "turning it off loses that row, as it always did"
                    (with-current-buffer "*csvdt output*" (buffer-string))
                    "to_utc,a\n2026-07-26T10:00:00+00:00,b\n")

  ;; A whole-buffer run already starts at the header, so nothing is prepended.
  (with-temp-buffer
    (insert content)
    (csvdt--run (list (cons (point-min) (point-max))) arguments))
  (test-csvdt-check "a whole-buffer run is unchanged"
                    (with-current-buffer "*csvdt output*" (buffer-string))
                    (concat "to_utc,x\n"
                            "2026-07-25T10:00:00+00:00,a\n"
                            "2026-07-26T10:00:00+00:00,b\n"))

  ;; Without a header option there is no header to carry, and prepending one
  ;; would add a row that is not in the selection.
  (with-temp-buffer
    (insert content)
    (goto-char (point-min))
    (forward-line 1)
    (csvdt--run (list (cons (point) (point-max))) '("-u0" "-i" "0,replace")))
  (test-csvdt-check "no header option means nothing is prepended"
                    (with-current-buffer "*csvdt output*" (buffer-string))
                    (concat "2026-07-25T10:00:00+00:00,a\n"
                            "2026-07-26T10:00:00+00:00,b\n")))

;; A region starting inside the header line is left alone: it already carries
;; part of one, and guessing what was meant would be worse than not.
(with-temp-buffer
  (insert "ts,x\n2026-07-25T12:00:00+02:00,a\n")
  (goto-char (+ (point-min) 2))
  (test-csvdt-check "a region inside the header gets nothing prepended"
                    (csvdt--header-line (point) '("-H" "-p"))
                    nil))

;;; The package is one package.el can install

;; Every convention of an installable package was here -- lexical binding, an
;; Author, a URL, Package-Requires, an SPDX line, a Commentary, eight
;; `;;;###autoload' cookies and a provide -- except the Version header
;; package.el actually requires.  Without it `package-buffer-info' refuses the
;; file outright, so `M-x package-install-file' could not install it and the
;; autoload cookies were writing autoloads for a path nobody could reach.
(let ((info (with-temp-buffer
              (insert-file-contents
               (expand-file-name "csvdt.el" test-csvdt-directory))
              (condition-case err
                  (progn (require 'package) (package-buffer-info))
                (error (error-message-string err))))))
  (test-csvdt-check "package.el can read csvdt.el as a package"
                    (if (stringp info) info (and (package-desc-p info) t))
                    t))

;; And the version it carries is csvdt's own.  The README's reason for having
;; no separate version -- "the copy next to a given csvdt is the one that
;; matches it" -- is the reason this header has to track the crate rather than
;; drift on its own, the same bargain the RPM spec's Version is held to in CI.
(let* ((crate (with-temp-buffer
                (insert-file-contents
                 (expand-file-name "../Cargo.toml" test-csvdt-directory))
                (goto-char (point-min))
                (and (re-search-forward "^version = \"\\([^\"]+\\)\"" nil t)
                     (match-string 1))))
       (package (with-temp-buffer
                  (insert-file-contents
                   (expand-file-name "csvdt.el" test-csvdt-directory))
                  (goto-char (point-min))
                  (and (re-search-forward "^;; Version: *\\(.+\\)$" nil t)
                       (string-trim (match-string 1))))))
  (test-csvdt-check "csvdt.el's Version header is the crate's version"
                    (list crate package)
                    (list crate crate)))

;;; No option name is written down in the package, beyond those

(let ((source (with-temp-buffer
                (insert-file-contents
                 (expand-file-name "csvdt.el" test-csvdt-directory))
                (buffer-string)))
      (leaked '()))
  (dolist (option (csvdt-options))
    ;; The three the package calls to ask its questions are necessarily
    ;; named, and so are the few a defcustom holds -- those are covered by
    ;; the staleness checks above.  Every other option must be absent, or
    ;; it could fall out of date.
    ;; As a standalone token: a private function name like
    ;; `csvdt--split-arguments' contains "--split" without mentioning an
    ;; option, so the character before the dashes must not be part of a
    ;; symbol.
    (when (and (string-prefix-p "--" option)
               (not (member option '("--help" "--version" "--list-options")))
               (not (member option csvdt-header-options))
               (not (member option csvdt-single-quote-options))
               (not (member option csvdt-read-delimiter-options))
               (string-match-p (concat "\\(?:^\\|[^A-Za-z0-9]\\)"
                                       (regexp-quote option) "\\(?:$\\|[^A-Za-z0-9-]\\)")
                               source))
      (push option leaked)))
  (test-csvdt-check "no csvdt option name appears in csvdt.el" leaked nil))

;;; Banked lines
;;
;; Banking -- picking out lines here and there rather than one stretch -- is
;; another package's job.  What is checked here is that this one asks for them,
;; puts them in order, snaps them to whole lines and sends them as one file.
;; A list standing in for the banking package is all that needs faking.

(defconst test-csvdt-rows
  (concat "ts,x\n"
          "2026-07-25T12:00:00+02:00,a\n"
          "2026-07-25T13:00:00+02:00,b\n"
          "2026-07-25T14:00:00+02:00,c\n"
          "2026-07-25T15:00:00+02:00,d\n")
  "A header and four rows, one per line, so a bank is easy to describe.")

;;; Selection shapes
;;
;; Whatever sets the region -- Emacs itself or a modal editor on top of it --
;; reaches this package as `use-region-p\=' and the region bounds, so what
;; matters is the shape it leaves behind rather than which package left it.

(require 'rect)

(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  ;; A rectangle spans columns, but a CSV record is a whole line: the rows it
  ;; touches go out entire.  Picking columns is csvdt's own job, via --remove
  ;; or --arrange.
  (goto-char (point-min))
  (forward-line 1)
  (forward-char 2)
  (push-mark (point) t t)
  (rectangle-mark-mode 1)
  (forward-line 2)
  (forward-char 4)
  (activate-mark)
  (test-csvdt-check "a rectangle sends the whole rows it spans"
                    (let ((range (csvdt--whole-lines (region-beginning)
                                                     (region-end))))
                      (buffer-substring-no-properties (car range) (cdr range)))
                    (concat "2026-07-25T12:00:00+02:00,a\n"
                            "2026-07-25T13:00:00+02:00,b\n"
                            "2026-07-25T14:00:00+02:00,c\n")))


;;; A record is not always a line
;;
;; A quoted field may hold newlines -- an address, a notes column, an
;; embedded document -- and then one record spans several lines.  Widening
;; a selection to whole lines is not enough there: a range starting inside
;; such a field is the tail of a record, and it went out as though it were
;; a record of its own.  That converted cleanly and reported success, with
;; a field nobody wrote.

(defconst test-csvdt-spanning
  (concat "h,when\n"
          "\"x\ny\",2026-07-25T12:00:00+02:00\n"
          "row2,2026-07-25T13:00:00+02:00\n")
  "A header and two records, the first of which covers two lines.")

(with-temp-buffer
  (insert test-csvdt-spanning)
  ;; Line 3 is "y\",2026-..." -- the back half of record one.
  (let* ((beg (progn (goto-char (point-min)) (forward-line 2) (point)))
         (end (progn (forward-line 1) (point)))
         (range (csvdt--whole-records beg end ?\" ?,)))
    (test-csvdt-check
     "a range opening inside a quoted field is widened back to the record"
     (buffer-substring-no-properties (car range) (cdr range))
     "\"x\ny\",2026-07-25T12:00:00+02:00\n"))

  ;; And a range that already sits on record boundaries is left alone, which
  ;; is every range in a file whose fields hold no newlines.
  (let* ((beg (progn (goto-char (point-min)) (forward-line 3) (point)))
         (end (point-max))
         (range (csvdt--whole-records beg end ?\" ?,)))
    (test-csvdt-check "a range already on record boundaries is untouched"
                      range (cons beg end))))

(with-temp-buffer
  ;; Two banked lines inside one record that spans several.  They do not
  ;; touch, so nothing merges them when the list is normalised -- and then
  ;; each widens to the same record, which went out once per range: a
  ;; duplicate row, valid CSV, clean-looking, and nothing in a count to
  ;; question it.
  (insert "h,when\n\"x\ny\nz\",2026-07-25T12:00:00+02:00\nrow2,2026-07-26T12:00:00+02:00\n")
  (let* ((line (lambda (n)
                 (goto-char (point-min))
                 (forward-line n)
                 (cons (point) (progn (forward-line 1) (point)))))
         (ranges (list (funcall line 1) (funcall line 3)))
         (widened (mapcar (lambda (r)
                            (csvdt--whole-records (car r) (cdr r) ?\" ?,))
                          ranges)))
    (test-csvdt-check "two banked lines in one record widen to the same record"
                      (equal (car widened) (cadr widened))
                      t)
    (test-csvdt-check "and it is sent once rather than once per range"
                      (length (csvdt--drop-covered widened))
                      1))

  ;; Ranges that share nothing are all kept, in the order they came: buffer
  ;; order is asked of the caller rather than imposed here.
  (test-csvdt-check "ranges covering different records are all kept"
                    (csvdt--drop-covered '((30 . 40) (10 . 20)))
                    '((30 . 40) (10 . 20))))

(with-temp-buffer
  ;; The header itself can span lines, and taking one of them handed csvdt a
  ;; fragment that opened a quote and never closed it.
  (insert "\"a\nb\",when\nrow1,2026-07-25T12:00:00+02:00\n")
  (goto-char (point-min))
  (forward-line 2)
  (test-csvdt-check "a header spanning two lines is carried whole"
                    (csvdt--header-line (point) '("-H" "-p"))
                    "\"a\nb\",when\n"))

(with-temp-buffer
  ;; A quote that does not begin a field is data, not an opener: the reader
  ;; takes a"b as three characters, and reading it as the start of a quoted
  ;; run would drag the following lines into a record that never held them.
  (insert "a\"b,c\nd,e\n")
  (test-csvdt-check "a quote inside an unquoted field opens nothing"
                    (csvdt--inside-quoted-field-p (point-max) ?\" ?,)
                    nil))

(with-temp-buffer
  ;; A doubled quote is one escaped quote and leaves the run open.
  (insert "\"a\"\"b\nc\",d\n")
  (goto-char (point-min))
  (forward-line 1)
  (test-csvdt-check "a doubled quote does not close the run"
                    (csvdt--inside-quoted-field-p (point) ?\" ?,)
                    t))

(dolist (case '((nil ?\" ?,)
                (("--single-quote") ?' ?,)
                (("--read-delimiter" ";") ?\" ?\;)
                (("--read-delimiter=;") ?\" ?\;)
                ;; Past "--" everything is positional to clap, so an
                ;; argument spelled like an option there is a filename.
                (("--" "--single-quote") ?\" ?,)))
  (test-csvdt-check (format "%S reads as the dialect it names" (car case))
                    (csvdt--dialect (car case))
                    (cons (nth 1 case) (nth 2 case))))

(defun test-csvdt-line-bounds (n)
  "Return the (BEG . END) of line N, newline included."
  (save-excursion
    (goto-char (point-min))
    (forward-line (1- n))
    (cons (point) (min (point-max) (line-beginning-position 2)))))

(defmacro test-csvdt-banking (lines &rest body)
  "Evaluate BODY with LINES standing in for a banking package's answer."
  (declare (indent 1))
  `(let ((csvdt-banked-lines-function
          (lambda () (mapcar #'test-csvdt-line-bounds ,lines))))
     ,@body))

;; Lines picked out with gaps between them: what was skipped must not appear,
;; and the header must still reach csvdt though it was never banked.
(with-temp-buffer
  (insert test-csvdt-rows)
  (test-csvdt-banking '(2 4)
    (test-csvdt-check "banked lines are two separate stretches"
                      (length (csvdt--normalise-ranges (csvdt--banked-lines))) 2)
    (test-csvdt-typed "-H -p -u0 -i 0,replace"
      (call-interactively #'csvdt-run-banked))
    (test-csvdt-check "a run sends only the banked lines"
                      (test-csvdt-output)
                      (concat "to_utc,x\n"
                              "2026-07-25T10:00:00+00:00,a\n"
                              "2026-07-25T12:00:00+00:00,c\n"))))

;; Reported out of order, and reported twice.
(with-temp-buffer
  (insert test-csvdt-rows)
  (test-csvdt-banking '(4 2 4)
    (test-csvdt-check "ranges are ordered and de-duplicated"
                      (length (csvdt--normalise-ranges (csvdt--banked-lines))) 2)
    (test-csvdt-typed "-H -p" (call-interactively #'csvdt-run-banked))
    (test-csvdt-check "and reach csvdt in buffer order, once each"
                      (test-csvdt-output)
                      (concat "ts,x\n"
                              "2026-07-25T12:00:00+02:00,a\n"
                              "2026-07-25T14:00:00+02:00,c\n"))))

;; Lines that touch are one stretch, not two abutting ones.
(with-temp-buffer
  (insert test-csvdt-rows)
  (test-csvdt-banking '(2 3)
    (test-csvdt-check "adjacent lines merge into one range"
                      (length (csvdt--normalise-ranges (csvdt--banked-lines))) 1)))

;; Half a line is half a record, so a partial range is widened.
(with-temp-buffer
  (insert test-csvdt-rows)
  (let ((csvdt-banked-lines-function
         ;; Two characters out of the middle of the header line.
         (lambda () (list (cons (+ (point-min) 1) (+ (point-min) 3))))))
    (test-csvdt-check "a partial range is widened to whole lines"
                      (let ((range (car (csvdt--normalise-ranges (csvdt--banked-lines)))))
                        (buffer-substring-no-properties (car range) (cdr range)))
                      "ts,x\n")))

;; Overlays are accepted as well as conses, since that is how a banking
;; package most likely holds them -- and a dead one is dropped rather than
;; signalling.
(with-temp-buffer
  (insert test-csvdt-rows)
  (let* ((live (let ((b (test-csvdt-line-bounds 2)))
                 (make-overlay (car b) (cdr b))))
         (dead (let ((b (test-csvdt-line-bounds 3)))
                 (make-overlay (car b) (cdr b))))
         (csvdt-banked-lines-function (lambda () (list live dead))))
    (delete-overlay dead)
    (test-csvdt-check "overlays are accepted, and a dead one is dropped"
                      (length (csvdt--normalise-ranges (csvdt--banked-lines))) 1)
    (test-csvdt-typed "-H -p" (call-interactively #'csvdt-run-banked))
    (test-csvdt-check "the live overlay is what runs"
                      (test-csvdt-output)
                      "ts,x\n2026-07-25T12:00:00+02:00,a\n")))

;; What a run would cover, for when the output is not what was expected.  The
;; two ways a bank goes unseen read differently, which is the point of it.
(with-temp-buffer
  (insert test-csvdt-rows)
  (test-csvdt-check "unset says so, and reports the whole buffer"
                    (let ((csvdt-banked-lines-function nil))
                      (current-message-of (csvdt-describe-run)))
                    (concat "csvdt would run over the whole buffer, 5 lines.  "
                            "Banked: nothing, `csvdt-banked-lines-function' "
                            "is unset.  Region: none."))
  (test-csvdt-check "set but reporting nothing reads differently"
                    (let ((csvdt-banked-lines-function (lambda () nil)))
                      (current-message-of (csvdt-describe-run)))
                    (concat "csvdt would run over the whole buffer, 5 lines.  "
                            "Banked: nothing reported -- lines that look "
                            "banked may not be.  Region: none."))
  (test-csvdt-check "and banked lines are counted"
                    (test-csvdt-banking '(2 4)
                      (current-message-of (csvdt-describe-run)))
                    (concat "csvdt would run over 2 lines in 2 stretches.  "
                            "Banked: 2 lines in 2 stretches.  Region: none.")))

;; Counted the way a run counts, which since records may span lines means
;; whole records.  Stopping at the normalising described two banked lines
;; inside one such record as two stretches of one line, where the run makes
;; one stretch of three -- and this is the command reached for when a run
;; covered something unexpected, so being wrong exactly where the run is
;; surprising is the one thing it cannot afford.
(with-temp-buffer
  (insert "h,when\n\"x\ny\nz\",2026-07-25T12:00:00+02:00\n"
          "row2,2026-07-26T12:00:00+02:00\n")
  (let* ((bounds (lambda (n)
                   (goto-char (point-min))
                   (forward-line n)
                   (cons (point) (progn (forward-line 1) (point)))))
         (banked (list (funcall bounds 1) (funcall bounds 3))))
    (setq-local csvdt-banked-lines-function (lambda () banked))
    (test-csvdt-check "describe counts whole records, not the lines banked"
                      (current-message-of (csvdt-describe-run))
                      (concat "csvdt would run over 3 lines in 1 stretch.  "
                              "Banked: 3 lines in 1 stretch.  Region: none."))
    ;; And what it says is what the run then does.
    (test-csvdt-check "and the run covers exactly that"
                      (progn (csvdt-run-banked '())
                             (test-csvdt-output))
                      "\"x\ny\nz\",2026-07-25T12:00:00+02:00\n")))

;; The overlay helper, which is how a banking package is usually plugged in.
(with-temp-buffer
  (insert test-csvdt-rows)
  (let* ((banked (let ((b (test-csvdt-line-bounds 3)))
                   (make-overlay (car b) (cdr b))))
         (other (let ((b (test-csvdt-line-bounds 4)))
                  (make-overlay (car b) (cdr b))))
         (csvdt-banked-lines-function
          (csvdt-banked-lines-from-overlays 'test-csvdt-marker)))
    (overlay-put banked 'test-csvdt-marker t)
    ;; An overlay from something else in the buffer must not be picked up.
    (overlay-put other 'face 'bold)
    (test-csvdt-check "the overlay helper finds only overlays with the property"
                      (length (csvdt--normalise-ranges (csvdt--banked-lines))) 1)
    (test-csvdt-typed "-H -p" (call-interactively #'csvdt-run-banked))
    (test-csvdt-check "and runs exactly those lines"
                      (test-csvdt-output)
                      "ts,x\n2026-07-25T13:00:00+02:00,b\n")))

;; What a banking package reports is not always well formed, and a range that
;; is wrong in a way this package cannot see is worse than one it refuses.
(with-temp-buffer
  (insert test-csvdt-rows)

  ;; A pair is two ends, not a direction.  Taken as given, a reversed one
  ;; widened to nothing and the lines it named went silently missing.
  (let ((csvdt-banked-lines-function
         (lambda () (list (cons (cdr (test-csvdt-line-bounds 3))
                                (car (test-csvdt-line-bounds 2)))))))
    (test-csvdt-check "a reversed pair names the lines between its ends"
                      (csvdt--normalise-ranges (csvdt--banked-lines))
                      (list (cons (car (test-csvdt-line-bounds 2))
                                  (cdr (test-csvdt-line-bounds 3))))))

  ;; Positions past the end clamp to nothing, which is not a stretch.
  (let ((csvdt-banked-lines-function
         (lambda () (list (cons (+ (point-max) 100) (+ (point-max) 200))))))
    (test-csvdt-check "a pair naming nothing is dropped, not counted"
                      (csvdt--normalise-ranges (csvdt--banked-lines)) nil))

  ;; nil is what filtering a list of one's own leaves behind.
  (let ((csvdt-banked-lines-function (lambda () (list nil))))
    (test-csvdt-check "nil in the list is tolerated"
                      (csvdt--normalise-ranges (csvdt--banked-lines)) nil))

  ;; Anything else is a mistake worth naming, rather than a raw
  ;; wrong-type-argument from somewhere inside.
  (let ((csvdt-banked-lines-function (lambda () (list 5))))
    (test-csvdt-check "something that is neither pair nor overlay is named"
                      (condition-case err
                          (progn (csvdt--normalise-ranges (csvdt--banked-lines)) nil)
                        (user-error
                         (and (string-match-p "csvdt-banked-lines-function"
                                              (cadr err))
                              (string-match-p "neither" (cadr err))
                              t)))
                      t))

  ;; An overlay from elsewhere describes positions in that buffer, which mean
  ;; something else here -- and would be clamped into range rather than
  ;; refused, so the run would quietly cover lines nobody picked out.
  (let ((elsewhere (generate-new-buffer "csvdt-elsewhere")))
    (with-current-buffer elsewhere (insert "not this buffer at all\n"))
    (let* ((foreign (with-current-buffer elsewhere
                      (make-overlay (point-min) (point-max))))
           (csvdt-banked-lines-function (lambda () (list foreign))))
      (test-csvdt-check "an overlay from another buffer is refused"
                        (csvdt--normalise-ranges (csvdt--banked-lines)) nil))
    (kill-buffer elsewhere)))

;; Banked lines and the region are both used: the region is usually the piece
;; not banked yet, and dropping either would run over less than was picked out.
(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (forward-line 3)
  (push-mark (point) t t)
  (goto-char (point-max))
  (activate-mark)
  (test-csvdt-banking '(2)
    (test-csvdt-typed "-H -p" (call-interactively #'csvdt-run-dwim))
    (test-csvdt-check "dwim runs the banked lines and the region together"
                      (test-csvdt-output)
                      (concat "ts,x\n"
                              "2026-07-25T12:00:00+02:00,a\n"   ; banked
                              "2026-07-25T14:00:00+02:00,c\n"   ; region
                              "2026-07-25T15:00:00+02:00,d\n"))))

;; A region over a line that is already banked must not send it twice.
(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (forward-line 1)
  (push-mark (point) t t)
  (forward-line 2)
  (activate-mark)
  (test-csvdt-banking '(2)
    (test-csvdt-typed "-H -p" (call-interactively #'csvdt-run-dwim))
    (test-csvdt-check "a region overlapping a banked line does not duplicate it"
                      (test-csvdt-output)
                      (concat "ts,x\n"
                              "2026-07-25T12:00:00+02:00,a\n"
                              "2026-07-25T13:00:00+02:00,b\n"))))

;; Banked lines with no region are unchanged.
(with-temp-buffer
  (insert test-csvdt-rows)
  (test-csvdt-banking '(3)
    (test-csvdt-typed "-H -p" (call-interactively #'csvdt-run-dwim))
    (test-csvdt-check "banked lines alone still run alone"
                      (test-csvdt-output)
                      "ts,x\n2026-07-25T13:00:00+02:00,b\n")))

;; A region with nothing banked is unchanged.
(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (forward-line 2)
  (push-mark (point) t t)
  (forward-line 1)
  (activate-mark)
  (test-csvdt-typed "-H -p" (call-interactively #'csvdt-run-dwim))
  (test-csvdt-check "a region with nothing banked still runs alone"
                    (test-csvdt-output)
                    "ts,x\n2026-07-25T13:00:00+02:00,b\n"))

;; With nothing banking, everything behaves as it did before.
(with-temp-buffer
  (insert test-csvdt-rows)
  (test-csvdt-check "no banking function means no banked ranges"
                    (csvdt--normalise-ranges (csvdt--banked-lines)) nil)
  (test-csvdt-typed "-H -p" (call-interactively #'csvdt-run-dwim))
  (test-csvdt-check "and dwim is left alone"
                    (test-csvdt-output) test-csvdt-rows)
  (test-csvdt-check "running banked lines says there are none"
                    (condition-case err
                        (progn (csvdt-run-banked '("-H" "-p")) nil)
                      (user-error (and (string-match-p "No banked lines" (cadr err))
                                       t)))
                    t))

;; A stretch that does not end in a newline must not run into the one sent
;; after it.  No command can arrange that: the ranges are normalised into
;; buffer order first, so the only stretch that can end without a newline --
;; the one reaching the end of the buffer -- is always the last, with nothing
;; after it to be joined onto and csvdt supplying the terminator itself.  The
;; guard is `csvdt--run's, and buffer order is something its docstring asks
;; for rather than something it imposes, so that is where the question is put:
;; the newline-less line first, the line above it second.  Without the guard
;; the two arrive as "3,41,2", one row made out of both.
(with-temp-buffer
  (insert "a,b\n1,2\n3,4")
  ;; No header option, so nothing is prepended and the two lines are all that
  ;; comes back -- which is what makes a join visible if there is one.
  (csvdt--run (list (test-csvdt-line-bounds 3) (test-csvdt-line-bounds 2)) '())
  (test-csvdt-check "a stretch without a final newline does not join the next"
                    (test-csvdt-output) "3,4\n1,2\n"))

;; And through the command, where such a stretch is always the last one: the
;; line comes back as a record of its own, neither lost for want of a
;; terminator nor run together with the line above it.
(with-temp-buffer
  (insert "a,b\n1,2\n3,4")
  (test-csvdt-banking '(2 3)
    (test-csvdt-typed "" (call-interactively #'csvdt-run-banked))
    (test-csvdt-check "a newline-less last line is a record of its own"
                      (test-csvdt-output) "1,2\n3,4\n")))

;;; A region need not be one stretch
;;
;; Emacs has no notion of banked lines, but it does have a non-contiguous
;; region: `region-bounds' reports one (BEG . END) per piece, and a package
;; defines its own selection by hooking `region-extract-function'.  Reading
;; that means such a selection needs no configuration here at all.

(with-temp-buffer
  (insert test-csvdt-rows)
  (test-csvdt-check "no region means no ranges" (csvdt--region-ranges) nil))

(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (forward-line 1)
  (push-mark (point) t t)
  (forward-line 1)
  (activate-mark)
  (test-csvdt-check "an ordinary region is one range"
                    (length (csvdt--region-ranges)) 1))

(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (forward-line 1)
  (forward-char 2)
  (push-mark (point) t t)
  (rectangle-mark-mode 1)
  (forward-line 2)
  (forward-char 4)
  (activate-mark)
  ;; A rectangle is one piece per line, not a single stretch.
  (test-csvdt-check "a rectangle is one range per line"
                    (length (csvdt--region-ranges)) 3))

;; The region answers what is selected now, so releasing the mark takes its
;; pieces with it.  That is the whole reason a bank needs an accessor of its
;; own: a bank has to survive exactly this, or it could not accumulate.
(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (forward-line 1)
  (forward-char 2)
  (push-mark (point) t t)
  (rectangle-mark-mode 1)
  (forward-line 2)
  (forward-char 4)
  (activate-mark)
  (test-csvdt-check "a non-contiguous region has pieces while active"
                    (length (csvdt--region-ranges)) 3)
  (deactivate-mark)
  (test-csvdt-check "and none once the mark is released"
                    (csvdt--region-ranges) nil))

;; A selection defined the native way, by a package that hooks
;; `region-extract-function'.  Nothing about csvdt is configured.
(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (forward-line 1)
  (push-mark (point) t t)
  (goto-char (point-max))
  (activate-mark)
  (setq-local region-extract-function
              (lambda (method)
                (if (eq method 'bounds)
                    (list (test-csvdt-line-bounds 2) (test-csvdt-line-bounds 4))
                  (buffer-substring (region-beginning) (region-end)))))
  (test-csvdt-check "a non-contiguous region is read as its pieces"
                    (length (csvdt--region-ranges)) 2)
  ;; `csvdt-banked-lines-function' is left at its nil default throughout this
  ;; block, which is what makes the run below the region's doing alone.  Said
  ;; here rather than checked: nothing has bound it, so it is a precondition
  ;; of the case and not something any code path could get wrong.
  (test-csvdt-typed "-H -p -u0 -i 0,replace"
    (call-interactively #'csvdt-run-dwim))
  ;; Row 3 lies between the two pieces and must not be swept up, which taking
  ;; the region's two ends instead of its pieces would have done.
  (test-csvdt-check "the gap between the pieces is not swept up"
                    (test-csvdt-output)
                    (concat "to_utc,x\n"
                            "2026-07-25T10:00:00+00:00,a\n"
                            "2026-07-25T12:00:00+00:00,c\n")))

;;; A region's pieces are normalised, as the banked ones always were
;;
;; `csvdt--run' asks for ranges in buffer order.  `region-bounds' reports them
;; that way, but a package hooking `region-extract-function' defines its own
;; selection and is under no such obligation: its pieces may arrive in any
;; order, overlapping, or naming nothing.  Only the banked path normalised, so
;; the region path handed them straight on -- out of order, and a line covered
;; by two pieces sent twice.

(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (push-mark (point) t t)
  (goto-char (point-max))
  (activate-mark)
  ;; Row 3 first, then rows 1-2, then row 2 again.
  (setq-local region-extract-function
              (lambda (method)
                (if (eq method 'bounds)
                    (list (test-csvdt-line-bounds 4)
                          (cons (car (test-csvdt-line-bounds 2))
                                (cdr (test-csvdt-line-bounds 3)))
                          (test-csvdt-line-bounds 3))
                  (buffer-substring (region-beginning) (region-end)))))
  (test-csvdt-check "the pieces arrive unsorted and overlapping"
                    (length (csvdt--region-ranges)) 3)
  (test-csvdt-typed "-u0 -i 0,replace"
    (call-interactively #'csvdt-run-region))
  ;; Buffer order, and row 2 once. Unnormalised this was c, a, b, b -- which
  ;; a conversion measuring between consecutive rows would read as the order
  ;; the events happened in.
  (test-csvdt-check "a run over them is in buffer order, with no line twice"
                    (test-csvdt-output)
                    (concat "2026-07-25T10:00:00+00:00,a\n"
                            "2026-07-25T11:00:00+00:00,b\n"
                            "2026-07-25T12:00:00+00:00,c\n")))

;; A rectangle's pieces are one per line and contiguous once widened, so a run
;; makes one stretch of them -- and the command that says what a run would
;; cover has to agree with the run about that.
(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (forward-line 1)
  (forward-char 2)
  (push-mark (point) t t)
  (rectangle-mark-mode 1)
  (forward-line 2)
  (forward-char 4)
  (activate-mark)
  ;; The accessor still answers what the region reported, which is per line.
  (test-csvdt-check "the accessor still reports a rectangle as its pieces"
                    (length (csvdt--region-ranges)) 3)
  (test-csvdt-check "but they normalise to the one stretch a run covers"
                    (length (csvdt--normalise-ranges (csvdt--region-ranges))) 1)
  ;; And the command that exists to say what a run would cover says the same
  ;; number.  Describing the raw pieces instead reported a three-line
  ;; rectangle as three stretches where the run makes one -- the one place
  ;; the two are printed side by side and could disagree.
  (test-csvdt-check "and describe-run counts them the way the run does"
                    (current-message-of (csvdt-describe-run))
                    (concat "csvdt would run over 3 lines in 1 stretch.  "
                            "Banked: nothing, `csvdt-banked-lines-function' "
                            "is unset.  Region: 3 lines in 1 stretch.")))

;; A piece naming nothing is dropped rather than making an empty run look
;; like a successful one, which is what `csvdt--normalise-ranges' is for.
(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  ;; A real, non-empty region -- otherwise `use-region-p' is nil and this
  ;; never reaches the piece at all.
  (goto-char (point-min))
  (push-mark (point) t t)
  (goto-char (point-max))
  (activate-mark)
  ;; ...whose pieces name nothing: both ends at the end of the buffer.
  (setq-local region-extract-function
              (lambda (method)
                (if (eq method 'bounds)
                    (list (cons (point-max) (point-max)))
                  "")))
  (test-csvdt-check "a region covering no whole line is refused, not run"
                    (condition-case err
                        (progn (test-csvdt-typed "-H -p"
                                 (call-interactively #'csvdt-run-region))
                               'no-error)
                      (user-error (cadr err)))
                    "The region covers no whole line"))

;; The blame for a malformed piece follows where it came from. Routing the
;; region through the same normaliser must not start pointing at the banked
;; setting, which is not what reported it.
(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min))
  (push-mark (point) t t)
  (goto-char (point-max))
  (activate-mark)
  (setq-local region-extract-function
              (lambda (method)
                (if (eq method 'bounds) (list "not a range") "")))
  (test-csvdt-check "a malformed region piece names the region, not the bank"
                    (condition-case err
                        (test-csvdt-typed "-H -p"
                          (call-interactively #'csvdt-run-region))
                      (user-error (cadr err)))
                    "`region-bounds' reported \"not a range\", which is neither a position pair nor an overlay"))

;;; Partially selected lines
;;
;; A CSV record is a line.  A selection starting or ending mid-row used to go
;; out as it stood, so csvdt was handed a truncated field and read it as a
;; value rather than as damage.

(with-temp-buffer
  (insert test-csvdt-rows)
  ;; From partway into the first data row, to the end of the buffer.
  (goto-char (point-min))
  (forward-line 1)
  (forward-char 11)
  (csvdt--run (list (cons (point) (point-max))) '("-u0" "-i" "0,replace"))
  (test-csvdt-check "a region starting mid-row is widened to whole rows"
                    (test-csvdt-output)
                    (concat "2026-07-25T10:00:00+00:00,a\n"
                            "2026-07-25T11:00:00+00:00,b\n"
                            "2026-07-25T12:00:00+00:00,c\n"
                            "2026-07-25T13:00:00+00:00,d\n")))

(with-temp-buffer
  (insert test-csvdt-rows)
  ;; Ending partway into a row takes that row whole rather than dropping it.
  (goto-char (point-min))
  (forward-line 1)
  (let ((beg (point)))
    (forward-line 1)
    (forward-char 5)
    (csvdt--run (list (cons beg (point))) '("-H" "-p")))
  ;; The region began at the first data row and ended five characters into the
  ;; second, so both come out whole -- the partial one included rather than
  ;; truncated or dropped.
  (test-csvdt-check "a region ending mid-row takes that row whole"
                    (test-csvdt-output)
                    (concat "ts,x\n"
                            "2026-07-25T12:00:00+02:00,a\n"
                            "2026-07-25T13:00:00+02:00,b\n")))

;; The widening must not fire when there was nothing to widen, or the note
;; would appear on every run.
(with-temp-buffer
  (insert test-csvdt-rows)
  (test-csvdt-check "a whole-buffer range needs no widening"
                    (csvdt--whole-lines (point-min) (point-max))
                    (cons (point-min) (point-max)))
  (goto-char (point-min))
  (forward-line 2)
  (test-csvdt-check "and neither does a range already on line boundaries"
                    (csvdt--whole-lines (point-min) (point))
                    (cons (point-min) (point))))

;; A buffer whose last line has no newline is still a whole line.
(with-temp-buffer
  (insert "a,b\n1,2")
  (goto-char (point-max))
  (test-csvdt-check "a last line without a newline widens to the end"
                    (csvdt--whole-lines (- (point-max) 1) (point-max))
                    (cons (- (point-max) 3) (point-max))))


;;; A named banking function that is not defined

;; The setting names a function; if the package providing it is not loaded, or
;; is older than the function named, calling it gave a bare "Symbol's function
;; definition is void" that said nothing about the setting being at fault.
;; Returning nil instead would be worse: the run would cover the whole buffer
;; without a word.
(let ((csvdt-banked-lines-function 'csvdt-test-no-such-banking-function))
  (test-csvdt-check
   "a function that is not defined is a configuration error"
   (condition-case error
       (progn (csvdt--banked-lines) "returned")
     (user-error (and (string-match-p "which is not defined"
                                      (error-message-string error))
                      t)))
   t)

  ;; csvdt-describe-run exists to explain what a run would cover. It is the one
  ;; command that must report this rather than raise it.
  (with-temp-buffer
    (insert "a,b\n1,2\n")
    (test-csvdt-check
     "and csvdt-describe-run says so rather than failing"
     (let ((said nil))
       (cl-letf (((symbol-function 'message)
                  (lambda (format &rest arguments)
                    (setq said (apply #'format format arguments)))))
         (csvdt-describe-run))
       (and (string-match-p "is not defined" said)
            (string-match-p "whole buffer" said)
            t))
     t)))

;; A function that is defined and reports nothing is a different thing, and
;; still reads as an absence rather than a mistake.
(let ((csvdt-banked-lines-function (lambda () nil)))
  (test-csvdt-check "a function reporting nothing is not an error"
                    (csvdt--banked-lines)
                    nil))

;;; A filename among the arguments would replace the selection

;; These commands always send the lines on standard input, so a filename in the
;; argument line is not a second input: csvdt reads the file and ignores the
;; selection, and the output buffer looks exactly like a successful run on it.
(let ((elsewhere (make-temp-file "test-csvdt-other" nil ".csv" "z,z\n9,9\n")))
  (unwind-protect
      (progn
        (test-csvdt-check
         "a real file among the arguments is found"
         (csvdt--file-arguments (list "-H" "-p" elsewhere))
         (list elsewhere))

        ;; It has to be refused before anything runs, or the wrong contents are
        ;; displayed as though they were the selection's.
        (with-temp-buffer
          (insert "a,b\n1,2\n")
          (test-csvdt-check
           "and refused rather than run"
           (condition-case error
               (progn (csvdt--run (list (cons (point-min) (point-max)))
                                  (list "-H" "-p" elsewhere))
                      "ran anyway")
             (user-error
              (and (string-match-p "ignore the selection"
                                   (error-message-string error))
                   t)))
           t))

        ;; Everything after "--" is a positional argument, whatever it looks
        ;; like, so a filename there counts too.
        (test-csvdt-check
         "a file after -- counts as well"
         (csvdt--file-arguments (list "-H" "--" elsewhere))
         (list elsewhere)))
    (delete-file elsewhere)))

;; Values that belong to the option before them are not filenames, whether they
;; are attached, separate, clustered or joined with "=". Getting this wrong the
;; other way would refuse valid command lines.
(dolist (args '(("-H" "-p")
                ("-H" "-p" "-u0")
                ("-H" "-p" "-u" "0")
                ("-Hp" "-u" "0")
                ("-H" "-p" "--trim" "none")
                ("-H" "-p" "--remove" "1")
                ("-H" "-p" "-i" "0,replace" "-u0")
                ("-H" "-p" "-a" "1,0")
                ("-H" "-p" "-o" "0,+01:00")
                ("-H" "-p" "-f=widest")
                ("-H" "-")))
  (test-csvdt-check (format "no filename in %s" (string-join args " "))
                    (csvdt--file-arguments args)
                    nil))

;; The same table read the other way round, with a filename after the option:
;; one carrying its own value leaves the word after it where the filename
;; goes, and that word has to still be reported.  Taking every option for one
;; that eats the next argument, "-u0 other.csv" stops being flagged -- csvdt
;; reads that file, ignores the selection, and the output buffer looks exactly
;; like a successful run on the lines nobody sent.
(let ((elsewhere (make-temp-file "test-csvdt-other" nil ".csv" "z,z\n9,9\n")))
  (unwind-protect
      (dolist (args (list (list "-u0" elsewhere) (list "-Hu0" elsewhere)))
        (test-csvdt-check (format "a file after %s is still found" (car args))
                          (csvdt--file-arguments args)
                          (list elsewhere)))
    (delete-file elsewhere)))

;; The check only reports what csvdt would really open. A word left where the
;; filename goes that names nothing is left to csvdt, whose own message for it
;; is better than a guess from here -- it names the option the word belongs to.
(test-csvdt-check "a word that is not a file is left to csvdt"
                  (csvdt--file-arguments (list "-H" "-p" "-f" "fix"))
                  nil)

;; A directory is not something csvdt can read either, and it is not what this
;; is looking for: only a regular file silently replaces the selection.
(test-csvdt-check "a directory is not a filename argument"
                  (csvdt--file-arguments (list "-H" "-p" temporary-file-directory))
                  nil)

;;; The review fixes, each pinned

;; A backslash sequence that is not a Lisp escape inside double quotes used to
;; escape as a raw reader error; now it is a user-error carrying the way out.
(test-csvdt-check
 "a bad escape in double quotes is a user-error pointing at single quotes"
 (condition-case err
     (progn (csvdt--split-arguments "-x \"C:\\Users\\data\"") 'no-error)
   (user-error (and (string-match-p "single quotes" (cadr err)) t))
   (error 'raw-error))
 t)

;; A sequence the Lisp reader DOES know is refused all the same when it is
;; outside the shell-familiar set: \d is DEL to the reader, so "C:\data"
;; sailed through as C:<DEL>ata -- the same path as above, corrupted rather
;; than caught, depending on the letter after the backslash.
(test-csvdt-check
 "a valid Lisp escape outside the shell set is refused, not read"
 (condition-case err
     (csvdt--split-arguments "-x \"C:\\data\"")
   (user-error (and (string-match-p "single quotes" (cadr err)) t))
   (error 'raw-error))
 t)

;; And \n inside double quotes stays an escape (a newline), not a refusal:
;; the familiar set still means what a shell would make it mean.
(test-csvdt-check
 "the shell-familiar escapes still pass through the refusal"
 (csvdt--split-arguments "-x \"a\\tb\\nc\"")
 '("-x" "a\tb\nc"))

;; The declared Emacs 27.1 stays true by construction: the two calls that
;; broke it must not come back.  Their names in comments are fine -- this
;; looks for call positions.
(let ((source (with-temp-buffer
                (insert-file-contents (expand-file-name "csvdt.el"
                                                        test-csvdt-directory))
                (buffer-string))))
  (test-csvdt-check "no call to defvar-keymap (Emacs 29) in csvdt.el"
                    (and (string-match-p "(defvar-keymap" source) t) nil)
  (test-csvdt-check "no call to string-search (Emacs 28) in csvdt.el"
                    (and (string-match-p "(string-search" source) t) nil))

;; The keymap carries what the README documents, describe-run included.
(test-csvdt-check "? is bound to csvdt-describe-run"
                  (lookup-key csvdt-command-map "?") #'csvdt-describe-run)
(test-csvdt-check "k is bound to csvdt-run-banked"
                  (lookup-key csvdt-command-map "k") #'csvdt-run-banked)

;; "-fH" is two flags to clap -- -f's value must be attached with "=", so
;; nothing after it in a cluster can be one -- and the header-carry logic has
;; to see the -H in it.  "-uH" is -u with an attached value, and must not.
(test-csvdt-check "-fH asks for a header"
                  (and (csvdt--header-requested-p '("-fH")) t) t)
(test-csvdt-check "-uH does not (H is -u's value)"
                  (csvdt--header-requested-p '("-uH")) nil)

;; A stale range past the end of a buffer whose last line has no newline used
;; to clamp onto that line; it names no character and is dropped.
(with-temp-buffer
  (insert "a,1\nb,2")
  (test-csvdt-check "a range wholly past a newline-less buffer end is dropped"
                    (csvdt--normalise-ranges '((100 . 200))) nil)
  (test-csvdt-check "a live range beside it is unaffected"
                    (csvdt--normalise-ranges '((1 . 3))) '((1 . 5))))

;; A directory is executable to file-executable-p, and is not a program.
;;
;; Each of these used to be told "Cannot find", which is the one thing a file
;; sitting right there at that path is not: the reader checks with ls, finds
;; it, and goes looking for the wrong problem. What is wrong is named now.
(test-csvdt-check
 "a directory as csvdt-executable is refused as a directory"
 (let ((csvdt-executable temporary-file-directory))
   (condition-case err (progn (csvdt--program) 'no-error)
     (user-error (and (string-match-p "is a directory" (cadr err))
                      (string-match-p "csvdt-executable" (cadr err))
                      t))))
 t)

(let ((plain (make-temp-file "csvdt-not-executable")))
  (unwind-protect
      (test-csvdt-check
       "a file that is there but not executable says so, and how to fix it"
       (let ((csvdt-executable plain))
         (condition-case err (progn (csvdt--program) 'no-error)
           (user-error (and (string-match-p "is not executable" (cadr err))
                            (string-match-p "chmod" (cadr err))
                            t))))
       t)
    (delete-file plain)))

;; A bare name is looked for along `exec-path', which is not the PATH of the
;; shell that started Emacs unless something arranged for it to be -- the
;; usual reason a program on the terminal's PATH is not found here.
(test-csvdt-check
 "a bare name that is nowhere names exec-path"
 ;; A name with no "exec-path" in it, so what the check matches is the
 ;; message's wording and not the name interpolated into it. Chosen the
 ;; other way round at first, and it passed against a message that said
 ;; nothing of the sort.
 (let ((csvdt-executable "csvdt-nowhere-at-all"))
   (condition-case err (progn (csvdt--program) 'no-error)
     (user-error (and (string-match-p "exec-path" (cadr err)) t))))
 t)

;; And a path that names nothing at all is still simply not found.
(test-csvdt-check
 "a path to nothing is still 'cannot find'"
 (let ((csvdt-executable (expand-file-name "csvdt-no-such-file"
                                           temporary-file-directory)))
   (condition-case err (progn (csvdt--program) 'no-error)
     (user-error (and (string-match-p "Cannot find" (cadr err))
                      (not (string-match-p "exec-path" (cadr err)))
                      t))))
 t)

;; The option cache is keyed on the binary's path and mtime, so a warm lookup
;; spawns nothing -- it used to run `csvdt --version' per call -- and touching
;; the binary invalidates it.
(csvdt--option-records)
(let ((spawns 0))
  (cl-letf* ((real (symbol-function 'call-process))
             ((symbol-function 'call-process)
              (lambda (&rest args) (setq spawns (1+ spawns))
                (apply real args))))
    (csvdt--option-records)
    (csvdt-options)
    (test-csvdt-check "warm option lookups spawn no subprocess" spawns 0)))
(let* ((copy (make-temp-file "csvdt-copy"))
       (spawns 0))
  (copy-file csvdt-executable copy t)
  (set-file-modes copy #o755)
  (let ((csvdt-executable copy)
        (csvdt--options-cache nil))
    (csvdt--option-records)
    (set-file-times copy (time-add (current-time) 5))
    (cl-letf* ((real (symbol-function 'call-process))
               ((symbol-function 'call-process)
                (lambda (&rest args) (setq spawns (1+ spawns))
                  (apply real args))))
      (csvdt--option-records)
      (test-csvdt-check "a changed mtime re-reads the options"
                        (> spawns 0) t)))
  (delete-file copy))

;; What csvdt says on stderr about a successful run reaches the summary
;; message, and a mid-line selection is reported as widened again -- the
;; notice was dead while every command handed csvdt--run pre-widened ranges.
(defvar test-csvdt-last-message nil)
(defun test-csvdt-capture-message (format &rest arguments)
  (when format
    (setq test-csvdt-last-message (apply #'format format arguments))))

(with-temp-buffer
  (insert "ts,x\n2026-07-25T12:00:00Z,a\nnonsense,b\n2026-07-26T12:00:00Z,c\n")
  (cl-letf (((symbol-function 'message) #'test-csvdt-capture-message))
    (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p" "-u0")))
  (test-csvdt-check "a successful run's stderr reaches the summary"
                    (and (string-match-p "could not be converted"
                                         test-csvdt-last-message)
                         t)
                    t))

(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min)) (forward-line 1) (forward-char 7)
  (push-mark (point) t t)
  (goto-char (- (point-max) 3))
  (activate-mark)
  (cl-letf (((symbol-function 'message) #'test-csvdt-capture-message))
    (test-csvdt-typed "-u0 -i 0,replace"
      (call-interactively #'csvdt-run-region)))
  (test-csvdt-check "a mid-line selection is reported as widened"
                    (and (string-match-p "widened to whole lines"
                                         test-csvdt-last-message)
                         t)
                    t))

(with-temp-buffer
  (insert test-csvdt-rows)
  (setq-local transient-mark-mode t)
  (goto-char (point-min)) (forward-line 1)
  (push-mark (point) t t)
  (forward-line 2)
  (activate-mark)
  (cl-letf (((symbol-function 'message) #'test-csvdt-capture-message))
    (test-csvdt-typed "-u0 -i 0,replace"
      (call-interactively #'csvdt-run-region)))
  (test-csvdt-check "a whole-line selection is not"
                    (string-match-p "widened" test-csvdt-last-message) nil))

;; How many stretches went in is the only word the summary says about a run
;; having covered the banked lines as well as the region, rather than one of
;; the two: without it a run over two stretches reads exactly like a run over
;; one, and the line count alone does not tell them apart.
(with-temp-buffer
  (insert test-csvdt-rows)
  (test-csvdt-banking '(2 4)
    (test-csvdt-check "the summary says how many stretches went in"
                      (current-message-of
                       (test-csvdt-typed "-H -p"
                         (call-interactively #'csvdt-run-banked)))
                      "csvdt -H -p: 3 lines from 2 stretches")))

;;; An end at end-of-line is whole
;;
;; An overlay covering a line usually stops before the newline -- the shape
;; `csvdt-banked-lines-from-overlays' reports -- and the boundary check called
;; that end mid-line, so every banked run said "selection widened to whole
;; lines" about rows that were whole.

(with-temp-buffer
  (insert "a,1\nb,2\nc,3\n")
  (let ((eol1 (save-excursion (goto-char (point-min)) (line-end-position)))
        (bol2 (save-excursion (goto-char (point-min)) (forward-line 1) (point))))
    ;; Unit level first: each end shape, judged alone.
    (test-csvdt-check "an end at end-of-line is whole"
                      (csvdt--mid-line-boundary-p (list (cons (point-min) eol1)))
                      nil)
    (test-csvdt-check "an end mid-line is not"
                      (and (csvdt--mid-line-boundary-p
                            (list (cons (point-min) (1- eol1))))
                           t)
                      t)
    ;; A beginning at end-of-line pulls that entire line in, which is exactly
    ;; the widening the notice exists to report.
    (test-csvdt-check "a beginning at end-of-line is a boundary"
                      (and (csvdt--mid-line-boundary-p
                            (list (cons eol1 (line-beginning-position 3))))
                           t)
                      t)
    (test-csvdt-check "a beginning at start-of-line is whole"
                      (csvdt--mid-line-boundary-p
                       (list (cons bol2 (line-beginning-position 3))))
                      nil)))

;; End to end: banked lines marked with bol..eol overlays run without the
;; notice, since nothing meaningful was widened.
(with-temp-buffer
  (insert "a,1\nb,2\nc,3\n")
  (dolist (n '(0 1))
    (save-excursion
      (goto-char (point-min)) (forward-line n)
      (let ((o (make-overlay (point) (line-end-position))))
        (overlay-put o 'test-csvdt-banked t))))
  (setq-local csvdt-banked-lines-function
              (csvdt-banked-lines-from-overlays 'test-csvdt-banked))
  (cl-letf (((symbol-function 'message) #'test-csvdt-capture-message))
    (test-csvdt-typed "-H -p"
      (call-interactively #'csvdt-run-banked)))
  (test-csvdt-check "whole-line overlays run without the widened notice"
                    (string-match-p "widened" test-csvdt-last-message) nil)
  (test-csvdt-check "and the run still covers exactly the banked rows"
                    (test-csvdt-output) "a,1\nb,2\n"))

;; The decoding the boundary check and normalise share, asked directly: what
;; one drops the other must not report on.
(with-temp-buffer
  (insert "a,1\nb,2")
  (test-csvdt-check "a piece past a newline-less end has no bounds"
                    (csvdt--piece-bounds '(100 . 200)) nil)
  (test-csvdt-check "and the boundary check is equally blind to it"
                    (csvdt--mid-line-boundary-p '((100 . 200))) nil)
  (test-csvdt-check "a reversed pair is two ends, not a direction"
                    (csvdt--piece-bounds '(5 . 1)) '(1 . 5)))

;;; The third audit round, each fix pinned

;; A pasted line's trailing newline is a separator, not part of the last
;; argument -- split-string-and-unquote always split on them, and the
;; rewritten splitter regressed that.
(dolist (case '(("-H -p -u0\n" ("-H" "-p" "-u0"))
                ("-H\n-p" ("-H" "-p"))
                ("-H\r\n-p" ("-H" "-p"))))
  (test-csvdt-check (format "newlines separate: %S" (car case))
                    (csvdt--split-arguments (car case))
                    (cadr case)))

;; The option cache keys on the truename's mtime, so a binary rebuilt
;; behind a stable symlink invalidates it -- the link's own mtime never
;; moves, and keying on it served stale records until Emacs restarted.
(let* ((dir (make-temp-file "csvdt-lnk" t))
       (copy (concat dir "/real-csvdt"))
       (link (concat dir "/csvdt")))
  (copy-file csvdt-executable copy)
  (set-file-modes copy #o755)
  (make-symbolic-link copy link)
  (let ((csvdt-executable link)
        (csvdt--options-cache nil)
        (spawns 0))
    (csvdt--option-records)
    (cl-letf* ((real (symbol-function 'call-process))
               ((symbol-function 'call-process)
                (lambda (&rest args) (setq spawns (1+ spawns))
                  (apply real args))))
      (csvdt--option-records)
      (test-csvdt-check "warm lookups through a symlink spawn nothing"
                        spawns 0)
      (set-file-times copy (time-add (current-time) 5))
      (csvdt--option-records)
      (test-csvdt-check "touching the target behind the symlink re-reads"
                        (> spawns 0) t)))
  (delete-directory dir t))

;; The prompt's version string comes from the cache once it is warm; the
;; command that exists to ask still asks live.
(csvdt--option-records)
(let ((spawns 0))
  (cl-letf* ((real (symbol-function 'call-process))
             ((symbol-function 'call-process)
              (lambda (&rest args) (setq spawns (1+ spawns))
                (apply real args))))
    (csvdt--cached-version)
    (test-csvdt-check "the prompt's version spawns nothing when warm"
                      spawns 0))
  (test-csvdt-check "and it matches what the binary says"
                    (csvdt--cached-version) (csvdt-version-string)))

;; dwim blames whichever source reported a malformed piece.  Normalising
;; the two lists together pointed every complaint at the banked setting,
;; including for pieces the region supplied.
(with-temp-buffer
  (insert "a,1\nb,2\n")
  (setq-local transient-mark-mode t)
  (goto-char (point-min)) (push-mark (point) t t)
  (goto-char (point-max)) (activate-mark)
  (setq-local region-extract-function
              (lambda (method) (if (eq method 'bounds) (list "junk") "")))
  (test-csvdt-check
   "dwim blames the region for a region piece"
   (condition-case err
       (test-csvdt-typed "-H -p" (call-interactively #'csvdt-run-dwim))
     (user-error (and (string-match-p "`region-bounds'" (cadr err)) t)))
   t))

(with-temp-buffer
  (insert "a,1\nb,2\n")
  (setq-local csvdt-banked-lines-function (lambda () (list "junk")))
  (test-csvdt-check
   "and the banked setting for a banked piece"
   (condition-case err
       (test-csvdt-typed "-H -p" (call-interactively #'csvdt-run-dwim))
     (user-error (and (string-match-p "csvdt-banked-lines-function"
                                      (cadr err))
                      t)))
   t))

;;; Round five, each fix pinned

;; An '=' anywhere means option=value to clap, never a cluster -- scanned
;; as one, "-f=keep" yielded junk flags, and a customised
;; csvdt-header-options naming a letter the value held prepended the
;; buffer header to a run that never asked for one.
(test-csvdt-check "-f=keep is not a flag cluster"
                  (csvdt--short-flags "-f=keep") nil)
(test-csvdt-check "and cannot trip a customised header option"
                  (let ((csvdt-header-options '("-k")))
                    (csvdt--header-requested-p '("-f=keep")))
                  nil)
(test-csvdt-check "a real cluster still scans"
                  (csvdt--short-flags "-Hp") '("-H" "-p"))

;; A zero-width piece widens to its entire containing line -- a row the
;; selection held no character of, the largest widening there is -- and
;; was the one case the widened notice stayed silent about.
(with-temp-buffer
  (insert "a,1\nb,2\nc,3\n")
  (test-csvdt-check "an empty piece mid-line is the largest widening"
                    (and (csvdt--mid-line-boundary-p '((6 . 6))) t) t)
  (test-csvdt-check "empty at start-of-line still gains its whole row"
                    (and (csvdt--mid-line-boundary-p '((5 . 5))) t) t)
  (test-csvdt-check "and it still normalises to that row"
                    (csvdt--normalise-ranges '((6 . 6))) '((5 . 9)))
  (test-csvdt-check "an empty piece at the very end names nothing"
                    (csvdt--mid-line-boundary-p
                     (list (cons (point-max) (point-max))))
                    nil))

;;; Rounds eight and nine, each fix pinned

;; A name carrying a directory part means that file -- execvp's rule, and
;; the shell's.  `executable-find' does not hold to it: handed "./csvdt",
;; it joins the name onto every `exec-path' entry, so a same-named binary
;; installed anywhere on PATH silently won over the file the user actually
;; named.  Two fakes with the same name, and the one on PATH must lose.
(let ((dir (file-name-as-directory (make-temp-file "csvdt-here" t)))
      (path-dir (file-name-as-directory (make-temp-file "csvdt-on-path" t))))
  (dolist (file (list (concat dir "fake")
                      (concat dir "sub/fake")
                      (concat path-dir "fake")))
    (make-directory (file-name-directory file) t)
    (with-temp-file file (insert "#!/bin/sh\nexit 0\n"))
    (set-file-modes file #o755))
  (let ((default-directory dir)
        (exec-path (list (directory-file-name path-dir))))
    (let ((csvdt-executable "./fake"))
      (test-csvdt-check "./name is the file it names, never a PATH search"
                        (csvdt--program) (concat dir "fake")))
    (let ((csvdt-executable "sub/fake"))
      (test-csvdt-check "a relative path resolves against default-directory"
                        (csvdt--program) (concat dir "sub/fake")))
    ;; And absolute, so the process call and the cache's mtime key mean one
    ;; file whatever the current directory becomes later.
    (let ((csvdt-executable "./fake"))
      (test-csvdt-check "and comes back absolute"
                        (file-name-absolute-p (csvdt--program)) t))
    ;; A bare name is the one shape that still goes to PATH.
    (let ((csvdt-executable "fake"))
      (test-csvdt-check "a bare name is still found on PATH"
                        (csvdt--program) (executable-find "fake"))))
  (delete-directory dir t)
  (delete-directory path-dir t))

;; The completion token starts where the last separator ended, and the
;; separators are `csvdt--split-arguments's whole set: skipping back over
;; only space and tab made a pasted "-H\n--arr" one completion token,
;; offering candidates for a string RET would have split in two.
(dolist (separator '("\n" "\r" "\f" "\v"))
  (with-temp-buffer
    (insert "-H" separator "--arr")
    (let ((capf (csvdt--complete-argument)))
      (test-csvdt-check (format "%S bounds the argument being completed"
                                separator)
                        (buffer-substring (nth 0 capf) (nth 1 capf))
                        "--arr"))))

;; Formfeed and vertical tab separate arguments, as
;; `split-string-and-unquote' always had them do -- the rewritten splitter
;; took newlines back but left these two folded into the argument.
(test-csvdt-check "formfeed and vertical tab separate arguments"
                  (csvdt--split-arguments "a\fb\vc")
                  '("a" "b" "c"))

;; A pair of markers is how a package holds positions that must survive
;; edits, so it is what a banking package most naturally reports.  They
;; were silently dropped, and dwim, finding nothing banked, ran over the
;; whole buffer without a word.
(with-temp-buffer
  (insert test-csvdt-rows)
  (let* ((bounds (test-csvdt-line-bounds 2))
         (beg (copy-marker (car bounds)))
         (end (copy-marker (cdr bounds))))
    (test-csvdt-check "a pair of markers is a range like any other"
                      (csvdt--normalise-ranges (list (cons beg end)))
                      (list bounds))
    (test-csvdt-check "and an integer pairs with a marker"
                      (csvdt--normalise-ranges (list (cons (car bounds) end)))
                      (list bounds)))

  ;; A marker from another buffer describes a position there, not here --
  ;; the foreign overlay's treatment, for the same reason.
  (let ((elsewhere (generate-new-buffer "csvdt-marker-elsewhere")))
    (with-current-buffer elsewhere (insert "not this buffer at all\n"))
    (let ((foreign (with-current-buffer elsewhere (copy-marker 3))))
      (test-csvdt-check "a marker from another buffer is refused"
                        (csvdt--normalise-ranges (list (cons foreign foreign)))
                        nil)
      (kill-buffer elsewhere)
      ;; Killing the buffer leaves it pointing nowhere, which is what a
      ;; dead overlay's ends look like, and it gets the same silence.
      (test-csvdt-check "and one whose buffer is gone is dropped"
                        (csvdt--normalise-ranges (list (cons foreign foreign)))
                        nil)))

  ;; A cons with junk for an end is the bare-integer mistake in better
  ;; disguise: (5 . nil) used to pass the shape check and then be dropped
  ;; without a word further down.
  (test-csvdt-check "a pair with a nil end is named, not dropped"
                    (condition-case err
                        (progn (csvdt--normalise-ranges '((5 . nil))) nil)
                      (user-error (and (string-match-p "neither" (cadr err)) t)))
                    t))

;; Everything after "--" is positional to clap, so an -H there is a
;; filename, not the option -- and scanning past the separator prepended
;; the buffer's header to a run that never asked for one.
(test-csvdt-check "-H before -- asks for a header"
                  (and (csvdt--header-requested-p '("-H" "--")) t) t)
(test-csvdt-check "-H after -- is positional, not the option"
                  (csvdt--header-requested-p '("--" "-H")) nil)

;; And the filename check agrees: a file really named -H, sitting after
;; the separator, is exactly what csvdt would open.
(let ((dir (make-temp-file "csvdt-dash" t)))
  (with-temp-file (expand-file-name "-H" dir) (insert "z,z\n"))
  (let ((default-directory (file-name-as-directory dir)))
    (test-csvdt-check "a file named -H after -- is found"
                      (csvdt--file-arguments '("-p" "--" "-H"))
                      '("-H")))
  (delete-directory dir t))

;; A "~" path is expanded by `file-regular-p' but not by csvdt, which
;; answers it with its own loud cannot-read error: the silent replacement
;; the filename check exists to catch cannot happen, so flagging one
;; claimed csvdt would read a file it never would.
(let* ((home-file (make-temp-file (expand-file-name "csvdt-tilde-" "~")
                                  nil ".csv" "z,z\n"))
       (tilde (concat "~/" (file-name-nondirectory home-file))))
  (unwind-protect
      (progn
        (test-csvdt-check "the ~ path does name a real file"
                          (and (file-regular-p tilde) t) t)
        (test-csvdt-check "yet is not reported as one csvdt would read"
                          (csvdt--file-arguments (list "-H" "-p" tilde))
                          nil))
    (delete-file home-file)))

;; When the option data comes from the --help fallback, no option's value
;; arity is known and the filename check stands down entirely: guessing
;; scanned an option's separate value as a positional, and refused the run
;; whenever a file of that name happened to exist.
(let ((stub (make-temp-file "old-csvdt" nil ".sh"))
      (data (make-temp-file "csvdt-value" nil ".csv" "z,z\n9,9\n")))
  (with-temp-file stub
    (insert "#!/bin/sh\n"
            "case \"$1\" in\n"
            "  --version) echo 'csvdt 0.9.0 (IANA tzdata 2025b)' ;;\n"
            "  --help) exec " csvdt-executable " --help ;;\n"
            "  *) echo 'error: unexpected argument' >&2; exit 2 ;;\n"
            "esac\n"))
  (set-file-modes stub #o755)
  (let ((csvdt-executable stub)
        (csvdt--options-cache nil))
    (test-csvdt-check "the fallback records really say nothing about values"
                      (seq-some (lambda (record) (plist-get record :value))
                                (csvdt--option-records))
                      nil)
    (test-csvdt-check "so an option's separate value is not scanned as a file"
                      (csvdt--file-arguments (list "-u" data))
                      nil))
  (delete-file stub)
  (delete-file data))

;;; Round ten, each fix pinned

;; csvdt--run pins the process coding to UTF-8 in both directions, but
;; csvdt--output-of -- through which --help, --list-options and --version
;; arrive -- left decoding to the locale.  --help held a curly quote, so a
;; latin-1 locale read its UTF-8 bytes as three characters of mojibake in
;; the help buffer.
;;
;; Asked of a stub rather than of csvdt itself.  This used to read csvdt's
;; own --help for the quote, and then the help corpus was made ASCII, which
;; the manual page rendered from it requires: the check went on passing with
;; nothing left in the text that could tell a decoded run from an undecoded
;; one.  What is pinned here is csvdt--output-of, so the bytes it is given
;; belong to the test.
(let ((stub (make-temp-file "utf8-csvdt" nil ".sh")))
  ;; The quote by its UTF-8 bytes, so the test does not depend on how this
  ;; file itself was decoded: U+2019 is e2 80 99, which latin-1 misreads as
  ;; U+00E2 U+0080 U+0099.
  (with-temp-file stub
    (insert "#!/bin/sh\nprintf 'it\\342\\200\\231s here\\n'\n"))
  (set-file-modes stub #o755)
  (let ((csvdt-executable stub)
        (default-process-coding-system '(iso-latin-1-unix . iso-latin-1-unix)))
    (let ((help (csvdt--help-text)))
      (test-csvdt-check "--help decodes as UTF-8 whatever the locale"
                        (and (string-match-p (string #x2019) help)
                             (not (string-match-p (string #x00E2 #x0080 #x0099)
                                                  help))
                             t)
                        t)))
  (delete-file stub))

;; A run that exited non-zero had its output discarded.  On the
;; nothing-converted exit csvdt writes the output in full, and its own
;; message -- relayed verbatim -- points at the parse_err markers in it:
;; dropping them left an Emacs user told "the output was written either way"
;; about output no one could see.
(when (get-buffer "*csvdt output*")
  (kill-buffer "*csvdt output*"))
(with-temp-buffer
  (insert "a,b\nx,2\n")
  (test-csvdt-check
   "a nothing-converted run still signals"
   (condition-case err
       (progn (csvdt--run (list (cons (point-min) (point-max)))
                          '("-H" "-u" "0"))
              'no-error)
     (user-error (and (string-match-p "exited 1" (cadr err))
                      (string-match-p "nothing converted" (cadr err))
                      t)))
   t))
(test-csvdt-check "and the parse_err rows it wrote are shown"
                  (and (get-buffer "*csvdt output*")
                       (with-current-buffer "*csvdt output*" (buffer-string)))
                  "x,parse_err,2\n")

;; A failure that wrote nothing has nothing to show, and showing it anyway
;; would blank a result already on display.
(with-current-buffer (get-buffer-create "*csvdt output*")
  (erase-buffer)
  (insert "kept,from\nthe,last,run\n"))
(with-temp-buffer
  (insert "a,b\n1,2\n")
  ;; Column 5 is out of bounds, which csvdt refuses before writing a byte.
  (condition-case nil
      (csvdt--run (list (cons (point-min) (point-max))) '("-u" "5"))
    (user-error nil)))
(test-csvdt-check "a failure with empty output leaves the last result alone"
                  (with-current-buffer "*csvdt output*" (buffer-string))
                  "kept,from\nthe,last,run\n")

;; Feeding a result back through csvdt can fail too, and there the output
;; buffer's contents are the input: a failed run must not replace the data
;; with its debris -- this failure writes rows, so without the guard it would.
(with-current-buffer (get-buffer-create "*csvdt output*")
  (erase-buffer)
  (insert "a,b\nx,2\n")
  (condition-case nil
      (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-u" "0"))
    (user-error nil))
  (test-csvdt-check "a failed run fed from the output buffer keeps its input"
                    (buffer-string)
                    "a,b\nx,2\n"))

;;; Round eleven, each fix pinned

;; The UTF-8 commitment was made with a let-binding in the source buffer,
;; which binds whatever binding that buffer has: a buffer that had set
;; either coding variable locally left the temp buffer where the assemble
;; path -- a header carried, or more than one stretch -- actually starts the
;; process seeing the global value instead.  Under a latin-1 locale that
;; sent latin-1 bytes to a program that refuses them, and the run died on
;; csvdt's invalid-UTF-8 error rather than converting anything.
(with-temp-buffer
  ;; U+00E9 by code point, as above, so the test does not depend on how this
  ;; file itself was decoded.
  (insert "a,b\n"
          "caf" (string #x00E9) ",1\n"
          "between,2\n"
          "z" (string #x00E9) ",3\n")
  ;; The shape that broke it is a buffer-local binding of its own, whatever
  ;; that binding happens to say.
  (set (make-local-variable 'coding-system-for-write) 'utf-8-unix)
  (test-csvdt-check
   "a buffer-local coding system does not cost the assemble path its UTF-8"
   (let ((default-process-coding-system '(iso-latin-1-unix . iso-latin-1-unix)))
     ;; Two stretches, so the text is assembled in a temp buffer rather than
     ;; handed straight to the process.  The failure was a user-error from
     ;; csvdt itself, so it is caught and shown rather than left to end the
     ;; run of tests.
     (condition-case err
         (progn (csvdt--run (list (test-csvdt-line-bounds 2)
                                  (test-csvdt-line-bounds 4))
                            '("-H" "-p"))
                (test-csvdt-output))
       (user-error (cadr err))))
   (concat "a,b\n"
           "caf" (string #x00E9) ",1\n"
           "z" (string #x00E9) ",3\n")))

;;; A narrowing that cuts a record in two says so
;;
;; Widening to whole lines can only reach as far as the accessible portion,
;; so a buffer narrowed to end mid-line has no whole line to widen to: the
;; clamp left half a record and the run said nothing about it.  Widening
;; past the restriction would send lines nobody chose, so the cut is
;; reported rather than hidden or overridden.

(with-temp-buffer
  (insert "a,b\n1,2\n3,4\n")
  ;; Narrowed to stop just after the last row's comma, so what is reachable
  ;; of that record is "3," -- half a row, with no whole line to widen to.
  (narrow-to-region (point-min) (- (point-max) 2))
  (test-csvdt-check "a narrowing ending mid-line cuts the last record"
                    (and (csvdt--narrowing-cuts-record-p
                          (list (cons (point-min) (point-max))) ?\" ?,)
                         t)
                    t)
  (test-csvdt-check "and a run over it says so in its summary"
                    (and (string-match-p
                          ", last record cut short by the narrowing"
                          (or (current-message-of
                               (csvdt--run (list (cons (point-min) (point-max)))
                                           '("-H" "-p")))
                              ""))
                         t)
                    t))

(with-temp-buffer
  (insert "a,b\n1,2\n3,4\n")
  ;; Ending exactly at end-of-line: only the newline is out of reach, and
  ;; csvdt supplies the terminator itself, so the record is whole.
  (narrow-to-region (point-min) (- (point-max) 1))
  (test-csvdt-check "a narrowing ending at end-of-line cuts nothing"
                    (csvdt--narrowing-cuts-record-p
                     (list (cons (point-min) (point-max))) ?\" ?,)
                    nil))

(with-temp-buffer
  ;; Not narrowed at all: a last line without a newline is a whole record,
  ;; and reading it as a cut would put the note on every such buffer.
  (insert "a,b\n1,2")
  (test-csvdt-check "a last line without a newline is not a cut"
                    (csvdt--narrowing-cuts-record-p
                     (list (cons (point-min) (point-max))) ?\" ?,)
                    nil))

;; The same cut at the other end.
;;
;; Only the end of the accessible portion was asked about, so a narrowing
;; that begins mid-record went unreported -- and it is the same cut, with
;; the same two ways of happening.  Mid-line, csvdt sees a short first
;; record and says so about a line, which sends the reader looking at data
;; that is not the matter.  Inside a quoted field it was worse: the run
;; succeeded, exit 0, with the first record's opening half out of reach and
;; nothing said at all.

(with-temp-buffer
  (insert "a,b\n1,2\n3,4\n")
  ;; Beginning one character into the second line: what is reachable of
  ;; that record is ",2" -- half a row, with no whole line to widen back to.
  (narrow-to-region (+ (point-min) 5) (point-max))
  (test-csvdt-check "a narrowing beginning mid-line cuts the first record"
                    (csvdt--narrowing-cuts-record-p
                     (list (cons (point-min) (point-max))) ?\" ?,)
                    'start)
  (test-csvdt-check "and a run over it says so in its summary"
                    (and (string-match-p
                          ", first record cut short by the narrowing"
                          (or (current-message-of
                               (csvdt--run (list (cons (point-min) (point-max)))
                                           '("-H" "-p")))
                              ""))
                         t)
                    t))

(with-temp-buffer
  ;; Beginning at a line boundary that is inside a quoted field: whole
  ;; lines are reachable, so nothing looks amiss, and the record they
  ;; belong to is severed all the same.
  (insert "h,when\n\"x\ny\",1\n3,4\n")
  (narrow-to-region (+ (point-min) 10) (point-max))
  (test-csvdt-check "a narrowing beginning inside a quoted field cuts it too"
                    (csvdt--narrowing-cuts-record-p
                     (list (cons (point-min) (point-max))) ?\" ?,)
                    'start))

(with-temp-buffer
  (insert "a,b\n1,2\n3,4\n")
  ;; Cut at both ends at once, which is one narrowing and two half records.
  (narrow-to-region (+ (point-min) 5) (- (point-max) 2))
  (test-csvdt-check "a narrowing can cut at both ends"
                    (csvdt--narrowing-cuts-record-p
                     (list (cons (point-min) (point-max))) ?\" ?,)
                    'both)
  (test-csvdt-check "and the summary names both"
                    (and (string-match-p
                          ", first and last records cut short by the narrowing"
                          (or (current-message-of
                               (csvdt--run (list (cons (point-min) (point-max)))
                                           '("-H" "-p")))
                              ""))
                         t)
                    t))

(with-temp-buffer
  (insert "a,b\n1,2\n3,4\n")
  ;; Beginning exactly at a line boundary: the record is whole, and reading
  ;; that as a cut would put the note on every narrowed-to-lines buffer.
  (narrow-to-region (+ (point-min) 4) (point-max))
  (test-csvdt-check "a narrowing beginning at a line boundary cuts nothing"
                    (csvdt--narrowing-cuts-record-p
                     (list (cons (point-min) (point-max))) ?\" ?,)
                    nil))

;; And `csvdt-describe-run' says it too.
;;
;; The run reported the cut and the command for asking *before* a run did
;; not, which is the wrong way round: a record the narrowing has cut in half
;; is the plainest case of what that command promises to report -- what this
;; package can see differing from what the buffer looks like.

(with-temp-buffer
  (insert "a,b\n1,2\n3,4\n")
  (narrow-to-region (point-min) (- (point-max) 2))
  (test-csvdt-check "describe-run says the narrowing cut the last record"
                    (and (string-match-p
                          "Last record cut short by the narrowing"
                          (or (current-message-of (csvdt-describe-run)) ""))
                         t)
                    t))

(with-temp-buffer
  (insert "a,b\n1,2\n3,4\n")
  (narrow-to-region (+ (point-min) 5) (point-max))
  (test-csvdt-check "describe-run says it of the first record as well"
                    (and (string-match-p
                          "First record cut short by the narrowing"
                          (or (current-message-of (csvdt-describe-run)) ""))
                         t)
                    t))

(with-temp-buffer
  ;; And an unnarrowed buffer's answer is the sentence it always was, since
  ;; a note on every run is a note nobody reads.
  (insert "a,b\n1,2\n3,4\n")
  (test-csvdt-check "and says nothing of narrowing when nothing is cut"
                    (and (string-match-p
                          "cut short by the narrowing"
                          (or (current-message-of (csvdt-describe-run)) ""))
                         t)
                    nil))

(with-temp-buffer
  ;; And the failure carries it, which is the case the check is for: csvdt
  ;; refuses the ragged record and names a line, and the line is not the
  ;; matter -- the half of it out of reach is.
  (insert "a,b,c\n1,2,3\n4,5,6\n")
  (narrow-to-region (+ (point-min) 8) (point-max))
  (test-csvdt-check "a failed run names the cut as well"
                    (and (string-match-p
                          "note the first record cut short by the narrowing"
                          (condition-case err
                              (progn (csvdt--run
                                      (list (cons (point-min) (point-max)))
                                      '("-H" "-p"))
                                     "")
                            (error (error-message-string err))))
                         t)
                    t))

(with-temp-buffer
  ;; The cut that looked tidy: a narrowing ending exactly at a line
  ;; boundary which is nonetheless inside a quoted field.  Whole lines are
  ;; reachable, so the mid-line test saw nothing, while the record they
  ;; belong to was severed all the same -- and asked to accept ragged
  ;; records, csvdt took the truncated one rather than refusing it, so the
  ;; field came back short of its second half at exit 0.
  (insert "h,when\n\"x\ny\",2026-07-25T12:00:00+02:00\nrow2,2026-07-26T12:00:00+02:00\n")
  (narrow-to-region (point-min)
                    (progn (goto-char (point-min)) (forward-line 2) (point)))
  (test-csvdt-check "a narrowing ending inside a quoted field cuts the record"
                    (and (csvdt--narrowing-cuts-record-p
                          (list (cons (point-min) (point-max))) ?\" ?,)
                         t)
                    t)
  (test-csvdt-check "and the run says so rather than passing it off as a row"
                    (and (string-match-p
                          ", last record cut short by the narrowing"
                          (or (current-message-of
                               (csvdt--run (list (cons (point-min) (point-max)))
                                           '("-f")))
                              ""))
                         t)
                    t))

(with-temp-buffer
  ;; And a quoted field that closes inside the accessible portion is not a
  ;; cut, so the note stays off a buffer merely holding such a field.
  (insert "h,when\n\"x\ny\",2026-07-25T12:00:00+02:00\nrow2,2026-07-26T12:00:00+02:00\n")
  (narrow-to-region (point-min)
                    (progn (goto-char (point-min)) (forward-line 3) (point)))
  (test-csvdt-check "a narrowing past the closing quote cuts nothing"
                    (csvdt--narrowing-cuts-record-p
                     (list (cons (point-min) (point-max))) ?\" ?,)
                    nil))

;;; An indirect buffer holds the same text
;;
;; An indirect buffer and its base hold the same characters at the same
;; positions, so a marker or overlay made in one names exactly the text the
;; other would name.  `clone-indirect-buffer' is an ordinary way to look at
;; a file, and banking made in the base was refused there as foreign:
;; `csvdt-run-banked' said "No banked lines", and `csvdt-run-dwim', finding
;; nothing banked, ran over the whole buffer without a word.

(let* ((base (generate-new-buffer "csvdt-base"))
       (bounds (with-current-buffer base
                 (insert test-csvdt-rows)
                 (test-csvdt-line-bounds 2)))
       (markers (with-current-buffer base
                  (list (cons (copy-marker (car bounds))
                              (copy-marker (cdr bounds))))))
       (overlay (with-current-buffer base
                  (make-overlay (car bounds) (cdr bounds))))
       (indirect (make-indirect-buffer base "csvdt-indirect")))

  (test-csvdt-check "markers made in the base name that line in the base"
                    (with-current-buffer base
                      (csvdt--normalise-ranges markers))
                    (list bounds))
  (test-csvdt-check "and the same line in its indirect buffer"
                    (with-current-buffer indirect
                      (csvdt--normalise-ranges markers))
                    (list bounds))
  ;; Overlays are per buffer rather than shared, but one made in the base
  ;; still describes this text, so it is not foreign here either.
  (test-csvdt-check "an overlay from the base is not foreign to it"
                    (with-current-buffer indirect
                      (csvdt--normalise-ranges (list overlay)))
                    (list bounds))
  ;; And the run itself covers exactly that line, which is what being
  ;; refused took away.  Refused, this was the "No banked lines" user-error,
  ;; so it is caught and shown rather than left to end the run of tests.
  (with-current-buffer indirect
    (setq-local csvdt-banked-lines-function (lambda () markers))
    (test-csvdt-check "banking made in the base runs in the indirect buffer"
                      (condition-case err
                          (progn (test-csvdt-typed "-H -p"
                                   (call-interactively #'csvdt-run-banked))
                                 (test-csvdt-output))
                        (user-error (cadr err)))
                      "ts,x\n2026-07-25T12:00:00+02:00,a\n"))

  ;; A buffer that merely exists elsewhere shares no text, and its positions
  ;; mean something else here: refused as they always were.
  (let ((elsewhere (generate-new-buffer "csvdt-unrelated")))
    (with-current-buffer elsewhere (insert "not this buffer at all\n"))
    (let ((marker (with-current-buffer elsewhere (copy-marker 3)))
          (foreign (with-current-buffer elsewhere
                     (make-overlay (point-min) (point-max)))))
      (test-csvdt-check "a marker from an unrelated buffer is still refused"
                        (with-current-buffer indirect
                          (csvdt--normalise-ranges (list (cons marker marker))))
                        nil)
      (test-csvdt-check "and an overlay from one likewise"
                        (with-current-buffer indirect
                          (csvdt--normalise-ranges (list foreign)))
                        nil))
    (kill-buffer elsewhere))

  ;; An indirect buffer has to go before its base does.
  (kill-buffer indirect)
  (kill-buffer base))

;;; An output mode that signals costs no more than one that is missing

;; A mode that does not exist has always left the buffer plain; one that
;; exists and then signals took the whole run report with it -- no line
;; count, no stderr notes, no window -- leaving the result in a buffer
;; nobody was shown.  Both failures end the same way now.

(defun test-csvdt-exploding-mode ()
  ;; Half applied and then signalling, which is what a mode whose hook
  ;; raises looks like: the buffer is left claiming to be in it.
  (kill-all-local-variables)
  (setq major-mode 'test-csvdt-exploding-mode)
  (setq mode-name "Exploding")
  (error "boom"))

(let ((csvdt-output-mode 'test-csvdt-exploding-mode)
      (said nil))
  (with-temp-buffer
    (insert "a,b\n0,1\n")
    (setq said (condition-case err
                   (current-message-of
                    (csvdt--run (list (cons (point-min) (point-max)))
                                '("-H" "-p")))
                 (error (error-message-string err)))))
  (test-csvdt-check "a mode that signals still leaves the run reported"
                    (and said (string-match-p "\\`csvdt -H -p: 2 lines" said) t)
                    t)
  (test-csvdt-check "with the output where it belongs"
                    (test-csvdt-output) "a,b\n0,1\n")
  (test-csvdt-check "and the buffer left plain, as a missing mode leaves it"
                    (with-current-buffer "*csvdt output*" major-mode)
                    'fundamental-mode))

;;; The stderr file a run makes is taken away again
;;
;; csvdt's stderr is collected in a temporary file so that what it said can be
;; shown with the summary.  The file is deleted in an `unwind-protect', which
;; is what makes a failing run cost no more than a successful one: an Emacs
;; left open for a day otherwise dribbles one file per run into the temporary
;; directory, and the runs that leave them are the ones nobody is watching.

(defun test-csvdt-stderr-files ()
  "Return how many stderr files `csvdt--run' has left in the temp directory."
  (length (directory-files temporary-file-directory nil "\\`csvdt-stderr")))

(let ((before (test-csvdt-stderr-files)))
  (with-temp-buffer
    (insert "a,b\n1,2\n")
    (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p")))
  (test-csvdt-check "a successful run leaves no stderr file behind"
                    (- (test-csvdt-stderr-files) before) 0))

(let ((before (test-csvdt-stderr-files)))
  (with-temp-buffer
    (insert "a,b\n1,2\n")
    ;; Column 9 is out of bounds, so this one signals on its way out -- and
    ;; the file has to go all the same.
    (condition-case nil
        (csvdt--run (list (cons (point-min) (point-max))) '("-H" "-p" "-u9"))
      (user-error nil)))
  (test-csvdt-check "and neither does one that failed"
                    (- (test-csvdt-stderr-files) before) 0))

;;; Something that is not a list at all

;; The naming is per item, so a banking or region function returning a
;; vector, a number or a string reached `dolist' and signalled the raw
;; wrong-type-argument that the per-item naming exists to prevent.  The
;; `error' clauses below catch exactly that, so a revert reads as a failed
;; check rather than as the end of the run.

(dolist (junk (list [1 2] 5 "1,3"))
  (test-csvdt-check (format "%S is named rather than signalling raw" junk)
                    (condition-case err
                        (progn (csvdt--normalise-ranges junk) 'no-error)
                      (user-error
                       (and (string-match-p "is not a list of positions or overlays"
                                            (cadr err))
                            t))
                      (error 'raw-error))
                    t))

;; The source is named as it is for a malformed item, since a region
;; function is as able to report the wrong shape as a banking one.
(test-csvdt-check "and the source it came from is named with it"
                  (condition-case err
                      (progn (csvdt--normalise-ranges [1 2] "`region-bounds'")
                             'no-error)
                    (user-error (and (string-match-p "`region-bounds'" (cadr err)) t))
                    (error 'raw-error))
                  t)

;; A list whose items are junk is the older mistake and keeps its older
;; words: the two say different things because they are different faults.
(test-csvdt-check "a list of junk still reads as junk in the list"
                  (condition-case err
                      (progn (csvdt--normalise-ranges (list "oops")) 'no-error)
                    (user-error
                     (and (string-match-p "neither a position pair nor an overlay"
                                          (cadr err))
                          (not (string-match-p "is not a list of" (cadr err)))
                          t))
                    (error 'raw-error))
                  t)

(message "\n%s"
         (cond
          ((< test-csvdt-ran test-csvdt-expected)
           (format "ONLY %d OF %d CHECKS RAN"
                   test-csvdt-ran test-csvdt-expected))
          ((zerop test-csvdt-failures)
           (format "ALL PASS (%d checks)" test-csvdt-ran))
          (t (format "%d FAILURE(S)" test-csvdt-failures))))
(kill-emacs (if (and (zerop test-csvdt-failures)
                     (>= test-csvdt-ran test-csvdt-expected))
                0 1))

;;; test-csvdt.el ends here


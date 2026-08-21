;;; tty-test.el --- Keystroke-level tests for csvdt.el  -*- lexical-binding: t; -*-

;; SPDX-License-Identifier: GPL-3.0-or-later

;;; Commentary:

;; Drives the minibuffer with real key sequences, which `emacs --batch' cannot
;; do -- it reads its own stdin, so a macro never reaches a minibuffer.  Run
;; from this directory under a pty:
;;
;;     ./tty-test.sh
;;
;; This is separate from run-tests.sh because it needs a terminal.  What it
;; covers that the batch tests cannot: that the prefill is selected and so
;; replaced by what is typed, that a deliberate keystroke keeps it instead,
;; that a comma typed into an argument stays in it, that TAB really completes
;; an option, that a recalled line runs again, and that the line was
;; remembered in csvdt's own history rather than in the one every minibuffer
;; shares.
;;
;; And that a region marked with the keys a person marks one with is the
;; region the run covers.  The batch tests set the mark themselves, which is
;; the one thing they cannot do the way it is really done: whether a region
;; counts as live is settled by `transient-mark-mode' and by whether a
;; command activated the mark, and the command loop is what settles both.

;;; Code:

(add-to-list 'load-path (file-name-directory (or load-file-name buffer-file-name)))
(require 'csvdt)

(setq csvdt-executable
      (or (getenv "CSVDT")
          (expand-file-name "../target/release/csvdt"
                            (file-name-directory (or load-file-name
                                                     buffer-file-name)))))

(defvar csvdt-tty-failures 0)
(defvar csvdt-tty-log '())
(defvar csvdt-tty-ran 0
  "How many checks were reached, counted before the log is consumed.")

(defconst csvdt-tty-expected 11
  "How many checks this file makes.
Raise it with the count.  A run reporting fewer has not passed, whatever
the ones it did reach said.")

;; Plain data: what a run does is visible without needing a header at all.
(defconst csvdt-tty-columns "a,b,c\n0,1,2\n")

;; A header and a timestamp, so that whether "-H -p" survived is visible in the
;; output: with it the header is named after the action, without it the header
;; is parsed as data and comes back as a parse error marker.  The plain buffer
;; cannot show the difference -- an action-free run looks the same either way.
(defconst csvdt-tty-timestamps "ts,other\n2026-07-25T12:00:00+02:00,x\n")

;; Several rows, so that a selection covering some of them can be told from
;; one covering all.  Plain data again: which records were sent is the
;; question, and an action would only add a column to look past.
(defconst csvdt-tty-rows "a,b\n0,zero\n1,one\n2,two\n3,three\n")

(defun csvdt-tty-drive (content keys &optional before)
  "Type BEFORE then KEYS over a buffer holding CONTENT.
Split out from `csvdt-tty-check' so that a check asking something other
than what landed in the output buffer can still be typed for real."
  (let ((buffer (get-buffer-create "csvdt-tty-source")))
    ;; A displayed buffer, not a temporary one: the command loop
    ;; reads keys in the selected window's buffer, so a macro
    ;; run against a temp buffer never finds the binding.
    (switch-to-buffer buffer)
    (erase-buffer)
    (insert content)
    (use-local-map
     (let ((map (make-sparse-keymap)))
       (define-key map (kbd "C-c t") #'csvdt-run-buffer)
       (define-key map (kbd "C-c r") #'csvdt-run-region)
       (define-key map (kbd "C-c d") #'csvdt-run-dwim)
       map))
    ;; The region has to be live for the commands that look
    ;; for one, and -Q leaves the mode on -- but a run that
    ;; inherited it off would quietly test the fallback path
    ;; three times instead.
    (transient-mark-mode 1)
    (deactivate-mark)
    (execute-kbd-macro
     (kbd (concat (or before "C-c t") " " keys)))))

(defun csvdt-tty-record (name got want)
  "Record NAME as reached, and as passed when GOT equals WANT."
  (if (equal got want)
      (push (format "  ok   %s" name) csvdt-tty-log)
    (setq csvdt-tty-failures (1+ csvdt-tty-failures))
    (push (format "  FAIL %s\n    got:  %S\n    want: %S" name got want)
          csvdt-tty-log)))

(defun csvdt-tty-check (name content keys want &optional before)
  "Type KEYS over a buffer holding CONTENT, expecting WANT, and record NAME.
KEYS are appended to the binding that starts the command, so they are read
by the minibuffer exactly as a person typing them would be.

BEFORE is typed first, in the buffer, before that binding.  It is where a
region gets marked: doing it with keys rather than by setting the mark here
is the whole point of this file, since whether a region counts as live is
settled by the command loop and not by the caller.

BEFORE may name a different binding to finish with, as \"keys C-c r\" does,
so that the command under test is reached the way it would be reached."
  (csvdt-tty-record
   name
   (condition-case err
       (progn (csvdt-tty-drive content keys before)
              (with-current-buffer "*csvdt output*" (buffer-string)))
     (error (format "SIGNAL %S" err)))
   want))

(defun csvdt-tty-check-history (name)
  "Record NAME on which history list a submitted line was filed under.
Every other check here reads the output buffer, and a line submitted at
the prompt reaches that buffer the same way whichever list it was
remembered in -- so none of them can tell csvdt's own history from the
one every minibuffer shares.  Passing nil for `read-from-minibuffer\\='s
HIST argument leaves all ten of them green while sending csvdt's command
lines to `minibuffer-history', where an unrelated prompt offers them on
\\[previous-history-element] and offers its own back here.

Both lists are bound afresh, so what is asked is where this run's line
went and not what some earlier one left behind."
  (csvdt-tty-record
   name
   (condition-case err
       (let ((csvdt--arguments-history nil)
             (minibuffer-history nil))
         (csvdt-tty-drive csvdt-tty-columns "RET")
         (list csvdt--arguments-history minibuffer-history))
     (error (format "SIGNAL %S" err)))
   (list (list csvdt-default-arguments) nil)))

(defun csvdt-tty-run ()
  "Run the keystroke tests and exit, reporting the result."
  (let ((csvdt--arguments-history nil)
        (csvdt-default-arguments "-H -p"))

    ;; RET alone submits the prefill as it stands, selected or not.
    (csvdt-tty-check "RET submits the prefill" csvdt-tty-timestamps
                     "RET"
                     "ts,other\n2026-07-25T12:00:00+02:00,x\n")

    ;; The line above typed straight over the selected prefill, so the header
    ;; options are gone and the header is parsed as data.  If the prefill had
    ;; merely been appended to, this would have run with them.
    (csvdt-tty-check "typing replaces the selected prefill" csvdt-tty-timestamps
                     "- u 0 SPC - i SPC 0 , r e p l a c e RET"
                     "parse_err,other\n2026-07-25T10:00:00+00:00,x\n")

    ;; The same line after keeping the prefill: now the header is a header.
    (csvdt-tty-check "right arrow keeps it, to be added to"
                     csvdt-tty-timestamps
                     "<right> SPC - u 0 SPC - i SPC 0 , r e p l a c e RET"
                     "to_utc,other\n2026-07-25T10:00:00+00:00,x\n")

    ;; C-f does the same, for anyone whose hands stay on the home row.
    (csvdt-tty-check "C-f keeps it too" csvdt-tty-timestamps
                     "C-f SPC - u 0 SPC - i SPC 0 , r e p l a c e RET"
                     "to_utc,other\n2026-07-25T10:00:00+00:00,x\n")

    ;; The case a comma-separated reader broke: the comma has to stay inside
    ;; the argument rather than ending it.
    (csvdt-tty-check "a comma typed into a value stays in it"
                     csvdt-tty-columns
                     "<right> SPC - a SPC 1 , 0 RET"
                     "b,a,c\n1,0,2\n")

    ;; TAB completes a partial long option from what csvdt itself reports.
    (csvdt-tty-check "TAB completes a partial option" csvdt-tty-columns
                     "<right> SPC - - a r r TAB SPC 2 , 0 RET"
                     "c,a,b\n2,0,1\n")

    ;; And the line just used can be recalled and run again.  C-a C-k clears
    ;; the prefill first, so what runs is the recalled line alone.
    (csvdt-tty-check "M-p recalls the previous line" csvdt-tty-columns
                     "C-a C-k M-p RET"
                     "c,a,b\n2,0,1\n")

    ;; And that the line was remembered in csvdt's own history rather than in
  ;; the one every minibuffer shares.  The check above says a previous line
  ;; comes back; it cannot say which list it came back from, and both hold
  ;; the same single line within one run.
  (csvdt-tty-check-history "the line is filed under csvdt's own history")

  ;; A region marked with real keys.  Every check above runs the whole
    ;; buffer, and the batch tests reach the region by setting the mark
    ;; themselves -- which is the one thing they cannot do the way a person
    ;; does it.  `use-region-p' consults `transient-mark-mode' and whether
    ;; the mark was activated by a command, and only the command loop
    ;; settles either, so a region that is live here may be dead in batch
    ;; and the other way about.
    ;;
    ;; M-< to the header, C-n onto the first data row, C-SPC to mark, then
    ;; two more C-n: the region covers the first two data rows and stops at
    ;; the start of the third.  What comes back says which records were sent,
    ;; and the two rows it leaves out are what makes the answer worth having.
    (csvdt-tty-check "a region marked by hand sends only its own records"
                     csvdt-tty-rows
                     "C-a C-k RET"
                     "0,zero\n1,one\n"
                     "M-< C-n C-SPC C-n C-n C-c r")

    ;; The same selection through the do-what-I-mean command, which has to
    ;; notice the region for itself rather than being told.
    (csvdt-tty-check "and dwim finds that region without being told"
                     csvdt-tty-rows
                     "C-a C-k RET"
                     "0,zero\n1,one\n"
                     "M-< C-n C-SPC C-n C-n C-c d")

    ;; With no region, dwim falls back to the whole buffer.  The pair is what
    ;; makes either worth anything: a dwim that always ran everything would
    ;; pass the check above's opposite and fail this one, and a dwim that
    ;; never ran anything would fail both.
    (csvdt-tty-check "and runs the whole buffer when there is none"
                     csvdt-tty-rows
                     "C-a C-k RET"
                     "a,b\n0,zero\n1,one\n2,two\n3,three\n"
                     "C-c d"))

  ;; Counted, not just tallied.  ALL PASS was printed whenever nothing had
  ;; failed, which is also true of a run where nothing was tried -- a check
  ;; commented out while something was being chased, a `let' that returned
  ;; early, an edit that left the calls unreachable.  This file is the only
  ;; keystroke cover there is, and it runs under a pty in CI where nobody
  ;; reads the lines above the verdict.
  (setq csvdt-tty-ran (length csvdt-tty-log))
  (let* ((ran csvdt-tty-ran)
         (expected csvdt-tty-expected)
         (report (concat (string-join (nreverse csvdt-tty-log) "\n") "\n"
                        (cond
                         ((< ran expected)
                          (format "ONLY %d OF %d CHECKS RAN\n" ran expected))
                         ((zerop csvdt-tty-failures) "ALL PASS\n")
                         (t (format "%d FAILURE(S)\n" csvdt-tty-failures)))))
        ;; Reported through a file rather than the terminal: Emacs is drawing a
        ;; frame on that terminal, and the report would be lost in the escape
        ;; sequences.  tty-test.sh supplies the path and prints what lands here.
        (destination (getenv "CSVDT_TTY_REPORT")))
    (if destination
        (with-temp-file destination (insert report))
      (send-string-to-terminal report)))
  ;; Read from the count taken above: `nreverse' has since eaten the list
  ;; the report was built from, so asking its length here answers 1.
  (kill-emacs (if (and (zerop csvdt-tty-failures)
                       (>= csvdt-tty-ran csvdt-tty-expected))
                  0 1)))

(defun csvdt-tty-give-up ()
  "Report and exit if the tests are still running.
A key sequence that leaves the minibuffer open waits for input that is
never coming, and Emacs under a pty has no other reason to stop -- so a
broken expectation would hang rather than fail.  Reported as a failure,
since a run that cannot finish has not passed."
  (let ((destination (getenv "CSVDT_TTY_REPORT"))
        (report (concat (string-join (nreverse csvdt-tty-log) "\n")
                        (if csvdt-tty-log "\n" "")
                        "TIMED OUT waiting for the keystroke tests to finish\n")))
    (if destination
        (with-temp-file destination (insert report))
      (send-string-to-terminal report)))
  (kill-emacs 1))

;; Run once Emacs has a terminal to read keys from.
(add-hook 'emacs-startup-hook
          (lambda ()
            (run-with-idle-timer 0.3 nil #'csvdt-tty-run)
            (run-with-timer 60 nil #'csvdt-tty-give-up)))

;;; tty-test.el ends here

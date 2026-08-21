;;; csvdt.el --- Front end for the csvdt command-line tool  -*- lexical-binding: t; -*-

;; Copyright (C) 2025-2026 Michael A Jones

;; Author: Michael A Jones <yardquit@pm.me>
;; URL: https://github.com/YardQuit/csvdt
;; Version: 1.0.0
;; Package-Requires: ((emacs "27.1"))
;; SPDX-License-Identifier: GPL-3.0-or-later

;; This program is free software: you can redistribute it and/or modify it
;; under the terms of the GNU General Public License as published by the Free
;; Software Foundation, either version 3 of the License, or (at your option)
;; any later version.  See the LICENSE file at the root of this repository.

;;; Commentary:

;; Runs csvdt on the current buffer or region and shows the result in another
;; buffer, which pairs with `csv-mode' for looking at the output.
;;
;; Almost nothing about csvdt's options is written down here.  The help
;; buffer is csvdt's own `--help' output, and the completion candidates come
;; from `csvdt --list-options', which csvdt prints from its own argument
;; definitions for exactly this purpose.  A csvdt that gains an option is
;; therefore usable from Emacs immediately, with no change to this file --
;; which is the whole point of doing it this way.
;;
;; Three options are named, in defcustoms, and only because a region run
;; cannot be worked out without them: which flag says the first record is a
;; header, and which two set the quote and the delimiter, since where a
;; record begins depends on both.  A test asserts that each is an option
;; csvdt still reports, and another asserts that no other option name
;; appears in this file at all, so neither the three nor the silence around
;; them can go stale unnoticed.
;;
;; A csvdt too old to know `--list-options', or one printing a format version
;; this file does not read, falls back to parsing the option names out of
;; `--help'.

;;; Code:

(require 'ansi-color)
(require 'delsel)
(require 'seq)
(require 'subr-x)

(defgroup csvdt nil
  "Front end for the csvdt command-line tool."
  :group 'tools
  :prefix "csvdt-")

(defcustom csvdt-executable "csvdt"
  "Name of, or path to, the csvdt executable."
  :type 'string
  :group 'csvdt)

(defcustom csvdt-default-arguments "-H -p"
  "Arguments prefilled when reading a csvdt command line.
The default assumes a file with a header row, as buffers visited with
`csv-mode' usually have, and asks for the header to be printed.

It is prefilled *selected*, so typing replaces it rather than adding to
it, and it takes a deliberate keystroke to keep.  That matters because
the two possible mistakes are not symmetric: passing the header option to
a file that has no header is quiet -- the first row is taken as a header
and, depending on the insert position, either goes through unconverted or
is replaced by the generated header name and lost -- while leaving it off
a file that does have one is loud, the header coming back as a parse
error marker.  A prefill that had to be noticed and deleted pointed at
the quiet mistake; one that has to be kept does not."
  :type 'string
  :group 'csvdt)

(defcustom csvdt-region-carries-header t
  "Whether a region run is given the buffer's header line.
A region selected below the header has no header of its own, so csvdt
would take its first row as one -- and quietly, since a header is exempt
from whatever conversion is being applied: the row is either passed
through unconverted or, where the action replaces its column, lost
altogether.  With this on, the buffer's real header line is put in front
of the region, so the whole selection is data and the output is named
after the columns it actually came from.

Only when the arguments ask for a header, and only when the region starts
below it.  See `csvdt-header-options'."
  :type 'boolean
  :group 'csvdt)

(defcustom csvdt-header-options '("-H" "--has-header")
  "The csvdt options that say the first record is a header.
Named because `csvdt-region-carries-header' cannot work without knowing
which they are.  Everything else is discovered from the binary.

A test asserts that every form here is an option csvdt actually reports,
so this cannot quietly go stale: if it were ever renamed, the check fails
rather than the feature silently doing nothing."
  :type '(repeat string)
  :group 'csvdt)

(defcustom csvdt-single-quote-options '("--single-quote")
  "The csvdt options that make the quote character the single one.
Named for the same reason as `csvdt-header-options': finding where a
record begins means knowing which character quotes a field, and a run
told to read single quotes has its records in different places.

Covered by the same test, so a rename cannot leave it silently wrong."
  :type '(repeat string)
  :group 'csvdt)

(defcustom csvdt-read-delimiter-options '("--read-delimiter")
  "The csvdt options that set the field delimiter for reading.
Named for the same reason as `csvdt-single-quote-options': a quote only
opens a field where a field begins, and where a field begins depends on
the delimiter.

Covered by the same test, so a rename cannot leave it silently wrong."
  :type '(repeat string)
  :group 'csvdt)

(defcustom csvdt-banked-lines-function nil
  "Where banked lines come from, when something banks them.
Banking -- picking out lines here and there and working on those, rather
than on one stretch -- is not this package's to provide, and packages that
do it already exist.  This is how one is plugged in.

The value is a function of no arguments, called in the buffer being
processed, returning either a list of (BEG . END) positions -- integers
or markers -- or a list of overlays.  Ranges are put in buffer order and
widened to whole lines -- and out to whole records, where a quoted field
spans several -- before use, so an implementation need not do either.

nil, the default, means nothing is banked and the region or the buffer is
used as before."
  :type '(choice (const :tag "Nothing banks lines" nil) function)
  :group 'csvdt)

(defcustom csvdt-output-mode
  (if (fboundp 'csv-mode) 'csv-mode 'fundamental-mode)
  "Major mode for the buffer csvdt output is written to.
A mode that is not available leaves the buffer in Fundamental mode rather
than losing the run it was already produced by."
  :type 'function
  :group 'csvdt)

(defconst csvdt--option-list-format 1
  "Version of csvdt's `--list-options' format this file knows how to read.
csvdt prints it on the first line.  An unrecognised version means the
fields may no longer mean what is assumed below, so the output is
declined rather than misread.")

(defvar csvdt--options-cache nil
  "List of (PROGRAM MTIME VERSION . RECORDS) from the last option discovery.
Keyed on the program's path and modification time, so a csvdt upgrade
invalidates it by itself without a subprocess being spawned to ask: the
cache used to be checked against `csvdt --version\', which meant one run
of the binary per lookup -- and a single command looks options up once
per argument, so a run with four arguments forked csvdt several times to
learn nothing had changed.  VERSION is kept for anything that wants to
say which csvdt the records came from.")

;;; Talking to the binary

(defun csvdt--program ()
  "Return the csvdt executable, or signal if it cannot be found."
  ;; A name carrying a directory part means that file, never a PATH
  ;; search -- execvp's rule, and the shell's.  executable-find does not
  ;; hold to it: handed "./csvdt", it joins the name onto every
  ;; `exec-path' entry, so a same-named binary installed anywhere on
  ;; PATH silently won over the file the user actually named.  Only a
  ;; bare name is PATH-searched now; anything with a slash is taken as
  ;; the file it names.
  (or (if (file-name-directory csvdt-executable)
          ;; Not a directory, which file-executable-p is also true of: a
          ;; path mis-set to one would otherwise come back as the program
          ;; and fail later, in call-process's words rather than this
          ;; one's.  Expanded before it is returned, so call-process and
          ;; the option cache's mtime key mean one file, rather than
          ;; whichever one the current directory makes of a relative
          ;; name.
          (and (file-executable-p csvdt-executable)
               (not (file-directory-p csvdt-executable))
               (expand-file-name csvdt-executable))
        (executable-find csvdt-executable))
      ;; Named for what is actually the matter. All three of these used to
      ;; come back as "Cannot find", which is the one thing a file sitting
      ;; right there at that path is not: the reader checks with ls, sees it,
      ;; and goes looking for the wrong problem.
      (cond
       ((and (file-name-directory csvdt-executable)
             (file-directory-p csvdt-executable))
        (user-error "`%s' is a directory, not the csvdt program.  \
Set `csvdt-executable' to the program itself" csvdt-executable))
       ((and (file-name-directory csvdt-executable)
             (file-exists-p csvdt-executable))
        (user-error "`%s' is not executable.  chmod +x it, or set \
`csvdt-executable' to one that is" csvdt-executable))
       ((file-name-directory csvdt-executable)
        (user-error "Cannot find `%s'.  Set `csvdt-executable'"
                    csvdt-executable))
       (t
        ;; A bare name is looked for along `exec-path', which is not the
        ;; PATH of the shell that started Emacs unless something arranged
        ;; for it to be -- the usual reason a program on the terminal's
        ;; PATH is not found here.
        (user-error "Cannot find `%s' on `exec-path'.  Set \
`csvdt-executable' to its full path" csvdt-executable)))))

(defun csvdt--output-of (&rest args)
  "Run csvdt with ARGS and no input, returning its standard output.
Returns nil when csvdt exits non-zero, so callers can degrade rather than
break if a future version declines the arguments."
  (with-temp-buffer
    ;; The same commitment csvdt--run makes: csvdt speaks UTF-8 in both
    ;; directions.  --help held a curly quote when this was written, and
    ;; leaving the decoding to the locale rendered it as mojibake in the help
    ;; buffer anywhere the locale is not UTF-8.  The help corpus is ASCII now
    ;; -- the manual page rendered from it has to be -- so nothing in it is
    ;; misread today; this stays because the commitment is to the encoding
    ;; csvdt writes rather than to what it currently happens to say, and any
    ;; non-ASCII returning to --help or --list-options would be corrupted the
    ;; same way.
    (let* ((coding-system-for-read 'utf-8-unix)
           (coding-system-for-write 'utf-8-unix)
           (status (apply #'call-process (csvdt--program) nil
                          (list t nil) nil args)))
      (and (eq status 0) (buffer-string)))))

(defun csvdt-version-string ()
  "Return csvdt's version line, including the bundled time zone database."
  (string-trim (or (csvdt--output-of "--version") "")))

;;; Help, straight from the binary

(defun csvdt--help-text ()
  "Return csvdt's `--help' output."
  (or (csvdt--output-of "--help")
      (user-error "`%s --help' failed" csvdt-executable)))

;;;###autoload
(defun csvdt-help ()
  "Show csvdt's own `--help' output in a help buffer.
The text comes from the binary on each invocation, so it always describes
the csvdt actually installed."
  (interactive)
  (let ((text (csvdt--help-text)))
    (with-help-window "*csvdt help*"
      (with-current-buffer standard-output
        (insert text)
        (ansi-color-apply-on-region (point-min) (point-max))))))

;;; Option discovery, from the binary's own list

(defun csvdt--parse-option-list (text)
  "Return option records from TEXT, csvdt's `--list-options' output.
Each record is a plist with :short, :long, :value and :separator, from
the tab-separated fields csvdt prints.  :value is nil when it is not
known, which is how the `--help' fallback below reports it.

Returns nil when TEXT is not a format this file reads, so the caller can
fall back to `--help' instead."
  (let ((lines (split-string (or text "") "\n" t))
        (records '()))
    (when (string-match-p (format "\\`# csvdt-option-list %d\\'"
                                  csvdt--option-list-format)
                          (or (car lines) ""))
      (dolist (line (cdr lines))
        (unless (string-prefix-p "#" line)
          (let ((fields (split-string line "\t")))
            (push (list :short (nth 0 fields)
                        :long (nth 1 fields)
                        :value (nth 2 fields)
                        :separator (nth 3 fields))
                  records))))
      (nreverse records))))

;;; Option discovery, fallback for a csvdt without --list-options

(defun csvdt--parse-options (help)
  "Return option records for what clap lists in HELP.
Matches both the short-and-long form (\"  -x, --xxx <VALUE>\") and the
long-only form (\"      --xxx <VALUE>\"), so no option name appears in this
file at all.  A help layout does not say whether an option takes a value
in any way worth parsing, so :value is left nil, meaning not known --
and `csvdt--file-arguments\=' declines to guess at filenames without that
knowledge, rather than mistaking an option's value for one."
  (let ((records '())
        (case-fold-search nil))
    (with-temp-buffer
      (insert help)
      (goto-char (point-min))
      (while (re-search-forward
              "^[ \t]+\\(?:-\\([A-Za-z]\\), \\)?--\\([A-Za-z][A-Za-z0-9-]*\\)"
              nil t)
        (let ((long (concat "--" (match-string 2))))
          ;; csvdt's own help text names options in its prose, and a wrapped
          ;; line can start with one, so the same option is matched more than
          ;; once.  First match wins -- that is the entry in the options list,
          ;; the later ones being descriptions of it.
          (unless (seq-find (lambda (record) (equal (plist-get record :long) long))
                            records)
            (push (list :short (if (match-string 1)
                                   (concat "-" (match-string 1))
                                 "")
                        :long long
                        :value nil
                        :separator nil)
                  records)))))
    (nreverse records)))

(defun csvdt--option-records ()
  "Return csvdt's options as records, cached per binary.
Asks for the machine-readable list first and reads `--help' only if that
is unavailable, so an older csvdt still works.

The cache is validated against the program's path and modification time,
which a lookup can check without running anything.  A rebuilt csvdt gets
a fresh mtime and so a fresh read; the version string is fetched once at
that point and stored alongside, not consulted per call."
  (let* ((program
          ;; The real file, not a symlink to it: `file-attributes\=' describes
          ;; the link itself, whose mtime never moves, so a csvdt rebuilt
          ;; behind a stable ~/bin symlink would never invalidate the cache.
          (file-truename (csvdt--program)))
         (mtime (file-attribute-modification-time (file-attributes program))))
    (if (and (equal (nth 0 csvdt--options-cache) program)
             (equal (nth 1 csvdt--options-cache) mtime))
        (nthcdr 3 csvdt--options-cache)
      (let ((records
             (or (csvdt--parse-option-list (csvdt--output-of "--list-options"))
                 (csvdt--parse-options (csvdt--help-text)))))
        (setq csvdt--options-cache
              (append (list program mtime (csvdt-version-string)) records))
        records))))

(defun csvdt--cached-version ()
  "Return csvdt's version line without spawning it when the cache is warm.
The prompt shows the version on every command, and reading it live undid
half of what the mtime-keyed cache saved: one spawn per command, for a
string that only changes when the binary does -- exactly what the cache
already tracks.  `csvdt-version-string\=' stays live for the command that
exists to ask."
  (csvdt--option-records)
  (nth 2 csvdt--options-cache))

(defun csvdt-options ()
  "Return the option strings to offer as completion candidates.
An option whose value must be attached with \"=\" is offered with it, since
an argument built without it would be read as a filename instead."
  (let ((options '()))
    (dolist (record (csvdt--option-records))
      (dolist (form (list (plist-get record :short) (plist-get record :long)))
        (unless (or (null form) (string-empty-p form))
          (push form options)
          (when (equal (plist-get record :separator) "equals")
            (push (concat form "=") options)))))
    (nreverse (delete-dups options))))

;;; Reading a command line

;; The line is read whole and split the way a shell would, NOT as a
;; comma-separated list of arguments.  A comma is csvdt's own separator inside
;; a value -- a column list, a column-and-position pair, a column-and-offset
;; pair -- so splitting the line on commas tears those arguments in half and
;; hands csvdt nonsense.  Completion is therefore per token rather than over
;; the whole line, which is what TAB in the minibuffer does below.

(defvar csvdt--arguments-history nil
  "Minibuffer history of csvdt command lines.")

(defun csvdt--keep-prefill (n)
  "Keep the selected prefilled arguments and move past them.
With nothing selected this is an ordinary `forward-char' of N, so the key goes
back to behaving as it always does once the prefill has been dealt with.
Bound rather than left to plain motion because point already sits at the
end of the prefill, where moving forward would only ring the bell."
  (interactive "p")
  (if (use-region-p)
      (progn (deactivate-mark) (goto-char (point-max)))
    (forward-char n)))

(defvar csvdt--arguments-map
  (let ((map (make-sparse-keymap)))
    (set-keymap-parent map minibuffer-local-map)
    (define-key map (kbd "TAB") #'completion-at-point)
    (define-key map (kbd "<right>") #'csvdt--keep-prefill)
    (define-key map (kbd "C-f") #'csvdt--keep-prefill)
    map)
  "Keymap used while reading a csvdt command line.
TAB completes the argument at point.  `minibuffer-local-map' is the
parent rather than `minibuffer-local-completion-map', because the line is
a list of arguments and only some of them are options -- SPC and comma
have to insert themselves.")

(defun csvdt--complete-argument ()
  "Complete the whitespace-delimited argument before point.
An entry for `completion-at-point-functions' while a command line is
being read."
  (let ((end (point))
        (beg (save-excursion
               ;; The same separator set `csvdt--split-arguments' honors,
               ;; newline and its siblings included: skipping over only
               ;; space and tab made a pasted "-H\n--arr" one completion
               ;; token, where RET would have split it in two.
               (skip-chars-backward "^ \t\n\r\f\v" (minibuffer-prompt-end))
               (point))))
    (list beg end (ignore-errors (csvdt-options)) :exclusive 'no)))

(defun csvdt--closing-quote (line start quote)
  "Index in LINE of the QUOTE closing a run that opened before START, or nil.
Inside a double-quoted run a backslash escapes the next character, so an
escaped quote does not close it.  A single-quoted run has no escapes at
all, which is what a shell does with them and what lets one hold a
backslash as itself."
  (let ((index start)
        (length (length line))
        (found nil))
    (while (and (not found) (< index length))
      (let ((char (aref line index)))
        (cond
         ((and (eq quote ?\") (eq char ?\\) (< (1+ index) length))
          (setq index (+ index 2)))
         ((eq char quote)
          (setq found index))
         (t (setq index (1+ index))))))
    found))

(defun csvdt--split-arguments (line)
  "Split LINE into arguments the way a shell would.
A quoted argument may hold a space; an unquoted one may hold a comma,
which is what csvdt's own value lists need.

Both quote characters, because the examples people paste come from a
shell and a shell takes either.  Only the double one used to be
understood, so a pasted \\='#\\=' arrived as three characters and csvdt
refused it -- clearly, but for a reason that was about this prompt rather
than about the option.

A double-quoted run honours the shell-familiar backslash escapes --
\\t \\n \\r \\\\ \\\" and no others, so \\=\"\\t\" is a tab and
\\=\"C:\\data\" is refused rather than read as Lisp's DEL escape --
while a single-quoted one is literal throughout.  So a lone quote
character is written by wrapping it in the other kind, and a value that
must keep its backslashes belongs in single quotes.

Outside quotes a backslash is left as itself, which is not the shell's
rule but is the one this prompt has always had -- a Windows path typed
as a value has to survive, and there is no filename here for a shell's
escaping to be worth more than that."
  (let ((index 0)
        (length (length line))
        (args '())
        (current nil))
    (while (< index length)
      (let ((char (aref line index)))
        (cond
         ;; Whitespace outside quotes ends the argument being gathered.
         ;; Newlines included: a line pasted with its trailing newline used
         ;; to fold it into the last argument, which csvdt then refused --
         ;; `split-string-and-unquote\=' split on them, and this must too.
         ;; Formfeed and vertical tab likewise: that function split on
         ;; them, and dropping them from the set would fold one into an
         ;; argument where it never was before.
         ((memq char '(?\s ?\t ?\n ?\r ?\f ?\v))
          (when current (push current args) (setq current nil))
          (setq index (1+ index)))
         ((memq char '(?\" ?'))
          (let ((end (csvdt--closing-quote line (1+ index) char)))
            (unless end (user-error "Unbalanced quotes in: %s" line))
            (setq current
                  (concat (or current "")
                          (if (eq char ?')
                              (substring line (1+ index) end)
                            ;; Only the shell-familiar escapes reach the
                            ;; reader.  It knows more of them -- \d is DEL and
                            ;; \a is a bell -- so "C:\data" read raw became
                            ;; C-followed-by-DEL-ata with no error, while
                            ;; "C:\Users" tripped over \U and was refused: the
                            ;; same path corrupted or caught by the letter
                            ;; after the backslash.  Now every sequence
                            ;; outside the familiar set is answered the same
                            ;; way, with the way out: single quotes take
                            ;; backslashes literally.
                            (let ((run (substring line (1+ index) end))
                                  (scan 0))
                              (while (< scan (length run))
                                (if (not (eq (aref run scan) ?\\))
                                    (setq scan (1+ scan))
                                  (unless (and (< (1+ scan) (length run))
                                               (memq (aref run (1+ scan))
                                                     '(?t ?n ?r ?\\ ?\")))
                                    (user-error
                                     "Cannot read %s: inside double quotes a backslash starts an escape, and only \\t \\n \\r \\\\ \\\" are accepted.  Use single quotes, which take backslashes literally"
                                     (substring line index (1+ end))))
                                  (setq scan (+ scan 2))))
                              (condition-case nil
                                  (or (car (split-string-and-unquote
                                            (substring line index (1+ end))))
                                      "")
                                (error
                                 (user-error
                                  "Cannot read %s: inside double quotes a backslash starts an escape (\"\\t\" is a tab).  Use single quotes, which take backslashes literally"
                                  (substring line index (1+ end))))))))
                  index (1+ end))))
         (t
          (setq current (concat (or current "") (string char))
                index (1+ index))))))
    (when current (push current args))
    (nreverse args)))

(defun csvdt--replace-selection ()
  "Let a self-inserting command replace the selected prefill.
`delete-selection-pre-hook' declines unless the global
`delete-selection-mode' is on, so its helper is called directly: the
selection is this package's doing, and should behave the same for
everyone.  The helper is what knows which commands replace a selection
and which supersede it, so that judgement is left to it."
  (when (and (use-region-p) (not buffer-read-only))
    (delete-selection-helper (and (symbolp this-command)
                                  (get this-command 'delete-selection)))))

(defun csvdt--select-prefill ()
  "Leave the prefilled arguments selected, point at their end.
Typing then replaces them, \\<csvdt--arguments-map>\\[csvdt--keep-prefill]
keeps them to be added to, and RET submits them as they stand."
  (when (> (point-max) (minibuffer-prompt-end))
    ;; Locally, so a selection still counts as one for someone who turns
    ;; `transient-mark-mode' off globally.
    (setq-local transient-mark-mode t)
    (add-hook 'pre-command-hook #'csvdt--replace-selection nil t)
    (goto-char (point-max))
    (set-mark (minibuffer-prompt-end))
    (activate-mark)))

(defun csvdt--read-arguments ()
  "Read a csvdt command line, completing on options csvdt reports itself.
`csvdt-default-arguments' is prefilled and left selected, so typing
replaces it and \\<csvdt--arguments-map>\\[csvdt--keep-prefill] keeps it."
  (csvdt--split-arguments
   (minibuffer-with-setup-hook
       (lambda ()
         (add-hook 'completion-at-point-functions
                   #'csvdt--complete-argument nil t)
         (csvdt--select-prefill))
     (read-from-minibuffer
      (format "csvdt arguments (%s): " (csvdt--cached-version))
      csvdt-default-arguments csvdt--arguments-map nil
      'csvdt--arguments-history))))

;;; Running it

(defun csvdt--apply-output-mode ()
  "Put the current buffer in `csvdt-output-mode'.
A mode that turns out not to be available leaves the buffer plain rather
than aborting a run csvdt has already done the work for."
  ;; A mode that exists and then signals costs the same run as one that
  ;; does not exist, and used to cost more: the error escaped past the
  ;; line count, the stderr notes and the window, leaving the result in a
  ;; buffer nobody was shown.  Both failures now end the same way.
  (if (fboundp csvdt-output-mode)
      (condition-case nil
          (funcall csvdt-output-mode)
        (error (fundamental-mode)))
    (fundamental-mode)))

(defun csvdt--short-flags (argument)
  "Return the short flags clustered in ARGUMENT, or nil if it is not a cluster.
clap lets flags be written together, so \"-Hp\" is two of them.  A short
option whose value can be attached consumes the rest of the argument, so
scanning stops at the first of those -- which is what makes
\"-i0,replace\" one option and its value rather than a run of flags.
A short whose value must be attached with \"=\" does not: nothing after
it in a cluster can be its value, so to clap it is just another flag and
scanning walks over it -- \"-fH\" is two flags, and treating it as \"-f
plus a value\" hid the \"-H\" from `csvdt--header-requested-p\'.  Which
options are which comes from csvdt itself; when it does not say, none
are assumed and only whole arguments match."
  (when (and (string-prefix-p "-" argument)
             (not (string-prefix-p "--" argument))
             ;; An '=' anywhere means option=value to clap -- "-f=keep" is
             ;; -f with an attached mode, never a cluster. Scanned as one it
             ;; yielded junk flags ("-=" "-k" ...), harmless under the
             ;; default `csvdt-header-options\=' but a false header-carry
             ;; the moment that defcustom names a letter the value holds.
             (not (string-match-p "=" argument))
             (> (length argument) 1))
    (let ((valued (csvdt--shorts-taking-a-separate-value))
          (flags '())
          (index 1)
          (stop nil))
      (while (and (not stop) (< index (length argument)))
        (let ((short (concat "-" (substring argument index (1+ index)))))
          (if (member short valued)
              (setq stop t)
            (push short flags)
            (setq index (1+ index)))))
      (nreverse flags))))

(defun csvdt--shorts-taking-a-separate-value ()
  "Return the short options whose value may be the next argument.
An option whose value must be attached with \"=\" takes no separate one, so
writing it apart leaves that word sitting where the filename goes.  That is
the case this exists to notice rather than to allow for."
  (let ((shorts '()))
    (dolist (record (csvdt--option-records))
      (let ((short (plist-get record :short)))
        (when (and short (not (string-empty-p short))
                   (member (plist-get record :value) '("required" "optional"))
                   (not (equal (plist-get record :separator) "equals")))
          (push short shorts))))
    shorts))

(defun csvdt--expects-a-following-value (argument)
  "Non-nil when ARGUMENT is an option whose value is the argument after it.
Nil when the value is attached, when the option takes none, and when it
must be attached with \"=\"."
  (cond
   ((string-prefix-p "--" argument)
    (and (not (string-match-p "=" argument))
         (seq-some (lambda (record)
                     (and (equal (plist-get record :long) argument)
                          (member (plist-get record :value)
                                  '("required" "optional"))
                          (not (equal (plist-get record :separator) "equals"))))
                   (csvdt--option-records))))
   ((and (string-prefix-p "-" argument) (> (length argument) 1))
    ;; A cluster ends at the first short that takes a value, and only takes a
    ;; separate one if nothing follows it in the argument: "-Hu 0" does, and
    ;; "-Hu0" carries its own.
    (let ((valued (csvdt--shorts-taking-a-separate-value))
          (index 1)
          (expects nil))
      (while (< index (length argument))
        (if (member (concat "-" (substring argument index (1+ index))) valued)
            (setq expects (= index (1- (length argument)))
                  index (length argument))
          (setq index (1+ index))))
      expects))))

(defun csvdt--file-arguments (args)
  "Return the arguments in ARGS that csvdt would read as a file to open.
Only ones that name something on disk: an argument left where the filename
goes is worth reporting when csvdt would silently read it, and when it
would not, csvdt's own error says so better than a guess from here could.
That matters because these commands always send the input on standard
input, so a filename does not add a second input -- it replaces the
selection, and the result looks exactly like a successful run on it.

Nothing is reported at all when the option table cannot say which
options take a separate value -- the `--help' fallback's records, where
:value is nil throughout.  Telling a value from a positional then needs
exactly the knowledge that is missing, and guessing refused valid runs:
an option's separate value was scanned as a filename, and refused
whenever a file of that name happened to exist.  A missed warning only
returns csvdt to what every run before this check was; a false refusal
stops a correct one."
  (unless (seq-some (lambda (record) (plist-get record :value))
                    (csvdt--option-records))
    (setq args nil))
  (let ((files '())
        (rest args)
        (positional nil))
    (while rest
      (let ((argument (car rest)))
        (cond
         ;; Everything after "--" is positional, whatever it looks like.
         ((and (not positional) (equal argument "--"))
          (setq positional t rest (cdr rest)))
         ((and (not positional) (csvdt--expects-a-following-value argument))
          (setq rest (cddr rest)))
         ((and (not positional)
               (string-prefix-p "-" argument)
               (> (length argument) 1))
          (setq rest (cdr rest)))
         (t
          ;; Not "~" paths, though: `file-regular-p' expands the tilde and
          ;; csvdt does not, so csvdt answers such an argument with its own
          ;; loud cannot-read error -- the silent replacement this check
          ;; exists to catch cannot happen, and flagging it here claimed
          ;; csvdt would read a file it never would.
          (when (and (not (string-empty-p argument))
                     (not (string-prefix-p "~" argument))
                     (file-regular-p argument))
            (push argument files))
          (setq rest (cdr rest))))))
    (nreverse files)))

(defun csvdt--header-requested-p (args)
  "Non-nil when ARGS ask csvdt to treat the first record as a header.
Only up to \"--\": past it everything is positional to clap, so an
argument spelled -H there is a filename, not the option."
  (seq-some
   (lambda (argument)
     (or (member argument csvdt-header-options)
         (seq-intersection (csvdt--short-flags argument) csvdt-header-options)))
   (seq-take-while (lambda (argument) (not (equal argument "--"))) args)))

(defun csvdt--header-line (beg args)
  "Return the buffer's header line to put in front of a region at BEG.
Nil unless ARGS ask for a header and BEG is below it -- a region starting
inside the header line is left alone, since it already carries part of
one and guessing what was meant would be worse than not."
  (when (and csvdt-region-carries-header
             (csvdt--header-requested-p args))
    (save-excursion
      (goto-char (point-min))
      (let* ((dialect (csvdt--dialect args))
             ;; The header's whole record, not its first line.  A header
             ;; whose own field holds a newline spans two, and taking one
             ;; of them handed csvdt a fragment opening a quote that never
             ;; closed -- which swallowed the rows behind it, and failed
             ;; the run for a reason that named neither the header nor the
             ;; field.
             (header-end (cdr (csvdt--whole-records
                               (point-min) (line-beginning-position 2)
                               (car dialect) (cdr dialect)))))
        (when (>= beg header-end)
          (buffer-substring-no-properties (point-min) header-end))))))

(defun csvdt--whole-lines (beg end)
  "Return BEG..END widened to whole lines.
Half a line is half a record whatever else is true: sending part of one
would hand csvdt a truncated row.  A range ending exactly at the start of
a line stops there, having been selected up to the previous line\'s
newline rather than into this one.

Whole lines are not yet whole records -- a quoted field may hold newlines
and carry one record across several.  `csvdt--whole-records\=' finishes
the job; this is the cheap part of it, and the whole of it for a file
whose fields hold no newlines."
  (save-excursion
    (let ((start (progn (goto-char beg) (line-beginning-position)))
          (finish (progn (goto-char end)
                         (if (and (> end beg) (bolp))
                             (point)
                           (min (point-max) (line-beginning-position 2))))))
      (cons start (max finish start)))))

(defun csvdt--dialect (args)
  "Return (QUOTE . DELIMITER) as characters, as ARGS leave them.
Only up to \"--\", on the same reasoning as `csvdt--header-requested-p':
past it clap reads everything as positional."
  (let ((options (seq-take-while (lambda (argument) (not (equal argument "--")))
                                 args)))
    (cons (if (seq-some (lambda (argument)
                          (member argument csvdt-single-quote-options))
                        options)
              ?'
            ?\")
          (or (csvdt--delimiter-in options) ?,))))

(defun csvdt--delimiter-in (options)
  "The delimiter character OPTIONS name, or nil for the default.
Both spellings clap accepts: attached with \"=\" and given as the next
argument."
  (let ((found nil))
    (while (and options (not found))
      (let ((argument (car options)))
        (cond
         ((member argument csvdt-read-delimiter-options)
          (setq found (and (cadr options) (aref (cadr options) 0))))
         (t
          ;; The attached spelling.  Measured off the option's own name
          ;; rather than by looking for the "=", which keeps this to
          ;; Emacs 27's string functions as the rest of the file does.
          (let ((name (seq-find (lambda (name)
                                  (string-prefix-p (concat name "=") argument))
                                csvdt-read-delimiter-options)))
            (when name
              (let ((value (substring argument (1+ (length name)))))
                (setq found (and (not (string-empty-p value))
                                 (aref value 0)))))))))
      (setq options (cdr options)))
    found))

(defun csvdt--field-initial-p (pos delimiter)
  "Whether POS is where a field begins.
The first character of the accessible portion, or the one just past a
DELIMITER or a line ending."
  (or (eq pos (point-min))
      (memq (char-before pos) (list delimiter ?\n ?\r))))

(defun csvdt--inside-quoted-field-p (pos quote delimiter)
  "Non-nil when POS falls inside a quoted field rather than between records.
Read from the start of the accessible portion, since nothing nearer can
say which side of a quote a position is on.  Jumps quote to quote rather
than character to character, so the cost follows how many QUOTE
characters come before POS rather than how much text does -- DELIMITER
says where a field begins, which is the only place a quote opens one:
measured at
0.17 s over 14 MB holding twelve thousand of them, and 1.9 s over 18 MB
where every field is quoted.  A run over a whole buffer has nothing
before it and pays nothing."
  (save-excursion
    (goto-char (point-min))
    (let ((inside nil)
          (needle (char-to-string quote)))
      (while (search-forward needle pos t)
        (if inside
            ;; A doubled quote inside a run is one escaped quote: the run
            ;; goes on, and the second must not be read as opening anything.
            (if (eq (char-after) quote)
                (forward-char 1)
              (setq inside nil))
          ;; Outside, a quote opens a run only where a field begins.
          ;; Anywhere else it is data: a\"b is three literal characters to
          ;; the reader, not the start of something.
          (when (csvdt--field-initial-p (1- (point)) delimiter)
            (setq inside t))))
      inside)))

(defun csvdt--whole-records (beg end quote delimiter)
  "Return BEG..END widened out to whole records.
A record is not always a line.  A quoted field may hold newlines -- an
address, a notes column, an embedded document -- and then one record
spans several lines, which is ordinary CSV and which csvdt reads and
writes faithfully.  Widening to whole lines is therefore not enough on
its own: a range starting inside such a field hands over the tail of a
record as though it were a record of its own.  That parses cleanly,
converts cleanly and reports success, and the field it produces is not
the data -- x\\n y arrives as y\", with nothing to say so.

QUOTE and DELIMITER are the dialect the run will read with, since what
counts as a record depends on both.

Only ever widens.  A range already sitting on record boundaries is
returned unchanged, which is every range in a file whose fields hold no
newlines at all."
  (let ((start beg)
        (finish end))
    ;; Back to the line the record opens on.
    (while (and (> start (point-min))
                (csvdt--inside-quoted-field-p start quote delimiter))
      (setq start (save-excursion
                    (goto-char start)
                    (forward-line -1)
                    (point))))
    ;; And on to the line past the one it closes on.
    (while (and (< finish (point-max))
                (csvdt--inside-quoted-field-p finish quote delimiter))
      (setq finish (save-excursion
                     (goto-char finish)
                     (forward-line 1)
                     (point))))
    (cons start (max finish start))))

(defun csvdt--narrowing-cuts-record-p (ranges quote delimiter)
  "Which end of RANGES the buffer\\='s narrowing cuts a record at.
Returns `start\\=', `end\\=', `both\\=' or nil.  QUOTE and DELIMITER are the
dialect the run will read with, which is what decides where a record ends.
Widening can only reach as far as the accessible portion: a buffer
narrowed to begin or end mid-record has nothing whole to widen to, so the clamp
leaves half a record and nothing says so.  It goes to csvdt as a short
row -- passed through truncated, or, once a conversion is asked for,
refused as a ragged record naming a line rather than the narrowing behind
it.  Widening past the restriction would send lines nobody chose, so the
run reports the cut instead of hiding or overriding it.

Two ways to be cut, because a record is not always a line.  Mid-line is
the plain one.  The other is a narrowing that ends at a line boundary
which is nonetheless inside a quoted field: whole lines are reachable
there, so the mid-line test saw nothing, while the record they belong to
was still severed.  That one could pass quietly: asked to accept ragged
records -- the obvious thing to reach for when a count looks off -- csvdt
took the truncated one rather than refusing it, and a field holding a
newline arrived without the half of itself that was out of reach, exit 0,
with the rows behind it gone."
  (let* ((start (car (car ranges)))
         (finish (cdr (car (last ranges))))
         (at-start
          (and (= start (point-min))
               ;; Widened, because both questions are about text out of
               ;; reach: whether a quote before the restriction is still
               ;; open here, and whether the line this sits on began before
               ;; it.
               (save-restriction
                 (widen)
                 (or (csvdt--inside-quoted-field-p start quote delimiter)
                     (save-excursion
                       (goto-char start)
                       ;; Only the start of a line begins a record.
                       ;; Anywhere else, the first record is the tail of one.
                       (not (bolp)))))))
         (at-end
          (and (= finish (point-max))
               (or
                ;; Ending inside a quoted field: what is reachable stops
                ;; partway through a record however tidy the line looks.
                (csvdt--inside-quoted-field-p finish quote delimiter)
                (save-restriction
                  (widen)
                  (save-excursion
                    (goto-char finish)
                    ;; At end-of-line only the newline is out of reach, and
                    ;; the record itself is whole -- csvdt supplies the
                    ;; terminator.
                    (not (or (bolp) (eolp)))))))))
    (cond ((and at-start at-end) 'both)
          (at-start 'start)
          (at-end 'end))))

(defun csvdt--cut-note (cut)
  "How to say that the narrowing left CUT half-reachable.
One phrasing for both the run summary and the failure message: a run
that fails because of the cut is the case the check exists for, and it
was the one that did not mention it."
  (pcase cut
    ('both "first and last records cut short by the narrowing")
    ('start "first record cut short by the narrowing")
    ('end "last record cut short by the narrowing")))

(defun csvdt--piece-bounds (item)
  "Return ITEM as a sorted (BEG . END) pair in this buffer, or nil.
ITEM is a cons or an overlay, as `csvdt-banked-lines-function\=' and the
region report pieces.  One decoding, shared by `csvdt--normalise-ranges\='
and `csvdt--mid-line-boundary-p\=', so what counts as a live piece cannot
drift between what a run covers and what it says about the selection.

A pair's ends may be integers or markers -- a marker is how a package
holds a position that must survive edits, so pairs of them are what a
banking package most naturally reports.  They were silently dropped
instead, and `csvdt-run-dwim\=', finding nothing banked, ran over the
whole buffer without a word.

nil for a dead overlay, which reports nil for both ends, and for a
marker pointing nowhere; for an overlay or a marker belonging to
another buffer, whose positions mean something else entirely here and
would otherwise be clamped onto lines nobody picked out; and for a
piece starting at or past the end of the buffer, which names no
character -- clamped into range instead, a stale one in a buffer whose
last line has no newline widened onto that line, where \"No banked
lines\" was the truth.

Either way round: a pair is two ends, not a direction, and
`region-beginning\=' and `region-end\=' are the only reason one usually
arrives sorted.  Taking them as given turned a reversed pair into an
empty range, so the lines it named went silently missing."
  (when (or (consp item) (overlayp item))
    (let ((beg (csvdt--piece-position
                (if (overlayp item) (overlay-start item) (car item))))
          (end (csvdt--piece-position
                (if (overlayp item) (overlay-end item) (cdr item))))
          (elsewhere (and (overlayp item)
                          (not (csvdt--same-text-p (overlay-buffer item))))))
      (when (and (integerp beg) (integerp end) (not elsewhere)
                 (< (min beg end) (point-max)))
        (cons (min beg end) (max beg end))))))

(defun csvdt--same-text-p (buffer)
  "Whether BUFFER is this one, or shares its text as an indirect buffer.
An indirect buffer and its base hold the same characters at the same
positions, so a marker or overlay made in one names exactly the text the
other would name -- unlike a position from an unrelated buffer, which is
what the foreign-buffer refusal is for.  `clone-indirect-buffer\=' is an
ordinary way to look at a file, and banking made in the base was refused
there: `csvdt-run-banked\=' said \"No banked lines\", and
`csvdt-run-dwim\=', finding nothing banked, ran over the whole buffer."
  (and (buffer-live-p buffer)
       (eq (or (buffer-base-buffer buffer) buffer)
           (or (buffer-base-buffer (current-buffer)) (current-buffer)))))

(defun csvdt--piece-position (part)
  "Return PART as a position in this buffer, or nil.
PART is one end of a banked piece: an integer, a marker, or whatever a
dead overlay reported an end as (nil).  A marker in an unrelated buffer
gets the foreign overlay's treatment -- its position means something else
entirely here -- and one pointing nowhere has no position to give."
  (cond ((integerp part) part)
        ((and (markerp part)
              (csvdt--same-text-p (marker-buffer part)))
         (marker-position part))))

(defun csvdt--normalise-ranges (items &optional source)
  "Return ITEMS as whole-line (BEG . END) conses, in buffer order.
ITEMS may hold conses or overlays, in any order, overlapping or repeated.
Dead overlays are dropped, every range is widened to whole lines, and
ranges that touch are merged -- so a selection that happens to abut a
banked stretch becomes one, and no line is ever sent twice.
SOURCE names where ITEMS came from, for the error a malformed one raises;
it defaults to `csvdt-banked-lines-function\=', which is the usual source
and the one a reader would think to check."
  ;; The naming below is per item, so a function returning something that
  ;; is not a list at all -- a vector, a number, a string -- reached dolist
  ;; and signalled the raw wrong-type-argument this exists to prevent.
  (unless (listp items)
    (user-error "%s reported %S, which is not a list of positions or overlays"
                (or source "`csvdt-banked-lines-function'") items))
  (let ((ranges '()))
    (dolist (item items)
      ;; nil is tolerated: a package filtering its own list produces them.
      ;; Anything else that is neither a pair of positions nor an overlay
      ;; is a mistake worth naming, rather than a raw wrong-type-argument
      ;; from inside -- and a cons with junk for an end, (5 . nil) say, is
      ;; the same mistake in better disguise: it used to pass this check
      ;; and be dropped without a word further down.
      (unless (or (null item) (overlayp item)
                  (and (consp item)
                       (or (integerp (car item)) (markerp (car item)))
                       (or (integerp (cdr item)) (markerp (cdr item)))))
        (user-error "%s reported %S, which is neither a position pair nor an overlay"
                    (or source "`csvdt-banked-lines-function'") item))
      (let ((bounds (csvdt--piece-bounds item)))
        (when bounds
          (let ((range (csvdt--whole-lines (car bounds) (cdr bounds))))
            ;; A pair naming nothing -- both ends past the buffer, say --
            ;; widens to nothing.  Dropped, so it cannot pad the count of
            ;; stretches or make an empty run look like a successful one.
            (when (< (car range) (cdr range))
              (push range ranges))))))
    (csvdt--merge-ranges ranges)))

(defun csvdt--merge-ranges (ranges)
  "Return RANGES sorted into buffer order, touching or overlapping ones joined.
Fresh conses throughout, so the list handed in is neither reordered nor
rewritten."
  (let ((sorted (sort (copy-sequence ranges)
                      (lambda (a b) (< (car a) (car b)))))
        (merged '()))
    (dolist (range sorted)
      (if (and merged (>= (cdr (car merged)) (car range)))
          (setcdr (car merged) (max (cdr (car merged)) (cdr range)))
        (push (cons (car range) (cdr range)) merged)))
    (nreverse merged)))

(defun csvdt--drop-covered (ranges)
  "Return RANGES with any part an earlier one already covers removed.
Needed after widening, which brings ranges together that were apart: two
banked lines inside one record that spans several widen to that same
record, and it went out once per range -- a duplicate row, valid CSV,
clean-looking, and nothing in a count to question it.

Order is left exactly as given, unlike `csvdt--merge-ranges\='.  Buffer
order is something `csvdt--run\=' asks of its caller rather than something
it imposes, and sorting here would quietly take that decision away from
whoever assembled the list."
  (let ((kept '()))
    (dolist (range ranges)
      (let ((beg (car range))
            (end (cdr range))
            (trimmed nil))
        (dolist (seen kept)
          (when (and (< beg (cdr seen)) (> end (car seen)))
            (setq trimmed t)
            (cond
             ;; Wholly inside something already going out: nothing left.
             ((and (>= beg (car seen)) (<= end (cdr seen))) (setq beg end))
             ;; Overlapping at one end: keep the part that is new.
             ((>= beg (car seen)) (setq beg (cdr seen)))
             ((<= end (cdr seen)) (setq end (car seen))))))
        ;; An empty range that nothing covered is passed on as it came: a
        ;; whole-buffer run over an empty buffer is one of those, and
        ;; dropping it left the run with no range at all.  Only a range
        ;; trimmed away to nothing is really gone.
        (when (or (not trimmed) (< beg end))
          (push (cons beg end) kept))))
    (nreverse kept)))

(defun csvdt-banked-lines-from-overlays (property)
  "Return a function reporting overlays in the buffer that carry PROPERTY.
A fallback for a package that exposes no function of its own: naming the
property it happens to set reaches into its internals, and can break when
those change.  Prefer pointing `csvdt-banked-lines-function\=' straight at
a function the package documents, where there is one.

Most packages that bank lines mark them with an overlay, so where there is
not, this is usually the whole of plugging one in:

    (setq csvdt-banked-lines-function
          (csvdt-banked-lines-from-overlays \='some-package-banked))

To find PROPERTY, put point on a banked line and run
\\[describe-text-properties]: the overlays covering it are listed with the
properties they carry.  Anything but the face is a good candidate, a face
being shared with whatever else highlights that buffer."
  (lambda ()
    (seq-filter (lambda (overlay) (overlay-get overlay property))
                (overlays-in (point-min) (point-max)))))

(defun csvdt--region-ranges ()
  "Return the active region as (BEG . END) conses, or nil when there is none.
`region-bounds\=' rather than `region-beginning\=' and `region-end\=', because
a region need not be one stretch: a rectangle is one piece per line, and a
package that defines its own selection through `region-extract-function\='
reports its pieces the same way.  Taking the two ends instead would swallow
everything lying between them."
  (and (use-region-p) (region-bounds)))

(defun csvdt--banked-lines ()
  "Return what `csvdt-banked-lines-function' reports, or nil.
A function that is named but not defined is a configuration error rather
than an absence of banked lines: it came out as a bare \"Symbol\'s function
definition is void\", which does not say that the setting is what is wrong,
and returning nil instead would run over the whole buffer without a word.
The likeliest cause is the package that defines it not being loaded, or
being older than the function named."
  (when csvdt-banked-lines-function
    (unless (functionp csvdt-banked-lines-function)
      (user-error
       "`csvdt-banked-lines-function' names %S, which is not defined%s"
       csvdt-banked-lines-function
       " -- is the package that provides it loaded, and new enough?"))
    (funcall csvdt-banked-lines-function)))

(defun csvdt--mid-line-boundary-p (items)
  "Non-nil when a piece in ITEMS begins or ends inside a line.
ITEMS are the raw pieces a command was given -- conses or overlays, as
`csvdt--normalise-ranges\' takes them -- asked before normalising widens
them, because afterwards every range is whole lines and nothing is left
to notice.

An end sits on a line boundary at the start of a line, at the end of
one, or at the end of the buffer.  The end of a line, because an
overlay covering a line usually stops before the newline -- that is the
shape `csvdt-banked-lines-from-overlays\=' reports -- and the row it
names is whole: widening adds only the newline, and calling that a
widened selection put the notice on every banked run.  A beginning is
on a boundary only at the start of a line: beginning at the end of one
pulls that entire line in, which is exactly the widening this reports."
  (save-excursion
    (seq-some
     (lambda (item)
       (let ((bounds (csvdt--piece-bounds item)))
         (when bounds
           (let ((low (car bounds))
                 (high (min (cdr bounds) (point-max))))
             ;; A zero-width piece widens to its entire containing line --
             ;; a row the selection held no character of, which is the
             ;; largest widening there is, and was the one case this
             ;; stayed silent about.
             (if (= low high)
                 t
               (or (progn (goto-char low) (not (bolp)))
                   (progn (goto-char high)
                          (not (or (bolp) (eolp)
                                   (= high (point-max)))))))))))
     items)))

(defun csvdt--run (ranges args &optional snapped)
  "Pipe RANGES of this buffer through csvdt with ARGS and show the result.
RANGES is a list of (BEG . END) conses in buffer order; more than one is
how banked lines reach csvdt as a single file.  Ranges below the buffer's
header are sent with that header in front of them, so csvdt does not take
the first selected row for one.  On failure the exit status and csvdt's
own message are reported, since it writes a specific reason for every
refusal; on success anything csvdt said on stderr -- how much did not
convert, rows a re-read would take for comments -- is shown with the
summary rather than thrown away.
SNAPPED says the caller's raw selection had a boundary inside a line, so
the run covers whole rows the selection only touched.  The commands
compute it with `csvdt--mid-line-boundary-p\' before normalising, since
normalised ranges are already whole lines and carry no sign of it."
  (let ((files (csvdt--file-arguments args)))
    (when files
      (user-error
       "csvdt would read %s and ignore the selection; %s"
       (string-join files ", ")
       "these commands send it the lines on standard input")))
  (let* ((program (csvdt--program))
         ;; Whatever was selected, whole records are what go out. Half a line
         ;; is half a record: a selection starting mid-row hands csvdt a
         ;; truncated field, which it reads as a value rather than as damage.
         ;; Reported below when it makes a difference, so a selection that was
         ;; not what it looked like says so.
         (widened (mapcar (lambda (range)
                            (csvdt--whole-lines (car range) (cdr range)))
                          ranges))
         (snapped (or snapped (not (equal widened ranges))))
         (ranges widened)
         ;; Whole lines are not yet whole records, though, because a quoted
         ;; field may hold newlines and then one record covers several lines.
         ;; A range that begins inside such a field looks like a row and is
         ;; the tail of one; it converted cleanly and reported success, with
         ;; the cut field arriving as data nobody wrote. Pushed out to the
         ;; record boundaries instead, and said, since the run then covers
         ;; more than was selected.
         (dialect (csvdt--dialect args))
         ;; Merged again afterwards: widening can bring two ranges onto the
         ;; same record, and sending it once per range put a duplicate row
         ;; in the output that nothing else would have questioned.
         (recorded (csvdt--drop-covered
                    (mapcar (lambda (range)
                              (csvdt--whole-records (car range) (cdr range)
                                                    (car dialect) (cdr dialect)))
                            ranges)))
         (spanned (not (equal recorded ranges)))
         (ranges recorded)
         (cut (csvdt--narrowing-cuts-record-p ranges (car dialect) (cdr dialect)))
         (header (csvdt--header-line (caar ranges) args))
         (source (current-buffer))
         (errfile (make-temp-file "csvdt-stderr"))
         ;; UTF-8 in both directions rather than whatever the locale makes the
         ;; default process coding: csvdt requires UTF-8 and refuses anything
         ;; else, so a buffer Emacs decoded from cp1252 has to be re-encoded on
         ;; the way out, not passed through as the bytes it came from.  The
         ;; -unix part normalises CRLF endings too, so a Windows CSV does not
         ;; reach csvdt with stray carriage returns inside its last field.
         ;;
         ;; Bound again inside the assemble path below, since a let-binding
         ;; made here binds whatever binding the SOURCE buffer has -- and a
         ;; buffer-local one leaves the temp buffer that assembles the text
         ;; seeing the global value instead.  A source buffer that had set
         ;; either variable locally therefore sent latin-1 to a program that
         ;; refuses it, and the run died on csvdt's invalid-UTF-8 error.
         (coding-system-for-write 'utf-8-unix)
         (coding-system-for-read 'utf-8-unix)
         ;; Written to a scratch buffer rather than straight into the output
         ;; buffer, because they can be the same buffer -- feeding a result
         ;; back through csvdt is a reasonable thing to do, and erasing the
         ;; output buffer first would take the input away mid-read.
         (result (generate-new-buffer " *csvdt-result*"))
         (notes nil)
         status)
    (unwind-protect
        (progn
          (setq status
                (if (and (null header) (null (cdr ranges)))
                    ;; One range and nothing to put in front of it: hand the
                    ;; buffer straight to the process, which is the whole-buffer
                    ;; case and costs no copy however large the file is.
                    (apply #'call-process-region (caar ranges) (cdar ranges)
                           program nil (list result errfile) nil args)
                  ;; Otherwise assemble, since a header and the lines it
                  ;; belongs to are not adjacent and banked lines are not
                  ;; adjacent to each other.  Only ever a selection's worth of
                  ;; text.
                  (with-temp-buffer
                    (let ((coding-system-for-write 'utf-8-unix)
                          (coding-system-for-read 'utf-8-unix))
                      (when header (insert header))
                      (dolist (range ranges)
                        (insert-buffer-substring source (car range) (cdr range))
                        ;; A buffer whose last line has no newline gives a range
                        ;; that does not end in one, which would run the next
                        ;; banked line onto the end of it.
                        (unless (or (bobp) (eq (char-before) ?\n))
                          (insert "\n")))
                      (apply #'call-process-region (point-min) (point-max)
                             program nil (list result errfile) nil args)))))
          (let ((said (with-temp-buffer
                        (insert-file-contents errfile)
                        (string-trim (buffer-string)))))
            (unless (eq status 0)
              ;; What csvdt wrote before failing is shown, not discarded.
              ;; On the nothing-converted exit the output is complete and
              ;; its parse_err markers are exactly the diagnostic csvdt's
              ;; own message points at; on a mid-stream abort the partial
              ;; rows show where it stopped.  A shell user has both in
              ;; front of them, and dropping them left an Emacs user told
              ;; "the output was written either way" about output no one
              ;; could see.  Except when the run was fed from the output
              ;; buffer itself: there its contents are the input, and a
              ;; failed run must not replace the data with its debris.
              (unless (or (zerop (buffer-size result))
                          (eq source (get-buffer "*csvdt output*")))
                (display-buffer (csvdt--fill-output result)))
              ;; csvdt names itself on every line it writes to stderr, which
              ;; is what a shell user needs and what this message has already
              ;; said: without the trim it reads "csvdt exited 1: csvdt: ...".
              ;; With the cut named, because this is the failure the check
              ;; is for: csvdt sees a short record and says so about a line,
              ;; which sends the reader looking at data that is not the
              ;; matter -- the half of it out of reach is.
              (user-error "csvdt exited %s: %s%s" status
                          (if (string-empty-p said)
                              "(no message)"
                            (string-remove-prefix "csvdt: " said))
                          (if cut
                              (concat " -- note the " (csvdt--cut-note cut))
                            "")))
            (setq notes (unless (string-empty-p said) said)))
          (let ((out (csvdt--fill-output result)))
            (display-buffer out)
            (message "csvdt %s: %d lines%s%s%s%s%s"
                     (string-join args " ")
                     (with-current-buffer out
                       (count-lines (point-min) (point-max)))
                     ;; Said when more than one stretch went in, so it is
                     ;; visible that a run covered the banked lines as well as
                     ;; the region rather than one of the two.
                     (if (cdr ranges)
                         (format " from %d stretches" (length ranges))
                       "")
                     (if snapped ", selection widened to whole lines" "")
                     ;; Named apart from the line widening above: that one
                     ;; says the selection was short of a line, this one
                     ;; says a record was longer than the lines it looked
                     ;; like, which is a thing about the data.
                     (if spanned
                         ", widened to whole records: a quoted field spans lines"
                       "")
                     ;; Said however the run went: a truncated last record
                     ;; is the kind of thing that looks like the data until
                     ;; someone counts columns.
                     (if cut (concat ", " (csvdt--cut-note cut)) "")
                     ;; What csvdt said on stderr about a run that still
                     ;; succeeded: how much did not convert, rows a re-read
                     ;; would take for comments.  The binary goes out of its way to
                     ;; say these; dropping them here left an Emacs user with
                     ;; less than a shell user saw.
                     (if notes (concat "\n" notes) ""))))
      (kill-buffer result)
      (delete-file errfile))))

(defun csvdt--fill-output (result)
  "Replace the output buffer's contents with RESULT's, returning the buffer."
  (let ((out (get-buffer-create "*csvdt output*")))
    (with-current-buffer out
      (let ((inhibit-read-only t))
        (erase-buffer)
        ;; Overlays another package left on the previous result would
        ;; otherwise survive it.  Emptying the buffer collapses one to
        ;; zero width rather than removing it, and an overlay that
        ;; advances with text inserted at its end then grows over the
        ;; whole of the next result -- so a line highlighted for having
        ;; been banked colours everything that replaces it.  The
        ;; contents they described are gone either way.
        (remove-overlays)
        (insert-buffer-substring-no-properties result))
      (csvdt--apply-output-mode)
      (goto-char (point-min)))
    out))

;;; Running it

;;;###autoload
(defun csvdt-run-buffer (args)
  "Run csvdt over the whole buffer with ARGS."
  (interactive (list (csvdt--read-arguments)))
  (csvdt--run (list (cons (point-min) (point-max))) args))

;;;###autoload
(defun csvdt-run-region (args)
  "Run csvdt over the active region with ARGS.
Useful for trying something on a handful of rows before the whole file.
Every piece of the region, where it has more than one -- see
`csvdt--region-ranges\='.
The pieces are normalised first, as the banked ones always were.
`csvdt--run\=' asks for ranges in buffer order, and a region need not
report them that way: `region-bounds\=' does, but a package hooking
`region-extract-function\=' defines its own selection and may report the
pieces in any order, overlapping, or naming nothing at all.  Handing those
straight on sent rows out of buffer order -- which csvdt reads, where it
measures between consecutive rows, as the order the events happened in --
and sent a line covered by two pieces twice."
  (interactive (list (csvdt--read-arguments)))
  (let* ((pieces (csvdt--region-ranges))
         (ranges (csvdt--normalise-ranges pieces "`region-bounds'")))
    (unless ranges
      (user-error (if (use-region-p)
                      "The region covers no whole line"
                    "No active region")))
    (csvdt--run ranges args (csvdt--mid-line-boundary-p pieces))))

;;;###autoload
(defun csvdt-run-banked (args)
  "Run csvdt over the banked lines with ARGS.
The lines come from `csvdt-banked-lines-function', so whichever package
banks them is the one that decides what is banked; this only asks.  They
are sent as one file, in buffer order, so a duration measured across them
is measured across what was banked rather than across what was skipped."
  (interactive (list (csvdt--read-arguments)))
  (let* ((banked (csvdt--banked-lines))
         (ranges (csvdt--normalise-ranges banked)))
    (unless ranges
      (user-error "No banked lines%s"
                  (if csvdt-banked-lines-function
                      ""
                    "; set `csvdt-banked-lines-function' to where yours come from")))
    (csvdt--run ranges args (csvdt--mid-line-boundary-p banked))))

;;;###autoload
(defun csvdt-run-dwim (args)
  "Run csvdt with ARGS on the banked lines and the region together.
Both, not one or the other: the region is usually the piece not banked
yet, and dropping either would mean running over less than was picked out.
They are merged, so a selection abutting or inside a banked stretch does
not send a line twice.  With neither, the whole buffer."
  (interactive (list (csvdt--read-arguments)))
  ;; Each list is normalised under its own name, so a malformed piece is
  ;; blamed on whichever source reported it -- normalising the two together
  ;; pointed every complaint at `csvdt-banked-lines-function\=', including
  ;; for pieces the region supplied.  The merged result is normalised once
  ;; more, which is what merges a selection into a banked stretch it abuts.
  (let* ((banked-raw (csvdt--banked-lines))
         (region-raw (csvdt--region-ranges))
         (ranges (csvdt--normalise-ranges
                  (append (csvdt--normalise-ranges banked-raw)
                          (csvdt--normalise-ranges region-raw
                                                   "`region-bounds'")))))
    (csvdt--run (or ranges (list (cons (point-min) (point-max)))) args
                (and ranges
                     (csvdt--mid-line-boundary-p
                      (append banked-raw region-raw))))))

;;;###autoload
(defun csvdt--describe-dialect ()
  "The dialect a run started from the prefilled arguments would read with.
Which character quotes a field, and which separates them, decide where a
record ends -- and `csvdt-describe-run' answers before any argument line
has been typed, so `csvdt-default-arguments' is the best statement of
intent available to it.  A prefill that will not split is not worth
failing this command over, since it is the one command that must not
raise what it is here to explain: the plain dialect stands in, and a run
whose own arguments name another may cover a little more than described."
  (or (ignore-errors
        (csvdt--dialect (csvdt--split-arguments csvdt-default-arguments)))
      (cons ?\" ?,)))

;;;###autoload
(defun csvdt-describe-run ()
  "Say what a run would cover, without running anything.
For when the output covers more or less than expected: it reports what
this package can see, which is not always what the buffer looks like.
Lines that merely look banked -- highlighted by something else, or banked
by a package `csvdt-banked-lines-function' does not point at -- are not
banked as far as csvdt is concerned, and this says so.

Counted the way a run counts: whole records, so a stretch reaching into a
field that spans lines is described as reaching over all of it."
  (interactive)
  (let* ((dialect (csvdt--describe-dialect))
         ;; Settled the way a run settles them: normalised, then widened out
         ;; to whole records and merged again. Stopping at the normalising
         ;; was right when a record was a line and wrong afterwards -- two
         ;; banked lines inside one record that spans several were described
         ;; as two stretches of one line where the run makes one stretch of
         ;; three. This command is reached for when a run covered something
         ;; unexpected, so being wrong exactly where the run is surprising is
         ;; the one thing it cannot afford.
         (settle
          (lambda (rs)
            (csvdt--drop-covered
             (mapcar (lambda (r)
                       (csvdt--whole-records (car r) (cdr r)
                                             (car dialect) (cdr dialect)))
                     rs))))
         (reported (and (functionp csvdt-banked-lines-function)
                       (csvdt--banked-lines)))
         (banked (funcall settle (csvdt--normalise-ranges reported)))
         ;; Normalised, because this command exists to say what a run would
         ;; cover and a run normalises. Describing the raw pieces reported a
         ;; three-line rectangle as three stretches where the run makes one.
         (region (funcall settle
                          (csvdt--normalise-ranges (csvdt--region-ranges)
                                                   "`region-bounds'")))
         (ranges (funcall settle
                          (csvdt--normalise-ranges (append reported region))))
         ;; Asked of what a run would send, which with nothing banked or
         ;; selected is the accessible portion -- the same range the run
         ;; passes, so the same answer.
         (cut (csvdt--narrowing-cuts-record-p
               (or ranges (list (cons (point-min) (point-max))))
               (car dialect) (cdr dialect)))
         (describe
          (lambda (rs)
            (let ((lines (apply #'+ (mapcar (lambda (r)
                                              (count-lines (car r) (cdr r)))
                                            rs))))
              (format "%d line%s in %d stretch%s"
                      lines (if (= lines 1) "" "s")
                      (length rs) (if (= (length rs) 1) "" "es"))))))
    (message
     "csvdt would run over %s.  Banked: %s.  Region: %s.%s"
     (if ranges
         (funcall describe ranges)
       (format "the whole buffer, %d lines"
               (count-lines (point-min) (point-max))))
     (cond ((null csvdt-banked-lines-function)
            "nothing, `csvdt-banked-lines-function' is unset")
           ;; The one command that must not raise what it is here to explain.
           ((not (functionp csvdt-banked-lines-function))
            (format "nothing, `csvdt-banked-lines-function' names %S, \
which is not defined"
                    csvdt-banked-lines-function))
           (banked (funcall describe banked))
           (t "nothing reported -- lines that look banked may not be"))
     (if region (funcall describe region) "none")
     ;; The run says this and this did not, which is the wrong way round:
     ;; the command exists to be asked *before* a run, and a record the
     ;; narrowing has cut in half is the plainest case of what it promises
     ;; to report -- what this package can see differing from what the
     ;; buffer looks like. Only when there is one, so an ordinary buffer's
     ;; answer is the sentence it always was.
     ;; The note as its own sentence rather than under a "Narrowing:"
     ;; label, which read "Narrowing: last record cut short by the
     ;; narrowing". One phrasing is the point of the helper, so it is the
     ;; label that goes.
     (if cut
         (let ((note (csvdt--cut-note cut)))
           (format "  %s%s." (upcase (substring note 0 1)) (substring note 1)))
       ""))))

;;;###autoload
(defun csvdt-version ()
  "Report the csvdt in use, and which time zone database is built into it."
  (interactive)
  (message "%s" (csvdt-version-string)))

;; Not `defvar-keymap', which arrived in Emacs 29: this package declares
;; 27.1, and this form is what 27 has.  Same for `string-search' (28.1),
;; avoided above -- the declared minimum is only true if the file loads
;; and runs there.
(defvar csvdt-command-map
  (let ((map (make-sparse-keymap)))
    (define-key map "r" #'csvdt-run-dwim)
    (define-key map "b" #'csvdt-run-buffer)
    (define-key map " " #'csvdt-run-region)
    (define-key map "k" #'csvdt-run-banked)
    (define-key map "?" #'csvdt-describe-run)
    (define-key map "h" #'csvdt-help)
    (define-key map "v" #'csvdt-version)
    map)
  "Keymap for csvdt commands.")

(provide 'csvdt)
;;; csvdt.el ends here

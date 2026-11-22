;;; csvdt.el --- Front end for the csvdt command-line tool  -*- lexical-binding: t; -*-

;; Copyright (C) 2025-2026 Michael A Jones

;; Author: Michael A Jones <yardquit@pm.me>
;; Maintainer: Michael A Jones <yardquit@pm.me>
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
;; therefore usable from Emacs immediately, with no change to this file,
;; which is the whole point of doing it this way.
;;
;; Three options are named, in defcustoms, because a region run cannot be
;; worked out without them: which flag says the first record is a header,
;; and which two set the quote and the delimiter, since where a record
;; begins depends on both.  The tests check that each is an option csvdt
;; still reports and that no other option name appears in this file.
;;
;; A run happens in the background: the selected text is written to a
;; temporary file in pieces, csvdt is started on the file, and the result is
;; shown by the process's sentinel, so Emacs is not held up by a large file.
;;
;; A csvdt too old to know `--list-options', or one printing a format version
;; this file does not read, falls back to parsing the option names out of
;; `--help'.

;;; Code:

(require 'ansi-color)
(require 'cl-lib)
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
The default assumes a file with a header row and asks for the header to
be printed.

The text is prefilled selected, so typing replaces it and a deliberate
keystroke keeps it.  Passing the header option to a file without a header
fails quietly (the first row is taken as the header and may be lost),
while leaving it off a file that has one fails loudly, so keeping the
option has to be a choice."
  :type 'string
  :group 'csvdt)

(defcustom csvdt-region-carries-header t
  "Whether a region run is given the buffer's header line.
A region below the header has no header of its own, so csvdt would take
its first row as one and pass it through unconverted, or lose it.  With
this on, the buffer's header record is put in front of the region, so the
whole selection is data and the output is named after the right columns.

Only when the arguments ask for a header and the region starts below it.
See `csvdt-header-options'."
  :type 'boolean
  :group 'csvdt)

(defcustom csvdt-header-options '("-H" "--has-header")
  "The csvdt options that say the first record is a header.
`csvdt-region-carries-header' needs to know which they are; every other
option is discovered from the binary.  The tests check that each form here
is an option csvdt reports."
  :type '(repeat string)
  :group 'csvdt)

(defcustom csvdt-single-quote-options '("--single-quote")
  "The csvdt options that make the quote character the single one.
Finding where a record begins means knowing which character quotes a
field.  The tests check each form against the options csvdt reports."
  :type '(repeat string)
  :group 'csvdt)

(defcustom csvdt-read-delimiter-options '("--read-delimiter")
  "The csvdt options that set the field delimiter for reading.
A quote opens a field only where a field begins, which depends on the
delimiter.  The tests check each form against the options csvdt reports."
  :type '(repeat string)
  :group 'csvdt)

(defcustom csvdt-banked-lines-function nil
  "Where banked lines come from, when something banks them.
Banking means picking out lines here and there and working on those
rather than on one stretch.  Other packages provide it; this is how one
is plugged in.

The value is a function of no arguments, called in the buffer being
processed, returning a list of (BEG . END) positions (integers or
markers) or a list of overlays.  Ranges are put in buffer order and
widened to whole lines, and out to whole records where a quoted field
spans several, so an implementation need not do either.

nil, the default, means nothing is banked."
  :type '(choice (const :tag "Nothing banks lines" nil) function)
  :group 'csvdt)

(defcustom csvdt-output-mode
  (if (fboundp 'csv-mode) 'csv-mode 'fundamental-mode)
  "Major mode for the buffer csvdt output is written to.
A mode that is not available leaves the buffer in Fundamental mode."
  :type 'function
  :group 'csvdt)

(defconst csvdt--option-list-format 1
  "Version of csvdt's `--list-options' format this file knows how to read.
csvdt prints it on the first line.  Output in another version is declined,
since its fields may mean something else.")

(defvar csvdt--options-cache nil
  "List of (PROGRAM MTIME VERSION . RECORDS) from the last option discovery.
Keyed on the program's path and modification time, so an upgraded csvdt
invalidates it without running anything to ask.  VERSION is the version
line of the csvdt the records came from.")

;;; Talking to the binary

(defun csvdt--program ()
  "Return the csvdt executable, or signal if it cannot be found."
  ;; A name with a directory part means that file and is never searched
  ;; for on PATH (the rule execvp and shells follow); `executable-find'
  ;; would join "./csvdt" onto every `exec-path' entry.
  (or (if (file-name-directory csvdt-executable)
          ;; A directory also passes `file-executable-p'.  The name is
          ;; expanded so the process and the option cache's mtime key
          ;; name the same file whatever the current directory is.
          (and (file-executable-p csvdt-executable)
               (not (file-directory-p csvdt-executable))
               (expand-file-name csvdt-executable))
        (executable-find csvdt-executable))
      ;; Each message names what is wrong: "cannot find" would be false
      ;; for a file that is there but is a directory or not executable.
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
        ;; `exec-path' need not match the PATH of the shell that started
        ;; Emacs, the usual reason a program found in a terminal is not
        ;; found here.
        (user-error "Cannot find `%s' on `exec-path'.  Set \
`csvdt-executable' to its full path" csvdt-executable)))))

(defun csvdt--output-of (&rest args)
  "Run csvdt with ARGS and no input, returning its standard output.
Returns nil when csvdt exits non-zero, so callers can degrade rather than
break if a future version declines the arguments."
  (with-temp-buffer
    ;; csvdt speaks UTF-8 in both directions; a locale decoding would
    ;; garble any non-ASCII text in its output.
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
Matches the short-and-long form (\"  -x, --xxx <VALUE>\") and the
long-only form (\"      --xxx <VALUE>\").  The layout does not say whether
an option takes a value, so :value is nil, meaning not known, and
`csvdt--file-arguments' then declines to guess at filenames."
  (let ((records '())
        (case-fold-search nil))
    (with-temp-buffer
      (insert help)
      (goto-char (point-min))
      (while (re-search-forward
              "^[ \t]+\\(?:-\\([A-Za-z]\\), \\)?--\\([A-Za-z][A-Za-z0-9-]*\\)"
              nil t)
        (let ((long (concat "--" (match-string 2))))
          ;; The help prose names options too, so one can match twice.  The
          ;; first match is the entry in the options list.
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
is unavailable, so an older csvdt still works.  The cache is checked
against the program's path and modification time; the version string is
fetched when the records are."
  (let* ((program
          ;; The real file: a symlink's own mtime never moves.
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
`csvdt-version-string' always asks the binary."
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

;; The line is split the way a shell would, not on commas: a comma is
;; csvdt's own separator inside a value, a column list or a column-and-offset
;; pair.  Completion is therefore per token, not over the whole line.

(defvar csvdt--arguments-history nil
  "Minibuffer history of csvdt command lines.")

(defun csvdt--keep-prefill (n)
  "Keep the selected prefilled arguments and move past them.
With nothing selected this is an ordinary `forward-char' of N.  It is bound
because point already sits at the end of the prefill, where plain motion
would only ring the bell."
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
a list of arguments and only some of them are options: SPC and comma
have to insert themselves.")

(defun csvdt--complete-argument ()
  "Complete the whitespace-delimited argument before point.
An entry for `completion-at-point-functions' while a command line is
being read."
  (let ((end (point))
        (beg (save-excursion
               ;; The separator set `csvdt--split-arguments' uses.
               (skip-chars-backward "^ \t\n\r\f\v" (minibuffer-prompt-end))
               (point))))
    (list beg end (ignore-errors (csvdt-options)) :exclusive 'no)))

(defun csvdt--closing-quote (line start quote)
  "Index in LINE of the QUOTE closing a run that opened before START, or nil.
Inside a double-quoted run a backslash escapes the next character, so an
escaped quote does not close it.  A single-quoted run has no escapes."
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
which csvdt's value lists need.  Both quote characters are understood.

A double-quoted run honours the backslash escapes \\t \\n \\r \\\\ \\\"
and no others, so \\=\"\\t\" is a tab and \\=\"C:\\data\" is refused.  A
single-quoted run is literal throughout, so a value that must keep its
backslashes belongs in single quotes, and a lone quote character is
written by wrapping it in the other kind.

Outside quotes a backslash is left as itself, so a Windows path typed as
a value survives."
  (let ((index 0)
        (length (length line))
        (args '())
        (current nil))
    (while (< index length)
      (let ((char (aref line index)))
        (cond
         ;; Whitespace outside quotes ends the argument being gathered.
         ;; The set is the one `split-string-and-unquote' uses.
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
                            ;; Only the familiar escapes reach the Lisp
                            ;; reader, which knows more (\d is DEL), so a
                            ;; path like "C:\data" is refused, not mangled.
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
A mode that is missing or signals leaves the buffer plain, so the run's
result is still shown."
  (if (fboundp csvdt-output-mode)
      (condition-case nil
          (funcall csvdt-output-mode)
        (error (fundamental-mode)))
    (fundamental-mode)))

(defun csvdt--short-flags (argument)
  "Return the short flags clustered in ARGUMENT, or nil if it is not a cluster.
clap lets flags be written together, so \"-Hp\" is two of them.  A short
option that can take an attached value consumes the rest of the argument,
so \"-i0,replace\" is one option and its value.  A short whose value must
be attached with \"=\" is just another flag in a cluster: \"-fH\" is two.
Which options are which comes from csvdt; when it does not say, only
whole arguments match."
  (when (and (string-prefix-p "-" argument)
             (not (string-prefix-p "--" argument))
             ;; An "=" means option=value to clap: "-f=keep" is -f with an
             ;; attached value, never a cluster.
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

(defun csvdt--takes-separate-value-p (record)
  "Non-nil when the option RECORD may take its value as the next argument.
That is so when it takes a value at all and does not insist on \"=\"."
  (and (member (plist-get record :value) '("required" "optional"))
       (not (equal (plist-get record :separator) "equals"))))

(defun csvdt--shorts-taking-a-separate-value ()
  "Return the short options whose value may be the next argument."
  (let ((shorts '()))
    (dolist (record (csvdt--option-records))
      (let ((short (plist-get record :short)))
        (when (and short (not (string-empty-p short))
                   (csvdt--takes-separate-value-p record))
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
                          (csvdt--takes-separate-value-p record)))
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
Only ones that name a file on disk.  The commands send the selection on
standard input, so a filename replaces it and the result looks like a
successful run on the file.  Nothing is reported when the option table
cannot say which options take a separate value (the `--help' fallback,
where :value is nil), since telling a value from a filename needs that."
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
          ;; Not "~" paths: `file-regular-p' expands the tilde and csvdt
          ;; does not, so csvdt refuses those itself.
          (when (and (not (string-empty-p argument))
                     (not (string-prefix-p "~" argument))
                     (file-regular-p argument))
            (push argument files))
          (setq rest (cdr rest))))))
    (nreverse files)))

(defun csvdt--options-part (args)
  "Return the part of ARGS before \"--\", which clap reads as options.
Past \"--\" everything is positional."
  (seq-take-while (lambda (argument) (not (equal argument "--"))) args))

(defun csvdt--header-requested-p (args)
  "Non-nil when ARGS ask csvdt to treat the first record as a header."
  (seq-some
   (lambda (argument)
     (or (member argument csvdt-header-options)
         (seq-intersection (csvdt--short-flags argument) csvdt-header-options)))
   (csvdt--options-part args)))

(defun csvdt--header-line (beg args &optional records)
  "Return the buffer's header record to put in front of a region at BEG.
Nil unless ARGS ask for a header and BEG is below it: a region starting
inside the header already carries part of one, and guessing what was
meant would be worse than leaving it alone.  The header is a whole
record, which spans two lines when one of its fields holds a newline.
RECORDS is the index from `csvdt--make-records', made when nil."
  (when (and csvdt-region-carries-header
             (csvdt--header-requested-p args))
    (save-excursion
      (goto-char (point-min))
      (let* ((dialect (csvdt--dialect args))
             (header-end (cdr (csvdt--whole-records
                               (point-min) (line-beginning-position 2)
                               (car dialect) (cdr dialect) records))))
        (when (>= beg header-end)
          (buffer-substring-no-properties (point-min) header-end))))))

(defun csvdt--whole-lines (beg end)
  "Return BEG..END widened to whole lines.
A range ending exactly at the start of a line stops there.  Whole lines are
not yet whole records, since a quoted field may span lines; see
`csvdt--whole-records'."
  (save-excursion
    (let ((start (progn (goto-char beg) (line-beginning-position)))
          (finish (progn (goto-char end)
                         (if (and (> end beg) (bolp))
                             (point)
                           (min (point-max) (line-beginning-position 2))))))
      (cons start (max finish start)))))

(defun csvdt--option-value (names options)
  "Return the value OPTIONS give to an option called one of NAMES, or nil.
Both spellings clap accepts are read: attached with \"=\" and as the next
argument.  An empty or missing value is nil."
  (let ((found nil))
    (while (and options (not found))
      (let* ((argument (car options))
             (name (seq-find (lambda (name)
                               (string-prefix-p (concat name "=") argument))
                             names))
             (value (cond ((member argument names) (cadr options))
                          (name (substring argument (1+ (length name)))))))
        (when (and value (not (string-empty-p value)))
          (setq found value)))
      (setq options (cdr options)))
    found))

(defun csvdt--option-given-p (names options)
  "Non-nil when OPTIONS include an option called one of NAMES."
  (seq-some (lambda (argument)
              (or (member argument names)
                  (seq-some (lambda (name)
                              (string-prefix-p (concat name "=") argument))
                            names)))
            options))

(defun csvdt--dialect (args)
  "Return (QUOTE . DELIMITER) as characters, as ARGS leave them."
  (let ((options (csvdt--options-part args)))
    (cons (if (csvdt--option-given-p csvdt-single-quote-options options)
              ?'
            ?\")
          (let ((value (csvdt--option-value csvdt-read-delimiter-options
                                            options)))
            (if value (aref value 0) ?,)))))

(defun csvdt--field-initial-p (pos delimiter)
  "Whether POS is where a field begins.
That is the first character of the accessible portion, or the one just past
a DELIMITER or a line ending."
  (or (eq pos (point-min))
      (memq (char-before pos) (list delimiter ?\n ?\r))))

;;; Where quoted fields hold line breaks

;; A record is not always a line: a quoted field may hold newlines.  The
;; index below finds those fields in one forward pass, so any number of
;; positions can be asked about for the price of reading the buffer once.

(cl-defstruct (csvdt--records
               (:constructor csvdt--make-records
                             (quote-char delimiter &optional off
                                         &aux (scanned (point-min)))))
  "How far a buffer has been read for quoted fields that span lines.
QUOTE-CHAR and DELIMITER are the dialect.  SCANNED is the position up to
which the buffer has been read.  SPANS is a vector of the first COUNT
(OPEN . CLOSE) pairs found, in buffer order, where OPEN is the position of
the opening quote and CLOSE that of the closing one, or the end of the
buffer for a quote never closed.  Only fields holding a line break are
kept.  OFF says not to look at all, so every position is outside a field."
  quote-char delimiter off scanned (spans (make-vector 16 nil)) (count 0))

(defun csvdt--records-add (records open close)
  "Add the quoted field from OPEN to CLOSE to RECORDS."
  (let ((spans (csvdt--records-spans records))
        (count (csvdt--records-count records)))
    (when (= count (length spans))
      (setq spans (vconcat spans (make-vector count nil)))
      (setf (csvdt--records-spans records) spans))
    (aset spans count (cons open close))
    (setf (csvdt--records-count records) (1+ count))))

(defun csvdt--records-extend (records pos)
  "Read RECORDS' buffer far enough to say what lies at POS.
Quotes are jumped to rather than every character read, and a quote opens a
field only where a field begins.  A doubled quote inside a field is an
escaped quote and leaves it open."
  (when (and (not (csvdt--records-off records))
             (> pos (csvdt--records-scanned records)))
    (let ((quote-char (csvdt--records-quote-char records))
          (delimiter (csvdt--records-delimiter records))
          (open nil)
          (done nil))
      (save-excursion
        (goto-char (csvdt--records-scanned records))
        (while (not done)
          (if (search-forward (char-to-string quote-char)
                              (if open nil (min pos (point-max))) t)
              (let ((at (1- (point))))
                (cond
                 ((not open)
                  (when (csvdt--field-initial-p at delimiter)
                    (setq open at)))
                 ((eq (char-after) quote-char)
                  (forward-char 1))
                 (t
                  (when (save-excursion (goto-char open)
                                        (search-forward "\n" at t))
                    (csvdt--records-add records open at))
                  (setq open nil)))
                (when (and (not open) (>= (point) pos))
                  (setq done t)))
            (when open
              (csvdt--records-add records open (point-max))
              (goto-char (point-max)))
            (setq done t)))
        (setf (csvdt--records-scanned records) (max pos (point)))))))

(defun csvdt--records-span-at (records pos)
  "Return the (OPEN . CLOSE) field of RECORDS that holds POS, or nil.
A position is held from just after the opening quote to the closing one.
Only fields with a line break are known, so this answers for positions at
the start of a line or the end of the buffer, which are the ones a record
boundary can be."
  (csvdt--records-extend records pos)
  (let ((spans (csvdt--records-spans records))
        (low 0)
        (high (csvdt--records-count records)))
    ;; The first span opening at or after POS.
    (while (< low high)
      (let ((middle (/ (+ low high) 2)))
        (if (< (car (aref spans middle)) pos)
            (setq low (1+ middle))
          (setq high middle))))
    (when (> low 0)
      (let ((span (aref spans (1- low))))
        (and (<= pos (cdr span)) span)))))

(defun csvdt--inside-quoted-field-p (pos quote delimiter &optional records)
  "Non-nil when POS falls inside a quoted field rather than between records.
POS is a line start or the end of the buffer.  QUOTE and DELIMITER are the
dialect.  RECORDS is the index to read from, made when nil; pass one to
ask about many positions with a single pass over the buffer."
  (and (csvdt--records-span-at
        (or records (csvdt--make-records quote delimiter)) pos)
       t))

(defun csvdt--whole-records (beg end quote delimiter &optional records)
  "Return BEG..END widened out to whole records.
A quoted field may hold newlines, and then one record spans several lines.
A range starting inside such a field would hand csvdt the tail of a record
as if it were one, which parses and converts without complaint.  QUOTE and
DELIMITER are the dialect the run reads with; RECORDS is the index to use,
made when nil.  A range already on record boundaries is returned as it
is."
  (let ((records (or records (csvdt--make-records quote delimiter)))
        (start beg)
        (finish end)
        span)
    (while (and (> start (point-min))
                (setq span (csvdt--records-span-at records start)))
      (setq start (save-excursion (goto-char (car span))
                                  (line-beginning-position))))
    (while (and (< finish (point-max))
                (setq span (csvdt--records-span-at records finish)))
      (setq finish (save-excursion (goto-char (cdr span))
                                   (line-beginning-position 2))))
    (cons start (max finish start))))

(defun csvdt--settle-ranges (ranges records)
  "Return RANGES, which are whole lines, widened to whole records.
Ranges brought onto the same record are not sent twice."
  (csvdt--drop-covered
   (mapcar (lambda (range)
             (csvdt--whole-records (car range) (cdr range)
                                   (csvdt--records-quote-char records)
                                   (csvdt--records-delimiter records)
                                   records))
           ranges)))

(defun csvdt--narrowing-cuts-record-p (ranges quote delimiter &optional records)
  "Which end of RANGES the buffer's narrowing cuts a record at.
Returns `start', `end', `both' or nil.  QUOTE and DELIMITER are the
dialect, RECORDS the index to use, made when nil.

Widening cannot go past the accessible portion, so a buffer narrowed to
begin or end mid-record leaves half a record, which csvdt reads as a short
row.  The cut is reported rather than hidden or overridden.  A record is
cut mid-line, or at a line boundary that lies inside a quoted field."
  (let* ((records (or records (csvdt--make-records quote delimiter)))
         (start (car (car ranges)))
         (finish (cdr (car (last ranges))))
         (narrowed (buffer-narrowed-p))
         (at-start
          (and (= start (point-min))
               ;; The text before the narrowing decides whether a quote is
               ;; still open here, and whether this line began before it.
               (save-restriction
                 (widen)
                 (or (csvdt--records-span-at
                      (if narrowed
                          (csvdt--make-records quote delimiter
                                               (csvdt--records-off records))
                        records)
                      start)
                     (save-excursion
                       (goto-char start)
                       (not (bolp)))))))
         (at-end
          (and (= finish (point-max))
               (or (csvdt--records-span-at records finish)
                   ;; At end-of-line only the newline is out of reach, and
                   ;; csvdt supplies the terminator.
                   (save-restriction
                     (widen)
                     (save-excursion
                       (goto-char finish)
                       (not (or (bolp) (eolp)))))))))
    (cond ((and at-start at-end) 'both)
          (at-start 'start)
          (at-end 'end))))

(defun csvdt--cut-note (cut)
  "How to say that the narrowing left CUT half-reachable.
CUT is the value `csvdt--narrowing-cuts-record-p' returns."
  (pcase cut
    ('both "first and last records cut short by the narrowing")
    ('start "first record cut short by the narrowing")
    ('end "last record cut short by the narrowing")))

(defun csvdt--piece-bounds (item)
  "Return ITEM as a sorted (BEG . END) pair in this buffer, or nil.
ITEM is a cons or an overlay, as `csvdt-banked-lines-function' and the
region report pieces.  The ends of a pair may be integers or markers, in
either order.  The result is nil for a dead overlay, for a marker pointing
nowhere, for an overlay or marker belonging to an unrelated buffer, and for
a piece starting at or past the end of this one."
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
positions, so a marker or overlay made in one names the same text in the
other, unlike a position from an unrelated buffer."
  (and (buffer-live-p buffer)
       (eq (or (buffer-base-buffer buffer) buffer)
           (or (buffer-base-buffer (current-buffer)) (current-buffer)))))

(defun csvdt--piece-position (part)
  "Return PART, one end of a banked piece, as a position in this buffer.
PART is an integer, a marker, or nil (what a dead overlay reports).  The
result is nil for a marker in an unrelated buffer or pointing nowhere."
  (cond ((integerp part) part)
        ((and (markerp part)
              (csvdt--same-text-p (marker-buffer part)))
         (marker-position part))))

(defun csvdt--normalise-ranges (items &optional source)
  "Return ITEMS as whole-line (BEG . END) conses, in buffer order.
ITEMS may hold conses or overlays, in any order, overlapping or repeated.
Dead overlays are dropped, every range is widened to whole lines, and
ranges that touch are merged, so no line is sent twice.  SOURCE names where
ITEMS came from, for the error a malformed one raises; it defaults to
`csvdt-banked-lines-function'."
  (unless (listp items)
    (user-error "%s reported %S, which is not a list of positions or overlays"
                (or source "`csvdt-banked-lines-function'") items))
  (let ((ranges '()))
    (dolist (item items)
      ;; nil is tolerated: a package filtering its own list produces them.
      (unless (or (null item) (overlayp item)
                  (and (consp item)
                       (or (integerp (car item)) (markerp (car item)))
                       (or (integerp (cdr item)) (markerp (cdr item)))))
        (user-error "%s reported %S, which is neither a position pair nor an overlay"
                    (or source "`csvdt-banked-lines-function'") item))
      (let ((bounds (csvdt--piece-bounds item)))
        (when bounds
          (let ((range (csvdt--whole-lines (car bounds) (cdr bounds))))
            ;; A pair naming nothing widens to nothing and is dropped, so
            ;; it cannot make an empty run look like a successful one.
            (when (< (car range) (cdr range))
              (push range ranges))))))
    (csvdt--merge-ranges ranges)))

(defun csvdt--merge-ranges (ranges)
  "Return RANGES sorted into buffer order, touching or overlapping ones joined.
The conses are fresh, so the list handed in is left alone."
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
Widening to records can bring ranges onto the same record, and it would go
out once per range.  Order is left as given, unlike `csvdt--merge-ranges'.

Ranges that begin in buffer order, which every list a command builds does,
are done in one pass: whatever the earlier ones cover that lies at or past
the start of the next is an unbroken stretch ending at the furthest end
seen.  Any other order is compared range against range."
  (let ((kept '())
        (ascending (let ((previous nil) (sorted t))
                     (dolist (range ranges)
                       (when (and previous (< (car range) previous))
                         (setq sorted nil))
                       (setq previous (car range)))
                     sorted))
        (furthest nil))
    (dolist (range ranges)
      (let ((beg (car range))
            (end (cdr range))
            (trimmed nil))
        (if ascending
            (when (and furthest (< beg furthest))
              (setq trimmed t
                    beg (min end furthest)))
          (dolist (seen kept)
            (when (and (< beg (cdr seen)) (> end (car seen)))
              (setq trimmed t)
              (cond
               ((and (>= beg (car seen)) (<= end (cdr seen))) (setq beg end))
               ((>= beg (car seen)) (setq beg (cdr seen)))
               ((<= end (cdr seen)) (setq end (car seen)))))))
        (setq furthest (max (or furthest end) end))
        ;; An empty range nothing covered is kept: a whole-buffer run over
        ;; an empty buffer is one.  Only a range trimmed to nothing goes.
        (when (or (not trimmed) (< beg end))
          (push (cons beg end) kept))))
    (nreverse kept)))

(defun csvdt-banked-lines-from-overlays (property)
  "Return a function reporting overlays in the buffer that carry PROPERTY.
For a package that exposes no function of its own.  Naming a property it
happens to set reaches into its internals, so prefer pointing
`csvdt-banked-lines-function' at a function the package documents.

Most packages that bank lines mark them with an overlay, so this is
usually the whole of plugging one in:

    (setq csvdt-banked-lines-function
          (csvdt-banked-lines-from-overlays \\='some-package-banked))

To find PROPERTY, put point on a banked line and run
\\[describe-text-properties]: the overlays covering it are listed with the
properties they carry.  Anything but the face is a good candidate, a face
being shared with whatever else highlights that buffer."
  (lambda ()
    (seq-filter (lambda (overlay) (overlay-get overlay property))
                (overlays-in (point-min) (point-max)))))

(defun csvdt--region-ranges ()
  "Return the active region as (BEG . END) conses, or nil when there is none.
A region need not be one stretch: a rectangle is one piece per line, and a
package defining its own selection through `region-extract-function'
reports its pieces the same way, hence `region-bounds'."
  (and (use-region-p) (region-bounds)))

(defun csvdt--banked-lines ()
  "Return what `csvdt-banked-lines-function' reports, or nil.
A function that is named but not defined is a configuration error, most
likely from the package that defines it not being loaded."
  (when csvdt-banked-lines-function
    (unless (functionp csvdt-banked-lines-function)
      (user-error
       "`csvdt-banked-lines-function' names %S, which is not defined%s"
       csvdt-banked-lines-function
       " -- is the package that provides it loaded, and new enough?"))
    (funcall csvdt-banked-lines-function)))

(defun csvdt--mid-line-boundary-p (items)
  "Non-nil when a piece in ITEMS begins or ends inside a line.
ITEMS are the raw pieces a command was given, conses or overlays, before
`csvdt--normalise-ranges' widens them.

An end is on a boundary at the start of a line, at the end of one, or at
the end of the buffer.  The end of a line counts because an overlay covering
a line usually stops before the newline.  A beginning is on a boundary only
at the start of a line.  A zero-width piece widens to a whole line it holds
no character of, and is reported."
  (save-excursion
    (seq-some
     (lambda (item)
       (let ((bounds (csvdt--piece-bounds item)))
         (when bounds
           (let ((low (car bounds))
                 (high (min (cdr bounds) (point-max))))
             (if (= low high)
                 t
               (or (progn (goto-char low) (not (bolp)))
                   (progn (goto-char high)
                          (not (or (bolp) (eolp)
                                   (= high (point-max)))))))))))
     items)))

(defun csvdt--selection-ranges (banked region)
  "Return what BANKED and REGION pieces cover, as a plist.
BANKED and REGION are the raw pieces from `csvdt--banked-lines' and
`csvdt--region-ranges'.  :banked and :region are each normalised under
their own name, so a malformed piece is blamed on its source, and :all is
the two merged, which also joins a region to a banked stretch it abuts."
  (let ((banked (csvdt--normalise-ranges banked))
        (region (csvdt--normalise-ranges region "`region-bounds'")))
    (list :banked banked :region region
          :all (csvdt--normalise-ranges (append banked region)))))

(defun csvdt--plan (ranges args &optional snapped records)
  "Work out what a run over RANGES with ARGS covers.
RANGES are (BEG . END) conses in buffer order.  Returns a plist:
:ranges    the ranges widened to whole lines, then to whole records;
:snapped   non-nil when widening to lines changed them, or SNAPPED is
           non-nil because the caller's raw selection had a boundary
           inside a line;
:spanned   non-nil when widening to records changed them;
:cut       what the narrowing cuts, from `csvdt--narrowing-cuts-record-p';
:header    the header record to put in front, or nil;
:records   the index of quoted fields, which holds all that was read.
RECORDS is an index to reuse."
  (let* ((lines (mapcar (lambda (range)
                          (csvdt--whole-lines (car range) (cdr range)))
                        ranges))
         (dialect (csvdt--dialect args))
         (records (or records
                      (csvdt--make-records (car dialect) (cdr dialect))))
         (settled (csvdt--settle-ranges lines records)))
    (list :ranges settled
          :snapped (or snapped (not (equal lines ranges)))
          :spanned (not (equal settled lines))
          :cut (csvdt--narrowing-cuts-record-p
                settled (car dialect) (cdr dialect) records)
          :header (csvdt--header-line (caar settled) args records)
          :records records)))

(cl-defstruct (csvdt--job (:constructor csvdt--make-job))
  "One csvdt run, from its input being written to its result being shown.
SOURCE is the buffer the text comes from, ARGS the arguments, and INFO a
plist of what the report needs.  PIECES is the input still to be written
to INPUT, a temporary file, and TICK the source's modification count when
the run began.  PROCESS, RESULT and ERRORS are the csvdt process and the
buffers for its output and messages, once it has started, and PENDING how
many of the output and error streams are still open.  DONE is set when
the run has been reported, ERROR then holds the message if it failed.
ABORT is a reason the run was stopped, and SUPERSEDED says a later run
took its place."
  source args info pieces tick input process result errors (pending 2)
  done error abort superseded)

(defvar csvdt--current-job nil
  "The `csvdt--job' in progress, or nil.")

(defconst csvdt--chunk-size 1048576
  "How many characters of the buffer are written for csvdt at a time.")

(defun csvdt--input-pieces (ranges header)
  "Return the text of RANGES, with HEADER in front, as a list of pieces.
A piece is a string or a (BEG . END) pair of positions in this buffer.  One
range and no header go as they are.  Otherwise each stretch is made to end
in a newline, so the next one does not run on to it."
  (if (and (null header) (null (cdr ranges)))
      (list (car ranges))
    (let ((pieces '()))
      (when header
        (push header pieces)
        (unless (string-suffix-p "\n" header)
          (push "\n" pieces)))
      (dolist (range ranges)
        (when (< (car range) (cdr range))
          (push range pieces)
          (unless (eq (char-before (cdr range)) ?\n)
            (push "\n" pieces))))
      (nreverse pieces))))

(defun csvdt--next-input (source pieces)
  "Take up to `csvdt--chunk-size' characters from the front of PIECES.
PIECES are as `csvdt--input-pieces' returns them and SOURCE is the buffer
their positions are in.  Returns (TEXT . REST), REST being what is left."
  (let ((budget csvdt--chunk-size)
        (parts '()))
    (while (and pieces (> budget 0))
      (let ((piece (car pieces)))
        (if (stringp piece)
            (progn (push piece parts)
                   (setq budget (- budget (length piece))
                         pieces (cdr pieces)))
          (let ((stop (min (cdr piece) (+ (car piece) budget))))
            (push (with-current-buffer source
                    (save-restriction
                      (widen)
                      (buffer-substring-no-properties (car piece) stop)))
                  parts)
            (setq budget (- budget (- stop (car piece)))
                  pieces (if (< stop (cdr piece))
                             (cons (cons stop (cdr piece)) (cdr pieces))
                           (cdr pieces)))))))
    (cons (apply #'concat (nreverse parts)) pieces)))

(defun csvdt--stop (job reason &optional superseded)
  "Stop JOB, to be reported as having failed for REASON.
With SUPERSEDED it is dropped without a report, a later run having taken its
place."
  (unless (csvdt--job-done job)
    (setf (csvdt--job-abort job) reason
          (csvdt--job-superseded job) superseded)
    (if (process-live-p (csvdt--job-process job))
        (delete-process (csvdt--job-process job))
      (csvdt--finish job))))

(defun csvdt--write (job)
  "Write the next part of JOB's input to its file, then arrange for the rest.
The text is read from the source buffer a chunk at a time, from a timer, so
Emacs stays responsive and the buffer is never copied whole.  Once it is
all written csvdt is started on the file.  If the source has changed since
the run began, what was written no longer matches it and the run is
stopped."
  (let ((source (csvdt--job-source job)))
    (unless (or (csvdt--job-abort job) (csvdt--job-done job))
      (condition-case err
          (cond
           ((not (and (buffer-live-p source)
                      (eq (csvdt--job-tick job)
                          (buffer-chars-modified-tick source))))
            (csvdt--stop job "The buffer changed while csvdt was reading it"))
           ((null (csvdt--job-pieces job))
            (csvdt--launch job))
           (t
            (let ((next (csvdt--next-input source (csvdt--job-pieces job)))
                  (coding-system-for-write 'utf-8-unix))
              ;; In a buffer of its own, which has no local coding system
              ;; to override the one bound here.
              (with-temp-buffer
                (write-region (car next) nil (csvdt--job-input job) t
                              'silent))
              (setf (csvdt--job-pieces job) (cdr next))
              (run-at-time 0 nil #'csvdt--write job))))
        (error (csvdt--stop job (error-message-string err)))))))

(defun csvdt--sentinel (process _event)
  "Count PROCESS, a csvdt run or its error stream, as ended.
The run is reported once both have, so that all of csvdt's messages are
in hand."
  (unless (or (process-live-p process) (process-get process 'csvdt-ended))
    (process-put process 'csvdt-ended t)
    (let ((job (process-get process 'csvdt-job)))
      (when (zerop (setf (csvdt--job-pending job) (1- (csvdt--job-pending job))))
        (csvdt--finish job)))))

(defun csvdt--launch (job)
  "Start csvdt on the input JOB has written.
csvdt reads the file rather than standard input, which is what lets it be
written in pieces and read back at the speed of the disk, not of a pipe."
  (let* ((program (csvdt--program))
         (args (csvdt--job-args job))
         (result (generate-new-buffer " *csvdt-result*"))
         (errors (generate-new-buffer " *csvdt-errors*"))
         (process nil))
    (setf (csvdt--job-result job) result
          (csvdt--job-errors job) errors)
    (setq process
          (make-process
           :name "csvdt" :buffer result
           :command (append (list program) args
                            (if (member "--" args) '() '("--"))
                            (list (csvdt--job-input job)))
           ;; csvdt requires UTF-8 and refuses anything else.
           :coding 'utf-8-unix
           :connection-type 'pipe :noquery t
           :stderr errors :sentinel #'csvdt--sentinel))
    (let ((stderr (get-buffer-process errors)))
      (set-process-coding-system stderr 'utf-8-unix)
      (set-process-query-on-exit-flag stderr nil)
      (set-process-sentinel stderr #'csvdt--sentinel)
      (process-put stderr 'csvdt-job job))
    (process-put process 'csvdt-job job)
    (setf (csvdt--job-process job) process)))

(defun csvdt--start (source args pieces info)
  "Start a run of csvdt with ARGS on PIECES of SOURCE, returning its job.
The run is asynchronous: this returns at once, and `csvdt--finish' shows
the result when csvdt is done.  INFO is a plist of what the report needs.
A run still going is stopped and its result dropped."
  (when (and csvdt--current-job (not (csvdt--job-done csvdt--current-job)))
    (csvdt--stop csvdt--current-job nil t))
  (let ((job (csvdt--make-job
              :source source :args args :info info :pieces pieces
              :tick (buffer-chars-modified-tick source)
              :input (make-temp-file "csvdt-input"))))
    (setq csvdt--current-job job)
    (run-at-time 0 nil #'csvdt--write job)
    job))

(defun csvdt--fail (job message)
  "Record MESSAGE as why JOB failed, and say so."
  (setf (csvdt--job-error job) message)
  (message "%s" message))

(defun csvdt--finish (job)
  "Report JOB, whose process, if it got as far as one, has ended.
Marks it done when it has been reported, whatever came of it."
  (unless (csvdt--job-done job)
    (unwind-protect
        (unless (csvdt--job-superseded job)
          (condition-case err
              (csvdt--report job)
            (error (csvdt--fail job (error-message-string err)))))
      (dolist (buffer (list (csvdt--job-result job) (csvdt--job-errors job)))
        (when (buffer-live-p buffer)
          (kill-buffer buffer)))
      (ignore-errors (delete-file (csvdt--job-input job)))
      (when (eq csvdt--current-job job)
        (setq csvdt--current-job nil))
      (setf (csvdt--job-done job) t))))

(defun csvdt--report (job)
  "Show what the run JOB made, or why it failed.
A failed run shows what csvdt wrote before failing, except one fed from the
output buffer, whose contents are the input and are left alone."
  (let* ((info (csvdt--job-info job))
         (process (csvdt--job-process job))
         (result (csvdt--job-result job))
         (ranges (plist-get info :ranges))
         (cut (plist-get info :cut))
         (said (and (csvdt--job-errors job)
                    (with-current-buffer (csvdt--job-errors job)
                      (string-trim (buffer-string))))))
    (cond
     ((csvdt--job-abort job) (csvdt--fail job (csvdt--job-abort job)))
     ((not (and (eq (process-status process) 'exit)
                (eq (process-exit-status process) 0)))
      (unless (or (zerop (buffer-size result))
                  (eq (csvdt--job-source job) (get-buffer "*csvdt output*")))
        (display-buffer (csvdt--fill-output result)))
      ;; csvdt names itself on every line it writes, which this message has
      ;; already said.  The cut is named because csvdt reports a short
      ;; record as a problem with a line, which sends the reader to data
      ;; that is not the matter.
      (csvdt--fail job
                   (format "csvdt exited %s: %s%s" (process-exit-status process)
                           (if (string-empty-p said)
                               "(no message)"
                             (string-remove-prefix "csvdt: " said))
                           (if cut
                               (concat " -- note the " (csvdt--cut-note cut))
                             ""))))
     (t
      (let ((out (csvdt--fill-output result)))
        (display-buffer out)
        (message "csvdt %s: %d lines%s%s%s%s%s"
                 (string-join (csvdt--job-args job) " ")
                 (with-current-buffer out
                   (count-lines (point-min) (point-max)))
                 (if (cdr ranges)
                     (format " from %d stretches" (length ranges))
                   "")
                 (if (plist-get info :snapped)
                     ", selection widened to whole lines"
                   "")
                 (if (plist-get info :spanned)
                     ", widened to whole records: a quoted field spans lines"
                   "")
                 (if cut (concat ", " (csvdt--cut-note cut)) "")
                 (if (string-empty-p said) "" (concat "\n" said))))))))

(defun csvdt--run (ranges args &optional snapped)
  "Start csvdt with ARGS on RANGES of this buffer, and return its job.
RANGES is a list of (BEG . END) conses in buffer order, more than one being
how banked lines reach csvdt as one file.  They are widened as
`csvdt--plan' says, and ranges below the buffer's header go with that
header in front, so csvdt does not take the first selected row for one.
SNAPPED says the caller's raw selection had a boundary inside a line,
which `csvdt--mid-line-boundary-p' reports before normalising hides it.

The run happens in the background.  When csvdt is done the result is shown
in `*csvdt output*' and a summary is given, or the exit status and csvdt's
own message if it failed.  `csvdt--await' waits for the returned job."
  (let ((files (csvdt--file-arguments args)))
    (when files
      (user-error
       "Cannot run: csvdt would read %s and ignore the selection; %s"
       (string-join files ", ")
       "these commands send it the lines as a file of their own")))
  (csvdt--program)
  (let ((plan (csvdt--plan ranges args snapped)))
    (csvdt--start (current-buffer) args
                  (csvdt--input-pieces (plist-get plan :ranges)
                                       (plist-get plan :header))
                  (list :ranges (plist-get plan :ranges)
                        :snapped (plist-get plan :snapped)
                        :spanned (plist-get plan :spanned)
                        :cut (plist-get plan :cut)))))

(defun csvdt--await (job &optional timeout)
  "Wait for the run JOB to be reported, and signal if it failed.
For Lisp that wants a run to be over before it goes on, batch scripts and
tests above all.  The commands never call it, so Emacs is not held up.
TIMEOUT is the seconds to wait, a minute by default.  A failed run signals
a `user-error' with the message it reported.  Returns JOB."
  (let ((deadline (+ (float-time) (or timeout 60))))
    (while (and (not (csvdt--job-done job))
                (< (float-time) deadline))
      (accept-process-output nil 0.001)))
  (unless (csvdt--job-done job)
    (error "The csvdt run was not reported in time"))
  (when (csvdt--job-error job)
    (user-error "%s" (csvdt--job-error job)))
  job)

(defun csvdt--fill-output (result)
  "Make the text of buffer RESULT the contents of the output buffer.
Returns the output buffer.  The text is swapped between the two rather than
copied, so a large result is held once.  RESULT is left with what the
output buffer held, overlays another package put on it included."
  (let ((out (get-buffer-create "*csvdt output*")))
    (with-current-buffer out
      (let ((inhibit-read-only t))
        (buffer-swap-text result))
      (csvdt--apply-output-mode)
      (goto-char (point-min)))
    out))

;;; Commands

;;;###autoload
(defun csvdt-run-buffer (args)
  "Run csvdt over the whole buffer with ARGS.
The run is in the background; the result is shown when csvdt is done."
  (interactive (list (csvdt--read-arguments)))
  (csvdt--run (list (cons (point-min) (point-max))) args))

;;;###autoload
(defun csvdt-run-region (args)
  "Run csvdt over the active region with ARGS.
Useful for trying something on a handful of rows before the whole file.
Every piece of the region, where it has more than one: see
`csvdt--region-ranges'.  The pieces are put in buffer order and merged
first, since a package defining its own selection through
`region-extract-function' may report them in any order or overlapping.
The run is in the background; the result is shown when csvdt is done."
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
is measured across what was banked rather than across what was skipped.
The run is in the background; the result is shown when csvdt is done."
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
yet.  They are merged, so a selection abutting or inside a banked stretch
does not send a line twice.  With neither, the whole buffer.
The run is in the background; the result is shown when csvdt is done."
  (interactive (list (csvdt--read-arguments)))
  (let* ((banked-raw (csvdt--banked-lines))
         (region-raw (csvdt--region-ranges))
         (ranges (plist-get (csvdt--selection-ranges banked-raw region-raw)
                            :all)))
    (csvdt--run (or ranges (list (cons (point-min) (point-max)))) args
                (and ranges
                     (csvdt--mid-line-boundary-p
                      (append banked-raw region-raw))))))

;;;###autoload
(defun csvdt-describe-run ()
  "Say what a run would cover, without running anything.
For when the output covers more or less than expected.  It reports what
this package sees, which is not always what the buffer looks like: lines
that merely look banked, highlighted by something else or banked by a
package `csvdt-banked-lines-function' does not point at, are not banked as
far as csvdt is concerned.  Counted as a run counts, in whole records.  The
dialect is the one `csvdt-default-arguments' gives, since no arguments have
been typed yet."
  (interactive)
  (let* ((args (ignore-errors
                 (csvdt--split-arguments csvdt-default-arguments)))
         ;; This command must not raise what it is here to explain, so an
         ;; undefined banking function is described, not signalled.
         (selection (csvdt--selection-ranges
                     (and (functionp csvdt-banked-lines-function)
                          (csvdt--banked-lines))
                     (csvdt--region-ranges)))
         (selected (plist-get selection :all))
         (plan (csvdt--plan (or selected (list (cons (point-min) (point-max))))
                            args))
         (records (plist-get plan :records))
         (ranges (and selected (plist-get plan :ranges)))
         (banked (csvdt--settle-ranges (plist-get selection :banked) records))
         (region (csvdt--settle-ranges (plist-get selection :region) records))
         (cut (plist-get plan :cut))
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
           ((not (functionp csvdt-banked-lines-function))
            (format "nothing, `csvdt-banked-lines-function' names %S, \
which is not defined"
                    csvdt-banked-lines-function))
           (banked (funcall describe banked))
           (t "nothing reported -- lines that look banked may not be"))
     (if region (funcall describe region) "none")
     ;; Its own sentence, since a "Narrowing:" label would read "Narrowing:
     ;; last record cut short by the narrowing".
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
;; avoided above: the declared minimum is only true if the file loads
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

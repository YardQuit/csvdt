;;;; tuatara/test.lisp --- The csvdt package: arguments, whole records, and runs of a real csvdt.
;;;; Copyright (C) 2025-2026 Michael A Jones.  GPL-3.0-or-later; see LICENSE.
;;;;
;;;; Run with Tuatara's own tests (tools/test.sh) once this directory is
;;;; linked into its packages/ as csvdt; tuatara/run-tests.sh does that for a
;;;; run of these alone. The runs need a csvdt: $CSVDT, else the one cargo
;;;; builds in this repository, else the one on the PATH; without any, they
;;;; are skipped.

(in-package #:tuatara.tests)

(define-test csvdt
  :description "The csvdt package: command lines split as a shell splits them, options read from csvdt itself,
selections widened to whole records, the header carried, and csvdt run over a buffer, a selection and banked lines.")

(defparameter *csvdt-test-program*
  (or (uiop:getenv "CSVDT")
      (let ((built (merge-pathnames "../target/release/csvdt" (or *load-truename* *default-pathname-defaults*))))
        (and (probe-file built) (namestring (truename built))))
      (tuatara.io:find-program "csvdt"))
  "The csvdt the tests run, or NIL when there is none.")

(defmacro with-csvdt (() &body body)
  "Run BODY with *CSVDT-PROGRAM* the csvdt the tests run, or skip it without one."
  `(if (null *csvdt-test-program*)
       (skip "No csvdt to run: set CSVDT or build it" (true nil))
       (let ((tuatara-user::*csvdt-program* *csvdt-test-program*)
             (tuatara.defaults::*csvdt-options-cache* nil)
             (tuatara.defaults::*csvdt-history* '()))
         ,@body)))

(defun csvdt-test-rope (text)
  "TEXT as a rope."
  (tuatara.text:string->rope text))

(define-test (csvdt csvdt-split-arguments)
  (flet ((split (line) (tuatara.defaults::csvdt-split-arguments line)))
    ;; a comma stays inside its argument
    (is equal '("-H" "-p" "-a" "1,0" "-i" "0,replace") (split "-H -p -a 1,0 -i 0,replace"))
    (is equal '("-o0,+02:00") (split "-o0,+02:00"))
    ;; either quote, each holding what the other cannot
    (is equal '("--print-delimiter" " ") (split "--print-delimiter \" \""))
    (is equal '("--comment" "#") (split "--comment '#'"))
    (is equal '("--print-delimiter" "'") (split "--print-delimiter \"'\""))
    (is equal '("--print-delimiter" "\"") (split "--print-delimiter '\"'"))
    ;; the shell's escapes in double quotes, none in single ones
    (is equal (list (string #\Tab)) (split "\"\\t\""))
    (is equal '("C:\\data") (split "'C:\\data'"))
    (fail (split "\"C:\\data\""))
    ;; a backslash outside quotes is itself
    (is equal '("C:\\Users\\x") (split "C:\\Users\\x"))
    ;; every space a shell splits on, a pasted newline too
    (is equal '("-H" "--arr") (split (format nil "-H~%--arr~%")))
    (fail (split "-H \"open"))))

(define-test (csvdt csvdt-option-lists)
  (let ((records (tuatara.defaults::csvdt-parse-option-list
                  (format nil "# csvdt-option-list 1~%# short	long	value	separator	value-name~%-H	--has-header	none	none	~%-f	--flexible	optional	equals	keep~%"))))
    (is = 2 (length records))
    (is equal "--flexible" (getf (second records) :long))
    (is equal "equals" (getf (second records) :separator)))
  ;; a format not known, or no list at all: declined, for --help to be read instead
  (false (tuatara.defaults::csvdt-parse-option-list (format nil "# csvdt-option-list 2~%-H	--has-header	none	none~%")))
  (false (tuatara.defaults::csvdt-parse-option-list ""))
  ;; the --help fallback: options at the start of a line, the first of each
  (let ((records (tuatara.defaults::csvdt-parse-help
                  (format nil "Options:~%  -H, --has-header  first row~%      --trim <T>  how~%      --trim again in prose~%"))))
    (is equal '("--has-header" "--trim") (mapcar (lambda (r) (getf r :long)) records))
    (is equal "-H" (getf (first records) :short))
    (false (getf (first records) :value))))

(define-test (csvdt csvdt-whole-records)
  (let ((rope (csvdt-test-rope (format nil "h,when~%\"x~%y\",2026-07-25T12:00:00+02:00~%z,2026-07-26T12:00:00+02:00~%"))))
    ;; a range starting inside a quoted field is widened back to its record
    (is equal (cons 7 41) (tuatara.defaults::csvdt-whole-records rope 10 41 34 44))
    (true (tuatara.defaults::csvdt-inside-quoted-field-p rope 10 34 44))
    (false (tuatara.defaults::csvdt-inside-quoted-field-p rope 41 34 44))
    ;; a quote not where a field begins is data
    (false (tuatara.defaults::csvdt-inside-quoted-field-p (csvdt-test-rope (format nil "a\"b,c~%d,e~%")) 7 34 44)))
  ;; part of a line takes the whole line; an end at a line's start stops there
  (let ((rope (csvdt-test-rope (format nil "ab,c~%de,f~%gh,i~%"))))
    (is equal (cons 0 10) (tuatara.defaults::csvdt-whole-lines rope 2 7))
    (is equal (cons 5 10) (tuatara.defaults::csvdt-whole-lines rope 5 10))
    ;; two lines widening to one record are sent once
    (is equal '((0 . 10)) (tuatara.defaults::csvdt-drop-covered '((0 . 10) (5 . 10))))))

(define-test (csvdt csvdt-header-options)
  (with-csvdt ()
    (flet ((header-p (line) (tuatara.defaults::csvdt-header-requested-p (tuatara.defaults::csvdt-split-arguments line))))
      (true (header-p "-H -p"))
      (true (header-p "--has-header"))
      (true (header-p "-Hp"))
      (true (header-p "-pH"))
      ;; an H inside another option's value, or past --, is no header option
      (false (header-p "-u0 -iH"))
      (false (header-p "-p -- -H")))
    ;; every option this package names is one csvdt reports
    (let ((reported (tuatara.defaults::csvdt-options)))
      (dolist (option (append tuatara-user::*csvdt-header-options* tuatara-user::*csvdt-single-quote-options*
                              tuatara-user::*csvdt-read-delimiter-options*))
        (true (member option reported :test #'string=) "~a is not an option csvdt reports" option))
      ;; an option whose value must be joined with = is offered with it
      (true (member "--flexible=" reported :test #'string=)))))

(define-test (csvdt csvdt-names-no-other-option)
  ;; options are csvdt's to report: none but the three settings' is named in the package
  (with-csvdt ()
    (let* ((source (alexandria:read-file-into-string
                    (merge-pathnames "csvdt.lisp" (or *load-truename* *default-pathname-defaults*))))
           (allowed (append '("--list-options" "--help" "--version")
                            tuatara-user::*csvdt-header-options* tuatara-user::*csvdt-single-quote-options*
                            tuatara-user::*csvdt-read-delimiter-options*)))
      (dolist (option (tuatara.defaults::csvdt-options))
        (let ((option (string-right-trim "=" option)))
          (unless (or (member option allowed :test #'string=) (< (length option) 3))
            (false (cl-ppcre:scan (format nil "[^A-Za-z0-9-]~a[^A-Za-z0-9-]" (cl-ppcre:quote-meta-chars option)) source)
                   "~a is named in csvdt.lisp" option)))))))

(defparameter *csvdt-test-text*
  (format nil "ts,x~%2026-07-25T12:00:00+02:00,a~%2026-07-26T12:00:00+02:00,b~%2026-07-27T12:00:00+02:00,c~%")
  "A buffer with a header and a timestamp: without a conversion, -H -p and no
arguments at all would give the same output.")

(defun csvdt-test-output ()
  "The text of *csvdt output*, once the runs started have ended and been taken in."
  (loop while (find-if #'tuatara.runtime:job-running-p tuatara.runtime:*jobs*) do (sleep 0.01))
  (let ((out (tuatara.buffer:find-buffer "*csvdt output*")))
    (and out (tuatara.buffer:buffer-string out))))

(defun csvdt-test-run (keys &rest options)
  "Run KEYS in a session (RUN-KEYS-IN-TURNS takes OPTIONS), *csvdt output*
killed first; the frontend, the editor and the output."
  (let ((old (tuatara.buffer:find-buffer "*csvdt output*")))
    (when old (tuatara.buffer:kill-buffer old)))
  (multiple-value-bind (f editor) (apply #'run-keys-in-turns (list keys) :text *csvdt-test-text* options)
    (values f editor (csvdt-test-output))))

(define-test (csvdt csvdt-run-buffer)
  (with-csvdt ()
    ;; the arguments offered, selected, typed over
    (multiple-value-bind (f editor out) (csvdt-test-run "M-x c s v d t - r u n - b u f f e r RET - H SPC - p SPC - u 0 SPC - i SPC 0 , r e p l a c e RET")
      (declare (ignore editor))
      (is string= (format nil "to_utc,x~%2026-07-25T10:00:00+00:00,a~%2026-07-26T10:00:00+00:00,b~%2026-07-27T10:00:00+00:00,c~%") out)
      (true (tuatara.frontend.fake:fake-frame-showing f "csvdt -H -p -u0 -i 0,replace: 4 lines")))
    ;; RET runs the arguments offered as they are
    (multiple-value-bind (f editor out) (csvdt-test-run "M-x c s v d t - r u n - b u f f e r RET RET")
      (declare (ignore f editor))
      (is string= *csvdt-test-text* out))
    ;; the right arrow keeps them, to add to
    (multiple-value-bind (f editor out) (csvdt-test-run "M-x c s v d t - r u n - b u f f e r RET <right> SPC - u 0 SPC - i SPC 0 , r e p l a c e RET")
      (declare (ignore f editor))
      (true (search "10:00:00+00:00" out)))
    ;; the source is never changed
    (csvdt-test-run "M-x c s v d t - r u n - b u f f e r RET - H SPC - u 0 RET")
    (is string= *csvdt-test-text* (tuatara.buffer:buffer-string (tuatara.buffer:find-buffer "*scratch*")))))

(define-test (csvdt csvdt-run-region-carries-header)
  (with-csvdt ()
    ;; from the middle of the first row to the middle of the second: both rows, whole, the header in front
    (multiple-value-bind (f editor out)
        (csvdt-test-run "M-x c s v d t - r u n - r e g i o n RET - H SPC - p SPC - u 0 SPC - i SPC 0 , r e p l a c e RET" :point 12 :mark 40)
      (declare (ignore editor))
      (is string= (format nil "to_utc,x~%2026-07-25T10:00:00+00:00,a~%2026-07-26T10:00:00+00:00,b~%") out)
      (true (tuatara.frontend.fake:fake-frame-showing f "selection widened to whole lines")))
    ;; without a header asked for, nothing is put in front
    (multiple-value-bind (f editor out)
        (csvdt-test-run "M-x c s v d t - r u n - r e g i o n RET - u 0 SPC - i SPC 0 , r e p l a c e RET" :point 33 :mark 61)
      (declare (ignore f editor))
      (is string= (format nil "2026-07-26T10:00:00+00:00,b~%") out))))

(define-test (csvdt csvdt-run-banked)
  (with-csvdt ()
    (let ((old (tuatara.buffer:find-buffer "*csvdt output*")))
      (when old (tuatara.buffer:kill-buffer old)))
    ;; the first and third rows banked: sent as one file, the header in front, the row between left out
    (multiple-value-bind (f editor)
        (run-keys (list (lambda ()
                          (let ((b (tuatara.command:current-buffer)))
                            (tuatara.defaults::bank-lines b (cons 5 33))
                            (tuatara.defaults::bank-lines b (cons 61 89))))
                        "M-x c s v d t - r u n - b a n k e d RET - H SPC - p SPC - u 0 SPC - i SPC 0 , r e p l a c e RET")
                  :text *csvdt-test-text* :width 120)
      (declare (ignore editor))
      (is string= (format nil "to_utc,x~%2026-07-25T10:00:00+00:00,a~%2026-07-27T10:00:00+00:00,c~%") (csvdt-test-output))
      (true (tuatara.frontend.fake:fake-frame-showing f "3 lines from 2 stretches")))
    ;; dwim takes the banked lines and the selection together, a line in both sent once
    (multiple-value-bind (f editor)
        (run-keys (list (lambda ()
                          (let ((b (tuatara.command:current-buffer)))
                            (tuatara.defaults::bank-lines b (cons 5 33))
                            (setf (tuatara.buffer:buffer-mark b) 20 (tuatara.buffer:buffer-point b) 40
                                  (tuatara.buffer:buffer-mark-active-p b) t)))
                        "M-x c s v d t - r u n - d w i m RET - H SPC - p SPC - u 0 SPC - i SPC 0 , r e p l a c e RET")
                  :text *csvdt-test-text* :width 120)
      (declare (ignore f editor))
      (is string= (format nil "to_utc,x~%2026-07-25T10:00:00+00:00,a~%2026-07-26T10:00:00+00:00,b~%") (csvdt-test-output)))))

(define-test (csvdt csvdt-refusals)
  (with-csvdt ()
    ;; a file named among the arguments would take the lines' place
    (with-test-directory (dir)
      (let ((file (namestring (merge-pathnames "sales.csv" dir))))
        (alexandria:write-string-into-file "a,b" file)
        (let ((b (tuatara.buffer:make-buffer "refused" :text "a,b")))
          (unwind-protect (fail (tuatara.defaults::csvdt-run b '((0 . 3)) (list "-H" file)))
            (tuatara.buffer:kill-buffer b)))))
    ;; a value apart from its option is no file, though a file of that name is there
    (is equal '() (tuatara.defaults::csvdt-file-arguments '("-u" "csvdt") "/usr/bin/"))
    ;; csvdt's own reason, with its exit status
    (multiple-value-bind (f editor) (csvdt-test-run "M-x c s v d t - r u n - b u f f e r RET - H SPC - u 9 RET")
      (declare (ignore editor))
      (true (tuatara.frontend.fake:fake-frame-showing f "csvdt exited")))
    ;; a program that is not there says which setting to set
    (let ((tuatara-user::*csvdt-program* "/no/such/csvdt"))
      (fail (tuatara.defaults::csvdt-program "/")))))

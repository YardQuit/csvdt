;;;; tuatara/run-tests.lisp --- The csvdt package's tests alone, in a Tuatara loaded from source.
;;;; Copyright (C) 2025-2026 Michael A Jones.  GPL-3.0-or-later; see LICENSE.
;;;;
;;;; sbcl --non-interactive --load tuatara/run-tests.lisp, with TUATARA naming
;;;; a Tuatara checkout (run-tests.sh sets it). Exits 0 when every test
;;;; passes, 1 when one fails, 2 on a compiler warning in the package.

(defvar *tuatara* (let ((path (sb-ext:posix-getenv "TUATARA")))
                    (pathname (if (char= #\/ (char path (1- (length path)))) path (concatenate (quote string) path "/")))))
(load (merge-pathnames "tools/quicklisp.lisp" *tuatara*))
(asdf:load-asd (merge-pathnames "tuatara.asd" *tuatara*))
(ql:quickload :tuatara/tests :silent t)

(defvar *compiler-warnings* '())
(handler-bind ((warning (lambda (w)
                          (unless (typep w 'style-warning)
                            (push (princ-to-string w) *compiler-warnings*)))))
  (load (merge-pathnames "load.lisp" *load-truename*))
  (load (merge-pathnames "test.lisp" *load-truename*)))
(when *compiler-warnings*
  (format t "~&~%Compiler warnings (~a):~%~{  ~a~%~}" (length *compiler-warnings*) (reverse *compiler-warnings*))
  (sb-ext:exit :code 2))
(let ((report (parachute:test 'tuatara.tests::csvdt :report 'parachute:plain)))
  (tuatara.tests::remove-test-directories)
  (sb-ext:exit :code (if (eql (parachute:status report) :passed) 0 1)))

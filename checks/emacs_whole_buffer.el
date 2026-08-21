;;; The front end must give what the binary gives, byte for byte.  -*- lexical-binding: t; -*-
;;
;; A run from Emacs assembles the text, encodes it, hands it over and decodes
;; the answer.  Every one of those steps can alter bytes -- a locale's coding
;; system, a line ending, a buffer without a final newline -- so the check is
;; simply that the output buffer holds what the shell would have shown, for
;; the same input and the same arguments.
;;
;; A run the binary refuses must reach the user as an error rather than as
;; output.  That is the package's documented behaviour, so it is checked as
;; such rather than compared as text.

(require 'csvdt)

(defvar checks-failures 0)
(defvar checks-run 0)

(defun checks-cli (text args)
  "Exit status and standard output for TEXT through the binary."
  (with-temp-buffer
    (let ((coding-system-for-write 'utf-8-unix)
          (coding-system-for-read 'utf-8-unix)
          (input (make-temp-file "csvdt-check"))
          status)
      (unwind-protect
          (progn
            (with-temp-file input
              (let ((coding-system-for-write 'utf-8-unix)) (insert text)))
            (setq status (apply #'call-process csvdt-executable input
                                (list t nil) nil args))
            (cons status (buffer-string)))
        (delete-file input)))))

(defun checks-front-end (text args)
  "Exit-ish status and output buffer contents for the same run from Emacs."
  (let ((source (generate-new-buffer " *csvdt-check-source*")))
    (unwind-protect
        (with-current-buffer source
          (insert text)
          (condition-case _
              (progn (csvdt-run-buffer args)
                     (cons 0 (with-current-buffer "*csvdt output*"
                               (buffer-string))))
            (error (cons 1 "raised"))))
      (kill-buffer source)
      (when (get-buffer "*csvdt output*") (kill-buffer "*csvdt output*")))))

(defun checks-compare (label text args)
  (let* ((want (checks-cli text args))
         (got (checks-front-end text args)))
    (setq checks-run (1+ checks-run))
    (unless (if (eq (car want) 0)
                (equal want got)
              (eq (car got) 1))
      (setq checks-failures (1+ checks-failures))
      (when (<= checks-failures 6)
        (princ (format "  %s %S\n    input: %S\n    shell: %S\n    emacs: %S\n"
                       label args text want got))))))

(let ((inputs
       '(("plain" . "a,b,c\n2024-06-15T14:00:00Z,1,x\n2024-06-16T15:30:00+02:00,2,y\n")
         ("no final newline" . "a,b,c\n2024-06-15T14:00:00Z,1,x")
         ("crlf" . "a,b,c\r\n2024-06-15T14:00:00Z,1,x\r\n")
         ("quoted" . "a,b,c\n\"x,1\",\"q\"\"q\",\"y\nz\"\n")
         ("unicode" . "a,b,c\né,中文,\U0001F600\n")
         ("empty fields" . "a,b,c\n,,\n")
         ("ragged" . "a,b,c\n1,2\n3,4,5,6\n")
         ("comment" . "#lead\na,b,c\n2024-06-15T14:00:00Z,1,x\n")
         ("blank line" . "a,b,c\n\n1,2,3\n")
         ("leap second" . "a,b\n2016-12-31T23:59:60Z,1\n2024-06-15T14:00:60Z,2\n")
         ("padded" . "a,b,c\n  2024-06-15T14:00:00Z  , 1 ,x\n")))
      (argsets
       '(nil ("-H" "-p") ("-u0") ("-H" "-p" "-u0") ("-s0") ("-r1")
         ("-d0") ("-f") ("-f=fix") ("-f=widest") ("--trim" "all")
         ("-u0" "-i0,replace") ("--remove" "1") ("-a" "2,0")
         ("--comment" "#") ("-H" "-p" "-f=widest") ("-o0,+05:30"))))
  (dolist (pair inputs)
    (dolist (args argsets)
      (checks-compare (car pair) (cdr pair) args))))

(princ (format "emacs whole buffer: %d runs, %d divergences\n"
               checks-run checks-failures))
(kill-emacs (if (> checks-failures 0) 1 0))

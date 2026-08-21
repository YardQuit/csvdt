;;; A selection must send exactly the records it touches, each once.  -*- lexical-binding: t; -*-
;;
;; The buffer is assembled from records whose byte spans are known here by
;; construction, so which records a selection touches is arithmetic rather
;; than anything the package decides.  That is what makes this a check rather
;; than a restatement: the reference does not share the code under test.
;;
;; Both shapes of selection are covered.  A region is one stretch, and the
;; question is whether it reaches out to whole records -- a stretch beginning
;; inside a field that spans lines is the back half of a record, and sending
;; it alone converts cleanly into data nobody wrote.  Banked lines are
;; several stretches, and the question is the opposite one: two of them
;; inside the same record must not send it twice.

(require 'csvdt)

(defvar checks-failures 0)
(defvar checks-runs 0)
(defvar checks-spanning 0)
(defvar checks-shared 0)

(defun checks-field ()
  (pcase (random 9)
    (0 "plain")
    (1 "\"quoted\"")
    (2 "\"holds,a,comma\"")
    (3 "\"holds\na\nnewline\"")
    (4 "\"doubled\"\"quote\"")
    (5 "\"three\nline\nfield\"")
    (6 "")
    (7 "with space")
    (_ "x")))

(defun checks-build (count)
  "Return (TEXT . SPANS), each record's (BEG . END) known by construction."
  (let ((text "h1,h2,when\n") (spans '()))
    (push (cons 1 (1+ (length text))) spans)
    (dotimes (n count)
      (let* ((record (concat (checks-field) "," (checks-field) ","
                             (format "2026-07-%02dT12:00:00+02:00"
                                     (1+ (mod n 28))) "\n"))
             (beg (1+ (length text))))
        (setq text (concat text record))
        (push (cons beg (+ beg (length record))) spans)))
    (cons text (nreverse spans))))

(defun checks-cli (text args)
  (with-temp-buffer
    (let ((coding-system-for-write 'utf-8-unix)
          (coding-system-for-read 'utf-8-unix)
          (input (make-temp-file "csvdt-check")) status)
      (unwind-protect
          (progn
            (with-temp-file input
              (let ((coding-system-for-write 'utf-8-unix)) (insert text)))
            (setq status (apply #'call-process csvdt-executable input
                                (list t nil) nil args))
            (cons status (buffer-string)))
        (delete-file input)))))

(defun checks-touched (spans ranges)
  "The spans any of RANGES overlaps, in buffer order, each once."
  (seq-filter (lambda (span)
                (seq-some (lambda (range)
                            (and (< (car span) (cdr range))
                                 (> (cdr span) (car range))))
                          ranges))
              spans))

(defun checks-selection (args ranges-of label)
  "Run one case: RANGES-OF returns the selection for the built buffer."
  (let* ((built (checks-build (+ 2 (random 5))))
         (text (car built))
         (spans (cdr built))
         (buffer (generate-new-buffer " *csvdt-check*")))
    (unwind-protect
        (with-current-buffer buffer
          (insert text)
          (let* ((ranges (funcall ranges-of))
                 (touched (checks-touched spans ranges)))
            (when (and ranges touched)
              (setq checks-runs (1+ checks-runs))
              (when (> (length touched) (length ranges))
                (setq checks-spanning (1+ checks-spanning)))
              (when (< (length touched) (length ranges))
                (setq checks-shared (1+ checks-shared)))
              (let* ((header (car spans))
                     (below (>= (car (car touched)) (cdr header)))
                     (body (mapconcat
                            (lambda (span)
                              (substring text (1- (car span)) (1- (cdr span))))
                            touched ""))
                     (expected (if (and csvdt-region-carries-header below
                                        (csvdt--header-requested-p args))
                                   (concat (substring text (1- (car header))
                                                      (1- (cdr header)))
                                           body)
                                 body))
                     (want (checks-cli expected args))
                     (got (condition-case _
                              (progn (csvdt--run ranges args)
                                     (cons 0 (with-current-buffer "*csvdt output*"
                                               (buffer-string))))
                            (error (cons 1 "raised")))))
                (unless (if (eq (car want) 0) (equal want got) (eq (car got) 1))
                  (setq checks-failures (1+ checks-failures))
                  (when (<= checks-failures 5)
                    (princ (format "  %s %S\n    buffer:   %S\n    ranges:   %S\n    should send: %S\n    shell: %S\n    emacs: %S\n"
                                   label args text ranges expected want got))))))))
      (kill-buffer buffer)
      (when (get-buffer "*csvdt output*") (kill-buffer "*csvdt output*")))))

(random "csvdt-selection-checks")

;; One stretch, anywhere at all.
(dolist (args '(nil ("-u2") ("-H" "-p") ("-H" "-p" "-u2") ("-s2") ("-f=widest")))
  (dolist (carries '(t nil))
    (dotimes (_ 60)
      (let ((csvdt-region-carries-header carries))
        (checks-selection
         args
         (lambda ()
           (let* ((beg (+ 1 (random (max 1 (- (point-max) 1)))))
                  (end (+ beg (random (max 1 (- (point-max) beg))))))
             (list (cons beg end))))
         "region")))))

;; Several stretches, which is what a banking package reports.
(dolist (args '(nil ("-u2") ("-f=widest")))
  (dotimes (_ 150)
    (checks-selection
     args
     (lambda ()
       (let* ((lines (count-lines (point-min) (point-max)))
              (picks (sort (delete-dups
                            (let (acc)
                              (dotimes (_ (+ 2 (random 2)))
                                (push (random (max 1 lines)) acc))
                              acc))
                           #'<)))
         (mapcar (lambda (n)
                   (goto-char (point-min))
                   (forward-line n)
                   (cons (point) (progn (forward-line 1) (point))))
                 picks)))
     "banked")))

(princ (format "emacs selections: %d runs, %d reaching past their lines, %d sharing a record, %d divergences\n"
               checks-runs checks-spanning checks-shared checks-failures))
(kill-emacs (if (> checks-failures 0) 1 0))

;;;; tuatara/load.lisp --- Load the csvdt package: csvdt run over a buffer, a selection or banked lines.
;;;; Copyright (C) 2025-2026 Michael A Jones.  GPL-3.0-or-later; see LICENSE.
;;;;
;;;; Kept in the csvdt repository rather than with Tuatara, because it and the
;;;; csvdt beside it share a contract. With this directory linked into
;;;; Tuatara's packages/ as csvdt, one line in the init file loads it:
;;;; (load-package "csvdt"). Without the link, (load "/path/to/csvdt/tuatara/load.lisp").

(load (merge-pathnames "csvdt.lisp" (or *load-truename* *default-pathname-defaults*)))

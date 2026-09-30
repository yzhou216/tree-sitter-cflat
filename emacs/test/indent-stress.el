;;; indent-stress.el --- Reindent a corpus and check nothing changed -*- lexical-binding: t; -*-

;; SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
;; SPDX-License-Identifier: GPL-3.0-or-later

;;; Commentary:

;; Indentation only moves whitespace, so reindenting a buffer must leave its
;; parse tree unchanged.  An indent rule that fired inside a multi-line block
;; comment, for instance, would silently rewrite the comment's contents.
;;
;; This script checks that property, and that indentation is idempotent,
;; for every `.cb' file under a directory.  Point it at the extracted
;; corpus or at `scripts/fuzz.rs --dump' output:
;;
;;   CFLAT_GRAMMAR_DIR=... emacs -Q --batch -L emacs \
;;     -l emacs/test/indent-stress.el -f cflat-indent-stress-main DIR

;;; Code:

(require 'treesit)

(when-let* ((dir (getenv "CFLAT_GRAMMAR_DIR"))
            ((not (string-empty-p dir))))
  (add-to-list 'treesit-extra-load-path (file-name-as-directory dir)))

(require 'cflat-ts-mode)

(defun cflat-indent-stress--tree (source)
  "Return the parse tree of SOURCE as a string, ignoring whitespace."
  (with-temp-buffer
    (insert source)
    (treesit-node-string (treesit-parser-root-node (treesit-parser-create 'cflat)))))

(defun cflat-indent-stress--check (file)
  "Reindent FILE's contents and describe what went wrong, or return nil."
  (let* ((original (with-temp-buffer
                     (insert-file-contents file)
                     (buffer-string)))
         (before (cflat-indent-stress--tree original)))
    (with-temp-buffer
      (insert original)
      (cflat-ts-mode)
      (indent-region (point-min) (point-max))
      (let* ((indented (buffer-string))
             (after (cflat-indent-stress--tree indented)))
        (if (not (equal before after))
            (format "tree changed\n--- before ---\n%s\n--- after ---\n%s\n--- text ---\n%s"
                    before after indented)
          (indent-region (point-min) (point-max))
          (unless (equal (buffer-string) indented)
            (format "indentation is not idempotent\n--- first ---\n%s\n--- second ---\n%s"
                    indented (buffer-string))))))))

(defun cflat-indent-stress-main ()
  "Check every `.cb' file under the directory named in `argv'."
  (let* ((directory (or (car argv) (error "Usage: ... -f cflat-indent-stress-main DIR")))
         (files (or (directory-files-recursively directory (rx ".cb" eos))
                    (error "No .cb files under %s" directory)))
         (failures (seq-count
                    (lambda (file)
                      (when-let* ((problem (cflat-indent-stress--check file)))
                        (princ (format "FAIL %s\n%s\n\n" file problem))))
                    files)))
    (princ (format "indent stress: %d/%d files preserved their tree and are idempotent\n"
                   (- (length files) failures) (length files)))
    (kill-emacs (if (zerop failures) 0 1))))

(provide 'indent-stress)

;;; indent-stress.el ends here

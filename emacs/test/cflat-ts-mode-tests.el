;;; cflat-ts-mode-tests.el --- Tests for cflat-ts-mode -*- lexical-binding: t; -*-

;; SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
;; SPDX-License-Identifier: GPL-3.0-or-later

;;; Commentary:

;; Run with `make -C emacs test', which builds the grammar first.  By hand:
;;
;;   CFLAT_GRAMMAR_DIR=/path/to/dir/with/libtree-sitter-cflat.so \
;;     emacs -Q --batch -L emacs -l emacs/test/cflat-ts-mode-tests.el \
;;           -f ert-run-tests-batch-and-exit

;;; Code:

(require 'ert)
(require 'treesit)

(when-let* ((dir (getenv "CFLAT_GRAMMAR_DIR"))
            ((not (string-empty-p dir))))
  (add-to-list 'treesit-extra-load-path (file-name-as-directory dir)))

(require 'cflat-ts-mode)

(defvar cflat-tests--program "\
// A point in the plane.
struct Point {
  x: int,
  y: int,
  parent: &Point
}

extern print_num : (int) -> int;

fn square(x: int) -> int {
  return x * x;
}

fn main() -> int {
  let p: &Point, i: int;
  p = new Point;
  i = p.x;
  if i > 0 {
    i = square(i);
  } else {
    i = 0;
  }
  while i > 0 {
    i = i - 1;
  }
  return i;
}
"
  "Well-formed, canonically indented C♭ program shared by these tests.")

(defmacro cflat-tests--with-buffer (content &rest body)
  "Run BODY in a `cflat-ts-mode' buffer containing CONTENT."
  (declare (indent 1) (debug (form body)))
  `(with-temp-buffer
     (insert ,content)
     (goto-char (point-min))
     (cflat-ts-mode)
     ,@body))

(defun cflat-tests--face-at (needle content &optional level)
  "Return the face applied to the first occurrence of NEEDLE in CONTENT.
LEVEL sets `treesit-font-lock-level' and defaults to 4.

The search is case-sensitive, as C♭ identifiers are, because several
tests look for a capitalized type name that also occurs lowercased in a
comment."
  (cflat-tests--with-buffer content
    (setq-local treesit-font-lock-level (or level 4))
    (treesit-font-lock-recompute-features)
    (font-lock-mode 1)
    (font-lock-ensure)
    (goto-char (point-min))
    (let ((case-fold-search nil))
      (search-forward needle))
    (get-text-property (match-beginning 0) 'face)))

(defun cflat-tests--reindented (content)
  "Return CONTENT after `indent-region' over the whole buffer."
  (cflat-tests--with-buffer content
    (indent-region (point-min) (point-max))
    (buffer-string)))

(ert-deftest cflat-ts-mode-activates ()
  (cflat-tests--with-buffer cflat-tests--program
    (should (eq major-mode 'cflat-ts-mode))
    (should (treesit-parser-list nil 'cflat))))

(ert-deftest cflat-ts-mode-registers-its-grammar-recipe ()
  (should (equal (assq 'cflat treesit-language-source-alist)
                 '(cflat "https://github.com/yzhou216/tree-sitter-cflat"))))

(ert-deftest cflat-ts-mode-parses-without-error ()
  (cflat-tests--with-buffer cflat-tests--program
    (should-not (treesit-search-subtree
                 (treesit-buffer-root-node 'cflat)
                 "\\`ERROR\\'"))))

(ert-deftest cflat-ts-mode-sets-comment-syntax ()
  (cflat-tests--with-buffer cflat-tests--program
    (should (equal comment-start "// "))
    (should (equal comment-end ""))))

(ert-deftest cflat-ts-mode-comment-region-uses-line-comments ()
  (cflat-tests--with-buffer "fn f() -> int {\n  return 0;\n}\n"
    (comment-region (point-min) (point-max))
    (should (string-prefix-p "//" (buffer-string)))))

(ert-deftest cflat-ts-mode-indent-new-line-continues-a-line-comment ()
  (cflat-tests--with-buffer "fn f() -> int {\n  // a comment\n}\n"
    (search-forward "comment")
    (default-indent-new-line)
    (should (equal (buffer-substring (pos-bol) (pos-eol)) "  // "))))

(ert-deftest cflat-ts-mode-indent-is-idempotent ()
  (should (equal (cflat-tests--reindented cflat-tests--program)
                 cflat-tests--program)))

(ert-deftest cflat-ts-mode-indent-from-flat ()
  (should (equal (cflat-tests--reindented
                  (replace-regexp-in-string "^[ \t]+" "" cflat-tests--program))
                 cflat-tests--program)))

(ert-deftest cflat-ts-mode-indent-from-overindented ()
  ;; Only non-blank lines, the only ones `indent-region' touches
  (should (equal (cflat-tests--reindented
                  (replace-regexp-in-string "^\\(.\\)" "      \\1"
                                            cflat-tests--program))
                 cflat-tests--program)))

(ert-deftest cflat-ts-mode-indent-respects-offset ()
  (cflat-tests--with-buffer "fn f() -> int {\nreturn 0;\n}\n"
    (setq-local cflat-ts-indent-offset 4)
    (indent-region (point-min) (point-max))
    (should (equal (buffer-string) "fn f() -> int {\n    return 0;\n}\n"))))

(ert-deftest cflat-ts-mode-indent-else-aligns-with-if ()
  (should (equal (cflat-tests--reindented
                  "fn f() -> int {\nif x {\ni = 1;\n} else {\ni = 2;\n}\n}\n")
                 "fn f() -> int {\n  if x {\n    i = 1;\n  } else {\n    i = 2;\n  }\n}\n")))

(ert-deftest cflat-ts-mode-indent-struct-fields ()
  (should (equal (cflat-tests--reindented "struct P {\nx: int,\ny: int\n}\n")
                 "struct P {\n  x: int,\n  y: int\n}\n")))

(ert-deftest cflat-ts-mode-indent-nested-blocks ()
  (should (equal (cflat-tests--reindented
                  "fn f() -> int {\nwhile a {\nif b {\nwhile c {\nx = 1;\n}\n}\n}\n}\n")
                 (concat "fn f() -> int {\n"
                         "  while a {\n"
                         "    if b {\n"
                         "      while c {\n"
                         "        x = 1;\n"
                         "      }\n"
                         "    }\n"
                         "  }\n"
                         "}\n"))))

(ert-deftest cflat-ts-mode-indent-leaves-comment-bodies-alone ()
  "A ragged multi-line comment body must survive `indent-region'.

The tree-shape stress check cannot see this, because reflowing a
comment's interior changes its text but not the tree."
  (let ((source (concat "fn f() -> int {\n"
                        "  /* a block comment\n"
                        "       deliberately ragged\n"
                        "   inside a function */\n"
                        "  return 0;\n"
                        "}\n")))
    (should (equal (cflat-tests--reindented source) source))))

(ert-deftest cflat-ts-mode-indent-leaves-unterminated-comment-bodies-alone ()
  (let ((source "fn f() -> int {}\n/* never closed\n      ragged\n   still open\n"))
    (should (equal (cflat-tests--reindented source) source))))

(ert-deftest cflat-ts-mode-indent-top-level-is-column-zero ()
  (should (equal (cflat-tests--reindented "   struct A {}\n     fn f() -> int {}\n")
                 "struct A {}\nfn f() -> int {}\n")))

(ert-deftest cflat-ts-mode-fontifies-comments ()
  (should (eq (cflat-tests--face-at "// A point" cflat-tests--program)
              'font-lock-comment-face)))

(ert-deftest cflat-ts-mode-fontifies-keywords ()
  (dolist (keyword '("struct" "extern" "fn" "let" "if" "else" "while" "return" "new"))
    (should (eq (cflat-tests--face-at keyword cflat-tests--program)
                'font-lock-keyword-face))))

(ert-deftest cflat-ts-mode-fontifies-types ()
  (should (eq (cflat-tests--face-at "int" "fn f(a: int) -> int {}\n")
              'font-lock-type-face)))

(ert-deftest cflat-ts-mode-fontifies-function-definition-name ()
  (should (eq (cflat-tests--face-at "square" cflat-tests--program)
              'font-lock-function-name-face)))

(ert-deftest cflat-ts-mode-fontifies-struct-name ()
  (should (eq (cflat-tests--face-at "Point" cflat-tests--program)
              'font-lock-type-face)))

(ert-deftest cflat-ts-mode-fontifies-numbers ()
  (should (eq (cflat-tests--face-at "17" "fn f() -> int { return 17; }\n")
              'font-lock-number-face)))

(ert-deftest cflat-ts-mode-fontifies-nil-as-constant ()
  (should (eq (cflat-tests--face-at "nil" "fn f() -> int { p = nil; }\n")
              'font-lock-constant-face)))

(ert-deftest cflat-ts-mode-fontifies-word-operators-as-operators ()
  (dolist (operator '("and" "or" "not"))
    (should (eq (cflat-tests--face-at
                 operator
                 (format "fn f() -> int { x = a %s b; }\n" operator))
                'font-lock-operator-face))))

(ert-deftest cflat-ts-mode-fontifies-symbol-operators ()
  (should (eq (cflat-tests--face-at "+" "fn f() -> int { x = a + b; }\n")
              'font-lock-operator-face)))

(ert-deftest cflat-ts-mode-fontifies-dereference-as-an-operator ()
  (should (eq (cflat-tests--face-at "*" "fn f() -> int { x = p.*; }\n")
              'font-lock-operator-face)))

(ert-deftest cflat-ts-mode-fontifies-call-target ()
  (should (eq (cflat-tests--face-at "square(i)" cflat-tests--program)
              'font-lock-function-call-face)))

(ert-deftest cflat-ts-mode-fontifies-field-access ()
  (should (eq (cflat-tests--face-at "x;" "fn f() -> int { i = p.x; }\n")
              'font-lock-property-use-face)))

(ert-deftest cflat-ts-mode-fontifies-struct-fields-as-properties ()
  (should (eq (cflat-tests--face-at "x: int" cflat-tests--program)
              'font-lock-property-name-face)))

(ert-deftest cflat-ts-mode-fontifies-brackets-and-delimiters ()
  (let ((source "fn f() -> int { return 0; }\n"))
    (should (eq (cflat-tests--face-at "{" source) 'font-lock-bracket-face))
    (should (eq (cflat-tests--face-at ";" source) 'font-lock-delimiter-face))
    (should (eq (cflat-tests--face-at "->" source) 'font-lock-delimiter-face))))

(ert-deftest cflat-ts-mode-declaration-colon-is-a-delimiter ()
  (should (eq (cflat-tests--face-at ":" "fn f(a: int) -> int {}\n")
              'font-lock-delimiter-face)))

(ert-deftest cflat-ts-mode-ternary-colon-is-an-operator ()
  (should (eq (cflat-tests--face-at ": c" "fn f() -> int { x = a ? b : c; }\n")
              'font-lock-operator-face)))

(ert-deftest cflat-ts-mode-warns-about-unterminated-block-comment ()
  (should (eq (cflat-tests--face-at "/* oops" "fn f() -> int {}\n/* oops")
              'font-lock-warning-face)))

(ert-deftest cflat-ts-mode-warns-about-unterminated-line-comment ()
  ;; No trailing newline, so by the C♭ spec this comment is not terminated
  (should (eq (cflat-tests--face-at "// oops" "fn f() -> int {}\n// oops")
              'font-lock-warning-face)))

(ert-deftest cflat-ts-mode-terminated-line-comment-is-not-a-warning ()
  (should (eq (cflat-tests--face-at "// fine" "fn f() -> int {}\n// fine\n")
              'font-lock-comment-face)))

(ert-deftest cflat-ts-mode-font-lock-level-1-is-comments-and-definitions ()
  (should (eq (cflat-tests--face-at "// A point" cflat-tests--program 1)
              'font-lock-comment-face))
  (should (eq (cflat-tests--face-at "square" cflat-tests--program 1)
              'font-lock-function-name-face))
  (should-not (cflat-tests--face-at "struct" cflat-tests--program 1)))

(ert-deftest cflat-ts-mode-font-lock-level-2-adds-keywords-and-types ()
  (should (eq (cflat-tests--face-at "struct" cflat-tests--program 2)
              'font-lock-keyword-face))
  ;; Not `cflat-tests--program', whose opening comment contains "point"
  (should (eq (cflat-tests--face-at "int" "fn f(a: int) -> int {}\n" 2)
              'font-lock-type-face))
  (should-not (cflat-tests--face-at "1" "fn f() -> int { return 1; }\n" 2)))

(ert-deftest cflat-ts-mode-font-lock-level-3-adds-numbers-and-calls ()
  (should (eq (cflat-tests--face-at "1" "fn f() -> int { return 1; }\n" 3)
              'font-lock-number-face))
  (should (eq (cflat-tests--face-at "square(i)" cflat-tests--program 3)
              'font-lock-function-call-face))
  (should-not (cflat-tests--face-at "+" "fn f() -> int { x = a + b; }\n" 3)))

(ert-deftest cflat-ts-mode-font-lock-level-4-adds-operators-and-brackets ()
  (should (eq (cflat-tests--face-at "+" "fn f() -> int { x = a + b; }\n" 4)
              'font-lock-operator-face))
  (should (eq (cflat-tests--face-at "{" "fn f() -> int { x = 1; }\n" 4)
              'font-lock-bracket-face)))

(ert-deftest cflat-ts-mode-imenu-lists-definitions ()
  (cflat-tests--with-buffer cflat-tests--program
    (let* ((index (treesit-simple-imenu))
           (names (lambda (group)
                    (mapcar #'car (cdr (assoc group index))))))
      (should (equal (funcall names "Function") '("square" "main")))
      (should (equal (funcall names "Struct") '("Point")))
      (should (equal (funcall names "Extern") '("print_num"))))))

(ert-deftest cflat-ts-mode-defun-name ()
  (cflat-tests--with-buffer cflat-tests--program
    (search-forward "fn square")
    (should (equal (treesit-defun-name (treesit-defun-at-point)) "square"))))

(ert-deftest cflat-ts-mode-which-function-reports-the-enclosing-defun ()
  (cflat-tests--with-buffer cflat-tests--program
    (search-forward "i = square(i)")
    (should (equal (add-log-current-defun) "main"))))

(ert-deftest cflat-ts-mode-defun-navigation ()
  (cflat-tests--with-buffer cflat-tests--program
    (goto-char (point-max))
    (dolist (heading '("fn main" "fn square" "extern print_num" "struct Point"))
      (beginning-of-defun)
      (should (looking-at-p heading)))))

(ert-deftest cflat-ts-mode-end-of-defun ()
  (cflat-tests--with-buffer cflat-tests--program
    (search-forward "fn square")
    (beginning-of-defun)
    (end-of-defun)
    (should (equal (buffer-substring-no-properties
                    (line-beginning-position 0) (line-end-position 0))
                   "}"))))

(ert-deftest cflat-ts-mode-mark-defun-selects-a-whole-function ()
  (cflat-tests--with-buffer cflat-tests--program
    (search-forward "return x * x")
    (mark-defun)
    (let ((region (buffer-substring-no-properties (region-beginning) (region-end))))
      (should (string-match-p "fn square" region))
      (should-not (string-match-p "fn main" region)))))

(ert-deftest cflat-ts-mode-list-navigation-crosses-balanced-brackets ()
  (cflat-tests--with-buffer "fn f(a: int, b: int) -> int {}\n"
    (search-forward "(")
    (backward-char)
    (forward-sexp)
    (should (equal (char-before) ?\)))))

(ert-deftest cflat-ts-mode-outline-headings-are-the-definitions ()
  "`outline-minor-mode' sees every top-level definition and nothing else."
  (cflat-tests--with-buffer cflat-tests--program
    (outline-minor-mode 1)
    (let (headings)
      (while (outline-next-heading)
        (push (buffer-substring-no-properties (pos-bol) (pos-eol)) headings))
      (should (equal (nreverse headings)
                     '("struct Point {"
                       "extern print_num : (int) -> int;"
                       "fn square(x: int) -> int {"
                       "fn main() -> int {"))))))

(ert-deftest cflat-ts-mode-hideshow-folds-the-enclosing-body ()
  (cflat-tests--with-buffer cflat-tests--program
    (hs-minor-mode 1)
    (let ((body (search-forward "return x * x")))
      (hs-hide-block)
      (should (invisible-p body))
      (should-not (invisible-p (search-forward "fn main"))))))

(ert-deftest cflat-ts-mode-handles-empty-buffer ()
  (cflat-tests--with-buffer ""
    (should (eq major-mode 'cflat-ts-mode))
    (indent-region (point-min) (point-max))
    (should (equal (buffer-string) ""))))

(ert-deftest cflat-ts-mode-handles-broken-source ()
  "A buffer mid-edit must not signal from indentation or font-lock."
  (dolist (source '("fn" "fn f(" "fn f() -> {" "struct S { x: }" "}{" "let"))
    (cflat-tests--with-buffer source
      (setq-local treesit-font-lock-level 4)
      (treesit-font-lock-recompute-features)
      (font-lock-mode 1)
      (font-lock-ensure)
      (indent-region (point-min) (point-max)))))

(provide 'cflat-ts-mode-tests)

;;; cflat-ts-mode-tests.el ends here

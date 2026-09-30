;;; cflat-ts-mode.el --- Major mode for C♭, using tree-sitter -*- lexical-binding: t; -*-

;; SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Author: Yiyu Zhou <yzhou155@dons.usfca.edu>
;; Maintainer: Yiyu Zhou <yzhou155@dons.usfca.edu>
;; Version: 0.1.0
;; Package-Requires: ((emacs "31.1"))
;; Keywords: languages, tree-sitter, cflat
;; URL: https://github.com/yzhou216/tree-sitter-cflat

;; This file is not part of GNU Emacs.

;;; Commentary:

;; Major mode for C♭, the toy language from the USF CS 414 compilers
;; course, built on Emacs's tree-sitter support.  It provides font-lock,
;; indentation, Imenu, `which-function-mode', `outline-minor-mode',
;; `hs-minor-mode', comment filling and structural navigation.
;;
;; Files ending in `.cb' open in this mode.  This file registers the
;; grammar's recipe in `treesit-language-source-alist', so a missing
;; grammar is built on demand as `treesit-auto-install-grammar' directs, or
;; by hand with M-x treesit-install-language-grammar RET cflat RET.
;;
;; C♭ treats an unterminated comment as a lexing error.  The grammar
;; reports one as an `unterminated_comment' node rather than failing the
;; parse, and this mode paints it with `font-lock-warning-face', so a stray
;; `/*' is visible at once instead of silently swallowing the rest of the
;; buffer.

;;; Code:

(require 'treesit)
(require 'c-ts-common)

(treesit-declare-unavailable-functions)

;; Generated parser lives on the release branch, not master
(add-to-list 'treesit-language-source-alist
             '(cflat "https://github.com/yzhou216/tree-sitter-cflat"
                     :revision "release")
             t)

(defgroup cflat nil
  "Major mode for the C♭ programming language."
  :group 'languages
  :prefix "cflat-ts-"
  :link '(url-link "https://github.com/yzhou216/tree-sitter-cflat"))

(defcustom cflat-ts-indent-offset 2
  "Number of spaces for each indentation step in `cflat-ts-mode'."
  :type 'natnum
  :safe #'natnump)

(defvar cflat-ts-mode--syntax-table
  (let ((table (make-syntax-table)))
    (modify-syntax-entry ?/  ". 124" table)
    (modify-syntax-entry ?*  ". 23b" table)
    (modify-syntax-entry ?\n ">"     table)
    ;; Symbol constituents in the standard syntax table
    (dolist (operator '(?+ ?- ?& ?= ?< ?>))
      (modify-syntax-entry operator "." table))
    table)
  "Syntax table for `cflat-ts-mode'.")

(defvar cflat-ts-mode--font-lock-settings
  (treesit-font-lock-rules
   :default-language 'cflat

   :feature 'comment
   '((comment) @font-lock-comment-face)

   :feature 'definition
   '((function_definition name: (identifier) @font-lock-function-name-face)
     (extern_declaration name: (identifier) @font-lock-function-name-face)
     (struct_declaration name: (identifier) @font-lock-type-face)
     (field_declaration_list
      (declaration name: (identifier) @font-lock-property-name-face))
     (parameter_list
      (declaration name: (identifier) @font-lock-variable-name-face))
     (let_declaration
      (declaration name: (identifier) @font-lock-variable-name-face)))

   :feature 'keyword
   '(["break" "continue" "else" "extern" "fn" "if" "let" "new" "return"
      "struct" "while"]
     @font-lock-keyword-face
     ;; Spelled as words, so they arrive at the keyword level
     ["and" "or" "not"] @font-lock-operator-face)

   :feature 'type
   '((primitive_type) @font-lock-type-face
     (struct_type name: (identifier) @font-lock-type-face))

   :feature 'constant
   '((nil) @font-lock-constant-face)

   :feature 'number
   '((number) @font-lock-number-face)

   :feature 'function
   '((call_expression function: (identifier) @font-lock-function-call-face)
     (call_expression
      function: (field_access field: (identifier) @font-lock-function-call-face)))

   :feature 'property
   '((field_access field: (identifier) @font-lock-property-use-face))

   :feature 'operator
   '(["+" "-" "*" "/" "==" "!=" "<" "<=" ">" ">=" "=" "&"] @font-lock-operator-face
     (ternary_expression ["?" ":"] @font-lock-operator-face))

   :feature 'bracket
   '(["(" ")" "[" "]" "{" "}"] @font-lock-bracket-face)

   :feature 'delimiter
   '([";" "," "." "->"] @font-lock-delimiter-face
     (declaration ":" @font-lock-delimiter-face)
     (extern_declaration ":" @font-lock-delimiter-face))

   :feature 'variable
   '((identifier) @font-lock-variable-use-face)

   :feature 'error
   '((unterminated_comment) @font-lock-warning-face
     (ERROR) @font-lock-warning-face))
  "Tree-sitter font-lock settings for `cflat-ts-mode'.")

(defvar cflat-ts-mode--indent-rules
  `((cflat
     ;; The root starts at the first token, so it is the first line's node
     ;; whenever the buffer opens with indentation.
     ((or (node-is "source_file") (parent-is "source_file")) column-0 0)
     ((node-is ,(rx bos (or "}" ")" "]") eos)) standalone-parent 0)
     ((node-is "else") parent-bol 0)
     ;; When no node starts on a line, the node covering it is passed as
     ;; the parent, which on a comment's later lines is the comment itself.
     ;; These lines keep their indentation rather than lose it to the
     ;; enclosing block's.
     ((parent-is ,(rx bos (or "comment" "unterminated_comment") eos)) no-indent 0)
     ((parent-is ,(rx bos (or "function_body" "block" "field_declaration_list"
                              "parameter_list" "argument_list"
                              "type_parameter_list" "array_allocation"
                              "let_declaration" "return_statement" "assignment"
                              "expression_statement" "binary_expression"
                              "ternary_expression")
                      eos))
      standalone-parent cflat-ts-indent-offset)
     (catch-all parent-bol 0)))
  "Tree-sitter indentation rules for `cflat-ts-mode'.")

(defun cflat-ts-mode--defun-name (node)
  "Return the name of the definition NODE, or nil if it has none."
  (treesit-node-text (treesit-node-child-by-field-name node "name") t))

;;;###autoload
(define-derived-mode cflat-ts-mode prog-mode "C♭"
  "Major mode for editing C♭, powered by tree-sitter."
  :group 'cflat
  :syntax-table cflat-ts-mode--syntax-table

  ;; `treesit-ready-p' also checks the buffer size
  (when (and (treesit-ensure-installed 'cflat)
             (treesit-ready-p 'cflat))
    (setq treesit-primary-parser (treesit-parser-create 'cflat))

    (c-ts-common-comment-setup)

    (setq-local treesit-font-lock-settings cflat-ts-mode--font-lock-settings
                treesit-font-lock-feature-list
                '((comment definition)
                  (keyword type)
                  (constant number function property)
                  (operator bracket delimiter variable error)))

    (setq-local treesit-simple-indent-rules cflat-ts-mode--indent-rules
                indent-tabs-mode nil
                electric-indent-chars (append "{}():;," electric-indent-chars))

    (setq-local treesit-defun-name-function #'cflat-ts-mode--defun-name
                treesit-simple-imenu-settings
                '(("Struct" "\\`struct_declaration\\'" nil nil)
                  ("Extern" "\\`extern_declaration\\'" nil nil)
                  ("Function" "\\`function_definition\\'" nil nil)))

    ;; No `comment' thing.  `treesit-forward-comment' assumes a line comment
    ;; stops short of its newline, but C♭'s includes it, so `forward-comment'
    ;; would also swallow the line after it whenever that line is blank.
    (setq-local treesit-thing-settings
                `((cflat
                   (defun ,(rx bos (or "function_definition"
                                       "struct_declaration"
                                       "extern_declaration")
                               eos))
                   (sexp ,(rx bos (or "declaration"
                                      "parameter_list"
                                      "argument_list"
                                      "type_parameter_list"
                                      "field_declaration_list"
                                      "function_body"
                                      "block"
                                      "identifier"
                                      "number"
                                      "primitive_type"
                                      "struct_type"
                                      "parenthesized_expression")
                              eos))
                   (list ,(rx bos (or "parameter_list"
                                      "argument_list"
                                      "type_parameter_list"
                                      "field_declaration_list"
                                      "function_body"
                                      "block")
                              eos))
                   (sentence ,(rx bos (or "expression_statement"
                                          "assignment"
                                          "let_declaration"
                                          "return_statement"
                                          "break_statement"
                                          "continue_statement"
                                          "if_statement"
                                          "while_statement")
                                  eos))
                   (text ,(rx bos (or "comment" "unterminated_comment") eos)))))

    (treesit-major-mode-setup)))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.cb\\'" . cflat-ts-mode))

(provide 'cflat-ts-mode)

;;; cflat-ts-mode.el ends here

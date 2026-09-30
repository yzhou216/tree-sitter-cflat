; SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
; SPDX-License-Identifier: GPL-3.0-or-later

; When several captures land on one node, tree-sitter's highlighter keeps the
; one appearing LAST, so this file runs from general to specific.  Moving the
; catch-all `(identifier)` to the end would repaint every type, function and
; property name as a plain variable.  The same holds within a pattern, hence
; `@spell @comment` rather than the reverse.  `test/highlight/` pins the order.

(identifier) @variable

(number) @number
(nil) @constant.builtin

(primitive_type) @type.builtin
(struct_type name: (identifier) @type)
(struct_declaration name: (identifier) @type)

(field_declaration_list
  (declaration name: (identifier) @property))
(parameter_list
  (declaration name: (identifier) @variable.parameter))
(let_declaration
  (declaration name: (identifier) @variable))

(field_access field: (identifier) @property)

(function_definition name: (identifier) @function)
(extern_declaration name: (identifier) @function)
(call_expression function: (identifier) @function.call)
(call_expression
  function: (field_access field: (identifier) @function.call))

[
  "struct"
  "extern"
  "fn"
  "let"
] @keyword

[
  "if"
  "else"
] @keyword.conditional

"while" @keyword.repeat

[
  "break"
  "continue"
  "return"
] @keyword.return

[
  "new"
  "and"
  "or"
  "not"
] @keyword.operator

[
  "+"
  "-"
  "*"
  "/"
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "="
  "&"
] @operator

(ternary_expression
  [
    "?"
    ":"
  ] @operator)

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ";"
  ","
  "."
  "->"
] @punctuation.delimiter

(declaration ":" @punctuation.delimiter)
(extern_declaration ":" @punctuation.delimiter)

(comment) @spell @comment

(unterminated_comment) @comment.error @error

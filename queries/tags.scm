; SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
; SPDX-License-Identifier: GPL-3.0-or-later

(function_definition
  name: (identifier) @name) @definition.function

(extern_declaration
  name: (identifier) @name) @definition.function

(struct_declaration
  name: (identifier) @name) @definition.class

(field_declaration_list
  (declaration name: (identifier) @name) @definition.field)

(call_expression
  function: (identifier) @name) @reference.call

(call_expression
  function: (field_access field: (identifier) @name)) @reference.call

(struct_type
  name: (identifier) @name) @reference.type

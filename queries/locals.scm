; SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
; SPDX-License-Identifier: GPL-3.0-or-later

; `let` declarations are function-wide, so blocks open no scope of their own
(function_definition) @local.scope

; Field names live in a namespace of their own, one per struct
(struct_declaration) @local.scope

(function_definition name: (identifier) @local.definition.function)
(extern_declaration name: (identifier) @local.definition.function)
(struct_declaration name: (identifier) @local.definition.type)

(parameter_list
  (declaration name: (identifier) @local.definition.parameter))

(let_declaration
  (declaration name: (identifier) @local.definition.var))

(field_declaration_list
  (declaration name: (identifier) @local.definition.field))

(identifier) @local.reference

; Scopes
(source_file) @local.scope
(function_definition) @local.scope
(class_definition) @local.scope
(code_segment) @local.scope
(for_statement) @local.scope
(while_statement) @local.scope
(do_while_statement) @local.scope
(do_until_statement) @local.scope
(until_statement) @local.scope
(foreach_statement) @local.scope
(loop_statement) @local.scope
(if_statement) @local.scope

; Definitions
(function_definition
  name: (identifier) @local.definition.function)

(class_definition
  name: (id_list
    (identifier) @local.definition.type))

(var_decl
  name: (identifier) @local.definition.var)

(parameter
  (var_decl
    name: (identifier) @local.definition.parameter))

; References
(identifier) @local.reference

(struct_definition
  struct_name: (identifier) @local.definition)

(struct_definition
    (typed_identifier) @local.definition)

(typed_assignment_statement
  left: (typed_identifier) @local.definition)

(typed_assignment_statement
  right: (argument_list (identifier) @local.reference))

(pfield) @local.reference

(type_identifier_legacy) @local.reference

(global_typed_identifier) @local.definition

(opcode_statement
  (typed_identifier) @local.definition)

(opcode_statement
  (argument_list
    (_) @local.definition))

(assignment_statement
  left: (identifier) @local.definition)

(assignment_statement
  left: (typed_identifier) @local.definition)

(label_statement) @local.definition

(macro_usage) @local.definition

(macro_define
  (macro_name) @local.definition)

(argument_list
  (identifier) @local.reference)

(score_statement_group) @local.definition
(score_statement_i) @local.definition
(score_statement_f) @local.definition

(score_nestable_loop
  count: (number) @local.definition)

(identifier) @local.reference
(typed_identifier) @local.reference
(opcode_name) @local.reference
(struct_definition
  struct_name: (identifier) @local.reference)

(instrument_definition
  name: (_) @local.definition)

(modern_udo_inputs
  (identifier) @local.definition)

(modern_udo_inputs
  (typed_identifier) @local.definition)

(legacy_typed_assignment_statement
  left: (_) @local.definition)

(array_access) @local.reference

(opcode_statement
  (type_identifier_legacy) @local.definition)

(header_assignment
  (header_identifier) @local.definition)

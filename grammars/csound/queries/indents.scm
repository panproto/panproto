(instrument_definition) @indent
(instrument_definition "endin" @outdent)

(udo_definition_legacy) @indent
(udo_definition_legacy "endop" @outdent)

(udo_definition_modern) @indent
(udo_definition_modern "endop" @outdent)

(internal_raw_block
    (kw_close_raw_string) @outdent)

(if_statement) @indent
(if_statement
    (then_block) @indent)

[
    (kw_endif)
    (kw_fi)
    (else_block)
    (elseif_block)
] @outdent

(else_block) @indent
(elseif_block) @indent

(switch_statement) @indent
(switch_statement
  [(case_block) (default_block)] @indent)
(case_block
  case_body: (opcode_statement) @indent)
(case_block
  case_body: (assignment_statement) @indent)
(default_block
  default_body: (opcode_statement) @indent)
(default_block
  default_body: (assignment_statement) @indent)
(kw_switch_end) @outdent

(while_loop) @indent
(while_loop (kw_od) @outdent)

(until_loop) @indent
(until_loop (kw_od) @outdent)

(for_loop) @indent
(for_loop (kw_od) @outdent)

(score_nestable_loop) @indent
(score_nestable_loop "}" @outdent)
(kw_open_raw_string) @indent
(kw_close_raw_string) @outdent

(parenthesized_expression) @indent

(function_call) @indent

(modern_udo_inputs) @indent

(modern_udo_outputs) @indent

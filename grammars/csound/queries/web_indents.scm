(instrument_definition) @indent.begin
(instrument_definition "endin" @indent.end)

[
    (udo_definition_legacy)
    (udo_definition_modern)
] @indent.begin

[
    "endop"
    (kw_close_raw_string)
] @indent.end

(if_statement) @indent.begin

[
    (else_block)
    (elseif_block)
] @indent.branch

[
    (kw_endif)
    (kw_fi)
] @indent.end

[
    (while_loop)
    (until_loop)
    (for_loop)
] @indent.begin

(kw_od) @indent.end

(switch_statement) @indent.begin
(kw_switch_end) @indent.end

[
    (case_block)
    (default_block)
] @indent.branch

(case_block case_body: (opcode_statement) @indent.begin)
(default_block default_body: (opcode_statement) @indent.begin)

(score_nestable_loop) @indent.begin
(score_nestable_loop "}" @indent.end)

(kw_open_raw_string) @indent.begin

(parenthesized_expression) @indent.begin

(function_call) @indent.begin

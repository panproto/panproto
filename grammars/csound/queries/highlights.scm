(global_typed_identifier name: (identifier) @variable)

(flag_identifier) @tag
(flag_content flag_type: (_) @constant.builtin)
(flag_content flag_value: (_) @variable)

(label_statement) @label

(udo_definition_legacy name: (opcode_name) @entity.name.function)
(udo_definition_modern name: (opcode_name) @entity.name.function)

(udo_definition_legacy inputs: (_) @type)
(udo_definition_legacy outputs: (_) @type)
(udo_definition_modern outputs: (_) @type)
(udo_definition_legacy "opcode" @keyword)
(udo_definition_modern "opcode" @keyword)
(udo_definition_legacy "endop" @keyword)
(udo_definition_modern "endop" @keyword)

(instrument_definition "instr" @keyword.emphasis.strong)
(instrument_definition name: (_) @entity.name.function)

(instrument_definition "endin" @keyword.emphasis.strong)

(comment) @comment
(block_comment) @comment
(line_continuation) @comment
(line_continuation_comment) @comment
(string) @string
(number) @number

[
    (header_identifier)
    (pfield)
] @constant.builtin.emphasis

(included_file) @string

[
    (kw_include)
    (kw_includestr)
    (kw_define)
    (kw_ifdef)
    (kw_undef)
    (kw_end)
    (kw_ifndef)
    (kw_elsedef)
] @keyword.emphasis.strong

[
    (kw_if )
    (kw_then)
    (kw_ithen)
    (kw_kthen)
    (kw_elseif)
    (kw_else)
    (kw_endif)
    (kw_fi)
    (kw_while)
    (kw_until)
    (kw_do)
    (kw_od)
    (kw_for)
    (kw_in)
    (kw_switch_start)
    (kw_case_key)
    (kw_default_key)
    (kw_xin)
    (kw_xout)
    (kw_void)
    (kw_switch_end)
    (global_keyword)
    (score_statement_i)
    (score_statement_f)
    (score_statement_group)
    (score_statement_safe)
    (instr_p1)
    (func_p1)
    (group_p1)
    (group_p1_safe)
    (swmacro_p1)
    (score_carry)
    (score_plus)
    (score_z_operator)
    (score_exclamation)
    (score_np_operator)
    (score_pp_operator)
    (boolean_var)
    (break_statement)
    (continue_statement)
] @keyword

(identifier) @variable

(struct_definition
  struct_name: (identifier) @type)

(struct_definition struct_field: (typed_identifier
    name: (identifier) @variable))

(struct_definition
    "struct" @keyword)

(struct_access
    called_struct: (_) @variable
    struct_member: (identifier) @property)

[
    (tag_synthesizer_start)
    (tag_synthesizer_end)
    (tag_instruments_start)
    (tag_instruments_end)
    (tag_options_start)
    (tag_options_end)
    (tag_score_start)
    (tag_score_start_python)
    (tag_score_start_csbeats)
    (tag_score_end)
    (tag_cabbage_start)
    (tag_cabbage_end)
    (tag_cabbageara_start)
    (tag_cabbageara_end)
    (tag_csfileb_start)
    (tag_csfileb_end)
    (tag_csmidifileb)
    (tag_cssamplefileb)
    (tag_csfile_start)
    (tag_csfile_end)
    (tag_licence_start)
    (tag_licence_end)
    (tag_short_licence_start)
    (tag_short_licence_end)
    (tag_html_start)
    (tag_html_end)
] @tag.emphasis

[
    "="
    "+"
    "-"
    "*"
    "/"
    "%"
    "^"
    "&"
    "|"
    "<"
    ">"
    "@"
    "!"
    "#"
    "?"
    ":"
    "+="
    "-="
    "*="
    "/="
    "<<"
    ">>"
    ">="
    "<="
    "=="
    "!="
    "&&"
    "||"
    "~"
    (score_random_operator)
    (score_ramping)
    (score_plus_p_operator)
    (score_minus_p_operator)
    (mod_equal)
] @operator

[","] @punctuation.delimiter

[
    "("
    ")"
    "["
    "]"
    "{"
    "}"
    (kw_open_raw_string)
    (kw_close_raw_string)
] @punctuation.bracket

[
  (return_statement)
  (kw_reinit)
  (kw_rigoto)
  (kw_kgoto)
  (kw_igoto)
  (kw_goto)
] @function

(macro_usage
    macro_symbol: "$" @macro.emphasis.strong
    id: (identifier) @macro.emphasis.strong)

(macro_name
    id: (_) @macro.emphasis.strong)

(type_identifier) @type

(typed_identifier
  name: (identifier) @variable)

(typed_identifier
  type: (identifier) @type)

((opcode_name) @function
 (#not-has-child? @function "typed_opcode_name"))

(opcode_name
    (typed_opcode_name
        name: (_) @function
        type: (_) @type))

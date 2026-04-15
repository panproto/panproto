;; ============================================================================
;; COMMENTS
;; ============================================================================
((comment) @comment.line
 (#match? @comment.line "^//"))

((comment) @comment.block
 (#match? @comment.block "^/\\*"))

((comment) @comment.block
 (#match? @comment.block "^\\(\\*"))

(comment) @comment

;; ============================================================================
;; IDENTIFIERS, VARIABLES AND CONSTANTS
;; ============================================================================
;; (identifier) @identifier
(identifier) @variable
(system_variable) @variable.builtin
(compiler_variable) @variable.builtin

;; Uppercase identifiers are probably constants
((identifier) @constant
 (#match? @constant "^[A-Z][A-Z\\d_]*$"))

;; Constants
(system_constant) @constant.builtin

(field_identifier) @property

;; ============================================================================
;; KEYWORDS
;; ============================================================================
[
    (program_name_keyword)
    (module_name_keyword)
    (define_device_keyword)
    (define_constant_keyword)
    (define_type_keyword)
    (define_variable_keyword)
    (define_system_variable_keyword)
    (define_start_keyword)
    (define_event_keyword)
    (define_mutually_exclusive_keyword)
    (define_function_keyword)
    (define_library_function_keyword)
    (define_combine_keyword)
    (define_connect_level_keyword)
    (define_latching_keyword)
    (define_toggling_keyword)
    (define_program_keyword)
    (define_call_keyword)
    (define_module_keyword)

    (if_keyword)
    (else_keyword)
    (switch_keyword)
    (case_keyword)
    (default_keyword)
    (while_keyword)
    (for_keyword)
    (break_keyword)
    (continue_keyword)
    (return_keyword)
    (select_keyword)
    (active_keyword)

    (push_keyword)
    (release_keyword)
    (hold_keyword)
    (repeat_keyword)

    (on_keyword)
    (off_keyword)

    (online_keyword)
    (offline_keyword)
    (onerror_keyword)
    (string_keyword)
    (command_keyword)
    (awake_keyword)
    (standby_keyword)

    (send_level_keyword)
    (send_string_keyword)
    (send_command_keyword)
    (clear_buffer_keyword)
    (create_buffer_keyword)
    (create_multi_buffer_keyword)
    (call_keyword)
    (system_call_keyword)

    (devchan_on_keyword)
    (devchan_off_keyword)
    (devchan_to_keyword)
    (devchan_min_to_keyword)
    (devchan_total_off_keyword)
    (devchan_pulse_keyword)

    (wait_keyword)
    (wait_until_keyword)
    (cancel_wait_keyword)
    (cancel_wait_until_keyword)
    (cancel_all_wait_keyword)
    (cancel_all_wait_until_keyword)

    (button_event_keyword)
    (channel_event_keyword)
    (data_event_keyword)
    (level_event_keyword)
    (timeline_event_keyword)
    (custom_event_keyword)

    (struct_keyword)
    (structure_keyword)

    (band)
    (bor)
    (bxor)
    (bnot)
    (lshift)
    (rshift)

    (and)
    (or)
    (not)
    (xor)
] @keyword

;; ============================================================================
;; TYPES, STORAGE CLASSES AND QUALIFIERS
;; ============================================================================
[
    (char_keyword)
    (widechar_keyword)
    (integer_keyword)
    (sinteger_keyword)
    (long_keyword)
    (slong_keyword)
    (float_keyword)
    (double_keyword)
    (dev_keyword)
    (devlev_keyword)
    (devchan_keyword)
    (variant_keyword)
    (variantarray_keyword)
] @type

(system_type) @type.builtin

(local_var_keyword) @type.storage
(stack_var_keyword) @type.storage

(type_qualifier) @type.qualifier
(constant_keyword) @type.qualifier
(volatile_keyword) @type.qualifier
(non_volatile_keyword) @type.qualifier
(persistent_keyword) @type.qualifier

;; Type definitions in struct declarations
(struct_specifier
  name: (type_identifier) @type.custom)

;; Field types in struct declarations
(field_declaration
  type: (type_specifier
    (type_identifier) @type.custom))

;; Custom types in local variable declarations with storage specifiers
(declaration
  (storage_class_specifier)
  type: (type_identifier) @type.custom)

;; Structure field types that are custom types
(field_declaration
  type: (type_identifier) @type.custom)

;; Custom types in local variable declarations - direct pattern
(compound_statement
  (declaration
    (storage_class_specifier)
    .
    (_) @type
    .
    (identifier)))

;; Custom types in variable declarations with type qualifiers
(declaration
  (type_qualifier)
  .
  (type_identifier) @type.custom
  .
  (identifier))

;; ============================================================================
;; EVENT REFERENCES AND DEVICE EXPRESSIONS
;; ============================================================================
;; Brackets in references
(button_event_devchan_reference
  "[" @punctuation.bracket
  "]" @punctuation.bracket)

(data_event_device_reference
  "[" @punctuation.bracket
  "]" @punctuation.bracket)

(level_event_devlev_reference
  "[" @punctuation.bracket
  "]" @punctuation.bracket)

(channel_event_devchan_reference
  "[" @punctuation.bracket
  "]" @punctuation.bracket)

(timeline_event_id_reference
  "[" @punctuation.bracket
  "]" @punctuation.bracket)

(custom_event_reference
  "[" @punctuation.bracket
  "]" @punctuation.bracket)

;; Device references and expressions
(devchan_expression
  "[" @punctuation.bracket
  "]" @punctuation.bracket)

(devlev_expression
  "[" @punctuation.bracket
  "]" @punctuation.bracket)

;; ============================================================================
;; OPERATORS AND PUNCTUATION
;; ============================================================================
;; Operators
[
    "="
    "+"
    "-"
    "*"
    "/"
    "%"
    ">"
    "<"
    "&"
    "|"
    "^"
    "!"
    "~"
    "&&"
    "||"
    "=="
    "!="
    "<="
    ">="
    "<<"
    ">>"
    "++"
    "--"
    "<>"
    (range_operator)
] @operator

;; Punctuation
[
    "("
    ")"
    "{"
    "}"
    "["
    "]"
] @punctuation.bracket

[
    "."
    ";"
    ","
    ":"
] @punctuation.delimiter

;; Parameter lists
(parameter_list) @punctuation.bracket

;; ============================================================================
;; FUNCTIONS AND PARAMETERS
;; ============================================================================
;; Functions
(call_expression
  function: (system_function) @function.builtin)
(call_expression
  function: (identifier) @function)
(function_definition
  name: (identifier) @function)

;; Library function declarations
(function_declaration
  name: (identifier) @function)

;; Parameters
(parameter_declaration
  declarator: (identifier) @parameter)

;; Parameters in declaration
;; (parameter_declaration
;;   declarator: (identifier) @parameter)

;; Parameters in function definition
(function_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (identifier) @parameter)))

;; Parameters in call definition
(call_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (identifier) @parameter)))

;; Parameters in function declarator
(function_declarator
  parameters: (parameter_list
    (parameter_declaration
      declarator: (identifier) @parameter)))

;; Function calls with arguments
(call_expression
  arguments: (argument_list
    (identifier) @variable.parameter))
(call_expression
  arguments: (argument_list
    (system_constant) @constant.builtin.parameter))

;; Array parameters in function definition
(function_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (identifier) @parameter))))

;; Array parameters in function declaration
(function_declaration
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (identifier) @parameter))))

;; Parameter types
(parameter_declaration
  (type_specifier) @type)

;; Parameter references within function bodies
((identifier) @parameter
 (#is? @parameter local.reference)
 (#eq? @parameter local.definition))

;; Parameter declaration direct highlight
(parameter_list
  (parameter_declaration
    declarator: (identifier) @parameter))

;; 2D array parameters in function declarations
(function_declaration
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (array_declarator
          declarator: (identifier) @parameter)))))

;; 3D array parameters in function declarations
(function_declaration
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (array_declarator
          declarator: (array_declarator
            declarator: (identifier) @parameter))))))

;; 4D array parameters in function declarations
(function_declaration
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (array_declarator
          declarator: (array_declarator
            declarator: (array_declarator
              declarator: (identifier) @parameter)))))))

;; 2D array parameters in function definitions
(function_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (array_declarator
          declarator: (identifier) @parameter)))))

;; 3D array parameters in function definitions
(function_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (array_declarator
          declarator: (array_declarator
            declarator: (identifier) @parameter))))))

;; 4D array parameters in function definitions
(function_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (array_declarator
          declarator: (array_declarator
            declarator: (array_declarator
              declarator: (identifier) @parameter)))))))

;; ============================================================================
;; LITERALS
;; ============================================================================
(string_literal) @string
;; (escape_sequence) @string.escape
;; (escape_sequence) @constant.character.escape
;; (string_content) @string.content
(number_literal) @number
(device_literal) @number
"\"" @string

;; ============================================================================
;; PREPROCESSOR
;; ============================================================================
(preproc_include_keyword) @preprocessor
(preproc_define_keyword) @preprocessor
(preproc_warn_keyword) @preprocessor
(preproc_disable_warning_keyword) @preprocessor
(preproc_if_defined_keyword) @preprocessor
(preproc_if_not_defined_keyword) @preprocessor
(preproc_else_keyword) @preprocessor
(preproc_end_if_keyword) @preprocessor

;; Preproc Arguments
;; Highlight all preproc_args initially as constants
(preproc_arg) @constant

;; Then specifically override numeric values
((preproc_arg) @number
 (#match? @number "^[ \t]*[0-9]+[ \t]*$"))

;; And string values with quotes
((preproc_arg) @string
 (#match? @string "'"))

;; ============================================================================
;; ERROR HANDLING
;; ============================================================================
(MISSING) @missing
(ERROR) @error

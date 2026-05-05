((html_block
    [(raw_script) (generic_closing_tag)] @injection.content)
    (#set! injection.language "html"))

((python_block
    [(raw_script) (generic_closing_tag) (strong_string)] @injection.content)
    (#set! injection.language "python"))

; ((cabbage_block
;     [(raw_script) (generic_closing_tag) (string)] @injection.content)
;     (#set! injection.language "json"))

((cabbage_block
    (cabbage_json_block) @injection.content)
    (#set! injection.language "json"))

((opcode_statement
    (opcode_name) @_name
    (argument_list
        (internal_raw_block
            (raw_text) @injection.content)))
    (#match? @_name "^pyruni$")
    (#set! injection.language "python"))

((opcode_statement
    (opcode_name) @_name
    (argument_list
        (parenthesized_expression
            (internal_raw_block
                (raw_text) @injection.content))))
    (#match? @_name "^pyruni$")
    (#set! injection.language "python"))

((opcode_statement
    (opcode_name) @_name
    (argument_list
        (internal_raw_block
            (raw_text) @injection.content)))
    (#match? @_name "^system$")
    (#set! injection.language "bash"))

((function_call
    (opcode_name) @_name
    (argument_list
        (internal_raw_block
            (raw_text) @injection.content)))
    (#match? @_name "^system")
    (#set! injection.language "bash"))

((opcode_statement
    (opcode_name) @_name
    (argument_list
        (internal_raw_block
            (raw_text) @injection.content)))
    (#not-match? @_name "^(pyruni|system)")
    (#set! injection.language "csound"))

((function_call
    (opcode_name) @_name
    (argument_list
        (internal_raw_block
            (raw_text) @injection.content)))
    (#not-match? @_name "^(pyruni|system)")
    (#set! injection.language "csound"))

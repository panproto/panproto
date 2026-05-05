; Function bodies
(function_definition
  (code_segment) @fold)

; Class bodies
(class_definition
  (class_body) @fold)

; Control flow bodies
(if_statement
  consequence: (code_segment) @fold)

(if_statement
  alternative: (code_segment) @fold)

(while_statement
  body: (code_segment) @fold)

(do_while_statement
  body: (code_segment) @fold)

(do_until_statement
  body: (code_segment) @fold)

(until_statement
  body: (code_segment) @fold)

(foreach_statement
  body: (code_segment) @fold)

(for_statement
  body: (code_segment) @fold)

(loop_statement
  body: (code_segment) @fold)

; Multi-line comments
(comment) @fold

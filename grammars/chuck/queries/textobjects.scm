; Functions
(function_definition) @function.outer

(function_definition
  (code_segment) @function.inner)

; Classes
(class_definition) @class.outer

(class_definition
  (class_body) @class.inner)

; Parameters
(parameter) @parameter.outer

(parameter
  (var_decl
    name: (identifier) @parameter.inner))

; Loops
(while_statement) @loop.outer
(while_statement
  body: (code_segment) @loop.inner)

(for_statement) @loop.outer
(for_statement
  body: (code_segment) @loop.inner)

(loop_statement) @loop.outer
(loop_statement
  body: (code_segment) @loop.inner)

(until_statement) @loop.outer
(until_statement
  body: (code_segment) @loop.inner)

(foreach_statement) @loop.outer
(foreach_statement
  body: (code_segment) @loop.inner)

; Conditionals
(if_statement) @conditional.outer
(if_statement
  consequence: (code_segment) @conditional.inner)

; Comments
(comment) @comment.outer

; Calls
(postfix_expression
  function: (postfix_expression)
  (argument_list)) @call.outer

(postfix_expression
  function: (postfix_expression)
  (argument_list) @call.inner)

; Blocks
(code_segment) @block.outer

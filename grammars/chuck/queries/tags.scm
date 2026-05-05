; Functions
(function_definition
  name: (identifier) @name) @definition.function

; Classes
(class_definition
  name: (id_list
    (identifier) @name)) @definition.class

; Methods (functions inside classes)
(class_body
  (function_definition
    name: (identifier) @name) @definition.method)

; Variables/fields
(var_decl
  name: (identifier) @name) @definition.var

; Function calls
(postfix_expression
  function: (postfix_expression
    (identifier) @name)) @reference.call

; Method calls
(postfix_expression
  object: (postfix_expression)
  member: (identifier) @name) @reference.call

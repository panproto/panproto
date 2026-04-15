;; Scopes
;; ======

;; Function scope (parameters defined here are visible in the body)
(function_definition
  body: (compound_statement) @local.scope)

(call_definition
  body: (compound_statement) @local.scope)

;; Block scopes (these create new variable scopes)
(compound_statement) @local.scope

;; Loops and conditionals
(for_statement) @local.scope
(while_statement) @local.scope
(if_statement) @local.scope
(switch_statement) @local.scope
(select_statement) @local.scope
(wait_statement) @local.scope
(wait_until_statement) @local.scope

;; Event blocks
(button_event_type) @local.scope
(data_event_type) @local.scope
(channel_event_type) @local.scope

;; Definitions
;; ===========

;; Function parameters
(function_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (identifier) @local.definition)))

;; Function parameters (array)
(function_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (identifier) @local.definition))))

(call_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (identifier) @local.definition)))

(call_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (identifier) @local.definition))))

;; Function definitions
(function_definition
  name: (identifier) @local.definition)

;; Local variables
(declaration
  (storage_class_specifier)
  declarator: (identifier) @local.definition)

;; Regular variable declarations
(declaration
  declarator: (identifier) @local.definition)

;; References
;; ==========

;; Any identifiers that are not part of declarations
(identifier) @local.reference

;; Exclude certain kinds of identifiers from being treated as references
;; For example, field names or type names
;; ((field_expression
;;   field: (field_identifier)) @_field
;;  (#set! "local.reference" ""))

;; ((type_specifier
;;   (identifier)) @_type
;;  (#set! "local.reference" ""))
 ;; Exclude field names
((field_expression
  field: (field_identifier)) @_field
 (#set! @_field "local.reference" false))

;; Exclude type names
((type_specifier
  (identifier)) @_type
 (#set! @_type "local.reference" false))

 ;; Scopes
(function_definition) @local.scope
(call_definition) @local.scope

;; Regular parameter definitions
(function_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (identifier) @local.definition)))

;; Array parameter definitions
(function_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (array_declarator
        declarator: (identifier) @local.definition))))

;; Call parameter definitions
(call_definition
  parameters: (parameter_list
    (parameter_declaration
      declarator: (identifier) @local.definition)))

;; References
(identifier) @local.reference

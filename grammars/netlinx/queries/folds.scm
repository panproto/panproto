;; ============================================================================
;; BASIC BLOCKS
;; ============================================================================
;; Compound statements/blocks
(compound_statement) @fold

;; ============================================================================
;; STRUCTURES
;; ============================================================================
;; Struct definitions
(struct_specifier
  body: (field_declaration_list) @fold)

;; ============================================================================
;; FUNCTIONS AND CALLS
;; ============================================================================
;; Function definitions
(function_definition
  body: (compound_statement) @fold)

;; Call definitions
(call_definition
  body: (compound_statement) @fold)

;; ============================================================================
;; EVENT HANDLERS
;; ============================================================================
;; Event handlers
(button_event_block) @fold
(channel_event_block) @fold
(data_event_block) @fold

;; Custom events and timeline events
(custom_event_definition
  body: (compound_statement) @fold)
(timeline_event_definition
  body: (compound_statement) @fold)

;; Level events
(level_event_definition
  body: (compound_statement) @fold)

;; ============================================================================
;; CONTROL FLOW
;; ============================================================================
;; Control flow statements
(if_statement
  consequence: (compound_statement) @fold
  alternative: (else_clause (compound_statement) @fold)?)
(if_statement
  alternative: (else_clause (if_statement)) @fold)
(switch_statement
  body: (compound_statement) @fold)
(select_statement) @fold
(while_statement
  body: (compound_statement) @fold)
(for_statement
  body: (compound_statement) @fold)

;; ============================================================================
;; MISCELLANEOUS
;; ============================================================================
;; Initializer lists
(initializer_list) @fold

;; Comments (only multi-line)
(comment) @fold

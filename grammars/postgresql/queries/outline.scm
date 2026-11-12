; outline.scm — code outline / buffer symbol structure for Zed and other
; consumers that use @item + @name outline queries.

; ── Tables ────────────────────────────────────────────────────────────────────

(CreateStmt
  (kw_create)
  (kw_table) @context
  (qualified_name
    (ColId
      (identifier) @name) .)) @item

(CreateStmt
  (kw_create)
  (kw_table) @context
  (qualified_name
    (indirection
      (indirection_el
        (attr_name
          (ColLabel
            (identifier) @name)))))) @item

(CreateAsStmt
  (kw_create)
  (kw_table) @context
  (create_as_target
    (qualified_name
      (ColId
        (identifier) @name) .))) @item

(CreateAsStmt
  (kw_create)
  (kw_table) @context
  (create_as_target
    (qualified_name
      (indirection
        (indirection_el
          (attr_name
            (ColLabel
              (identifier) @name))))))) @item

(CreateForeignTableStmt
  (kw_create)
  (kw_foreign) @context
  (kw_table) @context
  (qualified_name
    (ColId
      (identifier) @name) .)) @item

(CreateForeignTableStmt
  (kw_create)
  (kw_foreign) @context
  (kw_table) @context
  (qualified_name
    (indirection
      (indirection_el
        (attr_name
          (ColLabel
            (identifier) @name)))))) @item

; ── Indexes ───────────────────────────────────────────────────────────────────

(IndexStmt
  (kw_create)
  (kw_index) @context
  (opt_single_name
    (ColId
      (identifier) @name))) @item

(IndexStmt
  (kw_create)
  (kw_index) @context
  (name
    (ColId
      (identifier) @name))) @item

; ── Schemas ───────────────────────────────────────────────────────────────────

(CreateSchemaStmt
  (kw_create)
  (kw_schema) @context
  (ColId
    (identifier) @name)) @item

; ── Views / materialized views ────────────────────────────────────────────────

(ViewStmt
  (kw_create)
  (kw_view) @context
  (qualified_name
    (ColId
      (identifier) @name) .)) @item

(ViewStmt
  (kw_create)
  (kw_view) @context
  (qualified_name
    (indirection
      (indirection_el
        (attr_name
          (ColLabel
            (identifier) @name)))))) @item

(CreateMatViewStmt
  (kw_create)
  (kw_materialized) @context
  (kw_view) @context
  (create_mv_target
    (qualified_name
      (ColId
        (identifier) @name) .))) @item

(CreateMatViewStmt
  (kw_create)
  (kw_materialized) @context
  (kw_view) @context
  (create_mv_target
    (qualified_name
      (indirection
        (indirection_el
          (attr_name
            (ColLabel
              (identifier) @name))))))) @item

; ── Sequences ─────────────────────────────────────────────────────────────────

(CreateSeqStmt
  (kw_create)
  (kw_sequence) @context
  (qualified_name
    (ColId
      (identifier) @name) .)) @item

(CreateSeqStmt
  (kw_create)
  (kw_sequence) @context
  (qualified_name
    (indirection
      (indirection_el
        (attr_name
          (ColLabel
            (identifier) @name)))))) @item

; ── Types ─────────────────────────────────────────────────────────────────────

(DefineStmt
  (kw_create)
  (kw_type) @context
  (any_name
    (ColId
      (identifier) @name) .)) @item

(DefineStmt
  (kw_create)
  (kw_type) @context
  (any_name
    (attrs
      (attr_name
        (ColLabel
          (identifier) @name))))) @item

; Domains are user-defined types too.
(CreateDomainStmt
  (kw_create)
  (kw_domain) @context
  (any_name
    (ColId
      (identifier) @name) .)) @item

(CreateDomainStmt
  (kw_create)
  (kw_domain) @context
  (any_name
    (attrs
      (attr_name
        (ColLabel
          (identifier) @name))))) @item

; ── Functions / procedures ────────────────────────────────────────────────────

(CreateFunctionStmt
  (kw_create)
  [
    (kw_function) @context
    (kw_procedure) @context
  ]
  (func_name
    (type_function_name
      (identifier) @name) .)) @item

(CreateFunctionStmt
  (kw_create)
  [
    (kw_function) @context
    (kw_procedure) @context
  ]
  (func_name
    (indirection
      (indirection_el
        (attr_name
          (ColLabel
            (identifier) @name)))))) @item

; ── Triggers ──────────────────────────────────────────────────────────────────

(CreateTrigStmt
  (kw_create)
  (kw_trigger) @context
  (name
    (ColId
      (identifier) @name))) @item

(CreateEventTrigStmt
  (kw_create)
  (kw_event) @context
  (kw_trigger) @context
  (name
    (ColId
      (identifier) @name))) @item

; ── Row-level security policies ───────────────────────────────────────────────

(CreatePolicyStmt
  (kw_create)
  (kw_policy) @context
  (name
    (ColId
      (identifier) @name))) @item

; ── Extensions and procedural languages ───────────────────────────────────────

(CreateExtensionStmt
  (kw_create)
  (kw_extension) @context
  (name
    (ColId
      (identifier) @name))) @item

(CreatePLangStmt
  (kw_create)
  (kw_language) @context
  (name
    (ColId
      (identifier) @name))) @item

; ── Publications / subscriptions ──────────────────────────────────────────────

(CreatePublicationStmt
  (kw_create)
  (kw_publication) @context
  (name
    (ColId
      (identifier) @name))) @item

(CreateSubscriptionStmt
  (kw_create)
  (kw_subscription) @context
  (name
    (ColId
      (identifier) @name))) @item

; ── Other named database objects ──────────────────────────────────────────────

(CreateStatsStmt
  (kw_create)
  (kw_statistics) @context
  (opt_qualified_name
    (any_name
      (ColId
        (identifier) @name) .))) @item

(CreateStatsStmt
  (kw_create)
  (kw_statistics) @context
  (opt_qualified_name
    (any_name
      (attrs
        (attr_name
          (ColLabel
            (identifier) @name)))))) @item

(CreateStatsStmt
  (kw_create)
  (kw_statistics) @context
  (any_name
    (ColId
      (identifier) @name) .)) @item

(CreateStatsStmt
  (kw_create)
  (kw_statistics) @context
  (any_name
    (attrs
      (attr_name
        (ColLabel
          (identifier) @name))))) @item

(CreateTableSpaceStmt
  (kw_create)
  (kw_tablespace) @context
  (name
    (ColId
      (identifier) @name))) @item

(CreateFdwStmt
  (kw_create)
  (kw_foreign) @context
  (kw_data) @context
  (kw_wrapper) @context
  (name
    (ColId
      (identifier) @name))) @item

(CreateForeignServerStmt
  (kw_create)
  (kw_server) @context
  (name
    (ColId
      (identifier) @name))) @item

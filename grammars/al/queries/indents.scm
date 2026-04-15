; AL Language (v2) - Tree-sitter Indentation Queries
; Compatible with Neovim (nvim-treesitter) and Helix editor

; =============================================================================
; Indent Begin - nodes that increase indentation
; =============================================================================

; Object declarations (all use { ... })
[
  (table_declaration)
  (tableextension_declaration)
  (page_declaration)
  (pageextension_declaration)
  (pagecustomization_declaration)
  (codeunit_declaration)
  (report_declaration)
  (reportextension_declaration)
  (query_declaration)
  (xmlport_declaration)
  (enum_declaration)
  (enumextension_declaration)
  (interface_declaration)
  (controladdin_declaration)
  (dotnet_declaration)
  (profile_declaration)
  (profileextension_declaration)
  (permissionset_declaration)
  (permissionsetextension_declaration)
  (entitlement_declaration)
] @indent.begin

; Code blocks (begin ... end)
(code_block) @indent.begin

; Sections that use { ... }
[
  (fields_section)
  (keys_section)
  (fieldgroups_section)
  (layout_section)
  (actions_section)
  (dataset_section)
  (elements_section)
  (labels_section)
  (rendering_section)
  (requestpage_section)
  (views_section)
  (schema_section)
] @indent.begin

; Layout elements
[
  (area_section)
  (group_section)
  (repeater_section)
  (cuegroup_section)
  (fixed_section)
  (grid_section)
  (part_section)
  (systempart_section)
  (usercontrol_section)
] @indent.begin

; Action sections
[
  (action_area_section)
  (action_group_section)
  (action_declaration)
  (customaction_declaration)
  (systemaction_declaration)
  (fileuploadaction_declaration)
  (actionref_declaration)
] @indent.begin

; Report sections
[
  (report_dataitem)
  (report_column)
  (rendering_layout)
  (label_section)
] @indent.begin

; Query sections
[
  (query_dataitem)
  (query_column)
  (query_filter)
] @indent.begin

; XMLport elements
[
  (xmlport_element)
  (xmlport_attribute)
] @indent.begin

; Declarations with bodies
[
  (field_declaration)
  (enum_value_declaration)
  (key_declaration)
  (view_definition)
  (assembly_declaration)
] @indent.begin

; Triggers
(trigger_declaration) @indent.begin

; Var sections
(var_section) @indent.begin

; Control flow (bodies are indented)
[
  (if_statement)
  (case_statement)
  (case_branch)
  (case_else_branch)
  (for_statement)
  (foreach_statement)
  (while_statement)
  (repeat_statement)
  (with_statement)
] @indent.begin

; Modification blocks
[
  (addafter_modification)
  (addbefore_modification)
  (addfirst_modification)
  (addlast_modification)
  (modify_modification)
  (moveafter_modification)
  (movebefore_modification)
  (movefirst_modification)
  (movelast_modification)
  (addafter_action_modification)
  (addbefore_action_modification)
  (addfirst_action_modification)
  (addlast_action_modification)
  (modify_action_modification)
  (addafter_dataset_modification)
  (addbefore_dataset_modification)
  (addfirst_dataset_modification)
  (addlast_dataset_modification)
  (add_dataset_modification)
  (addfirst_fieldgroup_modification)
  (addlast_fieldgroup_modification)
  (addafter_views_modification)
  (addbefore_views_modification)
  (addfirst_views_modification)
  (addlast_views_modification)
] @indent.begin

; Preprocessor conditionals
[
  (preproc_conditional)
  (preproc_conditional_actions)
  (preproc_conditional_case)
  (preproc_conditional_controladdin)
  (preproc_conditional_dataset)
  (preproc_conditional_fieldgroups)
  (preproc_conditional_fields)
  (preproc_conditional_keys)
  (preproc_conditional_layout)
  (preproc_conditional_object)
  (preproc_conditional_query)
  (preproc_conditional_report)
  (preproc_conditional_statement)
  (preproc_conditional_var)
  (preproc_conditional_var_block)
  (preproc_conditional_xmlport)
] @indent.begin

; Argument and parameter lists
(argument_list) @indent.begin
(parameter_list) @indent.begin

; =============================================================================
; Indent End - tokens that signal end of indentation
; =============================================================================

"}" @indent.end
")" @indent.end
"]" @indent.end

; Code block begin/end tokens (at depth 0 — named nodes)
(code_block (end_keyword) @indent.end)

; =============================================================================
; Indent Branch - closing tokens that should dedent to match their opener
; =============================================================================

[
  "}"
  ")"
  "]"
] @indent.branch

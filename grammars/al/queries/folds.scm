; AL Language (v2) - Tree-sitter Code Folding Queries
; Defines regions that can be collapsed in editors supporting tree-sitter folding

; =============================================================================
; Object Declarations
; =============================================================================

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
] @fold

; =============================================================================
; Procedures and Triggers
; =============================================================================

(procedure) @fold
(trigger_declaration) @fold
(event_declaration) @fold
(interface_procedure) @fold
(preproc_split_procedure) @fold

; =============================================================================
; Code Blocks (begin ... end)
; =============================================================================

(code_block) @fold

; =============================================================================
; Sections
; =============================================================================

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
] @fold

; =============================================================================
; Layout Elements
; =============================================================================

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
] @fold

; =============================================================================
; Action Sections
; =============================================================================

[
  (action_area_section)
  (action_group_section)
  (action_declaration)
  (customaction_declaration)
  (systemaction_declaration)
  (fileuploadaction_declaration)
] @fold

; =============================================================================
; Report Elements
; =============================================================================

[
  (report_dataitem)
  (report_column)
  (rendering_layout)
  (label_section)
] @fold

; =============================================================================
; Query Elements
; =============================================================================

[
  (query_dataitem)
  (query_column)
  (query_filter)
] @fold

; =============================================================================
; XMLport Elements
; =============================================================================

[
  (xmlport_element)
  (xmlport_attribute)
] @fold

; =============================================================================
; Declarations with Bodies
; =============================================================================

[
  (field_declaration)
  (enum_value_declaration)
  (key_declaration)
  (view_definition)
  (assembly_declaration)
] @fold

; =============================================================================
; Var Sections
; =============================================================================

(var_section) @fold

; =============================================================================
; Control Flow
; =============================================================================

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
] @fold

; =============================================================================
; Modification Blocks
; =============================================================================

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
] @fold

; =============================================================================
; Comments
; =============================================================================

(multiline_comment) @fold

; =============================================================================
; Preprocessor Regions
; =============================================================================

(preproc_region) @fold

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
] @fold

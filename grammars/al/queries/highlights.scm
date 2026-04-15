; AL Language (v2) - Tree-sitter Syntax Highlighting Queries
; See: https://tree-sitter.github.io/tree-sitter/syntax-highlighting
;
; Keywords are exposed as named nodes (e.g., if_keyword, table_keyword) for
; precise highlighting. begin/end are named at depth 0 via external scanner.

; =============================================================================
; Comments
; =============================================================================

(comment) @comment.line
(multiline_comment) @comment.block

; =============================================================================
; Preprocessor Directives
; =============================================================================

[
  (preproc_if)
  (preproc_else)
  (preproc_elif)
  (preproc_endif)
  (pragma)
  (preproc_region)
  (preproc_endregion)
] @keyword.directive

; =============================================================================
; Keywords - Control Flow
; =============================================================================

[
  (if_keyword)
  (then_keyword)
  (else_keyword)
  (case_keyword)
  (of_keyword)
  (while_keyword)
  (do_keyword)
  (for_keyword)
  (foreach_keyword)
  (in_keyword)
  (repeat_keyword)
  (until_keyword)
  (exit_keyword)
  (break_keyword)
  (continue_keyword)
  (with_keyword)
  (asserterror_keyword)
  (to_keyword)
  (downto_keyword)
  (begin_keyword)
  (end_keyword)
] @keyword.control

; =============================================================================
; Keywords - Declarations
; =============================================================================

[
  (procedure_keyword)
  (trigger_keyword)
  (var_keyword)
  (event_keyword)
] @keyword.declaration

; =============================================================================
; Keywords - Object Types
; =============================================================================

[
  (table_keyword)
  (tableextension_keyword)
  (page_keyword)
  (pageextension_keyword)
  (codeunit_keyword)
  (report_keyword)
  (reportextension_keyword)
  (query_keyword)
  (xmlport_keyword)
  (enum_keyword)
  (enumextension_keyword)
  (interface_keyword)
  (controladdin_keyword)
  (dotnet_keyword)
  (profile_keyword)
  (profileextension_keyword)
  (permissionset_keyword)
  (permissionsetextension_keyword)
  (entitlement_keyword)
  (pagecustomization_keyword)
] @keyword.type

; =============================================================================
; Keywords - Structure
; =============================================================================

[
  (namespace_keyword)
  (using_keyword)
  (extends_keyword)
  (implements_keyword)
  (customizes_keyword)
] @keyword.import

; =============================================================================
; Keywords - Sections
; =============================================================================

[
  (fields_keyword)
  (keys_keyword)
  (key_keyword)
  (fieldgroups_keyword)
  (fieldgroup_keyword)
  (actions_keyword)
  (layout_keyword)
  (area_keyword)
  (group_keyword)
  (repeater_keyword)
  (cuegroup_keyword)
  (fixed_keyword)
  (grid_keyword)
  (part_keyword)
  (systempart_keyword)
  (usercontrol_keyword)
  (dataset_keyword)
  (elements_keyword)
  (dataitem_keyword)
  (column_keyword)
  (filter_keyword)
  (labels_keyword)
  (rendering_keyword)
  (requestpage_keyword)
  (schema_keyword)
  (views_keyword)
  (view_keyword)
] @keyword.structure

; =============================================================================
; Keywords - Modifiers
; =============================================================================

[
  (local_keyword)
  (internal_keyword)
  (protected_keyword)
  (temporary_keyword)
] @keyword.modifier

; Procedure modifier (access modifiers on procedures)
(procedure_modifier) @keyword.modifier

; Object type keyword (used in permission sets, database refs, etc.)
(object_type_keyword) @keyword

; =============================================================================
; Operators
; =============================================================================

; Assignment operator
":=" @operator

; Arithmetic operators
[
  "+"
  "-"
  "*"
  "/"
] @operator

; Comparison operators (named node wrapping =, <>, <, >, <=, >=)
(comparison_operator) @operator

; Range operator
".." @operator

; Member access
"." @punctuation.delimiter

; Enum/scope qualifier
"::" @operator

; Ternary conditional
"?" @operator

; Keyword operators (string literal alternatives, matchable)
[
  "and" "AND" "And"
  "or" "OR" "Or"
  "xor" "XOR" "Xor"
  "not" "NOT" "Not"
  "div" "DIV" "Div"
  "mod" "MOD" "Mod"
  "in" "IN" "In"
] @keyword.operator

; =============================================================================
; Punctuation
; =============================================================================

";" @punctuation.delimiter
":" @punctuation.delimiter
"," @punctuation.delimiter

"(" @punctuation.bracket
")" @punctuation.bracket
"[" @punctuation.bracket
"]" @punctuation.bracket
"{" @punctuation.bracket
"}" @punctuation.bracket

; =============================================================================
; Literals
; =============================================================================

(string_literal) @string
(integer) @number
(decimal) @number.float
(biginteger_literal) @number
(boolean) @constant.builtin
(date_literal) @string.special
(time_literal) @string.special
(datetime_literal) @string.special

; =============================================================================
; Types
; =============================================================================

; Built-in types (Integer, Text, Boolean, Date, etc.)
(basic_type) @type.builtin

; Parameterized built-in types
(text_type) @type.builtin
(code_type) @type.builtin

; Record type references
(record_type
  reference: [(identifier) (quoted_identifier)] @type)

; Object reference types (Codeunit, Page, Report, etc.)
(object_reference_type
  reference: [(identifier) (quoted_identifier) (integer)] @type)

; Array, List, Dictionary types
(array_type) @type
(list_type) @type
(dictionary_type) @type

; Option type
(option_type) @type

; Type specifications
(type_specification) @type

; DotNet type references
(dotnet_type
  reference: [(identifier) (quoted_identifier)] @type)

; =============================================================================
; Object Declarations
; =============================================================================

; Tables
(table_declaration
  object_id: (integer) @constant)
(table_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Pages
(page_declaration
  object_id: (integer) @constant)
(page_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Codeunits
(codeunit_declaration
  object_id: (integer) @constant)
(codeunit_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Reports
(report_declaration
  object_id: (integer) @constant)
(report_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Queries
(query_declaration
  object_id: (integer) @constant)
(query_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; XMLports
(xmlport_declaration
  object_id: (integer) @constant)
(xmlport_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Enums
(enum_declaration
  object_id: (integer) @constant)
(enum_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Interfaces
(interface_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; ControlAddins
(controladdin_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Profiles
(profile_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Permission Sets
(permissionset_declaration
  object_id: (integer) @constant)
(permissionset_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Entitlements
(entitlement_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Page Customizations
(pagecustomization_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; DotNet
(dotnet_declaration) @type

; =============================================================================
; Extension Declarations
; =============================================================================

(tableextension_declaration
  object_id: (integer) @constant)
(tableextension_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

(pageextension_declaration
  object_id: (integer) @constant)
(pageextension_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

(enumextension_declaration
  object_id: (integer) @constant)
(enumextension_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

(reportextension_declaration
  object_id: (integer) @constant)
(reportextension_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

(profileextension_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

(permissionsetextension_declaration
  object_id: (integer) @constant)
(permissionsetextension_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; Split declarations (preprocessor-split object headers)
(preproc_split_declaration
  object_id: (integer) @constant)
(preproc_split_declaration
  object_name: [(identifier) (quoted_identifier)] @type.definition)

; =============================================================================
; Procedures and Triggers
; =============================================================================

; Procedure names
(procedure
  name: [(identifier) (quoted_identifier)] @function.definition)

; Event declarations
(event_declaration
  name: [(identifier) (quoted_identifier)] @function.definition)

; Trigger names
(trigger_declaration
  name: [(identifier) (quoted_identifier)] @function.definition)

; Interface procedures
(interface_procedure
  name: [(identifier) (quoted_identifier)] @function.definition)

; Split procedures (preprocessor-split)
(preproc_split_procedure
  name: [(identifier) (quoted_identifier)] @function.definition)

; =============================================================================
; Fields and Variables
; =============================================================================

; Table field declarations
(field_declaration
  id: (integer) @constant)
(field_declaration
  name: [(identifier) (quoted_identifier)] @property.definition)

; Variable declarations
(variable_declaration
  name: [(identifier) (quoted_identifier)] @variable.definition)

; Parameters
(parameter
  name: [(identifier) (quoted_identifier)] @variable.parameter)

; Label declarations
(label_declaration
  name: (identifier) @constant.definition)

; Enum value declarations
(enum_value_declaration
  value_id: (integer) @constant)
(enum_value_declaration
  value_name: [(identifier) (quoted_identifier)] @constant.definition)

; Key declarations
(key_declaration
  name: [(identifier) (quoted_identifier)] @property.definition)

; Fieldgroup declarations
(fieldgroup_declaration
  name: [(identifier) (quoted_identifier)] @property.definition)

; Return values
(procedure
  return_value: [(identifier) (quoted_identifier)] @variable.definition)
(trigger_declaration
  return_value: [(identifier) (quoted_identifier)] @variable.definition)

; For loop variables
(for_statement
  variable: (identifier) @variable)

; Foreach loop variables
(foreach_statement
  variable: (identifier) @variable)

; =============================================================================
; Expressions
; =============================================================================

; Direct function calls
(call_expression
  function: (identifier) @function.call)

; Method calls on objects
(call_expression
  function: (member_expression
    member: (identifier) @function.method.call))

; Member expression components
(member_expression
  object: (identifier) @variable)
(member_expression
  member: (identifier) @property)

; Database references
(database_reference
  table_name: [(identifier) (quoted_identifier)] @type)

; Qualified enum values
(qualified_enum_value
  enum_type: [(identifier) (quoted_identifier)] @type)
(qualified_enum_value
  value: [(identifier) (quoted_identifier)] @constant)

; Option members
(option_member) @constant

; =============================================================================
; Actions
; =============================================================================

(action_declaration
  name: [(identifier) (quoted_identifier)] @function)

(actionref_declaration
  action_name: [(identifier) (quoted_identifier)] @function)

(customaction_declaration
  name: [(identifier) (quoted_identifier)] @function)

(systemaction_declaration
  name: [(identifier) (quoted_identifier)] @function)

(fileuploadaction_declaration
  name: [(identifier) (quoted_identifier)] @function)

(separator_action
  name: [(identifier) (quoted_identifier)] @punctuation.special)

; =============================================================================
; Namespace and Using
; =============================================================================

(namespace_declaration
  name: (namespace_name) @module)

(using_statement
  namespace: (namespace_name) @module)

; =============================================================================
; Implements Clause
; =============================================================================

(implements_clause
  interface: [(identifier) (quoted_identifier)] @type)

; =============================================================================
; Attributes
; =============================================================================

(attribute_item) @attribute

(attribute_content
  name: (identifier) @attribute)

; =============================================================================
; Properties
; =============================================================================

; Property name
(property
  name: (property_name) @property)

; Keyword identifiers (contextual keywords used as values)
(keyword_identifier) @keyword

; =============================================================================
; Query and Report Elements
; =============================================================================

; Query dataitems
(query_dataitem
  name: [(identifier) (quoted_identifier)] @variable)
(query_dataitem
  table_name: [(identifier) (quoted_identifier)] @type)

; Query columns
(query_column
  name: [(identifier) (quoted_identifier)] @property)

; Report dataitems
(report_dataitem
  name: [(identifier) (quoted_identifier)] @variable)
(report_dataitem
  table_name: [(identifier) (quoted_identifier)] @type)

; Report columns
(report_column
  name: [(identifier) (quoted_identifier)] @property)

; =============================================================================
; Page Fields
; =============================================================================

(page_field
  name: [(identifier) (quoted_identifier)] @property)

; =============================================================================
; XMLport Elements
; =============================================================================

(xmlport_element
  name: [(identifier) (quoted_identifier)] @property)
(xmlport_attribute
  name: [(identifier) (quoted_identifier)] @property)

; =============================================================================
; View Definitions
; =============================================================================

(view_definition
  name: [(identifier) (quoted_identifier)] @property.definition)

; =============================================================================
; DotNet
; =============================================================================

(assembly_declaration
  name: [(string_literal) (quoted_identifier) (dotnet_assembly_name)] @string)

(type_declaration) @type

; =============================================================================
; Permission Types
; =============================================================================

(permission_type) @keyword
(tabledata_permission
  table_name: [(identifier) (quoted_identifier)] @type)

; =============================================================================
; Errors
; =============================================================================

(ERROR) @error

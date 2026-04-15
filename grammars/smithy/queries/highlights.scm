
; Preproc

(control_key) @preproc

; Namespace

(namespace) @namespace

; Includes

[
  "use"
] @include

; Fields (Members)

(key_identifier) @field
(shape_member
  (field) @field)
(operation_field) @field
(operation_error_field) @field

; Constants

(enum_member
  (enum_field) @constant)

; Types

(identifier) @type
(structure_resource
  (shape_id) @type)

; Attributes

(mixins
  (shape_id) @attribute)
(trait_statement
  (shape_id (#set! "priority" 105)) @attribute)

; Operators

[
  "@"
  "-"
  "="
  ":="
] @operator

; Keywords

[
  "apply"
  "for"
  "metadata"
  "namespace"
  "with"
  ; shape types
  "bigDecimal"
  "bigInteger"
  "blob"
  "boolean"
  "byte"
  "document"
  "double"
  "enum"
  "float"
  "intEnum"
  "integer"
  "list"
  "long"
  "map"
  "operation"
  "resource"
  "service"
  "set"
  "short"
  "string"
  "structure"
  "timestamp"
  "union"
] @keyword

; Literals

(string) @string
(escape_sequence) @string.escape

(number) @number

(float) @float

(boolean) @boolean

(null) @constant.builtin

; Misc

[
  "$"
  "#"
] @punctuation.special

["{" "}"] @punctuation.bracket

["(" ")"] @punctuation.bracket

["[" "]"] @punctuation.bracket

[
  ":"
  "."
] @punctuation.delimiter

; Comments

[
  (comment)
  (documentation_comment)
] @spell @comment

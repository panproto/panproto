; Punctuation
[
  "%["
  "@["
  "~["
  "*["
  "]"
  "("
  ")"
] @punctuation.bracket

[
  eq: _
  ","
] @punctuation.delimiter

[
  "%"
  "?"
  "#"
] @punctuation.special

; Entities
(intent) @module

(slot) @type

(variation) @variable.member

(alias) @embedded

(number) @number

(argument
  key: (string) @attribute
  value: (string) @string)

(escape) @string.escape

; Import
"import" @keyword

(file) @string.special

; Text
(word) @markup

; Comment
(comment) @comment

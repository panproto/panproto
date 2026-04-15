; The markup doesn't contain elements like "classes" or "functions".
; These are used to provide a reasonable treesitter based jump and
; select experience.
; For instance "change inner function" allows us to replace an
; entire block quote, leaving the ">" prefix.
; The choices are a bit subjective though.
; Classes, the highest level
(thematic_break) @class.outer

(section
  (section_content) @class.inner
  (#offset! @class.inner 0 0 -1 0)) @class.outer

; Functions, the next level
(heading
  (content) @function.inner) @function.outer

(div
  (content) @function.inner) @function.outer

(block_quote
  (content) @function.inner) @function.outer

(code_block
  (code) @function.inner) @function.outer

(raw_block
  (content) @function.inner) @function.outer

; Inner selects current list item, outer selects whole list
(list
  (_) @function.inner) @function.outer

; Inner selects row, outer selects whole table
(table
  (_) @function.inner) @function.outer

(footnote
  (footnote_content) @function.inner) @function.outer

; Blocks, included inside functions
(list_item
  (list_item_content) @block.inner) @block.outer

(table_row) @block.outer

(table_separator) @block.outer

[
  (table_cell_alignment)
  (table_cell)
] @block.inner

; Attributes, extra things attached to elements
(block_attribute
  (args) @attribute.inner) @attribute.outer

(inline_attribute
  (args) @attribute.inner) @attribute.outer

(table_caption
  (content) @attribute.inner) @attribute.outer

(emphasis
  (content) @attribute.inner) @attribute.outer

(strong
  (content) @attribute.inner) @attribute.outer

(highlighted
  (content) @attribute.inner) @attribute.outer

(insert
  (content) @attribute.inner) @attribute.outer

(delete
  (content) @attribute.inner) @attribute.outer

(symbol) @attribute.outer

(superscript
  (content) @attribute.inner) @attribute.outer

(subscript
  (content) @attribute.inner) @attribute.outer

(verbatim
  (content) @attribute.inner) @attribute.outer

; Parameters, inside a description of a thing
[
  (class_name)
  (class)
  (identifier)
  (key_value)
  (language)
] @parameter.outer

[
  (key)
  (value)
] @parameter.inner

; Statements, extra outer definitions
(link_reference_definition
  (_) @statement.inner) @statement.outer

; Footnote is a function, can't reuse that here.
; Use @statement.outer as a jump-to point.
(footnote
  (reference_label) @statement.inner)

(footnote
  (footnote_marker_begin) @statement.outer)

; Comments
(comment
  (content) @comment.inner) @comment.outer

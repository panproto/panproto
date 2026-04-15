([
  "{{"
  "}}"
  "{{-"
  "-}}"
  "{%"
  "%}"
  "{%-"
  "-%}"
  ] @tag.delimiter
 (#set! priority 101))


([
  "]"
  "["
  ")"
  "("
  ] @punctuation.bracket
 (#set! priority 101))

([
  ","
  "."
  ] @punctuation.delimiter
 (#set! priority 101))


([
  "as"
  "assign"
  (break_statement)
  "by"
  "capture"
  "case"
  (continue_statement)
  (custom_unpaired_statement)
  "cycle"
  "decrement"
  "doc"
  "echo"
  "else"
  "elsif"
  "endcapture"
  "endcase"
  "enddoc"
  "endfor"
  "endform"
  "endif"
  "endjavascript"
  "endpaginate"
  "endraw"
  "endschema"
  "endstyle"
  "endstylesheet"
  "endtablerow"
  "endunless"
  "for"
  "form"
  "if"
  "include"
  "include_relative"
  "increment"
  "javascript"
  "layout"
  "liquid"
  "paginate"
  "raw"
  "render"
  "schema"
  "section"
  "sections"
  "style"
  "stylesheet"
  "tablerow"
  "unless"
  "when"
  "with"
  ] @keyword
 (#set! priority 101))

([
  "and"
  "contains"
  "in"
  "or"
  ] @keyword.operator
 (#set! priority 101))

([
  "|"
  ":"
  "="
  (predicate)
  ] @operator
 (#set! priority 101))

((identifier) @variable (#set! priority 101))
((string) @string (#set! priority 101))
((boolean) @boolean (#set! priority 101))
((number) @number (#set! priority 101))

(filter
  name: (identifier) @function.call (#set! priority 101))

(raw_statement
  (raw_content) @text.reference (#set! priority 102))

((comment) @comment (#set! priority 102))

((doc) @comment.documentation (#set! priority 102))
((doc_content) @comment.documentation (#set! priority 102))
((doc_description_annotation) @keyword (#set! priority 103))
((doc_example_annotation) @comment.documentation (#set! priority 102))
((doc_example_content) @none (#set! priority 103))
((doc_param_name) @variable (#set! priority 103))
((doc_type) @type (#set! priority 103))
("@param" @keyword (#set! priority 103))
("@example" @keyword (#set! priority 103))

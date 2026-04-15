; tree-sitter-awk v0.7.2

; https://tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries

; Order matters

(ns_qualified_name (namespace) @namespace)
(ns_qualified_name "::" @operator)

(func_def name: (_ (identifier) @function) @function)
(func_call name: (_ (identifier) @function) @function)

[
  (identifier)
  (field_ref)
] @variable
(field_ref (_) @variable)

(string) @string
(number) @number
(regex) @regexp
(comment) @comment

[
  "function"
  "func"
  "print"
  "printf"
  "if"
  "else"
  "do"
  "while"
  "for"
  "in"
  "delete"
  "return"
  "exit"
  "switch"
  "case"
  "default"
  (break_statement)
  (continue_statement)
  (next_statement)
  (nextfile_statement)
  (getline_input)
  (getline_file)
] @keyword

[
  "@include"
  "@load"
  "@namespace"
  (pattern)
] @namespace

(binary_exp [
  "^"
  "**"
  "*"
  "/"
  "%"
  "+"
  "-"
  "<"
  ">"
  "<="
  ">="
  "=="
  "!="
  "~"
  "!~"
  "in"
  "&&"
  "||"
] @operator)

(unary_exp [
  "!"
  "+"
  "-"
] @operator)

(assignment_exp [
  "="
  "+="
  "-="
  "*="
  "/="
  "%="
  "^="
] @operator)

(ternary_exp [
  "?"
  ":"
] @operator)

(update_exp [
  "++"
  "--"
] @operator)

(redirected_io_statement [
  ">"
  ">>"
] @operator)

(piped_io_statement [
  "|"
  "|&"
] @operator)

[
  ";"
  ","
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @operator

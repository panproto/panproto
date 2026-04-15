[
  (block)
  (defined_resource_type)
  (parameter_list)
  (attribute_type_entry)
  (resource_declaration)
  (selector)
  (case_statement)
  (hash)
  (array)
] @indent.begin


[
  "}"
  "]"
  ")"
] @indent.end

[ "{" "}" ] @indent.branch

[ "[" "]" ] @indent.branch

[ "(" ")" ] @indent.branch

[
  (string)
  (comment)
  (ERROR)
] @indent.auto

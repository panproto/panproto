(match key: _ @keyword)
(assignment key: _ @property)

[
  (system_const)
  (run_type)
  (import_type)
  (attribute)
  (kernel_param)
  (seclabel)
] @attribute

(env_var) @constant

(value) @string

(pattern) @string.regexp

(fmt_sub . _ @variable.builtin)

(var_sub . _ @variable.builtin)

[
 "\\\""
 (c_escape)
] @string.escape

[
  (octal)
  (number)
] @number

[
  (match_op)
  (assignment_op)
] @operator

"+" @punctuation.special

[ "{" "}" ] @punctuation.bracket

[ "," (linebreak) ] @punctuation.delimiter

(comment) @comment

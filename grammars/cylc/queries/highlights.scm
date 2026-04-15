(ERROR) @emphasis.strong

(include_statement) @embedded

(comment) @comment

(_
  brackets_open: _ @operator
  name: _? @title
  brackets_close: _ @operator)

(graph_section
  name: _? @property)

(task_section
  name: _? @emphasis)



(graph_setting
  key: (_) @number
  operator: (_)? @operator)

(quoted_graph_string
  quotes_open: _ @string
  quotes_close: _ @string)

(multiline_graph_string
  quotes_open: _ @string
  quotes_close: _ @string)

[
  (graph_logical) 
  (graph_arrow)
  (graph_parenthesis)
] @operator

(intercycle_annotation
  (recurrence) @number)

(graph_task
  xtrigger: _? @property
  suicide: _? @property
  name: _ @emphasis)

(task_parameter
  "<" @punctuation
  (nametag)? @text.literal
  ">" @punctuation)

(intercycle_annotation
  "[" @punctuation
  (recurrence)? @number
  "]" @punctuation)

(task_output
    ":" @punctuation
    (nametag) @variable
    "?"? @punctuation)

(setting
  key: (key) @variable
  operator: (_)? @operator
  value: [
    (unquoted_string) @string
    (quoted_string) @string
    (multiline_string) @string
    (boolean) @boolean
    (integer) @number
  ]?)

(datetime) @number

[
  (jinja2_expression)
  (jinja2_statement)
  (jinja2_comment)
  (jinja2_shebang)
] @text.literal
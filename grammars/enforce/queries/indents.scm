[
  (block)
  (class_body)
  (enum_body)
  (switch_body)
  (array_creation)
  (formal_parameters)
  (actual_parameters)
] @indent.begin

[
  "("
  ")"
  "}"
] @indent.branch

[
  ")"
  "}"
] @indent.end

(if
  condition: (_) @indent.begin)

; Supports if without braces (but not both if-else without braces)
(if
  consequence: (_) @indent.end @_consequence
  (#not-kind-eq? @_consequence "block")
  ("else" @indent.branch
    [
      (if
        (block) @indent.dedent)? @indent.dedent
      (block)? @indent.dedent
      (_)? @indent.dedent
    ])?) @indent.begin

(comment_line) @indent.ignore

[
  (ERROR)
  (comment_block)
] @indent.auto

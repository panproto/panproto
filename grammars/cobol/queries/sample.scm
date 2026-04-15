(
  (move_statement
     src: (_)
     dst: (qualified_word) @first-dst)
  .
  (move_statement
     src: (_)
     dst: (qualified_word) @second-dst)
  (#eq? @first-dst @second-dst)
)

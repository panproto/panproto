; Indent after opening braces
[
  (code_segment)
  (class_body)
] @indent.begin

; Dedent at closing braces
"}" @indent.end @indent.branch

; Brackets
"(" @indent.begin
")" @indent.end @indent.branch
"[" @indent.begin
"]" @indent.end @indent.branch

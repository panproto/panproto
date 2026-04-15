((match
  key: [ "PROGRAM" "RUN" ]
  (value (content) @injection.content))
  (#set! injection.language "bash"))

((assignment
  key: "RUN"
  (value (content) @injection.content))
  (#set! injection.language "bash"))

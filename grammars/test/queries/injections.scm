(test
  (header
    (name)
    (attributes
      . ; skip over non-language attributes
      (attribute
        !language)*
      . ; select only the first language attribute
      (attribute
        language: (parameter) @injection.language)))
  (input) @injection.content)

((test
  [
    (header
      (separator)
      .
      (name)
      .
      (separator))
    (header
      (attributes
        .
        (attribute
          !cst)+ .))
  ]
  (output) @injection.content)
  (#set! injection.language "query"))

((test
  (header
    (attributes
      (attribute
        cst: _)))
  (output) @injection.content)
  (#set! injection.language "cst"))

; NOTE: custom dynamic injection
((test
  (header
    [
      (attributes
        (attribute
          !language))
      ((name)
        .
        (separator))
    ])
  (input) @injection.content)
  (#set-language-from-grammar!))

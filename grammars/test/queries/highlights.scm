(name) @markup.heading

[
  ":cst"
  ":error"
  ":fail-fast"
  ":language"
  ":platform"
  ":skip"
] @attribute

(attribute
  language: (parameter) @string)

(attribute
  platform: (parameter) @constant.builtin)

((test
  (separator) @punctuation.delimiter)
  (#set! conceal ""))

((header
  (separator) @punctuation.delimiter)
  (#set! conceal ""))

[
  "("
  ")"
] @punctuation.bracket

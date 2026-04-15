"let" @keyword
"in" @keyword

[
  "{"
  "}"
  "["
  "]"
] @punctuation.bracket

(path_seg) @string.special.key
"." @punctuation.delimiter

(input) @constant
(comment) @comment

(string) @string
(char) @string
(integer) @number
(float) @float
(boolean) @boolean
(null) @keyword

(ERROR) @error

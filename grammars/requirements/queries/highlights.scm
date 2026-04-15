;; packages

(package) @variable

(extras (package) @variable.parameter)

(path) @markup.link

(url) @markup.link.url

;; versions

(version_cmp) @operator

(version) @number

;; markers

(marker_var) @attribute

(marker_op) @keyword

;; options

(option) @function

"=" @operator

;; punctuation

[ "[" "]" "(" ")" ] @punctuation.bracket

[ "," ";" "@" ] @punctuation.delimiter

[ "${" "}" ] @punctuation.special

;; misc

(env_var) @constant

(quoted_string) @string

(linebreak) @escape

(comment) @comment

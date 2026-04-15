(comment) @comment

(tag_name) @constant
(
  (tag_name) @constant.builtin
  ; https://www.script-example.com/html-tag-liste
  (#any-of? @constant.builtin
   "head" "title" "base" "link" "meta" "style"
   "body" "article" "section" "nav" "aside" "h1" "h2" "h3" "h4" "h5" "h6" "hgroup" "header" "footer" "address"
   "p" "hr" "pre" "blockquote" "ol" "ul" "menu" "li" "dl" "dt" "dd" "figure" "figcaption" "main" "div"
   "a" "em" "strong" "small" "s" "cite" "q" "dfn" "abbr" "ruby" "rt" "rp" "data" "time" "code" "var" "samp" "kbd" "sub" "sup" "i" "b" "u" "mark" "bdi" "bdo" "span" "br" "wbr"
   "ins" "del"
   "picture" "source" "img" "iframe" "embed" "object" "param" "video" "audio" "track" "map" "area"
   "table" "caption" "colgroup" "col" "tbody" "thead" "tfoot" "tr" "td" "th"
   "form" "label" "input" "button" "select" "datalist" "optgroup" "option" "textarea" "output" "progress" "meter" "fieldset" "legend"
   "details" "summary" "dialog"
   "script" "noscript" "template" "slot" "canvas")
)

(content) @none

(id) @attribute
(class) @attribute

(quoted_attribute_value) @string
(attribute_name) @symbol
(
  (attribute_name) @keyword
  (#match? @keyword "^(\\(.*\\)|\\[.*\\]|\\*.*)$")
) @keyword

[
  ":"
  "{{"
  "}}"
  "+"
  "|"
] @punctuation.delimiter

(keyword) @keyword
((keyword) @include (#eq? @include "include"))
((keyword) @repeat (#any-of? @repeat "for" "each" "of" "in" "while"))
((keyword) @conditional (#any-of? @conditional "if" "else" "else if" "unless"))
((keyword) @keyword.function (#any-of? @keyword.function "block" "mixin"))

(filter_name) @method.call

(mixin_use (mixin_name) @method.call)
(mixin_definition (mixin_name) @function)


; highlights.scm

"[Query]" @property
"[QueryStringParams]" @property
"[Form]" @property
"[FormParams]" @property
"[Multipart]" @property
"[MultipartFormData]" @property
"[Cookies]" @property
"[Captures]" @property
"[Asserts]" @property
"[Options]" @property
"[BasicAuth]" @property

(comment) @comment @spell

(key_string) @property
(json_key_string) @property

(value_string) @string
(quoted_string) @string
(json_string) @string
(file_value) @string.special
(regex) @string.regex

"\\" @string.escape
(regex_escaped_char) @string.escape
(quoted_string_escaped_char) @string.escape
(key_string_escaped_char) @string.escape
(value_string_escaped_char) @string.escape
(oneline_string_escaped_char) @string.escape
(multiline_string_escaped_char) @string.escape
(filename_escaped_char) @string.escape
(json_string_escaped_char) @string.escape

(method) @type.builtin
(multiline_string_type) @type

"status" @function.builtin
"url" @function.builtin
"header" @function.builtin
"cookie" @function.builtin
"body" @function.builtin
"xpath" @function.builtin
"jsonpath" @function.builtin
"regex" @function.builtin
"variable" @function.builtin
"duration" @function.builtin
"sha256" @function.builtin
"md5" @function.builtin
"bytes" @function.builtin
"daysAfterNow" @function.builtin
"daysBeforeNow" @function.builtin
"htmlEscape" @function.builtin
"htmlUnescape" @function.builtin
"decode" @function.builtin
"format" @function.builtin
"nth" @function.builtin
"replace" @function.builtin
"split" @function.builtin
"toDate" @function.builtin
"toInt" @function.builtin
"urlEncode" @function.builtin
"urlDecode" @function.builtin
"count" @function.builtin

(filter _) @attribute

(version) @string.special

"null" @constant.builtin
(option (_ key: _ @constant.builtin) )

(boolean) @boolean

(variable_name) @variable

"==" @operator
"!=" @operator
">" @operator
">=" @operator
"<" @operator
"<=" @operator
(_ predicate_name: _ @keyword.operator)


(integer) @number
(float) @float
(status) @number
(json_number) @float

":" @punctuation.delimiter
"," @punctuation.delimiter

"[" @punctuation.bracket
"]" @punctuation.bracket
"{" @punctuation.bracket
"}" @punctuation.bracket
"{{" @punctuation.special
"}}" @punctuation.special

"base64," @string.special
"file," @string.special
"hex," @string.special

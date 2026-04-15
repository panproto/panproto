((setting
  key: (key) @key (#any-of? @key
      "script"
      "pre-script"
      "post-script")
  value: (_ 
    (string_content) @content))
 (#set! "language" "bash"))

; Requires no spacing around "=" in environment settings for proper highlighting.
; Could be improved if Zed allows to specify the target node of the injected 
; language, instead of using the root node.
; See this proposal:
; https://github.com/tree-sitter/tree-sitter/issues/3625
((task_section
  (sub_section_2
    name: (_) @section_name (#match? @section_name "environment")
    (setting) @content))
 (#set! "language" "bash")
 (#set! combined))

;; Auto-generated UnicodeLib.axi built-ins

;; Functions
(call_expression
  function: (identifier) @function.builtin
  (#match? @function.builtin "(?i)^(wc_encode|wc_decode|_wc|wc_to_ch|ch_to_wc|wc_find_string|wc_left_string|wc_length_string|wc_lower_string|wc_max_length_string|wc_mid_string|wc_remove_string|wc_right_string|wc_set_length_string|wc_upper_string|wc_compare_string|wc_get_buffer_char|wc_get_buffer_string|wc_concat_string|__wc_explode_file_handle|__wc_compose_file_handle|__wc_get_file_header|__wc_get_file_format|wc_file_open|wc_file_close|wc_file_read|wc_file_read_line|wc_file_write|wc_file_write_line|wc_tp_encode)$"))

;; Constants
(identifier) @constant.builtin
  (#match? @constant.builtin "(?i)^(__unicode_lib_version__|wc_format_ascii|wc_format_unicode|wc_format_unicode_be|wc_format_utf8|wc_format_tp|wc_max_string_size|wc_max_g3_str_length|wc_max_g4_str_length)$")

;; Variables
(identifier) @variable.builtin
  (#match? @variable.builtin "(?i)^(cwc_upper_lookup|cwc_upper_result|cwc_lower_lookup|cwc_lower_result)$")
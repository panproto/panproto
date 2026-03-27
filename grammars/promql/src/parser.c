#include <tree_sitter/parser.h>

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 150
#define LARGE_STATE_COUNT 2
#define SYMBOL_COUNT 88
#define ALIAS_COUNT 0
#define TOKEN_COUNT 54
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 0
#define MAX_ALIAS_SEQUENCE_LENGTH 12
#define PRODUCTION_ID_COUNT 1

enum {
  anon_sym_LPAREN = 1,
  anon_sym_RPAREN = 2,
  sym_float_literal = 3,
  anon_sym_LBRACK = 4,
  anon_sym_RBRACK = 5,
  anon_sym_offset = 6,
  anon_sym_DASH = 7,
  anon_sym_AT = 8,
  anon_sym_start_LPAREN_RPAREN = 9,
  anon_sym_end_LPAREN_RPAREN = 10,
  aux_sym_at_token1 = 11,
  anon_sym_LBRACE = 12,
  anon_sym_COMMA = 13,
  anon_sym_RBRACE = 14,
  anon_sym_EQ = 15,
  anon_sym_BANG_EQ = 16,
  anon_sym_EQ_TILDE = 17,
  anon_sym_BANG_TILDE = 18,
  aux_sym_grouping_token1 = 19,
  aux_sym_grouping_token2 = 20,
  anon_sym_CARET = 21,
  anon_sym_STAR = 22,
  anon_sym_SLASH = 23,
  anon_sym_PERCENT = 24,
  anon_sym_PLUS = 25,
  anon_sym_EQ_EQ = 26,
  anon_sym_GT = 27,
  anon_sym_GT_EQ = 28,
  anon_sym_LT = 29,
  anon_sym_LT_EQ = 30,
  aux_sym_binary_expression_token1 = 31,
  aux_sym_binary_expression_token2 = 32,
  aux_sym_binary_expression_token3 = 33,
  aux_sym_binary_expression_token4 = 34,
  aux_sym_binary_expression_token5 = 35,
  aux_sym_binary_grouping_token1 = 36,
  aux_sym_binary_grouping_token2 = 37,
  aux_sym_binary_grouping_token3 = 38,
  aux_sym_binary_grouping_token4 = 39,
  anon_sym_COLON = 40,
  sym__double_quoted_string = 41,
  sym__single_quoted_string = 42,
  sym__backtick_quoted_string = 43,
  aux_sym__duration_token1 = 44,
  anon_sym_ms = 45,
  anon_sym_s = 46,
  anon_sym_m = 47,
  anon_sym_h = 48,
  anon_sym_d = 49,
  anon_sym_w = 50,
  anon_sym_y = 51,
  sym__identifier = 52,
  sym_comment = 53,
  sym_query = 54,
  sym__query = 55,
  sym__query_expression = 56,
  sym__literal_expression = 57,
  sym_string_literal = 58,
  sym__selector_expression = 59,
  sym_instant_vector_selector = 60,
  sym_range_vector_selector = 61,
  sym_range_selection = 62,
  sym_modifier = 63,
  sym_offset = 64,
  sym_at = 65,
  sym__series_matcher = 66,
  sym_label_selectors = 67,
  sym_label_matcher = 68,
  sym_metric_name = 69,
  sym_label_name = 70,
  sym_label_value = 71,
  sym__call_expression = 72,
  sym_function_call = 73,
  sym_function_name = 74,
  sym_function_args = 75,
  sym_grouping = 76,
  sym__operator_expression = 77,
  sym_binary_expression = 78,
  sym_binary_grouping = 79,
  sym__subquery_expression = 80,
  sym_subquery = 81,
  sym_subquery_range_selection = 82,
  sym__quoted_string = 83,
  aux_sym__duration = 84,
  aux_sym_label_selectors_repeat1 = 85,
  aux_sym_function_args_repeat1 = 86,
  aux_sym_grouping_repeat1 = 87,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [anon_sym_LPAREN] = "(",
  [anon_sym_RPAREN] = ")",
  [sym_float_literal] = "float_literal",
  [anon_sym_LBRACK] = "[",
  [anon_sym_RBRACK] = "]",
  [anon_sym_offset] = "offset",
  [anon_sym_DASH] = "-",
  [anon_sym_AT] = "@",
  [anon_sym_start_LPAREN_RPAREN] = "start()",
  [anon_sym_end_LPAREN_RPAREN] = "end()",
  [aux_sym_at_token1] = "at_token1",
  [anon_sym_LBRACE] = "{",
  [anon_sym_COMMA] = ",",
  [anon_sym_RBRACE] = "}",
  [anon_sym_EQ] = "=",
  [anon_sym_BANG_EQ] = "!=",
  [anon_sym_EQ_TILDE] = "=~",
  [anon_sym_BANG_TILDE] = "!~",
  [aux_sym_grouping_token1] = "grouping_token1",
  [aux_sym_grouping_token2] = "grouping_token2",
  [anon_sym_CARET] = "^",
  [anon_sym_STAR] = "*",
  [anon_sym_SLASH] = "/",
  [anon_sym_PERCENT] = "%",
  [anon_sym_PLUS] = "+",
  [anon_sym_EQ_EQ] = "==",
  [anon_sym_GT] = ">",
  [anon_sym_GT_EQ] = ">=",
  [anon_sym_LT] = "<",
  [anon_sym_LT_EQ] = "<=",
  [aux_sym_binary_expression_token1] = "binary_expression_token1",
  [aux_sym_binary_expression_token2] = "binary_expression_token2",
  [aux_sym_binary_expression_token3] = "binary_expression_token3",
  [aux_sym_binary_expression_token4] = "binary_expression_token4",
  [aux_sym_binary_expression_token5] = "binary_expression_token5",
  [aux_sym_binary_grouping_token1] = "binary_grouping_token1",
  [aux_sym_binary_grouping_token2] = "binary_grouping_token2",
  [aux_sym_binary_grouping_token3] = "binary_grouping_token3",
  [aux_sym_binary_grouping_token4] = "binary_grouping_token4",
  [anon_sym_COLON] = ":",
  [sym__double_quoted_string] = "_double_quoted_string",
  [sym__single_quoted_string] = "_single_quoted_string",
  [sym__backtick_quoted_string] = "_backtick_quoted_string",
  [aux_sym__duration_token1] = "_duration_token1",
  [anon_sym_ms] = "ms",
  [anon_sym_s] = "s",
  [anon_sym_m] = "m",
  [anon_sym_h] = "h",
  [anon_sym_d] = "d",
  [anon_sym_w] = "w",
  [anon_sym_y] = "y",
  [sym__identifier] = "_identifier",
  [sym_comment] = "comment",
  [sym_query] = "query",
  [sym__query] = "_query",
  [sym__query_expression] = "_query_expression",
  [sym__literal_expression] = "_literal_expression",
  [sym_string_literal] = "string_literal",
  [sym__selector_expression] = "_selector_expression",
  [sym_instant_vector_selector] = "instant_vector_selector",
  [sym_range_vector_selector] = "range_vector_selector",
  [sym_range_selection] = "range_selection",
  [sym_modifier] = "modifier",
  [sym_offset] = "offset",
  [sym_at] = "at",
  [sym__series_matcher] = "_series_matcher",
  [sym_label_selectors] = "label_selectors",
  [sym_label_matcher] = "label_matcher",
  [sym_metric_name] = "metric_name",
  [sym_label_name] = "label_name",
  [sym_label_value] = "label_value",
  [sym__call_expression] = "_call_expression",
  [sym_function_call] = "function_call",
  [sym_function_name] = "function_name",
  [sym_function_args] = "function_args",
  [sym_grouping] = "grouping",
  [sym__operator_expression] = "_operator_expression",
  [sym_binary_expression] = "binary_expression",
  [sym_binary_grouping] = "binary_grouping",
  [sym__subquery_expression] = "_subquery_expression",
  [sym_subquery] = "subquery",
  [sym_subquery_range_selection] = "subquery_range_selection",
  [sym__quoted_string] = "_quoted_string",
  [aux_sym__duration] = "_duration",
  [aux_sym_label_selectors_repeat1] = "label_selectors_repeat1",
  [aux_sym_function_args_repeat1] = "function_args_repeat1",
  [aux_sym_grouping_repeat1] = "grouping_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [anon_sym_RPAREN] = anon_sym_RPAREN,
  [sym_float_literal] = sym_float_literal,
  [anon_sym_LBRACK] = anon_sym_LBRACK,
  [anon_sym_RBRACK] = anon_sym_RBRACK,
  [anon_sym_offset] = anon_sym_offset,
  [anon_sym_DASH] = anon_sym_DASH,
  [anon_sym_AT] = anon_sym_AT,
  [anon_sym_start_LPAREN_RPAREN] = anon_sym_start_LPAREN_RPAREN,
  [anon_sym_end_LPAREN_RPAREN] = anon_sym_end_LPAREN_RPAREN,
  [aux_sym_at_token1] = aux_sym_at_token1,
  [anon_sym_LBRACE] = anon_sym_LBRACE,
  [anon_sym_COMMA] = anon_sym_COMMA,
  [anon_sym_RBRACE] = anon_sym_RBRACE,
  [anon_sym_EQ] = anon_sym_EQ,
  [anon_sym_BANG_EQ] = anon_sym_BANG_EQ,
  [anon_sym_EQ_TILDE] = anon_sym_EQ_TILDE,
  [anon_sym_BANG_TILDE] = anon_sym_BANG_TILDE,
  [aux_sym_grouping_token1] = aux_sym_grouping_token1,
  [aux_sym_grouping_token2] = aux_sym_grouping_token2,
  [anon_sym_CARET] = anon_sym_CARET,
  [anon_sym_STAR] = anon_sym_STAR,
  [anon_sym_SLASH] = anon_sym_SLASH,
  [anon_sym_PERCENT] = anon_sym_PERCENT,
  [anon_sym_PLUS] = anon_sym_PLUS,
  [anon_sym_EQ_EQ] = anon_sym_EQ_EQ,
  [anon_sym_GT] = anon_sym_GT,
  [anon_sym_GT_EQ] = anon_sym_GT_EQ,
  [anon_sym_LT] = anon_sym_LT,
  [anon_sym_LT_EQ] = anon_sym_LT_EQ,
  [aux_sym_binary_expression_token1] = aux_sym_binary_expression_token1,
  [aux_sym_binary_expression_token2] = aux_sym_binary_expression_token2,
  [aux_sym_binary_expression_token3] = aux_sym_binary_expression_token3,
  [aux_sym_binary_expression_token4] = aux_sym_binary_expression_token4,
  [aux_sym_binary_expression_token5] = aux_sym_binary_expression_token5,
  [aux_sym_binary_grouping_token1] = aux_sym_binary_grouping_token1,
  [aux_sym_binary_grouping_token2] = aux_sym_binary_grouping_token2,
  [aux_sym_binary_grouping_token3] = aux_sym_binary_grouping_token3,
  [aux_sym_binary_grouping_token4] = aux_sym_binary_grouping_token4,
  [anon_sym_COLON] = anon_sym_COLON,
  [sym__double_quoted_string] = sym__double_quoted_string,
  [sym__single_quoted_string] = sym__single_quoted_string,
  [sym__backtick_quoted_string] = sym__backtick_quoted_string,
  [aux_sym__duration_token1] = aux_sym__duration_token1,
  [anon_sym_ms] = anon_sym_ms,
  [anon_sym_s] = anon_sym_s,
  [anon_sym_m] = anon_sym_m,
  [anon_sym_h] = anon_sym_h,
  [anon_sym_d] = anon_sym_d,
  [anon_sym_w] = anon_sym_w,
  [anon_sym_y] = anon_sym_y,
  [sym__identifier] = sym__identifier,
  [sym_comment] = sym_comment,
  [sym_query] = sym_query,
  [sym__query] = sym__query,
  [sym__query_expression] = sym__query_expression,
  [sym__literal_expression] = sym__literal_expression,
  [sym_string_literal] = sym_string_literal,
  [sym__selector_expression] = sym__selector_expression,
  [sym_instant_vector_selector] = sym_instant_vector_selector,
  [sym_range_vector_selector] = sym_range_vector_selector,
  [sym_range_selection] = sym_range_selection,
  [sym_modifier] = sym_modifier,
  [sym_offset] = sym_offset,
  [sym_at] = sym_at,
  [sym__series_matcher] = sym__series_matcher,
  [sym_label_selectors] = sym_label_selectors,
  [sym_label_matcher] = sym_label_matcher,
  [sym_metric_name] = sym_metric_name,
  [sym_label_name] = sym_label_name,
  [sym_label_value] = sym_label_value,
  [sym__call_expression] = sym__call_expression,
  [sym_function_call] = sym_function_call,
  [sym_function_name] = sym_function_name,
  [sym_function_args] = sym_function_args,
  [sym_grouping] = sym_grouping,
  [sym__operator_expression] = sym__operator_expression,
  [sym_binary_expression] = sym_binary_expression,
  [sym_binary_grouping] = sym_binary_grouping,
  [sym__subquery_expression] = sym__subquery_expression,
  [sym_subquery] = sym_subquery,
  [sym_subquery_range_selection] = sym_subquery_range_selection,
  [sym__quoted_string] = sym__quoted_string,
  [aux_sym__duration] = aux_sym__duration,
  [aux_sym_label_selectors_repeat1] = aux_sym_label_selectors_repeat1,
  [aux_sym_function_args_repeat1] = aux_sym_function_args_repeat1,
  [aux_sym_grouping_repeat1] = aux_sym_grouping_repeat1,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [anon_sym_LPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RPAREN] = {
    .visible = true,
    .named = false,
  },
  [sym_float_literal] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_LBRACK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_offset] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DASH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_AT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_start_LPAREN_RPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_end_LPAREN_RPAREN] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_at_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_LBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COMMA] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EQ_TILDE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG_TILDE] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_grouping_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_grouping_token2] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_CARET] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_STAR] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SLASH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PERCENT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PLUS] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EQ_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_GT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_GT_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LT_EQ] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_binary_expression_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_binary_expression_token2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_binary_expression_token3] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_binary_expression_token4] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_binary_expression_token5] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_binary_grouping_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_binary_grouping_token2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_binary_grouping_token3] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_binary_grouping_token4] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [sym__double_quoted_string] = {
    .visible = false,
    .named = true,
  },
  [sym__single_quoted_string] = {
    .visible = false,
    .named = true,
  },
  [sym__backtick_quoted_string] = {
    .visible = false,
    .named = true,
  },
  [aux_sym__duration_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_ms] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_s] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_m] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_h] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_d] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_w] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_y] = {
    .visible = true,
    .named = false,
  },
  [sym__identifier] = {
    .visible = false,
    .named = true,
  },
  [sym_comment] = {
    .visible = true,
    .named = true,
  },
  [sym_query] = {
    .visible = true,
    .named = true,
  },
  [sym__query] = {
    .visible = false,
    .named = true,
  },
  [sym__query_expression] = {
    .visible = false,
    .named = true,
  },
  [sym__literal_expression] = {
    .visible = false,
    .named = true,
  },
  [sym_string_literal] = {
    .visible = true,
    .named = true,
  },
  [sym__selector_expression] = {
    .visible = false,
    .named = true,
  },
  [sym_instant_vector_selector] = {
    .visible = true,
    .named = true,
  },
  [sym_range_vector_selector] = {
    .visible = true,
    .named = true,
  },
  [sym_range_selection] = {
    .visible = true,
    .named = true,
  },
  [sym_modifier] = {
    .visible = true,
    .named = true,
  },
  [sym_offset] = {
    .visible = true,
    .named = true,
  },
  [sym_at] = {
    .visible = true,
    .named = true,
  },
  [sym__series_matcher] = {
    .visible = false,
    .named = true,
  },
  [sym_label_selectors] = {
    .visible = true,
    .named = true,
  },
  [sym_label_matcher] = {
    .visible = true,
    .named = true,
  },
  [sym_metric_name] = {
    .visible = true,
    .named = true,
  },
  [sym_label_name] = {
    .visible = true,
    .named = true,
  },
  [sym_label_value] = {
    .visible = true,
    .named = true,
  },
  [sym__call_expression] = {
    .visible = false,
    .named = true,
  },
  [sym_function_call] = {
    .visible = true,
    .named = true,
  },
  [sym_function_name] = {
    .visible = true,
    .named = true,
  },
  [sym_function_args] = {
    .visible = true,
    .named = true,
  },
  [sym_grouping] = {
    .visible = true,
    .named = true,
  },
  [sym__operator_expression] = {
    .visible = false,
    .named = true,
  },
  [sym_binary_expression] = {
    .visible = true,
    .named = true,
  },
  [sym_binary_grouping] = {
    .visible = true,
    .named = true,
  },
  [sym__subquery_expression] = {
    .visible = false,
    .named = true,
  },
  [sym_subquery] = {
    .visible = true,
    .named = true,
  },
  [sym_subquery_range_selection] = {
    .visible = true,
    .named = true,
  },
  [sym__quoted_string] = {
    .visible = false,
    .named = true,
  },
  [aux_sym__duration] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_label_selectors_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_function_args_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_grouping_repeat1] = {
    .visible = false,
    .named = false,
  },
};

static const TSSymbol ts_alias_sequences[PRODUCTION_ID_COUNT][MAX_ALIAS_SEQUENCE_LENGTH] = {
  [0] = {0},
};

static const uint16_t ts_non_terminal_alias_map[] = {
  0,
};

static const TSStateId ts_primary_state_ids[STATE_COUNT] = {
  [0] = 0,
  [1] = 1,
  [2] = 2,
  [3] = 3,
  [4] = 4,
  [5] = 5,
  [6] = 6,
  [7] = 7,
  [8] = 8,
  [9] = 9,
  [10] = 10,
  [11] = 11,
  [12] = 12,
  [13] = 13,
  [14] = 14,
  [15] = 15,
  [16] = 16,
  [17] = 17,
  [18] = 18,
  [19] = 19,
  [20] = 20,
  [21] = 21,
  [22] = 22,
  [23] = 23,
  [24] = 24,
  [25] = 25,
  [26] = 26,
  [27] = 27,
  [28] = 28,
  [29] = 29,
  [30] = 30,
  [31] = 31,
  [32] = 32,
  [33] = 33,
  [34] = 34,
  [35] = 35,
  [36] = 36,
  [37] = 37,
  [38] = 38,
  [39] = 39,
  [40] = 40,
  [41] = 41,
  [42] = 42,
  [43] = 43,
  [44] = 44,
  [45] = 45,
  [46] = 46,
  [47] = 47,
  [48] = 48,
  [49] = 49,
  [50] = 50,
  [51] = 51,
  [52] = 52,
  [53] = 53,
  [54] = 54,
  [55] = 55,
  [56] = 56,
  [57] = 57,
  [58] = 58,
  [59] = 59,
  [60] = 60,
  [61] = 61,
  [62] = 62,
  [63] = 63,
  [64] = 64,
  [65] = 65,
  [66] = 66,
  [67] = 67,
  [68] = 68,
  [69] = 69,
  [70] = 70,
  [71] = 71,
  [72] = 72,
  [73] = 73,
  [74] = 74,
  [75] = 75,
  [76] = 76,
  [77] = 77,
  [78] = 78,
  [79] = 79,
  [80] = 80,
  [81] = 81,
  [82] = 82,
  [83] = 83,
  [84] = 84,
  [85] = 85,
  [86] = 86,
  [87] = 87,
  [88] = 88,
  [89] = 89,
  [90] = 90,
  [91] = 91,
  [92] = 92,
  [93] = 93,
  [94] = 94,
  [95] = 95,
  [96] = 96,
  [97] = 97,
  [98] = 98,
  [99] = 99,
  [100] = 100,
  [101] = 101,
  [102] = 102,
  [103] = 103,
  [104] = 104,
  [105] = 105,
  [106] = 106,
  [107] = 107,
  [108] = 108,
  [109] = 109,
  [110] = 110,
  [111] = 111,
  [112] = 112,
  [113] = 113,
  [114] = 114,
  [115] = 115,
  [116] = 116,
  [117] = 117,
  [118] = 118,
  [119] = 119,
  [120] = 120,
  [121] = 121,
  [122] = 122,
  [123] = 123,
  [124] = 124,
  [125] = 125,
  [126] = 126,
  [127] = 127,
  [128] = 128,
  [129] = 129,
  [130] = 130,
  [131] = 131,
  [132] = 132,
  [133] = 133,
  [134] = 134,
  [135] = 135,
  [136] = 136,
  [137] = 137,
  [138] = 138,
  [139] = 139,
  [140] = 140,
  [141] = 141,
  [142] = 142,
  [143] = 143,
  [144] = 144,
  [145] = 145,
  [146] = 146,
  [147] = 147,
  [148] = 148,
  [149] = 149,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  switch (state) {
    case 0:
      if (eof) ADVANCE(85);
      if (lookahead == '!') ADVANCE(19);
      if (lookahead == '"') ADVANCE(5);
      if (lookahead == '#') ADVANCE(183);
      if (lookahead == '%') ADVANCE(116);
      if (lookahead == '\'') ADVANCE(10);
      if (lookahead == '(') ADVANCE(86);
      if (lookahead == ')') ADVANCE(87);
      if (lookahead == '*') ADVANCE(114);
      if (lookahead == '+') ADVANCE(117);
      if (lookahead == ',') ADVANCE(104);
      if (lookahead == '-') ADVANCE(98);
      if (lookahead == '/') ADVANCE(115);
      if (lookahead == '0') ADVANCE(144);
      if (lookahead == ':') ADVANCE(137);
      if (lookahead == '<') ADVANCE(121);
      if (lookahead == '=') ADVANCE(106);
      if (lookahead == '>') ADVANCE(119);
      if (lookahead == '@') ADVANCE(99);
      if (lookahead == 'O') ADVANCE(55);
      if (lookahead == 'W') ADVANCE(49);
      if (lookahead == '[') ADVANCE(95);
      if (lookahead == ']') ADVANCE(96);
      if (lookahead == '^') ADVANCE(113);
      if (lookahead == '`') ADVANCE(21);
      if (lookahead == 'd') ADVANCE(150);
      if (lookahead == 'e') ADVANCE(30);
      if (lookahead == 'h') ADVANCE(149);
      if (lookahead == 'm') ADVANCE(148);
      if (lookahead == 'o') ADVANCE(27);
      if (lookahead == 's') ADVANCE(147);
      if (lookahead == 'w') ADVANCE(152);
      if (lookahead == 'y') ADVANCE(153);
      if (lookahead == '{') ADVANCE(103);
      if (lookahead == '}') ADVANCE(105);
      if (lookahead == 'A' ||
          lookahead == 'a') ADVANCE(56);
      if (lookahead == 'B' ||
          lookahead == 'b') ADVANCE(63);
      if (lookahead == 'G' ||
          lookahead == 'g') ADVANCE(71);
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(45);
      if (lookahead == 'U' ||
          lookahead == 'u') ADVANCE(57);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(0)
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(102);
      END_STATE();
    case 1:
      if (lookahead == '!') ADVANCE(19);
      if (lookahead == '"') ADVANCE(5);
      if (lookahead == '#') ADVANCE(183);
      if (lookahead == '\'') ADVANCE(10);
      if (lookahead == '(') ADVANCE(86);
      if (lookahead == ')') ADVANCE(87);
      if (lookahead == ',') ADVANCE(104);
      if (lookahead == '.') ADVANCE(81);
      if (lookahead == '0') ADVANCE(89);
      if (lookahead == '=') ADVANCE(107);
      if (lookahead == '`') ADVANCE(21);
      if (lookahead == '{') ADVANCE(103);
      if (('+' <= lookahead && lookahead <= '-')) ADVANCE(16);
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(167);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(155);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(1)
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(90);
      if (lookahead == ':' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          ('_' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 2:
      if (lookahead == '"') ADVANCE(5);
      if (lookahead == '#') ADVANCE(183);
      if (lookahead == '\'') ADVANCE(10);
      if (lookahead == '(') ADVANCE(86);
      if (lookahead == '.') ADVANCE(81);
      if (lookahead == '0') ADVANCE(89);
      if (lookahead == '`') ADVANCE(21);
      if (lookahead == '{') ADVANCE(103);
      if (lookahead == '+' ||
          lookahead == '-') ADVANCE(16);
      if (lookahead == 'B' ||
          lookahead == 'b') ADVANCE(175);
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(161);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(155);
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(169);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(2)
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(90);
      if (lookahead == ':' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          ('_' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 3:
      if (lookahead == '"') ADVANCE(5);
      if (lookahead == '#') ADVANCE(183);
      if (lookahead == '\'') ADVANCE(10);
      if (lookahead == '(') ADVANCE(86);
      if (lookahead == '.') ADVANCE(81);
      if (lookahead == '0') ADVANCE(89);
      if (lookahead == '`') ADVANCE(21);
      if (lookahead == '{') ADVANCE(103);
      if (lookahead == '+' ||
          lookahead == '-') ADVANCE(16);
      if (lookahead == 'G' ||
          lookahead == 'g') ADVANCE(178);
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(167);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(155);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(3)
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(90);
      if (lookahead == ':' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          ('_' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 4:
      if (lookahead == '"') ADVANCE(5);
      if (lookahead == '#') ADVANCE(183);
      if (lookahead == '\'') ADVANCE(10);
      if (lookahead == '(') ADVANCE(86);
      if (lookahead == '.') ADVANCE(81);
      if (lookahead == '0') ADVANCE(89);
      if (lookahead == '`') ADVANCE(21);
      if (lookahead == '{') ADVANCE(103);
      if (lookahead == '+' ||
          lookahead == '-') ADVANCE(16);
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(161);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(155);
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(169);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(4)
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(90);
      if (lookahead == ':' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          ('_' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 5:
      if (lookahead == '"') ADVANCE(138);
      if (lookahead == '\\') ADVANCE(6);
      if (lookahead != 0) ADVANCE(5);
      END_STATE();
    case 6:
      if (lookahead == '"') ADVANCE(139);
      if (lookahead == '\\') ADVANCE(6);
      if (lookahead != 0) ADVANCE(5);
      END_STATE();
    case 7:
      if (lookahead == '#') ADVANCE(183);
      if (lookahead == ')') ADVANCE(87);
      if (lookahead == '}') ADVANCE(105);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(7)
      if (lookahead == ':' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 8:
      if (lookahead == '#') ADVANCE(183);
      if (lookahead == 'd') ADVANCE(150);
      if (lookahead == 'h') ADVANCE(149);
      if (lookahead == 'm') ADVANCE(148);
      if (lookahead == 's') ADVANCE(146);
      if (lookahead == 'w') ADVANCE(151);
      if (lookahead == 'y') ADVANCE(153);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(8)
      END_STATE();
    case 9:
      if (lookahead == '#') ADVANCE(183);
      if (lookahead == 'e') ADVANCE(30);
      if (lookahead == 's') ADVANCE(33);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(9)
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(102);
      END_STATE();
    case 10:
      if (lookahead == '\'') ADVANCE(140);
      if (lookahead == '\\') ADVANCE(11);
      if (lookahead != 0) ADVANCE(10);
      END_STATE();
    case 11:
      if (lookahead == '\'') ADVANCE(141);
      if (lookahead == '\\') ADVANCE(11);
      if (lookahead != 0) ADVANCE(10);
      END_STATE();
    case 12:
      if (lookahead == '(') ADVANCE(14);
      END_STATE();
    case 13:
      if (lookahead == '(') ADVANCE(15);
      END_STATE();
    case 14:
      if (lookahead == ')') ADVANCE(101);
      END_STATE();
    case 15:
      if (lookahead == ')') ADVANCE(100);
      END_STATE();
    case 16:
      if (lookahead == '.') ADVANCE(81);
      if (lookahead == '0') ADVANCE(89);
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(61);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(38);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(90);
      END_STATE();
    case 17:
      if (lookahead == '2') ADVANCE(128);
      END_STATE();
    case 18:
      if (lookahead == '=') ADVANCE(108);
      END_STATE();
    case 19:
      if (lookahead == '=') ADVANCE(108);
      if (lookahead == '~') ADVANCE(110);
      END_STATE();
    case 20:
      if (lookahead == '=') ADVANCE(118);
      END_STATE();
    case 21:
      if (lookahead == '\\') ADVANCE(22);
      if (lookahead == '`') ADVANCE(142);
      if (lookahead != 0) ADVANCE(21);
      END_STATE();
    case 22:
      if (lookahead == '\\') ADVANCE(22);
      if (lookahead == '`') ADVANCE(143);
      if (lookahead != 0) ADVANCE(21);
      END_STATE();
    case 23:
      if (lookahead == '_') ADVANCE(54);
      END_STATE();
    case 24:
      if (lookahead == 'a') ADVANCE(31);
      END_STATE();
    case 25:
      if (lookahead == 'd') ADVANCE(12);
      END_STATE();
    case 26:
      if (lookahead == 'e') ADVANCE(34);
      END_STATE();
    case 27:
      if (lookahead == 'f') ADVANCE(29);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(129);
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(126);
      END_STATE();
    case 28:
      if (lookahead == 'f') ADVANCE(29);
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(126);
      END_STATE();
    case 29:
      if (lookahead == 'f') ADVANCE(32);
      END_STATE();
    case 30:
      if (lookahead == 'n') ADVANCE(25);
      END_STATE();
    case 31:
      if (lookahead == 'r') ADVANCE(35);
      END_STATE();
    case 32:
      if (lookahead == 's') ADVANCE(26);
      END_STATE();
    case 33:
      if (lookahead == 't') ADVANCE(24);
      END_STATE();
    case 34:
      if (lookahead == 't') ADVANCE(97);
      END_STATE();
    case 35:
      if (lookahead == 't') ADVANCE(13);
      END_STATE();
    case 36:
      if (lookahead == '+' ||
          lookahead == '-') ADVANCE(82);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(92);
      END_STATE();
    case 37:
      if (lookahead == 'A' ||
          lookahead == 'a') ADVANCE(58);
      END_STATE();
    case 38:
      if (lookahead == 'A' ||
          lookahead == 'a') ADVANCE(59);
      END_STATE();
    case 39:
      if (lookahead == 'D' ||
          lookahead == 'd') ADVANCE(125);
      END_STATE();
    case 40:
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(73);
      END_STATE();
    case 41:
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(43);
      END_STATE();
    case 42:
      if (lookahead == 'F' ||
          lookahead == 'f') ADVANCE(88);
      END_STATE();
    case 43:
      if (lookahead == 'F' ||
          lookahead == 'f') ADVANCE(76);
      END_STATE();
    case 44:
      if (lookahead == 'G' ||
          lookahead == 'g') ADVANCE(131);
      END_STATE();
    case 45:
      if (lookahead == 'G' ||
          lookahead == 'g') ADVANCE(62);
      END_STATE();
    case 46:
      if (lookahead == 'G' ||
          lookahead == 'g') ADVANCE(47);
      END_STATE();
    case 47:
      if (lookahead == 'H' ||
          lookahead == 'h') ADVANCE(77);
      END_STATE();
    case 48:
      if (lookahead == 'H' ||
          lookahead == 'h') ADVANCE(67);
      END_STATE();
    case 49:
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(74);
      END_STATE();
    case 50:
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(46);
      END_STATE();
    case 51:
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(60);
      END_STATE();
    case 52:
      if (lookahead == 'L' ||
          lookahead == 'l') ADVANCE(40);
      END_STATE();
    case 53:
      if (lookahead == 'L' ||
          lookahead == 'l') ADVANCE(123);
      END_STATE();
    case 54:
      if (lookahead == 'L' ||
          lookahead == 'l') ADVANCE(41);
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(50);
      END_STATE();
    case 55:
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(129);
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(126);
      END_STATE();
    case 56:
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(39);
      if (lookahead == 'T' ||
          lookahead == 't') ADVANCE(37);
      END_STATE();
    case 57:
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(52);
      END_STATE();
    case 58:
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(17);
      END_STATE();
    case 59:
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(88);
      END_STATE();
    case 60:
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(44);
      END_STATE();
    case 61:
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(42);
      END_STATE();
    case 62:
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(65);
      END_STATE();
    case 63:
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(66);
      if (lookahead == 'Y' ||
          lookahead == 'y') ADVANCE(111);
      END_STATE();
    case 64:
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(78);
      END_STATE();
    case 65:
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(70);
      END_STATE();
    case 66:
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(53);
      END_STATE();
    case 67:
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(79);
      END_STATE();
    case 68:
      if (lookahead == 'P' ||
          lookahead == 'p') ADVANCE(23);
      END_STATE();
    case 69:
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(126);
      END_STATE();
    case 70:
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(51);
      END_STATE();
    case 71:
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(64);
      END_STATE();
    case 72:
      if (lookahead == 'S' ||
          lookahead == 's') ADVANCE(127);
      END_STATE();
    case 73:
      if (lookahead == 'S' ||
          lookahead == 's') ADVANCE(72);
      END_STATE();
    case 74:
      if (lookahead == 'T' ||
          lookahead == 't') ADVANCE(48);
      END_STATE();
    case 75:
      if (lookahead == 'T' ||
          lookahead == 't') ADVANCE(112);
      END_STATE();
    case 76:
      if (lookahead == 'T' ||
          lookahead == 't') ADVANCE(133);
      END_STATE();
    case 77:
      if (lookahead == 'T' ||
          lookahead == 't') ADVANCE(135);
      END_STATE();
    case 78:
      if (lookahead == 'U' ||
          lookahead == 'u') ADVANCE(68);
      END_STATE();
    case 79:
      if (lookahead == 'U' ||
          lookahead == 'u') ADVANCE(75);
      END_STATE();
    case 80:
      if (lookahead == 'Y' ||
          lookahead == 'y') ADVANCE(111);
      END_STATE();
    case 81:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(91);
      END_STATE();
    case 82:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(92);
      END_STATE();
    case 83:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(93);
      END_STATE();
    case 84:
      if (eof) ADVANCE(85);
      if (lookahead == '!') ADVANCE(18);
      if (lookahead == '#') ADVANCE(183);
      if (lookahead == '%') ADVANCE(116);
      if (lookahead == '(') ADVANCE(86);
      if (lookahead == ')') ADVANCE(87);
      if (lookahead == '*') ADVANCE(114);
      if (lookahead == '+') ADVANCE(117);
      if (lookahead == ',') ADVANCE(104);
      if (lookahead == '-') ADVANCE(98);
      if (lookahead == '/') ADVANCE(115);
      if (lookahead == ':') ADVANCE(137);
      if (lookahead == '<') ADVANCE(121);
      if (lookahead == '=') ADVANCE(20);
      if (lookahead == '>') ADVANCE(119);
      if (lookahead == '@') ADVANCE(99);
      if (lookahead == 'O') ADVANCE(69);
      if (lookahead == '[') ADVANCE(95);
      if (lookahead == ']') ADVANCE(96);
      if (lookahead == '^') ADVANCE(113);
      if (lookahead == 'o') ADVANCE(28);
      if (lookahead == '{') ADVANCE(103);
      if (lookahead == 'A' ||
          lookahead == 'a') ADVANCE(56);
      if (lookahead == 'B' ||
          lookahead == 'b') ADVANCE(80);
      if (lookahead == 'U' ||
          lookahead == 'u') ADVANCE(57);
      if (lookahead == 'W' ||
          lookahead == 'w') ADVANCE(49);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(84)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(144);
      END_STATE();
    case 85:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 86:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 87:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 88:
      ACCEPT_TOKEN(sym_float_literal);
      END_STATE();
    case 89:
      ACCEPT_TOKEN(sym_float_literal);
      if (lookahead == '.') ADVANCE(81);
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(36);
      if (lookahead == 'X' ||
          lookahead == 'x') ADVANCE(83);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(90);
      END_STATE();
    case 90:
      ACCEPT_TOKEN(sym_float_literal);
      if (lookahead == '.') ADVANCE(81);
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(36);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(90);
      END_STATE();
    case 91:
      ACCEPT_TOKEN(sym_float_literal);
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(36);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(91);
      END_STATE();
    case 92:
      ACCEPT_TOKEN(sym_float_literal);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(92);
      END_STATE();
    case 93:
      ACCEPT_TOKEN(sym_float_literal);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(93);
      END_STATE();
    case 94:
      ACCEPT_TOKEN(sym_float_literal);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 95:
      ACCEPT_TOKEN(anon_sym_LBRACK);
      END_STATE();
    case 96:
      ACCEPT_TOKEN(anon_sym_RBRACK);
      END_STATE();
    case 97:
      ACCEPT_TOKEN(anon_sym_offset);
      END_STATE();
    case 98:
      ACCEPT_TOKEN(anon_sym_DASH);
      END_STATE();
    case 99:
      ACCEPT_TOKEN(anon_sym_AT);
      END_STATE();
    case 100:
      ACCEPT_TOKEN(anon_sym_start_LPAREN_RPAREN);
      END_STATE();
    case 101:
      ACCEPT_TOKEN(anon_sym_end_LPAREN_RPAREN);
      END_STATE();
    case 102:
      ACCEPT_TOKEN(aux_sym_at_token1);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(102);
      END_STATE();
    case 103:
      ACCEPT_TOKEN(anon_sym_LBRACE);
      END_STATE();
    case 104:
      ACCEPT_TOKEN(anon_sym_COMMA);
      END_STATE();
    case 105:
      ACCEPT_TOKEN(anon_sym_RBRACE);
      END_STATE();
    case 106:
      ACCEPT_TOKEN(anon_sym_EQ);
      if (lookahead == '=') ADVANCE(118);
      if (lookahead == '~') ADVANCE(109);
      END_STATE();
    case 107:
      ACCEPT_TOKEN(anon_sym_EQ);
      if (lookahead == '~') ADVANCE(109);
      END_STATE();
    case 108:
      ACCEPT_TOKEN(anon_sym_BANG_EQ);
      END_STATE();
    case 109:
      ACCEPT_TOKEN(anon_sym_EQ_TILDE);
      END_STATE();
    case 110:
      ACCEPT_TOKEN(anon_sym_BANG_TILDE);
      END_STATE();
    case 111:
      ACCEPT_TOKEN(aux_sym_grouping_token1);
      END_STATE();
    case 112:
      ACCEPT_TOKEN(aux_sym_grouping_token2);
      END_STATE();
    case 113:
      ACCEPT_TOKEN(anon_sym_CARET);
      END_STATE();
    case 114:
      ACCEPT_TOKEN(anon_sym_STAR);
      END_STATE();
    case 115:
      ACCEPT_TOKEN(anon_sym_SLASH);
      END_STATE();
    case 116:
      ACCEPT_TOKEN(anon_sym_PERCENT);
      END_STATE();
    case 117:
      ACCEPT_TOKEN(anon_sym_PLUS);
      END_STATE();
    case 118:
      ACCEPT_TOKEN(anon_sym_EQ_EQ);
      END_STATE();
    case 119:
      ACCEPT_TOKEN(anon_sym_GT);
      if (lookahead == '=') ADVANCE(120);
      END_STATE();
    case 120:
      ACCEPT_TOKEN(anon_sym_GT_EQ);
      END_STATE();
    case 121:
      ACCEPT_TOKEN(anon_sym_LT);
      if (lookahead == '=') ADVANCE(122);
      END_STATE();
    case 122:
      ACCEPT_TOKEN(anon_sym_LT_EQ);
      END_STATE();
    case 123:
      ACCEPT_TOKEN(aux_sym_binary_expression_token1);
      END_STATE();
    case 124:
      ACCEPT_TOKEN(aux_sym_binary_expression_token1);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 125:
      ACCEPT_TOKEN(aux_sym_binary_expression_token2);
      END_STATE();
    case 126:
      ACCEPT_TOKEN(aux_sym_binary_expression_token3);
      END_STATE();
    case 127:
      ACCEPT_TOKEN(aux_sym_binary_expression_token4);
      END_STATE();
    case 128:
      ACCEPT_TOKEN(aux_sym_binary_expression_token5);
      END_STATE();
    case 129:
      ACCEPT_TOKEN(aux_sym_binary_grouping_token1);
      END_STATE();
    case 130:
      ACCEPT_TOKEN(aux_sym_binary_grouping_token1);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 131:
      ACCEPT_TOKEN(aux_sym_binary_grouping_token2);
      END_STATE();
    case 132:
      ACCEPT_TOKEN(aux_sym_binary_grouping_token2);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 133:
      ACCEPT_TOKEN(aux_sym_binary_grouping_token3);
      END_STATE();
    case 134:
      ACCEPT_TOKEN(aux_sym_binary_grouping_token3);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 135:
      ACCEPT_TOKEN(aux_sym_binary_grouping_token4);
      END_STATE();
    case 136:
      ACCEPT_TOKEN(aux_sym_binary_grouping_token4);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 137:
      ACCEPT_TOKEN(anon_sym_COLON);
      END_STATE();
    case 138:
      ACCEPT_TOKEN(sym__double_quoted_string);
      END_STATE();
    case 139:
      ACCEPT_TOKEN(sym__double_quoted_string);
      if (lookahead == '"') ADVANCE(138);
      if (lookahead == '\\') ADVANCE(6);
      if (lookahead != 0) ADVANCE(5);
      END_STATE();
    case 140:
      ACCEPT_TOKEN(sym__single_quoted_string);
      END_STATE();
    case 141:
      ACCEPT_TOKEN(sym__single_quoted_string);
      if (lookahead == '\'') ADVANCE(140);
      if (lookahead == '\\') ADVANCE(11);
      if (lookahead != 0) ADVANCE(10);
      END_STATE();
    case 142:
      ACCEPT_TOKEN(sym__backtick_quoted_string);
      END_STATE();
    case 143:
      ACCEPT_TOKEN(sym__backtick_quoted_string);
      if (lookahead == '\\') ADVANCE(22);
      if (lookahead == '`') ADVANCE(142);
      if (lookahead != 0) ADVANCE(21);
      END_STATE();
    case 144:
      ACCEPT_TOKEN(aux_sym__duration_token1);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(144);
      END_STATE();
    case 145:
      ACCEPT_TOKEN(anon_sym_ms);
      END_STATE();
    case 146:
      ACCEPT_TOKEN(anon_sym_s);
      END_STATE();
    case 147:
      ACCEPT_TOKEN(anon_sym_s);
      if (lookahead == 't') ADVANCE(24);
      END_STATE();
    case 148:
      ACCEPT_TOKEN(anon_sym_m);
      if (lookahead == 's') ADVANCE(145);
      END_STATE();
    case 149:
      ACCEPT_TOKEN(anon_sym_h);
      END_STATE();
    case 150:
      ACCEPT_TOKEN(anon_sym_d);
      END_STATE();
    case 151:
      ACCEPT_TOKEN(anon_sym_w);
      END_STATE();
    case 152:
      ACCEPT_TOKEN(anon_sym_w);
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(74);
      END_STATE();
    case 153:
      ACCEPT_TOKEN(anon_sym_y);
      END_STATE();
    case 154:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == '_') ADVANCE(166);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 155:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'A' ||
          lookahead == 'a') ADVANCE(168);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('B' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 156:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(158);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 157:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'F' ||
          lookahead == 'f') ADVANCE(94);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 158:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'F' ||
          lookahead == 'f') ADVANCE(179);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 159:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'G' ||
          lookahead == 'g') ADVANCE(132);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 160:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'G' ||
          lookahead == 'g') ADVANCE(162);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 161:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'G' ||
          lookahead == 'g') ADVANCE(171);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(157);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 162:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'H' ||
          lookahead == 'h') ADVANCE(180);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 163:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(160);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 164:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(170);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 165:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'L' ||
          lookahead == 'l') ADVANCE(124);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 166:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'L' ||
          lookahead == 'l') ADVANCE(156);
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(163);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 167:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(157);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 168:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(94);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 169:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(130);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 170:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(159);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 171:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'N' ||
          lookahead == 'n') ADVANCE(173);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 172:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(165);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 173:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(177);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 174:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(181);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 175:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'O' ||
          lookahead == 'o') ADVANCE(172);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 176:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'P' ||
          lookahead == 'p') ADVANCE(154);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 177:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(164);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 178:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(174);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 179:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'T' ||
          lookahead == 't') ADVANCE(134);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 180:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'T' ||
          lookahead == 't') ADVANCE(136);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 181:
      ACCEPT_TOKEN(sym__identifier);
      if (lookahead == 'U' ||
          lookahead == 'u') ADVANCE(176);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 182:
      ACCEPT_TOKEN(sym__identifier);
      if (('0' <= lookahead && lookahead <= ':') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(182);
      END_STATE();
    case 183:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(183);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 1},
  [2] = {.lex_state = 2},
  [3] = {.lex_state = 4},
  [4] = {.lex_state = 4},
  [5] = {.lex_state = 4},
  [6] = {.lex_state = 4},
  [7] = {.lex_state = 4},
  [8] = {.lex_state = 4},
  [9] = {.lex_state = 1},
  [10] = {.lex_state = 1},
  [11] = {.lex_state = 84},
  [12] = {.lex_state = 84},
  [13] = {.lex_state = 1},
  [14] = {.lex_state = 1},
  [15] = {.lex_state = 1},
  [16] = {.lex_state = 1},
  [17] = {.lex_state = 1},
  [18] = {.lex_state = 1},
  [19] = {.lex_state = 1},
  [20] = {.lex_state = 1},
  [21] = {.lex_state = 1},
  [22] = {.lex_state = 84},
  [23] = {.lex_state = 1},
  [24] = {.lex_state = 84},
  [25] = {.lex_state = 84},
  [26] = {.lex_state = 84},
  [27] = {.lex_state = 84},
  [28] = {.lex_state = 84},
  [29] = {.lex_state = 84},
  [30] = {.lex_state = 84},
  [31] = {.lex_state = 84},
  [32] = {.lex_state = 84},
  [33] = {.lex_state = 84},
  [34] = {.lex_state = 84},
  [35] = {.lex_state = 84},
  [36] = {.lex_state = 84},
  [37] = {.lex_state = 84},
  [38] = {.lex_state = 84},
  [39] = {.lex_state = 84},
  [40] = {.lex_state = 84},
  [41] = {.lex_state = 84},
  [42] = {.lex_state = 84},
  [43] = {.lex_state = 84},
  [44] = {.lex_state = 84},
  [45] = {.lex_state = 84},
  [46] = {.lex_state = 84},
  [47] = {.lex_state = 84},
  [48] = {.lex_state = 84},
  [49] = {.lex_state = 84},
  [50] = {.lex_state = 84},
  [51] = {.lex_state = 84},
  [52] = {.lex_state = 84},
  [53] = {.lex_state = 84},
  [54] = {.lex_state = 84},
  [55] = {.lex_state = 84},
  [56] = {.lex_state = 84},
  [57] = {.lex_state = 84},
  [58] = {.lex_state = 84},
  [59] = {.lex_state = 84},
  [60] = {.lex_state = 84},
  [61] = {.lex_state = 84},
  [62] = {.lex_state = 84},
  [63] = {.lex_state = 84},
  [64] = {.lex_state = 84},
  [65] = {.lex_state = 84},
  [66] = {.lex_state = 84},
  [67] = {.lex_state = 84},
  [68] = {.lex_state = 84},
  [69] = {.lex_state = 84},
  [70] = {.lex_state = 84},
  [71] = {.lex_state = 84},
  [72] = {.lex_state = 84},
  [73] = {.lex_state = 84},
  [74] = {.lex_state = 84},
  [75] = {.lex_state = 3},
  [76] = {.lex_state = 3},
  [77] = {.lex_state = 3},
  [78] = {.lex_state = 3},
  [79] = {.lex_state = 1},
  [80] = {.lex_state = 1},
  [81] = {.lex_state = 1},
  [82] = {.lex_state = 1},
  [83] = {.lex_state = 1},
  [84] = {.lex_state = 1},
  [85] = {.lex_state = 1},
  [86] = {.lex_state = 1},
  [87] = {.lex_state = 1},
  [88] = {.lex_state = 1},
  [89] = {.lex_state = 1},
  [90] = {.lex_state = 8},
  [91] = {.lex_state = 1},
  [92] = {.lex_state = 0},
  [93] = {.lex_state = 84},
  [94] = {.lex_state = 1},
  [95] = {.lex_state = 7},
  [96] = {.lex_state = 7},
  [97] = {.lex_state = 7},
  [98] = {.lex_state = 7},
  [99] = {.lex_state = 0},
  [100] = {.lex_state = 7},
  [101] = {.lex_state = 7},
  [102] = {.lex_state = 84},
  [103] = {.lex_state = 0},
  [104] = {.lex_state = 0},
  [105] = {.lex_state = 7},
  [106] = {.lex_state = 0},
  [107] = {.lex_state = 0},
  [108] = {.lex_state = 0},
  [109] = {.lex_state = 84},
  [110] = {.lex_state = 0},
  [111] = {.lex_state = 0},
  [112] = {.lex_state = 7},
  [113] = {.lex_state = 0},
  [114] = {.lex_state = 0},
  [115] = {.lex_state = 7},
  [116] = {.lex_state = 7},
  [117] = {.lex_state = 84},
  [118] = {.lex_state = 7},
  [119] = {.lex_state = 7},
  [120] = {.lex_state = 0},
  [121] = {.lex_state = 7},
  [122] = {.lex_state = 84},
  [123] = {.lex_state = 0},
  [124] = {.lex_state = 7},
  [125] = {.lex_state = 0},
  [126] = {.lex_state = 7},
  [127] = {.lex_state = 84},
  [128] = {.lex_state = 0},
  [129] = {.lex_state = 7},
  [130] = {.lex_state = 0},
  [131] = {.lex_state = 0},
  [132] = {.lex_state = 7},
  [133] = {.lex_state = 9},
  [134] = {.lex_state = 7},
  [135] = {.lex_state = 0},
  [136] = {.lex_state = 0},
  [137] = {.lex_state = 7},
  [138] = {.lex_state = 0},
  [139] = {.lex_state = 0},
  [140] = {.lex_state = 7},
  [141] = {.lex_state = 84},
  [142] = {.lex_state = 84},
  [143] = {.lex_state = 0},
  [144] = {.lex_state = 84},
  [145] = {.lex_state = 0},
  [146] = {.lex_state = 0},
  [147] = {.lex_state = 0},
  [148] = {.lex_state = 0},
  [149] = {.lex_state = 0},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [anon_sym_RPAREN] = ACTIONS(1),
    [anon_sym_LBRACK] = ACTIONS(1),
    [anon_sym_RBRACK] = ACTIONS(1),
    [anon_sym_offset] = ACTIONS(1),
    [anon_sym_DASH] = ACTIONS(1),
    [anon_sym_AT] = ACTIONS(1),
    [anon_sym_start_LPAREN_RPAREN] = ACTIONS(1),
    [anon_sym_end_LPAREN_RPAREN] = ACTIONS(1),
    [aux_sym_at_token1] = ACTIONS(1),
    [anon_sym_LBRACE] = ACTIONS(1),
    [anon_sym_COMMA] = ACTIONS(1),
    [anon_sym_RBRACE] = ACTIONS(1),
    [anon_sym_EQ] = ACTIONS(1),
    [anon_sym_BANG_EQ] = ACTIONS(1),
    [anon_sym_EQ_TILDE] = ACTIONS(1),
    [anon_sym_BANG_TILDE] = ACTIONS(1),
    [aux_sym_grouping_token1] = ACTIONS(1),
    [aux_sym_grouping_token2] = ACTIONS(1),
    [anon_sym_CARET] = ACTIONS(1),
    [anon_sym_STAR] = ACTIONS(1),
    [anon_sym_SLASH] = ACTIONS(1),
    [anon_sym_PERCENT] = ACTIONS(1),
    [anon_sym_PLUS] = ACTIONS(1),
    [anon_sym_EQ_EQ] = ACTIONS(1),
    [anon_sym_GT] = ACTIONS(1),
    [anon_sym_GT_EQ] = ACTIONS(1),
    [anon_sym_LT] = ACTIONS(1),
    [anon_sym_LT_EQ] = ACTIONS(1),
    [aux_sym_binary_expression_token1] = ACTIONS(1),
    [aux_sym_binary_expression_token2] = ACTIONS(1),
    [aux_sym_binary_expression_token3] = ACTIONS(1),
    [aux_sym_binary_expression_token4] = ACTIONS(1),
    [aux_sym_binary_expression_token5] = ACTIONS(1),
    [aux_sym_binary_grouping_token1] = ACTIONS(1),
    [aux_sym_binary_grouping_token2] = ACTIONS(1),
    [aux_sym_binary_grouping_token3] = ACTIONS(1),
    [aux_sym_binary_grouping_token4] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [sym__double_quoted_string] = ACTIONS(1),
    [sym__single_quoted_string] = ACTIONS(1),
    [sym__backtick_quoted_string] = ACTIONS(1),
    [aux_sym__duration_token1] = ACTIONS(1),
    [anon_sym_ms] = ACTIONS(1),
    [anon_sym_s] = ACTIONS(1),
    [anon_sym_m] = ACTIONS(1),
    [anon_sym_h] = ACTIONS(1),
    [anon_sym_d] = ACTIONS(1),
    [anon_sym_w] = ACTIONS(1),
    [anon_sym_y] = ACTIONS(1),
    [sym_comment] = ACTIONS(3),
  },
  [1] = {
    [sym_query] = STATE(148),
    [sym__query] = STATE(72),
    [sym__query_expression] = STATE(72),
    [sym__literal_expression] = STATE(72),
    [sym_string_literal] = STATE(72),
    [sym__selector_expression] = STATE(72),
    [sym_instant_vector_selector] = STATE(72),
    [sym_range_vector_selector] = STATE(72),
    [sym__series_matcher] = STATE(12),
    [sym_label_selectors] = STATE(12),
    [sym_metric_name] = STATE(26),
    [sym__call_expression] = STATE(72),
    [sym_function_call] = STATE(72),
    [sym_function_name] = STATE(93),
    [sym__operator_expression] = STATE(72),
    [sym_binary_expression] = STATE(72),
    [sym__subquery_expression] = STATE(72),
    [sym_subquery] = STATE(72),
    [sym__quoted_string] = STATE(70),
    [anon_sym_LPAREN] = ACTIONS(5),
    [sym_float_literal] = ACTIONS(7),
    [anon_sym_LBRACE] = ACTIONS(9),
    [sym__double_quoted_string] = ACTIONS(11),
    [sym__single_quoted_string] = ACTIONS(11),
    [sym__backtick_quoted_string] = ACTIONS(11),
    [sym__identifier] = ACTIONS(13),
    [sym_comment] = ACTIONS(3),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 14,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(15), 1,
      sym_float_literal,
    ACTIONS(17), 1,
      aux_sym_binary_expression_token1,
    STATE(14), 1,
      sym_binary_grouping,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    ACTIONS(19), 2,
      aux_sym_binary_grouping_token1,
      aux_sym_binary_grouping_token2,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(52), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [59] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(21), 1,
      sym_float_literal,
    STATE(16), 1,
      sym_binary_grouping,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    ACTIONS(19), 2,
      aux_sym_binary_grouping_token1,
      aux_sym_binary_grouping_token2,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(45), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [115] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(23), 1,
      sym_float_literal,
    STATE(19), 1,
      sym_binary_grouping,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    ACTIONS(19), 2,
      aux_sym_binary_grouping_token1,
      aux_sym_binary_grouping_token2,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(56), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [171] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(25), 1,
      sym_float_literal,
    STATE(23), 1,
      sym_binary_grouping,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    ACTIONS(19), 2,
      aux_sym_binary_grouping_token1,
      aux_sym_binary_grouping_token2,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(58), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [227] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(27), 1,
      sym_float_literal,
    STATE(17), 1,
      sym_binary_grouping,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    ACTIONS(19), 2,
      aux_sym_binary_grouping_token1,
      aux_sym_binary_grouping_token2,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(51), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [283] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(29), 1,
      sym_float_literal,
    STATE(18), 1,
      sym_binary_grouping,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    ACTIONS(19), 2,
      aux_sym_binary_grouping_token1,
      aux_sym_binary_grouping_token2,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(53), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [339] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(31), 1,
      sym_float_literal,
    STATE(20), 1,
      sym_binary_grouping,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    ACTIONS(19), 2,
      aux_sym_binary_grouping_token1,
      aux_sym_binary_grouping_token2,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(54), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [395] = 12,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(33), 1,
      anon_sym_RPAREN,
    ACTIONS(35), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(71), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [447] = 12,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(35), 1,
      sym_float_literal,
    ACTIONS(37), 1,
      anon_sym_RPAREN,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(71), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [499] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(43), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(41), 3,
      anon_sym_LPAREN,
      aux_sym_grouping_token1,
      aux_sym_grouping_token2,
    ACTIONS(39), 21,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [535] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LBRACK,
    ACTIONS(50), 1,
      anon_sym_offset,
    ACTIONS(52), 1,
      anon_sym_AT,
    STATE(22), 1,
      sym_range_selection,
    STATE(34), 1,
      sym_offset,
    STATE(35), 1,
      sym_at,
    STATE(68), 1,
      sym_modifier,
    ACTIONS(54), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(45), 17,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [583] = 12,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(56), 1,
      anon_sym_RPAREN,
    ACTIONS(58), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(63), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [635] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(25), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(58), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [684] = 12,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(60), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(73), 1,
      sym__query,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(74), 12,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [735] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(62), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(48), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [784] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(64), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(59), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [833] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(66), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(55), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [882] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(68), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(62), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [931] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(70), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(49), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [980] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(35), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(71), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [1029] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(50), 1,
      anon_sym_offset,
    ACTIONS(52), 1,
      anon_sym_AT,
    STATE(34), 1,
      sym_offset,
    STATE(35), 1,
      sym_at,
    STATE(66), 1,
      sym_modifier,
    ACTIONS(74), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(72), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1072] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(5), 1,
      anon_sym_LPAREN,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(13), 1,
      sym__identifier,
    ACTIONS(76), 1,
      sym_float_literal,
    STATE(26), 1,
      sym_metric_name,
    STATE(70), 1,
      sym__quoted_string,
    STATE(93), 1,
      sym_function_name,
    STATE(12), 2,
      sym__series_matcher,
      sym_label_selectors,
    ACTIONS(11), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
    STATE(57), 13,
      sym__query,
      sym__query_expression,
      sym__literal_expression,
      sym_string_literal,
      sym__selector_expression,
      sym_instant_vector_selector,
      sym_range_vector_selector,
      sym__call_expression,
      sym_function_call,
      sym__operator_expression,
      sym_binary_expression,
      sym__subquery_expression,
      sym_subquery,
  [1121] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(82), 1,
      aux_sym__duration_token1,
    STATE(24), 1,
      aux_sym__duration,
    ACTIONS(80), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(78), 21,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_RBRACK,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
      anon_sym_COLON,
  [1158] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(50), 1,
      anon_sym_offset,
    ACTIONS(52), 1,
      anon_sym_AT,
    STATE(34), 1,
      sym_offset,
    STATE(35), 1,
      sym_at,
    STATE(69), 1,
      sym_modifier,
    ACTIONS(87), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(85), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1201] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    STATE(36), 1,
      sym_label_selectors,
    ACTIONS(91), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(89), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1237] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(80), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(78), 22,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_RBRACK,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
      anon_sym_COLON,
      aux_sym__duration_token1,
  [1269] = 5,
    ACTIONS(3), 1,
      sym_comment,
    STATE(67), 1,
      sym_grouping,
    ACTIONS(95), 2,
      aux_sym_grouping_token1,
      aux_sym_grouping_token2,
    ACTIONS(97), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(93), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1304] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 1,
      aux_sym__duration_token1,
    STATE(24), 1,
      aux_sym__duration,
    ACTIONS(101), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(99), 19,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1339] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 1,
      aux_sym__duration_token1,
    STATE(24), 1,
      aux_sym__duration,
    ACTIONS(107), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(105), 19,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1374] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(111), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(109), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      aux_sym_grouping_token1,
      aux_sym_grouping_token2,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1404] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(115), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(113), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      aux_sym_grouping_token1,
      aux_sym_grouping_token2,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1434] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(119), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(117), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1464] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(52), 1,
      anon_sym_AT,
    STATE(64), 1,
      sym_at,
    ACTIONS(123), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(121), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1498] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(50), 1,
      anon_sym_offset,
    STATE(64), 1,
      sym_offset,
    ACTIONS(123), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(121), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1532] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(127), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(125), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1562] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(131), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(129), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1592] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(135), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(133), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      aux_sym_grouping_token1,
      aux_sym_grouping_token2,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1622] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(137), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1652] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(143), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(141), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1682] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(147), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(145), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      aux_sym_grouping_token1,
      aux_sym_grouping_token2,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1712] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(151), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(149), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1742] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(153), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1772] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(159), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(157), 20,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_AT,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1802] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(171), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(165), 4,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
    ACTIONS(161), 8,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_COMMA,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1841] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(175), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(173), 19,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_offset,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1870] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(179), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(177), 19,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1899] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(171), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(165), 4,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
    ACTIONS(181), 8,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_COMMA,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1938] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(183), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(181), 17,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [1971] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(187), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(185), 19,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2000] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(189), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(161), 14,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2035] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(189), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(161), 12,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2072] = 4,
    ACTIONS(3), 1,
      sym_comment,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(189), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(161), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2103] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(189), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(161), 17,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2136] = 4,
    ACTIONS(3), 1,
      sym_comment,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(183), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(181), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2167] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(171), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(191), 3,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
    ACTIONS(165), 4,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
    ACTIONS(161), 5,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_COMMA,
      aux_sym_binary_expression_token5,
  [2208] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(195), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(193), 12,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2245] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(183), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(181), 12,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2282] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(183), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(181), 14,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2317] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(199), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(197), 19,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2346] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(203), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(201), 19,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2375] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(171), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(191), 3,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
    ACTIONS(165), 4,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
    ACTIONS(181), 5,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_COMMA,
      aux_sym_binary_expression_token5,
  [2416] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    ACTIONS(205), 1,
      anon_sym_RPAREN,
    ACTIONS(207), 1,
      anon_sym_LBRACK,
    ACTIONS(209), 1,
      anon_sym_COMMA,
    ACTIONS(211), 1,
      aux_sym_binary_expression_token5,
    STATE(25), 1,
      sym_subquery_range_selection,
    STATE(110), 1,
      aux_sym_function_args_repeat1,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(171), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(191), 3,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
    ACTIONS(165), 4,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
  [2465] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(215), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(213), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2493] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(219), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(217), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2521] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(223), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(221), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2549] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(227), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(225), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2577] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(231), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(229), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2605] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(235), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(233), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2633] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(239), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(237), 18,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2661] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    ACTIONS(207), 1,
      anon_sym_LBRACK,
    ACTIONS(211), 1,
      aux_sym_binary_expression_token5,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(171), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(241), 2,
      anon_sym_RPAREN,
      anon_sym_COMMA,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(191), 3,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
    ACTIONS(165), 4,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
  [2705] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    ACTIONS(207), 1,
      anon_sym_LBRACK,
    ACTIONS(211), 1,
      aux_sym_binary_expression_token5,
    ACTIONS(243), 1,
      ts_builtin_sym_end,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(171), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(191), 3,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
    ACTIONS(165), 4,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
  [2748] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_CARET,
    ACTIONS(207), 1,
      anon_sym_LBRACK,
    ACTIONS(211), 1,
      aux_sym_binary_expression_token5,
    STATE(25), 1,
      sym_subquery_range_selection,
    ACTIONS(163), 2,
      anon_sym_DASH,
      anon_sym_PLUS,
    ACTIONS(171), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(169), 3,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
    ACTIONS(191), 3,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
    ACTIONS(165), 4,
      anon_sym_BANG_EQ,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
  [2788] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(245), 1,
      anon_sym_RPAREN,
    ACTIONS(249), 2,
      anon_sym_GT,
      anon_sym_LT,
    ACTIONS(247), 15,
      anon_sym_LBRACK,
      anon_sym_DASH,
      anon_sym_BANG_EQ,
      anon_sym_CARET,
      anon_sym_STAR,
      anon_sym_SLASH,
      anon_sym_PERCENT,
      anon_sym_PLUS,
      anon_sym_EQ_EQ,
      anon_sym_GT_EQ,
      anon_sym_LT_EQ,
      aux_sym_binary_expression_token2,
      aux_sym_binary_expression_token3,
      aux_sym_binary_expression_token4,
      aux_sym_binary_expression_token5,
  [2816] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(253), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(255), 2,
      aux_sym_binary_grouping_token3,
      aux_sym_binary_grouping_token4,
    ACTIONS(251), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [2835] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(259), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(261), 2,
      aux_sym_binary_grouping_token3,
      aux_sym_binary_grouping_token4,
    ACTIONS(257), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [2854] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(265), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(267), 2,
      aux_sym_binary_grouping_token3,
      aux_sym_binary_grouping_token4,
    ACTIONS(263), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [2873] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(271), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(273), 2,
      aux_sym_binary_grouping_token3,
      aux_sym_binary_grouping_token4,
    ACTIONS(269), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [2892] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(275), 1,
      anon_sym_LPAREN,
    ACTIONS(277), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(279), 4,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [2909] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(283), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(281), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [2924] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(285), 1,
      anon_sym_LPAREN,
    ACTIONS(265), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(263), 4,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [2941] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(289), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(287), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [2956] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(291), 1,
      anon_sym_LPAREN,
    ACTIONS(253), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(251), 4,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [2973] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(295), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(293), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [2988] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(299), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(297), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [3003] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(303), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(301), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [3018] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(277), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(279), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [3033] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(305), 1,
      anon_sym_LPAREN,
    ACTIONS(271), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(269), 4,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [3050] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(253), 2,
      sym_float_literal,
      sym__identifier,
    ACTIONS(251), 5,
      anon_sym_LPAREN,
      anon_sym_LBRACE,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [3065] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(309), 1,
      anon_sym_m,
    ACTIONS(307), 6,
      anon_sym_ms,
      anon_sym_s,
      anon_sym_h,
      anon_sym_d,
      anon_sym_w,
      anon_sym_y,
  [3080] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(313), 1,
      anon_sym_EQ,
    ACTIONS(311), 5,
      anon_sym_RPAREN,
      anon_sym_COMMA,
      anon_sym_BANG_EQ,
      anon_sym_EQ_TILDE,
      anon_sym_BANG_TILDE,
  [3094] = 4,
    ACTIONS(3), 1,
      sym_comment,
    STATE(138), 1,
      sym__quoted_string,
    STATE(146), 1,
      sym_label_value,
    ACTIONS(315), 3,
      sym__double_quoted_string,
      sym__single_quoted_string,
      sym__backtick_quoted_string,
  [3109] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(317), 1,
      anon_sym_LPAREN,
    STATE(28), 1,
      sym_function_args,
    STATE(139), 1,
      sym_grouping,
    ACTIONS(95), 2,
      aux_sym_grouping_token1,
      aux_sym_grouping_token2,
  [3126] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(319), 1,
      anon_sym_EQ,
    ACTIONS(321), 3,
      anon_sym_BANG_EQ,
      anon_sym_EQ_TILDE,
      anon_sym_BANG_TILDE,
  [3138] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(323), 1,
      anon_sym_RBRACE,
    ACTIONS(325), 1,
      sym__identifier,
    STATE(94), 1,
      sym_label_name,
    STATE(145), 1,
      sym_label_matcher,
  [3154] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(327), 1,
      anon_sym_RBRACE,
    STATE(94), 1,
      sym_label_name,
    STATE(145), 1,
      sym_label_matcher,
  [3170] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(329), 1,
      anon_sym_RBRACE,
    STATE(94), 1,
      sym_label_name,
    STATE(108), 1,
      sym_label_matcher,
  [3186] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(331), 1,
      anon_sym_RPAREN,
    STATE(143), 1,
      sym_label_name,
  [3199] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(333), 1,
      anon_sym_RPAREN,
    ACTIONS(335), 1,
      anon_sym_COMMA,
    STATE(99), 1,
      aux_sym_grouping_repeat1,
  [3212] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    STATE(94), 1,
      sym_label_name,
    STATE(145), 1,
      sym_label_matcher,
  [3225] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(338), 1,
      anon_sym_RPAREN,
    STATE(107), 1,
      sym_label_name,
  [3238] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 1,
      aux_sym__duration_token1,
    ACTIONS(340), 1,
      anon_sym_RBRACK,
    STATE(24), 1,
      aux_sym__duration,
  [3251] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(241), 1,
      anon_sym_RPAREN,
    ACTIONS(342), 1,
      anon_sym_COMMA,
    STATE(103), 1,
      aux_sym_function_args_repeat1,
  [3264] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(345), 1,
      anon_sym_RPAREN,
    ACTIONS(347), 1,
      anon_sym_COMMA,
    STATE(106), 1,
      aux_sym_grouping_repeat1,
  [3277] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(349), 1,
      anon_sym_RPAREN,
    STATE(143), 1,
      sym_label_name,
  [3290] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(349), 1,
      anon_sym_RPAREN,
    ACTIONS(351), 1,
      anon_sym_COMMA,
    STATE(99), 1,
      aux_sym_grouping_repeat1,
  [3303] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(353), 1,
      anon_sym_RPAREN,
    ACTIONS(355), 1,
      anon_sym_COMMA,
    STATE(111), 1,
      aux_sym_grouping_repeat1,
  [3316] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(357), 1,
      anon_sym_COMMA,
    ACTIONS(359), 1,
      anon_sym_RBRACE,
    STATE(128), 1,
      aux_sym_label_selectors_repeat1,
  [3329] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 1,
      aux_sym__duration_token1,
    ACTIONS(361), 1,
      anon_sym_RBRACK,
    STATE(102), 1,
      aux_sym__duration,
  [3342] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(37), 1,
      anon_sym_RPAREN,
    ACTIONS(363), 1,
      anon_sym_COMMA,
    STATE(103), 1,
      aux_sym_function_args_repeat1,
  [3355] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(331), 1,
      anon_sym_RPAREN,
    ACTIONS(365), 1,
      anon_sym_COMMA,
    STATE(99), 1,
      aux_sym_grouping_repeat1,
  [3368] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(367), 1,
      anon_sym_RPAREN,
    STATE(143), 1,
      sym_label_name,
  [3381] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_COMMA,
    ACTIONS(372), 1,
      anon_sym_RBRACE,
    STATE(113), 1,
      aux_sym_label_selectors_repeat1,
  [3394] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(374), 1,
      anon_sym_RPAREN,
    ACTIONS(376), 1,
      anon_sym_COMMA,
    STATE(99), 1,
      aux_sym_grouping_repeat1,
  [3407] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(374), 1,
      anon_sym_RPAREN,
    STATE(143), 1,
      sym_label_name,
  [3420] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(378), 1,
      anon_sym_RPAREN,
    STATE(143), 1,
      sym_label_name,
  [3433] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 1,
      aux_sym__duration_token1,
    ACTIONS(380), 1,
      anon_sym_DASH,
    STATE(30), 1,
      aux_sym__duration,
  [3446] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(382), 1,
      anon_sym_RPAREN,
    STATE(125), 1,
      sym_label_name,
  [3459] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(384), 1,
      anon_sym_RPAREN,
    STATE(104), 1,
      sym_label_name,
  [3472] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(386), 1,
      anon_sym_RPAREN,
    ACTIONS(388), 1,
      anon_sym_COMMA,
    STATE(114), 1,
      aux_sym_grouping_repeat1,
  [3485] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(390), 1,
      anon_sym_RPAREN,
    STATE(143), 1,
      sym_label_name,
  [3498] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 1,
      aux_sym__duration_token1,
    ACTIONS(392), 1,
      anon_sym_RBRACK,
    STATE(24), 1,
      aux_sym__duration,
  [3511] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(386), 1,
      anon_sym_RPAREN,
    ACTIONS(388), 1,
      anon_sym_COMMA,
    STATE(99), 1,
      aux_sym_grouping_repeat1,
  [3524] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(386), 1,
      anon_sym_RPAREN,
    STATE(143), 1,
      sym_label_name,
  [3537] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(394), 1,
      anon_sym_RPAREN,
    ACTIONS(396), 1,
      anon_sym_COMMA,
    STATE(130), 1,
      aux_sym_grouping_repeat1,
  [3550] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(394), 1,
      anon_sym_RPAREN,
    STATE(131), 1,
      sym_label_name,
  [3563] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 1,
      aux_sym__duration_token1,
    ACTIONS(398), 1,
      anon_sym_COLON,
    STATE(24), 1,
      aux_sym__duration,
  [3576] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(327), 1,
      anon_sym_RBRACE,
    ACTIONS(400), 1,
      anon_sym_COMMA,
    STATE(113), 1,
      aux_sym_label_selectors_repeat1,
  [3589] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(402), 1,
      anon_sym_RPAREN,
    STATE(143), 1,
      sym_label_name,
  [3602] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(402), 1,
      anon_sym_RPAREN,
    ACTIONS(404), 1,
      anon_sym_COMMA,
    STATE(99), 1,
      aux_sym_grouping_repeat1,
  [3615] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(402), 1,
      anon_sym_RPAREN,
    ACTIONS(404), 1,
      anon_sym_COMMA,
    STATE(135), 1,
      aux_sym_grouping_repeat1,
  [3628] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(402), 1,
      anon_sym_RPAREN,
    STATE(136), 1,
      sym_label_name,
  [3641] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(406), 3,
      anon_sym_start_LPAREN_RPAREN,
      anon_sym_end_LPAREN_RPAREN,
      aux_sym_at_token1,
  [3650] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(408), 1,
      anon_sym_RPAREN,
    STATE(143), 1,
      sym_label_name,
  [3663] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(408), 1,
      anon_sym_RPAREN,
    ACTIONS(410), 1,
      anon_sym_COMMA,
    STATE(99), 1,
      aux_sym_grouping_repeat1,
  [3676] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(408), 1,
      anon_sym_RPAREN,
    ACTIONS(410), 1,
      anon_sym_COMMA,
    STATE(123), 1,
      aux_sym_grouping_repeat1,
  [3689] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    ACTIONS(408), 1,
      anon_sym_RPAREN,
    STATE(120), 1,
      sym_label_name,
  [3702] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(412), 2,
      anon_sym_COMMA,
      anon_sym_RBRACE,
  [3710] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(317), 1,
      anon_sym_LPAREN,
    STATE(67), 1,
      sym_function_args,
  [3720] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym__identifier,
    STATE(143), 1,
      sym_label_name,
  [3730] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 1,
      aux_sym__duration_token1,
    STATE(122), 1,
      aux_sym__duration,
  [3740] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 1,
      aux_sym__duration_token1,
    STATE(29), 1,
      aux_sym__duration,
  [3750] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(333), 2,
      anon_sym_RPAREN,
      anon_sym_COMMA,
  [3758] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 1,
      aux_sym__duration_token1,
    STATE(127), 1,
      aux_sym__duration,
  [3768] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(372), 2,
      anon_sym_COMMA,
      anon_sym_RBRACE,
  [3776] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(414), 2,
      anon_sym_COMMA,
      anon_sym_RBRACE,
  [3784] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(416), 1,
      anon_sym_LPAREN,
  [3791] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(418), 1,
      ts_builtin_sym_end,
  [3798] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(420), 1,
      anon_sym_LPAREN,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(2)] = 0,
  [SMALL_STATE(3)] = 59,
  [SMALL_STATE(4)] = 115,
  [SMALL_STATE(5)] = 171,
  [SMALL_STATE(6)] = 227,
  [SMALL_STATE(7)] = 283,
  [SMALL_STATE(8)] = 339,
  [SMALL_STATE(9)] = 395,
  [SMALL_STATE(10)] = 447,
  [SMALL_STATE(11)] = 499,
  [SMALL_STATE(12)] = 535,
  [SMALL_STATE(13)] = 583,
  [SMALL_STATE(14)] = 635,
  [SMALL_STATE(15)] = 684,
  [SMALL_STATE(16)] = 735,
  [SMALL_STATE(17)] = 784,
  [SMALL_STATE(18)] = 833,
  [SMALL_STATE(19)] = 882,
  [SMALL_STATE(20)] = 931,
  [SMALL_STATE(21)] = 980,
  [SMALL_STATE(22)] = 1029,
  [SMALL_STATE(23)] = 1072,
  [SMALL_STATE(24)] = 1121,
  [SMALL_STATE(25)] = 1158,
  [SMALL_STATE(26)] = 1201,
  [SMALL_STATE(27)] = 1237,
  [SMALL_STATE(28)] = 1269,
  [SMALL_STATE(29)] = 1304,
  [SMALL_STATE(30)] = 1339,
  [SMALL_STATE(31)] = 1374,
  [SMALL_STATE(32)] = 1404,
  [SMALL_STATE(33)] = 1434,
  [SMALL_STATE(34)] = 1464,
  [SMALL_STATE(35)] = 1498,
  [SMALL_STATE(36)] = 1532,
  [SMALL_STATE(37)] = 1562,
  [SMALL_STATE(38)] = 1592,
  [SMALL_STATE(39)] = 1622,
  [SMALL_STATE(40)] = 1652,
  [SMALL_STATE(41)] = 1682,
  [SMALL_STATE(42)] = 1712,
  [SMALL_STATE(43)] = 1742,
  [SMALL_STATE(44)] = 1772,
  [SMALL_STATE(45)] = 1802,
  [SMALL_STATE(46)] = 1841,
  [SMALL_STATE(47)] = 1870,
  [SMALL_STATE(48)] = 1899,
  [SMALL_STATE(49)] = 1938,
  [SMALL_STATE(50)] = 1971,
  [SMALL_STATE(51)] = 2000,
  [SMALL_STATE(52)] = 2035,
  [SMALL_STATE(53)] = 2072,
  [SMALL_STATE(54)] = 2103,
  [SMALL_STATE(55)] = 2136,
  [SMALL_STATE(56)] = 2167,
  [SMALL_STATE(57)] = 2208,
  [SMALL_STATE(58)] = 2245,
  [SMALL_STATE(59)] = 2282,
  [SMALL_STATE(60)] = 2317,
  [SMALL_STATE(61)] = 2346,
  [SMALL_STATE(62)] = 2375,
  [SMALL_STATE(63)] = 2416,
  [SMALL_STATE(64)] = 2465,
  [SMALL_STATE(65)] = 2493,
  [SMALL_STATE(66)] = 2521,
  [SMALL_STATE(67)] = 2549,
  [SMALL_STATE(68)] = 2577,
  [SMALL_STATE(69)] = 2605,
  [SMALL_STATE(70)] = 2633,
  [SMALL_STATE(71)] = 2661,
  [SMALL_STATE(72)] = 2705,
  [SMALL_STATE(73)] = 2748,
  [SMALL_STATE(74)] = 2788,
  [SMALL_STATE(75)] = 2816,
  [SMALL_STATE(76)] = 2835,
  [SMALL_STATE(77)] = 2854,
  [SMALL_STATE(78)] = 2873,
  [SMALL_STATE(79)] = 2892,
  [SMALL_STATE(80)] = 2909,
  [SMALL_STATE(81)] = 2924,
  [SMALL_STATE(82)] = 2941,
  [SMALL_STATE(83)] = 2956,
  [SMALL_STATE(84)] = 2973,
  [SMALL_STATE(85)] = 2988,
  [SMALL_STATE(86)] = 3003,
  [SMALL_STATE(87)] = 3018,
  [SMALL_STATE(88)] = 3033,
  [SMALL_STATE(89)] = 3050,
  [SMALL_STATE(90)] = 3065,
  [SMALL_STATE(91)] = 3080,
  [SMALL_STATE(92)] = 3094,
  [SMALL_STATE(93)] = 3109,
  [SMALL_STATE(94)] = 3126,
  [SMALL_STATE(95)] = 3138,
  [SMALL_STATE(96)] = 3154,
  [SMALL_STATE(97)] = 3170,
  [SMALL_STATE(98)] = 3186,
  [SMALL_STATE(99)] = 3199,
  [SMALL_STATE(100)] = 3212,
  [SMALL_STATE(101)] = 3225,
  [SMALL_STATE(102)] = 3238,
  [SMALL_STATE(103)] = 3251,
  [SMALL_STATE(104)] = 3264,
  [SMALL_STATE(105)] = 3277,
  [SMALL_STATE(106)] = 3290,
  [SMALL_STATE(107)] = 3303,
  [SMALL_STATE(108)] = 3316,
  [SMALL_STATE(109)] = 3329,
  [SMALL_STATE(110)] = 3342,
  [SMALL_STATE(111)] = 3355,
  [SMALL_STATE(112)] = 3368,
  [SMALL_STATE(113)] = 3381,
  [SMALL_STATE(114)] = 3394,
  [SMALL_STATE(115)] = 3407,
  [SMALL_STATE(116)] = 3420,
  [SMALL_STATE(117)] = 3433,
  [SMALL_STATE(118)] = 3446,
  [SMALL_STATE(119)] = 3459,
  [SMALL_STATE(120)] = 3472,
  [SMALL_STATE(121)] = 3485,
  [SMALL_STATE(122)] = 3498,
  [SMALL_STATE(123)] = 3511,
  [SMALL_STATE(124)] = 3524,
  [SMALL_STATE(125)] = 3537,
  [SMALL_STATE(126)] = 3550,
  [SMALL_STATE(127)] = 3563,
  [SMALL_STATE(128)] = 3576,
  [SMALL_STATE(129)] = 3589,
  [SMALL_STATE(130)] = 3602,
  [SMALL_STATE(131)] = 3615,
  [SMALL_STATE(132)] = 3628,
  [SMALL_STATE(133)] = 3641,
  [SMALL_STATE(134)] = 3650,
  [SMALL_STATE(135)] = 3663,
  [SMALL_STATE(136)] = 3676,
  [SMALL_STATE(137)] = 3689,
  [SMALL_STATE(138)] = 3702,
  [SMALL_STATE(139)] = 3710,
  [SMALL_STATE(140)] = 3720,
  [SMALL_STATE(141)] = 3730,
  [SMALL_STATE(142)] = 3740,
  [SMALL_STATE(143)] = 3750,
  [SMALL_STATE(144)] = 3758,
  [SMALL_STATE(145)] = 3768,
  [SMALL_STATE(146)] = 3776,
  [SMALL_STATE(147)] = 3784,
  [SMALL_STATE(148)] = 3791,
  [SMALL_STATE(149)] = 3798,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [7] = {.entry = {.count = 1, .reusable = false}}, SHIFT(72),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(97),
  [11] = {.entry = {.count = 1, .reusable = true}}, SHIFT(70),
  [13] = {.entry = {.count = 1, .reusable = false}}, SHIFT(11),
  [15] = {.entry = {.count = 1, .reusable = false}}, SHIFT(52),
  [17] = {.entry = {.count = 1, .reusable = false}}, SHIFT(5),
  [19] = {.entry = {.count = 1, .reusable = false}}, SHIFT(149),
  [21] = {.entry = {.count = 1, .reusable = false}}, SHIFT(45),
  [23] = {.entry = {.count = 1, .reusable = false}}, SHIFT(56),
  [25] = {.entry = {.count = 1, .reusable = false}}, SHIFT(58),
  [27] = {.entry = {.count = 1, .reusable = false}}, SHIFT(51),
  [29] = {.entry = {.count = 1, .reusable = false}}, SHIFT(53),
  [31] = {.entry = {.count = 1, .reusable = false}}, SHIFT(54),
  [33] = {.entry = {.count = 1, .reusable = true}}, SHIFT(31),
  [35] = {.entry = {.count = 1, .reusable = false}}, SHIFT(71),
  [37] = {.entry = {.count = 1, .reusable = true}}, SHIFT(32),
  [39] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_metric_name, 1),
  [41] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_name, 1),
  [43] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_metric_name, 1),
  [45] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_instant_vector_selector, 1),
  [47] = {.entry = {.count = 2, .reusable = true}}, REDUCE(sym_instant_vector_selector, 1), SHIFT(141),
  [50] = {.entry = {.count = 1, .reusable = true}}, SHIFT(117),
  [52] = {.entry = {.count = 1, .reusable = true}}, SHIFT(133),
  [54] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_instant_vector_selector, 1),
  [56] = {.entry = {.count = 1, .reusable = true}}, SHIFT(38),
  [58] = {.entry = {.count = 1, .reusable = false}}, SHIFT(63),
  [60] = {.entry = {.count = 1, .reusable = false}}, SHIFT(74),
  [62] = {.entry = {.count = 1, .reusable = false}}, SHIFT(48),
  [64] = {.entry = {.count = 1, .reusable = false}}, SHIFT(59),
  [66] = {.entry = {.count = 1, .reusable = false}}, SHIFT(55),
  [68] = {.entry = {.count = 1, .reusable = false}}, SHIFT(62),
  [70] = {.entry = {.count = 1, .reusable = false}}, SHIFT(49),
  [72] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_range_vector_selector, 2),
  [74] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_range_vector_selector, 2),
  [76] = {.entry = {.count = 1, .reusable = false}}, SHIFT(57),
  [78] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__duration, 2),
  [80] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__duration, 2),
  [82] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__duration, 2), SHIFT_REPEAT(90),
  [85] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_subquery, 2),
  [87] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_subquery, 2),
  [89] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__series_matcher, 1),
  [91] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__series_matcher, 1),
  [93] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_call, 2),
  [95] = {.entry = {.count = 1, .reusable = true}}, SHIFT(147),
  [97] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_call, 2),
  [99] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_offset, 3),
  [101] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_offset, 3),
  [103] = {.entry = {.count = 1, .reusable = true}}, SHIFT(90),
  [105] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_offset, 2),
  [107] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_offset, 2),
  [109] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_args, 5),
  [111] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_args, 5),
  [113] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_args, 4),
  [115] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_args, 4),
  [117] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_range_selection, 3),
  [119] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_range_selection, 3),
  [121] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_modifier, 1),
  [123] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_modifier, 1),
  [125] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__series_matcher, 2),
  [127] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__series_matcher, 2),
  [129] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label_selectors, 4),
  [131] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_label_selectors, 4),
  [133] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_args, 2),
  [135] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_args, 2),
  [137] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label_selectors, 2),
  [139] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_label_selectors, 2),
  [141] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label_selectors, 3),
  [143] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_label_selectors, 3),
  [145] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_args, 3),
  [147] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_args, 3),
  [149] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label_selectors, 5),
  [151] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_label_selectors, 5),
  [153] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_subquery_range_selection, 4),
  [155] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_subquery_range_selection, 4),
  [157] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_subquery_range_selection, 5),
  [159] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_subquery_range_selection, 5),
  [161] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_expression, 3),
  [163] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [165] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [167] = {.entry = {.count = 1, .reusable = true}}, SHIFT(7),
  [169] = {.entry = {.count = 1, .reusable = true}}, SHIFT(8),
  [171] = {.entry = {.count = 1, .reusable = false}}, SHIFT(2),
  [173] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_at, 2),
  [175] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_at, 2),
  [177] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_grouping, 4),
  [179] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_grouping, 4),
  [181] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_expression, 4),
  [183] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_expression, 4),
  [185] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_grouping, 6),
  [187] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_grouping, 6),
  [189] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_expression, 3),
  [191] = {.entry = {.count = 1, .reusable = true}}, SHIFT(3),
  [193] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_expression, 5),
  [195] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_expression, 5),
  [197] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_grouping, 3),
  [199] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_grouping, 3),
  [201] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_grouping, 5),
  [203] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_grouping, 5),
  [205] = {.entry = {.count = 1, .reusable = true}}, SHIFT(41),
  [207] = {.entry = {.count = 1, .reusable = true}}, SHIFT(144),
  [209] = {.entry = {.count = 1, .reusable = true}}, SHIFT(10),
  [211] = {.entry = {.count = 1, .reusable = true}}, SHIFT(4),
  [213] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_modifier, 2),
  [215] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_modifier, 2),
  [217] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__query, 3),
  [219] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__query, 3),
  [221] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_range_vector_selector, 3),
  [223] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_range_vector_selector, 3),
  [225] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_call, 3),
  [227] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_call, 3),
  [229] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_instant_vector_selector, 2),
  [231] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_instant_vector_selector, 2),
  [233] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_subquery, 3),
  [235] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_subquery, 3),
  [237] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_string_literal, 1),
  [239] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_string_literal, 1),
  [241] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_function_args_repeat1, 2),
  [243] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_query, 1),
  [245] = {.entry = {.count = 1, .reusable = true}}, SHIFT(65),
  [247] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__query, 1),
  [249] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__query, 1),
  [251] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_grouping, 6),
  [253] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_grouping, 6),
  [255] = {.entry = {.count = 1, .reusable = false}}, SHIFT(79),
  [257] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_grouping, 3),
  [259] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_grouping, 3),
  [261] = {.entry = {.count = 1, .reusable = false}}, SHIFT(81),
  [263] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_grouping, 4),
  [265] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_grouping, 4),
  [267] = {.entry = {.count = 1, .reusable = false}}, SHIFT(88),
  [269] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_grouping, 5),
  [271] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_grouping, 5),
  [273] = {.entry = {.count = 1, .reusable = false}}, SHIFT(83),
  [275] = {.entry = {.count = 1, .reusable = true}}, SHIFT(137),
  [277] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_grouping, 7),
  [279] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_grouping, 7),
  [281] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_grouping, 10),
  [283] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_grouping, 10),
  [285] = {.entry = {.count = 1, .reusable = true}}, SHIFT(118),
  [287] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_grouping, 9),
  [289] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_grouping, 9),
  [291] = {.entry = {.count = 1, .reusable = true}}, SHIFT(132),
  [293] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_grouping, 8),
  [295] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_grouping, 8),
  [297] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_grouping, 11),
  [299] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_grouping, 11),
  [301] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_grouping, 12),
  [303] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_grouping, 12),
  [305] = {.entry = {.count = 1, .reusable = true}}, SHIFT(126),
  [307] = {.entry = {.count = 1, .reusable = true}}, SHIFT(27),
  [309] = {.entry = {.count = 1, .reusable = false}}, SHIFT(27),
  [311] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label_name, 1),
  [313] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_label_name, 1),
  [315] = {.entry = {.count = 1, .reusable = true}}, SHIFT(138),
  [317] = {.entry = {.count = 1, .reusable = true}}, SHIFT(13),
  [319] = {.entry = {.count = 1, .reusable = false}}, SHIFT(92),
  [321] = {.entry = {.count = 1, .reusable = true}}, SHIFT(92),
  [323] = {.entry = {.count = 1, .reusable = true}}, SHIFT(42),
  [325] = {.entry = {.count = 1, .reusable = true}}, SHIFT(91),
  [327] = {.entry = {.count = 1, .reusable = true}}, SHIFT(37),
  [329] = {.entry = {.count = 1, .reusable = true}}, SHIFT(39),
  [331] = {.entry = {.count = 1, .reusable = true}}, SHIFT(78),
  [333] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_grouping_repeat1, 2),
  [335] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_grouping_repeat1, 2), SHIFT_REPEAT(140),
  [338] = {.entry = {.count = 1, .reusable = true}}, SHIFT(76),
  [340] = {.entry = {.count = 1, .reusable = true}}, SHIFT(44),
  [342] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_function_args_repeat1, 2), SHIFT_REPEAT(21),
  [345] = {.entry = {.count = 1, .reusable = true}}, SHIFT(47),
  [347] = {.entry = {.count = 1, .reusable = true}}, SHIFT(105),
  [349] = {.entry = {.count = 1, .reusable = true}}, SHIFT(61),
  [351] = {.entry = {.count = 1, .reusable = true}}, SHIFT(116),
  [353] = {.entry = {.count = 1, .reusable = true}}, SHIFT(77),
  [355] = {.entry = {.count = 1, .reusable = true}}, SHIFT(98),
  [357] = {.entry = {.count = 1, .reusable = true}}, SHIFT(96),
  [359] = {.entry = {.count = 1, .reusable = true}}, SHIFT(40),
  [361] = {.entry = {.count = 1, .reusable = true}}, SHIFT(43),
  [363] = {.entry = {.count = 1, .reusable = true}}, SHIFT(9),
  [365] = {.entry = {.count = 1, .reusable = true}}, SHIFT(121),
  [367] = {.entry = {.count = 1, .reusable = true}}, SHIFT(86),
  [369] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_label_selectors_repeat1, 2), SHIFT_REPEAT(100),
  [372] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_label_selectors_repeat1, 2),
  [374] = {.entry = {.count = 1, .reusable = true}}, SHIFT(85),
  [376] = {.entry = {.count = 1, .reusable = true}}, SHIFT(112),
  [378] = {.entry = {.count = 1, .reusable = true}}, SHIFT(50),
  [380] = {.entry = {.count = 1, .reusable = true}}, SHIFT(142),
  [382] = {.entry = {.count = 1, .reusable = true}}, SHIFT(89),
  [384] = {.entry = {.count = 1, .reusable = true}}, SHIFT(60),
  [386] = {.entry = {.count = 1, .reusable = true}}, SHIFT(80),
  [388] = {.entry = {.count = 1, .reusable = true}}, SHIFT(115),
  [390] = {.entry = {.count = 1, .reusable = true}}, SHIFT(75),
  [392] = {.entry = {.count = 1, .reusable = true}}, SHIFT(33),
  [394] = {.entry = {.count = 1, .reusable = true}}, SHIFT(87),
  [396] = {.entry = {.count = 1, .reusable = true}}, SHIFT(129),
  [398] = {.entry = {.count = 1, .reusable = true}}, SHIFT(109),
  [400] = {.entry = {.count = 1, .reusable = true}}, SHIFT(95),
  [402] = {.entry = {.count = 1, .reusable = true}}, SHIFT(84),
  [404] = {.entry = {.count = 1, .reusable = true}}, SHIFT(134),
  [406] = {.entry = {.count = 1, .reusable = true}}, SHIFT(46),
  [408] = {.entry = {.count = 1, .reusable = true}}, SHIFT(82),
  [410] = {.entry = {.count = 1, .reusable = true}}, SHIFT(124),
  [412] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label_value, 1),
  [414] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label_matcher, 3),
  [416] = {.entry = {.count = 1, .reusable = true}}, SHIFT(119),
  [418] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [420] = {.entry = {.count = 1, .reusable = true}}, SHIFT(101),
};

#ifdef __cplusplus
extern "C" {
#endif
#ifdef _WIN32
#define extern __declspec(dllexport)
#endif

extern const TSLanguage *tree_sitter_promql(void) {
  static const TSLanguage language = {
    .version = LANGUAGE_VERSION,
    .symbol_count = SYMBOL_COUNT,
    .alias_count = ALIAS_COUNT,
    .token_count = TOKEN_COUNT,
    .external_token_count = EXTERNAL_TOKEN_COUNT,
    .state_count = STATE_COUNT,
    .large_state_count = LARGE_STATE_COUNT,
    .production_id_count = PRODUCTION_ID_COUNT,
    .field_count = FIELD_COUNT,
    .max_alias_sequence_length = MAX_ALIAS_SEQUENCE_LENGTH,
    .parse_table = &ts_parse_table[0][0],
    .small_parse_table = ts_small_parse_table,
    .small_parse_table_map = ts_small_parse_table_map,
    .parse_actions = ts_parse_actions,
    .symbol_names = ts_symbol_names,
    .symbol_metadata = ts_symbol_metadata,
    .public_symbol_map = ts_symbol_map,
    .alias_map = ts_non_terminal_alias_map,
    .alias_sequences = &ts_alias_sequences[0][0],
    .lex_modes = ts_lex_modes,
    .lex_fn = ts_lex,
    .primary_state_ids = ts_primary_state_ids,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif

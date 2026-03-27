#include "tree_sitter/parser.h"

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#ifdef _MSC_VER
#pragma optimize("", off)
#elif defined(__clang__)
#pragma clang optimize off
#elif defined(__GNUC__)
#pragma GCC optimize ("O0")
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 206
#define LARGE_STATE_COUNT 2
#define SYMBOL_COUNT 69
#define ALIAS_COUNT 1
#define TOKEN_COUNT 47
#define EXTERNAL_TOKEN_COUNT 5
#define FIELD_COUNT 1
#define MAX_ALIAS_SEQUENCE_LENGTH 7
#define PRODUCTION_ID_COUNT 12

enum ts_symbol_identifiers {
  anon_sym_ATbrief = 1,
  anon_sym_BSLASHbrief = 2,
  aux_sym_brief_header_token1 = 3,
  anon_sym_COMMA = 4,
  aux_sym_tag_token1 = 5,
  aux_sym_tag_token2 = 6,
  sym_tag_name_with_argument = 7,
  sym_tag_name_with_multiple_arguments = 8,
  sym_tag_name_with_types = 9,
  sym_tag_name_with_self_types = 10,
  sym_tag_name_with_type = 11,
  sym_tag_name = 12,
  aux_sym_identifier_token1 = 13,
  anon_sym_COLON_COLON = 14,
  anon_sym_TILDE = 15,
  anon_sym_LPAREN = 16,
  anon_sym_RPAREN = 17,
  anon_sym_LBRACK = 18,
  anon_sym_in = 19,
  anon_sym_out = 20,
  anon_sym_inout = 21,
  anon_sym_RBRACK = 22,
  anon_sym_BSLASHa = 23,
  anon_sym_BSLASHc = 24,
  anon_sym_LTa = 25,
  aux_sym_link_token1 = 26,
  anon_sym_GT = 27,
  aux_sym_link_token2 = 28,
  anon_sym_LT_SLASHa_GT = 29,
  sym_function_link = 30,
  anon_sym_ATcode = 31,
  anon_sym_LBRACE = 32,
  anon_sym_DOT = 33,
  anon_sym_RBRACE = 34,
  anon_sym_ATendcode = 35,
  sym__text = 36,
  sym__singleline_begin = 37,
  sym__multiline_begin = 38,
  anon_sym_SLASH = 39,
  anon_sym_STAR_SLASH = 40,
  sym__text_line = 41,
  sym_brief_text = 42,
  sym_code_block_start = 43,
  sym_code_block_language = 44,
  sym_code_block_content = 45,
  sym_code_block_end = 46,
  sym_document = 47,
  sym_brief_header = 48,
  sym_brief_description = 49,
  sym_description = 50,
  sym_tag = 51,
  sym__expression = 52,
  sym_identifier = 53,
  sym_qualified_identifier = 54,
  sym_function = 55,
  sym_storageclass = 56,
  sym_emphasis = 57,
  sym_code_word = 58,
  sym_link = 59,
  sym_code_block = 60,
  sym__multiline_end = 61,
  aux_sym_document_repeat1 = 62,
  aux_sym_document_repeat2 = 63,
  aux_sym_brief_description_repeat1 = 64,
  aux_sym_description_repeat1 = 65,
  aux_sym_tag_repeat1 = 66,
  aux_sym_tag_repeat2 = 67,
  aux_sym_qualified_identifier_repeat1 = 68,
  alias_sym_code = 69,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [anon_sym_ATbrief] = "tag_name",
  [anon_sym_BSLASHbrief] = "tag_name",
  [aux_sym_brief_header_token1] = "brief_description",
  [anon_sym_COMMA] = ",",
  [aux_sym_tag_token1] = "tag_token1",
  [aux_sym_tag_token2] = "type",
  [sym_tag_name_with_argument] = "tag_name",
  [sym_tag_name_with_multiple_arguments] = "tag_name",
  [sym_tag_name_with_types] = "tag_name",
  [sym_tag_name_with_self_types] = "tag_name",
  [sym_tag_name_with_type] = "tag_name",
  [sym_tag_name] = "tag_name",
  [aux_sym_identifier_token1] = "identifier_token1",
  [anon_sym_COLON_COLON] = "::",
  [anon_sym_TILDE] = "~",
  [anon_sym_LPAREN] = "(",
  [anon_sym_RPAREN] = ")",
  [anon_sym_LBRACK] = "[",
  [anon_sym_in] = "in",
  [anon_sym_out] = "out",
  [anon_sym_inout] = "inout",
  [anon_sym_RBRACK] = "]",
  [anon_sym_BSLASHa] = "\\a",
  [anon_sym_BSLASHc] = "\\c",
  [anon_sym_LTa] = "<a",
  [aux_sym_link_token1] = "link_token1",
  [anon_sym_GT] = ">",
  [aux_sym_link_token2] = "text",
  [anon_sym_LT_SLASHa_GT] = "</a>",
  [sym_function_link] = "function_link",
  [anon_sym_ATcode] = "@code",
  [anon_sym_LBRACE] = "{",
  [anon_sym_DOT] = ".",
  [anon_sym_RBRACE] = "}",
  [anon_sym_ATendcode] = "@endcode",
  [sym__text] = "_text",
  [sym__singleline_begin] = "_singleline_begin",
  [sym__multiline_begin] = "_multiline_begin",
  [anon_sym_SLASH] = "/",
  [anon_sym_STAR_SLASH] = "*/",
  [sym__text_line] = "_text_line",
  [sym_brief_text] = "brief_text",
  [sym_code_block_start] = "code_block_start",
  [sym_code_block_language] = "code_block_language",
  [sym_code_block_content] = "code_block_content",
  [sym_code_block_end] = "code_block_end",
  [sym_document] = "document",
  [sym_brief_header] = "brief_header",
  [sym_brief_description] = "brief_description",
  [sym_description] = "description",
  [sym_tag] = "tag",
  [sym__expression] = "_expression",
  [sym_identifier] = "identifier",
  [sym_qualified_identifier] = "qualified_identifier",
  [sym_function] = "function",
  [sym_storageclass] = "storageclass",
  [sym_emphasis] = "emphasis",
  [sym_code_word] = "code_word",
  [sym_link] = "link",
  [sym_code_block] = "code_block",
  [sym__multiline_end] = "_multiline_end",
  [aux_sym_document_repeat1] = "document_repeat1",
  [aux_sym_document_repeat2] = "document_repeat2",
  [aux_sym_brief_description_repeat1] = "brief_description_repeat1",
  [aux_sym_description_repeat1] = "description_repeat1",
  [aux_sym_tag_repeat1] = "tag_repeat1",
  [aux_sym_tag_repeat2] = "tag_repeat2",
  [aux_sym_qualified_identifier_repeat1] = "qualified_identifier_repeat1",
  [alias_sym_code] = "code",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [anon_sym_ATbrief] = sym_tag_name,
  [anon_sym_BSLASHbrief] = sym_tag_name,
  [aux_sym_brief_header_token1] = sym_brief_description,
  [anon_sym_COMMA] = anon_sym_COMMA,
  [aux_sym_tag_token1] = aux_sym_tag_token1,
  [aux_sym_tag_token2] = aux_sym_tag_token2,
  [sym_tag_name_with_argument] = sym_tag_name,
  [sym_tag_name_with_multiple_arguments] = sym_tag_name,
  [sym_tag_name_with_types] = sym_tag_name,
  [sym_tag_name_with_self_types] = sym_tag_name,
  [sym_tag_name_with_type] = sym_tag_name,
  [sym_tag_name] = sym_tag_name,
  [aux_sym_identifier_token1] = aux_sym_identifier_token1,
  [anon_sym_COLON_COLON] = anon_sym_COLON_COLON,
  [anon_sym_TILDE] = anon_sym_TILDE,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [anon_sym_RPAREN] = anon_sym_RPAREN,
  [anon_sym_LBRACK] = anon_sym_LBRACK,
  [anon_sym_in] = anon_sym_in,
  [anon_sym_out] = anon_sym_out,
  [anon_sym_inout] = anon_sym_inout,
  [anon_sym_RBRACK] = anon_sym_RBRACK,
  [anon_sym_BSLASHa] = anon_sym_BSLASHa,
  [anon_sym_BSLASHc] = anon_sym_BSLASHc,
  [anon_sym_LTa] = anon_sym_LTa,
  [aux_sym_link_token1] = aux_sym_link_token1,
  [anon_sym_GT] = anon_sym_GT,
  [aux_sym_link_token2] = aux_sym_link_token2,
  [anon_sym_LT_SLASHa_GT] = anon_sym_LT_SLASHa_GT,
  [sym_function_link] = sym_function_link,
  [anon_sym_ATcode] = anon_sym_ATcode,
  [anon_sym_LBRACE] = anon_sym_LBRACE,
  [anon_sym_DOT] = anon_sym_DOT,
  [anon_sym_RBRACE] = anon_sym_RBRACE,
  [anon_sym_ATendcode] = anon_sym_ATendcode,
  [sym__text] = sym__text,
  [sym__singleline_begin] = sym__singleline_begin,
  [sym__multiline_begin] = sym__multiline_begin,
  [anon_sym_SLASH] = anon_sym_SLASH,
  [anon_sym_STAR_SLASH] = anon_sym_STAR_SLASH,
  [sym__text_line] = sym__text_line,
  [sym_brief_text] = sym_brief_text,
  [sym_code_block_start] = sym_code_block_start,
  [sym_code_block_language] = sym_code_block_language,
  [sym_code_block_content] = sym_code_block_content,
  [sym_code_block_end] = sym_code_block_end,
  [sym_document] = sym_document,
  [sym_brief_header] = sym_brief_header,
  [sym_brief_description] = sym_brief_description,
  [sym_description] = sym_description,
  [sym_tag] = sym_tag,
  [sym__expression] = sym__expression,
  [sym_identifier] = sym_identifier,
  [sym_qualified_identifier] = sym_qualified_identifier,
  [sym_function] = sym_function,
  [sym_storageclass] = sym_storageclass,
  [sym_emphasis] = sym_emphasis,
  [sym_code_word] = sym_code_word,
  [sym_link] = sym_link,
  [sym_code_block] = sym_code_block,
  [sym__multiline_end] = sym__multiline_end,
  [aux_sym_document_repeat1] = aux_sym_document_repeat1,
  [aux_sym_document_repeat2] = aux_sym_document_repeat2,
  [aux_sym_brief_description_repeat1] = aux_sym_brief_description_repeat1,
  [aux_sym_description_repeat1] = aux_sym_description_repeat1,
  [aux_sym_tag_repeat1] = aux_sym_tag_repeat1,
  [aux_sym_tag_repeat2] = aux_sym_tag_repeat2,
  [aux_sym_qualified_identifier_repeat1] = aux_sym_qualified_identifier_repeat1,
  [alias_sym_code] = alias_sym_code,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [anon_sym_ATbrief] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_BSLASHbrief] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_brief_header_token1] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_COMMA] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_tag_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_tag_token2] = {
    .visible = true,
    .named = true,
  },
  [sym_tag_name_with_argument] = {
    .visible = true,
    .named = true,
  },
  [sym_tag_name_with_multiple_arguments] = {
    .visible = true,
    .named = true,
  },
  [sym_tag_name_with_types] = {
    .visible = true,
    .named = true,
  },
  [sym_tag_name_with_self_types] = {
    .visible = true,
    .named = true,
  },
  [sym_tag_name_with_type] = {
    .visible = true,
    .named = true,
  },
  [sym_tag_name] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_identifier_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_COLON_COLON] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_TILDE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LBRACK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_in] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_out] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_inout] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BSLASHa] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BSLASHc] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LTa] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_link_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_GT] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_link_token2] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_LT_SLASHa_GT] = {
    .visible = true,
    .named = false,
  },
  [sym_function_link] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_ATcode] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DOT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ATendcode] = {
    .visible = true,
    .named = false,
  },
  [sym__text] = {
    .visible = false,
    .named = true,
  },
  [sym__singleline_begin] = {
    .visible = false,
    .named = true,
  },
  [sym__multiline_begin] = {
    .visible = false,
    .named = true,
  },
  [anon_sym_SLASH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_STAR_SLASH] = {
    .visible = true,
    .named = false,
  },
  [sym__text_line] = {
    .visible = false,
    .named = true,
  },
  [sym_brief_text] = {
    .visible = true,
    .named = true,
  },
  [sym_code_block_start] = {
    .visible = true,
    .named = true,
  },
  [sym_code_block_language] = {
    .visible = true,
    .named = true,
  },
  [sym_code_block_content] = {
    .visible = true,
    .named = true,
  },
  [sym_code_block_end] = {
    .visible = true,
    .named = true,
  },
  [sym_document] = {
    .visible = true,
    .named = true,
  },
  [sym_brief_header] = {
    .visible = true,
    .named = true,
  },
  [sym_brief_description] = {
    .visible = true,
    .named = true,
  },
  [sym_description] = {
    .visible = true,
    .named = true,
  },
  [sym_tag] = {
    .visible = true,
    .named = true,
  },
  [sym__expression] = {
    .visible = false,
    .named = true,
  },
  [sym_identifier] = {
    .visible = true,
    .named = true,
  },
  [sym_qualified_identifier] = {
    .visible = true,
    .named = true,
  },
  [sym_function] = {
    .visible = true,
    .named = true,
  },
  [sym_storageclass] = {
    .visible = true,
    .named = true,
  },
  [sym_emphasis] = {
    .visible = true,
    .named = true,
  },
  [sym_code_word] = {
    .visible = true,
    .named = true,
  },
  [sym_link] = {
    .visible = true,
    .named = true,
  },
  [sym_code_block] = {
    .visible = true,
    .named = true,
  },
  [sym__multiline_end] = {
    .visible = false,
    .named = true,
  },
  [aux_sym_document_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_document_repeat2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_brief_description_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_description_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_tag_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_tag_repeat2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_qualified_identifier_repeat1] = {
    .visible = false,
    .named = false,
  },
  [alias_sym_code] = {
    .visible = true,
    .named = true,
  },
};

enum ts_field_identifiers {
  field_function = 1,
};

static const char * const ts_field_names[] = {
  [0] = NULL,
  [field_function] = "function",
};

static const TSFieldMapSlice ts_field_map_slices[PRODUCTION_ID_COUNT] = {
  [1] = {.index = 0, .length = 1},
  [5] = {.index = 1, .length = 2},
  [6] = {.index = 3, .length = 2},
  [7] = {.index = 5, .length = 2},
  [8] = {.index = 7, .length = 2},
  [9] = {.index = 9, .length = 3},
  [10] = {.index = 12, .length = 2},
  [11] = {.index = 14, .length = 3},
};

static const TSFieldMapEntry ts_field_map_entries[] = {
  [0] =
    {field_function, 1},
  [1] =
    {field_function, 1},
    {field_function, 2},
  [3] =
    {field_function, 1},
    {field_function, 2, .inherited = true},
  [5] =
    {field_function, 1},
    {field_function, 3},
  [7] =
    {field_function, 1},
    {field_function, 3, .inherited = true},
  [9] =
    {field_function, 1},
    {field_function, 2, .inherited = true},
    {field_function, 3},
  [12] =
    {field_function, 0, .inherited = true},
    {field_function, 1, .inherited = true},
  [14] =
    {field_function, 1},
    {field_function, 3, .inherited = true},
    {field_function, 4},
};

static const TSSymbol ts_alias_sequences[PRODUCTION_ID_COUNT][MAX_ALIAS_SEQUENCE_LENGTH] = {
  [0] = {0},
  [2] = {
    [1] = aux_sym_tag_token2,
  },
  [3] = {
    [1] = aux_sym_link_token2,
  },
  [4] = {
    [1] = alias_sym_code,
  },
};

static const uint16_t ts_non_terminal_alias_map[] = {
  sym_identifier, 2,
    sym_identifier,
    aux_sym_tag_token2,
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
  [8] = 3,
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
  [19] = 6,
  [20] = 9,
  [21] = 10,
  [22] = 17,
  [23] = 7,
  [24] = 24,
  [25] = 25,
  [26] = 11,
  [27] = 27,
  [28] = 24,
  [29] = 29,
  [30] = 27,
  [31] = 31,
  [32] = 32,
  [33] = 27,
  [34] = 25,
  [35] = 35,
  [36] = 13,
  [37] = 12,
  [38] = 14,
  [39] = 32,
  [40] = 40,
  [41] = 41,
  [42] = 42,
  [43] = 43,
  [44] = 27,
  [45] = 45,
  [46] = 46,
  [47] = 47,
  [48] = 48,
  [49] = 49,
  [50] = 16,
  [51] = 51,
  [52] = 52,
  [53] = 53,
  [54] = 54,
  [55] = 55,
  [56] = 56,
  [57] = 57,
  [58] = 27,
  [59] = 15,
  [60] = 60,
  [61] = 61,
  [62] = 42,
  [63] = 40,
  [64] = 17,
  [65] = 65,
  [66] = 66,
  [67] = 24,
  [68] = 27,
  [69] = 27,
  [70] = 17,
  [71] = 71,
  [72] = 25,
  [73] = 25,
  [74] = 24,
  [75] = 75,
  [76] = 32,
  [77] = 27,
  [78] = 27,
  [79] = 29,
  [80] = 31,
  [81] = 35,
  [82] = 42,
  [83] = 27,
  [84] = 32,
  [85] = 43,
  [86] = 40,
  [87] = 41,
  [88] = 48,
  [89] = 60,
  [90] = 56,
  [91] = 53,
  [92] = 49,
  [93] = 27,
  [94] = 40,
  [95] = 47,
  [96] = 57,
  [97] = 61,
  [98] = 42,
  [99] = 55,
  [100] = 100,
  [101] = 101,
  [102] = 65,
  [103] = 103,
  [104] = 104,
  [105] = 105,
  [106] = 106,
  [107] = 66,
  [108] = 108,
  [109] = 109,
  [110] = 27,
  [111] = 111,
  [112] = 112,
  [113] = 113,
  [114] = 114,
  [115] = 115,
  [116] = 27,
  [117] = 117,
  [118] = 118,
  [119] = 119,
  [120] = 120,
  [121] = 75,
  [122] = 122,
  [123] = 123,
  [124] = 124,
  [125] = 125,
  [126] = 113,
  [127] = 118,
  [128] = 111,
  [129] = 115,
  [130] = 130,
  [131] = 101,
  [132] = 100,
  [133] = 105,
  [134] = 103,
  [135] = 130,
  [136] = 114,
  [137] = 117,
  [138] = 106,
  [139] = 109,
  [140] = 140,
  [141] = 141,
  [142] = 140,
  [143] = 141,
  [144] = 144,
  [145] = 144,
  [146] = 146,
  [147] = 147,
  [148] = 147,
  [149] = 147,
  [150] = 146,
  [151] = 151,
  [152] = 151,
  [153] = 153,
  [154] = 153,
  [155] = 153,
  [156] = 156,
  [157] = 157,
  [158] = 57,
  [159] = 157,
  [160] = 153,
  [161] = 157,
  [162] = 157,
  [163] = 163,
  [164] = 164,
  [165] = 165,
  [166] = 166,
  [167] = 167,
  [168] = 168,
  [169] = 169,
  [170] = 170,
  [171] = 171,
  [172] = 172,
  [173] = 173,
  [174] = 174,
  [175] = 175,
  [176] = 176,
  [177] = 166,
  [178] = 165,
  [179] = 179,
  [180] = 180,
  [181] = 164,
  [182] = 182,
  [183] = 183,
  [184] = 179,
  [185] = 185,
  [186] = 186,
  [187] = 174,
  [188] = 164,
  [189] = 183,
  [190] = 183,
  [191] = 186,
  [192] = 192,
  [193] = 183,
  [194] = 186,
  [195] = 195,
  [196] = 169,
  [197] = 192,
  [198] = 198,
  [199] = 169,
  [200] = 169,
  [201] = 201,
  [202] = 176,
  [203] = 203,
  [204] = 186,
  [205] = 182,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(140);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == ')') ADVANCE(280);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '.') ADVANCE(315);
      if (lookahead == '/') ADVANCE(393);
      if (lookahead == ':') ADVANCE(343);
      if (lookahead == '<') ADVANCE(356);
      if (lookahead == '>') ADVANCE(297);
      if (lookahead == '@') ADVANCE(44);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(45);
      if (lookahead == ']') ADVANCE(288);
      if (lookahead == 'i') ADVANCE(263);
      if (lookahead == 'o') ADVANCE(266);
      if (lookahead == '{') ADVANCE(314);
      if (lookahead == '}') ADVANCE(316);
      if (lookahead == '~') ADVANCE(277);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(139);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 1:
      if (lookahead == '\n') SKIP(2);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == ')') ADVANCE(280);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(393);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(356);
      if (lookahead == '>') ADVANCE(297);
      if (lookahead == '@') ADVANCE(44);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(45);
      if (lookahead == ']') ADVANCE(288);
      if (lookahead == 'i') ADVANCE(263);
      if (lookahead == 'o') ADVANCE(266);
      if (lookahead == '~') ADVANCE(277);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(1);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 2:
      if (lookahead == '\n') SKIP(2);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == ')') ADVANCE(280);
      if (lookahead == '*') ADVANCE(2);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(393);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(356);
      if (lookahead == '>') ADVANCE(297);
      if (lookahead == '@') ADVANCE(44);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(45);
      if (lookahead == ']') ADVANCE(288);
      if (lookahead == 'i') ADVANCE(263);
      if (lookahead == 'o') ADVANCE(266);
      if (lookahead == '~') ADVANCE(277);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(2);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(1);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 3:
      if (lookahead == '\n') SKIP(4);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == '/') ADVANCE(392);
      if (lookahead == ':') ADVANCE(327);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(50);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(54);
      if (lookahead == '~') ADVANCE(328);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(399);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(3);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(319);
      if (lookahead != 0) ADVANCE(329);
      END_STATE();
    case 4:
      if (lookahead == '\n') SKIP(4);
      if (lookahead == '*') ADVANCE(4);
      if (lookahead == '/') ADVANCE(392);
      if (lookahead == ':') ADVANCE(327);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(50);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(54);
      if (lookahead == '~') ADVANCE(328);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(4);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(399);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(3);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(319);
      if (lookahead != 0) ADVANCE(329);
      END_STATE();
    case 5:
      ADVANCE_MAP(
        '\n', 38,
        ')', 309,
        '*', 333,
        '.', 173,
        '<', 34,
        '\\', 6,
        '{', 6,
        '}', 6,
      );
      if (lookahead != 0) ADVANCE(5);
      END_STATE();
    case 6:
      if (lookahead == '\n') ADVANCE(38);
      if (lookahead == ')') ADVANCE(310);
      if (lookahead == '.') ADVANCE(175);
      if (lookahead == '<') ADVANCE(35);
      if (lookahead != 0) ADVANCE(6);
      END_STATE();
    case 7:
      if (lookahead == '\n') SKIP(9);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(343);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(8);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 8:
      if (lookahead == '\n') SKIP(9);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(8);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 9:
      if (lookahead == '\n') SKIP(9);
      if (lookahead == '*') ADVANCE(9);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(9);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(8);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 10:
      if (lookahead == '\n') SKIP(12);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(343);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(11);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 11:
      if (lookahead == '\n') SKIP(12);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(11);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 12:
      if (lookahead == '\n') SKIP(12);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == '*') ADVANCE(12);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(12);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(11);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 13:
      if (lookahead == '\n') SKIP(15);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(343);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(14);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 14:
      if (lookahead == '\n') SKIP(15);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(14);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 15:
      if (lookahead == '\n') SKIP(15);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == '*') ADVANCE(15);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(15);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(14);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 16:
      if (lookahead == '\n') SKIP(17);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(404);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (lookahead == '~') ADVANCE(409);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(16);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(262);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(410);
      END_STATE();
    case 17:
      if (lookahead == '\n') SKIP(17);
      if (lookahead == '*') ADVANCE(17);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(404);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (lookahead == '~') ADVANCE(409);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(17);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(16);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(262);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(410);
      END_STATE();
    case 18:
      if (lookahead == '\n') SKIP(19);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(18);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(410);
      END_STATE();
    case 19:
      if (lookahead == '\n') SKIP(19);
      if (lookahead == '*') ADVANCE(19);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(19);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(18);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(410);
      END_STATE();
    case 20:
      if (lookahead == '\n') SKIP(20);
      if (lookahead == '*') SKIP(20);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '~') ADVANCE(276);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(20);
      if (lookahead == '@' ||
          lookahead == '\\') ADVANCE(133);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(21);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(273);
      END_STATE();
    case 21:
      if (lookahead == '\n') SKIP(20);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '~') ADVANCE(276);
      if (lookahead == '@' ||
          lookahead == '\\') ADVANCE(133);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(21);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(273);
      END_STATE();
    case 22:
      if (lookahead == '\n') SKIP(22);
      if (lookahead == '*') SKIP(22);
      if (lookahead == 'i') ADVANCE(97);
      if (lookahead == 'o') ADVANCE(126);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(22);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(23);
      END_STATE();
    case 23:
      if (lookahead == '\n') SKIP(22);
      if (lookahead == 'i') ADVANCE(97);
      if (lookahead == 'o') ADVANCE(126);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(23);
      END_STATE();
    case 24:
      if (lookahead == '\n') SKIP(25);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(277);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(24);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 25:
      if (lookahead == '\n') SKIP(25);
      if (lookahead == '*') ADVANCE(25);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(277);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(25);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(24);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 26:
      if (lookahead == '\n') SKIP(27);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(404);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (lookahead == '~') ADVANCE(409);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(26);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(269);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(410);
      END_STATE();
    case 27:
      if (lookahead == '\n') SKIP(27);
      if (lookahead == '*') ADVANCE(27);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(404);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (lookahead == '~') ADVANCE(409);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(27);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(26);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(269);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(410);
      END_STATE();
    case 28:
      if (lookahead == '\n') SKIP(30);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(343);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(29);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 29:
      if (lookahead == '\n') SKIP(30);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(29);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 30:
      if (lookahead == '\n') SKIP(30);
      if (lookahead == '*') ADVANCE(30);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(30);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(29);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(336);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 31:
      if (lookahead == '\n') SKIP(32);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(31);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 32:
      if (lookahead == '\n') SKIP(32);
      if (lookahead == '*') ADVANCE(32);
      if (lookahead == '/') ADVANCE(391);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(52);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(345);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(32);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(31);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 33:
      if (lookahead == '(') ADVANCE(35);
      if (lookahead == ':') ADVANCE(42);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(33);
      END_STATE();
    case 34:
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(378);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(35);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(34);
      END_STATE();
    case 35:
      if (lookahead == ')') ADVANCE(302);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(35);
      END_STATE();
    case 36:
      if (lookahead == '*') ADVANCE(380);
      if (lookahead == '.') ADVANCE(169);
      if (lookahead == '<') ADVANCE(37);
      if (lookahead == '\n' ||
          lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(38);
      if (lookahead != 0) ADVANCE(36);
      END_STATE();
    case 37:
      if (lookahead == '*') ADVANCE(381);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(37);
      END_STATE();
    case 38:
      if (lookahead == '.') ADVANCE(169);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(38);
      END_STATE();
    case 39:
      if (lookahead == '/') ADVANCE(394);
      END_STATE();
    case 40:
      if (lookahead == '/') ADVANCE(53);
      END_STATE();
    case 41:
      if (lookahead == ':') ADVANCE(134);
      END_STATE();
    case 42:
      if (lookahead == ':') ADVANCE(132);
      END_STATE();
    case 43:
      if (lookahead == '>') ADVANCE(301);
      END_STATE();
    case 44:
      ADVANCE_MAP(
        'a', 66,
        'b', 110,
        'c', 91,
        'd', 71,
        'e', 98,
        'f', 95,
        'n', 46,
        'o', 128,
        'p', 56,
        's', 47,
        't', 131,
        'v', 58,
      );
      END_STATE();
    case 45:
      ADVANCE_MAP(
        'a', 289,
        'b', 116,
        'c', 291,
        'd', 71,
        'e', 129,
        'f', 95,
        'n', 46,
        'o', 128,
        'p', 56,
        's', 47,
        't', 131,
        'v', 58,
      );
      END_STATE();
    case 46:
      if (lookahead == 'a') ADVANCE(94);
      END_STATE();
    case 47:
      if (lookahead == 'a') ADVANCE(185);
      END_STATE();
    case 48:
      if (lookahead == 'a') ADVANCE(118);
      END_STATE();
    case 49:
      ADVANCE_MAP(
        'a', 204,
        'b', 245,
        'c', 227,
        'd', 207,
        'e', 258,
        'f', 231,
        'n', 192,
        'o', 257,
        'p', 193,
        's', 194,
        't', 260,
        'v', 197,
        '~', 135,
        '{', 191,
        '}', 191,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('g' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 50:
      ADVANCE_MAP(
        'a', 204,
        'b', 245,
        'c', 226,
        'd', 207,
        'e', 258,
        'f', 231,
        'n', 192,
        'o', 257,
        'p', 193,
        's', 194,
        't', 260,
        'v', 197,
        '~', 135,
        '{', 191,
        '}', 191,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('g' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 51:
      ADVANCE_MAP(
        'a', 204,
        'c', 227,
        'd', 207,
        'e', 258,
        'f', 231,
        'n', 192,
        'o', 257,
        'p', 193,
        's', 194,
        't', 260,
        'v', 197,
        '~', 135,
        '{', 191,
        '}', 191,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 52:
      ADVANCE_MAP(
        'a', 204,
        'c', 226,
        'd', 207,
        'e', 258,
        'f', 231,
        'n', 192,
        'o', 257,
        'p', 193,
        's', 194,
        't', 260,
        'v', 197,
        '~', 135,
        '{', 191,
        '}', 191,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 53:
      if (lookahead == 'a') ADVANCE(43);
      END_STATE();
    case 54:
      ADVANCE_MAP(
        'a', 290,
        'b', 249,
        'c', 292,
        'd', 207,
        'e', 258,
        'f', 231,
        'n', 192,
        'o', 257,
        'p', 193,
        's', 194,
        't', 260,
        'v', 197,
        '~', 135,
        '{', 191,
        '}', 191,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('g' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 55:
      ADVANCE_MAP(
        'a', 290,
        'c', 292,
        'd', 207,
        'e', 258,
        'f', 231,
        'n', 192,
        'o', 257,
        'p', 193,
        's', 194,
        't', 260,
        'v', 197,
        '~', 135,
        '{', 191,
        '}', 191,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 56:
      if (lookahead == 'a') ADVANCE(115);
      if (lookahead == 'r') ADVANCE(103);
      END_STATE();
    case 57:
      if (lookahead == 'a') ADVANCE(93);
      END_STATE();
    case 58:
      if (lookahead == 'a') ADVANCE(111);
      END_STATE();
    case 59:
      if (lookahead == 'a') ADVANCE(64);
      END_STATE();
    case 60:
      if (lookahead == 'a') ADVANCE(63);
      END_STATE();
    case 61:
      if (lookahead == 'c') ADVANCE(104);
      END_STATE();
    case 62:
      if (lookahead == 'c') ADVANCE(76);
      END_STATE();
    case 63:
      if (lookahead == 'c') ADVANCE(74);
      END_STATE();
    case 64:
      if (lookahead == 'd') ADVANCE(187);
      END_STATE();
    case 65:
      if (lookahead == 'd') ADVANCE(124);
      END_STATE();
    case 66:
      if (lookahead == 'd') ADVANCE(65);
      END_STATE();
    case 67:
      if (lookahead == 'd') ADVANCE(61);
      END_STATE();
    case 68:
      if (lookahead == 'd') ADVANCE(72);
      END_STATE();
    case 69:
      if (lookahead == 'd') ADVANCE(73);
      END_STATE();
    case 70:
      if (lookahead == 'd') ADVANCE(80);
      END_STATE();
    case 71:
      if (lookahead == 'e') ADVANCE(84);
      END_STATE();
    case 72:
      if (lookahead == 'e') ADVANCE(312);
      END_STATE();
    case 73:
      if (lookahead == 'e') ADVANCE(317);
      END_STATE();
    case 74:
      if (lookahead == 'e') ADVANCE(181);
      END_STATE();
    case 75:
      if (lookahead == 'e') ADVANCE(85);
      END_STATE();
    case 76:
      if (lookahead == 'e') ADVANCE(106);
      END_STATE();
    case 77:
      if (lookahead == 'e') ADVANCE(86);
      END_STATE();
    case 78:
      if (lookahead == 'e') ADVANCE(119);
      END_STATE();
    case 79:
      if (lookahead == 'e') ADVANCE(112);
      END_STATE();
    case 80:
      if (lookahead == 'e') ADVANCE(83);
      END_STATE();
    case 81:
      if (lookahead == 'e') ADVANCE(70);
      END_STATE();
    case 82:
      if (lookahead == 'e') ADVANCE(113);
      END_STATE();
    case 83:
      if (lookahead == 'f') ADVANCE(187);
      END_STATE();
    case 84:
      if (lookahead == 'f') ADVANCE(87);
      END_STATE();
    case 85:
      if (lookahead == 'f') ADVANCE(165);
      END_STATE();
    case 86:
      if (lookahead == 'f') ADVANCE(167);
      END_STATE();
    case 87:
      if (lookahead == 'g') ADVANCE(114);
      END_STATE();
    case 88:
      if (lookahead == 'i') ADVANCE(101);
      END_STATE();
    case 89:
      if (lookahead == 'i') ADVANCE(75);
      END_STATE();
    case 90:
      if (lookahead == 'i') ADVANCE(77);
      END_STATE();
    case 91:
      if (lookahead == 'l') ADVANCE(48);
      if (lookahead == 'o') ADVANCE(68);
      END_STATE();
    case 92:
      if (lookahead == 'l') ADVANCE(102);
      END_STATE();
    case 93:
      if (lookahead == 'm') ADVANCE(183);
      END_STATE();
    case 94:
      if (lookahead == 'm') ADVANCE(78);
      END_STATE();
    case 95:
      if (lookahead == 'n') ADVANCE(187);
      END_STATE();
    case 96:
      if (lookahead == 'n') ADVANCE(181);
      END_STATE();
    case 97:
      if (lookahead == 'n') ADVANCE(283);
      END_STATE();
    case 98:
      if (lookahead == 'n') ADVANCE(67);
      if (lookahead == 'x') ADVANCE(62);
      END_STATE();
    case 99:
      if (lookahead == 'o') ADVANCE(87);
      END_STATE();
    case 100:
      if (lookahead == 'o') ADVANCE(125);
      END_STATE();
    case 101:
      if (lookahead == 'o') ADVANCE(96);
      END_STATE();
    case 102:
      if (lookahead == 'o') ADVANCE(59);
      END_STATE();
    case 103:
      if (lookahead == 'o') ADVANCE(109);
      END_STATE();
    case 104:
      if (lookahead == 'o') ADVANCE(69);
      END_STATE();
    case 105:
      if (lookahead == 'p') ADVANCE(189);
      END_STATE();
    case 106:
      if (lookahead == 'p') ADVANCE(123);
      END_STATE();
    case 107:
      if (lookahead == 'p') ADVANCE(60);
      END_STATE();
    case 108:
      if (lookahead == 'p') ADVANCE(81);
      END_STATE();
    case 109:
      if (lookahead == 'p') ADVANCE(82);
      END_STATE();
    case 110:
      if (lookahead == 'r') ADVANCE(89);
      END_STATE();
    case 111:
      if (lookahead == 'r') ADVANCE(187);
      END_STATE();
    case 112:
      if (lookahead == 'r') ADVANCE(92);
      END_STATE();
    case 113:
      if (lookahead == 'r') ADVANCE(122);
      END_STATE();
    case 114:
      if (lookahead == 'r') ADVANCE(100);
      END_STATE();
    case 115:
      if (lookahead == 'r') ADVANCE(57);
      END_STATE();
    case 116:
      if (lookahead == 'r') ADVANCE(90);
      END_STATE();
    case 117:
      if (lookahead == 's') ADVANCE(189);
      END_STATE();
    case 118:
      if (lookahead == 's') ADVANCE(117);
      END_STATE();
    case 119:
      if (lookahead == 's') ADVANCE(107);
      END_STATE();
    case 120:
      if (lookahead == 't') ADVANCE(284);
      END_STATE();
    case 121:
      if (lookahead == 't') ADVANCE(286);
      END_STATE();
    case 122:
      if (lookahead == 't') ADVANCE(130);
      END_STATE();
    case 123:
      if (lookahead == 't') ADVANCE(88);
      END_STATE();
    case 124:
      if (lookahead == 't') ADVANCE(99);
      END_STATE();
    case 125:
      if (lookahead == 'u') ADVANCE(105);
      END_STATE();
    case 126:
      if (lookahead == 'u') ADVANCE(120);
      END_STATE();
    case 127:
      if (lookahead == 'u') ADVANCE(121);
      END_STATE();
    case 128:
      if (lookahead == 'v') ADVANCE(79);
      END_STATE();
    case 129:
      if (lookahead == 'x') ADVANCE(62);
      END_STATE();
    case 130:
      if (lookahead == 'y') ADVANCE(187);
      END_STATE();
    case 131:
      if (lookahead == 'y') ADVANCE(108);
      END_STATE();
    case 132:
      if (lookahead == '~') ADVANCE(137);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(33);
      END_STATE();
    case 133:
      if (lookahead == '~') ADVANCE(135);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(191);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 134:
      if (lookahead == '~') ADVANCE(136);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      END_STATE();
    case 135:
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(191);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 136:
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      END_STATE();
    case 137:
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(33);
      END_STATE();
    case 138:
      if (lookahead != 0 &&
          lookahead != '.' &&
          lookahead != '<') ADVANCE(38);
      END_STATE();
    case 139:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(140);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == ')') ADVANCE(280);
      if (lookahead == '*') ADVANCE(39);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(393);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(356);
      if (lookahead == '>') ADVANCE(297);
      if (lookahead == '@') ADVANCE(44);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(45);
      if (lookahead == ']') ADVANCE(288);
      if (lookahead == 'i') ADVANCE(263);
      if (lookahead == 'o') ADVANCE(266);
      if (lookahead == '~') ADVANCE(277);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(139);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 140:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(140);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == ')') ADVANCE(280);
      if (lookahead == '*') ADVANCE(2);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(393);
      if (lookahead == ':') ADVANCE(344);
      if (lookahead == '<') ADVANCE(356);
      if (lookahead == '>') ADVANCE(297);
      if (lookahead == '@') ADVANCE(44);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(45);
      if (lookahead == ']') ADVANCE(288);
      if (lookahead == 'i') ADVANCE(263);
      if (lookahead == 'o') ADVANCE(266);
      if (lookahead == '~') ADVANCE(277);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(140);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(139);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      if (lookahead != 0) ADVANCE(346);
      END_STATE();
    case 141:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(141);
      if (lookahead == ')') ADVANCE(280);
      if (lookahead == '*') SKIP(141);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(388);
      if (lookahead == ':') ADVANCE(41);
      if (lookahead == '<') ADVANCE(40);
      if (lookahead == '>') ADVANCE(297);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (lookahead == ']') ADVANCE(288);
      if (lookahead == '~') ADVANCE(137);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(141);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(143);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(270);
      END_STATE();
    case 142:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(141);
      if (lookahead == ')') ADVANCE(280);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '.') ADVANCE(315);
      if (lookahead == '/') ADVANCE(388);
      if (lookahead == ':') ADVANCE(41);
      if (lookahead == '<') ADVANCE(40);
      if (lookahead == '>') ADVANCE(297);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (lookahead == ']') ADVANCE(288);
      if (lookahead == '{') ADVANCE(314);
      if (lookahead == '}') ADVANCE(316);
      if (lookahead == '~') ADVANCE(137);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(143);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(270);
      END_STATE();
    case 143:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(141);
      if (lookahead == ')') ADVANCE(280);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == '/') ADVANCE(388);
      if (lookahead == ':') ADVANCE(41);
      if (lookahead == '<') ADVANCE(40);
      if (lookahead == '>') ADVANCE(297);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (lookahead == ']') ADVANCE(288);
      if (lookahead == '~') ADVANCE(137);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(143);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(270);
      END_STATE();
    case 144:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(144);
      if (lookahead == '*') SKIP(144);
      if (lookahead == ':') ADVANCE(373);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(49);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(54);
      if (lookahead == '~') ADVANCE(374);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(144);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(138);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(145);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(365);
      if (lookahead != 0) ADVANCE(375);
      END_STATE();
    case 145:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(144);
      if (lookahead == ':') ADVANCE(373);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(49);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(54);
      if (lookahead == '~') ADVANCE(374);
      if (lookahead == '{' ||
          lookahead == '}') ADVANCE(138);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(145);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(365);
      if (lookahead != 0 &&
          lookahead != '*') ADVANCE(375);
      END_STATE();
    case 146:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(146);
      if (lookahead == '*') SKIP(146);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(278);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(146);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(147);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(272);
      if (lookahead != 0 &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 147:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(146);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(278);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(147);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(272);
      if (lookahead != 0 &&
          lookahead != '*' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 148:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(148);
      if (lookahead == '*') SKIP(148);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(148);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(149);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 149:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(148);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(149);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          lookahead != '*' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 150:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(148);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == ':') ADVANCE(358);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(149);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          lookahead != '*' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 151:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(151);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == '*') SKIP(151);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(151);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(152);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 152:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(151);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(152);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          lookahead != '*' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 153:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(151);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == ':') ADVANCE(358);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(152);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          lookahead != '*' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 154:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(154);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == '*') SKIP(154);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(154);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(155);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 155:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(154);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(155);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          lookahead != '*' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 156:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(154);
      if (lookahead == '(') ADVANCE(279);
      if (lookahead == ':') ADVANCE(358);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(155);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          lookahead != '*' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 157:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(157);
      if (lookahead == '*') SKIP(157);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == ':') ADVANCE(41);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (lookahead == '~') ADVANCE(137);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(157);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(158);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(271);
      END_STATE();
    case 158:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(157);
      if (lookahead == ',') ADVANCE(176);
      if (lookahead == ':') ADVANCE(41);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(51);
      if (lookahead == '~') ADVANCE(137);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(158);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(271);
      END_STATE();
    case 159:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(159);
      if (lookahead == '*') SKIP(159);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(159);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(160);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 160:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(159);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(160);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          lookahead != '*' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 161:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(159);
      if (lookahead == ':') ADVANCE(358);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(160);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(348);
      if (lookahead != 0 &&
          lookahead != '*' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 162:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(162);
      if (lookahead == '*') SKIP(162);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (lookahead == '\t' ||
          lookahead == ' ') SKIP(162);
      if ((0x0b <= lookahead && lookahead <= '\r')) SKIP(163);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(272);
      if (lookahead != 0 &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 163:
      if (eof) ADVANCE(164);
      if (lookahead == '\n') SKIP(162);
      if (lookahead == ':') ADVANCE(357);
      if (lookahead == '<') ADVANCE(359);
      if (lookahead == '@') ADVANCE(51);
      if (lookahead == '[') ADVANCE(281);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead == '~') ADVANCE(361);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(163);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(272);
      if (lookahead != 0 &&
          lookahead != '*' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(362);
      END_STATE();
    case 164:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 165:
      ACCEPT_TOKEN(anon_sym_ATbrief);
      END_STATE();
    case 166:
      ACCEPT_TOKEN(anon_sym_ATbrief);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 167:
      ACCEPT_TOKEN(anon_sym_BSLASHbrief);
      END_STATE();
    case 168:
      ACCEPT_TOKEN(anon_sym_BSLASHbrief);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 169:
      ACCEPT_TOKEN(aux_sym_brief_header_token1);
      END_STATE();
    case 170:
      ACCEPT_TOKEN(aux_sym_brief_header_token1);
      if (lookahead == '!') ADVANCE(401);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(377);
      if (lookahead == '/') ADVANCE(363);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(402);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(363);
      END_STATE();
    case 171:
      ACCEPT_TOKEN(aux_sym_brief_header_token1);
      if (lookahead == '!') ADVANCE(34);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(378);
      if (lookahead == '/') ADVANCE(376);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(35);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(376);
      END_STATE();
    case 172:
      ACCEPT_TOKEN(aux_sym_brief_header_token1);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(377);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(402);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(401);
      END_STATE();
    case 173:
      ACCEPT_TOKEN(aux_sym_brief_header_token1);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(378);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(35);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(34);
      END_STATE();
    case 174:
      ACCEPT_TOKEN(aux_sym_brief_header_token1);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(402);
      END_STATE();
    case 175:
      ACCEPT_TOKEN(aux_sym_brief_header_token1);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(35);
      END_STATE();
    case 176:
      ACCEPT_TOKEN(anon_sym_COMMA);
      END_STATE();
    case 177:
      ACCEPT_TOKEN(aux_sym_tag_token1);
      if (lookahead == '\n') ADVANCE(178);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(177);
      END_STATE();
    case 178:
      ACCEPT_TOKEN(aux_sym_tag_token1);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(178);
      END_STATE();
    case 179:
      ACCEPT_TOKEN(aux_sym_tag_token2);
      if (lookahead == '\t' ||
          (0x0b <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(179);
      if (lookahead != 0 &&
          (lookahead < '\t' || '\r' < lookahead)) ADVANCE(180);
      END_STATE();
    case 180:
      ACCEPT_TOKEN(aux_sym_tag_token2);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(180);
      END_STATE();
    case 181:
      ACCEPT_TOKEN(sym_tag_name_with_argument);
      END_STATE();
    case 182:
      ACCEPT_TOKEN(sym_tag_name_with_argument);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 183:
      ACCEPT_TOKEN(sym_tag_name_with_multiple_arguments);
      END_STATE();
    case 184:
      ACCEPT_TOKEN(sym_tag_name_with_multiple_arguments);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 185:
      ACCEPT_TOKEN(sym_tag_name_with_types);
      END_STATE();
    case 186:
      ACCEPT_TOKEN(sym_tag_name_with_types);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 187:
      ACCEPT_TOKEN(sym_tag_name_with_self_types);
      END_STATE();
    case 188:
      ACCEPT_TOKEN(sym_tag_name_with_self_types);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 189:
      ACCEPT_TOKEN(sym_tag_name_with_type);
      END_STATE();
    case 190:
      ACCEPT_TOKEN(sym_tag_name_with_type);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 191:
      ACCEPT_TOKEN(sym_tag_name);
      END_STATE();
    case 192:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'a') ADVANCE(230);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 193:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'a') ADVANCE(248);
      if (lookahead == 'r') ADVANCE(237);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 194:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'a') ADVANCE(186);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 195:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'a') ADVANCE(251);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 196:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'a') ADVANCE(229);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 197:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'a') ADVANCE(243);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 198:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'a') ADVANCE(202);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 199:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'a') ADVANCE(201);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 200:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'c') ADVANCE(211);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 201:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'c') ADVANCE(208);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 202:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'd') ADVANCE(188);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 203:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'd') ADVANCE(255);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 204:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'd') ADVANCE(203);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 205:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'd') ADVANCE(209);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 206:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'd') ADVANCE(210);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 207:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(219);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 208:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(182);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 209:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(313);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 210:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(218);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 211:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(239);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 212:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(206);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 213:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(220);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 214:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(244);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 215:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(252);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 216:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(221);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 217:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'e') ADVANCE(247);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 218:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'f') ADVANCE(188);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 219:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'f') ADVANCE(222);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 220:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'f') ADVANCE(166);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 221:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'f') ADVANCE(168);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 222:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'g') ADVANCE(246);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 223:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'i') ADVANCE(235);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 224:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'i') ADVANCE(213);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 225:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'i') ADVANCE(216);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 226:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'l') ADVANCE(195);
      if (lookahead == 'o') ADVANCE(205);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 227:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'l') ADVANCE(195);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 228:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'l') ADVANCE(236);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 229:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'm') ADVANCE(184);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 230:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'm') ADVANCE(215);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 231:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'n') ADVANCE(188);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 232:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'n') ADVANCE(182);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 233:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'o') ADVANCE(222);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 234:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'o') ADVANCE(256);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 235:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'o') ADVANCE(232);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 236:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'o') ADVANCE(198);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 237:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'o') ADVANCE(242);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 238:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'p') ADVANCE(190);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 239:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'p') ADVANCE(253);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 240:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'p') ADVANCE(212);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 241:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'p') ADVANCE(199);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 242:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'p') ADVANCE(217);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 243:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'r') ADVANCE(188);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 244:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'r') ADVANCE(228);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 245:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'r') ADVANCE(224);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 246:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'r') ADVANCE(234);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 247:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'r') ADVANCE(254);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 248:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'r') ADVANCE(196);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 249:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'r') ADVANCE(225);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 250:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 's') ADVANCE(190);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 251:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 's') ADVANCE(250);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 252:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 's') ADVANCE(241);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 253:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 't') ADVANCE(223);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 254:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 't') ADVANCE(259);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 255:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 't') ADVANCE(233);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 256:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'u') ADVANCE(238);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 257:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'v') ADVANCE(214);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 258:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'x') ADVANCE(200);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 259:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'y') ADVANCE(188);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 260:
      ACCEPT_TOKEN(sym_tag_name);
      if (lookahead == 'y') ADVANCE(240);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 261:
      ACCEPT_TOKEN(sym_tag_name);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 262:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '\n') ADVANCE(178);
      if (lookahead == '(') ADVANCE(402);
      if (lookahead == ':') ADVANCE(405);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(177);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(262);
      END_STATE();
    case 263:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == ':') ADVANCE(337);
      if (lookahead == 'n') ADVANCE(282);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      END_STATE();
    case 264:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == ':') ADVANCE(337);
      if (lookahead == 't') ADVANCE(285);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      END_STATE();
    case 265:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == ':') ADVANCE(337);
      if (lookahead == 't') ADVANCE(287);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      END_STATE();
    case 266:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == ':') ADVANCE(337);
      if (lookahead == 'u') ADVANCE(264);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      END_STATE();
    case 267:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == ':') ADVANCE(337);
      if (lookahead == 'u') ADVANCE(265);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      END_STATE();
    case 268:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == ':') ADVANCE(337);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      END_STATE();
    case 269:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '(') ADVANCE(402);
      if (lookahead == ':') ADVANCE(405);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(269);
      END_STATE();
    case 270:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '(') ADVANCE(35);
      if (lookahead == ':') ADVANCE(42);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(178);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(270);
      END_STATE();
    case 271:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '(') ADVANCE(35);
      if (lookahead == ':') ADVANCE(42);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(271);
      END_STATE();
    case 272:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (lookahead == '(') ADVANCE(376);
      if (lookahead == ':') ADVANCE(349);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(272);
      END_STATE();
    case 273:
      ACCEPT_TOKEN(aux_sym_identifier_token1);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(273);
      END_STATE();
    case 274:
      ACCEPT_TOKEN(anon_sym_COLON_COLON);
      if (lookahead == '~') ADVANCE(341);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      END_STATE();
    case 275:
      ACCEPT_TOKEN(anon_sym_COLON_COLON);
      if (lookahead == '~') ADVANCE(353);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      END_STATE();
    case 276:
      ACCEPT_TOKEN(anon_sym_TILDE);
      END_STATE();
    case 277:
      ACCEPT_TOKEN(anon_sym_TILDE);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(335);
      END_STATE();
    case 278:
      ACCEPT_TOKEN(anon_sym_TILDE);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(347);
      END_STATE();
    case 279:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 280:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 281:
      ACCEPT_TOKEN(anon_sym_LBRACK);
      END_STATE();
    case 282:
      ACCEPT_TOKEN(anon_sym_in);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == ':') ADVANCE(337);
      if (lookahead == 'o') ADVANCE(267);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      END_STATE();
    case 283:
      ACCEPT_TOKEN(anon_sym_in);
      if (lookahead == 'o') ADVANCE(127);
      END_STATE();
    case 284:
      ACCEPT_TOKEN(anon_sym_out);
      END_STATE();
    case 285:
      ACCEPT_TOKEN(anon_sym_out);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == ':') ADVANCE(337);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      END_STATE();
    case 286:
      ACCEPT_TOKEN(anon_sym_inout);
      END_STATE();
    case 287:
      ACCEPT_TOKEN(anon_sym_inout);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == ':') ADVANCE(337);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(268);
      END_STATE();
    case 288:
      ACCEPT_TOKEN(anon_sym_RBRACK);
      END_STATE();
    case 289:
      ACCEPT_TOKEN(anon_sym_BSLASHa);
      if (lookahead == 'd') ADVANCE(65);
      END_STATE();
    case 290:
      ACCEPT_TOKEN(anon_sym_BSLASHa);
      if (lookahead == 'd') ADVANCE(203);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 291:
      ACCEPT_TOKEN(anon_sym_BSLASHc);
      if (lookahead == 'l') ADVANCE(48);
      END_STATE();
    case 292:
      ACCEPT_TOKEN(anon_sym_BSLASHc);
      if (lookahead == 'l') ADVANCE(195);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 293:
      ACCEPT_TOKEN(anon_sym_LTa);
      END_STATE();
    case 294:
      ACCEPT_TOKEN(aux_sym_link_token1);
      if (lookahead == '\n') ADVANCE(294);
      if (lookahead == '*') ADVANCE(294);
      if (lookahead == '\t' ||
          lookahead == ' ') ADVANCE(294);
      if ((0x0b <= lookahead && lookahead <= '\r')) ADVANCE(295);
      if (lookahead != 0 &&
          lookahead != '>') ADVANCE(296);
      END_STATE();
    case 295:
      ACCEPT_TOKEN(aux_sym_link_token1);
      if (lookahead == '\n') ADVANCE(294);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(295);
      if (lookahead != 0 &&
          lookahead != '>') ADVANCE(296);
      END_STATE();
    case 296:
      ACCEPT_TOKEN(aux_sym_link_token1);
      if (lookahead != 0 &&
          lookahead != '>') ADVANCE(296);
      END_STATE();
    case 297:
      ACCEPT_TOKEN(anon_sym_GT);
      END_STATE();
    case 298:
      ACCEPT_TOKEN(aux_sym_link_token2);
      if (lookahead == '\n') ADVANCE(298);
      if (lookahead == '*') ADVANCE(298);
      if (lookahead == '\t' ||
          lookahead == ' ') ADVANCE(298);
      if ((0x0b <= lookahead && lookahead <= '\r')) ADVANCE(299);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(300);
      END_STATE();
    case 299:
      ACCEPT_TOKEN(aux_sym_link_token2);
      if (lookahead == '\n') ADVANCE(298);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(299);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(300);
      END_STATE();
    case 300:
      ACCEPT_TOKEN(aux_sym_link_token2);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(300);
      END_STATE();
    case 301:
      ACCEPT_TOKEN(anon_sym_LT_SLASHa_GT);
      END_STATE();
    case 302:
      ACCEPT_TOKEN(sym_function_link);
      END_STATE();
    case 303:
      ACCEPT_TOKEN(sym_function_link);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '*', 334,
        '.', 169,
        '/', 325,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(303);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(325);
      END_STATE();
    case 304:
      ACCEPT_TOKEN(sym_function_link);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '*', 334,
        '.', 169,
        '/', 325,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(325);
      END_STATE();
    case 305:
      ACCEPT_TOKEN(sym_function_link);
      if (lookahead == '\n') ADVANCE(38);
      if (lookahead == '*') ADVANCE(334);
      if (lookahead == '.') ADVANCE(169);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(398);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(397);
      END_STATE();
    case 306:
      ACCEPT_TOKEN(sym_function_link);
      if (lookahead == '\n') ADVANCE(38);
      if (lookahead == '.') ADVANCE(169);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(398);
      END_STATE();
    case 307:
      ACCEPT_TOKEN(sym_function_link);
      ADVANCE_MAP(
        '!', 36,
        '*', 380,
        '.', 169,
        '/', 371,
        '\n', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(307);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(371);
      END_STATE();
    case 308:
      ACCEPT_TOKEN(sym_function_link);
      ADVANCE_MAP(
        '!', 36,
        '*', 380,
        '.', 169,
        '/', 371,
        '\n', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(371);
      END_STATE();
    case 309:
      ACCEPT_TOKEN(sym_function_link);
      if (lookahead == '*') ADVANCE(380);
      if (lookahead == '.') ADVANCE(169);
      if (lookahead == '\n' ||
          lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(38);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(36);
      END_STATE();
    case 310:
      ACCEPT_TOKEN(sym_function_link);
      if (lookahead == '.') ADVANCE(169);
      if (lookahead != 0 &&
          lookahead != '<') ADVANCE(38);
      END_STATE();
    case 311:
      ACCEPT_TOKEN(sym_function_link);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      END_STATE();
    case 312:
      ACCEPT_TOKEN(anon_sym_ATcode);
      END_STATE();
    case 313:
      ACCEPT_TOKEN(anon_sym_ATcode);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(261);
      END_STATE();
    case 314:
      ACCEPT_TOKEN(anon_sym_LBRACE);
      END_STATE();
    case 315:
      ACCEPT_TOKEN(anon_sym_DOT);
      END_STATE();
    case 316:
      ACCEPT_TOKEN(anon_sym_RBRACE);
      END_STATE();
    case 317:
      ACCEPT_TOKEN(anon_sym_ATendcode);
      END_STATE();
    case 318:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '(', 330,
        '*', 334,
        '.', 169,
        '/', 325,
        ':', 320,
        '<', 342,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(318);
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 319:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '(', 330,
        '/', 326,
        ':', 320,
        '.', 342,
        '<', 342,
        '*', 398,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(318);
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 320:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '*', 334,
        '.', 169,
        '/', 325,
        ':', 322,
        '<', 342,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 321:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '*', 334,
        '.', 169,
        '/', 325,
        '<', 342,
        '~', 324,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(303);
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 322:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '*', 334,
        '.', 169,
        '/', 325,
        '<', 342,
        '~', 323,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(318);
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 323:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '*', 334,
        '.', 169,
        '/', 325,
        '<', 342,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(318);
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 324:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '*', 334,
        '.', 169,
        '/', 325,
        '<', 342,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(303);
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 325:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '*', 334,
        '.', 169,
        '/', 325,
        '<', 342,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 326:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '.', 169,
        '/', 326,
        '<', 342,
        '*', 398,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 327:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '/', 326,
        ':', 321,
        '.', 342,
        '<', 342,
        '*', 398,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 328:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '/', 326,
        '.', 342,
        '<', 342,
        '*', 398,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(318);
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 329:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 397,
        '/', 326,
        '.', 342,
        '<', 342,
        '*', 398,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (lookahead != 0) ADVANCE(325);
      END_STATE();
    case 330:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 395,
        ')', 304,
        '*', 332,
        '.', 170,
        '/', 330,
        '<', 363,
        '\\', 396,
        '{', 396,
        '}', 396,
      );
      if (lookahead != 0) ADVANCE(330);
      END_STATE();
    case 331:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '!', 5,
        ')', 308,
        '*', 333,
        '.', 171,
        '/', 331,
        '<', 376,
        '\\', 6,
        '{', 6,
        '}', 6,
      );
      if (lookahead != 0) ADVANCE(331);
      END_STATE();
    case 332:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        ')', 305,
        '*', 332,
        '.', 172,
        '<', 401,
        '/', 396,
        '\\', 396,
        '{', 396,
        '}', 396,
      );
      if (lookahead != 0) ADVANCE(395);
      END_STATE();
    case 333:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        ')', 309,
        '*', 333,
        '.', 173,
        '<', 34,
        '/', 6,
        '\\', 6,
        '{', 6,
        '}', 6,
      );
      if (lookahead != 0) ADVANCE(5);
      END_STATE();
    case 334:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '\n', 38,
        '*', 334,
        '.', 169,
        '<', 403,
        '/', 398,
        '\\', 398,
        '{', 398,
        '}', 398,
      );
      if (lookahead != 0) ADVANCE(397);
      END_STATE();
    case 335:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == '*') ADVANCE(379);
      if (lookahead == '/') ADVANCE(342);
      if (lookahead == ':') ADVANCE(337);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(335);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}') ADVANCE(342);
      END_STATE();
    case 336:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '(') ADVANCE(363);
      if (lookahead == '/') ADVANCE(346);
      if (lookahead == ':') ADVANCE(337);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(335);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}') ADVANCE(342);
      END_STATE();
    case 337:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '*') ADVANCE(379);
      if (lookahead == '/') ADVANCE(342);
      if (lookahead == ':') ADVANCE(339);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(342);
      END_STATE();
    case 338:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '*') ADVANCE(379);
      if (lookahead == '/') ADVANCE(342);
      if (lookahead == '~') ADVANCE(341);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(342);
      END_STATE();
    case 339:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '*') ADVANCE(379);
      if (lookahead == '/') ADVANCE(342);
      if (lookahead == '~') ADVANCE(340);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(335);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(342);
      END_STATE();
    case 340:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '*') ADVANCE(379);
      if (lookahead == '/') ADVANCE(342);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(335);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}') ADVANCE(342);
      END_STATE();
    case 341:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '*') ADVANCE(379);
      if (lookahead == '/') ADVANCE(342);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}') ADVANCE(342);
      END_STATE();
    case 342:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '*') ADVANCE(379);
      if (lookahead == '/') ADVANCE(342);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(342);
      END_STATE();
    case 343:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '/') ADVANCE(346);
      if (lookahead == ':') ADVANCE(274);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(342);
      END_STATE();
    case 344:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '/') ADVANCE(346);
      if (lookahead == ':') ADVANCE(338);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(342);
      END_STATE();
    case 345:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '/') ADVANCE(346);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(335);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}') ADVANCE(342);
      END_STATE();
    case 346:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(403);
      if (lookahead == '/') ADVANCE(346);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(342);
      END_STATE();
    case 347:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '(') ADVANCE(376);
      if (lookahead == '*') ADVANCE(381);
      if (lookahead == '/') ADVANCE(355);
      if (lookahead == ':') ADVANCE(349);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(347);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 348:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '(') ADVANCE(376);
      if (lookahead == '/') ADVANCE(362);
      if (lookahead == ':') ADVANCE(349);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(347);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 349:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '*') ADVANCE(381);
      if (lookahead == '/') ADVANCE(355);
      if (lookahead == ':') ADVANCE(352);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 350:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '*') ADVANCE(381);
      if (lookahead == '/') ADVANCE(355);
      if (lookahead == '>') ADVANCE(301);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 351:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '*') ADVANCE(381);
      if (lookahead == '/') ADVANCE(355);
      if (lookahead == '~') ADVANCE(353);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(355);
      END_STATE();
    case 352:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '*') ADVANCE(381);
      if (lookahead == '/') ADVANCE(355);
      if (lookahead == '~') ADVANCE(354);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(347);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}' &&
          lookahead != '~') ADVANCE(355);
      END_STATE();
    case 353:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '*') ADVANCE(381);
      if (lookahead == '/') ADVANCE(355);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 354:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '*') ADVANCE(381);
      if (lookahead == '/') ADVANCE(355);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(347);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 355:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '*') ADVANCE(381);
      if (lookahead == '/') ADVANCE(355);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 356:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '/') ADVANCE(360);
      if (lookahead == 'a') ADVANCE(293);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 357:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '/') ADVANCE(362);
      if (lookahead == ':') ADVANCE(351);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 358:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '/') ADVANCE(362);
      if (lookahead == ':') ADVANCE(275);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 359:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '/') ADVANCE(362);
      if (lookahead == 'a') ADVANCE(293);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 360:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '/') ADVANCE(362);
      if (lookahead == 'a') ADVANCE(350);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 361:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '/') ADVANCE(362);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(347);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          (lookahead < 'a' || '{' < lookahead) &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 362:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(37);
      if (lookahead == '/') ADVANCE(362);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '*' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(355);
      END_STATE();
    case 363:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(401);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(377);
      if (lookahead == '/') ADVANCE(363);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(402);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(363);
      END_STATE();
    case 364:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '(', 331,
        '*', 380,
        '.', 169,
        '/', 371,
        ':', 366,
        '<', 355,
        '\n', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(364);
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 365:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '(', 331,
        '/', 372,
        ':', 366,
        '.', 355,
        '<', 355,
        '\n', 38,
        '*', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(364);
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 366:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '*', 380,
        '.', 169,
        '/', 371,
        ':', 368,
        '<', 355,
        '\n', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 367:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '*', 380,
        '.', 169,
        '/', 371,
        '<', 355,
        '~', 370,
        '\n', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(307);
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 368:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '*', 380,
        '.', 169,
        '/', 371,
        '<', 355,
        '~', 369,
        '\n', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(364);
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 369:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '*', 380,
        '.', 169,
        '/', 371,
        '<', 355,
        '\n', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(364);
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 370:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '*', 380,
        '.', 169,
        '/', 371,
        '<', 355,
        '\n', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(307);
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 371:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '*', 380,
        '.', 169,
        '/', 371,
        '<', 355,
        '\n', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 372:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '.', 169,
        '/', 372,
        '<', 355,
        '\n', 38,
        '*', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 373:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '/', 372,
        ':', 367,
        '.', 355,
        '<', 355,
        '\n', 38,
        '*', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 374:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '/', 372,
        '.', 355,
        '<', 355,
        '\n', 38,
        '*', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(364);
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 375:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '!', 36,
        '/', 372,
        '.', 355,
        '<', 355,
        '\n', 38,
        '*', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (lookahead != 0) ADVANCE(371);
      END_STATE();
    case 376:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '!') ADVANCE(34);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(378);
      if (lookahead == '/') ADVANCE(376);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(35);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(376);
      END_STATE();
    case 377:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(377);
      if (lookahead == '/' ||
          lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(402);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(401);
      END_STATE();
    case 378:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(378);
      if (lookahead == '/' ||
          lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(35);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(34);
      END_STATE();
    case 379:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '*') ADVANCE(379);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '/' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(403);
      END_STATE();
    case 380:
      ACCEPT_TOKEN(sym__text);
      ADVANCE_MAP(
        '*', 380,
        '.', 169,
        '<', 37,
        '\n', 38,
        '/', 38,
        '\\', 38,
        '{', 38,
        '}', 38,
      );
      if (lookahead != 0) ADVANCE(36);
      END_STATE();
    case 381:
      ACCEPT_TOKEN(sym__text);
      if (lookahead == '*') ADVANCE(381);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '/' &&
          lookahead != '\\' &&
          lookahead != '{' &&
          lookahead != '}') ADVANCE(37);
      END_STATE();
    case 382:
      ACCEPT_TOKEN(sym__singleline_begin);
      END_STATE();
    case 383:
      ACCEPT_TOKEN(sym__singleline_begin);
      if (lookahead == '!') ADVANCE(385);
      if (lookahead == '*') ADVANCE(389);
      if (lookahead == '/') ADVANCE(387);
      if (lookahead == '<') ADVANCE(382);
      END_STATE();
    case 384:
      ACCEPT_TOKEN(sym__singleline_begin);
      if (lookahead == '!') ADVANCE(385);
      if (lookahead == '<') ADVANCE(382);
      if (lookahead == '*' ||
          lookahead == '/') ADVANCE(389);
      END_STATE();
    case 385:
      ACCEPT_TOKEN(sym__singleline_begin);
      if (lookahead == '<') ADVANCE(382);
      END_STATE();
    case 386:
      ACCEPT_TOKEN(sym__multiline_begin);
      END_STATE();
    case 387:
      ACCEPT_TOKEN(sym__multiline_begin);
      if (lookahead == '!') ADVANCE(390);
      if (lookahead == '*') ADVANCE(389);
      if (lookahead == '/') ADVANCE(387);
      if (lookahead == '<') ADVANCE(386);
      END_STATE();
    case 388:
      ACCEPT_TOKEN(sym__multiline_begin);
      if (lookahead == '!') ADVANCE(390);
      if (lookahead == '*') ADVANCE(389);
      if (lookahead == '/') ADVANCE(384);
      if (lookahead == '<') ADVANCE(386);
      END_STATE();
    case 389:
      ACCEPT_TOKEN(sym__multiline_begin);
      if (lookahead == '!') ADVANCE(390);
      if (lookahead == '<') ADVANCE(386);
      if (lookahead == '*' ||
          lookahead == '/') ADVANCE(389);
      END_STATE();
    case 390:
      ACCEPT_TOKEN(sym__multiline_begin);
      if (lookahead == '<') ADVANCE(386);
      END_STATE();
    case 391:
      ACCEPT_TOKEN(anon_sym_SLASH);
      END_STATE();
    case 392:
      ACCEPT_TOKEN(anon_sym_SLASH);
      if (lookahead == '\n') ADVANCE(38);
      if (lookahead == '!') ADVANCE(397);
      if (lookahead == '/') ADVANCE(326);
      if (lookahead == '*' ||
          lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(398);
      if (lookahead != 0 &&
          lookahead != '.' &&
          lookahead != '/' &&
          lookahead != '<') ADVANCE(325);
      END_STATE();
    case 393:
      ACCEPT_TOKEN(anon_sym_SLASH);
      if (lookahead == '!') ADVANCE(390);
      if (lookahead == '*') ADVANCE(389);
      if (lookahead == '/') ADVANCE(383);
      if (lookahead == '<') ADVANCE(386);
      END_STATE();
    case 394:
      ACCEPT_TOKEN(anon_sym_STAR_SLASH);
      END_STATE();
    case 395:
      ACCEPT_TOKEN(sym__text_line);
      ADVANCE_MAP(
        '\n', 38,
        ')', 305,
        '*', 332,
        '.', 172,
        '<', 401,
        '\\', 396,
        '{', 396,
        '}', 396,
      );
      if (lookahead != 0) ADVANCE(395);
      END_STATE();
    case 396:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == '\n') ADVANCE(38);
      if (lookahead == ')') ADVANCE(306);
      if (lookahead == '.') ADVANCE(174);
      if (lookahead == '<') ADVANCE(402);
      if (lookahead != 0) ADVANCE(396);
      END_STATE();
    case 397:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == '\n') ADVANCE(38);
      if (lookahead == '*') ADVANCE(334);
      if (lookahead == '.') ADVANCE(169);
      if (lookahead == '<') ADVANCE(403);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(398);
      if (lookahead != 0) ADVANCE(397);
      END_STATE();
    case 398:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == '\n') ADVANCE(38);
      if (lookahead == '.') ADVANCE(169);
      if (lookahead == '<') ADVANCE(410);
      if (lookahead != 0) ADVANCE(398);
      END_STATE();
    case 399:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == '\n') ADVANCE(38);
      if (lookahead == '.' ||
          lookahead == '<') ADVANCE(410);
      if (lookahead != 0) ADVANCE(398);
      END_STATE();
    case 400:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == '(') ADVANCE(402);
      if (lookahead == ':') ADVANCE(405);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(400);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(410);
      END_STATE();
    case 401:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(377);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(402);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(401);
      END_STATE();
    case 402:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(402);
      END_STATE();
    case 403:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == '*') ADVANCE(379);
      if (lookahead == '\\' ||
          lookahead == '{' ||
          lookahead == '}') ADVANCE(410);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(403);
      END_STATE();
    case 404:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == ':') ADVANCE(407);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(410);
      END_STATE();
    case 405:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == ':') ADVANCE(406);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(410);
      END_STATE();
    case 406:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == '~') ADVANCE(409);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(400);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(410);
      END_STATE();
    case 407:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead == '~') ADVANCE(408);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(410);
      END_STATE();
    case 408:
      ACCEPT_TOKEN(sym__text_line);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(311);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(410);
      END_STATE();
    case 409:
      ACCEPT_TOKEN(sym__text_line);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(400);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(410);
      END_STATE();
    case 410:
      ACCEPT_TOKEN(sym__text_line);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(410);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0, .external_lex_state = 1},
  [1] = {.lex_state = 142},
  [2] = {.lex_state = 3, .external_lex_state = 2},
  [3] = {.lex_state = 24, .external_lex_state = 2},
  [4] = {.lex_state = 29, .external_lex_state = 2},
  [5] = {.lex_state = 145},
  [6] = {.lex_state = 8, .external_lex_state = 2},
  [7] = {.lex_state = 8, .external_lex_state = 2},
  [8] = {.lex_state = 147},
  [9] = {.lex_state = 31, .external_lex_state = 2},
  [10] = {.lex_state = 8, .external_lex_state = 2},
  [11] = {.lex_state = 8, .external_lex_state = 2},
  [12] = {.lex_state = 29, .external_lex_state = 2},
  [13] = {.lex_state = 29, .external_lex_state = 2},
  [14] = {.lex_state = 29, .external_lex_state = 2},
  [15] = {.lex_state = 29, .external_lex_state = 2},
  [16] = {.lex_state = 29, .external_lex_state = 2},
  [17] = {.lex_state = 10, .external_lex_state = 2},
  [18] = {.lex_state = 160},
  [19] = {.lex_state = 149},
  [20] = {.lex_state = 163},
  [21] = {.lex_state = 149},
  [22] = {.lex_state = 13, .external_lex_state = 2},
  [23] = {.lex_state = 149},
  [24] = {.lex_state = 7, .external_lex_state = 2},
  [25] = {.lex_state = 7, .external_lex_state = 2},
  [26] = {.lex_state = 149},
  [27] = {.lex_state = 10, .external_lex_state = 2},
  [28] = {.lex_state = 28, .external_lex_state = 2},
  [29] = {.lex_state = 8, .external_lex_state = 2},
  [30] = {.lex_state = 13, .external_lex_state = 2},
  [31] = {.lex_state = 29, .external_lex_state = 3},
  [32] = {.lex_state = 7, .external_lex_state = 2},
  [33] = {.lex_state = 7, .external_lex_state = 2},
  [34] = {.lex_state = 28, .external_lex_state = 2},
  [35] = {.lex_state = 29, .external_lex_state = 3},
  [36] = {.lex_state = 160},
  [37] = {.lex_state = 160},
  [38] = {.lex_state = 160},
  [39] = {.lex_state = 28, .external_lex_state = 2},
  [40] = {.lex_state = 8, .external_lex_state = 2},
  [41] = {.lex_state = 16, .external_lex_state = 2},
  [42] = {.lex_state = 8, .external_lex_state = 2},
  [43] = {.lex_state = 8, .external_lex_state = 2},
  [44] = {.lex_state = 28, .external_lex_state = 2},
  [45] = {.lex_state = 18, .external_lex_state = 2},
  [46] = {.lex_state = 18, .external_lex_state = 2},
  [47] = {.lex_state = 26, .external_lex_state = 2},
  [48] = {.lex_state = 26, .external_lex_state = 2},
  [49] = {.lex_state = 26, .external_lex_state = 2},
  [50] = {.lex_state = 160},
  [51] = {.lex_state = 18, .external_lex_state = 2},
  [52] = {.lex_state = 18, .external_lex_state = 2},
  [53] = {.lex_state = 29, .external_lex_state = 2},
  [54] = {.lex_state = 18, .external_lex_state = 2},
  [55] = {.lex_state = 29, .external_lex_state = 2},
  [56] = {.lex_state = 29, .external_lex_state = 2},
  [57] = {.lex_state = 29, .external_lex_state = 2},
  [58] = {.lex_state = 29, .external_lex_state = 2},
  [59] = {.lex_state = 160},
  [60] = {.lex_state = 29, .external_lex_state = 2},
  [61] = {.lex_state = 29, .external_lex_state = 2},
  [62] = {.lex_state = 29, .external_lex_state = 2},
  [63] = {.lex_state = 29, .external_lex_state = 2},
  [64] = {.lex_state = 153},
  [65] = {.lex_state = 16, .external_lex_state = 2},
  [66] = {.lex_state = 26, .external_lex_state = 2},
  [67] = {.lex_state = 150},
  [68] = {.lex_state = 16, .external_lex_state = 2},
  [69] = {.lex_state = 153},
  [70] = {.lex_state = 156},
  [71] = {.lex_state = 18, .external_lex_state = 2},
  [72] = {.lex_state = 150},
  [73] = {.lex_state = 161},
  [74] = {.lex_state = 161},
  [75] = {.lex_state = 26, .external_lex_state = 2},
  [76] = {.lex_state = 150},
  [77] = {.lex_state = 156},
  [78] = {.lex_state = 150},
  [79] = {.lex_state = 149},
  [80] = {.lex_state = 160, .external_lex_state = 4},
  [81] = {.lex_state = 160, .external_lex_state = 4},
  [82] = {.lex_state = 149},
  [83] = {.lex_state = 161},
  [84] = {.lex_state = 161},
  [85] = {.lex_state = 149},
  [86] = {.lex_state = 149},
  [87] = {.lex_state = 142},
  [88] = {.lex_state = 158},
  [89] = {.lex_state = 160},
  [90] = {.lex_state = 160},
  [91] = {.lex_state = 160},
  [92] = {.lex_state = 158},
  [93] = {.lex_state = 160},
  [94] = {.lex_state = 160},
  [95] = {.lex_state = 158},
  [96] = {.lex_state = 160},
  [97] = {.lex_state = 160},
  [98] = {.lex_state = 160},
  [99] = {.lex_state = 160},
  [100] = {.lex_state = 18, .external_lex_state = 2},
  [101] = {.lex_state = 18, .external_lex_state = 2},
  [102] = {.lex_state = 142},
  [103] = {.lex_state = 18, .external_lex_state = 2},
  [104] = {.lex_state = 18, .external_lex_state = 2},
  [105] = {.lex_state = 18, .external_lex_state = 2},
  [106] = {.lex_state = 18, .external_lex_state = 2},
  [107] = {.lex_state = 158},
  [108] = {.lex_state = 18, .external_lex_state = 2},
  [109] = {.lex_state = 18, .external_lex_state = 2},
  [110] = {.lex_state = 142},
  [111] = {.lex_state = 18, .external_lex_state = 2},
  [112] = {.lex_state = 18, .external_lex_state = 2},
  [113] = {.lex_state = 18, .external_lex_state = 2},
  [114] = {.lex_state = 18, .external_lex_state = 2},
  [115] = {.lex_state = 18, .external_lex_state = 2},
  [116] = {.lex_state = 18, .external_lex_state = 2},
  [117] = {.lex_state = 18, .external_lex_state = 2},
  [118] = {.lex_state = 18, .external_lex_state = 2},
  [119] = {.lex_state = 142},
  [120] = {.lex_state = 142},
  [121] = {.lex_state = 158},
  [122] = {.lex_state = 142},
  [123] = {.lex_state = 142},
  [124] = {.lex_state = 142},
  [125] = {.lex_state = 142},
  [126] = {.lex_state = 142},
  [127] = {.lex_state = 142},
  [128] = {.lex_state = 142},
  [129] = {.lex_state = 142},
  [130] = {.lex_state = 21},
  [131] = {.lex_state = 142},
  [132] = {.lex_state = 142},
  [133] = {.lex_state = 142},
  [134] = {.lex_state = 142},
  [135] = {.lex_state = 21},
  [136] = {.lex_state = 142},
  [137] = {.lex_state = 142},
  [138] = {.lex_state = 142},
  [139] = {.lex_state = 142},
  [140] = {.lex_state = 21},
  [141] = {.lex_state = 21},
  [142] = {.lex_state = 21},
  [143] = {.lex_state = 21},
  [144] = {.lex_state = 21, .external_lex_state = 4},
  [145] = {.lex_state = 21, .external_lex_state = 4},
  [146] = {.lex_state = 24},
  [147] = {.lex_state = 23},
  [148] = {.lex_state = 23},
  [149] = {.lex_state = 23},
  [150] = {.lex_state = 24},
  [151] = {.lex_state = 24},
  [152] = {.lex_state = 24},
  [153] = {.lex_state = 21},
  [154] = {.lex_state = 21},
  [155] = {.lex_state = 21},
  [156] = {.lex_state = 142, .external_lex_state = 5},
  [157] = {.lex_state = 21},
  [158] = {.lex_state = 21},
  [159] = {.lex_state = 21},
  [160] = {.lex_state = 21},
  [161] = {.lex_state = 21},
  [162] = {.lex_state = 21},
  [163] = {.lex_state = 0, .external_lex_state = 5},
  [164] = {.lex_state = 142},
  [165] = {.lex_state = 21},
  [166] = {.lex_state = 179},
  [167] = {.lex_state = 142},
  [168] = {.lex_state = 0},
  [169] = {.lex_state = 0},
  [170] = {.lex_state = 0},
  [171] = {.lex_state = 142},
  [172] = {.lex_state = 0},
  [173] = {.lex_state = 0},
  [174] = {.lex_state = 142},
  [175] = {.lex_state = 0},
  [176] = {.lex_state = 142},
  [177] = {.lex_state = 179},
  [178] = {.lex_state = 21},
  [179] = {.lex_state = 21},
  [180] = {.lex_state = 0},
  [181] = {.lex_state = 142},
  [182] = {.lex_state = 295},
  [183] = {.lex_state = 142},
  [184] = {.lex_state = 21},
  [185] = {.lex_state = 0, .external_lex_state = 5},
  [186] = {.lex_state = 142},
  [187] = {.lex_state = 142},
  [188] = {.lex_state = 142},
  [189] = {.lex_state = 142},
  [190] = {.lex_state = 142},
  [191] = {.lex_state = 142},
  [192] = {.lex_state = 299},
  [193] = {.lex_state = 142},
  [194] = {.lex_state = 142},
  [195] = {.lex_state = 0, .external_lex_state = 6},
  [196] = {.lex_state = 0},
  [197] = {.lex_state = 299},
  [198] = {.lex_state = 0, .external_lex_state = 7},
  [199] = {.lex_state = 0},
  [200] = {.lex_state = 0},
  [201] = {.lex_state = 0, .external_lex_state = 6},
  [202] = {.lex_state = 142},
  [203] = {.lex_state = 0},
  [204] = {.lex_state = 142},
  [205] = {.lex_state = 295},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [anon_sym_ATbrief] = ACTIONS(1),
    [anon_sym_BSLASHbrief] = ACTIONS(1),
    [anon_sym_COMMA] = ACTIONS(1),
    [sym_tag_name_with_argument] = ACTIONS(1),
    [sym_tag_name_with_multiple_arguments] = ACTIONS(1),
    [sym_tag_name_with_types] = ACTIONS(1),
    [sym_tag_name_with_self_types] = ACTIONS(1),
    [sym_tag_name_with_type] = ACTIONS(1),
    [aux_sym_identifier_token1] = ACTIONS(1),
    [anon_sym_COLON_COLON] = ACTIONS(1),
    [anon_sym_TILDE] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [anon_sym_RPAREN] = ACTIONS(1),
    [anon_sym_LBRACK] = ACTIONS(1),
    [anon_sym_in] = ACTIONS(1),
    [anon_sym_out] = ACTIONS(1),
    [anon_sym_inout] = ACTIONS(1),
    [anon_sym_RBRACK] = ACTIONS(1),
    [anon_sym_BSLASHa] = ACTIONS(1),
    [anon_sym_BSLASHc] = ACTIONS(1),
    [anon_sym_LTa] = ACTIONS(1),
    [anon_sym_GT] = ACTIONS(1),
    [anon_sym_LT_SLASHa_GT] = ACTIONS(1),
    [sym_function_link] = ACTIONS(1),
    [anon_sym_ATcode] = ACTIONS(1),
    [anon_sym_LBRACE] = ACTIONS(1),
    [anon_sym_DOT] = ACTIONS(1),
    [anon_sym_RBRACE] = ACTIONS(1),
    [anon_sym_ATendcode] = ACTIONS(1),
    [sym__text] = ACTIONS(1),
    [sym__singleline_begin] = ACTIONS(1),
    [sym__multiline_begin] = ACTIONS(1),
    [anon_sym_SLASH] = ACTIONS(1),
    [anon_sym_STAR_SLASH] = ACTIONS(1),
    [sym__text_line] = ACTIONS(1),
    [sym_brief_text] = ACTIONS(1),
    [sym_code_block_start] = ACTIONS(1),
    [sym_code_block_language] = ACTIONS(1),
    [sym_code_block_content] = ACTIONS(1),
    [sym_code_block_end] = ACTIONS(1),
  },
  [1] = {
    [sym_document] = STATE(180),
    [sym__singleline_begin] = ACTIONS(3),
    [sym__multiline_begin] = ACTIONS(5),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 23,
    ACTIONS(9), 1,
      aux_sym_brief_header_token1,
    ACTIONS(11), 1,
      sym_tag_name_with_argument,
    ACTIONS(13), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(15), 1,
      sym_tag_name_with_types,
    ACTIONS(17), 1,
      sym_tag_name_with_self_types,
    ACTIONS(19), 1,
      sym_tag_name_with_type,
    ACTIONS(21), 1,
      sym_tag_name,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(33), 1,
      anon_sym_ATcode,
    ACTIONS(37), 1,
      sym__text_line,
    ACTIONS(39), 1,
      sym_code_block_start,
    STATE(4), 1,
      sym_brief_header,
    STATE(12), 1,
      sym_storageclass,
    STATE(51), 1,
      sym_description,
    STATE(173), 1,
      sym__multiline_end,
    ACTIONS(7), 2,
      anon_sym_ATbrief,
      anon_sym_BSLASHbrief,
    ACTIONS(31), 2,
      sym_function_link,
      sym__text,
    ACTIONS(35), 2,
      anon_sym_SLASH,
      anon_sym_STAR_SLASH,
    STATE(52), 3,
      sym_tag,
      sym_code_block,
      aux_sym_document_repeat1,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
  [78] = 13,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(31), 1,
      sym__text,
    ACTIONS(43), 1,
      aux_sym_identifier_token1,
    ACTIONS(45), 1,
      anon_sym_TILDE,
    ACTIONS(49), 1,
      sym_function_link,
    STATE(22), 1,
      sym_identifier,
    STATE(111), 1,
      sym_description,
    ACTIONS(47), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(13), 3,
      sym__expression,
      sym_qualified_identifier,
      sym_function,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(41), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [133] = 22,
    ACTIONS(11), 1,
      sym_tag_name_with_argument,
    ACTIONS(13), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(15), 1,
      sym_tag_name_with_types,
    ACTIONS(17), 1,
      sym_tag_name_with_self_types,
    ACTIONS(19), 1,
      sym_tag_name_with_type,
    ACTIONS(21), 1,
      sym_tag_name,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(31), 1,
      sym__text,
    ACTIONS(33), 1,
      anon_sym_ATcode,
    ACTIONS(39), 1,
      sym_code_block_start,
    ACTIONS(49), 1,
      sym_function_link,
    ACTIONS(51), 1,
      anon_sym_SLASH,
    ACTIONS(53), 1,
      anon_sym_STAR_SLASH,
    ACTIONS(55), 1,
      sym__text_line,
    STATE(12), 1,
      sym_storageclass,
    STATE(54), 1,
      sym_description,
    STATE(170), 1,
      sym__multiline_end,
    STATE(45), 3,
      sym_tag,
      sym_code_block,
      aux_sym_document_repeat1,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
  [205] = 19,
    ACTIONS(57), 1,
      ts_builtin_sym_end,
    ACTIONS(61), 1,
      aux_sym_brief_header_token1,
    ACTIONS(63), 1,
      sym_tag_name_with_argument,
    ACTIONS(65), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(67), 1,
      sym_tag_name_with_types,
    ACTIONS(69), 1,
      sym_tag_name_with_self_types,
    ACTIONS(71), 1,
      sym_tag_name_with_type,
    ACTIONS(73), 1,
      sym_tag_name,
    ACTIONS(75), 1,
      anon_sym_LBRACK,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    STATE(18), 1,
      sym_brief_header,
    STATE(37), 1,
      sym_storageclass,
    STATE(119), 1,
      sym_description,
    ACTIONS(59), 2,
      anon_sym_ATbrief,
      anon_sym_BSLASHbrief,
    ACTIONS(83), 2,
      sym_function_link,
      sym__text,
    STATE(120), 2,
      sym_tag,
      aux_sym_document_repeat2,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
  [269] = 11,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(31), 1,
      sym__text,
    ACTIONS(49), 1,
      sym_function_link,
    ACTIONS(85), 1,
      anon_sym_COMMA,
    STATE(10), 1,
      aux_sym_tag_repeat1,
    STATE(101), 1,
      sym_description,
    ACTIONS(89), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(87), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [316] = 11,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(31), 1,
      sym__text,
    ACTIONS(49), 1,
      sym_function_link,
    ACTIONS(85), 1,
      anon_sym_COMMA,
    STATE(29), 1,
      aux_sym_tag_repeat1,
    STATE(103), 1,
      sym_description,
    ACTIONS(93), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(91), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [363] = 13,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(83), 1,
      sym__text,
    ACTIONS(95), 1,
      aux_sym_identifier_token1,
    ACTIONS(97), 1,
      anon_sym_TILDE,
    ACTIONS(99), 1,
      sym_function_link,
    STATE(70), 1,
      sym_identifier,
    STATE(128), 1,
      sym_description,
    ACTIONS(47), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(36), 3,
      sym__expression,
      sym_qualified_identifier,
      sym_function,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(41), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [414] = 11,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(31), 1,
      sym__text,
    ACTIONS(49), 1,
      sym_function_link,
    ACTIONS(101), 1,
      aux_sym_identifier_token1,
    STATE(14), 1,
      sym_identifier,
    STATE(111), 1,
      sym_description,
    ACTIONS(47), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(41), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [461] = 11,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(31), 1,
      sym__text,
    ACTIONS(49), 1,
      sym_function_link,
    ACTIONS(85), 1,
      anon_sym_COMMA,
    STATE(29), 1,
      aux_sym_tag_repeat1,
    STATE(118), 1,
      sym_description,
    ACTIONS(105), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(103), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [508] = 11,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(31), 1,
      sym__text,
    ACTIONS(49), 1,
      sym_function_link,
    ACTIONS(85), 1,
      anon_sym_COMMA,
    STATE(7), 1,
      aux_sym_tag_repeat1,
    STATE(118), 1,
      sym_description,
    ACTIONS(105), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(103), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [555] = 9,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(31), 1,
      sym__text,
    ACTIONS(49), 1,
      sym_function_link,
    STATE(111), 1,
      sym_description,
    ACTIONS(47), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(41), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [596] = 9,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(31), 1,
      sym__text,
    ACTIONS(49), 1,
      sym_function_link,
    STATE(101), 1,
      sym_description,
    ACTIONS(89), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(87), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [637] = 9,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(31), 1,
      sym__text,
    ACTIONS(49), 1,
      sym_function_link,
    STATE(114), 1,
      sym_description,
    ACTIONS(109), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(15), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(107), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [678] = 8,
    ACTIONS(25), 1,
      anon_sym_BSLASHa,
    ACTIONS(27), 1,
      anon_sym_BSLASHc,
    ACTIONS(29), 1,
      anon_sym_LTa,
    ACTIONS(115), 1,
      sym_function_link,
    ACTIONS(117), 1,
      sym__text,
    ACTIONS(113), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(16), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(111), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [716] = 8,
    ACTIONS(123), 1,
      anon_sym_BSLASHa,
    ACTIONS(126), 1,
      anon_sym_BSLASHc,
    ACTIONS(129), 1,
      anon_sym_LTa,
    ACTIONS(132), 1,
      sym_function_link,
    ACTIONS(135), 1,
      sym__text,
    ACTIONS(121), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    STATE(16), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(119), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [754] = 5,
    ACTIONS(142), 1,
      anon_sym_COLON_COLON,
    ACTIONS(144), 1,
      anon_sym_LPAREN,
    STATE(24), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(138), 6,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(140), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [786] = 17,
    ACTIONS(63), 1,
      sym_tag_name_with_argument,
    ACTIONS(65), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(67), 1,
      sym_tag_name_with_types,
    ACTIONS(69), 1,
      sym_tag_name_with_self_types,
    ACTIONS(71), 1,
      sym_tag_name_with_type,
    ACTIONS(73), 1,
      sym_tag_name,
    ACTIONS(75), 1,
      anon_sym_LBRACK,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(83), 1,
      sym__text,
    ACTIONS(99), 1,
      sym_function_link,
    ACTIONS(146), 1,
      ts_builtin_sym_end,
    STATE(37), 1,
      sym_storageclass,
    STATE(122), 1,
      sym_description,
    STATE(125), 2,
      sym_tag,
      aux_sym_document_repeat2,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
  [842] = 11,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(83), 1,
      sym__text,
    ACTIONS(99), 1,
      sym_function_link,
    ACTIONS(148), 1,
      anon_sym_COMMA,
    STATE(21), 1,
      aux_sym_tag_repeat1,
    STATE(131), 1,
      sym_description,
    ACTIONS(89), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(87), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [885] = 11,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(83), 1,
      sym__text,
    ACTIONS(99), 1,
      sym_function_link,
    ACTIONS(150), 1,
      aux_sym_identifier_token1,
    STATE(38), 1,
      sym_identifier,
    STATE(128), 1,
      sym_description,
    ACTIONS(47), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(41), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [928] = 11,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(83), 1,
      sym__text,
    ACTIONS(99), 1,
      sym_function_link,
    ACTIONS(148), 1,
      anon_sym_COMMA,
    STATE(79), 1,
      aux_sym_tag_repeat1,
    STATE(127), 1,
      sym_description,
    ACTIONS(105), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(103), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [971] = 5,
    ACTIONS(152), 1,
      anon_sym_COLON_COLON,
    ACTIONS(154), 1,
      anon_sym_LPAREN,
    STATE(28), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(138), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(140), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1002] = 11,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(83), 1,
      sym__text,
    ACTIONS(99), 1,
      sym_function_link,
    ACTIONS(148), 1,
      anon_sym_COMMA,
    STATE(79), 1,
      aux_sym_tag_repeat1,
    STATE(134), 1,
      sym_description,
    ACTIONS(93), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(91), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [1045] = 4,
    ACTIONS(142), 1,
      anon_sym_COLON_COLON,
    STATE(25), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(156), 6,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(158), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1074] = 4,
    ACTIONS(164), 1,
      anon_sym_COLON_COLON,
    STATE(25), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(160), 6,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(162), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1103] = 11,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(83), 1,
      sym__text,
    ACTIONS(99), 1,
      sym_function_link,
    ACTIONS(148), 1,
      anon_sym_COMMA,
    STATE(23), 1,
      aux_sym_tag_repeat1,
    STATE(127), 1,
      sym_description,
    ACTIONS(105), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(103), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [1146] = 2,
    ACTIONS(167), 7,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LPAREN,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(169), 13,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1171] = 4,
    ACTIONS(152), 1,
      anon_sym_COLON_COLON,
    STATE(34), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(156), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(158), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1199] = 4,
    ACTIONS(171), 1,
      anon_sym_COMMA,
    STATE(29), 1,
      aux_sym_tag_repeat1,
    ACTIONS(176), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(174), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1227] = 2,
    ACTIONS(167), 6,
      sym_code_block_start,
      anon_sym_LPAREN,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(169), 13,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1251] = 5,
    ACTIONS(180), 1,
      sym_tag_name,
    ACTIONS(185), 1,
      sym_brief_text,
    STATE(31), 1,
      aux_sym_brief_description_repeat1,
    ACTIONS(183), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(178), 11,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1281] = 2,
    ACTIONS(160), 6,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(162), 13,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1305] = 2,
    ACTIONS(167), 6,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(169), 13,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1329] = 4,
    ACTIONS(188), 1,
      anon_sym_COLON_COLON,
    STATE(34), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(160), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(162), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1357] = 5,
    ACTIONS(193), 1,
      sym_tag_name,
    ACTIONS(197), 1,
      sym_brief_text,
    STATE(31), 1,
      aux_sym_brief_description_repeat1,
    ACTIONS(195), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(191), 11,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1387] = 9,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(83), 1,
      sym__text,
    ACTIONS(99), 1,
      sym_function_link,
    STATE(131), 1,
      sym_description,
    ACTIONS(89), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(87), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [1424] = 9,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(83), 1,
      sym__text,
    ACTIONS(99), 1,
      sym_function_link,
    STATE(128), 1,
      sym_description,
    ACTIONS(47), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(41), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [1461] = 9,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(83), 1,
      sym__text,
    ACTIONS(99), 1,
      sym_function_link,
    STATE(136), 1,
      sym_description,
    ACTIONS(109), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(59), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(107), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [1498] = 2,
    ACTIONS(160), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(162), 13,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1521] = 2,
    ACTIONS(199), 6,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(201), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1544] = 8,
    ACTIONS(203), 1,
      anon_sym_COMMA,
    ACTIONS(205), 1,
      aux_sym_tag_token1,
    ACTIONS(209), 1,
      aux_sym_identifier_token1,
    ACTIONS(213), 1,
      sym_function_link,
    STATE(47), 1,
      aux_sym_tag_repeat2,
    STATE(109), 1,
      sym_identifier,
    ACTIONS(211), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(207), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1579] = 2,
    ACTIONS(215), 6,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(217), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1602] = 2,
    ACTIONS(176), 6,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(174), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1625] = 2,
    ACTIONS(167), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(169), 13,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1648] = 15,
    ACTIONS(11), 1,
      sym_tag_name_with_argument,
    ACTIONS(13), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(15), 1,
      sym_tag_name_with_types,
    ACTIONS(17), 1,
      sym_tag_name_with_self_types,
    ACTIONS(19), 1,
      sym_tag_name_with_type,
    ACTIONS(21), 1,
      sym_tag_name,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(33), 1,
      anon_sym_ATcode,
    ACTIONS(39), 1,
      sym_code_block_start,
    ACTIONS(219), 1,
      anon_sym_SLASH,
    ACTIONS(221), 1,
      anon_sym_STAR_SLASH,
    ACTIONS(223), 1,
      sym__text_line,
    STATE(12), 1,
      sym_storageclass,
    STATE(203), 1,
      sym__multiline_end,
    STATE(71), 3,
      sym_tag,
      sym_code_block,
      aux_sym_document_repeat1,
  [1696] = 15,
    ACTIONS(11), 1,
      sym_tag_name_with_argument,
    ACTIONS(13), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(15), 1,
      sym_tag_name_with_types,
    ACTIONS(17), 1,
      sym_tag_name_with_self_types,
    ACTIONS(19), 1,
      sym_tag_name_with_type,
    ACTIONS(21), 1,
      sym_tag_name,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(33), 1,
      anon_sym_ATcode,
    ACTIONS(39), 1,
      sym_code_block_start,
    ACTIONS(223), 1,
      sym__text_line,
    ACTIONS(225), 1,
      anon_sym_SLASH,
    ACTIONS(227), 1,
      anon_sym_STAR_SLASH,
    STATE(12), 1,
      sym_storageclass,
    STATE(168), 1,
      sym__multiline_end,
    STATE(71), 3,
      sym_tag,
      sym_code_block,
      aux_sym_document_repeat1,
  [1744] = 7,
    ACTIONS(203), 1,
      anon_sym_COMMA,
    ACTIONS(209), 1,
      aux_sym_identifier_token1,
    ACTIONS(233), 1,
      sym_function_link,
    STATE(66), 1,
      aux_sym_tag_repeat2,
    STATE(117), 1,
      sym_identifier,
    ACTIONS(231), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(229), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1776] = 7,
    ACTIONS(203), 1,
      anon_sym_COMMA,
    ACTIONS(209), 1,
      aux_sym_identifier_token1,
    ACTIONS(239), 1,
      sym_function_link,
    STATE(49), 1,
      aux_sym_tag_repeat2,
    STATE(113), 1,
      sym_identifier,
    ACTIONS(237), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(235), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1808] = 7,
    ACTIONS(203), 1,
      anon_sym_COMMA,
    ACTIONS(209), 1,
      aux_sym_identifier_token1,
    ACTIONS(245), 1,
      sym_function_link,
    STATE(66), 1,
      aux_sym_tag_repeat2,
    STATE(100), 1,
      sym_identifier,
    ACTIONS(243), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(241), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1840] = 8,
    ACTIONS(247), 1,
      anon_sym_BSLASHa,
    ACTIONS(250), 1,
      anon_sym_BSLASHc,
    ACTIONS(253), 1,
      anon_sym_LTa,
    ACTIONS(256), 1,
      sym_function_link,
    ACTIONS(259), 1,
      sym__text,
    ACTIONS(121), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(50), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(119), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [1874] = 15,
    ACTIONS(11), 1,
      sym_tag_name_with_argument,
    ACTIONS(13), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(15), 1,
      sym_tag_name_with_types,
    ACTIONS(17), 1,
      sym_tag_name_with_self_types,
    ACTIONS(19), 1,
      sym_tag_name_with_type,
    ACTIONS(21), 1,
      sym_tag_name,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(33), 1,
      anon_sym_ATcode,
    ACTIONS(39), 1,
      sym_code_block_start,
    ACTIONS(51), 1,
      anon_sym_SLASH,
    ACTIONS(53), 1,
      anon_sym_STAR_SLASH,
    ACTIONS(55), 1,
      sym__text_line,
    STATE(12), 1,
      sym_storageclass,
    STATE(170), 1,
      sym__multiline_end,
    STATE(45), 3,
      sym_tag,
      sym_code_block,
      aux_sym_document_repeat1,
  [1922] = 15,
    ACTIONS(11), 1,
      sym_tag_name_with_argument,
    ACTIONS(13), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(15), 1,
      sym_tag_name_with_types,
    ACTIONS(17), 1,
      sym_tag_name_with_self_types,
    ACTIONS(19), 1,
      sym_tag_name_with_type,
    ACTIONS(21), 1,
      sym_tag_name,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(33), 1,
      anon_sym_ATcode,
    ACTIONS(39), 1,
      sym_code_block_start,
    ACTIONS(51), 1,
      anon_sym_SLASH,
    ACTIONS(53), 1,
      anon_sym_STAR_SLASH,
    ACTIONS(223), 1,
      sym__text_line,
    STATE(12), 1,
      sym_storageclass,
    STATE(170), 1,
      sym__multiline_end,
    STATE(71), 3,
      sym_tag,
      sym_code_block,
      aux_sym_document_repeat1,
  [1970] = 2,
    ACTIONS(264), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(262), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [1992] = 15,
    ACTIONS(11), 1,
      sym_tag_name_with_argument,
    ACTIONS(13), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(15), 1,
      sym_tag_name_with_types,
    ACTIONS(17), 1,
      sym_tag_name_with_self_types,
    ACTIONS(19), 1,
      sym_tag_name_with_type,
    ACTIONS(21), 1,
      sym_tag_name,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(33), 1,
      anon_sym_ATcode,
    ACTIONS(39), 1,
      sym_code_block_start,
    ACTIONS(219), 1,
      anon_sym_SLASH,
    ACTIONS(221), 1,
      anon_sym_STAR_SLASH,
    ACTIONS(266), 1,
      sym__text_line,
    STATE(12), 1,
      sym_storageclass,
    STATE(203), 1,
      sym__multiline_end,
    STATE(46), 3,
      sym_tag,
      sym_code_block,
      aux_sym_document_repeat1,
  [2040] = 2,
    ACTIONS(270), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(268), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2062] = 2,
    ACTIONS(274), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(272), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2084] = 2,
    ACTIONS(278), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(276), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2106] = 2,
    ACTIONS(167), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(169), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2128] = 8,
    ACTIONS(77), 1,
      anon_sym_BSLASHa,
    ACTIONS(79), 1,
      anon_sym_BSLASHc,
    ACTIONS(81), 1,
      anon_sym_LTa,
    ACTIONS(280), 1,
      sym_function_link,
    ACTIONS(282), 1,
      sym__text,
    ACTIONS(113), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    STATE(50), 4,
      sym_emphasis,
      sym_code_word,
      sym_link,
      aux_sym_description_repeat1,
    ACTIONS(111), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [2162] = 2,
    ACTIONS(286), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(284), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2184] = 2,
    ACTIONS(290), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(288), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2206] = 2,
    ACTIONS(215), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(217), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2228] = 2,
    ACTIONS(199), 5,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(201), 12,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      anon_sym_ATcode,
      sym__text,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2250] = 5,
    ACTIONS(292), 1,
      anon_sym_COLON_COLON,
    ACTIONS(294), 1,
      anon_sym_LPAREN,
    STATE(67), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(138), 5,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(140), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2278] = 3,
    ACTIONS(298), 1,
      aux_sym_tag_token1,
    ACTIONS(296), 5,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(300), 10,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      aux_sym_identifier_token1,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2301] = 4,
    ACTIONS(302), 1,
      anon_sym_COMMA,
    STATE(66), 1,
      aux_sym_tag_repeat2,
    ACTIONS(307), 4,
      sym_code_block_start,
      anon_sym_LBRACK,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(305), 10,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      aux_sym_identifier_token1,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2326] = 4,
    ACTIONS(292), 1,
      anon_sym_COLON_COLON,
    STATE(72), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(156), 5,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(158), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2351] = 2,
    ACTIONS(167), 6,
      sym_code_block_start,
      anon_sym_COMMA,
      aux_sym_tag_token1,
      anon_sym_LBRACK,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(169), 10,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      aux_sym_identifier_token1,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2372] = 2,
    ACTIONS(167), 6,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LPAREN,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(169), 10,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2393] = 5,
    ACTIONS(309), 1,
      anon_sym_COLON_COLON,
    ACTIONS(311), 1,
      anon_sym_LPAREN,
    STATE(74), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(138), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(140), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2420] = 14,
    ACTIONS(313), 1,
      sym_tag_name_with_argument,
    ACTIONS(316), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(319), 1,
      sym_tag_name_with_types,
    ACTIONS(322), 1,
      sym_tag_name_with_self_types,
    ACTIONS(325), 1,
      sym_tag_name_with_type,
    ACTIONS(328), 1,
      sym_tag_name,
    ACTIONS(331), 1,
      anon_sym_LBRACK,
    ACTIONS(334), 1,
      anon_sym_ATcode,
    ACTIONS(337), 1,
      anon_sym_SLASH,
    ACTIONS(339), 1,
      anon_sym_STAR_SLASH,
    ACTIONS(341), 1,
      sym__text_line,
    ACTIONS(344), 1,
      sym_code_block_start,
    STATE(12), 1,
      sym_storageclass,
    STATE(71), 3,
      sym_tag,
      sym_code_block,
      aux_sym_document_repeat1,
  [2465] = 4,
    ACTIONS(347), 1,
      anon_sym_COLON_COLON,
    STATE(72), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(160), 5,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(162), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2490] = 4,
    ACTIONS(350), 1,
      anon_sym_COLON_COLON,
    STATE(73), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(160), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(162), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2514] = 4,
    ACTIONS(309), 1,
      anon_sym_COLON_COLON,
    STATE(73), 1,
      aux_sym_qualified_identifier_repeat1,
    ACTIONS(156), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(158), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2538] = 2,
    ACTIONS(353), 5,
      sym_code_block_start,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      sym_function_link,
      anon_sym_SLASH,
    ACTIONS(355), 10,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      aux_sym_identifier_token1,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [2558] = 2,
    ACTIONS(160), 5,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(162), 10,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2578] = 2,
    ACTIONS(167), 5,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(169), 10,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2598] = 2,
    ACTIONS(167), 5,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(169), 10,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2618] = 4,
    ACTIONS(357), 1,
      anon_sym_COMMA,
    STATE(79), 1,
      aux_sym_tag_repeat1,
    ACTIONS(176), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(174), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2642] = 5,
    ACTIONS(360), 1,
      sym_tag_name,
    ACTIONS(363), 1,
      sym_brief_text,
    STATE(80), 1,
      aux_sym_brief_description_repeat1,
    ACTIONS(183), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(178), 8,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2668] = 5,
    ACTIONS(366), 1,
      sym_tag_name,
    ACTIONS(368), 1,
      sym_brief_text,
    STATE(80), 1,
      aux_sym_brief_description_repeat1,
    ACTIONS(195), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(191), 8,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2694] = 2,
    ACTIONS(215), 5,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(217), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2713] = 2,
    ACTIONS(167), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(169), 10,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2732] = 2,
    ACTIONS(160), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(162), 10,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_COLON_COLON,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2751] = 2,
    ACTIONS(176), 5,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(174), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2770] = 2,
    ACTIONS(199), 5,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(201), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2789] = 8,
    ACTIONS(370), 1,
      anon_sym_COMMA,
    ACTIONS(372), 1,
      aux_sym_tag_token1,
    ACTIONS(374), 1,
      aux_sym_identifier_token1,
    ACTIONS(376), 1,
      sym_function_link,
    STATE(95), 1,
      aux_sym_tag_repeat2,
    STATE(139), 1,
      sym_identifier,
    ACTIONS(211), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(207), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [2820] = 7,
    ACTIONS(370), 1,
      anon_sym_COMMA,
    ACTIONS(374), 1,
      aux_sym_identifier_token1,
    ACTIONS(378), 1,
      sym_function_link,
    STATE(92), 1,
      aux_sym_tag_repeat2,
    STATE(126), 1,
      sym_identifier,
    ACTIONS(237), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(235), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [2848] = 2,
    ACTIONS(286), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(284), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2866] = 2,
    ACTIONS(274), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(272), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2884] = 2,
    ACTIONS(264), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(262), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2902] = 7,
    ACTIONS(370), 1,
      anon_sym_COMMA,
    ACTIONS(374), 1,
      aux_sym_identifier_token1,
    ACTIONS(380), 1,
      sym_function_link,
    STATE(107), 1,
      aux_sym_tag_repeat2,
    STATE(132), 1,
      sym_identifier,
    ACTIONS(243), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(241), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [2930] = 2,
    ACTIONS(167), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(169), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2948] = 2,
    ACTIONS(199), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(201), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [2966] = 7,
    ACTIONS(370), 1,
      anon_sym_COMMA,
    ACTIONS(374), 1,
      aux_sym_identifier_token1,
    ACTIONS(382), 1,
      sym_function_link,
    STATE(107), 1,
      aux_sym_tag_repeat2,
    STATE(137), 1,
      sym_identifier,
    ACTIONS(231), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(229), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [2994] = 2,
    ACTIONS(278), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(276), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [3012] = 2,
    ACTIONS(290), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(288), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [3030] = 2,
    ACTIONS(215), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(217), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [3048] = 2,
    ACTIONS(270), 4,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      anon_sym_LTa,
      sym_function_link,
    ACTIONS(268), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_BSLASHa,
      anon_sym_BSLASHc,
      sym__text,
  [3066] = 2,
    ACTIONS(386), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(384), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3083] = 2,
    ACTIONS(105), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(103), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3100] = 3,
    ACTIONS(388), 1,
      aux_sym_tag_token1,
    ACTIONS(296), 4,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      sym_function_link,
    ACTIONS(300), 7,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      aux_sym_identifier_token1,
  [3119] = 2,
    ACTIONS(392), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(390), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3136] = 2,
    ACTIONS(396), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(394), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3153] = 2,
    ACTIONS(386), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(384), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3170] = 2,
    ACTIONS(400), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(398), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3187] = 4,
    ACTIONS(402), 1,
      anon_sym_COMMA,
    STATE(107), 1,
      aux_sym_tag_repeat2,
    ACTIONS(307), 3,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
      sym_function_link,
    ACTIONS(305), 7,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      aux_sym_identifier_token1,
  [3208] = 2,
    ACTIONS(407), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(405), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3225] = 2,
    ACTIONS(411), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(409), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3242] = 2,
    ACTIONS(167), 5,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      aux_sym_tag_token1,
      anon_sym_LBRACK,
      sym_function_link,
    ACTIONS(169), 7,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      aux_sym_identifier_token1,
  [3259] = 2,
    ACTIONS(89), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(87), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3276] = 2,
    ACTIONS(415), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(413), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3293] = 2,
    ACTIONS(419), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(417), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3310] = 2,
    ACTIONS(423), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(421), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3327] = 2,
    ACTIONS(109), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(107), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3344] = 2,
    ACTIONS(167), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(169), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3361] = 2,
    ACTIONS(400), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(398), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3378] = 2,
    ACTIONS(93), 3,
      sym_code_block_start,
      anon_sym_LBRACK,
      anon_sym_SLASH,
    ACTIONS(91), 9,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      anon_sym_ATcode,
      anon_sym_STAR_SLASH,
      sym__text_line,
  [3395] = 10,
    ACTIONS(63), 1,
      sym_tag_name_with_argument,
    ACTIONS(65), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(67), 1,
      sym_tag_name_with_types,
    ACTIONS(69), 1,
      sym_tag_name_with_self_types,
    ACTIONS(71), 1,
      sym_tag_name_with_type,
    ACTIONS(73), 1,
      sym_tag_name,
    ACTIONS(75), 1,
      anon_sym_LBRACK,
    ACTIONS(146), 1,
      ts_builtin_sym_end,
    STATE(37), 1,
      sym_storageclass,
    STATE(125), 2,
      sym_tag,
      aux_sym_document_repeat2,
  [3427] = 10,
    ACTIONS(63), 1,
      sym_tag_name_with_argument,
    ACTIONS(65), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(67), 1,
      sym_tag_name_with_types,
    ACTIONS(69), 1,
      sym_tag_name_with_self_types,
    ACTIONS(71), 1,
      sym_tag_name_with_type,
    ACTIONS(73), 1,
      sym_tag_name,
    ACTIONS(75), 1,
      anon_sym_LBRACK,
    ACTIONS(146), 1,
      ts_builtin_sym_end,
    STATE(37), 1,
      sym_storageclass,
    STATE(123), 2,
      sym_tag,
      aux_sym_document_repeat2,
  [3459] = 2,
    ACTIONS(353), 4,
      ts_builtin_sym_end,
      anon_sym_COMMA,
      anon_sym_LBRACK,
      sym_function_link,
    ACTIONS(355), 7,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
      aux_sym_identifier_token1,
  [3475] = 10,
    ACTIONS(63), 1,
      sym_tag_name_with_argument,
    ACTIONS(65), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(67), 1,
      sym_tag_name_with_types,
    ACTIONS(69), 1,
      sym_tag_name_with_self_types,
    ACTIONS(71), 1,
      sym_tag_name_with_type,
    ACTIONS(73), 1,
      sym_tag_name,
    ACTIONS(75), 1,
      anon_sym_LBRACK,
    ACTIONS(425), 1,
      ts_builtin_sym_end,
    STATE(37), 1,
      sym_storageclass,
    STATE(124), 2,
      sym_tag,
      aux_sym_document_repeat2,
  [3507] = 10,
    ACTIONS(427), 1,
      ts_builtin_sym_end,
    ACTIONS(429), 1,
      sym_tag_name_with_argument,
    ACTIONS(432), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(435), 1,
      sym_tag_name_with_types,
    ACTIONS(438), 1,
      sym_tag_name_with_self_types,
    ACTIONS(441), 1,
      sym_tag_name_with_type,
    ACTIONS(444), 1,
      sym_tag_name,
    ACTIONS(447), 1,
      anon_sym_LBRACK,
    STATE(37), 1,
      sym_storageclass,
    STATE(123), 2,
      sym_tag,
      aux_sym_document_repeat2,
  [3539] = 10,
    ACTIONS(63), 1,
      sym_tag_name_with_argument,
    ACTIONS(65), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(67), 1,
      sym_tag_name_with_types,
    ACTIONS(69), 1,
      sym_tag_name_with_self_types,
    ACTIONS(71), 1,
      sym_tag_name_with_type,
    ACTIONS(73), 1,
      sym_tag_name,
    ACTIONS(75), 1,
      anon_sym_LBRACK,
    ACTIONS(450), 1,
      ts_builtin_sym_end,
    STATE(37), 1,
      sym_storageclass,
    STATE(123), 2,
      sym_tag,
      aux_sym_document_repeat2,
  [3571] = 10,
    ACTIONS(63), 1,
      sym_tag_name_with_argument,
    ACTIONS(65), 1,
      sym_tag_name_with_multiple_arguments,
    ACTIONS(67), 1,
      sym_tag_name_with_types,
    ACTIONS(69), 1,
      sym_tag_name_with_self_types,
    ACTIONS(71), 1,
      sym_tag_name_with_type,
    ACTIONS(73), 1,
      sym_tag_name,
    ACTIONS(75), 1,
      anon_sym_LBRACK,
    ACTIONS(425), 1,
      ts_builtin_sym_end,
    STATE(37), 1,
      sym_storageclass,
    STATE(123), 2,
      sym_tag,
      aux_sym_document_repeat2,
  [3603] = 2,
    ACTIONS(419), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(417), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3616] = 2,
    ACTIONS(93), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(91), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3629] = 2,
    ACTIONS(89), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(87), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3642] = 2,
    ACTIONS(109), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(107), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3655] = 6,
    ACTIONS(452), 1,
      aux_sym_identifier_token1,
    ACTIONS(454), 1,
      anon_sym_TILDE,
    ACTIONS(456), 1,
      anon_sym_LBRACK,
    STATE(17), 1,
      sym_identifier,
    STATE(142), 1,
      sym_storageclass,
    STATE(6), 3,
      sym__expression,
      sym_qualified_identifier,
      sym_function,
  [3676] = 2,
    ACTIONS(105), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(103), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3689] = 2,
    ACTIONS(386), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(384), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3702] = 2,
    ACTIONS(386), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(384), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3715] = 2,
    ACTIONS(392), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(390), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3728] = 6,
    ACTIONS(456), 1,
      anon_sym_LBRACK,
    ACTIONS(458), 1,
      aux_sym_identifier_token1,
    ACTIONS(460), 1,
      anon_sym_TILDE,
    STATE(64), 1,
      sym_identifier,
    STATE(140), 1,
      sym_storageclass,
    STATE(19), 3,
      sym__expression,
      sym_qualified_identifier,
      sym_function,
  [3749] = 2,
    ACTIONS(423), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(421), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3762] = 2,
    ACTIONS(400), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(398), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3775] = 2,
    ACTIONS(400), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(398), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3788] = 2,
    ACTIONS(411), 2,
      ts_builtin_sym_end,
      anon_sym_LBRACK,
    ACTIONS(409), 6,
      sym_tag_name_with_argument,
      sym_tag_name_with_multiple_arguments,
      sym_tag_name_with_types,
      sym_tag_name_with_self_types,
      sym_tag_name_with_type,
      sym_tag_name,
  [3801] = 4,
    ACTIONS(458), 1,
      aux_sym_identifier_token1,
    ACTIONS(460), 1,
      anon_sym_TILDE,
    STATE(64), 1,
      sym_identifier,
    STATE(26), 3,
      sym__expression,
      sym_qualified_identifier,
      sym_function,
  [3816] = 4,
    ACTIONS(452), 1,
      aux_sym_identifier_token1,
    ACTIONS(454), 1,
      anon_sym_TILDE,
    STATE(17), 1,
      sym_identifier,
    STATE(43), 3,
      sym__expression,
      sym_qualified_identifier,
      sym_function,
  [3831] = 4,
    ACTIONS(452), 1,
      aux_sym_identifier_token1,
    ACTIONS(454), 1,
      anon_sym_TILDE,
    STATE(17), 1,
      sym_identifier,
    STATE(11), 3,
      sym__expression,
      sym_qualified_identifier,
      sym_function,
  [3846] = 4,
    ACTIONS(458), 1,
      aux_sym_identifier_token1,
    ACTIONS(460), 1,
      anon_sym_TILDE,
    STATE(64), 1,
      sym_identifier,
    STATE(85), 3,
      sym__expression,
      sym_qualified_identifier,
      sym_function,
  [3861] = 3,
    STATE(35), 1,
      aux_sym_brief_description_repeat1,
    STATE(56), 1,
      sym_brief_description,
    ACTIONS(462), 2,
      sym_brief_text,
      sym_tag_name,
  [3872] = 3,
    STATE(81), 1,
      aux_sym_brief_description_repeat1,
    STATE(90), 1,
      sym_brief_description,
    ACTIONS(464), 2,
      sym_brief_text,
      sym_tag_name,
  [3883] = 3,
    ACTIONS(466), 1,
      aux_sym_identifier_token1,
    ACTIONS(468), 1,
      sym_function_link,
    STATE(65), 1,
      sym_identifier,
  [3893] = 2,
    ACTIONS(470), 1,
      anon_sym_in,
    ACTIONS(472), 2,
      anon_sym_out,
      anon_sym_inout,
  [3901] = 2,
    ACTIONS(474), 1,
      anon_sym_in,
    ACTIONS(476), 2,
      anon_sym_out,
      anon_sym_inout,
  [3909] = 2,
    ACTIONS(478), 1,
      anon_sym_in,
    ACTIONS(480), 2,
      anon_sym_out,
      anon_sym_inout,
  [3917] = 3,
    ACTIONS(374), 1,
      aux_sym_identifier_token1,
    ACTIONS(482), 1,
      sym_function_link,
    STATE(102), 1,
      sym_identifier,
  [3927] = 3,
    ACTIONS(466), 1,
      aux_sym_identifier_token1,
    ACTIONS(484), 1,
      sym_function_link,
    STATE(41), 1,
      sym_identifier,
  [3937] = 3,
    ACTIONS(374), 1,
      aux_sym_identifier_token1,
    ACTIONS(486), 1,
      sym_function_link,
    STATE(87), 1,
      sym_identifier,
  [3947] = 2,
    ACTIONS(488), 1,
      aux_sym_identifier_token1,
    STATE(200), 1,
      sym_identifier,
  [3954] = 2,
    ACTIONS(488), 1,
      aux_sym_identifier_token1,
    STATE(199), 1,
      sym_identifier,
  [3961] = 2,
    ACTIONS(488), 1,
      aux_sym_identifier_token1,
    STATE(196), 1,
      sym_identifier,
  [3968] = 2,
    ACTIONS(490), 1,
      anon_sym_LBRACE,
    ACTIONS(492), 1,
      sym_code_block_content,
  [3975] = 2,
    ACTIONS(494), 1,
      aux_sym_identifier_token1,
    STATE(32), 1,
      sym_identifier,
  [3982] = 1,
    ACTIONS(278), 2,
      aux_sym_identifier_token1,
      anon_sym_TILDE,
  [3987] = 2,
    ACTIONS(496), 1,
      aux_sym_identifier_token1,
    STATE(39), 1,
      sym_identifier,
  [3994] = 2,
    ACTIONS(488), 1,
      aux_sym_identifier_token1,
    STATE(169), 1,
      sym_identifier,
  [4001] = 2,
    ACTIONS(498), 1,
      aux_sym_identifier_token1,
    STATE(76), 1,
      sym_identifier,
  [4008] = 2,
    ACTIONS(500), 1,
      aux_sym_identifier_token1,
    STATE(84), 1,
      sym_identifier,
  [4015] = 1,
    ACTIONS(502), 1,
      sym_code_block_content,
  [4019] = 1,
    ACTIONS(504), 1,
      anon_sym_RBRACK,
  [4023] = 1,
    ACTIONS(506), 1,
      aux_sym_identifier_token1,
  [4027] = 1,
    ACTIONS(508), 1,
      aux_sym_tag_token2,
  [4031] = 1,
    ACTIONS(510), 1,
      anon_sym_RBRACE,
  [4035] = 1,
    ACTIONS(512), 1,
      ts_builtin_sym_end,
  [4039] = 1,
    ACTIONS(514), 1,
      anon_sym_LPAREN,
  [4043] = 1,
    ACTIONS(425), 1,
      ts_builtin_sym_end,
  [4047] = 1,
    ACTIONS(516), 1,
      anon_sym_DOT,
  [4051] = 1,
    ACTIONS(518), 1,
      anon_sym_ATendcode,
  [4055] = 1,
    ACTIONS(146), 1,
      ts_builtin_sym_end,
  [4059] = 1,
    ACTIONS(520), 1,
      anon_sym_LT_SLASHa_GT,
  [4063] = 1,
    ACTIONS(522), 1,
      anon_sym_ATendcode,
  [4067] = 1,
    ACTIONS(524), 1,
      anon_sym_GT,
  [4071] = 1,
    ACTIONS(526), 1,
      aux_sym_tag_token2,
  [4075] = 1,
    ACTIONS(528), 1,
      aux_sym_identifier_token1,
  [4079] = 1,
    ACTIONS(530), 1,
      aux_sym_identifier_token1,
  [4083] = 1,
    ACTIONS(532), 1,
      ts_builtin_sym_end,
  [4087] = 1,
    ACTIONS(534), 1,
      anon_sym_RBRACK,
  [4091] = 1,
    ACTIONS(536), 1,
      aux_sym_link_token1,
  [4095] = 1,
    ACTIONS(538), 1,
      anon_sym_RPAREN,
  [4099] = 1,
    ACTIONS(540), 1,
      aux_sym_identifier_token1,
  [4103] = 1,
    ACTIONS(542), 1,
      sym_code_block_content,
  [4107] = 1,
    ACTIONS(544), 1,
      anon_sym_RPAREN,
  [4111] = 1,
    ACTIONS(546), 1,
      anon_sym_LT_SLASHa_GT,
  [4115] = 1,
    ACTIONS(548), 1,
      anon_sym_RBRACK,
  [4119] = 1,
    ACTIONS(550), 1,
      anon_sym_RPAREN,
  [4123] = 1,
    ACTIONS(552), 1,
      anon_sym_RPAREN,
  [4127] = 1,
    ACTIONS(554), 1,
      anon_sym_RPAREN,
  [4131] = 1,
    ACTIONS(556), 1,
      aux_sym_link_token2,
  [4135] = 1,
    ACTIONS(558), 1,
      anon_sym_RPAREN,
  [4139] = 1,
    ACTIONS(560), 1,
      anon_sym_RPAREN,
  [4143] = 1,
    ACTIONS(562), 1,
      sym_code_block_language,
  [4147] = 1,
    ACTIONS(564), 1,
      anon_sym_LPAREN,
  [4151] = 1,
    ACTIONS(566), 1,
      aux_sym_link_token2,
  [4155] = 1,
    ACTIONS(568), 1,
      sym_code_block_end,
  [4159] = 1,
    ACTIONS(570), 1,
      anon_sym_LPAREN,
  [4163] = 1,
    ACTIONS(572), 1,
      anon_sym_LPAREN,
  [4167] = 1,
    ACTIONS(574), 1,
      sym_code_block_language,
  [4171] = 1,
    ACTIONS(576), 1,
      anon_sym_GT,
  [4175] = 1,
    ACTIONS(450), 1,
      ts_builtin_sym_end,
  [4179] = 1,
    ACTIONS(578), 1,
      anon_sym_RPAREN,
  [4183] = 1,
    ACTIONS(580), 1,
      aux_sym_link_token1,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(2)] = 0,
  [SMALL_STATE(3)] = 78,
  [SMALL_STATE(4)] = 133,
  [SMALL_STATE(5)] = 205,
  [SMALL_STATE(6)] = 269,
  [SMALL_STATE(7)] = 316,
  [SMALL_STATE(8)] = 363,
  [SMALL_STATE(9)] = 414,
  [SMALL_STATE(10)] = 461,
  [SMALL_STATE(11)] = 508,
  [SMALL_STATE(12)] = 555,
  [SMALL_STATE(13)] = 596,
  [SMALL_STATE(14)] = 637,
  [SMALL_STATE(15)] = 678,
  [SMALL_STATE(16)] = 716,
  [SMALL_STATE(17)] = 754,
  [SMALL_STATE(18)] = 786,
  [SMALL_STATE(19)] = 842,
  [SMALL_STATE(20)] = 885,
  [SMALL_STATE(21)] = 928,
  [SMALL_STATE(22)] = 971,
  [SMALL_STATE(23)] = 1002,
  [SMALL_STATE(24)] = 1045,
  [SMALL_STATE(25)] = 1074,
  [SMALL_STATE(26)] = 1103,
  [SMALL_STATE(27)] = 1146,
  [SMALL_STATE(28)] = 1171,
  [SMALL_STATE(29)] = 1199,
  [SMALL_STATE(30)] = 1227,
  [SMALL_STATE(31)] = 1251,
  [SMALL_STATE(32)] = 1281,
  [SMALL_STATE(33)] = 1305,
  [SMALL_STATE(34)] = 1329,
  [SMALL_STATE(35)] = 1357,
  [SMALL_STATE(36)] = 1387,
  [SMALL_STATE(37)] = 1424,
  [SMALL_STATE(38)] = 1461,
  [SMALL_STATE(39)] = 1498,
  [SMALL_STATE(40)] = 1521,
  [SMALL_STATE(41)] = 1544,
  [SMALL_STATE(42)] = 1579,
  [SMALL_STATE(43)] = 1602,
  [SMALL_STATE(44)] = 1625,
  [SMALL_STATE(45)] = 1648,
  [SMALL_STATE(46)] = 1696,
  [SMALL_STATE(47)] = 1744,
  [SMALL_STATE(48)] = 1776,
  [SMALL_STATE(49)] = 1808,
  [SMALL_STATE(50)] = 1840,
  [SMALL_STATE(51)] = 1874,
  [SMALL_STATE(52)] = 1922,
  [SMALL_STATE(53)] = 1970,
  [SMALL_STATE(54)] = 1992,
  [SMALL_STATE(55)] = 2040,
  [SMALL_STATE(56)] = 2062,
  [SMALL_STATE(57)] = 2084,
  [SMALL_STATE(58)] = 2106,
  [SMALL_STATE(59)] = 2128,
  [SMALL_STATE(60)] = 2162,
  [SMALL_STATE(61)] = 2184,
  [SMALL_STATE(62)] = 2206,
  [SMALL_STATE(63)] = 2228,
  [SMALL_STATE(64)] = 2250,
  [SMALL_STATE(65)] = 2278,
  [SMALL_STATE(66)] = 2301,
  [SMALL_STATE(67)] = 2326,
  [SMALL_STATE(68)] = 2351,
  [SMALL_STATE(69)] = 2372,
  [SMALL_STATE(70)] = 2393,
  [SMALL_STATE(71)] = 2420,
  [SMALL_STATE(72)] = 2465,
  [SMALL_STATE(73)] = 2490,
  [SMALL_STATE(74)] = 2514,
  [SMALL_STATE(75)] = 2538,
  [SMALL_STATE(76)] = 2558,
  [SMALL_STATE(77)] = 2578,
  [SMALL_STATE(78)] = 2598,
  [SMALL_STATE(79)] = 2618,
  [SMALL_STATE(80)] = 2642,
  [SMALL_STATE(81)] = 2668,
  [SMALL_STATE(82)] = 2694,
  [SMALL_STATE(83)] = 2713,
  [SMALL_STATE(84)] = 2732,
  [SMALL_STATE(85)] = 2751,
  [SMALL_STATE(86)] = 2770,
  [SMALL_STATE(87)] = 2789,
  [SMALL_STATE(88)] = 2820,
  [SMALL_STATE(89)] = 2848,
  [SMALL_STATE(90)] = 2866,
  [SMALL_STATE(91)] = 2884,
  [SMALL_STATE(92)] = 2902,
  [SMALL_STATE(93)] = 2930,
  [SMALL_STATE(94)] = 2948,
  [SMALL_STATE(95)] = 2966,
  [SMALL_STATE(96)] = 2994,
  [SMALL_STATE(97)] = 3012,
  [SMALL_STATE(98)] = 3030,
  [SMALL_STATE(99)] = 3048,
  [SMALL_STATE(100)] = 3066,
  [SMALL_STATE(101)] = 3083,
  [SMALL_STATE(102)] = 3100,
  [SMALL_STATE(103)] = 3119,
  [SMALL_STATE(104)] = 3136,
  [SMALL_STATE(105)] = 3153,
  [SMALL_STATE(106)] = 3170,
  [SMALL_STATE(107)] = 3187,
  [SMALL_STATE(108)] = 3208,
  [SMALL_STATE(109)] = 3225,
  [SMALL_STATE(110)] = 3242,
  [SMALL_STATE(111)] = 3259,
  [SMALL_STATE(112)] = 3276,
  [SMALL_STATE(113)] = 3293,
  [SMALL_STATE(114)] = 3310,
  [SMALL_STATE(115)] = 3327,
  [SMALL_STATE(116)] = 3344,
  [SMALL_STATE(117)] = 3361,
  [SMALL_STATE(118)] = 3378,
  [SMALL_STATE(119)] = 3395,
  [SMALL_STATE(120)] = 3427,
  [SMALL_STATE(121)] = 3459,
  [SMALL_STATE(122)] = 3475,
  [SMALL_STATE(123)] = 3507,
  [SMALL_STATE(124)] = 3539,
  [SMALL_STATE(125)] = 3571,
  [SMALL_STATE(126)] = 3603,
  [SMALL_STATE(127)] = 3616,
  [SMALL_STATE(128)] = 3629,
  [SMALL_STATE(129)] = 3642,
  [SMALL_STATE(130)] = 3655,
  [SMALL_STATE(131)] = 3676,
  [SMALL_STATE(132)] = 3689,
  [SMALL_STATE(133)] = 3702,
  [SMALL_STATE(134)] = 3715,
  [SMALL_STATE(135)] = 3728,
  [SMALL_STATE(136)] = 3749,
  [SMALL_STATE(137)] = 3762,
  [SMALL_STATE(138)] = 3775,
  [SMALL_STATE(139)] = 3788,
  [SMALL_STATE(140)] = 3801,
  [SMALL_STATE(141)] = 3816,
  [SMALL_STATE(142)] = 3831,
  [SMALL_STATE(143)] = 3846,
  [SMALL_STATE(144)] = 3861,
  [SMALL_STATE(145)] = 3872,
  [SMALL_STATE(146)] = 3883,
  [SMALL_STATE(147)] = 3893,
  [SMALL_STATE(148)] = 3901,
  [SMALL_STATE(149)] = 3909,
  [SMALL_STATE(150)] = 3917,
  [SMALL_STATE(151)] = 3927,
  [SMALL_STATE(152)] = 3937,
  [SMALL_STATE(153)] = 3947,
  [SMALL_STATE(154)] = 3954,
  [SMALL_STATE(155)] = 3961,
  [SMALL_STATE(156)] = 3968,
  [SMALL_STATE(157)] = 3975,
  [SMALL_STATE(158)] = 3982,
  [SMALL_STATE(159)] = 3987,
  [SMALL_STATE(160)] = 3994,
  [SMALL_STATE(161)] = 4001,
  [SMALL_STATE(162)] = 4008,
  [SMALL_STATE(163)] = 4015,
  [SMALL_STATE(164)] = 4019,
  [SMALL_STATE(165)] = 4023,
  [SMALL_STATE(166)] = 4027,
  [SMALL_STATE(167)] = 4031,
  [SMALL_STATE(168)] = 4035,
  [SMALL_STATE(169)] = 4039,
  [SMALL_STATE(170)] = 4043,
  [SMALL_STATE(171)] = 4047,
  [SMALL_STATE(172)] = 4051,
  [SMALL_STATE(173)] = 4055,
  [SMALL_STATE(174)] = 4059,
  [SMALL_STATE(175)] = 4063,
  [SMALL_STATE(176)] = 4067,
  [SMALL_STATE(177)] = 4071,
  [SMALL_STATE(178)] = 4075,
  [SMALL_STATE(179)] = 4079,
  [SMALL_STATE(180)] = 4083,
  [SMALL_STATE(181)] = 4087,
  [SMALL_STATE(182)] = 4091,
  [SMALL_STATE(183)] = 4095,
  [SMALL_STATE(184)] = 4099,
  [SMALL_STATE(185)] = 4103,
  [SMALL_STATE(186)] = 4107,
  [SMALL_STATE(187)] = 4111,
  [SMALL_STATE(188)] = 4115,
  [SMALL_STATE(189)] = 4119,
  [SMALL_STATE(190)] = 4123,
  [SMALL_STATE(191)] = 4127,
  [SMALL_STATE(192)] = 4131,
  [SMALL_STATE(193)] = 4135,
  [SMALL_STATE(194)] = 4139,
  [SMALL_STATE(195)] = 4143,
  [SMALL_STATE(196)] = 4147,
  [SMALL_STATE(197)] = 4151,
  [SMALL_STATE(198)] = 4155,
  [SMALL_STATE(199)] = 4159,
  [SMALL_STATE(200)] = 4163,
  [SMALL_STATE(201)] = 4167,
  [SMALL_STATE(202)] = 4171,
  [SMALL_STATE(203)] = 4175,
  [SMALL_STATE(204)] = 4179,
  [SMALL_STATE(205)] = 4183,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = false}}, SHIFT(5),
  [5] = {.entry = {.count = 1, .reusable = false}}, SHIFT(2),
  [7] = {.entry = {.count = 1, .reusable = false}}, SHIFT(144),
  [9] = {.entry = {.count = 1, .reusable = false}}, SHIFT(53),
  [11] = {.entry = {.count = 1, .reusable = false}}, SHIFT(3),
  [13] = {.entry = {.count = 1, .reusable = false}}, SHIFT(130),
  [15] = {.entry = {.count = 1, .reusable = false}}, SHIFT(151),
  [17] = {.entry = {.count = 1, .reusable = false}}, SHIFT(177),
  [19] = {.entry = {.count = 1, .reusable = false}}, SHIFT(9),
  [21] = {.entry = {.count = 1, .reusable = false}}, SHIFT(12),
  [23] = {.entry = {.count = 1, .reusable = true}}, SHIFT(149),
  [25] = {.entry = {.count = 1, .reusable = false}}, SHIFT(178),
  [27] = {.entry = {.count = 1, .reusable = false}}, SHIFT(179),
  [29] = {.entry = {.count = 1, .reusable = true}}, SHIFT(205),
  [31] = {.entry = {.count = 1, .reusable = false}}, SHIFT(15),
  [33] = {.entry = {.count = 1, .reusable = false}}, SHIFT(156),
  [35] = {.entry = {.count = 1, .reusable = false}}, SHIFT(173),
  [37] = {.entry = {.count = 1, .reusable = false}}, SHIFT(52),
  [39] = {.entry = {.count = 1, .reusable = true}}, SHIFT(195),
  [41] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 1, 0, 0),
  [43] = {.entry = {.count = 1, .reusable = false}}, SHIFT(30),
  [45] = {.entry = {.count = 1, .reusable = false}}, SHIFT(154),
  [47] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 1, 0, 0),
  [49] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [51] = {.entry = {.count = 1, .reusable = true}}, SHIFT(170),
  [53] = {.entry = {.count = 1, .reusable = false}}, SHIFT(170),
  [55] = {.entry = {.count = 1, .reusable = false}}, SHIFT(45),
  [57] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_document, 1, 0, 0),
  [59] = {.entry = {.count = 1, .reusable = false}}, SHIFT(145),
  [61] = {.entry = {.count = 1, .reusable = false}}, SHIFT(91),
  [63] = {.entry = {.count = 1, .reusable = false}}, SHIFT(8),
  [65] = {.entry = {.count = 1, .reusable = false}}, SHIFT(135),
  [67] = {.entry = {.count = 1, .reusable = false}}, SHIFT(152),
  [69] = {.entry = {.count = 1, .reusable = false}}, SHIFT(166),
  [71] = {.entry = {.count = 1, .reusable = false}}, SHIFT(20),
  [73] = {.entry = {.count = 1, .reusable = false}}, SHIFT(37),
  [75] = {.entry = {.count = 1, .reusable = true}}, SHIFT(147),
  [77] = {.entry = {.count = 1, .reusable = false}}, SHIFT(165),
  [79] = {.entry = {.count = 1, .reusable = false}}, SHIFT(184),
  [81] = {.entry = {.count = 1, .reusable = true}}, SHIFT(182),
  [83] = {.entry = {.count = 1, .reusable = false}}, SHIFT(59),
  [85] = {.entry = {.count = 1, .reusable = true}}, SHIFT(141),
  [87] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 2, 0, 0),
  [89] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 2, 0, 0),
  [91] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 4, 0, 0),
  [93] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 4, 0, 0),
  [95] = {.entry = {.count = 1, .reusable = false}}, SHIFT(77),
  [97] = {.entry = {.count = 1, .reusable = false}}, SHIFT(160),
  [99] = {.entry = {.count = 1, .reusable = true}}, SHIFT(59),
  [101] = {.entry = {.count = 1, .reusable = false}}, SHIFT(58),
  [103] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 3, 0, 0),
  [105] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 3, 0, 0),
  [107] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 2, 0, 2),
  [109] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 2, 0, 2),
  [111] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_description, 1, 0, 0),
  [113] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_description, 1, 0, 0),
  [115] = {.entry = {.count = 1, .reusable = true}}, SHIFT(16),
  [117] = {.entry = {.count = 1, .reusable = false}}, SHIFT(16),
  [119] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0),
  [121] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0),
  [123] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0), SHIFT_REPEAT(178),
  [126] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0), SHIFT_REPEAT(179),
  [129] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0), SHIFT_REPEAT(205),
  [132] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0), SHIFT_REPEAT(16),
  [135] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0), SHIFT_REPEAT(16),
  [138] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__expression, 1, 0, 0),
  [140] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__expression, 1, 0, 0),
  [142] = {.entry = {.count = 1, .reusable = false}}, SHIFT(157),
  [144] = {.entry = {.count = 1, .reusable = true}}, SHIFT(193),
  [146] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_document, 2, 0, 0),
  [148] = {.entry = {.count = 1, .reusable = true}}, SHIFT(143),
  [150] = {.entry = {.count = 1, .reusable = false}}, SHIFT(93),
  [152] = {.entry = {.count = 1, .reusable = false}}, SHIFT(159),
  [154] = {.entry = {.count = 1, .reusable = true}}, SHIFT(190),
  [156] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_qualified_identifier, 2, 0, 0),
  [158] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_qualified_identifier, 2, 0, 0),
  [160] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_qualified_identifier_repeat1, 2, 0, 0),
  [162] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_qualified_identifier_repeat1, 2, 0, 0),
  [164] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_qualified_identifier_repeat1, 2, 0, 0), SHIFT_REPEAT(157),
  [167] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_identifier, 1, 0, 0),
  [169] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_identifier, 1, 0, 0),
  [171] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tag_repeat1, 2, 0, 0), SHIFT_REPEAT(141),
  [174] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_tag_repeat1, 2, 0, 0),
  [176] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_tag_repeat1, 2, 0, 0),
  [178] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_brief_description_repeat1, 2, 0, 0),
  [180] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_brief_description_repeat1, 2, 0, 0), SHIFT_REPEAT(31),
  [183] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_brief_description_repeat1, 2, 0, 0),
  [185] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_brief_description_repeat1, 2, 0, 0), SHIFT_REPEAT(31),
  [188] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_qualified_identifier_repeat1, 2, 0, 0), SHIFT_REPEAT(159),
  [191] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_brief_description, 1, 0, 0),
  [193] = {.entry = {.count = 1, .reusable = false}}, SHIFT(31),
  [195] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_brief_description, 1, 0, 0),
  [197] = {.entry = {.count = 1, .reusable = true}}, SHIFT(31),
  [199] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function, 4, 0, 0),
  [201] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function, 4, 0, 0),
  [203] = {.entry = {.count = 1, .reusable = true}}, SHIFT(146),
  [205] = {.entry = {.count = 1, .reusable = true}}, SHIFT(48),
  [207] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 2, 0, 1),
  [209] = {.entry = {.count = 1, .reusable = false}}, SHIFT(116),
  [211] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 2, 0, 1),
  [213] = {.entry = {.count = 1, .reusable = true}}, SHIFT(109),
  [215] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function, 3, 0, 0),
  [217] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function, 3, 0, 0),
  [219] = {.entry = {.count = 1, .reusable = true}}, SHIFT(203),
  [221] = {.entry = {.count = 1, .reusable = false}}, SHIFT(203),
  [223] = {.entry = {.count = 1, .reusable = false}}, SHIFT(71),
  [225] = {.entry = {.count = 1, .reusable = true}}, SHIFT(168),
  [227] = {.entry = {.count = 1, .reusable = false}}, SHIFT(168),
  [229] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 3, 0, 6),
  [231] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 3, 0, 6),
  [233] = {.entry = {.count = 1, .reusable = true}}, SHIFT(106),
  [235] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 3, 0, 1),
  [237] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 3, 0, 1),
  [239] = {.entry = {.count = 1, .reusable = true}}, SHIFT(113),
  [241] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 4, 0, 8),
  [243] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 4, 0, 8),
  [245] = {.entry = {.count = 1, .reusable = true}}, SHIFT(105),
  [247] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0), SHIFT_REPEAT(165),
  [250] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0), SHIFT_REPEAT(184),
  [253] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0), SHIFT_REPEAT(182),
  [256] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0), SHIFT_REPEAT(50),
  [259] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_description_repeat1, 2, 0, 0), SHIFT_REPEAT(50),
  [262] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_brief_header, 1, 0, 0),
  [264] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_brief_header, 1, 0, 0),
  [266] = {.entry = {.count = 1, .reusable = false}}, SHIFT(46),
  [268] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_link, 5, 0, 0),
  [270] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_link, 5, 0, 0),
  [272] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_brief_header, 2, 0, 0),
  [274] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_brief_header, 2, 0, 0),
  [276] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_storageclass, 3, 0, 0),
  [278] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_storageclass, 3, 0, 0),
  [280] = {.entry = {.count = 1, .reusable = true}}, SHIFT(50),
  [282] = {.entry = {.count = 1, .reusable = false}}, SHIFT(50),
  [284] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_code_word, 2, 0, 4),
  [286] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_code_word, 2, 0, 4),
  [288] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_emphasis, 2, 0, 3),
  [290] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_emphasis, 2, 0, 3),
  [292] = {.entry = {.count = 1, .reusable = false}}, SHIFT(161),
  [294] = {.entry = {.count = 1, .reusable = true}}, SHIFT(183),
  [296] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_tag_repeat2, 2, 0, 1),
  [298] = {.entry = {.count = 1, .reusable = true}}, SHIFT(75),
  [300] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_tag_repeat2, 2, 0, 1),
  [302] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tag_repeat2, 2, 0, 10), SHIFT_REPEAT(146),
  [305] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_tag_repeat2, 2, 0, 10),
  [307] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_tag_repeat2, 2, 0, 10),
  [309] = {.entry = {.count = 1, .reusable = false}}, SHIFT(162),
  [311] = {.entry = {.count = 1, .reusable = true}}, SHIFT(189),
  [313] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(3),
  [316] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(130),
  [319] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(151),
  [322] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(177),
  [325] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(9),
  [328] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(12),
  [331] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(149),
  [334] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(156),
  [337] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0),
  [339] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0),
  [341] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(71),
  [344] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(195),
  [347] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_qualified_identifier_repeat1, 2, 0, 0), SHIFT_REPEAT(161),
  [350] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_qualified_identifier_repeat1, 2, 0, 0), SHIFT_REPEAT(162),
  [353] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_tag_repeat2, 3, 0, 1),
  [355] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_tag_repeat2, 3, 0, 1),
  [357] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tag_repeat1, 2, 0, 0), SHIFT_REPEAT(143),
  [360] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_brief_description_repeat1, 2, 0, 0), SHIFT_REPEAT(80),
  [363] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_brief_description_repeat1, 2, 0, 0), SHIFT_REPEAT(80),
  [366] = {.entry = {.count = 1, .reusable = false}}, SHIFT(80),
  [368] = {.entry = {.count = 1, .reusable = true}}, SHIFT(80),
  [370] = {.entry = {.count = 1, .reusable = true}}, SHIFT(150),
  [372] = {.entry = {.count = 1, .reusable = true}}, SHIFT(88),
  [374] = {.entry = {.count = 1, .reusable = false}}, SHIFT(110),
  [376] = {.entry = {.count = 1, .reusable = true}}, SHIFT(139),
  [378] = {.entry = {.count = 1, .reusable = true}}, SHIFT(126),
  [380] = {.entry = {.count = 1, .reusable = true}}, SHIFT(133),
  [382] = {.entry = {.count = 1, .reusable = true}}, SHIFT(138),
  [384] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 5, 0, 11),
  [386] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 5, 0, 11),
  [388] = {.entry = {.count = 1, .reusable = true}}, SHIFT(121),
  [390] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 5, 0, 0),
  [392] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 5, 0, 0),
  [394] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_code_block, 7, 0, 0),
  [396] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_code_block, 7, 0, 0),
  [398] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 4, 0, 9),
  [400] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 4, 0, 9),
  [402] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tag_repeat2, 2, 0, 10), SHIFT_REPEAT(150),
  [405] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_code_block, 3, 0, 0),
  [407] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_code_block, 3, 0, 0),
  [409] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 3, 0, 5),
  [411] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 3, 0, 5),
  [413] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_code_block, 4, 0, 0),
  [415] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_code_block, 4, 0, 0),
  [417] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 4, 0, 7),
  [419] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 4, 0, 7),
  [421] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tag, 3, 0, 2),
  [423] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tag, 3, 0, 2),
  [425] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_document, 3, 0, 0),
  [427] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_document_repeat2, 2, 0, 0),
  [429] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat2, 2, 0, 0), SHIFT_REPEAT(8),
  [432] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat2, 2, 0, 0), SHIFT_REPEAT(135),
  [435] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat2, 2, 0, 0), SHIFT_REPEAT(152),
  [438] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat2, 2, 0, 0), SHIFT_REPEAT(166),
  [441] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat2, 2, 0, 0), SHIFT_REPEAT(20),
  [444] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat2, 2, 0, 0), SHIFT_REPEAT(37),
  [447] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat2, 2, 0, 0), SHIFT_REPEAT(147),
  [450] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_document, 4, 0, 0),
  [452] = {.entry = {.count = 1, .reusable = true}}, SHIFT(27),
  [454] = {.entry = {.count = 1, .reusable = true}}, SHIFT(153),
  [456] = {.entry = {.count = 1, .reusable = true}}, SHIFT(148),
  [458] = {.entry = {.count = 1, .reusable = true}}, SHIFT(69),
  [460] = {.entry = {.count = 1, .reusable = true}}, SHIFT(155),
  [462] = {.entry = {.count = 1, .reusable = true}}, SHIFT(35),
  [464] = {.entry = {.count = 1, .reusable = true}}, SHIFT(81),
  [466] = {.entry = {.count = 1, .reusable = false}}, SHIFT(68),
  [468] = {.entry = {.count = 1, .reusable = true}}, SHIFT(65),
  [470] = {.entry = {.count = 1, .reusable = false}}, SHIFT(164),
  [472] = {.entry = {.count = 1, .reusable = true}}, SHIFT(164),
  [474] = {.entry = {.count = 1, .reusable = false}}, SHIFT(188),
  [476] = {.entry = {.count = 1, .reusable = true}}, SHIFT(188),
  [478] = {.entry = {.count = 1, .reusable = false}}, SHIFT(181),
  [480] = {.entry = {.count = 1, .reusable = true}}, SHIFT(181),
  [482] = {.entry = {.count = 1, .reusable = true}}, SHIFT(102),
  [484] = {.entry = {.count = 1, .reusable = true}}, SHIFT(41),
  [486] = {.entry = {.count = 1, .reusable = true}}, SHIFT(87),
  [488] = {.entry = {.count = 1, .reusable = true}}, SHIFT(77),
  [490] = {.entry = {.count = 1, .reusable = true}}, SHIFT(171),
  [492] = {.entry = {.count = 1, .reusable = true}}, SHIFT(172),
  [494] = {.entry = {.count = 1, .reusable = true}}, SHIFT(33),
  [496] = {.entry = {.count = 1, .reusable = true}}, SHIFT(44),
  [498] = {.entry = {.count = 1, .reusable = true}}, SHIFT(78),
  [500] = {.entry = {.count = 1, .reusable = true}}, SHIFT(83),
  [502] = {.entry = {.count = 1, .reusable = true}}, SHIFT(175),
  [504] = {.entry = {.count = 1, .reusable = true}}, SHIFT(96),
  [506] = {.entry = {.count = 1, .reusable = true}}, SHIFT(97),
  [508] = {.entry = {.count = 1, .reusable = true}}, SHIFT(129),
  [510] = {.entry = {.count = 1, .reusable = true}}, SHIFT(163),
  [512] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_document, 5, 0, 0),
  [514] = {.entry = {.count = 1, .reusable = true}}, SHIFT(204),
  [516] = {.entry = {.count = 1, .reusable = true}}, SHIFT(201),
  [518] = {.entry = {.count = 1, .reusable = true}}, SHIFT(108),
  [520] = {.entry = {.count = 1, .reusable = true}}, SHIFT(99),
  [522] = {.entry = {.count = 1, .reusable = true}}, SHIFT(104),
  [524] = {.entry = {.count = 1, .reusable = true}}, SHIFT(192),
  [526] = {.entry = {.count = 1, .reusable = true}}, SHIFT(115),
  [528] = {.entry = {.count = 1, .reusable = true}}, SHIFT(61),
  [530] = {.entry = {.count = 1, .reusable = true}}, SHIFT(60),
  [532] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [534] = {.entry = {.count = 1, .reusable = true}}, SHIFT(57),
  [536] = {.entry = {.count = 1, .reusable = true}}, SHIFT(176),
  [538] = {.entry = {.count = 1, .reusable = true}}, SHIFT(82),
  [540] = {.entry = {.count = 1, .reusable = true}}, SHIFT(89),
  [542] = {.entry = {.count = 1, .reusable = true}}, SHIFT(198),
  [544] = {.entry = {.count = 1, .reusable = true}}, SHIFT(86),
  [546] = {.entry = {.count = 1, .reusable = true}}, SHIFT(55),
  [548] = {.entry = {.count = 1, .reusable = true}}, SHIFT(158),
  [550] = {.entry = {.count = 1, .reusable = true}}, SHIFT(98),
  [552] = {.entry = {.count = 1, .reusable = true}}, SHIFT(62),
  [554] = {.entry = {.count = 1, .reusable = true}}, SHIFT(63),
  [556] = {.entry = {.count = 1, .reusable = true}}, SHIFT(174),
  [558] = {.entry = {.count = 1, .reusable = true}}, SHIFT(42),
  [560] = {.entry = {.count = 1, .reusable = true}}, SHIFT(40),
  [562] = {.entry = {.count = 1, .reusable = true}}, SHIFT(185),
  [564] = {.entry = {.count = 1, .reusable = true}}, SHIFT(186),
  [566] = {.entry = {.count = 1, .reusable = true}}, SHIFT(187),
  [568] = {.entry = {.count = 1, .reusable = true}}, SHIFT(112),
  [570] = {.entry = {.count = 1, .reusable = true}}, SHIFT(191),
  [572] = {.entry = {.count = 1, .reusable = true}}, SHIFT(194),
  [574] = {.entry = {.count = 1, .reusable = true}}, SHIFT(167),
  [576] = {.entry = {.count = 1, .reusable = true}}, SHIFT(197),
  [578] = {.entry = {.count = 1, .reusable = true}}, SHIFT(94),
  [580] = {.entry = {.count = 1, .reusable = true}}, SHIFT(202),
};

enum ts_external_scanner_symbol_identifiers {
  ts_external_token_brief_text = 0,
  ts_external_token_code_block_start = 1,
  ts_external_token_code_block_language = 2,
  ts_external_token_code_block_content = 3,
  ts_external_token_code_block_end = 4,
};

static const TSSymbol ts_external_scanner_symbol_map[EXTERNAL_TOKEN_COUNT] = {
  [ts_external_token_brief_text] = sym_brief_text,
  [ts_external_token_code_block_start] = sym_code_block_start,
  [ts_external_token_code_block_language] = sym_code_block_language,
  [ts_external_token_code_block_content] = sym_code_block_content,
  [ts_external_token_code_block_end] = sym_code_block_end,
};

static const bool ts_external_scanner_states[8][EXTERNAL_TOKEN_COUNT] = {
  [1] = {
    [ts_external_token_brief_text] = true,
    [ts_external_token_code_block_start] = true,
    [ts_external_token_code_block_language] = true,
    [ts_external_token_code_block_content] = true,
    [ts_external_token_code_block_end] = true,
  },
  [2] = {
    [ts_external_token_code_block_start] = true,
  },
  [3] = {
    [ts_external_token_brief_text] = true,
    [ts_external_token_code_block_start] = true,
  },
  [4] = {
    [ts_external_token_brief_text] = true,
  },
  [5] = {
    [ts_external_token_code_block_content] = true,
  },
  [6] = {
    [ts_external_token_code_block_language] = true,
  },
  [7] = {
    [ts_external_token_code_block_end] = true,
  },
};

#ifdef __cplusplus
extern "C" {
#endif
void *tree_sitter_doxygen_external_scanner_create(void);
void tree_sitter_doxygen_external_scanner_destroy(void *);
bool tree_sitter_doxygen_external_scanner_scan(void *, TSLexer *, const bool *);
unsigned tree_sitter_doxygen_external_scanner_serialize(void *, char *);
void tree_sitter_doxygen_external_scanner_deserialize(void *, const char *, unsigned);

#ifdef TREE_SITTER_HIDE_SYMBOLS
#define TS_PUBLIC
#elif defined(_WIN32)
#define TS_PUBLIC __declspec(dllexport)
#else
#define TS_PUBLIC __attribute__((visibility("default")))
#endif

TS_PUBLIC const TSLanguage *tree_sitter_doxygen(void) {
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
    .field_names = ts_field_names,
    .field_map_slices = ts_field_map_slices,
    .field_map_entries = ts_field_map_entries,
    .symbol_metadata = ts_symbol_metadata,
    .public_symbol_map = ts_symbol_map,
    .alias_map = ts_non_terminal_alias_map,
    .alias_sequences = &ts_alias_sequences[0][0],
    .lex_modes = ts_lex_modes,
    .lex_fn = ts_lex,
    .external_scanner = {
      &ts_external_scanner_states[0][0],
      ts_external_scanner_symbol_map,
      tree_sitter_doxygen_external_scanner_create,
      tree_sitter_doxygen_external_scanner_destroy,
      tree_sitter_doxygen_external_scanner_scan,
      tree_sitter_doxygen_external_scanner_serialize,
      tree_sitter_doxygen_external_scanner_deserialize,
    },
    .primary_state_ids = ts_primary_state_ids,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif

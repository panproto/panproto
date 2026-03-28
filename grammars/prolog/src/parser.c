#include "tree_sitter/parser.h"

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 70
#define LARGE_STATE_COUNT 5
#define SYMBOL_COUNT 53
#define ALIAS_COUNT 0
#define TOKEN_COUNT 29
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 2
#define MAX_ALIAS_SEQUENCE_LENGTH 4
#define PRODUCTION_ID_COUNT 4

enum ts_symbol_identifiers {
  sym_end = 1,
  sym_directive_head = 2,
  aux_sym_atom_token1 = 3,
  sym_variable_term = 4,
  aux_sym_operator_notation_token1 = 5,
  aux_sym_operator_notation_token2 = 6,
  anon_sym_SEMI = 7,
  anon_sym_DASH_GT = 8,
  anon_sym_BQUOTE_COMMA_BQUOTE = 9,
  anon_sym_BSLASH_PLUS = 10,
  aux_sym_operator_notation_token3 = 11,
  aux_sym_operator_notation_token4 = 12,
  aux_sym_operator_notation_token5 = 13,
  anon_sym_STAR_STAR = 14,
  anon_sym_CARET = 15,
  aux_sym_operator_notation_token6 = 16,
  anon_sym_PIPE = 17,
  sym_double_quoted_list_notation = 18,
  anon_sym_LPAREN = 19,
  sym_close = 20,
  sym_open_list = 21,
  sym_close_list = 22,
  sym_open_curly = 23,
  sym_close_curly = 24,
  sym_comma = 25,
  sym_integer = 26,
  sym_float_number = 27,
  sym_comment = 28,
  sym_source_file = 29,
  sym_directive_term = 30,
  sym_clause_term = 31,
  sym__term = 32,
  sym__compound_term = 33,
  sym__atomic_term = 34,
  sym__number = 35,
  sym_atom = 36,
  sym_empty_list = 37,
  sym_empty_curly_brackets = 38,
  sym_functional_notation = 39,
  sym_arg_list = 40,
  sym_arg_list_separator = 41,
  sym__arg = 42,
  sym_operator_notation = 43,
  sym_list_notation = 44,
  sym__list_notation_items = 45,
  sym_list_notation_separator = 46,
  sym_curly_bracketed_notation = 47,
  sym_open = 48,
  sym_open_ct = 49,
  aux_sym_source_file_repeat1 = 50,
  aux_sym_arg_list_repeat1 = 51,
  aux_sym__list_notation_items_repeat1 = 52,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [sym_end] = "end",
  [sym_directive_head] = "directive_head",
  [aux_sym_atom_token1] = "atom_token1",
  [sym_variable_term] = "variable_term",
  [aux_sym_operator_notation_token1] = "binary_operator",
  [aux_sym_operator_notation_token2] = "prefix_operator",
  [anon_sym_SEMI] = "semicolon",
  [anon_sym_DASH_GT] = "binary_operator",
  [anon_sym_BQUOTE_COMMA_BQUOTE] = "comma",
  [anon_sym_BSLASH_PLUS] = "prefix_operator",
  [aux_sym_operator_notation_token3] = "binary_operator",
  [aux_sym_operator_notation_token4] = "binary_operator",
  [aux_sym_operator_notation_token5] = "binary_operator",
  [anon_sym_STAR_STAR] = "binary_operator",
  [anon_sym_CARET] = "binary_operator",
  [aux_sym_operator_notation_token6] = "prexif_operator",
  [anon_sym_PIPE] = "|",
  [sym_double_quoted_list_notation] = "double_quoted_list_notation",
  [anon_sym_LPAREN] = "(",
  [sym_close] = "close",
  [sym_open_list] = "open_list",
  [sym_close_list] = "close_list",
  [sym_open_curly] = "open_curly",
  [sym_close_curly] = "close_curly",
  [sym_comma] = "comma",
  [sym_integer] = "integer",
  [sym_float_number] = "float_number",
  [sym_comment] = "comment",
  [sym_source_file] = "source_file",
  [sym_directive_term] = "directive_term",
  [sym_clause_term] = "clause_term",
  [sym__term] = "_term",
  [sym__compound_term] = "_compound_term",
  [sym__atomic_term] = "_atomic_term",
  [sym__number] = "_number",
  [sym_atom] = "atom",
  [sym_empty_list] = "empty_list",
  [sym_empty_curly_brackets] = "empty_curly_brackets",
  [sym_functional_notation] = "functional_notation",
  [sym_arg_list] = "arg_list",
  [sym_arg_list_separator] = "arg_list_separator",
  [sym__arg] = "_arg",
  [sym_operator_notation] = "operator_notation",
  [sym_list_notation] = "list_notation",
  [sym__list_notation_items] = "_list_notation_items",
  [sym_list_notation_separator] = "list_notation_separator",
  [sym_curly_bracketed_notation] = "curly_bracketed_notation",
  [sym_open] = "open",
  [sym_open_ct] = "open_ct",
  [aux_sym_source_file_repeat1] = "source_file_repeat1",
  [aux_sym_arg_list_repeat1] = "arg_list_repeat1",
  [aux_sym__list_notation_items_repeat1] = "_list_notation_items_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [sym_end] = sym_end,
  [sym_directive_head] = sym_directive_head,
  [aux_sym_atom_token1] = aux_sym_atom_token1,
  [sym_variable_term] = sym_variable_term,
  [aux_sym_operator_notation_token1] = aux_sym_operator_notation_token1,
  [aux_sym_operator_notation_token2] = aux_sym_operator_notation_token2,
  [anon_sym_SEMI] = anon_sym_SEMI,
  [anon_sym_DASH_GT] = aux_sym_operator_notation_token1,
  [anon_sym_BQUOTE_COMMA_BQUOTE] = sym_comma,
  [anon_sym_BSLASH_PLUS] = aux_sym_operator_notation_token2,
  [aux_sym_operator_notation_token3] = aux_sym_operator_notation_token1,
  [aux_sym_operator_notation_token4] = aux_sym_operator_notation_token1,
  [aux_sym_operator_notation_token5] = aux_sym_operator_notation_token1,
  [anon_sym_STAR_STAR] = aux_sym_operator_notation_token1,
  [anon_sym_CARET] = aux_sym_operator_notation_token1,
  [aux_sym_operator_notation_token6] = aux_sym_operator_notation_token6,
  [anon_sym_PIPE] = anon_sym_PIPE,
  [sym_double_quoted_list_notation] = sym_double_quoted_list_notation,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [sym_close] = sym_close,
  [sym_open_list] = sym_open_list,
  [sym_close_list] = sym_close_list,
  [sym_open_curly] = sym_open_curly,
  [sym_close_curly] = sym_close_curly,
  [sym_comma] = sym_comma,
  [sym_integer] = sym_integer,
  [sym_float_number] = sym_float_number,
  [sym_comment] = sym_comment,
  [sym_source_file] = sym_source_file,
  [sym_directive_term] = sym_directive_term,
  [sym_clause_term] = sym_clause_term,
  [sym__term] = sym__term,
  [sym__compound_term] = sym__compound_term,
  [sym__atomic_term] = sym__atomic_term,
  [sym__number] = sym__number,
  [sym_atom] = sym_atom,
  [sym_empty_list] = sym_empty_list,
  [sym_empty_curly_brackets] = sym_empty_curly_brackets,
  [sym_functional_notation] = sym_functional_notation,
  [sym_arg_list] = sym_arg_list,
  [sym_arg_list_separator] = sym_arg_list_separator,
  [sym__arg] = sym__arg,
  [sym_operator_notation] = sym_operator_notation,
  [sym_list_notation] = sym_list_notation,
  [sym__list_notation_items] = sym__list_notation_items,
  [sym_list_notation_separator] = sym_list_notation_separator,
  [sym_curly_bracketed_notation] = sym_curly_bracketed_notation,
  [sym_open] = sym_open,
  [sym_open_ct] = sym_open_ct,
  [aux_sym_source_file_repeat1] = aux_sym_source_file_repeat1,
  [aux_sym_arg_list_repeat1] = aux_sym_arg_list_repeat1,
  [aux_sym__list_notation_items_repeat1] = aux_sym__list_notation_items_repeat1,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [sym_end] = {
    .visible = true,
    .named = true,
  },
  [sym_directive_head] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_atom_token1] = {
    .visible = false,
    .named = false,
  },
  [sym_variable_term] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_operator_notation_token1] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_operator_notation_token2] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_SEMI] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_DASH_GT] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_BQUOTE_COMMA_BQUOTE] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_BSLASH_PLUS] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_operator_notation_token3] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_operator_notation_token4] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_operator_notation_token5] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_STAR_STAR] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_CARET] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_operator_notation_token6] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_PIPE] = {
    .visible = true,
    .named = false,
  },
  [sym_double_quoted_list_notation] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_LPAREN] = {
    .visible = true,
    .named = false,
  },
  [sym_close] = {
    .visible = true,
    .named = true,
  },
  [sym_open_list] = {
    .visible = true,
    .named = true,
  },
  [sym_close_list] = {
    .visible = true,
    .named = true,
  },
  [sym_open_curly] = {
    .visible = true,
    .named = true,
  },
  [sym_close_curly] = {
    .visible = true,
    .named = true,
  },
  [sym_comma] = {
    .visible = true,
    .named = true,
  },
  [sym_integer] = {
    .visible = true,
    .named = true,
  },
  [sym_float_number] = {
    .visible = true,
    .named = true,
  },
  [sym_comment] = {
    .visible = true,
    .named = true,
  },
  [sym_source_file] = {
    .visible = true,
    .named = true,
  },
  [sym_directive_term] = {
    .visible = true,
    .named = true,
  },
  [sym_clause_term] = {
    .visible = true,
    .named = true,
  },
  [sym__term] = {
    .visible = false,
    .named = true,
  },
  [sym__compound_term] = {
    .visible = false,
    .named = true,
  },
  [sym__atomic_term] = {
    .visible = false,
    .named = true,
  },
  [sym__number] = {
    .visible = false,
    .named = true,
  },
  [sym_atom] = {
    .visible = true,
    .named = true,
  },
  [sym_empty_list] = {
    .visible = true,
    .named = true,
  },
  [sym_empty_curly_brackets] = {
    .visible = true,
    .named = true,
  },
  [sym_functional_notation] = {
    .visible = true,
    .named = true,
  },
  [sym_arg_list] = {
    .visible = true,
    .named = true,
  },
  [sym_arg_list_separator] = {
    .visible = true,
    .named = true,
  },
  [sym__arg] = {
    .visible = false,
    .named = true,
  },
  [sym_operator_notation] = {
    .visible = true,
    .named = true,
  },
  [sym_list_notation] = {
    .visible = true,
    .named = true,
  },
  [sym__list_notation_items] = {
    .visible = false,
    .named = true,
  },
  [sym_list_notation_separator] = {
    .visible = true,
    .named = true,
  },
  [sym_curly_bracketed_notation] = {
    .visible = true,
    .named = true,
  },
  [sym_open] = {
    .visible = true,
    .named = true,
  },
  [sym_open_ct] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_source_file_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_arg_list_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__list_notation_items_repeat1] = {
    .visible = false,
    .named = false,
  },
};

enum ts_field_identifiers {
  field_function = 1,
  field_operator = 2,
};

static const char * const ts_field_names[] = {
  [0] = NULL,
  [field_function] = "function",
  [field_operator] = "operator",
};

static const TSFieldMapSlice ts_field_map_slices[PRODUCTION_ID_COUNT] = {
  [1] = {.index = 0, .length = 1},
  [2] = {.index = 1, .length = 1},
  [3] = {.index = 2, .length = 1},
};

static const TSFieldMapEntry ts_field_map_entries[] = {
  [0] =
    {field_operator, 0},
  [1] =
    {field_operator, 1},
  [2] =
    {field_function, 0},
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
  [48] = 29,
  [49] = 49,
  [50] = 50,
  [51] = 51,
  [52] = 52,
  [53] = 53,
  [54] = 54,
  [55] = 45,
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
};

static TSCharacterRange aux_sym_atom_token1_character_set_1[] = {
  {'!', '!'}, {'#', '$'}, {'&', '\''}, {'*', '/'}, {':', '@'}, {'\\', '\\'}, {'^', '^'}, {'a', 'z'},
  {'~', '~'},
};

static TSCharacterRange aux_sym_atom_token1_character_set_2[] = {
  {'\n', '\n'}, {'"', '"'}, {'\'', '\''}, {'0', '7'}, {'\\', '\\'}, {'`', 'b'}, {'f', 'f'}, {'n', 'n'},
  {'r', 'r'}, {'t', 't'}, {'v', 'v'}, {'x', 'x'},
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(41);
      ADVANCE_MAP(
        '"', 3,
        '%', 1,
        '(', 79,
        ')', 80,
        '*', 73,
        '+', 70,
        ',', 85,
        '-', 71,
        '.', 42,
        '/', 74,
        ':', 11,
        ';', 60,
        '<', 67,
        '=', 66,
        '>', 69,
        '?', 12,
        '@', 17,
        '[', 81,
        '\\', 9,
        ']', 82,
        '^', 76,
        '_', 57,
        '`', 10,
        'i', 29,
        'm', 28,
        'r', 26,
        '{', 83,
        '|', 77,
        '}', 84,
      );
      if ((!eof && lookahead == 00) ||
          lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == ' ') SKIP(0);
      if (('A' <= lookahead && lookahead <= 'Z')) ADVANCE(57);
      END_STATE();
    case 1:
      if ((!eof && lookahead == 00)) ADVANCE(95);
      if (lookahead == '\n') ADVANCE(94);
      if (lookahead != 0) ADVANCE(1);
      END_STATE();
    case 2:
      ADVANCE_MAP(
        '"', 3,
        '%', 1,
        '\'', 5,
        '(', 79,
        '+', 54,
        '-', 53,
        '/', 47,
        '0', 87,
        ':', 51,
        '?', 51,
        '[', 81,
        '\\', 50,
        ']', 82,
        '_', 57,
        '{', 83,
        '}', 84,
        '!', 45,
        ';', 45,
      );
      if ((!eof && lookahead == 00) ||
          lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == ' ') SKIP(2);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(88);
      if (('#' <= lookahead && lookahead <= '&') ||
          ('*' <= lookahead && lookahead <= '@') ||
          lookahead == '^' ||
          lookahead == '~') ADVANCE(55);
      if (('A' <= lookahead && lookahead <= 'Z')) ADVANCE(57);
      if (('a' <= lookahead && lookahead <= 'z')) ADVANCE(56);
      END_STATE();
    case 3:
      if (lookahead == '"') ADVANCE(78);
      if (lookahead == '\\') ADVANCE(30);
      if ((' ' <= lookahead && lookahead <= '~')) ADVANCE(3);
      END_STATE();
    case 4:
      ADVANCE_MAP(
        '%', 1,
        '(', 79,
        ')', 80,
        '*', 73,
        '+', 70,
        ',', 85,
        '-', 71,
        '.', 42,
        '/', 74,
        ':', 13,
        ';', 60,
        '<', 67,
        '=', 66,
        '>', 69,
        '@', 17,
        '\\', 15,
        ']', 82,
        '^', 76,
        '`', 10,
        'i', 29,
        'm', 28,
        'r', 26,
        '|', 77,
        '}', 84,
      );
      if ((!eof && lookahead == 00) ||
          lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == ' ') SKIP(4);
      END_STATE();
    case 5:
      if (lookahead == '\'') ADVANCE(46);
      if (lookahead == '\\') ADVANCE(31);
      if ((' ' <= lookahead && lookahead <= '~')) ADVANCE(5);
      END_STATE();
    case 6:
      if (lookahead == '\'') ADVANCE(86);
      END_STATE();
    case 7:
      if (lookahead == '*') ADVANCE(7);
      if (lookahead == '/') ADVANCE(94);
      if (lookahead != 0) ADVANCE(8);
      END_STATE();
    case 8:
      if (lookahead == '*') ADVANCE(7);
      if (lookahead != 0) ADVANCE(8);
      END_STATE();
    case 9:
      if (lookahead == '+') ADVANCE(63);
      if (lookahead == '/') ADVANCE(70);
      if (lookahead == '=') ADVANCE(68);
      END_STATE();
    case 10:
      if (lookahead == ',') ADVANCE(24);
      END_STATE();
    case 11:
      if (lookahead == '-') ADVANCE(43);
      END_STATE();
    case 12:
      if (lookahead == '-') ADVANCE(59);
      END_STATE();
    case 13:
      if (lookahead == '-') ADVANCE(58);
      END_STATE();
    case 14:
      if (lookahead == '.') ADVANCE(65);
      END_STATE();
    case 15:
      if (lookahead == '/') ADVANCE(70);
      if (lookahead == '=') ADVANCE(68);
      END_STATE();
    case 16:
      if (lookahead == '<') ADVANCE(65);
      END_STATE();
    case 17:
      if (lookahead == '<') ADVANCE(65);
      if (lookahead == '=') ADVANCE(16);
      if (lookahead == '>') ADVANCE(68);
      END_STATE();
    case 18:
      if (lookahead == '=') ADVANCE(65);
      END_STATE();
    case 19:
      if (lookahead == '>') ADVANCE(58);
      END_STATE();
    case 20:
      if (lookahead == '\\') ADVANCE(3);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(20);
      END_STATE();
    case 21:
      if (lookahead == '\\') ADVANCE(3);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(21);
      END_STATE();
    case 22:
      if (lookahead == '\\') ADVANCE(5);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(22);
      END_STATE();
    case 23:
      if (lookahead == '\\') ADVANCE(5);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(23);
      END_STATE();
    case 24:
      if (lookahead == '`') ADVANCE(62);
      END_STATE();
    case 25:
      if (lookahead == 'd') ADVANCE(72);
      END_STATE();
    case 26:
      if (lookahead == 'e') ADVANCE(27);
      END_STATE();
    case 27:
      if (lookahead == 'm') ADVANCE(72);
      END_STATE();
    case 28:
      if (lookahead == 'o') ADVANCE(25);
      END_STATE();
    case 29:
      if (lookahead == 's') ADVANCE(65);
      END_STATE();
    case 30:
      if (lookahead == 'x') ADVANCE(37);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(20);
      if (set_contains(aux_sym_atom_token1_character_set_2, 12, lookahead)) ADVANCE(3);
      END_STATE();
    case 31:
      if (lookahead == 'x') ADVANCE(39);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(22);
      if (set_contains(aux_sym_atom_token1_character_set_2, 12, lookahead)) ADVANCE(5);
      END_STATE();
    case 32:
      if (lookahead == '+' ||
          lookahead == '-') ADVANCE(36);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(93);
      END_STATE();
    case 33:
      if (lookahead == '0' ||
          lookahead == '1') ADVANCE(89);
      END_STATE();
    case 34:
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(90);
      END_STATE();
    case 35:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(92);
      END_STATE();
    case 36:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(93);
      END_STATE();
    case 37:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(21);
      END_STATE();
    case 38:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(91);
      END_STATE();
    case 39:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(23);
      END_STATE();
    case 40:
      if (eof) ADVANCE(41);
      ADVANCE_MAP(
        '"', 3,
        '%', 1,
        '\'', 5,
        '(', 79,
        '+', 54,
        '-', 53,
        '/', 47,
        '0', 87,
        ':', 52,
        '?', 51,
        '[', 81,
        '\\', 50,
        '_', 57,
        '{', 83,
        '!', 45,
        ';', 45,
      );
      if ((!eof && lookahead == 00) ||
          lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == ' ') SKIP(40);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(88);
      if (('#' <= lookahead && lookahead <= '&') ||
          ('*' <= lookahead && lookahead <= '@') ||
          lookahead == '^' ||
          lookahead == '~') ADVANCE(55);
      if (('A' <= lookahead && lookahead <= 'Z')) ADVANCE(57);
      if (('a' <= lookahead && lookahead <= 'z')) ADVANCE(56);
      END_STATE();
    case 41:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 42:
      ACCEPT_TOKEN(sym_end);
      END_STATE();
    case 43:
      ACCEPT_TOKEN(sym_directive_head);
      END_STATE();
    case 44:
      ACCEPT_TOKEN(sym_directive_head);
      if ((set_contains(aux_sym_atom_token1_character_set_1, 9, lookahead)) &&
          lookahead != '!' &&
          lookahead != '\'' &&
          lookahead != ';' &&
          (lookahead < 'a' || 'z' < lookahead)) ADVANCE(55);
      END_STATE();
    case 45:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      END_STATE();
    case 46:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if (lookahead == '\'') ADVANCE(5);
      END_STATE();
    case 47:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if (lookahead == '*') ADVANCE(49);
      if ((set_contains(aux_sym_atom_token1_character_set_1, 9, lookahead)) &&
          lookahead != '!' &&
          lookahead != '\'' &&
          lookahead != ';' &&
          (lookahead < 'a' || 'z' < lookahead)) ADVANCE(55);
      END_STATE();
    case 48:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if (lookahead == '*') ADVANCE(48);
      if (lookahead == '/') ADVANCE(55);
      if ((set_contains(aux_sym_atom_token1_character_set_1, 9, lookahead)) &&
          lookahead != '!' &&
          lookahead != '\'' &&
          lookahead != ';' &&
          (lookahead < 'a' || 'z' < lookahead)) ADVANCE(49);
      if (lookahead != 0) ADVANCE(8);
      END_STATE();
    case 49:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if (lookahead == '*') ADVANCE(48);
      if ((set_contains(aux_sym_atom_token1_character_set_1, 9, lookahead)) &&
          lookahead != '!' &&
          lookahead != '\'' &&
          lookahead != ';' &&
          (lookahead < 'a' || 'z' < lookahead)) ADVANCE(49);
      if (lookahead != 0) ADVANCE(8);
      END_STATE();
    case 50:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if (lookahead == '+') ADVANCE(64);
      if ((set_contains(aux_sym_atom_token1_character_set_1, 9, lookahead)) &&
          lookahead != '!' &&
          lookahead != '\'' &&
          lookahead != ';' &&
          (lookahead < 'a' || 'z' < lookahead)) ADVANCE(55);
      END_STATE();
    case 51:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if (lookahead == '-') ADVANCE(55);
      if ((set_contains(aux_sym_atom_token1_character_set_1, 9, lookahead)) &&
          lookahead != '!' &&
          lookahead != '\'' &&
          lookahead != ';' &&
          (lookahead < 'a' || 'z' < lookahead)) ADVANCE(55);
      END_STATE();
    case 52:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if (lookahead == '-') ADVANCE(44);
      if ((set_contains(aux_sym_atom_token1_character_set_1, 9, lookahead)) &&
          lookahead != '!' &&
          lookahead != '\'' &&
          lookahead != ';' &&
          (lookahead < 'a' || 'z' < lookahead)) ADVANCE(55);
      END_STATE();
    case 53:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if (lookahead == '0') ADVANCE(87);
      if (lookahead == '\\') ADVANCE(55);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(88);
      if (lookahead == '#' ||
          lookahead == '$' ||
          lookahead == '&' ||
          ('*' <= lookahead && lookahead <= ':') ||
          ('<' <= lookahead && lookahead <= '@') ||
          lookahead == '^' ||
          lookahead == '~') ADVANCE(55);
      END_STATE();
    case 54:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if (lookahead == '0') ADVANCE(87);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(88);
      if (lookahead == '#' ||
          lookahead == '$' ||
          lookahead == '&' ||
          ('*' <= lookahead && lookahead <= ':') ||
          ('<' <= lookahead && lookahead <= '@') ||
          lookahead == '\\' ||
          lookahead == '^' ||
          lookahead == '~') ADVANCE(55);
      END_STATE();
    case 55:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if ((set_contains(aux_sym_atom_token1_character_set_1, 9, lookahead)) &&
          lookahead != '!' &&
          lookahead != '\'' &&
          lookahead != ';' &&
          (lookahead < 'a' || 'z' < lookahead)) ADVANCE(55);
      END_STATE();
    case 56:
      ACCEPT_TOKEN(aux_sym_atom_token1);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(56);
      END_STATE();
    case 57:
      ACCEPT_TOKEN(sym_variable_term);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(57);
      END_STATE();
    case 58:
      ACCEPT_TOKEN(aux_sym_operator_notation_token1);
      END_STATE();
    case 59:
      ACCEPT_TOKEN(aux_sym_operator_notation_token2);
      END_STATE();
    case 60:
      ACCEPT_TOKEN(anon_sym_SEMI);
      END_STATE();
    case 61:
      ACCEPT_TOKEN(anon_sym_DASH_GT);
      END_STATE();
    case 62:
      ACCEPT_TOKEN(anon_sym_BQUOTE_COMMA_BQUOTE);
      END_STATE();
    case 63:
      ACCEPT_TOKEN(anon_sym_BSLASH_PLUS);
      END_STATE();
    case 64:
      ACCEPT_TOKEN(anon_sym_BSLASH_PLUS);
      if ((set_contains(aux_sym_atom_token1_character_set_1, 9, lookahead)) &&
          lookahead != '!' &&
          lookahead != '\'' &&
          lookahead != ';' &&
          (lookahead < 'a' || 'z' < lookahead)) ADVANCE(55);
      END_STATE();
    case 65:
      ACCEPT_TOKEN(aux_sym_operator_notation_token3);
      END_STATE();
    case 66:
      ACCEPT_TOKEN(aux_sym_operator_notation_token3);
      if (lookahead == '.') ADVANCE(14);
      if (lookahead == ':') ADVANCE(18);
      if (lookahead == '\\') ADVANCE(18);
      if (lookahead == '<' ||
          lookahead == '=') ADVANCE(65);
      END_STATE();
    case 67:
      ACCEPT_TOKEN(aux_sym_operator_notation_token3);
      if (lookahead == '<') ADVANCE(72);
      END_STATE();
    case 68:
      ACCEPT_TOKEN(aux_sym_operator_notation_token3);
      if (lookahead == '=') ADVANCE(65);
      END_STATE();
    case 69:
      ACCEPT_TOKEN(aux_sym_operator_notation_token3);
      if (lookahead == '=') ADVANCE(65);
      if (lookahead == '>') ADVANCE(72);
      END_STATE();
    case 70:
      ACCEPT_TOKEN(aux_sym_operator_notation_token4);
      END_STATE();
    case 71:
      ACCEPT_TOKEN(aux_sym_operator_notation_token4);
      if (lookahead == '-') ADVANCE(19);
      if (lookahead == '>') ADVANCE(61);
      END_STATE();
    case 72:
      ACCEPT_TOKEN(aux_sym_operator_notation_token5);
      END_STATE();
    case 73:
      ACCEPT_TOKEN(aux_sym_operator_notation_token5);
      if (lookahead == '*') ADVANCE(75);
      END_STATE();
    case 74:
      ACCEPT_TOKEN(aux_sym_operator_notation_token5);
      if (lookahead == '*') ADVANCE(8);
      if (lookahead == '/') ADVANCE(72);
      if (lookahead == '\\') ADVANCE(70);
      END_STATE();
    case 75:
      ACCEPT_TOKEN(anon_sym_STAR_STAR);
      END_STATE();
    case 76:
      ACCEPT_TOKEN(anon_sym_CARET);
      END_STATE();
    case 77:
      ACCEPT_TOKEN(anon_sym_PIPE);
      END_STATE();
    case 78:
      ACCEPT_TOKEN(sym_double_quoted_list_notation);
      if (lookahead == '"') ADVANCE(3);
      END_STATE();
    case 79:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 80:
      ACCEPT_TOKEN(sym_close);
      END_STATE();
    case 81:
      ACCEPT_TOKEN(sym_open_list);
      END_STATE();
    case 82:
      ACCEPT_TOKEN(sym_close_list);
      END_STATE();
    case 83:
      ACCEPT_TOKEN(sym_open_curly);
      END_STATE();
    case 84:
      ACCEPT_TOKEN(sym_close_curly);
      END_STATE();
    case 85:
      ACCEPT_TOKEN(sym_comma);
      END_STATE();
    case 86:
      ACCEPT_TOKEN(sym_integer);
      END_STATE();
    case 87:
      ACCEPT_TOKEN(sym_integer);
      if (lookahead == '\'') ADVANCE(6);
      if (lookahead == '.') ADVANCE(35);
      if (lookahead == 'b') ADVANCE(33);
      if (lookahead == 'o') ADVANCE(34);
      if (lookahead == 'x') ADVANCE(38);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(88);
      END_STATE();
    case 88:
      ACCEPT_TOKEN(sym_integer);
      if (lookahead == '.') ADVANCE(35);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(88);
      END_STATE();
    case 89:
      ACCEPT_TOKEN(sym_integer);
      if (lookahead == '0' ||
          lookahead == '1') ADVANCE(89);
      END_STATE();
    case 90:
      ACCEPT_TOKEN(sym_integer);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(90);
      END_STATE();
    case 91:
      ACCEPT_TOKEN(sym_integer);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(91);
      END_STATE();
    case 92:
      ACCEPT_TOKEN(sym_float_number);
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(32);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(92);
      END_STATE();
    case 93:
      ACCEPT_TOKEN(sym_float_number);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(93);
      END_STATE();
    case 94:
      ACCEPT_TOKEN(sym_comment);
      END_STATE();
    case 95:
      ACCEPT_TOKEN(sym_comment);
      if ((!eof && lookahead == 00)) ADVANCE(95);
      if (lookahead == '\n') ADVANCE(94);
      if (lookahead != 0) ADVANCE(1);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 40},
  [2] = {.lex_state = 40},
  [3] = {.lex_state = 40},
  [4] = {.lex_state = 2},
  [5] = {.lex_state = 2},
  [6] = {.lex_state = 2},
  [7] = {.lex_state = 2},
  [8] = {.lex_state = 2},
  [9] = {.lex_state = 2},
  [10] = {.lex_state = 2},
  [11] = {.lex_state = 2},
  [12] = {.lex_state = 2},
  [13] = {.lex_state = 2},
  [14] = {.lex_state = 2},
  [15] = {.lex_state = 2},
  [16] = {.lex_state = 2},
  [17] = {.lex_state = 2},
  [18] = {.lex_state = 2},
  [19] = {.lex_state = 2},
  [20] = {.lex_state = 2},
  [21] = {.lex_state = 2},
  [22] = {.lex_state = 2},
  [23] = {.lex_state = 2},
  [24] = {.lex_state = 4},
  [25] = {.lex_state = 4},
  [26] = {.lex_state = 4},
  [27] = {.lex_state = 4},
  [28] = {.lex_state = 4},
  [29] = {.lex_state = 4},
  [30] = {.lex_state = 4},
  [31] = {.lex_state = 4},
  [32] = {.lex_state = 4},
  [33] = {.lex_state = 4},
  [34] = {.lex_state = 4},
  [35] = {.lex_state = 4},
  [36] = {.lex_state = 4},
  [37] = {.lex_state = 4},
  [38] = {.lex_state = 4},
  [39] = {.lex_state = 4},
  [40] = {.lex_state = 4},
  [41] = {.lex_state = 4},
  [42] = {.lex_state = 4},
  [43] = {.lex_state = 4},
  [44] = {.lex_state = 4},
  [45] = {.lex_state = 4},
  [46] = {.lex_state = 40},
  [47] = {.lex_state = 40},
  [48] = {.lex_state = 4},
  [49] = {.lex_state = 4},
  [50] = {.lex_state = 4},
  [51] = {.lex_state = 2},
  [52] = {.lex_state = 2},
  [53] = {.lex_state = 4},
  [54] = {.lex_state = 2},
  [55] = {.lex_state = 4},
  [56] = {.lex_state = 2},
  [57] = {.lex_state = 4},
  [58] = {.lex_state = 0},
  [59] = {.lex_state = 0},
  [60] = {.lex_state = 0},
  [61] = {.lex_state = 0},
  [62] = {.lex_state = 0},
  [63] = {.lex_state = 0},
  [64] = {.lex_state = 0},
  [65] = {.lex_state = 0},
  [66] = {.lex_state = 0},
  [67] = {.lex_state = 0},
  [68] = {.lex_state = 0},
  [69] = {.lex_state = 0},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [sym_end] = ACTIONS(1),
    [sym_directive_head] = ACTIONS(1),
    [sym_variable_term] = ACTIONS(1),
    [aux_sym_operator_notation_token1] = ACTIONS(1),
    [aux_sym_operator_notation_token2] = ACTIONS(1),
    [anon_sym_SEMI] = ACTIONS(1),
    [anon_sym_DASH_GT] = ACTIONS(1),
    [anon_sym_BQUOTE_COMMA_BQUOTE] = ACTIONS(1),
    [anon_sym_BSLASH_PLUS] = ACTIONS(1),
    [aux_sym_operator_notation_token3] = ACTIONS(1),
    [aux_sym_operator_notation_token4] = ACTIONS(1),
    [aux_sym_operator_notation_token5] = ACTIONS(1),
    [anon_sym_STAR_STAR] = ACTIONS(1),
    [anon_sym_CARET] = ACTIONS(1),
    [anon_sym_PIPE] = ACTIONS(1),
    [sym_double_quoted_list_notation] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [sym_close] = ACTIONS(1),
    [sym_open_list] = ACTIONS(1),
    [sym_close_list] = ACTIONS(1),
    [sym_open_curly] = ACTIONS(1),
    [sym_close_curly] = ACTIONS(1),
    [sym_comma] = ACTIONS(1),
    [sym_comment] = ACTIONS(3),
  },
  [1] = {
    [sym_source_file] = STATE(69),
    [sym_directive_term] = STATE(2),
    [sym_clause_term] = STATE(2),
    [sym__term] = STATE(49),
    [sym__compound_term] = STATE(49),
    [sym__atomic_term] = STATE(49),
    [sym__number] = STATE(49),
    [sym_atom] = STATE(24),
    [sym_empty_list] = STATE(27),
    [sym_empty_curly_brackets] = STATE(27),
    [sym_functional_notation] = STATE(49),
    [sym_operator_notation] = STATE(49),
    [sym_list_notation] = STATE(49),
    [sym_curly_bracketed_notation] = STATE(49),
    [sym_open] = STATE(11),
    [aux_sym_source_file_repeat1] = STATE(2),
    [ts_builtin_sym_end] = ACTIONS(5),
    [sym_directive_head] = ACTIONS(7),
    [aux_sym_atom_token1] = ACTIONS(9),
    [sym_variable_term] = ACTIONS(11),
    [aux_sym_operator_notation_token2] = ACTIONS(13),
    [anon_sym_BSLASH_PLUS] = ACTIONS(15),
    [aux_sym_operator_notation_token6] = ACTIONS(17),
    [sym_double_quoted_list_notation] = ACTIONS(11),
    [anon_sym_LPAREN] = ACTIONS(19),
    [sym_open_list] = ACTIONS(21),
    [sym_open_curly] = ACTIONS(23),
    [sym_integer] = ACTIONS(25),
    [sym_float_number] = ACTIONS(11),
    [sym_comment] = ACTIONS(27),
  },
  [2] = {
    [sym_directive_term] = STATE(3),
    [sym_clause_term] = STATE(3),
    [sym__term] = STATE(49),
    [sym__compound_term] = STATE(49),
    [sym__atomic_term] = STATE(49),
    [sym__number] = STATE(49),
    [sym_atom] = STATE(24),
    [sym_empty_list] = STATE(27),
    [sym_empty_curly_brackets] = STATE(27),
    [sym_functional_notation] = STATE(49),
    [sym_operator_notation] = STATE(49),
    [sym_list_notation] = STATE(49),
    [sym_curly_bracketed_notation] = STATE(49),
    [sym_open] = STATE(11),
    [aux_sym_source_file_repeat1] = STATE(3),
    [ts_builtin_sym_end] = ACTIONS(29),
    [sym_directive_head] = ACTIONS(7),
    [aux_sym_atom_token1] = ACTIONS(9),
    [sym_variable_term] = ACTIONS(11),
    [aux_sym_operator_notation_token2] = ACTIONS(13),
    [anon_sym_BSLASH_PLUS] = ACTIONS(15),
    [aux_sym_operator_notation_token6] = ACTIONS(17),
    [sym_double_quoted_list_notation] = ACTIONS(11),
    [anon_sym_LPAREN] = ACTIONS(19),
    [sym_open_list] = ACTIONS(21),
    [sym_open_curly] = ACTIONS(23),
    [sym_integer] = ACTIONS(25),
    [sym_float_number] = ACTIONS(11),
    [sym_comment] = ACTIONS(27),
  },
  [3] = {
    [sym_directive_term] = STATE(3),
    [sym_clause_term] = STATE(3),
    [sym__term] = STATE(49),
    [sym__compound_term] = STATE(49),
    [sym__atomic_term] = STATE(49),
    [sym__number] = STATE(49),
    [sym_atom] = STATE(24),
    [sym_empty_list] = STATE(27),
    [sym_empty_curly_brackets] = STATE(27),
    [sym_functional_notation] = STATE(49),
    [sym_operator_notation] = STATE(49),
    [sym_list_notation] = STATE(49),
    [sym_curly_bracketed_notation] = STATE(49),
    [sym_open] = STATE(11),
    [aux_sym_source_file_repeat1] = STATE(3),
    [ts_builtin_sym_end] = ACTIONS(31),
    [sym_directive_head] = ACTIONS(33),
    [aux_sym_atom_token1] = ACTIONS(36),
    [sym_variable_term] = ACTIONS(39),
    [aux_sym_operator_notation_token2] = ACTIONS(42),
    [anon_sym_BSLASH_PLUS] = ACTIONS(45),
    [aux_sym_operator_notation_token6] = ACTIONS(48),
    [sym_double_quoted_list_notation] = ACTIONS(39),
    [anon_sym_LPAREN] = ACTIONS(51),
    [sym_open_list] = ACTIONS(54),
    [sym_open_curly] = ACTIONS(57),
    [sym_integer] = ACTIONS(60),
    [sym_float_number] = ACTIONS(39),
    [sym_comment] = ACTIONS(27),
  },
  [4] = {
    [sym__term] = STATE(45),
    [sym__compound_term] = STATE(45),
    [sym__atomic_term] = STATE(45),
    [sym__number] = STATE(45),
    [sym_atom] = STATE(29),
    [sym_empty_list] = STATE(27),
    [sym_empty_curly_brackets] = STATE(27),
    [sym_functional_notation] = STATE(45),
    [sym__arg] = STATE(58),
    [sym_operator_notation] = STATE(45),
    [sym_list_notation] = STATE(45),
    [sym__list_notation_items] = STATE(66),
    [sym_curly_bracketed_notation] = STATE(45),
    [sym_open] = STATE(11),
    [aux_sym_atom_token1] = ACTIONS(9),
    [sym_variable_term] = ACTIONS(63),
    [aux_sym_operator_notation_token2] = ACTIONS(13),
    [anon_sym_BSLASH_PLUS] = ACTIONS(15),
    [aux_sym_operator_notation_token6] = ACTIONS(17),
    [sym_double_quoted_list_notation] = ACTIONS(63),
    [anon_sym_LPAREN] = ACTIONS(19),
    [sym_open_list] = ACTIONS(21),
    [sym_close_list] = ACTIONS(65),
    [sym_open_curly] = ACTIONS(23),
    [sym_integer] = ACTIONS(67),
    [sym_float_number] = ACTIONS(63),
    [sym_comment] = ACTIONS(27),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 16,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(67), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(29), 1,
      sym_atom,
    STATE(59), 1,
      sym__arg,
    STATE(67), 1,
      sym_arg_list,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(63), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(45), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [59] = 15,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(71), 1,
      sym_close_curly,
    ACTIONS(73), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(69), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(57), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [115] = 15,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(77), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(48), 1,
      sym_atom,
    STATE(68), 1,
      sym__arg,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(75), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(55), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [171] = 15,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(67), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(29), 1,
      sym_atom,
    STATE(64), 1,
      sym__arg,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(63), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(45), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [227] = 15,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(67), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(29), 1,
      sym_atom,
    STATE(65), 1,
      sym__arg,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(63), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(45), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [283] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(81), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(79), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(30), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [336] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(85), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(83), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(53), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [389] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(89), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(87), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(28), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [442] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(93), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(91), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(34), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [495] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(97), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(95), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(35), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [548] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(101), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(99), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(36), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [601] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(105), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(103), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(37), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [654] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(109), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(107), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(38), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [707] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(113), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(111), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(39), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [760] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(117), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(115), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(40), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [813] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(121), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(119), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(41), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [866] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(125), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(123), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(42), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [919] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(129), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(127), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(31), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [972] = 14,
    ACTIONS(9), 1,
      aux_sym_atom_token1,
    ACTIONS(13), 1,
      aux_sym_operator_notation_token2,
    ACTIONS(15), 1,
      anon_sym_BSLASH_PLUS,
    ACTIONS(17), 1,
      aux_sym_operator_notation_token6,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(21), 1,
      sym_open_list,
    ACTIONS(23), 1,
      sym_open_curly,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(133), 1,
      sym_integer,
    STATE(11), 1,
      sym_open,
    STATE(24), 1,
      sym_atom,
    STATE(27), 2,
      sym_empty_list,
      sym_empty_curly_brackets,
    ACTIONS(131), 3,
      sym_variable_term,
      sym_double_quoted_list_notation,
      sym_float_number,
    STATE(50), 8,
      sym__term,
      sym__compound_term,
      sym__atomic_term,
      sym__number,
      sym_functional_notation,
      sym_operator_notation,
      sym_list_notation,
      sym_curly_bracketed_notation,
  [1025] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_LPAREN,
    STATE(5), 1,
      sym_open_ct,
    ACTIONS(137), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(135), 12,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1054] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(143), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(141), 13,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
      anon_sym_PIPE,
      anon_sym_LPAREN,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1078] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(147), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(145), 13,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
      anon_sym_PIPE,
      anon_sym_LPAREN,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1102] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(151), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(149), 13,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
      anon_sym_PIPE,
      anon_sym_LPAREN,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1126] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(153), 10,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1157] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_LPAREN,
    STATE(5), 1,
      sym_open_ct,
    ACTIONS(137), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(165), 4,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_comma,
    ACTIONS(135), 6,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
  [1186] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(167), 1,
      anon_sym_SEMI,
    ACTIONS(169), 1,
      anon_sym_DASH_GT,
    ACTIONS(171), 2,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      sym_comma,
    ACTIONS(153), 6,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
  [1223] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(173), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(153), 10,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1250] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(177), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(175), 12,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1273] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(181), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(179), 12,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1296] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(167), 1,
      anon_sym_SEMI,
    ACTIONS(169), 1,
      anon_sym_DASH_GT,
    ACTIONS(171), 2,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      sym_comma,
    ACTIONS(183), 6,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
  [1333] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(167), 1,
      anon_sym_SEMI,
    ACTIONS(169), 1,
      anon_sym_DASH_GT,
    ACTIONS(171), 2,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      sym_comma,
    ACTIONS(183), 6,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
  [1370] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(169), 1,
      anon_sym_DASH_GT,
    ACTIONS(171), 2,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      sym_comma,
    ACTIONS(183), 7,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
  [1405] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(171), 2,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      sym_comma,
    ACTIONS(183), 8,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
  [1438] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(185), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(183), 10,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1469] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(185), 2,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
    ACTIONS(183), 10,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1498] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(185), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(183), 10,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1525] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(185), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(183), 12,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1548] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(185), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(183), 10,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1575] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(189), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(187), 12,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1598] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(193), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(191), 12,
      sym_end,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_close_curly,
      sym_comma,
  [1621] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(167), 1,
      anon_sym_SEMI,
    ACTIONS(169), 1,
      anon_sym_DASH_GT,
    ACTIONS(171), 1,
      anon_sym_BQUOTE_COMMA_BQUOTE,
    ACTIONS(195), 1,
      aux_sym_operator_notation_token1,
    ACTIONS(165), 4,
      anon_sym_PIPE,
      sym_close,
      sym_close_list,
      sym_comma,
  [1658] = 3,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(199), 6,
      sym_directive_head,
      aux_sym_atom_token1,
      aux_sym_operator_notation_token2,
      anon_sym_BSLASH_PLUS,
      aux_sym_operator_notation_token6,
      sym_integer,
    ACTIONS(197), 7,
      ts_builtin_sym_end,
      sym_variable_term,
      sym_double_quoted_list_notation,
      anon_sym_LPAREN,
      sym_open_list,
      sym_open_curly,
      sym_float_number,
  [1679] = 3,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(203), 6,
      sym_directive_head,
      aux_sym_atom_token1,
      aux_sym_operator_notation_token2,
      anon_sym_BSLASH_PLUS,
      aux_sym_operator_notation_token6,
      sym_integer,
    ACTIONS(201), 7,
      ts_builtin_sym_end,
      sym_variable_term,
      sym_double_quoted_list_notation,
      anon_sym_LPAREN,
      sym_open_list,
      sym_open_curly,
      sym_float_number,
  [1700] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_LPAREN,
    ACTIONS(165), 1,
      sym_close_list,
    STATE(5), 1,
      sym_open_ct,
    ACTIONS(137), 3,
      aux_sym_operator_notation_token3,
      aux_sym_operator_notation_token4,
      aux_sym_operator_notation_token5,
    ACTIONS(135), 7,
      aux_sym_operator_notation_token1,
      anon_sym_SEMI,
      anon_sym_DASH_GT,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      anon_sym_STAR_STAR,
      anon_sym_CARET,
      sym_comma,
  [1727] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(167), 1,
      anon_sym_SEMI,
    ACTIONS(169), 1,
      anon_sym_DASH_GT,
    ACTIONS(195), 1,
      aux_sym_operator_notation_token1,
    ACTIONS(205), 1,
      sym_end,
    ACTIONS(171), 2,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      sym_comma,
  [1762] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(167), 1,
      anon_sym_SEMI,
    ACTIONS(169), 1,
      anon_sym_DASH_GT,
    ACTIONS(195), 1,
      aux_sym_operator_notation_token1,
    ACTIONS(207), 1,
      sym_end,
    ACTIONS(171), 2,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      sym_comma,
  [1797] = 3,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(209), 5,
      aux_sym_atom_token1,
      aux_sym_operator_notation_token2,
      anon_sym_BSLASH_PLUS,
      aux_sym_operator_notation_token6,
      sym_integer,
    ACTIONS(211), 6,
      sym_variable_term,
      sym_double_quoted_list_notation,
      anon_sym_LPAREN,
      sym_open_list,
      sym_open_curly,
      sym_float_number,
  [1816] = 3,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(213), 5,
      aux_sym_atom_token1,
      aux_sym_operator_notation_token2,
      anon_sym_BSLASH_PLUS,
      aux_sym_operator_notation_token6,
      sym_integer,
    ACTIONS(215), 6,
      sym_variable_term,
      sym_double_quoted_list_notation,
      anon_sym_LPAREN,
      sym_open_list,
      sym_open_curly,
      sym_float_number,
  [1835] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(167), 1,
      anon_sym_SEMI,
    ACTIONS(169), 1,
      anon_sym_DASH_GT,
    ACTIONS(195), 1,
      aux_sym_operator_notation_token1,
    ACTIONS(217), 1,
      sym_close,
    ACTIONS(171), 2,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      sym_comma,
  [1870] = 3,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(219), 5,
      aux_sym_atom_token1,
      aux_sym_operator_notation_token2,
      anon_sym_BSLASH_PLUS,
      aux_sym_operator_notation_token6,
      sym_integer,
    ACTIONS(221), 6,
      sym_variable_term,
      sym_double_quoted_list_notation,
      anon_sym_LPAREN,
      sym_open_list,
      sym_open_curly,
      sym_float_number,
  [1889] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(165), 1,
      sym_close_list,
    ACTIONS(167), 1,
      anon_sym_SEMI,
    ACTIONS(169), 1,
      anon_sym_DASH_GT,
    ACTIONS(195), 1,
      aux_sym_operator_notation_token1,
    ACTIONS(171), 2,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      sym_comma,
  [1924] = 3,
    ACTIONS(27), 1,
      sym_comment,
    ACTIONS(223), 5,
      aux_sym_atom_token1,
      aux_sym_operator_notation_token2,
      anon_sym_BSLASH_PLUS,
      aux_sym_operator_notation_token6,
      sym_integer,
    ACTIONS(225), 6,
      sym_variable_term,
      sym_double_quoted_list_notation,
      anon_sym_LPAREN,
      sym_open_list,
      sym_open_curly,
      sym_float_number,
  [1943] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(155), 1,
      aux_sym_operator_notation_token3,
    ACTIONS(157), 1,
      aux_sym_operator_notation_token4,
    ACTIONS(159), 1,
      aux_sym_operator_notation_token5,
    ACTIONS(161), 1,
      anon_sym_STAR_STAR,
    ACTIONS(163), 1,
      anon_sym_CARET,
    ACTIONS(167), 1,
      anon_sym_SEMI,
    ACTIONS(169), 1,
      anon_sym_DASH_GT,
    ACTIONS(195), 1,
      aux_sym_operator_notation_token1,
    ACTIONS(227), 1,
      sym_close_curly,
    ACTIONS(171), 2,
      anon_sym_BQUOTE_COMMA_BQUOTE,
      sym_comma,
  [1978] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(229), 1,
      anon_sym_PIPE,
    ACTIONS(231), 1,
      sym_close_list,
    ACTIONS(233), 1,
      sym_comma,
    STATE(8), 1,
      sym_list_notation_separator,
    STATE(60), 1,
      aux_sym__list_notation_items_repeat1,
  [1997] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(235), 1,
      sym_close,
    ACTIONS(237), 1,
      sym_comma,
    STATE(9), 1,
      sym_arg_list_separator,
    STATE(61), 1,
      aux_sym_arg_list_repeat1,
  [2013] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(233), 1,
      sym_comma,
    ACTIONS(239), 1,
      sym_close_list,
    STATE(8), 1,
      sym_list_notation_separator,
    STATE(62), 1,
      aux_sym__list_notation_items_repeat1,
  [2029] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(237), 1,
      sym_comma,
    ACTIONS(241), 1,
      sym_close,
    STATE(9), 1,
      sym_arg_list_separator,
    STATE(63), 1,
      aux_sym_arg_list_repeat1,
  [2045] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(243), 1,
      sym_close_list,
    ACTIONS(245), 1,
      sym_comma,
    STATE(8), 1,
      sym_list_notation_separator,
    STATE(62), 1,
      aux_sym__list_notation_items_repeat1,
  [2061] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(248), 1,
      sym_close,
    ACTIONS(250), 1,
      sym_comma,
    STATE(9), 1,
      sym_arg_list_separator,
    STATE(63), 1,
      aux_sym_arg_list_repeat1,
  [2077] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(243), 2,
      sym_close_list,
      sym_comma,
  [2085] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(248), 2,
      sym_close,
      sym_comma,
  [2093] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(253), 1,
      sym_close_list,
  [2100] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(255), 1,
      sym_close,
  [2107] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(257), 1,
      sym_close_list,
  [2114] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(259), 1,
      ts_builtin_sym_end,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(5)] = 0,
  [SMALL_STATE(6)] = 59,
  [SMALL_STATE(7)] = 115,
  [SMALL_STATE(8)] = 171,
  [SMALL_STATE(9)] = 227,
  [SMALL_STATE(10)] = 283,
  [SMALL_STATE(11)] = 336,
  [SMALL_STATE(12)] = 389,
  [SMALL_STATE(13)] = 442,
  [SMALL_STATE(14)] = 495,
  [SMALL_STATE(15)] = 548,
  [SMALL_STATE(16)] = 601,
  [SMALL_STATE(17)] = 654,
  [SMALL_STATE(18)] = 707,
  [SMALL_STATE(19)] = 760,
  [SMALL_STATE(20)] = 813,
  [SMALL_STATE(21)] = 866,
  [SMALL_STATE(22)] = 919,
  [SMALL_STATE(23)] = 972,
  [SMALL_STATE(24)] = 1025,
  [SMALL_STATE(25)] = 1054,
  [SMALL_STATE(26)] = 1078,
  [SMALL_STATE(27)] = 1102,
  [SMALL_STATE(28)] = 1126,
  [SMALL_STATE(29)] = 1157,
  [SMALL_STATE(30)] = 1186,
  [SMALL_STATE(31)] = 1223,
  [SMALL_STATE(32)] = 1250,
  [SMALL_STATE(33)] = 1273,
  [SMALL_STATE(34)] = 1296,
  [SMALL_STATE(35)] = 1333,
  [SMALL_STATE(36)] = 1370,
  [SMALL_STATE(37)] = 1405,
  [SMALL_STATE(38)] = 1438,
  [SMALL_STATE(39)] = 1469,
  [SMALL_STATE(40)] = 1498,
  [SMALL_STATE(41)] = 1525,
  [SMALL_STATE(42)] = 1548,
  [SMALL_STATE(43)] = 1575,
  [SMALL_STATE(44)] = 1598,
  [SMALL_STATE(45)] = 1621,
  [SMALL_STATE(46)] = 1658,
  [SMALL_STATE(47)] = 1679,
  [SMALL_STATE(48)] = 1700,
  [SMALL_STATE(49)] = 1727,
  [SMALL_STATE(50)] = 1762,
  [SMALL_STATE(51)] = 1797,
  [SMALL_STATE(52)] = 1816,
  [SMALL_STATE(53)] = 1835,
  [SMALL_STATE(54)] = 1870,
  [SMALL_STATE(55)] = 1889,
  [SMALL_STATE(56)] = 1924,
  [SMALL_STATE(57)] = 1943,
  [SMALL_STATE(58)] = 1978,
  [SMALL_STATE(59)] = 1997,
  [SMALL_STATE(60)] = 2013,
  [SMALL_STATE(61)] = 2029,
  [SMALL_STATE(62)] = 2045,
  [SMALL_STATE(63)] = 2061,
  [SMALL_STATE(64)] = 2077,
  [SMALL_STATE(65)] = 2085,
  [SMALL_STATE(66)] = 2093,
  [SMALL_STATE(67)] = 2100,
  [SMALL_STATE(68)] = 2107,
  [SMALL_STATE(69)] = 2114,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 0, 0, 0),
  [7] = {.entry = {.count = 1, .reusable = false}}, SHIFT(23),
  [9] = {.entry = {.count = 1, .reusable = false}}, SHIFT(27),
  [11] = {.entry = {.count = 1, .reusable = true}}, SHIFT(49),
  [13] = {.entry = {.count = 1, .reusable = false}}, SHIFT(10),
  [15] = {.entry = {.count = 1, .reusable = false}}, SHIFT(12),
  [17] = {.entry = {.count = 1, .reusable = false}}, SHIFT(22),
  [19] = {.entry = {.count = 1, .reusable = true}}, SHIFT(56),
  [21] = {.entry = {.count = 1, .reusable = true}}, SHIFT(4),
  [23] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [25] = {.entry = {.count = 1, .reusable = false}}, SHIFT(49),
  [27] = {.entry = {.count = 1, .reusable = false}}, SHIFT_EXTRA(),
  [29] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 1, 0, 0),
  [31] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0),
  [33] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(23),
  [36] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(27),
  [39] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(49),
  [42] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(10),
  [45] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(12),
  [48] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(22),
  [51] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(56),
  [54] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(4),
  [57] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(6),
  [60] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(49),
  [63] = {.entry = {.count = 1, .reusable = true}}, SHIFT(45),
  [65] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
  [67] = {.entry = {.count = 1, .reusable = false}}, SHIFT(45),
  [69] = {.entry = {.count = 1, .reusable = true}}, SHIFT(57),
  [71] = {.entry = {.count = 1, .reusable = true}}, SHIFT(26),
  [73] = {.entry = {.count = 1, .reusable = false}}, SHIFT(57),
  [75] = {.entry = {.count = 1, .reusable = true}}, SHIFT(55),
  [77] = {.entry = {.count = 1, .reusable = false}}, SHIFT(55),
  [79] = {.entry = {.count = 1, .reusable = true}}, SHIFT(30),
  [81] = {.entry = {.count = 1, .reusable = false}}, SHIFT(30),
  [83] = {.entry = {.count = 1, .reusable = true}}, SHIFT(53),
  [85] = {.entry = {.count = 1, .reusable = false}}, SHIFT(53),
  [87] = {.entry = {.count = 1, .reusable = true}}, SHIFT(28),
  [89] = {.entry = {.count = 1, .reusable = false}}, SHIFT(28),
  [91] = {.entry = {.count = 1, .reusable = true}}, SHIFT(34),
  [93] = {.entry = {.count = 1, .reusable = false}}, SHIFT(34),
  [95] = {.entry = {.count = 1, .reusable = true}}, SHIFT(35),
  [97] = {.entry = {.count = 1, .reusable = false}}, SHIFT(35),
  [99] = {.entry = {.count = 1, .reusable = true}}, SHIFT(36),
  [101] = {.entry = {.count = 1, .reusable = false}}, SHIFT(36),
  [103] = {.entry = {.count = 1, .reusable = true}}, SHIFT(37),
  [105] = {.entry = {.count = 1, .reusable = false}}, SHIFT(37),
  [107] = {.entry = {.count = 1, .reusable = true}}, SHIFT(38),
  [109] = {.entry = {.count = 1, .reusable = false}}, SHIFT(38),
  [111] = {.entry = {.count = 1, .reusable = true}}, SHIFT(39),
  [113] = {.entry = {.count = 1, .reusable = false}}, SHIFT(39),
  [115] = {.entry = {.count = 1, .reusable = true}}, SHIFT(40),
  [117] = {.entry = {.count = 1, .reusable = false}}, SHIFT(40),
  [119] = {.entry = {.count = 1, .reusable = true}}, SHIFT(41),
  [121] = {.entry = {.count = 1, .reusable = false}}, SHIFT(41),
  [123] = {.entry = {.count = 1, .reusable = true}}, SHIFT(42),
  [125] = {.entry = {.count = 1, .reusable = false}}, SHIFT(42),
  [127] = {.entry = {.count = 1, .reusable = true}}, SHIFT(31),
  [129] = {.entry = {.count = 1, .reusable = false}}, SHIFT(31),
  [131] = {.entry = {.count = 1, .reusable = true}}, SHIFT(50),
  [133] = {.entry = {.count = 1, .reusable = false}}, SHIFT(50),
  [135] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__atomic_term, 1, 0, 0),
  [137] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__atomic_term, 1, 0, 0),
  [139] = {.entry = {.count = 1, .reusable = true}}, SHIFT(52),
  [141] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_empty_list, 2, 0, 0),
  [143] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_empty_list, 2, 0, 0),
  [145] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_empty_curly_brackets, 2, 0, 0),
  [147] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_empty_curly_brackets, 2, 0, 0),
  [149] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_atom, 1, 0, 0),
  [151] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_atom, 1, 0, 0),
  [153] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_operator_notation, 2, 0, 1),
  [155] = {.entry = {.count = 1, .reusable = false}}, SHIFT(17),
  [157] = {.entry = {.count = 1, .reusable = false}}, SHIFT(18),
  [159] = {.entry = {.count = 1, .reusable = false}}, SHIFT(19),
  [161] = {.entry = {.count = 1, .reusable = true}}, SHIFT(20),
  [163] = {.entry = {.count = 1, .reusable = true}}, SHIFT(21),
  [165] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__arg, 1, 0, 0),
  [167] = {.entry = {.count = 1, .reusable = true}}, SHIFT(14),
  [169] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [171] = {.entry = {.count = 1, .reusable = true}}, SHIFT(16),
  [173] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_operator_notation, 2, 0, 1),
  [175] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_list_notation, 3, 0, 0),
  [177] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_list_notation, 3, 0, 0),
  [179] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_curly_bracketed_notation, 3, 0, 0),
  [181] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_curly_bracketed_notation, 3, 0, 0),
  [183] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_operator_notation, 3, 0, 2),
  [185] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_operator_notation, 3, 0, 2),
  [187] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_operator_notation, 3, 0, 0),
  [189] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_operator_notation, 3, 0, 0),
  [191] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_functional_notation, 4, 0, 3),
  [193] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_functional_notation, 4, 0, 3),
  [195] = {.entry = {.count = 1, .reusable = true}}, SHIFT(13),
  [197] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_directive_term, 3, 0, 0),
  [199] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_directive_term, 3, 0, 0),
  [201] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_clause_term, 2, 0, 0),
  [203] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_clause_term, 2, 0, 0),
  [205] = {.entry = {.count = 1, .reusable = true}}, SHIFT(47),
  [207] = {.entry = {.count = 1, .reusable = true}}, SHIFT(46),
  [209] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_list_notation_separator, 1, 0, 0),
  [211] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_list_notation_separator, 1, 0, 0),
  [213] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_open_ct, 1, 0, 0),
  [215] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_open_ct, 1, 0, 0),
  [217] = {.entry = {.count = 1, .reusable = true}}, SHIFT(43),
  [219] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_arg_list_separator, 1, 0, 0),
  [221] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_arg_list_separator, 1, 0, 0),
  [223] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_open, 1, 0, 0),
  [225] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_open, 1, 0, 0),
  [227] = {.entry = {.count = 1, .reusable = true}}, SHIFT(33),
  [229] = {.entry = {.count = 1, .reusable = true}}, SHIFT(7),
  [231] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__list_notation_items, 1, 0, 0),
  [233] = {.entry = {.count = 1, .reusable = true}}, SHIFT(51),
  [235] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_arg_list, 1, 0, 0),
  [237] = {.entry = {.count = 1, .reusable = true}}, SHIFT(54),
  [239] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__list_notation_items, 2, 0, 0),
  [241] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_arg_list, 2, 0, 0),
  [243] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__list_notation_items_repeat1, 2, 0, 0),
  [245] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__list_notation_items_repeat1, 2, 0, 0), SHIFT_REPEAT(51),
  [248] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_arg_list_repeat1, 2, 0, 0),
  [250] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_arg_list_repeat1, 2, 0, 0), SHIFT_REPEAT(54),
  [253] = {.entry = {.count = 1, .reusable = true}}, SHIFT(32),
  [255] = {.entry = {.count = 1, .reusable = true}}, SHIFT(44),
  [257] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__list_notation_items, 3, 0, 0),
  [259] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
};

#ifdef __cplusplus
extern "C" {
#endif
#ifdef TREE_SITTER_HIDE_SYMBOLS
#define TS_PUBLIC
#elif defined(_WIN32)
#define TS_PUBLIC __declspec(dllexport)
#else
#define TS_PUBLIC __attribute__((visibility("default")))
#endif

TS_PUBLIC const TSLanguage *tree_sitter_prolog(void) {
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
    .primary_state_ids = ts_primary_state_ids,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif

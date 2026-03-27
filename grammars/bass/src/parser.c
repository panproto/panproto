#include "tree_sitter/parser.h"

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 32
#define LARGE_STATE_COUNT 27
#define SYMBOL_COUNT 36
#define ALIAS_COUNT 0
#define TOKEN_COUNT 22
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 2
#define MAX_ALIAS_SEQUENCE_LENGTH 3
#define PRODUCTION_ID_COUNT 2

enum ts_symbol_identifiers {
  sym_symbol = 1,
  anon_sym_COLON = 2,
  sym_command = 3,
  sym_path = 4,
  anon_sym_CARET = 5,
  anon_sym_LPAREN = 6,
  anon_sym_RPAREN = 7,
  anon_sym_LBRACE = 8,
  anon_sym_RBRACE = 9,
  anon_sym_LBRACK = 10,
  anon_sym_RBRACK = 11,
  sym_number = 12,
  anon_sym_DQUOTE = 13,
  sym_string_fragment = 14,
  aux_sym__escape_sequence_token1 = 15,
  sym_escape_sequence = 16,
  sym_ignore = 17,
  sym_null = 18,
  anon_sym_false = 19,
  anon_sym_true = 20,
  sym_comment = 21,
  sym_source = 22,
  sym_form = 23,
  sym_keyword = 24,
  sym_symbind = 25,
  sym_meta = 26,
  sym_list = 27,
  sym_scope = 28,
  sym_cons = 29,
  sym_literal = 30,
  sym_string = 31,
  sym__escape_sequence = 32,
  sym_boolean = 33,
  aux_sym_source_repeat1 = 34,
  aux_sym_string_repeat1 = 35,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [sym_symbol] = "symbol",
  [anon_sym_COLON] = ":",
  [sym_command] = "command",
  [sym_path] = "path",
  [anon_sym_CARET] = "^",
  [anon_sym_LPAREN] = "(",
  [anon_sym_RPAREN] = ")",
  [anon_sym_LBRACE] = "{",
  [anon_sym_RBRACE] = "}",
  [anon_sym_LBRACK] = "[",
  [anon_sym_RBRACK] = "]",
  [sym_number] = "number",
  [anon_sym_DQUOTE] = "\"",
  [sym_string_fragment] = "string_fragment",
  [aux_sym__escape_sequence_token1] = "_escape_sequence_token1",
  [sym_escape_sequence] = "escape_sequence",
  [sym_ignore] = "ignore",
  [sym_null] = "null",
  [anon_sym_false] = "false",
  [anon_sym_true] = "true",
  [sym_comment] = "comment",
  [sym_source] = "source",
  [sym_form] = "form",
  [sym_keyword] = "keyword",
  [sym_symbind] = "symbind",
  [sym_meta] = "meta",
  [sym_list] = "list",
  [sym_scope] = "scope",
  [sym_cons] = "cons",
  [sym_literal] = "literal",
  [sym_string] = "string",
  [sym__escape_sequence] = "_escape_sequence",
  [sym_boolean] = "boolean",
  [aux_sym_source_repeat1] = "source_repeat1",
  [aux_sym_string_repeat1] = "string_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [sym_symbol] = sym_symbol,
  [anon_sym_COLON] = anon_sym_COLON,
  [sym_command] = sym_command,
  [sym_path] = sym_path,
  [anon_sym_CARET] = anon_sym_CARET,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [anon_sym_RPAREN] = anon_sym_RPAREN,
  [anon_sym_LBRACE] = anon_sym_LBRACE,
  [anon_sym_RBRACE] = anon_sym_RBRACE,
  [anon_sym_LBRACK] = anon_sym_LBRACK,
  [anon_sym_RBRACK] = anon_sym_RBRACK,
  [sym_number] = sym_number,
  [anon_sym_DQUOTE] = anon_sym_DQUOTE,
  [sym_string_fragment] = sym_string_fragment,
  [aux_sym__escape_sequence_token1] = aux_sym__escape_sequence_token1,
  [sym_escape_sequence] = sym_escape_sequence,
  [sym_ignore] = sym_ignore,
  [sym_null] = sym_null,
  [anon_sym_false] = anon_sym_false,
  [anon_sym_true] = anon_sym_true,
  [sym_comment] = sym_comment,
  [sym_source] = sym_source,
  [sym_form] = sym_form,
  [sym_keyword] = sym_keyword,
  [sym_symbind] = sym_symbind,
  [sym_meta] = sym_meta,
  [sym_list] = sym_list,
  [sym_scope] = sym_scope,
  [sym_cons] = sym_cons,
  [sym_literal] = sym_literal,
  [sym_string] = sym_string,
  [sym__escape_sequence] = sym__escape_sequence,
  [sym_boolean] = sym_boolean,
  [aux_sym_source_repeat1] = aux_sym_source_repeat1,
  [aux_sym_string_repeat1] = aux_sym_string_repeat1,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [sym_symbol] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [sym_command] = {
    .visible = true,
    .named = true,
  },
  [sym_path] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_CARET] = {
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
  [anon_sym_LBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LBRACK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACK] = {
    .visible = true,
    .named = false,
  },
  [sym_number] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_DQUOTE] = {
    .visible = true,
    .named = false,
  },
  [sym_string_fragment] = {
    .visible = true,
    .named = true,
  },
  [aux_sym__escape_sequence_token1] = {
    .visible = false,
    .named = false,
  },
  [sym_escape_sequence] = {
    .visible = true,
    .named = true,
  },
  [sym_ignore] = {
    .visible = true,
    .named = true,
  },
  [sym_null] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_false] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_true] = {
    .visible = true,
    .named = false,
  },
  [sym_comment] = {
    .visible = true,
    .named = true,
  },
  [sym_source] = {
    .visible = true,
    .named = true,
  },
  [sym_form] = {
    .visible = false,
    .named = true,
    .supertype = true,
  },
  [sym_keyword] = {
    .visible = true,
    .named = true,
  },
  [sym_symbind] = {
    .visible = true,
    .named = true,
  },
  [sym_meta] = {
    .visible = true,
    .named = true,
  },
  [sym_list] = {
    .visible = true,
    .named = true,
  },
  [sym_scope] = {
    .visible = true,
    .named = true,
  },
  [sym_cons] = {
    .visible = true,
    .named = true,
  },
  [sym_literal] = {
    .visible = false,
    .named = true,
    .supertype = true,
  },
  [sym_string] = {
    .visible = true,
    .named = true,
  },
  [sym__escape_sequence] = {
    .visible = false,
    .named = true,
  },
  [sym_boolean] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_source_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_string_repeat1] = {
    .visible = false,
    .named = false,
  },
};

enum ts_field_identifiers {
  field_marker = 1,
  field_meta = 2,
};

static const char * const ts_field_names[] = {
  [0] = NULL,
  [field_marker] = "marker",
  [field_meta] = "meta",
};

static const TSFieldMapSlice ts_field_map_slices[PRODUCTION_ID_COUNT] = {
  [1] = {.index = 0, .length = 2},
};

static const TSFieldMapEntry ts_field_map_entries[] = {
  [0] =
    {field_marker, 0},
    {field_meta, 1},
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
};

static TSCharacterRange sym_command_character_set_2[] = {
  {0, 0x08}, {0x0e, 0x1f}, {'!', '!'}, {'#', '\''}, {'*', '+'}, {'-', '.'}, {'0', '9'}, {'<', 'Z'},
  {'\\', '\\'}, {'_', 'z'}, {'|', '|'}, {'~', 0x84}, {0x86, 0x9f}, {0xa1, 0x10ffff},
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(15);
      ADVANCE_MAP(
        '"', 30,
        '#', 43,
        '(', 21,
        ')', 22,
        '+', 48,
        '-', 45,
        '.', 3,
        '/', 19,
        ':', 16,
        ';', 54,
        '[', 25,
        '\\', 44,
        ']', 26,
        '^', 20,
        '{', 23,
        '}', 24,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ' ||
          lookahead == ',') SKIP(13);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(27);
      if (lookahead == '*' ||
          ('A' <= lookahead && lookahead <= '_') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(46);
      if (lookahead != 0 &&
          lookahead != 0x85 &&
          lookahead != 0xa0) ADVANCE(53);
      END_STATE();
    case 1:
      if (lookahead == '"') ADVANCE(30);
      if (lookahead == '#') ADVANCE(33);
      if (lookahead == ';') ADVANCE(31);
      if (lookahead == '\\') ADVANCE(5);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ' ||
          lookahead == ',') ADVANCE(32);
      if (lookahead != 0) ADVANCE(34);
      END_STATE();
    case 2:
      if (lookahead == '#') ADVANCE(43);
      if (lookahead == '.') ADVANCE(3);
      if (lookahead == '/') ADVANCE(19);
      if (lookahead == ';') ADVANCE(54);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ' ||
          lookahead == ',') SKIP(2);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(4);
      if (lookahead == '*' ||
          lookahead == '-' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(46);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 3:
      if (lookahead == '/') ADVANCE(19);
      if (('.' <= lookahead && lookahead <= '9')) ADVANCE(4);
      if (lookahead == '*' ||
          lookahead == '-' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(17);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(18);
      END_STATE();
    case 4:
      if (lookahead == '/') ADVANCE(19);
      if (lookahead == '*' ||
          ('-' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(4);
      END_STATE();
    case 5:
      if (lookahead == 'u') ADVANCE(6);
      if (lookahead == 'x') ADVANCE(12);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(37);
      if (lookahead == '"' ||
          lookahead == '\'' ||
          lookahead == '?' ||
          lookahead == '\\' ||
          lookahead == 'a' ||
          lookahead == 'b' ||
          lookahead == 'f' ||
          lookahead == 'n' ||
          lookahead == 'r' ||
          ('t' <= lookahead && lookahead <= 'v')) ADVANCE(39);
      if (lookahead != 0) ADVANCE(35);
      END_STATE();
    case 6:
      if (lookahead == '{') ADVANCE(10);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(11);
      END_STATE();
    case 7:
      if (lookahead == '}') ADVANCE(39);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(7);
      END_STATE();
    case 8:
      if (lookahead == ' ' ||
          lookahead == '!') ADVANCE(54);
      END_STATE();
    case 9:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(39);
      END_STATE();
    case 10:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(7);
      END_STATE();
    case 11:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(12);
      END_STATE();
    case 12:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(9);
      END_STATE();
    case 13:
      if (eof) ADVANCE(15);
      ADVANCE_MAP(
        '"', 30,
        '#', 43,
        '(', 21,
        ')', 22,
        '+', 48,
        '-', 45,
        '.', 3,
        '/', 19,
        ':', 16,
        ';', 54,
        '[', 25,
        ']', 26,
        '^', 20,
        '{', 23,
        '}', 24,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ' ||
          lookahead == ',') SKIP(13);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(27);
      if (lookahead == '*' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(46);
      if (lookahead != 0 &&
          lookahead != 0x85 &&
          lookahead != 0xa0) ADVANCE(53);
      END_STATE();
    case 14:
      if (eof) ADVANCE(15);
      if (lookahead == '#') ADVANCE(8);
      if (lookahead == ';') ADVANCE(54);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ' ||
          lookahead == ',') SKIP(14);
      END_STATE();
    case 15:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 16:
      ACCEPT_TOKEN(anon_sym_COLON);
      END_STATE();
    case 17:
      ACCEPT_TOKEN(sym_command);
      if (lookahead == '/') ADVANCE(19);
      if (lookahead == '*' ||
          ('-' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(17);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(18);
      END_STATE();
    case 18:
      ACCEPT_TOKEN(sym_command);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(18);
      END_STATE();
    case 19:
      ACCEPT_TOKEN(sym_path);
      if (lookahead == '/') ADVANCE(19);
      if (lookahead == '*' ||
          ('-' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(19);
      END_STATE();
    case 20:
      ACCEPT_TOKEN(anon_sym_CARET);
      END_STATE();
    case 21:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 22:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 23:
      ACCEPT_TOKEN(anon_sym_LBRACE);
      END_STATE();
    case 24:
      ACCEPT_TOKEN(anon_sym_RBRACE);
      END_STATE();
    case 25:
      ACCEPT_TOKEN(anon_sym_LBRACK);
      END_STATE();
    case 26:
      ACCEPT_TOKEN(anon_sym_RBRACK);
      END_STATE();
    case 27:
      ACCEPT_TOKEN(sym_number);
      if (lookahead == '/') ADVANCE(19);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(27);
      if (lookahead == '*' ||
          lookahead == '-' ||
          lookahead == '.' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(4);
      END_STATE();
    case 28:
      ACCEPT_TOKEN(sym_number);
      if (lookahead == '/') ADVANCE(19);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(28);
      if (lookahead == '*' ||
          lookahead == '-' ||
          lookahead == '.' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(46);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 29:
      ACCEPT_TOKEN(sym_number);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(29);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 30:
      ACCEPT_TOKEN(anon_sym_DQUOTE);
      END_STATE();
    case 31:
      ACCEPT_TOKEN(sym_string_fragment);
      if (lookahead == '\n') ADVANCE(34);
      if (lookahead != 0 &&
          lookahead != '"' &&
          lookahead != '\\') ADVANCE(31);
      END_STATE();
    case 32:
      ACCEPT_TOKEN(sym_string_fragment);
      if (lookahead == '#') ADVANCE(33);
      if (lookahead == ';') ADVANCE(31);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ' ||
          lookahead == ',') ADVANCE(32);
      if (lookahead != 0 &&
          lookahead != '"' &&
          lookahead != '#' &&
          lookahead != '\\') ADVANCE(34);
      END_STATE();
    case 33:
      ACCEPT_TOKEN(sym_string_fragment);
      if (lookahead == ' ' ||
          lookahead == '!') ADVANCE(31);
      if (lookahead != 0 &&
          (lookahead < ' ' || '"' < lookahead) &&
          lookahead != '\\') ADVANCE(34);
      END_STATE();
    case 34:
      ACCEPT_TOKEN(sym_string_fragment);
      if (lookahead != 0 &&
          lookahead != '"' &&
          lookahead != '\\') ADVANCE(34);
      END_STATE();
    case 35:
      ACCEPT_TOKEN(aux_sym__escape_sequence_token1);
      END_STATE();
    case 36:
      ACCEPT_TOKEN(aux_sym__escape_sequence_token1);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(41);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 37:
      ACCEPT_TOKEN(aux_sym__escape_sequence_token1);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(40);
      END_STATE();
    case 38:
      ACCEPT_TOKEN(aux_sym__escape_sequence_token1);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 39:
      ACCEPT_TOKEN(sym_escape_sequence);
      END_STATE();
    case 40:
      ACCEPT_TOKEN(sym_escape_sequence);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(39);
      END_STATE();
    case 41:
      ACCEPT_TOKEN(sym_escape_sequence);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(42);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 42:
      ACCEPT_TOKEN(sym_escape_sequence);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 43:
      ACCEPT_TOKEN(sym_symbol);
      if (lookahead == ' ') ADVANCE(54);
      if (lookahead == '!') ADVANCE(49);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 44:
      ACCEPT_TOKEN(sym_symbol);
      if (lookahead == '"') ADVANCE(39);
      if (lookahead == 'u') ADVANCE(47);
      if (lookahead == 'x') ADVANCE(52);
      if (('0' <= lookahead && lookahead <= '7')) ADVANCE(36);
      if (lookahead == '\'' ||
          lookahead == '?' ||
          lookahead == '\\' ||
          lookahead == 'a' ||
          lookahead == 'b' ||
          lookahead == 'f' ||
          lookahead == 'n' ||
          lookahead == 'r' ||
          ('t' <= lookahead && lookahead <= 'v')) ADVANCE(42);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ' ||
          lookahead == '(' ||
          lookahead == ')' ||
          lookahead == ',' ||
          lookahead == '/' ||
          lookahead == ':' ||
          lookahead == ';' ||
          ('[' <= lookahead && lookahead <= '^') ||
          lookahead == '{' ||
          lookahead == '}' ||
          lookahead == 0x85 ||
          lookahead == 0xa0) ADVANCE(35);
      if (lookahead != 0) ADVANCE(38);
      END_STATE();
    case 45:
      ACCEPT_TOKEN(sym_symbol);
      if (lookahead == '/') ADVANCE(19);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(28);
      if (lookahead == '*' ||
          lookahead == '-' ||
          lookahead == '.' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(46);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 46:
      ACCEPT_TOKEN(sym_symbol);
      if (lookahead == '/') ADVANCE(19);
      if (lookahead == '*' ||
          ('-' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(46);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 47:
      ACCEPT_TOKEN(sym_symbol);
      if (lookahead == '{') ADVANCE(10);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(50);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 48:
      ACCEPT_TOKEN(sym_symbol);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(29);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 49:
      ACCEPT_TOKEN(sym_symbol);
      if (lookahead == '\t' ||
          (0x0b <= lookahead && lookahead <= '\r') ||
          lookahead == ' ' ||
          lookahead == '"' ||
          lookahead == '(' ||
          lookahead == ')' ||
          lookahead == ',' ||
          lookahead == '/' ||
          lookahead == ':' ||
          lookahead == ';' ||
          lookahead == '[' ||
          lookahead == ']' ||
          lookahead == '^' ||
          lookahead == '{' ||
          lookahead == '}' ||
          lookahead == 0x85 ||
          lookahead == 0xa0) ADVANCE(54);
      if (lookahead != 0 &&
          (lookahead < '\t' || '\r' < lookahead)) ADVANCE(49);
      END_STATE();
    case 50:
      ACCEPT_TOKEN(sym_symbol);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(52);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 51:
      ACCEPT_TOKEN(sym_symbol);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(42);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 52:
      ACCEPT_TOKEN(sym_symbol);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(51);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 53:
      ACCEPT_TOKEN(sym_symbol);
      if ((!eof && set_contains(sym_command_character_set_2, 14, lookahead))) ADVANCE(53);
      END_STATE();
    case 54:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(54);
      END_STATE();
    default:
      return false;
  }
}

static bool ts_lex_keywords(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (lookahead == '_') ADVANCE(1);
      if (lookahead == 'f') ADVANCE(2);
      if (lookahead == 'n') ADVANCE(3);
      if (lookahead == 't') ADVANCE(4);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ' ||
          lookahead == ',') SKIP(0);
      END_STATE();
    case 1:
      ACCEPT_TOKEN(sym_ignore);
      END_STATE();
    case 2:
      if (lookahead == 'a') ADVANCE(5);
      END_STATE();
    case 3:
      if (lookahead == 'u') ADVANCE(6);
      END_STATE();
    case 4:
      if (lookahead == 'r') ADVANCE(7);
      END_STATE();
    case 5:
      if (lookahead == 'l') ADVANCE(8);
      END_STATE();
    case 6:
      if (lookahead == 'l') ADVANCE(9);
      END_STATE();
    case 7:
      if (lookahead == 'u') ADVANCE(10);
      END_STATE();
    case 8:
      if (lookahead == 's') ADVANCE(11);
      END_STATE();
    case 9:
      if (lookahead == 'l') ADVANCE(12);
      END_STATE();
    case 10:
      if (lookahead == 'e') ADVANCE(13);
      END_STATE();
    case 11:
      if (lookahead == 'e') ADVANCE(14);
      END_STATE();
    case 12:
      ACCEPT_TOKEN(sym_null);
      END_STATE();
    case 13:
      ACCEPT_TOKEN(anon_sym_true);
      END_STATE();
    case 14:
      ACCEPT_TOKEN(anon_sym_false);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 13},
  [2] = {.lex_state = 13},
  [3] = {.lex_state = 13},
  [4] = {.lex_state = 13},
  [5] = {.lex_state = 13},
  [6] = {.lex_state = 13},
  [7] = {.lex_state = 13},
  [8] = {.lex_state = 13},
  [9] = {.lex_state = 13},
  [10] = {.lex_state = 13},
  [11] = {.lex_state = 13},
  [12] = {.lex_state = 13},
  [13] = {.lex_state = 13},
  [14] = {.lex_state = 13},
  [15] = {.lex_state = 13},
  [16] = {.lex_state = 13},
  [17] = {.lex_state = 13},
  [18] = {.lex_state = 13},
  [19] = {.lex_state = 13},
  [20] = {.lex_state = 13},
  [21] = {.lex_state = 13},
  [22] = {.lex_state = 13},
  [23] = {.lex_state = 13},
  [24] = {.lex_state = 13},
  [25] = {.lex_state = 13},
  [26] = {.lex_state = 13},
  [27] = {.lex_state = 1},
  [28] = {.lex_state = 1},
  [29] = {.lex_state = 1},
  [30] = {.lex_state = 2},
  [31] = {.lex_state = 14},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [sym_symbol] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [sym_command] = ACTIONS(1),
    [sym_path] = ACTIONS(1),
    [anon_sym_CARET] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [anon_sym_RPAREN] = ACTIONS(1),
    [anon_sym_LBRACE] = ACTIONS(1),
    [anon_sym_RBRACE] = ACTIONS(1),
    [anon_sym_LBRACK] = ACTIONS(1),
    [anon_sym_RBRACK] = ACTIONS(1),
    [sym_number] = ACTIONS(1),
    [anon_sym_DQUOTE] = ACTIONS(1),
    [aux_sym__escape_sequence_token1] = ACTIONS(1),
    [sym_escape_sequence] = ACTIONS(1),
    [sym_ignore] = ACTIONS(1),
    [sym_null] = ACTIONS(1),
    [anon_sym_false] = ACTIONS(1),
    [anon_sym_true] = ACTIONS(1),
    [sym_comment] = ACTIONS(3),
  },
  [1] = {
    [sym_source] = STATE(31),
    [sym_form] = STATE(3),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [aux_sym_source_repeat1] = STATE(3),
    [ts_builtin_sym_end] = ACTIONS(5),
    [sym_symbol] = ACTIONS(7),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(11),
    [sym_path] = ACTIONS(13),
    [anon_sym_CARET] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_LBRACE] = ACTIONS(19),
    [anon_sym_LBRACK] = ACTIONS(21),
    [sym_number] = ACTIONS(23),
    [anon_sym_DQUOTE] = ACTIONS(25),
    [sym_ignore] = ACTIONS(23),
    [sym_null] = ACTIONS(23),
    [anon_sym_false] = ACTIONS(27),
    [anon_sym_true] = ACTIONS(27),
    [sym_comment] = ACTIONS(3),
  },
  [2] = {
    [sym_form] = STATE(2),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [aux_sym_source_repeat1] = STATE(2),
    [ts_builtin_sym_end] = ACTIONS(29),
    [sym_symbol] = ACTIONS(31),
    [anon_sym_COLON] = ACTIONS(34),
    [sym_command] = ACTIONS(37),
    [sym_path] = ACTIONS(40),
    [anon_sym_CARET] = ACTIONS(43),
    [anon_sym_LPAREN] = ACTIONS(46),
    [anon_sym_RPAREN] = ACTIONS(29),
    [anon_sym_LBRACE] = ACTIONS(49),
    [anon_sym_RBRACE] = ACTIONS(29),
    [anon_sym_LBRACK] = ACTIONS(52),
    [anon_sym_RBRACK] = ACTIONS(29),
    [sym_number] = ACTIONS(55),
    [anon_sym_DQUOTE] = ACTIONS(58),
    [sym_ignore] = ACTIONS(55),
    [sym_null] = ACTIONS(55),
    [anon_sym_false] = ACTIONS(61),
    [anon_sym_true] = ACTIONS(61),
    [sym_comment] = ACTIONS(3),
  },
  [3] = {
    [sym_form] = STATE(2),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [aux_sym_source_repeat1] = STATE(2),
    [ts_builtin_sym_end] = ACTIONS(64),
    [sym_symbol] = ACTIONS(7),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(11),
    [sym_path] = ACTIONS(13),
    [anon_sym_CARET] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_LBRACE] = ACTIONS(19),
    [anon_sym_LBRACK] = ACTIONS(21),
    [sym_number] = ACTIONS(23),
    [anon_sym_DQUOTE] = ACTIONS(25),
    [sym_ignore] = ACTIONS(23),
    [sym_null] = ACTIONS(23),
    [anon_sym_false] = ACTIONS(27),
    [anon_sym_true] = ACTIONS(27),
    [sym_comment] = ACTIONS(3),
  },
  [4] = {
    [sym_form] = STATE(2),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [aux_sym_source_repeat1] = STATE(2),
    [sym_symbol] = ACTIONS(7),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(11),
    [sym_path] = ACTIONS(13),
    [anon_sym_CARET] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_LBRACE] = ACTIONS(19),
    [anon_sym_LBRACK] = ACTIONS(21),
    [anon_sym_RBRACK] = ACTIONS(66),
    [sym_number] = ACTIONS(23),
    [anon_sym_DQUOTE] = ACTIONS(25),
    [sym_ignore] = ACTIONS(23),
    [sym_null] = ACTIONS(23),
    [anon_sym_false] = ACTIONS(27),
    [anon_sym_true] = ACTIONS(27),
    [sym_comment] = ACTIONS(3),
  },
  [5] = {
    [sym_form] = STATE(9),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [aux_sym_source_repeat1] = STATE(9),
    [sym_symbol] = ACTIONS(7),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(11),
    [sym_path] = ACTIONS(13),
    [anon_sym_CARET] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_RPAREN] = ACTIONS(68),
    [anon_sym_LBRACE] = ACTIONS(19),
    [anon_sym_LBRACK] = ACTIONS(21),
    [sym_number] = ACTIONS(23),
    [anon_sym_DQUOTE] = ACTIONS(25),
    [sym_ignore] = ACTIONS(23),
    [sym_null] = ACTIONS(23),
    [anon_sym_false] = ACTIONS(27),
    [anon_sym_true] = ACTIONS(27),
    [sym_comment] = ACTIONS(3),
  },
  [6] = {
    [sym_form] = STATE(8),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [aux_sym_source_repeat1] = STATE(8),
    [sym_symbol] = ACTIONS(7),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(11),
    [sym_path] = ACTIONS(13),
    [anon_sym_CARET] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_LBRACE] = ACTIONS(19),
    [anon_sym_RBRACE] = ACTIONS(70),
    [anon_sym_LBRACK] = ACTIONS(21),
    [sym_number] = ACTIONS(23),
    [anon_sym_DQUOTE] = ACTIONS(25),
    [sym_ignore] = ACTIONS(23),
    [sym_null] = ACTIONS(23),
    [anon_sym_false] = ACTIONS(27),
    [anon_sym_true] = ACTIONS(27),
    [sym_comment] = ACTIONS(3),
  },
  [7] = {
    [sym_form] = STATE(4),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [aux_sym_source_repeat1] = STATE(4),
    [sym_symbol] = ACTIONS(7),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(11),
    [sym_path] = ACTIONS(13),
    [anon_sym_CARET] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_LBRACE] = ACTIONS(19),
    [anon_sym_LBRACK] = ACTIONS(21),
    [anon_sym_RBRACK] = ACTIONS(72),
    [sym_number] = ACTIONS(23),
    [anon_sym_DQUOTE] = ACTIONS(25),
    [sym_ignore] = ACTIONS(23),
    [sym_null] = ACTIONS(23),
    [anon_sym_false] = ACTIONS(27),
    [anon_sym_true] = ACTIONS(27),
    [sym_comment] = ACTIONS(3),
  },
  [8] = {
    [sym_form] = STATE(2),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [aux_sym_source_repeat1] = STATE(2),
    [sym_symbol] = ACTIONS(7),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(11),
    [sym_path] = ACTIONS(13),
    [anon_sym_CARET] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_LBRACE] = ACTIONS(19),
    [anon_sym_RBRACE] = ACTIONS(74),
    [anon_sym_LBRACK] = ACTIONS(21),
    [sym_number] = ACTIONS(23),
    [anon_sym_DQUOTE] = ACTIONS(25),
    [sym_ignore] = ACTIONS(23),
    [sym_null] = ACTIONS(23),
    [anon_sym_false] = ACTIONS(27),
    [anon_sym_true] = ACTIONS(27),
    [sym_comment] = ACTIONS(3),
  },
  [9] = {
    [sym_form] = STATE(2),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [aux_sym_source_repeat1] = STATE(2),
    [sym_symbol] = ACTIONS(7),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(11),
    [sym_path] = ACTIONS(13),
    [anon_sym_CARET] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_RPAREN] = ACTIONS(76),
    [anon_sym_LBRACE] = ACTIONS(19),
    [anon_sym_LBRACK] = ACTIONS(21),
    [sym_number] = ACTIONS(23),
    [anon_sym_DQUOTE] = ACTIONS(25),
    [sym_ignore] = ACTIONS(23),
    [sym_null] = ACTIONS(23),
    [anon_sym_false] = ACTIONS(27),
    [anon_sym_true] = ACTIONS(27),
    [sym_comment] = ACTIONS(3),
  },
  [10] = {
    [sym_form] = STATE(11),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [sym_symbol] = ACTIONS(7),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(11),
    [sym_path] = ACTIONS(13),
    [anon_sym_CARET] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_LBRACE] = ACTIONS(19),
    [anon_sym_LBRACK] = ACTIONS(21),
    [sym_number] = ACTIONS(23),
    [anon_sym_DQUOTE] = ACTIONS(25),
    [sym_ignore] = ACTIONS(23),
    [sym_null] = ACTIONS(23),
    [anon_sym_false] = ACTIONS(27),
    [anon_sym_true] = ACTIONS(27),
    [sym_comment] = ACTIONS(3),
  },
  [11] = {
    [sym_form] = STATE(22),
    [sym_keyword] = STATE(21),
    [sym_symbind] = STATE(12),
    [sym_meta] = STATE(21),
    [sym_list] = STATE(21),
    [sym_scope] = STATE(21),
    [sym_cons] = STATE(21),
    [sym_literal] = STATE(21),
    [sym_string] = STATE(18),
    [sym_boolean] = STATE(18),
    [sym_symbol] = ACTIONS(7),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(11),
    [sym_path] = ACTIONS(13),
    [anon_sym_CARET] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_LBRACE] = ACTIONS(19),
    [anon_sym_LBRACK] = ACTIONS(21),
    [sym_number] = ACTIONS(23),
    [anon_sym_DQUOTE] = ACTIONS(25),
    [sym_ignore] = ACTIONS(23),
    [sym_null] = ACTIONS(23),
    [anon_sym_false] = ACTIONS(27),
    [anon_sym_true] = ACTIONS(27),
    [sym_comment] = ACTIONS(3),
  },
  [12] = {
    [sym_keyword] = STATE(20),
    [ts_builtin_sym_end] = ACTIONS(78),
    [sym_symbol] = ACTIONS(80),
    [anon_sym_COLON] = ACTIONS(9),
    [sym_command] = ACTIONS(80),
    [sym_path] = ACTIONS(78),
    [anon_sym_CARET] = ACTIONS(78),
    [anon_sym_LPAREN] = ACTIONS(78),
    [anon_sym_RPAREN] = ACTIONS(78),
    [anon_sym_LBRACE] = ACTIONS(78),
    [anon_sym_RBRACE] = ACTIONS(78),
    [anon_sym_LBRACK] = ACTIONS(78),
    [anon_sym_RBRACK] = ACTIONS(78),
    [sym_number] = ACTIONS(80),
    [anon_sym_DQUOTE] = ACTIONS(78),
    [sym_ignore] = ACTIONS(80),
    [sym_null] = ACTIONS(80),
    [anon_sym_false] = ACTIONS(80),
    [anon_sym_true] = ACTIONS(80),
    [sym_comment] = ACTIONS(3),
  },
  [13] = {
    [ts_builtin_sym_end] = ACTIONS(82),
    [sym_symbol] = ACTIONS(84),
    [anon_sym_COLON] = ACTIONS(82),
    [sym_command] = ACTIONS(84),
    [sym_path] = ACTIONS(82),
    [anon_sym_CARET] = ACTIONS(82),
    [anon_sym_LPAREN] = ACTIONS(82),
    [anon_sym_RPAREN] = ACTIONS(82),
    [anon_sym_LBRACE] = ACTIONS(82),
    [anon_sym_RBRACE] = ACTIONS(82),
    [anon_sym_LBRACK] = ACTIONS(82),
    [anon_sym_RBRACK] = ACTIONS(82),
    [sym_number] = ACTIONS(84),
    [anon_sym_DQUOTE] = ACTIONS(82),
    [sym_ignore] = ACTIONS(84),
    [sym_null] = ACTIONS(84),
    [anon_sym_false] = ACTIONS(84),
    [anon_sym_true] = ACTIONS(84),
    [sym_comment] = ACTIONS(3),
  },
  [14] = {
    [ts_builtin_sym_end] = ACTIONS(86),
    [sym_symbol] = ACTIONS(88),
    [anon_sym_COLON] = ACTIONS(86),
    [sym_command] = ACTIONS(88),
    [sym_path] = ACTIONS(86),
    [anon_sym_CARET] = ACTIONS(86),
    [anon_sym_LPAREN] = ACTIONS(86),
    [anon_sym_RPAREN] = ACTIONS(86),
    [anon_sym_LBRACE] = ACTIONS(86),
    [anon_sym_RBRACE] = ACTIONS(86),
    [anon_sym_LBRACK] = ACTIONS(86),
    [anon_sym_RBRACK] = ACTIONS(86),
    [sym_number] = ACTIONS(88),
    [anon_sym_DQUOTE] = ACTIONS(86),
    [sym_ignore] = ACTIONS(88),
    [sym_null] = ACTIONS(88),
    [anon_sym_false] = ACTIONS(88),
    [anon_sym_true] = ACTIONS(88),
    [sym_comment] = ACTIONS(3),
  },
  [15] = {
    [ts_builtin_sym_end] = ACTIONS(90),
    [sym_symbol] = ACTIONS(92),
    [anon_sym_COLON] = ACTIONS(90),
    [sym_command] = ACTIONS(92),
    [sym_path] = ACTIONS(90),
    [anon_sym_CARET] = ACTIONS(90),
    [anon_sym_LPAREN] = ACTIONS(90),
    [anon_sym_RPAREN] = ACTIONS(90),
    [anon_sym_LBRACE] = ACTIONS(90),
    [anon_sym_RBRACE] = ACTIONS(90),
    [anon_sym_LBRACK] = ACTIONS(90),
    [anon_sym_RBRACK] = ACTIONS(90),
    [sym_number] = ACTIONS(92),
    [anon_sym_DQUOTE] = ACTIONS(90),
    [sym_ignore] = ACTIONS(92),
    [sym_null] = ACTIONS(92),
    [anon_sym_false] = ACTIONS(92),
    [anon_sym_true] = ACTIONS(92),
    [sym_comment] = ACTIONS(3),
  },
  [16] = {
    [ts_builtin_sym_end] = ACTIONS(94),
    [sym_symbol] = ACTIONS(96),
    [anon_sym_COLON] = ACTIONS(94),
    [sym_command] = ACTIONS(96),
    [sym_path] = ACTIONS(94),
    [anon_sym_CARET] = ACTIONS(94),
    [anon_sym_LPAREN] = ACTIONS(94),
    [anon_sym_RPAREN] = ACTIONS(94),
    [anon_sym_LBRACE] = ACTIONS(94),
    [anon_sym_RBRACE] = ACTIONS(94),
    [anon_sym_LBRACK] = ACTIONS(94),
    [anon_sym_RBRACK] = ACTIONS(94),
    [sym_number] = ACTIONS(96),
    [anon_sym_DQUOTE] = ACTIONS(94),
    [sym_ignore] = ACTIONS(96),
    [sym_null] = ACTIONS(96),
    [anon_sym_false] = ACTIONS(96),
    [anon_sym_true] = ACTIONS(96),
    [sym_comment] = ACTIONS(3),
  },
  [17] = {
    [ts_builtin_sym_end] = ACTIONS(98),
    [sym_symbol] = ACTIONS(100),
    [anon_sym_COLON] = ACTIONS(98),
    [sym_command] = ACTIONS(100),
    [sym_path] = ACTIONS(98),
    [anon_sym_CARET] = ACTIONS(98),
    [anon_sym_LPAREN] = ACTIONS(98),
    [anon_sym_RPAREN] = ACTIONS(98),
    [anon_sym_LBRACE] = ACTIONS(98),
    [anon_sym_RBRACE] = ACTIONS(98),
    [anon_sym_LBRACK] = ACTIONS(98),
    [anon_sym_RBRACK] = ACTIONS(98),
    [sym_number] = ACTIONS(100),
    [anon_sym_DQUOTE] = ACTIONS(98),
    [sym_ignore] = ACTIONS(100),
    [sym_null] = ACTIONS(100),
    [anon_sym_false] = ACTIONS(100),
    [anon_sym_true] = ACTIONS(100),
    [sym_comment] = ACTIONS(3),
  },
  [18] = {
    [ts_builtin_sym_end] = ACTIONS(102),
    [sym_symbol] = ACTIONS(104),
    [anon_sym_COLON] = ACTIONS(102),
    [sym_command] = ACTIONS(104),
    [sym_path] = ACTIONS(102),
    [anon_sym_CARET] = ACTIONS(102),
    [anon_sym_LPAREN] = ACTIONS(102),
    [anon_sym_RPAREN] = ACTIONS(102),
    [anon_sym_LBRACE] = ACTIONS(102),
    [anon_sym_RBRACE] = ACTIONS(102),
    [anon_sym_LBRACK] = ACTIONS(102),
    [anon_sym_RBRACK] = ACTIONS(102),
    [sym_number] = ACTIONS(104),
    [anon_sym_DQUOTE] = ACTIONS(102),
    [sym_ignore] = ACTIONS(104),
    [sym_null] = ACTIONS(104),
    [anon_sym_false] = ACTIONS(104),
    [anon_sym_true] = ACTIONS(104),
    [sym_comment] = ACTIONS(3),
  },
  [19] = {
    [ts_builtin_sym_end] = ACTIONS(106),
    [sym_symbol] = ACTIONS(108),
    [anon_sym_COLON] = ACTIONS(106),
    [sym_command] = ACTIONS(108),
    [sym_path] = ACTIONS(106),
    [anon_sym_CARET] = ACTIONS(106),
    [anon_sym_LPAREN] = ACTIONS(106),
    [anon_sym_RPAREN] = ACTIONS(106),
    [anon_sym_LBRACE] = ACTIONS(106),
    [anon_sym_RBRACE] = ACTIONS(106),
    [anon_sym_LBRACK] = ACTIONS(106),
    [anon_sym_RBRACK] = ACTIONS(106),
    [sym_number] = ACTIONS(108),
    [anon_sym_DQUOTE] = ACTIONS(106),
    [sym_ignore] = ACTIONS(108),
    [sym_null] = ACTIONS(108),
    [anon_sym_false] = ACTIONS(108),
    [anon_sym_true] = ACTIONS(108),
    [sym_comment] = ACTIONS(3),
  },
  [20] = {
    [ts_builtin_sym_end] = ACTIONS(110),
    [sym_symbol] = ACTIONS(112),
    [anon_sym_COLON] = ACTIONS(110),
    [sym_command] = ACTIONS(112),
    [sym_path] = ACTIONS(110),
    [anon_sym_CARET] = ACTIONS(110),
    [anon_sym_LPAREN] = ACTIONS(110),
    [anon_sym_RPAREN] = ACTIONS(110),
    [anon_sym_LBRACE] = ACTIONS(110),
    [anon_sym_RBRACE] = ACTIONS(110),
    [anon_sym_LBRACK] = ACTIONS(110),
    [anon_sym_RBRACK] = ACTIONS(110),
    [sym_number] = ACTIONS(112),
    [anon_sym_DQUOTE] = ACTIONS(110),
    [sym_ignore] = ACTIONS(112),
    [sym_null] = ACTIONS(112),
    [anon_sym_false] = ACTIONS(112),
    [anon_sym_true] = ACTIONS(112),
    [sym_comment] = ACTIONS(3),
  },
  [21] = {
    [ts_builtin_sym_end] = ACTIONS(78),
    [sym_symbol] = ACTIONS(80),
    [anon_sym_COLON] = ACTIONS(78),
    [sym_command] = ACTIONS(80),
    [sym_path] = ACTIONS(78),
    [anon_sym_CARET] = ACTIONS(78),
    [anon_sym_LPAREN] = ACTIONS(78),
    [anon_sym_RPAREN] = ACTIONS(78),
    [anon_sym_LBRACE] = ACTIONS(78),
    [anon_sym_RBRACE] = ACTIONS(78),
    [anon_sym_LBRACK] = ACTIONS(78),
    [anon_sym_RBRACK] = ACTIONS(78),
    [sym_number] = ACTIONS(80),
    [anon_sym_DQUOTE] = ACTIONS(78),
    [sym_ignore] = ACTIONS(80),
    [sym_null] = ACTIONS(80),
    [anon_sym_false] = ACTIONS(80),
    [anon_sym_true] = ACTIONS(80),
    [sym_comment] = ACTIONS(3),
  },
  [22] = {
    [ts_builtin_sym_end] = ACTIONS(114),
    [sym_symbol] = ACTIONS(116),
    [anon_sym_COLON] = ACTIONS(114),
    [sym_command] = ACTIONS(116),
    [sym_path] = ACTIONS(114),
    [anon_sym_CARET] = ACTIONS(114),
    [anon_sym_LPAREN] = ACTIONS(114),
    [anon_sym_RPAREN] = ACTIONS(114),
    [anon_sym_LBRACE] = ACTIONS(114),
    [anon_sym_RBRACE] = ACTIONS(114),
    [anon_sym_LBRACK] = ACTIONS(114),
    [anon_sym_RBRACK] = ACTIONS(114),
    [sym_number] = ACTIONS(116),
    [anon_sym_DQUOTE] = ACTIONS(114),
    [sym_ignore] = ACTIONS(116),
    [sym_null] = ACTIONS(116),
    [anon_sym_false] = ACTIONS(116),
    [anon_sym_true] = ACTIONS(116),
    [sym_comment] = ACTIONS(3),
  },
  [23] = {
    [ts_builtin_sym_end] = ACTIONS(118),
    [sym_symbol] = ACTIONS(120),
    [anon_sym_COLON] = ACTIONS(118),
    [sym_command] = ACTIONS(120),
    [sym_path] = ACTIONS(118),
    [anon_sym_CARET] = ACTIONS(118),
    [anon_sym_LPAREN] = ACTIONS(118),
    [anon_sym_RPAREN] = ACTIONS(118),
    [anon_sym_LBRACE] = ACTIONS(118),
    [anon_sym_RBRACE] = ACTIONS(118),
    [anon_sym_LBRACK] = ACTIONS(118),
    [anon_sym_RBRACK] = ACTIONS(118),
    [sym_number] = ACTIONS(120),
    [anon_sym_DQUOTE] = ACTIONS(118),
    [sym_ignore] = ACTIONS(120),
    [sym_null] = ACTIONS(120),
    [anon_sym_false] = ACTIONS(120),
    [anon_sym_true] = ACTIONS(120),
    [sym_comment] = ACTIONS(3),
  },
  [24] = {
    [ts_builtin_sym_end] = ACTIONS(122),
    [sym_symbol] = ACTIONS(124),
    [anon_sym_COLON] = ACTIONS(122),
    [sym_command] = ACTIONS(124),
    [sym_path] = ACTIONS(122),
    [anon_sym_CARET] = ACTIONS(122),
    [anon_sym_LPAREN] = ACTIONS(122),
    [anon_sym_RPAREN] = ACTIONS(122),
    [anon_sym_LBRACE] = ACTIONS(122),
    [anon_sym_RBRACE] = ACTIONS(122),
    [anon_sym_LBRACK] = ACTIONS(122),
    [anon_sym_RBRACK] = ACTIONS(122),
    [sym_number] = ACTIONS(124),
    [anon_sym_DQUOTE] = ACTIONS(122),
    [sym_ignore] = ACTIONS(124),
    [sym_null] = ACTIONS(124),
    [anon_sym_false] = ACTIONS(124),
    [anon_sym_true] = ACTIONS(124),
    [sym_comment] = ACTIONS(3),
  },
  [25] = {
    [ts_builtin_sym_end] = ACTIONS(126),
    [sym_symbol] = ACTIONS(128),
    [anon_sym_COLON] = ACTIONS(126),
    [sym_command] = ACTIONS(128),
    [sym_path] = ACTIONS(126),
    [anon_sym_CARET] = ACTIONS(126),
    [anon_sym_LPAREN] = ACTIONS(126),
    [anon_sym_RPAREN] = ACTIONS(126),
    [anon_sym_LBRACE] = ACTIONS(126),
    [anon_sym_RBRACE] = ACTIONS(126),
    [anon_sym_LBRACK] = ACTIONS(126),
    [anon_sym_RBRACK] = ACTIONS(126),
    [sym_number] = ACTIONS(128),
    [anon_sym_DQUOTE] = ACTIONS(126),
    [sym_ignore] = ACTIONS(128),
    [sym_null] = ACTIONS(128),
    [anon_sym_false] = ACTIONS(128),
    [anon_sym_true] = ACTIONS(128),
    [sym_comment] = ACTIONS(3),
  },
  [26] = {
    [ts_builtin_sym_end] = ACTIONS(130),
    [sym_symbol] = ACTIONS(132),
    [anon_sym_COLON] = ACTIONS(130),
    [sym_command] = ACTIONS(132),
    [sym_path] = ACTIONS(130),
    [anon_sym_CARET] = ACTIONS(130),
    [anon_sym_LPAREN] = ACTIONS(130),
    [anon_sym_RPAREN] = ACTIONS(130),
    [anon_sym_LBRACE] = ACTIONS(130),
    [anon_sym_RBRACE] = ACTIONS(130),
    [anon_sym_LBRACK] = ACTIONS(130),
    [anon_sym_RBRACK] = ACTIONS(130),
    [sym_number] = ACTIONS(132),
    [anon_sym_DQUOTE] = ACTIONS(130),
    [sym_ignore] = ACTIONS(132),
    [sym_null] = ACTIONS(132),
    [anon_sym_false] = ACTIONS(132),
    [anon_sym_true] = ACTIONS(132),
    [sym_comment] = ACTIONS(3),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(134), 1,
      anon_sym_DQUOTE,
    ACTIONS(136), 1,
      sym_string_fragment,
    ACTIONS(138), 2,
      aux_sym__escape_sequence_token1,
      sym_escape_sequence,
    STATE(28), 2,
      sym__escape_sequence,
      aux_sym_string_repeat1,
  [18] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(140), 1,
      anon_sym_DQUOTE,
    ACTIONS(142), 1,
      sym_string_fragment,
    ACTIONS(144), 2,
      aux_sym__escape_sequence_token1,
      sym_escape_sequence,
    STATE(29), 2,
      sym__escape_sequence,
      aux_sym_string_repeat1,
  [36] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(146), 1,
      anon_sym_DQUOTE,
    ACTIONS(148), 1,
      sym_string_fragment,
    ACTIONS(151), 2,
      aux_sym__escape_sequence_token1,
      sym_escape_sequence,
    STATE(29), 2,
      sym__escape_sequence,
      aux_sym_string_repeat1,
  [54] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(156), 1,
      sym_path,
    ACTIONS(154), 2,
      sym_command,
      sym_symbol,
  [65] = 2,
    ACTIONS(158), 1,
      ts_builtin_sym_end,
    ACTIONS(160), 1,
      sym_comment,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(27)] = 0,
  [SMALL_STATE(28)] = 18,
  [SMALL_STATE(29)] = 36,
  [SMALL_STATE(30)] = 54,
  [SMALL_STATE(31)] = 65,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = false}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source, 0, 0, 0),
  [7] = {.entry = {.count = 1, .reusable = false}}, SHIFT(12),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(30),
  [11] = {.entry = {.count = 1, .reusable = false}}, SHIFT(21),
  [13] = {.entry = {.count = 1, .reusable = true}}, SHIFT(21),
  [15] = {.entry = {.count = 1, .reusable = true}}, SHIFT(10),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(5),
  [19] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [21] = {.entry = {.count = 1, .reusable = true}}, SHIFT(7),
  [23] = {.entry = {.count = 1, .reusable = false}}, SHIFT(18),
  [25] = {.entry = {.count = 1, .reusable = true}}, SHIFT(27),
  [27] = {.entry = {.count = 1, .reusable = false}}, SHIFT(15),
  [29] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0),
  [31] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(12),
  [34] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(30),
  [37] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(21),
  [40] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(21),
  [43] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(10),
  [46] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(5),
  [49] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(6),
  [52] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(7),
  [55] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(18),
  [58] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(27),
  [61] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_source_repeat1, 2, 0, 0), SHIFT_REPEAT(15),
  [64] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source, 1, 0, 0),
  [66] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
  [68] = {.entry = {.count = 1, .reusable = true}}, SHIFT(13),
  [70] = {.entry = {.count = 1, .reusable = true}}, SHIFT(16),
  [72] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [74] = {.entry = {.count = 1, .reusable = true}}, SHIFT(24),
  [76] = {.entry = {.count = 1, .reusable = true}}, SHIFT(23),
  [78] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_form, 1, 0, 0),
  [80] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_form, 1, 0, 0),
  [82] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_list, 2, 0, 0),
  [84] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_list, 2, 0, 0),
  [86] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_keyword, 2, 0, 0),
  [88] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_keyword, 2, 0, 0),
  [90] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_boolean, 1, 0, 0),
  [92] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_boolean, 1, 0, 0),
  [94] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_scope, 2, 0, 0),
  [96] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_scope, 2, 0, 0),
  [98] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_cons, 2, 0, 0),
  [100] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_cons, 2, 0, 0),
  [102] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_literal, 1, 0, 0),
  [104] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_literal, 1, 0, 0),
  [106] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_string, 2, 0, 0),
  [108] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_string, 2, 0, 0),
  [110] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_symbind, 2, 0, 0),
  [112] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_symbind, 2, 0, 0),
  [114] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_meta, 3, 0, 1),
  [116] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_meta, 3, 0, 1),
  [118] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_list, 3, 0, 0),
  [120] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_list, 3, 0, 0),
  [122] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_scope, 3, 0, 0),
  [124] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_scope, 3, 0, 0),
  [126] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_cons, 3, 0, 0),
  [128] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_cons, 3, 0, 0),
  [130] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_string, 3, 0, 0),
  [132] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_string, 3, 0, 0),
  [134] = {.entry = {.count = 1, .reusable = false}}, SHIFT(19),
  [136] = {.entry = {.count = 1, .reusable = true}}, SHIFT(28),
  [138] = {.entry = {.count = 1, .reusable = false}}, SHIFT(28),
  [140] = {.entry = {.count = 1, .reusable = false}}, SHIFT(26),
  [142] = {.entry = {.count = 1, .reusable = true}}, SHIFT(29),
  [144] = {.entry = {.count = 1, .reusable = false}}, SHIFT(29),
  [146] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0),
  [148] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0), SHIFT_REPEAT(29),
  [151] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0), SHIFT_REPEAT(29),
  [154] = {.entry = {.count = 1, .reusable = false}}, SHIFT(14),
  [156] = {.entry = {.count = 1, .reusable = true}}, SHIFT(14),
  [158] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [160] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
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

TS_PUBLIC const TSLanguage *tree_sitter_bass(void) {
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
    .keyword_lex_fn = ts_lex_keywords,
    .keyword_capture_token = sym_symbol,
    .primary_state_ids = ts_primary_state_ids,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif

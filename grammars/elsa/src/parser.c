#include <tree_sitter/parser.h>

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 50
#define LARGE_STATE_COUNT 6
#define SYMBOL_COUNT 27
#define ALIAS_COUNT 3
#define TOKEN_COUNT 16
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 4
#define MAX_ALIAS_SEQUENCE_LENGTH 5
#define PRODUCTION_ID_COUNT 6

enum {
  anon_sym_let = 1,
  anon_sym_EQ = 2,
  anon_sym_eval = 3,
  anon_sym_COLON = 4,
  anon_sym_BSLASH = 5,
  anon_sym_DASH_GT = 6,
  anon_sym_LPAREN = 7,
  anon_sym_RPAREN = 8,
  anon_sym_EQa_GT = 9,
  anon_sym_EQb_GT = 10,
  anon_sym_EQd_GT = 11,
  anon_sym_EQ_STAR_GT = 12,
  anon_sym_EQ_TILDE_GT = 13,
  sym_identifier = 14,
  sym_comment = 15,
  sym_source_file = 16,
  sym_definition = 17,
  sym_reduction = 18,
  sym_abstraction = 19,
  sym_application = 20,
  sym_term = 21,
  sym_step = 22,
  aux_sym_source_file_repeat1 = 23,
  aux_sym_source_file_repeat2 = 24,
  aux_sym_reduction_repeat1 = 25,
  aux_sym_abstraction_repeat1 = 26,
  alias_sym_function = 27,
  alias_sym_method = 28,
  alias_sym_parameter = 29,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [anon_sym_let] = "let",
  [anon_sym_EQ] = "=",
  [anon_sym_eval] = "eval",
  [anon_sym_COLON] = ":",
  [anon_sym_BSLASH] = "\\",
  [anon_sym_DASH_GT] = "->",
  [anon_sym_LPAREN] = "(",
  [anon_sym_RPAREN] = ")",
  [anon_sym_EQa_GT] = "=a>",
  [anon_sym_EQb_GT] = "=b>",
  [anon_sym_EQd_GT] = "=d>",
  [anon_sym_EQ_STAR_GT] = "=*>",
  [anon_sym_EQ_TILDE_GT] = "=~>",
  [sym_identifier] = "identifier",
  [sym_comment] = "comment",
  [sym_source_file] = "source_file",
  [sym_definition] = "definition",
  [sym_reduction] = "reduction",
  [sym_abstraction] = "abstraction",
  [sym_application] = "application",
  [sym_term] = "term",
  [sym_step] = "step",
  [aux_sym_source_file_repeat1] = "source_file_repeat1",
  [aux_sym_source_file_repeat2] = "source_file_repeat2",
  [aux_sym_reduction_repeat1] = "reduction_repeat1",
  [aux_sym_abstraction_repeat1] = "abstraction_repeat1",
  [alias_sym_function] = "function",
  [alias_sym_method] = "method",
  [alias_sym_parameter] = "parameter",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [anon_sym_let] = anon_sym_let,
  [anon_sym_EQ] = anon_sym_EQ,
  [anon_sym_eval] = anon_sym_eval,
  [anon_sym_COLON] = anon_sym_COLON,
  [anon_sym_BSLASH] = anon_sym_BSLASH,
  [anon_sym_DASH_GT] = anon_sym_DASH_GT,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [anon_sym_RPAREN] = anon_sym_RPAREN,
  [anon_sym_EQa_GT] = anon_sym_EQa_GT,
  [anon_sym_EQb_GT] = anon_sym_EQb_GT,
  [anon_sym_EQd_GT] = anon_sym_EQd_GT,
  [anon_sym_EQ_STAR_GT] = anon_sym_EQ_STAR_GT,
  [anon_sym_EQ_TILDE_GT] = anon_sym_EQ_TILDE_GT,
  [sym_identifier] = sym_identifier,
  [sym_comment] = sym_comment,
  [sym_source_file] = sym_source_file,
  [sym_definition] = sym_definition,
  [sym_reduction] = sym_reduction,
  [sym_abstraction] = sym_abstraction,
  [sym_application] = sym_application,
  [sym_term] = sym_term,
  [sym_step] = sym_step,
  [aux_sym_source_file_repeat1] = aux_sym_source_file_repeat1,
  [aux_sym_source_file_repeat2] = aux_sym_source_file_repeat2,
  [aux_sym_reduction_repeat1] = aux_sym_reduction_repeat1,
  [aux_sym_abstraction_repeat1] = aux_sym_abstraction_repeat1,
  [alias_sym_function] = alias_sym_function,
  [alias_sym_method] = alias_sym_method,
  [alias_sym_parameter] = alias_sym_parameter,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [anon_sym_let] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_eval] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BSLASH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DASH_GT] = {
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
  [anon_sym_EQa_GT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EQb_GT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EQd_GT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EQ_STAR_GT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EQ_TILDE_GT] = {
    .visible = true,
    .named = false,
  },
  [sym_identifier] = {
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
  [sym_definition] = {
    .visible = true,
    .named = true,
  },
  [sym_reduction] = {
    .visible = true,
    .named = true,
  },
  [sym_abstraction] = {
    .visible = true,
    .named = true,
  },
  [sym_application] = {
    .visible = true,
    .named = true,
  },
  [sym_term] = {
    .visible = true,
    .named = true,
  },
  [sym_step] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_source_file_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_source_file_repeat2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_reduction_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_abstraction_repeat1] = {
    .visible = false,
    .named = false,
  },
  [alias_sym_function] = {
    .visible = true,
    .named = true,
  },
  [alias_sym_method] = {
    .visible = true,
    .named = true,
  },
  [alias_sym_parameter] = {
    .visible = true,
    .named = true,
  },
};

enum {
  field_abstraction = 1,
  field_argument = 2,
  field_body = 3,
  field_parameters = 4,
};

static const char * const ts_field_names[] = {
  [0] = NULL,
  [field_abstraction] = "abstraction",
  [field_argument] = "argument",
  [field_body] = "body",
  [field_parameters] = "parameters",
};

static const TSFieldMapSlice ts_field_map_slices[PRODUCTION_ID_COUNT] = {
  [4] = {.index = 0, .length = 2},
  [5] = {.index = 2, .length = 2},
};

static const TSFieldMapEntry ts_field_map_entries[] = {
  [0] =
    {field_abstraction, 0},
    {field_argument, 1},
  [2] =
    {field_body, 3},
    {field_parameters, 1},
};

static const TSSymbol ts_alias_sequences[PRODUCTION_ID_COUNT][MAX_ALIAS_SEQUENCE_LENGTH] = {
  [0] = {0},
  [1] = {
    [1] = alias_sym_function,
  },
  [2] = {
    [1] = alias_sym_method,
  },
  [3] = {
    [0] = alias_sym_parameter,
  },
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
  [8] = 3,
  [9] = 9,
  [10] = 10,
  [11] = 4,
  [12] = 12,
  [13] = 13,
  [14] = 13,
  [15] = 13,
  [16] = 3,
  [17] = 17,
  [18] = 4,
  [19] = 19,
  [20] = 20,
  [21] = 21,
  [22] = 21,
  [23] = 7,
  [24] = 24,
  [25] = 25,
  [26] = 25,
  [27] = 6,
  [28] = 21,
  [29] = 25,
  [30] = 30,
  [31] = 31,
  [32] = 32,
  [33] = 33,
  [34] = 7,
  [35] = 6,
  [36] = 36,
  [37] = 37,
  [38] = 37,
  [39] = 39,
  [40] = 37,
  [41] = 41,
  [42] = 42,
  [43] = 41,
  [44] = 41,
  [45] = 45,
  [46] = 46,
  [47] = 47,
  [48] = 48,
  [49] = 49,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(18);
      if (lookahead == '(') ADVANCE(28);
      if (lookahead == ')') ADVANCE(29);
      if (lookahead == '-') ADVANCE(4);
      if (lookahead == ':') ADVANCE(25);
      if (lookahead == '=') ADVANCE(22);
      if (lookahead == '\\') ADVANCE(26);
      if (lookahead == 'e') ADVANCE(39);
      if (lookahead == 'l') ADVANCE(36);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(0)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 1:
      if (lookahead == '(') ADVANCE(28);
      if (lookahead == ')') ADVANCE(29);
      if (lookahead == '-') ADVANCE(4);
      if (lookahead == '=') ADVANCE(21);
      if (lookahead == '\\') ADVANCE(26);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(1)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 2:
      if (lookahead == '*') ADVANCE(5);
      if (lookahead == 'a') ADVANCE(7);
      if (lookahead == 'b') ADVANCE(8);
      if (lookahead == 'd') ADVANCE(9);
      if (lookahead == '~') ADVANCE(6);
      END_STATE();
    case 3:
      if (lookahead == '-') ADVANCE(42);
      END_STATE();
    case 4:
      if (lookahead == '-') ADVANCE(42);
      if (lookahead == '>') ADVANCE(27);
      END_STATE();
    case 5:
      if (lookahead == '>') ADVANCE(33);
      END_STATE();
    case 6:
      if (lookahead == '>') ADVANCE(34);
      END_STATE();
    case 7:
      if (lookahead == '>') ADVANCE(30);
      END_STATE();
    case 8:
      if (lookahead == '>') ADVANCE(31);
      END_STATE();
    case 9:
      if (lookahead == '>') ADVANCE(32);
      END_STATE();
    case 10:
      if (lookahead == 'a') ADVANCE(12);
      END_STATE();
    case 11:
      if (lookahead == 'e') ADVANCE(13);
      END_STATE();
    case 12:
      if (lookahead == 'l') ADVANCE(23);
      END_STATE();
    case 13:
      if (lookahead == 't') ADVANCE(19);
      END_STATE();
    case 14:
      if (lookahead == 'v') ADVANCE(10);
      END_STATE();
    case 15:
      if (lookahead != 0 &&
          lookahead != '\r') ADVANCE(42);
      if (lookahead == '\r') ADVANCE(43);
      END_STATE();
    case 16:
      if (eof) ADVANCE(18);
      if (lookahead == '(') ADVANCE(28);
      if (lookahead == '-') ADVANCE(3);
      if (lookahead == '=') ADVANCE(2);
      if (lookahead == '\\') ADVANCE(26);
      if (lookahead == 'e') ADVANCE(39);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(16)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 17:
      if (eof) ADVANCE(18);
      if (lookahead == '-') ADVANCE(3);
      if (lookahead == '=') ADVANCE(2);
      if (lookahead == 'e') ADVANCE(14);
      if (lookahead == 'l') ADVANCE(11);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(17)
      END_STATE();
    case 18:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 19:
      ACCEPT_TOKEN(anon_sym_let);
      END_STATE();
    case 20:
      ACCEPT_TOKEN(anon_sym_let);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(41);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 21:
      ACCEPT_TOKEN(anon_sym_EQ);
      END_STATE();
    case 22:
      ACCEPT_TOKEN(anon_sym_EQ);
      if (lookahead == '*') ADVANCE(5);
      if (lookahead == '~') ADVANCE(6);
      END_STATE();
    case 23:
      ACCEPT_TOKEN(anon_sym_eval);
      END_STATE();
    case 24:
      ACCEPT_TOKEN(anon_sym_eval);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(41);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 25:
      ACCEPT_TOKEN(anon_sym_COLON);
      END_STATE();
    case 26:
      ACCEPT_TOKEN(anon_sym_BSLASH);
      END_STATE();
    case 27:
      ACCEPT_TOKEN(anon_sym_DASH_GT);
      END_STATE();
    case 28:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 29:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 30:
      ACCEPT_TOKEN(anon_sym_EQa_GT);
      END_STATE();
    case 31:
      ACCEPT_TOKEN(anon_sym_EQb_GT);
      END_STATE();
    case 32:
      ACCEPT_TOKEN(anon_sym_EQd_GT);
      END_STATE();
    case 33:
      ACCEPT_TOKEN(anon_sym_EQ_STAR_GT);
      END_STATE();
    case 34:
      ACCEPT_TOKEN(anon_sym_EQ_TILDE_GT);
      END_STATE();
    case 35:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(37);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(41);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 36:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(38);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(41);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 37:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(24);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(41);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 38:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(20);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(41);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 39:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'v') ADVANCE(35);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(41);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 40:
      ACCEPT_TOKEN(sym_identifier);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(41);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(40);
      END_STATE();
    case 41:
      ACCEPT_TOKEN(sym_identifier);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(41);
      END_STATE();
    case 42:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead == '\\') ADVANCE(15);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(42);
      END_STATE();
    case 43:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead != 0 &&
          lookahead != '\\') ADVANCE(42);
      if (lookahead == '\\') ADVANCE(15);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 17},
  [2] = {.lex_state = 16},
  [3] = {.lex_state = 16},
  [4] = {.lex_state = 16},
  [5] = {.lex_state = 16},
  [6] = {.lex_state = 16},
  [7] = {.lex_state = 16},
  [8] = {.lex_state = 0},
  [9] = {.lex_state = 17},
  [10] = {.lex_state = 17},
  [11] = {.lex_state = 0},
  [12] = {.lex_state = 0},
  [13] = {.lex_state = 1},
  [14] = {.lex_state = 1},
  [15] = {.lex_state = 1},
  [16] = {.lex_state = 1},
  [17] = {.lex_state = 17},
  [18] = {.lex_state = 1},
  [19] = {.lex_state = 1},
  [20] = {.lex_state = 1},
  [21] = {.lex_state = 1},
  [22] = {.lex_state = 1},
  [23] = {.lex_state = 0},
  [24] = {.lex_state = 1},
  [25] = {.lex_state = 1},
  [26] = {.lex_state = 1},
  [27] = {.lex_state = 0},
  [28] = {.lex_state = 1},
  [29] = {.lex_state = 1},
  [30] = {.lex_state = 17},
  [31] = {.lex_state = 17},
  [32] = {.lex_state = 17},
  [33] = {.lex_state = 17},
  [34] = {.lex_state = 1},
  [35] = {.lex_state = 1},
  [36] = {.lex_state = 1},
  [37] = {.lex_state = 1},
  [38] = {.lex_state = 1},
  [39] = {.lex_state = 1},
  [40] = {.lex_state = 1},
  [41] = {.lex_state = 1},
  [42] = {.lex_state = 1},
  [43] = {.lex_state = 1},
  [44] = {.lex_state = 1},
  [45] = {.lex_state = 0},
  [46] = {.lex_state = 1},
  [47] = {.lex_state = 1},
  [48] = {.lex_state = 0},
  [49] = {.lex_state = 1},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [anon_sym_let] = ACTIONS(1),
    [anon_sym_EQ] = ACTIONS(1),
    [anon_sym_eval] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [anon_sym_BSLASH] = ACTIONS(1),
    [anon_sym_DASH_GT] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [anon_sym_RPAREN] = ACTIONS(1),
    [anon_sym_EQ_STAR_GT] = ACTIONS(1),
    [anon_sym_EQ_TILDE_GT] = ACTIONS(1),
    [sym_identifier] = ACTIONS(1),
    [sym_comment] = ACTIONS(3),
  },
  [1] = {
    [sym_source_file] = STATE(48),
    [sym_definition] = STATE(17),
    [sym_reduction] = STATE(31),
    [aux_sym_source_file_repeat1] = STATE(17),
    [aux_sym_source_file_repeat2] = STATE(31),
    [ts_builtin_sym_end] = ACTIONS(5),
    [anon_sym_let] = ACTIONS(7),
    [anon_sym_eval] = ACTIONS(9),
    [sym_comment] = ACTIONS(3),
  },
  [2] = {
    [sym_abstraction] = STATE(7),
    [sym_application] = STATE(7),
    [sym_term] = STATE(4),
    [sym_step] = STATE(24),
    [aux_sym_reduction_repeat1] = STATE(10),
    [ts_builtin_sym_end] = ACTIONS(11),
    [anon_sym_eval] = ACTIONS(13),
    [anon_sym_BSLASH] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_EQa_GT] = ACTIONS(19),
    [anon_sym_EQb_GT] = ACTIONS(19),
    [anon_sym_EQd_GT] = ACTIONS(19),
    [anon_sym_EQ_STAR_GT] = ACTIONS(19),
    [anon_sym_EQ_TILDE_GT] = ACTIONS(19),
    [sym_identifier] = ACTIONS(21),
    [sym_comment] = ACTIONS(3),
  },
  [3] = {
    [sym_abstraction] = STATE(7),
    [sym_application] = STATE(7),
    [sym_term] = STATE(4),
    [ts_builtin_sym_end] = ACTIONS(23),
    [anon_sym_eval] = ACTIONS(25),
    [anon_sym_BSLASH] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_EQa_GT] = ACTIONS(23),
    [anon_sym_EQb_GT] = ACTIONS(23),
    [anon_sym_EQd_GT] = ACTIONS(23),
    [anon_sym_EQ_STAR_GT] = ACTIONS(23),
    [anon_sym_EQ_TILDE_GT] = ACTIONS(23),
    [sym_identifier] = ACTIONS(21),
    [sym_comment] = ACTIONS(3),
  },
  [4] = {
    [sym_abstraction] = STATE(7),
    [sym_application] = STATE(7),
    [sym_term] = STATE(4),
    [ts_builtin_sym_end] = ACTIONS(27),
    [anon_sym_eval] = ACTIONS(29),
    [anon_sym_BSLASH] = ACTIONS(27),
    [anon_sym_LPAREN] = ACTIONS(27),
    [anon_sym_EQa_GT] = ACTIONS(27),
    [anon_sym_EQb_GT] = ACTIONS(27),
    [anon_sym_EQd_GT] = ACTIONS(27),
    [anon_sym_EQ_STAR_GT] = ACTIONS(27),
    [anon_sym_EQ_TILDE_GT] = ACTIONS(27),
    [sym_identifier] = ACTIONS(29),
    [sym_comment] = ACTIONS(3),
  },
  [5] = {
    [sym_abstraction] = STATE(7),
    [sym_application] = STATE(7),
    [sym_term] = STATE(4),
    [ts_builtin_sym_end] = ACTIONS(31),
    [anon_sym_eval] = ACTIONS(33),
    [anon_sym_BSLASH] = ACTIONS(15),
    [anon_sym_LPAREN] = ACTIONS(17),
    [anon_sym_EQa_GT] = ACTIONS(31),
    [anon_sym_EQb_GT] = ACTIONS(31),
    [anon_sym_EQd_GT] = ACTIONS(31),
    [anon_sym_EQ_STAR_GT] = ACTIONS(31),
    [anon_sym_EQ_TILDE_GT] = ACTIONS(31),
    [sym_identifier] = ACTIONS(21),
    [sym_comment] = ACTIONS(3),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(37), 2,
      anon_sym_eval,
      sym_identifier,
    ACTIONS(35), 8,
      ts_builtin_sym_end,
      anon_sym_BSLASH,
      anon_sym_LPAREN,
      anon_sym_EQa_GT,
      anon_sym_EQb_GT,
      anon_sym_EQd_GT,
      anon_sym_EQ_STAR_GT,
      anon_sym_EQ_TILDE_GT,
  [18] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(41), 2,
      anon_sym_eval,
      sym_identifier,
    ACTIONS(39), 8,
      ts_builtin_sym_end,
      anon_sym_BSLASH,
      anon_sym_LPAREN,
      anon_sym_EQa_GT,
      anon_sym_EQb_GT,
      anon_sym_EQd_GT,
      anon_sym_EQ_STAR_GT,
      anon_sym_EQ_TILDE_GT,
  [36] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(23), 1,
      ts_builtin_sym_end,
    ACTIONS(43), 1,
      anon_sym_BSLASH,
    ACTIONS(45), 1,
      anon_sym_LPAREN,
    ACTIONS(47), 1,
      sym_identifier,
    STATE(11), 1,
      sym_term,
    ACTIONS(25), 2,
      anon_sym_let,
      anon_sym_eval,
    STATE(23), 2,
      sym_abstraction,
      sym_application,
  [63] = 5,
    ACTIONS(3), 1,
      sym_comment,
    STATE(9), 1,
      aux_sym_reduction_repeat1,
    STATE(24), 1,
      sym_step,
    ACTIONS(31), 2,
      ts_builtin_sym_end,
      anon_sym_eval,
    ACTIONS(49), 5,
      anon_sym_EQa_GT,
      anon_sym_EQb_GT,
      anon_sym_EQd_GT,
      anon_sym_EQ_STAR_GT,
      anon_sym_EQ_TILDE_GT,
  [84] = 5,
    ACTIONS(3), 1,
      sym_comment,
    STATE(9), 1,
      aux_sym_reduction_repeat1,
    STATE(24), 1,
      sym_step,
    ACTIONS(52), 2,
      ts_builtin_sym_end,
      anon_sym_eval,
    ACTIONS(19), 5,
      anon_sym_EQa_GT,
      anon_sym_EQb_GT,
      anon_sym_EQd_GT,
      anon_sym_EQ_STAR_GT,
      anon_sym_EQ_TILDE_GT,
  [105] = 5,
    ACTIONS(3), 1,
      sym_comment,
    STATE(11), 1,
      sym_term,
    STATE(23), 2,
      sym_abstraction,
      sym_application,
    ACTIONS(27), 3,
      ts_builtin_sym_end,
      anon_sym_BSLASH,
      anon_sym_LPAREN,
    ACTIONS(29), 3,
      anon_sym_let,
      anon_sym_eval,
      sym_identifier,
  [126] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(43), 1,
      anon_sym_BSLASH,
    ACTIONS(45), 1,
      anon_sym_LPAREN,
    ACTIONS(47), 1,
      sym_identifier,
    ACTIONS(54), 1,
      ts_builtin_sym_end,
    STATE(11), 1,
      sym_term,
    ACTIONS(56), 2,
      anon_sym_let,
      anon_sym_eval,
    STATE(23), 2,
      sym_abstraction,
      sym_application,
  [153] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(58), 1,
      anon_sym_BSLASH,
    ACTIONS(60), 1,
      anon_sym_LPAREN,
    ACTIONS(62), 1,
      anon_sym_RPAREN,
    ACTIONS(64), 1,
      sym_identifier,
    STATE(18), 1,
      sym_term,
    STATE(34), 2,
      sym_abstraction,
      sym_application,
  [176] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(58), 1,
      anon_sym_BSLASH,
    ACTIONS(60), 1,
      anon_sym_LPAREN,
    ACTIONS(64), 1,
      sym_identifier,
    ACTIONS(66), 1,
      anon_sym_RPAREN,
    STATE(18), 1,
      sym_term,
    STATE(34), 2,
      sym_abstraction,
      sym_application,
  [199] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(58), 1,
      anon_sym_BSLASH,
    ACTIONS(60), 1,
      anon_sym_LPAREN,
    ACTIONS(64), 1,
      sym_identifier,
    ACTIONS(68), 1,
      anon_sym_RPAREN,
    STATE(18), 1,
      sym_term,
    STATE(34), 2,
      sym_abstraction,
      sym_application,
  [222] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(23), 1,
      anon_sym_RPAREN,
    ACTIONS(58), 1,
      anon_sym_BSLASH,
    ACTIONS(60), 1,
      anon_sym_LPAREN,
    ACTIONS(64), 1,
      sym_identifier,
    STATE(18), 1,
      sym_term,
    STATE(34), 2,
      sym_abstraction,
      sym_application,
  [245] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(7), 1,
      anon_sym_let,
    ACTIONS(9), 1,
      anon_sym_eval,
    ACTIONS(70), 1,
      ts_builtin_sym_end,
    STATE(30), 2,
      sym_definition,
      aux_sym_source_file_repeat1,
    STATE(33), 2,
      sym_reduction,
      aux_sym_source_file_repeat2,
  [266] = 4,
    ACTIONS(3), 1,
      sym_comment,
    STATE(18), 1,
      sym_term,
    STATE(34), 2,
      sym_abstraction,
      sym_application,
    ACTIONS(27), 4,
      anon_sym_BSLASH,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      sym_identifier,
  [283] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_BSLASH,
    ACTIONS(17), 1,
      anon_sym_LPAREN,
    ACTIONS(72), 1,
      sym_identifier,
    STATE(2), 1,
      sym_term,
    STATE(7), 2,
      sym_abstraction,
      sym_application,
  [303] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(43), 1,
      anon_sym_BSLASH,
    ACTIONS(45), 1,
      anon_sym_LPAREN,
    ACTIONS(74), 1,
      sym_identifier,
    STATE(12), 1,
      sym_term,
    STATE(23), 2,
      sym_abstraction,
      sym_application,
  [323] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(58), 1,
      anon_sym_BSLASH,
    ACTIONS(60), 1,
      anon_sym_LPAREN,
    ACTIONS(64), 1,
      sym_identifier,
    STATE(14), 1,
      sym_term,
    STATE(34), 2,
      sym_abstraction,
      sym_application,
  [343] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(58), 1,
      anon_sym_BSLASH,
    ACTIONS(60), 1,
      anon_sym_LPAREN,
    ACTIONS(64), 1,
      sym_identifier,
    STATE(15), 1,
      sym_term,
    STATE(34), 2,
      sym_abstraction,
      sym_application,
  [363] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(39), 3,
      ts_builtin_sym_end,
      anon_sym_BSLASH,
      anon_sym_LPAREN,
    ACTIONS(41), 3,
      anon_sym_let,
      anon_sym_eval,
      sym_identifier,
  [377] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_BSLASH,
    ACTIONS(17), 1,
      anon_sym_LPAREN,
    ACTIONS(72), 1,
      sym_identifier,
    STATE(5), 1,
      sym_term,
    STATE(7), 2,
      sym_abstraction,
      sym_application,
  [397] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(58), 1,
      anon_sym_BSLASH,
    ACTIONS(60), 1,
      anon_sym_LPAREN,
    ACTIONS(64), 1,
      sym_identifier,
    STATE(16), 1,
      sym_term,
    STATE(34), 2,
      sym_abstraction,
      sym_application,
  [417] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(43), 1,
      anon_sym_BSLASH,
    ACTIONS(45), 1,
      anon_sym_LPAREN,
    ACTIONS(74), 1,
      sym_identifier,
    STATE(8), 1,
      sym_term,
    STATE(23), 2,
      sym_abstraction,
      sym_application,
  [437] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(35), 3,
      ts_builtin_sym_end,
      anon_sym_BSLASH,
      anon_sym_LPAREN,
    ACTIONS(37), 3,
      anon_sym_let,
      anon_sym_eval,
      sym_identifier,
  [451] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(58), 1,
      anon_sym_BSLASH,
    ACTIONS(60), 1,
      anon_sym_LPAREN,
    ACTIONS(64), 1,
      sym_identifier,
    STATE(13), 1,
      sym_term,
    STATE(34), 2,
      sym_abstraction,
      sym_application,
  [471] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_BSLASH,
    ACTIONS(17), 1,
      anon_sym_LPAREN,
    ACTIONS(72), 1,
      sym_identifier,
    STATE(3), 1,
      sym_term,
    STATE(7), 2,
      sym_abstraction,
      sym_application,
  [491] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(78), 1,
      anon_sym_let,
    ACTIONS(76), 2,
      ts_builtin_sym_end,
      anon_sym_eval,
    STATE(30), 2,
      sym_definition,
      aux_sym_source_file_repeat1,
  [506] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(9), 1,
      anon_sym_eval,
    ACTIONS(70), 1,
      ts_builtin_sym_end,
    STATE(32), 2,
      sym_reduction,
      aux_sym_source_file_repeat2,
  [520] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(81), 1,
      ts_builtin_sym_end,
    ACTIONS(83), 1,
      anon_sym_eval,
    STATE(32), 2,
      sym_reduction,
      aux_sym_source_file_repeat2,
  [534] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(9), 1,
      anon_sym_eval,
    ACTIONS(86), 1,
      ts_builtin_sym_end,
    STATE(32), 2,
      sym_reduction,
      aux_sym_source_file_repeat2,
  [548] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(39), 4,
      anon_sym_BSLASH,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      sym_identifier,
  [558] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(35), 4,
      anon_sym_BSLASH,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      sym_identifier,
  [568] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(88), 1,
      anon_sym_DASH_GT,
    ACTIONS(90), 1,
      sym_identifier,
    STATE(36), 1,
      aux_sym_abstraction_repeat1,
  [581] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(93), 1,
      anon_sym_DASH_GT,
    ACTIONS(95), 1,
      sym_identifier,
    STATE(36), 1,
      aux_sym_abstraction_repeat1,
  [594] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(95), 1,
      sym_identifier,
    ACTIONS(97), 1,
      anon_sym_DASH_GT,
    STATE(36), 1,
      aux_sym_abstraction_repeat1,
  [607] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(99), 3,
      anon_sym_BSLASH,
      anon_sym_LPAREN,
      sym_identifier,
  [616] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(95), 1,
      sym_identifier,
    ACTIONS(101), 1,
      anon_sym_DASH_GT,
    STATE(36), 1,
      aux_sym_abstraction_repeat1,
  [629] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(95), 1,
      sym_identifier,
    STATE(40), 1,
      aux_sym_abstraction_repeat1,
  [639] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(103), 2,
      anon_sym_DASH_GT,
      sym_identifier,
  [647] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(95), 1,
      sym_identifier,
    STATE(37), 1,
      aux_sym_abstraction_repeat1,
  [657] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(95), 1,
      sym_identifier,
    STATE(38), 1,
      aux_sym_abstraction_repeat1,
  [667] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(105), 1,
      anon_sym_COLON,
  [674] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(107), 1,
      anon_sym_EQ,
  [681] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(109), 1,
      sym_identifier,
  [688] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(111), 1,
      ts_builtin_sym_end,
  [695] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(113), 1,
      sym_identifier,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(6)] = 0,
  [SMALL_STATE(7)] = 18,
  [SMALL_STATE(8)] = 36,
  [SMALL_STATE(9)] = 63,
  [SMALL_STATE(10)] = 84,
  [SMALL_STATE(11)] = 105,
  [SMALL_STATE(12)] = 126,
  [SMALL_STATE(13)] = 153,
  [SMALL_STATE(14)] = 176,
  [SMALL_STATE(15)] = 199,
  [SMALL_STATE(16)] = 222,
  [SMALL_STATE(17)] = 245,
  [SMALL_STATE(18)] = 266,
  [SMALL_STATE(19)] = 283,
  [SMALL_STATE(20)] = 303,
  [SMALL_STATE(21)] = 323,
  [SMALL_STATE(22)] = 343,
  [SMALL_STATE(23)] = 363,
  [SMALL_STATE(24)] = 377,
  [SMALL_STATE(25)] = 397,
  [SMALL_STATE(26)] = 417,
  [SMALL_STATE(27)] = 437,
  [SMALL_STATE(28)] = 451,
  [SMALL_STATE(29)] = 471,
  [SMALL_STATE(30)] = 491,
  [SMALL_STATE(31)] = 506,
  [SMALL_STATE(32)] = 520,
  [SMALL_STATE(33)] = 534,
  [SMALL_STATE(34)] = 548,
  [SMALL_STATE(35)] = 558,
  [SMALL_STATE(36)] = 568,
  [SMALL_STATE(37)] = 581,
  [SMALL_STATE(38)] = 594,
  [SMALL_STATE(39)] = 607,
  [SMALL_STATE(40)] = 616,
  [SMALL_STATE(41)] = 629,
  [SMALL_STATE(42)] = 639,
  [SMALL_STATE(43)] = 647,
  [SMALL_STATE(44)] = 657,
  [SMALL_STATE(45)] = 667,
  [SMALL_STATE(46)] = 674,
  [SMALL_STATE(47)] = 681,
  [SMALL_STATE(48)] = 688,
  [SMALL_STATE(49)] = 695,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 0),
  [7] = {.entry = {.count = 1, .reusable = true}}, SHIFT(47),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(49),
  [11] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_reduction, 4, .production_id = 2),
  [13] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_reduction, 4, .production_id = 2),
  [15] = {.entry = {.count = 1, .reusable = true}}, SHIFT(43),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(22),
  [19] = {.entry = {.count = 1, .reusable = true}}, SHIFT(39),
  [21] = {.entry = {.count = 1, .reusable = false}}, SHIFT(7),
  [23] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_abstraction, 4, .production_id = 5),
  [25] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_abstraction, 4, .production_id = 5),
  [27] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_application, 2, .production_id = 4),
  [29] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_application, 2, .production_id = 4),
  [31] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_reduction_repeat1, 2),
  [33] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_reduction_repeat1, 2),
  [35] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_term, 3),
  [37] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_term, 3),
  [39] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_term, 1),
  [41] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_term, 1),
  [43] = {.entry = {.count = 1, .reusable = true}}, SHIFT(41),
  [45] = {.entry = {.count = 1, .reusable = true}}, SHIFT(28),
  [47] = {.entry = {.count = 1, .reusable = false}}, SHIFT(23),
  [49] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_reduction_repeat1, 2), SHIFT_REPEAT(39),
  [52] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_reduction, 5, .production_id = 2),
  [54] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_definition, 4, .production_id = 1),
  [56] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_definition, 4, .production_id = 1),
  [58] = {.entry = {.count = 1, .reusable = true}}, SHIFT(44),
  [60] = {.entry = {.count = 1, .reusable = true}}, SHIFT(21),
  [62] = {.entry = {.count = 1, .reusable = true}}, SHIFT(27),
  [64] = {.entry = {.count = 1, .reusable = true}}, SHIFT(34),
  [66] = {.entry = {.count = 1, .reusable = true}}, SHIFT(35),
  [68] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [70] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 1),
  [72] = {.entry = {.count = 1, .reusable = true}}, SHIFT(7),
  [74] = {.entry = {.count = 1, .reusable = true}}, SHIFT(23),
  [76] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2),
  [78] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(47),
  [81] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_source_file_repeat2, 2),
  [83] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat2, 2), SHIFT_REPEAT(49),
  [86] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 2),
  [88] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_abstraction_repeat1, 2),
  [90] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_abstraction_repeat1, 2), SHIFT_REPEAT(42),
  [93] = {.entry = {.count = 1, .reusable = true}}, SHIFT(29),
  [95] = {.entry = {.count = 1, .reusable = true}}, SHIFT(42),
  [97] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
  [99] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_step, 1),
  [101] = {.entry = {.count = 1, .reusable = true}}, SHIFT(26),
  [103] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_abstraction_repeat1, 1, .production_id = 3),
  [105] = {.entry = {.count = 1, .reusable = true}}, SHIFT(19),
  [107] = {.entry = {.count = 1, .reusable = true}}, SHIFT(20),
  [109] = {.entry = {.count = 1, .reusable = true}}, SHIFT(46),
  [111] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [113] = {.entry = {.count = 1, .reusable = true}}, SHIFT(45),
};

#ifdef __cplusplus
extern "C" {
#endif
#ifdef _WIN32
#define extern __declspec(dllexport)
#endif

extern const TSLanguage *tree_sitter_elsa(void) {
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

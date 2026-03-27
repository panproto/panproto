#include <tree_sitter/parser.h>

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 33
#define LARGE_STATE_COUNT 5
#define SYMBOL_COUNT 26
#define ALIAS_COUNT 2
#define TOKEN_COUNT 12
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 0
#define MAX_ALIAS_SEQUENCE_LENGTH 3
#define PRODUCTION_ID_COUNT 3

enum {
  sym_identifier = 1,
  anon_sym_EQ = 2,
  anon_sym_LPAREN = 3,
  anon_sym_RPAREN = 4,
  anon_sym_STAR = 5,
  anon_sym_QMARK = 6,
  anon_sym_SQUOTE = 7,
  anon_sym_COLON = 8,
  anon_sym_PIPE = 9,
  sym_token_identifier = 10,
  sym_comment = 11,
  sym_grammar = 12,
  sym_node = 13,
  sym_rule = 14,
  sym__atom_rule = 15,
  sym_repetition = 16,
  sym_optional = 17,
  sym_node_rule = 18,
  sym_token = 19,
  sym_label = 20,
  sym_sequence = 21,
  sym_alternation = 22,
  aux_sym_grammar_repeat1 = 23,
  aux_sym_sequence_repeat1 = 24,
  aux_sym_alternation_repeat1 = 25,
  alias_sym_definition = 26,
  alias_sym_label_name = 27,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [sym_identifier] = "identifier",
  [anon_sym_EQ] = "=",
  [anon_sym_LPAREN] = "(",
  [anon_sym_RPAREN] = ")",
  [anon_sym_STAR] = "*",
  [anon_sym_QMARK] = "\?",
  [anon_sym_SQUOTE] = "'",
  [anon_sym_COLON] = ":",
  [anon_sym_PIPE] = "|",
  [sym_token_identifier] = "token_identifier",
  [sym_comment] = "comment",
  [sym_grammar] = "grammar",
  [sym_node] = "node",
  [sym_rule] = "rule",
  [sym__atom_rule] = "_atom_rule",
  [sym_repetition] = "repetition",
  [sym_optional] = "optional",
  [sym_node_rule] = "node_rule",
  [sym_token] = "token",
  [sym_label] = "label",
  [sym_sequence] = "sequence",
  [sym_alternation] = "alternation",
  [aux_sym_grammar_repeat1] = "grammar_repeat1",
  [aux_sym_sequence_repeat1] = "sequence_repeat1",
  [aux_sym_alternation_repeat1] = "alternation_repeat1",
  [alias_sym_definition] = "definition",
  [alias_sym_label_name] = "label_name",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [sym_identifier] = sym_identifier,
  [anon_sym_EQ] = anon_sym_EQ,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [anon_sym_RPAREN] = anon_sym_RPAREN,
  [anon_sym_STAR] = anon_sym_STAR,
  [anon_sym_QMARK] = anon_sym_QMARK,
  [anon_sym_SQUOTE] = anon_sym_SQUOTE,
  [anon_sym_COLON] = anon_sym_COLON,
  [anon_sym_PIPE] = anon_sym_PIPE,
  [sym_token_identifier] = sym_token_identifier,
  [sym_comment] = sym_comment,
  [sym_grammar] = sym_grammar,
  [sym_node] = sym_node,
  [sym_rule] = sym_rule,
  [sym__atom_rule] = sym__atom_rule,
  [sym_repetition] = sym_repetition,
  [sym_optional] = sym_optional,
  [sym_node_rule] = sym_node_rule,
  [sym_token] = sym_token,
  [sym_label] = sym_label,
  [sym_sequence] = sym_sequence,
  [sym_alternation] = sym_alternation,
  [aux_sym_grammar_repeat1] = aux_sym_grammar_repeat1,
  [aux_sym_sequence_repeat1] = aux_sym_sequence_repeat1,
  [aux_sym_alternation_repeat1] = aux_sym_alternation_repeat1,
  [alias_sym_definition] = alias_sym_definition,
  [alias_sym_label_name] = alias_sym_label_name,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [sym_identifier] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_EQ] = {
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
  [anon_sym_STAR] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SQUOTE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PIPE] = {
    .visible = true,
    .named = false,
  },
  [sym_token_identifier] = {
    .visible = true,
    .named = true,
  },
  [sym_comment] = {
    .visible = true,
    .named = true,
  },
  [sym_grammar] = {
    .visible = true,
    .named = true,
  },
  [sym_node] = {
    .visible = true,
    .named = true,
  },
  [sym_rule] = {
    .visible = true,
    .named = true,
  },
  [sym__atom_rule] = {
    .visible = false,
    .named = true,
  },
  [sym_repetition] = {
    .visible = true,
    .named = true,
  },
  [sym_optional] = {
    .visible = true,
    .named = true,
  },
  [sym_node_rule] = {
    .visible = true,
    .named = true,
  },
  [sym_token] = {
    .visible = true,
    .named = true,
  },
  [sym_label] = {
    .visible = true,
    .named = true,
  },
  [sym_sequence] = {
    .visible = true,
    .named = true,
  },
  [sym_alternation] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_grammar_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_sequence_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_alternation_repeat1] = {
    .visible = false,
    .named = false,
  },
  [alias_sym_definition] = {
    .visible = true,
    .named = true,
  },
  [alias_sym_label_name] = {
    .visible = true,
    .named = true,
  },
};

static const TSSymbol ts_alias_sequences[PRODUCTION_ID_COUNT][MAX_ALIAS_SEQUENCE_LENGTH] = {
  [0] = {0},
  [1] = {
    [0] = alias_sym_definition,
  },
  [2] = {
    [0] = alias_sym_label_name,
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
  [6] = 5,
  [7] = 7,
  [8] = 7,
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
  [24] = 20,
  [25] = 22,
  [26] = 17,
  [27] = 27,
  [28] = 28,
  [29] = 29,
  [30] = 30,
  [31] = 31,
  [32] = 32,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(4);
      if (lookahead == '\'') ADVANCE(10);
      if (lookahead == '(') ADVANCE(6);
      if (lookahead == ')') ADVANCE(7);
      if (lookahead == '*') ADVANCE(8);
      if (lookahead == '/') ADVANCE(1);
      if (lookahead == ':') ADVANCE(11);
      if (lookahead == '=') ADVANCE(5);
      if (lookahead == '?') ADVANCE(9);
      if (lookahead == '|') ADVANCE(12);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(0)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(13);
      END_STATE();
    case 1:
      if (lookahead == '/') ADVANCE(19);
      END_STATE();
    case 2:
      if (lookahead == '/') ADVANCE(16);
      if (lookahead == '\\') ADVANCE(3);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(15);
      if (lookahead != 0 &&
          lookahead != '\'') ADVANCE(17);
      END_STATE();
    case 3:
      if (lookahead == '\'' ||
          lookahead == '\\') ADVANCE(17);
      END_STATE();
    case 4:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 5:
      ACCEPT_TOKEN(anon_sym_EQ);
      END_STATE();
    case 6:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 7:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 8:
      ACCEPT_TOKEN(anon_sym_STAR);
      END_STATE();
    case 9:
      ACCEPT_TOKEN(anon_sym_QMARK);
      END_STATE();
    case 10:
      ACCEPT_TOKEN(anon_sym_SQUOTE);
      END_STATE();
    case 11:
      ACCEPT_TOKEN(anon_sym_COLON);
      END_STATE();
    case 12:
      ACCEPT_TOKEN(anon_sym_PIPE);
      END_STATE();
    case 13:
      ACCEPT_TOKEN(sym_identifier);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(13);
      END_STATE();
    case 14:
      ACCEPT_TOKEN(sym_token_identifier);
      if (lookahead == '\n') ADVANCE(17);
      if (lookahead == '\'') ADVANCE(19);
      if (lookahead == '\\') ADVANCE(18);
      if (lookahead != 0) ADVANCE(14);
      END_STATE();
    case 15:
      ACCEPT_TOKEN(sym_token_identifier);
      if (lookahead == '/') ADVANCE(16);
      if (lookahead == '\\') ADVANCE(3);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(15);
      if (lookahead != 0 &&
          lookahead != '\'') ADVANCE(17);
      END_STATE();
    case 16:
      ACCEPT_TOKEN(sym_token_identifier);
      if (lookahead == '/') ADVANCE(14);
      if (lookahead == '\\') ADVANCE(3);
      if (lookahead != 0 &&
          lookahead != '\'') ADVANCE(17);
      END_STATE();
    case 17:
      ACCEPT_TOKEN(sym_token_identifier);
      if (lookahead == '\\') ADVANCE(3);
      if (lookahead != 0 &&
          lookahead != '\'') ADVANCE(17);
      END_STATE();
    case 18:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead == '\'' ||
          lookahead == '\\') ADVANCE(14);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(19);
      END_STATE();
    case 19:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(19);
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
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 0},
  [2] = {.lex_state = 0},
  [3] = {.lex_state = 0},
  [4] = {.lex_state = 0},
  [5] = {.lex_state = 0},
  [6] = {.lex_state = 0},
  [7] = {.lex_state = 0},
  [8] = {.lex_state = 0},
  [9] = {.lex_state = 0},
  [10] = {.lex_state = 0},
  [11] = {.lex_state = 0},
  [12] = {.lex_state = 0},
  [13] = {.lex_state = 0},
  [14] = {.lex_state = 0},
  [15] = {.lex_state = 0},
  [16] = {.lex_state = 0},
  [17] = {.lex_state = 0},
  [18] = {.lex_state = 0},
  [19] = {.lex_state = 0},
  [20] = {.lex_state = 0},
  [21] = {.lex_state = 0},
  [22] = {.lex_state = 0},
  [23] = {.lex_state = 0},
  [24] = {.lex_state = 0},
  [25] = {.lex_state = 0},
  [26] = {.lex_state = 0},
  [27] = {.lex_state = 0},
  [28] = {.lex_state = 2},
  [29] = {.lex_state = 0},
  [30] = {.lex_state = 0},
  [31] = {.lex_state = 0},
  [32] = {.lex_state = 0},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [sym_identifier] = ACTIONS(1),
    [anon_sym_EQ] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [anon_sym_RPAREN] = ACTIONS(1),
    [anon_sym_STAR] = ACTIONS(1),
    [anon_sym_QMARK] = ACTIONS(1),
    [anon_sym_SQUOTE] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [anon_sym_PIPE] = ACTIONS(1),
    [sym_comment] = ACTIONS(3),
  },
  [1] = {
    [sym_grammar] = STATE(32),
    [sym_node] = STATE(18),
    [aux_sym_grammar_repeat1] = STATE(18),
    [ts_builtin_sym_end] = ACTIONS(5),
    [sym_identifier] = ACTIONS(7),
    [sym_comment] = ACTIONS(3),
  },
  [2] = {
    [sym_rule] = STATE(27),
    [sym__atom_rule] = STATE(11),
    [sym_repetition] = STATE(11),
    [sym_optional] = STATE(11),
    [sym_node_rule] = STATE(11),
    [sym_token] = STATE(11),
    [sym_label] = STATE(11),
    [sym_sequence] = STATE(17),
    [sym_alternation] = STATE(23),
    [aux_sym_sequence_repeat1] = STATE(5),
    [sym_identifier] = ACTIONS(9),
    [anon_sym_LPAREN] = ACTIONS(11),
    [anon_sym_SQUOTE] = ACTIONS(13),
    [sym_comment] = ACTIONS(3),
  },
  [3] = {
    [sym__atom_rule] = STATE(11),
    [sym_repetition] = STATE(11),
    [sym_optional] = STATE(11),
    [sym_node_rule] = STATE(11),
    [sym_token] = STATE(11),
    [sym_label] = STATE(11),
    [aux_sym_sequence_repeat1] = STATE(3),
    [ts_builtin_sym_end] = ACTIONS(15),
    [sym_identifier] = ACTIONS(17),
    [anon_sym_LPAREN] = ACTIONS(20),
    [anon_sym_RPAREN] = ACTIONS(15),
    [anon_sym_SQUOTE] = ACTIONS(23),
    [anon_sym_PIPE] = ACTIONS(15),
    [sym_comment] = ACTIONS(3),
  },
  [4] = {
    [sym_rule] = STATE(31),
    [sym__atom_rule] = STATE(11),
    [sym_repetition] = STATE(11),
    [sym_optional] = STATE(11),
    [sym_node_rule] = STATE(11),
    [sym_token] = STATE(11),
    [sym_label] = STATE(11),
    [sym_sequence] = STATE(26),
    [sym_alternation] = STATE(23),
    [aux_sym_sequence_repeat1] = STATE(6),
    [sym_identifier] = ACTIONS(9),
    [anon_sym_LPAREN] = ACTIONS(11),
    [anon_sym_SQUOTE] = ACTIONS(13),
    [sym_comment] = ACTIONS(3),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(11), 1,
      anon_sym_LPAREN,
    ACTIONS(13), 1,
      anon_sym_SQUOTE,
    ACTIONS(28), 1,
      sym_identifier,
    STATE(3), 1,
      aux_sym_sequence_repeat1,
    ACTIONS(26), 2,
      ts_builtin_sym_end,
      anon_sym_PIPE,
    STATE(11), 6,
      sym__atom_rule,
      sym_repetition,
      sym_optional,
      sym_node_rule,
      sym_token,
      sym_label,
  [28] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(9), 1,
      sym_identifier,
    ACTIONS(11), 1,
      anon_sym_LPAREN,
    ACTIONS(13), 1,
      anon_sym_SQUOTE,
    STATE(3), 1,
      aux_sym_sequence_repeat1,
    ACTIONS(26), 2,
      anon_sym_RPAREN,
      anon_sym_PIPE,
    STATE(11), 6,
      sym__atom_rule,
      sym_repetition,
      sym_optional,
      sym_node_rule,
      sym_token,
      sym_label,
  [56] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(9), 1,
      sym_identifier,
    ACTIONS(11), 1,
      anon_sym_LPAREN,
    ACTIONS(13), 1,
      anon_sym_SQUOTE,
    STATE(5), 1,
      aux_sym_sequence_repeat1,
    STATE(21), 1,
      sym_sequence,
    STATE(11), 6,
      sym__atom_rule,
      sym_repetition,
      sym_optional,
      sym_node_rule,
      sym_token,
      sym_label,
  [83] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(9), 1,
      sym_identifier,
    ACTIONS(11), 1,
      anon_sym_LPAREN,
    ACTIONS(13), 1,
      anon_sym_SQUOTE,
    STATE(6), 1,
      aux_sym_sequence_repeat1,
    STATE(21), 1,
      sym_sequence,
    STATE(11), 6,
      sym__atom_rule,
      sym_repetition,
      sym_optional,
      sym_node_rule,
      sym_token,
      sym_label,
  [110] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(9), 1,
      sym_identifier,
    ACTIONS(11), 1,
      anon_sym_LPAREN,
    ACTIONS(13), 1,
      anon_sym_SQUOTE,
    STATE(16), 6,
      sym__atom_rule,
      sym_repetition,
      sym_optional,
      sym_node_rule,
      sym_token,
      sym_label,
  [131] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_COLON,
    ACTIONS(31), 8,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_QMARK,
      anon_sym_SQUOTE,
      anon_sym_PIPE,
      sym_identifier,
  [148] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(37), 1,
      anon_sym_STAR,
    ACTIONS(39), 1,
      anon_sym_QMARK,
    ACTIONS(35), 6,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_SQUOTE,
      anon_sym_PIPE,
      sym_identifier,
  [166] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(41), 8,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_QMARK,
      anon_sym_SQUOTE,
      anon_sym_PIPE,
      sym_identifier,
  [180] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(43), 8,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_QMARK,
      anon_sym_SQUOTE,
      anon_sym_PIPE,
      sym_identifier,
  [194] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(45), 8,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_QMARK,
      anon_sym_SQUOTE,
      anon_sym_PIPE,
      sym_identifier,
  [208] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 8,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_QMARK,
      anon_sym_SQUOTE,
      anon_sym_PIPE,
      sym_identifier,
  [222] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(49), 8,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_QMARK,
      anon_sym_SQUOTE,
      anon_sym_PIPE,
      sym_identifier,
  [236] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(53), 1,
      anon_sym_PIPE,
    STATE(20), 1,
      aux_sym_alternation_repeat1,
    ACTIONS(51), 2,
      ts_builtin_sym_end,
      sym_identifier,
  [250] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(7), 1,
      sym_identifier,
    ACTIONS(55), 1,
      ts_builtin_sym_end,
    STATE(19), 2,
      sym_node,
      aux_sym_grammar_repeat1,
  [264] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(57), 1,
      ts_builtin_sym_end,
    ACTIONS(59), 1,
      sym_identifier,
    STATE(19), 2,
      sym_node,
      aux_sym_grammar_repeat1,
  [278] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(53), 1,
      anon_sym_PIPE,
    STATE(22), 1,
      aux_sym_alternation_repeat1,
    ACTIONS(62), 2,
      ts_builtin_sym_end,
      sym_identifier,
  [292] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(64), 4,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      anon_sym_PIPE,
      sym_identifier,
  [302] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(66), 1,
      anon_sym_PIPE,
    STATE(22), 1,
      aux_sym_alternation_repeat1,
    ACTIONS(64), 2,
      ts_builtin_sym_end,
      sym_identifier,
  [316] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(69), 3,
      ts_builtin_sym_end,
      anon_sym_RPAREN,
      sym_identifier,
  [325] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(62), 1,
      anon_sym_RPAREN,
    ACTIONS(71), 1,
      anon_sym_PIPE,
    STATE(25), 1,
      aux_sym_alternation_repeat1,
  [338] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(64), 1,
      anon_sym_RPAREN,
    ACTIONS(73), 1,
      anon_sym_PIPE,
    STATE(25), 1,
      aux_sym_alternation_repeat1,
  [351] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_RPAREN,
    ACTIONS(71), 1,
      anon_sym_PIPE,
    STATE(24), 1,
      aux_sym_alternation_repeat1,
  [364] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(76), 2,
      ts_builtin_sym_end,
      sym_identifier,
  [372] = 2,
    ACTIONS(78), 1,
      sym_token_identifier,
    ACTIONS(80), 1,
      sym_comment,
  [379] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(82), 1,
      anon_sym_EQ,
  [386] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(84), 1,
      anon_sym_SQUOTE,
  [393] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(86), 1,
      anon_sym_RPAREN,
  [400] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(88), 1,
      ts_builtin_sym_end,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(5)] = 0,
  [SMALL_STATE(6)] = 28,
  [SMALL_STATE(7)] = 56,
  [SMALL_STATE(8)] = 83,
  [SMALL_STATE(9)] = 110,
  [SMALL_STATE(10)] = 131,
  [SMALL_STATE(11)] = 148,
  [SMALL_STATE(12)] = 166,
  [SMALL_STATE(13)] = 180,
  [SMALL_STATE(14)] = 194,
  [SMALL_STATE(15)] = 208,
  [SMALL_STATE(16)] = 222,
  [SMALL_STATE(17)] = 236,
  [SMALL_STATE(18)] = 250,
  [SMALL_STATE(19)] = 264,
  [SMALL_STATE(20)] = 278,
  [SMALL_STATE(21)] = 292,
  [SMALL_STATE(22)] = 302,
  [SMALL_STATE(23)] = 316,
  [SMALL_STATE(24)] = 325,
  [SMALL_STATE(25)] = 338,
  [SMALL_STATE(26)] = 351,
  [SMALL_STATE(27)] = 364,
  [SMALL_STATE(28)] = 372,
  [SMALL_STATE(29)] = 379,
  [SMALL_STATE(30)] = 386,
  [SMALL_STATE(31)] = 393,
  [SMALL_STATE(32)] = 400,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_grammar, 0),
  [7] = {.entry = {.count = 1, .reusable = true}}, SHIFT(29),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(10),
  [11] = {.entry = {.count = 1, .reusable = true}}, SHIFT(4),
  [13] = {.entry = {.count = 1, .reusable = true}}, SHIFT(28),
  [15] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_sequence_repeat1, 2),
  [17] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_sequence_repeat1, 2), SHIFT_REPEAT(10),
  [20] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_sequence_repeat1, 2), SHIFT_REPEAT(4),
  [23] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_sequence_repeat1, 2), SHIFT_REPEAT(28),
  [26] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_sequence, 1),
  [28] = {.entry = {.count = 2, .reusable = true}}, REDUCE(sym_sequence, 1), SHIFT(10),
  [31] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_node_rule, 1),
  [33] = {.entry = {.count = 1, .reusable = true}}, SHIFT(9),
  [35] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_sequence_repeat1, 1),
  [37] = {.entry = {.count = 1, .reusable = true}}, SHIFT(12),
  [39] = {.entry = {.count = 1, .reusable = true}}, SHIFT(13),
  [41] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_repetition, 2),
  [43] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_optional, 2),
  [45] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__atom_rule, 3),
  [47] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_token, 3),
  [49] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label, 3, .production_id = 2),
  [51] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_alternation, 1),
  [53] = {.entry = {.count = 1, .reusable = true}}, SHIFT(7),
  [55] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_grammar, 1),
  [57] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_grammar_repeat1, 2),
  [59] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_grammar_repeat1, 2), SHIFT_REPEAT(29),
  [62] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_alternation, 2),
  [64] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_alternation_repeat1, 2),
  [66] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_alternation_repeat1, 2), SHIFT_REPEAT(7),
  [69] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_rule, 1),
  [71] = {.entry = {.count = 1, .reusable = true}}, SHIFT(8),
  [73] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_alternation_repeat1, 2), SHIFT_REPEAT(8),
  [76] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_node, 3, .production_id = 1),
  [78] = {.entry = {.count = 1, .reusable = false}}, SHIFT(30),
  [80] = {.entry = {.count = 1, .reusable = false}}, SHIFT_EXTRA(),
  [82] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [84] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [86] = {.entry = {.count = 1, .reusable = true}}, SHIFT(14),
  [88] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
};

#ifdef __cplusplus
extern "C" {
#endif
#ifdef _WIN32
#define extern __declspec(dllexport)
#endif

extern const TSLanguage *tree_sitter_ungrammar(void) {
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
    .keyword_lex_fn = ts_lex_keywords,
    .keyword_capture_token = sym_identifier,
    .primary_state_ids = ts_primary_state_ids,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif

#include <tree_sitter/parser.h>
#include <wctype.h>

enum TokenType {
  UNESCAPED_SINGLE_QUOTE_STRING_FRAGMENT,
  UNESCAPED_DOUBLE_QUOTE_STRING_FRAGMENT,
  UNESCAPED_BACKTICK_STRING_FRAGMENT,
};

void *tree_sitter_yuck_external_scanner_create() { return NULL; }

void tree_sitter_yuck_external_scanner_destroy(void *payload) {}

unsigned tree_sitter_yuck_external_scanner_serialize(void *payload,
                                                     char *buffer) {
  return 0;
}
void tree_sitter_yuck_external_scanner_deserialize(void *payload,
                                                   const char *buffer,
                                                   unsigned length) {}

static void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

static bool scan_string_literal_fragment(TSLexer *lexer, int32_t quote) {
  for (bool has_content = false;; has_content = true) {
    lexer->mark_end(lexer);
    int32_t next = lexer->lookahead;
    if (next == quote) {
      return has_content;
    } else if (next == '\0') {
      return false;
    } else if (next == '$') {
      advance(lexer);
      if (lexer->lookahead == '{') {
        return has_content;
      }
    } else if (next == '\\') {
      return has_content;
    } else {
      advance(lexer);
    }
  }
}

bool tree_sitter_yuck_external_scanner_scan(void *payload, TSLexer *lexer,
                                            const bool *valid_symbols) {
  if (valid_symbols[UNESCAPED_DOUBLE_QUOTE_STRING_FRAGMENT]) {
    lexer->result_symbol = UNESCAPED_DOUBLE_QUOTE_STRING_FRAGMENT;
    return scan_string_literal_fragment(lexer, '"');
  }

  if (valid_symbols[UNESCAPED_SINGLE_QUOTE_STRING_FRAGMENT]) {
    lexer->result_symbol = UNESCAPED_SINGLE_QUOTE_STRING_FRAGMENT;
    return scan_string_literal_fragment(lexer, '\'');
  }

  if (valid_symbols[UNESCAPED_BACKTICK_STRING_FRAGMENT]) {
    lexer->result_symbol = UNESCAPED_BACKTICK_STRING_FRAGMENT;
    return scan_string_literal_fragment(lexer, '`');
  }

  return false;
}

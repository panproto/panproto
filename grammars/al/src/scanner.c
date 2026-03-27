#include "tree_sitter/parser.h"
#include <stdlib.h>
#include <string.h>
#include <wctype.h>

enum TokenType {
  PROPERTY_NAME = 0,
  CONTINUE_AS_IDENTIFIER = 1,
  PREPROC_OPEN = 2,
  PREPROC_CLOSE = 3,
  BEGIN_KEYWORD = 4,
  END_KEYWORD = 5,
  PREPROC_SPLIT_BEGIN = 6,
  PREPROC_SPLIT_END = 7,
};

typedef struct {
  uint8_t depth;  // current #if/#endif nesting depth (max 255)
} ScannerState;

void *tree_sitter_al_external_scanner_create() {
  ScannerState *state = calloc(1, sizeof(ScannerState));
  return state;
}

void tree_sitter_al_external_scanner_destroy(void *payload) {
  free(payload);
}

unsigned tree_sitter_al_external_scanner_serialize(void *payload, char *buffer) {
  ScannerState *state = (ScannerState *)payload;
  buffer[0] = (char)state->depth;
  return 1;
}

void tree_sitter_al_external_scanner_deserialize(
  void *payload, const char *buffer, unsigned length
) {
  ScannerState *state = (ScannerState *)payload;
  state->depth = (length > 0) ? (uint8_t)buffer[0] : 0;
}

static bool is_identifier_start(int32_t c) {
  return iswalpha(c) || c == '_';
}

static bool is_identifier_char(int32_t c) {
  return iswalnum(c) || c == '_';
}

// Skip whitespace and newlines (advance without marking)
static void skip_whitespace(TSLexer *lexer) {
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
         lexer->lookahead == '\r' || lexer->lookahead == '\n' ||
         lexer->lookahead == '\f') {
    lexer->advance(lexer, true);
  }
}

// Read a keyword case-insensitively. Returns true if matched and lookahead is
// not an identifier character after the keyword (i.e., whole word matched).
// Advances the lexer past the keyword on success.
static bool read_keyword_ci(TSLexer *lexer, const char *keyword) {
  for (int i = 0; keyword[i] != '\0'; i++) {
    if (towlower(lexer->lookahead) != keyword[i]) return false;
    lexer->advance(lexer, false);
  }
  // Ensure it's a whole-word match (not followed by identifier chars)
  if (is_identifier_char(lexer->lookahead)) return false;
  return true;
}

// Peek (without advancing) whether the keyword follows at current position.
// Skips whitespace first. Returns true if the keyword is found as a whole word.
static bool peek_keyword_ci(TSLexer *lexer, const char *keyword) {
  // We can't actually peek without advancing in the tree-sitter API.
  // This function advances freely — on false returns tree-sitter resets the lexer.
  skip_whitespace(lexer);
  for (int i = 0; keyword[i] != '\0'; i++) {
    if (towlower(lexer->lookahead) != keyword[i]) return false;
    lexer->advance(lexer, false);
  }
  if (is_identifier_char(lexer->lookahead)) return false;
  return true;
}

bool tree_sitter_al_external_scanner_scan(
  void *payload,
  TSLexer *lexer,
  const bool *valid_symbols
) {
  ScannerState *state = (ScannerState *)payload;

  // Error recovery guard: when all externals are valid, the parser is in
  // error recovery mode. Don't match anything — let the parser handle it.
  if (valid_symbols[PROPERTY_NAME] && valid_symbols[CONTINUE_AS_IDENTIFIER] &&
      valid_symbols[PREPROC_OPEN] && valid_symbols[PREPROC_CLOSE] &&
      valid_symbols[BEGIN_KEYWORD] && valid_symbols[END_KEYWORD] &&
      valid_symbols[PREPROC_SPLIT_BEGIN] &&
      valid_symbols[PREPROC_SPLIT_END]) {
    return false;
  }

  // PREPROC_OPEN (#if) and PREPROC_CLOSE (#endif) — combined dispatch
  // We must handle both in one block because consuming '#' is irreversible
  // within a single scanner call.
  if (valid_symbols[PREPROC_OPEN] || valid_symbols[PREPROC_CLOSE]) {
    skip_whitespace(lexer);
    if (lexer->lookahead == '#') {
      lexer->advance(lexer, false);
      if (valid_symbols[PREPROC_OPEN] && read_keyword_ci(lexer, "if")) {
        state->depth++;
        lexer->result_symbol = PREPROC_OPEN;
        return true;
      }
      // Note: if "if" didn't match, we consumed '#' + partial chars.
      // But read_keyword_ci only advances while chars match, so if it
      // returned false on the first char, we only consumed '#'.
      // For "#endif", after failing "#if" match (first char 'e' != 'i'),
      // the lexer is at 'e'. We can still try "endif".
      if (valid_symbols[PREPROC_CLOSE] && read_keyword_ci(lexer, "endif")) {
        if (state->depth > 0) state->depth--;
        lexer->result_symbol = PREPROC_CLOSE;
        return true;
      }
      // '#' was consumed but neither matched — return false
      return false;
    }
  }

  // BEGIN_KEYWORD: 'begin' at depth 0 only — decline at depth > 0
  if (valid_symbols[BEGIN_KEYWORD] && state->depth == 0) {
    skip_whitespace(lexer);
    if (read_keyword_ci(lexer, "begin")) {
      lexer->result_symbol = BEGIN_KEYWORD;
      return true;
    }
  }

  // END_KEYWORD: 'end' at depth 0 only — decline at depth > 0
  if (valid_symbols[END_KEYWORD] && state->depth == 0) {
    skip_whitespace(lexer);
    if (read_keyword_ci(lexer, "end")) {
      lexer->result_symbol = END_KEYWORD;
      return true;
    }
  }

  // PREPROC_SPLIT_BEGIN: 'begin' at depth > 0, immediately before #endif
  //
  // '#' handling: peek_keyword_ci is called with "#endif" (the full string
  // including '#'). PREPROC_OPEN/CLOSE manually advance past '#' before calling
  // read_keyword_ci("if"/"endif"). These are DIFFERENT conventions — do not mix.
  if (valid_symbols[PREPROC_SPLIT_BEGIN] && state->depth > 0) {
    skip_whitespace(lexer);
    if (read_keyword_ci(lexer, "begin")) {
      lexer->mark_end(lexer);  // token covers only 'begin'
      if (peek_keyword_ci(lexer, "#endif")) {
        lexer->result_symbol = PREPROC_SPLIT_BEGIN;
        return true;
      }
      // 'begin' found but #endif not next — return false.
      // tree-sitter resets lexer to pre-scan position; anonymous kw('begin') fires.
      return false;
    }
    return false;
  }

  // PREPROC_SPLIT_END: 'end' at depth > 0, followed by ';' then #else or #endif
  // Analogous to PREPROC_SPLIT_BEGIN but for 'end' tokens.
  // Fires when code_block's 'end' is inside a #if block.
  if (valid_symbols[PREPROC_SPLIT_END] && state->depth > 0) {
    skip_whitespace(lexer);
    if (read_keyword_ci(lexer, "end")) {
      lexer->mark_end(lexer);  // token covers only 'end'
      // Check for ';' then whitespace then '#else' or '#endif'
      while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
             lexer->lookahead == '\r' || lexer->lookahead == '\n' ||
             lexer->lookahead == '\f') {
        lexer->advance(lexer, false);
      }
      if (lexer->lookahead == ';') {
        lexer->advance(lexer, false);
        // Now check for #else or #endif after whitespace
        while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
               lexer->lookahead == '\r' || lexer->lookahead == '\n' ||
               lexer->lookahead == '\f') {
          lexer->advance(lexer, false);
        }
        if (lexer->lookahead == '#') {
          lexer->advance(lexer, false);
          if (read_keyword_ci(lexer, "else") || read_keyword_ci(lexer, "endif")) {
            lexer->result_symbol = PREPROC_SPLIT_END;
            return true;
          }
        }
      }
      // 'end' found but not followed by ; then #else/#endif — return false.
      return false;
    }
    return false;
  }

  // CONTINUE_AS_IDENTIFIER: match 'continue' followed by ':='
  if (valid_symbols[CONTINUE_AS_IDENTIFIER]) {
    // Skip leading whitespace
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
           lexer->lookahead == '\r' || lexer->lookahead == '\n' ||
           lexer->lookahead == '\f') {
      lexer->advance(lexer, true);
    }

    // Check for 'continue' (case-insensitive, exactly 8 chars)
    const char *keyword = "continue";
    int pos = 0;
    bool match = true;

    if (!is_identifier_start(lexer->lookahead)) {
      // Not an identifier — can't be continue_as_identifier or property_name
      return false;
    }

    // Try to match 'continue'
    while (is_identifier_char(lexer->lookahead)) {
      if (pos < 8) {
        if (towlower(lexer->lookahead) != keyword[pos]) {
          match = false;
        }
        pos++;
      } else {
        match = false;
        pos++;
      }
      lexer->advance(lexer, false);
    }

    // Check if exactly 'continue' (8 chars)
    if (match && pos == 8) {
      lexer->mark_end(lexer);

      // Skip whitespace
      while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
             lexer->lookahead == '\r' || lexer->lookahead == '\n' ||
             lexer->lookahead == '\f') {
        lexer->advance(lexer, false);
      }

      // Check for ':='
      if (lexer->lookahead == ':') {
        lexer->advance(lexer, false);
        if (lexer->lookahead == '=') {
          lexer->result_symbol = CONTINUE_AS_IDENTIFIER;
          return true;
        }
      }
    }

    // Did not match continue_as_identifier
    // Note: we can't fall through to PROPERTY_NAME here because we've
    // already consumed characters. The scanner will be called again
    // for the same position if we return false.
    return false;
  }

  // PROPERTY_NAME: match identifier followed by = (not :=)
  if (valid_symbols[PROPERTY_NAME]) {
    // Skip leading whitespace (extras are not skipped before external scanner)
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
           lexer->lookahead == '\r' || lexer->lookahead == '\n' ||
           lexer->lookahead == '\f') {
      lexer->advance(lexer, true);  // true = skip (whitespace)
    }

    // Must start with identifier character
    if (!is_identifier_start(lexer->lookahead)) return false;

    // Mark start
    lexer->mark_end(lexer);

    // Consume identifier
    while (is_identifier_char(lexer->lookahead)) {
      lexer->advance(lexer, false);
    }

    // Mark end of identifier (before whitespace/equals)
    lexer->mark_end(lexer);

    // Skip whitespace
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
           lexer->lookahead == '\r' || lexer->lookahead == '\f') {
      lexer->advance(lexer, false);
    }

    // Check for = but not :=
    if (lexer->lookahead == '=') {
      lexer->result_symbol = PROPERTY_NAME;
      return true;
    }

    return false;
  }

  return false;
}

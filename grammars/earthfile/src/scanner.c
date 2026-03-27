#include "tree_sitter/parser.h"

#include <ctype.h>
#include <stdio.h>
#include <string.h>
// #include <wctype.h>

// Enable this for debugging
// #define DEBUG_PRINT

#ifndef __FILE_NAME__
#define __FILE_NAME__ __FILE__
#endif

// #define DEBUG_PRINT

#ifdef DEBUG_PRINT
#define dbg_print(...)                                                         \
  do {                                                                         \
    fprintf(stderr, "    \033[96;1mparse: \033[0m");                           \
    fprintf(stderr, __VA_ARGS__);                                              \
  } while (0)
#else
#define dbg_print(...)
#endif

#define SBYTES sizeof(Scanner)

enum TokenType {
  INDENT,
  DEDENT,
};

typedef struct Scanner {
  uint32_t prev_indent;
  bool has_seen_eof;
} Scanner;

// This function should create your scanner object. It will only be called once
// anytime your language is set on a parser. Often, you will want to allocate
// memory on the heap and return a pointer to it. If your external scanner
// doesn’t need to maintain any state, it’s ok to return NULL.
void *tree_sitter_earthfile_external_scanner_create(void) {
  Scanner *ptr = (Scanner *)calloc(SBYTES, 1);
  return ptr;
}

// This function should free any memory used by your scanner. It is called once
// when a parser is deleted or assigned a different language. It receives as an
// argument the same pointer that was returned from the create function. If your
// create function didn’t allocate any memory, this function can be a noop.
void tree_sitter_earthfile_external_scanner_destroy(void *payload) {
  free(payload);
}

// Serialize the state of the scanner. This is called when the parser is
// serialized. It receives as an argument the same pointer that was returned
// from the create function.
unsigned tree_sitter_earthfile_external_scanner_serialize(void *payload,
                                                     char *buffer) {
  memcpy(buffer, payload, SBYTES);
  return SBYTES;
}

// Reconstruct a scanner from the serialized state. This is called when the
// parser is deserialized.
void tree_sitter_earthfile_external_scanner_deserialize(void *payload,
                                                   const char *buffer,
                                                   unsigned length) {
  Scanner *ptr = (Scanner *)payload;
  if (length == 0) {
    ptr->prev_indent = 0;
    ptr->has_seen_eof = false;
    return;
  }
  memcpy(ptr, buffer, SBYTES);
}

// Continue and include the preceding character in the token
static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

// Continue and discard the preceding character
static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

// An EOF works as a dedent
static bool handle_eof(TSLexer *lexer, Scanner *state,
                       const bool *valid_symbols) {
  if (state->has_seen_eof) {
    return false;
  }

  lexer->mark_end(lexer);

  if (valid_symbols[DEDENT] && state->prev_indent != 0) {
    dbg_print("handle_eof: DEDENT\n");
    lexer->result_symbol = DEDENT;
    state->has_seen_eof = true;
    return true;
  }

  return false;
}

// This function is responsible for recognizing external tokens. It should
// return true if a token was recognized, and false otherwise.
bool tree_sitter_earthfile_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
  dbg_print("--------------------\n");
  Scanner *scanner = (Scanner *)(payload);

  if (lexer->eof(lexer)) {
    return handle_eof(lexer, scanner, valid_symbols);
  }

  dbg_print("lexer->lookahead %i: '%c'\n", lexer->get_column(lexer), lexer->lookahead);

  if (valid_symbols[INDENT] || valid_symbols[DEDENT]) {
    do {
      dbg_print("while lexer->lookahead %i: '%c'\n", lexer->get_column(lexer), lexer->lookahead);
      switch (lexer->lookahead) {
        case '\n':
        case '\r':
        case '\f':
          advance(lexer);
          break;

        case '\t':
        case ' ':
          skip(lexer);
          break;
      }
    } while (!lexer->eof(lexer) && isspace(lexer->lookahead));

    if (lexer->eof(lexer)) {
      return handle_eof(lexer, scanner, valid_symbols);
    }

    uint32_t indent = lexer->get_column(lexer);
    if (indent > scanner->prev_indent && valid_symbols[INDENT] &&
        scanner->prev_indent == 0) {
      dbg_print("INDENT: %d\n", indent);
      lexer->result_symbol = INDENT;
      scanner->prev_indent = indent;
      return true;
    }
    if (indent < scanner->prev_indent && valid_symbols[DEDENT] && indent == 0) {
      dbg_print("DEDENT: %d\n", indent);
      lexer->result_symbol = DEDENT;
      scanner->prev_indent = indent;
      return true;
    }
  }

  dbg_print("no token, returning false\n");
  return false;
}

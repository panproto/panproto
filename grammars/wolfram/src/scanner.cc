
#include <tree_sitter/parser.h>

#include <cwctype> // for std::iswspace

enum TokenType {
  COMMENT
};

struct Scanner {
  Scanner() {}

  unsigned serialize(char *buffer) {
    return 0;
  }

  void deserialize(const char *buffer, unsigned length) {
    ;
  }

  bool scan_comment(TSLexer *lexer) {

    int depth = 1;

    while (!lexer->eof(lexer)) {
      
      switch (lexer->lookahead) {
        case '*': {

          lexer->advance(lexer, false);
          
          switch (lexer->lookahead) {
            case ')': {

              depth--;

              if (depth == 0) {
                lexer->result_symbol = COMMENT;
                lexer->advance(lexer, false);
                lexer->mark_end(lexer);
                return true;
              }

              break;
            }
          }

          break;
        }

        case '(': {

          lexer->advance(lexer, false);
          
          switch (lexer->lookahead) {
            case '*': {
              depth++;
              break;
            }
          }

          break;
        }

        default: {
          lexer->advance(lexer, false);
          break;
        }
      }
    }

    return false;
  }

  bool scan(TSLexer *lexer, const bool *valid_symbols) {

    while (iswspace(lexer->lookahead)) {
      lexer->advance(lexer, true);
    }

    switch (lexer->lookahead) {
      case '(': {

        lexer->advance(lexer, false);

        if (lexer->lookahead == '*') {

          lexer->advance(lexer, false);
          
          return scan_comment(lexer);
        }

        break;
      }
    }

    return false;
  }
};

extern "C" {

void *tree_sitter_wolfram_external_scanner_create() {
  return new Scanner();
}

void tree_sitter_wolfram_external_scanner_destroy(void *payload) {
  Scanner *scanner = static_cast<Scanner *>(payload);
  delete scanner;
}

unsigned tree_sitter_wolfram_external_scanner_serialize(void *payload, char *buffer) {
  Scanner *scanner = static_cast<Scanner *>(payload);
  return scanner->serialize(buffer);
}

void tree_sitter_wolfram_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
  Scanner *scanner = static_cast<Scanner *>(payload);
  scanner->deserialize(buffer, length);
}

bool tree_sitter_wolfram_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
  Scanner *scanner = static_cast<Scanner *>(payload);
  return scanner->scan(lexer, valid_symbols);
}

}

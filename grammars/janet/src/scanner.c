#include <tree_sitter/parser.h>
#include <wctype.h>

enum TokenType {
  _LONG_STR,
  _LONG_BUFFER,
};

void * tree_sitter_janet_external_scanner_create(void) { return NULL; }

void tree_sitter_janet_external_scanner_destroy(void *payload) {}

void tree_sitter_janet_external_scanner_reset(void *payload) {}

unsigned tree_sitter_janet_external_scanner_serialize(
    void *payload,
    char *buffer)
{
  return 0;
}

void tree_sitter_janet_external_scanner_deserialize(
    void *payload,
    const char *buffer,
    unsigned length
) { }

static void advance(TSLexer *lexer) {
    lexer->advance(lexer, false);
}

bool tree_sitter_janet_external_scanner_scan(
    void *payload,
    TSLexer *lexer,
    const bool *valid_symbols
) {
    while (iswspace(lexer->lookahead)) lexer->advance(lexer, true);

    // This was based on other person implementation of tree-sitter-janet:
    //    https://github.com/sogaiu/tree-sitter-janet/blob/master/src/scanner.cc
    if (valid_symbols[_LONG_STR] || valid_symbols[_LONG_BUFFER]) {
        if (lexer->lookahead == '@') {
            lexer->result_symbol = _LONG_BUFFER;
            advance(lexer);
        } else {
            lexer->result_symbol = _LONG_STR;
        }

        if (lexer->lookahead != '`') return false;

        advance(lexer);

        // Parse the first n consecutive backtips
        // stop when it's not
        int open_backtips = 1;
        while (true) {
            if (lexer->lookahead == 0) return false;

            if (lexer->lookahead == '`') {
                open_backtips++;
                advance(lexer);
                continue;
            } else {
                advance(lexer);
                break;
            }
        }

        int close_backtips = 0;
        while (true) {
            if (lexer->lookahead == 0) return false;

            if (lexer->lookahead == '`') {
                close_backtips++;
                if (close_backtips == open_backtips) {
                    advance(lexer);
                    return true;
                }
            } else {
                close_backtips = 0;
            }

            advance(lexer);
        }
    }

    return false;
}

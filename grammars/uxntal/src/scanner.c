#include "tree_sitter/parser.h"

#include <wctype.h>

enum TokenType { COMMENT };

void *tree_sitter_uxntal_external_scanner_create() { return NULL; }

void tree_sitter_uxntal_external_scanner_destroy(void *payload) {}

unsigned tree_sitter_uxntal_external_scanner_serialize(void *payload, char *buffer) { return 0; }

void tree_sitter_uxntal_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {}

static void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

bool tree_sitter_uxntal_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
    bool is_in_string = false;
    while (iswspace(lexer->lookahead)) {
        advance(lexer);
    }

    if (lexer->lookahead == '"') {
        is_in_string = true;
    }

    if (lexer->lookahead == '(' && !is_in_string) {
        advance(lexer);

        unsigned nesting_depth = 1;
        for (;;) {
            switch (lexer->lookahead) {
                case '\0':
                    return false;
                case '(':
                    advance(lexer);
                    nesting_depth++;
                    break;
                case ')':
                    advance(lexer);
                    nesting_depth--;
                    if (nesting_depth == 0) {
                        lexer->result_symbol = COMMENT;
                        return true;
                    }
                    break;
                default:
                    advance(lexer);
                    break;
            }
        }
    }

    if (lexer->lookahead != '"') {
        is_in_string = false;
    }

    return false;
}

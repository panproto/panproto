#include "tree_sitter/parser.h"

enum TokenType { NEWLINE };

void *tree_sitter_cooklang_external_scanner_create() { return NULL; }

bool tree_sitter_cooklang_external_scanner_scan(void *payload, TSLexer *lexer,
                                                const bool *valid_symbols) {
    if (lexer->lookahead == '\n' && valid_symbols[NEWLINE]) {
        lexer->result_symbol = NEWLINE;
        return true;
    }
    return false;
}

unsigned tree_sitter_cooklang_external_scanner_serialize(void *payload,
                                                         char *buffer) {
    return 0;
}

void tree_sitter_cooklang_external_scanner_deserialize(void *payload,
                                                       const char *buffer,
                                                       unsigned length) {}

void tree_sitter_cooklang_external_scanner_destroy(void *payload) {}

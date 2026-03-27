#include "tree_sitter/parser.h"
#include "tree_sitter/alloc.h"

enum TokenType {
    LOOKBACK_SEMICOLON,
    CLOSING_BRACE_MARKER,
    CLOSING_BRACE_UNMARKER,
};

bool* tree_sitter_haxe_external_scanner_create(void) {
    bool* just_saw_brace = ts_malloc(sizeof(bool));
    *just_saw_brace = false;
    return just_saw_brace;
}

void tree_sitter_haxe_external_scanner_destroy(bool* just_saw_brace) {
    ts_free(just_saw_brace);
}

unsigned tree_sitter_haxe_external_scanner_serialize(
    bool* just_saw_brace,
    char* buffer
) {
    // printf("serialising\n");
    buffer[0] = (char)(*just_saw_brace);
    return 1;
}

void tree_sitter_haxe_external_scanner_deserialize(
    bool* just_saw_brace,
    const char* buffer,
    unsigned length
) {
    if (length > 0) {
        *just_saw_brace = buffer[0];
    } else {
        *just_saw_brace = false;
    }
}

static bool is_whitespace(int32_t c) {
    return c == ' ' || c == '\n' || c == '\t' || c == '\r';
}

bool tree_sitter_haxe_external_scanner_scan(
    bool* just_saw_brace,
    TSLexer* lexer,
    const bool* valid_symbols
) {
    if (valid_symbols[LOOKBACK_SEMICOLON]) {
        if (lexer->lookahead == ';') {
            *just_saw_brace = false;
            lexer->result_symbol = LOOKBACK_SEMICOLON;
            lexer->advance(lexer, false);
            return true;
        } else if (*just_saw_brace) {
            *just_saw_brace = false;
            lexer->result_symbol = LOOKBACK_SEMICOLON;
            return true;
        }
        return false;
    }

    if (valid_symbols[CLOSING_BRACE_MARKER]) {
        lexer->mark_end(lexer);
        while (is_whitespace(lexer->lookahead)) {
            lexer->advance(lexer, true);
        }
        if (lexer->lookahead == '}') {
            *just_saw_brace = true;
            lexer->result_symbol = CLOSING_BRACE_MARKER;
            return true;
        }
    }

    if (valid_symbols[CLOSING_BRACE_UNMARKER] && *just_saw_brace && !(lexer->lookahead == '}' || is_whitespace(lexer->lookahead))) {
        *just_saw_brace = false;
        lexer->result_symbol = CLOSING_BRACE_UNMARKER;
        return true;
    }

    return false;
}

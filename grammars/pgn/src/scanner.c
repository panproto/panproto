#include "tree_sitter/parser.h"

enum TokenType {
    FULL_LINE_COMMENT_DELIMITER_BOL_ASSERTION,
    NOVELTY_GLYPH_BEFORE_PUNCTUATION,
};

static bool is_unicode_whitespace(int32_t codepoint) {
    switch (codepoint) {
        case 0x0009: // \t
        case 0x000A: // \n
        case 0x000B: // vertical tab
        case 0x000C: // form feed
        case 0x000D: // \r
        case 0x0020: // space
        case 0x0085: // next line (NEL)
        case 0x00A0: // no-break space
        case 0x1680: // ogham space mark
        case 0x2000: // en quad
        case 0x2001: // em quad
        case 0x2002: // en space
        case 0x2003: // em space
        case 0x2004: // three-per-em space
        case 0x2005: // four-per-em space
        case 0x2006: // six-per-em space
        case 0x2007: // figure space
        case 0x2008: // punctuation space
        case 0x2009: // thin space
        case 0x200A: // hair space
        case 0x2028: // line separator
        case 0x2029: // paragraph separator
        case 0x202F: // narrow no-break space
        case 0x205F: // medium mathematical space
        case 0x3000: // ideographic space
            return true;
        default:
            return false;
    }
}

void *tree_sitter_pgn_external_scanner_create(void) {
    return NULL;
}

void tree_sitter_pgn_external_scanner_destroy(void *payload) {
}

unsigned tree_sitter_pgn_external_scanner_serialize(void *payload, char *buffer) {
    return 0;
}

void tree_sitter_pgn_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
}

bool tree_sitter_pgn_external_scanner_scan(
    void *payload,
    TSLexer *lexer,
    const bool *valid_symbols
) {
    if (valid_symbols[FULL_LINE_COMMENT_DELIMITER_BOL_ASSERTION]) {
        // It's not really clear from the doc why we have to advance over whitespace, but we do.
        // eof is not really needed in this form, but does no harm.
        // If we must advance over whitespace, we accept Unicode to be consistent with grammar.js.
        while (!lexer->eof(lexer) && is_unicode_whitespace(lexer->lookahead)) {
            lexer->advance(lexer, true);
        }
        if (lexer->lookahead == '%' && lexer->get_column(lexer) == 0) {
            lexer->result_symbol = FULL_LINE_COMMENT_DELIMITER_BOL_ASSERTION;
            return true;
        }
    }
    if (valid_symbols[NOVELTY_GLYPH_BEFORE_PUNCTUATION]) {
        if (lexer->lookahead == 'N') {
            lexer->advance(lexer, false);
            // The close parenthesis lookahead is strictly needed; the others are for lenience.
            if (lexer->lookahead == ')' ||
                lexer->lookahead == '(' ||
                lexer->lookahead == '{') {
                lexer->result_symbol = NOVELTY_GLYPH_BEFORE_PUNCTUATION;
                return true;
            }
        }
    }
    return false;
}

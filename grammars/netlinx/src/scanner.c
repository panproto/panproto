#include "tree_sitter/parser.h"

#include <stdio.h>
#include <wctype.h>

#define DEBUG_AUTOMATIC_SEMICOLON 0

enum TokenType {
    AUTOMATIC_SEMICOLON,
};

void *tree_sitter_netlinx_external_scanner_create() { return NULL; }

void tree_sitter_netlinx_external_scanner_destroy(void *p) {}

unsigned tree_sitter_netlinx_external_scanner_serialize(void *payload, char *buffer) { return 0; }

void tree_sitter_netlinx_external_scanner_deserialize(void *p, const char *b, unsigned n) {}

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

typedef enum {
    REJECT,     // Semicolon is illegal, ie a syntax error occurred
    NO_NEWLINE, // Unclear if semicolon will be legal, continue
    ACCEPT,     // Semicolon is legal, assuming a comment was encountered
} WhitespaceResult;

/**
 * @param consume If false, only consume enough to check if comment indicates semicolon-legality
 */
static WhitespaceResult scan_whitespace_and_comments(TSLexer *lexer, bool *scanned_comment, bool consume) {
    bool saw_block_newline = false;

    for (;;) {
        while (iswspace(lexer->lookahead)) {
            skip(lexer);
        }

        if (lexer->lookahead == '/') {
            skip(lexer);

            if (lexer->lookahead == '/') {
                skip(lexer);

                while (lexer->lookahead != 0 && lexer->lookahead != '\n' && lexer->lookahead != 0x2028 &&
                       lexer->lookahead != 0x2029) {
                    skip(lexer);
                }

                *scanned_comment = true;
            } else if (lexer->lookahead == '*') {
                skip(lexer);

                while (lexer->lookahead != 0) {
                    if (lexer->lookahead == '*') {
                        skip(lexer);

                        if (lexer->lookahead == '/') {
                            skip(lexer);
                            *scanned_comment = true;

                            if (lexer->lookahead != '/' && !consume) {
                                return saw_block_newline ? ACCEPT : NO_NEWLINE;
                            }

                            break;
                        }
                    } else if (lexer->lookahead == '\n' || lexer->lookahead == 0x2028 || lexer->lookahead == 0x2029) {
                        saw_block_newline = true;
                        skip(lexer);
                    } else {
                        skip(lexer);
                    }
                }
            } else {
                return REJECT;
            }
        } else {
            return ACCEPT;
        }
    }
}

static bool scan_automatic_semicolon(TSLexer *lexer, bool comment_condition, bool *scanned_comment) {
    lexer->result_symbol = AUTOMATIC_SEMICOLON;
    lexer->mark_end(lexer);

    for (;;) {
        if (lexer->lookahead == 0) {
            return true;    // EOF - insert a semicolon
        }

        if (lexer->lookahead == '/') {
            WhitespaceResult result = scan_whitespace_and_comments(lexer, scanned_comment, false);
            if (result == REJECT) {
                return false;
            }

            if (result == ACCEPT && comment_condition && lexer->lookahead != ',' && lexer->lookahead != '=') {
                return true;
            }
        }

        if (lexer->lookahead == '}') {
            return true; // Before closing brace - insert a semicolon
        }

        if (lexer->is_at_included_range_start(lexer)) {
            return true;
        }

        if (lexer->lookahead == '\n' || lexer->lookahead == 0x2028 || lexer->lookahead == 0x2029) {
            break;  // Found a newline - potentially insert a semicolon
        }

        if (!iswspace(lexer->lookahead)) {
            return false;
        }

        skip(lexer);
    }

    skip(lexer);    // Skip newline

    if (scan_whitespace_and_comments(lexer, scanned_comment, true) == REJECT) {
        return false;
    }

    // Don't insert a semicolon before these characters
    switch (lexer->lookahead) {
        case ',':
        case ':':
        case ';':
        case '*':
        case '%':
        case '>':
        case '<':
        case '=':
        case '^':
        case '|':
        case '&':
        case '/':
            return false;

        // Insert a semicolon before opening parenthesis or bracket when preceded by a closing one
        case '(':
        case '[':
            return true;

        // Insert a semicolon before decimals literals but not otherwise.
        case '.':
            skip(lexer);
            return iswdigit(lexer->lookahead);

        // Insert a semicolon before `--` and `++`, but not before binary `+` or `-`.
        case '+':
            skip(lexer);
            return lexer->lookahead == '+';
        case '-':
            skip(lexer);
            return lexer->lookahead == '-';

        // Don't insert a semicolon before `!=`, but do insert one before a unary `!`.
        case '!':
            skip(lexer);
            return lexer->lookahead != '=';

        default:
            break;
    }

    // By default insert a semicolon
    return true;
}

bool tree_sitter_netlinx_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
    if (valid_symbols[AUTOMATIC_SEMICOLON]) {
        #if DEBUG_AUTOMATIC_SEMICOLON
        fprintf(stderr, "Scanner called: AUTOMATIC_SEMICOLON=%d, lookahead=%c\n",
            valid_symbols[AUTOMATIC_SEMICOLON], lexer->lookahead);
        #endif

        bool scanned_comment = false;
        bool ret = scan_automatic_semicolon(lexer, true, &scanned_comment);

        #if DEBUG_AUTOMATIC_SEMICOLON
        fprintf(stderr, "SCANNER: scan_automatic_semicolon returned %d\n", ret);
        #endif

        return ret;
    }

    return false;
}

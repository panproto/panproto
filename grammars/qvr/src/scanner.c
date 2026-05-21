/* External scanner for QVR's Python-style indentation layout.
 *
 * Emits four tokens:
 *
 *   NEWLINE  end of a statement at the current indent level
 *   INDENT   the indent column went up; opens a block
 *   DEDENT   the indent column went down; closes a block
 *   EOF      sentinel emitted once at end of input
 *
 * Adapted from the tree-sitter-python reference scanner; string
 * handling stripped out (QVR uses a regex-based string token directly
 * in grammar.js), comment handling stripped (QVR's comments are
 * tree-sitter `extras` and never carry an indent contribution).
 */

#include "tree_sitter/array.h"
#include "tree_sitter/parser.h"

#include <stdint.h>
#include <stdio.h>
#include <string.h>

enum TokenType {
    NEWLINE,
    INDENT,
    DEDENT,
    EOF_TOKEN,
};

typedef struct {
    Array(uint16_t) indents;
    /* Number of zero-width NEWLINE tokens we have emitted while
     * already at end-of-input. After each EOF NEWLINE the parser
     * either accepts it and advances internal state (we are
     * draining a decl-trailing NEWLINE) or rejects it; either way
     * we must commit to EOF_TOKEN on the second EOF call to avoid
     * looping. */
    uint8_t eof_newlines_emitted;
} Scanner;

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }
static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

bool tree_sitter_qvr_external_scanner_scan(
    void *payload, TSLexer *lexer, const bool *valid_symbols
) {
    Scanner *scanner = (Scanner *)payload;
    /* Mark the token end at the entry position. Without this,
     * tree-sitter implicitly extends the token to wherever the
     * scanner left the lexer after its last ``skip()`` call, so
     * peek-style indent measurements would silently consume the
     * scanned characters. Explicit ``mark_end`` at entry makes
     * the default token width zero; only INDENT and NEWLINE
     * deliberately advance ``mark_end`` to extend their tokens. */
    lexer->mark_end(lexer);

    /* At true end of input we must make progress on every call or
     * tree-sitter re-enters the scanner forever. The drain order is
     * deliberate:
     *
     *   1. DEDENTs for every still-open indent block: the outer
     *      grammar's ``program_decl`` / ``signature_decl`` /
     *      ``deduction_decl`` end with ``$._dedent`` before their
     *      trailing NEWLINE, so we must close blocks first.
     *   2. NEWLINE when the parser still expects one: the same
     *      decl rules then consume ``$._newline``.
     *   3. EOF_TOKEN as the final terminator that the
     *      ``source_file`` rule consumes; this is what stops
     *      tree-sitter from re-entering the scanner forever.
     *
     * The previous ordering preferred EOF_TOKEN over NEWLINE, which
     * left the parser stuck mid-rule (expecting NEWLINE after a
     * DEDENT) on every file that ended cleanly.
     */
    if (lexer->eof(lexer)) {
        if (valid_symbols[DEDENT] && scanner->indents.size > 1) {
            array_pop(&scanner->indents);
            scanner->eof_newlines_emitted = 0;
            lexer->result_symbol = DEDENT;
            return true;
        }
        /* At EOF: drain the parser's remaining expectations. The
         * grammar's decl-trailing rule expects ``$._newline``
         * before the source_file's ``$._eof`` consumes the final
         * terminator. We emit one NEWLINE at end-of-input (just
         * enough to close the outermost statement), then commit
         * to EOF_TOKEN for every subsequent call so tree-sitter
         * cannot loop on zero-width re-emission.
         */
        if (
            valid_symbols[NEWLINE]
            && scanner->eof_newlines_emitted == 0
        ) {
            scanner->eof_newlines_emitted = 1;
            lexer->result_symbol = NEWLINE;
            return true;
        }
        if (valid_symbols[EOF_TOKEN]) {
            lexer->result_symbol = EOF_TOKEN;
            return true;
        }
        return false;
    }

    /* The scanner has two entry shapes:
     *
     *   A) The parser just consumed a line-terminating token and
     *      is asking for NEWLINE / INDENT / DEDENT. ``mark_end``
     *      is at the very start of the new line, and the lookahead
     *      is either the leading whitespace of that line or the
     *      first non-whitespace character.
     *
     *   B) The parser is mid-statement and asked for NEWLINE. The
     *      lookahead is `\n`.
     *
     * We handle them with a single loop: skip a single line
     * terminator (if any), then measure the indent of the next
     * non-blank line. Whether the parser accepts an INDENT, DEDENT,
     * or NEWLINE depends on ``valid_symbols``; we always emit the
     * token that closes the most parser obligation.
     */

    /* Single-phase line scan.
     *
     * The scanner is called at one of two positions:
     *
     *   (a) Immediately after a `\n` was emitted as part of a
     *       prior token (or at the very start of the file). The
     *       lookahead is the first character of the new line: a
     *       space / tab (indented line), the line's first content
     *       character (zero-indent), or `\n` (blank line).
     *
     *   (b) Inside a statement, where the parser asked for
     *       NEWLINE. The lookahead is `\n` (or end of file).
     *
     * We handle both by walking from the current position through
     * any combination of leading whitespace, blank lines, and
     * comment-only lines, tracking:
     *
     *   * ``saw_newline`` — whether we consumed at least one `\n`
     *     (i.e. a statement boundary).
     *   * ``indent_length`` — the indent of the most recent line
     *     whose first character was content.
     *
     * Three commits are possible, in priority order:
     *
     *   1. INDENT — ``indent_length`` > top of indent stack AND
     *      INDENT is valid. ``mark_end`` extends past the leading
     *      whitespace so the parser's next token starts on
     *      content.
     *   2. DEDENT — ``indent_length`` < top of indent stack AND
     *      DEDENT is valid. ``mark_end`` stays at the line-start
     *      so the parser still has a chance to consume the
     *      decl-trailing NEWLINE on the next call.
     *   3. NEWLINE — we consumed a `\n` AND NEWLINE is valid.
     *      ``mark_end`` is set right after that `\n`.
     */

    /* Each call emits exactly one of NEWLINE / INDENT / DEDENT, and
     * the relative priority follows tree-sitter-python's reference
     * scanner:
     *
     *   * NEWLINE consumes a single `\n` (mark_end after `\n`).
     *   * INDENT consumes the leading whitespace of the new line
     *     (mark_end past it).
     *   * DEDENT is zero-width (mark_end stays at the entry position).
     *
     * NEWLINE has the highest priority when the current lookahead
     * is `\n`; only after NEWLINE has been consumed by the parser
     * do INDENT and DEDENT fire on subsequent calls.
     */

    /* NEWLINE path: lookahead is `\n` (possibly preceded by
     * `\r`/`\f`). Consume exactly one `\n` and emit. */
    if (
        valid_symbols[NEWLINE]
        && (
            lexer->lookahead == '\n'
            || lexer->lookahead == '\r'
            || lexer->lookahead == '\f'
        )
    ) {
        if (lexer->lookahead == '\r' || lexer->lookahead == '\f') {
            skip(lexer);
        }
        if (lexer->lookahead == '\n') {
            skip(lexer);
            lexer->mark_end(lexer);
            lexer->result_symbol = NEWLINE;
            return true;
        }
    }

    /* INDENT / DEDENT path: measure the indent of the current line
     * (the line we are at the start of, in the parser's view).
     * Walking past blank lines / comments here lets the indent
     * we compare against be the next *content* line's. */
    if (!(valid_symbols[INDENT] || valid_symbols[DEDENT])) {
        return false;
    }

    uint16_t indent_length = 0;
    for (;;) {
        int32_t c = lexer->lookahead;
        if (c == ' ') {
            indent_length++;
            skip(lexer);
        } else if (c == '\t') {
            indent_length += 8;
            skip(lexer);
        } else if (c == '\r' || c == '\f') {
            skip(lexer);
        } else if (c == '\n') {
            indent_length = 0;
            skip(lexer);
        } else if (c == '#') {
            while (lexer->lookahead && lexer->lookahead != '\n') {
                skip(lexer);
            }
            indent_length = 0;
        } else {
            break;
        }
    }

    if (scanner->indents.size > 0) {
        uint16_t current_indent_length = *array_back(&scanner->indents);
        if (valid_symbols[INDENT] && indent_length > current_indent_length) {
            array_push(&scanner->indents, indent_length);
            lexer->mark_end(lexer);
            lexer->result_symbol = INDENT;
            return true;
        }
        if (valid_symbols[DEDENT] && indent_length < current_indent_length) {
            array_pop(&scanner->indents);
            /* Zero-width DEDENT: leave ``mark_end`` at the entry
             * position so the parser still sees the upcoming line's
             * content (or another DEDENT) on the next call. */
            lexer->result_symbol = DEDENT;
            return true;
        }
    }

    return false;
}

unsigned tree_sitter_qvr_external_scanner_serialize(void *payload, char *buffer) {
    Scanner *scanner = (Scanner *)payload;

    size_t size = 0;

    /* Serialize the EOF-NEWLINE flag first (one byte). */
    if (size < TREE_SITTER_SERIALIZATION_BUFFER_SIZE) {
        buffer[size++] = (char)scanner->eof_newlines_emitted;
    }

    /* Serialize the indent stack, two bytes per entry (uint16_t LE).
     * Skip the implicit zero at the bottom of the stack; it is
     * reconstructed on deserialize. */
    uint32_t iter = 1;
    for (; iter < scanner->indents.size && size + 1 < TREE_SITTER_SERIALIZATION_BUFFER_SIZE; ++iter) {
        uint16_t indent_value = *array_get(&scanner->indents, iter);
        buffer[size++] = (char)(indent_value & 0xFF);
        buffer[size++] = (char)((indent_value >> 8) & 0xFF);
    }

    return size;
}

void tree_sitter_qvr_external_scanner_deserialize(
    void *payload, const char *buffer, unsigned length
) {
    Scanner *scanner = (Scanner *)payload;

    array_delete(&scanner->indents);
    array_push(&scanner->indents, 0);
    scanner->eof_newlines_emitted = 0;

    size_t size = 0;
    /* The first byte (when present) is the EOF-NEWLINE flag. */
    if (size < length) {
        scanner->eof_newlines_emitted = (uint8_t)buffer[size++];
    }
    while (size + 1 < length) {
        uint16_t indent_value = (unsigned char)buffer[size]
            | ((unsigned char)buffer[size + 1] << 8);
        array_push(&scanner->indents, indent_value);
        size += 2;
    }
}

void *tree_sitter_qvr_external_scanner_create(void) {
    Scanner *scanner = calloc(1, sizeof(Scanner));
    array_init(&scanner->indents);
    scanner->eof_newlines_emitted = 0;
    tree_sitter_qvr_external_scanner_deserialize(scanner, NULL, 0);
    return scanner;
}

void tree_sitter_qvr_external_scanner_destroy(void *payload) {
    Scanner *scanner = (Scanner *)payload;
    array_delete(&scanner->indents);
    free(scanner);
}

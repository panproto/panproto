#include <assert.h>
#include <tree_sitter/parser.h>

#define MAX(a, b) ((a) > (b) ? (a) : (b))

#define VEC_RESIZE(vec, _cap)                                                  \
    void *tmp = realloc((vec)->data, (_cap) * sizeof((vec)->data[0]));         \
    assert(tmp != NULL);                                                       \
    (vec)->data = tmp;                                                         \
    (vec)->cap = (_cap);

#define VEC_GROW(vec, _cap)                                                    \
    if ((vec)->cap < (_cap)) {                                                 \
        VEC_RESIZE((vec), (_cap));                                             \
    }

#define VEC_PUSH(vec, el)                                                      \
    if ((vec)->cap == (vec)->len) {                                            \
        VEC_RESIZE((vec), MAX(16, (vec)->len * 2));                            \
    }                                                                          \
    (vec)->data[(vec)->len++] = (el);

#define VEC_POP(vec) (vec)->len--;

#define VEC_BACK(vec) ((vec)->data[(vec)->len - 1])

#define VEC_FREE(vec)                                                          \
    {                                                                          \
        if ((vec)->data != NULL)                                               \
            free((vec)->data);                                                 \
    }

#define VEC_CLEAR(vec)                                                         \
    { (vec)->len = 0; }

enum TokenType {
    NEWLINE,
    INDENT,
    DEDENT,
    ERROR,
};

typedef struct {
    uint32_t len;
    uint32_t cap;
    uint16_t *data;
} indent_vec;

typedef struct {
    indent_vec *indents;
} Scanner;

static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

bool tree_sitter_firrtl_external_scanner_scan(void *payload, TSLexer *lexer,
                                              const bool *valid_symbols) {
    Scanner *scanner = (Scanner *)payload;

    lexer->mark_end(lexer);

    bool found_end_of_line = false;
    uint32_t indent_length = 0;
    for (;;) {
        if (lexer->lookahead == '\n') {
            found_end_of_line = true;
            indent_length = 0;
            skip(lexer);
        } else if (lexer->lookahead == ' ') {
            indent_length++;
            skip(lexer);
        } else if (lexer->lookahead == '\r' || lexer->lookahead == '\f') {
            indent_length = 0;
            skip(lexer);
        } else if (lexer->lookahead == '\t') {
            indent_length += 8;
            skip(lexer);
        } else if (lexer->lookahead == '#') {
            while (lexer->lookahead && lexer->lookahead != '\n') {
                skip(lexer);
            }
            skip(lexer);
            indent_length = 0;
        } else if (lexer->lookahead == '\\') {
            skip(lexer);
            if (lexer->lookahead == '\r') {
                skip(lexer);
            }
            if (lexer->lookahead == '\n' || lexer->eof(lexer)) {
                skip(lexer);
            } else {
                return false;
            }
        } else if (lexer->eof(lexer)) {
            indent_length = 0;
            found_end_of_line = true;
            break;
        } else {
            break;
        }
    }

    if (found_end_of_line) {
        if (scanner->indents->len > 0) {
            uint16_t current_indent_length = VEC_BACK(scanner->indents);

            if (valid_symbols[INDENT] &&
                indent_length > current_indent_length) {
                VEC_PUSH(scanner->indents, indent_length);
                lexer->result_symbol = INDENT;
                return true;
            }

            if ((valid_symbols[DEDENT] || !valid_symbols[NEWLINE]) &&
                indent_length < current_indent_length) {
                VEC_POP(scanner->indents);
                lexer->result_symbol = DEDENT;
                return true;
            }
        }

        if (valid_symbols[NEWLINE]) {
            lexer->result_symbol = NEWLINE;
            return true;
        }
    }

    return false;
}

unsigned tree_sitter_firrtl_external_scanner_serialize(void *payload,
                                                       char *buffer) {
    Scanner *scanner = (Scanner *)payload;

    size_t size = 0;
    int iter = 1;
    for (; iter < scanner->indents->len &&
           size < TREE_SITTER_SERIALIZATION_BUFFER_SIZE;
         ++iter) {
        buffer[size++] = (char)scanner->indents->data[iter];
    }
    return size;
}

void tree_sitter_firrtl_external_scanner_deserialize(void *payload,
                                                     const char *buffer,
                                                     unsigned length) {
    Scanner *scanner = (Scanner *)payload;
    VEC_CLEAR(scanner->indents);
    VEC_PUSH(scanner->indents, 0);

    if (length > 0) {
        size_t size = 0;
        for (; size < length; size++) {
            VEC_PUSH(scanner->indents, (unsigned char)buffer[size]);
        }
        assert(size == length);
    }
}

void *tree_sitter_firrtl_external_scanner_create() {
    Scanner *scanner = calloc(1, sizeof(Scanner));
    scanner->indents = calloc(1, sizeof(indent_vec));
    tree_sitter_firrtl_external_scanner_deserialize(scanner, NULL, 0);
    return scanner;
}

void tree_sitter_firrtl_external_scanner_destroy(void *payload) {
    Scanner *scanner = (Scanner *)payload;
    VEC_FREE(scanner->indents);
    free(scanner->indents);
    free(scanner);
}

// The layout tokens of lotml for tree-sitter: NEWLINE, INDENT and DEDENT, as Python's tokenizer
// makes them. Each is zero-width: the scanner looks ahead over line breaks, blank lines and
// comment lines without taking them, and the grammar's extras take them afterwards, so a
// comment still appears in the tree. A tab counts as four spaces. Inside brackets no layout
// token is valid, so a line break there is only whitespace.
//
// Written by hand, beside the grammar.js generated from harness/lotml_harness/lang/grammar.py.

#include "tree_sitter/array.h"
#include "tree_sitter/parser.h"

enum TokenType { NEWLINE, INDENT, DEDENT };

typedef struct {
    Array(uint16_t) indents;
} Scanner;

static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

void *tree_sitter_lotml_external_scanner_create(void) {
    Scanner *scanner = ts_calloc(1, sizeof(Scanner));
    array_init(&scanner->indents);
    array_push(&scanner->indents, 0);
    return scanner;
}

void tree_sitter_lotml_external_scanner_destroy(void *payload) {
    Scanner *scanner = (Scanner *)payload;
    array_delete(&scanner->indents);
    ts_free(scanner);
}

unsigned tree_sitter_lotml_external_scanner_serialize(void *payload, char *buffer) {
    Scanner *scanner = (Scanner *)payload;
    unsigned size = 0;
    // The first level is always 0, so it is not stored.
    for (uint32_t i = 1; i < scanner->indents.size; i++) {
        if (size + sizeof(uint16_t) > TREE_SITTER_SERIALIZATION_BUFFER_SIZE) {
            break;
        }
        uint16_t level = *array_get(&scanner->indents, i);
        memcpy(buffer + size, &level, sizeof(uint16_t));
        size += sizeof(uint16_t);
    }
    return size;
}

void tree_sitter_lotml_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
    Scanner *scanner = (Scanner *)payload;
    array_clear(&scanner->indents);
    array_push(&scanner->indents, 0);
    for (unsigned i = 0; i + sizeof(uint16_t) <= length; i += sizeof(uint16_t)) {
        uint16_t level;
        memcpy(&level, buffer + i, sizeof(uint16_t));
        array_push(&scanner->indents, level);
    }
}

bool tree_sitter_lotml_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
    Scanner *scanner = (Scanner *)payload;
    // In error recovery every token is valid; the layout is better left to the grammar.
    if (valid_symbols[NEWLINE] && valid_symbols[INDENT] && valid_symbols[DEDENT]) {
        return false;
    }
    lexer->mark_end(lexer);
    bool end_of_line = false;
    uint32_t indent = 0;
    for (;;) {
        if (lexer->lookahead == '\n') {
            end_of_line = true;
            indent = 0;
            skip(lexer);
        } else if (lexer->lookahead == ' ') {
            indent++;
            skip(lexer);
        } else if (lexer->lookahead == '\t') {
            indent += 4;
            skip(lexer);
        } else if (lexer->lookahead == '\r' || lexer->lookahead == '\f') {
            indent = 0;
            skip(lexer);
        } else if (lexer->lookahead == '#') {
            // A comment after code on its line is not layout; a comment line is skipped
            // like a blank one.
            if (!end_of_line) {
                return false;
            }
            while (lexer->lookahead && lexer->lookahead != '\n' && !lexer->eof(lexer)) {
                skip(lexer);
            }
        } else if (lexer->eof(lexer)) {
            end_of_line = true;
            indent = 0;
            break;
        } else {
            break;
        }
    }
    if (!end_of_line) {
        return false;
    }
    uint16_t current = *array_back(&scanner->indents);
    if (valid_symbols[INDENT] && indent > current) {
        array_push(&scanner->indents, (uint16_t)indent);
        lexer->result_symbol = INDENT;
        return true;
    }
    if (valid_symbols[DEDENT] && indent < current) {
        array_pop(&scanner->indents);
        lexer->result_symbol = DEDENT;
        return true;
    }
    if (valid_symbols[NEWLINE]) {
        lexer->result_symbol = NEWLINE;
        return true;
    }
    return false;
}

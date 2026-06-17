#include "tree_sitter/parser.h"

#include <wctype.h>

// External scanner for tree-sitter-abc.
//
// ABC is line-oriented. The grammar keeps `\n` out of `extras`, so the external
// tokens below are marked valid by tree-sitter only where they can legally
// appear: the line-leading field/directive tokens right after a `_newline`
// (line start), and the comment token both at a line start and after music on a
// line. The scanner therefore needs no explicit column tracking — it simply
// classifies the upcoming characters when one of its tokens is requested.
//
// Tokens (must match the `externals` order in grammar.js):
//   0 _field_key_tok      line-leading `LETTER:` for any letter except K/w/s
//   1 _key_field_key_tok  line-leading `K:` (the header terminator)
//   2 _lyric_key_tok      line-leading `w:`
//   3 _symbol_key_tok     line-leading `s:`
//   4 _directive_tok      `%%...` to end of line
//   5 _comment_tok        `%...`  to end of line (line start or trailing)
//   6 _error_sentinel     never emitted; present so the scanner runs in error
//                         recovery and can decline cleanly.

enum TokenType {
  FIELD_KEY_TOK,
  KEY_FIELD_KEY_TOK,
  LYRIC_KEY_TOK,
  SYMBOL_KEY_TOK,
  DIRECTIVE_TOK,
  COMMENT_TOK,
  ERROR_SENTINEL,
};

void *tree_sitter_abc_external_scanner_create(void) { return NULL; }
void tree_sitter_abc_external_scanner_destroy(void *payload) { (void)payload; }
unsigned tree_sitter_abc_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}
void tree_sitter_abc_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}

static void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

// Consume the rest of the current line (not the newline itself).
static void consume_to_eol(TSLexer *lexer) {
  while (lexer->lookahead != 0 && lexer->lookahead != '\n' && lexer->lookahead != '\r') {
    advance(lexer);
  }
}

bool tree_sitter_abc_external_scanner_scan(void *payload, TSLexer *lexer,
                                           const bool *valid_symbols) {
  (void)payload;

  // The error-recovery sentinel: when tree-sitter is in error recovery it marks
  // every external valid; decline so the internal lexer drives recovery.
  if (valid_symbols[ERROR_SENTINEL]) {
    return false;
  }

  int32_t c = lexer->lookahead;

  // ---- Comments and stylesheet directives: `%` / `%%` ----
  if (c == '%' && (valid_symbols[COMMENT_TOK] || valid_symbols[DIRECTIVE_TOK])) {
    advance(lexer); // first '%'
    if (lexer->lookahead == '%') {
      // `%%...` stylesheet directive (only meaningful at a line start, which is
      // the only place the grammar marks DIRECTIVE_TOK valid).
      if (valid_symbols[DIRECTIVE_TOK]) {
        advance(lexer); // second '%'
        consume_to_eol(lexer);
        lexer->result_symbol = DIRECTIVE_TOK;
        return true;
      }
      // Directive not valid here but comment is: a `%%` still reads as a comment
      // to end of line (a trailing `%% ...` on a music line is a comment).
      if (valid_symbols[COMMENT_TOK]) {
        consume_to_eol(lexer);
        lexer->result_symbol = COMMENT_TOK;
        return true;
      }
      return false;
    }
    // Plain `%...` comment.
    if (valid_symbols[COMMENT_TOK]) {
      consume_to_eol(lexer);
      lexer->result_symbol = COMMENT_TOK;
      return true;
    }
    return false;
  }

  // ---- Line-leading information-field key: `LETTER:` ----
  // Only attempted when one of the field-key tokens is valid (a line start).
  if ((valid_symbols[FIELD_KEY_TOK] || valid_symbols[KEY_FIELD_KEY_TOK] ||
       valid_symbols[LYRIC_KEY_TOK] || valid_symbols[SYMBOL_KEY_TOK]) &&
      ((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || c == '+')) {
    int32_t letter = c;
    advance(lexer);
    if (lexer->lookahead != ':') {
      // Not a field line — let the internal lexer read it (note, free text...).
      return false;
    }
    advance(lexer); // ':'

    // `K:` is the header terminator and a distinct token where requested
    // (header `key_field` + body key-change). Prefer it for the letter K.
    if (letter == 'K' && valid_symbols[KEY_FIELD_KEY_TOK]) {
      lexer->result_symbol = KEY_FIELD_KEY_TOK;
      return true;
    }
    if (letter == 'w' && valid_symbols[LYRIC_KEY_TOK]) {
      lexer->result_symbol = LYRIC_KEY_TOK;
      return true;
    }
    if (letter == 's' && valid_symbols[SYMBOL_KEY_TOK]) {
      lexer->result_symbol = SYMBOL_KEY_TOK;
      return true;
    }
    if (valid_symbols[FIELD_KEY_TOK]) {
      lexer->result_symbol = FIELD_KEY_TOK;
      return true;
    }
    return false;
  }

  return false;
}

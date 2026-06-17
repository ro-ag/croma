/**
 * @file ABC music notation grammar (ABC 2.1)
 * @author croma
 * @license MIT
 *
 * Grounded in croma's strict ABC 2.1 recognition (`MusicTokenKind` taxonomy in
 * crates/croma-core/src/syntax/music.rs and the music-line parser in
 * crates/croma-core/src/parse/). The grammar mirrors croma's surface tokens; it
 * does not invent syntax. It degrades gracefully (ERROR nodes) on edges croma's
 * parser recovers from — the croma-lsp semantic tokens backstop highlighting.
 *
 * ABC is line-oriented and newline-significant. An external scanner
 * (src/scanner.c) resolves the central ambiguity: at the start of a content
 * line, `K:` is a field but `C` is a note. At a line start the scanner emits a
 * field-key token (`X:`, `K:` → `_field_key_tok`; `w:` → `_lyric_key_tok`;
 * `s:` → `_symbol_key_tok`), a `%%...` directive line, or a `%...` comment.
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

module.exports = grammar({
  name: 'abc',

  // Newlines are structurally significant (one logical construct per line), so
  // `\n` is NOT in `extras`. Intra-line spacing is modeled explicitly.
  extras: () => [],

  externals: ($) => [
    // A line-leading information-field key + colon (`X:`, `T:`, `V:`, `M:`...),
    // for any letter other than `K`/`w`/`s`. Emitted only at a line start whose
    // first run is `LETTER:`. Disambiguates a field from a note (`C`).
    $._field_key_tok,
    // A line-leading `K:` key + colon. ABC's header terminator (§2.2), so it is
    // a distinct token: the K: line is what ends a `tune_header`.
    $._key_field_key_tok,
    // A line-leading `w:` aligned-lyric key + colon.
    $._lyric_key_tok,
    // A line-leading `s:` symbol-line key + colon.
    $._symbol_key_tok,
    // A line-leading stylesheet directive `%%...` (whole line, sans newline).
    $._directive_tok,
    // A comment `%...` to end of line (line start OR after music on a line).
    $._comment_tok,
    // Error sentinel so the external scanner cooperates with error recovery.
    $._error_sentinel,
  ],

  conflicts: ($) => [
    // A `%%`-directive or `%`-comment line BEFORE the first `X:` is either a
    // file-header item or the leading line of the first tune's header. GLR
    // explores both; the K:-terminated tune that follows resolves it.
    [$._file_header_item, $._header_line],
  ],

  rules: {
    // A file: an optional file-header region (directives / comments / fields /
    // free text appearing BEFORE the first tune, ABC 2.1 §2.2), then a sequence
    // of tunes. Sections are separated by blank lines. Keeping file-header
    // fields out of the post-tune position removes the field-vs-new-tune
    // ambiguity (a `LETTER:` line right after a tune's `K:` is always a body
    // line, never a fresh top-level field).
    source_file: ($) =>
      seq(
        repeat($._newline),
        // File-header region: each item self-terminates with its own newline,
        // and blank lines between items are absorbed.
        repeat(seq($._file_header_item, repeat($._newline))),
        optional(
          seq(
            $.tune,
            // Consecutive tunes are separated by a blank line (§2.2): two
            // adjacent `X:` with no blank line is malformed. Requiring the
            // blank-line gap also makes a `LETTER:` line right after a tune's
            // header unambiguously a body line, not the next tune.
            repeat(seq(repeat1($._newline), $.tune)),
            repeat($._newline),
          ),
        ),
      ),

    // The file-header region (before the first tune) holds stylesheet
    // directives, comments, free text, and — when they appear before any `X:` —
    // information `field` lines (e.g. a file-wide `I:abc-charset`, default
    // `M:`/`L:`). The field-vs-new-tune ambiguity that forced these out earlier
    // only existed in the POST-tune position, which no longer admits bare
    // fields, so they are safe here. A malformed pseudo-field (`notC:`) lands in
    // `free_text` and is tolerated.
    _file_header_item: ($) =>
      choice(
        seq($.stylesheet_directive, $._newline),
        seq($.comment, $._newline),
        $.field,
        $.free_text,
      ),

    // ---- Tune ------------------------------------------------------------
    tune: ($) => seq($.tune_header, optional($.tune_body)),

    // Header = field/directive/comment lines, terminated by the K: line.
    tune_header: ($) => seq(repeat($._header_line), $.key_field),

    _header_line: ($) =>
      choice(
        $.field,
        seq($.stylesheet_directive, $._newline),
        seq($.comment, $._newline),
      ),

    key_field: ($) =>
      seq(
        alias($._key_field_key_tok, $.field_key),
        optional(alias($._line_text, $.field_value)),
        $._newline,
      ),

    // Generic information field line: `LETTER: value`. Covers X:, T:, C:, M:,
    // L:, Q:, V:, U:, m:, I:, P:, and unrecognized letters (croma keeps them as
    // Unknown fields). `w:`/`s:` get dedicated body rules.
    field: ($) =>
      seq(
        alias($._field_key_tok, $.field_key),
        optional(alias($._line_text, $.field_value)),
        $._newline,
      ),

    // ---- Tune body -------------------------------------------------------
    tune_body: ($) => repeat1($._body_line),

    _body_line: ($) =>
      choice(
        $.lyric_line,
        $.symbol_line,
        $.field,
        $.key_field, // mid-tune key change (`K:G` line in the body)
        seq($.stylesheet_directive, $._newline),
        seq($.comment, $._newline),
        $.music_line,
      ),

    lyric_line: ($) =>
      seq(
        alias($._lyric_key_tok, $.field_key),
        optional(alias($._line_text, $.field_value)),
        $._newline,
      ),

    symbol_line: ($) =>
      seq(
        alias($._symbol_key_tok, $.field_key),
        optional(alias($._line_text, $.field_value)),
        $._newline,
      ),

    // A music line: a run of music elements ending in a newline. A `\`
    // line-continuation may join it to the next physical line; that next line is
    // usually more music, but ABC also allows an intervening information field
    // (`...|\` then `M:2/4` then `|...`) — croma joins across it
    // (merge_continued_barline_run). The `line_continuation` then an optional
    // `field` are consumed as ordinary line elements, so the field interruption
    // only appears where a continuation put the parser mid-line.
    music_line: ($) =>
      seq(
        repeat1(
          choice($._music_element, seq($.line_continuation, repeat($.field))),
        ),
        optional($.comment),
        $._newline,
      ),

    // ---- Music elements --------------------------------------------------
    _music_element: ($) =>
      choice(
        $.note,
        $.rest,
        $.multi_measure_rest,
        $.spacer,
        $.chord,
        $.grace_group,
        $.chord_symbol,
        $.annotation,
        $.decoration,
        $.tuplet,
        $.repeat_ending,
        $.barline,
        $.inline_field,
        $.slur,
        $.tie,
        $.broken_rhythm,
        $.overlay,
        $._score_line_break,
        $._space,
      ),

    note: ($) =>
      prec.right(
        seq(
          // A misplaced length run (`/` and/or digits) between an accidental and
          // its note (`^/c`, `_3/2D`) is malformed but very common in the wild;
          // croma recovers it (parse::music::misplaced_length_run_before_note).
          // It is only accepted WITH an accidental, so a bare leading `/` still
          // errors as in croma.
          optional(seq($.accidental, optional($._misplaced_length))),
          $.pitch,
          repeat($.octave),
          optional($.length),
        ),
      ),

    _misplaced_length: () => token(/[0-9]*\/+[0-9]*/),

    accidental: () => token(choice('^^', '__', '^', '_', '=')),

    pitch: () => /[A-Ga-g]/,

    octave: () => token(choice('\'', ',')),

    // Length: `2`, `/2`, `3/2`, `//`, `/`. (Numerator? slashes denominator?.)
    length: () =>
      token(
        choice(
          /[0-9]+\/[0-9]+/,
          /[0-9]+\/+/,
          /\/+[0-9]*/,
          /[0-9]+/,
        ),
      ),

    rest: ($) => seq(token(choice('z', 'x')), optional($.length)),

    multi_measure_rest: ($) => seq(token(choice('Z', 'X')), optional($.length)),

    spacer: () => 'y',

    chord: ($) =>
      seq(
        '[',
        repeat1(
          choice(
            $.note,
            $.tie,
            $.slur,
            $.decoration,
            $.chord_symbol,
            $.annotation,
            $._space,
          ),
        ),
        ']',
        optional($.length),
      ),

    grace_group: ($) =>
      seq(
        '{',
        optional('/'),
        repeat(
          choice($.note, $.chord, $.rest, $.slur, $.decoration, $._space),
        ),
        '}',
      ),

    // Quoted text: chord symbol vs positioned annotation, decided by the first
    // inner char (croma::classify_quoted_text: ^_<>@ → annotation).
    chord_symbol: () => token(seq('"', optional(/[^"^_<>@\n][^"\n]*/), '"')),
    annotation: () => token(seq('"', /[\^_<>@][^"\n]*/, '"')),

    decoration: () =>
      choice(
        token(seq('!', /[^!\n|]*/, '!')),
        token(seq('+', /[^+\n|]*/, '+')),
        // A bare `!` (or `+`) not closing a pair: the deprecated ABC line-break
        // marker (often at end of line, `...|!`) or a dangling delimiter. croma
        // recovers these as malformed; the grammar accepts them so they don't
        // poison the whole tune.
        token(prec(-1, '!')),
        token(prec(-1, '+')),
        '.',
        token(/[~HLMOPSTuv]/),
      ),

    tuplet: () =>
      token(
        seq(
          '(',
          /[0-9]+/,
          optional(seq(':', /[0-9]*/, optional(seq(':', /[0-9]*/)))),
        ),
      ),

    slur: () => token(choice('(', ')', '.(', '.)')),

    tie: () => token(choice('-', '.-')),

    broken_rhythm: () => token(choice(/>+/, /<+/)),

    overlay: () => '&',

    // Repeat ending: `[1`, `[1,2`, `[1-3`, `["text"`. The bar-glued `|1`
    // shorthand is emitted by the barline token, which stops before a digit, so
    // the digit run is consumed here as a (bar-anchored) ending.
    repeat_ending: () =>
      token(
        choice(
          seq('[', /[0-9]/, optional(/[0-9,\-]*/)),
          seq('[', '"', /[^"\n]*/, '"'),
          // Bar-glued shorthand after a barline (`|1`, `|1,3`, `|1-2`): a digit
          // optionally followed by a comma/range list.
          seq(/[0-9]/, optional(/[0-9,\-]*/)),
        ),
      ),

    // Barlines: every spelling croma's `barline_kind` recognizes, plus liberal
    // runs (§4.8: bar lines may be any sequence of `|`, `[`, `]`, `:`). A
    // bar-glued ending number (`|1`) is a separate `repeat_ending` node, so the
    // run stops before a trailing digit.
    //
    // `[`-led runs (`[|`, `[|:`, `[|]`) are explicit tokens: a bare `[` that
    // opens a chord / inline field / variant ending must NOT be eaten as a bar,
    // so the liberal run below intentionally does not start with `[`.
    barline: () =>
      choice(
        token('[|]'), //                            invisible
        token('[|:'), //                            bracket repeat start
        token('[|'), //                             initial (thick-thin)
        token(prec(1, /:?\|+:*[\]]?/)), //          |, ||, |:, ||:, :|, :||, :|], ...
        token(prec(1, /:+\|+[\]:]*/)), //           ::, ::|, :|:, ...
        token(prec(1, /\|*\]\|*:*/)), //            ], ||], ]|, ]|:, ...
        token(prec(1, /::+/)), //                   ::  (repeat both)
        token(/:\]/), //                            :]  (= :|], §4.8 liberal)
        token(/\.\|:?/), //                         dotted bar / dotted repeat start
        token(/\.:\|/), //                          dotted repeat end
      ),

    inline_field: ($) =>
      seq(
        '[',
        alias($._inline_field_key, $.field_key),
        optional(alias(token.immediate(/[^\]\n]+/), $.field_value)),
        ']',
      ),

    _inline_field_key: () => token(seq(/[A-Za-z]/, ':')),

    _score_line_break: () => '$',

    // Line continuation: a `\` at end of a music line joins it to the next
    // physical line (ABC 2.1 line-continuation). Trailing space and an optional
    // trailing comment after the `\` are absorbed, then the newline; the music
    // line keeps going on the next line rather than terminating.
    line_continuation: ($) =>
      seq('\\', optional($._space), optional($.comment), $._newline),

    // ---- Comments / directives ------------------------------------------
    comment: ($) => alias($._comment_tok, $.comment_text),

    stylesheet_directive: ($) => alias($._directive_tok, $.directive_text),

    free_text: ($) => seq($._free_text_line, $._newline),

    // ---- Lexical primitives ---------------------------------------------
    _line_text: () => /[^\n]+/,

    _free_text_line: () => /[^\n]+/,

    // Intra-line spacing. The backtick is ABC 2.1's invisible beam-grouping
    // separator (typographic only, no musical effect) and groups with spaces.
    _space: () => /[ \t`]+/,

    _newline: () => /\r?\n/,
  },
});

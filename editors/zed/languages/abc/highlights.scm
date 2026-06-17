; ABC syntax highlighting for Zed.
;
; Zed-flavored copy of the canonical grammar query
; (tree-sitter-abc/queries/highlights.scm). Capture names are standard
; tree-sitter highlight conventions and are kept IN SYNC with the canonical
; query; Zed maps them to the active theme (unrecognized captures degrade
; gracefully, and croma-lsp semantic tokens backstop highlighting). When the
; canonical grammar query changes, update this copy to match.
;
; Capture <-> croma-lsp semantic-token legend (crates/croma-lsp/src/tokens.rs):
;   pitch         -> @variable          (legend `variable`)
;   accidental    -> @attribute         (legend `modifier`)
;   octave        -> @attribute         (legend `modifier`)
;   length        -> @number            (legend `number`)
;   tuplet        -> @number            (legend `number`)
;   broken        -> @number            (legend `number`)
;   barline       -> @operator          (legend `operator`)
;   slur/tie      -> @operator          (legend `operator`)
;   repeat/overlay-> @operator          (legend `operator`)
;   chord_symbol  -> @string            (legend `string`)
;   annotation    -> @string            (legend `string`)
;   decoration    -> @function.macro    (legend `decorator`)
;   field_key     -> @keyword           (legend `keyword`)
;   inline field  -> @keyword           (legend `keyword`)
;   comment       -> @comment           (legend `comment`)
;   rest          -> @constant.builtin  (legend `abcRest`)

; ---- Pitches & note components ----
(pitch) @variable
(accidental) @attribute
(octave) @attribute
(length) @number

; ---- Rests / spacers ----
(rest) @constant.builtin
(multi_measure_rest) @constant.builtin
(spacer) @comment

; ---- Rhythm ----
(tuplet) @number
(broken_rhythm) @number

; ---- Structure operators ----
(barline) @operator
(slur) @operator
(tie) @operator
(repeat_ending) @operator
(overlay) @operator

; ---- Text attachments ----
(chord_symbol) @string
(annotation) @string

; ---- Decorations ----
(decoration) @function.macro

; ---- Fields ----
(field_key) @keyword
(field (field_value) @string)
(key_field (field_value) @constant)
(lyric_line (field_value) @string.special)
(symbol_line (field_value) @string.special)
(inline_field (field_key) @keyword)
(inline_field (field_value) @string)

; ---- Tune reference (X:) and title (T:) emphasis via field key text ----
((field_key) @keyword.directive
  (#match? @keyword.directive "^[XT]:"))

; ---- Comments & directives ----
(comment) @comment
(stylesheet_directive) @keyword.directive
(directive_text) @comment.doc

; ---- Free text / continuation ----
(free_text) @text
(line_continuation) @punctuation.special

; ---- Brackets / punctuation ----
"[" @punctuation.bracket
"]" @punctuation.bracket
"{" @punctuation.bracket
"}" @punctuation.bracket

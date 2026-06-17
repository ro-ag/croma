; tree-sitter-abc — syntax highlighting
;
; Capture names follow tree-sitter highlight conventions and are conceptually
; aligned with croma-lsp's semantic-token legend (crates/croma-lsp/src/tokens.rs):
;   pitch        -> variable/constant   (legend `variable`)
;   accidental   -> @attribute          (legend `modifier`)
;   octave       -> @attribute          (legend `modifier`)
;   length       -> @number             (legend `number`)
;   tuplet       -> @number             (legend `number`)
;   broken       -> @number             (legend `number`)
;   barline      -> @operator           (legend `operator`)
;   slur/tie     -> @operator           (legend `operator`)
;   repeat/overlay-> @operator          (legend `operator`)
;   chord_symbol -> @string             (legend `string`)
;   annotation   -> @string             (legend `string`)
;   decoration   -> @function.macro     (legend `decorator`)
;   field_key    -> @keyword            (legend `keyword`)
;   inline field -> @keyword            (legend `keyword`)
;   comment      -> @comment            (legend `comment`)
;   rest         -> @constant.builtin   (legend `abcRest`)
; Editors (Zed, nvim, helix) map these capture names to their own themes.

(pitch) @variable
(accidental) @attribute
(octave) @attribute
(length) @number

(rest) @constant.builtin
(multi_measure_rest) @constant.builtin
(spacer) @comment

(tuplet) @number
(broken_rhythm) @number

(barline) @operator
(slur) @operator
(tie) @operator
(repeat_ending) @operator
(overlay) @operator

(chord_symbol) @string
(annotation) @string

(decoration) @function.macro

(field_key) @keyword
(field (field_value) @string)
(key_field (field_value) @constant)
(lyric_line (field_value) @string.special)
(symbol_line (field_value) @string.special)
(inline_field (field_key) @keyword)
(inline_field (field_value) @string)

; Tune reference (X:) and title (T:) emphasis via field key text.
((field_key) @keyword.directive
  (#match? @keyword.directive "^[XT]:"))

(comment) @comment
(stylesheet_directive) @keyword.directive
(directive_text) @comment.documentation

(free_text) @text
(line_continuation) @punctuation.special

"[" @punctuation.bracket
"]" @punctuation.bracket
"{" @punctuation.bracket
"}" @punctuation.bracket

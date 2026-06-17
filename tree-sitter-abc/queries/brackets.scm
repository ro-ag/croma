; tree-sitter-abc — matching brackets
;
; The bracket pairs ABC uses inside a music line: chords `[...]`, grace groups
; `{...}`. (Slur parens `(`/`)` are separate per-note operators, not a single
; bracketed node, so they are not paired here.) Editors use these for
; match-highlighting and structural navigation.

(chord
  "[" @open
  "]" @close)

(grace_group
  "{" @open
  "}" @close)

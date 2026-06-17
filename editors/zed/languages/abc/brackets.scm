; ABC matching brackets for Zed.
;
; Zed-flavored copy of tree-sitter-abc/queries/brackets.scm — kept in sync with
; the canonical grammar query. The bracket pairs ABC uses inside a music line:
; chords `[...]` and grace groups `{...}`. (Slur parens `(`/`)` are separate
; per-note operators, not a single bracketed node, so they are not paired here.)
; Zed uses these for match-highlighting and structural navigation.

(chord
  "[" @open
  "]" @close)

(grace_group
  "{" @open
  "}" @close)

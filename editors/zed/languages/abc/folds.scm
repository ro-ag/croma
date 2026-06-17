; ABC code folding for Zed.
;
; Zed-flavored copy of tree-sitter-abc/queries/folds.scm — kept in sync with the
; canonical grammar query. A whole tune folds (header through body); the body
; folds on its own so the header can stay visible; the header folds too. ABC's
; other constructs are line-oriented, so these are the meaningful fold scopes.

(tune) @fold
(tune_body) @fold
(tune_header) @fold

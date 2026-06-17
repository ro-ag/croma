; tree-sitter-abc — folding
;
; A whole tune folds (from its X:/header down through the body), and the body
; folds on its own so the header can stay visible. Multi-line constructs are
; otherwise line-oriented in ABC, so these two are the meaningful fold scopes.

(tune) @fold
(tune_body) @fold
(tune_header) @fold

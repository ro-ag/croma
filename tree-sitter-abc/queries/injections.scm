; tree-sitter-abc — language injections
;
; This file is the ABC side of two injection stories:
;
; 1. ABC INTO MARKDOWN. A consumer's Markdown grammar injects ABC into fenced
;    code blocks tagged `abc`:
;
;      ```abc
;      X:1
;      K:C
;      CDEF|
;      ```
;
;    That rule lives in the MARKDOWN grammar's `injections.scm` (it matches the
;    fenced block's info string against the language name), e.g.:
;
;      ((fenced_code_block
;         (info_string (language) @injection.language)
;         (code_fence_content) @injection.content)
;       (#eq? @injection.language "abc"))
;
;    The Zed extension / web demo ships that markdown-side rule; ABC's own
;    grammar cannot inject itself into a host it does not parse.
;
; 2. INJECTIONS WITHIN ABC. ABC has no embedded foreign language in its core
;    surface (lyrics, chord symbols, and annotations are free text, not code),
;    so there are no within-ABC injections to declare here. This file is kept as
;    the canonical home for the markdown-side rule documented above and for any
;    future embedded-language support.

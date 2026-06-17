; tree-sitter-abc — Markdown injection query (host-side).
;
; Drop this into a Markdown tree-sitter consumer to highlight fenced code blocks
; tagged `abc` with the tree-sitter-abc grammar:
;
;     ```abc
;     X:1
;     K:C
;     CDEF|
;     ```
;
; This query runs against the MARKDOWN grammar's parse tree, not ABC's — ABC's
; own grammar cannot inject itself into a host it does not parse. It targets the
; modern split tree-sitter-markdown grammar (the one Neovim, Helix, and Zed use:
; tree-sitter-grammars/tree-sitter-markdown), whose `fenced_code_block` exposes
; the fence label as a `language` node inside `info_string`. Runtime-verified in
; croma stage G3 against @tree-sitter-grammars/tree-sitter-markdown@0.3.2 +
; `markdown-injection.md` (matches both ```abc blocks, excludes a ```rust one;
; the injected ABC then parses ERROR-free under tree-sitter-abc).
;
; WHY IT LIVES UNDER test/fixtures/ (not queries/): `tree-sitter test` validates
; every queries/*.scm against the ABC grammar, and `fenced_code_block` /
; `info_string` / `language` are MARKDOWN node types — so this would fail that
; validation. It is a host-grammar artifact, kept beside the `.md` it is proven
; against.
;
; Capture conventions: `@injection.language` names the language to inject;
; `@injection.content` is the region to parse with it. The `#eq?` predicate
; restricts injection to the `abc` label. (Use `#any-of?` to also match
; aliases, e.g. `(#any-of? @injection.language "abc" "abcnotation")`.)
;
; ---- Per-consumer wiring ----
;
; * Neovim (nvim-treesitter): place this file at
;     ~/.config/nvim/after/queries/markdown/injections.scm
;   (or `;; extends` + this rule to add to the bundled markdown injections), and
;   ensure the `abc` parser is installed (`:TSInstall abc` once the grammar is
;   registered, or a custom parser config pointing at tree-sitter-abc).
;
; * Helix: add the rule to
;     runtime/queries/markdown/injections.scm
;   (or a local `languages.toml` grammar entry for `abc`); Helix reads the same
;   `@injection.language` / `@injection.content` capture convention.
;
; * Zed: Zed performs fenced-block injection from the MARKDOWN extension/grammar,
;   keyed on the fence label matching a registered language's name (or a
;   configured alias), NOT from this ABC-side file. The croma Zed extension
;   (editors/zed) registers the language as `ABC` with `path_suffixes = ["abc"]`.
;   The reliable, verified fence label is the lowercase ``` ```abc ``` (it is
;   matched by `#eq? "abc"` here); whether Zed's bundled markdown auto-injects a
;   given label depends on the installed markdown grammar + the language-name
;   match, so the lowercase `abc` label is the one to use in docs and examples.

((fenced_code_block
  (info_string
    (language) @injection.language)
  (code_fence_content) @injection.content)
 (#eq? @injection.language "abc"))

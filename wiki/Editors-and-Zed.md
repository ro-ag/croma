# Editors & Zed

croma's editor story is built on two reusable pieces:

- **`croma-lsp`** — the stdio [[Language-Server|language server]] (diagnostics,
  formatting, semantic tokens, symbols, folding, hover, completion, code
  actions).
- **`tree-sitter-abc`** — a
  [tree-sitter grammar](https://github.com/ro-ag/croma/blob/main/tree-sitter-abc/README.md)
  for ABC 2.1, the **reusable syntax asset**: one grammar drives highlighting,
  folding, and structure across many consumers.

The intended user is a **developer** (musicians use croma's GPUI app, not a code
editor) — so this is developer tooling plus a syntax asset reusable far beyond
any one editor.

## Quick start (Zed)

1. Put the language server on your `PATH`:

   ```sh
   cargo install --path crates/croma-lsp     # → ~/.cargo/bin/croma-lsp
   ```

   (Once binary releases are published, the Zed extension downloads `croma-lsp`
   automatically; until then, `cargo install` is the path.)

2. Dev-install the extension: in Zed, run **`zed: install dev extension`** and
   select
   [`editors/zed/`](https://github.com/ro-ag/croma/blob/main/editors/zed/README.md).
   Open any `.abc` file — highlighting comes from the grammar, and the status
   bar shows `croma-lsp` attaching for diagnostics / formatting / hover /
   completion.

The Zed extension resolves `croma-lsp` **download-or-PATH**: a binary on `PATH`
first, then a GitHub-release auto-download per platform, else a clear error.

## One grammar, many consumers

| Consumer | How |
| --- | --- |
| **Zed** | extension registers the `ABC` language, wires the grammar, launches `croma-lsp` |
| **croma's GPUI app** | tree-sitter embeds directly (Zed = GPUI; same engine) |
| **Web / browser** | `tree-sitter build --wasm` → loaded by `web-tree-sitter`; live demo + headless `verify.mjs` gate |
| **Markdown** | ` ```abc ` fenced-block injection |
| **Neovim / Helix** | consume the grammar + queries directly |

A TextMate grammar (VS Code) was deliberately **not** built: it is
editor-specific and buys none of this reuse. VS Code is out of scope.

## Editor matrix

| Editor | Highlighting | LSP | Status |
| --- | --- | --- | --- |
| **Zed** | tree-sitter-abc (native) | `croma-lsp` (full) | shipped — dev-install |
| **Neovim** | tree-sitter-abc | `croma-lsp` via `nvim-lspconfig` | grammar + server ready; config snippet deferred |
| **Helix** | tree-sitter-abc | `croma-lsp` via `languages.toml` | grammar + server ready; config snippet deferred |
| **Browser / GPUI** | tree-sitter-abc (WASM) | — | shipped — web demo |
| VS Code | — | — | out of scope |

## Grammar coverage

`tree-sitter-abc` parses the full 10k ABC corpus at **99.46%** clean (9,946 /
10,000, no ERROR nodes); the categorized residual is backstopped by `croma-lsp`
semantic tokens.

## Full reference

Building the pieces (grammar generate/test/`--wasm` via wasi-sdk, the Zed
extension wasm build, the web demo) and the reuse evidence are in
[**`docs/editors.md`**](https://github.com/ro-ag/croma/blob/main/docs/editors.md),
[`tree-sitter-abc/README.md`](https://github.com/ro-ag/croma/blob/main/tree-sitter-abc/README.md),
and
[`editors/zed/README.md`](https://github.com/ro-ag/croma/blob/main/editors/zed/README.md).

# Editor integration

croma ships an editor story built on two reusable pieces:

- **`croma-lsp`** — a stdio [language server](lsp.md) (diagnostics, formatting,
  semantic tokens, document symbols, folding, hover, completion, code actions),
  a thin adapter over the proven core/formatter.
- **`tree-sitter-abc`** — a [tree-sitter grammar](../tree-sitter-abc/README.md)
  for ABC 2.1, the **reusable syntax asset**: one grammar drives highlighting,
  folding, and structure across many consumers.

The intended user is a **developer** (musicians use croma's GPUI app, not a code
editor). So this is developer tooling plus a syntax asset reusable far beyond any
one editor.

## Quick start (Zed)

1. Install the language server so it is on your `PATH`:

   ```sh
   cargo install --path crates/croma-lsp     # → ~/.cargo/bin/croma-lsp
   ```

   (Once croma cuts binary releases the Zed extension downloads `croma-lsp`
   automatically; until then, the `cargo install` above is the path.)

2. Dev-install the extension: in Zed, run **`zed: install dev extension`** and
   select [`editors/zed/`](../editors/zed/README.md). Open any `.abc` file —
   syntax highlighting comes from the grammar, and the status bar shows
   `croma-lsp` attaching for diagnostics/formatting/hover/completion.

Detail: [`editors/zed/README.md`](../editors/zed/README.md).

## The reuse story — one grammar, many consumers

| Consumer | How | Where |
|---|---|---|
| **Zed** | extension registers the `ABC` language, wires the grammar by repo path, launches `croma-lsp` | [`editors/zed/`](../editors/zed/README.md) |
| **croma's GPUI app** | tree-sitter embeds directly (Zed = GPUI; same engine) | — |
| **Web / browser** | `tree-sitter build --wasm` → `tree-sitter-abc.wasm`, loaded by `web-tree-sitter`; live demo + headless `verify.mjs` gate | [`tree-sitter-abc/web/`](../tree-sitter-abc/web/README.md) |
| **Markdown** | ` ```abc ` fenced-block injection | [`tree-sitter-abc/test/fixtures/markdown-injection.scm`](../tree-sitter-abc/test/fixtures/markdown-injection.scm) |
| **Neovim / Helix** | consume the grammar + queries directly (drop the injection/highlight queries in) | [`tree-sitter-abc/`](../tree-sitter-abc/README.md) |

A TextMate grammar (VS Code) was deliberately **not** built: it is editor-specific
and buys none of this reuse.

## Editor matrix

| Editor | Highlighting | LSP features | Status |
|---|---|---|---|
| **Zed** | tree-sitter-abc (native) | `croma-lsp` (full) | shipped — [`editors/zed/`](../editors/zed/README.md), dev-install |
| **Neovim** | tree-sitter-abc | `croma-lsp` via `nvim-lspconfig` | grammar + server ready; config snippet deferred |
| **Helix** | tree-sitter-abc | `croma-lsp` via `languages.toml` | grammar + server ready; config snippet deferred |
| **Browser / GPUI** | tree-sitter-abc (WASM) | — | shipped — [`web/`](../tree-sitter-abc/web/README.md) demo |
| VS Code | — | — | out of scope (no reuse value) |

## Evidence

- **Grammar coverage** — `tree-sitter-abc` parses the full 10k ABC corpus at
  **99.46%** clean (54 categorized, graceful-ERROR residual; `croma-lsp` semantic
  tokens backstop). Gate: `uv run python tools/prove_grammar_coverage.py`.
- **Web reuse** — headless `npm run verify:web` asserts the WASM grammar loads
  under `web-tree-sitter` and highlights with 0 root ERROR + > 0 captures.
- **Markdown injection** — runtime-verified against `tree-sitter-markdown`:
  ` ```abc ` blocks match and the injected ABC parses cleanly.
- **`croma-lsp`** — promotion legs A–E (diagnostics/formatting fidelity
  10000/10000, totality 0 panics/hangs, semantic tokens 10000/0, latency ~1 ms):
  see [`docs/lsp.md`](lsp.md).

## Binary distribution

The Zed extension resolves `croma-lsp` **download-or-PATH**: PATH (`cargo install`)
first, then GitHub-release auto-download per platform (lights up once the release
epic publishes binaries), else a clear error. So the extension works today and
gains zero-config install later without a code change.

## Building the pieces

- **Grammar:** `tree-sitter generate` / `tree-sitter test` / `tree-sitter build --wasm`
  (uses wasi-sdk, auto-downloaded; **no Docker/Emscripten**). See
  [`tree-sitter-abc/README.md`](../tree-sitter-abc/README.md).
- **Zed extension:** `cargo build --release --target wasm32-wasip1` from
  `editors/zed/` (use the repo-pinned 1.96.0 toolchain). Excluded from the cargo
  workspace.
- **Web demo:** `npm install && npm run verify:web` (or serve `web/`) from
  `tree-sitter-abc/`.

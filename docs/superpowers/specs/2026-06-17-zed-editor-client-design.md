# Design — ABC tree-sitter grammar + Zed editor-client (reusable syntax asset)

**Date:** 2026-06-17
**Status:** design (brainstormed, pending user review)
**Predecessor:** [`2026-06-17-lsp-promotion.md`](2026-06-17-lsp-promotion.md) — the
`croma-lsp` stdio server this client drives.

## Context & framing

`croma-lsp` is promoted/un-gated and ships in the default `cargo build` as the
`croma-lsp` binary (diagnostics, formatting, semanticTokens, documentSymbol,
foldingRange, hover, completion, codeAction). It needs an editor client to be
usable and demoable.

**Key framing decision (locked with the user):** the *user* of this tooling is a
**developer**, not a musician — musicians use the user's separate GPUI app, not a
code editor. So this epic is **developer tooling + a reusable syntax asset**, not
an end-user product. Two consequences:

- **VS Code is dropped.** A TextMate grammar would be a VS Code-only throwaway
  with zero reuse. "No musician would use this on VS Code," and a developer gets
  no strategic value from it.
- **The tree-sitter grammar is the strategic deliverable**, reusable across many
  consumers from one asset:
  - **Zed** — native highlighting / folds / structure (Zed = GPUI, the same
    engine as the user's app);
  - **the user's GPUI app** — tree-sitter embeds directly;
  - **web** — tree-sitter compiles to WASM (`web-tree-sitter`) → ABC highlighting
    in a browser/text widget;
  - **Markdown** — ` ```abc ` fenced-block injection;
  - **Neovim / Helix later** — also tree-sitter, so the deferred "config docs"
    editors get highlighting free from the same grammar.

**Zed is the first concrete consumer** and a real-engine smoke test of `croma-lsp`
in the GPUI runtime the user's app uses.

## Goals

1. A croma-owned `tree-sitter-abc` grammar (+ queries + tests) covering the core
   ABC 2.1 surface, grounded in croma's strict recognition / `MusicTokenKind`
   taxonomy.
2. A Zed extension that registers the `ABC` language (`.abc`), wires the grammar,
   and launches `croma-lsp`.
3. A `web-tree-sitter` WASM build + a tiny browser demo proving the grammar is
   reusable outside an editor.
4. Markdown ` ```abc ` injection.
5. Docs (`docs/editors.md`): install, Zed setup, the grammar-reuse story.

## Non-goals (this epic)

- VS Code / TextMate.
- Publishing to the Zed extension registry or any marketplace (needs the user's
  account; part of the release epic).
- A 100%-of-spec grammar — cover common ABC well; degrade gracefully (ERROR
  nodes); `croma-lsp` semantic tokens backstop highlighting.
- The other queued epics: A code-quality pass, B benchmark suite, C release
  mechanics, E docs reorg, F paper. (C's binary releases later "light up"
  auto-download — see Distribution.)

## Components (each isolated, independently testable)

### 1. `tree-sitter-abc/` (in-repo, extractable later)

Standard tree-sitter package layout so it can be lifted to its own repo / npm
later: `grammar.js`, `package.json` (tree-sitter-cli + web-tree-sitter build),
generated `src/parser.c` (committed per tree-sitter convention),
`queries/{highlights,injections,folds,brackets}.scm`, and `test/corpus/*.txt`
(tree-sitter's parse-tree test format).

Grammar scope — the core ABC 2.1 surface:
- **File** = optional file-header fields + free text + a sequence of **tunes**.
- **Tune** = `tune_header` (field lines `X:`/`T:`/`M:`/`L:`/`Q:`/`K:`/… ending at
  the `K:`) + `tune_body` (music lines, inline `[K:…]`/`[M:…]` fields, lyric
  `w:` / symbol `s:` lines).
- **Music line** tokens: notes (accidental? pitch octave-marks length?), rests,
  multi-measure rests, spacers, chords `[..]`, grace `{..}`, chord-symbols
  `"C"`, annotations `"^.."`, decorations `!..!`/`.`, tuplets `(3`, slurs `()`,
  ties `-`, broken rhythm `>`/`<`, barlines + repeat endings, overlays `&`.
- **Stylesheet directives** `%%…`, **comments** `%…`.

Capture names follow tree-sitter highlight conventions (`@keyword` field keys,
`@number` lengths/tuplets, `@string` chord-symbols/annotations, `@operator`
barlines/ties/slurs, `@function.macro`/`@attribute` decorations, `@comment`,
`@constant`/`@variable` pitches). Conceptually aligned with the LSP semantic-token
legend, but tree-sitter capture names are the canonical form; Zed/nvim consume
them with editor-specific theming.

**Quality gate (mirrors croma's corpus-proof culture):** parse the full 10k ABC
corpus with the built grammar and report the **root-`ERROR` / parse-error rate**;
drive it low and **adjudicate** the residual (real spec edges the grammar
intentionally doesn't model). This is the grammar's analog of the LSP/fmt corpus
proofs — evidence, not a vibe. A `tools/prove_grammar_coverage.py` (or a
`tree-sitter parse --quiet` sweep) reports the number.

### 2. `editors/zed/` — the Zed extension

- `extension.toml` — id/name/version; `[grammars.abc]` → the in-repo grammar by
  `repository` + `path` (+ `commit`); `[language_servers.croma-lsp]`
  `languages = ["ABC"]`.
- `src/lib.rs` — `impl zed_extension_api::Extension`; `language_server_command`
  resolves the `croma-lsp` binary via the **download-or-PATH resolver** below.
- `languages/abc/config.toml` — name `ABC`, `path_suffixes = ["abc"]`,
  `line_comment = "%"`, brackets/auto-close.
- `languages/abc/*.scm` — Zed-flavored copies of the canonical grammar queries
  (Zed loads highlight queries from the extension's language dir; canonical
  queries live with the grammar and are kept in sync).

Compiles to `wasm32-wasip1` (`cargo build` with the target); the resolver is
unit-testable.

### 3. Distribution — the download-or-PATH resolver (dissolves the release dependency)

`language_server_command` resolves `croma-lsp` in order:
1. **PATH / worktree** — `worktree.which("croma-lsp")` (works *today* via
   `cargo install --path crates/croma-lsp`);
2. **GitHub release auto-download** — `zed::latest_github_release` +
   `zed::download_file` for the platform asset (this is the user's chosen
   "auto-download"; it becomes functional once epic C cuts binary releases);
3. **else** — a clear error pointing at `cargo install`.

Built once, correctly: no hard ordering dependency on the release epic; the
extension simply prefers PATH now and gains auto-download when releases exist.

### 4. Markdown injection + web/WASM reuse

- **Injection:** an `injections.scm` rule so ` ```abc ` fenced blocks in Markdown
  highlight via the grammar (Zed + any tree-sitter markdown consumer).
- **Web:** `tree-sitter build --wasm` → `tree-sitter-abc.wasm`; a tiny static
  `web/` demo loads `web-tree-sitter` + the wasm + `highlights.scm` and renders a
  highlighted ABC sample in the browser — proof the grammar is reusable in the
  user's web/GPUI contexts.

### 5. Docs — `docs/editors.md`

Install (`cargo install --path crates/croma-lsp` now / auto-download later), Zed
setup (dev-install the extension), the grammar-reuse story (Markdown, web-tree-
sitter/WASM, future nvim/helix), and the grammar-coverage number.

## Toolchain prerequisites (risks to surface)

- **tree-sitter CLI** — not currently installed; **`cargo install tree-sitter-cli`**
  (Rust-native, fits the repo). Needed to generate `parser.c`, run `tree-sitter
  test`, and build the wasm.
- **node/npm** — present (`/opt/homebrew/bin`); used by `web-tree-sitter`.
- **`wasm32-wasip1` target** — not installed; `rustup target add wasm32-wasip1`
  for the Zed extension build.
- **WASM grammar build** — `tree-sitter build --wasm` may require emscripten or
  docker on some setups; if unavailable in the sandbox, the wasm is built in the
  step that has the toolchain and the demo documents the build command. Flagged,
  not assumed.
- **Zed app** — full extension smoke (install + open a `.abc` file) is a
  **manual, user-side** check; CI/automated scope is "the extension compiles to
  wasm + the resolver unit tests pass + the grammar corpus gate passes."

## Testing strategy

- **Grammar:** `tree-sitter test` corpus (parse-tree assertions) is the TDD unit;
  plus the 10k-corpus parse-error-rate gate (§1).
- **Zed extension:** compiles to `wasm32-wasip1`; resolver logic unit-tested.
- **Web:** the wasm loads and highlights a sample (headless check where possible,
  else a documented manual demo).
- **No regression:** this epic is **additive** — it touches no `croma-core`/
  `croma-fmt`/`croma-lsp` source; the forward/fmt/reader/LSP gates are unaffected
  by construction. (If `croma-lsp` needs a tweak to serve a client need, re-prove
  the LSP legs A–E.)

## Staging (each: TDD, landed via `land.py`, terse subagent receipt; orchestrator stays small)

| Stage | Branch | Scope | Gate |
|---|---|---|---|
| **G1** | `feature/ts-abc-grammar` | `tree-sitter-abc` grammar + queries + `test/corpus` + the 10k parse-coverage gate + `tools/prove_grammar_coverage.py`. | `tree-sitter test` green; corpus parse-error rate measured + residual adjudicated. |
| **G2** | `feature/zed-extension` | `editors/zed/` extension: `extension.toml`, Rust resolver (download-or-PATH), `languages/abc/` config + queries. | compiles to `wasm32-wasip1`; resolver unit tests pass. |
| **G3** | `feature/abc-web-wasm` | `tree-sitter build --wasm`; `web/` demo (web-tree-sitter + highlights); Markdown `injections.scm`. | wasm builds; demo highlights a sample; injection verified on a `.md` fixture. |
| **G4** | `docs/editors-doc` | `docs/editors.md`; tracker phase rows; README pointer. | docs land; tracker updated. |

## Discipline

Orchestrator holds this spec + the tracker + lands PRs + verifies; **all grammar/
extension/web/test code is delegated to per-stage subagents** returning terse
receipts. Branch per stage (`feature/*`, never `main`). No AI co-author trailer.
Session ends on `main`.

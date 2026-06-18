# The croma language server (`croma-lsp`)

`croma-lsp` is a stdio [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
server for ABC 2.1 source. It is a **proven, un-gated** feature: it is a *thin
adapter* over croma's already-proven core and formatter — it never reparses,
reformats, or re-derives diagnostics itself — and every capability is proven over
the full 10k corpus (see [Promotion evidence](#promotion-evidence)).

```sh
cargo build                       # the default build now ships the `croma-lsp` binary
target/debug/croma-lsp            # speak LSP over stdin/stdout
```

The server is started by an editor/LSP client, not run by hand; point your
client at the `croma-lsp` binary with stdio transport and associate it with the
`.abc` language.

## Capabilities

| LSP feature | Backed by | Notes |
|---|---|---|
| `publishDiagnostics` (push) | `croma_core::export_musicxml` diagnostics | byte `Span` → LSP `Range`; `source: "croma"`, code = the croma diagnostic code |
| `textDocument/formatting` | `croma_fmt::format` | one full-document `TextEdit`; byte-identical to `croma fmt` |
| `textDocument/semanticTokens/full` | `MusicTokenKind` spans | 11-type legend; delta-encoded; non-overlapping |
| `textDocument/documentSymbol` | tune + header-field spans | one symbol per tune, header fields as children |
| `textDocument/foldingRange` | tune spans | one fold per tune (header ∪ body) |
| `textDocument/hover` | static ABC 2.1 §3.1 / §4.14 tables | field-key and decoration docs |
| `textDocument/completion` | static tables | header field keys; decoration names (after `!`/`+`) |
| `textDocument/codeAction` | `croma_fmt::auto_fix` | `source.fixAll`: one whole-document `WorkspaceEdit` |

**Sync.** The server advertises **incremental** `textDocumentSync`; each
`didChange` range is applied to the in-memory document by clamped byte-splice (a
range-less change is a full replace). The parser is panic-free on malformed /
mid-edit input, so every keystroke state is safe to analyze.

**Position encoding.** The server negotiates **UTF-8** when the client offers it
(LSP 3.17 `general.positionEncodings`) — then an LSP character offset is a byte
offset, an exact match for croma's spans — and falls back to **UTF-16**
(characters counted as `char::len_utf16`) otherwise. All emitted ranges/lengths
are in the negotiated encoding.

## Architecture

The crate is split so the protocol logic is independent of the transport:

- **A pure, transport-free analysis layer** (`diagnostics`, `formatting`,
  `tokens`, `structure`, `hover`, `completion`, `code_action`, `position`,
  `document`) — synchronous, panic-free functions that map source text + an
  optional position to `lsp_types` payloads. No I/O, no runtime.
- **A thin `lsp-server` stdio loop** (`main.rs`) — owns the URI→text document
  store, negotiates capabilities, and dispatches each request/notification to the
  matching pure function. No business logic.

This is the same shape that lets the formatter prove itself in-process: the
promotion legs below drive the **pure functions over the corpus** (seconds, no
client process), and the transport is exercised separately over an in-memory
`Connection`.

Transport: **`lsp-server` + `lsp-types`** (synchronous). The handlers are fast
in-process CPU work over one document, so there is no concurrency to exploit; the
sync model is the lightest that covers stdio + incremental sync, and an explicit
dispatch loop makes the totality guarantee tractable. These deps live on the
`croma-lsp` binary only — `croma-core` stays zero-dependency +
crates.io-publishable (the `cargo tree -p croma-core --edges normal` CI guard
asserts it).

## Promotion evidence

The five promotion-bar legs — analogous to the formatter's `10000/0` and the
reader's `9935/9935` — are proven by an `ABC_ROOT`-gated, in-process harness
(`crates/croma-lsp/src/corpus_proof.rs`, mirroring the formatter's
`corpus_proof.rs`) plus an in-memory `Connection` transport test, and reported by
the black-box wrappers `tools/prove_lsp_totality.py` /
`tools/prove_lsp_fidelity.py`:

```sh
ABC_ROOT="$PWD/docs/untracked/corpus/zenodo-10k/abc" \
  cargo test -p croma-lsp --release -- --nocapture
```

| Leg | Bar | Result |
|---|---|---|
| **A. Diagnostics fidelity** | LSP-path diagnostics == core diagnostics (count + (severity, code) in order); every LSP `Range` reverses to the core byte `Span` | **10000 / 0 mismatches** |
| **B. Formatting identity** | applying the `formatting` edit == `croma_fmt::format`, byte-for-byte | **10000 / 0 mismatches** |
| **C. Totality** | 0 panics / 0 hangs over the corpus (didOpen + scripted didChange incl. truncate / delete-line / garbage-insert / clear), each analysis `catch_unwind`-isolated; the transport test bounds every client receive | **10000 files, 0 panics, 0 hangs** |
| **D. Semantic-token correctness** | token bytes exhaustive (cover every non-whitespace tokenized byte), non-overlapping, in-bounds, delta-encoding monotonic | **10000 / 0 violations** |
| **E. Latency** | diagnostics + semantic tokens < ~50 ms on an average ~200-line file | **~1 ms** (median of 20, a 200-line corpus file) |

A `>= 9000` file-count guard rejects a vacuous run (mis-set `ABC_ROOT`). The
harness uses an **absolute** `ABC_ROOT` because `cargo test`'s cwd is the package
directory.

There is **no `unwrap`/`expect`/`panic!`/index-panic/`debug_assert!` in the
non-test LSP source**; the workspace `unwrap_used` lint (denied under CI's
`-D warnings`) enforces it, and the totality leg proves it dynamically.

## Semantic-token legend

The advertised legend (index = order) is
`[VARIABLE, MODIFIER, NUMBER, OPERATOR, STRING, DECORATOR, MACRO, KEYWORD,
COMMENT, abcRest, abcError]`, mapped from `MusicTokenKind`:

| Token type | `MusicTokenKind` |
|---|---|
| `VARIABLE` | `Pitch` |
| `MODIFIER` | `Accidental`, `OctaveMark` |
| `NUMBER` | `Length`, `Tuplet`, `BrokenRhythm` |
| `OPERATOR` | `Barline`, `Slur`, `Tie`, `RepeatEnding`, `Overlay` |
| `STRING` | `ChordSymbol`, `Annotation` |
| `DECORATOR` | `Decoration` |
| `MACRO` | `Chord`, `GraceGroup` |
| `KEYWORD` | `InlineField` |
| `COMMENT` | `Comment`, `Spacer`, `ScoreLineBreak` |
| `abcRest` (custom) | `Rest`, `MultiMeasureRest` |
| `abcError` (custom) | `Malformed`, `Unsupported` |

`Whitespace` is skipped. **Overlap resolution:** the flat `MusicToken` stream is
*not* strictly non-overlapping — a container (`Chord`, `GraceGroup`) emits a span
enclosing its inner element tokens (and may even reach back over a leading
`Decoration`, e.g. `.[FA]2`). LSP forbids overlapping tokens, so the emitter
keeps the **widest token at each start** (sort by start asc, end desc; greedy
non-overlap). Every dropped token is contained in a kept one, so byte coverage is
preserved — leg D's "exhaustive" is union-coverage equality in bytes (every
highlighted byte covered, nothing invented), the strongest invariant the
overlapping container tokens allow. Header-field highlighting is currently
deferred (semantic tokens cover the music token stream).

## Formatting & code actions

Both emit a **whole-document edit**, never a span-mapped patch:
`textDocument/formatting` replaces the document with `croma_fmt::format(src)`;
the `source.fixAll` code action replaces it with `croma_fmt::auto_fix(src).output`
(offered only when `auto_fix` produces changes). `auto_fix` formats first, so its
`Change` spans refer to the *formatted* text — a whole-document replace sidesteps
remapping and is byte-identical to the proven core (leg B).

## Strict-spec note

The server inherits the parser's **strict ABC 2.1** default. Under it the `+...+`
decoration delimiter is not enabled (only `!...!`), so a `+decoration+` is a
`Malformed` token and does not hover — consistent with the
[parser recovery policy](../AGENTS.md). The decoration tables are grounded in the
names croma actually recognizes (`musicxml/notation.rs`, `musicxml/direction.rs`,
`parse/music.rs` shorthands), never invented.

## Layout

`crates/croma-lsp/src/`:

- `lib.rs` — public API + `analyze_document` (the diagnostics seam over `croma-core`).
- `position.rs` — byte `Span` ↔ `lsp_types::Range`, both encodings, with a clamped, reversible byte↔position mapping.
- `document.rs` — the URI→text store and incremental-sync apply.
- `diagnostics.rs` — core `Diagnostic` → `lsp_types::Diagnostic`.
- `formatting.rs` — `format` → full-document `TextEdit`.
- `tokens.rs` — `MusicTokenKind` walk → delta-encoded `SemanticTokens` (legend above).
- `structure.rs` — `documentSymbol` + `foldingRange`.
- `hover.rs`, `completion.rs`, `tables.rs` — static ABC 2.1 field/decoration docs.
- `code_action.rs` — `auto_fix` → `source.fixAll` `WorkspaceEdit`.
- `main.rs` — the `lsp-server` stdio loop, capability negotiation, and the in-memory `Connection` transport test.
- `corpus_proof.rs` — the `ABC_ROOT`-gated promotion harness (legs A–E + totality).

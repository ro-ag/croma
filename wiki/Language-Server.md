# Language server (`croma-lsp`)

`croma-lsp` is a stdio [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
server for ABC 2.1. It is a **thin adapter** over croma's already-proven core
and formatter — it never reparses, reformats, or re-derives diagnostics itself —
so its output is byte-identical to the CLI's, by construction. It is a proven,
un-gated capability and ships in the default build.

```sh
cargo install croma-lsp     # or build from source: cargo build --release
```

The server is launched by an editor / LSP client, not run by hand: point your
client at the `croma-lsp` binary with **stdio** transport and associate it with
the `.abc` language. For a ready-made setup, see [[Editors-and-Zed]].

## Capabilities

| LSP feature | Backed by |
| --- | --- |
| `publishDiagnostics` (push) | `croma_core::export_musicxml` diagnostics |
| `textDocument/formatting` | `croma_fmt::format` (one full-document edit) |
| `textDocument/semanticTokens/full` | the music-token stream (11-type legend) |
| `textDocument/documentSymbol` | one symbol per tune, header fields as children |
| `textDocument/foldingRange` | one fold per tune |
| `textDocument/hover` | static ABC 2.1 field / decoration tables |
| `textDocument/completion` | header field keys; decoration names |
| `textDocument/codeAction` | `croma_fmt::auto_fix` as `source.fixAll` |

- **Sync:** advertises **incremental** `textDocumentSync`; the parser is
  panic-free on malformed / mid-edit input, so every keystroke state is safe to
  analyze.
- **Position encoding:** negotiates **UTF-8** when offered (then a character
  offset is a byte offset, matching croma's spans), else falls back to UTF-16.
- **Strict-spec:** inherits the parser's strict ABC 2.1 default — e.g. the
  `+...+` decoration delimiter is not enabled, so `+decoration+` is a malformed
  token.

## Proven over the corpus

The promotion legs run over the full 10k corpus (see [[How-its-Proven]]):

| Leg | Result |
| --- | --- |
| Diagnostics fidelity (LSP path == core) | **10,000 / 0** mismatches |
| Formatting identity (== `croma_fmt::format`) | **10,000 / 0** mismatches |
| Totality (no panics / no hangs) | **10,000 files, 0 panics, 0 hangs** |
| Semantic-token correctness | **10,000 / 0** violations |
| Latency (diagnostics + tokens, real-size) | **~1 ms** |

The `croma-lsp` deps (`lsp-server`, `lsp-types`) live on the server binary only;
`croma-core` stays zero-dependency.

## Full reference

Architecture (transport-free analysis layer + thin stdio loop), the full
semantic-token legend, the promotion harness, and the layout are documented in
[**`docs/lsp.md`**](https://github.com/ro-ag/croma/blob/main/docs/lsp.md).

Latency percentiles (p50/p95/p99 per request type): [[Benchmarks]].

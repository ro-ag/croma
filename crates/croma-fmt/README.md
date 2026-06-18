# croma-fmt

`croma-fmt` is the ABC formatter library for [Croma](https://github.com/ro-ag/croma),
built directly on the `croma-core` parse and surface model rather than a separate
parser. It pretty-prints ABC notation into a canonical, idempotent form and offers
an `--auto-fix` pass that sanitizes loose source (multi-voice alignment, redundant
or malformed barlines, whitespace normalization) while preserving musical content.

Formatting is lossless and idempotent: re-running the formatter over already
formatted output is a no-op, a property proven over the full 10k-file ABC corpus.

This is a library crate. It backs the `croma fmt` subcommand in
[`croma-cli`](https://crates.io/crates/croma-cli) and the
`textDocument/formatting` capability in
[`croma-lsp`](https://crates.io/crates/croma-lsp).

```toml
[dependencies]
croma-fmt = "0.9"
```

See the [formatter documentation](https://github.com/ro-ag/croma/blob/main/docs/formatter.md)
for the full surface model and auto-fix policy.
